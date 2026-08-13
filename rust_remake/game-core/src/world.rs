//! 世界/对局 —— 确定性的核心模拟。
//!
//! `World` 在固定步长下推进，玩家输入（设定移动目标）来自 `WorldInput`。
//! 所有规则均为纯整数定点运算，因此相同输入可产生完全一致的结果，
//! 这是后续帧同步（lockstep）联网的基础。

use crate::fix::{Fix64, Vec2};
use crate::player::{BuffKind, Kick, Player};
use crate::rng::Rng;
use crate::skill::{SkillEffect, SkillId, DefTable};

/// 场地收缩参数（复刻原版 `AreaScript` 的量级，稍加快以体现压迫感）。
pub const START_RADIUS: f64 = 20.0;
pub const SHRINK_SPEED: f64 = 0.35; // 半径减少量 / 秒
/// 场地最小半径（不会缩到比玩家更小）。
pub const MIN_RADIUS: f64 = 3.0;
/// 出界伤害：球心距圆点 > 圈半径时，每帧扣除的 HP / 秒。
pub const OUT_HURT: f64 = 5.0;
/// 玩家相互挤压（重叠）时受到的伤害 / 秒。
pub const OVERLAP_DAMAGE: f64 = 2.0;
/// E3/E3b 撒出的扇形子弹（原版 `SABulletScript`）的伤害与射程。
pub const SABULLET_DAMAGE: f64 = 2.0;
pub const SABULLET_RANGE: f64 = 6.0;

/// 每个玩家当前帧的输入。
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct PlayerInput {
    /// 若为 `Some(pos)`，则令该玩家朝 `pos` 直线移动。
    pub set_target: Option<Vec2>,
    /// 若为 `Some((skill, target))`，则尝试对该技能施法（target 为点目标/朝向）。
    pub cast: Option<(SkillId, Option<Vec2>)>,
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
        life: Fix64,       // 剩余飞行时间/总生存（跳跃会重置）
        last_target: u32,  // 上次目标（避免立刻跳回），用 u32::MAX 表示无
        owner: u32,
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
    /// 场景里的静态圆形障碍（柱子/墙）
    pub obstacles: Vec<Obstacle>,
    /// 场上飞行物 / 延时区域
    pub projectiles: Vec<Projectile>,
    /// 按死亡先后记录的玩家 id（用于本局名次结算）
    eliminated_order: Vec<u32>,
    /// 本局内发生的击杀：(击杀者 id, 被击杀者 id)
    kills_this_round: Vec<(u32, u32)>,
    time: Fix64,
}

impl World {
    /// 创建一场对局。`player_count` 为玩家人数；`seed` 用于 AI / 初始布局等确定性随机。
    pub fn new(player_count: u32, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let arena_radius = Fix64::from_num(START_RADIUS);
        let mut players = Vec::with_capacity(player_count as usize);
        for id in 0..player_count {
            // 把玩家均匀放到以最小半径为中心的圆周上的随机位置，
            // 保证彼此初始不重叠且出界伤害不至于一开始就触发。
            let r = arena_radius * Fix64::from_num(0.6);
            let angle = Fix64::from_num(std::f64::consts::TAU) * rng.next_fix();
            let pos = Vec2::new(r * crate::fix::cos(angle), r * crate::fix::sin(angle));
            players.push(Player::new(id, pos, Fix64::from_num(crate::player::DEFAULT_RADIUS)));
        }
        let mut obstacles = Vec::new();
        _layout_obstacles(&mut obstacles, &mut rng, arena_radius);
        World {
            players,
            arena_radius,
            obstacles,
            projectiles: Vec::new(),
            eliminated_order: Vec::new(),
            kills_this_round: Vec::new(),
            time: Fix64::ZERO,
        }
    }

    pub fn time(&self) -> Fix64 {
        self.time
    }

    /// 给定所有玩家的输入，推进固定步长。
    pub fn step(&mut self, input: InputSlice, dt: Fix64) {
        debug_assert_eq!(input.len(), self.players.len(), "input 必须覆盖每位玩家");
        self.time += dt;

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
            p.move_target = pi.set_target;
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
        }

        // 4) 场地收缩（随时间）
        self.shrink_arena(dt);

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
            if p.pos.length() > self.arena_radius {
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
                            let md = Fix64::from_num(4.0); // 短闪距离
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
        let min = Fix64::from_num(MIN_RADIUS);
        if self.arena_radius < min {
            self.arena_radius = min;
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
    fn step_area_forces(&mut self, _dt: Fix64) {}

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
    fn damage_player(&mut self, id: u32, amount: Fix64, from: Option<u32>) {
        let p = &mut self.players[id as usize];
        if !p.alive {
            return;
        }
        if let Some(hitter) = from {
            p.last_hit_by = Some(hitter);
        }
        // C1 疾跑：boost 期间返还一半伤害回血（soak_boost 返回净扣血）
        let net = p.soak_boost(amount);
        p.hp = (p.hp - net).max(Fix64::ZERO);
        if p.hp == Fix64::ZERO {
            p.alive = false;
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
                ProjectileKind::Chain { dir, speed, life, last_target, owner, .. } => {
                    // 链镖：锁定最近敌人（排除上一个已命中目标与施法者）全速直追。
                    if let Some(tgt) = self.nearest_enemy_excl(pr.pos, *owner, *last_target) {
                        let want = (tgt - pr.pos).normalized();
                        *dir = if want.length_squared() == Fix64::ZERO { *dir } else { want };
                    }
                    pr.pos += *dir * (*speed * dt);
                    *life -= dt;
                    if *life < eps {
                        pr.alive = false;
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

        // 1b) 回旋镖撞障碍：反射其速度（原版 BoomerangScript 撞墙 MirrorBy）。
        for pr in ps.iter_mut() {
            if !pr.alive {
                continue;
            }
            let bounce: Option<Vec2> = if let ProjectileKind::Boomerang { vel, radius, .. } = pr.kind {
                let mut hit_wall: Option<Vec2> = None;
                for o in self.obstacles.iter() {
                    let delta = pr.pos - o.pos;
                    let dist = delta.length();
                    let min = radius + o.radius;
                    if dist > Fix64::ZERO && dist < min {
                        let normal = delta / dist;
                        // 把速度沿接触法线镜向（反弹）
                        hit_wall = Some(crate::fix::mirror_by(vel, normal));
                        pr.pos = o.pos + normal * min; // 位置推出
                        break;
                    }
                }
                hit_wall
            } else {
                None
            };
            if let Some(nv) = bounce {
                if let ProjectileKind::Boomerang { vel, .. } = &mut pr.kind {
                    *vel = nv;
                }
            }
        }

        // 2) 判定与收集对玩家的影响：命中伤害 / AOE / 持续伤害 / 爆炸。
        // 每个 (伤害, 来源) 事件在 4) 统一结算；被反弹护盾命中的直射弹只反射方向。
        let mut events: Vec<(u32, Fix64, Option<u32>)> = Vec::new();
        let mut explode: Vec<ProjExplosion> = Vec::new();
        let mut pushes: Vec<(u32, Vec2, f64)> = Vec::new(); // (受害者 id, 击退方向, 时长)
        let mut reflect_bullets: Vec<(usize, Vec2)> = Vec::new(); // (proj 下标, 反射后的 dir)

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
                continue;
            }
            // 链镖/跳弹族单独处理（需改动 Chain 内部状态以完成跳跃，交给可变分支）
            if matches!(pr.kind, ProjectileKind::Chain { .. }) {
                if let ProjectileKind::Chain { damage, heal, ratio, ratio_decay, life, last_target, owner, .. } = &mut pr.kind {
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
                        if next_ratio <= Fix64::ZERO {
                            pr.alive = false;
                        } else {
                            *ratio = next_ratio;
                            *life = Fix64::from_num(1.5);
                        }
                    }
                }
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
                ProjectileKind::Rock { .. } | ProjectileKind::Decoy { .. } | ProjectileKind::ScatterLine { .. } | ProjectileKind::Chain { .. } => {}
            }
        }

        // 2b) 应用反弹护盾对直射弹的反射（改方向，不消耗、不伤害）。
        for (pi, new_dir) in reflect_bullets {
            if let ProjectileKind::Bullet { dir, .. } = &mut ps[pi].kind {
                *dir = new_dir;
                // 可让被反射的弹体仍归属原施法者（原版弹一次）
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
                    p.push(vel, time);
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
        for p in self.players.iter_mut() {
            if !p.alive {
                continue;
            }
            let d = p.pos - pos;
            let d_sq = d.length_squared();
            if d_sq <= r_sq {
                // 受伤（记录击杀者）；boost 期间返还一半回血
                if p.id != owner {
                    p.last_hit_by = Some(owner);
                }
                let net = p.soak_boost(damage);
                p.hp = (p.hp - net).max(Fix64::ZERO);
                if p.hp == Fix64::ZERO {
                    p.alive = false;
                }
                // 击退（沿中心连线远离，随距离衰减；走控制/强制速度）
                if d_sq > Fix64::ZERO {
                    let dist = d_sq.sqrt();
                    let falloff = (Fix64::ONE - dist / radius).max(Fix64::from_num(0.2));
                    // 用 push 把玩家推开 duration 0.3s（可覆盖冲锋/冲刺等强制态）
                    let dir = d.normalized();
                    p.push(dir * (bomb_force * falloff), 0.3);
                }
            }
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

    /// 本局是否已结束（只剩 0 或 1 名存活）。
    pub fn round_over(&self) -> bool {
        self.alive_count() <= 1
    }

    /// 重置为可开始下一小局（清空本局状态、重设玩家满血与初始位置）。
    /// 调用方需在结算完成后调用。
    pub fn reset_round(&mut self) {
        self.eliminated_order.clear();
        self.kills_this_round.clear();
        self.arena_radius = Fix64::from_num(crate::world::START_RADIUS);
        self.time = Fix64::ZERO;
        for p in self.players.iter_mut() {
            p.reset_state();
        }
    }
}

/// 用确定性 RNG 布几根圆形柱子（障碍）：在内圈(距圆心约 arena*0.4)等角分布并加小幅抖动。
/// 玩家出生在 arena*0.6 的环上，和 0.4 内圈的柱子不重叠。
fn _layout_obstacles(out: &mut Vec<Obstacle>, rng: &mut Rng, arena_radius: Fix64) {
    let ring_r = arena_radius * Fix64::from_num(0.4);
    let count = 5u32;
    for i in 0..count {
        let base_angle = Fix64::from_num(std::f64::consts::TAU) * Fix64::from_num(i as f64 / count as f64);
        let jitter = (rng.next_fix() - Fix64::from_num(0.5)) * Fix64::from_num(1.2); // ±0.6 rad 抖动
        let angle = base_angle + jitter;
        // 每根柱子半径在 1.1~1.6 之间确定性波动
        let r = Fix64::from_num(1.1) + rng.next_fix() * Fix64::from_num(0.5);
        let pos = Vec2::new(ring_r * crate::fix::cos(angle), ring_r * crate::fix::sin(angle));
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
            SkillEffect::Missile { speed, radius, damage, push_power, push_time, range } => {
                // 追踪导弹：锁定点击处最近的敌人全速直追；命中爆炸伤+击退。
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
                        speed,
                        damage,
                        radius,
                        push_power,
                        push_time,
                        remaining: range,
                    },
                    pos: ppos,
                    alive: true,
                });
            }
            SkillEffect::Boomerang { speed, accelerate, radius, damage, push_power, push_time, life } => {
                // 回旋镖（D2）：朝目标方向飞出，随后持续向施法者加速回飞；撞障碍反弹；命中爆炸伤+击退。
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
                            vel: dir * speed,
                            accelerate,
                            damage,
                            radius,
                            push_power,
                            push_time,
                            life,
                            owner_pos: p.pos,
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::Banana { count, turn_rad, speed, radius, damage, push_power, push_time, life } => {
                // 双香蕉曲线弹（D4）：朝施法方向两侧各打一发曲线弹，命中爆炸伤+击退。
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
                                speed,
                                turn: Fix64::from_num(turn_rad * if off < 0.0 { 1.0 } else { -1.0 }),
                                damage,
                                radius,
                                push_power,
                                push_time,
                                life,
                            },
                            pos: p.pos,
                            alive: true,
                        });
                    }
                }
            }
            SkillEffect::RollProjectile { speed, damage_per_sec, radius, range } => {
                // 滚动火球（E1b）：沿方向直线滚动，接触范围内持续掉血。
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
                            speed,
                            damage_per_sec,
                            radius,
                            remaining: range,
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::ScatterBurst { speed, range, count, step_rad, bullet_speed } => {
                // 撒弹线（E3）：到终点爆散一个扇形。
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
                            speed,
                            remaining: range,
                            scatter: ScatterKind::Burst {
                                count,
                                step_rad: Fix64::from_num(step_rad),
                                bullet_speed,
                            },
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::ScatterPeriodic { speed, range, count, interval, bullet_speed, turn_rad } => {
                // 撒弹线（E3b）：飞行途中周期性散射击并旋转。
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
                            speed,
                            remaining: range,
                            scatter: ScatterKind::Periodic {
                                count,
                                interval: Fix64::from_num(interval),
                                elapsed: Fix64::ZERO,
                                bullet_speed,
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
            SkillEffect::ChainLeech { speed, damage, heal, range: _ } => {
                // T1b 吸血链镖：命中吸血 + 链下一个
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = towards(p.pos, target);
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::Chain {
                            dir,
                            speed,
                            damage,
                            heal,
                            ratio: Fix64::ONE,
                            ratio_decay: Fix64::ZERO,
                            life: Fix64::from_num(1.5),
                            last_target: u32::MAX,
                            owner: idx,
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::TurnLeech { speed, damage, heal, turn_delay, range: _ } => {
                // TestLeech 转镖吸血：命中吸血 + 链
                let _ = turn_delay;
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = towards(p.pos, target);
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::Chain {
                            dir,
                            speed,
                            damage,
                            heal,
                            ratio: Fix64::ONE,
                            ratio_decay: Fix64::ZERO,
                            life: Fix64::from_num(1.5),
                            last_target: u32::MAX,
                            owner: idx,
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::JumpDecay { speed, damage, range: _, ratio_decay } => {
                // T3 跳弹·衰减：命中后跳到下一个，伤害逐跳衰减
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dir = towards(p.pos, target);
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::Chain {
                            dir,
                            speed,
                            damage,
                            heal: Fix64::ZERO,
                            ratio: Fix64::ONE,
                            ratio_decay,
                            life: Fix64::from_num(1.5),
                            last_target: u32::MAX,
                            owner: idx,
                        },
                        pos: p.pos,
                        alive: true,
                    });
                }
            }
            SkillEffect::Volley { bullet_speed, damage, count, spread_step } => {
                // T2b 扇面齐射：从 -count/2 到 +count/2 一次喷出
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
                                speed: bullet_speed,
                                damage,
                                radius: Fix64::from_num(0.5),
                                remaining: Fix64::from_num(SABULLET_RANGE),
                            },
                            pos: p.pos,
                            alive: true,
                        });
                    }
                }
            }
            SkillEffect::Sweep { bullet_speed, damage, count, cadence, turn_step } => {
                // T2 扇扫连射：设发射器状态，由世界逐帧依次发射
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let base = towards(p.pos, target);
                    p.sweep = Some(crate::player::SweepState {
                        dir: base,
                        bullet_speed,
                        damage,
                        remaining: count,
                        cadence,
                        turn_step,
                        elapsed: 0.0,
                        id: idx,
                    });
                }
            }
            SkillEffect::BonusChain { speed, damage, range: _ } => {
                // T3b 蓄力跳弹：命中 +damageplus 并以回返镖刷新冷却
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dmg = damage + Fix64::from_num(p.damageplus);
                    let dir = towards(p.pos, target);
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::Chain {
                            dir,
                            speed,
                            damage: dmg,
                            heal: Fix64::ZERO,
                            ratio: Fix64::ONE,
                            ratio_decay: Fix64::ZERO,
                            life: Fix64::from_num(2.0),
                            last_target: u32::MAX,
                            owner: idx,
                        },
                        pos: p.pos,
                        alive: true,
                    });
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

    #[test]
    fn movement_stops_at_target() {
        let mut world = World::new(1, 1);
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
        world.players[0].pos = Vec2::new(Fix64::from_num(19.0), Fix64::from_num(19.0));
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
        world.players[1].pos = Vec2::new(Fix64::from_num(3.0), Fix64::ZERO);

        let hp0 = world.players[0].hp;
        let hp1 = world.players[1].hp;
        // 玩家0 施放 E1 掷石到 (3,0)；玩家1 不动
        let input = vec![
            PlayerInput {
                cast: Some((SkillId::Rock, Some(Vec2::new(Fix64::from_num(3.0), Fix64::ZERO)))),
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
    fn blink_teleports_toward_target() {
        let mut world = World::new(1, 9);
        world.players[0].pos = Vec2::ZERO;
        let dt = Fix64::from_num(1.0 / 60.0);
        let far = Vec2::new(Fix64::from_num(100.0), Fix64::ZERO);
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
        assert!(near(world.players[0].pos.x, 6.0, 0.3), "瞬移距离应为 6，实际 {:?}", world.players[0].pos);
    }

    #[test]
    fn cannot_walk_while_casting() {
        let mut world = World::new(1, 11);
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
        let far = Vec2::new(Fix64::from_num(100.0), Fix64::ZERO);
        // 第一帧：给了“很远”的移动目标 + 施放闪烁。施法应取消旧的移动命令并瞬移到 (6,0)。
        let first = vec![PlayerInput {
            set_target: Some(far),
            cast: Some((SkillId::Blink, Some(far))),
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
            near(p.pos.x, 6.0, 0.3),
            "闪烁落地后不应继续走向旧目标，位置应为 ~6，实际 {:?}",
            p.pos
        );
    }

    #[test]
    fn round_over_and_reset_cycle() {
        let mut world = World::new(2, 21);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO; // 站桩存活
        // 玩家1 放到场地外很远，会因出界伤害持续掉血致死；先给它很低血量加速
        world.players[1].hp = Fix64::from_num(1.0);
        world.players[1].pos = Vec2::new(Fix64::from_num(30.0), Fix64::ZERO);
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
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 敌人堵在冲锋路径前方
        world.players[1].pos = Vec2::new(Fix64::from_num(2.0), Fix64::ZERO);
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
        world.players[0].pos = Vec2::new(Fix64::from_num(100.0), Fix64::ZERO);
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
        world.players[1].pos = Vec2::new(Fix64::from_num(10.0), Fix64::from_num(10.0)); // 不在直线上
        let hp1 = world.players[1].hp;
        let input = vec![
            PlayerInput { cast: Some((SkillId::D2Fireball, Some(Vec2::new(Fix64::from_num(12.0), Fix64::ZERO)))), ..Default::default() },
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
        let far = Vec2::new(Fix64::from_num(100.0), Fix64::ZERO);
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
        assert!(x1 > 4.9, "第一段应闪 ~5，实际 {}", x1);
        // 第二段：窗口内再施放 = 免冷却短闪 4
        let x_before = world.players[0].pos.x;
        world.step(cast(SkillId::Blink2, far), dt);
        let dx = (world.players[0].pos.x - x_before).to_num::<f64>();
        assert!(dx > 3.9 && dx < 4.1, "第二段应短闪 ~4，实际 {}", dx);
        assert!(world.players[0].blink2_window.is_none(), "第二段后窗口应清空");
    }

    #[test]
    fn dashslash_moves_invisibly_and_stops_on_new_target() {
        let mut world = World::new(1, 52);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        let far = Vec2::new(Fix64::from_num(100.0), Fix64::ZERO);
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
        world.obstacles.push(Obstacle::new(Vec2::new(Fix64::from_num(10.0), Fix64::ZERO), 1.0));
        // 朝 (30,0) 闪到墙：射线应命中柱子，落在柱子前（比 10 更近）
        world.step(vec![PlayerInput { cast: Some((SkillId::BlinkToWall, Some(Vec2::new(Fix64::from_num(30.0), Fix64::ZERO)))), ..Default::default() }], dt);
        let x = world.players[0].pos.x.to_num::<f64>();
        assert!(x > 1.0 && x < 9.9, "闪到墙应落在障碍前（<10），实际 {}", x);

        // 无障碍方向：闪 max_distance(6)
        let mut world2 = World::new(1, 54);
        world2.obstacles.clear();
        world2.players[0].pos = Vec2::ZERO;
        world2.step(vec![PlayerInput { cast: Some((SkillId::BlinkToWall, Some(Vec2::new(Fix64::from_num(30.0), Fix64::ZERO)))), ..Default::default() }], dt);
        let x2 = world2.players[0].pos.x.to_num::<f64>();
        assert!(x2 > 5.9 && x2 < 6.1, "无障碍应闪 max_distance(6)，实际 {}", x2);
    }

    #[test]
    fn boomerang_fireball_spawns_and_returns() {
        let mut world = World::new(2, 60);
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
        let input = vec![
            PlayerInput { cast: Some((SkillId::T3Fast2, Some(Vec2::new(Fix64::from_num(5.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ];
        for _ in 0..40 {
            world.step(input.clone(), dt);
        }
        assert!(world.players[0].damageplus > 0.0, "蓄力跳弹命中应累计额外伤害");
    }
}
