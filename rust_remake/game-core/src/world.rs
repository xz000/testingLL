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
/// 出界伤害：球心距圆点 > 圈半径时，每帧扣除的 HP / 秒。
/// 圈外 = 熔岩（098b 语义统一，D8/M5）：踩上（出圈）每秒受 `Uo×10` 伤害。
pub const LAVA_HURT: f64 = Balance::default().out_hurt;
/// 兼容旧名（测试引用）。
pub const OUT_HURT: f64 = LAVA_HURT;
/// E3/E3b 撒出的扇形子弹（原版 `SABulletScript`）的伤害与射程。
pub const SABULLET_DAMAGE: f64 = Balance::default().sabullet_damage;
pub const SABULLET_RANGE: f64 = Balance::default().sabullet_range;
/// 暗物质（S018A）的**伤害**半径：098c `hc` 里 `Rr<$57E40`（=360000=600²）是**拉拽**半径（600，
/// 已存于 `SkillGrowth::radius`）；伤害另有更小的门限 `Rr<75000` → √75000 ≈ 273.9。
/// 两者不同，故伤害半径单独用此常量（引自 098c JASS `hc`）。
pub const DARK_MATTER_DAMAGE_RADIUS: f64 = 273.86;

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

/// 收缩默认总时长（秒）：满员时从开始收缩到缩到 0。90 ≈ 098c 1 人局 9 环 × `wo`=10s。
pub(crate) const DEFAULT_SHRINK_TOTAL_SECS: f64 = 90.0;

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
    /// 镜像分身（C 栏）：跟随施法者、模仿移动并周期施放火球的分身。
    /// `offset` 为相对施法者的固定偏移；`fire_timer` 倒计时到 0 则向最近敌人发射火球（伤害 = `fire_dmg`）。
    Clone {
        owner: u32,
        offset: Vec2,
        fire_timer: Fix64,
        fire_cd: Fix64,
        fire_dmg: Fix64,
        remaining: Fix64,
    },
    /// 引力场（Y3）：飞行场持续把附近敌人吸向场中心。
    Gravity {
        dir: Vec2,
        speed: Fix64,
        radius: Fix64,
        pull_speed: Fix64,
        /// 每秒伤害（098c 黑洞 mc：伤敌 0.3+0.2×L；A 形态持有，B 形态用力场 Star）。
        damage_per_sec: Fix64,
        remaining: Fix64,
    },
    /// 星域持续伤（Y3b）：静态区域，范围内敌掉血、对施法者回血。
    Star {
        owner: u32,
        radius: Fix64,
        damage_per_sec: Fix64,
        heal_per_sec: Fix64,
        remaining: Fix64,
        /// 力场形态（B4-Y）：治疗范围内全部队友（否则只奶 owner）。
        heal_team: bool,
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
        /// 命中副作用（S017 残废 / S019 拉拽；默认 Ki）。
        on_hit: crate::skill::W098bOnHit,
        /// 副作用时长（growth.duration 求值：残废 (4+0.25L)s / 锁链 0.5s）。
        debuff_dur: Fix64,
        /// 回旋镖横向侧偏速度（098c Wb=300/s，每次施放左右交替；D9 技能手感批）。
        lateral: Fix64,
        /// 回旋镖出程方向（弧线物理的前向轴）。
        forward_dir: Vec2,
        /// 到点碎裂弹片数（S009·目标形态=6；0=不碎裂，B4）。
        burst: u8,
        /// 区域形态侧弹发射间隔（S009·区域；ZERO=不发射，B4）。
        emit_cooldown: Fix64,
        /// 区域形态侧弹当前发射角（螺旋推进，B4）。
        emit_angle: f64,
        /// 回旋镖出程距离（098c cO：前向匀减速到 0 的位置）。
        out_dist: Fix64,
        /// 击中柱子时按 098c `xv` 反弹的**反弹系数**（0=被挡下消失；1=满反弹；S008=.75）。
        /// 反弹的同时仍按 098c 对柱子造成伤害（nx=40 可摧毁）。
        pillar_rest: Fix64,
        pillar_bounce: bool,
        /// 红链（S019B）附加闪电伤害（098c `sc`：目标为友军/柱子时引发，1.0→3.4）；其余技能为 0。
        lightning_dmg: Fix64,
    },
    /// 陨石落点（S008A，098c `iB`/`oB`）：**无飞行弹体**——2D 原生化为「落点定时爆炸」。
    /// 到点由 `oB` 规则结算：范围内异队玩家受 `damage × (1 - d/falloff_denom)` 并击退。
    DelayedBlast {
        /// AOE 半径（098c `qI = 210×√(1+.25×远程精通)`）。
        radius: Fix64,
        /// 中心伤害（098c `Zb` 的 `12+2L`）。
        damage: Fix64,
        /// KI 击退系数 JI。
        kb_ji: Fix64,
        /// 距离衰减分母（098c `400+40×xi`）。
        falloff_denom: Fix64,
        /// 剩余时间（098c `ev=1.35`），到 0 落地爆炸。
        remaining: Fix64,
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
const W098B_KB_TIME: f64 = 0.35;/// 098c 击退初速封顶（war3 单位/s）：`(100+魔法)×gx×JI` 在高张力下可达数千，
/// 与移速 210 相比已是 10 倍级——封顶防极端等级把人推出半张图。
const W098B_KB_MAX_SPEED: f64 = 2000.0;
/// 098b 火球点燃时长（秒）：consolidated S000「点燃 2.5×jn」。
const W098B_IGNITE_SECONDS: f64 = 2.5;

/// 098c 英雄撞柱/撞界的弹性系数 `xv`：`FR` 创建英雄时 `set xv[i]=.5`
/// （war3map_pretty.j:4953 `OO(1,..)` → `nv==1`，4979 `set xv[i]=.5`）。
/// 撞障碍走 `set Q=-Q*xv`（8656/8674）→ 该轴速度**反向 ×0.5（半速反弹）**，切向保留。
/// （投射物另有 `xv=1` 全反射，见弹体撞柱分支。）
const PLAYER_OBS_RESTITUTION: f64 = 0.5;

/// 098c KI 击退初速：`(100 + 目标当前魔法) × gX × Gn[攻] × hn[受] × JI`（单位/秒）。
/// 魔法为挨打回魔的张力值（出生 0 → 基数 100；挨打越多被推越远）。
/// `Gn[攻]` = 攻方伤害成长/灼烧惩罚；`hn[受]` = 受方受伤倍率（`dmg_taken_mult`）；
/// `Hn[受]`（精通击退减免）由 `push_knockback` 内的 `effective_kb_reduction` 承担。
/// 封顶防极端：初速上限 [`W098B_KB_MAX_SPEED`]。
fn warlock_ki_knockback(victim_mana: f64, gx: Fix64, ji: Fix64, atk_gn: f64, vic_hn: f64) -> Fix64 {
    let raw = Fix64::from_num((100.0 + victim_mana) * atk_gn * vic_hn) * gx * ji;
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
            | ProjectileKind::BindLine { .. }
            | ProjectileKind::Clone { .. }
            | ProjectileKind::DelayedBlast { .. } => return None,
        })
    }

    /// 098c「弹体互撞」的类别与旗标（简化版）。
    ///
    /// 实证：098c 每个弹体带类别 `nv` 与 `Av[3p+1..3]` 互撞旗标；相撞时两边各跑自己的处理器
    /// （火球 `ib`：对被撞弹体结算伤害并自毁 `IA(nr)`；回旋镖 `Ub` 只开 1/3 类 → 与同类不互撞）。
    /// 098c 弹体有 HP、我们无，故简化为**相撞互毁**（岩浆另有「吸收」路径，不在此列）。
    /// 返回 `(class, 可互撞的 class 位掩码)`；`None` = 不参与弹体互撞。
    fn missile_collision(&self) -> Option<(u8, u8)> {
        const C1: u8 = 1 << 0; // 弹（直射/追踪/弹跳/散射）
        const C2: u8 = 1 << 1; // 回旋镖
        const C3: u8 = 1 << 2; // 场/块（岩浆等）
        match self {
            ProjectileKind::W098b { proj, .. } => match proj {
                crate::skill::W098bProjKind::Boomerang => Some((2, C1 | C3)),
                crate::skill::W098bProjKind::Magma => None, // 岩浆走 magma_absorb
                _ => Some((1, C1 | C2 | C3)),
            },
            ProjectileKind::Bullet { .. }
            | ProjectileKind::Missile { .. }
            | ProjectileKind::PushBullet { .. }
            | ProjectileKind::BonusBomb { .. } => Some((1, C1 | C2 | C3)),
            _ => None,
        }
    }
}

/// 静态圆形障碍（原版 demo 里实际用作"墙/柱子"的碰撞体）。
/// 用圆盘描述，几何与玩家一致，但不参与名次/击杀/死亡判定。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obstacle {
    pub pos: Vec2,
    pub radius: Fix64,
    /// 柱子 HP（098c nx=40；被弹体伤害摧毁，每轮重生成时恢复）。
    pub hp: u32,
}

impl Obstacle {
    pub fn new(pos: Vec2, radius: f64) -> Self {
        Obstacle {
            pos,
            radius: Fix64::from_num(radius),
            hp: 40, // 098c nx=40
        }
    }
}

/// 纯表现用战斗事件（**不进快照、不进 `state_hash`**）。
///
/// 由**确定性模拟**产生（各端同样推演，各自得到相同事件），但表现层**不依赖**这个一致性：
/// 客户端只把它当作「本 tick 发生过什么」的日志，用于播报/飘字，**绝不回写模拟状态**。
/// 对应 098c 的自定义音效/漂字事件（见 `AUDIO_PLAN.md` §1）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CombatEvent {
    /// 一次 AoE 命中 ≥ 3 个敌人（098c `n>=3`）：`vampire` = 施法者戴死亡面具（`scourge_double`）。
    MultiHit { owner: u32, pos: Vec2, vampire: bool },
    /// 岩浆滚石把敌人拍扁（098c `Pancake`）。
    Pancake { victim: u32, pos: Vec2 },
    /// 一次沉默 ≥ 3 个目标（098c `Silencer`）。
    Silencer { owner: u32, pos: Vec2 },
    /// 天罚命中「被链接 + 特殊状态」的目标 → 断链（098c `Denied`/`aR`）。
    Denied { owner: u32, pos: Vec2 },
    /// 燃烧冲刺（S012 A，`Hr`）撞到队友 → 熄灭（098c `lb`「Burn out」）。
    Burnout { owner: u32, pos: Vec2 },
    /// 柱子被摧毁（HP 归零移除）：纯表现，客户端播放碎裂粒子。
    PillarBreak { pos: Vec2 },
    /// AoE 爆炸（新星/陨石/弹体爆炸）：纯表现，客户端播放扩散圆环。
    Explode { pos: Vec2, radius: Fix64 },
    /// 接触 AoE（098c `bA`/`SI`：冲锋/燃烧撞人）：纯表现，客户端画一圈。
    Splash { pos: Vec2, radius: Fix64 },
    /// 治疗脉冲（098c 虔诚：队友回血范围）：纯表现，客户端画绿圈。
    HealPulse { pos: Vec2, radius: Fix64 },
}

/// 确定性对局核心。
#[derive(Clone, Debug)]
pub struct World {
    pub players: Vec<Player>,
    pub arena_radius: Fix64,
    /// 基础生命恢复（HP/s）。098c 里它是主机可调常量
    /// （`-C9` → `In`，默认 `In=.05` / 0.1s tick = **0.5/s`）；这里做成可覆写字段，
    /// 便于测试隔离与以后接入房间设置（默认 = [`crate::balance::Balance::default`]）。
    pub base_regen: f64,
    /// 收缩**开局延迟**（秒，基准值；实际 = 本值 × √存活人数）。098c 设置 6 `wo`。
    pub shrink_delay_secs: Fix64,
    /// 收缩**总时长**（秒；满员从开始收缩到 0）。**连续收缩，非按环**；
    /// 实际总时长 = 本值 × √(存活/初始)，速率由「参考半径 / 实际总时长」反推。
    pub shrink_total_secs: Fix64,
    /// 本轮参考半径（开局/重设时的场地半径）：用于按“总时长”反推收缩速率。
    pub shrink_ref_radius: Fix64,
    /// 全局伤害倍率（098c 设置 2 `Gn`，默认 1.0）：所有技能/接触伤害乘它。
    pub damage_mult: Fix64,
    /// 全局击退倍率（098c 设置 3 `Hn`，默认 1.0）：KI/接触踢击的击退冲量乘它。
    pub knockback_mult: Fix64,
    /// 出界（岩浆）伤害倍率（098c 设置 1 `To`，默认 1.0 = 标准 9/s；`0` = 关闭岩浆）。
    pub lava_damage_mult: Fix64,
    /// 柱子模式（**我们自己的设置**）：0=关闭 1=随机（0~5 根，可无）2=每局必有（1~5 根）。
    pub pillar_mode: u8,
    /// 冰面模式（**我们自己的设置**）：0=关闭 1=随机（50% 有）2=每局必有。
    pub ice_mode: u8,
    /// 试验场模式（单机技能试验场）：不缩圈、不出圈掉血、不判对局结束。
    pub sandbox: bool,
    /// 柱子/障碍布局使用的确定性种子。每轮递增，保证各小局地形不同、且两端一致。
    pub round_seed: u64,
    /// 场景里的静态圆形障碍（柱子/墙）
    pub obstacles: Vec<Obstacle>,
    /// 场上飞行物 / 延时区域
    pub projectiles: Vec<Projectile>,
    /// 当前轮数（098c 岩浆成长用；调用方每轮开始时设置，快照同步）。
    pub round_number: u32,
    /// 本局伤害矩阵：`damage_matrix[攻][受]` = 累计伤害（助攻/最高伤害统计用，D6）；
    /// 随 `reset_round` 清空、随快照同步。
    pub damage_matrix: Vec<Vec<Fix64>>,
    /// 化身模式的**累计伤害积分**（098c `JV`：`fI` 里 `JV[i] += Rn[i]`，跨轮累加；
    /// 化身**被杀死**时其 `JV[化身]=0`，使其「当过就重新排队」）。每次加冕取最大者。
    pub avatar_score: Vec<Fix64>,
    /// 按死亡先后记录的玩家 id（用于本局名次结算）
    pub(crate) eliminated_order: Vec<u32>,
    /// 本局内发生的击杀：(击杀者 id, 被击杀者 id)
    pub(crate) kills_this_round: Vec<(u32, u32)>,
    pub(crate) time: Fix64,
    /// 瞬态渲染痕迹（仅客户端读取，不参与确定性逻辑/序列化）：闪电射线 (起点, 终点, 剩余显示秒)。
    /// 每帧 `step` 开头递减剩余时间、归零清空；由 `execute_effects` 的 Lightning 效果设置（Unity 原版约 0.1s），供 client 画线。
    /// 闪电视觉段列表（098c 闪电可经柱子反射产生多段；每段独立倒计时）。
    pub lightning_visual: Vec<(Vec2, Vec2, Fix64)>,
    /// 游戏模式（098c nn，B3）：1=round 默认 / 2=deathmatch / 3=avatar / 4=king / 5=lms。
    pub mode: u8,
    /// 化身玩家 id（模式 3；每轮由调用方设置）。
    pub avatar: Option<u32>,
    /// 国王玩家 id 列表（模式 4；弑王 Doom 用）。
    pub kings: Vec<u32>,
    /// F 槽施法替换（模式 3/4：化身→灾变 S020、国王→虔诚 S021；按玩家索引）。
    pub f_override: Vec<Option<SkillId>>,
    /// 强制回合结束（模式 3 化身被杀即结算）。
    pub round_forced: bool,
    /// 下一轮角色（reset_round 开头从上轮矩阵/rng 掷出，结尾应用）。
    pub(crate) pending_avatar: Option<u32>,
    pub(crate) pending_kings: Vec<u32>,
    /// 缩圈倒计时（098c EA：每 wo×√存活 秒烧掉一环，B5）。
    /// 用 `Fix64`（非 f64）保证跨端确定性；**随快照同步**——曾因漏序列化导致双机缩圈分叉。
    pub(crate) shrink_timer: Fix64,
    /// 冰面区域（U4 圆圈化：中心+半径的圆列表，可重叠拼形；空=本轮无冰面）。
    /// 冰面不被岩浆侵蚀。
    pub ice: Vec<(Vec2, Fix64)>,
    /// 纯表现用战斗事件（见 [`CombatEvent`]）：每 `step` 开头清空、结算点写入；
    /// **不序列化、不进 `state_hash`**。仅在 push 后到下次 `step` 前有效。
    pub combat_events: Vec<CombatEvent>,
}

/// `explode_at` 的伤害距离衰减方式（098c 实证）。
#[derive(Copy, Clone, Debug)]
enum DmgFalloff {
    None,
    /// 乘法：`dmg × (1 - d/k)`（陨石 `oB` 的 `Zb`）。
    Mul(Fix64),
    /// 加法：`dmg - d/k`（灾变 `qC`）。
    Sub(Fix64),
}

impl World {
    /// 创建一场对局。`player_count` 为玩家人数；`seed` 用于 AI / 初始布局等确定性随机。
    pub fn new(player_count: u32, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let arena_radius = Fix64::from_num(Balance::start_radius_for(player_count));
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
        _layout_obstacles(&mut obstacles, &mut rng, arena_radius, 1);
        World {
            players,
            arena_radius,
            base_regen: crate::balance::Balance::default().hp_regen,
            shrink_delay_secs: Fix64::from_num(crate::balance::Balance::default().shrink_delay_secs),
            shrink_total_secs: Fix64::from_num(DEFAULT_SHRINK_TOTAL_SECS),
            shrink_ref_radius: arena_radius,
            damage_mult: Fix64::ONE,
            knockback_mult: Fix64::ONE,
            lava_damage_mult: Fix64::ONE,
            pillar_mode: 1,
            ice_mode: 1,
            sandbox: false,
            round_seed: seed,
            obstacles,
            projectiles: Vec::new(),
            eliminated_order: Vec::new(),
            kills_this_round: Vec::new(),
            round_number: 1,
            damage_matrix: vec![vec![Fix64::ZERO; player_count as usize]; player_count as usize],
            avatar_score: vec![Fix64::ZERO; player_count as usize],
            time: Fix64::ZERO,
            lightning_visual: Vec::new(),
            mode: 1,
            avatar: None,
            kings: Vec::new(),
            f_override: vec![None; player_count as usize],
            round_forced: false,
            pending_avatar: None,
            pending_kings: Vec::new(),
            shrink_timer: Fix64::from_num(
                Balance::default().shrink_delay_secs * (player_count.max(1) as f64).sqrt(),
            ),
            ice: Vec::new(),
            combat_events: Vec::new(),
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
        // 纯表现事件每 tick 重建（见 `CombatEvent`）；不参与确定性/快照。
        self.combat_events.clear();
        let mut expire = false;
        for (_, _, rem) in self.lightning_visual.iter_mut() {
            *rem -= dt;
            if *rem <= Fix64::ZERO {
                expire = true;
            }
        }
        if expire {
            self.lightning_visual.retain(|(_, _, rem)| *rem > Fix64::ZERO);
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
        let mut phoenix_shots: Vec<(u32, Vec2, Vec2)> = Vec::new(); // (owner, 位置, 方向) 凤凰弹（B4-R）
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
            // B4-R 凤凰态（098c `WB`/`UB`）：
            //   - 普通移动指令 → `bO(gX, 18×0.5^(v/20))` **转向冲刺**（v 为每 tick 速度，单位/帧）；
            //   - 疾风步状态中（`Fr[gX]`）→ 额外发射凤凰弹（`tB`，速度 1000、半径 44）。
            if pi.set_target.is_some() && p.phoenix_remaining > Fix64::ZERO && p.control.is_some() {
                let t = pi.set_target.unwrap();
                let d = t - p.pos;
                if d.length() > Fix64::ZERO {
                    let nd = d.normalized();
                    if p.windwalk_state > Fix64::ZERO && !p.charging {
                        phoenix_shots.push((i as u32, p.pos, nd));
                    } else {
                        // 098c `bO`：每 tick 冲量，18×0.5^(v/20)（v 单位/帧）。v=0 → 18/tick≈600/s；v=20/tick→9/tick。
                        let v_tick = p.control.as_ref().unwrap().vel.length() * Fix64::from_num(0.03);
                        let imp = Fix64::from_num(18.0)
                            * Fix64::from_num(0.5_f64.powf((v_tick / Fix64::from_num(20.0)).to_num::<f64>()));
                        p.control.as_mut().unwrap().vel = nd * (imp / Fix64::from_num(0.03));
                    }
                }
                continue;
            }
            if pi.set_target.is_some() && p.dash_active {
                p.dash_active = false;
                p.dash_vel = Vec2::ZERO;
                p.control = None; // 若有残留强制态也一并清除
                p.remove_buff(BuffKind::Stealth);
            }
            // 仅在有新移动目标时更新 move_target；None 表示“本帧没有新目标”，不覆盖（让 shift 队列的移动得以保留）。
            // **施法中不接受新的移动目标**（098c：施法锁定走位）。
            // 客户端的移动目标是**持续电平量**（每帧重发，防帧同步输入缓存丢指令），
            // 且它只在「观察到 busy」那一帧才清自己的 `player_target` —— 网络下有 RTT，
            // 所以 host 侧必须自己丢掉施法期间到达的旧目标，否则角色会一边施法一边继续走。
            if let Some(t) = pi.set_target {
                if !p.caster.is_busy() {
                    p.move_target = Some(t);
                }
            }
        }
        for (pid, target) in fake_locs {
            self.fake_locate(pid, target);
        }

        // 3) 移动：本帧流程 = 清 pull → 场效应累加 pull → 合成速度推进 + buff 计时
        let mut new_deaths = Vec::new();
        let mut new_kills = Vec::new();
        for p in self.players.iter_mut() {
            p.reset_pull();
        }
        // 3b) 场效应贡献本帧附加速度（引力场 / 回拉线等；暂为空，各技能接入）
        self.step_area_forces(dt);
        // 3c) T2 扇扫连射：按心率依次发射
        self.step_sweep(dt);
        let ice_flags: Vec<bool> = self.players.iter().map(|p| self.on_ice(p.pos)).collect();
        for (p, flag) in self.players.iter_mut().zip(ice_flags) {
            p.on_ice = flag;
            p.step_velocity(dt);
            p.tick_buffs(dt);
            // 熔岩靴激活 CD 递减（098b 25s，D8/M5）。
            if p.lava_boot_cd > Fix64::ZERO {
                p.lava_boot_cd = (p.lava_boot_cd - dt).max(Fix64::ZERO);
            }
            // 凤凰态倒计时（B4-R）：到期解除冲刺。
            // 098c `Fr[unit]`：疾风步状态（A 冲锋 / B 隐身两形态都置位）。
            if p.windwalk_state > Fix64::ZERO {
                p.windwalk_state = (p.windwalk_state - dt).max(Fix64::ZERO);
                // 风步/冲锋结束 → 清招架就绪（098c `AA`）与冲锋标志（098c `DR`）。
                if p.windwalk_state == Fix64::ZERO {
                    p.parry_ready = false;
                    p.parry_cd = Fix64::ZERO;
                    p.charging = false;
                }
            }
            if p.phoenix_remaining > Fix64::ZERO {
                p.phoenix_remaining = (p.phoenix_remaining - dt).max(Fix64::ZERO);
                if p.phoenix_remaining == Fix64::ZERO && p.dash_active {
                    p.dash_active = false;
                    p.dash_vel = Vec2::ZERO;
                    p.control = None;
                }
            }
            // 基础生命恢复（098c `In[]`：地图自维护、0.1s 定时器回血；基础 **0.5/s**）
            // + 物品回复（斗篷/坠饰等）。化身另有角色倍率 `In *= (1+n/2)`（`role_regen_mult`）。
            // 注：098c 的精通只经 `Hn` 影响击退（kf L12917），无精通→回血链路。
            if p.alive && !p.healing_blocked() {
                let regen = ((self.base_regen
                    + p.item_fx.regen_add
                    - p.item_fx.regen_penalty)
                    .max(0.0))
                    * p.role_regen_mult;
                p.hp = (p.hp + Fix64::from_num(regen) * dt).min(p.max_hp);
            }
            // Doom（098c 国王模式：弑王者所在队伍 −10 HP/s，持续 50s）；可致死亡。
            if p.doom > 0.0 {
                if p.alive {
                    p.hp = (p.hp - Fix64::from_num(p.doom) * dt).max(Fix64::ZERO);
                    if p.hp == Fix64::ZERO {
                        p.alive = false;
                        new_deaths.push(p.id);
                    }
                }
                // 098c `LO(function II, 50, ...)` → 50 s 后 `In += 1`（解除）。
                p.doom_remaining = (p.doom_remaining - dt).max(Fix64::ZERO);
                if p.doom_remaining == Fix64::ZERO {
                    p.doom = 0.0;
                }
            }
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
        let collision_events =
            resolve_player_collisions(&mut self.players, dt, self.damage_mult, self.knockback_mult);
        self.combat_events.extend(collision_events);
        // 招架（098c `CA`）：在处理完本 tick 伤害后统一判定。
        self.process_parry(dt);
        // 燃烧冲刺熄灭/结束（自然结束或被打断）时清 `burning`。
        for p in self.players.iter_mut() {
            if p.burning && p.control.is_none() && p.kick.is_none() {
                p.burning = false;
            }
        }
        // 5b) 玩家与障碍（圆形柱子）的分离
        self.resolve_obstacles(dt);

        // 6) 飞行物 / 延时区域
        self.step_projectiles(dt);

        // 7) 边界：出界掉血（无自动回收，玩家需自己走位回去）+ 死亡
        let ice_flags: Vec<bool> = self.players.iter().map(|p| self.on_ice(p.pos)).collect();
        let mut lava_credit: Vec<(u32, u32, Fix64)> = Vec::new();
        let np = self.players.len();
        for (p, on_ice_now) in self.players.iter_mut().zip(ice_flags) {
            if !p.alive {
                continue;
            }
            if !self.sandbox && p.pos.length() > self.arena_radius && !on_ice_now {
                // 球心已出圈：持续掉血。回去靠自己走位。（boost 期间返一半回血）
                // 圈外 = 熔岩（098b 语义统一，D8）：踩上即受 Uo×10=9/s；
                // 熔岩靴抵抗 87.5%（M5 简化为常驻被动；098b 原版为「熔岩上天罚激活 3-5s
                // 窗口 + CD25s」，激活式 TODO）、-0.1 hp/s 惩罚照常生效。
                // 熔岩靴激活窗口内 ×12.5%；窗口外吃全额（激活条件见 Nova 臂，D8/M5）。
                let shielded = p.has_buff(BuffKind::LavaShield);
                let lava_mult = if shielded { 1.0 - p.item_fx.lava_resist_frac } else { 1.0 };
                let lava_mult = lava_mult * p.lava_taken_mult;
                // 岩浆伤害（098c 解码实证，war3map_pretty.j `nA` 每 0.1s 扣 `To[id]`）：
                // 每跳恒定 `To[id]≈0.9`，显示 `To[0]*$A`（$A=10 跳/秒）→ **约 9/s 恒定**。
                // 全 JASS 无「随回合数成长」的缩放（仅国王模式对君主 ±10% 抗岩浆、物品减速等），
                // MECHANICS.md「To[0] 随回合数成长」是文档误差；「拖延越久越痛」实际来自**缩圈导致暴露更多**，
                // 已由 shrink_arena 实现。故此处不用 round_scale，固定 1.0。
                let round_scale = 1.0;
                let net = p.soak_boost(
                    Fix64::from_num(OUT_HURT * lava_mult * round_scale)
                        * self.lava_damage_mult
                        * dt,
                );
                p.hp = (p.hp - net).max(Fix64::ZERO);
                // 098c `nA`：岩浆伤把**一半**记到「最后伤害我的人」名下（`Jn[受][An] += To/2`），
                // 用于助攻统计。注意它**不进** `Rn`（本轮伤害），故不影响化身加冕积分 —— 见上面的
                // 伤害结算处累加。这里先收集，循环外统一记账（避免与 players 的借用冲突）。
                if let Some(k) = p.last_hit_by {
                    if k != p.id && (k as usize) < np {
                        lava_credit.push((k, p.id, net / Fix64::from_num(2.0)));
                    }
                }
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
        // 岩浆伤害归属（098c `Jn[受][An] += To/2`）。
        for (attacker, victim, half) in lava_credit.drain(..) {
            if let Some(row) = self.damage_matrix.get_mut(attacker as usize) {
                if let Some(cell) = row.get_mut(victim as usize) {
                    *cell += half;
                }
            }
        }
        // 凤凰弹生成（B4-R）：转向处发射（直射弹，伤害=stats.damage）
        for (owner, pos, dir) in phoenix_shots.drain(..) {
            // 098c `tB` 实证：凤凰弹速度 **1000**、半径 **44**、寿命 `1.4×(1+.1ei)`；
            // 伤害因子 `Xv = Wr+wr`（R 槽等级，handler `ea/xa` 未解码）→ 用 S012 当前等级伤害代替。
            // 098c `sB` 实证：弹体伤害 = `4 + 0.5×Xv`，`Xv = Wr[id]+wr[id]`
            // = 槽 3（S012 本身，即 R 槽）+ 槽 2（S008/S009/S010，即 E 槽）的**等级之和**。
            // 这也是 098c 把凤凰与**疾风步**设计成连携（`UB` 的 `Fr[gX]` 分支）的根据。
            let (ei, lv_r, lv_e) = self.players.get(owner as usize).map(|p| {
                let lv_e = [crate::skill::SkillId::S008, crate::skill::SkillId::S009, crate::skill::SkillId::S010]
                    .iter()
                    .map(|s| p.skill_level(*s))
                    .max()
                    .unwrap_or(0);
                (p.mastery[2] as f64, p.skill_level(crate::skill::SkillId::S012), lv_e)
            }).unwrap_or((0.0, 1, 0));
            let dmg = Fix64::from_num(4.0 + 0.5 * (lv_e + lv_r) as f64);
            let speed = Fix64::from_num(1000.0);
            let life = Fix64::from_num(1.4 * (1.0 + 0.1 * ei));
            self.projectiles.push(Projectile {
                owner,
                kind: ProjectileKind::W098b {
                    proj: crate::skill::W098bProjKind::Straight,
                    vel: dir * speed,
                    speed,
                    radius: Fix64::from_num(44.0),
                    remaining: life,
                    life,
                    gx: dmg,
                    kb_ji: Fix64::from_num(0.8),
                    ignite: None,
                    blast: None,
                    target: None,
                    returning: false,
                    on_hit: crate::skill::W098bOnHit::Ki,
                    debuff_dur: Fix64::ZERO,
                    lateral: Fix64::ZERO,
                    forward_dir: dir,
                    out_dist: life * speed,
                    burst: 0,
                    emit_cooldown: Fix64::ZERO,
                    emit_angle: 0.0,
                    pillar_bounce: false,
                    pillar_rest: Fix64::ZERO,
                    lightning_dmg: Fix64::ZERO,
                },
                pos,
                alive: true,
            });
        }
        // 岩浆/Doom 死亡也走 record_death（模式钩子：DM 复活 / LMS 救活受害者 / 化身结算）。
        for victim in new_deaths {
            self.record_death(victim);
        }
        // 复活调度（模式 2/5，B3）：到点满血复活。
        self.tick_respawns();

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
                            // 098c：疾风步期间施放技能 → 提前结束风步（与直接施法同一条规则）。
                            self.players[i].end_windwalk();
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
        // F 槽施法替换快照（B3：化身→灾变 S020 / 国王→虔诚 S021），避免借用冲突。
        let f_overrides = self.f_override.clone();

        // a) 先应用新的施法请求（占用本帧的人手）
        for (idx, (p, pi)) in self.players.iter_mut().zip(input.iter()).enumerate() {
            if !p.alive {
                continue;
            }
            // 禁锢（Tied：定身+禁施法）与沉默（B4-Y：禁施法可移动）
            if p.tied() || p.silenced() {
                continue;
            }
            if let Some((skill, target)) = pi.cast {
                // F 槽替换（模式 3/4）：天罚 S001 在化身/国王手里变成灾变/虔诚。
                let skill = if skill == SkillId::S001 {
                    f_overrides.get(idx).copied().flatten().unwrap_or(skill)
                } else {
                    skill
                };
                // C3 影身·召回：已有锚点时再按影身 = 立即传回（原版 `BackToShadow` 忽略冷却）
                if skill == SkillId::Shadow && p.shadow_anchor.is_some() {
                    let anchor = p.shadow_anchor.take().unwrap();
                    p.pos = anchor;
                    p.shadow_window = Fix64::ZERO;
                    p.move_target = None;
                    p.end_windwalk(); // 施放技能 → 提前结束疾风步（098c）
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
                    p.end_windwalk(); // 施放技能 → 提前结束疾风步（098c）
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
                    // 098c：疾风步期间施放其它技能 → 提前结束风步（清隐身/计时/招架/移速）。
                    // 若施放的正是疾风步/冲锋本身，其效果结算时会重新挂上（等于刷新）。
                    p.end_windwalk();
                    just_cast[idx] = true;
                }
            }
        }

        // b) 推进施法状态机；收集本帧“前摇结束”的效果并执行
        let mut fire_queue: Vec<(u32, SkillId, Option<Vec2>)> = Vec::new();
        for (idx, p) in self.players.iter_mut().enumerate() {
            if !p.alive {
                // 死亡即取消在途前摇，避免“尸体施法”（record_death 也会清，双保险）。
                p.caster.interrupt();
                continue;
            }
            if let Some((id, target)) = p.caster.advance(dt) {
                fire_queue.push((idx as u32, id, target));
                p.caster.begin_cooldown(id);
            }
        }

        // 执行本帧完成前摇的技能效果
        execute_effects(self, &fire_queue);

                just_cast
    }

    /// 场地收缩（U3 连续化）：保留 098c 的时间节奏——首个间隔 `wo×√存活` 秒后开始，
    /// 之后以 `环宽/(wo×√存活)` 每秒的**连续速率**收缩（人越少越快）。
    /// 总吞没时长与按环步进的 098c 完全一致，只是抹平了 war3 地形格的阶梯感（D13）。
    fn shrink_arena(&mut self, dt: Fix64) {
        let alive = self.players.iter().filter(|p| p.alive).count().max(1) as f64;
        if self.shrink_timer > Fix64::ZERO {
            self.shrink_timer -= dt;
            return;
        }
        // 连续收缩（我方模型）：满员总时长 `shrink_total_secs`；实际总时长 = 本值 × √(存活/初始)。
        // 速率由「本轮参考半径 / 实际总时长」给出（参考半径开局固定 → 线性缩到 0）。
        let alive0 = self.players.len().max(1) as f64;
        let total = self.shrink_total_secs.to_num::<f64>().max(0.01);
        let scale = (alive0 / alive.max(1.0)).sqrt();
        let rate = self.shrink_ref_radius.to_num::<f64>() / total * scale;
        self.arena_radius = (self.arena_radius - Fix64::from_num(rate * dt.to_num::<f64>())).max(Fix64::ZERO);
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
                    // 回拉线（锁链）：`pull_speed` 的**符号**编码拉拽方向——
                    //   > 0：把绑定目标拉向施法者（**蓝链** ChainPull，原 Y1 回拉线语义）
                    //   < 0：把施法者拉向绑定目标（**红链** RedChain，文档「把你拉向敌人」）
                    // 用符号而非新增字段，避免改动 Tether 结构与序列化。
                    let sp = pull_speed;
                    if sp >= Fix64::ZERO {
                        let from = self.players.get(owner as usize).map(|p| p.pos).unwrap_or(Vec2::ZERO);
                        if let Some(t) = self.players.get_mut(target as usize) {
                            if t.alive {
                                let d = from - t.pos;
                                let dsq = d.length_squared();
                                if dsq > Fix64::from_num(1.1) {
                                    t.pull += d.normalized() * sp;
                                }
                            }
                        }
                    } else {
                        // 红链：反向——把施法者拉向目标。
                        let to = self.players.get(target as usize).map(|p| p.pos);
                        if let Some(to) = to {
                            if let Some(o) = self.players.get_mut(owner as usize) {
                                if o.alive {
                                    let d = to - o.pos;
                                    let dsq = d.length_squared();
                                    if dsq > Fix64::from_num(1.1) {
                                        o.pull += d.normalized() * (-sp);
                                    }
                                }
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
                    // 098c 撞障碍是**逐轴**响应（war3map_pretty.j:8654-8687 对 X/Y 各自 `RA(...)` 检查）：
                    // 英雄 `nv==1` 且 `xv=0.5`（`FR` 创建时设定）→ 走 `set Q=-Q*xv` 分支，
                    // 即该轴速度**反向 ×0.5（半速反弹）**，另一轴保留 → 斜撞沿墙弹开。
                    // （旧实现“接触即 `control = None`”/“逐轴清零”会整体停死，与 098c 不符。）
                    let rest = Fix64::from_num(PLAYER_OBS_RESTITUTION);
                    if let Some(c) = p.control.as_mut() {
                        if c.vel.x * dir.x < Fix64::ZERO {
                            c.vel.x = -c.vel.x * rest;
                        }
                        if c.vel.y * dir.y < Fix64::ZERO {
                            c.vel.y = -c.vel.y * rest;
                        }
                    }
                    if p.dash_active {
                        if p.dash_vel.x * dir.x < Fix64::ZERO {
                            p.dash_vel.x = -p.dash_vel.x * rest;
                        }
                        if p.dash_vel.y * dir.y < Fix64::ZERO {
                            p.dash_vel.y = -p.dash_vel.y * rest;
                        }
                    }
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
        // 死亡即打断施法：清除在途前摇，避免尸体继续走完施法（见 handle_casts 的 alive 门）。
        self.players[victim as usize].caster.interrupt();
        if let Some(k) = self.players[victim as usize].last_hit_by {
            self.kills_this_round.push((k, victim));
        }
        // 模式化身后处理（098c AI 分支，B3）：
        match self.mode {
            2 => {
                // 死亡竞赛：4 秒后复活（098c so=4s → OI 随机点满血）。
                self.players[victim as usize].respawn_at = Some(self.time + Fix64::from_num(4.0));
            }
            5 => {
                // LMS：凶手死亡 → 其受害者 3 秒后复活（098c so/3）。
                let killer_id = self.players[victim as usize].id;
                for p in self.players.iter_mut() {
                    if !p.alive && p.respawn_at.is_none() && p.last_hit_by == Some(killer_id) {
                        p.respawn_at = Some(self.time + Fix64::from_num(3.0));
                    }
                }
            }
            3 => {
                // 化身被杀 → 立即结算本轮（098c fI）；
                // 同时把化身的**累计伤害积分清零**（`set JV[FV]=0`）→ 下一轮改从其他人里选。
                if self.avatar == Some(victim) {
                    self.round_forced = true;
                    if let Some(score) = self.avatar_score.get_mut(victim as usize) {
                        *score = Fix64::ZERO;
                    }
                }
            }
            4 => {
                // 弑王 Doom（098c `AI` nn==4）：**王所在队伍**的存活成员 `In -= 1` → −10 HP/s，
                // 持续 50 s（`LO(function II, 50, ...)` 后 `In += 1` 解除）。
                // （JASS：`if bn[i] and Nn[i] and cn[NI]==cn[i] then In[i]=In[i]-1.`，NI=死去的王。）
                if self.kings.contains(&victim) {
                    let vteam = self.players[victim as usize].team;
                    for p in self.players.iter_mut() {
                        if p.team == vteam && p.alive && p.id != victim {
                            p.doom = 10.0;
                            p.doom_remaining = Fix64::from_num(50.0);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// 复活到随机出生点（模式 2/5；确定性 rng）。满血、清状态。
    fn revive_player(&mut self, idx: usize) {
        // 确定性复活点：以 round_seed+玩家 id 播种（两端一致）
        let mut rng = Rng::new(self.round_seed ^ (0x9E37_79B9_7F4A_7C15u64.wrapping_mul(idx as u64 + 1)));
        let angle = Fix64::from_num(std::f64::consts::TAU) * rng.next_fix();
        let ring = self.arena_radius * Fix64::from_num(0.5);
        let pos = Vec2::new(ring * crate::fix::cos(angle), ring * crate::fix::sin(angle));
        let p = &mut self.players[idx];
        p.alive = true;
        p.hp = p.max_hp;
        p.pos = pos;
        p.move_target = None;
        p.control = None;
        p.kick = None;
        p.rewind = None;
        p.blink2_window = None;
        p.respawn_at = None;
        p.last_hit_by = None;
        for b in p.buffs.iter_mut() {
            *b = crate::player::Buff::new(BuffKind::Speed(1.0), 0.0);
        }
    }

    /// 推进复活调度（模式 2/5）：到点复活。
    fn tick_respawns(&mut self) {
        let now = self.time;
        let due: Vec<usize> = self
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.alive && matches!(p.respawn_at, Some(t) if t <= now))
            .map(|(i, _)| i)
            .collect();
        for i in due {
            self.revive_player(i);
        }
    }

    /// 离散命中伤害（弹体/爆炸/近战等）：计入 098c `Gn` 伤害成长（命中敌人 ×1.1）。
    fn damage_player(&mut self, id: u32, amount: Fix64, from: Option<u32>) {
        self.damage_player_impl(id, amount, from, true);
    }

    /// 持续伤害（每帧 DoT：引力场/力场/锁链/滚动火球/线束）：同 `damage_player`，
    /// 但**不**涨 `Gn`。原因：098c 的 `Gn×1.1` 由 `bv[弹体]` 门控（每发弹体只触发一次），
    /// 而 DoT 是每帧结算；若每帧都乘 1.1（1.1^60/s）会指数爆炸，几帧内就秒杀满血。
    fn damage_player_dot(&mut self, id: u32, amount: Fix64, from: Option<u32>) {
        self.damage_player_impl(id, amount, from, false);
    }

    /// 伤害结算内核。`grow_gn` = 是否计入 `Gn` 伤害成长（DoT 传 `false`）。
    fn damage_player_impl(&mut self, id: u32, amount: Fix64, from: Option<u32>, grow_gn: bool) {
        // 098c Gn[施法者]（D9 批次1）：伤害成长 × 灼烧惩罚。先取系数再进可变借用。
        let gn = from
            .and_then(|f| self.players.get(f as usize))
            .map(|a| a.gn_factor())
            .unwrap_or(1.0);
        let world_dmg_mult = self.damage_mult;
        // 折算后伤害提出外层：矩阵记账（D6）/回魔（D9）统一用最终值。
        let dealt;
        let died = {
            let p = &mut self.players[id as usize];
            if !p.alive {
                return;
            }
            // 4.6b：按目标护甲×法抗折算 × 098c 攻方 Gn。
            // 守护之盾充能窗口（098c HC）：天罚后 5s 内受伤减免（25%/75%）。
            dealt = if from.is_some() {
                let base = amount * Fix64::from_num(gn * p.dmg_taken_mult) * world_dmg_mult;
                if p.has_buff(BuffKind::Aegis) && p.item_fx.smite_reduction > 0.0 {
                    base * Fix64::from_num(1.0 - p.item_fx.smite_reduction)
                } else {
                    base
                }
            } else {
                amount
            };
            if let Some(hitter) = from {
                p.last_hit_by = Some(hitter);
            }
            // C1 疾跑：boost 期间返还一半伤害回血（soak_boost 返回净扣血）
            let net = p.soak_boost(dealt);
            p.hp = (p.hp - net).max(Fix64::ZERO);
            if p.hp == Fix64::ZERO {
                p.alive = false;
                true
            } else {
                false
            }
        };
        // 098c 挨打回魔（D9 批次1）：目标魔法 += 受到的伤害（击退张力核心）。
        if let Some(p) = self.players.get_mut(id as usize) {
            if p.alive {
                p.mana += dealt.to_num::<f64>();
            }
        }
        // 098c 伤害成长（D9 批次1）：**离散命中**敌人时 Gn ×= 1.1（`grow_gn=false` 的 DoT 不算）。
        if grow_gn {
            if let Some(f) = from {
                if let Some(a) = self.players.get_mut(f as usize) {
                    if a.alive {
                        a.on_dealt_damage();
                    }
                }
            }
        }
        // 伤害矩阵记账（D6）：助攻/最高伤害统计的数据源（折算后值）。
        if let Some(f) = from {
            if f < self.players.len() as u32 && id < self.players.len() as u32 {
                self.damage_matrix[f as usize][id as usize] += dealt;
            }
        }
        // 化身模式**累计伤害积分**（098c `fI`：`JV[i] += Rn[i]`）。在伤害结算处累加，
        // 与 `Rn`（只计实际造成的伤害、不含岩浆等环境伤害）等价；供下一轮加冕取最大。
        if self.mode == 3 {
            if let Some(f) = from {
                if let Some(score) = self.avatar_score.get_mut(f as usize) {
                    *score += dealt;
                }
            }
        }
        // 死亡面具/鲜血之剑（M3 2c）+ 生命精通 vi（098c kf，B1）：攻方生命偷取与受伤点恢复。
        // vi 每级 +8%（098c L3299 HX×0.08×vi；死亡面具白送的 +3 吸血走 item lifesteal，不加精通级数）。
        // 2026-09-12 修正：098c 的 HX 是**实际造成的伤害**（护甲/法抗/Gn 折算后）→ 用 `dealt` 而非原始 `amount`，
        // 否则对高护甲目标会高估吸血/回血。
        if let Some(f) = from.and_then(|f| self.players.get(f as usize).map(|a| a.id)) {
            let (lifesteal, odh, vi) = {
                let a = &self.players[f as usize];
                (a.item_fx.lifesteal, a.item_fx.on_damage_heal, a.mastery[0])
            };
            let dealt_f = dealt.to_num::<f64>();
            let vi_steal = dealt_f * 0.08 * vi as f64;
            if lifesteal > 0.0 || odh > 0.0 || vi_steal > 0.0 {
                if let Some(a) = self.players.get_mut(f as usize) {
                    if a.alive {
                        let heal = dealt_f * lifesteal + odh + vi_steal;
                        a.hp = (a.hp + Fix64::from_num(heal)).min(a.max_hp);
                    }
                }
            }
        }
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
        // 陨石落点（098c `oB`）：(owner, 落点, 半径, 中心伤害, kb_ji, 衰减分母)
        let mut delayed_blasts: Vec<(u32, Vec2, Fix64, Fix64, Fix64, Fix64)> = Vec::new();
        // 碎裂/侧弹生成队列（B4：S009 目标形态到点碎裂、区域形态螺旋侧弹）
        // (owner, 位置, 速度, gx, 弹半径, 寿命, kb_ji)
        let mut spawn_bullets: Vec<(u32, Vec2, Vec2, Fix64, Fix64, Fix64, Fix64)> = Vec::new();
        // 098c 回旋镖回程到位结算：(owner, 位置, gx, kb_ji) —— 对命中半径 qI 内目标 AOE（距离衰减）。
        let mut boomerang_settles: Vec<(u32, Vec2, Fix64, Fix64)> = Vec::new();
        let eps = Fix64::from_num(1.0 / 65536.0);

        // 1) 推进整帧：倒计时 / 生命周期 / 弹体飞行
        for pr in ps.iter_mut() {
            match &mut pr.kind {
                ProjectileKind::DelayedBlast { radius, damage, kb_ji, falloff_denom, remaining } => {
                    // 陨石落点（098c `oB`）：倒计时结束 → 以落点为中心 AOE（伤害随距离衰减）。
                    *remaining -= dt;
                    if *remaining <= Fix64::ZERO {
                        pr.alive = false;
                        delayed_blasts.push((pr.owner, pr.pos, *radius, *damage, *kb_ji, *falloff_denom));
                    }
                }
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
                ProjectileKind::Clone { owner, offset, fire_timer, remaining, .. } => {
                    // 镜像分身：每帧贴到施法者 + 固定偏移（模仿移动）；倒计时归零消失。
                    // 开火倒计时在此递减（本段是 &mut 借用）；到点后的发射在 2) 段碰撞循环里做。
                    if let Some(o) = self.players.get(*owner as usize) {
                        if o.alive {
                            pr.pos = o.pos + *offset;
                        }
                    }
                    *remaining -= dt;
                    *fire_timer -= dt;
                    if *remaining <= Fix64::ZERO {
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
                ProjectileKind::W098b { proj, vel, speed, remaining, blast, target, returning, gx, kb_ji, forward_dir, out_dist, burst, emit_cooldown, emit_angle, lateral, on_hit, .. } => {
                    // 098b 弹体运动学：Straight/Bounce 直线（Bounce 的重定向在命中分支做）；
                    // Homing 全速直追锁定目标；Boomerang 出程恒速、过半程后朝施法者当前位置回拉。
                    // 到期时带 blast 的弹体（陨石）在原地爆炸。
                    *remaining -= dt;
                    if *remaining <= Fix64::ZERO {
                        pr.alive = false;
                        // S013B 搬运（`pB`）：弹体到期也把施法者传送过去。
                        if *on_hit == crate::skill::W098bOnHit::CarrySelf {
                            if let Some(o) = self.players.get_mut(pr.owner as usize) {
                                o.pos = pr.pos;
                            }
                        }
                        // 098c `oB`：回旋镖在飞行计时结束时就地做命中半径 qI 内 AOE 结算
                        //（**不是**靠「回到施法者附近」——玩家一移动就永远回不来了）。
                        if *proj == crate::skill::W098bProjKind::Boomerang {
                            boomerang_settles.push((pr.owner, pr.pos, *gx, *kb_ji));
                        }
                        if let Some(br) = blast {
                            expiry_blasts.push((pr.owner, pr.pos, *br, *gx, *kb_ji));
                        }
                        // S009·目标形态（B4）：到点碎裂成 6 枚环形弹片（098c dB：600/s 旋转喷出）
                        if *burst > 0 {
                            let n = *burst as i64;
                            let base = std::f64::consts::TAU / n as f64;
                            for k in 0..n {
                                // 确定性三角：f64 只用于「常数×k + 角度状态」的 IEEE 四则运算（逐位确定），
                                // 三角函数走 CORDIC（crate::fix），避免平台 libm 差异导致帧同步 desync。
                                let ang = Fix64::from_num(base * k as f64 + *emit_angle);
                                let d = Vec2::new(crate::fix::cos(ang), crate::fix::sin(ang));
                                spawn_bullets.push((pr.owner, pr.pos, d * Fix64::from_num(600.0), *gx, Fix64::from_num(15.0), Fix64::from_num(0.8), *kb_ji));
                            }
                        }
                    }
                    // S009·区域形态（B4）：飞行中每 0.12s 沿旋转角撒一枚侧弹（098c cB 螺旋）
                    if *emit_cooldown > Fix64::ZERO {
                        *emit_cooldown -= dt;
                        if *emit_cooldown <= Fix64::ZERO {
                            *emit_cooldown = Fix64::from_num(0.12);
                            *emit_angle += 0.52; // ≈30° 螺旋步进
                            // 确定性三角：走 CORDIC（见上）。
                            let ang = Fix64::from_num(*emit_angle);
                            let d = Vec2::new(crate::fix::cos(ang), crate::fix::sin(ang));
                            spawn_bullets.push((pr.owner, pr.pos, d * Fix64::from_num(600.0), *gx, Fix64::from_num(15.0), Fix64::from_num(0.7), *kb_ji));
                        }
                    }
                    match proj {
                        crate::skill::W098bProjKind::Straight | crate::skill::W098bProjKind::Bounce | crate::skill::W098bProjKind::Magma => {
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
                            // 098c 弧线物理（Ub/ub）：出程 = 前向匀减速 + 横向匀加速（横向从 ±300 到 ∓300），
                            // 前向归零转回程 = 横向加速度反向，沿对称弧线回飞施法者。
                            let dir = *forward_dir;
                            let perp = Vec2::new(-dir.y, dir.x);
                            // 前向减速度 yb = -speed²/(2·out_dist)；横向加速度 Yb = -speed·lateral/out_dist（098c Ub）。
                            let yb = -(*speed * *speed) / (Fix64::from_num(2.0) * *out_dist);
                            let yb_lat = -(*speed * *lateral) / *out_dist;
                            if !*returning {
                                *vel += (dir * yb + perp * yb_lat) * dt;
                                // 098c `ub`：回程镜像在飞行结束前 5 帧（0.15s）开始（`gv = ev - 5*.03`），
                                // 而不是等前向速度归零。
                                if *remaining <= Fix64::from_num(0.15) {
                                    *returning = true;
                                }
                            } else {
                                // 回程：横向加速度反向（098c ub：U=Y, w=z），前向继续减速朝施法者。
                                *vel += (dir * yb - perp * yb_lat) * dt;
                                // 注：结算由 `remaining` 到期触发（098c `oB`），不依赖与施法者的距离。
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
            for oi in 0..self.obstacles.len() {
                let o = self.obstacles[oi];
                let delta = pr.pos - o.pos;
                let dist = delta.length();
                let min = radius + o.radius;
                if dist > Fix64::ZERO && dist < min {
                    let normal = delta / dist;
                    if let ProjectileKind::Boomerang { vel, .. } = &mut pr.kind {
                        *vel = crate::fix::mirror_by(*vel, normal);
                        pr.pos = o.pos + normal * min; // 推出柱面，避免下帧仍重叠而反复反弹
                    } else if let ProjectileKind::W098b {
                        proj: crate::skill::W098bProjKind::Boomerang,
                        vel,
                        ..
                    } = &mut pr.kind
                    {
                        // 098b 回旋镖撞柱反弹（与 D2 原型同手感）；Straight/Homing 被柱子挡下消失。
                        *vel = crate::fix::mirror_by(*vel, normal);
                        pr.pos = o.pos + normal * min;
                    } else if let ProjectileKind::W098b { vel, pillar_bounce: true, pillar_rest, .. } = &mut pr.kind {
                        // 术士之战：火球击中柱子能够反弹（Straight 运动由 vel 驱动）。
                        // 反弹同时仍按 098c 对柱子造成伤害（nx=40 可摧毁），与「被挡下消失」分支一致。
                        // 注意：柱面是「面」，反弹应沿切向反射（v' = v − 2(v·n)n）。
                        // `mirror_by` 是「沿法线所在直线」反射（保留法向、翻转切向），正面撞击时 v 不变，故这里不用它。
                        let dot = vel.dot(normal); // normal 已是单位向量（delta/dist）
                        *vel -= normal * (dot * Fix64::from_num(2));
                        *vel = *vel * *pillar_rest; // 098c `xv`：1=满反弹、.75=衰减（S008）
                        pr.pos = o.pos + normal * min; // 推出柱面，避免下帧仍重叠而反复反弹
                        let dmg = match &pr.kind {
                            ProjectileKind::W098b { gx, .. } => gx.to_num::<f64>(),
                            _ => 0.0,
                        };
                        if dmg > 0.0 {
                            let o = &mut self.obstacles[oi];
                            o.hp = o.hp.saturating_sub(dmg.ceil() as u32);
                        }
                        if self.obstacles[oi].hp == 0 {
                            let ppos = self.obstacles[oi].pos;
                            self.combat_events.push(CombatEvent::PillarBreak { pos: ppos });
                            self.obstacles.remove(oi);
                        }
                    } else {
                        // 098c 柱子可摧毁（nx=40，D9 批次3）：火球类直伤弹命中扣 HP，归零移除
                        //（每轮 re-layout 即重生成）。其余弹体被挡下消失。
                        let dmg = match &pr.kind {
                            ProjectileKind::W098b { gx, .. } => gx.to_num::<f64>(),
                            ProjectileKind::Bullet { damage, .. } => damage.to_num::<f64>(),
                            _ => 0.0,
                        };
                        if dmg > 0.0 {
                            let o = &mut self.obstacles[oi];
                            o.hp = o.hp.saturating_sub(dmg.ceil() as u32);
                        }
                        if self.obstacles[oi].hp == 0 {
                            // 098c：柱子被摧毁移除（每轮 re-layout 即重生成）；掉落 Shard 待拾取系统
                            let ppos = self.obstacles[oi].pos;
                            self.combat_events.push(CombatEvent::PillarBreak { pos: ppos });
                            self.obstacles.remove(oi);
                        }
                        // S013B 搬运（098c `pB` tooltip）：「若碰到任何非术士障碍物，你会与它互换位置」
                        // —— 弹体撞柱时就地传送施法者（否则施法者永远到不了）。
                        if matches!(&pr.kind, ProjectileKind::W098b { on_hit: crate::skill::W098bOnHit::CarrySelf, .. }) {
                            if let Some(owner) = self.players.get_mut(pr.owner as usize) {
                                owner.pos = pr.pos;
                            }
                        }
                        pr.alive = false; // 被柱子挡下：直接消失
                    }
                    break;
                }
            }
        }

        // 2) 判定与收集对玩家的影响：命中伤害 / AOE / 持续伤害 / 爆炸。
        // 每个 (伤害, 来源) 事件在 4) 统一结算；被反弹护盾命中的直射弹只反射方向。
        let mut events: Vec<(u32, Fix64, Option<u32>)> = Vec::new();
        // 持续伤害（每帧 DoT：引力场 / 力场星域 / 锁链 / 滚动火球 / 线束）：
        // **不**触发 098c 的 `Gn×1.1` 伤害成长 —— 原版成长由 `bv[弹体]` 门控（每发弹体命中一次），
        // 若每帧都涨会指数爆炸（1.1^60/s）导致秒杀。
        let mut dot_events: Vec<(u32, Fix64, Option<u32>)> = Vec::new();
        let mut heals: Vec<(u32, Fix64)> = Vec::new(); // 力场治疗（B4-Y）
        let mut explode: Vec<ProjExplosion> = Vec::new();
        let mut pushes: Vec<(u32, Vec2, f64, bool)> = Vec::new(); // (受害者 id, 击退方向, 时长, 098b 衰减模型?)
        let mut reflect_bullets: Vec<(usize, Vec2)> = Vec::new(); // (proj 下标, 反射后的 dir)
        // 098b 弹跳弹重定向：(proj 下标, 本次受害者, 衰减后 gx, 朝下一目标的速度)。
        let mut bounce_redirs: Vec<(usize, u32, Fix64, Vec2)> = Vec::new();
        // 098b on_hit 控制效果：(受害者, Tied 时长)。
        let mut debuffs: Vec<(u32, f64)> = Vec::new();
        // 链体（锁链）生成：命中落地为持久 Tether，逐帧对绑定目标施加每秒伤害并按 pull_speed 符号拉拽。
        let mut tether_spawns: Vec<Projectile> = Vec::new();
        // 镜像分身（C 栏）火球生成：Clone 倒计时到点时朝最近敌人发射的火弹。
        let mut mirror_fires: Vec<Projectile> = Vec::new();
        // 镜像分身开火待写回队列：(分身下标, 方向, 伤害) —— 计时器重置需 &mut，延后到 2c3 段。
        let mut mirror_fire_queue: Vec<(usize, Vec2, Fix64)> = Vec::new();
        // 陨石灼烧 Scorched debuff：(受害者, 时长)。
        let mut debuffs_scorched: Vec<(u32, f64)> = Vec::new();
        // 098b 命中点燃场（S003/S004 无）：命中处生成 2.5s DoT 区域（复用 Star 的区域伤害逻辑）。
        let mut ignites: Vec<(u32, Vec2, Fix64, Fix64)> = Vec::new(); // (owner, 命中点, DoT 总量, 时长 s)
        let mut pancakes: Vec<(u32, f64)> = Vec::new(); // 「肉饼」减速（B4 岩浆滚石）
        let mut slows: Vec<(u32, f64)> = Vec::new(); // 汲取·减速（B4-T）
        let mut weakens: Vec<(u32, f64)> = Vec::new(); // 汲取·削弱（B4-T）
        let mut silences: Vec<(u32, f64)> = Vec::new(); // 禁锢·沉默（B4-Y）
        // 沉默来源：(施法者 owner, 受害者)，用于「一次沉默 ≥3 目标」播报（098c Silencer）。
        let mut silence_src: Vec<(u32, u32)> = Vec::new();
        let mut magma_absorb: Vec<(usize, u32, Fix64)> = Vec::new(); // (滚石索引, owner, 半径)

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
                                pushes.push((victim, dd.normalized() * push_power, push_time.to_num::<f64>(), false));
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
                            pushes.push((victim, dd.normalized() * *push_power, push_time.to_num::<f64>(), false));
                        }
                    }
                }
                ProjectileKind::Banana { damage, radius, push_time, push_power, .. } => {
                    // 香蕉弹：命中即直接伤害 + 击退
                    if let Some((victim, dd)) = nearest_hit(&self.players, pr.pos, pr.owner, *radius) {
                        pr.alive = false;
                        events.push((victim, *damage, Some(pr.owner)));
                        if dd.length_squared() > Fix64::ZERO {
                            pushes.push((victim, dd.normalized() * *push_power, push_time.to_num::<f64>(), false));
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
                            dot_events.push((p.id, *damage_per_sec * dt, Some(pr.owner)));
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
                                dot_events.push((p.id, *damage_per_sec * dt, Some(pr.owner)));
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
                            pushes.push((victim, dd.normalized() * *push_power, push_time.to_num::<f64>(), false));
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
                    // 镜像分身免疫：被链目标若处于 Mirror 期间，不结算链伤害/拉拽。
                    if !self.players.get(*target as usize).is_some_and(|p| p.has_buff(BuffKind::Mirror)) {
                        dot_events.push((*target, *damage_per_sec * dt, Some(*owner)));
                    }
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
                                dot_events.push((p.id, *damage_per_sec * dt, Some(*owner)));
                            }
                        }
                    }
                }
                ProjectileKind::Clone { owner, fire_timer, fire_dmg, .. } => {
                    // 镜像分身：开火倒计时到点 → 朝最近敌人发射一发火球。
                    // 本段是 `&pr.kind` 不可变借用，发射与计时器重置都延后到 2c3 段统一写回。
                    if *fire_timer <= Fix64::ZERO {
                        let oteam = self.players.get(*owner as usize).map(|p| p.team);
                        let mut best: Option<(Fix64, Vec2)> = None;
                        for q in self.players.iter() {
                            if !q.alive || Some(q.team) == oteam {
                                continue;
                            }
                            let ds = (q.pos - pr.pos).length_squared();
                            if best.map(|(b, _)| ds < b).unwrap_or(true) {
                                best = Some((ds, q.pos));
                            }
                        }
                        if let Some((_, tpos)) = best {
                            let dir = (tpos - pr.pos).normalized();
                            // (分身下标, 方向, 伤害) —— 火球与计时器重置在 2c3 段处理。
                            mirror_fire_queue.push((pi, dir, *fire_dmg));
                        }
                    }
                }
                ProjectileKind::W098b { proj: crate::skill::W098bProjKind::Magma, radius, .. } => {
                    // 岩浆滚石接触（098c OB/VB，B4）：推离 + 「肉饼」减速 ×0.1（1.5s）+ 吸收敌方弹体。
                    let owner = pr.owner;
                    let oteam: Option<u8> = self.players.get(owner as usize).map(|p| p.team);
                    for (j, q) in self.players.iter().enumerate() {
                        if !q.alive || Some(q.team) == oteam {
                            continue;
                        }
                        let d = q.pos - pr.pos;
                        let rr = *radius + q.radius;
                        if d.length_squared() <= rr * rr {
                            let dir = if d.length_squared() > Fix64::ZERO { d.normalized() } else { Vec2::new(Fix64::ONE, Fix64::ZERO) };
                            pushes.push((j as u32, dir * Fix64::from_num(300.0), 0.3, false));
                            pancakes.push((j as u32, 1.5));
                        }
                    }
                    magma_absorb.push((pi, owner, *radius));
                }
                ProjectileKind::Star { owner, radius, damage_per_sec, heal_per_sec, remaining: _, heal_team } => {
                    // 星域：范围内敌掉血、对施法者回血
                    for j in 0..n {
                        let p = &self.players[j];
                        if !p.alive {
                            continue;
                        }
                        let rr = *radius + p.radius;
                        if (p.pos - pr.pos).length_squared() <= rr * rr
                            && p.id != *owner
                            && Some(p.team) != self.players.get(*owner as usize).map(|o| o.team)
                        {
                            dot_events.push((p.id, *damage_per_sec * dt, Some(*owner)));
                            // 力场（B4-Y）：范围内敌人减速 45%（098c Lc：降低移速 45%）。
                            // 仅 heal_team（力场形态）施加；每帧刷新短窗避免离开后残留。
                            if *heal_team {
                                self.players[j].add_buff(BuffKind::Slow(0.55), 0.3);
                            }
                        }
                    }
                    // 力场（B4-Y）：heal_team 时治疗范围内全部队友（否则只奶 owner）
                    if *heal_team {
                        let oteam = self.players.get(*owner as usize).map(|p| p.team);
                        for (j, q) in self.players.iter().enumerate() {
                            if !q.alive || Some(q.team) != oteam {
                                continue;
                            }
                            let rr = *radius + q.radius;
                            if (q.pos - pr.pos).length_squared() <= rr * rr {
                                heals.push((j as u32, *heal_per_sec * dt));
                            }
                        }
                    } else if let Some(o) = self.players.get_mut(*owner as usize) {
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
                            pushes.push((victim, dd.normalized() * *push_power, push_time.to_num::<f64>(), false));
                        }
                    }
                }
                ProjectileKind::W098b { proj, radius, gx, kb_ji, ignite, blast, target, speed, on_hit, debuff_dur, lightning_dmg, .. } => {
                    // 098b 弹体命中：KI/FI 结算（PORT_098B_DECISIONS.md D3/M1）——
                    // FI 伤害 = gx × Gn[攻] × hn[守]（M1 Gn/hn=1，框架位预留）；
                    // KI 击退初速 = (100+目标魔法) × gx × kb_ji（动态，D9），方向沿弹-目标连线。
                    // Bounce 命中判定排除上一跳受害者（target）——重定向瞬间还贴着旧目标，
                    // 不排除会每帧重复结算同一目标刷伤害。
                    // 098c 回旋镖：飞行中不结算（oB 在回程寿命耗尽时对命中半径 qI 内目标一次性 AOE 结算）。
                    let hit = if *proj == crate::skill::W098bProjKind::Boomerang {
                        None
                    } else if blast.is_some() {
                        // 陨石：飞行途中不结算（098c `iB`：一路飞到点击点，仅在到点由 `oB` 做 AOE）。
                        None
                    } else if *on_hit == crate::skill::W098bOnHit::CarrySelf {
                        // 搬运弹体（098c `pB`）：不与玩家碰撞，飞抵落点后才传送施法者。
                        None
                    } else if *proj == crate::skill::W098bProjKind::Bounce {
                        nearest_hit_with_skip(&self.players, pr.pos, pr.owner, *radius, target.unwrap_or(pr.owner))
                    } else if *on_hit == crate::skill::W098bOnHit::RedChain {
                        // 红链可命中友军（触发闪电，098c sc）。
                        nearest_hit_any(&self.players, pr.pos, pr.owner, *radius)
                    } else {
                        nearest_hit(&self.players, pr.pos, pr.owner, *radius)
                    };
                    if let Some((victim, dd)) = hit {
                        let skip = *target;
                        // 锁链的伤害走 Tether 的 `damage_per_sec`（文档 `0.2+0.1×L` 是**每秒**，
                        // 与引力「每秒 0.3+0.2×L」同量级），不再按单发直伤结算（0.2 单发等于没有）。
                        let is_chain = *on_hit == crate::skill::W098bOnHit::ChainPull
                            || *on_hit == crate::skill::W098bOnHit::RedChain;
                        if !is_chain {
                            events.push((victim, *gx, Some(pr.owner)));
                        }
                        // 守护之盾充能（098c ib/ab/Eb）：火球命中敌人 → Ha 点亮（GX 设充能灯 1）。
                        // 火球指纹 = Straight + Ki + 有点燃（法杖/精通爆炸变体同样充能）。
                        if *proj == crate::skill::W098bProjKind::Straight
                            && *on_hit == crate::skill::W098bOnHit::Ki
                            && ignite.is_some()
                        {
                            if let Some(o) = self.players.get_mut(pr.owner as usize) {
                                if o.item_fx.aegis {
                                    o.aegis_charged = true;
                                }
                            }
                        }
                        // 锁链（蓝链拉目标 / 红链拉施法者）以拉拽为主：跳过 KI 击退
                        //（击退 700 位移会盖过 300 的拉拽）。
                        if !is_chain && dd.length_squared() > Fix64::ZERO {
                            let vmana = self.players[victim as usize].mana;
                            let atk_gn = self.players.get(pr.owner as usize).map(|a| a.gn_factor()).unwrap_or(1.0);
                            let vic_hn = self.players[victim as usize].dmg_taken_mult;
                            let kb = warlock_ki_knockback(vmana, *gx, *kb_ji, atk_gn, vic_hn);
                            pushes.push((victim, dd.normalized() * kb, W098B_KB_TIME, true));
                        }
                        if let Some(total) = ignite {
                            ignites.push((pr.owner, pr.pos, *total, (*debuff_dur).max(Fix64::from_num(W098B_IGNITE_SECONDS))));
                        }
                        if let Some(br) = blast {
                            // 远程精通火球的落点爆炸只在到点/撞柱触发（JASS Bb），直中不重复炸。
                            let is_fireball = *proj == crate::skill::W098bProjKind::Straight
                                && *on_hit == crate::skill::W098bOnHit::Ki
                                && ignite.is_some();
                            if !is_fireball {
                                expiry_blasts.push((pr.owner, pr.pos, *br, *gx, *kb_ji));
                            }
                        }
                        // on_hit 命中副作用（M2 批次C）：S017 残废 / S019 拉拽（KI 伤害照常）。
                        match on_hit {
                            crate::skill::W098bOnHit::Ki => {}
                            crate::skill::W098bOnHit::Scorched => {
                                // 陨石灼烧「烤肉饼」（D7）：输出 ×0.1 + 禁疗，时长 = growth.duration（4s）。
                                debuffs_scorched.push((victim, debuff_dur.to_num::<f64>()));
                            }
                            crate::skill::W098bOnHit::Cripple => {
                                // 残废：禁施法/禁移动近似为 Tied，时长 (4+0.25L)。
                                debuffs.push((victim, debuff_dur.to_num::<f64>()));
                            }
                            crate::skill::W098bOnHit::ChainPull => {
                                // 锁链（蓝链）：落地为持久 Tether——逐帧对绑定目标施加每秒伤害
                                //（damage_per_sec=gx=0.2+0.1×L），并把目标拉向施法者（pull_speed 取正）。
                                debuffs.push((victim, debuff_dur.to_num::<f64>()));
                                // 镜像分身无敌窗口：否决锁链（文档「否决锁链和负面效果」）。
                                if !self.players[victim as usize].mirror_immune() {
                                    tether_spawns.push(Projectile {
                                        owner: pr.owner,
                                        kind: ProjectileKind::Tether {
                                            owner: pr.owner,
                                            target: victim,
                                            damage_per_sec: *gx,
                                            pull_speed: Fix64::from_num(600.0), // >0：目标→施法者
                                            remaining: *debuff_dur,
                                            beam: true, // 沿连线切割经过的敌人
                                        },
                                        pos: pr.pos,
                                        alive: true,
                                    });
                                }
                            }
                            crate::skill::W098bOnHit::DrainSlow => {
                                // 汲取·减速（098c vc，B4-T）：目标移速 ×0.5 + 施法者回血伤害×50%。
                                slows.push((victim, debuff_dur.to_num::<f64>()));
                                if let Some(o) = self.players.get_mut(pr.owner as usize) {
                                    if o.alive {
                                        let heal = gx.to_num::<f64>() * 0.5;
                                        o.hp = (o.hp + Fix64::from_num(heal)).min(o.max_hp);
                                    }
                                }
                            }
                            crate::skill::W098bOnHit::Weaken => {
                                // 汲取·削弱（098c oc，B4-T）：目标输出 ×0.5。
                                #[cfg(test)]
                                if std::env::var("WKDBG").is_ok() {
                                    println!("DBG weaken hit victim={victim} dur={:?}", debuff_dur.to_num::<f64>());
                                }
                                weakens.push((victim, debuff_dur.to_num::<f64>()));
                            }
                            crate::skill::W098bOnHit::Recharge => {
                                // 弹跳弹·充能（098c cc，B4-T）：命中立即刷新施法者该技能冷却。
                                if let Some(o) = self.players.get_mut(pr.owner as usize) {
                                    o.caster.reset_cooldown(SkillId::S016);
                                }
                            }
                            crate::skill::W098bOnHit::RedChain => {
                                // 锁链·红链（文档「红链」）：链到敌人 → 把**施法者**拉向目标；
                                // 链到**友军/柱子** → 引发闪电（098c `sc`，伤害 lightning_dmg=1.0→3.4）。
                                let same_team = self.players.get(pr.owner as usize).map(|p| p.team)
                                    == self.players.get(victim as usize).map(|p| p.team);
                                if same_team {
                                    if *lightning_dmg > Fix64::ZERO {
                                        events.push((victim, *lightning_dmg, Some(pr.owner)));
                                        self.lightning_visual.push((pr.pos, self.players[victim as usize].pos, Fix64::from_num(0.1)));
                                    }
                                } else if !self.players[victim as usize].mirror_immune() {
                                    // 落地为持久 Tether，把施法者拉向命中目标（pull_speed 取负）。
                                    // 绑定目标仍逐帧承受每秒伤害（damage_per_sec=gx=0.2+0.1×L）。
                                    // 镜像分身无敌窗口：否决锁链（文档「否决锁链和负面效果」）。
                                    tether_spawns.push(Projectile {
                                        owner: pr.owner,
                                        kind: ProjectileKind::Tether {
                                            owner: pr.owner,
                                            target: victim,
                                            damage_per_sec: *gx,
                                            pull_speed: Fix64::from_num(-600.0), // <0：施法者→目标
                                            remaining: *debuff_dur,
                                            beam: true, // 沿连线切割经过的敌人
                                        },
                                        pos: pr.pos,
                                        alive: true,
                                    });
                                }
                            }
                            crate::skill::W098bOnHit::Silence => {
                                // 禁锢·沉默（098c CC，B4-Y）：禁施法（可移动）。
                                silences.push((victim, debuff_dur.to_num::<f64>()));
                                silence_src.push((pr.owner, victim));
                            }
                            crate::skill::W098bOnHit::SwapTarget => {
                                // S013A 换位（098c `MB`）：命中敌人 → 施法者与该敌人**互换位置**，弹体销毁。
                                let a = pr.owner as usize;
                                let b = victim as usize;
                                if a != b {
                                    let pa = self.players[a].pos;
                                    let pb = self.players[b].pos;
                                    self.players[a].pos = pb;
                                    self.players[b].pos = pa;
                                }
                            }
                            crate::skill::W098bOnHit::CarrySelf => {
                                // S013B 搬运（098c `pB`）：把施法者传送到弹体位置。
                                if let Some(o) = self.players.get_mut(pr.owner as usize) {
                                    o.pos = pr.pos;
                                }
                            }
                        }
                        // （098c 回旋镖飞行中不命中、不转回程：回程由运动学前向归零触发、到位时 AOE 结算。）
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
                        } else if *proj != crate::skill::W098bProjKind::Boomerang {
                            pr.alive = false;
                        }
                    }
                }
                ProjectileKind::Gravity { radius, damage_per_sec, .. } => {
                    // 黑洞（A 形态）：伤害半径比拉拽半径小（098c `hc`：`Rr<75000`≈274²；拉拽为 600²）。
                    // `damage_per_sec` 已是每秒 DPS（每 tick 0.1+0.2×等级 ÷ 0.06 换算，见 skill.rs）。
                    let owner = pr.owner;
                    let oteam = self.players.get(owner as usize).map(|p| p.team);
                    let dmg_r = (*radius).min(Fix64::from_num(DARK_MATTER_DAMAGE_RADIUS));
                    for j in 0..n {
                        let p = &self.players[j];
                        if !p.alive || Some(p.team) == oteam {
                            continue;
                        }
                        let rr = dmg_r + p.radius;
                        if (p.pos - pr.pos).length_squared() <= rr * rr {
                            dot_events.push((p.id, *damage_per_sec * dt, Some(owner)));
                        }
                    }
                }
                ProjectileKind::Rock { .. } | ProjectileKind::Decoy { .. } | ProjectileKind::ScatterLine { .. } | ProjectileKind::Chain { .. } | ProjectileKind::Returner { .. } | ProjectileKind::DelayedBlast { .. } => {}
            }
        }

        // 2b) 应用反弹护盾对直射弹的反射（改方向，不消耗、不伤害）。
        for (pi, new_dir) in reflect_bullets {
            if let ProjectileKind::Bullet { dir, .. } = &mut ps[pi].kind {
                *dir = new_dir;
                // 可让被反射的弹体仍归属原施法者（原版弹一次）
            }
        }

        // 2b2) 应用 098b on_hit 控制效果（Tied debuff / 拉向施法者 / 灼烧 Scorched）。
        for (vid, dur) in debuffs_scorched {
            if let Some(p) = self.players.get_mut(vid as usize) {
                if p.alive && !p.mirror_immune() {
                    p.add_buff(BuffKind::Scorched, dur);
                }
            }
        }
        for (vid, dur) in debuffs {
            if let Some(p) = self.players.get_mut(vid as usize) {
                if p.alive && !p.mirror_immune() {
                    p.add_buff(BuffKind::Tied, dur);
                }
            }
        }
        // 2b3) 弹体互撞（098c `Av`/`hv` 简化版）：不同队伍的飞行弹体相撞 → 互毁。
        // 回旋镖不与同类互撞（098c `Ub` 只开 1/3 类）；岩浆走 magma_absorb，已排除。
        {
            let n_proj = ps.len();
            for i in 0..n_proj {
                if !ps[i].alive {
                    continue;
                }
                let Some((ci, fi)) = ps[i].kind.missile_collision() else {
                    continue;
                };
                let ri = ps[i].kind.obstacle_radius().unwrap_or(Fix64::ZERO);
                for j in (i + 1)..n_proj {
                    if !ps[j].alive {
                        continue;
                    }
                    let Some((cj, fj)) = ps[j].kind.missile_collision() else {
                        continue;
                    };
                    if ps[i].owner == ps[j].owner {
                        continue;
                    }
                    let ti = self.players.get(ps[i].owner as usize).map(|p| p.team);
                    let tj = self.players.get(ps[j].owner as usize).map(|p| p.team);
                    if ti.is_some() && ti == tj {
                        continue;
                    }
                    if fi & (1 << (cj - 1)) == 0 || fj & (1 << (ci - 1)) == 0 {
                        continue;
                    }
                    let rj = ps[j].kind.obstacle_radius().unwrap_or(Fix64::ZERO);
                    let rr = ri + rj;
                    if (ps[i].pos - ps[j].pos).length_squared() <= rr * rr {
                        ps[i].alive = false;
                        ps[j].alive = false;
                    }
                }
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
        // 2c2) 回旋镖回程到位结算（098c oB）：对命中半径 qI = $D2×√(1+0.25xi) 内敌人
        // 造成伤害（Zb 随命中距离衰减：因子 = 1 − d/(400+40xi)）+ KI 击退。
        for (owner, pos, gx, kb_ji) in boomerang_settles {
            let xi = self.players.get(owner as usize).map(|p| p.mastery[1] as f64).unwrap_or(0.0);
            let q_i = Fix64::from_num(210.0 * (1.0 + 0.25 * xi).sqrt());
            for j in 0..n {
                let p = &self.players[j];
                if !p.alive || p.id == owner {
                    continue;
                }
                let d = (p.pos - pos).length();
                if d <= q_i + p.radius {
                    let factor = (Fix64::ONE - d / Fix64::from_num(400.0 + 40.0 * xi)).max(Fix64::ZERO);
                    events.push((p.id, gx * factor, Some(owner)));
                    if d > Fix64::ZERO {
                        let atk_gn = self.players.get(owner as usize).map(|a| a.gn_factor()).unwrap_or(1.0);
                        let kb = warlock_ki_knockback(p.mana, gx, kb_ji, atk_gn, p.dmg_taken_mult);
                        pushes.push((p.id, (p.pos - pos).normalized() * kb, W098B_KB_TIME, true));
                    }
                }
            }
        }

        // 2c3) 镜像分身开火：重置开火倒计时，并从分身位置射出火球。
        for (pi, dir, dmg) in mirror_fire_queue {
            let (Some(clone_owner), Some(clone_pos)) = (ps.get(pi).map(|c| c.owner), ps.get(pi).map(|c| c.pos)) else {
                continue;
            };
            let mut fire_cd = Fix64::ZERO;
            if let ProjectileKind::Clone { fire_timer, fire_cd: cd, .. } = &mut ps[pi].kind {
                *fire_timer = *cd;
                fire_cd = *cd;
            }
            // 计时器已重置但尚未推进（避免同帧再次触发）
            if fire_cd > Fix64::ZERO {
                mirror_fires.push(Projectile {
                    owner: clone_owner,
                    kind: ProjectileKind::W098b {
                        proj: crate::skill::W098bProjKind::Straight,
                        vel: dir * Fix64::from_num(600.0),
                        speed: Fix64::from_num(600.0),
                        radius: Fix64::from_num(35.0),
                        remaining: Fix64::from_num(2.0),
                        life: Fix64::from_num(2.0),
                        gx: dmg,
                        kb_ji: Fix64::ONE,
                        ignite: None,
                        blast: None,
                        target: None,
                        returning: false,
                        on_hit: crate::skill::W098bOnHit::Ki,
                        debuff_dur: Fix64::ZERO,
                        lateral: Fix64::ZERO,
                        forward_dir: dir,
                        out_dist: Fix64::from_num(1200.0),
                        burst: 0,
                        emit_cooldown: Fix64::ZERO,
                        emit_angle: 0.0,
                        pillar_bounce: false,
                        pillar_rest: Fix64::ZERO,
                        lightning_dmg: Fix64::ZERO,
                    },
                    pos: clone_pos,
                    alive: true,
                });
            }
        }

        // 3) 结算爆炸（石头 / 导弹）
        for e in &explode {
            self.explode_at(e.pos, e.owner, e.radius, e.damage, e.bomb_force, false, false, DmgFalloff::None);
        }

        // 4) 结算命中/持续伤害（受护盾吸收、记录击杀来源）
        for (victim, amount, from) in events {
            self.damage_player(victim, amount, from);
        }
        // 4a-bis) 持续伤害（DoT）：同上，但**不涨 Gn**（见 `dot_events` 声明处说明）。
        for (victim, amount, from) in dot_events {
            self.damage_player_dot(victim, amount, from);
        }
        // 4a) 结算弹体直接命中的击退（回旋镖 / 香蕉）
        for (victim, heal_amt) in heals.drain(..) {
            if let Some(p) = self.players.get_mut(victim as usize) {
                if p.alive {
                    p.hp = (p.hp + heal_amt).min(p.max_hp);
                }
            }
        }
        for (victim, vel, time, decay) in pushes {
            let vel = vel * self.knockback_mult;
            if let Some(p) = self.players.get_mut(victim as usize) {
                if p.alive {
                    if decay {
                        // 098b 衰减模型（D8）：初速缩放（有效击退减免在 push_knockback 内）。
                        p.push_knockback(vel);
                    } else {
                        p.push(vel, time); // Unity 版恒速：击退时长不再按 kb_factor 缩放（属性系统已删除）
                    }
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
        // 「肉饼」减速（B4 岩浆滚石）：Speed ×0.1 buff；同时发 Pancake 表现事件（098c 播报）。
        let pancake_events: Vec<CombatEvent> = pancakes
            .iter()
            .filter_map(|(v, _)| self.players.get(*v as usize).map(|p| CombatEvent::Pancake { victim: *v, pos: p.pos }))
            .collect();
        for (victim, dur) in pancakes.drain(..) {
            if let Some(p) = self.players.get_mut(victim as usize) {
                if p.alive {
                    p.add_buff(BuffKind::Pancake, dur);
                }
            }
        }
        self.combat_events.extend(pancake_events);
        // 汲取·减速/削弱（B4-T）
        for (victim, dur) in slows.drain(..) {
            if let Some(p) = self.players.get_mut(victim as usize) {
                if p.alive && !p.mirror_immune() {
                    p.add_buff(BuffKind::Slow(0.5), dur);
                }
            }
        }
        for (victim, dur) in weakens.drain(..) {
            if let Some(p) = self.players.get_mut(victim as usize) {
                if p.alive && !p.mirror_immune() {
                    p.add_buff(BuffKind::Weakened, dur);
                }
            }
        }
        // 禁锢·沉默（B4-Y）：禁施法（可移动）
        for (victim, dur) in silences.drain(..) {
            if let Some(p) = self.players.get_mut(victim as usize) {
                if p.alive && !p.mirror_immune() {
                    p.add_buff(BuffKind::Silenced, dur);
                }
            }
        }
        // 098c 播报：同一施法者本 tick 沉默 ≥ 3 个目标（Silencer）。
        {
            let mut counts: Vec<(u32, u32)> = Vec::new();
            for (owner, _) in silence_src.drain(..) {
                match counts.iter_mut().find(|(o, _)| *o == owner) {
                    Some((_, n)) => *n += 1,
                    None => counts.push((owner, 1)),
                }
            }
            for (owner, n) in counts {
                if n >= 3 {
                    let pos = self
                        .players
                        .get(owner as usize)
                        .map(|p| p.pos)
                        .unwrap_or(Vec2::new(Fix64::ZERO, Fix64::ZERO));
                    self.combat_events.push(CombatEvent::Silencer { owner, pos });
                }
            }
        }
        // 滚石吸收敌方弹体（B4）
        for (mi, owner, mradius) in magma_absorb.drain(..) {
            let (Some(mp), Some(oteam)) = (ps.get(mi), self.players.get(owner as usize).map(|p| p.team)) else {
                continue;
            };
            let (mpos, mr) = (mp.pos, mradius);
            for other in ps.iter_mut() {
                if !other.alive || other.owner == owner {
                    continue;
                }
                let o_other = self.players.get(other.owner as usize).map(|p| p.team);
                if o_other == Some(oteam) {
                    continue;
                }
                let orad = match &other.kind {
                    ProjectileKind::W098b { radius, .. } => *radius,
                    ProjectileKind::Bullet { radius, .. } => *radius,
                    _ => Fix64::from_num(15.0),
                };
                let lim = (mr + orad + Fix64::from_num(10.0)) * (mr + orad + Fix64::from_num(10.0));
                if (other.pos - mpos).length_squared() <= lim {
                    other.alive = false;
                }
            }
        }
        // 碎裂/侧弹生成（B4 S009 双形态）
        for (owner, pos, vel, gx, radius, life, kb_ji) in spawn_bullets.drain(..) {
            ps.push(Projectile {
                owner,
                kind: ProjectileKind::W098b {
                    proj: crate::skill::W098bProjKind::Straight,
                    vel,
                    speed: Fix64::from_num(600.0),
                    radius,
                    remaining: life,
                    life,
                    gx,
                    kb_ji,
                    ignite: None,
                    blast: None,
                    target: None,
                    returning: false,
                    on_hit: crate::skill::W098bOnHit::Ki,
                    debuff_dur: Fix64::ZERO,
                    lateral: Fix64::ZERO,
                    forward_dir: vel.normalized(),
                    out_dist: life * Fix64::from_num(600.0),
                    burst: 0,
                    emit_cooldown: Fix64::ZERO,
                    emit_angle: 0.0,
                    pillar_bounce: false,
                    pillar_rest: Fix64::ZERO,
                    lightning_dmg: Fix64::ZERO,
                },
                pos,
                alive: true,
            });
        }
        for (owner, center, br, gx, ji) in expiry_blasts.drain(..) {
            self.explode_at(center, owner, br, gx, Fix64::from_num(100.0) * gx * ji, false, false, DmgFalloff::None);
        }
        // 陨石落地（098c `oB`）：中心伤害 `12+2L`，随距离衰减 `(1 - d/(400+40xi))`，同队/自身免疫。
        for (owner, center, radius, damage, kb_ji, denom) in delayed_blasts.drain(..) {
            self.explode_at(center, owner, radius, damage, Fix64::from_num(100.0) * damage * kb_ji, true, false, DmgFalloff::Mul(denom));
        }
        // 4d) 098b 命中点燃场（S000 火球 xc）：命中处半径 75（spec aoe_radius_obj）、
        // 时长 2.5s（consolidated：2.5×jn），总量均摊为 DPS。复用 Star 的静态区域伤害。
        for (owner, pos, total, secs) in ignites.drain(..) {
            ps.push(Projectile {
                owner,
                kind: ProjectileKind::Star {
                    owner,
                    radius: Fix64::from_num(75.0),
                    damage_per_sec: total / secs,
                    heal_per_sec: Fix64::ZERO,
                    remaining: secs,
                            heal_team: false,
                        },
                pos,
                alive: true,
            });
        }

        // 4e) 链体（锁链）落地：作为持久 Tether 加入，逐帧对绑定目标施加每秒伤害 + 符号拉拽。
        for t in tether_spawns.drain(..) {
            ps.push(t);
        }
        // 4f) 镜像分身火球：作为普通 W098b 火弹加入。
        for f in mirror_fires.drain(..) {
            ps.push(f);
        }
        // 5) 写回并清除已死亡/失效的弹体
        ps.retain(|p| p.alive);
        self.projectiles = ps;
    }

    /// 射线-圆求交：返回 (沿射线距离 t, 交点)。无交返回 None。圆需在射线前方。
    fn ray_circle_t(origin: Vec2, dir: Vec2, center: Vec2, radius: Fix64) -> Option<(Fix64, Vec2)> {
        let oc = center - origin;
        let proj = oc.dot(dir);
        if proj < Fix64::ZERO {
            return None; // 圆心在射线后方
        }
        let perp_sq = oc.length_squared() - proj * proj;
        let r_sq = radius * radius;
        if perp_sq > r_sq {
            return None;
        }
        let back = (r_sq - perp_sq).sqrt();
        let t = proj - back; // 进入点
        if t < Fix64::ZERO {
            // 原点已在圆内：t=0
            Some((Fix64::ZERO, origin))
        } else {
            Some((t, origin + dir * t))
        }
    }

    /// 找到离某个位置最近、且不是 `owner` 的存活玩家。
    fn nearest_enemy(&self, pos: Vec2, owner: u32) -> Option<Vec2> {
        let owner_team = self.players.get(owner as usize).map(|p| p.team);
        let mut best: Option<(Fix64, Vec2)> = None;
        for p in self.players.iter() {
            if !p.alive || p.id == owner || Some(p.team) == owner_team {
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
        let owner_team = self.players.get(owner as usize).map(|p| p.team);
        let mut best: Option<(Fix64, Vec2)> = None;
        for p in self.players.iter() {
            if !p.alive || p.id == owner || Some(p.team) == owner_team {
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
        let owner_team = self.players.get(owner as usize).map(|p| p.team);
        let mut best: Option<(Fix64, Vec2)> = None;
        for p in self.players.iter() {
            if !p.alive || p.id == owner || p.id == skip || Some(p.team) == owner_team {
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
    /// `exclude_owner`：以自身为中心的 nova（098b S001/S020/S021）不伤施法者。
    /// `is_smite`：天罚系 nova（S001/S020/S021）——受害者的守护之盾减免生效（M3 2c）。
    /// `bomb_force`：击退初速基数（098c 动态击退按受击者 mana 在内部放大，D9）。
    #[allow(clippy::too_many_arguments)]
    /// 返回被命中的**非施法者**玩家数（098c mC 的 n：鲜血之剑/面具回血按命中敌人数结算）。
    fn explode_at(&mut self, pos: Vec2, owner: u32, radius: Fix64, damage: Fix64, bomb_force: Fix64, exclude_owner: bool, is_smite: bool, dmg_falloff: DmgFalloff) -> u32 {
        // 纯表现：记录一次爆炸（客户端画扩散圆环）。不参与快照/哈希。
        self.combat_events.push(CombatEvent::Explode { pos, radius });
        let r_sq = radius * radius;
        // 攻方 Gn 系数（灼烧 ×0.1，D7）：循环前取出，避免 iter_mut 借用冲突。
        let owner_gn = self
            .players
            .get(owner as usize)
            .map(|a| a.gn_factor())
            .unwrap_or(1.0);
        // 全局伤害倍率（098c `Gn`，D9）：098c 在统一伤害入口 `hI` 里乘 `Gn[攻方]`，
        // AoE 经 mI→hI 也乘；故这里同样乘上（否则陨石/新星等爆炸不吃倍率）。
        let world_dmg_mult = self.damage_mult;
        // 队伍过滤（098c cn[]，B2）：技能 nova 只伤异队（owner 同队天然排除）。
        let owner_team = self.players.get(owner as usize).map(|p| p.team);
        let mut deaths: Vec<u32> = Vec::new();
        let mut hit_non_owner = false;
        let mut hit_enemies: u32 = 0;
        let mut hits: Vec<u32> = Vec::new();
        for p in self.players.iter_mut() {
            if !p.alive || (exclude_owner && p.id == owner) || Some(p.team) == owner_team {
                continue;
            }
            let d = p.pos - pos;
            let d_sq = d.length_squared();
            if d_sq <= r_sq {
                // 受伤（记录击杀者）；boost 期间返还一半回血；护甲/法抗折算 + 098c 攻方 Gn（D9）。
                if p.id != owner {
                    p.last_hit_by = Some(owner);
                }
                let mut dmg = {
                    // 距离衰减（098c）：陨石为乘法 `×(1-d/k)`；灾变为加法 `dmg - d/k`。
                    let base = match dmg_falloff {
                        DmgFalloff::Sub(k) => (damage - d_sq.sqrt() / k).max(Fix64::ZERO),
                        _ => damage,
                    };
                    let mut v = base * Fix64::from_num(owner_gn * p.dmg_taken_mult) * world_dmg_mult;
                    if let DmgFalloff::Mul(k) = dmg_falloff {
                        v *= (Fix64::ONE - d_sq.sqrt() / k).max(Fix64::ZERO);
                    }
                    v
                };
                // 守护之盾充能窗口（098c HC 'aegs' buff 5*jn）：受伤减免（I00H 25% / I00I 75%）。
                if p.has_buff(BuffKind::Aegis) && p.item_fx.smite_reduction > 0.0 {
                    dmg *= Fix64::from_num(1.0 - p.item_fx.smite_reduction);
                }
                let net = p.soak_boost(dmg);
                p.hp = (p.hp - net).max(Fix64::ZERO);
                // 098c 挨打回魔（D9 批次1）。
                p.mana += dmg.to_num::<f64>();
                if p.id != owner {
                    hit_non_owner = true;
                    hit_enemies += 1;
                    hits.push(p.id);
                }
                if p.hp == Fix64::ZERO {
                    p.alive = false;
                    deaths.push(p.id);
                }
                // 击退（沿中心连线远离，随距离衰减；走控制/强制速度；击退抗性在 push 内统一折算）
                // nova 自伤不伴随自击退（098c 原版：只有被敌人打中才有击退）。
                if d_sq > Fix64::ZERO && !(is_smite && p.id == owner) {
                    let dist = d_sq.sqrt();
                    let falloff = (Fix64::ONE - dist / radius).max(Fix64::from_num(0.2));
                    let dir = d.normalized();
                    // nova/爆炸击退走 098c 衰减模型（D8/D9）；初速按受击者 mana 动态放大。
                    let vmana = p.mana;
                    let mut dyn_force = bomb_force.to_num::<f64>() * (100.0 + vmana) / 100.0;
                    // 守护之盾充能窗口：击退减半（098c HC Hn/2）。
                    if p.has_buff(BuffKind::Aegis) && p.item_fx.aegis_kb_reduction > 0.0 {
                        dyn_force *= 1.0 - p.item_fx.aegis_kb_reduction;
                    }
                    p.push_knockback(dir * Fix64::from_num(dyn_force * falloff.to_num::<f64>()) * self.knockback_mult);
                }
            }
        }
        // 098c 伤害成长（D9 批次1）：本次爆炸命中了非 owner 目标 → 施法者 Gn ×1.1。
        if hit_non_owner {
            if let Some(o) = self.players.get_mut(owner as usize) {
                if o.alive {
                    o.on_dealt_damage();
                }
            }
        }
        // 循环外记账，避免在 iter_mut 借用期间再借 self。
        for victim in deaths {
            self.record_death(victim);
        }
        // 098c 播报：一次 AoE 命中 ≥ 3 敌人（Hattrick；戴死亡面具则 Vampire）。
        if hit_enemies >= 3 {
            let (pos, vampire) = self
                .players
                .get(owner as usize)
                .map(|p| (p.pos, p.item_fx.scourge_double))
                .unwrap_or((pos, false));
            self.combat_events.push(CombatEvent::MultiHit { owner, pos, vampire });
        }
        // 098c `Denied`：天罚命中「被链接（`Fv`）+ 特殊状态（出界/凤凰/风步）」的目标 → 断链 + 播报。
        if is_smite {
            let arena = self.arena_radius;
            for &v in &hits {
                let info = self.players.get(v as usize).map(|p| {
                    (
                        p.alive,
                        p.pos.length() > arena,
                        p.phoenix_remaining > Fix64::ZERO,
                        p.windwalk_state > Fix64::ZERO,
                        p.pos,
                    )
                });
                let Some((alive, oob, phoenix, windwalk, vpos)) = info else { continue };
                if !alive || !(oob || phoenix || windwalk) {
                    continue;
                }
                if self.sever_links_to(v) > 0 {
                    self.combat_events.push(CombatEvent::Denied { owner, pos: vpos });
                }
            }
        }
        hit_enemies
    }

    /// 098c `aR`：断开**指向 `victim` 的链接弹体**（束缚/链索/束缚线），返回断开数量。
    fn sever_links_to(&mut self, victim: u32) -> usize {
        let before = self.projectiles.len();
        self.projectiles.retain(|pr| match &pr.kind {
            ProjectileKind::Tether { target, .. } => *target != victim,
            ProjectileKind::Chain { last_target, .. } => *last_target != victim,
            _ => true,
        });
        before - self.projectiles.len()
    }

    /// 死亡判定辅助：场上还存活多少玩家。
    pub fn alive_count(&self) -> usize {
        self.players.iter().filter(|p| p.alive).count()
    }

    /// 098c `CA` 招架（S010 B / `gr`）：风步中的单位**与敌人接触**时（我们以「接触」代 098c 的「被近战攻击」），
    /// 刷新风步（`Xr = 剩余 + 1.5×jn`，封顶 5s）、`gr` 进入 0.5s 冷却（`NA` 恢复），
    /// 并与攻击者**互相击退**（`MI`：攻击者 4.5、自己 2.25，× `100/(100+gn)`）。
    fn process_parry(&mut self, dt: Fix64) {
        let n = self.players.len();
        // `NA`：招架后 0.5s 恢复 `gr`（仅当仍在风步）。
        for i in 0..n {
            if self.players[i].parry_cd > Fix64::ZERO {
                self.players[i].parry_cd = (self.players[i].parry_cd - dt).max(Fix64::ZERO);
                if self.players[i].parry_cd == Fix64::ZERO && self.players[i].windwalk_state > Fix64::ZERO {
                    self.players[i].parry_ready = true;
                }
            }
        }
        for i in 0..n {
            let Some(att) = self.players[i].contact_by_enemy else { continue };
            let ai = att as usize;
            if ai >= n || ai == i {
                continue;
            }
            if self.players[i].windwalk_state <= Fix64::ZERO || !self.players[i].parry_ready {
                continue;
            }
            if self.players[ai].team == self.players[i].team {
                continue;
            }
            // 刷新风步（098c `Xr`，封顶 5s）。
            self.players[i].windwalk_state =
                (self.players[i].windwalk_state + Fix64::from_num(1.5)).min(Fix64::from_num(5.0));
            self.players[i].parry_ready = false;
            self.players[i].parry_cd = Fix64::from_num(0.5);
            // 互相击退（`MI`）：把攻击者推开、自己推开，均乘 `100/(100+gn)`。
            let gn = self.players[ai].gn_factor();
            let scale = Fix64::from_num(100.0 / (100.0 + gn));
            let d = self.players[i].pos - self.players[ai].pos;
            let dir = if d.length_squared() > Fix64::ZERO {
                d.normalized()
            } else {
                Vec2::new(Fix64::ONE, Fix64::ZERO)
            };
            // 互相推开（098c `MI(nr,Vr,4.5)`：把对方推离风步者；`MI(Vr,nr,2.25)`：把自己推离对方）。
            // `dir` = 对方 → 风步者；敌人应沿 `-dir` 远离，自己沿 `+dir` 远离。
            self.players[ai].push_knockback(-dir * Fix64::from_num(PARRY_KB_ATTACKER) * scale);
            self.players[i].push_knockback(dir * Fix64::from_num(PARRY_KB_SELF) * scale);
        }
        // 清瞬态（本 tick 的接触记录）。
        for p in self.players.iter_mut() {
            p.contact_by_enemy = None;
        }
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

    /// 098c 助攻判定（`AI`/`Jn` 实证）：对死者伤害**最高且 >0**、且非凶手者 = **唯一助攻者**。
    /// （098c 不是“有过伤害即助攻”，而是取 `Jn[受×12+攻]` 的 argmax；`cI>0` 即计。）
    pub fn assist_damager_of(&self, victim: u32, killer: u32) -> Option<u32> {
        let mut best: Option<(u32, Fix64)> = None;
        for (attacker, row) in self.damage_matrix.iter().enumerate() {
            if attacker == victim as usize || attacker as u32 == killer {
                continue;
            }
            let d = row[victim as usize];
            if d > Fix64::ZERO && best.map(|(_, bd)| d > bd).unwrap_or(true) {
                best = Some((attacker as u32, d));
            }
        }
        best.map(|(id, _)| id)
    }

    /// 掷冰面（098c YC/iA，冰面批）：按 `ice_mode`（0 关/1 随机 50%/2 必有）在场地内生成冰面。
    /// 冰面不被岩浆侵蚀（固定位置），站上滑行（抓地 ×0.25）。
    pub fn roll_ice(&mut self) {
        if self.ice_mode == 0 {
            self.ice.clear();
            return;
        }
        let mut rng = Rng::new(self.round_seed ^ 0x1CE_1CE);
        // 模式 1（随机）：50% 概率无冰；模式 2（必有）：总是生成。
        if self.ice_mode != 2 && rng.next_u64_below(2) == 0 {
            self.ice.clear();
            return;
        }
        // 1-3 个圆冰面（U4：circle brawl 主题；可重叠拼出冰湖）
        let arena = self.arena_radius.to_num::<f64>();
        let n = 1 + rng.next_u64_below(3) as usize;
        self.ice.clear();
        for _ in 0..n {
            let cx = (rng.next_fix().to_num::<f64>() - 0.5) * arena;
            let cy = (rng.next_fix().to_num::<f64>() - 0.5) * arena;
            let r = arena * (0.15 + 0.15 * rng.next_fix().to_num::<f64>());
            self.ice.push((Vec2::new(Fix64::from_num(cx), Fix64::from_num(cy)), Fix64::from_num(r)));
        }
    }

    /// 点位是否在任一冰面圆内。
    pub fn on_ice(&self, pos: Vec2) -> bool {
        self.ice
            .iter()
            .any(|(c, r)| (pos - *c).length_squared() <= *r * *r)
    }

    /// 设置游戏模式（098c nn，B3；每局开始前由调用方设置）。
    pub fn configure_mode(&mut self, mode: u8) {
        self.mode = mode;
    }

    /// 设置基础生命恢复（HP/s）。098c 对应主机常量 `-C9`（`In`，默认 0.5/s）。
    /// 配置收缩参数（房间设置）：
    /// `delay_secs` = 开局延迟基准（实际 ×√存活）；`total_secs` = 满员总时长基准（实际 ×√(存活/初始)）。
    pub fn configure_shrink(&mut self, delay_secs: f64, total_secs: f64) {
        self.shrink_delay_secs = Fix64::from_num(delay_secs.max(0.0));
        self.shrink_total_secs = Fix64::from_num(total_secs.max(0.01));
        // 参考半径取当前半径：设置立即生效，总时长从此刻起算。
        self.shrink_ref_radius = self.arena_radius;
        // 立即按新延迟重置计时器（否则设置要等下一轮才生效）；延迟同样 ×√存活（098c `wo*√sn`）。
        let alive = self.players.iter().filter(|p| p.alive).count().max(1) as f64;
        self.shrink_timer = self.shrink_delay_secs * Fix64::from_num(alive.sqrt());
    }

    pub fn configure_regen(&mut self, per_sec: f64) {
        self.base_regen = per_sec;
    }

    /// 配置全局倍率（房间设置）：伤害（098c 设置 2 `Gn`）、击退（设置 3 `Hn`）、
    /// 岩浆伤害（设置 1 `To`，`0` = 关闭）。三者默认均 1.0，不影响原行为。
    pub fn configure_mults(&mut self, damage: f64, knockback: f64, lava: f64) {
        self.damage_mult = Fix64::from_num(damage.max(0.0));
        self.knockback_mult = Fix64::from_num(knockback.max(0.0));
        self.lava_damage_mult = Fix64::from_num(lava.max(0.0));
    }

    /// 配置地形模式（**我们自己的设置**）：柱子 0 关/1 随机/2 必有；冰面同。
    /// 立即重铺（否则要等下一轮）；用 `round_seed` 确定性重建，两端一致。
    pub fn configure_terrain(&mut self, pillar_mode: u8, ice_mode: u8) {
        self.pillar_mode = pillar_mode;
        self.ice_mode = ice_mode;
        let mut rng = Rng::new(self.round_seed);
        self.obstacles.clear();
        _layout_obstacles(&mut self.obstacles, &mut rng, self.arena_radius, self.pillar_mode);
        self.roll_ice();
    }

    /// 每轮角色设置（B3）：化身（模式 3）与国王（模式 4）的 F 槽替换与增益。
    /// - 化身（098c `Bf`，n = 参与人数）：**独占一队**（`cn[FV]=1`，其余 `cn=0` 互为盟友）、
    ///   碰撞半径 50、`Gn ×1.5`、法术时长 ×1.2（jn）、回血 ×(1+n/2)、受击退 ÷(n/1.5)、
    ///   F→灾变 S020（`Vn[FV]` 上 `S001→S020`）。
    /// - 国王：受伤 ×0.9、岩浆 ×0.9（hn/To ×0.9，L12251）、F→虔诚 S021。
    ///
    /// 其余玩家的角色增益全部复位。
    pub fn set_roles(&mut self, avatar: Option<u32>, kings: &[u32]) {
        self.avatar = avatar;
        self.kings = kings.to_vec();
        self.f_override = vec![None; self.players.len()];
        self.round_forced = false;
        for p in self.players.iter_mut() {
            p.dmg_taken_mult = 1.0;
            p.lava_taken_mult = 1.0;
            p.dur_mult = 1.0;
            p.role_kb_mult = 1.0;
            p.role_regen_mult = 1.0;
            p.radius = Fix64::from_num(crate::balance::Balance::default().default_radius);
        }
        if let Some(av) = avatar {
            // 098c `Bf`（化身加冕）实证，n = 参与人数：
            //   `Rv[Xn[FV]]=50`（碰撞半径）、`Gn *= 1.5`、`jn *= 1.2`、
            //   `In *= (1 + n/2)`（回血）、`Hn /= (n/1.5)`（受击退）、
            //   `cn[FV]=1` 且其余人 `cn[i]=0` → **化身独占一队 vs 其余人互为盟友**、
            //   `S001 → S020`（F 槽换灾变）。
            let n = self.players.len().max(1) as f64;
            for p in self.players.iter_mut() {
                p.team = 0;
                // 098c `Bf`：`An[i]=FV` —— 把每个人的「最后伤害者」预设为化身。
                // 效果：岩浆等**环境伤害**的击杀/助攻记在化身头上（化身自己被环境杀则算自杀）。
                p.last_hit_by = avatar;
            }
            if let Some(p) = self.players.get_mut(av as usize) {
                p.team = 1;
                p.radius = Fix64::from_num(50.0);
                p.growth = 1.5;
                p.dur_mult = 1.2;
                p.role_kb_mult = 1.5 / n;
                p.role_regen_mult = 1.0 + n / 2.0;
            }
            self.f_override[av as usize] = Some(SkillId::S020);
        }
        for k in kings {
            if let Some(p) = self.players.get_mut(*k as usize) {
                p.dmg_taken_mult = 0.9;
                p.lava_taken_mult = 0.9;
            }
            if let Some(f) = self.f_override.get_mut(*k as usize) {
                *f = Some(SkillId::S021);
            }
        }
    }

    /// 取走本局统计到的击杀记录（供 meta 层结算），并清空。
    pub fn take_kills(&mut self) -> Vec<(u32, u32)> {
        std::mem::take(&mut self.kills_this_round)
    }

    /// 本局是否已结束：存活者全属同一队（098c iI）——FFA（各为一队）下等价于
    /// 「只剩 0 或 1 名存活」。试验场永不判结束。
    pub fn round_over(&self) -> bool {
        if self.sandbox {
            return false;
        }
        if self.round_forced {
            return true;
        }
        let alive: Vec<&Player> = self.players.iter().filter(|p| p.alive).collect();
        if alive.is_empty() {
            return true;
        }
        // LMS（模式 5，098c Zo）：存活者全被同一凶手杀害 → 该凶手「最后生还」即结束。
        if self.mode == 5 {
            let killers = alive.iter().map(|p| p.last_hit_by).collect::<Vec<_>>();
            if killers.iter().all(|k| k.is_some()) && killers.iter().all(|k| *k == killers[0]) {
                return true;
            }
        }
        alive.iter().all(|p| p.team == alive[0].team)
    }

    /// 本轮获胜方成员（存活者全体；全员死光=平局返回空）。仅在 `round_over()` 后有意义。
    pub fn round_winners(&self) -> Vec<u32> {
        self.players.iter().filter(|p| p.alive).map(|p| p.id).collect()
    }

    /// 重置为可开始下一小局（清空本局状态、重设玩家满血与初始位置）。
    /// 调用方需在结算完成后调用。
    pub fn reset_round(&mut self) {
        // 掷下一轮角色（化身=上轮伤害最高者；国王=每队随机）——必须在清矩阵前。
        self.roll_roles();
        self.eliminated_order.clear();
        self.kills_this_round.clear();
        for row in self.damage_matrix.iter_mut() {
            for v in row.iter_mut() {
                *v = Fix64::ZERO;
            }
        }
        self.projectiles.clear(); // 清掉上轮遗留的飞行物/延时区域
        self.arena_radius = Fix64::from_num(Balance::start_radius_for(self.players.len() as u32));
        self.shrink_ref_radius = self.arena_radius;
        self.time = Fix64::ZERO;
        // 缩圈计时重启（098c XA：回合开始即启动 EA 定时器）
        let alive = self.players.iter().filter(|p| p.alive).count().max(1) as f64;
        // 延迟同样按 √存活 缩放（098c `TimerStart(Sa, wo*SquareRoot(sn), ...)`）。
        self.shrink_timer = self.shrink_delay_secs * Fix64::from_num(alive.sqrt());
        // 冰面（冰面批）
        self.roll_ice();
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
        _layout_obstacles(&mut self.obstacles, &mut rng, self.arena_radius, self.pillar_mode);
        // 应用本轮角色（化身/国王 buff 与 F 槽替换）
        let (av, kings) = (self.pending_avatar, self.pending_kings.clone());
        self.set_roles(av, &kings);
    }

    /// 掷下一轮角色（reset_round 开头、清伤害矩阵前调用）：
    /// - 模式 3 化身 = 上一轮伤害最高者（无伤害数据时随机）。
    /// - 模式 4 国王 = 每队随机一人（确定性 rng：round_seed^round_number）。
    fn roll_roles(&mut self) {
        match self.mode {
            3 => {
                // 098c `fI`：取**累计伤害积分**最大者（`if Xr<JV[i]` → 平局保留最早者 = id 最小）。
                // 全为 0（首轮或全员未输出）→ 随机（对应 098c `Bf` 的 `FV=HR()` 随机分支）。
                let mut best: Option<(u32, Fix64)> = None;
                for (i, score) in self.avatar_score.iter().enumerate() {
                    if let Some(p) = self.players.get(i) {
                        if *score > Fix64::ZERO && best.map(|(_, b)| *score > b).unwrap_or(true) {
                            best = Some((p.id, *score));
                        }
                    }
                }
                self.pending_avatar = match best {
                    Some((id, _)) => Some(id),
                    None => {
                        let mut rng = Rng::new(self.round_seed ^ 0xA9_8C_11_22);
                        let n = self.players.len() as u64;
                        self.players.get(rng.next_u64_below(n) as usize).map(|p| p.id)
                    }
                };
            }
            4 => {
                let mut rng = Rng::new(self.round_seed ^ (self.round_number as u64).wrapping_mul(0x9E37));
                let mut teams: Vec<u8> = self.players.iter().map(|p| p.team).collect();
                teams.sort_unstable();
                teams.dedup();
                self.pending_kings.clear();
                for t in teams {
                    let cands: Vec<u32> = self.players.iter().filter(|p| p.team == t).map(|p| p.id).collect();
                    if !cands.is_empty() {
                        let pick = rng.next_u64_below(cands.len() as u64) as usize;
                        self.pending_kings.push(cands[pick]);
                    }
                }
            }
            _ => {}
        }
    }

    /// 测试钩子：手动掷角色（正常流程由 reset_round 内部调用）。
    #[cfg(test)]
    pub(crate) fn roll_roles_for_test(&mut self) {
        self.roll_roles();
    }

    /// 某玩家本轮造成的总伤害（伤害矩阵行和；化身计分用）。
    pub fn round_damage_of(&self, id: u32) -> f64 {
        self.damage_matrix
            .get(id as usize)
            .map(|row| row.iter().map(|v| v.to_num::<f64>()).sum())
            .unwrap_or(0.0)
    }
}

/// 用确定性 RNG 布柱子（圆形障碍）：在场地内圈的一个圆环上**基本均匀**分布。
///
/// “每轮不同”来自三处随机：整环随机旋转、环半径小幅波动、每根半径随机；
/// 但保持等分角距 + 小抖动，因此仍是明显的环状均匀分布。
/// 等分角距足够大，天然保证柱子之间不重叠（最小圆心距 ≫ 半径和），
/// 且环半径上限使柱子不碰玩家出生环（arena*0.6）、也不出界。
/// 每轮柱子数量随机（0~5，可为 0 = 无柱子）。
fn _layout_obstacles(out: &mut Vec<Obstacle>, rng: &mut Rng, arena_radius: Fix64, mode: u8) {
    // `mode` 是**我们自己的设置**（非 098c）：0=关闭；1=随机（每轮 0~5 根，可无）；2=每局必有（1~5 根）。
    if mode == 0 {
        return;
    }
    let count = if mode == 2 {
        1 + rng.next_u64_below(5) as usize
    } else {
        rng.next_u64_below(6) as usize
    };
    if count == 0 {
        return;
    }
    // 环半径围绕 0.4*arena 小幅波动（0.36~0.44），决定整环大小。
    let ring_r = arena_radius * (Fix64::from_num(0.36) + rng.next_fix() * Fix64::from_num(0.08));
    // 整环随机旋转 → 每轮布局都不同，但仍保持环状均匀。
    let rot = Fix64::from_num(std::f64::consts::TAU) * rng.next_fix();
    // 每根柱子相对等分角的小抖动（保持“基本均匀”）。
    let jitter = Fix64::from_num(0.15);
    // 柱子半径：与玩家（碰撞 32）相近的小幅波动 24~40（旧相对比例 1.1~1.6 × 碰撞因子 16，
    // 2026-09-05 用户确认「柱子大小应和玩家差不多、保留小幅随机波动」）。
    let min_r = Fix64::from_num(24.0);
    let max_r = Fix64::from_num(40.0);
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
        // B4 形态切换：按施法者的形态位取 A/B 定义
        let alt = world.players.get(idx as usize).map(|p| p.form_of(id)).unwrap_or(false);
        let def = DefTable::def_for(id, alt);
        let caster_level = {
            let p = &world.players[idx as usize];
            p.skill_level(id)
        };
        let stats = def.stats_at(caster_level);
        // 098c 时间精通（R00Y ei，B1）：法术持续/射程 +10%/级，火球系（S000/S003/S004）+15%/级。
        let ei = world.players[idx as usize].mastery[2] as f64;
        let ei_mult = |heavy: bool| 1.0 + if heavy { 0.15 } else { 0.1 } * ei;

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
            SkillEffect::Warlock098b { proj, speed, radius, life, kb_ji, ignite, blast, count, spread_step, on_hit } => {
                // 098b 名册弹体（M2 批次A 扩展：blast AoE/锥形连发）。
                // gx/点燃总量走 stats（growth.damage/extra 已按等级求值）；
                // 命中结算统一走 KI/FI（FI 伤害=gx×Gn×hn，KI 击退=DAMAGE_BASE×gx×JI）。
                let ppos = world.players[idx as usize].pos;
                // 时间精通：弹体寿命（=射程）缩放，火球三系权重 1.5（JASS ev=(1+.15ei) 等）。
                let mut life = life * Fix64::from_num(ei_mult(matches!(
                    id,
                    crate::skill::SkillId::S000
                        | crate::skill::SkillId::S003
                        | crate::skill::SkillId::S004
                )));
                // S016 弹跳弹：单跳射程 = max_distance（098c Range 900→1950），换算飞行时间 = range/speed。
                if proj == crate::skill::W098bProjKind::Bounce {
                    life = stats.max_distance / speed;
                }
                // S008 陨石（098c iB）：speed = 点击距离/1.35（变速度，1.35s 命中；非固定 400）。
                let mut speed = speed;
                if id == crate::skill::SkillId::S008 && !alt {
                    let d = target.map(|t| (t - ppos).length()).unwrap_or(Fix64::ZERO);
                    if d > Fix64::ZERO {
                        speed = d / Fix64::from_num(1.35);
                    }
                }
                // S009·目标形态（B4）：寿命截断到点击距离 → 在目标点碎裂（JASS GB 飞抵目标点分裂）。
                if id == crate::skill::SkillId::S009 && !alt {
                    if let Some(t) = target {
                        let dist = (t - ppos).length();
                        life = life.min(dist / speed);
                    }
                }
                // 远程精通（R00I xi，B1）：xi>0 火球获得落点爆炸——仅到点/撞柱触发，
                // 直中目标不重复爆炸（JASS Bb L5283 → sI）。半径 45×√(14+xi)（0.45×√ 尺度 ×100 换算 TODO w3q 校准）。
                let blast = if id == crate::skill::SkillId::S000 && world.players[idx as usize].mastery[1] > 0 {
                    let xi = world.players[idx as usize].mastery[1] as f64;
                    Some(Fix64::from_num(45.0 * (14.0 + xi).sqrt()))
                } else {
                    blast
                };
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
                // 回旋镖（S004，098c Ub）：出程距离 = 点击距离 clamp[300, 800×(1+0.15×时间精通)]，
                // 前向 1500/s 匀减速到出程点归零后回程。life 由固定射程(1.6×speed)改为「出程+回程」总时长兜底。
                let boomerang_out_dist = if proj == crate::skill::W098bProjKind::Boomerang {
                    let click = target.map(|t| (t - ppos).length()).unwrap_or(Fix64::from_num(800.0));
                    let ei = world.players[idx as usize].mastery[2] as f64;
                    let maxd = Fix64::from_num(800.0 * (1.0 + 0.15 * ei));
                    let od = click.clamp(Fix64::from_num(300.0), maxd);
                    // 098c `ev = -wb/yb`：前向减速到 0 的时间 = 2·od/speed（不是往返双倍）；
                    // 结算在此时刻（`oB`）。+0.05 帧容差。
                    life = od * Fix64::from_num(2.0) / speed + Fix64::from_num(0.05);
                    od
                } else {
                    life * speed
                };
                // 火球法杖（M3 2c，I00D）：持杖者 S000 火球直伤改 5.5+0.5×L、点燃总量改 3+0.5×L。
                let (gx, ignite_total) = if world.players[idx as usize].item_fx.fireball_burn
                    && id == crate::skill::SkillId::S000
                {
                    let lv = caster_level as f64;
                    (
                        Fix64::from_num(5.5 + 0.5 * lv),
                        Some(Fix64::from_num(3.0 + 0.5 * lv)),
                    )
                } else {
                    (stats.damage, ignite.map(|base| if stats.extra > Fix64::ZERO { stats.extra } else { base }))
                };
                // 陨石（S008A，098c `iB`/`oB`）：**无飞行弹体**——2D 原生化为「落点定时爆炸」。
                // 1.35s 后在点击点炸开：半径 `210×√(1+.25×远程精通)`；伤害 `(12+2L)×(1 - d/(400+40xi))`；
                // 同队（含施法者本人）免疫由 AOE 的队伍过滤保证。
                if id == crate::skill::SkillId::S008 && proj == crate::skill::W098bProjKind::Straight {
                    let ppos = world.players[idx as usize].pos;
                    let dir_v = match target {
                        Some(t) => {
                            let dd = t - ppos;
                            if dd.length() > Fix64::ZERO {
                                dd.normalized()
                            } else {
                                Vec2::new(Fix64::ONE, Fix64::ZERO)
                            }
                        }
                        None => Vec2::new(Fix64::ONE, Fix64::ZERO),
                    };
                    let landing = target.unwrap_or(ppos + dir_v * Fix64::from_num(800.0));
                    let xi = world.players[idx as usize].mastery[1] as f64;
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::DelayedBlast {
                            radius: Fix64::from_num(210.0 * (1.0 + 0.25 * xi).sqrt()),
                            damage: stats.damage,
                            kb_ji,
                            falloff_denom: Fix64::from_num(400.0 + 40.0 * xi),
                            remaining: Fix64::from_num(1.35),
                        },
                        pos: landing,
                        alive: true,
                    });
                    continue;
                }
                // 连发（count>1）：以施法方向为中心、±spread_step 对称扇出（火焰喷射锥形 5 道）。
                let half = (count.max(1) as i64 - 1) / 2;
                for k in -half..=half {
                    let ang = Fix64::from_num(spread_step) * Fix64::from_num(k);
                    let d = crate::fix::rotate_ccw(dir, ang);
                    // 回旋镖横向侧偏 ±300/s（098c Wb，左右交替）；初速 = 前向 speed + 横向 lateral（098c Ub）。
                    let (lat_val, vel_val) = if proj == crate::skill::W098bProjKind::Boomerang {
                        let side = if world.players[idx as usize].boomerang_side { 1.0 } else { -1.0 };
                        world.players[idx as usize].boomerang_side = !world.players[idx as usize].boomerang_side;
                        let lat = Fix64::from_num(side * 300.0);
                        let perp = Vec2::new(-d.y, d.x);
                        (lat, d * speed + perp * lat)
                    } else {
                        (Fix64::ZERO, d * speed)
                    };
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::W098b {
                            proj,
                            vel: vel_val,
                            speed,
                            radius,
                            remaining: life,
                            life,
                            gx,
                            kb_ji,
                            // 点燃总量随施法等级（growth.extra = 6+1.5×L；effect.ignite 仅作开关+L1 基准）。
                            ignite: ignite_total,
                            blast,
                            target: homing_target,
                            returning: false,
                            on_hit,
                            debuff_dur: stats.duration,
                            // 回旋镖横向侧偏速度（见上方 lat_val；非回旋镖恒 0）。
                            lateral: lat_val,
                            forward_dir: dir,
                            out_dist: boomerang_out_dist,
                            // B4 形态：S009·目标=到点碎裂 6 片；S009·区域=0.12s 螺旋侧弹
                            burst: if id == crate::skill::SkillId::S009 && !alt { 6 } else { 0 },
                            emit_cooldown: if id == crate::skill::SkillId::S009 && alt {
                                Fix64::from_num(0.12)
                            } else {
                                Fix64::ZERO
                            },
                            emit_angle: 0.0,
                            // 098c `xv>0` 的技能弹体撞柱**反弹**；其余（默认 `xv=-1`）被柱挡下消失。
                            // 见 `skill::pillar_bounce_for`（依据 JASS 的 `set xv[Nb]=…` 集合）。
                            pillar_bounce: crate::skill::pillar_bounce_for(id),
                            pillar_rest: crate::skill::pillar_restitution(id),
                            // 红链闪电伤害（098c `sc`：仅 S019B 用，其余 0）。
                            lightning_dmg: if on_hit == crate::skill::W098bOnHit::RedChain { stats.extra } else { Fix64::ZERO },
                        },
                        pos: ppos,
                        alive: true,
                    });
                }
            }
            SkillEffect::W098bNova { kind, radius, kb_ji } => {
                // 098c AoE nova（M2 批次D）：以自身为中心，复用 explode_at（敌 250 伤）。
                // 098c mC→FX(ii,cX) 实证 + 用户文档「包括自己！」：**nova 对施法者自身也扣血**，
                // 但 FX 有 0.5 HP 下限（不可自杀）；自伤无自击退。
                let ppos = world.players[idx as usize].pos;
                // 鲜血之剑（098c I00F/I00G）：天罚伤害平加 +1/+2（mC 实证 cX = 10 + Zr）。
                let gx = stats.damage + Fix64::from_num(world.players[idx as usize].item_fx.smite_bonus);
                // 熔岩靴激活（098c I00J-L「熔岩上天罚后激活」，M5/D8）：站熔岩 + 持靴 + CD 到期
                // → 挂 LavaShield（87.5% 抵抗 3/4/5s）；CD 25s 在 step tick 递减。
                if matches!(
                    kind,
                    crate::skill::W098bNovaKind::Smiting | crate::skill::W098bNovaKind::Devotion
                ) && ppos.length_squared() > world.arena_radius * world.arena_radius
                {
                    let p = &mut world.players[idx as usize];
                    if p.lava_boot_cd <= Fix64::ZERO
                        && p.item_fx.lava_resist_secs > 0.0
                        && !p.has_buff(BuffKind::LavaShield)
                    {
                        p.add_buff(BuffKind::LavaShield, p.item_fx.lava_resist_secs);
                        p.lava_boot_cd = Fix64::from_num(25.0);
                    }
                }
                let mut smite_hits: u32 = 0;
                match kind {
                    crate::skill::W098bNovaKind::Smiting => {
                        // S001 天罚（098c mC，普通局 F 键）：半径 250（按**半径**判定 `cO<=$FA`），
                        // 伤害随距离乘法衰减 `×(1-d/1000)`（mC `mI(...,1.-cO/$3E8)`），伤害 10+血剑。
                        smite_hits = world.explode_at(ppos, idx, radius, gx, Fix64::from_num(100.0) * gx * kb_ji, true, true, DmgFalloff::Mul(Fix64::from_num(1000.0)));
                    }
                    crate::skill::W098bNovaKind::Catastrophe => {
                        // S020 灾变（098c `qC` 实证）：伤害按阶段 `$B/$C/$E` = **11/12/14**（+血剑 Zr）；
                        // stage0/1：半径 300、伤害 `cX - d/60`；stage2：半径 400、伤害 `cX - d/40`；
                        // 受击方为异队（`cn[] != cn[ri]`，自身/同队免疫）；放完 stage+1；自身 +50 移速 4s。
                        let stage = world.players[idx as usize].catastrophe_stage % 3;
                        let (base, r, falloff_div) = match stage {
                            0 => (11.0, 300.0, 60.0),
                            1 => (12.0, 300.0, 60.0),
                            _ => (14.0, 400.0, 40.0),
                        };
                        let stage_gx = Fix64::from_num(base) + Fix64::from_num(world.players[idx as usize].item_fx.smite_bonus);
                        let r = Fix64::from_num(r);
                        let falloff_div = Fix64::from_num(falloff_div);
                        world.explode_at(ppos, idx, r, stage_gx, Fix64::from_num(100.0) * stage_gx * kb_ji, true, true, DmgFalloff::Sub(falloff_div));
                        world.players[idx as usize].catastrophe_stage = (stage + 1) % 3;
                        let p = &mut world.players[idx as usize];
                        p.add_buff(BuffKind::Speed(1.0 + 50.0 / 210.0), 4.0);
                    }
                    crate::skill::W098bNovaKind::Devotion => {
                        // S021 虔诚（098c QC，国王模式 F 技能）：伤敌同天罚（半径 250、衰减 ×(1-d/1000)）；500 内**队友**
                        //（不含自己，JASS `gX!=ii`）回血 cX/2、+60 移速 4s。FFA 无队友 → 纯伤害 nova。
                        world.explode_at(ppos, idx, radius, gx, Fix64::from_num(100.0) * gx * kb_ji, true, true, DmgFalloff::Mul(Fix64::from_num(1000.0)));
                        let caster_team = world.players[idx as usize].team;
                        let mut healed_any = false;
                        let allies: Vec<u32> = world
                            .players
                            .iter()
                            .filter(|p| p.alive && p.id != idx && p.team == caster_team)
                            .map(|p| p.id)
                            .collect();
                        for a in allies {
                            let p = &mut world.players[a as usize];
                            if (p.pos - ppos).length_squared() <= Fix64::from_num(500.0 * 500.0) {
                                p.hp = (p.hp + gx * Fix64::from_num(0.5)).min(p.max_hp);
                                p.add_buff(BuffKind::Speed(1.0 + 60.0 / 210.0), 4.0);
                                healed_any = true;
                            }
                        }
                        if healed_any {
                            world.combat_events.push(CombatEvent::HealPulse { pos: ppos, radius: Fix64::from_num(500.0) });
                        }
                    }
                }
                // 098c FX 自伤（D9 技能手感批）：nova 对施法者扣 gx 血，但 HP 不足以承受时
                // **保留 0.5**（显示为 1 滴血，不可自杀）——JASS 原文 `if Fn <= (cX+.5) then Fn = .5`。
                // 天罚/灾变/虔诚三系共用（mC handler 统一走 FX）。
                if let Some(p) = world.players.get_mut(idx as usize) {
                    if p.alive {
                        let hp = p.hp.to_num::<f64>();
                        let dmg = gx.to_num::<f64>();
                        p.hp = if hp <= dmg + 0.5 { Fix64::from_num(0.5) } else { p.hp - Fix64::from_num(dmg) };
                    }
                }
                // 098c mC 顺序：FX 自伤在前、DX 回血在后（回血按命中敌数 n 结算）——
                // 鲜血之剑回血 (Zr+1)×n；死亡面具翻倍（vi×2 → 吸血/回血 ×2）。
                if kind == crate::skill::W098bNovaKind::Smiting && smite_hits > 0 {
                    let p = &mut world.players[idx as usize];
                    if p.alive {
                        let mult = if p.item_fx.scourge_double { 2.0 } else { 1.0 };
                        let heal = p.item_fx.on_damage_heal * mult * smite_hits as f64
                            + gx.to_num::<f64>() * p.item_fx.lifesteal * mult * smite_hits as f64;
                        if heal > 0.0 {
                            p.hp = (p.hp + Fix64::from_num(heal)).min(p.max_hp);
                        }
                    }
                }
                // 守护之盾充能（098c HC/jX）：火球命中已充能 → 本次天罚获 5s 减伤/减击退窗口。
                if kind == crate::skill::W098bNovaKind::Smiting && smite_hits > 0 {
                    let p = &mut world.players[idx as usize];
                    if p.aegis_charged {
                        p.aegis_charged = false;
                        p.add_buff(BuffKind::Aegis, 5.0);
                    }
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
                        // S007 急行：+35 移速（Speed buff）+ 吸收窗口（Haste buff，吸收 50%→移速）。
                        // 吸收上限 sr = 3+2×L（098c KR），每点吸收 +15 移速（Qr）。
                        let lv = caster_level as f64;
                        if let Some(p) = world.players.get_mut(idx as usize) {
                            p.add_buff(BuffKind::Speed(speed.to_num::<f64>()), dur);
                            p.add_buff(BuffKind::Haste, dur);
                            p.s007_absorb = Fix64::from_num(3.0 + 2.0 * lv);
                            p.s007_bonus = Fix64::ZERO;
                        }
                    }
                    crate::skill::W098bUtilKind::Windwalk => {
                        // 疾风步·隐身（B 形态，098c IB）：隐身 + 较慢移速（无接触吸血，对齐 098c）。
                        if let Some(p) = world.players.get_mut(idx as usize) {
                            p.add_buff(BuffKind::Stealth, dur);
                            p.add_buff(BuffKind::Speed(speed.to_num::<f64>()), dur);
                            p.windwalk_state = Fix64::from_num(dur);
                            // 098c `IB`：进入风步 B 形态即获得一次招架就绪（`gr=true`）。
                            p.parry_ready = true;
                            p.parry_cd = Fix64::ZERO;
                        }
                    }
                    crate::skill::W098bUtilKind::Phoenix => {
                        // 冲撞·凤凰（098c WB，B4-R）：冲刺 + 凤凰态（移动指令转向+发弹）。
                        if let Some(p) = world.players.get_mut(idx as usize) {
                            let dir = match target {
                                Some(t) => { let d = t - p.pos; if d.length() > Fix64::ZERO { d.normalized() } else { Vec2::new(Fix64::ONE, Fix64::ZERO) } }
                                None => Vec2::new(Fix64::ONE, Fix64::ZERO),
                            };
                            let dist = stats.max_distance * Fix64::from_num(ei_mult(false));
                            let dur_s = (dist / speed).to_num::<f64>().max(dur);
                            p.push(dir * speed, dur_s);
                            p.kick = Some(Kick {
                                push_power: Fix64::from_num(150.0),
                                push_time: Fix64::from_num(0.3),
                                push_damage: stats.damage * Fix64::from_num(0.8),
                                remaining: Fix64::from_num(dur_s),
                                stop_on_hit: false,
                            });
                            p.phoenix_remaining = Fix64::from_num(dur);
                        }
                    }
                    crate::skill::W098bUtilKind::Charge => {
                        // 疾风步·冲锋（098c RB，B4）：移速 buff + 接触踢击窗口（撞敌背刺伤害 5.4+0.2947L，098c）。
                        // 098c RB 两形态都挂 'Agho' 隐身（war3map_pretty.j:5781-5804），A 形态（冲锋）同样隐身。
                        if let Some(p) = world.players.get_mut(idx as usize) {
                            p.add_buff(BuffKind::Stealth, dur);
                            p.add_buff(BuffKind::Speed(speed.to_num::<f64>()), dur);
                            p.windwalk_state = Fix64::from_num(dur);
                            // 098c `RB`：进入冲锋 A 形态（`fr=true`）。
                            p.charging = true;
                            p.kick = Some(Kick {
                                push_power: Fix64::from_num(150.0),
                                push_time: Fix64::from_num(0.3),
                                push_damage: stats.damage,
                                remaining: Fix64::from_num(dur),
                                stop_on_hit: false,
                            });
                        }
                    }
                    crate::skill::W098bUtilKind::Blink => {
                        if let Some(p) = world.players.get_mut(idx as usize) {
                            if let Some(t) = target {
                                let d = t - p.pos;
                                let dist = d.length();
                                let md = (stats.max_distance.max(max_distance)) * Fix64::from_num(ei_mult(false));
                                if dist > md {
                                    p.pos += d.normalized() * md;
                                } else {
                                    p.pos = t;
                                }
                            }
                        }
                    }
                    crate::skill::W098bUtilKind::Dash => {
                        // 冲撞（098b IB / 098c 对齐）：1300/s 强制位移 + 冲刺期间踢击窗口（撞人 KI 伤+击退）。
                        // 时长 = 最大距离/速度（距离截断到 max_distance）；撞敌即停见碰撞结算 stop_on_hit，
                        // 撞墙截断见 resolve_obstacles（清 control）。098c 碰撞（CA/BA）本身不对目标施加定身，
                        // 仅 伤害(mI)+魔法吸取(FX)+击退冲量，故此处不引入独立的「定身」状态。
                        if let Some(p) = world.players.get_mut(idx as usize) {
                            let dir = match target {
                                Some(t) => { let d = t - p.pos; if d.length() > Fix64::ZERO { d.normalized() } else { Vec2::new(Fix64::ONE, Fix64::ZERO) } }
                                None => Vec2::new(Fix64::ONE, Fix64::ZERO),
                            };
                            let dur_s = (stats.max_distance * Fix64::from_num(ei_mult(false)) / speed).to_num::<f64>();
                            p.push(dir * speed, dur_s);
                            p.kick = Some(Kick {
                                push_power: Fix64::from_num(100.0) * stats.damage, // 基数 100（目标 mana 放大 TODO）
                                push_time: Fix64::from_num(W098B_KB_TIME),
                                push_damage: stats.damage,
                                remaining: Fix64::from_num(dur_s),
                                stop_on_hit: true, // 098c BA：命中即停（Q=S=U=w=0）
                            });
                            // 098c `Hr`：S012 A 冲刺期间处于燃烧状态（撞队友 → Burnout）。
                            p.burning = true;
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
                // 098c 闪电（jb/Bb，D9 技能手感批）：光束**在柱子上反射**——
                // 每段沿方向找最近命中（玩家=结算 KI 伤害+击退后终止该段；柱子=镜向反射
                // 并以剩余射程递归），每段写入 lightning_visual（客户端逐段画线）。
                // 伤害 = 6+1×L 走 damage_player（KI 口径）；击退 = (100+目标魔法)×gx×JI。
                let gx = stats.damage;
                let mut origin = {
                    let p = &world.players[idx as usize];
                    p.pos + towards(p.pos, target) * p.radius
                };
                let mut dir = towards(origin, target);
                // 时间精通：闪电射程 +15%/级（JASS jb 600×(1+.15ei)）。
                let mut remaining = range * Fix64::from_num(ei_mult(true));
                let mut hit_any = false;
                for _bounce in 0..4 {
                    // 扫描本段最近命中：玩家 vs 柱子
                    let mut best_t = remaining;
                    let mut best_player: Option<u32> = None;
                    let mut best_pillar: Option<usize> = None;
                    let caster_team = world.players.get(idx as usize).map(|p| p.team);
                    for q in world.players.iter() {
                        if !q.alive || q.id == idx || Some(q.team) == caster_team {
                            continue;
                        }
                        if let Some((t, _)) = World::ray_circle_t(origin, dir, q.pos, q.radius) {
                            if t < best_t {
                                best_t = t;
                                best_player = Some(q.id);
                                best_pillar = None;
                            }
                        }
                    }
                    for (oi, o) in world.obstacles.iter().enumerate() {
                        if let Some((t, _)) = World::ray_circle_t(origin, dir, o.pos, o.radius) {
                            if t < best_t {
                                best_t = t;
                                best_player = None;
                                best_pillar = Some(oi);
                            }
                        }
                    }
                    let seg_end = origin + dir * best_t;
                    world.lightning_visual.push((origin, seg_end, Fix64::from_num(0.1)));
                    if let Some(pid) = best_player {
                        // 玩家命中：KI 伤害 + 击退，光束终止（098c：单段只结算一名玩家）
                        let vmana = world.players[pid as usize].mana;
                        let vic_hn = world.players[pid as usize].dmg_taken_mult;
                        let atk_gn = world.players.get(idx as usize).map(|a| a.gn_factor()).unwrap_or(1.0);
                        world.damage_player(pid, gx, Some(idx));
                        if let Some(p) = world.players.get_mut(pid as usize) {
                            if p.alive {
                                let kb = warlock_ki_knockback(vmana, gx, kb_ji, atk_gn, vic_hn);
                                p.push_knockback(dir * kb * world.knockback_mult);
                            }
                        }
                        hit_any = true;
                        break;
                    }
                    if let Some(oi) = best_pillar {
                        // 柱子反射（098c Bb 反射段，D9 技能手感批）：镜向方向继续，剩余射程递减
                        let normal = (origin + dir * best_t - world.obstacles[oi].pos).normalized();
                        origin = seg_end + normal;
                        dir = crate::fix::mirror_by(dir, normal);
                        remaining -= best_t;
                        hit_any = true;
                        continue;
                    }
                    break; // 无命中：光束到射程终点
                }
                let _ = hit_any;
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
                        stop_on_hit: false,
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
                        stop_on_hit: false,
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
                        stop_on_hit: false,
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
                // 连发数：098c 为**随等级成长**（S015 流射 6→12），由 growth.extra_* 经 `stats.extra` 传入；
                // 未配置（=0）时回退到效果里的静态 `count`，保持既有技能行为不变。
                let n = if stats.extra > Fix64::ZERO {
                    (stats.extra.to_num::<f64>().round() as u32).max(1)
                } else {
                    count
                };
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let base = towards(p.pos, target);
                    p.sweep = Some(crate::player::SweepState {
                        dir: base,
                        bullet_speed: stats.speed,
                        damage: stats.damage,
                        remaining: n,
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
                // 撞击迟缓（Y2）/直弹：直线弹命中→伤害 + 强推 push_time。（数值走 stats）
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
                world.lightning_visual.push((origin, end, Fix64::from_num(0.1)));
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
                // 吸引力（098c Force）随等级成长，走 stats.extra；effect 的 pull_speed 仅作 L1 兜底。
                let pull = if stats.extra > Fix64::ZERO { stats.extra } else { pull_speed };
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
                            pull_speed: pull,
                            damage_per_sec: stats.damage,
                            remaining: stats.duration,
                        },
                        pos: place,
                        alive: true,
                    });
                }
            }
            SkillEffect::StarZone { heal_per_sec, .. } => {
                // 星域/力场（Y3b/B4-Y）：点击处放持续伤敌的星。
                // 力场（S018B 形态）heal 走 stats.extra，且 heal_team=治疗范围内全部队友。
                let alt = world.players.get(idx as usize).map(|q| q.form_of(id)).unwrap_or(false);
                let place = target.unwrap_or(world.players[idx as usize].pos);
                let heal = if heal_per_sec > Fix64::ZERO { heal_per_sec } else { stats.extra };
                world.projectiles.push(Projectile {
                    owner: idx,
                    kind: ProjectileKind::Star {
                        owner: idx,
                        radius: stats.radius,
                        damage_per_sec: stats.damage,
                        heal_per_sec: heal,
                        remaining: stats.duration,
                        heal_team: id == crate::skill::SkillId::S018 && alt,
                    },
                    pos: place,
                    alive: true,
                });
            }
            SkillEffect::LineBeam { .. } => {
                // 旧的持续线占位已由 ScatterBurst 取代；此处不再落地。
            }
            SkillEffect::Mirror { count, duration, speed_bonus, fire_interval, clone_offset } => {
                // 镜像分身（C 栏）：施法者获得 +speed_bonus 移速倍率、持续 duration，
                // 期间免疫锁链与减益（「否决锁链和负面效果」）；并生成 count 个跟随施法者、
                // 周期施放火球的分身。火球伤害走 growth.damage（文档 1/1.5/2/2.5/3/3.5）。
                if let Some(p) = world.players.get_mut(idx as usize) {
                    let dur = duration.to_num::<f64>().max(0.01);
                    p.add_buff(BuffKind::Speed(1.0 + speed_bonus.to_num::<f64>()), dur);
                    p.add_buff(BuffKind::Mirror, dur);
                }
                let ppos = world.players[idx as usize].pos;
                let base = clone_offset;
                let fb_dmg = stats.damage;
                let n = count.max(1);
                for k in 0..n {
                    // 沿 X 轴左右交错分布（偶数时对称、奇数时居中也占一个）。
                    let sign = if n == 1 {
                        Fix64::ZERO
                    } else if k % 2 == 0 {
                        Fix64::ONE
                    } else {
                        -Fix64::ONE
                    };
                    let off = Vec2::new(base * sign, Fix64::ZERO);
                    world.projectiles.push(Projectile {
                        owner: idx,
                        kind: ProjectileKind::Clone {
                            owner: idx,
                            offset: off,
                            fire_timer: fire_interval, // 首发延后一个间隔
                            fire_cd: fire_interval,
                            fire_dmg: fb_dmg,
                            remaining: duration,
                        },
                        pos: ppos + off,
                        alive: true,
                    });
                }
            }
            SkillEffect::Unimplemented => {
                // 未实现技能的占位：不落地效果（仅消耗施法与冷却）
            }
        }
    }
}

/// 返回 `pos` 处半径 `radius` 内最近的存活玩家（排除 `owner`），给出 `(玩家 id, 指向玩家的方向向量)`。
fn nearest_hit(players: &[Player], pos: Vec2, owner: u32, radius: Fix64) -> Option<(u32, Vec2)> {
    let owner_team = players.iter().find(|p| p.id == owner).map(|p| p.team);
    let mut best: Option<(Fix64, u32)> = None;
    for p in players.iter() {
        if !p.alive || p.id == owner || Some(p.team) == owner_team {
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

/// 同 `nearest_hit`，但**不排除友军**（红链 S019B：链到友军/柱子时引发闪电，098c `sc`）。
fn nearest_hit_any(players: &[Player], pos: Vec2, owner: u32, radius: Fix64) -> Option<(u32, Vec2)> {
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
    let owner_team = players.iter().find(|p| p.id == owner).map(|p| p.team);
    let mut best: Option<(Fix64, u32)> = None;
    for p in players.iter() {
        if !p.alive || p.id == owner || p.id == skip || Some(p.team) == owner_team {
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

/// 098c 招架 `MI` 的冲量幅度（`MI(nr,Vr,4.5,..)` / `MI(Vr,nr,2.25,..)`）。
/// 098c 里 `MI` 把 `HX` 经 `(100+gn)*HX*Gn*hn*.03*Hn` 转成每帧速度；我们换算到自有的冲量尺度
/// （参考 kick `push_power=150` 对应伤害 5.4）→ 4.5→125、2.25→62.5（保持 2:1）。
const PARRY_KB_ATTACKER: f64 = 125.0;
const PARRY_KB_SELF: f64 = 62.5;

/// 098c `SI`/`pI` 的基础 AoE 半径（`pe = 0xA0 = 160`）；实际 = 本值 × (1 + 0.12 × 范围精通)。
const SI_RADIUS_BASE: f64 = 160.0;

/// 098c `bA`/`SI`：以 `center` 为圆心的 AoE 伤害（同 `pI`：半径 `pe×(1+.12xi)`，边缘衰减到 `qi`，乘 `Gn`）。
fn splash_damage(
    players: &mut [Player],
    center: Vec2,
    owner: u32,
    base: Fix64,
    radius: Fix64,
    qi: f64,
    damage_mult: Fix64,
) -> u32 {
    let mut hits: u32 = 0;
    let owner_team = players.get(owner as usize).map(|p| p.team);
    for p in players.iter_mut() {
        if !p.alive || p.id == owner || Some(p.team) == owner_team {
            continue;
        }
        let dist = (p.pos - center).length();
        if dist > radius {
            continue;
        }
        let c_o = if radius > Fix64::ZERO {
            1.0 - (dist / radius).to_num::<f64>()
        } else {
            1.0
        };
        let factor = qi + (1.0 - qi) * c_o;
        let dmg = base * Fix64::from_num(factor) * damage_mult;
        p.hp = (p.hp - p.soak_boost(dmg)).max(Fix64::ZERO);
        p.last_hit_by = Some(owner);
        hits += 1;
    }
    hits
}

/// 成对解析玩家圆球碰撞：把重叠的两球沿中心连线推开，避免相互穿透。
///
/// **无“挤压伤害”**（098c 已实证：重叠只做分离 + 动量交换，不扣血，见 `cc79e8f`）。
/// 本函数只处理**碰撞/接触触发**的交互（098c `hv[unit]` 条件）：踢击命中（`kick`）、
/// 破隐一击（`stealth_extra`）、同队 Burnout、风步招架（`contact_by_enemy`）。
/// 位置修正按半径反比分配（更小的球退得更多），保证确定性与顺序无关地一致。
fn resolve_player_collisions(players: &mut [Player], _dt: Fix64, damage_mult: Fix64, knockback_mult: Fix64) -> Vec<CombatEvent> {
    let mut events: Vec<CombatEvent> = Vec::new();
    let n = players.len();
    for i in 0..n {
        for j in (i + 1)..n {
            if !players[i].alive || !players[j].alive {
                continue;
            }
            let (a, b) = (players[i].clone(), players[j].clone());
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
            // 098c 动量交换（D9 批次2）：等质量（dv=0.5 → 系数 (0.5+0.5+2)/4=1）——
            // 沿碰撞法线交换速度分量（完全弹性）。各自保留切向分量。
            let va_n = players[i].cur_vel.dot(dir);
            let vb_n = players[j].cur_vel.dot(dir);
            if va_n > Fix64::ZERO || vb_n < Fix64::ZERO {
                // 相向或追赶：法向分量交换（a 的法向速度给 b，b 的给 a）
                let va_t = players[i].cur_vel - dir * va_n;
                let vb_t = players[j].cur_vel - dir * vb_n;
                players[i].cur_vel = va_t + dir * vb_n;
                players[j].cur_vel = vb_t + dir * va_n;
            }

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

            // （098c 没有“挤压伤害”；玩家重叠只做分离+动量交换，不扣血。）

            // 踢击/撞击效果（冲锋·潜行踢）：携带 kick 的一方撞到敌人，造成技能伤害+击退，并消耗 kick。
            let dir_b_from_a = if dist == Fix64::ZERO {
                Vec2::new(Fix64::ONE, Fix64::ZERO)
            } else {
                delta.normalized()
            };
            // 踢击只对异队生效（098c 冲撞/潜行踢命中「敌人」；同队穿过不触发，B2）。
            if players[i].team != players[j].team {
                // 098c `CA` 是近战/接触命中处理（`hv[unit]=ni`）；我们无自动攻击 → 以「接触」触发招架。
                players[i].contact_by_enemy = Some(players[j].id);
                players[j].contact_by_enemy = Some(players[i].id);
                if let Some(kick) = players[i].kick.take() {
                    // 098c `CA`：命中伤害经 `mI→hI`，乘 `Gn`（= `damage_mult`）。
                    let dmg = kick.push_damage * damage_mult;
                    players[j].hp = (players[j].hp - players[j].soak_boost(dmg)).max(Fix64::ZERO);
                    players[j].last_hit_by = Some(players[i].id);
                    // 击退：伤害 × 受击者精通减免 × Hn（`knockback_mult`）。
                    let imp = kick.push_power
                        * Fix64::from_num(1.0 - players[j].mastery_kb_reduction())
                        * knockback_mult;
                    players[j].push(dir_b_from_a * imp, kick.push_time.to_num::<f64>());
                    // 098c `CA` 按**攻方状态**的额外效果（见 SKILL_STATE_AUDIT §3）：
                    // - `fr`（S010A 冲锋）→ `bA`：范围精通>0 时以攻方为圆心 AoE（×Gn）。
                    // - `Hr`（S012A 燃烧）→ `FX` 自伤（不经 Gn）+ 若范围精通>0 且非冲锋则 AoE（×Gn）。
                    let (apos, aid, charging, burning) =
                        (players[i].pos, players[i].id, players[i].charging, players[i].burning);
                    let xi = players[i].mastery[1] as f64;
                    let radius = Fix64::from_num(SI_RADIUS_BASE * (1.0 + 0.12 * xi));
                    let qi = 0.15 * xi;
                    if charging {
                        if xi > 0.0 && splash_damage(players, apos, aid, kick.push_damage, radius, qi, damage_mult) > 0 {
                            events.push(CombatEvent::Splash { pos: apos, radius });
                        }
                    } else if burning {
                        players[i].hp = (players[i].hp - kick.push_damage).max(Fix64::ZERO);
                        if xi > 0.0 && splash_damage(players, apos, aid, kick.push_damage, radius, qi, damage_mult) > 0 {
                            events.push(CombatEvent::Splash { pos: apos, radius });
                        }
                    }
                    players[i].remove_buff(BuffKind::Stealth);
                    if kick.stop_on_hit {
                        players[i].control = None;
                        players[i].cur_vel = Vec2::ZERO;
                        players[i].burning = false;
                    }
                }
                if let Some(kick) = players[j].kick.take() {
                    let dmg = kick.push_damage * damage_mult;
                    players[i].hp = (players[i].hp - players[i].soak_boost(dmg)).max(Fix64::ZERO);
                    players[i].last_hit_by = Some(players[j].id);
                    let imp = kick.push_power
                        * Fix64::from_num(1.0 - players[i].mastery_kb_reduction())
                        * knockback_mult;
                    players[i].push(-dir_b_from_a * imp, kick.push_time.to_num::<f64>());
                    let (apos, aid, charging, burning) =
                        (players[j].pos, players[j].id, players[j].charging, players[j].burning);
                    let xi = players[j].mastery[1] as f64;
                    let radius = Fix64::from_num(SI_RADIUS_BASE * (1.0 + 0.12 * xi));
                    let qi = 0.15 * xi;
                    if charging {
                        if xi > 0.0 && splash_damage(players, apos, aid, kick.push_damage, radius, qi, damage_mult) > 0 {
                            events.push(CombatEvent::Splash { pos: apos, radius });
                        }
                    } else if burning {
                        players[j].hp = (players[j].hp - kick.push_damage).max(Fix64::ZERO);
                        if xi > 0.0 && splash_damage(players, apos, aid, kick.push_damage, radius, qi, damage_mult) > 0 {
                            events.push(CombatEvent::Splash { pos: apos, radius });
                        }
                    }
                    players[j].remove_buff(BuffKind::Stealth);
                    if kick.stop_on_hit {
                        players[j].control = None;
                        players[j].cur_vel = Vec2::ZERO;
                        players[j].burning = false;
                    }
                }
            } else {
                // 同队接触：燃烧冲刺（S012 A，098c `Hr`）撞队友 → 「Burn out」（098c `lb`）：
                // 对队友造成 `0.45×√(7+范围精通)` 伤害（`xi`=范围精通）；冲刺者熄火（清 Hr、停位移）。
                for (burner, ally) in [(i, j), (j, i)] {
                    if players[burner].burning && players[burner].alive && players[ally].alive {
                        let xi = players[ally].mastery[1] as f64;
                        let dmg = Fix64::from_num(0.45 * (7.0 + xi).sqrt());
                        players[ally].hp = (players[ally].hp - players[ally].soak_boost(dmg)).max(Fix64::ZERO);
                        players[ally].last_hit_by = Some(players[burner].id);
                        players[burner].burning = false;
                        players[burner].control = None;
                        players[burner].cur_vel = Vec2::ZERO;
                        events.push(CombatEvent::Burnout { owner: players[burner].id, pos: players[burner].pos });
                        break;
                    }
                }
            }
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Fix64, b: f64, tol: f64) -> bool {
        (a.to_num::<f64>() - b).abs() < tol
    }

    /// S012B 凤凰（098c `UB` 普通分支）：**非**疾风步状态时，移动指令只把冲刺转向
    /// （`bO(gX, 18×0.5^(v/20))`），不发射凤凰弹。
    #[test]
    fn s012b_phoenix_redirect_without_windwalk_steers_only() {
        let mut world = World::new(2, 1010);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].team = 0;
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[0].forms[SkillId::S012.as_u32() as usize] = true;
        world.players[1].pos = Vec2::new(d60(20.0), d60(20.0));
        world.players[1].move_target = None;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S012, Some(Vec2::new(d60(8.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let before = world.players[0].pos;
        world.step(vec![
            PlayerInput { set_target: Some(Vec2::new(Fix64::ZERO, d60(8.0))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        for _ in 0..10 {
            world.step(vec![PlayerInput::default(), PlayerInput::default()], dt);
        }
        assert!(
            !world.projectiles.iter().any(|p| matches!(p.kind, ProjectileKind::W098b { .. })),
            "非疾风步状态不应发弹"
        );
        // 冲刺应转向 +y（原为 +x）。
        assert!(world.players[0].pos.y > before.y, "转向后应向 +y 移动");
    }

    // ===== 测试尺度换算辅助（098b 过渡，PORT_098B_DECISIONS.md D4） =====
    // 世界已切 war3 尺度而测试直觉仍是旧（Unity demo 微缩）尺度：空间字面量经此换算，
    // 测试语义（机制验证）不变。legacy_scale_def 删除（名册替换完成）时一并清理。
    /// 旧尺度距离/坐标 → war3 尺度（×60 = 1200/20）。
    fn d60(x: f64) -> Fix64 {
        Fix64::from_num(x * 60.0)
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
            if !world.lightning_visual.is_empty() {
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

    /// 客户端协议的真实形态：**施法与移动都是持续电平量**（每帧同时下发 `set_target` + `cast`）。
    /// 既有回归测试在施法后就不再下发目标，掩盖了这个组合；这里按真实协议连续下发。
    ///
    /// 契约：移动中按技能 → 立刻开始施法；并且**施法中不再接受重发的旧移动目标**
    /// （客户端只在观察到 busy 那一帧才清自己的 `player_target`，网络下有 RTT，
    ///   所以 host 侧必须自己保证「施法锁住走位」）。
    #[test]
    fn cast_wins_over_continuously_resent_move_target() {
        let dt = Fix64::from_num(1.0 / 60.0);
        let far = Vec2::new(d60(100.0), Fix64::ZERO);
        let mut w = World::new(1, 33);
        w.obstacles.clear();
        w.players[0].pos = Vec2::ZERO;
        for _ in 0..10 {
            w.step(vec![PlayerInput { set_target: Some(far), ..Default::default() }], dt);
        }
        assert!(w.players[0].pos.x > Fix64::ZERO, "应先开始移动");

        // 移动中按技能：每帧**继续**下发同一个移动目标 + 施法请求。
        let mut started = false;
        for _ in 0..20 {
            w.step(
                vec![PlayerInput {
                    set_target: Some(far),
                    cast: Some((SkillId::Blink, Some(far))),
                    ..Default::default()
                }],
                dt,
            );
            if w.players[0].caster.is_busy() {
                started = true;
                break;
            }
        }
        assert!(started, "移动中施法应当直接开始（不应要求先停下）");
        assert!(w.players[0].move_target.is_none(), "施法成功应当清掉移动目标");

        // 后续帧仍然持续下发旧移动目标：**施法进行中**角色必须停住。
        // （施法结束后重新接受移动是正确的，所以只在 busy 期间断言。）
        let mut xs = Vec::new();
        for _ in 0..12 {
            if !w.players[0].caster.is_busy() {
                break;
            }
            w.step(
                vec![PlayerInput {
                    set_target: Some(far),
                    cast: Some((SkillId::Blink, Some(far))),
                    ..Default::default()
                }],
                dt,
            );
            xs.push(w.players[0].pos.x.to_num::<f64>());
        }
        assert!(xs.len() >= 2, "施法应至少持续几帧");
        let total = (xs[xs.len() - 1] - xs[0]).abs();
        assert!(
            total < 6.0,
            "施法进行中不应持续走向旧目标（{} 帧共 {total:.2}）",
            xs.len()
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
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        // war3 尺度下两半径 16：敌人放在接触距离外一点，施法者移动过去撞（踢击语义）。
        //（旧布阵 1.5 距离在 M0 尺度迁移后实为初始重叠，靠挤压伤 0.03 侥幸过测，
        // 已被全局回血 Nn=0.05/s 抵消——顺手把测试修回真意图。）
        world.players[1].pos = Vec2::new(Fix64::from_num(60.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.players[0].move_target = Some(Vec2::new(Fix64::from_num(200.0), Fix64::ZERO));
        let hp0 = world.players[0].hp;
        let hp1 = world.players[1].hp;
        let input = vec![
            PlayerInput {
                cast: Some((SkillId::StealthPush, None)),
                set_target: Some(Vec2::new(Fix64::from_num(200.0), Fix64::ZERO)),
                ..Default::default()
            },
            PlayerInput::default(),
        ];
        // StealthPush windup 0.25s + 移动接触（60 距离 @210/s ≈ 0.29s），跑 1s 足够
        for _ in 0..60 {
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

    /// S007 急行：吸收 50% 伤害转为移速（098c KR/Qr），吸收上限 sr=3+2L。
    #[test]
    fn s007_haste_absorbs_damage_into_speed() {
        let mut world = World::new(2, 43);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S007, None)), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..3 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].has_buff(BuffKind::Haste), "S007 吸收窗口应激活");
        assert!(world.players[0].s007_absorb > Fix64::ZERO, "初始应有可吸收量（3+2L）");
        assert_eq!(world.players[0].s007_bonus, Fix64::ZERO, "初始吸收转化为 0");
        // 出界受伤 → 吸收一部分，转化为移速加成。
        world.players[0].pos = Vec2::new(d60(100.0), Fix64::ZERO);
        for _ in 0..30 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].s007_bonus > Fix64::ZERO, "受伤后应累积移速加成（Qr）");
        assert!(world.players[0].s007_absorb < Fix64::from_num(3.0 + 2.0), "吸收量应减少（sr 上限 3+2L）");
    }

    /// S019B 红链：命中友军 → 引发闪电伤害（098c `sc`）。
    #[test]
    fn s019b_redchain_lightning_on_ally() {
        let mut world = World::new(2, 47);
        let dt = Fix64::from_num(1.0 / 60.0);
        // p1 与 p0 同队（友军）+ S019 切 B 形态（红链）。
        world.players[1].team = world.players[0].team;
        world.players[0].forms[SkillId::S019.as_u32() as usize] = true;
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(3.0), Fix64::ZERO);
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S019, Some(Vec2::new(d60(3.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..90 {
            world.step(none.clone(), dt);
            if world.players[1].hp < hp1 {
                break;
            }
        }
        assert!(world.players[1].hp < hp1, "红链命中友军应引发闪电伤害（098c sc）");
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
        world.players[1].pos = Vec2::new(d60(5.0), d60(5.0)); // 不在直线上（且在 640 场地内）
        let hp1 = world.players[1].hp;
        let input = vec![
            PlayerInput { cast: Some((SkillId::D2Fireball, Some(Vec2::new(d60(6.0), Fix64::ZERO)))), ..Default::default() },
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
        world.base_regen = 0.0; // 本测试只验证其他机制：屏蔽基础回血漂移
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
                heal_team: false,
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

    // 属性系统测试（attributes_derive_and_apply_deterministically / attributes_reduce_damage_and_push）
    // 已随属性购买系统删除（2026-09-12，098c 无此机制）。

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
    /// 098c 弧线回旋镖（Ub，D9 技能手感批）：命中后不消失——反向飞回施法者再消失。
    #[test]
    fn s004_boomerang_hits_then_returns_to_caster() {
        let mut world = World::new(2, 952);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(3.0), Fix64::ZERO); // 180 处的敌人（< 800 出程）
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S004, Some(Vec2::new(d60(3.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        // 098c 机制：回旋镖飞行全程不结算，只在回程到位时对命中半径 qI（210）内敌人 AOE（伤害随距离衰减）。
        let mut boom_seen = false;
        let mut hit_while_flying = false;
        let mut hit_on_return = false;
        let mut settled = false;
        for _ in 0..180 {
            world.step(none.clone(), dt);
            let boom_now = world
                .projectiles
                .iter()
                .any(|pr| matches!(pr.kind, ProjectileKind::W098b { proj: crate::skill::W098bProjKind::Boomerang, .. }));
            if boom_now {
                boom_seen = true;
            }
            if world.players[1].hp < hp1 {
                if boom_now {
                    hit_while_flying = true; // 不应发生：飞行途中不结算
                } else {
                    hit_on_return = true;
                }
            }
            if hit_on_return && !boom_now {
                settled = true;
                break;
            }
        }
        assert!(boom_seen, "回旋镖应至少存在若干帧（飞行中）");
        assert!(!hit_while_flying, "098c：回旋镖飞行途中不应结算伤害");
        assert!(hit_on_return, "回程到位应对 210 半径内敌人造成伤害");
        assert!(settled, "结算后回旋镖消失");
    }

    /// 弹体互撞（098c `Av`→ 简化互毁）：两队火球对飞应互毁、不伤及玩家。
    #[test]
    fn opposing_missiles_collide_and_destroy_each_other() {
        let mut world = World::new(2, 1201);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        let p0 = Vec2::new(d60(-3.0), Fix64::ZERO);
        let p1 = Vec2::new(d60(3.0), Fix64::ZERO);
        world.players[0].pos = p0;
        world.players[1].pos = p1;
        world.players[0].move_target = None;
        world.players[1].move_target = None;
        let hp0 = world.players[0].hp;
        let hp1 = world.players[1].hp;
        world.step(
            vec![
                PlayerInput { cast: Some((SkillId::S000, Some(p1))), ..Default::default() },
                PlayerInput { cast: Some((SkillId::S000, Some(p0))), ..Default::default() },
            ],
            dt,
        );
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        assert_eq!(world.players[0].hp, hp0, "对撞火球不应伤及施法者 0");
        assert_eq!(world.players[1].hp, hp1, "对撞火球不应伤及施法者 1");
        assert!(
            !world.projectiles.iter().any(|p| matches!(p.kind, ProjectileKind::W098b { .. })),
            "对撞后应无火球残留"
        );
    }

    /// S008 陨石（098c `iB`/`oB`）：**无飞行弹体**，落点 1.35s 后定时爆炸；自身/同队免疫。
    #[test]
    fn s008_meteor_is_delayed_blast_and_self_immune() {
        let mut world = World::new(2, 966);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 敌人站在落点附近（±18 内，位于衰减范围内，且在场内避免熔浆伤害）。
        let landing = Vec2::new(d60(6.0), Fix64::ZERO);
        world.players[1].pos = landing + Vec2::new(d60(0.3), Fix64::ZERO);
        world.players[1].move_target = None;
        let hp1_before = world.players[1].hp;
        let hp0_before = world.players[0].hp;
        world.step(
            vec![
                PlayerInput { cast: Some((SkillId::S008, Some(landing))), ..Default::default() },
                PlayerInput::default(),
            ],
            dt,
        );
        // 落地前：场上应是 DelayedBlast（落点），而不是任何飞行弹体。
        assert!(
            world.projectiles.iter().any(|p| matches!(p.kind, ProjectileKind::DelayedBlast { .. })),
            "陨石应在落点生成 DelayedBlast"
        );
        assert!(
            !world.projectiles.iter().any(|p| matches!(p.kind, ProjectileKind::W098b { .. })),
            "陨石不应有飞行弹体"
        );
        // 落地前不结算。
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        assert_eq!(world.players[1].hp, hp1_before, "1.0s 时还没到 1.35s，不应结算");
        // 到点：敌人受中心附近伤害（≈14）、施法者自身不受伤害、不卐 Scorched。
        for _ in 0..40 {
            world.step(none.clone(), dt);
        }
        assert!(
            world.players[1].hp < hp1_before,
            "到点应对范围内敌人造成伤害（{} -> {}）",
            hp1_before.to_num::<f64>(),
            world.players[1].hp.to_num::<f64>()
        );
        assert_eq!(world.players[0].hp, hp0_before, "陨石不应伤及施法者本人（同队免疫）");
        assert!(!world.players[1].has_buff(BuffKind::Scorched), "098c 陨石不施加灼烧");
    }

    /// M3 2c：死亡面具吸血——攻方按伤害 24% 回血；火球法杖改写火球直伤 5.5+0.5L。
    #[test]
    fn item_combat_hooks_lifesteal_and_firestaff() {
        // 吸血：玩家1 持面具，闪电打玩家0 → 回血 24%×10=2.4
        let mut world = World::new(2, 969);
        world.base_regen = 0.0; // 本测试只验证其他机制：屏蔽基础回血漂移
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::new(d60(5.0), Fix64::ZERO);
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::ZERO;
        world.players[1].move_target = None;
        world.players[1].hp = Fix64::from_num(50.0);
        world.players[1].set_items(&[crate::item::ItemId::FireMask]);
        world.step(vec![
            PlayerInput::default(),
            PlayerInput { cast: Some((SkillId::TestLightning, Some(Vec2::new(d60(5.0), Fix64::ZERO)))), ..Default::default() },
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..20 {
            if world.players[1].hp > Fix64::from_num(50.0) { break; }
            world.step(none.clone(), dt);
        }
        let healed = (world.players[1].hp - Fix64::from_num(50.0)).to_num::<f64>();
        assert!((healed - 2.4).abs() < 0.3, "面具应吸血 24%×10≈2.4，实际 {healed}");
        assert!(world.players[0].hp < world.players[0].max_hp, "目标应受伤");
    }

    /// 098c 天罚改件（mC 实证）：鲜血之剑 +1 伤/命中每敌回 2 血；
    /// 守护之盾 = 火球充能 → 天罚后 5s 减伤窗口（不再是无条件常驻减免）。
    #[test]
    fn smite_items_sword_and_shield() {
        let mut world = World::new(3, 970);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        // 施法者持剑（天罚伤害 10+1=11）+ 持盾
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[0].set_items(&[crate::item::ItemId::BloodSword1, crate::item::ItemId::GuardianShield1]);
        // p1 在 -120（火球不会碰它），p2 在 +120（充当充能靶；被击退飞出天罚半径）
        world.players[1].pos = Vec2::new(d60(-2.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.players[2].pos = Vec2::new(d60(2.0), Fix64::ZERO);
        world.players[2].move_target = None;
        let (hp0, hp1) = (world.players[0].hp, world.players[1].hp);
        // 先用火球命中 p2 充能（098c：ib 命中 → Ha=true；770 初速击退把 p2 推出 250 半径）
        world.players[0].caster = crate::skill::Caster::new();
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S000, Some(Vec2::new(d60(2.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(); 3];
        for _ in 0..40 {
            world.step(none.clone(), dt); // 火球飞抵目标 → 充能
        }
        assert!(world.players[0].aegis_charged, "火球命中后守护盾应充能");
        // 天罚：p1（未被火球碰）吃 10+1=11；施法者自伤后按命中敌数回血 (Zr+1)×n。
        // 火球直伤+灼烧 tick 已让 Gn 成长若干次（D9），重置以便断言裸伤害。
        world.players[0].caster = crate::skill::Caster::new();
        world.players[0].growth = 1.0;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S001, None)), ..Default::default() },
            PlayerInput::default(),
            PlayerInput::default(),
        ], dt);
        for f in 0..60 {
            world.step(none.clone(), dt); // windup 0.7s
            if f == 44 {
                // 天罚刚落地即采集：击退会把 p1 推出场外进岩浆（098c 正确行为），不计入
                // 距离衰减（098c `mI(...,1-d/1000)`）：p1 在 120 处 → 11×(1-120/1000)=9.68。
                let d1 = (hp1 - world.players[1].hp).to_num::<f64>();
                assert!((d1 - 9.68).abs() < 0.5, "目标应吃 11×(1-120/1000)=9.68 天罚，实际 {d1}");
            }
        }
        assert!(!world.players[0].aegis_charged, "天罚释放应消耗充能");
        // 施法者：满血 100 - 自伤 11 + 回血 2×n（n=1 或 2，取决于 p2 是否已飞出半径）
        let hp0_now = world.players[0].hp.to_num::<f64>();
        assert!((90.5..=93.5).contains(&hp0_now), "自伤 11 后应回血 2×n（91~93），实际 {hp0_now}");
        // 天罚后获得 5s 减伤窗口（Aegis buff）
        assert!(world.players[0].has_buff(BuffKind::Aegis), "天罚后应挂 5s 减伤窗口");
        let _ = hp0;
    }

    /// 098c 柱子可摧毁（D9 批次3）：火球命中扣 HP（nx=40），归零移除；每轮重生成。
    #[test]
    fn pillar_takes_damage_and_breaks() {
        let mut world = World::new(2, 975);
        world.obstacles.clear();
        world.obstacles.push(Obstacle::new(Vec2::new(d60(3.0), Fix64::ZERO), 24.0));
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(-8.0), Fix64::ZERO);
        world.players[1].move_target = None;
        // 对柱子连发火球（每发直伤 gx≈7 → 6 发摧毁 40HP）
        let mut saw_break = false;
        for _ in 0..10 {
            if world.obstacles.is_empty() {
                break;
            }
            world.players[0].caster = crate::skill::Caster::new(); // 清 CD
            world.step(vec![
                PlayerInput { cast: Some((SkillId::S000, Some(Vec2::new(d60(3.0), Fix64::ZERO)))), ..Default::default() },
                PlayerInput::default(),
            ], dt);
            let none = vec![PlayerInput::default(), PlayerInput::default()];
            for _ in 0..40 {
                world.step(none.clone(), dt);
                if world.combat_events.iter().any(|e| matches!(e, CombatEvent::PillarBreak { .. })) {
                    saw_break = true;
                }
            }
        }
        assert!(world.obstacles.is_empty(), "柱子 HP 40 应被火球连发摧毁");
        assert!(saw_break, "柱子被摧毁应产生 PillarBreak 表现事件");
    }

    /// 术士之战「火球击中柱子能够反弹」：火球（S000）撞柱**镜向反弹**继续飞行，
    /// 同时仍按 098c 对柱子造成伤害（nx=40 可摧毁）；其它直射弹仍被柱子挡下消失。
    #[test]
    fn fireball_bounces_off_pillar() {
        let mut world = World::new(2, 978);
        world.obstacles.clear();
        world.obstacles.push(Obstacle::new(Vec2::new(d60(3.0), Fix64::ZERO), 24.0));
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(-8.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.players[0].caster = crate::skill::Caster::new(); // 清 CD
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S000, Some(Vec2::new(d60(3.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        let mut bounced = false;
        for _ in 0..40 {
            world.step(none.clone(), dt);
            for pr in world.projectiles.iter() {
                if !pr.alive {
                    continue;
                }
                if let ProjectileKind::W098b { vel, pillar_bounce: true, .. } = pr.kind {
                    if vel.x < Fix64::ZERO {
                        bounced = true;
                    }
                }
            }
        }
        assert!(bounced, "火球撞柱应镜向反弹（vel.x 由正变负），而非被挡下消失");
        assert!(!world.obstacles.is_empty(), "单次反弹不应摧毁 40HP 柱子（火球直伤约 7）");
        assert!(world.obstacles[0].hp < 40, "反弹的同时应仍对柱子造成伤害，实际 HP={}", world.obstacles[0].hp);
    }

    /// 098c 动量交换（D9 批次2）：高速玩家撞低速玩家 → 速度法向分量交换。
    #[test]
    fn collision_exchanges_momentum() {
        let mut world = World::new(2, 974);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(Fix64::from_num(80.0), Fix64::ZERO); // 接触距离 60 附近
        world.players[1].move_target = None;
        // 玩家1 冲向玩家0（高法向速度），玩家0 静止
        world.players[1].cur_vel = Vec2::new(Fix64::from_num(-400.0), Fix64::ZERO);
        for _ in 0..30 {
            let v0 = world.players[0].cur_vel.length().to_num::<f64>();
            world.step(vec![PlayerInput::default(), PlayerInput::default()], dt);
            if v0 == 0.0 && world.players[0].cur_vel.length() > Fix64::ZERO {
                break; // 动量已交换
            }
        }
        // 交换后玩家0 应获得 -x 方向速度（被撞飞），玩家1 减速
        let v0 = world.players[0].cur_vel.length().to_num::<f64>();
        assert!(v0 > 100.0, "动量交换后玩家0 应获得速度，实际 {v0}");
        assert!(world.players[0].pos.x < d60(5.0), "玩家0 应被撞向 -x");
    }

    /// 098c 魔法张力（D9 批次1）：挨打回魔（受多少伤加多少魔）+ 击退随魔法放大。
    #[test]
    fn mana_tension_gain_and_knockback_scaling() {
        let mut world = World::new(2, 973);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(5.0), Fix64::ZERO);
        world.players[1].move_target = None;
        assert_eq!(world.players[1].mana, 0.0, "出生魔法 0");
        // 火球命中玩家1：直伤 7 → mana += 7；击退初速 = (100+7)×7×1.1 ≈ 824/s
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S000, Some(Vec2::new(d60(5.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[1].mana >= 7.0, "挨打应回魔（直伤 7），实际 {}", world.players[1].mana);
        assert!(world.players[0].growth > 1.0, "命中敌人应成长 Gn×1.1，实际 {}", world.players[0].growth);
        // 施法者成长后同技能伤害放大：第二发直伤 = 7×1.1
        let hp1 = world.players[1].hp;
        world.players[0].caster = crate::skill::Caster::new(); // 清 CD
        world.players[1].pos = Vec2::new(d60(5.0), Fix64::ZERO); // 击退后拉回
        world.players[1].control = None;
        world.players[1].move_target = None;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S000, Some(Vec2::new(d60(5.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        let total = hp1 - world.players[1].hp;
        // 第二发直伤在 Gn 成长后约为 7×1.1≈7.7（点燃等 DoT 不再随 Gn 指数上涨，见
        // `dot_damage_does_not_grow_gn_or_oneshot`）。这里只断言“第二发确实打中了且量级合理”。
        assert!(total > 7.0, "成长后第二发应仍有 ~7.7 直伤，合计 >7，实际 {total}");
    }

    /// M5 熔岩（圈外=熔岩统一，D8）：熔岩靴激活式——熔岩上用天罚 → 87.5% 窗口（1 档 3s）
    /// + CD 25s；窗口外吃全额 9/s + 惩罚。
    /// M5 熔岩靴激活式（098c I00J-L）：熔岩上用天罚 → LavaShield 87.5%×3s（1 档）+ CD25s；
    /// 窗口内熔岩伤 9×12.5%=1.125/s，窗口外恢复全额（含天罚自伤场景的复合口径，用区间断言）。
    #[test]
    fn lava_boots_resist_out_of_bounds_damage() {
        let mut world = World::new(1, 971);
        world.base_regen = 0.0; // 本测试只验证其他机制：屏蔽基础回血漂移
        world.players[0].pos = Vec2::new(d60(20.5), d60(20.5)); // 场外（熔岩上）
        world.players[0].set_items(&[crate::item::ItemId::LavaBoots1]);
        world.players[0].hp = Fix64::from_num(50.0);
        let dt = Fix64::from_num(1.0 / 60.0);
        // 站熔岩上放天罚（windup 0.7s）→ 激活 LavaShield 3s
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S001, None)), ..Default::default() },
        ], dt);
        let none = vec![PlayerInput::default()];
        for _ in 0..60 {
            if world.players[0].has_buff(BuffKind::LavaShield) {
                break;
            }
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].has_buff(BuffKind::LavaShield), "熔岩上天罚应激活抵抗窗口");
        assert!(world.players[0].lava_boot_cd > Fix64::from_num(24.0), "激活后进入 ~25s CD");
        // 窗口期 1s（含 windup 段）：总额 = 0.7s 天罚自伤段(10) + 熔岩伤 1.125 + ...
        // 复合口径复杂，用区间断言：窗口内 1s 净损应在 8~13 之间（自伤10+熔岩1.125-回血0，叠加期部分全额）
        let hp_start = world.players[0].hp.to_num::<f64>();
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        let lost = hp_start - world.players[0].hp.to_num::<f64>();
        assert!(lost > 0.5 && lost < 3.0, "窗口内熔岩伤应减免至 1.125/s，1s 损 {lost}");
        // 无靴对照：单独 1s 全额 9
        let mut world2 = World::new(1, 971);
        world2.base_regen = 0.0; // 屏蔽基础回血，单独量度熔岩伤
        world2.players[0].pos = Vec2::new(d60(20.5), d60(20.5));
        world2.players[0].hp = Fix64::from_num(50.0);
        for _ in 0..60 {
            world2.step(none.clone(), dt);
        }
        let lost2 = 50.0 - world2.players[0].hp.to_num::<f64>();
        assert!((lost2 - 9.0).abs() < 0.1, "无靴 1s 应掉 9，实际 {lost2}");
    }

    /// M3 物品派生数值：靴加速度、斗篷回血、头盔 kb 取 max、坠饰加生命上限。
    #[test]
    fn item_effects_apply_to_player_stats() {
        let mut world = World::new(1, 968);
        world.base_regen = 0.0; // 本测试只验证其他机制：屏蔽基础回血漂移
        let p = &mut world.players[0];
        p.set_items(&[
            crate::item::ItemId::Boots3,   // +40 移速
            crate::item::ItemId::Cloak3,   // +0.4 回复（098c 无移速惩罚）
            crate::item::ItemId::Helm3,    // -32% kb +20 生命 -15 移速
            crate::item::ItemId::Amulet3,  // +30 生命 +0.1 回复
        ]);
        // 移速平加：+40（靴）-15（头盔）= +25（098c 斗篷无移速惩罚）
        let expected_flat = 40.0 - 15.0;
        let expected_speed = crate::player::BASE_SPEED + expected_flat;
        let got = p.base_speed_for_test().to_num::<f64>();
        assert!((got - expected_speed).abs() < 0.01, "移速应 {expected_speed}，实际 {got}");
        // 回复：098c 基础 0.5/s + 0.4 斗篷 + 0.1 坠饰 = 1.0/s
        let regen = crate::balance::Balance::default().hp_regen + p.item_fx.regen_add - p.item_fx.regen_penalty;
        assert!((regen - 1.0).abs() < 1e-6, "回复应 0.5(基础)+0.4+0.1 = 1.0/s，实际 {regen}");
        // kb：098c 无点数属性 → 仅物品（头盔）0.32
        assert!((p.effective_kb_reduction() - 0.32).abs() < 1e-9);
        // 生命上限：基础 100 + 物品 20 + 30 = 150（refresh_derived 落账）
        p.refresh_derived();
        assert!(near(p.max_hp, 150.0, 0.01), "生命上限应 150，实际 {:?}", p.max_hp);
    }

    /// `configure_regen`：房间设置可调基础回血（098c 主机常量 `-C9`）。
    #[test]
    fn explode_multi_hit_emits_hattrick_or_vampire() {
        // 一次 AoE 命中 ≥3 敌人 → 098c 播报事件（Hattrick；戴死亡面具则 Vampire）。
        let setup = |w: &mut World| {
            w.obstacles.clear();
            w.players[0].pos = Vec2::new(Fix64::from_num(20.0), Fix64::ZERO);
            w.players[0].team = 0;
            for p in w.players.iter_mut().skip(1) {
                p.team = 1;
            }
            w.players[1].pos = Vec2::new(Fix64::from_num(1.0), Fix64::ZERO);
            w.players[2].pos = Vec2::new(Fix64::ZERO, Fix64::from_num(1.0));
            w.players[3].pos = Vec2::new(-Fix64::from_num(1.0), Fix64::ZERO);
        };
        let mut w = World::new(4, 7);
        setup(&mut w);
        let n = w.explode_at(
            Vec2::ZERO, 0, Fix64::from_num(3.0), Fix64::from_num(5.0), Fix64::ZERO,
            true, false, DmgFalloff::None,
        );
        assert_eq!(n, 3, "应命中 3 个敌人");
        assert!(
            w.combat_events.iter().any(|e| matches!(e, CombatEvent::Explode { .. })),
            "AoE 爆炸应产生 Explode 表现事件"
        );
        assert!(
            w.combat_events.iter().any(|e| matches!(e, CombatEvent::MultiHit { vampire: false, .. })),
            "≥3 命中应产生 Hattrick（非吸血鬼）事件"
        );

        let mut v = World::new(4, 7);
        setup(&mut v);
        v.players[0].set_items(&[crate::item::ItemId::FireMask]);
        let _ = v.explode_at(
            Vec2::ZERO, 0, Fix64::from_num(3.0), Fix64::from_num(5.0), Fix64::ZERO,
            true, false, DmgFalloff::None,
        );
        assert!(
            v.combat_events.iter().any(|e| matches!(e, CombatEvent::MultiHit { vampire: true, .. })),
            "戴死亡面具应产生 Vampire 事件"
        );

        // 事件不进模拟：`step` 开头清空上一帧表现事件。
        let none = vec![PlayerInput::default(); 4];
        v.step(none, Fix64::from_num(1.0 / 60.0));
        assert!(v.combat_events.is_empty(), "step 应清空上一帧表现事件");
    }

    /// 接触 AoE（`bA`/`SI`）：`splash_damage` 返回命中数（供表现事件决定是否画圈）。
    #[test]
    fn splash_damage_reports_hit_count() {
        let mut w = World::new(3, 11);
        for p in w.players.iter_mut() {
            p.alive = true;
        }
        w.players[0].team = 0;
        w.players[0].pos = Vec2::ZERO;
        w.players[1].team = 1;
        w.players[1].pos = Vec2::new(Fix64::from_num(30.0), Fix64::ZERO);
        w.players[2].team = 1;
        w.players[2].pos = Vec2::new(Fix64::from_num(300.0), Fix64::ZERO); // 半径外
        let hits = splash_damage(
            &mut w.players,
            Vec2::ZERO,
            0,
            Fix64::from_num(5.0),
            Fix64::from_num(50.0),
            0.0,
            Fix64::ONE,
        );
        assert_eq!(hits, 1, "仅半径内的敌人被命中");
    }

    /// 天罚/虔诚伤害随距离乘法衰减（098c `mI(...,1-d/1000)`）：中心 > 边缘。
    #[test]
    fn smite_damage_falls_off_with_distance() {
        let mut w = World::new(3, 5);
        for p in w.players.iter_mut() {
            p.alive = true;
        }
        w.players[0].team = 0;
        w.players[0].pos = Vec2::ZERO;
        w.players[1].team = 1;
        w.players[1].pos = Vec2::ZERO; // 中心
        w.players[2].team = 1;
        w.players[2].pos = Vec2::new(Fix64::from_num(200.0), Fix64::ZERO); // 半径内、远离中心
        let _ = w.explode_at(
            Vec2::ZERO,
            0,
            Fix64::from_num(250.0),
            Fix64::from_num(10.0),
            Fix64::ZERO,
            true,
            true,
            DmgFalloff::Mul(Fix64::from_num(1000.0)),
        );
        let d_center = 100.0 - w.players[1].hp.to_num::<f64>();
        let d_edge = 100.0 - w.players[2].hp.to_num::<f64>();
        assert!(d_center > d_edge, "中心伤害应高于边缘: {d_center} vs {d_edge}");
        assert!((d_edge / d_center - 0.8).abs() < 0.05, "200/1000 → 边缘约 0.8×，实际 {}", d_edge / d_center);
    }

    #[test]
    fn smite_denied_breaks_link_on_special_target() {
        let tether = || Projectile {
            owner: 0,
            kind: ProjectileKind::Tether {
                owner: 0,
                target: 1,
                damage_per_sec: Fix64::ZERO,
                pull_speed: Fix64::ZERO,
                remaining: Fix64::from_num(1.0),
                beam: false,
            },
            pos: Vec2::ZERO,
            alive: true,
        };
        // 目标处于风步（特殊状态）→ 天罚命中应断链 + Denied。
        let mut w = World::new(2, 4242);
        w.obstacles.clear();
        w.players[1].pos = Vec2::new(Fix64::from_num(2.0), Fix64::ZERO);
        w.players[1].windwalk_state = Fix64::from_num(1.0);
        w.projectiles.push(tether());
        let n = w.explode_at(
            w.players[1].pos, 0, Fix64::from_num(50.0), Fix64::from_num(3.0), Fix64::ZERO,
            true, true, DmgFalloff::None,
        );
        assert_eq!(n, 1);
        assert!(
            w.combat_events.iter().any(|e| matches!(e, CombatEvent::Denied { .. })),
            "被链接 + 特殊状态的天罚应触发 Denied"
        );
        assert!(!w.projectiles.iter().any(|p| matches!(p.kind, ProjectileKind::Tether { .. })), "应断链");

        // 非特殊状态 → 不断链、不 Denied。
        let mut w2 = World::new(2, 4242);
        w2.obstacles.clear();
        w2.players[1].pos = Vec2::new(Fix64::from_num(2.0), Fix64::ZERO);
        w2.projectiles.push(tether());
        let _ = w2.explode_at(
            w2.players[1].pos, 0, Fix64::from_num(50.0), Fix64::from_num(3.0), Fix64::ZERO,
            true, true, DmgFalloff::None,
        );
        assert!(!w2.combat_events.iter().any(|e| matches!(e, CombatEvent::Denied { .. })));
        assert!(w2.projectiles.iter().any(|p| matches!(p.kind, ProjectileKind::Tether { .. })));
    }

    #[test]
    fn burnout_triggers_on_ally_contact_while_burning() {
        // 098c `lb`：燃烧冲刺（Hr）撞到**同队**单位 → Burnout（熄火 + 对队友造成伤害）。
        let mut w = World::new(2, 5);
        w.players[0].team = 0;
        w.players[1].team = 0;
        w.players[0].pos = Vec2::ZERO;
        w.players[1].pos = Vec2::new(Fix64::from_num(1.0), Fix64::ZERO);
        w.players[0].burning = true;
        let hp1 = w.players[1].hp;
        let evs = resolve_player_collisions(&mut w.players, Fix64::from_num(1.0 / 60.0), Fix64::ONE, Fix64::ONE);
        assert!(
            evs.iter().any(|e| matches!(e, CombatEvent::Burnout { .. })),
            "同队接触应触发 Burnout 事件"
        );
        assert!(w.players[1].hp < hp1, "队友应受到 Burnout 伤害");
        assert!(!w.players[0].burning, "Burnout 应熄灭燃烧状态");
    }

    #[test]
    fn windwalk_parry_refreshes_and_knocks_back() {
        // 098c `CA` 招架：风步 B（`windwalk_state>0`）+ `gr` 就绪，被敌人伤害 →
        // 刷新风步、`gr` 进入 0.5s 冷却、双方互相击退。
        let mut w = World::new(2, 42);
        w.obstacles.clear();
        w.players[0].team = 0;
        w.players[1].team = 1;
        // 风步者在原点，敌人在 +x；招架应把两者**互相推开**（敌 −x、自己 +x）。
        w.players[0].pos = Vec2::ZERO;
        w.players[1].pos = Vec2::new(Fix64::from_num(60.0), Fix64::ZERO);
        w.players[0].windwalk_state = Fix64::from_num(1.0);
        w.players[0].parry_ready = true;
        w.players[0].contact_by_enemy = Some(1);
        let before_ww = w.players[0].windwalk_state;
        w.process_parry(Fix64::from_num(1.0 / 60.0));
        assert!(w.players[0].windwalk_state > before_ww, "招架应刷新风步");
        assert!(!w.players[0].parry_ready, "招架后 gr 应进入冷却");
        assert!(w.players[0].parry_cd > Fix64::ZERO);
        assert!(w.players[1].control.is_some(), "攻击者应被击退");
        assert!(w.players[0].control.is_some(), "风步者自己应被击退");
        // 方向：敌人沿 +x 远离风步者（control.vel.x > 0）；风步者沿 −x 远离敌人（< 0）。
        assert!(w.players[1].control.as_ref().unwrap().vel.x > Fix64::ZERO, "敌人应被推离风步者（+x）");
        assert!(w.players[0].control.as_ref().unwrap().vel.x < Fix64::ZERO, "风步者应被推离敌人（−x）");
        assert!(w.players[0].contact_by_enemy.is_none(), "瞬态应被清除");

        // `NA`：0.5s 后 `gr` 恢复（仍在风步）。
        w.process_parry(Fix64::from_num(0.6));
        assert!(w.players[0].parry_ready, "0.5s 后应恢复招架就绪");
    }

    #[test]
    fn windwalk_parry_ignores_allies_and_non_windwalkers() {
        let mut w = World::new(2, 42);
        w.obstacles.clear();
        // 同队伤害 → 不招架。
        w.players[0].team = 0;
        w.players[1].team = 0;
        w.players[0].windwalk_state = Fix64::from_num(1.0);
        w.players[0].parry_ready = true;
        w.players[0].contact_by_enemy = Some(1);
        w.process_parry(Fix64::from_num(1.0 / 60.0));
        assert!(w.players[0].parry_ready, "同队接触不应触发招架");
        assert!(w.players[0].control.is_none());
        // 非风步 → 不招架。
        w.players[1].team = 1;
        w.players[0].windwalk_state = Fix64::ZERO;
        w.players[0].contact_by_enemy = Some(1);
        w.process_parry(Fix64::from_num(1.0 / 60.0));
        assert!(w.players[1].control.is_none(), "非风步不应触发招架");
    }

    #[test]
    fn casting_a_skill_ends_windwalk() {
        // 098c：疾风步期间**施放其它技能** → 提前结束风步（隐身消失、风步计时清零、
        // 招架就绪/冷却重置，风步附带的移速 buff 一并移除），而不是等计时到期。
        let mut w = World::new(2, 4242);
        w.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        w.players[0].team = 0;
        w.players[1].team = 1;
        w.players[0].pos = Vec2::ZERO;
        w.players[1].pos = Vec2::new(d60(5.0), Fix64::ZERO);
        // 进入风步状态（隐身 + 移速 + 风步计时 + 招架就绪）。
        w.players[0].add_buff(crate::player::BuffKind::Stealth, 5.0);
        w.players[0].add_buff(crate::player::BuffKind::Speed(100.0), 5.0);
        w.players[0].windwalk_state = Fix64::from_num(5.0);
        w.players[0].parry_ready = true;
        assert!(w.players[0].stealth());

        // 施放火球（非风步技能）→ 施法成功的那一帧应立即结束风步。
        let input = vec![
            PlayerInput { cast: Some((SkillId::S000, Some(Vec2::new(d60(5.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ];
        w.step(input, dt);
        assert!(!w.players[0].stealth(), "施法后应现形（清除 Stealth）");
        assert_eq!(w.players[0].windwalk_state, Fix64::ZERO, "施法后风步计时应清零");
        assert!(!w.players[0].parry_ready, "施法后招架就绪应清除");
        assert!(
            !w.players[0].has_buff(crate::player::BuffKind::Speed(0.0)),
            "风步附带的移速 buff 应随风步结束而移除"
        );
    }

    #[test]
    fn enemy_contact_marks_contact_by_enemy() {
        // 接触敌方时在两个方向都标记瞬态（供 `process_parry` 判定）。
        let mut w = World::new(2, 9);
        w.players[0].team = 0;
        w.players[1].team = 1;
        w.players[0].pos = Vec2::ZERO;
        w.players[1].pos = Vec2::new(Fix64::from_num(1.0), Fix64::ZERO);
        let _ = resolve_player_collisions(&mut w.players, Fix64::from_num(1.0 / 60.0), Fix64::ONE, Fix64::ONE);
        assert_eq!(w.players[0].contact_by_enemy, Some(1));
        assert_eq!(w.players[1].contact_by_enemy, Some(0));
        // 同队接触不标记。
        w.players[1].team = 0;
        w.players[0].contact_by_enemy = None;
        let _ = resolve_player_collisions(&mut w.players, Fix64::from_num(1.0 / 60.0), Fix64::ONE, Fix64::ONE);
        assert_eq!(w.players[0].contact_by_enemy, None, "同队不应标记");
    }

    #[test]
    fn explode_at_scales_with_damage_mult() {
        // AoE（陨石/新星等经 explode_at）也应乘全局伤害倍率（098c 在 hI 入口乘 Gn）。
        let mut a = World::new(2, 321);
        a.obstacles.clear();
        a.players[1].pos = Vec2::new(Fix64::from_num(2.0), Fix64::ZERO);
        let mut b = World::new(2, 321);
        b.obstacles.clear();
        b.players[1].pos = Vec2::new(Fix64::from_num(2.0), Fix64::ZERO);
        b.damage_mult = Fix64::from_num(2.0);
        let hp_a = a.players[1].hp;
        let hp_b = b.players[1].hp;
        let at = |w: &mut World| {
            w.explode_at(
                Vec2::new(Fix64::from_num(2.0), Fix64::ZERO),
                0,
                Fix64::from_num(1.0),
                Fix64::from_num(10.0),
                Fix64::ZERO,
                false,
                false,
                DmgFalloff::None,
            )
        };
        at(&mut a);
        at(&mut b);
        let da = (hp_a - a.players[1].hp).to_num::<f64>();
        let db = (hp_b - b.players[1].hp).to_num::<f64>();
        assert!(da > 0.0, "基础 AoE 伤害应 > 0");
        assert!((db - 2.0 * da).abs() < 1e-6, "AoE 应乘 damage_mult：da={da} db={db}");
    }

    #[test]
    fn configure_mults_scales_damage_and_lava() {
        // configure_mults 存储三倍率
        let mut w0 = World::new(1, 1);
        w0.configure_mults(1.5, 0.5, 0.0);
        assert_eq!(w0.damage_mult, Fix64::from_num(1.5));
        assert_eq!(w0.knockback_mult, Fix64::from_num(0.5));
        assert_eq!(w0.lava_damage_mult, Fix64::ZERO);

        // 伤害倍率：同一伤害，damage_mult=2 → 掉血翻倍
        let mut a = World::new(2, 123);
        a.obstacles.clear();
        let mut b = World::new(2, 123);
        b.obstacles.clear();
        b.damage_mult = Fix64::from_num(2.0);
        a.damage_player(1, Fix64::from_num(10.0), Some(0));
        b.damage_player(1, Fix64::from_num(10.0), Some(0));
        let da = (a.players[1].max_hp - a.players[1].hp).to_num::<f64>();
        let db = (b.players[1].max_hp - b.players[1].hp).to_num::<f64>();
        assert!(da > 0.0, "基础伤害应 > 0");
        assert!((db - 2.0 * da).abs() < 1e-6, "damage_mult=2 应双倍，da={da} db={db}");

        // 岩浆倍率：0 = 关闭出界伤害
        let mut w = World::new(1, 123);
        w.obstacles.clear();
        w.players[0].pos = Vec2::new(w.arena_radius + Fix64::from_num(10.0), Fix64::ZERO);
        w.lava_damage_mult = Fix64::ZERO;
        let hp0 = w.players[0].hp;
        let none = vec![PlayerInput::default()];
        for _ in 0..30 {
            w.step(none.clone(), Fix64::from_num(1.0 / 60.0));
        }
        assert_eq!(w.players[0].hp, hp0, "lava_damage_mult=0 应关闭岩浆伤害");
    }

    #[test]
    fn terrain_modes_control_pillar_and_ice() {
        // 柱子/冰面模式（我们自己的设置）：0=关闭 2=必有。
        let mut w = World::new(2, 777);
        w.configure_terrain(0, 0);
        assert!(w.obstacles.is_empty(), "pillar_mode=0 应无柱子");
        assert!(w.ice.is_empty(), "ice_mode=0 应无冰面");
        let mut w2 = World::new(2, 777);
        w2.configure_terrain(2, 2);
        assert!(!w2.obstacles.is_empty(), "pillar_mode=2（每局必有）应至少 1 根");
        assert!(!w2.ice.is_empty(), "ice_mode=2（每局必有）应至少 1 块");
    }

    #[test]
    fn configure_regen_overrides_base_regen() {
        let mut world = World::new(1, 1003);
        world.configure_regen(2.0);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].hp = Fix64::from_num(50.0);
        for _ in 0..60 {
            world.step(vec![PlayerInput::default()], dt);
        }
        let gained = world.players[0].hp.to_num::<f64>() - 50.0;
        assert!((gained - 2.0).abs() < 0.05, "1s 应回 2.0，实际 {gained}");
    }

    /// 098c **基础回血 0.5/s**（`In=.05` 每 0.1s；同 tick 的岩浆 `To=.9`=9/s 为同刻度佐证）
    /// ——物品回复（斗篷）在其上叠加；灼烧禁疗。
    #[test]
    fn hp_regen_base_plus_item_and_blocked_by_scorch() {
        let mut world = World::new(1, 967);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].hp = Fix64::from_num(50.0);
        world.step(vec![PlayerInput::default()], dt);
        let none = vec![PlayerInput::default()];
        // 无物品：基础 0.5/s → 2s 回 1.0
        for _ in 0..120 {
            world.step(none.clone(), dt);
        }
        let base = world.players[0].hp.to_num::<f64>() - 50.0;
        assert!((base - 1.0).abs() < 0.05, "基础回血 2s 应回 1.0（0.5/s），实际 {base}");
        // 斗篷 3（+0.4/s）→ 合计 0.9/s，2s 回 1.8
        world.players[0].set_items(&[crate::item::ItemId::Cloak3]);
        world.players[0].hp = Fix64::from_num(50.0);
        for _ in 0..120 {
            world.step(none.clone(), dt);
        }
        let gained = world.players[0].hp.to_num::<f64>() - 50.0;
        assert!((gained - 1.8).abs() < 0.05, "基础 0.5 + 斗篷 0.4 = 0.9/s，2s 应回 1.8，实际 {gained}");
        // 灼烧 → 禁疗（含基础与物品回复）
        world.players[0].hp = Fix64::from_num(50.0);
        world.players[0].add_buff(BuffKind::Scorched, 4.0);
        for _ in 0..120 {
            world.step(none.clone(), dt);
        }
        assert!(near(world.players[0].hp, 50.0, 0.001), "灼烧期间不应回血，实际 {:?}", world.players[0].hp);
    }

    /// S001 天罚：以自身为中心 nova——伤附近敌人但不伤自己（exclude_owner）。
    #[test]
    fn s001_smiting_nova_hits_enemies_not_self() {
        let mut world = World::new(3, 963);
        world.base_regen = 0.0; // 本测试只验证其他机制：屏蔽基础回血漂移
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(2.0), Fix64::ZERO); // 120 < 250 内
        world.players[1].move_target = None;
        world.players[2].pos = Vec2::new(d60(8.0), Fix64::ZERO); // 480 > 250 外
        world.players[2].move_target = None;
        let (hp0, hp1, hp2) = (world.players[0].hp, world.players[1].hp, world.players[2].hp);
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S001, None)), ..Default::default() },
            PlayerInput::default(),
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(); 3];
        for _ in 0..60 {
            world.step(none.clone(), dt); // windup 0.7s
        }
        // 098c 实证：nova 对施法者自身也扣血（用户文档「包括自己！」）——但无自击退
        assert_eq!(world.players[0].hp, hp0 - Fix64::from_num(10), "天罚应自伤 10（无击退）");
        assert!(world.players[1].hp < hp1, "250 内敌人应受伤");
        assert_eq!(world.players[2].hp, hp2, "250 外敌人不应受伤");
    }

    /// S020 灾变：三级递进——伤害随 stage 递增且 stage 循环。
    #[test]
    fn s020_catastrophe_stages_escalate_and_cycle() {
        let mut world = World::new(2, 964);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(3.0), Fix64::ZERO); // 180 < 300/400
        world.players[1].move_target = None;
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        let mut drops: Vec<f64> = Vec::new();
        for stage in 0..3 {
            // 上一级的衰减击退（D8，位移 ~1400 且 3.5s 内持续）会把敌人推出半径——
            // 每轮拉回原位并清掉残留击退 control 再放。
            world.players[1].pos = Vec2::new(d60(3.0), Fix64::ZERO);
            world.players[1].control = None;
            world.players[1].move_target = None;
            let hp = world.players[1].hp;
            // 清冷却以连放（灾变 CD 3s）
            world.players[0].caster = crate::skill::Caster::new();
            world.step(vec![
                PlayerInput { cast: Some((SkillId::S020, None)), ..Default::default() },
                PlayerInput::default(),
            ], dt);
            for _ in 0..60 {
                world.step(none.clone(), dt);
            }
            let drop = (hp - world.players[1].hp).to_num::<f64>();
            drops.push(drop);
            let expect_stage = (stage + 1) % 3;
            assert_eq!(world.players[0].catastrophe_stage, expect_stage, "stage 应递进循环");
        }
        assert!(drops[0] > 0.0 && drops[1] > drops[0], "第二级伤害应高于第一级：{drops:?}");
        // 第三级半径 400（伤害也更高）；三级后回到 stage 0
        assert!(drops[2] > drops[1], "第三级应最强：{drops:?}");
    }

    /// S021 虔诚：伤敌人 + 自奶 gx×0.5（无队伍时只奶自己）。
    #[test]
    fn s021_devotion_hurts_enemy_and_heals_self() {
        let mut world = World::new(2, 965);
        world.base_regen = 0.0; // 本测试只验证其他机制：屏蔽基础回血漂移
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[0].hp = Fix64::from_num(50.0); // 半血施法者
        world.players[1].pos = Vec2::new(d60(3.0), Fix64::ZERO);
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S021, None)), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[1].hp < hp1, "虔诚应伤 250 内敌人");
        // 098c QC：虔诚自伤 10（FX 自扣）；**治疗只给队友**（JASS gX!=ii），FFA 下无队友 → 净 40
        assert!(near(world.players[0].hp, 40.0, 0.1), "FFA 下应只有自伤 10（净 40），实际 {:?}", world.players[0].hp);
    }

    /// S021 虔诚（队伍模式，B2）：500 内队友回血 cX/2 + 60 移速 4s；自己不自奶。
    #[test]
    fn s021_devotion_heals_teammates_in_team_mode() {
        let mut world = World::new(3, 986);
        world.base_regen = 0.0; // 本测试只验证其他机制：屏蔽基础回血漂移
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        // 0/1 同队 0，2 为敌人；施法者半血站桩，队友贴身
        world.players[0].team = 0;
        world.players[1].team = 0;
        world.players[2].team = 1;
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[0].hp = Fix64::from_num(50.0);
        world.players[1].pos = Vec2::new(d60(3.0), Fix64::ZERO); // 180 < 500 内
        world.players[1].move_target = None;
        world.players[1].hp = Fix64::from_num(40.0);
        world.players[2].pos = Vec2::new(d60(-3.0), Fix64::ZERO);
        world.players[2].move_target = None;
        let (hp1, hp2) = (world.players[1].hp, world.players[2].hp);
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S021, None)), ..Default::default() },
            PlayerInput::default(),
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default(), PlayerInput::default()];
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        // 队友：+5（cX/2=10/2）回血 + 60 移速 buff
        assert!(near(world.players[1].hp, 45.0, 0.1), "队友应回血 5（40→45），实际 {:?}", world.players[1].hp);
        assert!(world.players[1].has_buff(BuffKind::Speed(1.0 + 60.0 / 210.0)) || world.players[1].buffs.iter().any(|b| b.remaining > Fix64::ZERO && matches!(b.kind, BuffKind::Speed(_))), "队友应有移速 buff");
        // 自己：只有自伤 10（50→40），不自奶
        assert!(near(world.players[0].hp, 40.0, 0.1), "自己不应被治疗（40），实际 {:?}", world.players[0].hp);
        // 敌人受伤
        assert!(world.players[2].hp < hp2, "敌人应被虔诚伤害");
        let _ = hp1;
    }

    /// S017 致残：命中后目标被 Tied（禁施法），持续 (4+0.25L)。
    #[test]
    fn s017_cripple_ties_target() {
        let mut world = World::new(2, 960);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(4.0), Fix64::ZERO);
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S017, Some(Vec2::new(d60(4.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        let mut tied = false;
        for _ in 0..90 {
            world.step(none.clone(), dt);
            if world.players[1].tied() { tied = true; }
        }
        assert!(world.players[1].hp < hp1, "致残弹应造成 KI 伤害");
        assert!(tied, "命中后目标应被残废（Tied）");
    }

    /// S019 锁链：命中后目标被拉向施法者（位移朝施法者）+ 短暂 Tied。
    #[test]
    fn s019_chain_pulls_target_toward_caster() {
        let mut world = World::new(2, 961);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 敌人在 +x 方向 480 处；命中后应被拉向 -x（施法者方向）
        world.players[1].pos = Vec2::new(d60(8.0), Fix64::ZERO);
        world.players[1].move_target = None;
        let x_before = world.players[1].pos.x;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S019, Some(Vec2::new(d60(8.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..90 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[1].pos.x < x_before, "锁链应把目标拉向施法者（-x），实际 {:?}", world.players[1].pos.x);
    }

    /// S019 锁链：命中后落地为持久 Tether，逐帧对绑定目标施加**每秒**伤害（文档 `0.2+0.1×L` 每秒）。
    /// 验证「链子必须对链住的对象施加伤害」且为持续型（非单发直伤）。
    #[test]
    fn s019_chain_damages_bound_target_per_second() {
        let mut world = World::new(2, 963);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(8.0), Fix64::ZERO); // +x 480 处敌人
        world.players[1].move_target = None;
        let hp_before = world.players[1].hp.to_num::<f64>();
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S019, Some(Vec2::new(d60(8.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        // 早期几帧：Tether 刚生成，伤害尚未累积
        for _ in 0..4 {
            world.step(none.clone(), dt);
        }
        let hp_early = world.players[1].hp.to_num::<f64>();
        // 持续整段（Tether 寿命 0.5s ≈ 30 帧）
        for _ in 0..40 {
            world.step(none.clone(), dt);
        }
        let hp_after = world.players[1].hp.to_num::<f64>();
        assert!(hp_after < hp_before, "锁链应对绑定目标持续掉血，{} -> {}", hp_before, hp_after);
        assert!(hp_after < hp_early, "锁链伤害应逐帧累积（非单发），{} -> {}", hp_early, hp_after);
    }

    /// S022 镜像分身（C 栏）：施放后生成 2 个跟随施法者的分身，施法者获得 +25 移速与
    /// 「否决锁链和负面效果」免疫；分身周期射出火球造成伤害；持续时间（4s）结束后分身消失。
    #[test]
    fn s022_mirror_spawns_clones_casts_fireball_and_expires() {
        let mut world = World::new(2, 964);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(8.0), Fix64::ZERO); // +x 480 处敌人
        world.players[1].move_target = None;
        let hp_before = world.players[1].hp.to_num::<f64>();

        world.step(
            vec![
                PlayerInput { cast: Some((SkillId::S022, None)), ..Default::default() },
                PlayerInput::default(),
            ],
            dt,
        );

        // 1) 生成 2 个非实体分身
        let clones = world
            .projectiles
            .iter()
            .filter(|pr| matches!(pr.kind, ProjectileKind::Clone { .. }))
            .count();
        assert_eq!(clones, 2, "应生成 2 个镜像分身，实际 {}", clones);

        // 2) 施法者获得 +25 移速与镜像免疫
        assert!(world.players[0].has_buff(BuffKind::Mirror), "施法者应获得镜像免疫 buff");
        assert!(
            world.players[0].buff_value(BuffKind::Speed(1.0)) > 1.0,
            "施法者应获得 +25 移速，实际倍率 {}",
            world.players[0].buff_value(BuffKind::Speed(1.0))
        );

        // 3) 分身周期射出火球并造成伤害
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        let mut fireball_seen = false;
        for _ in 0..120 {
            world.step(none.clone(), dt);
            if world.projectiles.iter().any(|pr| matches!(pr.kind, ProjectileKind::W098b { .. })) {
                fireball_seen = true;
            }
        }
        assert!(fireball_seen, "分身应周期射出火球");
        let hp_after = world.players[1].hp.to_num::<f64>();
        assert!(hp_after < hp_before, "分身火球应对敌人造成伤害，{} -> {}", hp_before, hp_after);

        // 4) 镜像免疫：期间施加束缚应被否决
        world.players[0].add_buff(BuffKind::Tied, 2.0);
        assert!(!world.players[0].has_buff(BuffKind::Tied), "镜像期间应否决束缚（Tied）");

        // 5) 持续时间结束后分身消失
        for _ in 0..200 {
            world.step(none.clone(), dt);
        }
        let clones_left = world
            .projectiles
            .iter()
            .filter(|pr| matches!(pr.kind, ProjectileKind::Clone { .. }))
            .count();
        assert_eq!(clones_left, 0, "持续时间结束后分身应消失，实际 {}", clones_left);
    }

    /// S018 引力：施放后场上出现吸拉场，附近敌人被拉近。
    #[test]
    fn s018_gravity_zone_pulls_enemy() {
        let mut world = World::new(2, 962);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 敌人在场心 (360,0) 半径 200 内（距场心 120），应被吸向场心
        world.players[1].pos = Vec2::new(d60(6.0), d60(2.0));
        world.players[1].move_target = None;
        let dist_before = (world.players[1].pos - Vec2::new(d60(6.0), Fix64::ZERO)).length();
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S018, Some(Vec2::new(d60(6.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        let mut gravity_seen = false;
        for _ in 0..180 {
            world.step(none.clone(), dt);
            if world.projectiles.iter().any(|pr| matches!(pr.kind, ProjectileKind::Gravity { .. })) {
                gravity_seen = true;
            }
        }
        assert!(gravity_seen, "施放后场上应出现引力场弹体");
        let dist_after = (world.players[1].pos - Vec2::new(d60(6.0), Fix64::ZERO)).length();
        assert!(dist_after < dist_before, "引力场应把附近敌人吸向场心，{} -> {}", dist_before, dist_after);
    }

    /// S018 引力·黑洞（A 形态）：范围内敌人持续扣血（098c `hc`：每 tick `0.1+0.2×等级`，换算为 DPS）。
    #[test]
    fn s018_black_hole_damages_enemies_in_field() {
        let mut world = World::new(2, 1010);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].team = 0;
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].team = 1;
        // 敌人原地不动，位于落点场心（240,0）伤害半径 274 内
        world.players[1].pos = Vec2::new(d60(4.0), d60(1.0));
        world.players[1].move_target = None;
        let hp_before = world.players[1].hp.to_num::<f64>();
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S018, Some(Vec2::new(d60(4.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        let hp_after = world.players[1].hp.to_num::<f64>();
        assert!(hp_after < hp_before, "黑洞应每秒扣血（0.3+0.2×L），{} -> {}", hp_before, hp_after);
    }

    /// 文档数值回归：S019 锁链 / S018 引力 A·B 的伤害与回复公式（均按 098c w3a_strings 原斜率）。
    /// 锁链伤害 `0.2+0.2×L`；引力·黑洞伤害 `0.3+0.2×L`；
    /// 引力·力场 每秒伤害 `2.25+0.8214×L`、每秒生命恢复 `1.0+0.2×L`（MAX_HP=100）。
    /// 其中 L = 升级次数 = level-1（见 `SkillGrowth::stats`）。
    #[test]
    fn doc_s019_s018_growth_matches_doc() {
        let lvl = 3u32;
        let l = (lvl - 1) as f64;
        // S019 chain (A)
        let d = DefTable::def(SkillId::S019).growth.stats(lvl).damage.to_num::<f64>();
        assert!((d - (0.2 + 0.2 * l)).abs() < 1e-6, "chain dmg {d} != {}", 0.2 + 0.2 * l);
        // S018 gravity blackhole (A)：098c 每 tick 0.1+0.2×L（L1=0.3），按 0.06s 间隔换算为 DPS ×16.67。
        let d = DefTable::def(SkillId::S018).growth.stats(lvl).damage.to_num::<f64>();
        let k = 1.0 / 0.06;
        assert!((d - (0.3 + 0.2 * l) * k).abs() < 1e-3, "blackhole dps {d} != {}", (0.3 + 0.2 * l) * k);
        // S018 force field (B)
        let st = DefTable::def_alt(SkillId::S018).expect("S018 has B form").growth.stats(lvl);
        let d = st.damage.to_num::<f64>();
        let e = st.extra.to_num::<f64>();
        assert!((d - (2.25 + 0.8214 * l)).abs() < 1e-6, "field dps {d} != {}", 2.25 + 0.8214 * l);
        assert!((e - (1.0 + 0.2 * l)).abs() < 1e-6, "field hps {e} != {}", 1.0 + 0.2 * l);
    }

    /// S018 引力·黑洞（A 形态）：范围内敌人持续掉血（098c `hc`：每 tick `0.1+0.2×等级`）。
    #[test]
    fn s018_black_hole_damages_enemy_in_field() {
        let mut world = World::new(2, 1008);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].team = 0;
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].team = 1;
        // 敌人在落点（场心 240,0）伤害半径 274 内且不自行移动
        world.players[1].pos = Vec2::new(d60(4.0), d60(1.0));
        world.players[1].move_target = None;
        // 关掉基础回血，保证测的是黑洞伤害本身。
        world.configure_regen(0.0);
        let hp_before = world.players[1].hp.to_num::<f64>();
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S018, Some(Vec2::new(d60(4.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..120 {
            world.step(none.clone(), dt);
        }
        let hp_after = world.players[1].hp.to_num::<f64>();
        assert!(hp_after < hp_before, "黑洞应持续扣血（0.1+0.2×L 每 tick），{hp_before} -> {hp_after}");
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

    /// S012 冲撞：命中后施法者急停（098c BA，war3map_pretty.j:3771），不再继续冲过目标。
    #[test]
    fn s012_dash_stops_caster_on_hit() {
        let mut world = World::new(2, 9581);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(5.0), Fix64::ZERO); // 300 处的敌人
        world.players[1].move_target = None;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S012, Some(Vec2::new(d60(10.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        // 20 帧 ≈ 0.33s；1300/s 约 0.23s（14 帧）撞上，命中后应立刻停住。
        for _ in 0..20 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].control.is_none(), "命中后应清除强制位移（098c BA 急停）");
        assert!(world.players[0].pos.x < d60(5.0), "应停在敌人前方而非冲过目标，x={:?}", world.players[0].pos.x);
    }

    /// S012 冲撞：接触击退随**受击者精通**缩放（098c mI 的 Hn = 每级 -2.5%）。
    /// 两世界同招同距，仅受击者精通级数不同 → 击退位移按比例递减。
    /// 注：Hn 是精通减免，与法抗无关（属性系统删除后已无 spell_factor）。
    #[test]
    fn s012_dash_knockback_scales_with_mastery() {
        let setup = |mastery: [u8; 3]| -> Fix64 {
            let mut world = World::new(2, 9582);
            world.obstacles.clear();
            world.sandbox = true;
            let dt = Fix64::from_num(1.0 / 60.0);
            world.players[0].pos = Vec2::ZERO;
            world.players[0].move_target = None;
            world.players[1].pos = Vec2::new(d60(5.0), Fix64::ZERO);
            world.players[1].move_target = None;
            world.players[1].mastery = mastery; // 受击者精通（lf=和）
            world.step(vec![
                PlayerInput { cast: Some((SkillId::S012, Some(Vec2::new(d60(10.0), Fix64::ZERO)))), ..Default::default() },
                PlayerInput::default(),
            ], dt);
            let none = vec![PlayerInput::default(), PlayerInput::default()];
            for _ in 0..25 {
                world.step(none.clone(), dt);
            }
            world.players[1].pos.x // 被击退后的 x
        };
        let x0 = setup([0, 0, 0]); // lf=0 → 无减免
        let x4 = setup([2, 2, 0]); // lf=4 → -10%
        // 起始 x=300(=d60(5.0))；精通越高击退越弱，位移越小。
        let d0 = x0 - d60(5.0);
        let d4 = x4 - d60(5.0);
        assert!(d0 > Fix64::ZERO, "无精通目标应被击退，d0={:?}", d0);
        assert!(d4 < d0, "4 级精通的击退应弱于 0 级，d4={:?} d0={:?}", d4, d0);
        // 4 级 → ×0.9（容差 10%）
        assert!((d4 - d0 * Fix64::from_num(0.9)).abs() < d0 * Fix64::from_num(0.1),
            "4 级精通击退应≈×0.9，d4={:?} d0={:?}", d4, d0);
    }

    /// S012 冲撞：正面撞柱 → **半速反弹**（098c 英雄 `nv==1,xv=.5`：`set Q=-Q*xv`，
    /// war3map_pretty.j:4953/4979/8656），而不是停在障碍前。
    #[test]
    fn s012_dash_bounces_off_obstacle_head_on() {
        let mut world = World::new(2, 9584);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 远处放敌人，避免途中撞到而误触发急停分支。
        world.players[1].pos = Vec2::new(d60(20.0), Fix64::ZERO);
        world.players[1].move_target = None;
        // x=5m 处放半径 0.5m 的障碍，挡住 +x 冲撞路径。
        let obs_r = 30.0;
        world.obstacles.push(Obstacle::new(Vec2::new(d60(5.0), Fix64::ZERO), obs_r));
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S012, Some(Vec2::new(d60(10.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        // 先推进到刚接触，确认发生了反弹（该轴速度反向）。
        let mut bounced = false;
        let mut max_x = Fix64::ZERO;
        for _ in 0..40 {
            world.step(none.clone(), dt);
            max_x = max_x.max(world.players[0].pos.x);
            if let Some(c) = world.players[0].control.as_ref() {
                if c.vel.x < Fix64::ZERO {
                    bounced = true;
                }
            }
        }
        // 应停在障碍前（pos.x ≈ 障碍中心 - 障碍半径 - 玩家半径）。
        let contact_limit = d60(5.0) - Fix64::from_num(obs_r) - world.players[0].radius;
        assert!(max_x <= contact_limit + Fix64::from_num(3.0),
            "不应钻入障碍，max_x={:?} 上限={:?}", max_x, contact_limit);
        assert!(bounced, "英雄撞柱应按 098c 逐轴 `-Q*xv` 半速反弹（速度反向）");
        assert!(world.players[0].pos.x < contact_limit - Fix64::from_num(30.0),
            "反弹后应明显退回，pos.x={:?}", world.players[0].pos.x);
    }

    /// S012 冲撞：**斜撞**障碍 → 法向轴半速反弹、切向保留（098c 逐轴 `-Q*xv`），
    /// 而不是像旧实现那样整体清掉强制位移（`control=None`）或清零停死。
    #[test]
    fn s012_dash_slides_along_obstacle_at_angle() {
        let mut world = World::new(2, 4242);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 敌人在远处，避免途中碰人。
        world.players[1].pos = Vec2::new(d60(30.0), d60(30.0));
        world.players[1].move_target = None;
        // x=2m 处半径 0.5m 的障碍，法向为 +x。
        let obs = Vec2::new(d60(2.0), Fix64::ZERO);
        world.obstacles.push(Obstacle::new(obs, 30.0));
        // 斜向强制位移：x 快、y 慢 → 撞上后应保留 y 分量沿墙滑行。
        world.players[0].push(
            Vec2::new(Fix64::from_num(1300.0), Fix64::from_num(400.0)),
            1.0,
        );
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..10 {
            world.step(none.clone(), dt);
        }
        let p = &world.players[0];
        // 未钻入障碍（圆心距 >= 双方半径之和 - 容差）
        let sep = (p.pos - obs).length();
        let min_sep = Fix64::from_num(30.0) + p.radius;
        assert!(
            sep >= min_sep - Fix64::from_num(2.0),
            "不应钻入障碍：圆心距={:?} 需>={:?}",
            sep,
            min_sep
        );
        assert!(
            p.pos.y > Fix64::from_num(20.0),
            "斜撞应沿墙滑行（保留切向），y={:?}",
            p.pos.y
        );
        assert!(p.control.is_some(), "沿墙滑行时强制位移不应被整体清掉");
    }

    /// S013A 移形换位（098c `MB`）：**弹体**命中敌人 → 双方互换位置，弹体销毁。
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
        // 弹体飞行（1700/s，300 距离 ≈ 18 帧）后命中 → 换位。
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..40 {
            world.step(none.clone(), dt);
        }
        assert!(near_d(world.players[0].pos.x, 5.0, 1.0), "施法者应换到敌人位置，实际 {:?}", world.players[0].pos);
        assert!(near_d(world.players[1].pos.x, 0.0, 1.0), "敌人应被换到施法者原位置，实际 {:?}", world.players[1].pos);
        assert!(
            !world.projectiles.iter().any(|p| matches!(p.kind, ProjectileKind::W098b { .. })),
            "命中后换位弹体应销毁"
        );
    }

    /// 移动中施法：应**停止移动并开始施法**（098c 施法前先下 stop order）。
    #[test]
    fn cast_while_moving_stops_movement_and_starts_cast() {
        let mut world = World::new(2, 20260912);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = Some(Vec2::new(d60(10.0), Fix64::ZERO));
        // 同帧：既有移动目标（电平量重发），又下达施法（S001 天罚为自身 nova，无需点目标）
        world.step(vec![
            PlayerInput {
                set_target: Some(Vec2::new(d60(10.0), Fix64::ZERO)),
                cast: Some((SkillId::S001, None)),
                ..Default::default()
            },
            PlayerInput::default(),
        ], dt);
        assert!(world.players[0].caster.is_busy(), "移动中施法应立即开始施法");
        assert!(world.players[0].move_target.is_none(), "施法应清除移动目标（停下）");
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
        assert!(hp1 - world.players[1].hp >= Fix64::from_num(6.9), "直伤≈7（容差含全局回血漂移）");
        assert!(!world.lightning_visual.is_empty(), "应写 lightning_visual 供 client 画线");
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
        // 直线上两个敌人：8m 与 10m；陨石飞向 **9m**（两敌之间）后爆炸，均在半径 210（≈3.5m）内。
        world.players[1].pos = Vec2::new(d60(8.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.players[2].pos = Vec2::new(d60(10.0), Fix64::ZERO);
        world.players[2].move_target = None;
        let hp1 = world.players[1].hp;
        let hp2 = world.players[2].hp;
        world.step(
            vec![
                PlayerInput { cast: Some((SkillId::S008, Some(Vec2::new(d60(9.0), Fix64::ZERO)))), ..Default::default() },
                PlayerInput::default(),
                PlayerInput::default(),
            ],
            dt,
        );
        let none = vec![PlayerInput::default(), PlayerInput::default(), PlayerInput::default()];
        for _ in 0..150 {
            world.step(none.clone(), dt); // 速度 400 → 900 距离需 ~2.25s
        }
        assert!(world.players[1].hp < hp1, "爆炸应波及 8m 处敌人");
        assert!(world.players[2].hp < hp2, "爆炸半径 210（≈3.5m）应波及 10m 处敌人");
    }

    /// S016 弹跳弹：两敌布阵——第一跳全额 6、跳向第二敌 ×0.8≈4.8；寿命耗尽后消失。
    ///（跳序由 nearest 决定；击退方向沿来向推离，不会把目标推进下一跳判定圈。）
    #[test]
    fn s016_bounce_jumps_with_decay() {
        let mut world = World::new(3, 955);
        world.base_regen = 0.0; // 本测试只验证其他机制：屏蔽基础回血漂移
        world.obstacles.clear();
        world.sandbox = true; // 衰减击退位移 ~1335 会把人推出 1200 场地，排除出界掉血干扰
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 衰减击退（D8）位移 ~1335 沿弹向（+x）：敌2 放命中点垂直方向 -850（< 900 弹程，
        // 且 1 被推 +x 远离第二跳路径 → 不会三跳往返）。
        world.players[1].pos = Vec2::new(d60(5.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.players[2].pos = Vec2::new(d60(5.0), Fix64::from_num(-850.0));
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
        // 098c 成长（D9）：第一跳命中后施法者 Gn=1.1 → 第二跳 = 6×0.8×1.1≈5.28
        assert!((d2 - 6.0 * 0.8 * 1.1).abs() < 0.3, "第二跳应 ×0.8×Gn1.1≈5.28，实际 {d2}");
        // 末跳命中时寿命重置（~0.94s），到 2.2s 时必已耗尽（3 跳上限由全场扫描+飞程自然保证）。
        for _ in 0..40 {
            world.step(none.clone(), dt);
        }
        let still = world
            .projectiles
            .iter()
            .any(|pr| matches!(pr.kind, ProjectileKind::W098b { proj: crate::skill::W098bProjKind::Bounce, .. }));
        assert!(!still, "弹跳弹寿命逐跳耗尽后应消失，不得无限弹");
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

    /// 收缩参数可配（房间设置 6 `wo`）：`configure_shrink` 改变每环时长后，
    /// **同样的时间**内缩得更多/更少；延迟也按 √存活 缩放（098c `wo*√sn`）。
    #[test]
    fn shrink_rate_follows_room_setting() {
        let dt = Fix64::from_num(1.0 / 60.0);
        let mut fast = World::new(1, 77);
        let mut slow = World::new(1, 77);
        fast.obstacles.clear();
        slow.obstacles.clear();
        fast.configure_shrink(0.0, 9.0); // 总时长 9s → 快
        slow.configure_shrink(0.0, 90.0); // 总时长 90s → 慢

        let r0 = fast.arena_radius;
        for _ in 0..120 {
            fast.step(vec![PlayerInput::default()], dt);
            slow.step(vec![PlayerInput::default()], dt);
        }
        let d_fast = (r0 - fast.arena_radius).to_num::<f64>();
        let d_slow = (r0 - slow.arena_radius).to_num::<f64>();
        assert!(d_fast > d_slow * 5.0, "总时长 9s 应比 90s 快约 10 倍（{d_fast:.2} vs {d_slow:.2}）");

        // 延迟同样生效：给一个很长延迟 + 长环时长 → 延迟内完全不缩
        let mut delayed = World::new(1, 77);
        delayed.obstacles.clear();
        delayed.configure_shrink(5.0, 90.0);
        let before = delayed.arena_radius;
        for _ in 0..60 {
            delayed.step(vec![PlayerInput::default()], dt);
        }
        assert_eq!(delayed.arena_radius, before, "延迟期内不应收缩");
    }

    #[test]
    fn arena_shrinks_to_zero() {
        let mut world = World::new(1, 92);
        let dt = Fix64::from_num(1.0 / 60.0);
        // 1 人局：9 环×64=576 初始半径；首延迟 10s + 连续收缩 90s → ≈100s = 6000 帧。
        assert!((world.arena_radius.to_num::<f64>() - 576.0).abs() < 1.0, "1 人局初始半径应 576（9 环×64）");
        assert_eq!(Balance::start_radius_for(2) as i64, 640, "2 人场应 640（用户验证：直径 20 术士）");
        let none = vec![PlayerInput::default()];
        for _ in 0..7000 {
            world.step(none.clone(), dt);
        }
        assert!(world.arena_radius <= Fix64::from_num(0.01), "场地应缩到 0，实际 {:?}", world.arena_radius);
    }

    /// 缩圈连续化（U3）：首延迟后连续收缩（任意 1s 窗口内都有缩小），总时长不变。
    #[test]
    fn shrink_is_continuous_after_delay() {
        let mut world = World::new(2, 92);
        let dt = Fix64::from_num(1.0 / 60.0);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        let r0 = world.arena_radius.to_num::<f64>();
        // 首个间隔（10×√2≈14.1s）：不收缩
        for _ in 0..840 {
            world.step(none.clone(), dt);
        }
        assert_eq!(world.arena_radius.to_num::<f64>(), r0, "首延迟内不收缩");
        // 之后连续收缩：2 人局总时长 90s、参考半径 640 → 速率 ≈ 640/90 ≈ 7.1 码/s
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        let dropped = r0 - world.arena_radius.to_num::<f64>();
        assert!(dropped > 5.0 && dropped < 10.0, "连续收缩每秒 ≈7.1 码，实际 {dropped}");
    }



    #[test]
    fn projectile_kill_is_recorded_in_kills_and_eliminated_order() {
        // 回归 P3：被弹体/爆炸击杀曾因 step-7 死亡结算循环 `if !alive { continue }`
        // 跳过而永不记账，导致击杀金币全不发、名次奖励发错人。
        let mut world = World::new(2, 91);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[1].pos = Vec2::new(Fix64::from_num(3.0), Fix64::ZERO);
        world.players[1].hp = Fix64::from_num(1.0); // 一击致命（用 S000 火球打死）
        let input = vec![
            PlayerInput { cast: Some((SkillId::S000, Some(Vec2::new(Fix64::from_num(6.0), Fix64::ZERO)))), ..Default::default() },
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
    fn cmd_at_reads_queue_in_order() {
        // 表现层用 `cmd_at` 按序读取 shift 队列（索引 0 = 队头）。
        let mut w = World::new(1, 3);
        w.players[0].cmd_clear();
        w.players[0].cmd_push(Cmd::Move(Vec2::new(d60(1.0), Fix64::ZERO)));
        w.players[0].cmd_push(Cmd::Cast(SkillId::Rock, None));
        assert!(matches!(w.players[0].cmd_at(0), Some(Cmd::Move(_))));
        assert!(matches!(w.players[0].cmd_at(1), Some(Cmd::Cast(..))));
        assert!(w.players[0].cmd_at(2).is_none(), "越界返回 None");
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
        world.players[1].pos = Vec2::new(d60(6.0), Fix64::ZERO);
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        // 压入：先移动到 (3,0)，再朝 (6,0) 施放掷弹(Rock)（war3 尺度布阵，碰撞 32 下不初始重叠）
        world.step(vec![
            PlayerInput { queued: vec![Cmd::Move(Vec2::new(d60(3.0), Fix64::ZERO))], ..Default::default() },
            PlayerInput::default(),
        ], dt);
        world.step(vec![
            PlayerInput { queued: vec![Cmd::Cast(SkillId::Rock, Some(Vec2::new(d60(6.0), Fix64::ZERO)))], ..Default::default() },
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

    // ===== B1 精通系统（098c kf，D12.3） =====

    /// 生命精通 vi：伤害吸血 +8%/级（098c L3299 HX×0.08×vi），任何伤害生效。
    #[test]
    fn mastery_vi_lifesteal_per_level() {
        let mut world = World::new(2, 981);
        world.base_regen = 0.0; // 本测试只验证其他机制：屏蔽基础回血漂移
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::new(d60(5.0), Fix64::ZERO);
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::ZERO;
        world.players[1].move_target = None;
        world.players[1].hp = Fix64::from_num(50.0);
        world.players[1].mastery[0] = 2; // 生命精通 2 级 → 吸血 16%
        world.step(vec![
            PlayerInput::default(),
            PlayerInput { cast: Some((SkillId::TestLightning, Some(Vec2::new(d60(5.0), Fix64::ZERO)))), ..Default::default() },
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..20 {
            if world.players[1].hp > Fix64::from_num(50.0) { break; }
            world.step(none.clone(), dt);
        }
        let healed = (world.players[1].hp - Fix64::from_num(50.0)).to_num::<f64>();
        // 闪电 10 伤 × 16% = 1.6
        assert!((healed - 1.6).abs() < 0.25, "2 级生命精通应吸血 16%×10=1.6，实际 {healed}");
    }

    /// 时间精通 ei：弹体寿命缩放（火球系 +15%/级）——射程随之变长。
    #[test]
    fn mastery_ei_extends_projectile_life() {
        let mut world = World::new(1, 982);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[0].caster = crate::skill::Caster::new();
        world.players[0].mastery[2] = 2; // 时间精通 2 级 → 火球寿命 ×1.3
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S000, Some(Vec2::new(d60(20.0), Fix64::ZERO)))), ..Default::default() },
        ], dt);
        let life = world
            .projectiles
            .iter()
            .find_map(|pr| match pr.kind {
                ProjectileKind::W098b { life, .. } => Some(life.to_num::<f64>()),
                _ => None,
            })
            .expect("应有火球弹体");
        assert!((life - 1.3).abs() < 1e-3, "2 级时间精通火球寿命应 1.0×1.3=1.3s，实际 {life}");
    }

    /// 远程精通 xi：xi>0 火球获得落点爆炸（到点无目标也炸），xi=0 无爆炸。
    #[test]
    fn mastery_xi_gives_fireball_ground_blast() {
        let dt = Fix64::from_num(1.0 / 60.0);
        // 场景：施法者站场边 (-600,0) 朝场内射，弹体飞行 1000 码在 (400,0) 到点消失——
        // xi>0 时在落点爆炸；观察者在弹道侧面 (400,120)（爆炸半径 45×√15≈173 内），全程在场内。
        let mut world = World::new(2, 983);
        world.obstacles.clear();
        world.players[0].pos = Vec2::new(-d60(10.0), Fix64::ZERO);
        world.players[0].move_target = None;
        world.players[0].mastery[1] = 1; // 远程精通 1 级
        world.players[1].pos = Vec2::new(d60(6.0), d60(2.0));
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S000, Some(Vec2::new(d60(10.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..90 {
            world.step(none.clone(), dt);
        }
        let d1 = (hp1 - world.players[1].hp).to_num::<f64>();
        assert!(d1 > 1.0, "xi 爆炸应波及弹道侧 120 码的观察者（半径 ≈173），实际 {d1}");
        // 对照：xi=0 → 无落点爆炸，观察者不应受伤
        let mut world2 = World::new(2, 983);
        world2.obstacles.clear();
        world2.players[0].pos = Vec2::new(-d60(10.0), Fix64::ZERO);
        world2.players[0].move_target = None;
        world2.players[1].pos = Vec2::new(d60(6.0), d60(2.0));
        world2.players[1].move_target = None;
        let hp1b = world2.players[1].hp;
        world2.step(vec![
            PlayerInput { cast: Some((SkillId::S000, Some(Vec2::new(d60(10.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..90 {
            world2.step(none.clone(), dt);
        }
        assert_eq!(world2.players[1].hp, hp1b, "xi=0 无爆炸，观察者不应受伤");
    }

    /// 击退合成：精通每级 -2.5% 与属性/物品乘法合成（098c kf Hn 公式）。
    #[test]
    fn mastery_kb_reduction_composes_multiplicatively() {
        let mut world = World::new(1, 984);
        let p = &mut world.players[0];
        assert!((p.effective_kb_reduction() - 0.0).abs() < 1e-9, "无属性无精通应为 0");
        p.mastery = [2, 0, 0]; // lf=2 → -5%
        assert!((p.effective_kb_reduction() - 0.05).abs() < 1e-9);
        p.mastery = [3, 3, 3]; // lf=9 → 1-(0.775)=22.5%？(1-0.225)=0.775
        let got = p.effective_kb_reduction();
        assert!((got - 0.225).abs() < 1e-9, "9 级精通应 -22.5%，实际 {got}");
        // 与物品合成：头盔 32% + 精通 22.5% → 1-(0.68×0.775)=47.3%
        p.set_items(&[crate::item::ItemId::Helm3]);
        let got2 = p.effective_kb_reduction();
        assert!((got2 - 0.473).abs() < 1e-9, "物品×精通应乘法合成 47.3%，实际 {got2}");
    }
    // ===== B2 队伍系统（098c cn[]，D13 #18） =====

    /// 同队技能免疫：弹体/nova 只命中异队（nearest_hit/explode_at 团队过滤）。
    #[test]
    fn team_projectile_and_nova_spare_teammates() {
        let mut world = World::new(3, 987);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].team = 0;
        world.players[1].team = 0; // 队友挡在弹道上
        world.players[2].team = 1; // 敌人在队友身后
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(3.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.players[2].pos = Vec2::new(d60(6.0), Fix64::ZERO);
        world.players[2].move_target = None;
        let (hp1, hp2) = (world.players[1].hp, world.players[2].hp);
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S000, Some(Vec2::new(d60(6.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default(), PlayerInput::default()];
        for _ in 0..40 {
            world.step(none.clone(), dt);
        }
        assert_eq!(world.players[1].hp, hp1, "同队队友应被火球穿透（免疫）");
        assert!(world.players[2].hp < hp2, "异队敌人应被火球命中");
    }

    /// 天罚 nova 同队免伤 + 队伍回合判定：全活人同队即 round_over。
    #[test]
    fn team_round_over_when_one_side_left() {
        let mut world = World::new(4, 988);
        world.base_regen = 0.0; // 本测试只验证其他机制：屏蔽基础回血漂移
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].team = 0;
        world.players[1].team = 0;
        world.players[2].team = 1;
        world.players[3].team = 1;
        // 施法者归原点；队 1 两人站在其天罚范围内
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[2].pos = Vec2::new(d60(2.0), Fix64::ZERO);
        world.players[2].move_target = None;
        world.players[3].pos = Vec2::new(d60(2.0), d60(1.0));
        world.players[3].move_target = None;
        let (hp0, hp1, hp2) = (world.players[0].hp, world.players[1].hp, world.players[2].hp);
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S001, None)), ..Default::default() },
            PlayerInput::default(),
            PlayerInput::default(),
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(); 4];
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        // 施法者吃 FX 自伤 10；同队队友免疫；异队受伤
        assert!(near(world.players[0].hp, hp0.to_num::<f64>() - 10.0, 0.1), "施法者应自伤 10");
        assert_eq!(world.players[1].hp, hp1, "同队队友不应被天罚伤");
        assert!(world.players[2].hp < hp2, "异队应被天罚伤");
        // 队 1 全灭（模拟淘汰）→ 存活全属队 0 → round_over 且胜者为队 0 成员
        for p in world.players.iter_mut().skip(2) {
            p.alive = false;
        }
        assert!(world.round_over(), "存活方仅剩一队应判回合结束");
        let winners = world.round_winners();
        assert_eq!(winners, vec![0, 1], "胜者应为存活队全员");
    }

    // ===== B3 模式系统（098c nn，D13 #1） =====

    /// 死亡竞赛（模式 2）：死亡 4 秒后满血复活（098c so=4s）。
    #[test]
    fn dm_respawns_after_four_seconds() {
        let mut world = World::new(2, 990);
        world.obstacles.clear();
        world.configure_mode(2);
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(2.0), Fix64::ZERO);
        world.players[1].move_target = None;
        // 直接处决玩家 1（走 record_death 的 DM 钩子）——用天罚快速杀
        world.players[1].hp = Fix64::from_num(5.0);
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S001, None)), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..50 {
            world.step(none.clone(), dt);
        }
        assert!(!world.players[1].alive, "5 血应被天罚击杀（4s 内未复活）");
        // 推进到 4s+（240 帧）
        for _ in 0..250 {
            if world.players[1].alive {
                break;
            }
            world.step(none.clone(), dt);
        }
        assert!(world.players[1].alive, "DM 应在 4 秒后复活");
        assert_eq!(world.players[1].hp, world.players[1].max_hp, "复活应满血");
    }

    /// LMS（模式 5）：凶手死亡 → 其受害者复活；存活者同凶即 round_over。
    #[test]
    fn lms_killer_death_revives_victims_and_round_detects() {
        let mut world = World::new(3, 991);
        world.base_regen = 0.0; // 本测试只验证其他机制：屏蔽基础回血漂移
        world.obstacles.clear();
        world.configure_mode(5);
        let dt = Fix64::from_num(1.0 / 60.0);
        // 玩家 0/1 已死，凶手都是玩家 2
        world.players[0].alive = false;
        world.players[0].last_hit_by = Some(2);
        world.players[1].alive = false;
        world.players[1].last_hit_by = Some(2);
        world.players[2].alive = true;
        // 存活者（玩家 2）唯一 → 需要另一存活者且同凶手才判 LMS 结束：
        // 让玩家 0 复活调度先不触发，构造「存活者均被 2 杀」：复活 0（受害者）
        world.step(vec![PlayerInput::default(), PlayerInput::default(), PlayerInput::default()], dt);
        assert!(!world.players[0].alive && !world.players[1].alive, "无凶手死亡时不应复活");
        // 凶手 2 死亡（岩浆/直接标记走 record_death——用 explode 路径简化：直接调用内部不可行，改用出界）
        world.players[2].pos = Vec2::new(d60(100.0), Fix64::ZERO); // 远抛场外
        let inputs3 = vec![PlayerInput::default(), PlayerInput::default(), PlayerInput::default()];
        for _ in 0..700 {
            world.step(inputs3.clone(), dt);
            if world.players[2].hp <= Fix64::ZERO {
                break;
            }
        }
        assert!(!world.players[2].alive, "出界应烧死凶手 2");
        // 受害者（被 2 杀的 0/1）应在 3 秒后复活
        for _ in 0..220 {
            if world.players[0].alive {
                break;
            }
            world.step(inputs3.clone(), dt);
        }
        assert!(world.players[0].alive && world.players[1].alive, "凶手死后受害者应复活");
    }

    /// 化身/国王：F 槽施法替换 + 角色 buff（set_roles）。
    #[test]
    fn roles_replace_f_slot_and_apply_buffs() {
        let mut world = World::new(3, 992);
        world.base_regen = 0.0; // 本测试只验证其他机制：屏蔽基础回血漂移
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        world.configure_mode(3);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.set_roles(Some(0), &[]);
        assert_eq!(world.f_override[0], Some(SkillId::S020), "化身 F 应替换为灾变");
        assert!((world.players[0].dmg_taken_mult - 1.0).abs() < 1e-9);
        // 国王：替换虔诚 + 受伤/岩浆 ×0.9
        world.set_roles(None, &[1]);
        assert_eq!(world.f_override[1], Some(SkillId::S021), "国王 F 应替换为虔诚");
        assert!((world.players[1].dmg_taken_mult - 0.9).abs() < 1e-9);
        assert!((world.players[1].lava_taken_mult - 0.9).abs() < 1e-9);
        // 替换后施放 S001 实际执行 S020（灾变三级第一段）：对 250 内敌人造成 12+4×stage
        world.set_roles(Some(0), &[]);
        // 化身独占队 1（098c `cn[FV]=1`），其余人队 0 → 目标须在队 0 才是敌人
        world.players[1].team = 0;
        world.players[1].pos = Vec2::new(d60(2.0), Fix64::ZERO);
        world.players[1].move_target = None;
        let hp1 = world.players[1].hp;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S001, None)), ..Default::default() },
            PlayerInput::default(),
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default(), PlayerInput::default()];
        for _ in 0..50 {
            world.step(none.clone(), dt);
        }
        let d1 = (hp1 - world.players[1].hp).to_num::<f64>();
        // 化身 Gn ×1.5：灾变 stage0 基础 11、加法衰减 `-d/60`（d=120 → -2）→ (11-2)×1.5 = 13.5
        assert!((d1 - 13.5).abs() < 0.5, "化身灾变 stage0 应 (11-120/60)×Gn1.5=13.5 伤，实际 {d1}");
    }

    /// 化身角色（098c `Bf`）：**独占一队 vs 其余人互为盟友** + 按人数缩放。
    #[test]
    fn avatar_role_teams_and_scaling() {
        let mut world = World::new(4, 1002);
        world.configure_mode(3);
        world.set_roles(Some(2), &[]);
        // 队伍：化身 cn=1，其余 cn=0（互为盟友）
        assert_eq!(world.players[2].team, 1, "化身应独占队 1");
        for i in [0usize, 1, 3] {
            assert_eq!(world.players[i].team, 0, "其余人应为队 0（盟友）");
        }
        let p = &world.players[2];
        // 半径 50；Gn ×1.5；jn ×1.2
        assert!((p.radius.to_num::<f64>() - 50.0).abs() < 1e-6);
        assert!((p.growth - 1.5).abs() < 1e-9);
        assert!((p.dur_mult - 1.2).abs() < 1e-9);
        // In *= (1+n/2) → 回血倍率 3.0；Hn /= (n/1.5) → 角色击退乘子 1.5/4 = 0.375
        assert!((p.role_regen_mult - 3.0).abs() < 1e-9, "回血倍率应 1+4/2=3");
        assert!((p.role_kb_mult - 0.375).abs() < 1e-9, "击退乘子应 1.5/4=0.375");
        // 受击退 = 1 - Hn = 62.5%（无物品/精通时）
        assert!((p.effective_kb_reduction() - 0.625).abs() < 1e-9, "化身应少受击退 62.5%");
        // 国王同样调用 set_roles 时不应改动队伍
        world.set_roles(None, &[0]);
        assert_eq!(world.players[2].team, 1, "非化身轮的队伍保持不变");
        assert!((world.players[0].dmg_taken_mult - 0.9).abs() < 1e-9);
    }

    /// 国王模式：每队随机选王（同种子确定性）；弑王 → 凶手全队 Doom。
    #[test]
    fn king_mode_roll_and_doom() {
        let mut world = World::new(4, 993);
        world.base_regen = 0.0; // 本测试只验证其他机制：屏蔽基础回血漂移
        for (i, p) in world.players.iter_mut().enumerate() {
            p.team = if i < 2 { 0 } else { 1 };
        }
        world.configure_mode(4);
        world.round_number = 2;
        world.roll_roles_for_test();
        // 同 seed 掷两次结果一致
        let kings = world.pending_kings.clone();
        assert_eq!(kings.len(), 2, "两队各一王");
        assert_ne!(kings[0] % 2, kings[1] % 2, "两王应分属两队");
        world.reset_round();
        assert_eq!(world.kings, kings, "reset_round 应应用掷出的王");
        assert_eq!(world.f_override[kings[0] as usize], Some(SkillId::S021));
        // 弑王：队 1 的王被杀 → **王所在队伍（队 1）**存活成员 Doom（098c `AI` nn==4：`In -= 1`）
        let victim = kings.iter().find(|&&k| world.players[k as usize].team == 1).copied().unwrap();
        world.players[victim as usize].last_hit_by = Some(0);
        world.record_death(victim);
        assert_eq!(world.players[victim as usize].doom, 0.0, "死者本身（王）不因自己死亡得 Doom");
        let teammates: Vec<usize> = (0..4).filter(|&i| i != victim as usize && world.players[i].team == 1).collect();
        for &i in &teammates {
            // `In -= 1`（0.1s 刻度）⇒ -10 HP/s，持续 50s（`LO(function II, 50, ...)` 后恢复）
            assert!((world.players[i].doom - 10.0).abs() < 1e-9, "王所在队伍的存活队友应 Doom -10 HP/s");
            assert!(
                (world.players[i].doom_remaining - Fix64::from_num(50.0)).abs() < Fix64::from_num(1e-6),
                "Doom 应持续 50s"
            );
        }
        assert_eq!(world.players[0].doom, 0.0, "凶手（队0）不应得 Doom");
    }

    /// 助攻（098c `AI`/`Jn`）：对死者伤害最高且 >0、非凶手者 = 唯一助攻；凶手/无伤害则无。
    #[test]
    fn assist_is_top_damager_excluding_killer() {
        let mut world = World::new(4, 995);
        // 玩家1 被 0/2/3 都伤过；凶手=0，2 伤害最高 -> 助攻=2
        world.damage_matrix[0][1] = Fix64::from_num(9.0);
        world.damage_matrix[2][1] = Fix64::from_num(20.0);
        world.damage_matrix[3][1] = Fix64::from_num(4.0);
        assert_eq!(world.assist_damager_of(1, 0), Some(2), "助攻应取非凶手的最高伤害者");
        // 只有凶手伤过 -> 无助攻
        world.damage_matrix[2][1] = Fix64::ZERO;
        world.damage_matrix[3][1] = Fix64::ZERO;
        assert_eq!(world.assist_damager_of(1, 0), None, "只有凶手时无助攻");
        // 无任何伤害 -> 无助攻
        world.damage_matrix[0][1] = Fix64::ZERO;
        assert_eq!(world.assist_damager_of(1, 0), None);
    }

    /// 跃退公式含 Gn[攻]×hn[受]：攻方 Gn 越高、受方 hn 越低，击退越大。
    #[test]
    fn knockback_scales_with_attacker_gn_and_victim_hn() {
        // 直接验证公式（同 mana/gx/ji，只变 gn/hn）
        let base = super::warlock_ki_knockback(0.0, Fix64::ONE, Fix64::ONE, 1.0, 1.0);
        let big_gn = super::warlock_ki_knockback(0.0, Fix64::ONE, Fix64::ONE, 2.0, 1.0);
        let low_hn = super::warlock_ki_knockback(0.0, Fix64::ONE, Fix64::ONE, 1.0, 0.5);
        assert!((big_gn - base * Fix64::from_num(2.0)).abs() < Fix64::from_num(1e-6));
        assert!((low_hn - base * Fix64::from_num(0.5)).abs() < Fix64::from_num(1e-6));
    }

    /// 每轮开局 Gn = 0.5（098c XI）。
    #[test]
    fn round_start_gn_is_half() {
        let mut world = World::new(2, 996);
        world.players[0].growth = 3.3;
        world.players[0].reset_state();
        assert!((world.players[0].growth - 0.5).abs() < 1e-9, "重生/轮开局 Gn 应为 0.5");
    }

    /// 化身模式（098c `fI`）：加冕取**累计**伤害积分 `JV` 最大者；化身被杀死时其积分清零。
    #[test]
    fn avatar_rolls_to_top_cumulative_damager_and_resets_on_death() {
        let mut world = World::new(3, 994);
        world.configure_mode(3);
        // 积分在**伤害结算处**累加（见 `damage_player`），此处直接给定累计值：
        // 玩家 1 累计 42、玩家 0 累计 5、玩家 2 为 0
        world.avatar_score[1] = Fix64::from_num(42.0);
        world.avatar_score[0] = Fix64::from_num(5.0);
        world.reset_round();
        assert_eq!(world.avatar, Some(1), "化身应为累计伤害最高者（玩家1）");
        assert!((world.players[1].dmg_taken_mult - 1.0).abs() < 1e-9);
        assert_eq!(world.f_override[1], Some(SkillId::S020), "化身 F 应替换为灾变");
        assert_eq!(world.f_override[0], None, "非化身不应有 F 替换");
        // 加冕后把每个人的「最后伤害者」预设为化身（098c `Bf`：`An[i]=FV`）
        assert_eq!(world.players[0].last_hit_by, Some(1), "环境伤害应记在化身头上");
        assert_eq!(world.players[1].last_hit_by, Some(1), "化身自身的环境死算自杀");

        // 玩家 2 累计反超 → 下轮由玩家 2 加冕；玩家 1 的累计保留不重置
        world.avatar_score[2] = Fix64::from_num(50.0);
        world.reset_round();
        assert_eq!(world.avatar, Some(2), "累计反超者应加冕");
        assert!((world.avatar_score[1] - Fix64::from_num(42.0)).abs() < Fix64::from_num(1e-3));

        // 化身（玩家 2）被杀死 → 其累计清零 → 下一轮由次高的玩家 1 加冕
        world.record_death(2);
        assert!(world.avatar_score[2].abs() < Fix64::from_num(1e-6), "化身死亡应清零其累计积分");
        world.reset_round();
        assert_eq!(world.avatar, Some(1), "化身积分清零后应由次高者加冕");
    }

    /// 累计积分由**实际造成的伤害**驱动（098c `Rn`），环境伤害不计入。
    #[test]
    fn avatar_score_accumulates_from_dealt_damage_only() {
        let mut world = World::new(2, 1004);
        world.configure_mode(3);
        world.base_regen = 0.0;
        world.players[1].hp = Fix64::from_num(100.0);
        world.damage_player(1, Fix64::from_num(7.0), Some(0));
        assert!(
            (world.avatar_score[0] - Fix64::from_num(7.0)).abs() < Fix64::from_num(1e-3),
            "玩家0 应累计 7，实际 {:?}",
            world.avatar_score[0]
        );
        assert!(world.avatar_score[1].abs() < Fix64::from_num(1e-6), "受害者不应累计");
        // 出界（岩浆）伤害不计入积分，但会把一半记进归属矩阵
        world.players[1].pos = Vec2::new(d60(99.0), d60(99.0));
        world.players[1].last_hit_by = Some(0);
        let before = world.avatar_score[0];
        for _ in 0..30 {
            world.step(vec![PlayerInput::default(), PlayerInput::default()], Fix64::from_num(1.0 / 60.0));
        }
        assert!(
            (world.avatar_score[0] - before).abs() < Fix64::from_num(1e-3),
            "环境伤害不应计入化身积分"
        );
        assert!(
            world.damage_matrix[0][1] > Fix64::ZERO,
            "岩浆伤害的一半应记入归属矩阵（098c `Jn[受][An] += To/2`）"
        );
    }
    // ===== B4 形态切换（098c sC，D13 #7） =====

    /// S012 A 燃烧冲刺撞敌人：命中 + **自伤**（`FX` 不经 Gn）+ 熄灭（098c `CA` 的 `Hr[nr]` 分支）。
    #[test]
    fn s012_burning_dash_self_damage_on_enemy_contact() {
        let mut w = World::new(2, 777);
        w.obstacles.clear();
        w.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        w.players[0].team = 0;
        w.players[1].team = 1;
        w.players[0].pos = Vec2::ZERO;
        w.players[0].move_target = None;
        w.players[1].pos = Vec2::new(d60(4.0), Fix64::ZERO);
        w.players[1].move_target = None;
        let hp0_before = w.players[0].hp;
        w.step(vec![
            PlayerInput { cast: Some((SkillId::S012, Some(Vec2::new(d60(6.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        let mut burned = false;
        for _ in 0..120 {
            if w.players[0].burning {
                burned = true;
            }
            if w.players[0].kick.is_none() {
                break;
            }
            w.step(none.clone(), dt);
        }
        assert!(burned, "S012 A 冲刺期间应处于燃烧状态");
        assert!(w.players[0].hp < hp0_before, "燃烧冲刺撞敌人应自伤（FX）");
        assert!(!w.players[0].burning, "命中后应烧尽（熄灭）");
    }

    /// S010 双形态：冲锋（A）=移速 buff+接触踢击；隐身（B）=隐身+较慢移速。
    #[test]
    fn s010_form_a_charge_vs_b_invisibility() {
        let mut world = World::new(1, 996);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        // B 形态（隐身）：Stealth buff + 100 速
        world.players[0].forms[SkillId::S010.as_u32() as usize] = true;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S010, None)), ..Default::default() },
        ], dt);
        assert!(world.players[0].has_buff(BuffKind::Stealth), "B 形态应有隐身");
        // A 形态（冲锋）：098c RB 两形态都挂 'Agho' 隐身（w3a_strings.txt:1265
        // 「Wind Walk: Charge - Invisibility」；war3map_pretty.j:5781-5804），故冲锋也隐身，同时有踢击窗口。
        let mut world2 = World::new(1, 996);
        world2.obstacles.clear();
        world2.sandbox = true;
        world2.step(vec![
            PlayerInput { cast: Some((SkillId::S010, None)), ..Default::default() },
        ], dt);
        assert!(world2.players[0].has_buff(BuffKind::Stealth), "A 形态（冲锋）也应有隐身（098c RB）");
        assert!(world2.players[0].charging, "A 形态应置 `charging`（098c `fr`）");
        assert!(!world.players[0].charging, "B 形态不应置 `charging`");
        assert!(world2.players[0].kick.is_some(), "A 形态应有接触踢击窗口");
        let kick_dmg = world2.players[0].kick.as_ref().unwrap().push_damage.to_num::<f64>();
        assert!((kick_dmg - 5.4).abs() < 0.1, "冲锋踢击伤害应 5.4+0.2947L（L1=5.4，098c 背刺），实际 {kick_dmg}");
    }

    /// S010A 冲锋的额外 AoE（098c `bA`）**由范围精通门控**：`xi[id]>0` 才以攻方为圆心再放一个 AoE。
    /// 未点精通只有基础接触伤害；点了精通额外 AoE（半径 160×(1+.12xi)、边缘衰减）——对同一目标按距离打折，故**略小于两倍**。
    #[test]
    fn s010_break_strike_requires_range_mastery() {
        let run = |range_mastery: u8| -> Fix64 {
            let mut world = World::new(2, 9591);
            world.obstacles.clear();
            world.sandbox = true;
            let dt = Fix64::from_num(1.0 / 60.0);
            world.players[0].pos = Vec2::ZERO;
            world.players[0].move_target = None;
            world.players[1].pos = Vec2::new(d60(1.0), Fix64::ZERO); // 初始未接触
            world.players[1].move_target = None;
            world.players[0].mastery = [0, range_mastery, 0]; // mastery[1] = 远程精通(xi)
            world.step(vec![
                PlayerInput { cast: Some((SkillId::S010, None)), ..Default::default() },
                PlayerInput::default(),
            ], dt);
            // 走进敌人（kick 窗口内必然接触）；命中后 kick 被消耗即退出。
            world.players[0].move_target = Some(world.players[1].pos);
            let none = vec![PlayerInput::default(), PlayerInput::default()];
            for _ in 0..40 {
                if world.players[0].kick.is_none() {
                    break;
                }
                world.step(none.clone(), dt);
            }
            world.players[1].hp
        };
        let hp_no_mastery = run(0);
        let hp_with_mastery = run(1);
        let d_plain = Fix64::from_num(100.0) - hp_no_mastery;
        let d_bonus = Fix64::from_num(100.0) - hp_with_mastery;
        assert!(d_plain > Fix64::ZERO, "无精通也应有基础接触伤害，实际 {:?}", d_plain);
        assert!(d_bonus > d_plain,
            "有范围精通应追加 `bA` AoE：{:?} 应 > {:?}", d_bonus, d_plain);
        // `bA` 是以攻方为圆心的 AoE（半径 160×(1+.12xi)，边缘衰减）：命中同一目标时按距离打折，略小于 2 倍。
        assert!(d_bonus < d_plain * Fix64::from_num(1.99),
            "AoE 对同一目标有距离衰减，应 < 2 倍：{:?} vs {:?}", d_bonus, d_plain);
        assert!(d_bonus > d_plain * Fix64::from_num(1.3),
            "有范围精通应有明显的额外 AoE 伤害：{:?} vs {:?}", d_bonus, d_plain);
    }

    /// S009 双形态：目标（A）到点碎裂出弹片；区域（B）飞行中持续撒侧弹。
    #[test]
    fn s009_form_splitter_target_burst_and_area_emit() {
        // A 形态：朝远处射 → 到点碎裂出 6 枚弹片
        let mut world = World::new(1, 997);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S009, Some(Vec2::new(d60(8.0), Fix64::ZERO)))), ..Default::default() },
        ], dt);
        let none = vec![PlayerInput::default()];
        let mut max_bullets = 0usize;
        for _ in 0..135 {
            world.step(none.clone(), dt);
            let alive_now = world.projectiles.iter().filter(|p| p.alive && matches!(p.kind, ProjectileKind::W098b { .. })).count();
            max_bullets = max_bullets.max(alive_now);
        }
        assert!(max_bullets >= 4, "目标形态到点应碎裂出多枚弹片（主弹+弹片），窗口内峰值 {max_bullets}");
        // B 形态：区域慢速大弹 + 螺旋侧弹
        let mut world2 = World::new(1, 997);
        world2.obstacles.clear();
        world2.sandbox = true;
        world2.players[0].forms[SkillId::S009.as_u32() as usize] = true;
        world2.players[0].pos = Vec2::ZERO;
        world2.players[0].move_target = None;
        world2.step(vec![
            PlayerInput { cast: Some((SkillId::S009, Some(Vec2::new(d60(8.0), Fix64::ZERO)))), ..Default::default() },
        ], dt);
        let none = vec![PlayerInput::default()];
        let mut max_bullets2 = 0usize;
        for _ in 0..40 {
            world2.step(none.clone(), dt);
            let alive_now = world2.projectiles.iter().filter(|p| p.alive && matches!(p.kind, ProjectileKind::W098b { .. })).count();
            max_bullets2 = max_bullets2.max(alive_now);
        }
        assert!(max_bullets2 >= 3, "区域形态应持续撒出侧弹（主弹+侧弹），窗口内峰值 {max_bullets2}");
    }

    /// S008 岩浆滚石（B 形态）：接触敌人施加「肉饼」减速，寿命尽爆炸。
    #[test]
    fn s008_magma_boulder_pancakes_enemy() {
        let mut world = World::new(2, 998);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[0].forms[SkillId::S008.as_u32() as usize] = true; // B=岩浆
        world.players[1].pos = Vec2::new(d60(2.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.players[1].team = 1;
        // 第三个敌人站在滚石寿命尽头（098c OB: 400/s x 2s = 800 码）附近，验证寿命尽爆炸
        world.players.push(crate::player::Player::new(2, Vec2::new(d60(13.0), d60(0.5)), Fix64::from_num(30.0)));
        let n = world.players.len();
        world.players[2].team = 1;
        world.players[2].move_target = None;
        world.players[2].radius = Fix64::from_num(30.0);
        let _ = n;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S008, Some(Vec2::new(d60(4.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default(), PlayerInput::default()];
        for _ in 0..40 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[1].has_buff(BuffKind::Pancake), "被滚石压过应「肉饼」减速");
        // 滚石 2s 寿命尽爆炸（覆盖寿命 + 爆炸帧）
        for _ in 0..300 {
            if world.players[2].hp < world.players[2].max_hp {
                break;
            }
            world.step(none.clone(), dt);
        }
        assert!(world.players[2].hp < world.players[2].max_hp, "滚石寿命尽爆炸应伤到尽头处的敌人");
    }

    // ===== B4-T 形态机制 =====

    /// S014A 汲取·减速：目标移速 ×0.5、施法者回血伤害×50%。
    #[test]
    fn s014a_drain_slow_and_heal() {
        let mut world = World::new(2, 1001);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::new(d60(3.0), Fix64::ZERO);
        world.players[0].move_target = None;
        world.players[0].team = 1;
        world.players[0].hp = Fix64::from_num(40.0);
        world.players[1].pos = Vec2::ZERO;
        world.players[1].move_target = None;
        world.players[1].team = 0;
        world.step(vec![
            PlayerInput::default(),
            PlayerInput { cast: Some((SkillId::S014, Some(Vec2::new(d60(3.0), Fix64::ZERO)))), ..Default::default() },
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..30 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].has_buff(BuffKind::Slow(0.5)), "目标应被减速 ×0.5");
        let healed = (world.players[1].hp - Fix64::from_num(40.0)).to_num::<f64>();
        assert!(healed > 2.0, "施法者应回血 50%×伤害（≥3），实际 {healed}");
    }

    /// S014B 汲取·削弱：目标伤害输出 ×0.5。
    #[test]
    fn s014b_weaken_halves_output() {
        let mut world = World::new(2, 1002);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].team = 1;
        world.players[0].pos = Vec2::new(d60(3.0), Fix64::ZERO);
        world.players[0].move_target = None;
        world.players[1].team = 0;
        world.players[1].forms[SkillId::S014.as_u32() as usize] = true; // B=削弱（施法者形态位）
        world.players[1].pos = Vec2::ZERO;
        world.players[1].move_target = None;
        world.step(vec![
            PlayerInput::default(),
            PlayerInput { cast: Some((SkillId::S014, Some(Vec2::new(d60(3.0), Fix64::ZERO)))), ..Default::default() },
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..30 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].has_buff(BuffKind::Weakened), "目标应被削弱");
        assert!((world.players[0].gn_factor() - world.players[0].growth * 0.5).abs() < 1e-9, "被削弱者输出应 ×0.5");
    }

    /// S016B 弹跳弹·充能：命中刷新该技能冷却。
    #[test]
    fn s016b_recharge_refreshes_cooldown() {
        let mut world = World::new(2, 1003);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].forms[SkillId::S016.as_u32() as usize] = true;
        world.players[0].team = 0;
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        // 刻意制造冷却：先施放一次进入 CD
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S016, Some(Vec2::new(d60(3.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..30 {
            world.step(none.clone(), dt);
        }
        let cd_after_cast = world.players[0].caster.cooldown_remaining(SkillId::S016).to_num::<f64>();
        assert!(cd_after_cast > 0.0, "施放后应有冷却");
        // 击中敌人（充能形态命中即刷新）
        world.players[0].caster = crate::skill::Caster::new();
        world.players[1].team = 1;
        world.players[1].pos = Vec2::new(d60(2.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S016, Some(Vec2::new(d60(2.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        for _ in 0..30 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[1].hp < world.players[1].max_hp, "充能弹应命中敌人");
        // 刷新后冷却应小于初始 CD 20
        let cd = world.players[0].caster.cooldown_remaining(SkillId::S016).to_num::<f64>();
        assert!(cd < 19.5, "命中应刷新冷却（<19.5），实际 {cd}");
    }

    // ===== B4-Y 形态机制 =====

    /// S017B 沉默：目标禁施法但可移动；S017A 缠绕：目标定身（move_target 被清）。
    #[test]
    fn s017_silence_blocks_cast_tied_blocks_move() {
        let mut world = World::new(2, 1005);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].team = 1;
        world.players[0].pos = Vec2::new(d60(3.0), Fix64::ZERO);
        world.players[0].move_target = None;
        world.players[1].team = 0;
        world.players[1].pos = Vec2::ZERO;
        world.players[1].move_target = None;
        // B 形态沉默
        world.players[1].forms[SkillId::S017.as_u32() as usize] = true;
        world.step(vec![
            PlayerInput::default(),
            PlayerInput { cast: Some((SkillId::S017, Some(Vec2::new(d60(3.0), Fix64::ZERO)))), ..Default::default() },
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..30 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].has_buff(BuffKind::Silenced), "B 形态应施加沉默");
        // 沉默中无法施法
        world.players[0].caster = crate::skill::Caster::new();
        let m0 = world.players[0].move_target;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S001, None)), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        for _ in 0..5 {
            world.step(none.clone(), dt);
        }
        assert!(world.players[0].caster.is_busy() == false, "沉默者施法应被拒（不进入施法状态）");
        let _ = m0;
        // A 形态缠绕：定身（move_target 被清除）
        let mut world2 = World::new(2, 1005);
        world2.obstacles.clear();
        world2.sandbox = true;
        world2.players[0].team = 1;
        world2.players[0].pos = Vec2::new(d60(3.0), Fix64::ZERO);
        world2.players[1].team = 0;
        world2.players[1].pos = Vec2::ZERO;
        world2.players[1].move_target = None;
        world2.step(vec![
            PlayerInput::default(),
            PlayerInput { cast: Some((SkillId::S017, Some(Vec2::new(d60(3.0), Fix64::ZERO)))), ..Default::default() },
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..30 {
            world2.step(none.clone(), dt);
        }
        assert!(world2.players[0].has_buff(BuffKind::Tied), "A 形态应缠绕（Tied）");
        // 被缠绕者试图移动 → move_target 立即被清（定身）
        world2.players[0].move_target = Some(Vec2::new(d60(20.0), Fix64::ZERO));
        world2.step(vec![
            PlayerInput { set_target: Some(Vec2::new(d60(20.0), Fix64::ZERO)), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        assert!(world2.players[0].move_target.is_none(), "被缠绕者移动目标应被定身清除");
    }

    /// S018B 力场：范围内队友被治疗（heal_team）。
    #[test]
    fn s018b_force_field_heals_allies() {
        let mut world = World::new(3, 1006);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].team = 0;
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[0].forms[SkillId::S018.as_u32() as usize] = true; // B=力场
        world.players[1].team = 0; // 队友在落点
        world.players[1].pos = Vec2::new(d60(3.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.players[1].hp = Fix64::from_num(40.0);
        world.players[2].team = 1; // 敌人也在落点
        world.players[2].pos = Vec2::new(d60(3.5), Fix64::ZERO);
        world.players[2].move_target = None;
        world.players[2].hp = Fix64::from_num(50.0);
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S018, Some(Vec2::new(d60(3.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default(), PlayerInput::default()];
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        let h1 = world.players[1].hp.to_num::<f64>();
        let h2 = world.players[2].hp.to_num::<f64>();
        assert!(h1 > 40.0, "力场应治疗队友（40→↑），实际 {h1}");
        assert!(h2 < 50.0, "力场应伤害敌人（50→↓），实际 {h2}");
    }

    /// S018 引力·力场（B 形态）：范围内敌人被减速 45%（文档「降低移动速度 45%」→ ×0.55）。
    #[test]
    fn s018b_force_field_slows_enemy() {
        let mut world = World::new(2, 1009);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].team = 0;
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[0].forms[SkillId::S018.as_u32() as usize] = true; // B=力场
        world.players[1].team = 1;
        world.players[1].pos = Vec2::new(d60(3.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S018, Some(Vec2::new(d60(3.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..30 {
            world.step(none.clone(), dt);
        }
        assert!(
            world.players[1].has_buff(BuffKind::Slow(0.55)),
            "力场应给范围内敌人减速（×0.55 = -45%）"
        );
    }

    /// S019B 红链（文档「红链」）：命中敌人 → 把**施法者**拉向敌人
    /// （与 A 蓝链「拉目标向施法者」方向相反，见 `s019_chain_pulls_target_toward_caster`）。
    #[test]
    fn s019b_red_chain_pulls_caster_to_enemy() {
        let mut world = World::new(2, 1007);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].team = 0;
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[0].forms[SkillId::S019.as_u32() as usize] = true; // B=红链
        world.players[1].team = 1;
        world.players[1].pos = Vec2::new(d60(4.0), Fix64::ZERO); // 敌人在 +x
        world.players[1].move_target = None;
        let x_before = world.players[0].pos.x;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S019, Some(Vec2::new(d60(4.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        assert!(
            world.players[0].pos.x > x_before,
            "红链应把施法者拉向敌人（+x），{} -> {}",
            x_before,
            world.players[0].pos.x
        );
    }

    // ===== B4-R 形态机制 =====

    /// S013B 搬运：施法者被搬到目标点（不与敌人换位）。
    #[test]
    fn s013b_relocate_carries_caster() {
        let mut world = World::new(2, 1008);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].team = 0;
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[0].forms[SkillId::S013.as_u32() as usize] = true; // B=搬运
        world.players[1].team = 1;
        world.players[1].pos = Vec2::new(d60(4.0), Fix64::ZERO);
        world.players[1].move_target = None;
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S013, Some(Vec2::new(d60(8.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        // 搬运现在是**弹体**（098c `pB`）：飞抵落点后才把施法者传送过去，故需推进若干帧。
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..60 {
            world.step(none.clone(), dt);
        }
        let d = world.players[0].pos.length().to_num::<f64>();
        assert!(d > 300.0, "搬运应把施法者搬到远处（>300），实际 {d}");
        // 对照 A 形态（置换）：点空地时也是自己瞬移，但点敌人时换位——本测试验证 B 后不互换
    }

    /// S013B 搬运（098c `pB` tooltip）：弹体撞到**非术士障碍物**时，施法者与该障碍物互换位置
    /// （否则弹体被柱子挡下，施法者永远到不了）。
    #[test]
    fn s013b_relocate_bolt_hitting_obstacle_carries_caster() {
        let mut world = World::new(2, 1011);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].team = 0;
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[0].forms[SkillId::S013.as_u32() as usize] = true; // B=搬运
        world.players[1].team = 1;
        world.players[1].pos = Vec2::new(d60(30.0), d60(30.0));
        world.players[1].move_target = None;
        world.obstacles.push(Obstacle::new(Vec2::new(d60(5.0), Fix64::ZERO), 24.0));
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S013, Some(Vec2::new(d60(8.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let none = vec![PlayerInput::default(), PlayerInput::default()];
        for _ in 0..40 {
            world.step(none.clone(), dt);
        }
        let d = world.players[0].pos.length().to_num::<f64>();
        // 弹体在「柱半径 24 + 弹半径 40」处就被判定撞柱 → 施法者落在柱子前沿（≈240）。
        assert!((200.0..=340.0).contains(&d), "撞柱应把施法者搬到柱子处（≈240~300），实际 {d}");
    }

    /// S012B 凤凰：冲刺中转向 → 转向处发射凤凰弹。
    #[test]
    fn s012b_phoenix_redirect_spawns_missile() {
        let mut world = World::new(2, 1009);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].team = 0;
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = None;
        world.players[0].forms[SkillId::S012.as_u32() as usize] = true; // B=凤凰
        world.players[1].team = 1;
        world.players[1].pos = Vec2::new(d60(6.0), d60(3.0));
        world.players[1].move_target = None;
        // 朝右冲刺，随后转向敌人方向 → 转向处应发凤凰弹
        world.step(vec![
            PlayerInput { cast: Some((SkillId::S012, Some(Vec2::new(d60(8.0), Fix64::ZERO)))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        // 098c `UB`：只有**疾风步状态**（`Fr[gX]`）中移动指令才会发射凤凰弹。
        world.players[0].windwalk_state = Fix64::from_num(3.0);
        world.step(vec![
            PlayerInput { set_target: Some(Vec2::new(d60(6.0), d60(3.0))), ..Default::default() },
            PlayerInput::default(),
        ], dt);
        let mut spawned = false;
        for _ in 0..40 {
            world.step(vec![PlayerInput::default(), PlayerInput::default()], dt);
            if world.projectiles.iter().any(|p| matches!(p.kind, ProjectileKind::W098b { .. })) {
                spawned = true;
            }
        }
        assert!(spawned, "凤凰转向应发射凤凰弹");
        assert!(world.players[1].hp < world.players[1].max_hp, "凤凰弹/接触应伤害敌人");
    }

    // ===== 冰面系统（098c Idki/YC，冰面批） =====

    /// 冰面确定性生成：同 seed 同布局；50% 概率无冰面。
    #[test]
    fn ice_generation_deterministic() {
        let a = World::new(2, 77);
        let b = World::new(2, 77);
        assert_eq!(a.ice.is_empty(), b.ice.is_empty());
        assert_eq!(a.ice, b.ice, "同 seed 冰面布局应一致");
    }

    /// 冰面滑行：冰面上刹车距离显著更长（抓地 ×0.25）。
    #[test]
    fn ice_slows_braking() {
        let dt = Fix64::from_num(1.0 / 60.0);
        let mk = |ice: bool, seed: u64| {
            let mut w = World::new(1, seed);
            w.obstacles.clear();
            w.sandbox = true;
            if ice {
                // 冰面覆盖全场原点附近
                w.ice = vec![(Vec2::ZERO, Fix64::from_num(3000.0))];
            }
            w.players[0].pos = Vec2::ZERO;
            w.players[0].move_target = Some(Vec2::new(d60(3.0), Fix64::ZERO));
            w
        };
        // 各走 60 帧后下达"停"（无目标），再走 60 帧比较残余位移
        let run = |mut w: World| {
            let none = vec![PlayerInput::default()];
            for _ in 0..60 {
                w.step(none.clone(), dt);
            }
            let p0 = w.players[0].pos;
            w.players[0].move_target = None;
            for _ in 0..60 {
                w.step(none.clone(), dt);
            }
            (w.players[0].pos - p0).length().to_num::<f64>()
        };
        let normal = run(mk(false, 78));
        let icy = run(mk(true, 78));
        assert!(icy > normal * 1.5, "冰面滑行距离应显著大于正常地面（{} vs {}）", icy, normal);
    }

    /// 冰面到达不吸附（D14）：到达目标点后保留动量滑过——位置继续前进一段而非停在目标。
    #[test]
    fn ice_arrival_keeps_momentum() {
        let mut world = World::new(1, 80);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.ice = vec![(Vec2::new(d60(6.0), Fix64::ZERO), Fix64::from_num(3000.0))];
        world.players[0].pos = Vec2::ZERO;
        world.players[0].move_target = Some(Vec2::new(d60(3.0), Fix64::ZERO));
        let none = vec![PlayerInput::default()];
        // 走到目标附近（180 码 @ 满速 210 需约 1s，但冰面加速慢 ×0.25）
        let mut reached = false;
        for _ in 0..300 {
            world.step(none.clone(), dt);
            if world.players[0].move_target.is_none() {
                reached = true;
                break;
            }
        }
        assert!(reached, "应到达目标（move_target 被清除）");
        // 到达后应继续滑行（动量保留）：再走 10 帧，位置应继续前进
        let p_at_arrival = world.players[0].pos;
        for _ in 0..10 {
            world.step(none.clone(), dt);
        }
        let slide = (world.players[0].pos - p_at_arrival).length().to_num::<f64>();
        assert!(slide > 5.0, "冰面到达后应带动量滑过（10 帧滑距 >5），实际 {slide}");
        // 最终应自然停住（摩擦兜底）
        for _ in 0..600 {
            world.step(none.clone(), dt);
        }
        let v_final = world.players[0].cur_vel.length().to_num::<f64>();
        assert!(v_final < 1.0, "冰面摩擦应使角色最终停住，残余速度 {v_final}");
    }

    /// 击退地形阻尼（D14）：冰面被击退滑更远（×0.985 vs ×0.9776）。
    #[test]
    fn knockback_slides_further_on_ice() {
        let dt = Fix64::from_num(1.0 / 60.0);
        let run = |ice: bool| {
            let mut w = World::new(1, 81);
            w.obstacles.clear();
            w.sandbox = true;
            if ice {
                w.ice = vec![(Vec2::ZERO, Fix64::from_num(5000.0))];
            }
            w.players[0].pos = Vec2::ZERO;
            w.players[0].move_target = None;
            w.players[0].push_knockback(Vec2::new(Fix64::from_num(600.0), Fix64::ZERO));
            let none = vec![PlayerInput::default()];
            for _ in 0..300 {
                w.step(none.clone(), dt);
            }
            w.players[0].pos.x.to_num::<f64>()
        };
        let normal = run(false);
        let icy = run(true);
        assert!(icy > normal * 1.3, "冰面击退滑距应显著更远（{} vs {}）", icy, normal);
    }

    /// 冰面不被岩浆侵蚀：冰面上的点即使出圈也不掉血。
    #[test]
    fn ice_immune_to_lava() {
        let mut world = World::new(1, 79);
        world.obstacles.clear();
        let dt = Fix64::from_num(1.0 / 60.0);
        // 冰面中心放在远处（出圈处），玩家站上面
        world.ice = vec![(Vec2::new(d60(20.0), Fix64::ZERO), Fix64::from_num(200.0))];
        world.players[0].pos = Vec2::new(d60(20.0), Fix64::ZERO);
        world.players[0].move_target = None;
        let hp = world.players[0].hp;
        let none = vec![PlayerInput::default()];
        for _ in 0..120 {
            world.step(none.clone(), dt);
        }
        assert!(world.on_ice(world.players[0].pos), "玩家应站在冰面上");
        assert_eq!(world.players[0].hp, hp, "冰面应免疫岩浆（不掉血）");
    }

    /// 回归：持续伤害（DoT）**不得**触发 `Gn` 伤害成长。
    ///
    /// 旧 bug：引力场/力场等每帧 `damage_per_sec*dt` 都走 `damage_player` → `Gn×1.1`，
    /// 60Hz 下 `1.1^60 ≈ 300/秒` 指数爆炸，几帧内秒杀满血。
    /// 098c 的 `Gn×1.1` 由 `bv[弹体]` 门控（每发弹体命中一次），DoT 不算。
    #[test]
    fn dot_damage_does_not_grow_gn_or_oneshot() {
        use crate::skill::DefTable;
        // 数值锚：A 形态换算后 DPS ≈ 5.0（L1：每 tick 0.3 ÷ 0.06）；B 力场 2.25/s（098c 原值）。
        let a = DefTable::def(SkillId::S018).growth.stats(1);
        assert!((a.damage.to_num::<f64>() - 5.0).abs() < 1e-2, "暗物质 L1 DPS 应 ≈ 5");
        let b = DefTable::def_alt(SkillId::S018).unwrap().growth.stats(1);
        assert!((b.damage.to_num::<f64>() - 2.25).abs() < 1e-6, "力场 L1 应为 2.25/s");

        // A 形态：坤满 5s（引力场在场），Gn 不变、总伤害有界（不得秒杀）。
        let mut world = World::new(2, 7);
        world.obstacles.clear();
        world.sandbox = true;
        let dt = Fix64::from_num(1.0 / 60.0);
        world.players[0].pos = Vec2::ZERO;
        world.players[0].team = 0;
        world.players[0].move_target = None;
        world.players[1].pos = Vec2::new(d60(4.0), Fix64::ZERO);
        world.players[1].team = 1;
        world.players[1].move_target = None;
        let growth0 = world.players[0].growth;
        world.step(vec![PlayerInput { cast: Some((SkillId::S018, Some(Vec2::new(d60(4.0), Fix64::ZERO)))), ..Default::default() }, PlayerInput::default()], dt);
        for _ in 0..300 {
            world.players[1].move_target = None;
            world.players[1].pos = Vec2::new(d60(4.0), Fix64::ZERO); // 钉在伤害半径（274）内
            world.step(vec![PlayerInput::default(), PlayerInput::default()], dt);
        }
        assert_eq!(world.players[0].growth, growth0, "DoT 不应改变攻方 Gn（伤害成长）");
        let lost = 100.0 - world.players[1].hp.to_num::<f64>();
        assert!(lost > 1.0 && lost < 40.0, "暗物质 5s 总伤害应有界且非零（实测 {lost}）");

        // B 形态（力场）：同样不得涨 Gn / 秒杀。
        let mut world = World::new(2, 8);
        world.obstacles.clear();
        world.sandbox = true;
        world.players[0].pos = Vec2::ZERO;
        world.players[0].team = 0;
        world.players[0].move_target = None;
        world.players[0].forms[SkillId::S018.as_u32() as usize] = true; // B=力场
        world.players[1].pos = Vec2::new(d60(3.0), Fix64::ZERO);
        world.players[1].team = 1;
        world.players[1].move_target = None;
        let growth0 = world.players[0].growth;
        world.step(vec![PlayerInput { cast: Some((SkillId::S018, Some(Vec2::new(d60(3.0), Fix64::ZERO)))), ..Default::default() }, PlayerInput::default()], dt);
        for _ in 0..300 {
            world.players[1].move_target = None;
            world.players[1].pos = Vec2::new(d60(3.0), Fix64::ZERO); // 钉在场内，满 5s
            world.step(vec![PlayerInput::default(), PlayerInput::default()], dt);
        }
        assert_eq!(world.players[0].growth, growth0, "力场 DoT 不应改变攻方 Gn");
        let lost = 100.0 - world.players[1].hp.to_num::<f64>();
        assert!(lost < 30.0, "力场 5s 总伤害应有界（实测 {lost}）");
    }
}
