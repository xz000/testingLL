//! 技能系统 —— 定义与施法状态机。
//!
//! 设计基线（已确认）：
//! - 技能有 **前摇 windup** 与 **后摇 recovery**，均影响结算：前摇期间被打断则施法失败
//! - 等级成长：`实际数值 = 基础值 + 成长系数 × (等级-1)`（可后期调斜率）
//! - 冷却 / 点目标施法；效果由 `World` 在施法完成时执行
//!
//! 初期实现树：C(位移/自保)、R(近战推/闪身)、E(远程)。

use crate::fix::{Fix64, Vec2};

/// 技能树（对应原版按键：C/D/E/F/G/R/T/Y）。
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum SkillTree {
    C,
    R,
    E,
    D,
    Y,
    T,
    F,
    G,
}

impl SkillTree {
    /// 技能树的中文名（UI 显示）。
    pub fn name_zh(self) -> &'static str {
        match self {
            SkillTree::C => "身法",
            SkillTree::R => "突击",
            SkillTree::E => "远程",
            SkillTree::D => "弹幕",
            SkillTree::Y => "控场",
            SkillTree::T => "吸血",
            SkillTree::F => "秘法",
            SkillTree::G => "奥术",
        }
    }

    /// 该树下所有可选技能（学习阶段由此挑选）。
    /// 目前返回已实现/已注册的技能；未实现的（占位）也会列出，绑定后执行时按占位处理。
    pub fn skills_in_tree(self) -> &'static [SkillId] {
        use SkillId::*;
        match self {
            SkillTree::C => &[Boost, Shield, Shadow, Fake],
            SkillTree::R => &[Blink, Blink2, DashStrike, DashSlash, BlinkToWall],
            SkillTree::E => &[Rock, StoneShot, StealthPush, StealthPush2, LineBeam, LineExplode],
            SkillTree::D => &[D2Fireball, D3Missile, D4Fireball],
            SkillTree::T => &[TLeech, T2Shot, T2Volley, T3Fast, T3Fast2, TestLeech],
            SkillTree::Y => &[Y1BlueLine, Y1BlueLine2, Y2Delay, Y2Suite, Y3Zone, Y3Zone2],
            SkillTree::F => &[Test03],
            SkillTree::G => &[Test01],
        }
    }
}

/// 技能键位（原版 8 键，S 为停手）。
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CastKey {
    C,
    R,
    E,
    D,
    Y,
    T,
    F,
    G,
}

impl CastKey {
    pub const ALL: [CastKey; 8] = [
        CastKey::C,
        CastKey::R,
        CastKey::E,
        CastKey::D,
        CastKey::Y,
        CastKey::T,
        CastKey::F,
        CastKey::G,
    ];

    pub fn as_u32(self) -> u32 {
        match self {
            CastKey::C => 0,
            CastKey::R => 1,
            CastKey::E => 2,
            CastKey::D => 3,
            CastKey::Y => 4,
            CastKey::T => 5,
            CastKey::F => 6,
            CastKey::G => 7,
        }
    }

    pub fn letter(self) -> &'static str {
        match self {
            CastKey::C => "C",
            CastKey::R => "R",
            CastKey::E => "E",
            CastKey::D => "D",
            CastKey::Y => "Y",
            CastKey::T => "T",
            CastKey::F => "F",
            CastKey::G => "G",
        }
    }

    /// 该键对应的技能树。
    pub fn tree(self) -> SkillTree {
        match self {
            CastKey::C => SkillTree::C,
            CastKey::R => SkillTree::R,
            CastKey::E => SkillTree::E,
            CastKey::D => SkillTree::D,
            CastKey::Y => SkillTree::Y,
            CastKey::T => SkillTree::T,
            CastKey::F => SkillTree::F,
            CastKey::G => SkillTree::G,
        }
    }
}

/// 技能标识（覆盖原版全部技能）。
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum SkillId {
    // C：位移/自保
    Boost,
    Shield,
    Shadow,
    Fake,
    // R：近战推 / 闪身
    Blink,
    Blink2,
    DashStrike,
    DashSlash,
    BlinkToWall,
    // E：远程
    Rock,
    StoneShot,
    StealthPush,
    StealthPush2,
    LineBeam,
    LineExplode,
    // D：弹幕
    D2Fireball,
    D3Missile,
    D4Fireball,
    // T：弹幕/吸血
    TLeech,
    T2Shot,
    T2Volley,
    T3Fast,
    T3Fast2,
    TestLeech,
    // Y：线/持续区
    Y1BlueLine,
    Y1BlueLine2,
    Y2Delay,
    Y2Suite,
    Y3Zone,
    Y3Zone2,
    // 测试类
    Test01,
    Test03,
    // 预留
    _Reserved,
    _SelfExplode,
}

impl SkillId {
    /// 每个技能归属的技能树。
    pub fn tree(self) -> SkillTree {
        use SkillId::*;
        match self {
            Boost | Shield | Shadow | Fake => SkillTree::C,
            Blink | Blink2 | DashStrike | DashSlash | BlinkToWall => SkillTree::R,
            Rock | StoneShot | StealthPush | StealthPush2 | LineBeam | LineExplode => SkillTree::E,
            D2Fireball | D3Missile | D4Fireball => SkillTree::D,
            TLeech | T2Shot | T2Volley | T3Fast | T3Fast2 | TestLeech => SkillTree::T,
            Y1BlueLine | Y1BlueLine2 | Y2Delay | Y2Suite | Y3Zone | Y3Zone2 => SkillTree::Y,
            Test03 => SkillTree::F,
            Test01 => SkillTree::G,
            _Reserved | _SelfExplode => SkillTree::G,
        }
    }

    pub fn as_u32(self) -> u32 {
        // 密集索引：与 DefTable 一致即可（用于 skill_levels / cooldowns 数组下标）
        use SkillId::*;
        match self {
            Boost => 0,
            Shield => 1,
            Shadow => 2,
            Fake => 3,
            Blink => 4,
            Blink2 => 5,
            DashStrike => 6,
            DashSlash => 7,
            BlinkToWall => 8,
            Rock => 9,
            StoneShot => 10,
            StealthPush => 11,
            StealthPush2 => 12,
            LineBeam => 13,
            LineExplode => 14,
            D2Fireball => 15,
            D3Missile => 16,
            D4Fireball => 17,
            TLeech => 18,
            T2Shot => 19,
            T2Volley => 20,
            T3Fast => 21,
            T3Fast2 => 22,
            TestLeech => 23,
            Y1BlueLine => 24,
            Y1BlueLine2 => 25,
            Y2Delay => 26,
            Y2Suite => 27,
            Y3Zone => 28,
            Y3Zone2 => 29,
            Test01 => 30,
            Test03 => 31,
            _Reserved => 32,
            _SelfExplode => 33,
        }
    }
}

/// 技能效果类型（决定 `World` 施法完成后的处理方式）。
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SkillEffect {
    /// 疾跑/生命偷取（C1）：一段持续时间内受击返还一半伤害作回血，移速随累积回血成长。
    Boost { duration: Fix64 },
    /// 反弹护盾（C2）：一段时间内自身的护盾泡泡把撞上的弹体/玩家镜向反弹。
    ReflectShield { duration: Fix64 },
    /// 影身定锚（C3）：在当前点放一个标记，再次施法回到标记处（两阶段技能）。
    Shadow,
    /// 幻象（C4）第一阶段：进入「待幻」，等待右键设移动目标时在本体原位留假身并瞬移。
    FakeSetup { max_time: Fix64 },
    /// 闪烁闪身：朝目标方向移动至多 `max_distance` 的单位。
    Blink { max_distance: Fix64 },
    /// 高速冲锋：朝目标方向冲锋，期间撞击造成踢击伤害。
    DashStrike {
        speed: Fix64,
        duration: Fix64,
        push_power: Fix64,
        push_time: Fix64,
        push_damage: Fix64,
    },
    /// 远程掷石：在目标位置造一个延时爆炸的 AOE（砸中造成伤害+击退）。
    Rock {
        max_distance: Fix64,
        fuse: Fix64,
        radius: Fix64,
        damage: Fix64,
        bomb_force: Fix64,
    },
    /// 直射弹体：以固定速度沿施法方向飞出，命中（足够近）造成伤害。
    Bullet {
        speed: Fix64,
        damage: Fix64,
        radius: Fix64,
        range: Fix64,
    },
    /// 追踪导弹：朝最近敌人缓慢转向并从命中目标处爆炸（AOE）。
    Missile {
        speed: Fix64,
        turn: Fix64,
        radius: Fix64,
        damage: Fix64,
        range: Fix64,
    },
    /// 持续伤害线：短时向前延伸的杀伤段，碰到即伤（原版 E 树线/弹）。
    LineBeam {
        length: Fix64,
        width: Fix64,
        damage: Fix64,
        duration: Fix64,
    },
    /// 潜行踢：隐身并在持续时间内对撞击目标造成踢击伤害。
    StealthPush {
        duration: Fix64,
        push_power: Fix64,
        push_time: Fix64,
        push_damage: Fix64,
    },
    /// 尚未实现/占位：契约上存在但暂不落地效果（绑定后施法会被消耗，但不产生作用）。
    Unimplemented,
}

/// 由等级推导的完整数值（成长采用"基础 + 每级斜率"的简单线性模型）。
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SkillStats {
    pub windup: Fix64,
    pub recovery: Fix64,
    pub cooldown: Fix64,
    // 效果参数（未用到时为 0）
    pub damage: Fix64,
    pub range: Fix64,
    pub radius: Fix64,
    pub duration: Fix64,
    pub speed: Fix64,
    pub push_power: Fix64,
    pub push_time: Fix64,
    pub push_damage: Fix64,
    pub max_distance: Fix64,
    /// 附加值：护盾强度 / 弹体宽度 等（未用到时为 0）
    pub extra: Fix64,
}

/// 一个技能在其`等级 l`下的成长参数：基础值 + 斜率×(l-1)。
#[derive(Copy, Clone, Debug)]
pub struct SkillGrowth {
    pub windup_base: f64,
    pub windup_delta: f64,
    pub recovery_base: f64,
    pub cooldown_base: f64,
    pub cooldown_delta: f64, // 冷却通常随等级降低（负数）
    pub damage_base: f64,
    pub damage_delta: f64,
    pub range_base: f64,
    pub radius_base: f64,
    pub radius_delta: f64,
    pub duration_base: f64,
    pub duration_delta: f64,
    pub speed_base: f64,
    pub push_power_base: f64,
    pub push_power_delta: f64,
    pub push_time_base: f64,
    pub push_damage_base: f64,
    pub push_damage_delta: f64,
    pub max_distance_base: f64,
    pub max_distance_delta: f64,
    /// 护盾吸收量/弹体宽度等附加值（用于 Shield / Bullet / LineBeam 的 width、shield 强度）。
    pub extra_base: f64,
    pub extra_delta: f64,
}

impl SkillGrowth {
    pub fn stats(&self, level: u32) -> SkillStats {
        let l = (level.max(1) as f64) - 1.0;
        SkillStats {
            windup: Fix64::from_num(self.windup_base + self.windup_delta * l),
            recovery: Fix64::from_num(self.recovery_base),
            cooldown: Fix64::from_num((self.cooldown_base + self.cooldown_delta * l).max(0.1)),
            damage: Fix64::from_num(self.damage_base + self.damage_delta * l),
            range: Fix64::from_num(self.range_base),
            radius: Fix64::from_num(self.radius_base + self.radius_delta * l),
            duration: Fix64::from_num(self.duration_base + self.duration_delta * l),
            speed: Fix64::from_num(self.speed_base),
            push_power: Fix64::from_num(self.push_power_base + self.push_power_delta * l),
            push_time: Fix64::from_num(self.push_time_base),
            push_damage: Fix64::from_num(self.push_damage_base + self.push_damage_delta * l),
            max_distance: Fix64::from_num(self.max_distance_base + self.max_distance_delta * l),
            extra: Fix64::from_num(self.extra_base + self.extra_delta * l),
        }
    }
}

/// 技能定义（静态信息 + 成长规则）。
#[derive(Copy, Clone, Debug)]
pub struct SkillDef {
    pub id: SkillId,
    pub tree: SkillTree,
    pub name: &'static str,
    /// 是否需要点目标（`true` → 用点击方向/落点；`false` → 无需目标）
    pub needs_point: bool,
    /// 施法完成后的效果类型
    pub effect: SkillEffect,
    pub growth: SkillGrowth,
}

impl SkillDef {
    /// 是否仍可施放（不处于施法/后摇、且满足冷却）——由 CastEvent 状态判断。
    pub fn stats_at(&self, level: u32) -> SkillStats {
        self.growth.stats(level)
    }
}

/// 施法请求被拒绝的原因。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CastError {
    /// 仍在施法（前摇）或后摇中
    Busy,
    /// 冷却中
    CoolingDown { remaining: Fix64 },
    /// 距离过近 / 目标无效（点目标类技能）
    InvalidTarget,
}

/// 一次施法状态机的当前阶段。
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum CastPhase {
    Idle,
    /// 前摇中（施法准备，被打断则失败）
    Windup {
        id: SkillId,
        target: Option<Vec2>,
        remaining: Fix64,
    },
    /// 后摇中（施法已生效，等待收招）
    Recovery { id: SkillId, remaining: Fix64 },
}

/// 施法者状态：前/后摇 + 冷却。
///
/// 冷却用一个小固定数组表示（`SkillId::_Reserved` 为占位），
/// 全部使用定点数递增/递减，保证确定性。
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Caster {
    phase: CastPhase,
    cooldowns: [Fix64; MAX_SKILL_SLOTS],
}

const MAX_SKILL_SLOTS: usize = crate::MAX_SKILL_SLOTS;

impl Caster {
    pub fn new() -> Self {
        Caster {
            phase: CastPhase::Idle,
            cooldowns: [Fix64::ZERO; MAX_SKILL_SLOTS],
        }
    }
}

impl Default for Caster {
    fn default() -> Self {
        Self::new()
    }
}

impl Caster {

    pub fn phase(&self) -> CastPhase {
        self.phase
    }

    /// 该技能的剩余冷却（0 表示可用）。
    pub fn cooldown_remaining(&self, id: SkillId) -> Fix64 {
        self.cooldowns[id.as_u32() as usize]
    }

    /// 是否正处于施法/后摇中（不能再次施放）。
    pub fn is_busy(&self) -> bool {
        !matches!(self.phase, CastPhase::Idle)
    }

    /// 是否正处于前摇（施法准备）中。前摇期间不能移动。
    pub fn is_windup(&self) -> bool {
        matches!(self.phase, CastPhase::Windup { .. })
    }

    /// 尝试开始施法。校验冷却、前/后摇与目标有效性。
    pub fn try_cast(
        &mut self,
        def: &SkillDef,
        level: u32,
        target: Option<Vec2>,
        self_pos: Vec2,
        self_radius: Fix64,
    ) -> Result<(), CastError> {
        if self.is_busy() {
            return Err(CastError::Busy);
        }
        let cd = self.cooldowns[def.id.as_u32() as usize];
        if cd > Fix64::ZERO {
            return Err(CastError::CoolingDown { remaining: cd });
        }
        // 点目标技能：目标过近时把落点钳制到最小施法距离，而不是拒绝施法。
        // （原版直接判失败会导致“点了没反应”，且随位置偶发——改为钳制更友好、行为确定。）
        let mut target = target;
        if def.needs_point {
            match target {
                None => return Err(CastError::InvalidTarget),
                Some(t) => {
                    let min_range = self_radius.min(Fix64::from_num(1.0))
                        + Fix64::from_num(0.5);
                    let d = t - self_pos;
                    let len = d.length();
                    if len < min_range && len > Fix64::ZERO {
                        // 朝点击方向，但至少飞到最小施法距离
                        target = Some(self_pos + d.normalized() * min_range);
                    }
                }
            }
        }
        let stats = def.stats_at(level);
        let windup = stats.windup;
        self.phase = CastPhase::Windup {
            id: def.id,
            target,
            remaining: windup,
        };
        Ok(())
    }

    /// 被打断（被踢飞 / 重大控制）。非 Idle 时施法失败并退回 Idle。
    pub fn interrupt(&mut self) {
        if !matches!(self.phase, CastPhase::Idle) {
            self.phase = CastPhase::Idle;
        }
    }

    /// 一帧推进：前摇计时 → 施法完成，后摇计时 → 冷却计时。
    ///
    /// 返回 `Ok(Some((id, target)))` 表示此帧内前摇结束、需要执行效果；
    /// 返回 `Err(())` 表示施法完成但效果执行由调用方处理。
    pub fn advance(&mut self, dt: Fix64) -> Option<(SkillId, Option<Vec2>)> {
        // 冷却计时
        for cd in self.cooldowns.iter_mut() {
            if *cd > Fix64::ZERO {
                *cd = (*cd - dt).max(Fix64::ZERO);
            }
        }
        match self.phase {
            CastPhase::Idle => None,
            CastPhase::Windup { id, target, remaining } => {
                if remaining <= dt {
                    // 前摇结束 → 命中结算；进入后摇
                    let rec = self_recovery(id);
                    self.phase = CastPhase::Recovery { id, remaining: rec };
                    Some((id, target))
                } else {
                    self.phase = CastPhase::Windup {
                        id,
                        target,
                        remaining: remaining - dt,
                    };
                    None
                }
            }
            CastPhase::Recovery { id, remaining } => {
                if remaining <= dt {
                    self.phase = CastPhase::Idle;
                } else {
                    self.phase = CastPhase::Recovery {
                        id,
                        remaining: remaining - dt,
                    };
                }
                None
            }
        }
    }

    /// 施法完成/开始后设置冷却（由效果执行方调用）。
    pub fn begin_cooldown(&mut self, id: SkillId) {
        let base_cd = DefTable::def(id).growth.cooldown_base;
        self.cooldowns[id.as_u32() as usize] = Fix64::from_num(base_cd.max(0.1));
    }
}

fn self_recovery(_id: SkillId) -> Fix64 {
    // 后摇统一取技能定义中的 recovery；这里由 DefTable 提供
    DefTable::def(_id).stats_at(1).recovery
}

/// 技能定义表（初期实现：C / R / E 三棵树）。
///
/// 数值参考原版（R1/R2/R3b/E1/E2/C1/C3/C4）；前摇/后摇为本重写新增的
/// 网络手感补偿设计，先在原地取保守值，后续再调斜率。
pub struct DefTable;

impl DefTable {
    pub fn def(id: SkillId) -> SkillDef {
        use SkillEffect::*;
        // 已实现技能：给出真实数值（参考原版）。
        // 其余技能：暂以 Unimplemented 占位（契约存在，可绑定/学习，但暂不落地效果）。
        match id {
            SkillId::Boost => SkillDef {
                id,
                tree: SkillTree::C,
                name: "疾跑",
                needs_point: false,
                effect: Boost { duration: Fix64::from_num(5.0) },
                growth: SkillGrowth { recovery_base: 0.2, cooldown_base: 10.0, duration_base: 5.0, ..DEF_ZERO },
            },
            SkillId::Shield => SkillDef {
                id,
                tree: SkillTree::C,
                name: "护盾",
                needs_point: false,
                effect: ReflectShield { duration: Fix64::from_num(2.0) },
                growth: SkillGrowth { recovery_base: 0.1, cooldown_base: 5.0, duration_base: 2.0, ..DEF_ZERO },
            },
            SkillId::Shadow => SkillDef {
                id,
                tree: SkillTree::C,
                name: "影身",
                needs_point: false,
                effect: Shadow,
                growth: SkillGrowth { recovery_base: 0.1, cooldown_base: 3.0, duration_base: 2.5, ..DEF_ZERO }, // duration = 记号有效窗口
            },
            SkillId::Fake => SkillDef {
                id,
                tree: SkillTree::C,
                name: "幻象",
                needs_point: false,
                effect: FakeSetup { max_time: Fix64::from_num(3.0) },
                growth: SkillGrowth { recovery_base: 0.1, cooldown_base: 5.0, duration_base: 3.0, ..DEF_ZERO },
            },
            SkillId::Blink => SkillDef {
                id,
                tree: SkillTree::R,
                name: "闪烁",
                needs_point: true,
                effect: Blink { max_distance: Fix64::from_num(6.0) },
                growth: SkillGrowth { cooldown_base: 3.0, max_distance_base: 6.0, ..DEF_ZERO },
            },
            SkillId::DashStrike => SkillDef {
                id,
                tree: SkillTree::R,
                name: "冲锋",
                needs_point: true,
                effect: DashStrike {
                    speed: Fix64::from_num(8.0),
                    duration: Fix64::from_num(0.5),
                    push_power: Fix64::from_num(6.0),
                    push_time: Fix64::from_num(0.6),
                    push_damage: Fix64::from_num(8.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.15,
                    cooldown_base: 3.0,
                    push_power_base: 6.0,
                    push_power_delta: 1.0,
                    push_damage_base: 8.0,
                    push_damage_delta: 2.0,
                    ..DEF_ZERO
                },
            },
            SkillId::Rock => SkillDef {
                id,
                tree: SkillTree::E,
                name: "掷石",
                needs_point: true,
                effect: Rock {
                    max_distance: Fix64::from_num(5.0),
                    fuse: Fix64::from_num(0.7),
                    radius: Fix64::from_num(2.0),
                    damage: Fix64::from_num(10.0),
                    bomb_force: Fix64::from_num(8.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.2,
                    recovery_base: 0.2,
                    cooldown_base: 3.0,
                    damage_base: 10.0,
                    damage_delta: 2.0,
                    radius_base: 2.0,
                    duration_base: 0.7,
                    max_distance_base: 5.0,
                    ..DEF_ZERO
                },
            },
            SkillId::StealthPush => SkillDef {
                id,
                tree: SkillTree::E,
                name: "潜行踢",
                needs_point: false,
                effect: StealthPush {
                    duration: Fix64::from_num(2.0),
                    push_power: Fix64::from_num(4.0),
                    push_time: Fix64::from_num(0.5),
                    push_damage: Fix64::from_num(5.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.25,
                    recovery_base: 0.15,
                    cooldown_base: 5.0,
                    push_damage_base: 5.0,
                    push_damage_delta: 1.5,
                    ..DEF_ZERO
                },
            },
            // E 树：掷弹 / 直射弹（StoneShot、火球共用 Bullet 效果）
            SkillId::StoneShot => SkillDef {
                id,
                tree: SkillTree::E,
                name: "掷弹",
                needs_point: true,
                effect: Bullet {
                    speed: Fix64::from_num(9.0),
                    damage: Fix64::from_num(8.0),
                    radius: Fix64::from_num(0.8),
                    range: Fix64::from_num(12.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.15,
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    cooldown_delta: -0.2,
                    damage_base: 8.0,
                    damage_delta: 2.0,
                    speed_base: 9.0,
                    range_base: 12.0,
                    max_distance_delta: 1.0,
                    ..DEF_ZERO
                },
            },
            // E 树：持续伤害线
            SkillId::LineBeam => SkillDef {
                id,
                tree: SkillTree::E,
                name: "激光线",
                needs_point: true,
                effect: LineBeam {
                    length: Fix64::from_num(8.0),
                    width: Fix64::from_num(0.9),
                    damage: Fix64::from_num(3.0),
                    duration: Fix64::from_num(1.2),
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 6.0,
                    cooldown_delta: -0.3,
                    damage_base: 3.0,
                    damage_delta: 0.5,
                    range_base: 8.0,
                    duration_base: 1.2,
                    extra_base: 0.9, // 线宽
                    ..DEF_ZERO
                },
            },
            // D 树：火球（直射弹）
            SkillId::D2Fireball => SkillDef {
                id,
                tree: SkillTree::D,
                name: "火球",
                needs_point: true,
                effect: Bullet {
                    speed: Fix64::from_num(12.0),
                    damage: Fix64::from_num(12.0),
                    radius: Fix64::from_num(1.0),
                    range: Fix64::from_num(14.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.12,
                    recovery_base: 0.1,
                    cooldown_base: 2.5,
                    cooldown_delta: -0.2,
                    damage_base: 12.0,
                    damage_delta: 3.0,
                    speed_base: 12.0,
                    range_base: 14.0,
                    max_distance_delta: 1.0,
                    ..DEF_ZERO
                },
            },
            // D 树：追踪导弹
            SkillId::D3Missile => SkillDef {
                id,
                tree: SkillTree::D,
                name: "导弹",
                needs_point: false,
                effect: Missile {
                    speed: Fix64::from_num(7.0),
                    turn: Fix64::from_num(6.0),
                    radius: Fix64::from_num(1.6),
                    damage: Fix64::from_num(18.0),
                    range: Fix64::from_num(10.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.2,
                    recovery_base: 0.15,
                    cooldown_base: 4.0,
                    cooldown_delta: -0.3,
                    damage_base: 18.0,
                    damage_delta: 4.0,
                    radius_base: 1.6,
                    radius_delta: 0.1,
                    speed_base: 7.0,
                    ..DEF_ZERO
                },
            },
            // —— 其余技能：暂为未实现占位 ——
            skill => SkillDef {
                id,
                tree: skill.tree(),
                name: "未实现",
                needs_point: false,
                effect: Unimplemented,
                growth: DEF_ZERO,
            },
        }
    }
}


// —— 辅助构造 ——
const DEF_ZERO: SkillGrowth = SkillGrowth {
    windup_base: 0.0,
    windup_delta: 0.0,
    recovery_base: 0.1,
    cooldown_base: 1.0,
    cooldown_delta: 0.0,
    damage_base: 0.0,
    damage_delta: 0.0,
    range_base: 0.0,
    radius_base: 0.0,
    radius_delta: 0.0,
    duration_base: 0.0,
    duration_delta: 0.0,
    speed_base: 0.0,
    push_power_base: 0.0,
    push_power_delta: 0.0,
    push_time_base: 0.0,
    push_damage_base: 0.0,
    push_damage_delta: 0.0,
    max_distance_base: 0.0,
    max_distance_delta: 0.0,
    extra_base: 0.0,
    extra_delta: 0.0,
};

// 由于 SkillDef 由 DefTable::def 直接构造（非 const，因需运行时 from_num），
// 无需单独的 make 辅助函数。
// NOTE: 技能成长斜率集中在 `SkillGrowth` 表，后续调参只需改这里的数值。

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Fix64, b: f64, tol: f64) -> bool {
        (a.to_num::<f64>() - b).abs() < tol
    }

    #[test]
    fn growth_scales_with_level() {
        let def = DefTable::def(SkillId::Rock);
        let s1 = def.stats_at(1);
        let s3 = def.stats_at(3);
        // 等级3的伤害应高于等级1（基础 + 2× 斜率）
        assert!(s3.damage > s1.damage);
        assert!(near(s1.damage, 10.0, 1e-3));
        assert!(near(s3.damage, 14.0, 1e-3));
    }

    #[test]
    fn cooldown_blocks_recast() {
        let mut caster = Caster::new();
        let def = DefTable::def(SkillId::Blink);
        // 施放：进入前摇
        let r = caster.try_cast(&def, 1, Some(Vec2::new(Fix64::from_num(5.0), Fix64::ZERO)), Vec2::ZERO, Fix64::ONE);
        assert!(r.is_ok());
        assert!(matches!(caster.phase(), CastPhase::Windup { .. }));
        // 前摇未满时再次施放 → Busy
        let r2 = caster.try_cast(&def, 1, Some(Vec2::new(Fix64::from_num(5.0), Fix64::ZERO)), Vec2::ZERO, Fix64::ONE);
        assert_eq!(r2, Err(CastError::Busy));
    }

    #[test]
    fn interrupt_cancels_windup() {
        let mut caster = Caster::new();
        let def = DefTable::def(SkillId::Rock);
        caster.try_cast(&def, 1, Some(Vec2::new(Fix64::from_num(5.0), Fix64::ZERO)), Vec2::ZERO, Fix64::ONE).unwrap();
        caster.interrupt();
        assert!(matches!(caster.phase(), CastPhase::Idle));
    }

    #[test]
    fn windup_completes_then_recovery() {
        let mut caster = Caster::new();
        let def = DefTable::def(SkillId::Rock); // windup_base = 0.2
        caster.try_cast(&def, 1, Some(Vec2::new(Fix64::from_num(5.0), Fix64::ZERO)), Vec2::ZERO, Fix64::ONE).unwrap();
        let dt = Fix64::from_num(0.12);
        // 分两步推：第一步未满，第二步完成
        assert!(caster.advance(dt).is_none());
        let fired = caster.advance(dt);
        assert!(fired.is_some());
        assert!(matches!(caster.phase(), CastPhase::Recovery { .. }));
        // 后摇结束后回到 Idle
        for _ in 0..8 {
            caster.advance(dt);
        }
        assert!(matches!(caster.phase(), CastPhase::Idle));
    }

    #[test]
    fn point_target_clamps_too_close_instead_of_rejecting() {
        let mut caster = Caster::new();
        let def = DefTable::def(SkillId::Blink);
        // 目标紧贴自身：不再拒绝，而是钳制到最小施法距离后进入前摇
        let r = caster.try_cast(
            &def,
            1,
            Some(Vec2::new(Fix64::from_num(0.2), Fix64::ZERO)),
            Vec2::ZERO,
            Fix64::ONE,
        );
        assert!(r.is_ok(), "过近目标应被接受（钳制）而非拒绝");
        // 用一个全新的 Caster 验证“有效目标同样能施法”
        let mut caster2 = Caster::new();
        let r2 = caster2.try_cast(
            &def,
            1,
            Some(Vec2::new(Fix64::from_num(5.0), Fix64::ZERO)),
            Vec2::ZERO,
            Fix64::ONE,
        );
        assert!(r2.is_ok());
    }
}
