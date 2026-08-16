//! ggez 客户端 —— 阶段 1：核心玩法单机 demo。
//!
//! - 玩家圆：**右键**设置移动目标点，圆球匀速走过去，到达即停
//! - 场地逐渐收缩，出界扣血；球被挤到边缘/相互重叠会受压损血
//! - 若干机器人（确定性 AI）在同一场地游走，演示多人对抗氛围
//!
//! 玩法逻辑全部在 `game-core` 的 `World` 中，本文件只负责输入采集与渲染。

use game_core::fix::{cos, sin, Fix64, Vec2};
use game_core::meta::{MatchConfig, MatchPhase, MatchState};
use game_core::rng::Rng;
use game_core::skill::SkillId;
use game_core::world::{PlayerInput, World};
use ggez::event;
use ggez::graphics::{self, Canvas, Color, DrawMode, Mesh};
use ggez::mint::Point2;
use ggez::{Context, GameResult};

mod netlink;

/// 机器人数量（不含玩家本人）。当前 Solo/局域网均无本地 AI；保留该常量供将来“带 AI 测试”模式复用。
#[allow(dead_code)]
const BOTS: u32 = 7;
/// 固定步长模拟（帧率）
const TICK: f64 = 1.0 / 60.0;
/// 玩家本人 = id 0
const PLAYER_ID: u32 = 0;

/// host：客户端连续空闲这么多帧判定为掉线（自动 mark_dropped，不卡全队）。约 3 秒。
const HOST_DROP_TICKS: u32 = 180;
/// host：每隔多少帧保存一次世界快照（供重连）。约 0.5 秒。
const SNAPSHOT_EVERY: u64 = 30;
/// client：连续多少帧未收到权威帧判定为“掉线/等待重连”（进入重连 UI）。约 3 秒。
const CLIENT_STALE_TICKS: u64 = 180;
/// 单机开局配置超时：等这么久没按开始就用默认配置自动开始第一轮（避免窗口没焦点/按键收不到导致卡死）。
const PRE_GAME_TIMEOUT_SECS: f64 = 60.0;

/// 学习阶段里，数字键 1..N 用于从“选中的树”选择/绑定技能。
/// 这里定义 8 个键字母 → CastKey 的映射。
const KEY_LETTERS: [(&str, game_core::skill::CastKey); 8] = [
    ("c", game_core::skill::CastKey::C),
    ("r", game_core::skill::CastKey::R),
    ("e", game_core::skill::CastKey::E),
    ("d", game_core::skill::CastKey::D),
    ("y", game_core::skill::CastKey::Y),
    ("t", game_core::skill::CastKey::T),
    ("f", game_core::skill::CastKey::F),
    ("g", game_core::skill::CastKey::G),
];

/// 联网多局：学习阶段结束后、进入下一局前的“配置同步”阶段。
#[derive(Clone, Copy, PartialEq)]
enum NetCfgSync {
    /// 不处于同步（单机 / Fighting / Finished）。
    Idle,
    /// host：正在收齐各端配置（含自身），齐后广播 PlayerCfgAll 并完成。
    HostGather,
    /// client：已上传配置，正在等 host 广播 PlayerCfgAll。
    ClientWait,
}

/// 顶层应用状态（主菜单 / 各对战模式）。命令行可直通某模式，也可进主菜单选择。
#[derive(Clone, Copy, PartialEq, Debug)]
enum AppState {
    /// 主菜单：从三大入口选择。
    MainMenu,
    /// 单机技能试验场（无 AI，一个玩家自由测技能/数值）。
    Solo,
    /// 局域网：开房间（host，自身=player0）。
    LanHost { port: u16, total: u8 },
    /// 局域网：加入（client）。
    LanJoin { addr: std::net::SocketAddr },
}

/// 升级到某个等级的价格（简单坡度，后期可调）
fn upgrade_cost(current_level: u32) -> i32 {
    (current_level * 5 + 5) as i32
}

struct Game {
    /// 当前小局的战斗世界
    world: World,
    /// 多局 meta 状态（经济/升级/周期）
    meta: MatchState,
    /// 玩家本人待发送的移动目标（右键设置；成功发给 World 后由 World 保留）
    player_target: Option<Vec2>,
    /// 待发送的施法命令（左键确认后产生，直到世界进入前摇才清）
    pending_cast: Option<(SkillId, Option<Vec2>)>,
    /// 当前等左键确认的点目标技能（技能键按下后稳定保持，直到左键确认/右键/S 取消）
    pending_skill: Option<SkillId>,
    /// shift 键按住时压入的待执行指令队列（每帧把队首注入 PlayerInput.queued）
    queued_cmds: std::collections::VecDeque<game_core::player::Cmd>,
    /// shift 键按住点目标技能时，等左键确认的技能
    pending_shift_skill: Option<SkillId>,
    /// 本帧是否要给 World 下发"清空命令队列"信号（S / 普通即时操作打断）
    pending_clear_signal: bool,
    /// 本帧是否要给 World 下发"停止移动"信号（S 停手）
    pending_stop_signal: bool,
    /// 学习阶段当前选中的键（用于从该键的树里选技能/升级）
    learn_tree_key: Option<game_core::skill::CastKey>,
    /// 机器人的当前目标点
    bot_targets: Vec<Option<Vec2>>,
    /// 机器人的确定性随机源
    bot_rngs: Vec<Rng>,
    /// 累计未消费的模拟时间
    accumulator: f64,
    /// 世界坐标 → 屏幕坐标的缩放
    scale: f32,
    /// 相机偏移（竞技场中心在画面中央）
    offset: Point2<f32>,
    /// 联网模式：加入 host 后用于每帧收发/喂 World；`None` = 单机（含本地 AI 机器人）。
    net_link: Option<netlink::NetLinkUdp>,
    /// 联网模式：开房作 host，建连/握手阶段（自身=player 0）。
    net_host: Option<net::handshake::HostHandshake<net::transport::StdUdpTransport>>,
    /// 联网模式：开房作 host，运行阶段。
    net_host_ls: Option<net::lockstep::HostLockstep<net::transport::StdUdpTransport>>,
    /// 联网：是否已完成 READY/GO 统一起始（可开始推进）。host=已广播 GO；client=已收 GO。
    net_ready: bool,
    /// 联网多局：学习结束后「配置同步」阶段（见 `NetCfgSync`）。
    net_cfg: NetCfgSync,
    /// 开局前的技能配置阶段（第一局开始前先选/升级技能）。
    pre_game_config: bool,
    /// 顶层应用状态（主菜单 / 各模式）。
    app: AppState,
    /// 客户端是否已因长时间收不到帧而进入“掉线/重连”状态（显示重连界面）。
    conn_dropped: bool,
    /// 客户端是否正在发起重连（已按 R，正等 host 快照）。
    reconnect_attempting: bool,
    /// 主机端累计产帧数（用于周期保存快照）。
    host_frame_count: u64,
    /// 单机开局配置剩余的等待秒数（超时自动用默认配置开始，避免“按键无反应卡死”）。
    pre_game_timer: f64,
}

impl Game {
    fn new(ctx: &mut Context, app: AppState) -> GameResult<Self> {
        // 注册中文字体：用 include_bytes 内嵌，避免资源路径/VFS 解析问题。
        let font = ggez::graphics::FontData::from_slice(include_bytes!("../../assets/fonts/cjk.ttf"))?;
        ctx.gfx.add_font("cjk", font);

        // 联网：加入 host 或开房作 host；否则单机（含本地 AI 机器人）。
        let mut net_link: Option<netlink::NetLinkUdp> = None;
        let mut net_host: Option<net::handshake::HostHandshake<net::transport::StdUdpTransport>> = None;
        let net_host_ls: Option<net::lockstep::HostLockstep<net::transport::StdUdpTransport>> = None;
        // 主菜单/单机试验场：仅 1 个玩家且无 AI；Solo 也是 1 玩家无 AI。
        let mut player_count: u32 = 1;
        match app {
            AppState::MainMenu => {}
            AppState::Solo => {}
            AppState::LanJoin { addr } => {
                let mut link: netlink::NetLinkUdp =
                    netlink::NetLink::connect_udp(addr).map_err(ggez::GameError::from)?;
                eprintln!("[client] connecting to {addr}, my stable identity = {}", link.my_identity());
                for _ in 0..60 {
                    if link.join_handshake().map_err(ggez::GameError::from)? {
                        break;
                    }
                }
                player_count = link.player_count().max(1) as u32;
                net_link = Some(link);
            }
            AppState::LanHost { port, total } => {
                let t =
                    net::transport::StdUdpTransport::bind(&format!("0.0.0.0:{port}")).map_err(ggez::GameError::from)?;
                let hs = net::handshake::HostHandshake::new(t, total.max(1) as usize, true);
                player_count = total.max(1) as u32;
                net_host = Some(hs);
            }
        }
        let seed = 20260812u64;
        // Solo 试验场（含主菜单入口）：世界含「你 + 1 个不动靶子」→ 不判结束；meta 只记录你。
        // 这样无论从菜单按 1 进 Solo 还是 --solo 直通，世界都已是 sandbox。
        let mut world = match app {
            AppState::Solo | AppState::MainMenu => {
                let mut w = World::new(2, seed); // player0=你, player1=靶子
                w.sandbox = true;
                eprintln!("[solo] world players={} sandbox={}", w.players.len(), w.sandbox);
                w
            }
            _ => World::new(player_count.max(1), seed),
        };
        let meta_ids: Vec<u32> = match app {
            AppState::Solo | AppState::MainMenu => vec![0],
            _ => (0..player_count).collect(),
        };
        // 整场对抗：3 小局，所有玩家都纳入档案
        let mut meta = MatchState::new(MatchConfig::default(), &meta_ids, 8);
        // 观察/调试 `FASTROUND=1`：缩小场地加速局终、缩短学习时间、多开几局，便于用 netlogs 看多局循环。
        if std::env::var("FASTROUND").is_ok() {
            world.arena_radius = game_core::fix::Fix64::from_num(3.0);
            meta.config.learn_time_secs = 1.0;
            meta.config.total_rounds = 4;
        }
        // 默认绑定：每个键绑定其树的首个技能，保证开局即可用（玩家可在学习阶段改）
        for p in meta.profiles.iter_mut() {
            for key in game_core::skill::CastKey::ALL {
                let options = key.tree().skills_in_tree();
                if let Some(first) = options.first() {
                    p.bind_skill(key, *first);
                }
            }
        }

        // 当前所有模式（Solo/Lan）都不带本地 AI 机器人：Solo 无对手，Lan 是真人玩家。
        let bot_rngs: Vec<Rng> = Vec::new();
        let bot_targets: Vec<Option<Vec2>> = Vec::new();

        let (w, h) = ctx.gfx.drawable_size();
        Ok(Game {
            world,
            meta,
            player_target: None,
            pending_cast: None,
            pending_skill: None,
            queued_cmds: std::collections::VecDeque::new(),
            pending_shift_skill: None,
            pending_clear_signal: false,
            pending_stop_signal: false,
            // 默认首选一棵技能树（第一个键 C），让“按数字键绑技能”立即可用，不必先想到去按字母键选树。
            learn_tree_key: game_core::skill::CastKey::ALL.first().copied(),
            bot_targets,
            bot_rngs,
            accumulator: 0.0,
            scale: 1.0,
            offset: Point2 { x: w / 2.0, y: h / 2.0 },
            net_link,
            net_host,
            net_host_ls,
            net_ready: false,
            net_cfg: NetCfgSync::Idle,
            app,
            pre_game_config: app != AppState::MainMenu,
            conn_dropped: false,
            reconnect_attempting: false,
            host_frame_count: 0,
            pre_game_timer: PRE_GAME_TIMEOUT_SECS,
        })
    }

    fn update_camera(&mut self, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        // 令初始场地（半径约 20）约占较短边的 45%
        self.scale = sw.min(sh) * 0.45 / 20.0;
        self.offset.x = sw / 2.0;
        self.offset.y = sh / 2.0;
        Ok(())
    }

    /// 屏幕坐标 → 世界坐标。
    fn screen_to_world(&self, x: f32, y: f32) -> Vec2 {
        Vec2::new(
            Fix64::from_num((x - self.offset.x) / self.scale),
            Fix64::from_num((y - self.offset.y) / self.scale),
        )
    }

    /// 学习阶段交互：
    /// - 按字母键 → 选中该键对应的技能树（learn_tree_key）
    /// - 按数字键 1..N → 把选中的树里的第 N 个技能绑定到该键
    /// - 按 `=` → 升级当前键绑定的技能
    /// - 按 `X` → 洗点（全额退款 + 清空绑定）
    fn poll_learning(&mut self, ctx: &Context) {
        use ggez::input::keyboard::Key;

        // 字母键：选中技能树
        for (letter, key) in KEY_LETTERS {
            if ctx
                .keyboard
                .is_logical_key_just_pressed(&Key::Character(letter.into()))
            {
                eprintln!("[learn] select tree '{letter}' (key=Key::Character)");
                self.learn_tree_key = Some(key);
            }
        }

        let learn_key = self.learn_tree_key;

        // 数字键：绑定选中树里的第 N 个技能
        if let Some(key) = learn_key {
            for (i, skill) in key.tree().skills_in_tree().iter().enumerate() {
                let digit = char::from(b'1' + i as u8); // '1','2',...
                if ctx
                    .keyboard
                    .is_logical_key_just_pressed(&Key::Character(digit.to_string().into()))
                {
                    eprintln!("[learn] bind tree={} digit='{}' -> {}", key.letter(), digit, game_core::skill::DefTable::def(*skill).name);
                    if let Some(profile) = self
                        .meta
                        .profiles
                        .iter_mut()
                        .find(|pr| pr.player_id == PLAYER_ID)
                    {
                        profile.bind_skill(key, *skill);
                    }
                }
            }
        }

        // `=` 键：升级当前选中键绑定的技能
        if ctx.keyboard.is_logical_key_just_pressed(&Key::Character("=".into())) {
            eprintln!("[learn] '=' pressed, learn_tree_key={learn_key:?}");
            if let Some(key) = learn_key {
                if let Some(profile) = self
                    .meta
                    .profiles
                    .iter_mut()
                    .find(|pr| pr.player_id == PLAYER_ID)
                {
                    if let Some(skill) = profile.bound_skill(key) {
                        let cost = upgrade_cost(profile.skill_level(skill));
                        eprintln!("[learn] upgrade {} cost={}", game_core::skill::DefTable::def(skill).name, cost);
                        profile.upgrade_skill(skill, cost);
                    }
                }
            }
        }

        // `X`：洗点（全额退款）
        if ctx.keyboard.is_logical_key_just_pressed(&Key::Character("x".into())) {
            if let Some(profile) = self
                .meta
                .profiles
                .iter_mut()
                .find(|pr| pr.player_id == PLAYER_ID)
            {
                profile.respec(1.0);
            }
            self.learn_tree_key = None;
        }
    }

    /// 本局进行中：结算击杀、名次，进入学习阶段。
    fn settle_round(&mut self) {
        for (killer, _victim) in self.world.take_kills() {
            self.meta.register_kill(killer);
        }
        let placement = self.world.placement();
        self.meta.finish_round(placement);
    }

    /// 进入下一局前：把玩家的技能等级从档案同步到世界，并重置世界。
    fn teardown_round_end(&mut self) {
        // 把 meta.profiles 全量同步到 world.players，使所有端下一局的技能等级一致。
        // （联网下 profiles 已经由 host 广播的完整配置统一；单机下按本地各玩家档案设置。）
        for (profile, p) in self.meta.profiles.iter().zip(self.world.players.iter_mut()) {
            for i in 0..p.skill_levels.len().min(profile.skill_levels.len()) {
                p.skill_levels[i] = profile.skill_levels[i];
            }
        }
        self.world.reset_round();
        self.player_target = None;
        self.pending_cast = None;
        self.pending_skill = None;
        self.accumulator = 0.0;
    }

    /// 把 host 广播的完整玩家配置（`PlayerCfgAll` entries）应用回本地 `meta.profiles`。
    fn apply_player_cfgs(&mut self, entries: &[(u8, Vec<u8>)]) {
        for (player_index, bytes) in entries {
            if let Some(cfg) = game_core::progress::PlayerConfig::decode(bytes) {
                if let Some(profile) = self.meta.profiles.iter_mut().find(|pr| pr.player_id == *player_index as u32) {
                    cfg.apply_to(profile);
                }
            }
        }
    }

    /// 每帧统一轮询输入（键盘 + 鼠标都用 ggez 的 just-pressed 边沿检测）。
    fn poll_input(&mut self, ctx: &Context) {
        use ggez::input::keyboard::Key;
        use ggez::input::mouse::MouseButton;

        // 玩家档案（本帧只读绑定的技能）
        let me = self.self_index();
        let bound_for = |key: game_core::skill::CastKey| -> Option<SkillId> {
            self.meta
                .profiles
                .iter()
                .find(|pr| pr.player_id == me)
                .and_then(|p| p.bound_skill(key))
        };

        // shift 按住 = 预排队列模式（winit ModifiersState::shift_key()）。
        let shift = ctx.keyboard.active_modifiers.shift_key();

        // 1) 技能键：按下 → 施放该键绑定的技能（shift 时入列）
        // 注意：winit 对 shift+字母会给大写逻辑字符，故按大小写都匹配，否则 shift+技能无法触发。
        for (letter, key) in KEY_LETTERS {
            let lower = letter.to_string();
            let upper = letter.to_uppercase().to_string();
            let just = ctx.keyboard.is_logical_key_just_pressed(&Key::Character(lower.into()))
                || ctx.keyboard.is_logical_key_just_pressed(&Key::Character(upper.into()));
            if just {
                if let Some(skill) = bound_for(key) {
                    if game_core::skill::DefTable::def(skill).needs_point {
                        if shift {
                            self.player_target = None; // 队列操作：放弃即时移动目标，避免覆盖队列移动
                            self.pending_shift_skill = Some(skill);
                        } else {
                            self.pending_skill = Some(skill); // 等左键确认
                        }
                    } else if shift {
                        self.player_target = None;
                        self.queued_cmds.push_back(game_core::player::Cmd::Cast(skill, None));
                    } else {
                        self.pending_cast = Some((skill, None)); // 无需目标，直接施放
                        self.pending_clear_signal = true; // 普通施法打断已排的队列
                    }
                }
            }
        }
        // S: 停止移动 + 清空 shift 队列（含 World 里的队列）—— 同样大小写都匹配
        let s_pressed = ctx.keyboard.is_logical_key_pressed(&Key::Character("s".into()))
            || ctx.keyboard.is_logical_key_pressed(&Key::Character("S".into()));
        if s_pressed {
            self.player_target = None;
            self.pending_skill = None;
            self.pending_cast = None;
            self.pending_shift_skill = None;
            self.queued_cmds.clear();
            self.pending_clear_signal = true; // 让 World 也清空其命令队列
            self.pending_stop_signal = true;  // 让 World 停止当前移动
        }

        // 2) 左键：确认点目标技能（cursor 位置作为落点）
        if ctx.mouse.button_just_pressed(MouseButton::Left) {
            let m = ctx.mouse.position();
            let world = self.screen_to_world(m.x, m.y);
            if let Some(skill) = self.pending_skill.take() {
                self.player_target = None;
                self.pending_cast = Some((skill, Some(world)));
                self.pending_clear_signal = true; // 普通施法打断已排的队列
            } else if let Some(skill) = self.pending_shift_skill.take() {
                self.player_target = None;
                self.queued_cmds.push_back(game_core::player::Cmd::Cast(skill, Some(world)));
            }
        }

        // 3) 右键：设置移动目标（shift 时入列；普通右键即时移动会打断队列）
        if ctx.mouse.button_just_pressed(MouseButton::Right) {
            self.pending_skill = None;
            self.pending_cast = None;
            self.pending_shift_skill = None;
            let m = ctx.mouse.position();
            let world = self.screen_to_world(m.x, m.y);
            if shift {
                self.player_target = None; // 放弃即时移动，改为排队列
                self.queued_cmds.push_back(game_core::player::Cmd::Move(world));
            } else {
                self.queued_cmds.clear(); // 普通即时移动：打断并清空之前排的队列
                self.pending_clear_signal = true; // 也让 World 清空其队列
                self.player_target = Some(world);
            }
        }
    }

    /// 本机玩家在该次对局中的序号：单机/host 恒为 0，加入者为握手分配到的 `my_index`。
    fn self_index(&self) -> u32 {
        match &self.net_link {
            Some(l) => l.my_index() as u32,
            None => PLAYER_ID,
        }
    }

    /// 生成「本机玩家最终配置快照」的编码字节（学习阶段结束/就绪时上报给 host）。
    fn local_player_cfg(&self) -> Vec<u8> {
        let me = self.self_index();
        match self.meta.profiles.iter().find(|pr| pr.player_id == me) {
            Some(p) => game_core::progress::PlayerConfig::from_profile(p).encode(),
            None => Vec::new(), // 异常：不应发生；空配置
        }
    }

    /// 生成本（本机玩家）这一帧要下达的命令（移动 / 施法 / shift 队列 / 清队 / 停止）。
    /// 单机模式把它放到 `PLAYER_ID`；联网模式由 `NetLink` 上行给 host、按我的序号归位。
    fn local_player_input(&mut self) -> PlayerInput {
        let set_target = self.player_target;
        let cast = self.pending_cast;
        let queued = self.queued_cmds.drain(..).collect();
        let clear_queue = self.pending_clear_signal;
        let stop_move = self.pending_stop_signal;
        self.pending_clear_signal = false;
        self.pending_stop_signal = false;
        PlayerInput {
            set_target,
            cast,
            queued,
            clear_queue,
            stop_move,
        }
    }

    /// 生成本（模拟）帧内所有玩家的输入（单机：本机玩家 + 本地 AI 机器人）。
    fn compute_inputs(&mut self) -> Vec<PlayerInput> {
        let mut inputs: Vec<PlayerInput> = self
            .world
            .players
            .iter()
            .map(|_| PlayerInput::default())
            .collect();

        // 玩家本人
        let me = self.local_player_input();
        inputs[PLAYER_ID as usize] = me;

        // 机器人确定性 AI：需要新目标时从自身随机源挑一个场地内的点。
        let arena = self.world.arena_radius;
        for (i, input) in inputs.iter_mut().enumerate().skip(1) {
            let bot_idx = i - 1;
            let needs_new = match self.bot_targets[bot_idx] {
                None => true,
                Some(t) => {
                    let d = self.world.players[i].pos - t;
                    d.length_squared() <= Fix64::from_num(0.1)
                }
            };
            if needs_new {
                let r = arena * Fix64::from_num(0.8);
                let a = self.bot_rngs[bot_idx].next_fix() * Fix64::from_num(std::f64::consts::TAU);
                let t = Vec2::new(r * cos(a), r * sin(a));
                self.bot_targets[bot_idx] = Some(t);
            }
            input.set_target = self.bot_targets[bot_idx];
        }

        inputs
    }

    /// 客户端掉线后的重连流程：按 R 发起重连 → 向 host 拉快照 → 重建 World 并对齐基线 → 恢复。
    /// 在该状态下不推进世界（冻结，避免与 host 分叉），只等待重连成功或玩家放弃。
    fn poll_reconnect(&mut self, ctx: &mut Context, link: &mut netlink::NetLinkUdp) {
        use ggez::input::keyboard::Key;
        let r_pressed = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("r".into()))
            || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("R".into()));
        if !self.reconnect_attempting && !r_pressed {
            return; // 未按 R，不发起重连，保持空闲等待。
        }
        if !self.reconnect_attempting {
            self.reconnect_attempting = true;
            eprintln!("[client] reconnect flow: sending ReconnectReq...");
        }
        match link.try_reconnect() {
            Ok(Some((world_bytes, seq))) => {
                eprintln!("[client] got Snapshot seq={seq}, rebuilding World ({n} bytes)", n = world_bytes.len());
                link.align_after_reconnect().ok();
                match game_core::world_ser::world_from_bytes(&world_bytes) {
                    Some(w) => {
                        self.world = w;
                        // 清空本地输入残留，避免把掉线期间的输入误带到接回后。
                        self.player_target = None;
                        self.pending_cast = None;
                        self.pending_skill = None;
                        self.queued_cmds.clear();
                        self.pending_shift_skill = None;
                        self.pending_clear_signal = false;
                        self.pending_stop_signal = false;
                        self.conn_dropped = false;
                        self.reconnect_attempting = false;
                        // 重连一次成功即进入等待；重连接完毕后靠下一帧的权威帧驱动（stale 已归零）。
                        eprintln!("[client] reconnected: World rebuilt from snapshot, resuming lockstep");
                    }
                    None => {
                        eprintln!("[client] failed to decode snapshot, retrying on next keypress");
                        self.reconnect_attempting = false;
                    }
                }
            }
            Ok(None) => {
                // 尚未收到快照：保持等待（下帧再试）。
            }
            Err(e) => {
                eprintln!("[client] reconnect error: {e:?}");
                self.reconnect_attempting = false;
            }
        }
    }

    fn draw_scene(&mut self, ctx: &mut Context) -> GameResult {
        self.update_camera(ctx)?;
        let mut canvas = Canvas::from_frame(ctx, Color::from_rgb(18, 22, 34));

        // 瞄准指示：从玩家到鼠标的画一条线（点目标技能待左键确认）。
        if self.pending_skill.is_some() || self.pending_shift_skill.is_some() {
            if let Some(p) = self.world.players.get(self.self_index() as usize) {
                let pfx = p.pos.x.to_num::<f32>() * self.scale + self.offset.x;
                let pfy = p.pos.y.to_num::<f32>() * self.scale + self.offset.y;
                let mouse = ctx.mouse.position();
                let aimline = Mesh::new_line(
                    &ctx.gfx,
                    &[
                        Point2 { x: pfx, y: pfy },
                        Point2 { x: mouse.x, y: mouse.y },
                    ],
                    2.0,
                    Color::from_rgba(255, 220, 120, 190),
                )?;
                canvas.draw(&aimline, graphics::DrawParam::new());
                let aimdot = Mesh::new_circle(
                    &ctx.gfx,
                    DrawMode::fill(),
                    Point2 { x: mouse.x, y: mouse.y },
                    5.0,
                    0.5,
                    Color::from_rgba(255, 220, 120, 220),
                )?;
                canvas.draw(&aimdot, graphics::DrawParam::new());
            }
        }

        // 场地绳圈（逐渐收缩）
        let ar = self.world.arena_radius.to_num::<f32>();
        let fence = Mesh::new_circle(
            &ctx.gfx,
            DrawMode::stroke(3.0),
            self.offset,
            ar * self.scale,
            0.5,
            Color::from_rgb(120, 130, 160),
        )?;
        canvas.draw(&fence, graphics::DrawParam::new());

        // 障碍（圆形柱子：实心浅色圆 + 描边）
        for o in self.world.obstacles.iter() {
            let ox = o.pos.x.to_num::<f32>() * self.scale + self.offset.x;
            let oy = o.pos.y.to_num::<f32>() * self.scale + self.offset.y;
            let or = (o.radius.to_num::<f32>() * self.scale).max(3.0);
            let pillar = Mesh::new_circle(
                &ctx.gfx,
                DrawMode::fill(),
                Point2 { x: ox, y: oy },
                or,
                0.5,
                Color::from_rgb(70, 76, 96),
            )?;
            canvas.draw(&pillar, graphics::DrawParam::new());
            let pillar_edge = Mesh::new_circle(
                &ctx.gfx,
                DrawMode::stroke(2.0),
                Point2 { x: ox, y: oy },
                or,
                0.5,
                Color::from_rgb(120, 130, 160),
            )?;
            canvas.draw(&pillar_edge, graphics::DrawParam::new());
        }

        // 玩家圆与 HP 条
        let me_idx = self.self_index();
        for p in self.world.players.iter() {
            if !p.alive {
                continue;
            }
            let fx = p.pos.x.to_num::<f32>() * self.scale + self.offset.x;
            let fy = p.pos.y.to_num::<f32>() * self.scale + self.offset.y;
            let r = p.radius.to_num::<f32>() * self.scale;
            let mut color = player_color(p.id, me_idx);
            // 潜行：半透明（潜行踢 / 隐蔽效果）
            if p.stealth() {
                color.a = 0.4;
            }
            let ball = Mesh::new_circle(
                &ctx.gfx,
                DrawMode::fill(),
                Point2 { x: fx, y: fy },
                r.max(2.0),
                0.5,
                color,
            )?;
            canvas.draw(&ball, graphics::DrawParam::new());

            // 护盾：外圈浅色圆环（护盾激活时）
            if p.shield() {
                let sring = Mesh::new_circle(
                    &ctx.gfx,
                    DrawMode::stroke(3.0),
                    Point2 { x: fx, y: fy },
                    r + 6.0,
                    0.5,
                    Color::from_rgba(120, 210, 255, 190),
                )?;
                canvas.draw(&sring, graphics::DrawParam::new());
            }

            // 施法前摇提示：头顶一个不断消失的圆环（越接近完成越细小）
            if let game_core::skill::CastPhase::Windup { remaining, .. } = p.caster.phase() {
                let ring = Mesh::new_circle(
                    &ctx.gfx,
                    DrawMode::stroke(3.0),
                    Point2 { x: fx, y: fy },
                    r + 8.0 + (remaining.to_num::<f32>() * 120.0), // 前摇开始时大，结束时小
                    0.3,
                    Color::from_rgba(255, 180, 80, 200),
                )?;
                canvas.draw(&ring, graphics::DrawParam::new());
            }

            // HP 条
            let bw = (r * 2.0).max(16.0);
            let ratio = (p.hp / p.max_hp).to_num::<f32>().clamp(0.0, 1.0);
            let y_bar = fy - r - 12.0;
            let bg = Mesh::new_rectangle(
                &ctx.gfx,
                DrawMode::fill(),
                graphics::Rect::new(fx - bw / 2.0, y_bar, bw, 5.0),
                Color::from_rgba(10, 12, 18, 190),
            )?;
            let fg = Mesh::new_rectangle(
                &ctx.gfx,
                DrawMode::fill(),
                graphics::Rect::new(fx - bw / 2.0, y_bar, bw * ratio, 5.0),
                hp_color(ratio),
            )?;
            canvas.draw(&bg, graphics::DrawParam::new());
            canvas.draw(&fg, graphics::DrawParam::new());
        }

        // 飞行物（石头：显影半径提示延时区；幻象假身：淡色假圆）
        for pr in self.world.projectiles.iter() {
            let px = pr.pos.x.to_num::<f32>() * self.scale + self.offset.x;
            let py = pr.pos.y.to_num::<f32>() * self.scale + self.offset.y;
            match pr.kind {
                game_core::world::ProjectileKind::Rock { radius, .. } => {
                    let color = if pr.alive {
                        Color::from_rgba(230, 120, 60, 200)
                    } else {
                        Color::from_rgba(120, 60, 30, 160)
                    };
                    let pmesh = Mesh::new_circle(
                        &ctx.gfx,
                        DrawMode::stroke(2.0),
                        Point2 { x: px, y: py },
                        radius.to_num::<f32>() * self.scale,
                        0.5,
                        color,
                    )?;
                    canvas.draw(&pmesh, graphics::DrawParam::new());
                    let dot = Mesh::new_circle(
                        &ctx.gfx,
                        DrawMode::fill(),
                        Point2 { x: px, y: py },
                        4.0,
                        0.5,
                        Color::from_rgb(240, 140, 70),
                    )?;
                    canvas.draw(&dot, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Decoy { radius, .. } => {
                    // 幻象假身：淡色、接近玩家颜色的圆
                    let dec = Mesh::new_circle(
                        &ctx.gfx,
                        DrawMode::fill(),
                        Point2 { x: px, y: py },
                        radius.to_num::<f32>() * self.scale,
                        0.5,
                        Color::from_rgba(150, 160, 200, 120),
                    )?;
                    canvas.draw(&dec, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Bullet { dir, radius, .. } => {
                    // 直射弹：亮色小球，沿 dir 方向画一个速度小尾巴
                    let dirv = dir;
                    let tip_x = px + dirv.x.to_num::<f32>() * 6.0;
                    let tip_y = py + dirv.y.to_num::<f32>() * 6.0;
                    let tail = Mesh::new_line(
                        &ctx.gfx,
                        &[Point2 { x: px, y: py }, Point2 { x: tip_x, y: tip_y }],
                        2.0,
                        Color::from_rgba(255, 200, 80, 160),
                    )?;
                    canvas.draw(&tail, graphics::DrawParam::new());
                    let b = Mesh::new_circle(
                        &ctx.gfx,
                        DrawMode::fill(),
                        Point2 { x: px, y: py },
                        (radius.to_num::<f32>() * self.scale).max(4.0),
                        0.5,
                        Color::from_rgb(255, 210, 90),
                    )?;
                    canvas.draw(&b, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::PushBullet { dir, radius, .. } => {
                    // 撞击迟缓弹：暖橙龟球 + 大效果提示
                    let d = dir;
                    let tx = px + d.x.to_num::<f32>() * 8.0;
                    let ty = py + d.y.to_num::<f32>() * 8.0;
                    let tail = Mesh::new_line(&ctx.gfx, &[Point2 { x: px, y: py }, Point2 { x: tx, y: ty }], 3.0, Color::from_rgba(255, 150, 60, 190))?;
                    canvas.draw(&tail, graphics::DrawParam::new());
                    let b = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, (radius.to_num::<f32>() * self.scale).max(5.0), 0.5, Color::from_rgb(255, 160, 80))?;
                    canvas.draw(&b, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Missile { dir, radius, .. } => {
                    // 追踪导弹：深色大球 + 朝向小三角
                    let m = Mesh::new_circle(
                        &ctx.gfx,
                        DrawMode::fill(),
                        Point2 { x: px, y: py },
                        (radius.to_num::<f32>() * self.scale).max(5.0),
                        0.5,
                        Color::from_rgb(230, 90, 90),
                    )?;
                    canvas.draw(&m, graphics::DrawParam::new());
                    let d = dir;
                    let dx = d.x.to_num::<f32>();
                    let dy = d.y.to_num::<f32>();
                    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
                    let (ux, uy) = (dx / len, dy / len);
                    let tip = Point2 { x: px + ux * 12.0, y: py + uy * 12.0 };
                    let back = Point2 { x: px, y: py };
                    let line = Mesh::new_line(
                        &ctx.gfx,
                        &[back, tip],
                        3.0,
                        Color::from_rgb(240, 120, 60),
                    )?;
                    canvas.draw(&line, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Beam { dir, length, width, .. } => {
                    // 激光线：从 pr.pos 朝 dir 延伸 length 的亮色线段
                    let d = dir;
                    let fx = d.x.to_num::<f32>();
                    let fy = d.y.to_num::<f32>();
                    let f = length.to_num::<f32>() * self.scale;
                    let tip = Point2 { x: px + fx * f, y: py + fy * f };
                    let beam = Mesh::new_line(
                        &ctx.gfx,
                        &[Point2 { x: px, y: py }, tip],
                        (width.to_num::<f32>() * self.scale).max(3.0),
                        Color::from_rgba(150, 230, 255, 200),
                    )?;
                    canvas.draw(&beam, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Rolling { dir, radius, .. } => {
                    // 滚动火球：暖色球 + 旋转小拖尾（示意在滚动）
                    let d = dir;
                    let tx = px + d.x.to_num::<f32>() * 10.0;
                    let ty = py + d.y.to_num::<f32>() * 10.0;
                    let tail = Mesh::new_line(&ctx.gfx, &[Point2 { x: px, y: py }, Point2 { x: tx, y: ty }], 3.0, Color::from_rgba(255, 140, 60, 180))?;
                    canvas.draw(&tail, graphics::DrawParam::new());
                    let ball = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, (radius.to_num::<f32>() * self.scale).max(5.0), 0.5, Color::from_rgb(240, 120, 60))?;
                    canvas.draw(&ball, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::ScatterLine { dir, .. } => {
                    // 撒弹线：亮蓝移动小球 + 前方指示
                    let d = dir;
                    let tipx = px + d.x.to_num::<f32>() * 14.0;
                    let tipy = py + d.y.to_num::<f32>() * 14.0;
                    let tl = Mesh::new_line(&ctx.gfx, &[Point2 { x: px, y: py }, Point2 { x: tipx, y: tipy }], 4.0, Color::from_rgba(120, 200, 255, 220))?;
                    canvas.draw(&tl, graphics::DrawParam::new());
                    let dot = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, 6.0, 0.5, Color::from_rgb(90, 180, 255))?;
                    canvas.draw(&dot, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Boomerang { vel, radius, .. } => {
                    // 回旋镖：绿色球 + 沿速度方向的拖尾
                    let dv = vel;
                    let tx = px + dv.x.to_num::<f32>() * 0.4;
                    let ty = py + dv.y.to_num::<f32>() * 0.4;
                    let tail = Mesh::new_line(&ctx.gfx, &[Point2 { x: px, y: py }, Point2 { x: tx, y: ty }], 3.0, Color::from_rgba(120, 230, 140, 200))?;
                    canvas.draw(&tail, graphics::DrawParam::new());
                    let b = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, (radius.to_num::<f32>() * self.scale).max(5.0), 0.5, Color::from_rgb(90, 200, 110))?;
                    canvas.draw(&b, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Banana { dir, radius, .. } => {
                    // 香蕉弹：黄绿色曲线弹
                    let d = dir;
                    let tx = px + d.x.to_num::<f32>() * 12.0;
                    let ty = py + d.y.to_num::<f32>() * 12.0;
                    let tail = Mesh::new_line(&ctx.gfx, &[Point2 { x: px, y: py }, Point2 { x: tx, y: ty }], 3.0, Color::from_rgba(255, 220, 90, 200))?;
                    canvas.draw(&tail, graphics::DrawParam::new());
                    let b = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, (radius.to_num::<f32>() * self.scale).max(5.0), 0.5, Color::from_rgb(240, 200, 60))?;
                    canvas.draw(&b, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Chain { dir, .. } => {
                    // 链镖/跳弹：红色/品红亮球 + 朝向小拖尾
                    let d = dir;
                    let tx = px + d.x.to_num::<f32>() * 10.0;
                    let ty = py + d.y.to_num::<f32>() * 10.0;
                    let tail = Mesh::new_line(&ctx.gfx, &[Point2 { x: px, y: py }, Point2 { x: tx, y: ty }], 3.0, Color::from_rgba(255, 90, 140, 220))?;
                    canvas.draw(&tail, graphics::DrawParam::new());
                    let dot = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, 5.0, 0.5, Color::from_rgb(235, 90, 160))?;
                    canvas.draw(&dot, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::BonusBomb { dir, .. } => {
                    // 蓄力炸弹：橙红火球
                    let d = dir;
                    let tx = px + d.x.to_num::<f32>() * 10.0;
                    let ty = py + d.y.to_num::<f32>() * 10.0;
                    let tail = Mesh::new_line(&ctx.gfx, &[Point2 { x: px, y: py }, Point2 { x: tx, y: ty }], 3.0, Color::from_rgba(255, 160, 60, 220))?;
                    canvas.draw(&tail, graphics::DrawParam::new());
                    let dot = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, 6.0, 0.5, Color::from_rgb(255, 140, 60))?;
                    canvas.draw(&dot, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Returner { .. } => {
                    // 回返镖：青色小回镖
                    let dot = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, 5.0, 0.5, Color::from_rgb(120, 230, 220))?;
                    canvas.draw(&dot, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Tether { .. } => {
                    // 回拉线：蓝紫节点
                    let dot = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, 5.0, 0.5, Color::from_rgb(120, 140, 255))?;
                    canvas.draw(&dot, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Gravity { radius, .. } => {
                    // 引力场：半透明浅紫圈
                    let r = (radius.to_num::<f32>() * self.scale).max(8.0);
                    let ring = Mesh::new_circle(&ctx.gfx, DrawMode::stroke(2.0), Point2 { x: px, y: py }, r, 0.4, Color::from_rgba(170, 130, 255, 190))?;
                    canvas.draw(&ring, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Star { radius, .. } => {
                    // 星域：金色星形节点 + 半径
                    let r = (radius.to_num::<f32>() * self.scale).max(6.0);
                    let ring = Mesh::new_circle(&ctx.gfx, DrawMode::stroke(2.0), Point2 { x: px, y: py }, r, 0.4, Color::from_rgba(255, 220, 120, 190))?;
                    canvas.draw(&ring, graphics::DrawParam::new());
                    let dot = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, 5.0, 0.5, Color::from_rgb(255, 220, 120))?;
                    canvas.draw(&dot, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::BindLine { from, end, .. } => {
                    // 束缚线：一条束形线段
                    let fx = from.x.to_num::<f32>() * self.scale + self.offset.x;
                    let fy = from.y.to_num::<f32>() * self.scale + self.offset.y;
                    let ex = end.x.to_num::<f32>() * self.scale + self.offset.x;
                    let ey = end.y.to_num::<f32>() * self.scale + self.offset.y;
                    let line = Mesh::new_line(&ctx.gfx, &[Point2 { x: fx, y: fy }, Point2 { x: ex, y: ey }], 4.0, Color::from_rgba(200, 120, 255, 200))?;
                    canvas.draw(&line, graphics::DrawParam::new());
                }
            }
        }

        // 多局 meta 覆盖层（学习阶段 / 整场结束）
        self.draw_meta_overlay(&mut canvas, ctx)?;

        // 客户端掉线/重连覆盖层
        if self.conn_dropped {
            self.draw_reconnect_overlay(&mut canvas, ctx)?;
        }

        canvas.finish(ctx)?;
        Ok(())
    }

    /// 客户端掉线/重连提示覆盖层：提醒玩家已掉线，按 R 重连。
    fn draw_reconnect_overlay(&mut self, canvas: &mut Canvas, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        let dim = Mesh::new_rectangle(
            &ctx.gfx,
            DrawMode::fill(),
            graphics::Rect::new(0.0, 0.0, sw, sh),
            Color::from_rgba(8, 8, 12, 220),
        )?;
        canvas.draw(&dim, graphics::DrawParam::new());
        let cx = sw / 2.0;
        let status = if self.reconnect_attempting {
            "正在重连…"
        } else {
            "连接已断开"
        };
        draw_text(canvas, ctx, status, 42.0, Color::from_rgb(255, 190, 90), Point2 { x: cx, y: sh * 0.38 }, true)?;
        draw_text(canvas, ctx, "按 R 从 host 拉取快照重连", 24.0, Color::from_rgb(200, 205, 220), Point2 { x: cx, y: sh * 0.38 + 70.0 }, true)?;
        Ok(())
    }

    /// 渲染学习阶段 / 整场结束的信息覆盖层（无依赖文本，用简笔几何表示）。
    fn draw_meta_overlay(&mut self, canvas: &mut Canvas, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();

        match self.meta.phase {
            MatchPhase::Fighting => {
                // 技能冷却 HUD：底部一排 8 个键位槽，显示绑定技能图标/名称 + 冷却遮罩
                let self_idx = self.self_index();
                if let (Some(me), Some(me_player)) = (
                    self.meta.profiles.iter().find(|p| p.player_id == self_idx),
                    self.world.players.get(self_idx as usize),
                ) {
                    let slot_w = 56.0;
                    let slot_h = 56.0;
                    let gap = 12.0;
                    let n = game_core::skill::CastKey::ALL.len() as f32;
                    let total_w = n * slot_w + (n - 1.0) * gap;
                    let x0 = (sw - total_w) / 2.0;
                    let y0 = sh - slot_h - 24.0;
                    for (i, key) in game_core::skill::CastKey::ALL.iter().enumerate() {
                        let bx = x0 + i as f32 * (slot_w + gap);
                        let rect = graphics::Rect::new(bx, y0, slot_w, slot_h);
                        // 底板
                        let bg = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), rect, Color::from_rgba(15, 18, 26, 210))?;
                        canvas.draw(&bg, graphics::DrawParam::new());
                        let border = Mesh::new_rectangle(&ctx.gfx, DrawMode::stroke(2.0), rect, Color::from_rgba(90, 100, 120, 230))?;
                        canvas.draw(&border, graphics::DrawParam::new());

                        let skill = me.bound_skill(*key);
                        let slot_center = Point2 { x: bx + slot_w / 2.0, y: y0 + 22.0 };
                        // 技能名
                        let label = match skill {
                            Some(s) => game_core::skill::DefTable::def(s).name,
                            None => "—",
                        };
                        draw_text(canvas, ctx, key.letter(), 16.0, Color::from_rgb(200, 200, 215), Point2 { x: bx + 6.0, y: y0 + 4.0 }, true)?;
                        draw_text(canvas, ctx, label, 15.0, Color::WHITE, slot_center, true)?;

                        // 冷却遮罩 + 倒计时
                        if let Some(s) = skill {
                            let rem = me_player.caster.cooldown_remaining(s);
                            if rem > Fix64::ZERO {
                                let def = game_core::skill::DefTable::def(s);
                                let total = Fix64::from_num(def.growth.cooldown_base.max(0.1));
                                if total > Fix64::ZERO {
                                    let frac = (rem / total).to_num::<f32>().clamp(0.0, 1.0);
                                    let shade_h = slot_h * frac;
                                    let shade = Mesh::new_rectangle(
                                        &ctx.gfx,
                                        DrawMode::fill(),
                                        graphics::Rect::new(bx, y0, slot_w, shade_h),
                                        Color::from_rgba(30, 40, 60, 180),
                                    )?;
                                    canvas.draw(&shade, graphics::DrawParam::new());
                                    draw_text(canvas, ctx, &format!("{:.1}", rem.to_num::<f32>()), 20.0, Color::from_rgb(120, 200, 255), Point2 { x: bx + slot_w / 2.0, y: y0 + slot_h / 2.0 }, true)?;
                                }
                            }
                            // 前摇提示：该技能正在蓄力（Windup）时高亮描边 + 提示字
                            if let game_core::skill::CastPhase::Windup { id, .. } = me_player.caster.phase() {
                                if id == s {
                                    let wring = Mesh::new_rectangle(&ctx.gfx, DrawMode::stroke(3.0), rect, Color::from_rgb(255, 180, 80))?;
                                    canvas.draw(&wring, graphics::DrawParam::new());
                                    draw_text(canvas, ctx, "蓄力中", 15.0, Color::from_rgb(255, 200, 120), Point2 { x: bx + slot_w / 2.0, y: y0 + slot_h - 14.0 }, true)?;
                                }
                            }
                        }
                    }
                }
                // 操作提示：shift 连招队列
                draw_text(
                    canvas, ctx,
                    "Shift+右键 排移动 / Shift+技能(Shift+左键点目标) 排施法 / S 清空指令队列",
                    16.0, Color::from_rgba(170, 180, 200, 220),
                    Point2 { x: sw / 2.0, y: sh - 92.0 }, true)?;
            }
            MatchPhase::Learning => {
                // 半透明遮罩
                let dim = Mesh::new_rectangle(
                    &ctx.gfx,
                    DrawMode::fill(),
                    graphics::Rect::new(0.0, 0.0, sw, sh),
                    Color::from_rgba(8, 10, 16, 210),
                )?;
                canvas.draw(&dim, graphics::DrawParam::new());

                let cx = sw / 2.0;
                let mut y = sh * 0.18;

                let title = format!("第 {} / {} 局结束 —— 学习阶段", self.meta.round, self.meta.config.total_rounds);
                draw_text(canvas, ctx, &title, 34.0, Color::from_rgb(255, 210, 120), Point2 { x: cx, y }, true)?;
                y += 64.0;

                // 我的档案：金币 / 击杀 / 最佳名次
                if let Some(me) = self.meta.profiles.iter().find(|p| p.player_id == self.self_index()) {
                    let info = format!(
                        "金币 {}   击杀 {}   最佳名次 #{}",
                        me.gold, me.total_kills, me.best_placement
                    );
                    draw_text(canvas, ctx, &info, 26.0, Color::WHITE, Point2 { x: cx, y }, true)?;
                    y += 50.0;

                    // 提示操作
                    draw_text(
                        canvas, ctx,
                        "选键改技能：按 字母(C/R/E/D/Y/T/F/G) 选中该树 → 数字键选技能 → 按 = 升级，X 洗点",
                        20.0, Color::from_rgb(170,180,200), Point2 { x: cx, y }, true)?;
                    y += 44.0;

                    // 每个键：树名 + 已绑定技能
                    for key in game_core::skill::CastKey::ALL {
                        let bound = me.bound_skill(key);
                        let lv = bound.map(|s| me.skill_level(s)).unwrap_or(0);
                        let bound_txt = match bound {
                            Some(s) => format!("{} @Lv{lv}", game_core::skill::DefTable::def(s).name),
                            None => "未绑定".to_string(),
                        };
                        let color = if self.learn_tree_key == Some(key) {
                            Color::from_rgb(255, 210, 120)
                        } else {
                            Color::from_rgb(210, 215, 225)
                        };
                        let line = format!("[{}] {}树   {}", key.letter(), key.tree().name_zh(), bound_txt);
                        draw_text(canvas, ctx, &line, 21.0, color, Point2 { x: cx, y }, true)?;
                        y += 32.0;
                    }

                    // 被选中的树的技能选项
                    if let Some(key) = self.learn_tree_key {
                        y += 18.0;
                        draw_text(canvas, ctx, &format!("{} 树的技能（按数字选）：", key.letter()), 20.0, Color::from_rgb(255,210,120), Point2 { x: cx, y }, true)?;
                        y += 34.0;
                        for (i, skill) in key.tree().skills_in_tree().iter().enumerate() {
                            let line = format!("  {}  {}", i + 1, game_core::skill::DefTable::def(*skill).name);
                            draw_text(canvas, ctx, &line, 18.0, Color::from_rgb(220,220,230), Point2 { x: cx, y }, true)?;
                            y += 28.0;
                        }
                        y += 20.0;
                    }

                    y += 20.0;
                    draw_text(canvas, ctx, &format!("剩余学习时间：{:.0} 秒", self.meta.learn_remaining.max(0.0)), 22.0, Color::from_rgb(150,220,160), Point2 { x: cx, y }, true)?;
                }
            }
            MatchPhase::Finished => {
                // 终局结算
                let dim = Mesh::new_rectangle(
                    &ctx.gfx,
                    DrawMode::fill(),
                    graphics::Rect::new(0.0, 0.0, sw, sh),
                    Color::from_rgba(8, 10, 16, 230),
                )?;
                canvas.draw(&dim, graphics::DrawParam::new());
                let cx = sw / 2.0;
                let mut y = sh * 0.2;
                draw_text(canvas, ctx, "对局结束", 40.0, Color::from_rgb(255,190,90), Point2 { x: cx, y }, true)?;
                y += 70.0;
                // 按最佳名次排序展示所有玩家
                let mut sorted: Vec<_> = self.meta.profiles.iter().collect();
                sorted.sort_by_key(|p| p.best_placement);
                for p in sorted.iter() {
                    let line = format!("玩家{}  金币{}  击杀{}  最佳名次#{}", p.player_id, p.gold, p.total_kills, p.best_placement);
                    draw_text(canvas, ctx, &line, 24.0, Color::WHITE, Point2 { x: cx, y }, true)?;
                    y += 40.0;
                }
                y += 30.0;
                draw_text(canvas, ctx, "对局结束 —— 按 Q 返回主菜单", 22.0, Color::from_rgb(150, 200, 255), Point2 { x: cx, y: y + 30.0 }, true)?;
            }
        }
        Ok(())
    }
}

impl event::EventHandler for Game {
    fn update(&mut self, ctx: &mut Context) -> GameResult {
        let dt = ctx.time.delta().as_secs_f64();

        // 主菜单：轮询选择，其余模式走 MatchPhase。
        if self.app == AppState::MainMenu {
            use ggez::input::keyboard::Key;
            let just = |k: &str| ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.into()));
            if just("1") {
                // 单机试验场：world/meta 在构造时已是 1 玩家无 AI，直接切换即可。
                eprintln!("[menu] -> Solo");
                self.app = AppState::Solo;
                self.pre_game_config = true; // 先进开局配置
            } else if just("2") {
                eprintln!("[menu] 局域网需命令行：/--host <port> --players N / 或 /--join <host:port>");
            } else if just("3") {
                eprintln!("[menu] Steam 对战敬请期待。");
            }
            self.accumulator = 0.0;
            return Ok(());
        }

        // 开局前的技能配置（Solo 试验场 / 局域网）：选/升级技能，按 Space/O 开始第一局。
        // 开局前的技能配置（Solo 试验场 / 局域网）：选/升级技能，按 Space/O 开始第一局。
        // 注意：一旦进入配置同步（net_cfg != Idle，例如 host 按空格后 HostGather / client 上报后 ClientWait），
        // 本块必须【放行】到下面 Fighting 分支的同步逻辑，否则会一直 return、同步永不推进 → 卡死。
        if self.pre_game_config && self.app != AppState::MainMenu && self.net_cfg == NetCfgSync::Idle {
            // 局域网 host：开局配置阶段就同步接收 client 加入（不必等按了 Space 才开始收人），
            // 否则先到的 client 会因 host 未 poll_join 而握手超时。
            self.poll_host_join_phase();
            use ggez::input::keyboard::Key;
            self.poll_learning(ctx);
            // 空格或字母 O（确认）开始第一局。
            let done = ctx.keyboard.is_logical_key_just_pressed(&Key::Character(" ".into()))
                || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("o".into()));
            // 单机：超时自动用默认配置开始（防止窗口无焦点/按键收不到导致卡死）。
            let auto_done = if self.app == AppState::Solo && self.net_link.is_none() && self.net_host.is_none() && self.net_host_ls.is_none() {
                self.pre_game_timer -= dt;
                self.pre_game_timer <= 0.0
            } else {
                self.pre_game_timer = PRE_GAME_TIMEOUT_SECS; // 联网：不自动开始，重置计时供其它判断用
                false
            };
            if done {
                self.finish_pre_game();
            } else if auto_done {
                eprintln!("[solo] pre-game timeout -> auto-start with defaults");
                self.finish_pre_game();
            }
            self.accumulator = 0.0;
            return Ok(());
        }

        match self.meta.phase {
            MatchPhase::Finished => {
                // 整场对抗结束：不再模拟
                self.accumulator = 0.0;
                // 允许回主菜单：按 Q。
                use ggez::input::keyboard::Key;
                let q = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("q".into()))
                    || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("Q".into()));
                if q {
                    eprintln!("[meta] finished -> back to main menu");
                    self.reset_to_main_menu();
                }
                Ok(())
            }
            MatchPhase::Learning => {
                // 学习阶段：轮询购买升级输入 + 计时
                self.poll_learning(ctx);
                let now = self.meta.tick_learning(dt.min(0.25));
                // 若学习结束，准备进入下一局：联网需先做「配置同步」
                if self.meta.phase == MatchPhase::Fighting {
                    if self.net_link.is_some() {
                        eprintln!("[meta] round {} learning done -> ClientWait (config sync)", self.meta.round);
                        self.net_cfg = NetCfgSync::ClientWait;
                    } else if self.net_host_ls.is_some() {
                        eprintln!("[meta] round {} learning done -> HostGather (config sync)", self.meta.round);
                        self.net_cfg = NetCfgSync::HostGather;
                    } else {
                        eprintln!("[meta] round {} learning done -> next round (single)", self.meta.round);
                        self.teardown_round_end();
                    }
                }
                let _ = now;
                Ok(())
            }
            MatchPhase::Fighting => {
                // 每帧轮询输入（技能键 / 鼠标）
                self.poll_input(ctx);
                self.accumulator += dt.min(0.25);
                let ticking = Fix64::from_num(TICK);
                // 联网 · 开房作 host：接收 client 加入，全部到齐后移交 HostLockstep（不强制 READY/GO，
                // 由“host 收齐输入即产首帧”自然统一起始。）。
                self.poll_host_join_phase();
                if let Some(mut host) = std::mem::take(&mut self.net_host_ls) {
                    // 多于局：学习结束后的「配置同步」阶段——收齐各端配置(含自身) → 广播 PlayerCfgAll → 完成。
                    if self.net_cfg == NetCfgSync::HostGather {
                        let mut g_rcv = vec![0u8; 8192];
                        host.poll_cfg(&mut g_rcv);
                        let cfg_bytes = self.local_player_cfg();
                        if !cfg_bytes.is_empty() {
                            host.set_local_cfg(cfg_bytes);
                        }
                        if host.all_cfgs() {
                            let all = host.collect_cfgs().expect("all_cfgs 已确保收齐");
                            host.broadcast_cfgs(&all);
                            let stage = if self.pre_game_config { "pre-game" } else { "next round" };
                            eprintln!("[meta] host synced {} player configs -> {stage} (round {})", all.len(), self.meta.round);
                            self.apply_player_cfgs(&all);
                            self.teardown_round_end();
                            host.reset_cfgs(); // 为下一局复用
                            self.net_cfg = NetCfgSync::Idle;
                            self.pre_game_config = false;
                        }
                        self.net_host_ls = Some(host);
                        return Ok(()); // 同步阶段不推进战斗
                    }
                    // 联网 · host 运行：等齐 N 端输入才产 seq 帧（try_emit Some），用同帧喂自己 world 并广播。
                    let mut host_rcv = vec![0u8; 4096];
                    while self.accumulator >= TICK {
                        let me = self.local_player_input();
                        host.set_local_input(Some(game_core::netcode::encode_player_input(&me)));
                        host.poll(&mut host_rcv);
                        // 掉线判定：任一 client 空闲超时才自动 mark_dropped（不卡全队）。
                        for dropped_idx in host.auto_drop_idle(HOST_DROP_TICKS) {
                            eprintln!("[host] AUTO-DROP client {dropped_idx} (idle timeout) -> game continues");
                        }
                        if let Some((seq, frame)) = host.try_emit() {
                            if seq == 0 {
                                eprintln!("[host] emit seq=0: started, n_entries={}", frame.len());
                            }
                            let n = self.world.players.len();
                            let mut inputs = vec![PlayerInput::default(); n];
                            for (idx, bytes) in frame {
                                if (idx as usize) < n {
                                    inputs[idx as usize] =
                                        game_core::netcode::decode_player_input(&bytes).unwrap_or_default();
                                }
                            }
                            self.world.step(inputs, ticking);
                            // 周期保存快照（供掉线者重连时拉取当前状态接回）。
                            self.host_frame_count += 1;
                            if self.host_frame_count % SNAPSHOT_EVERY == 0 {
                                let wb = game_core::world_ser::world_to_bytes(&self.world);
                                host.set_snapshot(wb, host.next_seq());
                            }
                            self.accumulator -= TICK;
                        } else {
                            break; // 未收齐本帧：停在此 tick，下一帧 host.poll 会再收、补发缺失帧
                        }
                    }
                    self.net_host_ls = Some(host);
                } else if let Some(mut link) = std::mem::take(&mut self.net_link) {
                    // 多于局：学习结束后的「配置同步」阶段——上报我的配置，等 host 广播 PlayerCfgAll 后完成。
                    if self.net_cfg == NetCfgSync::ClientWait {
                        let cfg_bytes = self.local_player_cfg();
                        if !cfg_bytes.is_empty() {
                            link.upload_cfg(&cfg_bytes)?;
                        }
                        if let Some(all) = link.recv_cfg_all()? {
                            let stage = if self.pre_game_config { "pre-game" } else { "next round" };
                            eprintln!("[meta] client got {} player configs -> {stage} (round {})", all.len(), self.meta.round);
                            self.apply_player_cfgs(&all);
                            self.teardown_round_end();
                            self.net_cfg = NetCfgSync::Idle;
                            self.pre_game_config = false;
                        }
                        self.net_link = Some(link);
                        return Ok(()); // 同步阶段不推进战斗
                    }
                    // 联网：加入者 —— 每帧持续上行输入（让 host 能收齐并产首帧），并收帧推进。
                    // 收到首帧即开始（started 首帧凭底）；丢帧由 lockstep 自动补发，不会永久不同步。
                    // 若已判定掉线（conn_dropped）：冻结世界，等待重连入口（按 R）。
                    if self.conn_dropped {
                        // 只做重连尝试，不推进世界（避免与 host 分叉）。
                        self.poll_reconnect(ctx, &mut link);
                        self.net_link = Some(link);
                        self.accumulator = 0.0;
                        return Ok(());
                    }
                    while self.accumulator >= TICK {
                        let me = self.local_player_input();
                        let enc = game_core::netcode::encode_player_input(&me);
                        // 无条件上行（无论是否已收到首帧）。
                        link.upload(&enc)?;
                        // 收到权威帧则按权威推进（严格 lockstep，保证逐位一致）。
                        // 未收到帧【不乐观预测】——等待 host 的权威帧即可。乐观预测（4.7 阶段一）会与后续
                        // 权威帧叠加、导致本地 World 与 host 分叉（若要乐观手感需配完整回滚，见 LATENCY_MASKING 阶段二）。
                        if link.step_frame(&mut self.world, ticking)?.is_some() {
                            self.accumulator -= TICK;
                        } else {
                            link.bump_stale();
                            if link.stale_ticks() >= CLIENT_STALE_TICKS {
                                // 太久没收到权威帧 → 判定掉线，进入重连界面。
                                eprintln!("[client] NO frames for {} ticks -> connection dropped, waiting for reconnect (R)",
                                    link.stale_ticks());
                                self.conn_dropped = true;
                                return Ok(());
                            }
                            // 本 tick 不推进，等权威帧补齐（帧会由 lockstep 补发/排队，稍后追上）。
                            // 注意：不扣 accumulator，下一帧继续尝试收帧，避免时间凭空流逝导致分叉。
                            break;
                        }
                    }
                    self.net_link = Some(link);
                } else {
                    // 单机：Solo 试验场用「本机输入 + 其余(靶子)默认」；否则带 AI 机器人。
                    let is_solo = self.app == AppState::Solo;
                    while self.accumulator >= TICK {
                        if is_solo {
                            let me = self.self_index();
                            let n = self.world.players.len();
                            let mut inputs = vec![PlayerInput::default(); n];
                            if (me as usize) < n {
                                inputs[me as usize] = self.local_player_input();
                            }
                            self.world.step(inputs, ticking);
                        } else {
                            let inputs = self.compute_inputs();
                            self.world.step(inputs, ticking);
                        }
                        self.accumulator -= TICK;
                    }
                }
                // 一旦世界已进入前摇（施法被接受），清除待发送的施法命令，避免重复发送。
                if self.pending_cast.is_some() {
                    if let Some(p) = self.world.players.get(PLAYER_ID as usize) {
                        if p.caster.is_windup() {
                            self.pending_cast = None;
                        }
                    }
                }
                // 本局结束 → 结算并进入学习阶段
                if self.world.round_over() {
                    self.settle_round();
                }
                Ok(())
            }
        }
    }

    fn mouse_button_down_event(
        &mut self,
        _ctx: &mut Context,
        _button: ggez::input::mouse::MouseButton,
        _x: f32,
        _y: f32,
    ) -> GameResult {
        // 鼠标点击统一在 `poll_input` 里轮询处理；事件回调保持默认（无操作）。
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        if self.app == AppState::MainMenu {
            return self.draw_menu(ctx);
        }
        if self.pre_game_config {
            return self.draw_pre_game(ctx);
        }
        self.draw_scene(ctx)
    }
}

impl Game {
    /// 完成开局前的技能配置：第一局前同步各端 build（局域网走 HostGather/ClientWait），单机直接开打。
    /// 联网 host：接收 client 加入，全部到齐后把 `net_host` 移交为 `net_host_ls`。
    /// 在“开局配置”阶段与 Fighting 阶段都调用，确保 host 在等人时就开始收人（否则先到的 client 会握手超时）。
    fn poll_host_join_phase(&mut self) {
        if let Some(mut hs) = std::mem::take(&mut self.net_host) {
            let mut host_rcv = vec![0u8; 4096];
            hs.poll_join(&mut host_rcv);
            if hs.joined >= hs.expected() {
                eprintln!("[host] ALL {} clients joined -> hand to HostLockstep", hs.joined);
                let n = self.world.players.len();
                // 读各 client 槽位的稳定身份（Steam=SteamID，局域网=握手随机/指定），交付给 HostLockstep 供重连按身份找回。
                let expected_clients = n.saturating_sub(1); // host 参与占 player 0
                let identities: Vec<Option<u64>> = (0..expected_clients)
                    .map(|c| hs.identity_of((1 + c) as u8))
                    .collect();
                let transport = hs.into_transport();
                let mut host_ls = net::lockstep::HostLockstep::new(transport, n, true);
                host_ls.set_client_identities(&identities);
                self.net_host_ls = Some(host_ls);
                self.net_ready = true;
            } else {
                self.net_host = Some(hs); // 尚未收齐 client，继续等
            }
        }
    }

    /// 把整场对抗（Finished）退回主菜单：放弃当前网络连接，重建为 MainMenu 的沙盒世界/meta，并清空所有运行状态。
    fn reset_to_main_menu(&mut self) {
        let seed = 20260812u64;
        // 主菜单与 Solo 共用“2 玩家 + 靶子 + sandbox”世界（不判结束 / 不缩圈）。
        let mut w = game_core::world::World::new(2, seed);
        w.sandbox = true;
        self.world = w;
        self.meta = game_core::meta::MatchState::new(game_core::meta::MatchConfig::default(), &[0], 8);
        // 默认绑定：每个键绑定其树的首个技能，保证菜单进 Solo 时开局即可用。
        for profile in self.meta.profiles.iter_mut() {
            for key in game_core::skill::CastKey::ALL {
                if let Some(first) = key.tree().skills_in_tree().first() {
                    profile.bind_skill(key, *first);
                }
            }
        }
        self.app = AppState::MainMenu;
        // 放弃联网连接（UDP socket / 握手 / 帧同步关闭）。
        self.net_link = None;
        self.net_host = None;
        self.net_host_ls = None;
        self.net_ready = false;
        self.net_cfg = NetCfgSync::Idle;
        // 清空运行状态。
        self.pre_game_config = false; // 主菜单不进入开局配置；选了模式后再进。
        self.conn_dropped = false;
        self.reconnect_attempting = false;
        self.host_frame_count = 0;
        self.pre_game_timer = PRE_GAME_TIMEOUT_SECS;
        self.learn_tree_key = game_core::skill::CastKey::ALL.first().copied();
        self.bot_targets = Vec::new();
        self.bot_rngs = Vec::new();
        self.player_target = None;
        self.pending_cast = None;
        self.pending_skill = None;
        self.queued_cmds.clear();
        self.pending_shift_skill = None;
        self.pending_clear_signal = false;
        self.pending_stop_signal = false;
        self.accumulator = 0.0;
    }

    /// 开局前的技能配置（第一局开始前选择）。
    fn finish_pre_game(&mut self) {
        self.meta.enter_first_round(); // Fighting，round 保持 1
        if self.net_link.is_some() {
            eprintln!("[meta] pre-game done -> ClientWait config sync");
            self.net_cfg = NetCfgSync::ClientWait;
            // pre_game_config 保持 true，直到同步完成（见 Fighting 分支）。
        } else if self.net_host_ls.is_some() {
            eprintln!("[meta] pre-game done -> HostGather config sync");
            self.net_cfg = NetCfgSync::HostGather;
        } else {
            self.teardown_round_end();
            self.pre_game_config = false;
            eprintln!("[meta] pre-game config done -> round {} Fighting", self.meta.round);
        }
    }

    /// 开局前配置面板：显示当前绑定/等级/金币，提示按 Space 开始第一轮。
    fn draw_pre_game(&self, ctx: &mut Context) -> GameResult {
        let mut canvas = graphics::Canvas::from_frame(ctx, graphics::Color::from_rgb(18, 20, 26));
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;
        draw_text(&mut canvas, ctx, "开局 · 配置技能", 46.0, graphics::Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.12 }, true)?;
        draw_text(&mut canvas, ctx, "按 Space / O 开始第一轮", 22.0, graphics::Color::from_rgb(150, 200, 255), Point2 { x: cx, y: sh * 0.12 + 60.0 }, true)?;
        // 准备状态面板：显示各玩家已加入/已就绪，避免“以为卡住”。
        let me = self.self_index();
        if self.app != AppState::Solo {
            let mut r = sh * 0.12 + 96.0;
            draw_text(&mut canvas, ctx, "—— 玩家准备状态 ——", 20.0, graphics::Color::from_rgb(200, 210, 220), Point2 { x: cx, y: r }, true)?;
            r += 28.0;
            if let Some(host) = self.net_host_ls.as_ref() {
                let total = self.world.players.len();
                for i in 0..total {
                    let (name, ready) = if i == 0 {
                        ("host(你)".to_string(), host.local_cfg_ready())
                    } else {
                        (format!("玩家{i}"), host.client_cfg_ready(i as u8))
                    };
                    let (txt, col) = if ready {
                        (format!("  {name}  ✓ 已就绪"), Color::from_rgb(90, 220, 130))
                    } else if (i as u32) == me {
                        (format!("  {name}  ○ 等你按空格"), Color::from_rgb(240, 200, 70))
                    } else {
                        (format!("  {name}  ○ 等待上报"), Color::from_rgb(170, 175, 185))
                    };
                    draw_text(&mut canvas, ctx, &txt, 18.0, col, Point2 { x: cx, y: r }, true)?;
                    r += 26.0;
                }
            } else if let Some(hs) = self.net_host.as_ref() {
                draw_text(&mut canvas, ctx, &format!("  已加入 {}/{} 个玩家", hs.joined, hs.expected()), 18.0, Color::from_rgb(170, 175, 185), Point2 { x: cx, y: r }, true)?;
                draw_text(&mut canvas, ctx, "  等所有玩家加入后：每个窗口先点击再按空格就绪", 17.0, Color::from_rgb(140, 160, 180), Point2 { x: cx, y: r + 26.0 }, true)?;
            } else {
                // client：显示自身是否已就绪。
                let ready = self.net_cfg == NetCfgSync::ClientWait;
                let (txt, col) = if ready {
                    ("  ✓ 已就绪，等待 host 开始…".to_string(), Color::from_rgb(90, 220, 130))
                } else {
                    ("  ○ 未就绪 —— 请先点击本窗口，再按空格就绪".to_string(), Color::from_rgb(240, 200, 70))
                };
                draw_text(&mut canvas, ctx, &txt, 18.0, col, Point2 { x: cx, y: r }, true)?;
            }
        }
        if self.app == AppState::Solo {
            draw_text(&mut canvas, ctx, &format!("（单机：{:.0} 秒后自动用默认配置开始）", self.pre_game_timer.max(0.0)), 17.0, graphics::Color::from_rgb(140, 160, 180), Point2 { x: cx, y: sh * 0.12 + 92.0 }, true)?;
        }
        // 列出本机玩家的键位绑定与等级。
        let me = self.self_index();
        if let Some(pr) = self.meta.profiles.iter().find(|p| p.player_id == me) {
            let mut y = sh * 0.30;
            let gold_line = format!("金币：{}    击杀：{}    最佳名次：#{}", pr.gold, pr.total_kills, pr.best_placement);
            draw_text(&mut canvas, ctx, &gold_line, 24.0, graphics::Color::from_rgb(220, 224, 232), Point2 { x: cx, y }, true)?;
            y += 40.0;
            // 当前选中树：高亮字样，提醒按了字母 C/R/E... 已选中哪棵/可选技能。
            if let Some(sel) = self.learn_tree_key {
                let sel_line = format!("[{}] {} 树（当前选中）", sel.letter(), sel.tree().name_zh());
                draw_text(&mut canvas, ctx, &sel_line, 24.0, graphics::Color::from_rgb(255, 210, 120), Point2 { x: cx, y }, true)?;
                y += 36.0;
                for (i, skill) in sel.tree().skills_in_tree().iter().enumerate() {
                    let star = if pr.bound_skill(sel) == Some(*skill) { "  ◀" } else { "" };
                    draw_text(&mut canvas, ctx, &format!("  按 {}  →  {}{}", i + 1, game_core::skill::DefTable::def(*skill).name, star), 20.0, graphics::Color::from_rgb(215, 220, 230), Point2 { x: cx, y }, true)?;
                    y += 30.0;
                }
            } else {
                draw_text(&mut canvas, ctx, "（尚未选树：按字母 C/R/E/D/Y/T/F/G 选中一棵后，按数字键选技能）", 19.0, graphics::Color::from_rgb(170, 175, 185), Point2 { x: cx, y }, true)?;
                y += 30.0;
            }
            y += 16.0;
            draw_text(&mut canvas, ctx, "各键当前绑定：", 20.0, graphics::Color::from_rgb(225, 228, 235), Point2 { x: cx, y }, true)?;
            y += 30.0;
            for key in game_core::skill::CastKey::ALL {
                let bound = pr.bound_skill(key);
                let lv = bound.map(|s| pr.skill_level(s)).unwrap_or(0);
                let txt = match bound {
                    Some(s) => format!("[{}] {}  @Lv{}", key.letter(), game_core::skill::DefTable::def(s).name, lv),
                    None => format!("[{}] （未绑定）", key.letter()),
                };
                let highlight = self.learn_tree_key == Some(key);
                draw_text(&mut canvas, ctx, &txt, 22.0, if highlight { Color::from_rgb(255, 210, 120) } else { Color::from_rgb(225, 228, 235) }, Point2 { x: cx, y }, true)?;
                y += 30.0;
            }
        }
        draw_text(&mut canvas, ctx, "字母键选树 · 数字键绑技能 · = 升级 · X 洗点", 18.0, graphics::Color::from_rgb(160, 170, 185), Point2 { x: cx, y: sh * 0.88 }, true)?;
        canvas.finish(ctx)?;
        Ok(())
    }


    /// 主菜单：标题 + 三个入口（单机试验场 / 局域网 / Steam 占位）。
    fn draw_menu(&self, ctx: &mut Context) -> GameResult {
        let mut canvas = graphics::Canvas::from_frame(ctx, graphics::Color::from_rgb(18, 20, 26));
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;
        let title = "帧同步圆球竞技场";
        draw_text(&mut canvas, ctx, title, 52.0, graphics::Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.18 }, true)?;
        draw_text(&mut canvas, ctx, "请选择模式（按数字键）", 22.0, graphics::Color::from_rgb(200, 205, 215), Point2 { x: cx, y: sh * 0.18 + 70.0 }, true)?;
        let items = [
            "1  单机技能试验场（无 AI）",
            "2  局域网 · 开房间 / 加入（暂用命令行 --host / --join）",
            "3  Steam 对战（敬请期待）",
        ];
        for (i, s) in items.iter().enumerate() {
            let y = sh * 0.40 + (i as f32) * 46.0;
            draw_text(&mut canvas, ctx, s, 26.0, graphics::Color::from_rgb(225, 228, 235), Point2 { x: cx, y }, true)?;
        }
        draw_text(&mut canvas, ctx, "也可用命令行直通：--solo / --host <port> / --join <host:port>", 16.0, graphics::Color::from_rgb(150, 155, 165), Point2 { x: cx, y: sh * 0.90 }, true)?;
        canvas.finish(ctx)?;
        Ok(())
    }
}


fn player_color(id: u32, me: u32) -> Color {
    if id == me {
        Color::from_rgb(90, 200, 160)
    } else {
        const PALETTE: [[u8; 3]; 10] = [
            [240, 120, 100],
            [100, 150, 240],
            [245, 200, 90],
            [180, 120, 220],
            [90, 210, 230],
            [240, 160, 60],
            [150, 220, 120],
            [230, 130, 200],
            [160, 180, 90],
            [200, 140, 110],
        ];
        let c = PALETTE[(id as usize) % PALETTE.len()];
        Color::from_rgb(c[0], c[1], c[2])
    }
}

fn hp_color(ratio: f32) -> Color {
    if ratio > 0.5 {
        Color::from_rgb(90, 220, 130)
    } else if ratio > 0.25 {
        Color::from_rgb(240, 200, 70)
    } else {
        Color::from_rgb(235, 80, 70)
    }
}

/// 在屏幕上居中绘制文本（用 ggez 内置默认字体）。
fn draw_text(
    canvas: &mut Canvas,
    ctx: &Context,
    text: &str,
    size: f32,
    color: Color,
    center: Point2<f32>,
    _centered: bool,
) -> GameResult {
    use ggez::graphics::{Text, TextFragment};
    use ggez::mint::Vector2;
    let fragment = TextFragment::new(text).color(color).scale(size).font("cjk".to_string());
    let mut t = Text::new(fragment);
    t.set_bounds(Vector2 { x: 2000.0, y: 200.0 });
    // 手动粗略居中：先量尺寸
    let sz = t.measure(&ctx.gfx)?;
    let dest = Point2 {
        x: center.x - sz.x / 2.0,
        y: center.y,
    };
    canvas.draw(&t, graphics::DrawParam::new().dest(dest));
    Ok(())
}

/// 本局运行模式已并入 `AppState`（主菜单 / Solo / 局域网主机 / 局域网加入）。
/// 解析命令行（可选直通入口）：`--join <host:port>` / `--host <port>` [--players N] / `--solo`；
/// 无匹配选项 → 返回主菜单。抽成纯函数便于回归测试（曾因 `--solo` 分支漏 `i += 1` 导致死循环）。
fn parse_app_from_args(args: &[String]) -> AppState {
    let mut app = AppState::MainMenu;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--solo" => {
                app = AppState::Solo;
                i += 1;
            }
            "--join" if i + 1 < args.len() => {
                if let Ok(a) = args[i + 1].parse() {
                    app = AppState::LanJoin { addr: a };
                }
                i += 2;
            }
            "--host" if i + 1 < args.len() => {
                let port: u16 = args[i + 1].parse().unwrap_or(0);
                app = AppState::LanHost { port, total: 4 };
                i += 2;
            }
            "--players" if i + 1 < args.len() => {
                if let AppState::LanHost { total, .. } = &mut app {
                    *total = args[i + 1].parse().unwrap_or(4);
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    app
}

fn main() -> GameResult {
    // 解析命令行（可选直通入口）：--join <host:port> / --host <port> [--players N] / --solo。
    // 若无任一参数 → 进主菜单选择。
    let args: Vec<String> = std::env::args().collect();
    let app = parse_app_from_args(&args);
    eprintln!("[main] app = {:?} args = {:?}", app, args[1..].to_vec());
    eprintln!("[main] building ggez context (window)...");

    let (mut ctx, event_loop) = ggez::ContextBuilder::new("frame-sync-arena", "remake")
        .window_setup(ggez::conf::WindowSetup::default().title("帧同步圆球竞技场 — 阶段1"))
        .window_mode(
            ggez::conf::WindowMode::default()
                .dimensions(1280.0, 720.0)
                .resizable(true),
        )
        .build()?;

    let game = Game::new(&mut ctx, app)?;
    event::run(ctx, event_loop, game)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_parse_does_not_hang_and_selects_solo() {
        // 回归：`--solo` 曾因漏 `i += 1` 在命令行解析里死循环，导致单机无法启动。
        let s = |x: &[&str]| x.iter().map(|v| v.to_string()).collect::<Vec<_>>();
        assert_eq!(parse_app_from_args(&s(&["exe", "--solo"])), AppState::Solo);
        // 无参数 → 主菜单
        assert_eq!(parse_app_from_args(&s(&["exe"])), AppState::MainMenu);
        // host + players
        assert_eq!(
            parse_app_from_args(&s(&["exe", "--host", "9001", "--players", "6"])),
            AppState::LanHost { port: 9001, total: 6 }
        );
        // join
        assert_eq!(
            parse_app_from_args(&s(&["exe", "--join", "127.0.0.1:5199"])),
            AppState::LanJoin { addr: "127.0.0.1:5199".parse().unwrap() }
        );
    }
}
