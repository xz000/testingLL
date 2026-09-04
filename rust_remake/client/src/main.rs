//! ggez 客户端 —— 阶段 1：核心玩法单机 demo。
//!
//! - 玩家圆：**右键**设置移动目标点，圆球匀速走过去，到达即停
//! - 场地逐渐收缩，出界扣血；球被挤到边缘/相互重叠会受压损血
//! - 若干机器人（确定性 AI）在同一场地游走，演示多人对抗氛围
//!
//! 玩法逻辑全部在 `game-core` 的 `World` 中，本文件只负责输入采集与渲染。

// 发布版（release）在 Windows 上使用 GUI 子系统：不弹出黑色命令行窗口，改善玩家体验。
// debug 构建仍保留 console（便于本地看日志/调试）。publish.ps1 走 release，自动生效。
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use game_core::fix::{cos, sin, Fix64, Vec2};
use game_core::meta::{MatchConfig, MatchPhase, MatchState};
use game_core::rng::Rng;
use game_core::skill::SkillId;
use game_core::world::{PlayerInput, World};
use ggez::event::{self, EventHandler as _};
use ggez::graphics::{self, Canvas, Color, DrawMode, Mesh};
// 让 `SteamTransport::send_stats()`（trait 默认实现由 net-steam 覆盖）在泛型代码里可直接调用。
#[cfg(feature = "steam")]
use net::transport::Transport as _;
use ggez::mint::Point2;
use ggez::{Context, GameResult};

mod netlink;

// Steam 联机逻辑（feature 门控，独立模块便于阅读维护；字段与方法均属 `Game`，纯逻辑分组）。
mod steam;

/// 机器人数量（不含玩家本人）。当前 Solo/局域网均无本地 AI；保留该常量供将来“带 AI 测试”模式复用。
#[allow(dead_code)]
const BOTS: u32 = 7;
/// 固定步长模拟（帧率）
const TICK: f64 = 1.0 / 60.0;
/// 玩家本人 = id 0
const PLAYER_ID: u32 = 0;

/// host：客户端连续空闲这么多帧判定为掉线（自动 mark_dropped，不卡全队）。约 3 秒。
const HOST_DROP_TICKS: u32 = 180;
/// host 配置同步的稳定等待帧数：`all_cfgs` 满足后再等这么多帧（期间继续收最新配置），
/// 避免上一局在途旧包让 `all_cfgs` 提前满足、广播旧配置（修复局间绑定被清空的竞态）。
const HOST_CFG_SETTLE_TICKS: u32 = 15;
/// host：每隔多少帧保存一次世界快照（供重连）。约 0.5 秒。
const SNAPSHOT_EVERY: u64 = 30;
/// client：连续多少帧未收到权威帧判定为“掉线/等待重连”（进入重连 UI）。约 3 秒。
const CLIENT_STALE_TICKS: u64 = 180;
/// Steam（client）：进入迁移后，探测“host 是否还在”的帧数（发 ReconnectReq 等 Snapshot 应答）。约 1 秒。
/// 超时无应答则判定 host 掉线，开始选举新 host。
#[cfg(feature = "steam")]
const MIGRATE_PROBE_TICKS: u64 = 60;/// 单机开局配置超时：等这么久没按开始就用默认配置自动开始第一轮（避免窗口没焦点/按键收不到导致卡死）。
const PRE_GAME_TIMEOUT_SECS: f64 = 60.0;
/// 每局发放的成长点（4.6b，占位数值，后续平衡）。
const GROWTH_PER_ROUND: u32 = 3;
/// 用金币兑换 1 成长点的成本（占位）。
const GOLD_PER_GROWTH: i32 = 20;
/// 某属性已有 `cur` 点时，买下一级成长点的成本（便宜斜坡，占位）。
fn growth_attr_cost(cur: u32) -> u32 {
    cur + 1 // base 1，每级 +1
}

/// Steamworks 应用 AppID（对应根目录 `steam_appid.txt` = 908660）。
#[cfg(feature = "steam")]
const APP_ID: u32 = 908660;/// Steam P2P 虚拟端口（host/peer 约定一致）。
#[cfg(feature = "steam")]
const STEAM_VIRTUAL_PORT: i32 = 1337;
/// Steam 对局确定性世界种子（各端一致，重建 world 用）。
#[cfg(feature = "steam")]
const STEAM_SEED: u64 = 20260812;
/// Steam 房间：全员就绪后进入配置/开战的缓冲倒计时（秒）。
#[cfg(feature = "steam")]
const STEAM_READY_COUNTDOWN_SECS: f32 = 5.0;
/// Steam 房间：倒计时最后这么多秒**不允许取消就绪**（锁定），防止临界竞态（避免有人卡最后一瞬取消导致不同步）。
#[cfg(feature = "steam")]
const STEAM_COUNTDOWN_LOCK_SECS: f32 = 2.0;
/// Steam 建房：玩家人数上限允许的最大值（steamworks 支持到 250，这里给竞技场设实际可玩的上限）。
#[cfg(feature = "steam")]
const STEAM_MAX_PLAYERS: u8 = 64;
/// Steam 建房：创建大厅时等待回调的最大拍数（每拍 ≈50ms 内 pump 一次 Steam 回调）。
/// 500 拍 ≈ 25s，给慢网络/Steam 后端留足时间（此前 200 拍 ≈ 10s 偶发超时）。
#[cfg(feature = "steam")]
const STEAM_LOBBY_CREATE_BEATS: u32 = 500;
/// Steam 建房：默认玩家数（创建房间界面的初始值）。
#[cfg(feature = "steam")]
const STEAM_DEFAULT_PLAYERS: u8 = 2;
/// Steam 建房：总轮数上限允许的最大值（256 基本无限制）。
#[cfg(feature = "steam")]
const STEAM_MAX_ROUNDS: u32 = 256;
/// Steam 建房：默认总轮数（创建房间界面的初始值，与 MatchConfig 默认一致）。
#[cfg(feature = "steam")]
const STEAM_DEFAULT_ROUNDS: u32 = 3;
/// Steam 建房：局间准备时间（秒）的最小值。
#[cfg(feature = "steam")]
const STEAM_MIN_LEARN_SECS: u32 = 8;
/// Steam 建房：局间准备时间（秒）的最大值。
#[cfg(feature = "steam")]
const STEAM_MAX_LEARN_SECS: u32 = 256;
/// Steam 建房：局间准备时间（秒）默认值（与 MatchConfig 默认一致）。
#[cfg(feature = "steam")]
const STEAM_DEFAULT_LEARN_SECS: u32 = 20;
/// Steam 建房：金币类字段的单档金额上限（初始金币 / 每轮金币 / 每档名次奖励）。
#[cfg(feature = "steam")]
const STEAM_MAX_GOLD: i32 = 99999;
/// Steam 建房：开局初始金币默认值（第一局开始前一次性发放，与每轮参与奖独立叠加；默认 0）。
#[cfg(feature = "steam")]
const STEAM_DEFAULT_STARTING_GOLD: i32 = 0;
/// Steam 建房：每轮固定金币（参与奖）默认值（与 MatchConfig 默认一致）。
#[cfg(feature = "steam")]
const STEAM_DEFAULT_GOLD_PER_ROUND: i32 = 20;
/// Steam 建房：名次奖励默认输入（单数字 = 第一名奖励，自动按 0.6 比例递减到 0；
/// 也可输入逗号分隔档位 `30,20,10` 手动精确控制）。
#[cfg(feature = "steam")]
const STEAM_DEFAULT_PLACE_REWARD: &str = "30";
/// Steam 建房：名次奖励默认第一名金额（配合自动递减）。
#[cfg(feature = "steam")]
const STEAM_DEFAULT_PLACE_FIRST: i32 = 30;

/// 由「第一名奖励」自动生成名次奖励档位：每降一名奖励 ×0.6（向下取整），直到 ≤0。
/// 这样只需输一个数字即可覆盖任意玩家数（档位只影响前几名，后几名逐渐归零）。
#[cfg(feature = "steam")]
fn auto_place_rewards(first: i32) -> Vec<i32> {
    let mut out = Vec::new();
    let mut v = first.max(0);
    while v > 0 {
        out.push(v);
        if out.len() >= 64 {
            break;
        }
        v = (v as f64 * 0.6).floor() as i32;
    }
    if out.is_empty() {
        out.push(0);
    }
    out
}
/// Steam 房间列表：两次刷新（`request_lobby_list`）之间的最小间隔（秒）。
/// Steam 对大厅搜索接口有限速（Steam 官方建议每秒至多一次）；频繁触发会拿到空/陈旧结果（今实测 `1->0->1` 漂忽）。
#[cfg(feature = "steam")]
const LOBBY_REFRESH_COOLDOWN_SECS: f64 = 4.0;
/// Steam 房间（client）：连续这么多帧收不到 host 的 `RosterReady` 广播，即判定 host 已离开（自动退出房间回主菜单）。约 4 秒。
#[cfg(feature = "steam")]
const STEAM_LOBBY_SILENT_TIMEOUT_TICKS: u32 = 240;
/// Rich Presence：相同内容的最小重写间隔（秒）。内容变化时立即写，否则节流（Steam 对频繁 set 有限速）。
#[cfg(feature = "steam")]
const STEAM_PRESENCE_INTERVAL_SECS: f64 = 3.0;

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
    /// Steam：开厅作 host（自身=player0，其余由大厅成员按 SteamID 排序）。
    #[cfg(feature = "steam")]
    SteamHost { players: u8 },
    /// Steam：按 matchkey 自动加入 host 大厅（client）；`Some(lobby_id)` = 手动指定 host 打印的 LobbyId（fallback）。
    #[cfg(feature = "steam")]
    SteamJoin { lobby_id: Option<u64> },
}

/// 进行中的 Steam 大厅操作类型（S12：帧驱动异步，避免在游戏线程 `std::thread::sleep` 忙等）。
/// `enter_steam_mode` / CLI 启动只发起操作（`start_*`）并记下类型，真正「进房」由 `update` 每帧
/// `run_callbacks` 后 `tick_lobby` 完成、再调用 `finish_enter_steam_mode` 落地（建 lockstep/世界/战绩）。
#[cfg(feature = "steam")]
enum SteamLobbyPending {
    /// 建房（host）。`players` = 请求的玩家总数（含 host），进房后据此建 HostLockstep。
    Host { players: u8 },
    /// 加入（client）。`lobby_id` = 手动指定的大厅 id（好友分享/LobbyId），`None` = 按 matchkey 自动搜索。
    Join { lobby_id: Option<u64> },
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
    /// 本机角色上一帧是否处于施法中（`note_self_cast` 用：检测"刚进入施法"的边沿来清移动目标）。
    self_was_busy: bool,
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
    /// 每帧递增的帧计数（用于 IME 去重，见 `last_ime_commit_frame`）。
    frame: u64,
    /// IME 去重：最近一次 `Ime::Commit` 提交的帧（置为当时 `frame+1`，即 `just(c)` 将要运行的下一帧）。
    /// `just(c)` ASCII 白名单在该帧跳过，避免同一物理键重复插入（C8，需真机验证）。
    last_ime_commit_frame: u64,
    /// 世界坐标 → 屏幕坐标的缩放
    scale: f32,
    /// 相机偏移（竞技场中心在画面中央）
    offset: Point2<f32>,
    /// 联网模式：加入 host 后用于每帧收发/喂 World；`None` = 单机（含本地 AI 机器人）。
    net_link: Option<netlink::NetLinkUdp>,
    /// 局域网模式：本机在对局中的玩家序号（握手分配）。`net_link` 被 `mem::take` 临时置 None 时用它，
    /// 避免 `self_index` 回落到 `PLAYER_ID=0`、导致配置同步上报错 profile（同 steam_active 的用意）。
    lan_my_index: u8,
    /// Steam 联机：host 端帧同步（feature=steam）。
    #[cfg(feature = "steam")]
    steam_host_ls: Option<net::lockstep::HostLockstep<net_steam::SteamTransport>>,
    /// Steam 联机：client 端帧同步（feature=steam）。
    #[cfg(feature = "steam")]
    steam_cli_ls: Option<net::lockstep::ClientLockstep<net_steam::SteamTransport>>,
    /// Steam 联机：本机在大厅里的玩家槽位（`self_index` 用）。
    #[cfg(feature = "steam")]
    steam_my_index: u8,
    /// Steam 联机：是否处于 Steam 对局（进入房间即 true，回主菜单 false）。
    /// 用于 `self_index`/`steam_active` 判断——**不能**依赖 `steam_cli_ls`/`steam_host_ls`
    ///（它们会被 `mem::take` 临时置 None，导致取 `PLAYER_ID=0`、配置上报用错 profile）。
    #[cfg(feature = "steam")]
    steam_active: bool,
    /// Steam 联机：本机 SteamID（重连时作稳定身份发给 host 找回槽位）。
    #[cfg(feature = "steam")]
    steam_my_id: u64,
    /// Steam（client）：连续未收到权威帧的 tick 数（Steam 战斗端掉线探测；对齐局域网 stale_ticks）。
    #[cfg(feature = "steam")]
    steam_cli_stale_ticks: u64,
    /// Steam：本局参与玩家的稳定身份（SteamID）按 new index 排列（host 广播 `Participants` 后保存，**原始、不变**）。
    /// world index → SteamID 的映射：对局开始时确定，迁移时据此定位本端 world index 与掉线 host 的 index。
    #[cfg(feature = "steam")]
    steam_participants: Vec<u64>,
    /// Steam：当前**在线**参与玩家 SteamID 集（初始 = steam_participants；每次迁移排除掉线 host，并随 Takeover 同步）。
    /// 用于选举新 host（排除已掉线的 host，避免把已掉线的旧 host 再选出）。
    #[cfg(feature = "steam")]
    steam_online: Vec<u64>,
    /// Steam（client）：是否正处于「主机迁移」流程中（host 掉线 → 选举/接管/重定向）。
    #[cfg(feature = "steam")]
    steam_migrating: bool,
    /// Steam（client）：迁移流程累计 tick（探测 host 存活 / 等待 Takeover）。
    #[cfg(feature = "steam")]
    steam_migrate_ticks: u64,
    /// Steam（client）：选举出的新 host SteamID（0 = 尚未决定）。
    #[cfg(feature = "steam")]
    steam_new_host_id: u64,
    /// Steam（host）：接管后是否仍在「持续广播 Takeover」（直到首个在线 client 连上、产出首帧才停）。
    /// 避免晚进入迁移的 client 错过单次 Takeover 广播。
    #[cfg(feature = "steam")]
    steam_host_broadcasting_takeover: bool,
    /// Steam：全体就绪后的倒计时（秒）。
    #[cfg(feature = "steam")]
    steam_countdown: f32,
    /// Steam：是否仍在「房间/就绪」阶段（进配置菜单前）。
    #[cfg(feature = "steam")]
    steam_in_lobby: bool,
    /// Steam：本机是否已就绪（按 o toggle，可撤销）。
    #[cfg(feature = "steam")]
    steam_local_ready: bool,
    /// Steam：房间成员名单（槽位, 昵称, SteamID）。host 每 ~0.5s 经 `steam_refresh_roster` 刷新（显示新进成员），
    /// client 加入时构建；房间就绪界面按此列出成员。
    #[cfg(feature = "steam")]
    steam_roster: Vec<(u8, String, u64)>,
    /// Steam：本机最近一次收到的 host 房间「就绪状态快照」（多人一致界面；client 侧显示各成员就绪用）。
    #[cfg(feature = "steam")]
    steam_roster_ready: Vec<(u8, bool)>,
    /// Steam（client）：最近一次 host 广播快照是否为「全员就绪」。**持久记录**：
    /// 只在收到新快照时更新，避免“本帧恰好没收到广播”就回退 false 导致界面在“按 U 就绪/倒计时”间闪烁。
    #[cfg(feature = "steam")]
    steam_roster_all_ready: bool,
    /// Steam：全体就绪是否为真（host 计算；房间/就绪界面显示用）。
    #[cfg(feature = "steam")]
    steam_all_ready: bool,
    /// Steam：本机最近一次上报给 host 的就绪值（用于节流打印发送结果/变更）。
    #[cfg(feature = "steam")]
    steam_last_sent_ready: Option<bool>,
    /// Steam：房间阶段等待满员/就绪的累计帧数（用于节流打印诊断，避免每帧刷屏）。
    #[cfg(feature = "steam")]
    steam_lobby_wait_ticks: u32,
    /// Steam：本端是否已在「开局配置」阶段配好技能/配置（配完即置 true，host 据此收集各端 build_done 统一开战）。
    #[cfg(feature = "steam")]
    steam_build_done: bool,
    /// Steam（host）：房间阶段是否已经历过「全员就绪」状态（用于倒计时只在真正全员就绪后才开始，避免边界"秒进/永不进"）。
    #[cfg(feature = "steam")]
    steam_was_all_ready: bool,
    /// Steam：client 最近推进到的帧号（诊断：确认是否在收 host 权威帧）。
    #[cfg(feature = "steam")]
    steam_cli_last_seq: u64,
    /// Steam：主菜单内是否处于「大厅选择」子菜单（H 创建 / J 加入 / Q 返回）。
    #[cfg(feature = "steam")]
    steam_lobby_menu: bool,
    /// Steam 大厅子菜单（创建/加入/返回）的选中项（支持上下箭头 + 鼠标）。
    #[cfg(feature = "steam")]
    steam_lobby_selection: usize,
    /// Steam：是否处于「建房设置」界面（房间名/备注/人数，回车创建 / Q 取消）。
    #[cfg(feature = "steam")]
    steam_lobby_create: bool,
    /// Steam：是否处于「房间列表」界面（浏览公开大厅，方向键选中+回车加入 / R 刷新 / Q 返回）。
    #[cfg(feature = "steam")]
    steam_lobby_list: bool,
    /// Steam 建房设置：房间名当前输入。
    #[cfg(feature = "steam")]
    steam_create_name: String,
    /// Steam 建房设置：备注当前输入（可空）。
    #[cfg(feature = "steam")]
    steam_create_note: String,
    /// Steam 建房设置：当前聚焦字段（0=房间名，1=备注，2=人数）。
    #[cfg(feature = "steam")]
    steam_create_focus: usize,
    /// Steam：房间列表（缓存的公开大厅信息，供浏览选房）。
    #[cfg(feature = "steam")]
    steam_list_lobbies: Vec<net_steam::session::LobbyInfo>,
    /// Steam：房间列表当前选中项。
    #[cfg(feature = "steam")]
    steam_list_selection: usize,
    /// Steam：是否为房间列表拉取过一次（true=已在加载/已加载，避免反复 request_lobby_list）。
    #[cfg(feature = "steam")]
    steam_list_requested: bool,
    /// Steam：房间列表上次刷新的时间戳（秒，用于刷新节流，避免 Steam 搜索限速）。
    #[cfg(feature = "steam")]
    steam_list_last_refresh: f64,
    /// Steam：整个大厅流程持有的一次性 Steam 会话（进入大厅时 init 一次，建房/加入消费之；避免重复 init 单实例 steamworks）。
    #[cfg(feature = "steam")]
    steam_sess: Option<net_steam::session::SteamSession>,
    /// Steam：本机昵称（建房默认房间名/大厅展示用；进入大厅时读取并缓存）。
    #[cfg(feature = "steam")]
    steam_my_display_name: String,
    /// Steam：客户端要加入的指定大厅 LobbyId（从房间列表选中时设置；`enter_steam_mode` client 分支优先用其加入）。
    #[cfg(feature = "steam")]
    steam_join_lobby_id: Option<u64>,
    /// Steam：进行中的大厅操作（S12 异步）。`Some` = 已 `start_*` 发起、等待 `update` 每帧 `tick_lobby` 完成；
    /// 同时充当「连接中」状态（`draw`/`update` 据此显示「连接中…」并跳过菜单/房间输入）。`None` = 空闲/已进房。
    #[cfg(feature = "steam")]
    steam_lobby_pending: Option<SteamLobbyPending>,
    /// Steam：房主是否处于「编辑房间信息」子界面（房间就绪界面按 E 进入，回车保存 / Q 取消）。
    #[cfg(feature = "steam")]
    steam_room_edit: bool,
    /// Steam 房主编辑：当前聚焦字段（0=房间名，1=备注）。
    #[cfg(feature = "steam")]
    steam_room_edit_focus: usize,
    /// Steam 房主编辑：房间名当前编辑内容。
    #[cfg(feature = "steam")]
    steam_edit_name: String,
    /// Steam 房主编辑：备注当前编辑内容。
    #[cfg(feature = "steam")]
    steam_edit_note: String,
    /// Steam：当前所在房间的 LobbyId（编辑房间信息/锁房用；host/client 进入房间后设置）。
    #[cfg(feature = "steam")]
    steam_lobby_id: Option<u64>,
    /// Steam：当前房间是否已锁定（他人不能再加入；host 用 set_lobby_joinable 控制，本端记录状态）。
    #[cfg(feature = "steam")]
    steam_room_locked: bool,
    /// Steam：host 房间阶段刷新成员名单（roster）的节流计数器（client 加入后 host 才能看到并显示新成员）。
    #[cfg(feature = "steam")]
    steam_roster_refresh_ticks: u32,
    /// Steam：client 连续收不到 host 广播（RosterReady 心跳）的帧数；超过 `STEAM_LOBBY_SILENT_TIMEOUT_TICKS` 判定 host 已离开。
    #[cfg(feature = "steam")]
    steam_lobby_silent_ticks: u32,
    /// Steam：host 上次刷新 roster 看到的成员数（用于检测“有玩家离开”并提示 host，避免误以为卡住）。
    #[cfg(feature = "steam")]
    steam_last_roster_len: usize,
    /// Steam（host）：当前是否处于“不满员但在线者都已就绪、等待 host 手动开始”的状态（供界面提示）。
    #[cfg(feature = "steam")]
    steam_manual_start_pending: bool,
    /// Steam（host）：不满员时 host 已按回车确认，正在走与满员相同的开战倒计时。
    /// 单独存一个标志是因为倒计时的复位条件是「满员全员就绪」，而手动路径**永远不满员**，
    /// 若不额外记录，倒计时会在下一帧被复位条件立刻清掉。
    #[cfg(feature = "steam")]
    steam_manual_countdown: bool,
    /// Steam：host 广播的「不满员手动倒计时」剩余毫秒（client 端显示倒计时/最后 LOCK 秒锁定取消用）。
    /// 0=未激活。随 RosterReady 心跳每帧下发，client 持久记录，避免没收包那一帧回退闪烁。
    #[cfg(feature = "steam")]
    steam_manual_ms: u16,
    /// 联网模式：开房作 host，建连/握手阶段（自身=player 0）。
    net_host: Option<net::handshake::HostHandshake<net::transport::StdUdpTransport>>,
    /// 联网模式：开房作 host，运行阶段。
    net_host_ls: Option<net::lockstep::HostLockstep<net::transport::StdUdpTransport>>,
    /// host 配置同步稳定等待计数（见 HOST_CFG_SETTLE_TICKS）。
    host_cfg_settle: u32,
    /// host 配置同步首次进入标记：本轮 HostGather 是否已清空在途旧包（`drain_cfg`+`reset_cfgs`）。
    host_cfg_drained: bool,
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
    /// 重连已持续尝试的帧数（S1 stall-abort 用，仅 Steam 路径使用）：超过阈值仍无快照则放弃本次重连，避免无限重试卡死。
    #[cfg(feature = "steam")]
    reconnect_stall_ticks: u32,
    /// 主机端累计产帧数（用于周期保存快照）。
    host_frame_count: u64,
    /// 单机开局配置剩余的等待秒数（超时自动用默认配置开始，避免“按键无反应卡死”）。
    pre_game_timer: f64,
    /// 主菜单当前选中项（方向键 ↑/↓ 移动 + 回车确认；数字键直选同步更新）。
    menu_selection: usize,
    /// 主菜单「Steam 大厅」子界面里，创建房间的玩家人数上限（2..=STEAM_MAX_PLAYERS）。
    #[cfg(feature = "steam")]
    steam_create_players: u8,
    /// Steam 建房设置：总轮数（1..=STEAM_MAX_ROUNDS）。
    #[cfg(feature = "steam")]
    steam_create_rounds: u32,
    /// Steam 建房设置：玩家人数输入缓冲（字符串编辑，支持 Backspace 逐位删）。
    #[cfg(feature = "steam")]
    steam_create_players_buf: String,
    /// Steam 建房设置：总轮数输入缓冲（字符串编辑，支持 Backspace 逐位删）。
    #[cfg(feature = "steam")]
    steam_create_rounds_buf: String,
    /// Steam 建房设置：局间准备时间（秒，STEAM_MIN_LEARN_SECS..=STEAM_MAX_LEARN_SECS）。
    #[cfg(feature = "steam")]
    steam_create_learn: u32,
    /// Steam 建房设置：局间准备时间输入缓冲（字符串编辑，支持 Backspace 逐位删）。
    #[cfg(feature = "steam")]
    steam_create_learn_buf: String,
    /// Steam 建房设置：开局初始金币（0..=STEAM_MAX_GOLD）。
    #[cfg(feature = "steam")]
    steam_create_starting_gold: i32,
    /// Steam 建房设置：开局初始金币输入缓冲。
    #[cfg(feature = "steam")]
    steam_create_starting_gold_buf: String,
    /// Steam 建房设置：每轮固定金币（参与奖，0..=STEAM_MAX_GOLD）。
    #[cfg(feature = "steam")]
    steam_create_gold_per_round: i32,
    /// Steam 建房设置：每轮固定金币输入缓冲。
    #[cfg(feature = "steam")]
    steam_create_gold_per_round_buf: String,
    /// Steam 建房设置：名次奖励输入（单个数字 = 第一名，自动按 0.6 递减到 0；
    /// 或逗号分隔手动档位，如 "30,20,10"）。
    #[cfg(feature = "steam")]
    steam_create_place_buf: String,
    /// Steam 建房设置：解析后的名次奖励档位（索引 = 名次-1）。
    #[cfg(feature = "steam")]
    steam_create_place: Vec<i32>,
    /// 当前场次局间准备时间（秒；host 建房设定 / client 从大厅元数据读取，两端一致）。
    #[cfg(feature = "steam")]
    match_learn_secs: u32,
    /// 当前场次开局初始金币（host 建房设定 / client 从大厅元数据读取，两端一致）。
    #[cfg(feature = "steam")]
    match_starting_gold: i32,
    /// 当前场次每轮固定金币（参与奖；host 建房设定 / client 从大厅元数据读取，两端一致）。
    #[cfg(feature = "steam")]
    match_gold_per_round: i32,
    /// 当前场次单轮名次奖励档位（host 建房设定 / client 从大厅元数据读取，两端一致）。
    #[cfg(feature = "steam")]
    match_place_rewards: Vec<i32>,
    /// 当前场次的总轮数（host 建房设定 / client 从大厅元数据读取，两端一致）。
    #[cfg(feature = "steam")]
    match_rounds: u32,
    /// Steam：房间界面是否展开「邀请好友」面板（I 开关；不是模态，房间网络逻辑照常每帧跑）。
    #[cfg(feature = "steam")]
    steam_friend_list: bool,
    /// Steam：好友列表（展开面板时刷新一次；R 手动刷新）。
    #[cfg(feature = "steam")]
    steam_friends: Vec<net_steam::session::FriendInfo>,
    /// Steam：好友列表当前选中项。
    #[cfg(feature = "steam")]
    steam_friend_selection: usize,
    /// Steam：上次邀请动作的反馈文案（界面提示，如「已邀请 xxx」）。
    #[cfg(feature = "steam")]
    steam_friend_hint: String,
    /// Steam：Rich Presence 上次写入的时刻（秒，用于节流）。
    #[cfg(feature = "steam")]
    steam_presence_last: f64,
    /// Steam：Rich Presence 上次写入的内容（`状态|connect`），内容不变则不重复写。
    #[cfg(feature = "steam")]
    steam_presence_text: String,
    /// Steam：主菜单上是否已尝试过初始化会话（失败也不再每帧重试，避免刷屏 + 反复 SteamAPI_Init）。
    #[cfg(feature = "steam")]
    steam_session_tried: bool,
    /// Steam：各成员的 ping（毫秒），键 = SteamID；每 ~0.5s 刷新一次（帧同步对延迟敏感，值得显性展示）。
    #[cfg(feature = "steam")]
    steam_pings: Vec<(u64, i32)>,
    /// Steam：头像缓存（SteamID → 32x32 贴图）。拉过一次就缓存，Steam 首次常拉不到，下次自动重试。
    #[cfg(feature = "steam")]
    steam_avatars: Vec<(u64, graphics::Image)>,
    /// Steam：ping/头像刷新的节流计数（每 30 帧一次）。
    #[cfg(feature = "steam")]
    steam_net_ticks: u32,
    /// Steam：排行榜句柄槽（查找是异步回调，结果由 `run_callbacks` 推进后写回）。
    #[cfg(feature = "steam")]
    steam_lb_slot: net_steam::session::Shared<Option<net_steam::steamworks::Leaderboard>>,
    /// Steam：榜单前几名（下载同样是异步回调写回）。
    #[cfg(feature = "steam")]
    steam_lb_rows: net_steam::session::Shared<Vec<net_steam::session::LeaderboardRow>>,
    /// Steam：是否已对本场发起过排行榜查找（每会话只找一次）。
    #[cfg(feature = "steam")]
    steam_lb_requested: bool,
    /// Steam：本场战绩是否已上报（进入 Finished 只上报一次，避免每帧重复写）。
    #[cfg(feature = "steam")]
    steam_stats_recorded: bool,
    /// Steam：结算界面展示的统计快照（场次/胜场/击杀）。
    #[cfg(feature = "steam")]
    steam_stats_snapshot: Option<net_steam::session::StatsSnapshot>,
    /// Steam：成就/榜单提示条（文案 + 到期时刻，秒）。
    #[cfg(feature = "steam")]
    steam_toast: (String, f64),
}

/// 从磁盘加载完整 CJK 字体并注册为 "cjk"，避免把 17.7MB 的 cjk.ttf 内联进二进制。
///
/// 发布版 cjk.ttf 随 exe 一起分发（见 publish.ps1）；开发期在仓库 `assets/fonts/` 下。
/// 候选路径依次尝试：exe 同目录、exe 同目录的 `assets/fonts/`、以及相对 cwd 的仓库布局。
/// 全部失败（如字体文件被误删）时回退到内联的 168KB 子集（稀有字可能成豆腐块，但可执行文件仅 +168KB）。
fn load_cjk_font(ctx: &mut Context) -> GameResult<()> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("cjk.ttf"));
            candidates.push(dir.join("assets").join("fonts").join("cjk.ttf"));
        }
    }
    // 开发期 cwd 通常为 client/，仓库字体在 ../../assets/fonts/cjk.ttf
    candidates.push(std::path::PathBuf::from("assets/fonts/cjk.ttf"));
    candidates.push(std::path::PathBuf::from("../assets/fonts/cjk.ttf"));
    candidates.push(std::path::PathBuf::from("../../assets/fonts/cjk.ttf"));

    for path in &candidates {
        if let Ok(bytes) = std::fs::read(path) {
            match ggez::graphics::FontData::from_vec(bytes) {
                Ok(font) => {
                    ctx.gfx.add_font("cjk", font);
                    eprintln!("[font] 已加载 CJK 字体：{}", path.display());
                    return Ok(());
                }
                Err(e) => eprintln!("[font] 解析失败 {}: {:?}", path.display(), e),
            }
        }
    }
    eprintln!("[font] 未在磁盘找到 cjk.ttf，回退到内联 168k 子集字体");
    let font = ggez::graphics::FontData::from_vec(include_bytes!("../../assets/fonts/cjk-168k.ttf").to_vec())?;
    ctx.gfx.add_font("cjk", font);
    Ok(())
}

/// C8 去重判定（纯函数，便于单测）：本帧是否应抑制 ASCII 白名单插入。
///
/// `update` 开头 `frame = frame.wrapping_add(1)`；`on_text_input` 把 `last_ime_commit_frame`
/// 设为当时的 `frame+1`。若同一物理键在同一帧既触发 IME 提交（`last = frame_before+1`）又在该帧
/// `update` 时 `just(c)` 命中（此时 `frame` 已变为 `frame_before+1`），则二者相等 → 抑制，避免重复插入。
#[cfg(feature = "steam")]
#[inline]
fn ime_commit_suppresses_ascii(frame: u64, last_ime_commit_frame: u64) -> bool {
    last_ime_commit_frame == frame
}

impl Game {
    fn new(ctx: &mut Context, app: AppState) -> GameResult<Self> {
        // 注册中文字体：从磁盘加载完整 cjk.ttf（不内联进 17.7MB 二进制）；
        // 发布版 cjk.ttf 随 exe 一起分发，开发期在仓库 assets/ 下。找不到时回退内联 168k 子集。
        load_cjk_font(ctx)?;

        // 联网：加入 host 或开房作 host；否则单机（含本地 AI 机器人）。
        let mut net_link: Option<netlink::NetLinkUdp> = None;
        let mut net_host: Option<net::handshake::HostHandshake<net::transport::StdUdpTransport>> = None;
        let net_host_ls: Option<net::lockstep::HostLockstep<net::transport::StdUdpTransport>> = None;
        #[cfg(feature = "steam")]
        let steam_host_ls: Option<net::lockstep::HostLockstep<net_steam::SteamTransport>> = None;
        #[cfg(feature = "steam")]
        let steam_cli_ls: Option<net::lockstep::ClientLockstep<net_steam::SteamTransport>> = None;
        #[cfg(feature = "steam")]
        let steam_my_index: u8 = 0;
        #[cfg(feature = "steam")]
        let mut steam_sess: Option<net_steam::session::SteamSession> = None;
        #[cfg(feature = "steam")]
        let mut steam_lobby_pending: Option<SteamLobbyPending> = None;
        #[cfg(feature = "steam")]
        let steam_active: bool = false;
        #[cfg(feature = "steam")]
        let steam_my_id: u64 = 0;
        #[cfg(feature = "steam")]
        let steam_cli_stale_ticks: u64 = 0;
        #[cfg(feature = "steam")]
        let steam_participants: Vec<u64> = Vec::new();
        #[cfg(feature = "steam")]
        let steam_online: Vec<u64> = Vec::new();
        #[cfg(feature = "steam")]
        let steam_migrating: bool = false;
        #[cfg(feature = "steam")]
        let steam_migrate_ticks: u64 = 0;
        #[cfg(feature = "steam")]
        let steam_new_host_id: u64 = 0;
        #[cfg(feature = "steam")]
        let steam_host_broadcasting_takeover: bool = false;
        #[cfg(feature = "steam")]
        let steam_roster: Vec<(u8, String, u64)> = Vec::new();
        // CLI `--steam-host`/`--steam-join` 直通：大厅操作是帧驱动异步（S12），构造时尚未进房，
        // 由 `update` 每帧 `tick_lobby` 完成后才置 `steam_in_lobby=true`，故这里固定 false（true 会让 draw/update 误以为已进房）。
        // 主菜单/单机试验场：仅 1 个玩家且无 AI；Solo 也是 1 玩家无 AI。
        #[cfg(feature = "steam")]
        let init_rounds: u32 = STEAM_DEFAULT_ROUNDS;
        #[cfg(feature = "steam")]
        let init_learn_secs: u32 = STEAM_DEFAULT_LEARN_SECS;
        #[cfg(feature = "steam")]
        let init_starting_gold: i32 = STEAM_DEFAULT_STARTING_GOLD;
        #[cfg(feature = "steam")]
        let init_gold_per_round: i32 = STEAM_DEFAULT_GOLD_PER_ROUND;
        #[cfg(feature = "steam")]
        let init_place_rewards: Vec<i32> = auto_place_rewards(STEAM_DEFAULT_PLACE_FIRST);
        let mut player_count: u32 = 1;
        match app {
            AppState::MainMenu => {}
            AppState::Solo => {}
            #[cfg(feature = "steam")]
            AppState::SteamHost { players } => {
                // 帧驱动异步（S12）：只发起建厅，真正进房由 `update` 每帧 `tick_lobby` 完成后落地。
                let mut sess = net_steam::session::SteamSession::init(APP_ID, STEAM_VIRTUAL_PORT)
                    .map_err(ggez::GameError::from)?;
                sess.start_host_create(players.max(1) as u32, STEAM_LOBBY_CREATE_BEATS)
                    .map_err(ggez::GameError::from)?;
                steam_sess = Some(sess);
                steam_lobby_pending = Some(SteamLobbyPending::Host { players });
                player_count = players.max(1) as u32;
            }
            #[cfg(feature = "steam")]
            AppState::SteamJoin { lobby_id } => {
                // 同上：只发起加入，进房由 `update` 落地。
                let mut sess = net_steam::session::SteamSession::init(APP_ID, STEAM_VIRTUAL_PORT)
                    .map_err(ggez::GameError::from)?;
                match lobby_id {
                    Some(id) => sess.start_join_by_id(id, 240),
                    None => sess.start_find_and_join(240),
                }
                .map_err(ggez::GameError::from)?;
                steam_sess = Some(sess);
                steam_lobby_pending = Some(SteamLobbyPending::Join { lobby_id });
                player_count = 2;
            }
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
        // 整场对抗：3 小局，所有玩家都纳入档案。Steam 冷启动直通进房时用大厅元数据对齐的金币配置。
        let mut meta = {
            #[cfg(feature = "steam")]
            let cfg = MatchConfig {
                total_rounds: init_rounds,
                learn_time_secs: init_learn_secs as f64,
                gold_per_round: init_gold_per_round,
                starting_gold: init_starting_gold,
                place_rewards: init_place_rewards.clone(),
                ..Default::default()
            };
            #[cfg(not(feature = "steam"))]
            let cfg = MatchConfig::default();
            MatchState::new(cfg, &meta_ids, 8)
        };
        // 观察/调试 `FASTROUND=1`：缩小场地加速局终、缩短学习时间、多开几局，便于用 netlogs 看多局循环。
        if std::env::var("FASTROUND").is_ok() {
            world.arena_radius = game_core::fix::Fix64::from_num(3.0);
            meta.config.learn_time_secs = 3.0; // 给局间配置留 3s，方便手测时从容绑定/升级
            meta.config.total_rounds = 4;
        }
        // 开局不带任何默认技能：完全由玩家在配置/学习界面按字母选树 + 数字绑技能（4.6b/从零选择）。

        // 当前所有模式（Solo/Lan）都不带本地 AI 机器人：Solo 无对手，Lan 是真人玩家。
        let bot_rngs: Vec<Rng> = Vec::new();
        let bot_targets: Vec<Option<Vec2>> = Vec::new();

        let (w, h) = ctx.gfx.drawable_size();
        Ok(Game {
            world,
            meta,
            player_target: None,
            pending_cast: None,
            self_was_busy: false,
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
            frame: 0,
            // 初始为 MAX，确保首帧（frame 0，wrapping_sub 也为 0）不会误判为「本帧已 IME 提交」。
            last_ime_commit_frame: u64::MAX,
            scale: 1.0,
            offset: Point2 { x: w / 2.0, y: h / 2.0 },
            net_link,
            lan_my_index: PLAYER_ID as u8,
            net_host,
            net_host_ls,
            host_cfg_settle: 0,
            host_cfg_drained: false,
            #[cfg(feature = "steam")]
            steam_host_ls,
            #[cfg(feature = "steam")]
            steam_cli_ls,
            #[cfg(feature = "steam")]
            steam_my_index,
            #[cfg(feature = "steam")]
            steam_active,
            #[cfg(feature = "steam")]
            steam_my_id,
            #[cfg(feature = "steam")]
            steam_cli_stale_ticks,
            #[cfg(feature = "steam")]
            steam_participants,
            #[cfg(feature = "steam")]
            steam_online,
            #[cfg(feature = "steam")]
            steam_migrating,
            #[cfg(feature = "steam")]
            steam_migrate_ticks,
            #[cfg(feature = "steam")]
            steam_new_host_id,
            #[cfg(feature = "steam")]
            steam_host_broadcasting_takeover,
            #[cfg(feature = "steam")]
            steam_countdown: 0.0,
            #[cfg(feature = "steam")]
            steam_lobby_wait_ticks: 0,
            #[cfg(feature = "steam")]
            steam_cli_last_seq: 0,
            #[cfg(feature = "steam")]
            steam_last_sent_ready: None,
            #[cfg(feature = "steam")]
            steam_in_lobby: false,
            #[cfg(feature = "steam")]
            steam_local_ready: false,
            #[cfg(feature = "steam")]
            steam_roster,
            #[cfg(feature = "steam")]
            steam_roster_ready: Vec::new(),
            #[cfg(feature = "steam")]
            steam_roster_all_ready: false,
            #[cfg(feature = "steam")]
            steam_all_ready: false,
            #[cfg(feature = "steam")]
            steam_build_done: false,
            #[cfg(feature = "steam")]
            steam_was_all_ready: false,
            #[cfg(feature = "steam")]
            steam_lobby_menu: false,
            #[cfg(feature = "steam")]
            steam_lobby_selection: 0,
            #[cfg(feature = "steam")]
            steam_lobby_create: false,
            #[cfg(feature = "steam")]
            steam_lobby_list: false,
            #[cfg(feature = "steam")]
            steam_create_name: "我的房间".to_string(),
            #[cfg(feature = "steam")]
            steam_create_note: String::new(),
            #[cfg(feature = "steam")]
            steam_create_focus: 0,
            #[cfg(feature = "steam")]
            steam_list_lobbies: Vec::new(),
            #[cfg(feature = "steam")]
            steam_list_selection: 0,
            #[cfg(feature = "steam")]
            steam_list_requested: false,
            #[cfg(feature = "steam")]
            steam_list_last_refresh: -999.0,
            #[cfg(feature = "steam")]
            steam_sess,
            #[cfg(feature = "steam")]
            steam_my_display_name: String::new(),
            #[cfg(feature = "steam")]
            steam_join_lobby_id: None,
            #[cfg(feature = "steam")]
            steam_lobby_pending,
            #[cfg(feature = "steam")]
            steam_room_edit: false,
            #[cfg(feature = "steam")]
            steam_room_edit_focus: 0,
            #[cfg(feature = "steam")]
            steam_edit_name: String::new(),
            #[cfg(feature = "steam")]
            steam_edit_note: String::new(),
            #[cfg(feature = "steam")]
            steam_lobby_id: None,
            #[cfg(feature = "steam")]
            steam_room_locked: false,
            #[cfg(feature = "steam")]
            steam_roster_refresh_ticks: 0,
            #[cfg(feature = "steam")]
            steam_lobby_silent_ticks: 0,
            #[cfg(feature = "steam")]
            steam_last_roster_len: 0,
            #[cfg(feature = "steam")]
            steam_manual_start_pending: false,
            #[cfg(feature = "steam")]
            steam_manual_countdown: false,
            #[cfg(feature = "steam")]
            steam_manual_ms: 0,
            #[cfg(feature = "steam")]
            steam_friend_list: false,
            #[cfg(feature = "steam")]
            steam_friends: Vec::new(),
            #[cfg(feature = "steam")]
            steam_friend_selection: 0,
            #[cfg(feature = "steam")]
            steam_friend_hint: String::new(),
            #[cfg(feature = "steam")]
            steam_presence_last: -999.0,
            #[cfg(feature = "steam")]
            steam_presence_text: String::new(),
            #[cfg(feature = "steam")]
            steam_session_tried: false,
            #[cfg(feature = "steam")]
            steam_pings: Vec::new(),
            #[cfg(feature = "steam")]
            steam_avatars: Vec::new(),
            #[cfg(feature = "steam")]
            steam_net_ticks: 0,
            #[cfg(feature = "steam")]
            steam_lb_slot: net_steam::session::shared(None),
            #[cfg(feature = "steam")]
            steam_lb_rows: net_steam::session::shared(Vec::new()),
            #[cfg(feature = "steam")]
            steam_lb_requested: false,
            #[cfg(feature = "steam")]
            steam_stats_recorded: false,
            #[cfg(feature = "steam")]
            steam_stats_snapshot: None,
            #[cfg(feature = "steam")]
            steam_toast: (String::new(), 0.0),
            net_ready: false,
            net_cfg: NetCfgSync::Idle,
            app,
            pre_game_config: app != AppState::MainMenu,
            conn_dropped: false,
            reconnect_attempting: false,
            #[cfg(feature = "steam")]
            reconnect_stall_ticks: 0,
            host_frame_count: 0,
            pre_game_timer: PRE_GAME_TIMEOUT_SECS,
            menu_selection: 0,
            #[cfg(feature = "steam")]
            steam_create_players: STEAM_DEFAULT_PLAYERS,
            #[cfg(feature = "steam")]
            steam_create_rounds: STEAM_DEFAULT_ROUNDS,
            #[cfg(feature = "steam")]
            steam_create_players_buf: STEAM_DEFAULT_PLAYERS.to_string(),
            #[cfg(feature = "steam")]
            steam_create_rounds_buf: STEAM_DEFAULT_ROUNDS.to_string(),
            #[cfg(feature = "steam")]
            steam_create_learn: STEAM_DEFAULT_LEARN_SECS,
            #[cfg(feature = "steam")]
            steam_create_learn_buf: STEAM_DEFAULT_LEARN_SECS.to_string(),
            #[cfg(feature = "steam")]
            steam_create_starting_gold: STEAM_DEFAULT_STARTING_GOLD,
            #[cfg(feature = "steam")]
            steam_create_starting_gold_buf: STEAM_DEFAULT_STARTING_GOLD.to_string(),
            #[cfg(feature = "steam")]
            steam_create_gold_per_round: STEAM_DEFAULT_GOLD_PER_ROUND,
            #[cfg(feature = "steam")]
            steam_create_gold_per_round_buf: STEAM_DEFAULT_GOLD_PER_ROUND.to_string(),
            #[cfg(feature = "steam")]
            steam_create_place_buf: STEAM_DEFAULT_PLACE_REWARD.to_string(),
            #[cfg(feature = "steam")]
            steam_create_place: auto_place_rewards(STEAM_DEFAULT_PLACE_FIRST),
            #[cfg(feature = "steam")]
            match_rounds: init_rounds,
            #[cfg(feature = "steam")]
            match_learn_secs: init_learn_secs,
            #[cfg(feature = "steam")]
            match_starting_gold: init_starting_gold,
            #[cfg(feature = "steam")]
            match_gold_per_round: init_gold_per_round,
            #[cfg(feature = "steam")]
            match_place_rewards: init_place_rewards.clone(),
        })
    }

    fn update_camera(&mut self, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        // 令初始场地约占较短边的 45%（场地半径取 game-core 的 START_RADIUS，随 war3 尺度走）
        self.scale = sw.min(sh) * 0.45 / game_core::world::START_RADIUS as f32;
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
    // 判断某字符键是否刚被按下（大小写不敏感：Caps Lock / Shift 下字母也能匹配，如选树按 c 时 Caps Lock 收 `C` 也能命中）。
    fn char_just(ctx: &Context, s: &str) -> bool {
        use ggez::input::keyboard::Key;
        ctx.keyboard.is_logical_key_just_pressed(&Key::Character(s.to_lowercase().into()))
            || ctx.keyboard.is_logical_key_just_pressed(&Key::Character(s.to_uppercase().into()))
    }

    fn poll_learning(&mut self, ctx: &Context) {
        use ggez::input::keyboard::Key;
        let me = self.self_index();

        // 字母键：选中技能树
        for (letter, key) in KEY_LETTERS {
            if Self::char_just(ctx, letter) {
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
                    if let Some(profile) = self
                        .meta
                        .profiles
                        .iter_mut()
                        .find(|pr| pr.player_id == me)
                    {
                        profile.bind_skill(key, *skill);
                        eprintln!("[learn] bind tree={} digit='{}' -> {} onto pid={} binds={:?}", key.letter(), digit, game_core::skill::DefTable::def(*skill).name, profile.player_id, profile.key_slots.iter().map(|s| s.map(|x| x.as_u32())).collect::<Vec<_>>());
                    } else {
                        eprintln!("[learn] WARN bind failed: no profile pid={me} (pids={:?}) [self_index 找不到自己的 profile → 绑定丢失]", self.meta.profiles.iter().map(|p| p.player_id).collect::<Vec<_>>());
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
                    .find(|pr| pr.player_id == me)
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
        if Self::char_just(ctx, "x") {
            if let Some(profile) = self
                .meta
                .profiles
                .iter_mut()
                .find(|pr| pr.player_id == me)
            {
                profile.respec(1.0);
            }
            self.learn_tree_key = None;
        }
    }

    /// 4.6b 成长点/属性购买输入：
    /// - `Z`：用金币换 1 成长点。
    /// - `H`=Hp、`J`=Speed、`K`=Armor、`L`=法抗、`;`=击退。（U/I 蓝量键已随无蓝量系统移除）
    fn poll_growth_buy(&mut self, ctx: &Context) {
        use ggez::input::keyboard::Key;
        let me = self.self_index();
        let just = |k: &str| {
            ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.into()))
                || ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.to_uppercase().into()))
        };
        let Some(profile) = self.meta.profiles.iter_mut().find(|pr| pr.player_id == me) else {
            return;
        };
        if just("z") && profile.buy_growth_with_gold(GOLD_PER_GROWTH) {
            eprintln!("[attr] 金币→成长点：金币 {}", profile.gold);
        }
        let mut buy = |g: game_core::attribute::GrowthAttr| {
            let cur = profile.attributes.current(g);
            let cost = growth_attr_cost(cur);
            if profile.buy_attribute(g, cost) {
                eprintln!("[attr] 买 {g:?} +1（点 {}", profile.attributes.current(g));
            }
        };
        if just("h") { buy(game_core::attribute::GrowthAttr::Hp); }
        if just("j") { buy(game_core::attribute::GrowthAttr::Speed); }
        if just("k") { buy(game_core::attribute::GrowthAttr::Armor); }
        if just("l") { buy(game_core::attribute::GrowthAttr::SpellResist); }
        if just(";") { buy(game_core::attribute::GrowthAttr::KbResist); }
        // U/I（蓝上限/回蓝）已随无蓝量系统移除（PORT_098B_DECISIONS.md D3）。
    }

    /// 本局进行中：结算击杀、名次，进入学习阶段。
    fn settle_round(&mut self) {
        for (killer, _victim) in self.world.take_kills() {
            self.meta.register_kill(killer);
        }
        let placement = self.world.placement();
        self.meta.finish_round(placement);
        // 4.6b：每局给所有玩家发成长点（用于买属性）。
        for profile in self.meta.profiles.iter_mut() {
            profile.add_growth_points(GROWTH_PER_ROUND);
        }
    }

    /// 进入下一局前：把玩家的技能等级从档案同步到世界，并重置世界。
    fn teardown_round_end(&mut self) {
        // 诊断：同步前本端 profiles 技能等级（局间技能同步关键）。
        eprintln!("[teardown] profiles pre: {:?}", self.meta.profiles.iter().map(|pr| (pr.player_id, pr.skill_levels.clone())).collect::<Vec<_>>());
        // 把 meta.profiles 全量同步到 world.players，使所有端下一局的技能等级一致。
        // （联网下 profiles 已经由 host 广播的完整配置统一；单机下按本地各玩家档案设置。）
        for (profile, p) in self.meta.profiles.iter().zip(self.world.players.iter_mut()) {
            for i in 0..p.skill_levels.len().min(profile.skill_levels.len()) {
                p.skill_levels[i] = profile.skill_levels[i];
            }
            // 4.6b：把玩家属性（Hp/移速等）派生到战斗数值（确定性纯函数，跨端/跨局一致）。
            p.apply_attributes(&profile.attributes);
        }
        // 诊断：同步后 world 各玩家技能等级。
        eprintln!("[teardown] world post: {:?}", self.world.players.iter().enumerate().map(|(i, p)| (i as u32, p.skill_levels.to_vec())).collect::<Vec<_>>());
        self.world.reset_round();
        self.player_target = None;
        self.pending_cast = None;
        self.self_was_busy = false;
        self.pending_skill = None;
        self.accumulator = 0.0;
    }

    /// 把 host 广播的完整玩家配置（`PlayerCfgAll` entries）应用回本地 `meta.profiles`。
    fn apply_player_cfgs(&mut self, entries: &[(u8, Vec<u8>)]) {
        let pids: Vec<u32> = self.meta.profiles.iter().map(|pr| pr.player_id).collect();
        for (player_index, bytes) in entries {
            match game_core::progress::PlayerConfig::decode(bytes) {
                Some(cfg) => {
                    if let Some(profile) = self.meta.profiles.iter_mut().find(|pr| pr.player_id == *player_index as u32) {
                        let before: Vec<u32> = profile.skill_levels.clone();
                        let before_binds: Vec<Option<u32>> = profile.key_slots.iter().map(|s| s.map(|x| x.as_u32())).collect();
                        cfg.apply_to(profile);
                        eprintln!("[cfg-sync] applied idx={} -> pid={} skill {:?}->{:?} binds {:?}->{:?}", player_index, profile.player_id, before, profile.skill_levels, before_binds, profile.key_slots.iter().map(|s| s.map(|x| x.as_u32())).collect::<Vec<_>>());
                    } else {
                        eprintln!("[cfg-sync] WARN no profile matches idx={player_index} (pids={pids:?}) [game 技能同步若缺这段说明 profile 匹配失败]");
                    }
                }
                None => eprintln!("[cfg-sync] WARN decode fail idx={} len={}", player_index, bytes.len()),
            }
        }
    }

    /// 当前场次的完整 `MatchConfig`（host 建房设定 / client 从大厅元数据读取后两端一致）。
    #[cfg(feature = "steam")]
    fn match_config(&self) -> game_core::meta::MatchConfig {
        game_core::meta::MatchConfig {
            total_rounds: self.match_rounds,
            learn_time_secs: self.match_learn_secs as f64,
            gold_per_round: self.match_gold_per_round,
            starting_gold: self.match_starting_gold,
            place_rewards: self.match_place_rewards.clone(),
            ..Default::default()
        }
    }

    /// 对局开始时把 world 与 meta 重建为“本局参与玩家数” `p`（不满员时两端角色数量由此一致）：
    /// 参与者被收缩为连续 player 0..p-1（`self.steam_my_index` 由调用方同步更新）。首局开局用 seed 从零建。
    #[cfg(feature = "steam")]
    fn stage_world_for_participants(&mut self, p: usize, seed: u64) {
        self.world = game_core::world::World::new(p.max(1) as u32, seed);
        self.meta = game_core::meta::MatchState::new(
            self.match_config(),
            &(0..p.max(1)).map(|i| i as u32).collect::<Vec<u32>>(),
            8,
        );
        eprintln!("[steam] staged world for {p} participant player(s)");
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

    /// 本局运行模式已并入 `AppState`（主菜单 / Solo / 局域网主机 / 局域网加入）。
    /// Steam 模式是否激活（host 或 client 任一）。默认构建恒 false。
    fn steam_active(&self) -> bool {
        #[cfg(feature = "steam")]
        {
            self.steam_active
        }
        #[cfg(not(feature = "steam"))]
        {
            false
        }
    }

    /// 本机玩家在该次对局中的序号：单机/host 恒为 0，加入者为握手分配到的 `my_index`；Steam 用大厅槽位。
    fn self_index(&self) -> u32 {
        #[cfg(feature = "steam")]
        if self.steam_active {
            return self.steam_my_index as u32;
        }
        match &self.net_link {
            Some(l) => l.my_index() as u32,
            None => self.lan_my_index as u32,
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
        // 施法与移动都是**持续电平量**：在「施法被世界接受」（自己角色进入 is_busy）前持续重发，
        // 由 `note_self_cast` 在接受的那一帧清除。这样：
        //   - 帧同步下不会因 host 输入缓存覆盖而丢失施法指令（否则要点多次/迟滞，同移动 take 的回归）；
        //   - 施法一旦被接受立即清，冷却归零不会自动重放（问题 2）。
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

    /// 每次世界推进后调用一次：若本机角色**刚进入**施法（前摇/后摇开始），清除待发送的移动目标。
    ///
    /// 这是"精确版"实现：`player_target` 保持电平量（每帧重发，不会在帧同步输入缓存下丢失），
    /// 只在施法真正开始的这一帧（`is_busy` 的 false→true 边沿）清一次 →
    /// 施法结束（前摇结束进后摇后）不再自动走向旧目标（问题 1）；且施法失败
    /// （冷却中，`is_busy` 仍为 false）时角色不停下。用 `is_busy` 而非 `is_windup`，
    /// 能覆盖零前摇技能（Windup 一帧即转 Recovery，`is_windup` 永不成立）。
    fn note_self_cast(&mut self) {
        let me = self.self_index();
        let busy = self
            .world
            .players
            .get(me as usize)
            .map(|p| p.caster.is_busy())
            .unwrap_or(false);
        if busy && !self.self_was_busy {
            self.player_target = None;
            self.pending_cast = None;
        }
        self.self_was_busy = busy;
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
                        self.self_was_busy = false;
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

            // 施法前摇提示：只对“自己”显示（其他客户端不应看到蓄力动画，只看到释放瞬间的效果）。
            if p.id == me_idx {
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

        // 闪电（D1）射线可视化：从起点到命中点/终点画一条亮蓝线（Unity 原版 LineRenderer·Drawline）。
        if let Some((la, lb, _)) = self.world.lightning_visual {
            let lax = la.x.to_num::<f32>() * self.scale + self.offset.x;
            let lay = la.y.to_num::<f32>() * self.scale + self.offset.y;
            let lbx = lb.x.to_num::<f32>() * self.scale + self.offset.x;
            let lby = lb.y.to_num::<f32>() * self.scale + self.offset.y;
            let bolt = Mesh::new_line(
                &ctx.gfx,
                &[Point2 { x: lax, y: lay }, Point2 { x: lbx, y: lby }],
                3.0,
                Color::from_rgba(150, 210, 255, 230),
            )?;
            canvas.draw(&bolt, graphics::DrawParam::new());
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

        #[cfg(feature = "steam")]
        if self.steam_in_lobby {
            if self.steam_room_edit {
                self.draw_steam_room_edit(&mut canvas, ctx)?;
            } else {
                self.draw_steam_ready_overlay(&mut canvas, ctx)?;
            }
        }

        // 客户端掉线/重连覆盖层
        if self.conn_dropped {
            self.draw_reconnect_overlay(&mut canvas, ctx)?;
        }

        // Steam 提示条（成就上报等）：画在最上层，几秒后自动消失。
        #[cfg(feature = "steam")]
        {
            let now = ctx.time.time_since_start().as_secs_f64();
            if !self.steam_toast.0.is_empty() && now < self.steam_toast.1 {
                let (sw, sh) = ctx.gfx.drawable_size();
                draw_text(&mut canvas, ctx, &self.steam_toast.0, 22.0, Color::from_rgb(255, 215, 120), Point2 { x: sw / 2.0, y: sh * 0.08 }, true)?;
            }
        }

        canvas.finish(ctx)?;
        Ok(())
    }

    /// Steam 房间/就绪界面：列出成员昵称 + 就绪状态，按 o 就绪/取消，全就绪倒计时。
    #[cfg(feature = "steam")]
    fn draw_steam_ready_overlay(&mut self, canvas: &mut Canvas, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        let dim = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), graphics::Rect::new(0.0, 0.0, sw, sh), Color::from_rgba(8, 10, 16, 225))?;
        canvas.draw(&dim, graphics::DrawParam::new());
        let cx = sw / 2.0;
        draw_text(canvas, ctx, "房间 - 等待所有人就绪", 40.0, Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.18 }, true)?;
        // 房间名 + 人数 + 锁状态（host 读 matchmaking，client 用本地记录）。
        let (rname, rnote) = self.steam_current_room_info();
        let n_in = self.steam_roster.len();
        let lock_txt = if self.steam_room_locked { "[锁]" } else { "[开]" };
        let mut roomline = format!("房间：{rname}    人数 {n_in}");
        if self.steam_host_ls.is_some() {
            roomline.push_str(&format!("   {lock_txt}"));
        }
        draw_text(canvas, ctx, &roomline, 20.0, Color::from_rgb(190, 200, 215), Point2 { x: cx, y: sh * 0.18 + 40.0 }, true)?;
        if !rnote.is_empty() {
            draw_text(canvas, ctx, &format!("备注：{rnote}"), 18.0, Color::from_rgb(170, 180, 195), Point2 { x: cx, y: sh * 0.18 + 66.0 }, true)?;
        }
        let flow_y = if rnote.is_empty() { sh * 0.18 + 66.0 } else { sh * 0.18 + 92.0 };
        draw_text(canvas, ctx, "流程：全员就绪 → 倒计时 → 技能配置 → 配好后自动开战", 18.0, Color::from_rgb(160, 172, 190), Point2 { x: cx, y: flow_y }, true)?;
        // 倒计时提示：优先区分「不满员由房主手动确认」与「满员全员就绪」，否则会显示成误导的"全员就绪"。
        if self.steam_manual_countdown {
            // host 本机：不满员手动倒计时（本地直接数秒）。
            let secs = self.steam_countdown.max(0.0);
            let hint = if secs <= STEAM_COUNTDOWN_LOCK_SECS { "即将开始（不可取消）" } else { "按 U 可取消" };
            draw_text(canvas, ctx, &format!("人数不足（已入 {n_in}）：房主已确认，{secs:.0} 秒后进配置（{hint}）"), 26.0, Color::from_rgb(90, 220, 130), Point2 { x: cx, y: flow_y + 48.0 }, true)?;
        } else if self.steam_manual_ms > 0 {
            // client 端：收到 host 广播的不满足手动倒计时剩余毫秒，跨端显示同一倒计时。
            let secs = (self.steam_manual_ms as f32) / 1000.0;
            let hint = if secs <= STEAM_COUNTDOWN_LOCK_SECS { "即将开始（不可取消）" } else { "按 U 可取消" };
            draw_text(canvas, ctx, &format!("人数不足（已入 {n_in}）：房主已确认，{secs:.0} 秒后进配置（{hint}）"), 26.0, Color::from_rgb(90, 220, 130), Point2 { x: cx, y: flow_y + 48.0 }, true)?;
        } else if self.steam_all_ready {
            draw_text(canvas, ctx, &format!("全员就绪：{:.0} 秒后进配置（结束前按 U 可取消）", self.steam_countdown.max(0.0)), 28.0, Color::from_rgb(90, 220, 130), Point2 { x: cx, y: flow_y + 48.0 }, true)?;
        } else if self.steam_host_ls.is_some() && self.steam_manual_start_pending {
            // 不满员但在线者都就绪：不自动倒计时，由 host 按回车确认后才开始倒计时。
            draw_text(canvas, ctx, &format!("人数不足（已入 {n_in}）：当前全员就绪，按回车 开始倒计时"), 26.0, Color::from_rgb(255, 220, 120), Point2 { x: cx, y: flow_y + 48.0 }, true)?;
        } else if self.steam_local_ready {
            // 本机已就绪但还没全员就绪：别再提示“按 U 就绪”（那会让人以为自己没准备好）。
            let hint = if self.steam_host_ls.is_some() {
                "已就绪：等其他人就绪（人数不足时由你按回车开始）"
            } else {
                "已就绪：等其他人就绪"
            };
            draw_text(canvas, ctx, hint, 26.0, Color::from_rgb(150, 200, 255), Point2 { x: cx, y: flow_y + 48.0 }, true)?;
        } else {
            draw_text(canvas, ctx, "▶ 按 U 就绪（再按 U 取消）", 28.0, Color::from_rgb(255, 240, 120), Point2 { x: cx, y: flow_y + 48.0 }, true)?;
        }
        // host 附加“编辑房间”入口，显示在底部。
        if self.steam_host_ls.is_some() {
            draw_text(canvas, ctx, "E 编辑房间名/备注与锁定    I 邀请好友    Q 退出房间", 19.0, Color::from_rgb(160, 200, 255), Point2 { x: cx, y: sh * 0.90 }, true)?;
        } else {
            draw_text(canvas, ctx, "U 就绪/取消    I 邀请好友    Q 退出房间", 19.0, Color::from_rgb(160, 200, 255), Point2 { x: cx, y: sh * 0.90 }, true)?;
        }
        draw_text(canvas, ctx, "== 就绪状态 ==", 20.0, Color::from_rgb(200, 210, 220), Point2 { x: cx, y: sh * 0.42 }, true)?;
        let mut y = sh * 0.42 + 32.0;
        let roster_lookup = |slot: u8, fallback: bool| -> bool {
            self.steam_roster_ready
                .iter()
                .find(|(s, _)| *s == slot)
                .map(|(_, r)| *r)
                .unwrap_or(fallback)
        };
        let card_w = (sw * 0.46).min(520.0);
        let card_x = cx - card_w / 2.0;
        for (slot, name, id) in self.steam_roster.iter() {
            let is_me = *slot == self.steam_my_index;
            let (ready, col) = if is_me {
                (self.steam_local_ready, if self.steam_local_ready { Color::from_rgb(90, 220, 130) } else { Color::from_rgb(220, 220, 225) })
            } else if let Some(host) = self.steam_host_ls.as_ref() {
                // host 本机：直接读 HostLockstep 里的各 client 就绪。
                let r = host.client_ready(*slot);
                (r, if r { Color::from_rgb(90, 220, 130) } else { Color::from_rgb(220, 220, 225) })
            } else {
                // client 本机：用 host 广播的就绪状态快照（多人一致界面）。
                let r = roster_lookup(*slot, false);
                (r, if r { Color::from_rgb(90, 220, 130) } else { Color::from_rgb(220, 220, 225) })
            };
            // 成员卡片背景（就绪偏绿、未就绪深灰），与主菜单/配置界面卡片视觉一致。
            let bg_col = if ready { Color::from_rgb(34, 58, 44) } else { Color::from_rgb(30, 34, 44) };
            let bg = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), graphics::Rect::new(card_x, y - 6.0, card_w, 44.0), bg_col)?;
            canvas.draw(&bg, graphics::DrawParam::new());
            let mark = if ready { "[v]" } else { "[ ]" };
            let me_tag = if is_me { "（我）" } else { "" };
            draw_text(canvas, ctx, &format!("  {mark}  {name}{me_tag}"), 26.0, col, Point2 { x: card_x + 90.0, y }, true)?;
            // 延迟：画在卡片右端（没测到显示“--”）。
            let ping_txt = match self.steam_ping_of(*id) {
                Some(ms) => format!("{ms} ms"),
                None => "-- ms".to_string(),
            };
            draw_text(canvas, ctx, &ping_txt, 18.0, Color::from_rgb(150, 175, 205), Point2 { x: card_x + card_w - 60.0, y: y + 2.0 }, true)?;
            // 头像：画在卡片左侧外面（卡片内的昵称是居中排的，塞进去会压字），没拉到就留空。
            self.steam_draw_avatar(canvas, *id, card_x - 36.0, y - 4.0, 30.0);
            y += 46.0;
        }
        // 「邀请好友」面板（按 I 展开）：画在成员列表下方（`y` 即成员列表末尾），卡片样式与成员列表一致。
        #[cfg(feature = "steam")]
        if self.steam_friend_list {
            self.draw_steam_friend_panel(canvas, ctx, y)?;
        }
        Ok(())
    }

    /// 绘制「邀请好友」面板：好友昵称 + 在线/离线 + 是否已在房间；选中项高亮，行数按剩余空间自适应（最多 4 行）。
    /// `roster_end_y` 是成员列表画完后的 y，面板从它下面开始，避免与成员列表叠在一起。
    #[cfg(feature = "steam")]
    fn draw_steam_friend_panel(&self, canvas: &mut Canvas, ctx: &Context, roster_end_y: f32) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;
        let title_y = (roster_end_y + 14.0).max(sh * 0.58);
        let hint_y = sh * 0.83; // 面板操作提示
        let card_w = (sw * 0.46).min(520.0);
        let card_x = cx - card_w / 2.0;
        draw_text(canvas, ctx, "== 邀请好友 ==", 20.0, Color::from_rgb(200, 210, 220), Point2 { x: cx, y: title_y }, true)?;
        if self.steam_friends.is_empty() {
            draw_text(canvas, ctx, "（暂无好友 / 正在拉取…）", 20.0, Color::from_rgb(170, 178, 194), Point2 { x: cx, y: title_y + 34.0 }, true)?;
        } else {
            const ROW_H: f32 = 38.0;
            // 面板能放几行取决于离底部提示还剩多少空间（人数多时自然少放几行）。
            let fit = ((hint_y - (title_y + 30.0)) / ROW_H).floor() as usize;
            let max_rows = fit.clamp(1, 4);
            let n = self.steam_friends.len();
            // 选中项滚出可视窗口时，窗口跟着走（保证选中行总是可见）。
            let start = if n <= max_rows {
                0
            } else {
                (self.steam_friend_selection + 1).saturating_sub(max_rows).min(n - max_rows)
            };
            let mut y = title_y + 34.0;
            for i in start..(start + max_rows).min(n) {
                let f = &self.steam_friends[i];
                let selected = i == self.steam_friend_selection;
                let bg_col = if selected { Color::from_rgb(52, 60, 74) } else { Color::from_rgb(30, 34, 44) };
                let bg = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), graphics::Rect::new(card_x, y - 6.0, card_w, 36.0), bg_col)?;
                canvas.draw(&bg, graphics::DrawParam::new());
                let mark = if selected { "[v]" } else { "[ ]" };
                let tag = if f.in_lobby {
                    "（已在房间）"
                } else if f.online {
                    "（在线）"
                } else {
                    "（离线）"
                };
                let col = if selected { Color::WHITE } else { Color::from_rgb(205, 210, 222) };
                let name = if f.name.is_empty() { f.id.to_string() } else { f.name.clone() };
                draw_text(canvas, ctx, &format!("  {mark}  {name}{tag}"), 20.0, col, Point2 { x: card_x + 60.0, y }, true)?;
                // 头像（与成员列表同一位置：卡片左侧外面）。
                self.steam_draw_avatar(canvas, f.id, card_x - 36.0, y - 4.0, 30.0);
                y += 38.0;
            }
        }
        draw_text(canvas, ctx, "↑/↓ 选择    回车 邀请    A Steam 邀请窗口    R 刷新    I/Q 收起", 17.0, Color::from_rgb(160, 200, 255), Point2 { x: cx, y: hint_y }, true)?;
        if !self.steam_friend_hint.is_empty() {
            draw_text(canvas, ctx, &self.steam_friend_hint, 18.0, Color::from_rgb(255, 220, 120), Point2 { x: cx, y: hint_y + 24.0 }, true)?;
        }
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
                // 延迟指示（Steam 联机才有 ping；没测出来显示“--”）。
                #[cfg(feature = "steam")]
                if self.steam_cli_ls.is_some() || self.steam_host_ls.is_some() {
                    let host_id = self.steam_participants.first().copied().unwrap_or(0);
                    let mine = if self.steam_cli_ls.is_some() {
                        self.steam_ping_of(host_id)
                    } else {
                        // host：显示到各 client 里最差的一个（帧同步等最慢的那端）。
                        let my_id = self.steam_my_id;
                        self.steam_pings
                            .iter()
                            .filter(|(id, _)| *id != my_id)
                            .map(|(_, ms)| *ms)
                            .max()
                    };
                    let txt = match mine {
                        Some(ms) => format!("延迟 {ms} ms"),
                        None => "延迟 -- ms".to_string(),
                    };
                    draw_text(canvas, ctx, &txt, 18.0, Color::from_rgb(150, 175, 205), Point2 { x: 76.0, y: sh - 116.0 }, true)?;
                }
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

                let title = if self.meta.is_first_config() {
                    "开局配置 - 配置技能".to_string()
                } else {
                    format!("第 {} / {} 局结束 - 学习阶段", self.meta.round, self.meta.config.total_rounds)
                };
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
                        "选键改技能：按 字母(C/R/E/D/Y/T/F/G) 选中该树 -> 数字键选技能 -> 按 = 升级，X 洗点",
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
                // Steam：统计（后台配置后才有值）+ 榜单前几名 + 成就提示。
                #[cfg(feature = "steam")]
                if let Some(s) = self.steam_stats_snapshot {
                    let f = |v: Option<i32>| v.map(|n| n.to_string()).unwrap_or_else(|| "--".to_string());
                    let line = format!(
                        "Steam 统计：场次 {}    胜场 {}    击杀 {}",
                        f(s.matches),
                        f(s.wins),
                        f(s.kills)
                    );
                    draw_text(canvas, ctx, &line, 20.0, Color::from_rgb(170, 190, 215), Point2 { x: cx, y }, true)?;
                    y += 34.0;
                    // 榜单前 5（异步下载；没下载到就显示一行提示）。
                    let rows = self.steam_lb_rows.lock().unwrap().clone();
                    if rows.is_empty() {
                        draw_text(canvas, ctx, "排行榜：暂无数据（需在 Steamworks 后台建榜）", 18.0, Color::from_rgb(140, 150, 168), Point2 { x: cx, y }, true)?;
                        y += 30.0;
                    } else {
                        draw_text(canvas, ctx, "排行榜 TOP5", 20.0, Color::from_rgb(255, 210, 120), Point2 { x: cx, y }, true)?;
                        y += 30.0;
                        for r in rows.iter().take(5) {
                            let name = self
                                .steam_transport()
                                .map(|t| t.friends().get_friend(net_steam::steamworks::SteamId::from_raw(r.steam_id)).name())
                                .unwrap_or_default();
                            let who = if name.is_empty() { format!("{}", r.steam_id) } else { name };
                            draw_text(canvas, ctx, &format!("#{}  {}  {} 分", r.rank, who, r.score), 18.0, Color::from_rgb(205, 212, 225), Point2 { x: cx, y }, true)?;
                            y += 26.0;
                        }
                    }
                }
                draw_text(canvas, ctx, "对局结束 - 按 Q 返回主菜单", 22.0, Color::from_rgb(150, 200, 255), Point2 { x: cx, y: y + 30.0 }, true)?;
            }
        }
        Ok(())
    }
}

impl event::EventHandler for Game {
    fn update(&mut self, ctx: &mut Context) -> GameResult {
        // 帧计数（用于 IME 去重，见 `last_ime_commit_frame`）。每帧递增，含提前返回的分支。
        self.frame = self.frame.wrapping_add(1);
        let dt = ctx.time.delta().as_secs_f64();

        // S12：进行中的大厅操作（建厅/加入）是帧驱动异步，由 `update` 每帧 `run_callbacks` 后 `tick_lobby` 推进。
        // 连接期间跳过其余菜单/房间输入（也不应被认为已进房），只泵回调 + 推进，完成后才落地进房。
        #[cfg(feature = "steam")]
        if self.steam_lobby_pending.is_some() {
            self.steam_poll_lobby_pending(ctx);
            self.accumulator = 0.0;
            return Ok(());
        }

        // Steam 房间/就绪/编辑阶段：房主按 E 进「编辑房间信息」界面；否则进房间就绪界面。
        #[cfg(feature = "steam")]
        if self.steam_in_lobby {
            if self.steam_room_edit {
                return self.steam_room_edit_update(ctx, dt);
            }
            return self.steam_lobby_update(ctx, dt);
        }

        // 主菜单：方向键 ↑/↓ 选中 + 回车确认 + 数字快捷；局域网（S3 完成前）保留命令行提示。
        if self.app == AppState::MainMenu {
            // 好友从 Steam 好友列表点「加入游戏」→ 回调进房（需先有会话；Steam 没跑则静默跳过）。
            #[cfg(feature = "steam")]
            {
                self.steam_ensure_session();
                self.steam_poll_join_requests(ctx);
                // 被邀请进房后本帧不再处理菜单输入。
                if self.steam_in_lobby || self.steam_cli_ls.is_some() || self.steam_host_ls.is_some() {
                    self.accumulator = 0.0;
                    return Ok(());
                }
            }
            use ggez::input::keyboard::Key;
            use winit::keyboard::NamedKey;
            use ggez::input::mouse::MouseButton;
            let just = |k: char| ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.to_string().into()));
            let just_named = |n: NamedKey| ctx.keyboard.is_logical_key_just_pressed(&Key::Named(n));
            const MENU_COUNT: usize = 3;
            // 大厅子界面（主/建房设置/房间列表）内不响应主菜单的方向键/数字。
            #[cfg(feature = "steam")]
            let in_lobby_menu = self.steam_lobby_menu || self.steam_lobby_create || self.steam_lobby_list;
            #[cfg(not(feature = "steam"))]
            let in_lobby_menu = false;
            // 建房设置界面输入（房间名/备注/人数，回车创建 / Q 取消）。
            #[cfg(feature = "steam")]
            if self.steam_lobby_create {
                self.steam_lobby_create_update(ctx);
                self.accumulator = 0.0;
                return Ok(());
            }
            // 房间列表界面输入（浏览公开大厅，方向键+回车加入 / R 刷新 / Q 返回）。
            #[cfg(feature = "steam")]
            if self.steam_lobby_list {
                self.steam_lobby_list_update(ctx);
                self.accumulator = 0.0;
                return Ok(());
            }
            // Steam 大厅主界面（创建 / 加入 / 返回）：独立处理并返回，不落到下面主菜单动作块。
            // 放在主菜单动作处理【之前】，避免“刚进入大厅那一帧”把触发键（回车/鼠标）又当成大厅里的选择再触发一次（会直接进创建房间）。
            #[cfg(feature = "steam")]
            if self.steam_lobby_menu {
                // 上下箭头切换选中。
                if just_named(NamedKey::ArrowUp) {
                    self.steam_lobby_selection = (self.steam_lobby_selection + 2) % 3;
                } else if just_named(NamedKey::ArrowDown) {
                    self.steam_lobby_selection = (self.steam_lobby_selection + 1) % 3;
                }
                // 鼠标点击卡片：命中即选中并执行。
                let mut clicked = false;
                if ctx.mouse.button_just_pressed(MouseButton::Left) {
                    let (sw, sh) = ctx.gfx.drawable_size();
                    let card_w = (sw * 0.62).min(560.0);
                    let card_h = 96.0;
                    let card_x = sw / 2.0 - card_w / 2.0;
                    let y0 = sh * 0.34;
                    let gap = 26.0;
                    let p = ctx.mouse.position();
                    for i in 0..3 {
                        let y = y0 + i as f32 * (card_h + gap);
                        if graphics::Rect::new(card_x, y, card_w, card_h).contains(p) {
                            self.steam_lobby_selection = i;
                            self.steam_lobby_act(i);
                            clicked = true;
                        }
                    }
                }
                if !clicked {
                    if just('h') || just('H') || just(' ') {
                        self.steam_lobby_act(0);
                    } else if just('j') || just('J') {
                        self.steam_lobby_act(1);
                    } else if just('q') || just('Q') {
                        self.steam_lobby_act(2);
                    } else if just_named(NamedKey::Enter) || just('\r') {
                        self.steam_lobby_act(self.steam_lobby_selection);
                    }
                }
                self.accumulator = 0.0;
                return Ok(());
            }
            // 方向键移动选中（在 Steam 大厅子菜单里不干扰，只影响主菜单选中）。
            if !in_lobby_menu {
                if just_named(NamedKey::ArrowUp) {
                    self.menu_selection = (self.menu_selection + MENU_COUNT - 1) % MENU_COUNT;
                    eprintln!("[menu] select={}", self.menu_selection);
                } else if just_named(NamedKey::ArrowDown) {
                    self.menu_selection = (self.menu_selection + 1) % MENU_COUNT;
                    eprintln!("[menu] select={}", self.menu_selection);
                }
            }
            // 执行选中项对应的动作（回车 / 数字直选共用）。
            let mut act: Option<usize> = None;
            // 鼠标点击主菜单卡片（与键盘共用 menu_selection + act）。
            if !in_lobby_menu && ctx.mouse.button_just_pressed(MouseButton::Left) {
                let (sw, sh) = ctx.gfx.drawable_size();
                let card_w = (sw * 0.62).min(560.0);
                let card_h = 96.0;
                let card_x = sw / 2.0 - card_w / 2.0;
                let y0 = sh * 0.34;
                let gap = 26.0;
                let p = ctx.mouse.position();
                for i in 0..3 {
                    let y = y0 + i as f32 * (card_h + gap);
                    if graphics::Rect::new(card_x, y, card_w, card_h).contains(p) {
                        self.menu_selection = i;
                        act = Some(i);
                    }
                }
            }
            if in_lobby_menu {
                act = None;
            } else if just_named(NamedKey::Enter) || just('\r') {
                act = Some(self.menu_selection);
            } else if just('1') {
                self.menu_selection = 0;
                act = Some(0);
            } else if just('2') {
                self.menu_selection = 1;
                act = Some(1);
            } else if just('3') {
                self.menu_selection = 2;
                act = Some(2);
            }
            if let Some(sel) = act {
                match sel {
                    0 => {
                        // 单机试验场：world/meta 在构造时已是 1 玩家无 AI，直接切换即可。
                        eprintln!("[menu] -> Solo");
                        self.app = AppState::Solo;
                        self.meta.begin_first_round_config(); // 进首局配置学习（单机手动开始）
                        self.pre_game_config = true;
                    }
                    1 => {
                        eprintln!("[menu] 局域网建设中：需命令行 --host <port> / --join <host:port>");
                    }
                    2 => {
                        // 进入 Steam 大厅选择子菜单（H 创建 / J 加入 / Q 返回）。
                        #[cfg(feature = "steam")]
                        {
                            eprintln!("[menu] -> Steam lobby menu");
                            self.steam_lobby_menu = true;
                            self.steam_lobby_create = false;
                            self.steam_lobby_list = false;
                            // 进入大厅前初始化一次 Steam 会话（读本机昵称；建房/加入复用，避免重复 init 单实例）。
                            if self.steam_sess.is_none() {
                                match net_steam::session::SteamSession::init(APP_ID, STEAM_VIRTUAL_PORT) {
                                    Ok(s) => {
                                        self.steam_my_display_name = s.transport.friends()
                                            .get_friend(net_steam::steamworks::SteamId::from_raw(s.transport.steam_id()))
                                            .name();
                                        self.steam_sess = Some(s);
                                        eprintln!("[steam] session ready, display name='{}'", self.steam_my_display_name);
                                    }
                                    Err(e) => {
                                        eprintln!("[steam] session init failed: {e:?}");
                                        self.steam_sess = None;
                                    }
                                }
                            }
                        }
                        #[cfg(not(feature = "steam"))]
                        eprintln!("[menu] Steam 未启用（需 --features client/steam 构建）");
                    }
                    _ => {}
                }
            }
            self.accumulator = 0.0;
            return Ok(());
        }

        match self.meta.phase {
            MatchPhase::Finished => {
                // 整场对抗结束：不再模拟
                self.accumulator = 0.0;
                // Steam：整场结束上报一次战绩（统计 + 成就 + 排行榜），内部有“只上报一次”保护。
                #[cfg(feature = "steam")]
                if self.steam_active() {
                    self.steam_record_match_result(ctx.time.time_since_start().as_secs_f64());
                }
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
                self.poll_growth_buy(ctx);
                // 局域网 host：首局配置阶段仍收 client 加入（避免先到的 client 握手超时）。
                if self.meta.is_first_config() && self.net_host_ls.is_none() {
                    self.poll_host_join_phase();
                }

                // 单机试验场首局：保留手动开始（空格/回车/P），不走倒计时。
                let solo_first = self.app == AppState::Solo
                    && self.meta.is_first_config()
                    && !self.steam_active()
                    && self.net_link.is_none()
                    && self.net_host.is_none()
                    && self.net_host_ls.is_none();
                if solo_first {
                    use ggez::input::keyboard::Key;
                    use winit::keyboard::NamedKey;
                    let done = ctx.keyboard.is_logical_key_just_pressed(&Key::Character(" ".into()))
                        || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("p".into()))
                        || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("P".into()))
                        || ctx.keyboard.is_logical_key_just_pressed(&Key::Named(NamedKey::Enter))
                        || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("\r".into()));
                    // 超时兜底：窗口失焦/按键收不到时仍能开（手动仍是首选，超时仅防卡死）。
                    self.pre_game_timer -= dt;
                    let auto_done = self.pre_game_timer <= 0.0;
                    if done || auto_done {
                        eprintln!("[solo] first config {}", if auto_done { "timeout -> auto-start" } else { "manual start" });
                        self.meta.finish_first_round_config();
                        self.teardown_round_end();
                        self.pre_game_config = false;
                    }
                    self.accumulator = 0.0;
                    return Ok(());
                }

                // 其余（联机首局/局间、单机局间）：倒计时；归零后单机直接下一局、联机进配置同步。
                let now = self.meta.tick_learning(dt.min(0.25));
                if self.meta.phase == MatchPhase::Fighting {
                    let client_side = self.net_link.is_some()
                        || {
                            #[cfg(feature = "steam")]
                            {
                                self.steam_cli_ls.is_some()
                            }
                            #[cfg(not(feature = "steam"))]
                            {
                                false
                            }
                        };
                    let host_side = self.net_host_ls.is_some()
                        || {
                            #[cfg(feature = "steam")]
                            {
                                self.steam_host_ls.is_some()
                            }
                            #[cfg(not(feature = "steam"))]
                            {
                                false
                            }
                        };
                    if client_side || host_side {
                        // 进入配置同步前，清掉上一局残留的移动目标/施法请求：否则配置同步期间会随
                        // 输入上行传给 host（host 产帧后用它设 move_target）→ 下一局角色走向上一局目标。
                        self.player_target = None;
                        self.pending_cast = None;
                        let sync = if client_side {
                            NetCfgSync::ClientWait
                        } else {
                            NetCfgSync::HostGather
                        };
                        eprintln!("[meta] round {} learning done -> {} (config sync)", self.meta.round, if client_side { "ClientWait" } else { "HostGather" });
                        self.net_cfg = sync;
                        // host 进入配置同步：重置“已清空在途旧包”标记，首帧会 drain + reset（避免收到旧包当本轮配置）。
                        if sync == NetCfgSync::HostGather {
                            self.host_cfg_drained = false;
                            self.host_cfg_settle = 0;
                        }
                    } else {
                        eprintln!("[meta] round {} learning done -> next round (single)", self.meta.round);
                        self.teardown_round_end();
                    }
                }
                let _ = now;
                Ok(())
            }
            MatchPhase::Fighting => {
                // 对局进行中允许随时按 Esc 返回主菜单（含联网）：client 离开=正常掉线由其余端迁移/接管；
                // host 离开=触发现有 drop→迁移路径。与 Q 在 Finished 一致，消除 C1/C2 联网无法退出的死状态。
                use ggez::input::keyboard::Key;
                use winit::keyboard::NamedKey;
                if ctx.keyboard.is_logical_key_just_pressed(&Key::Named(NamedKey::Escape)) {
                    eprintln!("[exit] Esc -> back to main menu");
                    self.reset_to_main_menu();
                    self.accumulator = 0.0;
                    return Ok(());
                }
                // 每帧轮询输入（技能键 / 鼠标）
                self.poll_input(ctx);
                self.accumulator += dt.min(0.25);
                let ticking = Fix64::from_num(TICK);
                #[cfg(feature = "steam")]
                {
                    // Rich Presence：对局中 → “对局中（第 N 局）”（带 connect，好友可加入同一房间）。
                    self.steam_refresh_presence(ctx.time.time_since_start().as_secs_f64());
                    // 对局中也刷新 ping（HUD 显示到对端的延迟，卡顿来源一眼可辨）。
                    self.steam_refresh_network_info(ctx);
                    if let Some(mut host) = std::mem::take(&mut self.steam_host_ls) {
                        // Steam host：开局配置·配置同步阶段——收齐各端 PlayerCfg(含自身) → 广播 PlayerCfgAll → 统一开战。
                        // （与局域网 HostGather 同构；用可靠的 RoomState/每帧上行通道 + cfg 包，host 收齐即广播。）
                        if self.net_cfg == NetCfgSync::HostGather {
                            // 本轮配置同步首次进入：清空上一轮残留（cfgs）+ 在途旧包，避免收到旧包当本轮配置（局间绑定被清空的竞态）。
                            if !self.host_cfg_drained {
                                self.host_cfg_drained = true;
                                host.reset_cfgs();
                                host.drain_cfg();
                            }
                            // 保活：HostGather 阶段也收 client 心跳（RoomState，更新在场/配好）+ 广播就绪快照当心跳，双向保活。
                            let mut k_rcv = vec![0u8; 4096];
                            host.poll(&mut k_rcv);
                            host.broadcast_roster_ready(self.steam_local_ready, 0);
                            let mut g_rcv = vec![0u8; 256 * 1024];
                            host.poll_cfg(&mut g_rcv);
                            let cfg_bytes = self.local_player_cfg();
                            let local_cfg_ready = !cfg_bytes.is_empty();
                            if local_cfg_ready {
                                host.set_local_cfg(cfg_bytes);
                            }
                            // 诊断（节流）：HostGather 里等“谁”。本地配置是否就绪 + 已收到几个客户端配置 + 在场/配好 + 传输收发统计。
                            self.steam_lobby_wait_ticks = self.steam_lobby_wait_ticks.wrapping_add(1);
                            if self.steam_lobby_wait_ticks % 60 == 1 {
                                let st = host.transport_ref().send_stats();
                                let tc = host.transport_ref().recv_tag_counts();
                                eprintln!(
                                    "[steam-host] HostGather waiting: local_cfg={local_cfg_ready} cfgReady={} pres={} bd={}/{} exp={} stats(recv={},queued={}) tags(total={},skill={},room={}) cfgSeen={}",
                                    host.cfg_ready_count(),
                                    host.present_clients_count(),
                                    host.build_done_clients_count(),
                                    host.expected_clients(),
                                    host.expected_clients(),
                                    st.2,
                                    st.1,
                                    tc.0,
                                    tc.1,
                                    tc.2,
                                    host.player_cfg_packets_seen(),
                                );
                            }
                            if host.all_cfgs() {
                                // 竞态保护：等配置稳定（连续 HOST_CFG_SETTLE_TICKS 帧）再收集，避免上一局在途旧包让 all_cfgs 提前满足、广播旧配置。
                                if self.host_cfg_settle < HOST_CFG_SETTLE_TICKS {
                                    self.host_cfg_settle += 1;
                                    self.steam_host_ls = Some(host);
                                    self.accumulator = 0.0;
                                    return Ok(());
                                }
                                // 提前记住参与玩家数（host 只读）与首局标志，归还 host 后据此重建 world。
                                let p = host.participants_count();
                                let stage_first = self.pre_game_config;
                                let all = match host.collect_cfgs() {
                                    Some(a) => a,
                                    None => {
                                        eprintln!("[cfg-sync] 配置未收齐（竞态），本轮放弃同步，下一帧重试");
                                        self.steam_host_ls = Some(host);
                                        self.accumulator = 0.0;
                                        return Ok(());
                                    }
                                };
                                // 诊断：host 收集到各端配置的绑定（确认 client 上报的绑定是否到了 host）。
                                for (h_i, h_bytes) in &all {
                                    if let Some(h_cfg) = game_core::progress::PlayerConfig::decode(h_bytes) {
                                        eprintln!("[cfg-sync] HOST COLLECT idx={} binds={:?}", h_i, h_cfg.key_slots.iter().map(|s| s.map(|x| x.as_u32())).collect::<Vec<_>>());
                                    }
                                }
                                host.broadcast_cfgs(&all);
                                // 广播本局参与玩家 SteamID（按 new index），供各端在 host 掉线时确定性选举新 host。
                                let pids = host.participant_ids(self.steam_my_id);
                                host.broadcast_participants(&pids);
                                let stage = if stage_first { "pre-game" } else { "next round" };
                                eprintln!("[steam-host] synced {} player configs -> {stage} (round {}), participants={p}, ids={pids:?}", all.len(), self.meta.round);
                                host.reset_cfgs();
                                self.steam_host_ls = Some(host); // 归还 host，释放 borrow 后重建 world
                                // 首局开局：把 world/meta 重建为本局参与玩家数（不满员时两端角色一致）；此后多局沿用。
                                if stage_first {
                                    self.stage_world_for_participants(p, STEAM_SEED);
                                }
                                self.apply_player_cfgs(&all);
                                self.teardown_round_end();
                                self.net_cfg = NetCfgSync::Idle;
                                self.pre_game_config = false;
                                self.accumulator = 0.0;
                                return Ok(()); // 同步阶段不推进战斗
                            } else {
                                self.host_cfg_settle = 0;
                            }
                            self.steam_host_ls = Some(host);
                            self.accumulator = 0.0;
                            return Ok(()); // 同步阶段不推进战斗
                        }
                        // Steam host：配置同步完成后这里直接产帧（收齐各端输入才产，seq=0 即统一开始）。
                        let mut hrcv = vec![0u8; 256 * 1024];
                        let mut takeover_bcast = self.steam_host_broadcasting_takeover;
                        while self.accumulator >= TICK {
                            let me = self.local_player_input();
                            host.set_local_input(Some(game_core::netcode::encode_player_input(&me)));
                            host.poll(&mut hrcv);
                            // S2/S4：本 host 收到新 host 的 Takeover → 已被取缔（如客户端误判旧 host 掉线）。
                            // 停止作为权威、退回主菜单，避免与新 host 双权威脑裂（孤儿 host 续产帧）。
                            if host.is_superseded() {
                                eprintln!("[steam-host] SUPERSEDED: 收到新 host 的 Takeover，已取缔，退回主菜单");
                                self.reset_to_main_menu();
                                self.accumulator = 0.0;
                                return Ok(());
                            }
                            // 迁移接管后：持续广播 Takeover（基线=快照 seq + 更新后的在线参与集），直到首个在线 client 连上产帧才停。
                            if takeover_bcast {
                                if let Some((_, bseq)) = host.current_snapshot() {
                                    host.broadcast_takeover(*bseq, self.steam_online.clone());
                                }
                            }
                            // 掉线判定（Steam 战斗端）：某 client 连续未上行超时 → 自动 drop（默认输入占位），其余端继续，不空转等它。
                            for dropped_idx in host.auto_drop_idle(HOST_DROP_TICKS) {
                                eprintln!("[steam-host] AUTO-DROP client {dropped_idx} (idle timeout) -> game continues");
                            }
                            if let Some((seq, frame)) = host.try_emit() {
                                takeover_bcast = false; // 首个在线 client 已连上，停止广播 Takeover
                                if seq < 30 {
                                    eprintln!("[steam-host] emit seq={seq}, n_entries={}", frame.len());
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
                                self.note_self_cast();
                                // 周期快照（重连用 + 广播给所有 client，供「host 掉线接管」用）。
                                self.host_frame_count += 1;
                                if self.host_frame_count % SNAPSHOT_EVERY == 0 {
                                    host.broadcast_snapshot(game_core::world_ser::world_to_bytes(&self.world), host.next_seq());
                                }
                                self.accumulator -= TICK;
                            } else {
                                // 诊断（节流）：尝试产帧但没收到 client 输入 → 说明 host→client 帧/输入链可能断。
                                self.steam_lobby_wait_ticks = self.steam_lobby_wait_ticks.wrapping_add(1);
                                if self.steam_lobby_wait_ticks % 120 == 1 {
                                    eprintln!("[steam-host] trying to emit but waiting for client input (present={})", host.present_clients_count());
                                }
                                break;
                            }
                        }
                        self.steam_host_broadcasting_takeover = takeover_bcast;
                        self.steam_host_ls = Some(host);
                    } else if let Some(mut cli) = std::mem::take(&mut self.steam_cli_ls) {
                        // Steam client：开局配置·配置同步阶段——上报我的 PlayerCfg，等 host 广播 PlayerCfgAll 后完成。
                        if self.net_cfg == NetCfgSync::ClientWait {
                            // 保活：配置同步阶段也每帧上行（就绪 + 配好 + 在场输入），既防止 P2P 空闲被拆，
                            // 也持续向 host 续报 build_done=true（host 端判定“所有端配完”始终成立）。
                            let ref_enc = game_core::netcode::encode_player_input(&self.local_player_input());
                            let _ = cli.send_room_state(self.steam_local_ready, self.steam_build_done, &ref_enc);
                            // 上报我的 PlayerCfg（client 每帧发一次，确保 host 无论如何进入 HostGather 都能收到；
                            // Steam 可靠通道保证送达，重发仅为覆盖“host 尚未开始收集”的时序）。
                            let cfg_bytes = self.local_player_cfg();
                            let cfg_len = cfg_bytes.len();
                            let send_ok = if cfg_bytes.is_empty() {
                                false
                            } else {
                                cli.send_cfg(&cfg_bytes).is_ok()
                            };
                            // 诊断（节流）：确认 ClientWait 每帧真的在发 PlayerCfg（大小/发送是否成功/直发还是入队补发队列）。
                            self.steam_lobby_wait_ticks = self.steam_lobby_wait_ticks.wrapping_add(1);
                            if self.steam_lobby_wait_ticks % 60 == 1 {
                                let st = cli.transport_ref().send_stats();
                                eprintln!(
                                    "[steam-client] ClientWait sending cfg len={cfg_len} ok={send_ok} expect_seq={} stats(direct={},queued={},recv={})",
                                    cli.expect_seq(),
                                    st.0,
                                    st.1,
                                    st.2,
                                );
                            }
                            let mut c_rcv = vec![0u8; 256 * 1024];
                            if let Some((all, participants)) = cli.recv_cfg_all(&mut c_rcv)? {
                                let stage_first = self.pre_game_config;
                                let stage = if stage_first { "pre-game" } else { "next round" };
                                eprintln!("[steam-client] got {} player configs -> {stage} (round {}), participants={}", all.len(), self.meta.round, participants.len());
                                // 收 host 广播的本局参与玩家 SteamID（按 new index），供 host 掉线时确定性选举新 host。
                                if let Ok(Some(ids)) = cli.recv_participants(&mut c_rcv) {
                                    self.steam_participants = ids.clone();
                                    self.steam_online = ids.clone(); // 初始在线集 = 全参与
                                    eprintln!("[steam-client] got participants ids={ids:?}");
                                }
                                self.steam_cli_ls = Some(cli); // 归还 cli，释放 borrow 后重建 world
                                // 首局开局：用 host 广播的参与玩家数重建 world/meta（不满员时两端角色一致），
                                // 并把本机索引更新为“在参与列表中的位置”（原 index 可能因缺席而收缩）。
                                if stage_first && !participants.is_empty() {
                                    let p = participants.len();
                                    let me_orig = self.steam_my_index;
                                    let me_new = participants.iter().position(|&x| x == me_orig).unwrap_or(0) as u8;
                                    self.stage_world_for_participants(p, STEAM_SEED);
                                    self.steam_my_index = me_new;
                                    eprintln!("[steam-client] reindexed me orig={me_orig} -> new={me_new}");
                                }
                                self.apply_player_cfgs(&all);
                                self.teardown_round_end();
                                self.net_cfg = NetCfgSync::Idle;
                                self.pre_game_config = false;
                                self.accumulator = 0.0;
                                return Ok(()); // 同步阶段不推进战斗
                            }
                            self.steam_cli_ls = Some(cli);
                            self.accumulator = 0.0;
                            return Ok(()); // 同步阶段不推进战斗
                        }
                        // Steam client：就绪/配置已完成；这里上行输入 + 严格按权威帧推进（乐观预测关）。
                        // 上行用 `send_room_state`（合包，Steam P2P 下实测可靠）；`send_input` 单独发送曾实测间歇丢。
                        let mut c_rcv = vec![0u8; 256 * 1024];
                        // 已判定掉线：冻结世界，进入重连入口（按 R 拉快照重建），不再推进世界（避免与 host 分叉）。
                        if self.conn_dropped {
                            self.poll_steam_reconnect(ctx, &mut cli);
                            self.steam_cli_ls = Some(cli);
                            self.accumulator = 0.0;
                            return Ok(());
                        }
                        // 正处于「主机迁移/重连探测」流程：推进迁移状态机（探测 host 存活 / 选举 / 接管 / 重定向），
                        // 不推进世界（避免与最终权威分叉）。
                        if self.steam_migrating {
                            let leftover = self.poll_steam_migration(cli, &mut c_rcv)?;
                            // 若是新 host 已消费 cli（转为 host_ls），则不归还；否则归还 cli。
                            if let Some(cli) = leftover {
                                self.steam_cli_ls = Some(cli);
                            }
                            self.accumulator = 0.0;
                            return Ok(());
                        }
                        while self.accumulator >= TICK {
                            let me = self.local_player_input();
                            let enc = game_core::netcode::encode_player_input(&me);
                            let _ = cli.send_room_state(self.steam_local_ready, self.steam_build_done, &enc);
                            if let Some(ents) = cli.step_frame(&mut c_rcv).ok().flatten() {
                                self.steam_cli_stale_ticks = 0; // 收到权威帧 → 清零掉线计数
                                let n = self.world.players.len();
                                let mut inputs = vec![PlayerInput::default(); n];
                                for (idx, bytes) in ents {
                                    if (idx as usize) < n {
                                        inputs[idx as usize] =
                                            game_core::netcode::decode_player_input(&bytes).unwrap_or_default();
                                    }
                                }
                                self.world.step(inputs, ticking);
                                self.note_self_cast();
                                // 诊断：打印推进到哪一帧（前若干帧/变化时不刷屏）。
                                let last = cli.expect_seq().saturating_sub(1);
                                if self.steam_cli_last_seq != last {
                                    let n_ents = self.world.players.len();
                                    eprintln!("[steam-client] frame -> seq={last}, n_ents={n_ents}");
                                    self.steam_cli_last_seq = last;
                                }
                                self.accumulator -= TICK;
                            } else {
                                // 本帧无权威帧：累计掉线计数，超阈值进入「主机迁移/重连探测」（host 可能掉线）。
                                self.steam_cli_stale_ticks = self.steam_cli_stale_ticks.saturating_add(1);
                                if self.steam_cli_stale_ticks >= CLIENT_STALE_TICKS {
                                    eprintln!("[steam-client] NO frames for {} ticks -> entering reconnect-probe / host-migration", self.steam_cli_stale_ticks);
                                    self.steam_migrating = true;
                                    self.steam_migrate_ticks = 0;
                                    self.steam_new_host_id = 0;
                                    self.steam_cli_stale_ticks = 0;
                                    self.accumulator = 0.0;
                                    break;
                                }
                                break; // 等权威帧（不扣 accumulator，避免时间凭空流逝导致分叉）
                            }
                        }
                        self.steam_cli_ls = Some(cli);
                    }
                }
                // 非 Steam 模式才走 UDP host/client/单机 分支（Steam 已在上面的块里推进了世界）。
                if !self.steam_active() {
                // 联网 · 开房作 host：接收 client 加入，全部到齐后移交 HostLockstep（不强制 READY/GO，
                // 由“host 收齐输入即产首帧”自然统一起始。）。
                self.poll_host_join_phase();
                if let Some(mut host) = std::mem::take(&mut self.net_host_ls) {
                    // 多于局：学习结束后的「配置同步」阶段——收齐各端配置(含自身) → 广播 PlayerCfgAll → 完成。
                    if self.net_cfg == NetCfgSync::HostGather {
                        // 本轮配置同步首次进入：清空上一轮残留（cfgs）+ 在途旧包，避免收到旧包当本轮配置（局间绑定被清空的竞态）。
                        if !self.host_cfg_drained {
                            self.host_cfg_drained = true;
                            host.reset_cfgs();
                            host.drain_cfg();
                        }
                        let mut g_rcv = vec![0u8; 256 * 1024];
                        host.poll_cfg(&mut g_rcv);
                        let cfg_bytes = self.local_player_cfg();
                        if !cfg_bytes.is_empty() {
                            host.set_local_cfg(cfg_bytes);
                        }
                        if host.all_cfgs() {
                            // 竞态保护：等配置稳定（连续 HOST_CFG_SETTLE_TICKS 帧）再收集，避免上一局在途旧包让 all_cfgs 提前满足、广播旧配置。
                            if self.host_cfg_settle < HOST_CFG_SETTLE_TICKS {
                                self.host_cfg_settle += 1;
                                self.net_host_ls = Some(host);
                                return Ok(());
                            }
                            let all = match host.collect_cfgs() {
                                Some(a) => a,
                                None => {
                                    eprintln!("[cfg-sync] 配置未收齐（竞态），本轮放弃同步，下一帧重试");
                                    self.net_host_ls = Some(host);
                                    return Ok(());
                                }
                            };
                            // 诊断：host 收集到各端配置的绑定（确认 client 上报的绑定是否到了 host）。
                            for (h_i, h_bytes) in &all {
                                if let Some(h_cfg) = game_core::progress::PlayerConfig::decode(h_bytes) {
                                    eprintln!("[cfg-sync] HOST COLLECT idx={} binds={:?}", h_i, h_cfg.key_slots.iter().map(|s| s.map(|x| x.as_u32())).collect::<Vec<_>>());
                                }
                            }
                            host.broadcast_cfgs(&all);
                            let stage = if self.pre_game_config { "pre-game" } else { "next round" };
                            eprintln!("[meta] host synced {} player configs -> {stage} (round {})", all.len(), self.meta.round);
                            self.apply_player_cfgs(&all);
                            self.teardown_round_end();
                            host.reset_cfgs(); // 为下一局复用
                            self.net_cfg = NetCfgSync::Idle;
                            self.pre_game_config = false;
                        } else {
                            self.host_cfg_settle = 0;
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
                            self.note_self_cast();
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
                    // 缓存本机序号：net_link 已被 take，期间的 self_index()/local_player_cfg() 不能回落到 PLAYER_ID=0。
                    self.lan_my_index = link.my_index();
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
                            self.note_self_cast();
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
                    // host 尚未收齐 client（net_host 仍在 handshake 阶段、未转 HostLockstep）：不推进世界，
                    // 否则会误走下面的“单机带 AI”分支（bot_targets 为空）导致索引越界崩溃。
                    if self.net_host.is_some() {
                        self.accumulator = 0.0;
                        return Ok(());
                    }
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
                            self.note_self_cast();
                        } else {
                            let inputs = self.compute_inputs();
                            self.world.step(inputs, ticking);
                            self.note_self_cast();
                        }
                        self.accumulator -= TICK;
                    }
                }
                } // end 非 Steam 分支
                // 注：施法命令 `pending_cast` 已在 `local_player_input()` 里随编码消费（take），
                // 不需要这里按 `is_windup()` 反推清除（那段逻辑对零前摇技能永不成立，且查的是硬编码 PLAYER_ID）。
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

    /// 诊断（本次会话加）：打印收到的键盘事件 + 窗口焦点变化。
    /// 作用：区分“按键根本没到本进程（窗口无焦点/多窗口）”与“到了但逻辑没处理”。
    /// ggez 的键盘由 winit 事件分发，**窗口无焦点时不发 KeyboardInput**，直接观察日志即可定位。
    fn key_down_event(&mut self, _ctx: &mut Context, input: ggez::input::keyboard::KeyInput, repeated: bool) -> GameResult {
        eprintln!("[input] key_down focused logical={:?} repeated={repeated}", input.event.logical_key);
        Ok(())
    }

    fn focus_event(&mut self, _ctx: &mut Context, gained: bool) -> GameResult {
        eprintln!("[input] window focus gained={gained}");
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        // S12：大厅操作进行中（连接中）显示「连接中…」，不进房间/菜单界面。
        #[cfg(feature = "steam")]
        if self.steam_lobby_pending.is_some() {
            return self.draw_steam_connecting(ctx);
        }
        if self.app == AppState::MainMenu {
            return self.draw_menu(ctx);
        }
        self.draw_scene(ctx)
    }
}

impl Game {
    /// 文本输入回调（由自定义事件循环在 winit 的 `Ime::Commit` 事件上调用）：
    /// 把输入法确认提交的字符追加到当前聚焦的文本字段。支持中文 IME（建房界面/编辑房间信息的房间名与备注）。
    /// 注意：winit 0.30 已移除 `ReceivedCharacter`，文本只走 `Ime::Commit`（见事件循环处 `WindowEvent::Ime` 分支）。
    /// 仅在对应界面且聚焦文本字段（0/1）时生效；其他界面忽略。
    fn on_text_input(&mut self, text: &str) {
        // 标记最近一次 IME 提交发生在下一帧（即 `just(c)` 将要运行的帧），供 ASCII 白名单去重（C8）。
        self.last_ime_commit_frame = self.frame.wrapping_add(1);
        #[cfg(feature = "steam")]
        {
            if text.is_empty() {
                return;
            }
            // 剔除控制字符（\r \n \t 等）；中文全角字符/空格都保留。
            let clean: String = text.chars().filter(|c| !c.is_control()).collect();
            if clean.is_empty() {
                return;
            }
            let target: Option<&mut String> =
                if self.steam_lobby_create && self.steam_create_focus <= 1 {
                    Some(if self.steam_create_focus == 0 {
                        &mut self.steam_create_name
                    } else {
                        &mut self.steam_create_note
                    })
                } else if self.steam_room_edit && self.steam_room_edit_focus <= 1 {
                    Some(if self.steam_room_edit_focus == 0 {
                        &mut self.steam_edit_name
                    } else {
                        &mut self.steam_edit_note
                    })
                } else {
                    None
                };
            if let Some(buf) = target {
                for ch in clean.chars() {
                    if buf.len() < 80 {
                        buf.push(ch);
                    }
                }
            }
        }
        #[cfg(not(feature = "steam"))]
        {
            let _ = text;
        }
    }

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
        // 开局不带默认技能：玩家从零在配置界面选。
        self.app = AppState::MainMenu;
        // 放弃联网连接（UDP socket / 握手 / 帧同步关闭）。
        self.net_link = None;
        self.lan_my_index = PLAYER_ID as u8;
        self.net_host = None;
        self.net_host_ls = None;
        self.net_ready = false;
        self.net_cfg = NetCfgSync::Idle;
        // 放弃 Steam 会话（P2P 连接 / lockstep / 房间状态），回主菜单后重建。
        #[cfg(feature = "steam")]
        {
            // 先清 Rich Presence：会话还活着（lockstep 仍持有 transport）时才写得到，
            // 一旦下面把 lockstep 丢掉，Steam Client 就没了，好友会一直看到「加入游戏」。
            self.steam_clear_presence();
            self.steam_host_ls = None;
            self.steam_cli_ls = None;
            self.steam_in_lobby = false;
            self.steam_active = false;
            self.steam_local_ready = false;
            self.steam_build_done = false;
            self.steam_was_all_ready = false;
            self.steam_countdown = 0.0;
            self.steam_manual_start_pending = false;
            self.steam_manual_countdown = false;
            self.steam_manual_ms = 0;
            self.steam_roster_ready = Vec::new();
            self.steam_roster_all_ready = false;
            self.steam_all_ready = false;
            self.steam_roster = Vec::new();
            self.steam_lobby_id = None;
            self.steam_room_edit = false;
            self.steam_room_locked = false;
            self.steam_room_edit_focus = 0;
            self.steam_edit_name = String::new();
            self.steam_edit_note = String::new();
            self.steam_lobby_menu = false;
            self.steam_lobby_create = false;
            self.steam_lobby_list = false;
            self.steam_lobby_pending = None;
            self.steam_list_requested = false;
            self.steam_list_lobbies = Vec::new();
            self.steam_join_lobby_id = None;
            self.steam_roster_refresh_ticks = 0;
            self.steam_cli_stale_ticks = 0;
            self.steam_participants = Vec::new();
            self.steam_online = Vec::new();
            self.steam_migrating = false;
            self.steam_migrate_ticks = 0;
            self.steam_new_host_id = 0;
            self.steam_host_broadcasting_takeover = false;
            // 邀请面板/好友列表状态复位（presence 已在本块开头清掉）。
            self.steam_friend_list = false;
            self.steam_friends = Vec::new();
            self.steam_friend_selection = 0;
            self.steam_friend_hint = String::new();
            // ping/头像缓存（换房间就不该复用上一房的成员数据）。
            self.steam_pings = Vec::new();
            self.steam_avatars = Vec::new();
            self.steam_net_ticks = 0;
            // 本场战绩上报标记/提示条复位（下一场重新上报）。
            self.steam_stats_recorded = false;
            self.steam_stats_snapshot = None;
            self.steam_toast = (String::new(), 0.0);
            // 进房时 `steam_sess` 会被消费掉（传输归 lockstep），回到主菜单后可再初始化一次
            // （否则好友邀请与房间列表在主菜单上会永久失效）。
            self.steam_session_tried = false;
        }
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

    /// 「邀请好友」面板的输入（房间界面按 I 展开，非模态——房间网络逻辑每帧照常跑）：
    /// ↑/↓ 选择、**回车 邀请选中好友**、A 打开 Steam 邀请窗口（可勾多位）、R 刷新、I/Q 收起面板。
    #[cfg(feature = "steam")]
    fn steam_friend_list_update(&mut self, ctx: &Context) {
        use ggez::input::keyboard::Key;
        use winit::keyboard::NamedKey;
        let just = |k: char| ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.to_string().into()));
        let just_named = |n: NamedKey| ctx.keyboard.is_logical_key_just_pressed(&Key::Named(n));
        // 收起面板（I 或 Q；Q 第一次只收起面板，再按才退出房间）。
        if just('i') || just('I') || just('q') || just('Q') {
            self.steam_friend_list = false;
            return;
        }
        if just('r') || just('R') {
            self.steam_refresh_friends();
            self.steam_friend_hint = "已刷新好友列表".to_string();
            return;
        }
        if just('a') || just('A') {
            match self.steam_lobby_id {
                Some(lid) => {
                    if let Some(t) = self.steam_transport() {
                        net_steam::session::open_invite_dialog(t, lid);
                        self.steam_friend_hint = "已打开 Steam 邀请窗口（可勾选多位好友）".to_string();
                    }
                }
                None => self.steam_friend_hint = "尚未在房间里，无法邀请".to_string(),
            }
            return;
        }
        let n = self.steam_friends.len();
        if n == 0 {
            return;
        }
        if just_named(NamedKey::ArrowDown) {
            self.steam_friend_selection = (self.steam_friend_selection + 1) % n;
        } else if just_named(NamedKey::ArrowUp) {
            self.steam_friend_selection = (self.steam_friend_selection + n - 1) % n;
        }
        if just_named(NamedKey::Enter) || just('\r') {
            let sel = self.steam_friends[self.steam_friend_selection].clone();
            if sel.in_lobby {
                self.steam_friend_hint = format!("{} 已经在房间里了", sel.name);
                return;
            }
            match self.steam_lobby_id {
                Some(lid) => {
                    if let Some(t) = self.steam_transport() {
                        net_steam::session::invite_friend(t, lid, sel.id);
                        self.steam_friend_hint = format!("已邀请 {}{}", sel.name, if sel.online { "" } else { "（离线，邀请会等到其上线）" });
                    }
                }
                None => self.steam_friend_hint = "尚未在房间里，无法邀请".to_string(),
            }
        }
    }

    /// 房主「编辑房间信息」子界面输入：改房间名/备注，回车保存（写回 matchmaking 元数据），Q 取消；
    /// 附带 `L` 锁定/解锁房间（`set_lobby_joinable`；人数上限建房时固定，用锁房代替“开房后改人数”）。
    #[cfg(feature = "steam")]
    fn steam_room_edit_update(&mut self, ctx: &Context, _dt: f64) -> GameResult {
        use ggez::input::keyboard::Key;
        use winit::keyboard::NamedKey;
        let just = |k: char| ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.to_string().into()));
        let just_named = |n: NamedKey| ctx.keyboard.is_logical_key_just_pressed(&Key::Named(n));
        // 锁房：L 切换（host 且记录到字段），即时 set_lobby_joinable。
        if just('l') || just('L') {
            if let Some(ls) = self.steam_host_ls.as_ref() {
                if let Some(lid) = self.steam_lobby_id {
                    let mm = ls.transport_ref().matchmaking();
                    let lobby = net_steam::steamworks::LobbyId::from_raw(lid);
                    self.steam_room_locked = !self.steam_room_locked;
                    mm.set_lobby_joinable(lobby, !self.steam_room_locked);
                    eprintln!("[steam-room] set joinable={} (locked={})", !self.steam_room_locked, self.steam_room_locked);
                }
            }
        }
        // 字段切换 0=房间名 1=备注（↑/↓ 或 Tab）。
        if just_named(NamedKey::ArrowUp) || just_named(NamedKey::ArrowDown) || just_named(NamedKey::Tab) {
            self.steam_room_edit_focus = (self.steam_room_edit_focus + 1) % 2;
        }
        if just('q') || just('Q') {
            self.steam_room_edit = false;
            return Ok(());
        }
        if just_named(NamedKey::Enter) || just('\r') {
            // 保存：写回房间名/备注。
            if let Some(ls) = self.steam_host_ls.as_ref() {
                if let Some(lid) = self.steam_lobby_id {
                    let mm = ls.transport_ref().matchmaking();
                    let lobby = net_steam::steamworks::LobbyId::from_raw(lid);
                    let name = if self.steam_edit_name.trim().is_empty() {
                        "未命名房间"
                    } else {
                        self.steam_edit_name.trim()
                    };
                    mm.set_lobby_data(lobby, net_steam::session::ROOM_NAME_KEY, name);
                    mm.set_lobby_data(lobby, net_steam::session::ROOM_NOTE_KEY, self.steam_edit_note.trim());
                    eprintln!("[steam-room] saved name='{name}' note='{}'", self.steam_edit_note.trim());
                }
            }
            self.steam_room_edit = false;
            return Ok(());
        }
        // 文本输入：聚焦字段 0=名 1=备注。
        if just_named(NamedKey::Backspace) {
            let buf = if self.steam_room_edit_focus == 0 { &mut self.steam_edit_name } else { &mut self.steam_edit_note };
            buf.pop();
            return Ok(());
        }
        let buf = if self.steam_room_edit_focus == 0 { &mut self.steam_edit_name } else { &mut self.steam_edit_note };
        if buf.len() < 80 && !ime_commit_suppresses_ascii(self.frame, self.last_ime_commit_frame) {
            // 本帧已由 IME 提交文本时不走 ASCII 白名单，避免同一物理键重复插入（C8）。
            const CHARS: &str = " abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.(),;:!?'\"-_#@%&*+=/";
            for c in CHARS.chars() {
                if just(c) {
                    buf.push(c);
                    return Ok(());
                }
            }
        }
        self.accumulator = 0.0;
        Ok(())
    }

    /// 绘制「编辑房间信息」界面（房主）：房间名/备注 两字段 + 锁房状态。
    #[cfg(feature = "steam")]
    fn draw_steam_room_edit(&self, canvas: &mut Canvas, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;
        draw_text(canvas, ctx, "编辑房间信息", 36.0, Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.26 }, true)?;
        let labels = ["房间名", "备注"];
        let vals = [self.steam_edit_name.clone(), self.steam_edit_note.clone()];
        let mut y = sh * 0.42;
        let label_w = 200.0;
        let box_w = 420.0;
        let box_h = 56.0;
        let left = cx - box_w / 2.0 - 40.0;
        for i in 0..2 {
            let selected = i == self.steam_room_edit_focus;
            draw_text(canvas, ctx, labels[i], 24.0, Color::from_rgb(220, 224, 235), Point2 { x: left + (label_w + box_w) / 2.0, y: y + box_h / 2.0 - 16.0 }, true)?;
            let bg_col = if selected { Color::from_rgb(52, 60, 74) } else { Color::from_rgb(30, 34, 44) };
            let bg = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), graphics::Rect::new(left + label_w, y, box_w, box_h), bg_col)?;
            canvas.draw(&bg, graphics::DrawParam::new());
            let disp = if vals[i].is_empty() {
                if i == 0 { "（输入房间名）".to_string() } else { "（可选）".to_string() }
            } else {
                format!("  {}", vals[i])
            };
            draw_text(canvas, ctx, &disp, 22.0, if vals[i].is_empty() { Color::from_rgb(120, 130, 150) } else { Color::WHITE }, Point2 { x: left + label_w + box_w / 2.0, y: y + box_h / 2.0 - 14.0 }, true)?;
            y += box_h + 30.0;
        }
        let lock_txt = if self.steam_room_locked { "[v] 已锁定（他人不能加入）" } else { "[ ] 未锁定（可加入）" };
        draw_text(canvas, ctx, &format!("房间锁：{lock_txt}（按 L 切换）"), 22.0, if self.steam_room_locked { Color::from_rgb(235, 150, 90) } else { Color::from_rgb(140, 200, 160) }, Point2 { x: cx, y: y + 30.0 }, true)?;
        draw_text(canvas, ctx, "人数上限建房时固定（steamworks 限制），用房间锁控制新入", 17.0, Color::from_rgb(150, 160, 178), Point2 { x: cx, y: y + 62.0 }, true)?;
        draw_text(canvas, ctx, "回车 保存    Q 取消    L 锁房", 20.0, Color::from_rgb(160, 200, 255), Point2 { x: cx, y: sh * 0.90 }, true)?;
        Ok(())
    }

    /// host 在房间阶段刷新成员名单（roster）：client 加入后 host 才能看到并显示新成员。
    /// 从 matchmaking 读 `lobby_members` → `LobbyPlayerTable`（host=槽0，其余按 SteamID 升序）保持槽位与 lockstep 一致。
    #[cfg(feature = "steam")]
    fn steam_refresh_roster(&mut self) {
        let Some(lid) = self.steam_lobby_id else { return; };
        let Some(ls) = self.steam_host_ls.as_ref() else { return; };
        let t = ls.transport_ref();
        let host_id = t.steam_id();
        let mm = t.matchmaking();
        let lobby = net_steam::steamworks::LobbyId::from_raw(lid);
        let members: Vec<net_steam::lobby::SteamID> = mm.lobby_members(lobby).iter().map(|s| net_steam::lobby::SteamID(s.raw())).collect();
        let table = net_steam::lobby::LobbyPlayerTable::new(net_steam::lobby::SteamID(host_id), members);
        let fr = t.friends();
        let mut roster = Vec::with_capacity(table.total_players());
        for (slot, id) in table.identities_in_order() {
            let name = fr.get_friend(net_steam::steamworks::SteamId::from_raw(id.0)).name();
            roster.push((slot, name, id.0));
        }
        // 仅当成员集合变化时更新（避免每帧重建 + 日志）；并检测“有玩家离开”给 host 提示。
        let changed = roster.len() != self.steam_roster.len()
            || roster.iter().zip(self.steam_roster.iter()).any(|(a, b)| a != b);
        let new_len = roster.len();
        let prev_len = self.steam_last_roster_len;
        self.steam_last_roster_len = new_len;
        if changed {
            if prev_len > new_len && prev_len > 1 {
                eprintln!("[steam-host] a player left the room (members {prev_len} -> {new_len}), waiting");
            } else {
                eprintln!("[steam-host] roster updated: {new_len} member(s)");
            }
            self.steam_roster = roster;
        }
    }

    /// 退出房间：`leave_lobby`（让 Steam 后端不再占席）+ 清理会话回到主菜单。
    #[cfg(feature = "steam")]
    fn steam_leave_room(&mut self) {
        if let Some(lid) = self.steam_lobby_id {
            let lobby = net_steam::steamworks::LobbyId::from_raw(lid);
            if let Some(host) = self.steam_host_ls.as_ref() {
                host.transport_ref().matchmaking().leave_lobby(lobby);
            } else if let Some(cli) = self.steam_cli_ls.as_ref() {
                cli.transport_ref().matchmaking().leave_lobby(lobby);
            }
        }
        eprintln!("[steam] leave room -> main menu");
        self.steam_room_edit = false;
        self.steam_lobby_id = None;
        self.steam_room_locked = false;
        self.reset_to_main_menu();
    }

    /// Steam 房间/就绪阶段（每帧）：client 每帧上行「就绪+在场」合包；host poll 收各端、全员就绪倒数计时进配置。
    /// 房主按 E 进入编辑房间信息界面（见 `steam_room_edit_update`）。
    #[cfg(feature = "steam")]
    fn steam_lobby_update(&mut self, ctx: &Context, dt: f64) -> GameResult {
        use ggez::input::keyboard::Key;
        // 「邀请好友」面板展开时由面板优先吃键（I/Q 收起、↑↓ 选择、回车 邀请、A 开 Steam 邀请窗口、R 刷新）。
        // 面板是**非模态**的：下面房间的网络逻辑（上行/广播/倒计时）照常每帧跑，
        // 否则 host 打开面板挑人时 client 会因收不到 host 心跳而判定「host 已离开」自动退房。
        let panel_open = self.steam_friend_list;
        // I：展开/收起「邀请好友」面板（展开时拉一次好友列表）。
        let i_pressed = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("i".into()))
            || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("I".into()));
        if i_pressed && !panel_open {
            self.steam_friend_list = true;
            self.steam_friend_hint = String::new();
            self.steam_refresh_friends();
            eprintln!("[steam-invite] friend list opened ({} friends)", self.steam_friends.len());
            self.accumulator = 0.0;
            return Ok(());
        }
        if panel_open {
            self.steam_friend_list_update(ctx);
        }
        // 房间阶段只用 [U] 就绪/取消就绪；不再用 o/空格（避免 o 多重语义、避免与“配置确认”混淆）。
        let ready_pressed = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("u".into()))
            || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("U".into()));
        // Q：退出房间（leave_lobby + 回主菜单）。面板展开时 Q 只收起面板（由面板处理），避免误退出。
        let q_pressed = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("q".into()))
            || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("Q".into()));
        if q_pressed && !panel_open {
            self.steam_leave_room();
            self.accumulator = 0.0;
            return Ok(());
        }
        // host 按 E 进入「编辑房间信息」子界面（改房间名/备注；人数上限建房时固定，走锁房代替）。
        let e_pressed = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("e".into()))
            || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("E".into()));
        if e_pressed && !panel_open && self.steam_host_ls.is_some() {
            let (cur_name, cur_note) = self.steam_current_room_info();
            self.steam_edit_name = cur_name;
            self.steam_edit_note = cur_note;
            self.steam_room_edit_focus = 0;
            self.steam_room_edit = true;
            self.accumulator = 0.0;
            return Ok(());
        }
        // 倒计时锁定窗口：仅 host 端维护 `steam_was_all_ready`/`steam_countdown`；client 端恒为 false/0 → locked=false。
        // 锁定窗口内忽略「按 U 取消就绪」（防止有人卡在最后两秒取消导致不同步）。
        // client 端不满员手动倒计时用 host 广播的 manual_ms 判锁定，最后 LOCK 秒内不可按 U 取消（与 host 端一致）。
        let locked = (self.steam_was_all_ready && self.steam_countdown <= STEAM_COUNTDOWN_LOCK_SECS)
            || (self.steam_cli_ls.is_some() && self.steam_manual_ms > 0 && (self.steam_manual_ms as f32) / 1000.0 <= STEAM_COUNTDOWN_LOCK_SECS);
        if ready_pressed && !locked && !panel_open {
            self.steam_local_ready = !self.steam_local_ready;
            if !self.steam_local_ready {
                // 本端取消就绪：立即重置本地倒计时（不依赖 host 快照回传，避免“取消后重准备不重新数 5 秒”）。
                self.steam_was_all_ready = false;
                self.steam_countdown = 0.0;
            }
            eprintln!("[steam-lobby] local ready = {}", self.steam_local_ready);
        } else if ready_pressed && locked {
            eprintln!("[steam-lobby] ignoring ready-cancel during locked countdown");
        }
        let mut entered_config = false;
        // 本机当前输入（房间阶段就用它做「在场信号」，对齐局域网 upload；与对局开始后一致）。
        let presence_enc = game_core::netcode::encode_player_input(&self.local_player_input());
        if let Some(cli) = self.steam_cli_ls.as_mut() {
            // client：房间阶段用「就绪+在场+配好」合包持续上行（`RoomState`），走已证实可靠的输入在场通道。
            // 房间阶段 build_done 恒为 false（进配置后才置 true）。
            let room_res = cli.send_room_state(self.steam_local_ready, self.steam_build_done, &presence_enc);
            if let Err(e) = room_res {
                if self.steam_last_sent_ready.is_none() {
                    eprintln!("[steam-client] send_room_state failed: {e:?}");
                    self.steam_last_sent_ready = Some(self.steam_local_ready);
                }
            } else if self.steam_last_sent_ready != Some(self.steam_local_ready) {
                eprintln!("[steam-client] sent room_state ready={} to host", self.steam_local_ready);
                self.steam_last_sent_ready = Some(self.steam_local_ready);
            }
            // host 离开检测：每帧先累计“沉默”帧数；收不到 host 广播（RosterReady 心跳）累计，收到即清零。
            self.steam_lobby_silent_ticks = self.steam_lobby_silent_ticks.saturating_add(1);
            // 单次排空读 host 房间入包：StartConfig（进配置）与 RosterReady（界面）一次分类，绝不互吞。
            let mut rcv = [0u8; 256 * 1024];
            if let Ok((got_cfg, roster)) = cli.recv_room_inbox(&mut rcv) {
                if got_cfg {
                    eprintln!("[steam-client] host says all ready -> config menu");
                    entered_config = true;
                }
                if let Some((entries, manual_ms)) = roster {
                    self.steam_lobby_silent_ticks = 0; // 收到 host 广播 → 心跳正常。
                    self.steam_roster_ready = entries.clone();
                    // 持久记录 host 广播的手动倒计时剩余毫秒（仅收到新快照时更新，避免没收到包的帧回退闪烁）。
                    self.steam_manual_ms = manual_ms;
                    eprintln!("[steam-client] roster ready snapshot: {entries:?} manual_ms={manual_ms}");
                    let roster_cnt = self.world.players.len();
                    // 持久记录：仅在收到新快照时更新；若本帧恰好没收到广播，沿用上次快照，
                    // 避免“按 U 就绪 / 倒计时”在没收到广播的帧回退 false 而闪烁。
                    self.steam_roster_all_ready =
                        entries.len() >= roster_cnt && entries.iter().all(|(_, r)| *r);
                }
            }
            let roster_all_ready = self.steam_roster_all_ready;
            // client 端就绪倒计时：与 host 一致的缓冲，避免“一看到全员就绪就抢先进配置”。
            // 正常路径由 host 倒计时归零广播 StartConfig（got_cfg）触发；此处兜底：若 StartConfig 小包被丢，
            // 用可靠 RosterReady 启动同样长度的倒计时，归零后同样进配置，保证两端同时开始。
            // 不满员手动倒计时期间 client 用 host 广播的 manual_ms 判锁定（最后 LOCK 秒内不可按 U 取消），
            // 与 host 端锁定窗口一致，防止最后两秒有人取消导致两端不同步。
            let locked = (self.steam_was_all_ready && self.steam_countdown <= STEAM_COUNTDOWN_LOCK_SECS)
                || (self.steam_manual_ms > 0 && (self.steam_manual_ms as f32) / 1000.0 <= STEAM_COUNTDOWN_LOCK_SECS);
            if !roster_all_ready && !locked {
                self.steam_was_all_ready = false;
                self.steam_countdown = 0.0;
            } else if roster_all_ready && !self.steam_was_all_ready {
                self.steam_was_all_ready = true;
                self.steam_countdown = STEAM_READY_COUNTDOWN_SECS;
                eprintln!("[steam-client] all ready -> countdown {}", self.steam_countdown);
            }
            self.steam_all_ready = roster_all_ready || locked;
            if self.steam_was_all_ready {
                self.steam_countdown = (self.steam_countdown - dt.min(0.25) as f32).max(0.0);
                if self.steam_countdown <= 0.0 {
                    eprintln!("[steam-client] ready countdown zero -> config menu (StartConfig fallback)");
                    entered_config = true;
                }
            }
        } else if let Some(host) = self.steam_host_ls.as_mut() {
            // host：每帧 poll 收客户端（持续在场 + PlayerReady）；要求所有 client 在场 && 全体就绪。
            let mut rcv = [0u8; 256 * 1024];
            host.poll(&mut rcv);
            let all_present = host.saw_all_clients(); // 所有 expected client 都已上行过输入（满员在场）
            let all_clients_ready = host.all_clients_ready();
            let present = host.present_clients_count();
            let expected = host.expected_clients();
            let full = present >= expected; // 是否满员
            // 满员 && 全员（含 host）就绪 → 自动倒计时启动（现有）。
            let full_ready = self.steam_local_ready && all_present && all_clients_ready;
            // 不满员但“当前在场的都就绪” → 不自动倒计时，由 host 手动确认开始（人不满开打由 host 拍板）。
            let underfull_ready = !full && self.steam_local_ready && present > 0 && host.ready_clients_count() == present;
            self.steam_manual_start_pending = underfull_ready && !self.steam_manual_countdown;
            // 每帧广播就绪状态快照，让各端都能看到所有成员的就绪状态（多人一致界面）。
            let manual_ms = if self.steam_manual_countdown {
                (self.steam_countdown * 1000.0).ceil() as u16
            } else {
                0
            };
            host.broadcast_roster_ready(self.steam_local_ready, manual_ms);
            // —— 不满员路径：host 按回车**启动倒计时**（不再立即开始）。
            // 给其他人一个可见的缓冲，期间任何人按 U 取消就绪都会撤销这次倒计时；
            // 归零后与满员路径共用同一段「set_participants + broadcast_start_config」。
            // 面板展开时回车归面板（邀请好友），这里让位，避免“想邀请却开局”。
            if underfull_ready && !panel_open && !self.steam_manual_countdown {
                use winit::keyboard::NamedKey;
                let enter = ctx.keyboard.is_logical_key_just_pressed(&ggez::input::keyboard::Key::Named(NamedKey::Enter))
                    || ctx.keyboard.is_logical_key_just_pressed(&ggez::input::keyboard::Key::Character("\r".into()));
                if enter {
                    self.steam_manual_countdown = true;
                    self.steam_was_all_ready = true;
                    self.steam_countdown = STEAM_READY_COUNTDOWN_SECS;
                    eprintln!("[steam-host] host confirms underfull start ({present}/{expected} clients) -> {STEAM_READY_COUNTDOWN_SECS}s countdown");
                }
            }
            // 手动倒计时期间有人取消就绪 / 有人离场 → 撤销（最后 LOCK 秒内锁定，忽略取消）。
            if self.steam_manual_countdown && !underfull_ready && !locked {
                eprintln!("[steam-host] underfull countdown cancelled (someone un-readied or left)");
                self.steam_manual_countdown = false;
            }
            // —— 倒计时状态机（满员自动 + 不满员手动确认 共用）：可取消，最后 LOCK 秒锁定；归零启动。
            let countdown_active = full_ready || self.steam_manual_countdown;
            if !countdown_active && !locked {
                self.steam_was_all_ready = false;
                self.steam_countdown = 0.0;
                self.steam_manual_countdown = false;
            } else if countdown_active && !self.steam_was_all_ready {
                self.steam_was_all_ready = true;
                self.steam_countdown = STEAM_READY_COUNTDOWN_SECS;
            }
            self.steam_all_ready = countdown_active || locked;
            if self.steam_was_all_ready {
                self.steam_countdown = (self.steam_countdown - dt.min(0.25) as f32).max(0.0);
                if self.steam_countdown <= 0.0 {
                    // 缓冲归零 → 统一广播 StartConfig 进配置（参与集=当前在场者，不满员时只带已到场的人）。
                    let mask = host.present_mask();
                    host.set_participants(&mask);
                    let n = mask.iter().filter(|&&b| b).count();
                    let how = if self.steam_manual_countdown { "manual(underfull)" } else { "full ready" };
                    eprintln!("[steam-host] {how} countdown zero -> start with {n} participant client(s), mask={mask:?}");
                    host.broadcast_start_config();
                    entered_config = true;
                    self.steam_manual_countdown = false;
                }
            }
            if !countdown_active && !locked {
                // 节流诊断：每 ~120 帧打一次，说明“等了谁”（在场/就绪各几何），便于真机定位 Steam 联机卡点。
                self.steam_lobby_wait_ticks = self.steam_lobby_wait_ticks.wrapping_add(1);
                if self.steam_lobby_wait_ticks % 120 == 1 {
                    let pres = host.present_clients_count();
                    let rdy = host.ready_clients_count();
                    let alive = host.connected_clients_count();
                    let pkts = host.ready_packets_seen();
                    let exp = host.expected_clients();
                    eprintln!(
                        "[steam-host] waiting: local_ready={} present_clients={pres}/{exp} ready_clients={rdy}/{exp} alive_conns={alive} ready_pkts={pkts} underfull_ready={underfull_ready} full_ready={full_ready} manual_countdown={}",
                        self.steam_local_ready, self.steam_manual_countdown
                    );
                }
            }
        }
        // client：host 提前离开（超过 N 帧收不到 host 广播）→ 自动退出房间回主菜单（host 不应让 client 永久卡在等待）。
        let host_missing = self.steam_cli_ls.is_some()
            && self.steam_lobby_silent_ticks >= STEAM_LOBBY_SILENT_TIMEOUT_TICKS;
        if host_missing {
            eprintln!(
                "[steam-client] host left (no heartbeat for {} ticks) -> leave room",
                self.steam_lobby_silent_ticks
            );
            self.steam_leave_room();
            self.accumulator = 0.0;
            return Ok(());
        }
        if entered_config {
            self.steam_in_lobby = false;
            self.meta.begin_first_round_config(); // 首局进配置学习（倒计时归零开战）
            self.pre_game_config = true; // 供 Fighting 分支 stage_first 判断（首局重建 world）
            // 本端进入配置：build_done 由玩家在配置阶段重新按 o 确认（重新收集）。
            // （不再对 host 侧 client build_done 做 reset：client 会在其进入配置、按 o 后再次上报 build_done=true，
            //  避免“host 进配置晚于 client 已配完、reset 把已上报的 build_done 清掉导致 host 永远等不到”。）
            self.steam_build_done = false;
            self.net_cfg = NetCfgSync::Idle;
        }
        // host：节流刷新成员名单（client 加入后 host 界面才能显示新成员）。
        self.steam_roster_refresh_ticks = self.steam_roster_refresh_ticks.wrapping_add(1);
        if self.steam_roster_refresh_ticks % 30 == 1 {
            self.steam_refresh_roster();
        }
        // 节流刷新 ping 与头像（房间界面显示延迟/头像）。
        self.steam_refresh_network_info(ctx);
        // 排行榜句柄：待在房间时就查好（异步回调），整场结束要用。
        self.steam_ensure_leaderboard();
        // Rich Presence：房间里 → “房间「名」n/m 等待中” + connect 串（好友可一键加入）。
        self.steam_refresh_presence(ctx.time.time_since_start().as_secs_f64());
        self.accumulator = 0.0;
        Ok(())
    }

    /// 大厅子菜单（创建/加入/返回）执行选中项。0=创建 1=加入 2=返回。
    #[cfg(feature = "steam")]
    fn steam_lobby_act(&mut self, sel: usize) {
        match sel {
            0 => {
                eprintln!("[menu] Steam -> create-lobby setup");
                // 进入建房设置：重置字段；默认房间名用「昵称的房间」（若昵称已知）。
                let disp = self.steam_my_display_name.clone();
                self.steam_create_name = if disp.is_empty() {
                    "我的房间".to_string()
                } else {
                    format!("{disp}的房间")
                };
                self.steam_create_note = String::new();
                self.steam_create_focus = 0;
                self.steam_create_players = STEAM_DEFAULT_PLAYERS;
                self.steam_create_rounds = STEAM_DEFAULT_ROUNDS;
                self.steam_create_players_buf = STEAM_DEFAULT_PLAYERS.to_string();
                self.steam_create_rounds_buf = STEAM_DEFAULT_ROUNDS.to_string();
                self.steam_create_learn = STEAM_DEFAULT_LEARN_SECS;
                self.steam_create_learn_buf = STEAM_DEFAULT_LEARN_SECS.to_string();
                self.steam_create_starting_gold = STEAM_DEFAULT_STARTING_GOLD;
                self.steam_create_starting_gold_buf = STEAM_DEFAULT_STARTING_GOLD.to_string();
                self.steam_create_gold_per_round = STEAM_DEFAULT_GOLD_PER_ROUND;
                self.steam_create_gold_per_round_buf = STEAM_DEFAULT_GOLD_PER_ROUND.to_string();
                self.steam_create_place_buf = STEAM_DEFAULT_PLACE_REWARD.to_string();
                self.steam_create_place = auto_place_rewards(STEAM_DEFAULT_PLACE_FIRST);
                self.steam_lobby_create = true;
            }
            1 => {
                eprintln!("[menu] Steam -> join lobby list");
                self.steam_lobby_list = true;
                self.steam_list_requested = false;
                self.steam_list_lobbies = Vec::new();
                self.steam_list_selection = 0;
            }
            2 => {
                self.steam_lobby_menu = false;
            }
            _ => {}
        }
    }

    /// 建房设置界面输入：四个字段（房间名/备注/人数）。
    /// - ↑/↓ 或 Tab 切换字段；在文本字段可输入 ascii+空格+常用标点、Backspace 删末字符；人数字段 `+`/`-` 或直接输数字（2..=STEAM_MAX_PLAYERS）。
    /// - 回车=创建房间（用现有 steam_sess 建厅+写房间元数据）；Q=放弃返回大厅主界面。
    #[cfg(feature = "steam")]
    fn steam_lobby_create_update(&mut self, ctx: &mut Context) {
        use ggez::input::keyboard::Key;
        use winit::keyboard::NamedKey;
        let just = |k: char| ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.to_string().into()));
        let just_named = |n: NamedKey| ctx.keyboard.is_logical_key_just_pressed(&Key::Named(n));
        let parse_num = |s: &str, fallback: u32| s.parse::<u32>().unwrap_or(fallback);
        let parse_i32 = |s: &str, fallback: i32| s.trim().parse::<i32>().unwrap_or(fallback);
        // 字段编号与两列布局：左列=0..3（房名/备注/人数/轮数），右列=4..7（准备/初始金币/每轮金币/名次奖励）。
        // 二维方向键导航：↑↓ 同列上下移动，←→ 左右换列，Tab=↑（回退一格）。
        const NUM_COLS: usize = 2;
        const ROWS_PER_COL: usize = 4;
        let cur_col = self.steam_create_focus / ROWS_PER_COL;
        let cur_row = self.steam_create_focus % ROWS_PER_COL;
        let (nc, nr) = if just_named(NamedKey::ArrowUp) || just_named(NamedKey::Tab) {
            (cur_col, (cur_row + ROWS_PER_COL - 1) % ROWS_PER_COL)
        } else if just_named(NamedKey::ArrowDown) {
            (cur_col, (cur_row + 1) % ROWS_PER_COL)
        } else if just_named(NamedKey::ArrowLeft) {
            ((cur_col + NUM_COLS - 1) % NUM_COLS, cur_row)
        } else if just_named(NamedKey::ArrowRight) {
            ((cur_col + 1) % NUM_COLS, cur_row)
        } else {
            (cur_col, cur_row)
        };
        self.steam_create_focus = nc * ROWS_PER_COL + nr;
        if just('q') || just('Q') {
            self.steam_lobby_create = false; // 返回大厅主界面
            return;
        }
        match self.steam_create_focus {
            0 | 1 => {
                // 文本字段：房名 / 备注。
                if just_named(NamedKey::Backspace) {
                    let buf = if self.steam_create_focus == 0 { &mut self.steam_create_name } else { &mut self.steam_create_note };
                    buf.pop();
                    return;
                }
                let buf = if self.steam_create_focus == 0 { &mut self.steam_create_name } else { &mut self.steam_create_note };
                if buf.len() >= 80 {
                    return;
                }
                // 可打印 ascii 字符（字母大小写/数字/空格/常用标点）。
                // 本帧已由 IME 提交文本时不走 ASCII 白名单，避免同一物理键重复插入（C8）。
                if !ime_commit_suppresses_ascii(self.frame, self.last_ime_commit_frame) {
                    const CHARS: &str = " abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.(),;:!?'\"-_#@%&*+=/";
                    for c in CHARS.chars() {
                        if just(c) {
                            buf.push(c);
                            return;
                        }
                    }
                }
            }
            2 => {
                // 人数字段：数字输入 + Backspace 逐位删 + +/- 步进（2..=STEAM_MAX_PLAYERS）。
                if just('+') {
                    let v = parse_num(&self.steam_create_players_buf, STEAM_DEFAULT_PLAYERS as u32);
                    self.steam_create_players_buf = (v as i64 + 1).clamp(2, STEAM_MAX_PLAYERS as i64).to_string();
                } else if just('-') {
                    let v = parse_num(&self.steam_create_players_buf, STEAM_DEFAULT_PLAYERS as u32);
                    self.steam_create_players_buf = (v as i64 - 1).max(2).to_string();
                } else if just_named(NamedKey::Backspace) {
                    self.steam_create_players_buf.pop();
                } else {
                    for d in '0'..='9' {
                        if just(d) && self.steam_create_players_buf.len() < 3 {
                            self.steam_create_players_buf.push(d);
                        }
                    }
                }
            }
            3 => {
                // 轮数字段：数字输入 + Backspace 逐位删 + +/- 步进（1..=STEAM_MAX_ROUNDS）。
                if just('+') {
                    let v = parse_num(&self.steam_create_rounds_buf, STEAM_DEFAULT_ROUNDS);
                    self.steam_create_rounds_buf = (v as i64 + 1).clamp(1, STEAM_MAX_ROUNDS as i64).to_string();
                } else if just('-') {
                    let v = parse_num(&self.steam_create_rounds_buf, STEAM_DEFAULT_ROUNDS);
                    self.steam_create_rounds_buf = (v as i64 - 1).max(1).to_string();
                } else if just_named(NamedKey::Backspace) {
                    self.steam_create_rounds_buf.pop();
                } else {
                    for d in '0'..='9' {
                        if just(d) && self.steam_create_rounds_buf.len() < 4 {
                            self.steam_create_rounds_buf.push(d);
                        }
                    }
                }
            }
            4 => {
                // 局间准备时间字段：数字输入 + Backspace 逐位删 + +/- 步进（STEAM_MIN_LEARN_SECS..=STEAM_MAX_LEARN_SECS）。
                if just('+') {
                    let v = parse_num(&self.steam_create_learn_buf, STEAM_DEFAULT_LEARN_SECS);
                    self.steam_create_learn_buf = (v as i64 + 1).clamp(STEAM_MIN_LEARN_SECS as i64, STEAM_MAX_LEARN_SECS as i64).to_string();
                } else if just('-') {
                    let v = parse_num(&self.steam_create_learn_buf, STEAM_DEFAULT_LEARN_SECS);
                    self.steam_create_learn_buf = (v as i64 - 1).max(STEAM_MIN_LEARN_SECS as i64).to_string();
                } else if just_named(NamedKey::Backspace) {
                    self.steam_create_learn_buf.pop();
                } else {
                    for d in '0'..='9' {
                        if just(d) && self.steam_create_learn_buf.len() < 4 {
                            self.steam_create_learn_buf.push(d);
                        }
                    }
                }
            }
            5 | 6 => {
                // 金币数字字段：初始金币(5) / 每轮金币(6)。0..=STEAM_MAX_GOLD。
                let target = if self.steam_create_focus == 5 { "start" } else { "round" };
                let clamp_gold = |v: i32| v.clamp(0, STEAM_MAX_GOLD);
                let set = |buf: &mut String, v: i32| { *buf = clamp_gold(v).to_string(); };
                if just('+') {
                    if target == "start" {
                        let v = parse_i32(&self.steam_create_starting_gold_buf, STEAM_DEFAULT_STARTING_GOLD);
                        set(&mut self.steam_create_starting_gold_buf, v + 10);
                    } else {
                        let v = parse_i32(&self.steam_create_gold_per_round_buf, STEAM_DEFAULT_GOLD_PER_ROUND);
                        set(&mut self.steam_create_gold_per_round_buf, v + 10);
                    }
                } else if just('-') {
                    if target == "start" {
                        let v = parse_i32(&self.steam_create_starting_gold_buf, STEAM_DEFAULT_STARTING_GOLD);
                        set(&mut self.steam_create_starting_gold_buf, v - 10);
                    } else {
                        let v = parse_i32(&self.steam_create_gold_per_round_buf, STEAM_DEFAULT_GOLD_PER_ROUND);
                        set(&mut self.steam_create_gold_per_round_buf, v - 10);
                    }
                } else if just_named(NamedKey::Backspace) {
                    let buf = if target == "start" { &mut self.steam_create_starting_gold_buf } else { &mut self.steam_create_gold_per_round_buf };
                    buf.pop();
                } else {
                    let buf = if target == "start" { &mut self.steam_create_starting_gold_buf } else { &mut self.steam_create_gold_per_round_buf };
                    if buf.len() < 5 {
                        for d in '0'..='9' {
                            if just(d) {
                                buf.push(d);
                                break;
                            }
                        }
                    }
                }
            }
            7 => {
                // 名次奖励字段：输单个数字（第一名，自动递减）或逗号分隔手动档位（如 30,20,10）。数字 + 逗号输入。
                if just_named(NamedKey::Backspace) {
                    self.steam_create_place_buf.pop();
                    return;
                }
                if self.steam_create_place_buf.len() < 64 {
                    for c in [',', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9'] {
                        if just(c) {
                            self.steam_create_place_buf.push(c);
                            return;
                        }
                    }
                }
            }
            _ => {}
        }
        // 回车=创建房间（从编辑缓冲解析出最终值；空缓冲回退默认）。
        if just_named(NamedKey::Enter) || just('\r') {
            let players = parse_num(&self.steam_create_players_buf, STEAM_DEFAULT_PLAYERS as u32)
                .clamp(2, STEAM_MAX_PLAYERS as u32) as u8;
            let rounds = parse_num(&self.steam_create_rounds_buf, STEAM_DEFAULT_ROUNDS).clamp(1, STEAM_MAX_ROUNDS);
            let learn = parse_num(&self.steam_create_learn_buf, STEAM_DEFAULT_LEARN_SECS)
                .clamp(STEAM_MIN_LEARN_SECS, STEAM_MAX_LEARN_SECS);
            let starting_gold = parse_i32(&self.steam_create_starting_gold_buf, STEAM_DEFAULT_STARTING_GOLD)
                .clamp(0, STEAM_MAX_GOLD);
            let gold_per_round = parse_i32(&self.steam_create_gold_per_round_buf, STEAM_DEFAULT_GOLD_PER_ROUND)
                .clamp(0, STEAM_MAX_GOLD);
            // 名次奖励：单个数字=第一名，自动按 0.6 递减生成全部档位；逗号分隔=手动精确档位。
            let place: Vec<i32> = {
                let raw = self.steam_create_place_buf.trim();
                if raw.is_empty() {
                    auto_place_rewards(STEAM_DEFAULT_PLACE_FIRST)
                } else if raw.contains(',') {
                    let out: Vec<i32> = raw
                        .split(',')
                        .filter_map(|s| s.trim().parse::<i32>().ok())
                        .map(|v| v.clamp(0, STEAM_MAX_GOLD))
                        .take(64)
                        .collect();
                    if out.is_empty() {
                        auto_place_rewards(STEAM_DEFAULT_PLACE_FIRST)
                    } else {
                        out
                    }
                } else {
                    let first = parse_i32(raw, STEAM_DEFAULT_PLACE_FIRST).clamp(0, STEAM_MAX_GOLD);
                    auto_place_rewards(first)
                }
            };
            self.steam_create_players = players;
            self.steam_create_rounds = rounds;
            self.steam_create_learn = learn;
            self.steam_create_starting_gold = starting_gold;
            self.steam_create_gold_per_round = gold_per_round;
            self.steam_create_place = place;
            let name = self.steam_create_name.clone();
            let note = self.steam_create_note.clone();
            eprintln!("[steam] create lobby: players={players} rounds={rounds} learn={learn}s starting_gold={starting_gold} gold_per_round={gold_per_round} place={:?} name='{name}' note='{note}'", self.steam_create_place);
            self.steam_lobby_create = false;
            self.steam_lobby_menu = true;
            self.steam_list_requested = false;
            self.steam_list_lobbies = Vec::new();
            self.steam_list_selection = 0;
            self.enter_steam_mode(ctx, true, players, Some(&name), Some(&note));
        }
    }

    /// 房间列表界面输入：首次进入拉一次公开大厅列表，供浏览选房加入。
    /// 列表拉取是帧驱动异步（S12）：`start_list_lobbies` 注册回调后立即返回，每帧 `tick_lobby_list` 推进后落地。
    /// - ↑/↓ 选择；回车=加入选中的大厅；R=重新刷新；Q=返回大厅主界面。
    #[cfg(feature = "steam")]
    fn steam_lobby_list_update(&mut self, ctx: &mut Context) {
        use ggez::input::keyboard::Key;
        use winit::keyboard::NamedKey;
        let just = |k: char| ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.to_string().into()));
        let just_named = |n: NamedKey| ctx.keyboard.is_logical_key_just_pressed(&Key::Named(n));
        // 刷新节流：首次进入刷新一次；按 R 刷新需距上次 ≥ LOBBY_REFRESH_COOLDOWN_SECS（Steam 搜索接口限速，避免列表漂忽 1->0->1）。
        let first = !self.steam_list_requested;
        let now = ctx.time.time_since_start().as_secs_f64();
        let want_refresh = first || just('r') || just('R');
        if want_refresh && (first || now - self.steam_list_last_refresh >= LOBBY_REFRESH_COOLDOWN_SECS) {
            self.steam_list_requested = true;
            self.steam_list_last_refresh = now;
            // 发起异步列表拉取（S12：不 sleep，注册回调后返回；结果由下面 tick 推进）。
            if let Some(sess) = self.steam_sess.as_mut() {
                if let Err(e) = sess.start_list_lobbies(120) {
                    eprintln!("[steam-list] start list failed: {e:?}");
                }
            }
        } else if want_refresh {
            eprintln!("[steam-list] 刷新太快（Steam 搜索限速），请几秒后再按 R");
        }
        // 每帧推进异步列表拉取（无论是否刚刷新都要推进，直到 Done）。
        if let Some(sess) = self.steam_sess.as_mut() {
            match sess.tick_lobby_list() {
                net_steam::session::LobbyListProgress::Done(Ok(mut list)) => {
                    // 人数已满的大厅仍显示但不可选（steamworks 加入会失败）；这里仅排序展示。
                    list.sort_by_key(|l| (l.members >= l.limit, l.members));
                    self.steam_list_lobbies = list;
                    if self.steam_list_selection >= self.steam_list_lobbies.len() {
                        self.steam_list_selection = self.steam_list_lobbies.len().saturating_sub(1);
                    }
                    eprintln!("[steam-list] {} lobbies found", self.steam_list_lobbies.len());
                }
                net_steam::session::LobbyListProgress::Done(Err(e)) => {
                    eprintln!("[steam-list] list failed: {e:?}");
                    self.steam_list_lobbies = Vec::new();
                }
                net_steam::session::LobbyListProgress::Pending | net_steam::session::LobbyListProgress::Idle => {}
            }
        }
        if just('q') || just('Q') {
            self.steam_lobby_list = false;
            return;
        }
        if self.steam_list_lobbies.is_empty() {
            return; // 没有可加入房间（或加载中），等待/提示。
        }
        let n = self.steam_list_lobbies.len();
        if just_named(NamedKey::ArrowDown) {
            self.steam_list_selection = (self.steam_list_selection + 1) % n;
        } else if just_named(NamedKey::ArrowUp) {
            self.steam_list_selection = (self.steam_list_selection + n - 1) % n;
        }
        if just_named(NamedKey::Enter) || just('\r') {
            let sel = self.steam_list_selection;
            if sel < self.steam_list_lobbies.len() && self.steam_list_lobbies[sel].members < self.steam_list_lobbies[sel].limit {
                let lobby_id = self.steam_list_lobbies[sel].id;
                eprintln!("[steam] join lobby by id {lobby_id}");
                self.steam_lobby_list = false;
                self.steam_lobby_menu = true;
                self.steam_join_lobby_id = Some(lobby_id); // enter_steam_mode client 分支优先按其加入
                self.enter_steam_mode(ctx, false, 2, None, None);
            } else {
                eprintln!("[steam-list] 选中的房间已满或无效");
            }
        }
    }

    /// 从主菜单进入 Steam 大厅模式（S12 异步）：只发起建厅/加入（`start_*`），
    /// 真正「进房」由 `update` 每帧 `run_callbacks` 后 `tick_lobby` 完成、再调 `finish_enter_steam_mode` 落地
    /// （建 lockstep / 世界 / 战绩）。`is_host`=创建大厅，否则加入；`players` 仅 host 用；
    /// `room_name`/`room_note` 现为兼容保留（落地时改读 `self.steam_create_*`）。
    #[cfg(feature = "steam")]
    fn enter_steam_mode(&mut self, _ctx: &mut Context, is_host: bool, players: u8, _room_name: Option<&str>, _room_note: Option<&str>) {
        let kind = if is_host {
            SteamLobbyPending::Host { players }
        } else {
            // client：若从房间列表选中了具体大厅，优先按其加入（取走 steam_join_lobby_id）。
            SteamLobbyPending::Join { lobby_id: self.steam_join_lobby_id.take() }
        };
        if !self.steam_begin_lobby(kind) {
            // 会话缺失/发起失败 → 退回大厅主界面，等待用户重试。
            eprintln!("[steam-menu] enter_steam_mode: 无法发起大厅操作（会话缺失？）");
            self.steam_lobby_menu = true;
            self.steam_lobby_create = false;
            self.steam_lobby_list = false;
        }
    }

    /// 发起一个 Steam 大厅操作（S12 异步）：`start_*` 注册 steamworks 回调后立即返回，
    /// 后续由 `update` 每帧 `tick_lobby` 推进（必须在 `run_callbacks` 之后）。`steam_sess` 须已初始化；成功返回 true。
    #[cfg(feature = "steam")]
    fn steam_begin_lobby(&mut self, kind: SteamLobbyPending) -> bool {
        let Some(mut sess) = self.steam_sess.take() else {
            return false;
        };
        let r = match &kind {
            SteamLobbyPending::Host { players } => {
                sess.start_host_create((*players).max(1) as u32, STEAM_LOBBY_CREATE_BEATS)
            }
            SteamLobbyPending::Join { lobby_id } => match lobby_id {
                // 指定大厅 id 直接加入：单阶段，4s 超时足够。
                Some(id) => sess.start_join_by_id(*id, 240),
                // 按 matchkey 搜索+加入：两阶段共用 beats，给足 8s（原同步实现每阶段各 240 拍）。
                None => sess.start_find_and_join(480),
            },
        };
        match r {
            Ok(()) => {
                self.steam_sess = Some(sess);
                self.steam_lobby_pending = Some(kind);
                true
            }
            Err(e) => {
                eprintln!("[steam] begin lobby failed: {e:?}");
                self.steam_sess = Some(sess);
                false
            }
        }
    }

    /// 每帧推进进行中的大厅操作（S12）：须先 `run_callbacks` 泵出 steamworks 回调，再 `tick_lobby`。
    /// 完成（返回 `Done`）即落地进房或退回菜单。在 `update` 顶部、`steam_in_lobby` 检查之前调用。
    #[cfg(feature = "steam")]
    fn steam_poll_lobby_pending(&mut self, ctx: &mut Context) {
        if let Some(sess) = self.steam_sess.as_ref() {
            sess.run_callbacks();
        }
        let prog = match self.steam_sess.as_mut() {
            Some(sess) => sess.tick_lobby(),
            None => net_steam::session::LobbyProgress::Idle,
        };
        match prog {
            net_steam::session::LobbyProgress::Done(Ok(lobby)) => {
                self.finish_enter_steam_mode(ctx, lobby);
            }
            net_steam::session::LobbyProgress::Done(Err(e)) => {
                eprintln!("[steam] lobby op failed: {e:?}");
                self.steam_lobby_pending = None;
                self.steam_lobby_menu = true;
                self.steam_lobby_create = false;
                self.steam_lobby_list = false;
                self.steam_in_lobby = false;
            }
            net_steam::session::LobbyProgress::Pending | net_steam::session::LobbyProgress::Idle => {}
        }
    }

    /// 大厅操作完成后的落地（S12）：建 lockstep / 世界 / 战绩，并停在「房间/就绪界面」。
    /// `lobby` 已由 `tick_lobby` 的 `finalize_lobby` 写入 `sess.lobby` / `sess.table`。
    #[cfg(feature = "steam")]
    fn finish_enter_steam_mode(&mut self, _ctx: &mut Context, lobby: net_steam::steamworks::LobbyId) {
        let Some(kind) = self.steam_lobby_pending.take() else {
            return;
        };
        let seed = 20260812u64;
        let res = (|| -> std::io::Result<()> {
            let mut sess = self
                .steam_sess
                .take()
                .ok_or_else(|| std::io::Error::other("steam 会话丢失"))?;
            self.steam_lobby_id = Some(lobby.raw());
            let n: usize;
            match kind {
                SteamLobbyPending::Host { players } => {
                    sess.host_set_room_info(Some(self.steam_create_name.as_str()), Some(self.steam_create_note.as_str()))?;
                    sess.host_set_rounds(self.steam_create_rounds)?;
                    sess.host_set_learn(self.steam_create_learn)?;
                    sess.host_set_starting_gold(self.steam_create_starting_gold)?;
                    sess.host_set_gold_per_round(self.steam_create_gold_per_round)?;
                    sess.host_set_place_reward(&self.steam_create_place)?;
                    self.match_rounds = self.steam_create_rounds;
                    self.match_learn_secs = self.steam_create_learn;
                    self.match_starting_gold = self.steam_create_starting_gold;
                    self.match_gold_per_round = self.steam_create_gold_per_round;
                    self.match_place_rewards = self.steam_create_place.clone();
                    sess.prepare_transport()?;
                    self.steam_my_index = sess.my_slot();
                    self.steam_my_id = sess.transport.steam_id();
                    eprintln!("[steam-host] lobby={:?}, my slot={}", lobby.raw(), sess.my_slot());
                    let fr = sess.transport.friends();
                    let mut roster = Vec::new();
                    for (slot, id) in sess.identities() {
                        let name = fr.get_friend(net_steam::steamworks::SteamId::from_raw(id)).name();
                        roster.push((slot, name, id));
                    }
                    self.steam_roster = roster;
                    n = players.max(1) as usize;
                    let ids: Vec<Option<u64>> = sess.identities().iter().skip(1).map(|(_, v)| Some(*v)).collect();
                    let transport = sess.into_transport();
                    let mut host_ls = net::lockstep::HostLockstep::new(transport, n, true);
                    host_ls.set_client_identities(&ids);
                    self.steam_host_ls = Some(host_ls);
                    self.steam_cli_ls = None;
                    self.app = AppState::SteamHost { players };
                }
                SteamLobbyPending::Join { lobby_id: _ } => {
                    sess.prepare_transport()?;
                    self.steam_my_id = sess.transport.steam_id();
                    let total = sess.table.as_ref().map(|t| t.total_players()).unwrap_or(2);
                    self.match_rounds = sess.lobby_rounds().unwrap_or(STEAM_DEFAULT_ROUNDS);
                    self.match_learn_secs = sess.lobby_learn().unwrap_or(STEAM_DEFAULT_LEARN_SECS);
                    self.match_starting_gold = sess.lobby_starting_gold().unwrap_or(STEAM_DEFAULT_STARTING_GOLD);
                    self.match_gold_per_round = sess.lobby_gold_per_round().unwrap_or(STEAM_DEFAULT_GOLD_PER_ROUND);
                    self.match_place_rewards = sess.lobby_place_reward().unwrap_or_else(|| auto_place_rewards(STEAM_DEFAULT_PLACE_FIRST));
                    let host_id = sess.host_steam_id().unwrap_or(0);
                    let my_slot = sess.my_slot();
                    self.steam_my_index = my_slot;
                    let fr = sess.transport.friends();
                    let mut roster = Vec::new();
                    for (slot, id) in sess.identities() {
                        let name = fr.get_friend(net_steam::steamworks::SteamId::from_raw(id)).name();
                        roster.push((slot, name, id));
                    }
                    self.steam_roster = roster;
                    let transport = sess.into_transport();
                    self.steam_cli_ls = Some(net::lockstep::ClientLockstep::new(
                        transport,
                        my_slot,
                        net::transport::Peer::Steam { id: host_id, conn: None },
                    ));
                    self.steam_host_ls = None;
                    self.app = AppState::SteamJoin { lobby_id: None };
                    n = total.max(2);
                }
            }
            self.world = game_core::world::World::new(n.max(1) as u32, seed);
            self.meta = game_core::meta::MatchState::new(
                self.match_config(),
                &(0..n.max(1)).map(|i| i as u32).collect::<Vec<u32>>(),
                8,
            );
            Ok(())
        })();
        if let Err(e) = res {
            eprintln!("[steam-menu] failed to enter steam mode: {e:?}");
            self.steam_lobby_menu = true;
            self.steam_lobby_create = false;
            self.steam_lobby_list = false;
            self.steam_in_lobby = false;
            self.steam_lobby_pending = None;
            return;
        }
        // 进入房间/就绪界面（无需再手动输入房间号）。
        self.steam_lobby_menu = false;
        self.steam_in_lobby = true;
        self.steam_active = true;
        self.steam_local_ready = false;
        self.steam_build_done = false;
        self.steam_was_all_ready = false;
        self.steam_manual_start_pending = false;
        self.steam_manual_countdown = false;
        self.steam_manual_ms = 0;
        self.steam_roster_ready = Vec::new();
        self.steam_roster_all_ready = false;
        self.steam_all_ready = false;
        self.steam_countdown = 0.0;
        self.pre_game_config = false; // 由房间就绪 → StartConfig → true
        self.net_cfg = NetCfgSync::Idle;
        self.net_link = None;
        self.net_host = None;
        self.net_host_ls = None;
        self.net_ready = false;
        self.conn_dropped = false;
        self.reconnect_attempting = false;
        self.host_frame_count = 0;
        self.steam_cli_stale_ticks = 0;
        self.steam_participants = Vec::new();
        self.steam_online = Vec::new();
        self.steam_migrating = false;
        self.steam_migrate_ticks = 0;
        self.steam_new_host_id = 0;
        self.steam_host_broadcasting_takeover = false;
        // 邀请面板/好友列表状态复位；进房后立刻上报一次 Rich Presence（好友可一键加入）。
        self.steam_friend_list = false;
        self.steam_friends = Vec::new();
        self.steam_friend_selection = 0;
        self.steam_friend_hint = String::new();
        self.steam_presence_text = String::new(); // 强制立即写（不节流）
        self.steam_presence_last = -999.0;
        // 会话已被消费（成功），下次回主菜单允许再初始化一次。
        self.steam_session_tried = false;
        // 新一场：允许重新上报战绩（进新房间的入口）。
        self.steam_stats_recorded = false;
        self.steam_stats_snapshot = None;
        self.steam_toast = (String::new(), 0.0);
        self.accumulator = 0.0;
    }

    /// 开局前配置面板：显示当前绑定/等级/金币，提示按 Space 开始第一轮。
    /// TODO(绘制统一，阶段3)：首局已改走 Learning 界面，本函数待并入 `draw_meta_overlay` 后删除。
    #[allow(dead_code)]
    fn draw_pre_game(&self, ctx: &mut Context) -> GameResult {
        let mut canvas = graphics::Canvas::from_frame(ctx, graphics::Color::from_rgb(18, 20, 26));
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;
        draw_text(&mut canvas, ctx, "开局 - 配置技能", 46.0, graphics::Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.12 }, true)?;
        // Steam：配置阶段为「所有玩家配完统一开始」；否则为局域网/单机的按空格开始。
        #[cfg(feature = "steam")]
        if self.steam_cli_ls.is_some() || self.steam_host_ls.is_some() {
            if self.steam_build_done {
                draw_text(&mut canvas, ctx, "[v] 我已配好，等待所有玩家配完统一开始...", 22.0, graphics::Color::from_rgb(90, 220, 130), Point2 { x: cx, y: sh * 0.12 + 60.0 }, true)?;
            } else {
                draw_text(&mut canvas, ctx, "选择技能后按 P 确认配好", 22.0, graphics::Color::from_rgb(150, 200, 255), Point2 { x: cx, y: sh * 0.12 + 60.0 }, true)?;
            }
        } else {
            draw_text(&mut canvas, ctx, "按 Space/P 开始第一轮，Esc 返回主菜单", 22.0, graphics::Color::from_rgb(150, 200, 255), Point2 { x: cx, y: sh * 0.12 + 60.0 }, true)?;
        }
        #[cfg(not(feature = "steam"))]
        draw_text(&mut canvas, ctx, "按 Space/P 开始第一轮，Esc 返回主菜单", 22.0, graphics::Color::from_rgb(150, 200, 255), Point2 { x: cx, y: sh * 0.12 + 60.0 }, true)?;
        // 准备状态面板：显示各玩家已加入/已就绪，避免“以为卡住”。
        let me = self.self_index();
        if self.app != AppState::Solo {
            let mut r = sh * 0.12 + 96.0;
            draw_text(&mut canvas, ctx, "== 玩家准备状态 ==", 20.0, graphics::Color::from_rgb(200, 210, 220), Point2 { x: cx, y: r }, true)?;
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
                        (format!("  {name}  [v] 已就绪"), Color::from_rgb(90, 220, 130))
                    } else if (i as u32) == me {
                        (format!("  {name}  [ ] 等你按空格"), Color::from_rgb(240, 200, 70))
                    } else {
                        (format!("  {name}  [ ] 等待上报"), Color::from_rgb(170, 175, 185))
                    };
                    draw_text(&mut canvas, ctx, &txt, 18.0, col, Point2 { x: cx, y: r }, true)?;
                    r += 26.0;
                }
            } else if let Some(hs) = self.net_host.as_ref() {
                draw_text(&mut canvas, ctx, &format!("  已加入 {}/{} 个玩家", hs.joined, hs.expected()), 18.0, Color::from_rgb(170, 175, 185), Point2 { x: cx, y: r }, true)?;
                draw_text(&mut canvas, ctx, "  等所有玩家加入后：每个窗口先点击再按空格就绪", 17.0, Color::from_rgb(140, 160, 180), Point2 { x: cx, y: r + 26.0 }, true)?;
            } else {
                // LAN client：显示自身是否已就绪。
                let ready = self.net_cfg == NetCfgSync::ClientWait;
                let (txt, col) = if ready {
                    ("  [v] 已就绪，等待 host 开始...".to_string(), Color::from_rgb(90, 220, 130))
                } else {
                    ("  [ ] 未就绪 - 请先点击本窗口，再按空格就绪".to_string(), Color::from_rgb(240, 200, 70))
                };
                draw_text(&mut canvas, ctx, &txt, 18.0, col, Point2 { x: cx, y: r }, true)?;
            }
            // Steam：显示各端配好（build_done）状态，等待全员配完统一开始。
            #[cfg(feature = "steam")]
            if let Some(host) = self.steam_host_ls.as_ref() {
                let total = self.world.players.len();
                for i in 0..total {
                    let (name, done) = if i == 0 {
                        ("host(你)".to_string(), self.steam_build_done)
                    } else {
                        (format!("玩家{i}"), host.client_build_done(i as u8))
                    };
                    let (txt, col) = if done {
                        (format!("  {name}  [v] 已配好"), Color::from_rgb(90, 220, 130))
                    } else if (i as u32) == self.self_index() {
                        (format!("  {name}  [ ] 选技能后按 P"), Color::from_rgb(240, 200, 70))
                    } else {
                        (format!("  {name}  [ ] 配好中"), Color::from_rgb(170, 175, 185))
                    };
                    draw_text(&mut canvas, ctx, &txt, 18.0, col, Point2 { x: cx, y: r }, true)?;
                    r += 26.0;
                }
            } else if self.steam_cli_ls.is_some() {
                let (txt, col) = if self.steam_build_done {
                    ("  [v] 我已配好，等待全员配完统一开始...".to_string(), Color::from_rgb(90, 220, 130))
                } else {
                    ("  [ ] 选技能后按 P 确认配好".to_string(), Color::from_rgb(240, 200, 70))
                };
                draw_text(&mut canvas, ctx, &txt, 18.0, col, Point2 { x: cx, y: r }, true)?;
            }
        }
        if self.app == AppState::Solo {
            draw_text(&mut canvas, ctx, &format!("（单机：{:.0} 秒后自动用默认配置开始）", self.pre_game_timer.max(0.0)), 17.0, graphics::Color::from_rgb(140, 160, 180), Point2 { x: cx, y: sh * 0.12 + 92.0 }, true)?;
        }
        // 下方分左右两栏：左=技能树与键位绑定，右=成长点与属性购买；底部一条操作提示。
        let lcx = sw * 0.30; // 左栏中心
        let rcx = sw * 0.72; // 右栏中心
        let col_top = sh * 0.22;
        // —— 右栏：成长点 / 属性购买面板。
        if let Some(pr) = self.meta.profiles.iter().find(|p| p.player_id == self.self_index()) {
            let mut gy = col_top;
            draw_text(&mut canvas, ctx, "== 成长 / 属性 ==", 20.0, Color::from_rgb(130, 220, 255), Point2 { x: rcx, y: gy }, true)?;
            gy += 34.0;
            draw_text(&mut canvas, ctx, &format!("成长点 {}    金币 {}", pr.growth_points, pr.gold), 22.0, Color::from_rgb(220, 230, 245), Point2 { x: rcx, y: gy }, true)?;
            gy += 34.0;
            let a = &pr.attributes;
            draw_text(&mut canvas, ctx, &format!("生命 +{}%", a.hp_bonus * 10), 19.0, Color::from_rgb(200, 210, 220), Point2 { x: rcx, y: gy }, true)?;
            gy += 28.0;
            draw_text(&mut canvas, ctx, &format!("移速 +{}%", a.speed_bonus * 5), 19.0, Color::from_rgb(200, 210, 220), Point2 { x: rcx, y: gy }, true)?;
            gy += 28.0;
            draw_text(&mut canvas, ctx, &format!("护甲 -{}%  法抗 -{}%", a.armor * 6, a.spell_resist * 6), 18.0, Color::from_rgb(200, 210, 220), Point2 { x: rcx, y: gy }, true)?;
            gy += 28.0;
            draw_text(&mut canvas, ctx, &format!("击退 -{}%", a.kb_resist * 12), 18.0, Color::from_rgb(200, 210, 220), Point2 { x: rcx, y: gy }, true)?;
            gy += 34.0;
            draw_text(&mut canvas, ctx, "购买：Z 金币换点", 17.0, Color::from_rgb(160, 180, 200), Point2 { x: rcx, y: gy }, true)?;
            gy += 26.0;
            draw_text(&mut canvas, ctx, "H生命 J移速 K护甲", 17.0, Color::from_rgb(160, 180, 200), Point2 { x: rcx, y: gy }, true)?;
            gy += 26.0;
            draw_text(&mut canvas, ctx, "L法抗 ;击退", 17.0, Color::from_rgb(160, 180, 200), Point2 { x: rcx, y: gy }, true)?;
        }
        // —— 左栏：技能树与键位绑定。
        let me = self.self_index();
        if let Some(pr) = self.meta.profiles.iter().find(|p| p.player_id == me) {
            let mut y = col_top;
            draw_text(&mut canvas, ctx, "== 技能 配置 ==", 20.0, Color::from_rgb(255, 210, 120), Point2 { x: lcx, y }, true)?;
            y += 34.0;
            let gold_line = format!("金币：{}    击杀：{}    最佳名次：#{}", pr.gold, pr.total_kills, pr.best_placement);
            draw_text(&mut canvas, ctx, &gold_line, 20.0, Color::from_rgb(220, 224, 232), Point2 { x: lcx, y }, true)?;
            y += 34.0;
            // 当前选中树：高亮字样，提醒按了字母 C/R/E... 已选中哪棵/可选技能。
            if let Some(sel) = self.learn_tree_key {
                let sel_line = format!("[{}] {} 树（当前选中）", sel.letter(), sel.tree().name_zh());
                draw_text(&mut canvas, ctx, &sel_line, 22.0, Color::from_rgb(255, 210, 120), Point2 { x: lcx, y }, true)?;
                y += 34.0;
                for (i, skill) in sel.tree().skills_in_tree().iter().enumerate() {
                    let star = if pr.bound_skill(sel) == Some(*skill) { "  [已选]" } else { "" };
                    draw_text(&mut canvas, ctx, &format!("  {} {} {}", i + 1, game_core::skill::DefTable::def(*skill).name, star), 19.0, Color::from_rgb(215, 220, 230), Point2 { x: lcx, y }, true)?;
                    y += 28.0;
                }
                y += 10.0;
            } else {
                draw_text(&mut canvas, ctx, "（按字母 C/R/E/D/Y/T/F/G 选树）", 18.0, Color::from_rgb(170, 175, 185), Point2 { x: lcx, y }, true)?;
                y += 32.0;
            }
            draw_text(&mut canvas, ctx, "各键当前绑定：", 19.0, Color::from_rgb(225, 228, 235), Point2 { x: lcx, y }, true)?;
            y += 30.0;
            for key in game_core::skill::CastKey::ALL {
                let bound = pr.bound_skill(key);
                let lv = bound.map(|s| pr.skill_level(s)).unwrap_or(0);
                let txt = match bound {
                    Some(s) => format!("[{}] {}  @Lv{}", key.letter(), game_core::skill::DefTable::def(s).name, lv),
                    None => format!("[{}] （未绑定）", key.letter()),
                };
                let highlight = self.learn_tree_key == Some(key);
                draw_text(&mut canvas, ctx, &txt, 20.0, if highlight { Color::from_rgb(255, 210, 120) } else { Color::from_rgb(225, 228, 235) }, Point2 { x: lcx, y }, true)?;
                y += 28.0;
            }
        }
        draw_text(&mut canvas, ctx, "字母C/R/E/D/Y/T/F/G选树  数字绑技能  =升级  X洗点", 18.0, graphics::Color::from_rgb(160, 170, 185), Point2 { x: cx, y: sh * 0.92 }, true)?;
        canvas.finish(ctx)?;
        Ok(())
    }


    /// 主菜单：标题 + 三个入口（单机试验场 / 局域网 / Steam 大厅）；按 3 进入 Steam 大厅选择子菜单。
    fn draw_menu(&self, ctx: &mut Context) -> GameResult {
        let mut canvas = graphics::Canvas::from_frame(ctx, graphics::Color::from_rgb(18, 20, 26));
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;

        // 标题区
        let title = "帧同步圆球竞技场";
        draw_text(&mut canvas, ctx, title, 54.0, graphics::Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.14 }, true)?;
        draw_text(&mut canvas, ctx, "—— 选择对战模式 ——", 22.0, graphics::Color::from_rgb(200, 205, 215), Point2 { x: cx, y: sh * 0.14 + 64.0 }, true)?;

        // 卡片通用尺寸
        let card_w = (sw * 0.62).min(560.0);
        let card_h = 96.0;
        let card_x = cx - card_w / 2.0;
        let y0 = sh * 0.34;
        let gap = 26.0;

        #[cfg(feature = "steam")]
        let in_lobby_menu = self.steam_lobby_menu || self.steam_lobby_create || self.steam_lobby_list;
        #[cfg(not(feature = "steam"))]
        let in_lobby_menu = false;

        // Steam 大厅：建房设置 / 房间列表 / 大厅主界面三种子界面。
        if in_lobby_menu {
            // 建房设置界面。
            #[cfg(feature = "steam")]
            if self.steam_lobby_create {
                self.draw_steam_create_lobby(&mut canvas, ctx)?;
                canvas.finish(ctx)?;
                return Ok(());
            }
            // 房间列表界面。
            #[cfg(feature = "steam")]
            if self.steam_lobby_list {
                self.draw_steam_lobby_list(&mut canvas, ctx)?;
                canvas.finish(ctx)?;
                return Ok(());
            }
            // 大厅主界面：创建 / 加入 / 返回，同样卡片样式。
            #[cfg(feature = "steam")]
            {
                let subs: [(&str, &str); 3] = [
                    ("创建房间", "选房间名与玩家人数，然后进入房间"),
                    ("加入房间", "从房间列表选择并加入（S2 接入房间列表）"),
                    ("返回主菜单", "回到主菜单选择"),
                ];
                draw_text(&mut canvas, ctx, "Steam 对战 - 大厅", 34.0, graphics::Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.27 }, true)?;
                draw_text(&mut canvas, ctx, "H 创建    J 加入    Q 返回", 20.0, graphics::Color::from_rgb(200, 205, 215), Point2 { x: cx, y: sh * 0.36 }, true)?;
                let mpos = ctx.mouse.position();
                for (i, (name, desc)) in subs.iter().enumerate() {
                    let y = y0 + (i as f32) * (card_h + gap);
                    // 高亮：键盘选中最亮；鼠标悬停中亮；其他深灰。
                    let selected = i == self.steam_lobby_selection;
                    let hover = !selected && graphics::Rect::new(card_x, y, card_w, card_h).contains(mpos);
                    let bg_color = if selected {
                        Color::from_rgb(52, 60, 74)
                    } else if hover {
                        Color::from_rgb(40, 46, 58)
                    } else {
                        Color::from_rgb(30, 34, 44)
                    };
                    // 卡片背景
                    let bg = Mesh::new_rectangle(
                        &ctx.gfx, DrawMode::fill(),
                        graphics::Rect::new(card_x, y, card_w, card_h),
                        bg_color,
                    )?;
                    canvas.draw(&bg, graphics::DrawParam::new());
                    draw_text(&mut canvas, ctx, name, 30.0, graphics::Color::from_rgb(235, 238, 245), Point2 { x: cx, y: y + card_h * 0.5 - 16.0 }, true)?;
                    draw_text(&mut canvas, ctx, desc, 17.0, graphics::Color::from_rgb(150, 155, 168), Point2 { x: cx, y: y + card_h * 0.5 + 18.0 }, true)?;
                }
            }
            #[cfg(not(feature = "steam"))]
            {
                draw_text(&mut canvas, ctx, "Steam 未启用", 34.0, graphics::Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.36 }, true)?;
                draw_text(&mut canvas, ctx, "需要 --features client/steam 构建", 20.0, Color::from_rgb(200, 205, 215), Point2 { x: cx, y: sh * 0.44 }, true)?;
            }
            draw_text(&mut canvas, ctx, "H 创建    J 加入    Q 返回", 18.0, graphics::Color::from_rgb(160, 168, 182), Point2 { x: cx, y: sh * 0.90 }, true)?;
            canvas.finish(ctx)?;
            return Ok(());
        }

        // 主菜单三个入口卡片
        let items: [(u8, &str, &str); 3] = [
            (1, "单机技能试验场", "无 AI 自由试技能与数值（进入后配置技能开始）"),
            (2, "局域网对战", "建设中：需命令行 --host <port> / --join <host:port>"),
            (3, "Steam 在线对战", "联网与好友实时对抗（进入 Steam 大厅）"),
        ];
        let mpos = ctx.mouse.position();
        for (i, (num, name, desc)) in items.iter().enumerate() {
            let y = y0 + (i as f32) * (card_h + gap);
            let selected = i == self.menu_selection;
            let hover = !selected && graphics::Rect::new(card_x, y, card_w, card_h).contains(mpos);
            // 选中卡片：高亮背景条；悬停：中亮；未选中：深灰背景。
            let bg_color = if selected {
                Color::from_rgb(52, 60, 74)
            } else if hover {
                Color::from_rgb(40, 46, 58)
            } else {
                Color::from_rgb(28, 31, 38)
            };
            let bg = Mesh::new_rectangle(
                &ctx.gfx, DrawMode::fill(),
                graphics::Rect::new(card_x, y, card_w, card_h),
                bg_color,
            )?;
            canvas.draw(&bg, graphics::DrawParam::new());
            let mark = if selected { "[v]" } else { "[ ]" };
            let name_col = if selected { Color::WHITE } else { Color::from_rgb(210, 214, 225) };
            draw_text(&mut canvas, ctx, &format!("[{num}]  {mark}{name}"), 30.0, name_col, Point2 { x: cx, y: y + card_h * 0.5 - 16.0 }, true)?;
            draw_text(&mut canvas, ctx, desc, 17.0, Color::from_rgb(150, 156, 172), Point2 { x: cx, y: y + card_h * 0.5 + 20.0 }, true)?;
        }

        // 底部操作提示条
        draw_text(&mut canvas, ctx, "↑/↓ 选择    回车 确认    或直接按数字键", 18.0, graphics::Color::from_rgb(160, 168, 182), Point2 { x: cx, y: sh * 0.92 }, true)?;
        canvas.finish(ctx)?;
        Ok(())
    }

    /// 绘制「建房设置」界面：房间名 / 备注 / 人数 三字段，当前聚焦字段高亮。
    #[cfg(feature = "steam")]
    fn draw_steam_create_lobby(&self, canvas: &mut Canvas, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;
        draw_text(canvas, ctx, "创建房间", 38.0, Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.12 }, true)?;
        draw_text(canvas, ctx, "↑↓ ←→ 方向键切换字段 · 回车 创建 · Q 返回", 20.0, Color::from_rgb(180, 190, 205), Point2 { x: cx, y: sh * 0.12 + 44.0 }, true)?;

        let labels = [
            "房间名", "备注", "玩家人数", "总轮数",
            "准备时间(秒)", "初始金币", "每轮金币", "名次奖励",
        ];
        let hints = [
            "直接输入文字，Backspace 删除（支持中文输入法）",
            "可留空；直接输入文字",
            "+/− 步进，或直接输数字（2 ~ 64）",
            "+/− 步进，或直接输数字（1 ~ 256）",
            "局与局之间的准备时间（8 ~ 256 秒）",
            "第一局开局一次性发放，独立于每轮金币（0 ~ 99999）",
            "每轮固定参与奖（0 ~ 99999）",
            "输第一名金额（如 30，自动按 0.6 递减到 0）；或用逗号分隔手动档位 30,20,10",
        ];
        let placeholders = [
            "（输入房间名）", "（可留空）", "（默认 2）", "（默认 3）",
            "（默认 20 秒）", "（默认 0）", "（默认 20）", "（默认 30）",
        ];
        let vals = [
            self.steam_create_name.clone(),
            self.steam_create_note.clone(),
            self.steam_create_players_buf.clone(),
            self.steam_create_rounds_buf.clone(),
            self.steam_create_learn_buf.clone(),
            self.steam_create_starting_gold_buf.clone(),
            self.steam_create_gold_per_round_buf.clone(),
            self.steam_create_place_buf.clone(),
        ];
        let box_w = 320.0;
        let box_h = 46.0;
        let label_w = 150.0;
        let col_w = label_w + box_w;
        let gap = 70.0;
        let total_w = col_w * 2.0 + gap;
        let left_col_left = cx - total_w / 2.0;
        let right_col_left = left_col_left + col_w + gap;
        let row_h = 68.0;
        let y0 = sh * 0.26;
        // 字段 → 列/行：左列 0..4（房名/备注/人数/轮数），右列 4..8（准备/初始金币/每轮金币/名次奖励）。
        for i in 0..8 {
            let col = i / 4;
            let row = i % 4;
            let total_left = if col == 0 { left_col_left } else { right_col_left };
            let y = y0 + row as f32 * row_h;
            let selected = i == self.steam_create_focus;
            let bg_col = if selected { Color::from_rgb(56, 66, 84) } else { Color::from_rgb(28, 32, 42) };
            let bg = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), graphics::Rect::new(total_left + label_w, y, box_w, box_h), bg_col)?;
            canvas.draw(&bg, graphics::DrawParam::new());
            if selected {
                let border = Mesh::new_rectangle(&ctx.gfx, DrawMode::stroke(2.0), graphics::Rect::new(total_left + label_w, y, box_w, box_h), Color::from_rgb(255, 210, 120))?;
                canvas.draw(&border, graphics::DrawParam::new());
            }
            let label_col = if selected { Color::from_rgb(255, 210, 120) } else { Color::from_rgb(215, 220, 232) };
            draw_text(canvas, ctx, labels[i], 23.0, label_col, Point2 { x: total_left + label_w / 2.0, y: y + box_h / 2.0 - 14.0 }, true)?;
            let disp = if vals[i].is_empty() { placeholders[i].to_string() } else { vals[i].clone() };
            let val_col = if vals[i].is_empty() { Color::from_rgb(120, 130, 150) } else { Color::WHITE };
            draw_text(canvas, ctx, &disp, 20.0, val_col, Point2 { x: total_left + label_w + box_w / 2.0, y: y + box_h / 2.0 - 13.0 }, true)?;
            // 聚焦字段下方：该字段专属操作提示（左对齐到字段输入框左缘，更贴近）。
            if selected {
                draw_text(canvas, ctx, &format!("▶ {}", hints[i]), 16.0, Color::from_rgb(150, 200, 255), Point2 { x: total_left + label_w, y: y + box_h + 8.0 }, false)?;
            }
        }
        draw_text(canvas, ctx, "↑↓ ←→ 方向键切换字段 · 回车 创建房间 · Q 取消", 20.0, Color::from_rgb(160, 200, 255), Point2 { x: cx, y: sh * 0.90 }, true)?;
        Ok(())
    }

    /// 连接中界面（S12 异步建厅/加入期间）：显示「连接中…」，避免空帧或误进房间界面。
    #[cfg(feature = "steam")]
    fn draw_steam_connecting(&self, ctx: &mut Context) -> GameResult {
        let mut canvas = graphics::Canvas::from_frame(ctx, graphics::Color::from_rgb(18, 20, 26));
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;
        let cy = sh / 2.0;
        draw_text(&mut canvas, ctx, "连接中…", 44.0, graphics::Color::from_rgb(255, 210, 120), Point2 { x: cx, y: cy - 30.0 }, true)?;
        draw_text(&mut canvas, ctx, "正在连接 Steam 大厅，请稍候", 20.0, graphics::Color::from_rgb(180, 190, 205), Point2 { x: cx, y: cy + 24.0 }, true)?;
        canvas.finish(ctx)?;
        Ok(())
    }

    /// 绘制「房间列表」界面：公开大厅列表（房主昵称/房名/人数/备注），当前选中高亮。
    #[cfg(feature = "steam")]
    fn draw_steam_lobby_list(&self, canvas: &mut Canvas, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;
        draw_text(canvas, ctx, "加入房间", 36.0, Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.22 }, true)?;
        draw_text(canvas, ctx, "↑/↓ 选择，回车加入，R 刷新", 20.0, Color::from_rgb(180, 190, 205), Point2 { x: cx, y: sh * 0.22 + 50.0 }, true)?;
        if self.steam_list_lobbies.is_empty() {
            draw_text(canvas, ctx, "（暂无可加入的房间）", 28.0, Color::from_rgb(170, 178, 194), Point2 { x: cx, y: sh * 0.5 }, true)?;
            draw_text(canvas, ctx, "让好友先创建房间，或按 R 重新搜索", 18.0, Color::from_rgb(150, 160, 178), Point2 { x: cx, y: sh * 0.5 + 48.0 }, true)?;
        } else {
            let mut y = sh * 0.34;
            let head_w = (sw * 0.8).min(760.0);
            let head_x = cx - head_w / 2.0;
            for (i, l) in self.steam_list_lobbies.iter().enumerate() {
                // 房主昵称（临时查 friends；取不到则“房主”）。
                let owner_name = self
                    .steam_sess
                    .as_ref()
                    .map(|s| {
                        let id = net_steam::steamworks::SteamId::from_raw(l.owner);
                        s.transport.friends().get_friend(id).name()
                    })
                    .unwrap_or_else(|| "房主".to_string());
                let full = format!("{}   {}", owner_name, l.name);
                let meta = format!("人数 {}/{}    {}", l.members, l.limit, l.note);
                let selected = i == self.steam_list_selection;
                let bg_col = if selected { Color::from_rgb(52, 60, 74) } else { Color::from_rgb(28, 31, 38) };
                let bg = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), graphics::Rect::new(head_x, y, head_w, 64.0), bg_col)?;
                canvas.draw(&bg, graphics::DrawParam::new());
                let mark = if selected { "[v]" } else { "[ ]" };
                let name_col = if selected { Color::WHITE } else { Color::from_rgb(210, 214, 225) };
                draw_text(canvas, ctx, &format!("{mark}{full}"), 24.0, name_col, Point2 { x: cx, y: y + 20.0 }, true)?;
                draw_text(canvas, ctx, &meta, 16.0, Color::from_rgb(150, 156, 172), Point2 { x: cx, y: y + 44.0 }, true)?;
                y += 74.0;
            }
        }
        draw_text(canvas, ctx, "回车 加入    R 刷新    Q 返回", 18.0, Color::from_rgb(160, 200, 255), Point2 { x: cx, y: sh * 0.90 }, true)?;
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
            #[cfg(feature = "steam")]
            "--steam-host" => {
                app = AppState::SteamHost { players: 4 };
                i += 1;
            }
            #[cfg(feature = "steam")]
            "--steam-join" => {
                app = AppState::SteamJoin { lobby_id: None };
                i += 1;
                // 可选：后面跟一个 LobbyId（fallback 手动加入）。
                #[cfg(feature = "steam")]
                if i < args.len() && args[i].parse::<u64>().is_ok() {
                    if let AppState::SteamJoin { lobby_id } = &mut app {
                        *lobby_id = args[i].parse::<u64>().ok();
                    }
                    i += 1;
                }
            }
            // Steam 冷启动加入：好友接受邀请时，若本游戏未在运行，Steam 会用我们自己定义的
            // connect 串启动它（`+connect_lobby <id>`）——这里解析出来直接进那个大厅。
            #[cfg(feature = "steam")]
            "+connect_lobby" if i + 1 < args.len() => {
                if let Ok(id) = args[i + 1].parse::<u64>() {
                    app = AppState::SteamJoin { lobby_id: Some(id) };
                }
                i += 2;
            }
            "--players" if i + 1 < args.len() => {
                #[cfg(feature = "steam")]
                if let AppState::SteamHost { players } = &mut app {
                    *players = args[i + 1].parse().unwrap_or(4);
                }
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

/// 自定义 winit 事件循环：在 ggez 官方 `event::run` 基础上，额外接入 winit 的
/// `Ime::Commit` 事件（winit 0.30 已移除 `ReceivedCharacter`），使中文输入法（IME）能输入到房间名/备注等文本字段。
/// 其余事件（键盘/鼠标/触摸/绘制/退出）完全照搬 ggez `GgezApplicationHandler` 的分发逻辑，行为不变。
struct GameApp {
    ctx: Context,
    game: Game,
    ime_allowed: bool,
}

impl winit::application::ApplicationHandler for GameApp {
    fn resumed(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        // 启用 IME（中文输入法）。窗口在 ContextBuilder::build 时已创建，此处仅打开 IME 能力。
        if !self.ime_allowed {
            self.ctx.gfx.window().set_ime_allowed(true);
            self.ime_allowed = true;
        }
    }

    fn new_events(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _cause: winit::event::StartCause,
    ) {
        use winit::event_loop::ControlFlow;
        if self.ctx.fields.quit_requested {
            let res = self.game.quit_event(&mut self.ctx);
            self.ctx.fields.quit_requested = false;
            match res {
                Ok(false) => self.ctx.fields.continuing = false,
                Ok(true) => {}
                Err(e) => {
                    eprintln!("Error on quit_event: {e:?}");
                    event_loop.exit();
                    return;
                }
            }
        }
        if !self.ctx.fields.continuing {
            event_loop.exit();
            return;
        }
        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn window_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        mut window_id: winit::window::WindowId,
        mut event: winit::event::WindowEvent,
    ) {
        use winit::event::{ElementState, Ime, MouseScrollDelta, WindowEvent};
        ggez::event::process_window_event(&mut self.ctx, &mut window_id, &mut event);
        match event {
            WindowEvent::Ime(Ime::Commit(text)) => {
                // winit 0.30 统一用 IME 事件报告文本输入：
                // - 普通键盘字符（英文/数字/标点）与中文输入法组合提交都走 `Ime::Commit`；
                //   （本机 IME 已在 `resumed` 里 set_ime_allowed(true)）
                self.game.on_text_input(&text);
            }
            WindowEvent::Ime(_) => {}

            WindowEvent::Resized(size) => {
                let _ = self
                    .game
                    .resize_event(&mut self.ctx, size.width as f32, size.height as f32);
            }
            WindowEvent::CloseRequested => {
                if let Ok(false) = self.game.quit_event(&mut self.ctx) {
                    self.ctx.fields.continuing = false;
                }
            }
            WindowEvent::Focused(gained) => {
                let _ = self.game.focus_event(&mut self.ctx, gained);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let input = ggez::input::keyboard::KeyInput {
                    event,
                    mods: self.ctx.keyboard.active_modifiers,
                };
                let repeat = input.event.repeat;
                match input.event.state {
                    ElementState::Pressed => {
                        let _ = self.game.key_down_event(&mut self.ctx, input, repeat);
                    }
                    ElementState::Released => {
                        let _ = self.game.key_up_event(&mut self.ctx, input);
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (x, y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x, y),
                    MouseScrollDelta::PixelDelta(pos) => {
                        let scale = self.ctx.gfx.window().scale_factor();
                        let logical = pos.to_logical::<f32>(scale);
                        (logical.x, logical.y)
                    }
                };
                let _ = self.game.mouse_wheel_event(&mut self.ctx, x, y);
            }
            WindowEvent::MouseInput {
                state, button, ..
            } => {
                let p = self.ctx.mouse.position();
                match state {
                    ElementState::Pressed => {
                        let _ =
                            self.game
                                .mouse_button_down_event(&mut self.ctx, button, p.x, p.y);
                    }
                    ElementState::Released => {
                        let _ =
                            self.game
                                .mouse_button_up_event(&mut self.ctx, button, p.x, p.y);
                    }
                }
            }
            WindowEvent::CursorMoved { .. } => {
                let p = self.ctx.mouse.position();
                let d = self.ctx.mouse.last_delta();
                let _ = self
                    .game
                    .mouse_motion_event(&mut self.ctx, p.x, p.y, d.x, d.y);
            }
            WindowEvent::Touch(touch) => {
                let _ = self.game.touch_event(
                    &mut self.ctx,
                    touch.phase,
                    touch.location.x,
                    touch.location.y,
                );
            }
            WindowEvent::CursorEntered { .. } => {
                let _ = self.game.mouse_enter_or_leave(&mut self.ctx, true);
            }
            WindowEvent::CursorLeft { .. } => {
                let _ = self.game.mouse_enter_or_leave(&mut self.ctx, false);
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        mut device_id: winit::event::DeviceId,
        mut event: winit::event::DeviceEvent,
    ) {
        use winit::event::DeviceEvent;
        ggez::event::process_device_event(&mut self.ctx, &mut device_id, &mut event);
        if let DeviceEvent::MouseMotion { delta } = event {
            let _ = self.game.raw_mouse_motion_event(&mut self.ctx, delta.0, delta.1);
        }
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.ctx.time.tick();
        if let Err(e) = self.game.update(&mut self.ctx) {
            eprintln!("Error on update: {e:?}");
            event_loop.exit();
            return;
        }
        if let Err(e) = self.ctx.gfx.begin_frame() {
            eprintln!("Error on begin_frame: {e:?}");
            event_loop.exit();
            return;
        }
        if let Err(e) = self.game.draw(&mut self.ctx) {
            eprintln!("Error on draw: {e:?}");
            event_loop.exit();
            return;
        }
        if let Err(e) = self.ctx.gfx.end_frame() {
            eprintln!("Error on end_frame: {e:?}");
            event_loop.exit();
            return;
        }
        self.ctx.mouse.reset_delta();
        self.ctx.keyboard.save_keyboard_state();
        self.ctx.mouse.save_mouse_state();
    }
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
    // 用自定义事件循环替代 `event::run`，以接入中文 IME 输入。
    let mut app = GameApp {
        ctx,
        game,
        ime_allowed: false,
    };
    event_loop.run_app(&mut app).map_err(ggez::GameError::EventLoopError)
}

#[cfg(test)]
mod tests {
    use super::*;

    // C8：IME 去重判定（与 `ime_commit_suppresses_ascii` 对应）。
    // 设备验证（真机 IME 日志）已确认 c6db353 实现在 winit 0.30 下正确；
    // 以下为无头逻辑验证，覆盖两种事件时序。
    #[cfg(feature = "steam")]
    #[test]
    fn c8_same_frame_double_emission_is_suppressed() {
        // 物理键在帧 N 同时触发 IME 提交与 just(c)：
        //  - on_text_input 在 update 前执行，设 last_ime_commit_frame = N+1
        //  - update 开头 frame 变为 N+1
        // 期望：该帧 just(c) 被抑制（不重复插入）。
        let frame_before = 10u64;
        let last = frame_before.wrapping_add(1);
        let frame = frame_before.wrapping_add(1);
        assert!(
            ime_commit_suppresses_ascii(frame, last),
            "同帧 IME 提交 + just(c) 应被抑制，避免重复插入"
        );
    }

    #[cfg(feature = "steam")]
    #[test]
    fn c8_no_ime_commit_allows_ascii_insert() {
        // 普通键盘输入（未走 IME）：last_ime_commit_frame 保持初始 u64::MAX。
        let frame = 5u64;
        let last = u64::MAX;
        assert!(
            !ime_commit_suppresses_ascii(frame, last),
            "无 IME 提交时应正常允许 ASCII 白名单插入"
        );
    }

    #[cfg(feature = "steam")]
    #[test]
    fn c8_cross_frame_double_emission_residual_risk() {
        // 残留风险：若 winit 对同一物理键跨帧（≥2 帧后）双发 just(c)，当前方案不抑制。
        // 真实 IME 下几乎不可能（just_pressed 每帧清空，重复键同帧到达即被上例覆盖），
        // 但记录此边界以便后续若发现交叉输入法行为可快速定位。
        let ime_frame = 10u64;
        let last = ime_frame.wrapping_add(1); // 帧 N 提交
        let frame = ime_frame.wrapping_add(2); // 帧 N+1 的 update 后 frame = N+2
        assert!(
            !ime_commit_suppresses_ascii(frame, last),
            "跨帧(≥2帧)双发不被当前方案抑制——残留风险边界"
        );
    }

    #[cfg(feature = "steam")]
    #[test]
    fn auto_place_rewards_decays_to_zero() {
        // 30 → 30, 18, 10, 6, 3, 1（每名 ×0.6 向下取整，直到 ≤0）
        assert_eq!(auto_place_rewards(30), vec![30, 18, 10, 6, 3, 1]);
        assert_eq!(auto_place_rewards(0), vec![0], "0 名次奖励至少 1 档");
        assert_eq!(auto_place_rewards(1), vec![1]);
        // 大额也应收敛到 0（不无限增长），且覆盖多个名次。
        let big = auto_place_rewards(99999);
        assert!(*big.last().unwrap() >= 1);
        assert!(big.len() > 5 && big.len() <= 64);
        assert!(big.windows(2).all(|w| w[0] >= w[1]), "奖励应单调不增");
    }

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

    /// 好友接受邀请、而本游戏没在运行时，Steam 会用我们的 connect 串冷启动游戏
    /// （`+connect_lobby <id>`）——解析后应直接进那个大厅（`SteamJoin { lobby_id: Some(id) }`）。
    #[cfg(feature = "steam")]
    #[test]
    fn steam_connect_lobby_arg_selects_join_with_lobby_id() {
        let s = |x: &[&str]| x.iter().map(|v| v.to_string()).collect::<Vec<_>>();
        assert_eq!(
            parse_app_from_args(&s(&["exe", "+connect_lobby", "109775241105637376"])),
            AppState::SteamJoin { lobby_id: Some(109775241105637376) }
        );
        // id 非法 → 不误进房间（留在主菜单）。
        assert_eq!(
            parse_app_from_args(&s(&["exe", "+connect_lobby", "not-a-number"])),
            AppState::MainMenu
        );
        // 缺参数 → 主菜单（不能死循环 / 不能误解析后面的参数）。
        assert_eq!(parse_app_from_args(&s(&["exe", "+connect_lobby"])), AppState::MainMenu);
    }
}
