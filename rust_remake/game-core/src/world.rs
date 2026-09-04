//! 世界/对局 —— 确定性的核心模拟。
//!
//! `World` 在固定步长下推进，玩家输入（设定移动目标）来自 `WorldInput`。
//! 所有规则均为纯整数定点运算，因此相同输入可产生完全一致的结果，
//! 这是后续帧同步（lockstep）联网的基础。

use crate::balance::Balance;
use crate::fix::{Fix64, Vec2};
use crate::player::{BuffKind, Cmd, Kick, Player};
use crate::rng::Rng;
use crate::skill::{SkillEffect, SkillId, DefTable};

/// 场地收缩参数（复刻原版 `AreaScript` 的量级，稍加快以体现压迫感）。数值权威源见 [`crate::balance::Balance`]。
pub const START_RADIUS: f64 = Balance::default().start_radius;
pub const SHRINK_SPEED: f64 = Balance::default().shrink_speed; // 半径减少量 / 秒
/// 出界伤害：球心距圆点 > 圈半径时，每帧扣除的 HP / 秒。
pub const OUT_HURT: f64 = Balance::default().out_hurt;
/// 玩家相互挤压（重叠）时受到的伤害 / 秒。
pub const OVERLAP_DAMAGE: f64 = Balance::default().overlap_damage;
/// E3/E3b 撒出的扇形子弹（原版 `SABulletScript`）的伤害与射程。
pub const SABULLET_DAMAGE: f64 = Balance::default().sabullet_damage;
pub const SABULLET_RANGE: f64 = Balance::default().sabullet_range;

/// 每个玩家当前帧的输入。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerInput {
    /// 若为 `Some(pos)`，则令该玩家朝 `pos` 直线移动。
    pub set_target: Option<Vec2>,
    /// 若为 `Some((skill, target))`，则尝试对该技能施法（target 为点目标/朝向）。
    pub cast: Option<(SkillId, Option<Vec2>)>,
    /// 本帧新压入的 shift 指令（可批量）；由 `World` 全部入队，随后按施法节奏依次执行。
    pub queued: Vec<Cmd>,
    /// 若为 true，则本帧先清空该玩家在 `World` 中的命令队列（S 清队 / 普通即时操作打断）。
    pub clear_queue: bool,
    /// 若为 true，则本帧清除该玩家的移动目标（停止移动；S 停手）。
    pub stop_move: bool,
}

/// 一整帧里所有玩家的输入。
pub type InputSlice = Vec<PlayerInput>;

/// 场上一个飞行物 / 延时区域（石头、弹体、导弹、激光线、幻象假身）。
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Projectile {
    /// 所有者（用于免疫自身伤害）
    pub owner: u32,
    pub kind: ProjectileKind,
    pub pos: Vec2,
    pub alive: bool,
}

/// 一次爆炸结算参数（石头 / 导弹通用）。
struct ProjExplosion {
    pos: Vec2,
    owner: u32,
    radius: Fix64,
    damage: Fix64,
    bomb_force: Fix64,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ProjectileKind {
    /// 延时爆炸的石头：倒计时结束对半径内造成伤害+击退
    Rock {
        fuse: Fix64,
        radius: Fix64,
        damage: Fix64,
        bomb_force: Fix64,
    },
    /// 幻象假身：在场上停留一段时间后消失（纯迷惑，无伤害）
    Decoy { radius: Fix64, lifetime: Fix64 },
    /// 直射弹体：沿固定方向飞，命中最近的目标造成伤害后消失。
    Bullet {
        dir: Vec2,
        speed: Fix64,
        damage: Fix64,
        radius: Fix64,
        remaining: Fix64, // 剩余飞行距离（射程）
    },
    /// 追踪导弹：每帧朝目标全速直追，命中（或射程耗尽）后在其位置爆炸伤+击退。
    Missile {
        dir: Vec2,
        speed: Fix64,
        damage: Fix64,
        radius: Fix64,
        push_power: Fix64,
        push_time: Fix64,
        remaining: Fix64,
    },
    /// 回旋镖（D2）：持有速度矢量，每帧朝施法者加速回飞（原版 BoomerangScript）；撞障碍反弹；命中爆炸伤+击退。
    Boomerang {
        vel: Vec2,
        accelerate: Fix64,
        damage: Fix64,
        radius: Fix64,
        push_power: Fix64,
        push_time: Fix64,
        life: Fix64,
        owner_pos: Vec2, // 用于回飞拉拽的施法者位置
    },
    /// 双香蕉曲线弹（D4）：沿方向飞行并朝固定角速度旋转（曲线），命中爆炸伤+击退。
    Banana {
        dir: Vec2,
        speed: Fix64,
        turn: Fix64, // 每帧旋转角（弧度），为正则顺时针、负则逆时针
        damage: Fix64,
        radius: Fix64,
        push_power: Fix64,
        push_time: Fix64,
        life: Fix64,
    },
    /// 滚动火球（E1b 掷弹 StoneShot）：沿定速直线滚动，接触范围内的敌人持续掉血（DoT）。
    Rolling {
        dir: Vec2,
        speed: Fix64,
        damage_per_sec: Fix64,
        radius: Fix64,
        remaining: Fix64, // 剩余飞行距离（射程）
    },
    /// 撒弹线（E3/E3b）：沿方向飞行的线，到目标/沿途把扇形弹撒出去。
    ScatterLine {
        dir: Vec2,
        speed: Fix64,
        remaining: Fix64, // 剩余飞行距离
        scatter: ScatterKind,
    },
    /// 持续伤害线：一端在施法者，朝目标方向延伸，扫过即伤（LineBeam）。
    Beam {
        dir: Vec2,
        length: Fix64,
        width: Fix64,
        damage_per_sec: Fix64,
        remaining: Fix64,
    },
    /// 链式/跳弹镖（T1b/T3/TestLeech）：全速直追最近敌人，命中后跳跃到下一个（或吸血、衰减伤害）。
    Chain {
        dir: Vec2,
        speed: Fix64,
        damage: Fix64,
        heal: Fix64,       // 每次命中给施法者的回血量（T1b/TestLeech；0=无）
        ratio: Fix64,      // 本次命中伤害倍率（T3 衰减用；1 起）
        ratio_decay: Fix64,// 每次跳跃衰减量（T3；0=不衰减）
        life: Fix64,       // 剩余飞行时间/总生存（两跳之间到不了新目标则自然消失）
        last_target: u32,  // 上次目标（避免立刻跳回），用 u32::MAX 表示无
        owner: u32,
        max_chain: u32,    // 最多链跳次数（含首次命中后继续跳的累计上限；防止“吸血/跳弹”无限往返）
        hit_count: u32,    // 已命中次数；达到 max_chain 即消失
        turn_delay: Fix64, // 转镖（TestLeech）：初始沿直线飞行这段时间后，才开始追踪最近敌人（>0 为剩余延迟）
    },
    /// 蓄力跳弹·直线炸弹（T3b）：沿方向飞行，命中玩家→伤+推+累计 damageplus+生成回返镖；
    /// 射程耗尽没命中→damageplus 归零。
    BonusBomb {
        dir: Vec2,
        speed: Fix64,
        damage: Fix64,
        radius: Fix64,
        push_power: Fix64,
        push_time: Fix64,
        remaining: Fix64,
        owner: u32,
    },
    /// 回返镖（T3b）：全速向施法者返回，到位即刷新其蓄力跳弹的冷却并自毁。能命中敌人则伤+推。
    Returner {
        dir: Vec2,
        speed: Fix64,
        damage: Fix64,
        radius: Fix64,
        push_power: Fix64,
        push_time: Fix64,
        owner: u32,
    },
    /// 回拉/束缚线（Y1/Y1b）：记录绑定的目标玩家，每帧把它拉向施法者并持续掉血（beam 时额外扫射）。
    Tether {
        owner: u32,
        target: u32,
        damage_per_sec: Fix64,
        pull_speed: Fix64,
        remaining: Fix64,
        beam: bool,
    },
    /// 引力场（Y3）：飞行场持续把附近敌人吸向场中心。
    Gravity {
        dir: Vec2,
        speed: Fix64,
        radius: Fix64,
        pull_speed: Fix64,
        remaining: Fix64,
    },
    /// 星域持续伤（Y3b）：静态区域，范围内敌掉血、对施法者回血。
    Star {
        owner: u32,
        radius: Fix64,
        damage_per_sec: Fix64,
        heal_per_sec: Fix64,
        remaining: Fix64,
    },
    /// 束缚线（Y2b）：两点反向收拢；交汇成线时线上的敌人被束缚。
    BindLine {
        dir: Vec2,
        speed: Fix64,
        count: u32,
        fired: u32,
        bind_time: Fix64,
        from: Vec2,
        end: Vec2,
    },
    /// 撞击迟缓弹（Y2）：直线飞行，命中→伤害 + 沿弹-目标方向推离。
    PushBullet {
        dir: Vec2,
        speed: Fix64,
        damage: Fix64,
        radius: Fix64,
        push_power: Fix64,
        push_time: Fix64,
        remaining: Fix64,
    },
    /// 098b 名册弹体（M1/M2：S000/S003/S004/S008/S009/S014/S015/S016）。运动学见 `W098bProjKind`；
    /// 命中统一走 KI/FI 结算（FI 伤害 + KI 击退，PORT_098B_DECISIONS.md D3/M1）。
    W098b {
        /// 运动学形态。
        proj: crate::skill::W098bProjKind,
        /// 当前速度矢量（回旋镖回程时朝施法者加速；弹跳弹命中后重定向）。
        vel: Vec2,
        /// 弹速标量（Homing 全速直追用）。
        speed: Fix64,
        radius: Fix64,
        /// 剩余寿命（秒）。
        remaining: Fix64,
        /// 出程时长（Boomerang 出/回分界 = life/2）。
        life: Fix64,
        /// FI 伤害系数 gX（随施法等级已求值；Bounce 每跳 ×0.8）。
        gx: Fix64,
        /// KI 击退系数 JI。
        kb_ji: Fix64,
        /// 命中点燃 DoT 总量（无则 None）。
        ignite: Option<Fix64>,
        /// AoE 爆炸半径（陨石 200；None=单体命中）。命中或寿命尽时触发。
        blast: Option<Fix64>,
        /// Homing：锁定目标玩家 id；Bounce：上一跳命中的玩家 id（跳过）。
        target: Option<u32>,
        /// Boomerang 是否已转入回程。
        returning: bool,
    },
}

/// 撒弹线的撒弹方式。
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ScatterKind {
    /// E3：到终点一次性撒出一个扇形（`count` 发，角度步进 `step_rad`）。
    Burst { count: u32, step_rad: Fix64, bullet_speed: Fix64 },
    /// E3b：飞行途中每 `interval` 秒撒一发并让方向转 `turn_rad`。
    Periodic {
        count: u32,
        interval: Fix64,
        elapsed: Fix64,
        bullet_speed: Fix64,
        turn_rad: Fix64,
    },
}

/// 没有半径字段的飞行弹体（链镖系 / 撒弹线）用于「撞柱子」判定的碰撞半径。
/// 取值与直射弹 `Bullet` 的常规半径（0.5~0.6）对齐。
const PROJ_HIT_RADIUS_FALLBACK: f64 = 0.5;

// ===== 098b KI/FI 结算参数（PORT_098B_DECISIONS.md D3/M1） =====
/// 098b 击退近似时长（秒）：098b 本体是逐帧衰减（每帧 ×~0.96）的速度累积，
/// M1 以恒速 push 近似；此值与速度封顶共同标定总位移，TODO M2 对齐衰减模型。
const W098B_KB_TIME: f64 = 0.35;
/// 098b 击退初速封顶（war3 单位/s）：`DAMAGE_BASE×gx×JI` 在高等级可达 ~2300+，
/// 与移速 210 相比已是 10 倍级——封顶防极端等级把人推出半张图。
const W098B_KB_MAX_SPEED: f64 = 2000.0;
/// 098b 火球点燃时长（秒）：consolidated S000「点燃 2.5×jn」。
const W098B_IGNITE_SECONDS: f64 = 2.5;

/// 098b KI 击退初速：`(100+满蓝)×0.03 = DAMAGE_BASE` 折叠（D3）× `gX` × `JI`，封顶防超远。
fn warlock_ki_knockback(gx: Fix64, ji: Fix64) -> Fix64 {
    let raw = Fix64::from_num(crate::balance::DAMAGE_BASE) * gx * ji;
    raw.min(Fix64::from_num(W098B_KB_MAX_SPEED))
}

impl ProjectileKind {
    /// 该弹体是否参与「撞柱子（静态圆形障碍）」判定，以及判定时用的半径。
    ///
    /// 返回 `None` = 不参与：都是**不飞行**的类型——
    /// `Rock` 是落在目标点的延时爆炸物（不移动）、`Decoy` 是假身、`Beam` 是从施法者伸出的固定射线、
    /// `Tether` 绑定在目标玩家身上、`Star` 是静态区域、`BindLine` 是两点收拢的线。
    /// 前三者要做阻挡得改成"截断长度/改落点"，是另一类改动，本次不做。
    fn obstacle_radius(&self) -> Option<Fix64> {
        Some(match self {
            ProjectileKind::Bullet { radius, .. }
            | ProjectileKind::Missile { radius, .. }
            | ProjectileKind::Banana { radius, .. }
            | ProjectileKind::Rolling { radius, .. }
            | ProjectileKind::BonusBomb { radius, .. }
            | ProjectileKind::Returner { radius, .. }
            | ProjectileKind::Gravity { radius, .. }
            | ProjectileKind::PushBullet { radius, .. }
            // 回旋镖单独处理：撞柱是**反弹**而不是消失（原版 BoomerangScript 的 MirrorBy），保留原手感。
            | ProjectileKind::Boomerang { radius, .. } => *radius,
            // 098b 弹体：回旋镖（S004）撞柱反弹，其余撞柱消失（见撞障碍分支）。
            | ProjectileKind::W098b { radius, .. } => *radius,
            ProjectileKind::Chain { .. } | ProjectileKind::ScatterLine { .. } => {
                Fix64::from_num(PROJ_HIT_RADIUS_FALLBACK)
            }
            ProjectileKind::Rock { .. }
            | ProjectileKind::Decoy { .. }
            | ProjectileKind::Beam { .. }
            | ProjectileKind::Tether { .. }
            | ProjectileKind::Star { .. }
            | ProjectileKind::BindLine { .. } => return None,
        })
    }
}

/// 静态圆形障碍（原版 demo 里实际用作"墙/柱子"的碰撞体）。
/// 用圆盘描述，几何与玩家一致，但不参与名次/击杀/死亡判定。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obstacle {
    pub pos: Vec2,
    pub radius: Fix64,
}

impl Obstacle {
    pub fn new(pos: Vec2, radius: f64) -> Self {
        Obstacle {
            pos,
            radius: Fix64::from_num(radius),
        }
    }
}

/// 确定性对局核心。
#[derive(Clone, Debug)]
pub struct World {
    pub players: Vec<Player>,
    pub arena_radius: Fix64,
    /// 试验场模式（单机技能试验场）：不缩圈、不出圈掉血、不判对局结束。
    pub sandbox: bool,
    /// 柱子/障碍布局使用的确定性种子。每轮递增，保证各小局地形不同、且两端一致。
    pub round_seed: u64,
    /// 场景里的静态圆形障碍（柱子/墙）
    pub obstacles: Vec<Obstacle>,
    /// 场上飞行物 / 延时区域
    pub projectiles: Vec<Projectile>,
    /// 按死亡先后记录的玩家 id（用于本局名次结算）
    pub(crate) eliminated_order: Vec<u32>,
    /// 本局内发生的击杀：(击杀者 id, 被击杀者 id)
    pub(crate) kills_this_round: Vec<(u32, u32)>,
    pub(crate) time: Fix64,
    /// 瞬态渲染痕迹（仅客户端读取，不参与确定性逻辑/序列化）：闪电射线 (起点, 终点, 剩余显示秒)。
    /// 每帧 `step` 开头递减剩余时间、归零清空；由 `execute_effects` 的 Lightning 效果设置（Unity 原版约 0.1s），供 client 画线。
    pub lightning_visual: Option<(Vec2, Vec2, Fix64)>,
}

impl World {
    /// 创建一场对局。`player_count` 为玩家人数；`seed` 用于 AI / 初始布局等确定性随机。
    pub fn new(player_count: u32, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let arena_radius = Fix64::from_num(START_RADIUS);
        let mut players = Vec::with_capacity(player_count as usize);
        // 把玩家在 arena*0.6 的环上**均匀等分**分布（并整体随机旋转一帧视角），
        // 保证彼此初始不重叠且出界伤害不至于一开始就触发。
        let spawn_rot = Fix64::from_num(std::f64::consts::TAU) * rng.next_fix();
        for id in 0..player_count {
            let r = arena_radius * Fix64::from_num(0.6);
            let angle = spawn_rot
                + Fix64::from_num(std::f64::consts::TAU) * Fix64::from_num(id as f64 / player_count as f64);
            let pos = Vec2::new(r * crate::fix::cos(angle), r * crate::fix::sin(angle));
            players.push(Player::new(id, pos, Fix64::from_num(crate::player::DEFAULT_RADIUS)));
        }
        let mut obstacles = Vec::new();
        _layout_obstacles(&mut obstacles, &mut rng, arena_radius);
        World {
            players,
            arena_radius,
            sandbox: false,
            round_seed: seed,
            obstacles,
            projectiles: Vec::new(),
            eliminated_order: Vec::new(),
            kills_this_round: Vec::new(),
            time: Fix64::ZERO,
            lightning_visual: None,
        }
    }

    pub fn time(&self) -> Fix64 {
        self.time
    }

    /// 给定所有玩家的输入，推进固定步长。
    pub fn step(&mut self, input: InputSlice, dt: Fix64) {
        debug_assert_eq!(input.len(), self.players.len(), "input 必须覆盖每位玩家");
        self.time += dt;
        // 瞬态渲染痕迹：每帧递减剩余显示时间，归零后清空（由本帧施放的闪电效果重新设置并计时）。
        let expire = if let Some((_, _, rem)) = self.lightning_visual.as_mut() {
            *rem -= dt;
            *rem <= Fix64::ZERO
        } else {
            false
        };
        if expire {
            self.lightning_visual = None;
        }

        // 0) 先按 clear_queue 清空各玩家队列、按 stop_move 清移动目标，再入队新 shift 指令
        for (p, pi) in self.players.iter_mut().zip(input.iter()) {
            if pi.clear_queue {
                p.cmd_clear();
            }
            if pi.stop_move {
                p.move_target = None;
            }
            for c in pi.queued.iter().copied() {
                p.cmd_push(c);
            }
        }

        // 1) 技能：处理施法输入 → 推进施法状态机 → 结算完成的效果
        let just_cast = self.handle_casts(&input, dt);

        // 2) 应用移动输入（跳过本帧刚进入施法的玩家：施法取消旧移动命令）
        let mut fake_locs: Vec<(u32, Vec2)> = Vec::new();
        for (i, (p, pi)) in self.players.iter_mut().zip(input.iter()).enumerate() {
            if !p.alive || just_cast[i] {
                continue;
            }
            // C4 幻象：若处于「待幻」且收到移动目标 → 触发留假身+瞬移（不真正移动）。
            // 只有给出移动目标才触发；无目标时不取消（由 tick_buffs 的超时窗口回收）。
            if p.fake_active.is_some() {
                if let Some(target) = pi.set_target {
                    fake_locs.push((i as u32, target));
                }
                continue;
            }
            // R2b 冲刺斩：给新的移动目标 → 解除冲刺并现身（原版 `IdoDSWL`）。
            if pi.set_target.is_some() && p.dash_active {
                p.dash_active = false;
                p.dash_vel = Vec2::ZERO;
                p.control = None; // 若有残留强制态也一并清除
                p.remove_buff(BuffKind::Stealth);
            }
            // 仅在有新移动目标时更新 move_target；None 表示“本帧没有新目标”，不覆盖（让 shift 队列的移动得以保留）。
            if let Some(t) = pi.set_target {
                p.move_target = Some(t);
            }
        }
        for (pid, target) in fake_locs {
            self.fake_locate(pid, target);
        }

        // 3) 移动：本帧流程 = 清 pull → 场效应累加 pull → 合成速度推进 + buff 计时
        for p in self.players.iter_mut() {
            p.reset_pull();
        }
        // 3b) 场效应贡献本帧附加速度（引力场 / 回拉线等；暂为空，各技能接入）
        self.step_area_forces(dt);
        // 3c) T2 扇扫连射：按心率依次发射
        self.step_sweep(dt);
        for p in self.players.iter_mut() {
            p.step_velocity(dt);
            p.tick_buffs(dt);
            // S006 时光回溯（098b ER）：倒计时到点闪回锚点并还原 HP（不低于 1，避免回溯自杀）。
            if let Some((pos, hp, rem)) = p.rewind {
                let rem = rem - dt;
                if rem <= Fix64::ZERO {
                    p.pos = pos;
                    p.hp = hp.max(Fix64::ONE);
                    p.rewind = None;
                } else {
                    p.rewind = Some((pos, hp, rem));
                }
            }
        }

        // 4) 场地收缩（随时间）—— 试验场不缩圈
        if !self.sandbox {
            self.shrink_arena(dt);
        }

        // 5) 玩家之间的碰撞
        resolve_player_collisions(&mut self.players, dt);
        // 5b) 玩家与障碍（圆形柱子）的分离
        self.resolve_obstacles(dt);

        // 6) 飞行物 / 延时区域
        self.step_projectiles(dt);

        // 7) 边界：出界掉血（无自动回收，玩家需自己走位回去）+ 死亡
        let mut new_deaths = Vec::new();
        let mut new_kills = Vec::new();
        for p in self.players.iter_mut() {
            if !p.alive {
                continue;
            }
            if !self.sandbox && p.pos.length() > self.arena_radius {
                // 球心已出圈：持续掉血。回去靠自己走位。（boost 期间返一半回血）
                let net = p.soak_boost(Fix64::from_num(OUT_HURT) * dt);
                p.hp = (p.hp - net).max(Fix64::ZERO);
            }
            if p.hp <= Fix64::ZERO && p.alive {
                p.hp = Fix64::ZERO;
                p.alive = false;
                new_deaths.push(p.id);
                if let Some(k) = p.last_hit_by {
                    new_kills.push((k, p.id));
                }
            }
        }
        self.eliminated_order.extend(new_deaths);
        self.kills_this_round.extend(new_kills);

        // 8) shift 指令队列：空闲时逐个执行队头指令（行走完/施法做完再执行下一个）。
        self.step_command_queue();
    }

    /// shift 指令队列：玩家空闲（不施法、无移动目标、不在强制位移/冲刺）时，弹出队头指令执行。
    fn step_command_queue(&mut self) {
        for i in 0..self.players.len() {
            loop {
                if !self.players[i].alive {
                    break;
                }
                // 空闲判定
                let idle = self.players[i].caster.phase() == crate::skill::CastPhase::Idle
                    && self.players[i].move_target.is_none()
                    && self.players[i].control.is_none()
                    && !self.players[i].dash_active;
                if !idle {
                    break;
                }
                let Some(cmd) = self.players[i].cmd_peek() else {
                    break;
                };
                match cmd {
                    Cmd::Move(t) => {
                        self.players[i].cmd_pop();
                        self.players[i].move_target = Some(t);
                        break; // 开始移动：本帧停止级联，等到达后再执行下一个
                    }
                    Cmd::Cast(skill, target) => {
                        let def = crate::skill::DefTable::def(skill);
                        let lv = self.players[i].skill_level(skill);
                        let pos = self.players[i].pos;
                        let r = self.players[i].radius;
                        let ok = self.players[i]
                            .caster
                            .try_cast(&def, lv, target, pos, r)
                            .is_ok();
                        self.players[i].cmd_pop(); // 无论成败均消耗该指令
                        if ok {
                            // 施法开始（会取消移动）；施法状态机由下一帧 handle_casts 推进并结算效果。
                            self.players[i].move_target = None;
                        }
                        break; // 本帧不再继续执行后续指令（若施成功则进入 busy，若不成功则丢弃）
                    }
                    Cmd::Stop => {
                        self.players[i].cmd_pop();
                        self.players[i].move_target = None;
                        // 停止后继续尝试执行队列里下一个指令
                    }
                }
            }
        }
    }

    /// 施法流程：
    /// 1. 读取本帧的施法请求（若可用则进入前摇）
    /// 2. 推进所有玩家施法状态机（冷却计时 + 前/后摇）
    /// 3. 对前摇结束的技能执行效果
    ///
    /// 返回 `just_cast`：每个玩家本帧是否新开始了一次施法（用于取消旧移动命令）。
    fn handle_casts(&mut self, input: &InputSlice, dt: Fix64) -> Vec<bool> {
        let mut just_cast = vec![false; self.players.len()];

        // a) 先应用新的施法请求（占用本帧的人手）
        for (idx, (p, pi)) in self.players.iter_mut().zip(input.iter()).enumerate() {
            if !p.alive {
                continue;
            }
            if let Some((skill, target)) = pi.cast {
                // C3 影身·召回：已有锚点时再按影身 = 立即传回（原版 `BackToShadow` 忽略冷却）
                if skill == SkillId::Shadow && p.shadow_anchor.is_some() {
                    let anchor = p.shadow_anchor.take().unwrap();
                    p.pos = anchor;
                    p.shadow_window = Fix64::ZERO;
                    p.move_target = None;
                    just_cast[idx] = true;
                    continue;
                }
                // R1b 二段闪·第二段：窗口内再按 = 免冷却短闪一次（原版 `Skillscd`，距离 4）
                if skill == SkillId::Blink2 && p.blink2_window.is_some() {
                    p.blink2_window = None;
                    if let Some(t) = target {
                        let d = t - p.pos;
                        let dist = d.length();
                        if dist > Fix64::ZERO {
                            // 短闪距离（旧尺度 4 → war3 尺度 ×60，过渡换算见 PORT_098B_DECISIONS.md D4）
                            let md = Fix64::from_num(4.0 * 60.0);
                            if dist > md {
                                p.pos += d.normalized() * md;
                            } else {
                                p.pos = t;
                            }
                        }
                    }
                    p.move_target = None;
                    just_cast[idx] = true;
                    continue;
                }
                let def = DefTable::def(skill);
                // 无蓝量系统（PORT_098B_DECISIONS.md D3）：098b 施法不耗蓝，仅冷却/前摇门控。
                if p
                    .caster
                    .try_cast(&def, p.skill_level(skill), target, p.pos, p.radius)
                    .is_ok()
                {
                    // 施法开始：取消当前移动命令（施法优先于走位）
                    p.move_target = None;
                    just_cast[idx] = true;
                }
            }
        }

        // b) 推进施法状态机；收集本帧“前摇结束”的效果并执行
        let mut fire_queue: Vec<(u32, SkillId, Option<Vec2>)> = Vec::new();
        for (idx, p) in self.players.iter_mut().enumerate() {
            if let Some((id, target)) = p.caster.advance(dt) {
                fire_queue.push((idx as u32, id, target));
                p.caster.begin_cooldown(id);
            }
        }

        // 执行本帧完成前摇的技能效果
        execute_effects(self, &fire_queue);
        just_cast
    }

    fn shrink_arena(&mut self, dt: Fix64) {
        self.arena_radius -= Fix64::from_num(SHRINK_SPEED) * dt;
        // 复刻原版 AreaScript：缩到 0 才停（不留最小半径阈值），但用极小值避免归负。
        if self.arena_radius < Fix64::ZERO {
            self.arena_radius = Fix64::ZERO;
        }
    }

    /// C4 幻象·第二阶段：本体沿 `target` 方向瞬移 2，原位留 2 个假身（约 120° 间隔），
    /// 假身持续 `fake_window`（待幻剩余时间）秒。
    fn fake_locate(&mut self, pid: u32, target: Vec2) {
        let idx = pid as usize;
        let Some(p) = self.players.get_mut(idx) else { return };
        let center = p.pos;
        let dir = (target - center).normalized();
        let shift = if dir.length_squared() == Fix64::ZERO {
            Vec2::new(Fix64::from_num(2), Fix64::ZERO)
        } else {
            dir * Fix64::from_num(2)
        };
        let lifetime = p.fake_active.take().unwrap_or(Fix64::from_num(2.0));
        let radius = p.radius;
        p.pos = center + shift;
        p.move_target = None;
        // 两个假身：一个在原位，一个在原位 + 旋转 120° 偏移
        let off2 = crate::fix::rotate_ccw(shift, Fix64::from_num(std::f64::consts::TAU / 3.0));
        for off in [shift, off2] {
            self.projectiles.push(Projectile {
                owner: pid,
                kind: ProjectileKind::Decoy { radius, lifetime },
                pos: center + off,
                alive: true,
            });
        }
    }
    ///
    /// 各区域类技能（Y3 引力场、Y1 回拉线）接入时在此累加 `p.pull`。目前为空实现。
    /// 场效应（引力场 / 回拉线）对本帧附加速度的贡献：把要移动的力累加进各玩家 `pull`。
    fn step_area_forces(&mut self, _dt: Fix64) {
        // 引力场（Y3）：把半径内存活敌人吸向场中心
        for pr in self.projectiles.iter() {
            if !pr.alive {
                continue;
            }
            match pr.kind {
                ProjectileKind::Gravity { radius, pull_speed, .. } => {
                    for p in self.players.iter_mut() {
                        if !p.alive {
                            continue;
                        }
                        let d = pr.pos - p.pos;
                        let dsq = d.length_squared();
                        if dsq > Fix64::ZERO && dsq <= (radius + p.radius) * (radius + p.radius) {
                            p.pull += d.normalized() * pull_speed;
                        }
                    }
                }
                ProjectileKind::Tether { owner, target, pull_speed, .. } => {
                    // 回拉线：把绑定目标拉向施法者（owner/target 为 u32 值绑定）
                    let from = self.players.get(owner as usize).map(|p| p.pos).unwrap_or(Vec2::ZERO);
                    if let Some(t) = self.players.get_mut(target as usize) {
                        if t.alive {
                            let d = from - t.pos;
                            let dsq = d.length_squared();
                            if dsq > Fix64::from_num(1.1) {
                                t.pull += d.normalized() * pull_speed;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// T2 扇扫连射：每个带 `sweep` 状态的玩家按心率发射一枚扇形弹，发射完清状态。
    fn step_sweep(&mut self, dt: Fix64) {
        let mut spawn: Vec<(Vec2, Vec2, Fix64, Fix64)> = Vec::new(); // (pos, dir, speed, damage)
        for p in self.players.iter_mut() {
            if !p.alive {
                continue;
            }
            if let Some(s) = &mut p.sweep {
                s.elapsed += dt.to_num::<f64>();
                let cad = s.cadence.max(1e-3);
                while s.elapsed >= cad && s.remaining > 0 {
                    s.elapsed -= cad;
                    spawn.push((p.pos, s.dir, s.bullet_speed, s.damage));
                    s.remaining -= 1;
                    s.dir = crate::fix::rotate_ccw(s.dir, Fix64::from_num(s.turn_step));
                }
                if s.remaining == 0 {
                    p.sweep = None;
                }
            }
        }
        for (pos, dir, speed, damage) in spawn {
            self.projectiles.push(Projectile {
                owner: 0,
                kind: ProjectileKind::Bullet {
                    dir,
                    speed,
                    damage,
                    radius: Fix64::from_num(0.5),
                    remaining: Fix64::from_num(SABULLET_RANGE),
                },
                pos,
                alive: true,
            });
        }
    }

    /// 玩家与圆形障碍的分离：把重叠进柱子的玩家沿圆心连线推出去（纯位置修正，无伤害）。
    fn resolve_obstacles(&mut self, _dt: Fix64) {
        if self.obstacles.is_empty() {
            return;
        }
        for p in self.players.iter_mut() {
            if !p.alive {
                continue;
            }
            let mut hit_wall = false;
            for o in self.obstacles.iter() {
                let delta = p.pos - o.pos;
                let dist_sq = delta.length_squared();
                let min = p.radius + o.radius;
                if dist_sq < min * min {
                    let dist = dist_sq.sqrt();
                    let dir = if dist == Fix64::ZERO {
                        Vec2::new(Fix64::ONE, Fix64::ZERO)
                    } else {
                        delta / dist
                    };
                    let overlap = min - dist;
                    p.pos += dir * overlap;
                    hit_wall = true;
                }
            }
            // E2b 潜行踢·连推：携带 kick 又撞到障碍 → 排一个 0.3s 后的重新踢击（若总窗口还有）。
            if hit_wall && p.ricochet_window > Fix64::ZERO && p.ricochet_kick.is_some() {
                p.ricochet_pending = Some(Fix64::from_num(0.3));
                p.kick = None; // 撞墙即消耗本次踢击，等待重新触发
            }
        }
    }

    /// 对一位玩家施加一笔伤害。有护盾 buff 先吸收，再扣真血；记录击杀来源。
    /// 玩家死亡记账：记录淘汰顺序与击杀者，供 `placement()` / `take_kills()` 用。
    ///
    /// 调用约定：victim 在此前已 `alive = false`。每个玩家只死一次
    /// （`damage_player` 早退 / `explode_at` continue 已保证），故不会重复记账。
    fn record_death(&mut self, victim: u32) {
        self.eliminated_order.push(victim);
        if let Some(k) = self.players[victim as usize].last_hit_by {
            self.kills_this_round.push((k, victim));
        }
    }

    fn damage_player(&mut self, id: u32, amount: Fix64, from: Option<u32>) {
        let died = {
            let p = &mut self.players[id as usize];
            if !p.alive {
                return;
            }
            // 4.6b：玩家造成伤害按目标护甲×法抗折算。
            let amount = if from.is_some() {
                amount * Fix64::from_num(p.armor_factor * p.spell_factor)
            } else {
                amount
            };
            if let Some(hitter) = from {
                p.last_hit_by = Some(hitter);
            }
            // C1 疾跑：boost 期间返还一半伤害回血（soak_boost 返回净扣血）
            let net = p.soak_boost(amount);
            p.hp = (p.hp - net).max(Fix64::ZERO);
            if p.hp == Fix64::ZERO {
                p.alive = false;
                true
            } else {
                false
            }
        };
        if died {
            self.record_death(id);
        }
    }

    /// 推进飞行物 / 延时区域（倒计时、弹体飞行与命中、爆炸结算、假身生命周期）。
    ///
    /// 所有变更在做完后一次性写入，避免 `projectiles` 与 `players` 的借用冲突。
    fn step_projectiles(&mut self, dt: Fix64) {
        // 本地工作副本（Projectile 是 Copy），在其上推进位移/倒计时并判定命中。
        let mut ps = std::mem::take(&mut self.projectiles);
        let n = self.players.len();
        // 撒弹线/滚动火球产出的扇形子弹收集：(owner, pos, dir, bulletspeed)
        let mut spawn: Vec<(u32, Vec2, Vec2, Fix64)> = Vec::new();
        // T3b 命中的子弹生成的回返镖：(owner, pos, dir, speed)
        let mut returners: Vec<(u32, Vec2, Vec2, Fix64)> = Vec::new();
        // 098b AoE 爆炸（陨石命中/到期）：中心 KI 全额、线性距离衰减到 20%（近似 qI 衰减）。
        let mut expiry_blasts: Vec<(u32, Vec2, Fix64, Fix64, Fix64)> = Vec::new(); // (owner, 中心, 半径, gx, ji)
        let eps = Fix64::from_num(1.0 / 65536.0);

        // 1) 推进整帧：倒计时 / 生命周期 / 弹体飞行
        for pr in ps.iter_mut() {
            match &mut pr.kind {
                ProjectileKind::Rock { fuse, .. } => {
                    *fuse -= dt;
                    if *fuse <= Fix64::ZERO {
                        pr.alive = false;
                    }
                }
                ProjectileKind::Decoy { lifetime, .. } => {
                    *lifetime -= dt;
                    if *lifetime <= Fix64::ZERO {
                        pr.alive = false;
                    }
                }
                ProjectileKind::Bullet { dir, speed, remaining, .. } => {
                    pr.pos += *dir * (*speed * dt);
                    *remaining -= *speed * dt;
                    if *remaining < eps {
                        pr.alive = false;
                    }
                }
                ProjectileKind::Rolling { dir, speed, remaining, .. } => {
                    // 滚动火球：沿定速直线滚动，范围耗尽则消失。
                    pr.pos += *dir * (*speed * dt);
                    *remaining -= *speed * dt;
                    if *remaining < eps {
                        pr.alive = false;
                    }
                }
                ProjectileKind::ScatterLine { dir, speed, remaining, scatter } => {
                    // 撒弹线：沿方向飞行；到终点(E3 Burst)或沿途(E3b Periodic)撒扇形弹。
                    pr.pos += *dir * (*speed * dt);
                    *remaining -= *speed * dt;
                    let expired = *remaining < eps;
                    if expired {
                        pr.alive = false;
                    }
                    match scatter {
                        ScatterKind::Burst { count, step_rad, bullet_speed } => {
                            // 到终点一次性撒一个扇形（从 -count/2 步进到 +count/2）
                            if expired {
                                let mut bdir = *dir;
                                bdir = crate::fix::rotate_ccw(bdir, -*step_rad * Fix64::from_num(*count as f64 / 2.0));
                                for _ in 0..*count {
                                    spawn.push((pr.owner, pr.pos, bdir, *bullet_speed));
                                    bdir = crate::fix::rotate_ccw(bdir, *step_rad);
                                }
                            }
                        }
                        ScatterKind::Periodic { interval, elapsed, bullet_speed, turn_rad, .. } => {
                            // 每 interval 撒一发，并让方向转过 turn_rad
                            *elapsed += dt;
                            while *elapsed >= *interval {
                                *elapsed -= *interval;
                                spawn.push((pr.owner, pr.pos, *dir, *bullet_speed));
                                *dir = crate::fix::rotate_ccw(*dir, *turn_rad);
                            }
                        }
                    }
                }
                ProjectileKind::Missile { dir, speed, remaining, .. } => {
                    // 追踪导弹：锁定最近敌人全速直追（原版 `velocity = dir*Speed`）
                    if let Some(tgt) = self.nearest_enemy(pr.pos, pr.owner) {
                        let want = (tgt - pr.pos).normalized();
                        *dir = if want.length_squared() == Fix64::ZERO { *dir } else { want };
                    }
                    pr.pos += *dir * (*speed * dt);
                    *remaining -= *speed * dt;
                    if *remaining < eps {
                        pr.alive = false;
                    }
                }
                ProjectileKind::Boomerang { vel, accelerate, life, owner_pos, .. } => {
                    // 回旋镖：速度矢量每帧朝施法者拉拽（原版 `velocity += (sender-pos)*a`）
                    *vel += (*owner_pos - pr.pos) * (*accelerate * dt);
                    pr.pos += *vel * dt;
                    *life -= dt;
                    if *life < eps {
                        pr.alive = false;
                    }
                }
                ProjectileKind::Banana { dir, speed, turn, life, .. } => {
                    // 双香蕉：沿方向并以固定角速度旋转（曲线飞行）
                    pr.pos += *dir * (*speed * dt);
                    *dir = crate::fix::rotate_ccw(*dir, *turn * dt);
                    *life -= dt;
                    if *life < eps {
                        pr.alive = false;
                    }
                }
                ProjectileKind::Chain { dir, speed, life, last_target, owner, turn_delay, .. } => {
                    // 链镖/跳弹：转镖先沿直线飞行 turn_delay 秒，之后才追踪最近敌人（排除上一个目标与施法者）。
                    if *turn_delay > Fix64::ZERO {
                        *turn_delay -= dt;
                    } else if let Some(tgt) = self.nearest_enemy_excl(pr.pos, *owner, *last_target) {
                        let want = (tgt - pr.pos).normalized();
                        *dir = if want.length_squared() == Fix64::ZERO { *dir } else { want };
                    }
                    pr.pos += *dir * (*speed * dt);
                    *life -= dt;
                    if *life < eps {
                        pr.alive = false;
                    }
                }
                ProjectileKind::BonusBomb { dir, speed, remaining, .. } => {
                    // 蓄力炸弹：直线飞行，射程耗尽则消失
                    pr.pos += *dir * (*speed * dt);
                    *remaining -= *speed * dt;
                    if *remaining < eps {
                        pr.alive = false;
                    }
                }
                ProjectileKind::Returner { dir, speed, owner, .. } => {
                    // 回返镖：全速飞回施法者
                    if let Some(p) = self.players.get(*owner as usize) {
                        if p.alive {
                            let want = (p.pos - pr.pos).normalized();
                            *dir = if want.length_squared() == Fix64::ZERO { *dir } else { want };
                        }
                    }
                    pr.pos += *dir * (*speed * dt);
                }
                ProjectileKind::Tether { remaining, .. } => {
                    *remaining -= dt;
                    if *remaining < eps {
                        pr.alive = false;
                    }
                }
                ProjectileKind::Gravity { dir, speed, remaining, .. } => {
                    // 引力场缓慢前移，随后原地鼓动（简化：只前移一小段后停住）
                    pr.pos += *dir * (*speed * dt * Fix64::from_num(0.5));
                    *remaining -= dt;
                    if *remaining < eps {
                        pr.alive = false;
                    }
                }
                ProjectileKind::Star { remaining, .. } => {
                    *remaining -= dt;
                    if *remaining < eps {
                        pr.alive = false;
                    }
                }
                ProjectileKind::BindLine { dir, speed, fired, count, end, .. } => {
                    // 束缚线：后一点向前收拢（简化：从起点向 end 移动一点）
                    if *fired < *count {
                        pr.pos += *dir * (*speed * dt);
                        let d = *end - pr.pos;
                        if d.length_squared() <= (*speed * dt) * (*speed * dt) {
                            *fired = *count; // 收拢到位
                        }
                    }
                }
                ProjectileKind::PushBullet { dir, speed, remaining, .. } => {
                    // 撞击迟缓弹：直线飞行
                    pr.pos += *dir * (*speed * dt);
                    *remaining -= *speed * dt;
                    if *remaining < eps {
                        pr.alive = false;
                    }
                }
                ProjectileKind::W098b { proj, vel, speed, remaining, life, blast, target, returning, gx, kb_ji, .. } => {
                    // 098b 弹体运动学：Straight/Bounce 直线（Bounce 的重定向在命中分支做）；
                    // Homing 全速直追锁定目标；Boomerang 出程恒速、过半程后朝施法者当前位置回拉。
                    // 到期时带 blast 的弹体（陨石）在原地爆炸。
                    *remaining -= dt;
                    if *remaining <= Fix64::ZERO {
                        pr.alive = false;
                        if let Some(br) = blast {
                            expiry_blasts.push((pr.owner, pr.pos, *br, *gx, *kb_ji));
                        }
                    }
                    match proj {
                        crate::skill::W098bProjKind::Straight | crate::skill::W098bProjKind::Bounce => {
                            pr.pos += *vel * dt;
                        }
                        crate::skill::W098bProjKind::Homing => {
                            // 朝锁定目标全速直追；目标死亡则保持当前方向直线飞完剩余寿命。
                            if let Some(tid) = target {
                                if let Some(t) = self.players.get(*tid as usize) {
                                    if t.alive {
                                        let d = t.pos - pr.pos;
                                        if d.length() > Fix64::ZERO {
                                            *vel = d.normalized() * *speed;
                                        }
                                    }
                                }
                            }
                            pr.pos += *vel * dt;
                        }
                        crate::skill::W098bProjKind::Boomerang => {
                            let owner_pos = self
                                .players
                                .get(pr.owner as usize)
                                .map(|o| o.pos)
                                .unwrap_or(pr.pos);
                            if !*returning && *remaining <= *life / Fix64::from_num(2.0) {
                                *returning = true;
                            }
                            if *returning {
                                // 回程：持续朝施法者当前位置加速回飞；回到附近即收回（销毁）。
                                let d = owner_pos - pr.pos;
                                let dist = d.length();
                                if dist < Fix64::from_num(60.0) {
                                    pr.alive = false;
                                } else if dist > Fix64::ZERO {
                                    let back = (*speed * Fix64::from_num(1.5)).max(vel.length());
                                    *vel = d.normalized() * back;
                                }
                            }
                            pr.pos += *vel * dt;
                        }
                    }
                }
                ProjectileKind::Beam { remaining, .. } => {
                    *remaining -= dt;
                    if *remaining <= Fix64::ZERO {
                        pr.alive = false;
                    }
                }
            }
        }

        // 1b) 弹体撞障碍（柱子）。
        // 旧代码只判了回旋镖，导致火球/滚动火球/导弹/香蕉等**直接穿过柱子**打到后面的人。
        // 现在对所有「会飞行的弹体」统一判定：
        //   - 回旋镖：沿接触法线镜向反弹（原版 BoomerangScript 撞墙 MirrorBy），保留原有手感；
        //   - 其余：被柱子**挡下并消失**（不爆炸、不穿过）。
        // 不参与判定的类型见 `ProjectileKind::obstacle_radius`。
        for pr in ps.iter_mut() {
            if !pr.alive {
                continue;
            }
            let Some(radius) = pr.kind.obstacle_radius() else {
                continue;
            };
            for o in self.obstacles.iter() {
                let delta = pr.pos - o.pos;
                let dist = delta.length();
                let min = radius + o.radius;
                if dist > Fix64::ZERO && dist < min {
                    let normal = delta / dist;
                    if let ProjectileKind::Boomerang { vel, .. } = &mut pr.kind {
                        *vel = crate::fix::mirror_by(*vel, normal);
                        pr.pos = o.pos + normal * min; // 推出柱面，避免下帧仍重叠而反复反弹
                    } else if let ProjectileKind::W098b { proj: crate::skill::W098bProjKind::Boomerang, vel, .. } =
                        &mut pr.kind
                    {
                        // 098b 回旋镖撞柱反弹（与 D2 原型同手感）；Straight/Homing 被柱子挡下消失。
                        *vel = crate::fix::mirror_by(*vel, normal);
                        pr.pos = o.pos + normal * min;
                    } else {
                        pr.alive = false; // 被柱子挡下：直接消失
                    }
                    break;
                }
            }
        }

        // 2) 判定与收集对玩家的影响：命中伤害 / AOE / 持续伤害 / 爆炸。
        // 每个 (伤害, 来源) 事件在 4) 统一结算；被反弹护盾命中的直射弹只反射方向。
        let mut events: Vec<(u32, Fix64, Option<u32>)> = Vec::new();
        let mut explode: Vec<ProjExplosion> = Vec::new();
        let mut pushes: Vec<(u32, Vec2, f64)> = Vec::new(); // (受害者 id, 击退方向, 时长)
        let mut reflect_bullets: Vec<(usize, Vec2)> = Vec::new(); // (proj 下标, 反射后的 dir)
        // 098b 弹跳弹重定向：(proj 下标, 本次受害者, 衰减后 gx, 朝下一目标的速度)。
        let mut bounce_redirs: Vec<(usize, u32, Fix64, Vec2)> = Vec::new();
        // 098b 命中点燃场（S003/S004 无）：命中处生成 2.5s DoT 区域（复用 Star 的区域伤害逻辑）。
        let mut ignites: Vec<(u32, Vec2, Fix64)> = Vec::new(); // (owner, 命中点, DoT 总量)

        for (pi, pr) in ps.iter_mut().enumerate() {
            if !pr.alive {
                // 倒计时耗尽：石头原地爆炸
                if let ProjectileKind::Rock { radius, damage, bomb_force, .. } = pr.kind {
                    explode.push(ProjExplosion {
                        pos: pr.pos,
                        owner: pr.owner,
                        radius,
                        damage,
                        bomb_force,
                    });
                }
                // 蓄力炸弹射程耗尽未命中 → 施法者 damageplus 归零（原版 JumbScript.OnDestroy　!bonus）
                if let ProjectileKind::BonusBomb { owner, .. } = pr.kind {
                    if let Some(p) = self.players.get_mut(owner as usize) {
                        p.damageplus = 0.0;
                    }
                }
                continue;
            }
            // 链镖/跳弹族单独处理（需改动 Chain 内部状态以完成跳跃，交给可变分支）
            if matches!(pr.kind, ProjectileKind::Chain { .. }) {
                if let ProjectileKind::Chain { damage, heal, ratio, ratio_decay, life, last_target, owner, max_chain, hit_count, turn_delay, .. } = &mut pr.kind {
                    let lt = *last_target;
                    if let Some((victim, _dd)) =
                        nearest_hit_with_skip(&self.players, pr.pos, *owner, Fix64::from_num(0.6), lt)
                    {
                        let dmg = *damage * *ratio;
                        events.push((victim, dmg, Some(*owner)));
                        if *heal > Fix64::ZERO {
                            if let Some(p) = self.players.get_mut(*owner as usize) {
                                if p.alive {
                                    let healed = (p.max_hp - p.hp).min(*heal);
                                    p.hp += healed;
                                }
                            }
                        }
                        // T3b 蓄力：命中一次 +0.3
                        if *heal == Fix64::ZERO && *ratio_decay == Fix64::ZERO {
                            if let Some(p) = self.players.get_mut(*owner as usize) {
                                p.damageplus += 0.3;
                            }
                        }
                        let next_ratio = *ratio - *ratio_decay;
                        *last_target = victim;
                        *hit_count += 1;
                        *turn_delay = Fix64::ZERO; // 已命中，后续跳跃直接追踪
                        // 命中后：伤害倍率衰减到 0 或链跳数达上限 → 消失；否则继续跳（重置生命/衰减倍率）。
                        // 修复“吸血/跳弹无限往返”：max_chain 硬上限，加上不再无条件重置 life 也能自然耗尽。
                        let dead = next_ratio <= Fix64::ZERO || *hit_count >= *max_chain;
                        if dead {
                            pr.alive = false;
                        } else {
                            *ratio = next_ratio;
                            *life = Fix64::from_num(1.5);
                        }
                    }
                }
                continue;
            }
            // 回返镖（T3b）：回到施法者身边则刷新其 cd 并自毁；顺路命中其他敌人则伤+推
            if matches!(pr.kind, ProjectileKind::Returner { .. }) {
                let is_returner = matches!(pr.kind, ProjectileKind::Returner { .. });
                let _ = is_returner;
                let (owner, radius) = match pr.kind {
                    ProjectileKind::Returner { owner, radius, damage, push_power, push_time, .. } => {
                        // 碰到施法者
                        if let Some(p) = self.players.get(owner as usize) {
                            if p.alive {
                                let rr = radius + p.radius;
                                if (p.pos - pr.pos).length_squared() <= rr * rr {
                                    if let Some(po) = self.players.get_mut(owner as usize) {
                                        po.caster.reset_cooldown(crate::skill::SkillId::T3Fast2);
                                    }
                                    pr.alive = false;
                                }
                            }
                        }
                        // 顺路命中敌人
                        if let Some((victim, dd)) = nearest_hit(&self.players, pr.pos, owner, radius) {
                            events.push((victim, damage, Some(owner)));
                            if dd.length_squared() > Fix64::ZERO {
                                pushes.push((victim, dd.normalized() * push_power, push_time.to_num::<f64>()));
                            }
                        }
                        (owner, radius)
                    }
                    _ => unreachable!(),
                };
                let _ = (owner, radius);
                continue;
            }
            match &pr.kind {
                ProjectileKind::Bullet { dir, damage, radius, .. } => {
                    // 直射弹：命中最近的目标 → 若无反弹护盾则消耗弹体并结算伤害；有护盾则反射弹体。
                    let mut best: Option<(Fix64, u32, bool)> = None; // (d_sq, victim, has_reflect)
                    for j in 0..n {
                        let p = &self.players[j];
                        if !p.alive || p.id == pr.owner {
                            continue;
                        }
                        let rr = radius + p.radius;
                        let d_sq = (p.pos - pr.pos).length_squared();
                        if d_sq <= rr * rr && best.map(|(bd, _, _)| d_sq < bd).unwrap_or(true) {
                            best = Some((d_sq, p.id, p.shield()));
                        }
                    }
                    if let Some((_, victim, has_reflect)) = best {
                        if has_reflect {
                            // 反射：法线 = (弹体位置 - 受害者位置) 指向受害者，把 dir 镜向。
                            let normal = self.players[victim as usize].pos - pr.pos;
                            reflect_bullets.push((pi, crate::fix::mirror_by(*dir, normal)));
                        } else {
                            pr.alive = false;
                            events.push((victim, *damage, Some(pr.owner)));
                        }
                    }
                }
                ProjectileKind::Missile { damage, radius, push_power, .. } => {
                    // 导弹：命中即爆炸伤+击退
                    let mut hit_any = false;
                    for j in 0..n {
                        let p = &self.players[j];
                        if !p.alive || p.id == pr.owner {
                            continue;
                        }
                        if (p.pos - pr.pos).length_squared() <= *radius * *radius {
                            hit_any = true;
                            break;
                        }
                    }
                    if hit_any {
                        pr.alive = false;
                        explode.push(ProjExplosion {
                            pos: pr.pos,
                            owner: pr.owner,
                            radius: *radius,
                            damage: *damage,
                            bomb_force: *push_power,
                        });
                    }
                }
                ProjectileKind::Boomerang { damage, radius, push_time, push_power, .. } => {
                    // 回旋镖：命中最近玩家 → 直接伤害 + 沿弹体方向击退（原版 BombExplode）
                    if let Some((victim, dd)) = nearest_hit(&self.players, pr.pos, pr.owner, *radius) {
                        pr.alive = false;
                        events.push((victim, *damage, Some(pr.owner)));
                        if dd.length_squared() > Fix64::ZERO {
                            pushes.push((victim, dd.normalized() * *push_power, push_time.to_num::<f64>()));
                        }
                    }
                }
                ProjectileKind::Banana { damage, radius, push_time, push_power, .. } => {
                    // 香蕉弹：命中即直接伤害 + 击退
                    if let Some((victim, dd)) = nearest_hit(&self.players, pr.pos, pr.owner, *radius) {
                        pr.alive = false;
                        events.push((victim, *damage, Some(pr.owner)));
                        if dd.length_squared() > Fix64::ZERO {
                            pushes.push((victim, dd.normalized() * *push_power, push_time.to_num::<f64>()));
                        }
                    }
                }
                ProjectileKind::Rolling { dir, damage_per_sec, radius, .. } => {
                    // 滚动火球：覆盖到的敌人每帧持续掉血（DoT）
                    for j in 0..n {
                        let p = &self.players[j];
                        if !p.alive || p.id == pr.owner {
                            continue;
                        }
                        let rr = *radius + p.radius;
                        if (p.pos - pr.pos).length_squared() <= rr * rr {
                            events.push((p.id, *damage_per_sec * dt, Some(pr.owner)));
                        }
                    }
                    let _ = dir;
                }
                ProjectileKind::Beam { dir, length, width, damage_per_sec, .. } => {
                    // 持续伤害线：对线段内敌人造成每帧伤害
                    for j in 0..n {
                        let p = &self.players[j];
                        if !p.alive || p.id == pr.owner {
                            continue;
                        }
                        let rel = p.pos - pr.pos;
                        let along = rel.dot(*dir);
                        if along > Fix64::ZERO && along <= *length {
                            let perp = (rel - *dir * along).length();
                            if perp <= *width + p.radius {
                                events.push((p.id, *damage_per_sec * dt, Some(pr.owner)));
                            }
                        }
                    }
                }
                ProjectileKind::BonusBomb { damage, radius, push_power, push_time, owner, .. } => {
                    // 蓄力炸弹命中：伤+推+damageplus+生成回返镖
                    if let Some((victim, dd)) = nearest_hit(&self.players, pr.pos, *owner, *radius) {
                        pr.alive = false;
                        events.push((victim, *damage, Some(*owner)));
                        if dd.length_squared() > Fix64::ZERO {
                            pushes.push((victim, dd.normalized() * *push_power, push_time.to_num::<f64>()));
                        }
                        if let Some(p) = self.players.get_mut(*owner as usize) {
                            p.damageplus += 0.3;
                        }
                        // 原地生成回返镖（朝 owner 方向）
                        let back = self.players[*owner as usize].pos - pr.pos;
                        let bdir = if back.length_squared() == Fix64::ZERO { Vec2::new(Fix64::ONE, Fix64::ZERO) } else { back.normalized() };
                        returners.push((*owner, pr.pos, bdir, Fix64::from_num(14.0)));
                    }
                }
                ProjectileKind::Tether { owner, target, damage_per_sec, beam, .. } => {
                    // 回拉线：绑定目标持续掉血（伤害已含进 pull）；beam=Y1b 沿路径扫射
                    events.push((*target, *damage_per_sec * dt, Some(*owner)));
                    if *beam {
                        // 沿施法者→目标线段扫射经过的所有敌人
                        let from = self.players.get(*owner as usize).map(|p| p.pos).unwrap_or(Vec2::ZERO);
                        let to = self.players.get(*target as usize).map(|p| p.pos).unwrap_or(from);
                        for j in 0..n {
                            let p = &self.players[j];
                            if !p.alive || p.id == *owner || p.id == *target {
                                continue;
                            }
                            if point_near_segment(p.pos, from, to, p.radius) {
                                events.push((p.id, *damage_per_sec * dt, Some(*owner)));
                            }
                        }
                    }
                }
                ProjectileKind::Star { owner, radius, damage_per_sec, heal_per_sec, .. } => {
                    // 星域：范围内敌掉血、对施法者回血
                    for j in 0..n {
                        let p = &self.players[j];
                        if !p.alive {
                            continue;
                        }
                        let rr = *radius + p.radius;
                        if (p.pos - pr.pos).length_squared() <= rr * rr && p.id != *owner {
                            events.push((p.id, *damage_per_sec * dt, Some(*owner)));
                        }
                    }
                    if let Some(o) = self.players.get_mut(*owner as usize) {
                        if o.alive {
                            o.hp = (o.hp + *heal_per_sec * dt).min(o.max_hp);
                        }
                    }
                }
                ProjectileKind::BindLine { bind_time, from, end, .. } => {
                    // 束缚线：起点到终点整条线上的敌人被束缚（禁施法）
                    let mut to_bind: Vec<u32> = Vec::new();
                    for j in 0..n {
                        let p = &self.players[j];
                        if !p.alive {
                            continue;
                        }
                        // 判定余量（旧尺度 0.2 半径 → war3 尺度 ×16，过渡换算见 PORT_098B_DECISIONS.md D4）
                        if point_near_segment(p.pos, *from, *end, p.radius + Fix64::from_num(0.2 * 16.0)) {
                            to_bind.push(p.id);
                        }
                    }
                    for id in to_bind {
                        if let Some(pp) = self.players.get_mut(id as usize) {
                            pp.add_buff(BuffKind::Tied, bind_time.to_num::<f64>());
                        }
                    }
                }
                ProjectileKind::PushBullet { damage, radius, push_power, push_time, .. } => {
                    // 撞击迟缓弹：命中最近玩家 → 伤害 + 沿弹-目标方向强推 push_time
                    if let Some((victim, dd)) = nearest_hit(&self.players, pr.pos, pr.owner, *radius) {
                        pr.alive = false;
                        events.push((victim, *damage, Some(pr.owner)));
                        if dd.length_squared() > Fix64::ZERO {
                            pushes.push((victim, dd.normalized() * *push_power, push_time.to_num::<f64>()));
                        }
                    }
                }
                ProjectileKind::W098b { proj, radius, gx, kb_ji, ignite, blast, target, speed, .. } => {
                    // 098b 弹体命中：KI/FI 结算（PORT_098B_DECISIONS.md D3/M1）——
                    // FI 伤害 = gx × Gn[攻] × hn[守]（M1 Gn/hn=1，框架位预留）；
                    // KI 击退初速 = DAMAGE_BASE × gx × kb_ji（JI 系数），方向沿弹-目标连线。
                    // Bounce 命中判定排除上一跳受害者（target）——重定向瞬间还贴着旧目标，
                    // 不排除会每帧重复结算同一目标刷伤害。
                    let hit = if *proj == crate::skill::W098bProjKind::Bounce {
                        nearest_hit_with_skip(&self.players, pr.pos, pr.owner, *radius, target.unwrap_or(pr.owner))
                    } else {
                        nearest_hit(&self.players, pr.pos, pr.owner, *radius)
                    };
                    if let Some((victim, dd)) = hit {
                        let skip = *target;
                        events.push((victim, *gx, Some(pr.owner)));
                        if dd.length_squared() > Fix64::ZERO {
                            let kb = warlock_ki_knockback(*gx, *kb_ji);
                            pushes.push((victim, dd.normalized() * kb, W098B_KB_TIME));
                        }
                        if let Some(total) = ignite {
                            ignites.push((pr.owner, pr.pos, *total));
                        }
                        if let Some(br) = blast {
                            expiry_blasts.push((pr.owner, pr.pos, *br, *gx, *kb_ji));
                        }
                        // Bounce（S016 弹跳弹）：命中不消失——伤害 ×0.8（下限 0.2），
                        // 重定向到**全场**最近的「非 owner、非上一跳目标」敌人（不限判定半径——
                        // 半径内扫描会因 or_else 兜底重新选中贴脸的旧目标，弹永远到不了下一家）；
                        // 无新目标才消失。（重定向经 bounce_redirs 在 2c 段统一写回。）
                        if *proj == crate::skill::W098bProjKind::Bounce {
                            let new_gx = (*gx * Fix64::from_num(0.8)).max(Fix64::from_num(0.2));
                            let skip_id = skip.unwrap_or(victim);
                            let mut best: Option<(Fix64, u32)> = None;
                            for q in self.players.iter() {
                                if !q.alive || q.id == pr.owner || q.id == skip_id {
                                    continue;
                                }
                                let ds = (q.pos - pr.pos).length_squared();
                                if best.map(|(b, _)| ds < b).unwrap_or(true) {
                                    best = Some((ds, q.id));
                                }
                            }
                            match best {
                                Some((_, nid)) => {
                                    let ndd = self.players[nid as usize].pos - pr.pos;
                                    if ndd.length_squared() > Fix64::ZERO {
                                        bounce_redirs.push((pi, victim, new_gx, ndd.normalized() * *speed));
                                        // 不置 alive=false：继续飞向下一目标
                                    } else {
                                        pr.alive = false;
                                    }
                                }
                                None => pr.alive = false, // 无下一目标：消失
                            }
                        } else {
                            pr.alive = false;
                        }
                    }
                }
                ProjectileKind::Rock { .. } | ProjectileKind::Decoy { .. } | ProjectileKind::ScatterLine { .. } | ProjectileKind::Chain { .. } | ProjectileKind::Returner { .. } | ProjectileKind::Gravity { .. } => {}
            }
        }

        // 2b) 应用反弹护盾对直射弹的反射（改方向，不消耗、不伤害）。
        for (pi, new_dir) in reflect_bullets {
            if let ProjectileKind::Bullet { dir, .. } = &mut ps[pi].kind {
                *dir = new_dir;
                // 可让被反射的弹体仍归属原施法者（原版弹一次）
            }
        }

        // 2c) 应用 098b 弹跳弹的重定向（衰减后的 gx、朝下一目标的速度、记录上一跳受害者）。
        // 098b 弹跳弹的 life 是**单跳飞行时间**（spec ev），故每跳重置寿命。
        for (pi, last_victim, new_gx, new_vel) in bounce_redirs {
            if let ProjectileKind::W098b { gx, vel, target, remaining, life, .. } = &mut ps[pi].kind {
                *gx = new_gx;
                *vel = new_vel;
                *target = Some(last_victim); // 下一跳跳过本次受害者
                *remaining = *life; // 单跳寿命重置（ev 语义）
            }
        }

        // 3) 结算爆炸（石头 / 导弹）
        for e in &explode {
            self.explode_at(e.pos, e.owner, e.radius, e.damage, e.bomb_force);
        }

        // 4) 结算命中/持续伤害（受护盾吸收、记录击杀来源）
        for (victim, amount, from) in events {
            self.damage_player(victim, amount, from);
        }
        // 4a) 结算弹体直接命中的击退（回旋镖 / 香蕉）
        for (victim, vel, time) in pushes {
            if let Some(p) = self.players.get_mut(victim as usize) {
                if p.alive {
                    p.push(vel, time); // 击退抗性在 Player::push 内统一按 kb_factor 缩放
                }
            }
        }

        // 4b) 应用撒弹线/扇形弹的产出（作为新的直射 Bullet 加入）
        for (owner, pos, dir, bspeed) in spawn.drain(..) {
            ps.push(Projectile {
                owner,
                kind: ProjectileKind::Bullet {
                    dir,
                    speed: bspeed,
                    damage: Fix64::from_num(SABULLET_DAMAGE),
                    radius: Fix64::from_num(0.6),
                    remaining: Fix64::from_num(SABULLET_RANGE),
                },
                pos,
                alive: true,
            });
        }
        // 4c) 应用蓄力炸弹生成的回返镖
        for (owner, pos, dir, speed) in returners.drain(..) {
            ps.push(Projectile {
                owner,
                kind: ProjectileKind::Returner {
                    dir,
                    speed,
                    damage: Fix64::from_num(0.0),
                    radius: Fix64::from_num(0.6),
                    push_power: Fix64::from_num(5.0),
                    push_time: Fix64::from_num(1.0),
                    owner,
                },
                pos,
                alive: true,
            });
        }
        // 4d-0) 098b AoE 爆炸（陨石命中/到期）：复用 explode_at（中心伤害+距离衰减+连线击退），
        // 伤害=gx（explode_at 内部做护甲折算），击退力=KI 公式 warlock_ki_knockback。
        for (owner, center, br, gx, ji) in expiry_blasts.drain(..) {
            self.explode_at(center, owner, br, gx, warlock_ki_knockback(gx, ji));
        }
        // 4d) 098b 命中点燃场（S000 火球 xc）：命中处半径 75（spec aoe_radius_obj）、
        // 时长 2.5s（consolidated：2.5×jn），总量均摊为 DPS。复用 Star 的静态区域伤害。
        for (owner, pos, total) in ignites.drain(..) {
            ps.push(Projectile {
                owner,
                kind: ProjectileKind::Star {
                    owner,
                    radius: Fix64::from_num(75.0),
                    damage_per_sec: total / Fix64::from_num(W098B_IGNITE_SECONDS),
                    heal_per_sec: Fix64::ZERO,
                    remaining: Fix64::from_num(W098B_IGNITE_SECONDS),
                },
                pos,
                alive: true,
            });
        }

        // 5) 写回并清除已死亡/失效的弹体
        ps.retain(|p| p.alive);
        self.projectiles = ps;
    }

    /// 找到离某个位置最近、且不是 `owner` 的存活玩家。
    fn nearest_enemy(&self, pos: Vec2, owner: u32) -> Option<Vec2> {
        let mut best: Option<(Fix64, Vec2)> = None;
        for p in self.players.iter() {
            if !p.alive || p.id == owner {
                continue;
            }
            let d = (p.pos - pos).length_squared();
            if best.map(|(bd, _)| d < bd).unwrap_or(true) {
                best = Some((d, p.pos));
            }
        }
        best.map(|(_, v)| v)
    }

    /// 距 `anchor` 最近的非 `owner` 存活玩家（用于 D3 导弹锁定点击处最近目标）。
    fn nearest_other_enemy(&self, anchor: Vec2, owner: u32) -> Option<Vec2> {
        let mut best: Option<(Fix64, Vec2)> = None;
        for p in self.players.iter() {
            if !p.alive || p.id == owner {
                continue;
            }
            let d = (p.pos - anchor).length_squared();
            if best.map(|(bd, _)| d < bd).unwrap_or(true) {
                best = Some((d, p.pos));
            }
        }
        best.map(|(_, v)| v)
    }

    /// 距 `pos` 最近的非 `owner`、且 id != `skip` 的存活玩家（供链镖跳跃用）。
    fn nearest_enemy_excl(&self, pos: Vec2, owner: u32, skip: u32) -> Option<Vec2> {
        let mut best: Option<(Fix64, Vec2)> = None;
        for p in self.players.iter() {
            if !p.alive || p.id == owner || p.id == skip {
                continue;
            }
            let d = (p.pos - pos).length_squared();
            if best.map(|(bd, _)| d < bd).unwrap_or(true) {
                best = Some((d, p.pos));
            }
        }
        best.map(|(_, v)| v)
    }

    /// 从 `origin` 沿单位方向 `dir` 以 `max_dist` 做射线，找最近的圆形障碍/其他玩家（排除 `owner`）。
    ///
    /// 返回命中点（落在该圆表面的位置）。无命中时返回 `None`。
    /// 用于 R3b 闪到墙、以及将来的阻挡判定。
    fn raycast_first(&self, origin: Vec2, dir: Vec2, max_dist: Fix64, owner: u32) -> Option<Vec2> {
        let mut best_t: Option<Fix64> = None;
        for o in self.obstacles.iter() {
            if let Some(t) = ray_circle_hit(origin, dir, o.pos, o.radius) {
                if t <= max_dist && best_t.map(|bt| t < bt).unwrap_or(true) {
                    best_t = Some(t);
                }
            }
        }
        for p in self.players.iter() {
            if !p.alive || p.id == owner {
                continue;
            }
            if let Some(t) = ray_circle_hit(origin, dir, p.pos, p.radius) {
                if t <= max_dist && best_t.map(|bt| t < bt).unwrap_or(true) {
                    best_t = Some(t);
                }
            }
        }
        best_t.map(|t| origin + dir * t) // 命中点（圆表面）
    }

    /// 在 (pos) 处半径 `radius` 的爆炸：对范围内玩家造成伤害并按中心连线击退。
    fn explode_at(&mut self, pos: Vec2, owner: u32, radius: Fix64, damage: Fix64, bomb_force: Fix64) {
        let r_sq = radius * radius;
        let mut deaths: Vec<u32> = Vec::new();
        for p in self.players.iter_mut() {
            if !p.alive {
                continue;
            }
            let d = p.pos - pos;
            let d_sq = d.length_squared();
            if d_sq <= r_sq {
                // 受伤（记录击杀者）；boost 期间返还一半回血；护甲/法抗折算玩家造成伤害。
                if p.id != owner {
                    p.last_hit_by = Some(owner);
                }
                let dmg = damage * Fix64::from_num(p.armor_factor * p.spell_factor);
                let net = p.soak_boost(dmg);
                p.hp = (p.hp - net).max(Fix64::ZERO);
                if p.hp == Fix64::ZERO {
                    p.alive = false;
                    deaths.push(p.id);
                }
                // 击退（沿中心连线远离，随距离衰减；走控制/强制速度；击退抗性在 push 内统一折算）
                if d_sq > Fix64::ZERO {
                    let dist = d_sq.sqrt();
                    let falloff = (Fix64::ONE - dist / radius).max(Fix64::from_num(0.2));
                    let dir = d.normalized();
                    p.push(dir * (bomb_force * falloff), 0.3);
                }
            }
        }
        // 循环外记账，避免在 iter_mut 借用期间再借 self。
        for victim in deaths {
            self.record_death(victim);
        }
    }

    /// 死亡判定辅助：场上还存活多少玩家。
    pub fn alive_count(&self) -> usize {
        self.players.iter().filter(|p| p.alive).count()
    }

    /// 本局结束后的名次：`placement[i]` = 名次 i+1 的玩家 id（1=冠军）。
    ///
    /// 规则：按淘汰先后倒序（先死的名次靠后），最后仍存活的是冠军。
    pub fn placement(&self) -> Vec<u32> {
        let n = self.players.len();
        let mut list: Vec<u32> = self.eliminated_order.clone(); // 先死在前
        list.reverse(); // 改为 最后死在前（冠军在最前）
        // 存活者（未淘汰）：按 id 顺序排在前面（冠军必然是唯一存活者或最后死的）
        let mut alive: Vec<u32> = self
            .players
            .iter()
            .filter(|p| p.alive)
            .map(|p| p.id)
            .collect();
        alive.sort();
        list.splice(0..0, alive);
        // 若仍有玩家未被记录（理论上不会），补齐
        while list.len() < n {
            list.push(u32::MAX);
        }
        list
    }

    /// 取走本局统计到的击杀记录（供 meta 层结算），并清空。
    pub fn take_kills(&mut self) -> Vec<(u32, u32)> {
        std::mem::take(&mut self.kills_this_round)
    }

    /// 本局是否已结束（只剩 0 或 1 名存活）。试验场永不判结束。
    pub fn round_over(&self) -> bool {
        if self.sandbox {
            return false;
        }
        self.alive_count() <= 1
    }

    /// 重置为可开始下一小局（清空本局状态、重设玩家满血与初始位置）。
    /// 调用方需在结算完成后调用。
    pub fn reset_round(&mut self) {
        self.eliminated_order.clear();
        self.kills_this_round.clear();
        self.projectiles.clear(); // 清掉上轮遗留的飞行物/延时区域
        self.arena_radius = Fix64::from_num(crate::world::START_RADIUS);
        self.time = Fix64::ZERO;
        // 每轮推进布局种子 → 下一小局的柱子配置与上一轮不同（联机下两端 world 同步此字段，确定性一致）。
        // 用简单递增而非 LCG：LCG 在 2^64 上存在短周期点（如 20260812 经两次递推回到自身），
        // 递增保证每次严格不同（无回绕时）。Rng::new 为单射，不同 seed ⇒ 不同布局。
        self.round_seed = self.round_seed.wrapping_add(1);
        let mut rng = Rng::new(self.round_seed);
        // 把玩家放回出生环（0.6*arena 等分 + 整体随机旋转），与 World::new 初始布局一致。
        let spawn_rot = Fix64::from_num(std::f64::consts::TAU) * rng.next_fix();
        let n = self.players.len().max(1) as f64;
        for (id, p) in self.players.iter_mut().enumerate() {
            p.reset_state();
            let r = self.arena_radius * Fix64::from_num(0.6);
            let angle = spawn_rot
                + Fix64::from_num(std::f64::consts::TAU) * Fix64::from_num(id as f64 / n);
            p.pos = Vec2::new(r * crate::fix::cos(angle), r * crate::fix::sin(angle));
        }
        self.obstacles.clear();
        _layout_obstacles(&mut self.obstacles, &mut rng, self.arena_radius);
    }
}

/// 用确定性 RNG 布柱子（圆形障碍）：在场地内圈的一个圆环上**基本均匀**分布。
///
/// “每轮不同”来自三处随机：整环随机旋转、环半径小幅波动、每根半径随机；
/// 但保持等分角距 + 小抖动，因此仍是明显的环状均匀分布。
/// 等分角距足够大，天然保证柱子之间不重叠（最小圆心距 ≫ 半径和），
/// 且环半径上限使柱子不碰玩家出生环（arena*0.6）、也不出界。
/// 每轮柱子数量随机（0~5，可为 0 = 无柱子）。
fn _layout_obstacles(out: &mut Vec<Obstacle>, rng: &mut Rng, arena_radius: Fix64) {
    // 每轮柱子数量随机 0~5（0 = 本场无柱子），也由 round_seed 驱动、随轮次变化。
    let count = rng.next_u64_below(6) as usize;
    if count == 0 {
        return;
    }
    // 环半径围绕 0.4*arena 小幅波动（0.36~0.44），决定整环大小。
    let ring_r = arena_radius * (Fix64::from_num(0.36) + rng.next_fix() * Fix64::from_num(0.08));
    // 整环随机旋转 → 每轮布局都不同，但仍保持环状均匀。
    let rot = Fix64::from_num(std::f64::consts::TAU) * rng.next_fix();
    // 每根柱子相对等分角的小抖动（保持“基本均匀”）。
    let jitter = Fix64::from_num(0.15);
    let min_r = Fix64::from_num(1.1);
    let max_r = Fix64::from_num(1.6);
    for i in 0..count {
        let base = rot
            + Fix64::from_num(std::f64::consts::TAU) * Fix64::from_num(i as f64 / count as f64);
        let angle = base + (rng.next_fix() - Fix64::from_num(0.5)) * jitter * Fix64::from_num(2);
        let pos = Vec2::new(ring_r * crate::fix::cos(angle), ring_r * crate::fix::sin(angle));
        let r = min_r + (max_r - min_r) * rng.next_fix();
        out.push(Obstacle::new(pos, r.to_num::<f64>()));
    }
}

/// 执行一位玩家施法完成后产生的效果。
///
/// `queue` 中 `u32` 为玩家索引（当前阶段玩家索引 == id）。
/// 效果可能是：瞬移 / 加速 / 生成 Projectile 等。
fn execute_effects(world: &mut World, queue: &[(u32, SkillId, Option<Vec2>)]) {
    for &(idx, id, target) in queue {
        let def = DefTable::def(id);
        let caster_level = {
            let p = &world.players[idx as usize];
            p.skill_level(id)
        };
        let stats = def.stats_at(caster_level);

        match def.effect {
            SkillEffect::Boost { duration } => {
                // C1 疾跑：开启生命偷取 buff（受击返半 + 移速成长），持续 duration。
                if let Some(p) = world.players.get_mut(idx as usize) {
                    p.add_buff(BuffKind::Boost, duration.to_num::<f64>().max(0.01));
                }
            }
            SkillEffect::ReflectShield { duration } => {
                // C2 护盾：开启反弹 buff（不吸收，撞上来的弹体/玩家被镜向反射）。
                if let Some(p) = world.players.get_mut(idx as usize) {
                    p.add_buff(BuffKind::Reflect, duration.to_num::<f64>().max(0.01));
                }
            }
            SkillEffect::Bullet { speed, damage, radius, range } => {
                // 直射弹：朝目标方向飞出（无目标时朝当前朝向，这里给正 X 用占位）。
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = match target {
                        Some(t) => {
                            let d = t - p.pos;
                            if d.length() > Fix64::ZERO {
                                d.normalized()
                            } else {
                                Vec2::new(Fix64::ONE, Fix64::ZERO)
                            }
                        }
                        None => Vec2::new(Fix64::ONE, Fix64::ZERO),
                    };
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::Bullet {
                            dir,
                            speed,
                            damage,
                            radius,
                            remaining: range,
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::Warlock098b { proj, speed, radius, life, kb_ji, ignite, blast, count, spread_step } => {
                // 098b 名册弹体（M2 批次A 扩展：blast AoE/锥形连发）。
                // gx/点燃总量走 stats（growth.damage/extra 已按等级求值）；
                // 命中结算统一走 KI/FI（FI 伤害=gx×Gn×hn，KI 击退=DAMAGE_BASE×gx×JI）。
                let ppos = world.players[idx as usize].pos;
                // Homing：锁定「点击处最近敌人」（098b S003 语义，复用 Missile 原型的锚点搜索）。
                let homing_target = if proj == crate::skill::W098bProjKind::Homing {
                    let anchor = target.unwrap_or(ppos);
                    world
                        .nearest_other_enemy(anchor, idx)
                        .and_then(|epos| world.players.iter().find(|q| q.alive && q.pos == epos).map(|q| q.id))
                } else {
                    None
                };
                let dir = match target {
                    Some(t) => {
                        let d = t - ppos;
                        if d.length() > Fix64::ZERO {
                            d.normalized()
                        } else {
                            Vec2::new(Fix64::ONE, Fix64::ZERO)
                        }
                    }
                    None => Vec2::new(Fix64::ONE, Fix64::ZERO),
                };
                // 连发（count>1）：以施法方向为中心、±spread_step 对称扇出（火焰喷射锥形 5 道）。
                let half = (count.max(1) as i64 - 1) / 2;
                for k in -half..=half {
                    let ang = Fix64::from_num(spread_step) * Fix64::from_num(k);
                    let d = crate::fix::rotate_ccw(dir, ang);
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::W098b {
                            proj,
                            vel: d * speed,
                            speed,
                            radius,
                            remaining: life,
                            life,
                            gx: stats.damage,
                            kb_ji,
                            // 点燃总量随施法等级（growth.extra = 6+1.5×L；effect.ignite 仅作开关+L1 基准）。
                            ignite: ignite.map(|base| if stats.extra > Fix64::ZERO { stats.extra } else { base }),
                            blast,
                            target: homing_target,
                            returning: false,
                        },
                        pos: ppos,
                        alive: true,
                    });
                }
            }
            SkillEffect::W098bUtility { kind, speed, max_distance } => {
                // 098b 位移/增益系（M2 批次B）：duration/damage/max_distance 全走 stats（随等级），
                // speed 为常量（冲刺速度或移速乘数）。各 kind 机制见 W098bUtilKind doc。
                let dur = stats.duration.to_num::<f64>();
                match kind {
                    crate::skill::W098bUtilKind::Reflect => {
                        if let Some(p) = world.players.get_mut(idx as usize) {
                            p.add_buff(BuffKind::Reflect, dur);
                        }
                    }
                    crate::skill::W098bUtilKind::Rewind => {
                        // 标记当前位置+HP，3.6s 后闪回（098b fC/ER；已在回溯中则覆盖，M1 不拒绝）。
                        if let Some(p) = world.players.get_mut(idx as usize) {
                            p.rewind = Some((p.pos, p.hp, stats.duration));
                        }
                    }
                    crate::skill::W098bUtilKind::Haste => {
                        if let Some(p) = world.players.get_mut(idx as usize) {
                            p.add_buff(BuffKind::Speed(speed.to_num::<f64>()), dur);
                        }
                    }
                    crate::skill::W098bUtilKind::Windwalk => {
                        if let Some(p) = world.players.get_mut(idx as usize) {
                            p.add_buff(BuffKind::Stealth, dur);
                            p.add_buff(BuffKind::Speed(speed.to_num::<f64>()), dur);
                        }
                    }
                    crate::skill::W098bUtilKind::Blink => {
                        if let Some(p) = world.players.get_mut(idx as usize) {
                            if let Some(t) = target {
                                let d = t - p.pos;
                                let dist = d.length();
                                let md = stats.max_distance.max(max_distance);
                                if dist > md {
                                    p.pos += d.normalized() * md;
                                } else {
                                    p.pos = t;
                                }
                            }
                        }
                    }
                    crate::skill::W098bUtilKind::Dash => {
                        // 冲撞（098b IB）：1300/s 强制位移 + 冲刺期间踢击窗口（撞人 KI 伤+击退）。
                        // 时长 = 最大距离/速度；0.5s 定身为命中后效果（TODO 随踢击命中路径接入）。
                        if let Some(p) = world.players.get_mut(idx as usize) {
                            let dir = match target {
                                Some(t) => { let d = t - p.pos; if d.length() > Fix64::ZERO { d.normalized() } else { Vec2::new(Fix64::ONE, Fix64::ZERO) } }
                                None => Vec2::new(Fix64::ONE, Fix64::ZERO),
                            };
                            let dur_s = (stats.max_distance / speed).to_num::<f64>();
                            p.push(dir * speed, dur_s);
                            p.kick = Some(Kick {
                                push_power: warlock_ki_knockback(stats.damage, Fix64::ONE),
                                push_time: Fix64::from_num(W098B_KB_TIME),
                                push_damage: stats.damage,
                                remaining: Fix64::from_num(dur_s),
                            });
                        }
                    }
                    crate::skill::W098bUtilKind::Swap => {
                        // 移形换位（098b mB）：目标点附近有敌则互换位置，否则自身瞬移过去
                        //（复用 TestSwap 的目标搜索语义；弹体化 TODO）。
                        if let Some(t) = target {
                            let ppos = world.players[idx as usize].pos;
                            let d = t - ppos;
                            let dist = d.length();
                            let md = stats.max_distance.max(max_distance);
                            let real_dist = if dist > md { md } else { dist };
                            let near_r = Fix64::from_num(0.51 * 60.0);
                            if real_dist > near_r {
                                let dir = if dist > Fix64::ZERO { d.normalized() } else { Vec2::new(Fix64::ONE, Fix64::ZERO) };
                                let realplace = ppos + dir * real_dist;
                                let enemy_pos = world.nearest_other_enemy(realplace, idx);
                                let eid = enemy_pos.and_then(|epos| {
                                    let d2 = epos - realplace;
                                    if d2.length_squared() <= near_r * near_r {
                                        world.players.iter().find(|q| q.alive && q.id != idx && (q.pos - epos).length_squared() < Fix64::from_num(1.0)).map(|q| q.id)
                                    } else { None }
                                });
                                if let Some(eid) = eid {
                                    let epos = world.players[eid as usize].pos;
                                    world.players[eid as usize].pos = ppos;
                                    world.players[idx as usize].pos = epos;
                                } else {
                                    world.players[idx as usize].pos = realplace;
                                }
                            }
                        }
                    }
                }
            }
            SkillEffect::W098bBolt { range, kb_ji } => {
                // 098b 即时射线（S002 闪电）：沿方向 raycast 首个命中（玩家或障碍截断）；
                // FI 伤害 = gx×Gn×hn（growth.damage 随等级；走 damage_player 现有管线，含击杀记账）；
                // KI 击退 = DAMAGE_BASE×gx×JI（098b 口径）；lightning_visual 复用 D1 原型视觉通道。
                let gx = stats.damage;
                let (ppos, pradius) = {
                    let p = &world.players[idx as usize];
                    (p.pos, p.radius)
                };
                let dir = towards(ppos, target);
                let origin = ppos + dir * pradius;
                let end = if let Some(hit) = world.raycast_first(origin, dir, range, idx) {
                    let mut hit_player: Option<u32> = None;
                    for p in world.players.iter() {
                        if !p.alive || p.id == idx {
                            continue;
                        }
                        if (p.pos - hit).length_squared() <= p.radius * p.radius {
                            hit_player = Some(p.id);
                            break;
                        }
                    }
                    if let Some(pid) = hit_player {
                        world.damage_player(pid, gx, Some(idx));
                        if let Some(p) = world.players.get_mut(pid as usize) {
                            if p.alive {
                                let kb = warlock_ki_knockback(gx, kb_ji);
                                p.push(dir * kb, W098B_KB_TIME);
                            }
                        }
                    }
                    hit
                } else {
                    origin + dir * range
                };
                world.lightning_visual = Some((origin, end, Fix64::from_num(0.1)));
            }
            SkillEffect::Missile { .. } => {
                // 追踪导弹：锁定点击处最近的敌人全速直追；命中爆炸伤+击退。（数值走 stats，随等级成长）
                let ppos = world.players[idx as usize].pos;
                // 找出点击出发点（target 或施法者位置）最近的非施法者敌人
                let anchor = target.unwrap_or(ppos);
                let aim = world.nearest_other_enemy(anchor, idx);
                let dir = match aim {
                    Some(epos) => {
                        let d = epos - ppos;
                        if d.length() > Fix64::ZERO {
                            d.normalized()
                        } else {
                            Vec2::new(Fix64::ONE, Fix64::ZERO)
                        }
                    }
                    None => {
                        let d = anchor - ppos;
                        if d.length() > Fix64::ZERO {
                            d.normalized()
                        } else {
                            Vec2::new(Fix64::ONE, Fix64::ZERO)
                        }
                    }
                };
                world.projectiles.push(Projectile {
                    owner: idx,
                    kind: ProjectileKind::Missile {
                        dir,
                        speed: stats.speed,
                        damage: stats.damage,
                        radius: stats.radius,
                        push_power: stats.push_power,
                        push_time: stats.push_time,
                        remaining: stats.range,
                    },
                    pos: ppos,
                    alive: true,
                });
            }
            SkillEffect::Boomerang { accelerate, .. } => {
                // 回旋镖（D2）：朝目标方向飞出，随后持续向施法者加速回飞；撞障碍反弹；命中爆炸伤+击退。
                // 数值走 stats（随等级成长）；accelerate 无成长字段，用 effect 固定值。
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = match target {
                        Some(t) => {
                            let d = t - p.pos;
                            if d.length() > Fix64::ZERO {
                                d.normalized()
                            } else {
                                Vec2::new(Fix64::ONE, Fix64::ZERO)
                            }
                        }
                        None => Vec2::new(Fix64::ONE, Fix64::ZERO),
                    };
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::Boomerang {
                            vel: dir * stats.speed,
                            accelerate,
                            damage: stats.damage,
                            radius: stats.radius,
                            push_power: stats.push_power,
                            push_time: stats.push_time,
                            life: stats.duration,
                            owner_pos: p.pos,
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::Banana { count, turn_rad, .. } => {
                // 双香蕉曲线弹（D4）：朝施法方向两侧各打一发曲线弹，命中爆炸伤+击退。
                // 数值走 stats（随等级成长）；turn_rad 无成长字段，用 effect 固定值。
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let base_dir = match target {
                        Some(t) => {
                            let d = t - p.pos;
                            if d.length() > Fix64::ZERO {
                                d.normalized()
                            } else {
                                Vec2::new(Fix64::ONE, Fix64::ZERO)
                            }
                        }
                        None => Vec2::new(Fix64::ONE, Fix64::ZERO),
                    };
                    let span = (count as f64 - 1.0) / 2.0; // 居中分布
                    for i in 0..count {
                        // 原版 D4：两发呈 ±45° 对称曲线（BananaScript setmm 相反符号）
                        let off = (i as f64 - span) * 0.5; // count=2 → -0.25 / +0.25
                        let start_dir = crate::fix::rotate_ccw(base_dir, Fix64::from_num(off));
                        world.projectiles.push(Projectile {
                            owner: idx,
                            kind: ProjectileKind::Banana {
                                dir: start_dir,
                                speed: stats.speed,
                                turn: Fix64::from_num(turn_rad * if off < 0.0 { 1.0 } else { -1.0 }),
                                damage: stats.damage,
                                radius: stats.radius,
                                push_power: stats.push_power,
                                push_time: stats.push_time,
                                life: stats.duration,
                            },
                            pos: p.pos,
                            alive: true,
                        });
                    }
                }
            }
            SkillEffect::RollProjectile { .. } => {
                // 滚动火球（E1b）：沿方向直线滚动，接触范围内持续掉血。（数值走 stats，随等级成长）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = match target {
                        Some(t) => {
                            let d = t - p.pos;
                            if d.length() > Fix64::ZERO {
                                d.normalized()
                            } else {
                                Vec2::new(Fix64::ONE, Fix64::ZERO)
                            }
                        }
                        None => Vec2::new(Fix64::ONE, Fix64::ZERO),
                    };
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::Rolling {
                            dir,
                            speed: stats.speed,
                            damage_per_sec: stats.damage,
                            radius: stats.radius,
                            remaining: stats.range,
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::ScatterBurst { count, step_rad, .. } => {
                // 撒弹线（E3）：到终点爆散一个扇形。（数值走 stats，随等级成长）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = match target {
                        Some(t) => {
                            let d = t - p.pos;
                            if d.length() > Fix64::ZERO {
                                d.normalized()
                            } else {
                                Vec2::new(Fix64::ONE, Fix64::ZERO)
                            }
                        }
                        None => Vec2::new(Fix64::ONE, Fix64::ZERO),
                    };
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::ScatterLine {
                            dir,
                            speed: stats.speed,
                            remaining: stats.range,
                            scatter: ScatterKind::Burst {
                                count,
                                step_rad: Fix64::from_num(step_rad),
                                bullet_speed: stats.speed,
                            },
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::ScatterPeriodic { count, interval, turn_rad, .. } => {
                // 撒弹线（E3b）：飞行途中周期性散射击并旋转。（数值走 stats，随等级成长）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = match target {
                        Some(t) => {
                            let d = t - p.pos;
                            if d.length() > Fix64::ZERO {
                                d.normalized()
                            } else {
                                Vec2::new(Fix64::ONE, Fix64::ZERO)
                            }
                        }
                        None => Vec2::new(Fix64::ONE, Fix64::ZERO),
                    };
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::ScatterLine {
                            dir,
                            speed: stats.speed,
                            remaining: stats.range,
                            scatter: ScatterKind::Periodic {
                                count,
                                interval: Fix64::from_num(interval),
                                elapsed: Fix64::ZERO,
                                bullet_speed: stats.speed,
                                turn_rad: Fix64::from_num(turn_rad),
                            },
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::Blink { .. } => {
                // 朝目标方向瞬移至多 `stats.max_distance`
                if let Some(p) = world.players.get_mut(idx as usize) {
                    if let Some(t) = target {
                        let d = t - p.pos;
                        let dist = d.length();
                        let md = stats.max_distance;
                        if dist > md {
                            p.pos += d.normalized() * md;
                        } else {
                            p.pos = t;
                        }
                    }
                }
            }
            SkillEffect::Blink2 { .. } => {
                // 二段闪·第一段：同普通闪烁，随后开启一段可免冷却再闪一次的窗口。
                if let Some(p) = world.players.get_mut(idx as usize) {
                    if let Some(t) = target {
                        let d = t - p.pos;
                        let dist = d.length();
                        let md = stats.max_distance;
                        if dist > md {
                            p.pos += d.normalized() * md;
                        } else {
                            p.pos = t;
                        }
                    }
                    p.blink2_window = Some(stats.duration); // duration = 二段可用窗口
                }
            }
            SkillEffect::DashSlash { .. } => {
                // 冲刺斩：进入无限时长 + 全程隐身直线冲刺，直到玩家给新移动命令（IdoDSWL）。
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = match target {
                        Some(t) => {
                            let d = t - p.pos;
                            if d.length() > Fix64::ZERO {
                                d.normalized()
                            } else {
                                Vec2::new(Fix64::ONE, Fix64::ZERO)
                            }
                        }
                        None => Vec2::new(Fix64::ONE, Fix64::ZERO),
                    };
                    p.dash_active = true;
                    p.dash_vel = dir * stats.speed.max(Fix64::ONE);
                    p.add_buff(BuffKind::Stealth, 3600.0); // 长期隐身，直到 IdoDSWL 移除
                }
            }
            SkillEffect::BlinkToWall { .. } => {
                // 闪到墙：先以不可变借读取起点/方向/命中，再落点，避免与修改 pos 冲突。
                let (ppos, pradius) = {
                    let p = &world.players[idx as usize];
                    (p.pos, p.radius)
                };
                let dir = match target {
                    Some(t) => {
                        let d = t - ppos;
                        if d.length() > Fix64::ZERO {
                            d.normalized()
                        } else {
                            Vec2::new(Fix64::ONE, Fix64::ZERO)
                        }
                    }
                    None => Vec2::new(Fix64::ONE, Fix64::ZERO),
                };
                let origin = ppos + dir * pradius;
                let maxd = stats.max_distance;
                let land = match world.raycast_first(origin, dir, maxd, idx) {
                    Some(hit) => hit - dir * pradius, // 落在命中点前空一格
                    None => ppos + dir * maxd,
                };
                if let Some(p) = world.players.get_mut(idx as usize) {
                    p.pos = land;
                }
            }
            SkillEffect::Rock { .. } => {
                // 生成一个延时爆炸的石头
                if let Some(t) = target {
                    let pr = Projectile {
                        owner: idx,
                        kind: ProjectileKind::Rock {
                            fuse: stats.duration,
                            radius: stats.radius,
                            damage: stats.damage,
                            bomb_force: Fix64::from_num(8.0),
                        },
                        pos: t,
                        alive: true,
                    };
                    world.projectiles.push(pr);
                }
            }
            SkillEffect::DashStrike { .. } => {
                // 冲锋：朝目标方向高速移动 + 撞击踢击，持续一段时间
                // （改用自己的强制位移模型 push，与击退共享一套计时）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = match target {
                        Some(t) => { let d = t - p.pos; if d.length() > Fix64::ZERO { d.normalized() } else { Vec2::new(Fix64::ONE, Fix64::ZERO) } }
                        None => Vec2::new(Fix64::ONE, Fix64::ZERO),
                    };
                    p.push(dir * stats.speed.max(Fix64::ONE), stats.duration.to_num::<f64>());
                    p.kick = Some(Kick {
                        push_power: stats.push_power,
                        push_time: stats.push_time,
                        push_damage: stats.push_damage,
                        remaining: stats.duration,
                    });
                }
            }
            SkillEffect::StealthPush { duration, .. } => {
                // 潜行踢：隐身 + 接触踢击，持续一段时间（时长来自技能效果定义，非 growth）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    p.add_buff(BuffKind::Stealth, duration.to_num::<f64>());
                    p.kick = Some(Kick {
                        push_power: stats.push_power,
                        push_time: stats.push_time,
                        push_damage: stats.push_damage,
                        remaining: duration,
                    });
                }
            }
            SkillEffect::StealthPush2 { duration, .. } => {
                // 潜行踢·连推（E2b）：撞障碍后 0.3s 重新触发踢击（总窗口内可反复）。
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let k = Kick {
                        push_power: stats.push_power,
                        push_time: stats.push_time,
                        push_damage: stats.push_damage,
                        remaining: duration,
                    };
                    p.add_buff(BuffKind::Stealth, duration.to_num::<f64>());
                    p.kick = Some(k);
                    p.ricochet_kick = Some(k);
                    p.ricochet_window = duration;
                    p.ricochet_pending = None;
                }
            }
            SkillEffect::Shadow => {
                // 影身（C3）：若已有锚点则传送回锚点并清记号；否则在当前位置放下锚点并起一个有效期窗口。
                if let Some(p) = world.players.get_mut(idx as usize) {
                    match p.shadow_anchor {
                        Some(anchor) => {
                            p.pos = anchor;
                            p.shadow_anchor = None;
                            p.shadow_window = Fix64::ZERO;
                        }
                        None => {
                            p.shadow_anchor = Some(p.pos);
                            p.shadow_window = stats.duration; // maxshadowtime
                        }
                    }
                }
            }
            SkillEffect::FakeSetup { max_time } => {
                // C4 幻象·第一阶段：进入「待幻」，等待右键设移动目标时触发留假身+瞬移。
                if let Some(p) = world.players.get_mut(idx as usize) {
                    p.fake_active = Some(max_time); // 存的是剩余最长等待时间
                    p.move_target = None; // 施法会取消当前移动命令
                }
            }
            SkillEffect::ChainLeech { heal, .. } => {
                // T1b 吸血链镖：命中吸血 + 链下一个。（speed/damage 走 stats，随等级成长）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = towards(p.pos, target);
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::Chain {
                            dir,
                            speed: stats.speed,
                            damage: stats.damage,
                            heal,
                            ratio: Fix64::ONE,
                            ratio_decay: Fix64::ZERO,
                            life: Fix64::from_num(1.5),
                            last_target: u32::MAX,
                            owner: idx,
                            max_chain: 3,
                            hit_count: 0,
                            turn_delay: Fix64::ZERO,
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::TurnLeech { heal, turn_delay, .. } => {
                // TestLeech 转镖吸血：先直线飞 turn_delay 再转向最近敌人，命中吸血 + 链
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = towards(p.pos, target);
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::Chain {
                            dir,
                            speed: stats.speed,
                            damage: stats.damage,
                            heal,
                            ratio: Fix64::ONE,
                            ratio_decay: Fix64::ZERO,
                            life: Fix64::from_num(1.5),
                            last_target: u32::MAX,
                            owner: idx,
                            max_chain: 3,
                            hit_count: 0,
                            turn_delay,
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::JumpDecay { ratio_decay, .. } => {
                // T3 跳弹·衰减：命中后跳到下一个，伤害逐跳衰减。（speed/damage 走 stats，随等级成长）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = towards(p.pos, target);
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::Chain {
                            dir,
                            speed: stats.speed,
                            damage: stats.damage,
                            heal: Fix64::ZERO,
                            ratio: Fix64::ONE,
                            ratio_decay,
                            life: Fix64::from_num(1.5),
                            last_target: u32::MAX,
                            owner: idx,
                            max_chain: 8,
                            hit_count: 0,
                            turn_delay: Fix64::ZERO,
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::Volley { count, spread_step, .. } => {
                // T2b 扇面齐射：从 -count/2 到 +count/2 一次喷出。（bullet_speed/damage 走 stats）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let base = towards(p.pos, target);
                    let span = (count as f64 - 1.0) / 2.0 * spread_step;
                    for i in 0..count {
                        let off = (i as f64) * spread_step - span;
                        let d = crate::fix::rotate_ccw(base, Fix64::from_num(off));
                        world.projectiles.push(Projectile {
                            owner: idx,
                            kind: ProjectileKind::Bullet {
                                dir: d,
                                speed: stats.speed,
                                damage: stats.damage,
                                radius: Fix64::from_num(0.5),
                                remaining: Fix64::from_num(SABULLET_RANGE),
                            },
                            pos: p.pos,
                            alive: true,
                        });
                    }
                }
            }
            SkillEffect::Sweep { count, cadence, turn_step, .. } => {
                // T2 扇扫连射：设发射器状态，由世界逐帧依次发射。（bullet_speed/damage 走 stats）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let base = towards(p.pos, target);
                    p.sweep = Some(crate::player::SweepState {
                        dir: base,
                        bullet_speed: stats.speed,
                        damage: stats.damage,
                        remaining: count,
                        cadence,
                        turn_step,
                        elapsed: 0.0,
                        id: idx,
                    });
                }
            }
            SkillEffect::BonusChain { .. } => {
                // T3b 蓄力跳弹：发射一枚直线炸弹（伤害含累计 damageplus）。（数值走 stats）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dmg = stats.damage + Fix64::from_num(p.damageplus);
                    let dir = towards(p.pos, target);
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::BonusBomb {
                            dir,
                            speed: stats.speed,
                            damage: dmg,
                            radius: Fix64::from_num(0.8),
                            push_power: Fix64::from_num(6.0),
                            push_time: Fix64::from_num(1.0),
                            remaining: stats.range,
                            owner: idx,
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::Tether { beam, .. } => {
                // 回拉线（Y1/Y1b）：锁定点击处最近目标，拉向施法者并持续掉血。（数值走 stats）
                let ppos = world.players[idx as usize].pos;
                let anchor = target.unwrap_or(ppos);
                let tgt = world.nearest_other_enemy(anchor, idx);
                if let Some(tpos) = tgt {
                    let tid = world
                        .players
                        .iter()
                        .find(|p| p.alive && p.id != idx && (p.pos - tpos).length_squared() < Fix64::from_num(0.01))
                        .map(|p| p.id)
                        .unwrap_or(u32::MAX);
                    if tid != u32::MAX {
                        world.projectiles.push(Projectile {
                            owner: idx,
                            kind: ProjectileKind::Tether {
                                owner: idx,
                                target: tid,
                                damage_per_sec: stats.damage,
                                pull_speed: stats.speed,
                                remaining: stats.duration,
                                beam,
                            },
                            pos: ppos,
                            alive: true,
                        });
                    }
                }
            }
            SkillEffect::PushShot { .. } => {
                // 撞击迟缓（Y2）/爆炸弹（Test01）：直线弹命中→伤害 + 强推 push_time。（数值走 stats）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = towards(p.pos, target);
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::PushBullet {
                            dir,
                            speed: stats.speed,
                            damage: stats.damage,
                            radius: Fix64::from_num(0.6),
                            push_power: stats.push_power,
                            push_time: stats.push_time,
                            remaining: stats.range,
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::Lightning => {
                // 雷电（D1）：指向性即时射线，命中敌人伤害+推，撞障碍则停在障碍前（无效果）。
                let (ppos, pradius) = {
                    let p = &world.players[idx as usize];
                    (p.pos, p.radius)
                };
                let dir = towards(ppos, target);
                let origin = ppos + dir * pradius;
                let maxd = stats.range;
                // 本帧闪电的终点：命中点（玩家/障碍）或射线最大距离处；供 client 画闪电线（Unity 原版 Drawline）。
                let end = if let Some(hit) = world.raycast_first(origin, dir, maxd, idx) {
                    // 命中点若落在某存活玩家表面 → 命中该玩家（伤害 + 沿射线方向推）。
                    let mut hit_player: Option<u32> = None;
                    for p in world.players.iter() {
                        if !p.alive || p.id == idx {
                            continue;
                        }
                        if (p.pos - hit).length_squared() <= p.radius * p.radius {
                            hit_player = Some(p.id);
                            break;
                        }
                    }
                    if let Some(pid) = hit_player {
                        world.damage_player(pid, stats.damage, Some(idx));
                        if let Some(p) = world.players.get_mut(pid as usize) {
                            if p.alive {
                                p.push(dir * stats.push_power, stats.push_time.to_num::<f64>());
                            }
                        }
                    }
                    // 命中障碍：雷电被阻挡，无额外效果。
                    hit
                } else {
                    origin + dir * maxd
                };
                world.lightning_visual = Some((origin, end, Fix64::from_num(0.1)));
            }
            SkillEffect::Swap { .. } => {
                // 换位（R3a）：点目标，若目标位置附近有敌人则与之互换位置，否则自身瞬移过去。
                if let Some(t) = target {
                    let ppos = world.players[idx as usize].pos;
                    let d = t - ppos;
                    let dist = d.length();
                    let md = stats.max_distance;
                    let real_dist = if dist > md { md } else { dist };
                    // 过近不施法（旧尺度 0.51 → war3 尺度 ×60 ≈ 两个英雄半径；过渡换算见 PORT_098B_DECISIONS.md D4）。
                    if real_dist > Fix64::from_num(0.51 * 60.0) {
                        let dir = if dist > Fix64::ZERO {
                            d.normalized()
                        } else {
                            Vec2::new(Fix64::ONE, Fix64::ZERO)
                        };
                        let realplace = ppos + dir * real_dist;
                        // 目标位置附近是否有敌人（足够近视为"点到敌人"；判定半径同上 ×60）。
                        let enemy_pos = world.nearest_other_enemy(realplace, idx);
                        let near_r = Fix64::from_num(0.51 * 60.0);
                        let eid = enemy_pos.and_then(|epos| {
                            let d2 = epos - realplace;
                            if d2.length_squared() <= near_r * near_r {
                                world
                                    .players
                                    .iter()
                                    .find(|p| p.alive && p.id != idx && (p.pos - epos).length_squared() < Fix64::from_num(1.0))
                                    .map(|p| p.id)
                            } else {
                                None
                            }
                        });
                        if let Some(eid) = eid {
                            let epos = world.players[eid as usize].pos;
                            world.players[eid as usize].pos = ppos;
                            world.players[idx as usize].pos = epos;
                        } else {
                            world.players[idx as usize].pos = realplace;
                        }
                    }
                }
            }
            SkillEffect::BindLine { count, bind_time, .. } => {
                // 束缚线（Y2b）：制造一段朝目标推进、收拢后束缚线上敌人的线。（speed 走 stats）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = towards(p.pos, target);
                    // 线长（旧尺度 6 → war3 尺度 ×60，过渡换算见 PORT_098B_DECISIONS.md D4）
                    let end = p.pos + dir * Fix64::from_num(6.0 * 60.0);
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::BindLine {
                            dir,
                            speed: stats.speed,
                            count,
                            fired: 0,
                            bind_time: Fix64::from_num(bind_time),
                            from: p.pos,
                            end,
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::GravityZone { pull_speed, .. } => {
                // 引力场（Y3）：在点击处/朝目标方向发射一个吸引附近敌人的场。（数值走 stats）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = towards(p.pos, target);
                    let range = Fix64::from_num(stats.range.to_num::<f64>().max(1.0));
                    let place = if let Some(t) = target { t } else { p.pos + dir * range };
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::Gravity {
                            dir,
                            speed: stats.speed,
                            radius: stats.radius,
                            pull_speed,
                            remaining: stats.duration,
                        },
                        pos: place,
                        alive: true,
                    });
                }
            }
            SkillEffect::StarZone { heal_per_sec, .. } => {
                // 星域（Y3b）：在点击处放一颗持续伤/回血的星。（数值走 stats）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let place = target.unwrap_or(p.pos);
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::Star {
                            owner: idx,
                            radius: stats.radius,
                            damage_per_sec: stats.damage,
                            heal_per_sec,
                            remaining: stats.duration,
                        },
                        pos: place,
                        alive: true,
                    });
                }
            }
            SkillEffect::SelfExplode { self_stay, .. } => {
                // 蓄力自爆（F）：以施法者为中心 AOE；自己扣到残血、范围内敌人受伤并踢开。（数值走 stats）
                let ppos = world.players[idx as usize].pos;
                let radius = stats.radius;
                let damage = stats.damage;
                let kick = stats.push_power;
                let kick_time = stats.push_time;
                // 施法者自残：对照 Unity `GetHurt(min(10, hp-1))`——最多自扣 10 血、保底留 self_stay(1) 血。
                // （旧实现把施法者固定扣到 1 血，高血量时过伤、与 Unity 不符。）
                if let Some(p) = world.players.get_mut(idx as usize) {
                    if p.hp > self_stay {
                        let dmg = (p.hp - self_stay).min(Fix64::from_num(10));
                        p.hp = (p.hp - dmg).max(Fix64::ZERO);
                    }
                }
                // 范围内其他敌人：伤害 + 沿连线踢开
                let mut kick_map: Vec<(u32, Vec2, Fix64, Fix64)> = Vec::new(); // (id, dir, power, time)
                for i in 0..world.players.len() {
                    let pid = world.players[i].id;
                    if pid == idx || !world.players[i].alive {
                        continue;
                    }
                    let d = world.players[i].pos - ppos;
                    let dsq = d.length_squared();
                    if dsq <= (radius * radius) && dsq > Fix64::ZERO {
                        kick_map.push((pid, d.normalized(), kick, kick_time));
                    } else if dsq <= (radius * radius) {
                        kick_map.push((pid, Vec2::new(Fix64::ONE, Fix64::ZERO), kick, kick_time));
                    }
                }
                for (pid, dir, power, t) in kick_map {
                    world.damage_player(pid, damage, Some(idx));
                    if let Some(p) = world.players.get_mut(pid as usize) {
                        if p.alive {
                            p.push(dir * power, t.to_num::<f64>());
                        }
                    }
                }
            }
            SkillEffect::LineBeam { .. } => {
                // 旧的持续线占位已由 ScatterBurst 取代；此处不再落地。
            }
            SkillEffect::Unimplemented => {
                // 未实现技能的占位：不落地效果（仅消耗施法与冷却）
            }
        }
    }
}

/// 返回 `pos` 处半径 `radius` 内最近的存活玩家（排除 `owner`），给出 `(玩家 id, 指向玩家的方向向量)`。
fn nearest_hit(players: &[Player], pos: Vec2, owner: u32, radius: Fix64) -> Option<(u32, Vec2)> {
    let mut best: Option<(Fix64, u32)> = None;
    for p in players.iter() {
        if !p.alive || p.id == owner {
            continue;
        }
        let d = p.pos - pos;
        let d_sq = d.length_squared();
        let rr = (radius + p.radius) * (radius + p.radius);
        if d_sq <= rr && best.map(|(bd, _)| d_sq < bd).unwrap_or(true) {
            best = Some((d_sq, p.id));
        }
    }
    best.and_then(|(_, id)| {
        let owner_idx = players.iter().position(|p| p.id == id)?;
        Some((id, players[owner_idx].pos - pos))
    })
}

/// 同 `nearest_hit`，但额外排除一个 `skip` id（供链镖跳跃：不命中上一个目标）。
fn nearest_hit_with_skip(
    players: &[Player],
    pos: Vec2,
    owner: u32,
    radius: Fix64,
    skip: u32,
) -> Option<(u32, Vec2)> {
    let mut best: Option<(Fix64, u32)> = None;
    for p in players.iter() {
        if !p.alive || p.id == owner || p.id == skip {
            continue;
        }
        let d = p.pos - pos;
        let d_sq = d.length_squared();
        let rr = (radius + p.radius) * (radius + p.radius);
        if d_sq <= rr && best.map(|(bd, _)| d_sq < bd).unwrap_or(true) {
            best = Some((d_sq, p.id));
        }
    }
    best.and_then(|(_, id)| {
        let owner_idx = players.iter().position(|p| p.id == id)?;
        Some((id, players[owner_idx].pos - pos))
    })
}

/// 射线 `o + t*dir`（`dir` 已单位化）与圆心 `c` 半径 `r` 的首次交点的 `t`。
/// 无交点或从内部出发时返回 `None`。视 dir 单位化以确保 t 即距离。
fn ray_circle_hit(o: Vec2, dir: Vec2, c: Vec2, r: Fix64) -> Option<Fix64> {
    let f = o - c;
    let b = dir.dot(f); // 2b 相当于 -2*(d·(c-o)) 的推导
    let c_dot = f.length_squared() - r * r;
    // t² + 2bt + c = 0
    let disc = b * b - c_dot;
    if disc <= Fix64::ZERO {
        return None;
    }
    let sq = disc.sqrt();
    let t1 = -b - sq;
    let t2 = -b + sq;
    // 取正向且最小的交点（忽略负 t，即物体在起点后方）
    if t1 >= Fix64::ZERO {
        Some(t1)
    } else if t2 >= Fix64::ZERO {
        Some(t2)
    } else {
        None
    }
}

/// 从 `from` 朝 `target`（可选）的单位方向；无目标或重合时默认 +X。
fn towards(from: Vec2, target: Option<Vec2>) -> Vec2 {
    match target {
        Some(t) => {
            let d = t - from;
            if d.length() > Fix64::ZERO {
                d.normalized()
            } else {
                Vec2::new(Fix64::ONE, Fix64::ZERO)
            }
        }
        None => Vec2::new(Fix64::ONE, Fix64::ZERO),
    }
}

/// 点 `p` 是否在线段 [a, b] 附近（距离 <= width）。供束缚线/回拉线扫射判定。
fn point_near_segment(p: Vec2, a: Vec2, b: Vec2, width: Fix64) -> bool {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if len_sq <= Fix64::ZERO {
        return (p - a).length_squared() <= width * width;
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(Fix64::ZERO, Fix64::ONE);
    let proj = a + ab * t;
    (p - proj).length_squared() <= width * width
}

/// 两个圆球是否相交。保留供画线/技能判定等逻辑复用。
#[allow(dead_code)]
fn circles_overlap(a: Vec2, ar: Fix64, b: Vec2, br: Fix64) -> bool {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    let rr = ar + br;
    dx * dx + dy * dy < rr * rr
}

/// 成对解析玩家圆球碰撞：把重叠的两球沿中心连线推开，避免相互穿透。
///
/// 伤害处理：若两球重叠较深（被挤压）则双方各受一定伤害，鼓励拉开距离。
/// 位置修正按半径反比分配（更小的球退得更多），保证确定性与顺序无关地一致。
fn resolve_player_collisions(players: &mut [Player], dt: Fix64) {
    let n = players.len();
    for i in 0..n {
        for j in (i + 1)..n {
            if !players[i].alive || !players[j].alive {
                continue;
            }
            let (a, b) = (players[i], players[j]);
            let delta = b.pos - a.pos;
            let dist_sq = delta.length_squared();
            let min_dist = a.radius + b.radius;
            let min_sq = min_dist * min_dist;
            if dist_sq >= min_sq {
                continue;
            }
            let dist = dist_sq.sqrt();
            let overlap = if dist == Fix64::ZERO {
                // 完全重叠的退化情形：给定一个确定方向的推力
                min_dist * Fix64::from_num(0.5)
            } else {
                min_dist - dist
            };
            let dir = if dist == Fix64::ZERO {
                Vec2::new(Fix64::ONE, Fix64::ZERO)
            } else {
                delta / dist
            };

            // 位置修正（按半径反比）
            let total = a.radius + b.radius;
            let frac_a = if total == Fix64::ZERO {
                Fix64::from_num(0.5)
            } else {
                b.radius / total
            };
            // a 被向后推 overlap*frac_a，b 被向前推 overlap*(1-frac_a)
            let push_a = dir * (overlap * frac_a);
            let push_b = dir * (overlap * (Fix64::ONE - frac_a));
            players[i].pos -= push_a;
            players[j].pos += push_b;

            // C2 护盾·反弹：若哪一方带反弹护盾，把撞进来的对方的强制位移/推击镜向反射。
            // 法向量 = (b 指向 a 的方向 / 或 a 指向 b 的方向)。
            let normal_b_to_a = if dist == Fix64::ZERO {
                Vec2::new(Fix64::ONE, Fix64::ZERO)
            } else {
                -dir /* 指向 a 的单位向量 */
            };
            let normal_a_to_b = -normal_b_to_a;
            // j 有反弹护盾 → 反射 i 的运动（若 i 在强制位移则反射其 vel）
            if players[j].shield() {
                if let Some(c) = players[i].control.as_mut() {
                    c.vel = crate::fix::mirror_by(c.vel, normal_a_to_b);
                }
            }
            if players[i].shield() {
                if let Some(c) = players[j].control.as_mut() {
                    c.vel = crate::fix::mirror_by(c.vel, normal_b_to_a);
                }
            }

            // 挤压伤害：重叠越深伤害越高（boost 期间返半回血）
            let damage = Fix64::from_num(OVERLAP_DAMAGE) * dt
                * (overlap / min_dist).max(Fix64::from_num(0.15));
            players[i].hp = (players[i].hp - players[i].soak_boost(damage)).max(Fix64::ZERO);
            players[j].hp = (players[j].hp - players[j].soak_boost(damage)).max(Fix64::ZERO);

            // 踢击/撞击效果（冲锋·潜行踢）：携带 kick 的一方撞到敌人，造成技能伤害+击退，并消耗 kick。
            let dir_b_from_a = if dist == Fix64::ZERO {
                Vec2::new(Fix64::ONE, Fix64::ZERO)
            } else {
                delta.normalized()
            };
            if let Some(kick) = players[i].kick {
                players[j].hp = (players[j].hp - players[j].soak_boost(kick.push_damage)).max(Fix64::ZERO);
                players[j].last_hit_by = Some(players[i].id);
                players[j].push(dir_b_from_a * kick.push_power, kick.push_time.to_num::<f64>());
                players[i].kick = None;
                players[i].remove_buff(BuffKind::Stealth);
            }
            if let Some(kick) = players[j].kick {
                players[i].hp = (players[i].hp - players[i].soak_boost(kick.push_damage)).max(Fix64::ZERO);
                players[i].last_hit_by = Some(players[j].id);
                players[i].push(-dir_b_from_a * kick.push_power, kick.push_time.to_num::<f64>());
                players[j].kick = None;
                players[j].remove_buff(BuffKind::Stealth);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Fix64, b: f64, tol: f64) -> bool {
        (a.to_num::<f64>() - b).abs() < tol
    }

    // ===== 测试尺度换算辅助（098b 过渡，PORT_098B_DECISIONS.md D4） =====
    // 世界已切 war3 尺度而测试直觉仍是旧（Unity demo 微缩）尺度：空间字面量经此换算，
    // 测试语义（机制验证）不变。legacy_scale_def 删除（名册替换完成）时一并清理。
    /// 旧尺度距离/坐标 → war3 尺度（×60 = 1200/20）。
    fn d60(x: f64) -> Fix64 {
        Fix64::from_num(x * 60.0)
    }
    /// 旧尺度半径 → war3 尺度（×16 = 16/1）。
    fn r16(x: f64) -> Fix64 {
        Fix64::from_num(x * 16.0)
    }
    /// 距离语义的 near：期望与容差同为旧尺度，×60 后比较。
    fn near_d(a: Fix64, b: f64, tol: f64) -> bool {
        near(a, b * 60.0, tol * 60.0)
    }

    #[test]
    fn movement_stops_at_target() {
        let mut world = World::new(1, 1);
        world.obstacles.clear(); // 本测试只验证移动到目标，不依赖随机柱子（避免挡路）
        // 固定起点，避免随机布局影响断言
        world.players[0].pos = Vec2::ZERO;
        let dt = Fix64::from_num(1.0 / 60.0);
        let target = Vec2::new(Fix64::from_num(3.0), Fix64::ZERO);
        for _ in 0..240 {
            world.step(vec![PlayerInput { set_target: Some(target), ..Default::default() }], dt);
        }
        // 一定时间后应到达目标点附近
        let p = &world.players[0];
        assert!(near(p.pos.x, 3.0, 0.5));
        assert!(near(p.pos.y, 0.0, 0.5));
    }

    #[test]
    fn out_of_bounds_drains_hp() {
        let mut world = World::new(1, 1);
        // 直接把玩家放在场地很边缘
        world.players[0].pos = Vec2::new(d60(20.5), d60(20.5));
        let dt = Fix64::from_num(1.0 / 60.0);
        let hp_before = world.players[0].hp;
        for _ in 0..60 {
            world.step(vec![PlayerInput::default()], dt);
        }
        assert!(world.players[0].hp < hp_before);
    }

    #[test]
    fn collisions_push_apart_deterministically() {
        let mut a = World::new(2, 42);
        a.players[0].pos = Vec2::new(Fix64::ONE, Fix64::ZERO);
        a.players[1].pos = Vec2::new(-Fix64::ONE, Fix64::ZERO);
        let dt = Fix64::from_num(1.0 / 60.0);
        for _ in 0..120 {
            a.step(vec![PlayerInput::default(), PlayerInput::default()], dt);
        }

        let mut b = a.clone();
        // 相同输入必须得到逐位一致的结果
        for _ in 0..60 {
            b.step(vec![PlayerInput::default(), PlayerInput::default()], dt);
            a.step(vec![PlayerInput::default(), PlayerInput::default()], dt);
        }
        for i in 0..2 {
            assert_eq!(a.players[i].pos, b.players[i].pos);
        }
        // 两个堆叠的球应被推得彼此远离
        let d = a.players[0].pos - a.players[1].pos;
        let min = a.players[0].radius + a.players[1].radius;
        assert!(d.length_squared() >= min * min || !a.players[0].alive || !a.players[1].alive);
    }

    #[test]
    fn rock_damages_victim_after_windup_and_fuse() {
        let mut world = World::new(2, 7);
        let dt = Fix64::from_num(1.0 / 60.0);
        // 施法者固定，受害者放在落点附近
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(3.0), Fix64::ZERO);

        let hp0 = world.players[0].hp;
        let hp1 = world.players[1].hp;
        // 玩家0 施放 E1 掷石到 (3,0)；玩家1 不动
        let input = vec![
            PlayerInput {
                cast: Some((SkillId::Rock, Some(Vec2::new(d60(3.0), Fix64::ZERO)))),
                ..Default::default()
            },
            PlayerInput::default(),
        ];
        // 前摇 0.2s + fuse 0.7s，跑足够时间让石头爆炸
        for _ in 0..90 {
            world.step(input.clone(), dt);
            // 玩家0 前摇期间不动（无移动目标），避免误判
        }
        // 施法者满血（掷石不伤自己），受害者应已受伤且可能被击退
        assert_eq!(world.players[0].hp, hp0, "施法者不应自伤");
        assert!(world.players[1].hp < hp1, "受害者应受到爆炸伤害");
    }

    #[test]
    fn lightning_hits_enemy_along_ray() {
        let mut world = World::new(2, 8);
        world.obstacles.clear(); // 清除随机柱子，避免挡雷电射线
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(3.0), Fix64::ZERO);
        let hp0 = world.players[0].hp;
        let hp1 = world.players[1].hp;
        // 玩家0 施放 D1 雷电指向 (3,0)；受害者在射线路径上。
        let input = vec![
            PlayerInput {
                cast: Some((SkillId::TestLightning, Some(Vec2::new(d60(3.0), Fix64::ZERO)))),
                ..Default::default()
            },
            PlayerInput::default(),
        ];
        // 前摇 0.1s，跑足够帧数完成施法。
        let mut saw_bolt = false;
        for _ in 0..20 {
            world.step(input.clone(), dt);
            if world.lightning_visual.is_some() {
                saw_bolt = true; // 闪电射线可视化痕迹至少出现过一次（客户端据此画线）
            }
        }
        assert_eq!(world.players[0].hp, hp0, "施法者不应自伤");
        assert!(world.players[1].hp < hp1, "雷电应命中路径上的敌人并造成伤害");
        assert!(saw_bolt, "施放雷电后应设置 lightning_visual（供 client 画闪电线）");
    }

    #[test]
    fn swap_teleports_onto_empty_point() {
        let mut world = World::new(2, 8);
        world.obstacles.clear(); // 清随机柱子：否则敌人可能被柱子分离推开，误判为"被移动"
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 敌人在远处，目标点 (3,0) 无敌人。
        world.players[1].pos = Vec2::new(d60(8.0), Fix64::ZERO);
        let input = vec![
            PlayerInput {
                cast: Some((SkillId::TestSwap, Some(Vec2::new(d60(3.0), Fix64::ZERO)))),
                ..Default::default()
            },
            PlayerInput::default(),
        ];
        for _ in 0..20 {
            world.step(input.clone(), dt);
        }
        // 目标点无敌人 → 自身瞬移到 (3,0)。
        assert!(near_d(world.players[0].pos.x, 3.0, 0.3), "换位应瞬移到目标点，实际 {:?}", world.players[0].pos);
        // 敌人不受影响。
        assert!(near_d(world.players[1].pos.x, 8.0, 0.5), "远处敌人不应被移动");
    }

    #[test]
    fn swap_exchanges_position_with_enemy() {
        let mut world = World::new(2, 8);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 敌人恰好站在目标点 (3,0)。
        world.players[1].pos = Vec2::new(d60(3.0), Fix64::ZERO);
        let input = vec![
            PlayerInput {
                cast: Some((SkillId::TestSwap, Some(Vec2::new(d60(3.0), Fix64::ZERO)))),
                ..Default::default()
            },
            PlayerInput::default(),
        ];
        for _ in 0..20 {
            world.step(input.clone(), dt);
        }
        // 施法者到敌人位置，敌人被换到施法者原位置。
        assert!(near_d(world.players[0].pos.x, 3.0, 0.3), "施法者应到敌人位置，实际 {:?}", world.players[0].pos);
        assert!(near(world.players[1].pos.x, 0.0, 0.3), "敌人应被换到施法者原位置，实际 {:?}", world.players[1].pos);
    }

    #[test]
    fn blink_teleports_toward_target() {
        let mut world = World::new(1, 9);
        world.obstacles.clear(); // 本测试只验证闪烁，清掉随机柱子避免挡路
        world.players[0].pos = Vec2::ZERO;
        let dt = Fix64::from_num(1.0 / 60.0);
        let far = Vec2::new(d60(100.0), Fix64::ZERO);
        // R1 闪烁：前摇为 0（DEF_ZERO 的 rock 有 0.2 前摇，但 blink growth 用 DEF_ZERO windup_base=0）
        let input = vec![PlayerInput {
            cast: Some((SkillId::Blink, Some(far))),
            ..Default::default()
        }];
        // 跑几帧：windup(0) + recovery(0.1) 后瞬移完成
        for _ in 0..10 {
            world.step(input.clone(), dt);
        }
        // 应已瞬移到 max_distance(6) 方向
        assert!(near_d(world.players[0].pos.x, 6.0, 0.3), "瞬移距离应为 6，实际 {:?}", world.players[0].pos);
    }

    #[test]
    fn cannot_walk_while_casting() {
        let mut world = World::new(1, 11);
        world.obstacles.clear();
        world.players[0].pos = Vec2::ZERO;
        let dt = Fix64::from_num(1.0 / 60.0);
        // 选一个前摇非零的技能（Rock：前摇 0.2s）
        let cast_input = PlayerInput {
            cast: Some((SkillId::Rock, Some(Vec2::new(Fix64::from_num(5.0), Fix64::ZERO)))),
            ..Default::default()
        };
        // 开始施法 + 同帧还给了移动目标(3,0)；施法应优先使移动无效
        let mut start = vec![PlayerInput {
            set_target: Some(Vec2::new(Fix64::from_num(3.0), Fix64::ZERO)),
            ..cast_input
        }];
        let x0 = world.players[0].pos.x;
        // 前摇期间（约 12 帧）玩家不应位移
        for _ in 0..12 {
            let input = std::mem::take(&mut start);
            world.step(input, dt);
            start = vec![PlayerInput {
                cast: None,
                set_target: Some(Vec2::new(Fix64::from_num(3.0), Fix64::ZERO)),
                ..Default::default()
            }];
        }
        assert!(
            near(world.players[0].pos.x, x0.to_num::<f64>(), 1e-3),
            "施法(前摇)期间不应移动，位置应从 {} 变到 {:?}",
            x0.to_num::<f64>(),
            world.players[0].pos
        );
    }

    #[test]
    fn blink_cancels_previous_movement_order() {
        let mut world = World::new(1, 13);
        world.players[0].pos = Vec2::ZERO;
        let dt = Fix64::from_num(1.0 / 60.0);
        let far = Vec2::new(d60(100.0), Fix64::ZERO);
        // 第一帧：给了“很远”的移动目标 + 施放闪烁。施法应取消旧的移动命令并瞬移到 (6,0)。
        let first = vec![PlayerInput {
            set_target: Some(far),
            cast: Some((SkillId::Blink, Some(far))),
            ..Default::default()
        }];
        world.step(first, dt);
        // 之后（正确客户端）不再下发旧移动目标。
        let rest = vec![PlayerInput::default()];
        for _ in 1..12 {
            world.step(rest.clone(), dt);
        }
        // 落地后应立即停在瞬移点，不应继续朝 (100,0) 走
        let p = &world.players[0];
        assert!(
            near_d(p.pos.x, 6.0, 0.3),
            "闪烁落地后不应继续走向旧目标，位置应为 ~6，实际 {:?}",
            p.pos
        );
    }

    #[test]
    fn move_target_is_level_blank_frames_dont_drop_it_and_cast_clears_it() {
        // 批 A 依赖的两条 world 契约（曾把移动目标改成 take() 的那版方案的回归反例）：
        //   1) 移动目标是**电平量**：一旦 set_target 设置过，后续空输入帧（set_target=None）
        //      绝不能把它清掉 —— 否则在帧同步下，移动指令只发一次、被 host 输入缓存覆盖后
        //      就永久丢失（表现为“要点好几次右键才成功”）。
        //   2) 施法成功会清 move_target；此后若不再下发新目标（None），角色保持停住、
        //      不再自动走向旧目标（问题 1）。
        let dt = Fix64::from_num(1.0 / 60.0);
        let far = Vec2::new(d60(100.0), Fix64::ZERO);

        // (1) 电平量不丢：设置移动目标后，连续空输入帧不应把它清掉。
        let mut a = World::new(1, 31);
        a.obstacles.clear();
        a.players[0].pos = Vec2::ZERO;
        a.step(
            vec![PlayerInput { set_target: Some(far), ..Default::default() }],
            dt,
        );
        assert!(a.players[0].move_target.is_some(), "设置后应有移动目标");
        for _ in 0..8 {
            a.step(vec![PlayerInput::default()], dt); // set_target=None
        }
        assert!(
            a.players[0].move_target.is_some(),
            "空输入帧不应清掉移动目标（电平量不丢）"
        );

        // (2) 施法成功清移动；之后不再发目标（None）→ 保持停住，不自动走向旧目标。
        let mut b = World::new(1, 32);
        b.obstacles.clear();
        b.players[0].pos = Vec2::ZERO;
        b.step(
            vec![PlayerInput {
                set_target: Some(far),
                cast: Some((SkillId::Blink, Some(far))),
                ..Default::default()
            }],
            dt,
        );
        assert!(b.players[0].move_target.is_none(), "施法成功应清掉移动目标");
        for _ in 0..30 {
            b.step(vec![PlayerInput::default()], dt); // 施法后不再下发旧目标
        }
        assert!(
            near_d(b.players[0].pos.x, 6.0, 0.5),
            "施法后不应再走向旧目标，位置应为 ~6，实际 {:?}",
            b.players[0].pos
        );
    }

    #[test]
    fn round_over_and_reset_cycle() {
        let mut world = World::new(2, 21);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO; // 站桩存活
        // 玩家1 放到场地外很远，会因出界伤害持续掉血致死；先给它很低血量加速
        world.players[1].hp = Fix64::from_num(1.0);
        world.players[1].pos = Vec2::new(d60(30.0), Fix64::ZERO);
        let input = vec![PlayerInput::default(), PlayerInput::default()];
        let mut guard = 0;
        while !world.round_over() && guard < 600 {
            world.step(input.clone(), dt);
            guard += 1;
        }
        assert!(world.round_over(), "玩家1 应倒地，本局应当结束");
        // 名次：冠军是存活者（玩家0）在前
        let placement = world.placement();
        assert_eq!(placement[0], 0);

        // 重置后可再开下一局
        world.reset_round();
        assert!(!world.round_over());
        assert!(world.alive_count() == 2);
        assert_eq!(world.players[1].hp, world.players[1].max_hp);
    }

    #[test]
    fn dash_strike_charges_and_damages_on_contact() {
        let mut world = World::new(2, 31);
        world.obstacles.clear(); // 清除随机柱子，避免挡冲锋路径
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 敌人堵在冲锋路径前方、但不在初始接触距离（验证冲锋确实移动接近敌人）。
        world.players[1].pos = Vec2::new(Fix64::from_num(4.0), Fix64::ZERO);
        let hp1 = world.players[1].hp;
        // 施放冲锋朝 (20,0) 方向
        let input = vec![
            PlayerInput {
                cast: Some((SkillId::DashStrike, Some(Vec2::new(Fix64::from_num(20.0), Fix64::ZERO)))),
                ..Default::default()
            },
            PlayerInput::default(),
        ];
        // 跑若干帧（windup 0.15s + 冲锋途中）
        for _ in 0..30 {
            world.step(input.clone(), dt);
        }
        // 冲锋应让施法者朝目标方向移动（不是原地不动）。
        assert!(world.players[0].pos.x > 2.0, "冲锋应向前冲，实际 {:?}", world.players[0].pos);
        // 敌人应被撞伤（且位置被推开或翻腾）
        assert!(world.players[1].hp < hp1, "冲锋撞击应造成伤害");
    }

    #[test]
    fn stealth_push_damages_on_contact() {
        let mut world = World::new(2, 32);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[1].pos = Vec2::new(Fix64::from_num(1.5), Fix64::ZERO);
        let hp0 = world.players[0].hp;
        let hp1 = world.players[1].hp;
        let input = vec![
            PlayerInput {
                cast: Some((SkillId::StealthPush, None)),
                ..Default::default()
            },
            PlayerInput::default(),
        ];
        // StealthPush windup 0.25s，跑足够多帧让它进入 kick 并接触
        for _ in 0..40 {
            world.step(input.clone(), dt);
        }
        assert!(world.players[1].hp < hp1, "潜行踢接触应造成伤害");
        let _ = hp0;
    }

    #[test]
    fn shadow_two_phase_teleport() {
        let mut world = World::new(1, 33);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::new(Fix64::from_num(5.0), Fix64::ZERO);
        let none = vec![PlayerInput::default()];
        // 第一阶段：放锚（只发一次命令）
        world.step(vec![PlayerInput { cast: Some((SkillId::Shadow, None)), ..Default::default() }], dt);
        // 等待 windup(0)+recovery+冷却走完
        for _ in 0..70 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].shadow_anchor.is_some(), "影身第一阶段应放下锚点");
        let anchor = world.players[0].shadow_anchor.expect("锚点存在");
        // 移动后施放第二阶段：回到锚点
        world.players[0].pos = Vec2::ZERO;
        world.step(vec![PlayerInput { cast: Some((SkillId::Shadow, None)), ..Default::default() }], dt);
        for _ in 0..6 {
            world.step(none.clone(), dt);
        }
        assert_eq!(world.players[0].pos, anchor, "影身第二阶段应传回锚点");
    }

    #[test]
    fn shadow_auto_returns_when_window_expires() {
        let mut world = World::new(1, 42);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::new(Fix64::from_num(5.0), Fix64::ZERO);
        let none = vec![PlayerInput::default()];
        // 放锚
        world.step(vec![PlayerInput { cast: Some((SkillId::Shadow, None)), ..Default::default() }], dt);
        for _ in 0..10 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].shadow_anchor.is_some());
        // 走离锚点
        world.players[0].pos = Vec2::ZERO;
        // 记号窗口 2.5s = 150 帧；跑足够久让窗口到期 → 应自动回归到锚点 (5,0)
        for _ in 0..200 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].shadow_anchor.is_none(), "窗口到期应清记号");
        assert!(world.players[0].pos.x > Fix64::from_num(4.0), "到期应自动回归锚点");
    }

    #[test]
    fn fake_two_phase_leaves_decoys_and_teleports() {
        let mut world = World::new(1, 34);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        // 第一阶段：施放进入「待幻」，不立即留假身。
        world.step(vec![PlayerInput { cast: Some((SkillId::Fake, None)), ..Default::default() }], dt);
        let none = vec![PlayerInput::default()];
        for _ in 0..3 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].fake_active.is_some(), "施放后应处于待幻");
        assert_eq!(world.projectiles.iter().filter(|pr| matches!(pr.kind, ProjectileKind::Decoy { .. })).count(), 0, "待幻阶段不应留假身");
        // 第二阶段：给移动目标 → 本体瞬移 + 原位留 2 个假身。
        world.step(vec![PlayerInput { set_target: Some(Vec2::new(Fix64::from_num(4.0), Fix64::ZERO)), ..Default::default() }], dt);
        let decoys = world.projectiles.iter().filter(|pr| matches!(pr.kind, ProjectileKind::Decoy { .. })).count();
        assert!(decoys >= 2, "幻象应在原位留 2 个假身，实际 {}", decoys);
        assert!(world.players[0].fake_active.is_none(), "触发后应退出待幻");
        // 本体应已沿目标方向瞬移（原点 → 向右移动 2）
        assert!(world.players[0].pos.x > Fix64::from_num(1.0), "本体应向目标方向瞬移");
    }

    #[test]
    fn shield_reflects_and_expires() {
        let mut world = World::new(1, 40);
        let dt = Fix64::from_num(1.0 / 60.0);
        // p0 开反弹护盾。
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.step(vec![PlayerInput { cast: Some((SkillId::Shield, None)), ..Default::default() }], dt);
        let none = vec![PlayerInput::default()];
        for _ in 0..8 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].shield(), "护盾应已激活");
        let hp0 = world.players[0].hp;
        // 手动注入一枚朝 p0 (+x 方向) 飞的 Bullet（owner 用不存在的 99，使它能命中 p0(id=0)），
        // 命中带护盾的 p0 应被反射、不扣血。
        world.projectiles.push(Projectile {
            owner: 99,
            kind: ProjectileKind::Bullet {
                dir: Vec2::new(Fix64::ONE, Fix64::ZERO),
                speed: Fix64::from_num(6.0),
                damage: Fix64::from_num(10.0),
                radius: Fix64::from_num(0.6),
                remaining: Fix64::from_num(20.0),
            },
            pos: Vec2::new(Fix64::from_num(-2.0), Fix64::ZERO),
            alive: true,
        });
        let mut reflected = false;
        for _ in 0..60 {
            world.step(none.clone(), dt);
            if let Some(p) = world.projectiles.iter().find(|pr| matches!(pr.kind, ProjectileKind::Bullet { .. })) {
                if let ProjectileKind::Bullet { dir, .. } = p.kind {
                    if dir.x < Fix64::ZERO {
                        reflected = true;
                    }
                }
            }
        }
        assert_eq!(world.players[0].hp, hp0, "反弹护盾应弹开直射弹，不扣血");
        assert!(reflected, "直射弹应被护盾反向反射");
        // 等护盾过期
        for _ in 0..240 {
            world.step(none.clone(), dt);
        }
        assert!(!world.players[0].shield(), "护盾应已过期");
    }

    #[test]
    fn boost_lifesteals_half_on_hit() {
        let mut world = World::new(2, 41);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // p0 开疾跑(boost)。
        world.step(vec![
            PlayerInput { cast: Some((SkillId::Boost, None)), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..5 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].has_buff(BuffKind::Boost), "疾跑应已激活");
        let hp0 = world.players[0].hp;
        // 出界扣血：boost 返还一半 → 净扣一半。
        world.players[0].pos = Vec2::new(d60(100.0), Fix64::ZERO);
        for _ in 0..30 {
            world.step(none.clone(), dt);
        }
        let oob_total = Fix64::from_num(OUT_HURT) * Fix64::from_num(30.0 / 60.0);
        let expected_drop = (oob_total / Fix64::from_num(2)).to_num::<f64>();
        let actual_drop = (hp0 - world.players[0].hp).to_num::<f64>();
        assert!((actual_drop - expected_drop).abs() < 0.5, "boost 应返还一半回血，实际净扣 {} 期望 {}", actual_drop, expected_drop);
    }

    #[test]
    fn stone_shot_hits_victim() {
        let mut world = World::new(2, 41);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[1].pos = Vec2::new(Fix64::from_num(3.0), Fix64::ZERO); // 挡在直线上
        let hp1 = world.players[1].hp;
        // 施放掷弹（StoneShot=Bullet）朝 (10,0)
        let input = vec![
            PlayerInput { cast: Some((SkillId::StoneShot, Some(Vec2::new(Fix64::from_num(10.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ];
        // windup 0.15s + 飞行，跑 ~0.5s
        for _ in 0..30 {
            world.step(input.clone(), dt);
        }
        assert!(world.players[1].hp < hp1, "直射弹应命中并造成伤害");
        assert_eq!(world.players[1].last_hit_by, Some(0), "击杀来源应为施法者");
    }

    #[test]
    fn fireball_bullet_travels_and_disappears() {
        let mut world = World::new(2, 42);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[1].pos = Vec2::new(d60(10.0), d60(10.0)); // 不在直线上
        let hp1 = world.players[1].hp;
        let input = vec![
            PlayerInput { cast: Some((SkillId::D2Fireball, Some(Vec2::new(d60(12.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ];
        // 火球朝 (12,0) 直射，射程 14；跑足够长让它飞出/消失
        for _ in 0..90 {
            world.step(input.clone(), dt);
        }
        // 直线方向的敌人没有，所以不应命中远处另一个玩家
        assert_eq!(world.players[1].hp, hp1, "不在直线上的目标不应被命中");
        // 弹体应已（或正在飞行中）；此处只验证逻辑不崩溃、施法者不受伤
        assert!(world.players[0].alive);
    }

    #[test]
    fn missile_homes_and_explodes() {
        let mut world = World::new(2, 43);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[1].pos = Vec2::new(Fix64::from_num(5.0), Fix64::from_num(5.0));
        let hp1 = world.players[1].hp;
        // 导弹点目标：以点击处(敌人附近)锁定最近敌人
        let click = Vec2::new(Fix64::from_num(5.0), Fix64::from_num(5.0));
        let input = vec![
            PlayerInput { cast: Some((SkillId::D3Missile, Some(click))), ..Default::default() },
            PlayerInput::default(),
        ];
        // windup 0.2s + 全速直追 + 爆炸，跑 2s
        for _ in 0..120 {
            world.step(input.clone(), dt);
        }
        assert!(world.players[1].hp < hp1, "追踪导弹应命中目标并爆炸");
    }

    #[test]
    fn scatter_line_fans_bullets_at_end() {
        // 单玩家，无敌人干扰，验证撒弹线到终点会爆散出多个扇形子弹。
        let mut world = World::new(1, 44);
        world.obstacles.clear(); // 本测试只验证撒弹，不依赖随机柱子（弹体撞柱会被挡下）
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        let cast_only = vec![
            PlayerInput { cast: Some((SkillId::LineBeam, Some(Vec2::new(Fix64::from_num(30.0), Fix64::ZERO)))), ..Default::default() },
        ];
        // 只施放一次，之后空输入推进。撒弹线约 1.33s 到终点爆散。
        world.step(cast_only, dt);
        let none = vec![PlayerInput::default()];
        let mut max_bullets = 0usize;
        for _ in 0..120 {
            world.step(none.clone(), dt);
            let b = world.projectiles.iter().filter(|pr| matches!(pr.kind, ProjectileKind::Bullet { .. })).count();
            max_bullets = max_bullets.max(b);
        }
        assert!(max_bullets >= 8, "撒弹线到终点应爆散出 8 个扇形子弹，实际峰值 {}", max_bullets);
    }

    #[test]
    fn stealth_push2_ricochets_off_obstacle() {
        let mut world = World::new(1, 46);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.obstacles.clear();
        world.players[0].pos = Vec2::new(Fix64::from_num(2.0), Fix64::ZERO);
        world.players[0].move_target = None;
        // 在 (0,0) 放一根半径 2 的柱子：施放连推后玩家重叠其上会触发重新踢击
        world.obstacles.push(Obstacle::new(Vec2::ZERO, 2.0));
        let cast = vec![PlayerInput { cast: Some((SkillId::StealthPush2, None)), ..Default::default() }];
        world.step(cast, dt);
        let none = vec![PlayerInput::default()];
        // windup 0.25s 后 kick 生效并撞墙消耗 → 应进入 ricochet_pending
        for _ in 0..20 {
            world.step(none.clone(), dt);
        }
        assert!(
            world.players[0].ricochet_window > Fix64::ZERO,
            "连推应处于可重踢窗口"
        );
    }

    #[test]
    fn rolling_fireball_dots_enemy_on_contact() {
        let mut world = World::new(2, 45);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 敌人挡在滚动路径上
        world.players[1].pos = Vec2::new(Fix64::from_num(2.0), Fix64::ZERO);
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        let input = vec![
            PlayerInput { cast: Some((SkillId::StoneShot, Some(Vec2::new(Fix64::from_num(20.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ];
        // windup 0.15s + 滚动火球持续接触，跑 1s
        for _ in 0..60 {
            world.step(input.clone(), dt);
        }
        assert!(world.players[1].hp < hp1, "滚动火球接触应持续掉血（DoT）");
    }

    /// 回归：弹体必须被柱子（静态圆形障碍）挡住。
    /// 旧代码的 1b 段只判了回旋镖，火球/滚动火球/导弹/香蕉弹等都**直接穿过柱子**打到后面的人。
    /// 现在「会飞行的弹体」一律参与判定：回旋镖反弹、其余撞柱消失。
    #[test]
    fn obstacle_blocks_flying_projectiles() {
        /// 让 player0 朝 +X 发一发滚动火球（掷弹），返回（player1 掉了多少血，场上出现过的火球数）。
        fn fire(with_pillar: bool) -> (Fix64, usize) {
            let mut world = World::new(2, 77);
            world.obstacles.clear();
            world.sandbox = true; // 不缩圈、不判回合结束，保证只受柱子影响
            world.players[0].pos = Vec2::new(d60(-6.0), Fix64::ZERO);
            world.players[1].pos = Vec2::new(d60(6.0), Fix64::ZERO);
            world.players[0].move_target = None;
            world.players[1].move_target = None;
            if with_pillar {
                world.obstacles.push(Obstacle::new(Vec2::ZERO, 1.5 * 16.0));
            }
            let hp1 = world.players[1].hp;
            let dt = Fix64::from_num(1.0 / 60.0);
            let cast = vec![
                PlayerInput {
                    cast: Some((SkillId::StoneShot, Some(Vec2::new(d60(30.0), Fix64::ZERO)))),
                    ..Default::default()
                },
                PlayerInput::default(),
            ];
            world.step(cast, dt);
            let none = vec![PlayerInput::default(), PlayerInput::default()];
            let mut seen = 0usize;
            for _ in 0..180 {
                world.step(none.clone(), dt);
                let n = world
                    .projectiles
                    .iter()
                    .filter(|pr| pr.alive && matches!(pr.kind, ProjectileKind::Rolling { .. }))
                    .count();
                seen = seen.max(n);
            }
            (hp1 - world.players[1].hp, seen)
        }

        // 有柱子：火球被挡下，柱子后面的玩家一点伤害都不该吃到。
        let (dmg_blocked, _) = fire(true);
        assert!(
            dmg_blocked <= Fix64::ZERO,
            "柱子应完全挡住滚动火球，实际后面的玩家掉了 {dmg_blocked}"
        );
        // 对照组（无柱子）：同一发火球确实能打到人，证明上面不是因为施法压根没发生。
        let (dmg_open, seen) = fire(false);
        assert!(seen >= 1, "对照组：应至少生成一个滚动火球");
        assert!(
            dmg_open > Fix64::ZERO,
            "对照组：无柱子时同一发火球应能打到后面的玩家"
        );
    }

    #[test]
    fn obstacles_never_overlap_across_seeds() {
        // 对多种种子 + 多轮布局，验证柱子数量 ≤ 5、互不重叠、不出界、不碰玩家出生环。
        for seed in [1u64, 2, 42, 99, 908660, 20260812] {
            let mut w = World::new(2, seed);
            for round in 0..6 {
                if round > 0 {
                    w.reset_round();
                }
                let obs = &w.obstacles;
                assert!(obs.len() <= 5, "柱子数量应 ≤ 5，实际 {}", obs.len());
                for i in 0..obs.len() {
                    let a = &obs[i];
                    let d = a.pos.length().to_num::<f64>();
                    // 不出界（含半径仍远离边缘）
                    assert!(d + a.radius.to_num::<f64>() < w.arena_radius.to_num::<f64>() - 0.5,
                        "柱子应远离边缘 seed={seed} round={round} idx={i}");
                    // 不碰玩家出生环（玩家在 0.6*arena）
                    assert!(d < w.arena_radius.to_num::<f64>() * 0.6 - a.radius.to_num::<f64>() - 0.3,
                        "柱子不应碰玩家出生环 seed={seed} round={round} idx={i}");
                    for j in i + 1..obs.len() {
                        let b = &obs[j];
                        let dist = (a.pos - b.pos).length().to_num::<f64>();
                        assert!(dist >= a.radius.to_num::<f64>() + b.radius.to_num::<f64>() + 0.3,
                            "柱子不应重叠 seed={seed} round={round} {i}-{j} dist={dist}");
                    }
                }
            }
        }
    }

    #[test]
    fn obstacle_count_varies_including_zero() {
        // 柱子数量每轮随机、可为 0（无柱子）；统计多种 seed 应出现不同数量且包含 0。
        let mut counts: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for seed in 0..60u64 {
            let mut w = World::new(2, seed);
            counts.insert(w.obstacles.len());
            w.reset_round();
            counts.insert(w.obstacles.len());
        }
        assert!(counts.contains(&0), "柱子数量应包含 0（无柱子）");
        assert!(counts.len() >= 2, "柱子数量应出现多种取值，实际 {counts:?}");
        for c in &counts {
            assert!(*c <= 5, "柱子数量应 ≤ 5，实际 {c}");
        }
    }

    #[test]
    fn obstacles_change_across_rounds() {
        // 每轮 reset 用递增 round_seed，配置（数量/位置）随之变化；统计验证“不总是相同”。
        let mut configs: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut w = World::new(2, 20260812);
        for _ in 0..8 {
            configs.insert(format!("{:?}", w.obstacles));
            w.reset_round();
        }
        assert!(
            configs.len() >= 2,
            "8 轮内柱子配置应出现变化，实际 {} 种",
            configs.len()
        );
    }

    #[test]
    fn players_spawn_uniformly_on_ring() {
        // 玩家应在 0.6*arena 的环上均匀等分、互不重叠（含随机整体旋转后仍均匀）。
        for seed in [1u64, 7, 42, 908660] {
            for n in 2u32..=6 {
                let w = World::new(n, seed);
                assert_eq!(w.players.len(), n as usize);
                for i in 0..n as usize {
                    for j in i + 1..n as usize {
                        let d = (w.players[i].pos - w.players[j].pos).length().to_num::<f64>();
                        assert!(d >= 2.0, "玩家应均匀分布且不重叠 n={n} {i}-{j} dist={d}");
                    }
                }
            }
        }
    }

    #[test]
    fn reset_round_clears_projectiles_and_move_targets() {
        // 新轮不应残留上轮的飞行物与移动目标（上一轮的移动指令/子弹不能带到下一轮）。
        let mut w = World::new(2, 7);
        w.players[0].move_target = Some(Vec2::new(Fix64::from_num(5.0), Fix64::ZERO));
        w.projectiles.push(Projectile {
            owner: 0,
            pos: Vec2::ZERO,
            alive: true,
            kind: ProjectileKind::Bullet {
                dir: Vec2::new(Fix64::ONE, Fix64::ZERO),
                speed: Fix64::ONE,
                damage: Fix64::ONE,
                radius: Fix64::from_num(0.2),
                remaining: Fix64::ONE,
            },
        });
        // 再补一个“延时区域”类飞行物（如星域/束缚线），验证也一并清掉。
        w.projectiles.push(Projectile {
            owner: 0,
            pos: Vec2::ZERO,
            alive: true,
            kind: ProjectileKind::Star {
                owner: 0,
                radius: Fix64::from_num(2.0),
                damage_per_sec: Fix64::ONE,
                heal_per_sec: Fix64::ZERO,
                remaining: Fix64::from_num(3.0),
            },
        });
        w.reset_round();
        assert_eq!(w.projectiles.len(), 0, "新轮不应残留上轮的飞行物/延时区域");
        assert!(w.players[0].move_target.is_none(), "新轮不应残留上轮的移动目标");
    }

    #[test]
    fn reset_round_respawns_players_on_spawn_ring() {
        // 每轮结束玩家应重生回出生环（0.6*arena），而非留在上轮位置。
        let mut w = World::new(3, 42);
        w.players[0].pos = Vec2::ZERO;
        w.players[1].pos = Vec2::new(Fix64::from_num(100.0), Fix64::from_num(100.0));
        w.players[2].pos = Vec2::new(Fix64::from_num(-50.0), Fix64::ZERO);
        w.reset_round();
        let expected_r = w.arena_radius * Fix64::from_num(0.6);
        for (i, p) in w.players.iter().enumerate() {
            let d = p.pos.length();
            assert!(
                (d - expected_r).abs() < Fix64::from_num(0.01),
                "玩家应重生在 0.6 出生环 idx={i} d={d:?}"
            );
        }
        // 出生环上玩家应等分、互不重叠。
        for i in 0..w.players.len() {
            for j in i + 1..w.players.len() {
                let d = (w.players[i].pos - w.players[j].pos).length();
                assert!(d > Fix64::ONE, "出生环上玩家应等分不重叠 {i}-{j}");
            }
        }
    }

    #[test]
    fn obstacle_pushes_player_out() {
        let mut world = World::new(1, 50);
        world.obstacles.clear();
        world.players[0].pos = Vec2::new(Fix64::from_num(2.0), Fix64::ZERO);
        // 在 (0,0) 放一根半径 2 的柱子，玩家在 (2,0) 会与柱子重叠
        world.obstacles.push(Obstacle::new(Vec2::ZERO, 2.0));
        let dt = Fix64::from_num(1.0 / 60.0);
        world.step(vec![PlayerInput::default()], dt);
        // 玩家半径 1 + 柱子半径 2 = 3；重叠应从 (2,0) 被推到 >= (3,0)
        assert!(
            world.players[0].pos.x >= Fix64::from_num(2.99),
            "玩家应从柱子里被推出，pos = {:?}",
            world.players[0].pos
        );
    }

    #[test]
    fn blink2_second_stage_is_free_short_blink() {
        let mut world = World::new(1, 51);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        let far = Vec2::new(d60(100.0), Fix64::ZERO);
        let cast = |skill: SkillId, t: Vec2| vec![PlayerInput { cast: Some((skill, Some(t))), ..Default::default() }];
        // 第一段：普通闪烁到 max_distance(5)
        world.step(cast(SkillId::Blink2, far), dt);
        let none = vec![PlayerInput::default()];
        // 等前摇(0)+后摇完成，令窗口仍活着
        for _ in 0..20 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].blink2_window.is_some(), "第一段后应开启二段窗口");
        let x1 = world.players[0].pos.x.to_num::<f64>();
        assert!(x1 > 4.9 * 60.0, "第一段应闪 ~5，实际 {}", x1);
        // 第二段：窗口内再施放 = 免冷却短闪 4
        let x_before = world.players[0].pos.x;
        world.step(cast(SkillId::Blink2, far), dt);
        let dx = (world.players[0].pos.x - x_before).to_num::<f64>();
        assert!(dx > 3.9 * 60.0 && dx < 4.1 * 60.0, "第二段应短闪 ~4，实际 {}", dx);
        assert!(world.players[0].blink2_window.is_none(), "第二段后窗口应清空");
    }

    #[test]
    fn dashslash_moves_invisibly_and_stops_on_new_target() {
        let mut world = World::new(1, 52);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        let far = Vec2::new(d60(100.0), Fix64::ZERO);
        // 施放冲刺斩朝 (100,0)
        world.step(vec![PlayerInput { cast: Some((SkillId::DashSlash, Some(far))), ..Default::default() }], dt);
        let none = vec![PlayerInput::default()];
        // 冲刺斩有 windup 0.1s：跑几帧让施法完成并进入冲刺
        for _ in 0..10 {
            world.step(none.clone(), dt);
        }
        // 冲刺中：应处于隐身且持续位移
        assert!(world.players[0].dash_active, "冲刺斩应激活");
        assert!(world.players[0].stealth(), "冲刺斩应全程隐身");
        let x0 = world.players[0].pos.x;
        for _ in 0..10 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].pos.x > x0, "冲刺应持续前进");
        // 给出新的移动命令 → 解除冲刺 + 现身
        world.step(vec![PlayerInput { set_target: Some(Vec2::new(Fix64::from_num(30.0), Fix64::from_num(30.0))), ..Default::default() }], dt);
        assert!(!world.players[0].dash_active, "新移动命令应解除冲刺");
        assert!(!world.players[0].stealth(), "解除冲刺应现身");
    }

    #[test]
    fn blinktowall_lands_in_front_of_obstacle() {
        let mut world = World::new(1, 53);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.obstacles.clear();
        world.players[0].pos = Vec2::ZERO;
        // 在正前方 (10,0) 放一根半径 1 的柱子
        world.obstacles.push(Obstacle::new(Vec2::new(d60(10.0), Fix64::ZERO), 1.0 * 16.0));
        // 朝 (30,0) 闪到墙：射线应命中柱子，落在柱子前（比 10 更近）
        world.step(vec![PlayerInput { cast: Some((SkillId::BlinkToWall, Some(Vec2::new(d60(30.0), Fix64::ZERO)))), ..Default::default() }], dt);
        let x = world.players[0].pos.x.to_num::<f64>();
        assert!(x > 1.0 * 60.0 && x < 9.9 * 60.0, "闪到墙应落在障碍前（<10），实际 {}", x);

        // 无障碍方向：闪 max_distance(6)
        let mut world2 = World::new(1, 54);
        world2.obstacles.clear();
        world2.players[0].pos = Vec2::ZERO;
        world2.step(vec![PlayerInput { cast: Some((SkillId::BlinkToWall, Some(Vec2::new(d60(30.0), Fix64::ZERO)))), ..Default::default() }], dt);
        let x2 = world2.players[0].pos.x.to_num::<f64>();
        assert!(x2 > 5.9 * 60.0 && x2 < 6.1 * 60.0, "无障碍应闪 max_distance(6)，实际 {}", x2);
    }

    #[test]
    fn boomerang_fireball_spawns_and_returns() {
        let mut world = World::new(2, 60);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(Fix64::from_num(4.0), Fix64::ZERO);
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        let input = vec![
            PlayerInput { cast: Some((SkillId::D2Fireball, Some(Vec2::new(Fix64::from_num(10.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ];
        // 回旋镖应命中挡路的敌人并造成伤害+击退
        for _ in 0..60 {
            world.step(input.clone(), dt);
        }
        assert!(world.players[1].hp < hp1, "回旋镖命中敌人应造成伤害");
    }

    #[test]
    fn banana_curve_shots_hit_enemy() {
        let mut world = World::new(2, 61);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(Fix64::from_num(5.0), Fix64::ZERO);
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        let input = vec![
            PlayerInput { cast: Some((SkillId::D4Fireball, Some(Vec2::new(Fix64::from_num(8.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ];
        for _ in 0..80 {
            world.step(input.clone(), dt);
        }
        assert!(world.players[1].hp < hp1, "香蕉弹命中敌人应造成伤害");
    }

    #[test]
    fn tleech_chains_and_heals() {
        let mut world = World::new(3, 70);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].hp = Fix64::from_num(50.0); // 施法者先残血以观察回血
        for i in 1..3 {
            world.players[i].pos = Vec2::new(Fix64::from_num(3.0 + i as f64), Fix64::ZERO);
        }
        let hp0 = world.players[0].hp;
        let hp1 = world.players[1].hp;
        let hp2 = world.players[2].hp;
        let input = vec![
            PlayerInput { cast: Some((SkillId::TLeech, Some(Vec2::new(Fix64::from_num(4.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
            PlayerInput::default(),
        ];
        for _ in 0..90 {
            world.step(input.clone(), dt);
        }
        assert!(world.players[1].hp < hp1, "吸血链镖应命中敌人1");
        assert!(world.players[2].hp < hp2, "吸血链镖应链到敌人2");
        assert!(world.players[0].hp > hp0, "吸血链镖应给施法者回血");
    }

    /// 回归：转镖（TestLeech）先沿目标方向直线飞 turn_delay 后再转向最近敌人，
    /// 而不是全程自动追踪——否则会失去“飞镖先直飞再拐”的手感。
    #[test]
    fn turn_leech_turns_to_hit_side_enemy() {
        let mut world = World::new(2, 71);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].hp = Fix64::from_num(50.0); // 残血以便观察回血
        // 敌人放在侧上方（不在施法方向 (1,0) 的正前方）：只有镖转向后才能命中。
        world.players[1].pos = Vec2::new(Fix64::from_num(2.0), Fix64::from_num(2.0));
        let hp0 = world.players[0].hp;
        let hp1 = world.players[1].hp;
        let input = vec![
            PlayerInput { cast: Some((SkillId::TestLeech, Some(Vec2::new(Fix64::from_num(5.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ];
        for _ in 0..120 {
            world.step(input.clone(), dt);
        }
        assert!(world.players[1].hp < hp1, "转镖应转向命中侧面敌人（先直线后转向）");
        assert!(world.players[0].hp > hp0, "转镖命中应给施法者吸血回血");
    }

    /// 回归：吸血/跳弹镖必须**有限**（不再无限往返、也不无限重置生存时间而“永远存在”）。
    /// 修前：ratio_decay=0 + 每次命中重置 life=1.5 + 只排除上一目标 → 会在末两个敌人间无限往返；
    /// 修后：max_chain 硬上限 → 链跳 N 次后必然消失。
    #[test]
    fn chain_leech_terminates_not_infinite() {
        let mut world = World::new(5, 82);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        // 4 个敌人围一圈，保证“永远有最近的下一目标”，专门暴露无限往返。
        for i in 1..5 {
            world.players[i].pos = Vec2::new(Fix64::from_num(1.0 + i as f64), Fix64::ZERO);
        }
        let input = vec![
            PlayerInput { cast: Some((SkillId::TLeech, Some(Vec2::new(Fix64::from_num(2.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
            PlayerInput::default(),
            PlayerInput::default(),
            PlayerInput::default(),
        ];
        // 长时间运行（远超单镖正常生存 1.5s）。修前会因无限往返而链镖一直存活；修后应早已消失。
        for _ in 0..300 {
            world.step(input.clone(), dt);
            if world.projectiles.is_empty() {
                break;
            }
        }
        assert!(world.projectiles.is_empty(), "吸血链镖必须在有限次链跳后消失（修前会无限往返）");
    }

    /// 4.6b：属性派生确定性 + 应用到 Player（最大生命/移速）+ 序列化往返保留。
    #[test]
    fn attributes_derive_and_apply_deterministically() {
        use crate::attribute::Attributes;
        let mut world = World::new(2, 11);
        assert!((world.players[0].speed_mult - 1.0).abs() < 1e-9);

        let attrs = Attributes { hp_bonus: 5, speed_bonus: 4, ..Default::default() };
        // 5 点 hp → +50%；4 点 speed → +20%。
        let expected_max = crate::player::MAX_HP * (1.0 + 5.0 * crate::attribute::HP_PER_BONUS);
        world.players[0].apply_attributes(&attrs);
        assert!((world.players[0].max_hp - Fix64::from_num(expected_max)).abs() < Fix64::from_num(1e-6), "max_hp 应按属性加成");
        assert!((world.players[0].speed_mult - 1.2).abs() < 1e-9, "speed_mult 应 +20%");
        // 掉血后 apply 应保持血比（以当前 max_hp 的一半为准）。
        world.players[0].hp = world.players[0].max_hp / Fix64::from_num(2);
        world.players[0].apply_attributes(&attrs);
        let half = world.players[0].max_hp / Fix64::from_num(2);
        assert!((world.players[0].hp - half).abs() < Fix64::from_num(1e-6), "apply 应保持当前血比");

        // 序列化往返保留（speed_mult / max_hp 是确定性共享状态，重连快照须一致）。
        let bytes = crate::world_ser::world_to_bytes(&world);
        let back = crate::world_ser::world_from_bytes(&bytes).expect("decode");
        assert_eq!(back.players[0].max_hp, world.players[0].max_hp);
        assert_eq!(back.players[0].speed_mult, world.players[0].speed_mult);
        assert_eq!(back.players[0].armor_factor, world.players[0].armor_factor);
    }

    /// 4.6b 阶段2：护甲/法抗确实减少玩家造成的伤害（目标有防护时掉血更少）；击退抗性减少击退时长。
    #[test]
    fn attributes_reduce_damage_and_push() {
        use crate::attribute::Attributes;
        let dt = Fix64::from_num(1.0 / 60.0);

        // 用与 rock_damages_victim_after_windup_and_fuse 相同的可靠施放：施法者(0,0) 掷石到受害者旁。
        let mk = |armor: u32, spell: u32| -> Fix64 {
            let mut w = crate::world::World::new(2, 900);
            w.players[0].pos = Vec2::ZERO;
            w.players[1].pos = Vec2::new(Fix64::from_num(3.0), Fix64::ZERO);
            if armor > 0 || spell > 0 {
                w.players[1].apply_attributes(&Attributes {
                    armor,
                    spell_resist: spell,
                    ..Default::default()
                });
            }
            let in0 = vec![
                PlayerInput {
                    cast: Some((SkillId::Rock, Some(Vec2::new(Fix64::from_num(3.0), Fix64::ZERO)))),
                    ..Default::default()
                },
                PlayerInput::default(),
            ];
            for _ in 0..90 {
                w.step(in0.clone(), dt);
            }
            w.players[1].hp
        };

        let hp_base = mk(0, 0);
        let hp_armored = mk(8, 0);
        let hp_resisted = mk(0, 8);
        assert!(hp_armored > hp_base, "护甲应减少玩家伤害：base={hp_base} armored={hp_armored}");
        assert!(hp_resisted > hp_base, "法抗应减少玩家伤害");

        // 击退抗性：给玩家1高 kb，受 push 后 remaining 更短。
        let mut w = crate::world::World::new(2, 910);
        w.players[0].pos = Vec2::ZERO;
        w.players[1].pos = Vec2::new(Fix64::from_num(2.0), Fix64::ZERO);
        w.players[1].apply_attributes(&Attributes { kb_resist: 5, ..Default::default() });
        w.players[1].push(Vec2::new(Fix64::from_num(10.0), Fix64::ZERO), 2.0);
        assert!(w.players[1].control.map(|c| c.remaining.to_num::<f64>()).unwrap() < 2.0, "击退抗性应缩短击退");
    }

    // mana_drains_gates_and_regens 测试已随无蓝量系统删除（PORT_098B_DECISIONS.md D3）。

    // ===== 098b 名册行为测试（M1：S000/S003/S004；数值对账见 skill.rs tests） =====

    /// S000 火球：直飞命中 → FI 伤害（gx=7@L1）+ KI 击退（DAMAGE_BASE×gx×JI 封顶 2000）
    /// + 命中处生成 2.5s 点燃 DoT 场（Star 复用）。
    #[test]
    fn s000_fireball_hits_damages_knocks_and_ignites() {
        let mut world = World::new(2, 950);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(5.0), Fix64::ZERO); // 300 距离，飞行 ~0.3s
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        let input = vec![
            PlayerInput { cast: Some((SkillId::S000, Some(Vec2::new(d60(5.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ];
        world.step(input.clone(), dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        let mut ignited = false;
        for _ in 0..60 {
            // 1s 内必命中（射程 1000、目标静止）
            world.step(none.clone(), dt);
            if world.projectiles.iter().any(|pr| matches!(pr.kind, ProjectileKind::Star { .. })) {
                ignited = true;
            }
        }
        assert!(world.players[1].hp < hp1, "火球 FI 伤害应生效（L1 gx=7），hp {} -> {}", hp1, world.players[1].hp);
        assert!(
            hp1 - world.players[1].hp >= Fix64::from_num(7.0),
            "直伤至少 gx=7（不含点燃），实际掉血 {:?}",
            hp1 - world.players[1].hp
        );
        // KI 击退：命中方向 +x，初速 2000（封顶）×0.35s → 位移显著 >100
        assert!(
            world.players[1].pos.x > d60(5.0) + Fix64::from_num(100.0),
            "火球应把敌人朝弹向击退，实际 x={:?}",
            world.players[1].pos.x
        );
        assert!(ignited, "命中处应生成点燃 DoT 场（Star 复用）");
    }

    /// S003 追踪弹：锁定点击处最近敌人全速直追——目标横移也能转向命中。
    #[test]
    fn s003_homing_missile_tracks_moving_target() {
        let mut world = World::new(2, 951);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 敌人在右上，且持续向 +y 跑（横移考验追踪转向）
        world.players[1].pos = Vec2::new(d60(4.0), d60(2.0));
        world.players[1].move_target = Some(Vec2::new(d60(4.0), d60(10.0)));
        let hp1 = world.players[1].hp;
        let input = vec![
            PlayerInput { cast: Some((SkillId::S003, Some(Vec2::new(d60(4.0), d60(2.0))))), ..Default::default() },
            PlayerInput::default(),
        ];
        world.step(input.clone(), dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        let mut guard = 0;
        while world.players[1].hp >= hp1 && guard < 300 {
            world.step(none.clone(), dt); // life 4.5s=270 帧，5s 内应命中
            guard += 1;
        }
        assert!(world.players[1].hp < hp1, "追踪弹（900/s）应追上移速 210 的目标并造成伤害");
    }

    /// S004 回旋镖：出程后回程拉回施法者，回到附近即收回消失。
    #[test]
    fn s004_boomerang_returns_and_despawns() {
        let mut world = World::new(2, 952);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(-8.0), Fix64::ZERO); // 反方向，保证不误伤/不干扰
        world.players[1].move_target = None;
        let input = vec![
            PlayerInput { cast: Some((SkillId::S004, Some(Vec2::new(d60(10.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ];
        world.step(input.clone(), dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        // 出程阶段（前 ~0.5s）弹应在场且为 098b 回旋镖
        world.step(none.clone(), dt);
        let spawned = world
            .projectiles
            .iter()
            .any(|pr| matches!(pr.kind, ProjectileKind::W098b { proj: crate::skill::W098bProjKind::Boomerang, .. }));
        assert!(spawned, "施法后场上应有 098b 回旋镖弹体");
        // life 1.6s + 回程余量：3s 内应回到施法者附近（<60）并收回
        for _ in 0..180 {
            world.step(none.clone(), dt);
        }
        let still = world
            .projectiles
            .iter()
            .any(|pr| matches!(pr.kind, ProjectileKind::W098b { proj: crate::skill::W098bProjKind::Boomerang, .. }));
        assert!(!still, "回旋镖回程到家应收回消失（3s 足够出+回）");
    }

    /// S006 时光回溯：施放记锚点 → 受伤+位移 → 3.6s 后闪回锚点并还原 HP。
    #[test]
    fn s006_rewind_restores_position_and_hp() {
        let mut world = World::new(2, 956);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 施放回溯（锚点=(0,0), HP=100）
        world.step(vec![PlayerInput { cast: Some((SkillId::S006, None)), ..Default::default() }, PlayerInput::default()], dt);
        assert!(world.players[0].rewind.is_some(), "施放后应记录锚点");
        // 走远 + 掉血
        world.players[0].hp = Fix64::from_num(40.0);
        world.players[0].pos = Vec2::new(d60(8.0), Fix64::ZERO);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..240 {
            world.step(none.clone(), dt); // 4s > 3.6s
        }
        assert!(world.players[0].rewind.is_none(), "到点后应清锚点");
        assert!(near(world.players[0].pos.x, 0.0, 1.0) && near(world.players[0].pos.y, 0.0, 1.0), "应闪回锚点，实际 {:?}", world.players[0].pos);
        assert!(near(world.players[0].hp, 100.0, 0.01), "应还原 HP，实际 {:?}", world.players[0].hp);
    }

    /// S011 闪现：L1 瞬移至多 770。
    #[test]
    fn s011_blink_moves_within_range() {
        let mut world = World::new(1, 957);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        // 近点：直接到点
        world.step(vec![PlayerInput { cast: Some((SkillId::S011, Some(Vec2::new(d60(5.0), Fix64::ZERO)))), ..Default::default() }], dt);
        assert!(near_d(world.players[0].pos.x, 5.0, 0.5), "300 应直接到点，实际 {:?}", world.players[0].pos);
        // 远点（6000）：截断到 770
        world.players[0].pos = Vec2::ZERO;
        world.players[0].caster = crate::skill::Caster::new(); // 清冷却
        world.step(vec![PlayerInput { cast: Some((SkillId::S011, Some(Vec2::new(d60(100.0), Fix64::ZERO)))), ..Default::default() }], dt);
        assert!(near(world.players[0].pos.x, 770.0, 1.0), "超距应截断到 770，实际 {:?}", world.players[0].pos.x);
    }

    /// S012 冲撞：冲刺撞人造成 KI 伤害 + 冲刺位移生效。
    #[test]
    fn s012_dash_charges_and_kicks() {
        let mut world = World::new(2, 958);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(5.0), Fix64::ZERO); // 300 处的敌人
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S012, Some(Vec2::new(d60(10.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..60 {
            world.step(none.clone(), dt); // 300/1300 ≈ 0.23s 冲到
        }
        assert!(world.players[1].hp < hp1, "冲撞应撞到敌人造成伤害（L1 简化 5.4），hp {hp1} -> {}", world.players[1].hp);
        assert!(world.players[0].pos.x > d60(3.0), "施法者应冲向目标，实际 x={:?}", world.players[0].pos.x);
    }

    /// S013 移形换位：与目标点附近的敌人互换位置。
    #[test]
    fn s013_swap_exchanges_with_enemy() {
        let mut world = World::new(2, 959);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(5.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S013, Some(Vec2::new(d60(5.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        assert!(near_d(world.players[0].pos.x, 5.0, 0.5), "施法者应换到敌人位置，实际 {:?}", world.players[0].pos);
        assert!(near_d(world.players[1].pos.x, 0.0, 0.5), "敌人应被换到施法者原位置，实际 {:?}", world.players[1].pos);
    }

    /// S002 闪电：瞬发射线立即伤害（无前摇等待弹体），写 lightning_visual，KI 击退。
    #[test]
    fn s002_lightning_bolt_hits_instantly() {
        let mut world = World::new(2, 953);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(5.0), Fix64::ZERO); // 300 < 射程 600
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        world.step(
            vec![
                PlayerInput { cast: Some((SkillId::S002, Some(Vec2::new(d60(5.0), Fix64::ZERO)))), ..Default::default() },
                PlayerInput::default(),
            ],
            dt,
        );
        // 施法帧即结算（execute_effects 在 step 内同步跑）。
        assert!(world.players[1].hp < hp1, "闪电应瞬发命中（L1 伤 7），hp {hp1} -> {}", world.players[1].hp);
        assert!(hp1 - world.players[1].hp >= Fix64::from_num(7.0), "直伤至少 7");
        assert!(world.lightning_visual.is_some(), "应写 lightning_visual 供 client 画线");
        assert!(world.players[1].pos.x > d60(5.0), "闪电应击退敌人");
    }

    /// S008 陨石：直飞命中（或到期）触发 200 半径 AoE 爆炸——旁边玩家被波及。
    #[test]
    fn s008_meteor_blast_hits_nearby_players() {
        let mut world = World::new(3, 954);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 直线上两个敌人：500（被弹体直接命中）与 620（只在爆炸半径 200 内）。
        world.players[1].pos = Vec2::new(d60(8.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.players[2].pos = Vec2::new(d60(10.0), Fix64::ZERO);
        world.players[2].move_target = None;
        let hp1 = world.players[1].hp;
        let hp2 = world.players[2].hp;
        world.step(
            vec![
                PlayerInput { cast: Some((SkillId::S008, Some(Vec2::new(d60(12.0), Fix64::ZERO)))), ..Default::default() },
                PlayerInput::default(),
                PlayerInput::default(),
            ],
            dt,
        );
        let none = vec![PlayerInput::default(), PlayerInput::default(), PlayerInput::default()];
        for _ in 0..150 {
            world.step(none.clone(), dt); // 速度 400 → 800 距离需 2s
        }
        assert!(world.players[1].hp < hp1, "陨石直击目标应受伤");
        assert!(world.players[2].hp < hp2, "爆炸半径 200（≈3.3 旧距离）应波及 620 处的第二敌人");
    }

    /// S016 弹跳弹：两敌布阵——第一跳全额 6、跳向第二敌 ×0.8≈4.8；寿命耗尽后消失。
    ///（跳序由 nearest 决定；击退方向沿来向推离，不会把目标推进下一跳判定圈。）
    #[test]
    fn s016_bounce_jumps_with_decay() {
        let mut world = World::new(3, 955);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(5.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.players[2].pos = Vec2::new(d60(-5.0), Fix64::ZERO);
        world.players[2].move_target = None;
        let hp1 = world.players[1].hp;
        let hp2 = world.players[2].hp;
        world.step(
            vec![
                PlayerInput { cast: Some((SkillId::S016, Some(Vec2::new(d60(5.0), Fix64::ZERO)))), ..Default::default() },
                PlayerInput::default(),
                PlayerInput::default(),
            ],
            dt,
        );
        let none = vec![PlayerInput::default(); 3];
        for _ in 0..120 {
            world.step(none.clone(), dt);
        }
        let d1 = (hp1 - world.players[1].hp).to_num::<f64>();
        let d2 = (hp2 - world.players[2].hp).to_num::<f64>();
        assert!((d1 - 6.0).abs() < 0.3, "第一跳应全额 6，实际 {d1}");
        assert!((d2 - 6.0 * 0.8).abs() < 0.3, "第二跳应 ×0.8≈4.8，实际 {d2}");
        let still = world
            .projectiles
            .iter()
            .any(|pr| matches!(pr.kind, ProjectileKind::W098b { proj: crate::skill::W098bProjKind::Bounce, .. }));
        assert!(!still, "弹跳弹寿命（1s）耗尽后应消失，不得无限弹");
    }

    #[test]
    fn t3_jump_decays_damage() {
        let mut world = World::new(3, 71);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        for i in 1..3 {
            world.players[i].pos = Vec2::new(Fix64::from_num(3.0 + i as f64), Fix64::ZERO);
        }
        let hp1 = world.players[1].hp;
        let hp2 = world.players[2].hp;
        let input = vec![
            PlayerInput { cast: Some((SkillId::T3Fast, Some(Vec2::new(Fix64::from_num(4.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
            PlayerInput::default(),
        ];
        for _ in 0..90 {
            world.step(input.clone(), dt);
        }
        assert!(world.players[1].hp < hp1, "跳弹应命中敌人1");
        assert!(world.players[2].hp < hp2, "跳弹应链到敌人2");
    }

    #[test]
    fn t2_volley_and_sweep_spawn_many() {
        // T2b 扇面齐射：一次喷出 4 发
        let mut w1 = World::new(1, 72);
        let dt = Fix64::from_num(1.0 / 60.0);
        w1.players[0].pos = Vec2::ZERO;
        w1.step(vec![PlayerInput { cast: Some((SkillId::T2Volley, Some(Vec2::new(Fix64::from_num(5.0), Fix64::ZERO)))), ..Default::default() }], dt);
        let none = vec![PlayerInput::default()];
        for _ in 0..20 {
            w1.step(none.clone(), dt);
        }
        let bullets = w1.projectiles.iter().filter(|pr| matches!(pr.kind, ProjectileKind::Bullet { .. })).count();
        assert!(bullets >= 4, "扇面齐射应喷出 4 发，实际 {}", bullets);

        // T2 扇扫连射：随时间依次发射，统计峰值弹数
        let mut w2 = World::new(1, 73);
        w2.players[0].pos = Vec2::ZERO;
        w2.step(vec![PlayerInput { cast: Some((SkillId::T2Shot, Some(Vec2::new(Fix64::from_num(5.0), Fix64::ZERO)))), ..Default::default() }], dt);
        let mut peak = 0usize;
        for _ in 0..90 {
            w2.step(none.clone(), dt);
            peak = peak.max(w2.projectiles.iter().filter(|pr| matches!(pr.kind, ProjectileKind::Bullet { .. })).count());
        }
        assert!(peak >= 2, "扇扫连射应先后发射多发自爆弹，峰值 {}", peak);
        // 全部发完后清空发射状态
        assert!(w2.players[0].sweep.is_none(), "发射完应清空扇扫状态");
    }

    #[test]
    fn t3b_bonus_chain_accumulates_damage() {
        let mut world = World::new(2, 74);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[1].pos = Vec2::new(Fix64::from_num(3.0), Fix64::ZERO);
        // 只施放一次，随后空输入推进（否则每帧重施放会不断重置冷却）
        world.step(vec![
            PlayerInput { cast: Some((SkillId::T3Fast2, Some(Vec2::new(Fix64::from_num(5.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..40 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].damageplus > 0.0, "蓄力跳弹命中应累计额外伤害");
        // 回返镖应已飞回施法者并刷新其技能冷却（可立即再发）
        let cd = world.players[0].caster.cooldown_remaining(crate::skill::SkillId::T3Fast2);
        assert!(cd <= Fix64::ZERO, "回返镖到家应刷新蓄力跳弹冷却");
    }

    #[test]
    fn y1_tether_pulls_and_dots() {
        let mut world = World::new(2, 80);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(8.0), Fix64::ZERO);
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        // 施放蓝线回拉，点击在敌人附近锁定它
        world.step(vec![
            PlayerInput { cast: Some((SkillId::Y1BlueLine, Some(Vec2::new(d60(8.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[1].hp < hp1, "回拉线应持续掉血");
        // 敌人应被拉近施法者
        let dist = (world.players[1].pos - world.players[0].pos).length().to_num::<f64>();
        assert!(dist < 7.5 * 60.0, "回拉线应把敌人拉向施法者，实际距离 {}", dist);
    }

    #[test]
    fn y2_pushshot_damages_enemy() {
        let mut world = World::new(2, 81);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[1].pos = Vec2::new(Fix64::from_num(3.0), Fix64::ZERO);
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        let input = vec![
            PlayerInput { cast: Some((SkillId::Y2Delay, Some(Vec2::new(Fix64::from_num(6.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ];
        for _ in 0..50 {
            world.step(input.clone(), dt);
        }
        assert!(world.players[1].hp < hp1, "撞击迟缓弹应命中造成伤害");
        // 命中后应把敌人沿弹-目标方向推离施法者
        assert!(world.players[1].pos.x > Fix64::from_num(3.0), "撞击迟缓弹应把敌人推离");
    }

    #[test]
    fn y2b_bind_line_binds_enemy() {
        let mut world = World::new(2, 82);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(4.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::Y2Suite, Some(Vec2::new(d60(6.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[1].tied(), "束缚线应把线上敌人束缚（禁施法）");
    }

    #[test]
    fn y3b_star_zone_heals_owner_and_hurts_enemy() {
        let mut world = World::new(2, 83);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].hp = Fix64::from_num(40.0);
        world.players[1].pos = Vec2::new(Fix64::from_num(2.0), Fix64::ZERO); // 与星域重叠
        let hp0 = world.players[0].hp;
        let hp1 = world.players[1].hp;
        // 星域放在 (1,0) 附近覆盖敌人且为施法者回血
        world.step(vec![
            PlayerInput { cast: Some((SkillId::Y3Zone2, Some(Vec2::new(Fix64::from_num(1.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[1].hp < hp1, "星域应让范围内的敌人掉血");
        assert!(world.players[0].hp > hp0, "星域应给施法者回血");
    }

    #[test]
    fn arena_shrinks_to_zero() {
        let mut world = World::new(1, 92);
        let dt = Fix64::from_num(1.0 / 60.0);
        // 缩到 0 需要 20/0.35 ≈ 57s ≈ 3429 帧；跑够久确认能缩到 0（而非停在旧阈值 3.0）。
        let none = vec![PlayerInput::default()];
        for _ in 0..3600 {
            world.step(none.clone(), dt);
        }
        assert!(world.arena_radius <= Fix64::from_num(0.01), "场地应缩到 0，实际 {:?}", world.arena_radius);
    }

    #[test]
    fn f_self_explode_hurts_enemies_and_self() {
        let mut world = World::new(2, 90);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(r16(1.5), Fix64::ZERO); // 在自爆半径内
        world.players[1].move_target = None;
        // 施放蓄力自爆（windup 1s），随后空输入让它吟唱完成
        world.step(vec![
            PlayerInput { cast: Some((SkillId::Test03, None)), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..80 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[1].hp < world.players[1].max_hp, "自爆应伤到范围内敌人");
        // Unity：GetHurt(min(10, hp-1))，满血(100)自爆应最多自扣 10 → 剩 90，而非被固定扣到 1 血。
        let expected = world.players[0].max_hp - Fix64::from_num(10);
        assert!(
            (world.players[0].hp - expected).abs() < Fix64::from_num(0.01),
            "施法者应最多自扣 10 血（Unity min(10,hp-1)），当前 {:?} 预期 {expected:?}",
            world.players[0].hp
        );
        assert!(world.players[0].hp > Fix64::from_num(80.0), "自爆不应再把满血施法者打到 1 血");
    }

    #[test]
    fn f_self_explode_low_hp_floor_is_self_stay() {
        // Unity 低血量分支：GetHurt(min(10, hp-1))，当 hp<=11 时扣 hp-1 → 保底留 1 血。
        let mut world = World::new(2, 93);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[0].hp = Fix64::from_num(5);
        world.players[1].pos = Vec2::new(Fix64::from_num(100.0), Fix64::ZERO); // 远离自爆，只测施法者自残
        world.players[1].move_target = None;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::Test03, None)), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..80 {
            world.step(none.clone(), dt);
        }
        assert!(
            world.players[0].hp > Fix64::ZERO && world.players[0].hp <= Fix64::from_num(1.1),
            "低血量(5)自爆应保底留 1 血（Unity 扣 hp-1），当前 {:?}",
            world.players[0].hp
        );
    }

    #[test]
    fn g_straight_bomb_damages_enemy() {
        let mut world = World::new(2, 91);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[1].pos = Vec2::new(Fix64::from_num(3.0), Fix64::ZERO);
        let hp1 = world.players[1].hp;
        let input = vec![
            PlayerInput { cast: Some((SkillId::Test01, Some(Vec2::new(Fix64::from_num(6.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ];
        for _ in 0..50 {
            world.step(input.clone(), dt);
        }
        assert!(world.players[1].hp < hp1, "爆炸弹应命中造成伤害");
    }

    #[test]
    fn projectile_kill_is_recorded_in_kills_and_eliminated_order() {
        // 回归 P3：被弹体/爆炸击杀曾因 step-7 死亡结算循环 `if !alive { continue }`
        // 跳过而永不记账，导致击杀金币全不发、名次奖励发错人。
        let mut world = World::new(2, 91);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[1].pos = Vec2::new(Fix64::from_num(3.0), Fix64::ZERO);
        world.players[1].hp = Fix64::from_num(1.0); // 一击致命，但需靠 Test01 爆炸弹打死
        let input = vec![
            PlayerInput { cast: Some((SkillId::Test01, Some(Vec2::new(Fix64::from_num(6.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ];
        let mut guard = 0;
        while world.players[1].alive && guard < 120 {
            world.step(input.clone(), dt);
            guard += 1;
        }
        assert!(!world.players[1].alive, "玩家1 应被爆炸弹击杀");
        // 击杀记账应非空，且记录为 (击杀者=玩家0, 受害者=玩家1)
        assert!(!world.kills_this_round.is_empty(), "击杀应被记账（原 bug：空）");
        assert!(world.kills_this_round.contains(&(0, 1)), "kills_this_round 应含 (0,1)，实际 {:?}", world.kills_this_round);
        assert!(world.eliminated_order.contains(&1), "eliminated_order 应含玩家1，实际 {:?}", world.eliminated_order);
        // placement 冠军应为存活者（玩家0）
        assert_eq!(world.placement()[0], 0, "冠军应是存活者玩家0");
    }

    #[test]
    fn shift_queue_move_then_move() {
        let mut world = World::new(1, 100);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        // 在同一帧批量压入两条移动指令：先到 (4,0)，再到 (8,0)
        world.step(vec![PlayerInput { queued: vec![
            Cmd::Move(Vec2::new(Fix64::from_num(4.0), Fix64::ZERO)),
            Cmd::Move(Vec2::new(Fix64::from_num(8.0), Fix64::ZERO)),
        ], ..Default::default() }], dt);
        let none = vec![PlayerInput::default()];
        // 跑足够久让两条移动都走完
        for _ in 0..300 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].cmd_empty(), "指令应全部执行完");
        assert!(near(world.players[0].pos.x, 8.0, 0.5), "应先到 4 再到 8，实际 {:?}", world.players[0].pos);
    }

    #[test]
    fn shift_queue_move_then_cast() {
        let mut world = World::new(2, 101);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[1].pos = Vec2::new(Fix64::from_num(6.0), Fix64::ZERO);
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        // 压入：先移动到 (3,0)，再朝 (6,0) 施放掷弹(Rock)
        world.step(vec![
            PlayerInput { queued: vec![Cmd::Move(Vec2::new(Fix64::from_num(3.0), Fix64::ZERO))], ..Default::default() },
            PlayerInput::default(),
        ], dt);
        world.step(vec![
            PlayerInput { queued: vec![Cmd::Cast(SkillId::Rock, Some(Vec2::new(Fix64::from_num(6.0), Fix64::ZERO)))], ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..240 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].cmd_empty(), "指令应全部执行完");
        assert!(world.players[1].hp < hp1, "队列里的施法指令应真正施放并生效");
    }

    #[test]
    fn clear_queue_signal_empties_world_queue() {
        let mut world = World::new(1, 103);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        // 同一帧排两条移动：第一条立即 pop 转成 move_target 执行，第二条留在队列等待。
        world.step(vec![PlayerInput {
            queued: vec![
                Cmd::Move(Vec2::new(Fix64::from_num(4.0), Fix64::ZERO)),
                Cmd::Move(Vec2::new(Fix64::from_num(8.0), Fix64::ZERO)),
            ],
            ..Default::default()
        }], dt);
        assert_eq!(world.players[0].cmd_len, 1, "第一条已执行，队列应剩第二条");
        // 玩家仍在朝 4 走（move_target 未到达）时清队列 → 未执行的第二条被清掉。
        world.step(vec![PlayerInput { clear_queue: true, ..Default::default() }], dt);
        assert!(world.players[0].cmd_empty(), "clear_queue 应清掉队列里未执行的移动");
        // 跑足够久：第二条（到 8）已被清除，玩家不应再走向 8。
        let none = vec![PlayerInput::default()];
        for _ in 0..300 {
            world.step(none.clone(), dt);
        }
        assert!(
            world.players[0].pos.x.to_num::<f64>() < 7.0,
            "clear_queue 应阻止第二条移动执行，实际 x={:?}",
            world.players[0].pos
        );
    }

    #[test]
    fn stop_move_clears_move_target_in_world() {
        let mut world = World::new(1, 104);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        // 让玩家朝远处移动
        world.step(vec![PlayerInput { set_target: Some(Vec2::new(Fix64::from_num(50.0), Fix64::ZERO)), ..Default::default() }], dt);
        // 几帧后 stop_move 应清掉 move_target
        let none = vec![PlayerInput::default()];
        for _ in 0..6 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].move_target.is_some(), "移动中应有目标");
        world.step(vec![PlayerInput { stop_move: true, ..Default::default() }], dt);
        assert!(world.players[0].move_target.is_none(), "stop_move 应清除 move_target");
    }

    #[test]
    fn s_clears_command_queue() {
        // S 清空队列：压入几条指令后手动清空
        let mut p = crate::player::Player::new(0, Vec2::ZERO, Fix64::ONE);
        p.cmd_push(Cmd::Move(Vec2::new(Fix64::ONE, Fix64::ZERO)));
        p.cmd_push(Cmd::Cast(SkillId::Boost, None));
        assert_eq!(p.cmd_len, 2);
        p.cmd_clear();
        assert!(p.cmd_empty());
    }

    /// 试验场（sandbox）：不缩圈、round_over 恒 false（供单机技能试验场“不秒结束”）。
    #[test]
    fn sandbox_never_ends_and_no_shrink() {
        let dt = Fix64::from_num(1.0 / 60.0);
        // 正常模式：1 个玩家会立即 round_over。
        let normal = World::new(1, 7);
        assert!(normal.round_over(), "仅 1 玩家时默认对局视为结束");
        // sandbox：1 玩家也永不结束、不缩圈。
        let mut sw = World::new(1, 7);
        sw.sandbox = true;
        let start_r = sw.arena_radius;
        let none = vec![PlayerInput::default()];
        for _ in 0..120 {
            sw.step(none.clone(), dt);
        }
        assert!(!sw.round_over(), "sandbox 永不判结束");
        assert_eq!(sw.arena_radius, start_r, "sandbox 不缩圈");
    }
}
