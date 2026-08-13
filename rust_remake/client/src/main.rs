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

/// 机器人数量（不含玩家本人）
const BOTS: u32 = 7;
/// 固定步长模拟（帧率）
const TICK: f64 = 1.0 / 60.0;
/// 玩家本人 = id 0
const PLAYER_ID: u32 = 0;

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
}

impl Game {
    fn new(ctx: &mut Context) -> GameResult<Self> {
        // 注册中文字体：用 include_bytes 内嵌，避免资源路径/VFS 解析问题。
        let font = ggez::graphics::FontData::from_slice(include_bytes!("../../assets/fonts/cjk.ttf"))?;
        ctx.gfx.add_font("cjk", font);

        let player_count = 1 + BOTS;
        let seed = 20260812u64;
        let world = World::new(player_count, seed);
        // 整场对抗：3 小局，所有玩家都纳入档案
        let mut meta = MatchState::new(MatchConfig::default(), &(0..player_count).collect::<Vec<_>>(), 8);
        // 默认绑定：每个键绑定其树的首个技能，保证开局即可用（玩家可在学习阶段改）
        for p in meta.profiles.iter_mut() {
            for key in game_core::skill::CastKey::ALL {
                let options = key.tree().skills_in_tree();
                if let Some(first) = options.first() {
                    p.bind_skill(key, *first);
                }
            }
        }

        let bot_rngs = (1..player_count)
            .map(|id| Rng::new(seed ^ (id as u64).wrapping_mul(0x9E3779B97F4A7C15)))
            .collect();

        let (w, h) = ctx.gfx.drawable_size();
        Ok(Game {
            world,
            meta,
            player_target: None,
            pending_cast: None,
            pending_skill: None,
            queued_cmds: std::collections::VecDeque::new(),
            pending_shift_skill: None,
            learn_tree_key: None,
            bot_targets: vec![None; BOTS as usize],
            bot_rngs,
            accumulator: 0.0,
            scale: 1.0,
            offset: Point2 { x: w / 2.0, y: h / 2.0 },
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
                        .find(|pr| pr.player_id == PLAYER_ID)
                    {
                        profile.bind_skill(key, *skill);
                    }
                }
            }
        }

        // `=` 键：升级当前选中键绑定的技能
        if ctx.keyboard.is_logical_key_just_pressed(&Key::Character("=".into())) {
            if let Some(key) = learn_key {
                if let Some(profile) = self
                    .meta
                    .profiles
                    .iter_mut()
                    .find(|pr| pr.player_id == PLAYER_ID)
                {
                    if let Some(skill) = profile.bound_skill(key) {
                        let cost = upgrade_cost(profile.skill_level(skill));
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
        if let Some(profile) = self.meta.profiles.iter().find(|pr| pr.player_id == PLAYER_ID) {
            if let Some(p) = self.world.players.get_mut(PLAYER_ID as usize) {
                for i in 0..p.skill_levels.len().min(profile.skill_levels.len()) {
                    p.skill_levels[i] = profile.skill_levels[i];
                }
            }
        }
        self.world.reset_round();
        self.player_target = None;
        self.pending_cast = None;
        self.pending_skill = None;
        self.accumulator = 0.0;
    }

    /// 每帧统一轮询输入（键盘 + 鼠标都用 ggez 的 just-pressed 边沿检测）。
    fn poll_input(&mut self, ctx: &Context) {
        use ggez::input::keyboard::Key;
        use ggez::input::mouse::MouseButton;

        // 玩家档案（本帧只读绑定的技能）
        let bound_for = |key: game_core::skill::CastKey| -> Option<SkillId> {
            self.meta
                .profiles
                .iter()
                .find(|pr| pr.player_id == PLAYER_ID)
                .and_then(|p| p.bound_skill(key))
        };

        // shift 按住 = 预排队列模式（winit ModifiersState::shift_key()）。
        let shift = ctx.keyboard.active_modifiers.shift_key();

        // 1) 技能键：按下 → 施放该键绑定的技能（shift 时入列）
        for (letter, key) in KEY_LETTERS {
            let just = ctx
                .keyboard
                .is_logical_key_just_pressed(&Key::Character(letter.into()));
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
                    }
                }
            }
        }
        // S: 停止移动 + 清空 shift 队列
        if ctx.keyboard.is_logical_key_pressed(&Key::Character("s".into())) {
            self.player_target = None;
            self.pending_skill = None;
            self.pending_cast = None;
            self.pending_shift_skill = None;
            self.queued_cmds.clear();
        }

        // 2) 左键：确认点目标技能（cursor 位置作为落点）
        if ctx.mouse.button_just_pressed(MouseButton::Left) {
            let m = ctx.mouse.position();
            let world = self.screen_to_world(m.x, m.y);
            if let Some(skill) = self.pending_skill.take() {
                self.player_target = None;
                self.pending_cast = Some((skill, Some(world)));
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
                self.player_target = Some(world);
            }
        }
    }

    /// 生成本（模拟）帧内所有玩家的输入。
    fn compute_inputs(&mut self) -> Vec<PlayerInput> {
        let mut inputs: Vec<PlayerInput> = self
            .world
            .players
            .iter()
            .map(|_| PlayerInput::default())
            .collect();

        // 玩家本人：移动目标 + 施法命令 + 本帧把 shift 队列一次全部注入（批量）
        let p0 = &mut inputs[PLAYER_ID as usize];
        p0.set_target = self.player_target;
        p0.cast = self.pending_cast;
        p0.queued = self.queued_cmds.drain(..).collect();

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

    fn draw_scene(&mut self, ctx: &mut Context) -> GameResult {
        self.update_camera(ctx)?;
        let mut canvas = Canvas::from_frame(ctx, Color::from_rgb(18, 22, 34));

        // 瞄准指示：从玩家到鼠标的画一条线（点目标技能待左键确认）。
        if self.pending_skill.is_some() {
            if let Some(p) = self.world.players.get(PLAYER_ID as usize) {
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
        for p in self.world.players.iter() {
            if !p.alive {
                continue;
            }
            let fx = p.pos.x.to_num::<f32>() * self.scale + self.offset.x;
            let fy = p.pos.y.to_num::<f32>() * self.scale + self.offset.y;
            let r = p.radius.to_num::<f32>() * self.scale;
            let mut color = player_color(p.id);
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

        canvas.finish(ctx)?;
        Ok(())
    }

    /// 渲染学习阶段 / 整场结束的信息覆盖层（无依赖文本，用简笔几何表示）。
    fn draw_meta_overlay(&mut self, canvas: &mut Canvas, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();

        match self.meta.phase {
            MatchPhase::Fighting => {
                // 技能冷却 HUD：底部一排 8 个键位槽，显示绑定技能图标/名称 + 冷却遮罩
                if let (Some(me), Some(me_player)) = (
                    self.meta.profiles.iter().find(|p| p.player_id == PLAYER_ID),
                    self.world.players.get(PLAYER_ID as usize),
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
                if let Some(me) = self.meta.profiles.iter().find(|p| p.player_id == PLAYER_ID) {
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
            }
        }
        Ok(())
    }
}

impl event::EventHandler for Game {
    fn update(&mut self, ctx: &mut Context) -> GameResult {
        let dt = ctx.time.delta().as_secs_f64();

        match self.meta.phase {
            MatchPhase::Finished => {
                // 整场对抗结束：不再模拟
                self.accumulator = 0.0;
                Ok(())
            }
            MatchPhase::Learning => {
                // 学习阶段：轮询购买升级输入 + 计时
                self.poll_learning(ctx);
                let now = self.meta.tick_learning(dt.min(0.25));
                // 若学习结束，进入下一局
                if self.meta.phase == MatchPhase::Fighting {
                    self.teardown_round_end();
                }
                let _ = now;
                Ok(())
            }
            MatchPhase::Fighting => {
                // 每帧轮询输入（技能键 / 鼠标）
                self.poll_input(ctx);
                self.accumulator += dt.min(0.25);
                while self.accumulator >= TICK {
                    let inputs = self.compute_inputs();
                    self.world.step(inputs, Fix64::from_num(TICK));
                    self.accumulator -= TICK;
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
        self.draw_scene(ctx)
    }
}

fn player_color(id: u32) -> Color {
    if id == PLAYER_ID {
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

fn main() -> GameResult {
    let (mut ctx, event_loop) = ggez::ContextBuilder::new("frame-sync-arena", "remake")
        .window_setup(ggez::conf::WindowSetup::default().title("帧同步圆球竞技场 — 阶段1"))
        .window_mode(
            ggez::conf::WindowMode::default()
                .dimensions(1280.0, 720.0)
                .resizable(true),
        )
        .build()?;

    let game = Game::new(&mut ctx)?;
    event::run(ctx, event_loop, game)
}
