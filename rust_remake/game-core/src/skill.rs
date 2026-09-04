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
            SkillTree::C => &[Boost, Shield, Shadow, Fake, S005, S006, S007],
            SkillTree::R => &[Blink, Blink2, DashStrike, DashSlash, TestSwap, BlinkToWall, S011, S012, S013],
            SkillTree::E => &[Rock, StoneShot, StealthPush, StealthPush2, LineBeam, LineExplode, S008, S009, S010],
            SkillTree::D => &[TestLightning, D2Fireball, D3Missile, D4Fireball, S002, S003, S004],
            SkillTree::T => &[TLeech, T2Shot, T2Volley, T3Fast, T3Fast2, TestLeech, S014, S015, S016],
            SkillTree::Y => &[Y1BlueLine, Y1BlueLine2, Y2Delay, Y2Suite, Y3Zone, Y3Zone2, S017, S018, S019],
            SkillTree::F => &[Test03],
            SkillTree::G => &[Test01, S000],
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
    /// 雷电（D1）：指向性即时射线，命中敌人伤害+推，撞障碍停止。
    TestLightning,
    /// 换位（R3a）：点目标，有敌人则互换位置，否则瞬移过去。
    TestSwap,
    // 预留
    _Reserved,
    _SelfExplode,
    // ===== 098b 名册（PORT_098B_DECISIONS.md M1；编号=war3 技能 ID，数值 port_spec_098b.json） =====
    /// S000 火球（热键 G）：直射弹，命中 KI 伤害+击退并点燃 AoE（xc，2.5s DoT）。
    S000,
    /// S002 闪电（热键 D）：瞬发射线，伤害 6+等级、射程 (1+0.15×oi)×600、KI 击退。
    S002,
    /// S003 追踪弹（热键 D）：锁定点击处最近敌人全速直追，命中 KI 伤害+击退。
    S003,
    /// S004 回旋镖（热键 D）：飞出后回旋拉回施法者，命中 KI + qI 区域二次伤害（M1 先近似回程）。
    S004,
    /// S008 陨石（热键 E）：直飞目标点爆炸 AoE 200，KI($A+2L, .8)；灼烧 nB 数值未解码（TODO M2）。
    S008,
    /// S009 分裂弹（热键 E）：直射弹 KI(3, 1.4)；speed=GB/280 的 GB 未解码（M1 近似 900）。
    S009,
    /// S014 汲取（热键 T）：直射弹双段伤害（JI .2/.6，M1 合并为 .8）；吸血与减速形态 TODO M2。
    S014,
    /// S015 火焰喷射（热键 T）：锥形 5 道火焰小弹，命中 jI(2.6+0.4L, .65) AoE 45。
    S015,
    /// S016 弹跳弹（热键 T）：命中后跳向最近下一个目标，伤害 ×0.8/跳（gc 形态 5+L）。
    S016,
    // ---- M2 批次B：位移/增益系（PORT_098B_DECISIONS.md M2） ----
    /// S005 反射盾（热键 C）：(2.6+0.2L)s 内反弹来袭弹体（098b DC）。
    S005,
    /// S006 时光回溯（热键 C）：3.6s 后闪回施法点并还原 HP（098b fC）。
    S006,
    /// S007 急行（热键 C）：(6.2+0.8L)s +35 移速（098b jR；攻速无对应系统，TODO）。
    S007,
    /// S010 疾风步（热键 E）：3.1s 隐身+200 移速（098b OB；破隐一击 TODO）。
    S010,
    /// S011 瞬间移动（热键 R）：闪现 700+70L（098b hB）。
    S011,
    /// S012 冲撞（热键 R）：1300/s 冲刺 (650+50L)×1.1，命中 KI+击退+0.5s 定身（098b IB）。
    S012,
    /// S013 移形换位（热键 R）：与 660 内目标互换位置（098b mB；弹体化 TODO）。
    S013,
    // ---- M2 批次C：场/线控制系 ----
    /// S017 致残（热键 Y）：弹体命中 KI+残废 Tied (4+0.25L)s（098b eC；AoE 分支 TODO）。
    S017,
    /// S018 引力（热键 Y）：飞出引力场吸拉敌人（098b mc 升级版语义；拉速占位 TODO）。
    S018,
    /// S019 锁链（热键 Y）：弹体命中把目标拉向施法者 + 定身 0.5s（098b tc；S031 附加 TODO）。
    S019,
}

impl SkillId {
    /// 每个技能归属的技能树。
    pub fn tree(self) -> SkillTree {
        use SkillId::*;
        match self {
            Boost | Shield | Shadow | Fake => SkillTree::C,
            Blink | Blink2 | DashStrike | DashSlash | TestSwap | BlinkToWall => SkillTree::R,
            Rock | StoneShot | StealthPush | StealthPush2 | LineBeam | LineExplode => SkillTree::E,
            D2Fireball | D3Missile | D4Fireball | TestLightning => SkillTree::D,
            TLeech | T2Shot | T2Volley | T3Fast | T3Fast2 | TestLeech => SkillTree::T,
            Y1BlueLine | Y1BlueLine2 | Y2Delay | Y2Suite | Y3Zone | Y3Zone2 => SkillTree::Y,
            Test03 => SkillTree::F,
            Test01 => SkillTree::G,
            _Reserved | _SelfExplode => SkillTree::G,
            // 098b 热键（mechanics §6.1）：G=火球；D=闪电/追踪弹/回旋镖；E=陨石/分裂弹/疾风步/物品；
            // T=汲取/火焰喷射/弹跳弹/法术2；R=瞬间移动/冲撞/移形换位/法术1；C=反射盾/时光回溯/急行。
            S000 => SkillTree::G,
            S002 | S003 | S004 => SkillTree::D,
            S008 | S009 | S010 => SkillTree::E,
            S014 | S015 | S016 => SkillTree::T,
            S011 | S012 | S013 => SkillTree::R,
            S005 | S006 | S007 => SkillTree::C,
            S017 | S018 | S019 => SkillTree::Y,
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
            TestLightning => 34,
            TestSwap => 35,
            _Reserved => 32,
            _SelfExplode => 33,
            S000 => 36,
            S003 => 37,
            S004 => 38,
            S002 => 39,
            S008 => 40,
            S009 => 41,
            S014 => 42,
            S015 => 43,
            S016 => 44,
            S005 => 45,
            S006 => 46,
            S007 => 47,
            S010 => 48,
            S011 => 49,
            S012 => 50,
            S013 => 51,
            S017 => 52,
            S018 => 53,
            S019 => 54,
        }
    }

    /// `as_u32` 的逆映射（网络编解码用）。越界返回 `_Reserved`。
    pub fn from_u32(v: u32) -> SkillId {
        use SkillId::*;
        match v {
            0 => Boost,
            1 => Shield,
            2 => Shadow,
            3 => Fake,
            4 => Blink,
            5 => Blink2,
            6 => DashStrike,
            7 => DashSlash,
            8 => BlinkToWall,
            9 => Rock,
            10 => StoneShot,
            11 => StealthPush,
            12 => StealthPush2,
            13 => LineBeam,
            14 => LineExplode,
            15 => D2Fireball,
            16 => D3Missile,
            17 => D4Fireball,
            18 => TLeech,
            19 => T2Shot,
            20 => T2Volley,
            21 => T3Fast,
            22 => T3Fast2,
            23 => TestLeech,
            24 => Y1BlueLine,
            25 => Y1BlueLine2,
            26 => Y2Delay,
            27 => Y2Suite,
            28 => Y3Zone,
            29 => Y3Zone2,
            30 => Test01,
            31 => Test03,
            32 => _Reserved,
            33 => _SelfExplode,
            34 => TestLightning,
            35 => TestSwap,
            36 => S000,
            37 => S003,
            38 => S004,
            39 => S002,
            40 => S008,
            41 => S009,
            42 => S014,
            43 => S015,
            44 => S016,
            45 => S005,
            46 => S006,
            47 => S007,
            48 => S010,
            49 => S011,
            50 => S012,
            51 => S013,
            52 => S017,
            53 => S018,
            54 => S019,
            _ => _Reserved,
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
    /// 二段闪（R1b）：一次普通闪烁后，进入一个可免冷却再闪一次（更短距离）的窗口。
    /// 直接施放 = 第一次闪；窗口内再施放 = 第二次短闪。
    Blink2 { max_distance: Fix64 },
    /// 冲刺斩（R2b）：进入无限时长 + 全程隐身的直线冲刺，直到玩家给出新的移动命令才解除。
    DashSlash { speed: Fix64 },
    /// 闪到墙（R3b）：沿目标方向射线找最近的障碍/玩家，落在其前；无障碍则闪 `max_distance`。
    BlinkToWall { max_distance: Fix64 },
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
    /// 追踪导弹：朝点击处最近的敌人全速直追（命中爆炸伤+击退）。
    Missile {
        speed: Fix64,
        radius: Fix64,
        damage: Fix64,
        push_power: Fix64,
        push_time: Fix64,
        range: Fix64,
    },
    /// 回旋镖（D2）：持续朝施法者加速回飞 + 撞障碍反弹，命中敌人爆炸伤+击退。
    Boomerang {
        speed: Fix64,
        accelerate: Fix64,
        radius: Fix64,
        damage: Fix64,
        push_power: Fix64,
        push_time: Fix64,
        life: Fix64,
    },
    /// 双香蕉曲线弹（D4）：朝 ±45° 各打一发曲线飞行弹，命中爆炸伤+击退。
    Banana {
        count: u32,
        turn_rad: f64,
        speed: Fix64,
        radius: Fix64,
        damage: Fix64,
        push_power: Fix64,
        push_time: Fix64,
        life: Fix64,
    },
    /// 持续伤害线：短时向前延伸的杀伤段，碰到即伤（原版 E 树线/弹）。
    LineBeam {
        length: Fix64,
        width: Fix64,
        damage: Fix64,
        duration: Fix64,
    },
    /// 吸血链镖（T1b）：命中敌人吸血回己，并自动跳到最近的下一个敌人（链式吸血）。
    ChainLeech {
        speed: Fix64,
        damage: Fix64,
        heal: Fix64,
        range: Fix64,
    },
    /// 跳弹·衰减（T3）：高速镖，命中后跳到最近下一个，伤害逐跳衰减。
    JumpDecay {
        speed: Fix64,
        damage: Fix64,
        range: Fix64,
        ratio_decay: Fix64,
    },
    /// 转镖吸血（TestLeech）：直线飞行一段后折向最近敌人，命中吸血回己。
    TurnLeech {
        speed: Fix64,
        damage: Fix64,
        heal: Fix64,
        turn_delay: Fix64,
        range: Fix64,
    },
    /// 扇面齐射（T2b）：同时喷出若干发扇形爆炸弹。
    Volley {
        bullet_speed: Fix64,
        damage: Fix64,
        count: u32,
        spread_step: f64,
    },
    /// 扇扫连射（T2）：朝目标方向依次喷出若干发爆炸弹（每发角度微转）。
    Sweep {
        bullet_speed: Fix64,
        damage: Fix64,
        count: u32,
        cadence: f64,   // 每发间隔（秒）
        turn_step: f64, // 每发角度步进（弧度）
    },
    /// 跳弹·蓄力（T3b）：命中→爆炸伤+推并原地留一个回返镖；返回施法者即刷新技能冷却，
    /// 且累计的 `damageplus` 让后续伤害更高。
    BonusChain {
        speed: Fix64,
        damage: Fix64,
        range: Fix64,
    },
    /// 回拉线（Y1/Y1b）：命中目标后拉向施法者并持续掉血；`beam`=Y1b 沿路径额外扫射。
    Tether {
        damage: Fix64,
        pull_speed: Fix64,
        duration: Fix64,
        beam: bool,
    },
    /// 撞击迟缓弹（Y2）：直线弹命中后把目标推离一定时长。
    PushShot {
        speed: Fix64,
        damage: Fix64,
        push_power: Fix64,
        push_time: Fix64,
        range: Fix64,
    },
    /// 雷电（D1）：指向性即时射线，命中敌人伤害+推，撞障碍停止（无飞行弹体）。
    Lightning,
    /// 换位（R3a）：点目标，若目标位置附近有敌人则与之互换位置，否则自身瞬移到该点。
    Swap { max_distance: Fix64 },
    /// 束缚线（Y2b）：施法者身后两点反向收拢成线，线上的敌人被束缚（禁施法）。
    BindLine {
        speed: Fix64,
        count: u32,
        bind_time: f64,
    },
    /// 引力场（Y3）：飞行场持续把附近敌人吸向场中心。
    GravityZone {
        speed: Fix64,
        pull_speed: Fix64,
        radius: Fix64,
        life: f64,
        range: Fix64,
    },
    /// 星域持续伤（Y3b）：目标点放一颗星，范围内敌持续掉血、对施法者回血。
    StarZone {
        damage_per_sec: Fix64,
        heal_per_sec: Fix64,
        radius: Fix64,
        duration: f64,
        range: Fix64,
    },
    /// 潜行踢：隐身并在持续时间内对撞击目标造成踢击伤害。
    StealthPush {
        duration: Fix64,
        push_power: Fix64,
        push_time: Fix64,
        push_damage: Fix64,
    },
    /// 潜行踢·连推（E2b）：同 E2，但撞到障碍后 0.3s 重新触发一次踢击（窗口内可反复）。
    StealthPush2 {
        duration: Fix64,
        push_power: Fix64,
        push_time: Fix64,
        push_damage: Fix64,
    },
    /// 滚动火球（E1b）：沿定速直线滚动，接触范围内敌人持续掉血。
    RollProjectile {
        speed: Fix64,
        damage_per_sec: Fix64,
        radius: Fix64,
        range: Fix64,
    },
    /// 撒弹线·E3：沿方向飞行的线，到终点一次性撒扇形弹。
    ScatterBurst {
        speed: Fix64,
        range: Fix64,
        count: u32,
        step_rad: f64,
        bullet_speed: Fix64,
    },
    /// 撒弹线·E3b：飞行途中每 `interval` 秒撒一发并旋转方向。
    ScatterPeriodic {
        speed: Fix64,
        range: Fix64,
        count: u32,
        interval: f64,
        bullet_speed: Fix64,
        turn_rad: f64,
    },
    /// 蓄力自爆（F Test03）：吟唱结束后以自身为圆心 AOE 爆炸，自己扣到残血、范围内敌人掉血+踢开。
    SelfExplode {
        radius: Fix64,
        self_stay: Fix64,
        damage: Fix64,
        kick: Fix64,
        kick_time: Fix64,
    },
    /// 尚未实现/占位：契约上存在但暂不落地效果（绑定后施法会被消耗，但不产生作用）。
    Unimplemented,
    /// 098b 名册投射物（PORT_098B_DECISIONS.md M1/M2；数值来自 `port_098b/data/port_spec_098b.json`，
    /// 已是 war3 尺度——**直通 DefTable::def，不经 legacy_scale_def 缩放**）。
    ///
    /// 伤害/击退在命中时按 KI/FI 公式结算（`world::warlock_ki_impact`）：
    /// - 伤害（FI）= `gX × Gn[攻] × hn[守]`；gX = growth.damage_base/damage_delta 随等级。
    /// - 击退初速（KI）= `balance::DAMAGE_BASE × gX × kb_ji`（JI 系数，如火球 1.1×eb）。
    Warlock098b {
        /// 运动学形态（直线/追踪/回旋/弹跳）。
        proj: W098bProjKind,
        /// 弹速（war3 单位/秒，spec.projectile.speed）。
        speed: Fix64,
        /// 命中半径（spec.projectile.radius）。
        radius: Fix64,
        /// 存活秒数（spec.projectile.life 已按 oi=0 求值）。
        life: Fix64,
        /// 击退系数 JI（KI 公式的末项，如火球 `1.1*eb` → eb=1 时 1.1）。
        kb_ji: Fix64,
        /// 命中点燃：DoT 总伤害（在 IGNITE_SEC 内均摊；098b 火球 xc 点燃）。
        ignite: Option<Fix64>,
        /// 命中/寿命尽时的 AoE 爆炸半径（陨石 200；None=单体命中）。
        blast: Option<Fix64>,
        /// 连发数（火焰喷射锥形 5 道；其余 1）。
        count: u32,
        /// 连发角度步进（弧度，喷火 5.5°≈0.096；count=1 时忽略）。
        spread_step: f64,
        /// 命中副作用（默认 Ki；S017 残废 / S019 拉拽）。
        on_hit: W098bOnHit,
    },
    /// 098b 即时射线（S002 闪电）：无弹体，沿施法方向找首个命中（玩家或障碍截断），
    /// KI 伤害+击退并写 `lightning_visual` 供 client 画闪电线。
    /// FI 伤害 gX = growth.damage（随等级，闪电 6+1×L）。
    W098bBolt {
        /// 射程（闪电 (1+0.15×oi)×600 → oi=0 为 600）。
        range: Fix64,
        /// KI 击退系数 JI。
        kb_ji: Fix64,
    },
    /// 098b 位移/增益系（M2 批次B：S005/S006/S007/S010/S011/S012/S013）。
    /// 数值口径：duration/damage/max_distance 全走 growth（stats 按施法等级求值）；
    /// `speed` 为不随等级的常量（Dash 冲刺速度 / Haste·Windwalk 的移速乘数，war3 加法移速按
    /// 基础 210 换算成乘数，如 +35 → 1+35/210）。
    W098bUtility {
        kind: W098bUtilKind,
        speed: Fix64,
        /// Blink/Swap/Dash 的最大距离 L1 基准（成长走 growth.max_distance）。
        max_distance: Fix64,
    },
}

/// 098b 位移/增益子类型。
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum W098bUtilKind {
    /// S005 反射盾：反弹来袭弹体（duration = 2.6+0.2L）。
    Reflect,
    /// S006 时光回溯：3.6s 后闪回施法点并还原 HP。
    Rewind,
    /// S007 急行：+35 移速（duration = 6.2+0.8L）；攻速无对应系统（TODO）。
    Haste,
    /// S010 疾风步：隐身 +200 移速（duration 3.1）；破隐一击 TODO。
    Windwalk,
    /// S011 闪现：瞬移至多 700+70L。
    Blink,
    /// S012 冲撞：1300/s 冲刺至多 (650+50L)×1.1，命中 KI+击退+0.5s 定身。
    Dash,
    /// S013 移形换位：与 660 内目标互换位置（弹体化 TODO）。
    Swap,
}

/// 098b 投射物运动学形态。
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum W098bProjKind {
    /// 直线飞行（S000 火球/S008 陨石/S009 分裂弹/S014 汲取等）。
    Straight,
    /// 锁定目标全速直追（S003 追踪弹：速度含施法者动量，M1 忽略动量项）。
    Homing,
    /// 回旋镖（S004）：飞出后半程拉回施法者（098b 为侧向分量公式，M1 以出-回近似，TODO M2 对齐）。
    Boomerang,
    /// 弹跳弹（S016）：命中后跳向最近下一个目标（跳过上一目标），伤害 ×0.8/跳。
    Bounce,
}

/// 098b 弹体命中副作用（附加在 KI 伤害+击退之上的控制效果；时长走 growth.duration）。
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum W098bOnHit {
    /// 默认：仅 KI 伤害+击退。
    Ki,
    /// S017 致残：命中附加 Tied（残废禁施法/禁移动近似）(4+0.25L)s。
    Cripple,
    /// S019 锁链：把目标拉向施法者（朝施法者 600/s × 0.5s）+ Tied 0.5s。
    ChainPull,
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
    // mana_cost 已随无蓝量系统移除（PORT_098B_DECISIONS.md D3）。
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
    // mana_cost() 已随无蓝量系统移除（PORT_098B_DECISIONS.md D3）。
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

    /// 清零某个技能的冷却（供 T3b 回返镖到位刷新冷却等使用）。
    pub fn reset_cooldown(&mut self, id: SkillId) {
        self.cooldowns[id.as_u32() as usize] = Fix64::ZERO;
    }

    /// （序列化用）原始读取 phase + 冷却数组。
    pub(crate) fn raw_snapshot(&self) -> (CastPhase, [Fix64; MAX_SKILL_SLOTS]) {
        (self.phase, self.cooldowns)
    }
    /// （序列化用）原始恢复 phase + 冷却数组。
    pub(crate) fn raw_restore(&mut self, phase: CastPhase, cooldowns: [Fix64; MAX_SKILL_SLOTS]) {
        self.phase = phase;
        self.cooldowns = cooldowns;
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

// ===== 098b 尺度过渡缩放（PORT_098B_DECISIONS.md D4） =====
//
// 旧（Unity demo 微缩）数值 → war3 尺度的三类因子。世界尺度已切到 war3（balance.rs），
// 但技能表条目仍是旧尺度字面量——过渡期在 `DefTable::def()` 出口单点放大，
// **098b 名册替换完成时整体删除**（届时数值直接来自 port_spec_098b.json，无需换算）。
//
// 类别口径：速度类（speed/击退力/拉拽/加速度）×65.625（=210/3.2）；距离类（range/max_distance/length）×60（=1200/20）；
// 半径类（radius/width）×16（=16/1）。时长（秒）与伤害**不缩放**（HP 尺度 100 不变）。
const LEGACY_SPEED: f64 = 65.625;
const LEGACY_DIST: f64 = 60.0;
const LEGACY_RADIUS: f64 = 16.0;

fn sc(x: Fix64, factor: f64) -> Fix64 {
    Fix64::from_num(x.to_num::<f64>() * factor)
}

/// 对旧尺度技能定义做三类因子缩放（字段级，含 growth 成长参数）。
fn legacy_scale_def(mut d: SkillDef) -> SkillDef {
    use SkillEffect::*;
    let e = d.effect;
    d.effect = match e {
        Boost { duration } => Boost { duration },
        ReflectShield { duration } => ReflectShield { duration },
        Shadow => Shadow,
        FakeSetup { max_time } => FakeSetup { max_time },
        Blink { max_distance } => Blink { max_distance: sc(max_distance, LEGACY_DIST) },
        Blink2 { max_distance } => Blink2 { max_distance: sc(max_distance, LEGACY_DIST) },
        BlinkToWall { max_distance } => BlinkToWall { max_distance: sc(max_distance, LEGACY_DIST) },
        DashSlash { speed } => DashSlash { speed: sc(speed, LEGACY_SPEED) },
        DashStrike { speed, duration, push_power, push_time, push_damage } => DashStrike {
            speed: sc(speed, LEGACY_SPEED),
            duration,
            push_power: sc(push_power, LEGACY_SPEED),
            push_time,
            push_damage,
        },
        Rock { max_distance, fuse, radius, damage, bomb_force } => Rock {
            max_distance: sc(max_distance, LEGACY_DIST),
            fuse,
            radius: sc(radius, LEGACY_RADIUS),
            damage,
            bomb_force: sc(bomb_force, LEGACY_SPEED),
        },
        Bullet { speed, damage, radius, range } => Bullet {
            speed: sc(speed, LEGACY_SPEED),
            damage,
            radius: sc(radius, LEGACY_RADIUS),
            range: sc(range, LEGACY_DIST),
        },
        Missile { speed, radius, damage, push_power, push_time, range } => Missile {
            speed: sc(speed, LEGACY_SPEED),
            radius: sc(radius, LEGACY_RADIUS),
            damage,
            push_power: sc(push_power, LEGACY_SPEED),
            push_time,
            range: sc(range, LEGACY_DIST),
        },
        Boomerang { speed, accelerate, radius, damage, push_power, push_time, life } => Boomerang {
            speed: sc(speed, LEGACY_SPEED),
            accelerate: sc(accelerate, LEGACY_SPEED),
            radius: sc(radius, LEGACY_RADIUS),
            damage,
            push_power: sc(push_power, LEGACY_SPEED),
            push_time,
            life,
        },
        Banana { count, turn_rad, speed, radius, damage, push_power, push_time, life } => Banana {
            count,
            turn_rad,
            speed: sc(speed, LEGACY_SPEED),
            radius: sc(radius, LEGACY_RADIUS),
            damage,
            push_power: sc(push_power, LEGACY_SPEED),
            push_time,
            life,
        },
        LineBeam { length, width, damage, duration } => LineBeam {
            length: sc(length, LEGACY_DIST),
            width: sc(width, LEGACY_RADIUS),
            damage,
            duration,
        },
        ChainLeech { speed, damage, heal, range } => ChainLeech {
            speed: sc(speed, LEGACY_SPEED),
            damage,
            heal,
            range: sc(range, LEGACY_DIST),
        },
        JumpDecay { speed, damage, range, ratio_decay } => JumpDecay {
            speed: sc(speed, LEGACY_SPEED),
            damage,
            range: sc(range, LEGACY_DIST),
            ratio_decay,
        },
        TurnLeech { speed, damage, heal, turn_delay, range } => TurnLeech {
            speed: sc(speed, LEGACY_SPEED),
            damage,
            heal,
            turn_delay,
            range: sc(range, LEGACY_DIST),
        },
        Volley { bullet_speed, damage, count, spread_step } => Volley {
            bullet_speed: sc(bullet_speed, LEGACY_SPEED),
            damage,
            count,
            spread_step,
        },
        Sweep { bullet_speed, damage, count, cadence, turn_step } => Sweep {
            bullet_speed: sc(bullet_speed, LEGACY_SPEED),
            damage,
            count,
            cadence,
            turn_step,
        },
        BonusChain { speed, damage, range } => BonusChain {
            speed: sc(speed, LEGACY_SPEED),
            damage,
            range: sc(range, LEGACY_DIST),
        },
        Tether { damage, pull_speed, duration, beam } => Tether {
            damage,
            pull_speed: sc(pull_speed, LEGACY_SPEED),
            duration,
            beam,
        },
        PushShot { speed, damage, push_power, push_time, range } => PushShot {
            speed: sc(speed, LEGACY_SPEED),
            damage,
            push_power: sc(push_power, LEGACY_SPEED),
            push_time,
            range: sc(range, LEGACY_DIST),
        },
        Lightning => Lightning,
        Swap { max_distance } => Swap { max_distance: sc(max_distance, LEGACY_DIST) },
        BindLine { speed, count, bind_time } => BindLine {
            speed: sc(speed, LEGACY_SPEED),
            count,
            bind_time,
        },
        GravityZone { speed, pull_speed, radius, life, range } => GravityZone {
            speed: sc(speed, LEGACY_SPEED),
            pull_speed: sc(pull_speed, LEGACY_SPEED),
            radius: sc(radius, LEGACY_RADIUS),
            life,
            range: sc(range, LEGACY_DIST),
        },
        StarZone { damage_per_sec, heal_per_sec, radius, duration, range } => StarZone {
            damage_per_sec,
            heal_per_sec,
            radius: sc(radius, LEGACY_RADIUS),
            duration,
            range: sc(range, LEGACY_DIST),
        },
        StealthPush { duration, push_power, push_time, push_damage } => StealthPush {
            duration,
            push_power: sc(push_power, LEGACY_SPEED),
            push_time,
            push_damage,
        },
        StealthPush2 { duration, push_power, push_time, push_damage } => StealthPush2 {
            duration,
            push_power: sc(push_power, LEGACY_SPEED),
            push_time,
            push_damage,
        },
        RollProjectile { speed, damage_per_sec, radius, range } => RollProjectile {
            speed: sc(speed, LEGACY_SPEED),
            damage_per_sec,
            radius: sc(radius, LEGACY_RADIUS),
            range: sc(range, LEGACY_DIST),
        },
        ScatterBurst { speed, range, count, step_rad, bullet_speed } => ScatterBurst {
            speed: sc(speed, LEGACY_SPEED),
            range: sc(range, LEGACY_DIST),
            count,
            step_rad,
            bullet_speed: sc(bullet_speed, LEGACY_SPEED),
        },
        ScatterPeriodic { speed, range, count, interval, bullet_speed, turn_rad } => ScatterPeriodic {
            speed: sc(speed, LEGACY_SPEED),
            range: sc(range, LEGACY_DIST),
            count,
            interval,
            bullet_speed: sc(bullet_speed, LEGACY_SPEED),
            turn_rad,
        },
        SelfExplode { radius, self_stay, damage, kick, kick_time } => SelfExplode {
            radius: sc(radius, LEGACY_RADIUS),
            self_stay,
            damage,
            kick: sc(kick, LEGACY_SPEED),
            kick_time,
        },
        Unimplemented => Unimplemented,
        // 098b 名册已是 war3 尺度，透传不缩放（PORT_098B_DECISIONS.md D4）。
        w @ Warlock098b { .. } => w,
        b @ W098bBolt { .. } => b,
        u @ W098bUtility { .. } => u,
    };
    // growth：速度/击退力/距离/半径类基础与斜率同比放大；时长/伤害/冷却不动。
    let g = &mut d.growth;
    g.speed_base *= LEGACY_SPEED;
    g.push_power_base *= LEGACY_SPEED;
    g.push_power_delta *= LEGACY_SPEED;
    g.range_base *= LEGACY_DIST;
    g.max_distance_base *= LEGACY_DIST;
    g.max_distance_delta *= LEGACY_DIST;
    g.radius_base *= LEGACY_RADIUS;
    g.radius_delta *= LEGACY_RADIUS;
    d
}

/// 技能定义表（初期实现：C / R / E 三棵树）。
///
/// 数值参考原版（R1/R2/R3b/E1/E2/C1/C3/C4）；前摇/后摇为本重写新增的
/// 网络手感补偿设计，先在原地取保守值，后续再调斜率。
pub struct DefTable;

impl DefTable {
    pub fn def(id: SkillId) -> SkillDef {
        // 098b 名册：数值已是 war3 尺度（port_spec_098b.json），直通返回，不经 legacy 缩放。
        if let Some(d) = Self::warlock098b_def(id) {
            return d;
        }
        legacy_scale_def(Self::raw_def(id))
    }

    /// 098b 名册定义（M1：S000/S003/S004）。每个条目的数值都注明 spec 来源，
    /// 改动前先回 `port_098b/data/port_spec_098b.json` 与 `01_技能/abilities_consolidated_098b.md` 对账。
    fn warlock098b_def(id: SkillId) -> Option<SkillDef> {
        use SkillEffect::*;
        let def = match id {
            // S000 火球（G 键）——spec: CD 4.8 恒定 24 级；speed 1000 / radius 25 / life (1+.1*oi)=1.0s；
            // 伤害 gX = 6.3+.7*Xv（consolidated S000 行），JI = 1.1*eb（M1 eb=1）；
            // 点燃 xc：总量 (6+1.5*等级+xi)*jn²，时长 2.5*jn（M1 xi=0, jn=1, L1 总量 7.5）。
            SkillId::S000 => SkillDef {
                id,
                tree: SkillTree::G,
                name: "火球",
                needs_point: true,
                effect: Warlock098b {
                    proj: W098bProjKind::Straight,
                    speed: Fix64::from_num(1000.0),
                    radius: Fix64::from_num(25.0),
                    life: Fix64::from_num(1.0),
                    kb_ji: Fix64::from_num(1.1),
                    ignite: Some(Fix64::from_num(7.5)),
                    blast: None,
                    count: 1,
                    spread_step: 0.0,
                    on_hit: W098bOnHit::Ki,
                },
                growth: SkillGrowth {
                    cooldown_base: 4.8,
                    // growth 语义 = base + delta×(L-1)，base 填 L1 值：gX = 6.3+0.7×L（L 从 1 起）→ L1=7.0。
                    damage_base: 7.0,
                    damage_delta: 0.7,
                    // 点燃总量 = 6+1.5×L → L1=7.5。
                    extra_base: 7.5,
                    extra_delta: 1.5,
                    ..DEF_ZERO
                },
            },
            // S003 追踪弹（D 键）——spec: CD 15→9.5（9 级，步长 -0.6875）；speed 900 / radius 29；
            // life 4.5*(1+1.5*.1*oi)=4.5s；伤害 gX = jb(Er)（M1 用 6+0.5*Xv 近似，consolidated 标注公式 jb 未解码）。
            SkillId::S003 => SkillDef {
                id,
                tree: SkillTree::D,
                name: "追踪弹",
                needs_point: true,
                effect: Warlock098b {
                    proj: W098bProjKind::Homing,
                    speed: Fix64::from_num(900.0),
                    radius: Fix64::from_num(29.0),
                    life: Fix64::from_num(4.5),
                    kb_ji: Fix64::ONE,
                    ignite: None,
                    blast: None,
                    count: 1,
                    spread_step: 0.0,
                    on_hit: W098bOnHit::Ki,
                },
                growth: SkillGrowth {
                    cooldown_base: 15.0,
                    cooldown_delta: -0.6875,
                    // gX = jb(Er) 未解码，M1 近似 6+0.5×L → L1=6.5。
                    damage_base: 6.5,
                    damage_delta: 0.5,
                    ..DEF_ZERO
                },
            },
            // S004 回旋镖（D 键）——spec: CD 16→8.2（9 级，步长 -0.975）；radius 40；
            // 伤害 gX = 6.4+.8*Xv（consolidated S004 行，直伤）+ qI 区域二次 0.5*mI（M2 补）；
            // 速度：098b 为侧向分量公式（spec note），M1 用「出 400 / 回拉加速」近似，speed 字段存初速。
            SkillId::S004 => SkillDef {
                id,
                tree: SkillTree::D,
                name: "回旋镖",
                needs_point: true,
                effect: Warlock098b {
                    proj: W098bProjKind::Boomerang,
                    speed: Fix64::from_num(700.0),
                    radius: Fix64::from_num(40.0),
                    life: Fix64::from_num(1.6),
                    kb_ji: Fix64::ONE,
                    ignite: None,
                    blast: None,
                    count: 1,
                    spread_step: 0.0,
                    on_hit: W098bOnHit::Ki,
                },
                growth: SkillGrowth {
                    cooldown_base: 16.0,
                    cooldown_delta: -0.975,
                    // gX = 6.4+0.8×L → L1=7.2。
                    damage_base: 7.2,
                    damage_delta: 0.8,
                    ..DEF_ZERO
                },
            },
            // S002 闪电（D 键）——detailed hb：伤害 DX=6+1×Wr（随等级，growth.damage L1=7），
            // 射程 (1+0.15*oi)×600 = 600（oi=0）；CD 16.5→12（9 级，步长 -0.5625）。
            SkillId::S002 => SkillDef {
                id,
                tree: SkillTree::D,
                name: "闪电",
                needs_point: true,
                effect: W098bBolt { range: Fix64::from_num(600.0), kb_ji: Fix64::ONE },
                growth: SkillGrowth {
                    cooldown_base: 16.5,
                    cooldown_delta: -0.5625,
                    damage_base: 7.0,
                    damage_delta: 1.0,
                    ..DEF_ZERO
                },
            },
            // S008 陨石（E 键）——spec：CD 20→16.5（16 个 CD 档/max 20 级，M1 按 20 级线性步长 -0.183）；
            // speed 400 / radius 72 / life 2s / cast_range 1200 / aoe 200；
            // detailed XB：KI($A+2*Xv, .8)，$A=10 → gX=10+2L（L1=12）；灼烧 nB 时长 4s（数值未解码，TODO M2）。
            SkillId::S008 => SkillDef {
                id,
                tree: SkillTree::E,
                name: "陨石",
                needs_point: true,
                effect: Warlock098b {
                    proj: W098bProjKind::Straight,
                    speed: Fix64::from_num(400.0),
                    radius: Fix64::from_num(72.0),
                    life: Fix64::from_num(2.0),
                    kb_ji: Fix64::from_num(0.8),
                    ignite: None,
                    blast: Some(Fix64::from_num(200.0)),
                    count: 1,
                    spread_step: 0.0,
                    on_hit: W098bOnHit::Ki,
                },
                growth: SkillGrowth {
                    cooldown_base: 20.0,
                    cooldown_delta: -0.183,
                    damage_base: 12.0,
                    damage_delta: 2.0,
                    ..DEF_ZERO
                },
            },
            // S009 分裂弹（E 键）——spec：CD 30→20（20 级，步长 -0.526）；
            // detailed gB：ev=GB/280（GB 未解码，M1 speed 近似 900）、radius 50、impact fB=KI(3, 1.4)（伤害固定）。
            SkillId::S009 => SkillDef {
                id,
                tree: SkillTree::E,
                name: "分裂弹",
                needs_point: true,
                effect: Warlock098b {
                    proj: W098bProjKind::Straight,
                    speed: Fix64::from_num(900.0),
                    radius: Fix64::from_num(50.0),
                    life: Fix64::from_num(2.0),
                    kb_ji: Fix64::from_num(1.4),
                    ignite: None,
                    blast: None,
                    count: 1,
                    spread_step: 0.0,
                    on_hit: W098bOnHit::Ki,
                },
                growth: SkillGrowth {
                    cooldown_base: 30.0,
                    cooldown_delta: -0.526,
                    damage_base: 3.0,
                    damage_delta: 0.0,
                    ..DEF_ZERO
                },
            },
            // S014 汲取（T 键）——spec：CD 22→18.5（20 级，步长 -0.184）；speed 700 / radius 27 / life Ar/700（M1 取 3s）；
            // detailed ic：双段 KI(yO,.2)/KI(yO,.6)（yO 未解码，M1 近似 6+0.5L，双段合并 JI=0.8）；
            // 吸血/减速（S032 切换形态）TODO M2。
            SkillId::S014 => SkillDef {
                id,
                tree: SkillTree::T,
                name: "汲取",
                needs_point: true,
                effect: Warlock098b {
                    proj: W098bProjKind::Straight,
                    speed: Fix64::from_num(700.0),
                    radius: Fix64::from_num(27.0),
                    life: Fix64::from_num(3.0),
                    kb_ji: Fix64::from_num(0.8),
                    ignite: None,
                    blast: None,
                    count: 1,
                    spread_step: 0.0,
                    on_hit: W098bOnHit::Ki,
                },
                growth: SkillGrowth {
                    cooldown_base: 22.0,
                    cooldown_delta: -0.184,
                    damage_base: 6.5,
                    damage_delta: 0.5,
                    ..DEF_ZERO
                },
            },
            // S015 火焰喷射（T 键）——spec：CD 16→7（20 级，步长 -0.474）；
            // control Ic 非升级=0.08s 点脉冲 / 升级=锥形 5 道偏转 5.5°（M1 先做锥形近似）；
            // detailed：speed 800 / radius 22 / life 800/900≈0.89s；nc=jI(2.6+0.4Xv, .65)。
            SkillId::S015 => SkillDef {
                id,
                tree: SkillTree::T,
                name: "火焰喷射",
                needs_point: true,
                effect: Warlock098b {
                    proj: W098bProjKind::Straight,
                    speed: Fix64::from_num(800.0),
                    radius: Fix64::from_num(22.0),
                    life: Fix64::from_num(0.89),
                    kb_ji: Fix64::from_num(0.65),
                    ignite: None,
                    blast: None,
                    count: 5,
                    spread_step: 5.5_f64.to_radians(),
                    on_hit: W098bOnHit::Ki,
                },
                growth: SkillGrowth {
                    cooldown_base: 16.0,
                    cooldown_delta: -0.474,
                    damage_base: 3.0,
                    damage_delta: 0.4,
                    ..DEF_ZERO
                },
            },
            // S016 弹跳弹（T 键）——spec：CD 20 恒定（l1=lmax=20）；speed 900 / radius 35 / life 1s；
            // detailed gc（基础形态）：KI(gv×(5+Xv))（gv 未解码取 1 → gX=5+L，L1=6）；每跳 ×0.8。
            SkillId::S016 => SkillDef {
                id,
                tree: SkillTree::T,
                name: "弹跳弹",
                needs_point: true,
                effect: Warlock098b {
                    proj: W098bProjKind::Bounce,
                    speed: Fix64::from_num(900.0),
                    radius: Fix64::from_num(35.0),
                    life: Fix64::from_num(1.0),
                    kb_ji: Fix64::ONE,
                    ignite: None,
                    blast: None,
                    count: 1,
                    spread_step: 0.0,
                    on_hit: W098bOnHit::Ki,
                },
                growth: SkillGrowth {
                    cooldown_base: 20.0,
                    damage_base: 6.0,
                    damage_delta: 1.0,
                    ..DEF_ZERO
                },
            },
            // ===== M2 批次B：位移/增益系（数值来源 spec/control/durations，见各条目注释） =====
            // S005 反射盾（C 键）——spec：CD 25→14（9 级，步长 -1.375）；dur=(2.6+.2*vi)*jn → L1 2.8。
            SkillId::S005 => SkillDef {
                id,
                tree: SkillTree::C,
                name: "反射盾",
                needs_point: false,
                effect: W098bUtility { kind: W098bUtilKind::Reflect, speed: Fix64::ZERO, max_distance: Fix64::ZERO },
                growth: SkillGrowth {
                    cooldown_base: 25.0,
                    cooldown_delta: -1.375,
                    duration_base: 2.8,
                    duration_delta: 0.2,
                    ..DEF_ZERO
                },
            },
            // S006 时光回溯（C 键）——spec：CD 22→12（8 级，步长 -1.4286）；delay=3.6*jn 恒定。
            SkillId::S006 => SkillDef {
                id,
                tree: SkillTree::C,
                name: "时光回溯",
                needs_point: false,
                effect: W098bUtility { kind: W098bUtilKind::Rewind, speed: Fix64::ZERO, max_distance: Fix64::ZERO },
                growth: SkillGrowth {
                    cooldown_base: 22.0,
                    cooldown_delta: -1.4286,
                    duration_base: 3.6,
                    ..DEF_ZERO
                },
            },
            // S007 急行（C 键）——spec：CD 21→13（20 级，步长 -0.421）；dur=(6.2+.8*vi)*jn → L1 7.0；
            // 移速 +35（war3 加法 → 乘数 1+35/210）；攻速 tr 无对应系统（TODO M2 后续）。
            SkillId::S007 => SkillDef {
                id,
                tree: SkillTree::C,
                name: "急行",
                needs_point: false,
                effect: W098bUtility {
                    kind: W098bUtilKind::Haste,
                    speed: Fix64::from_num(1.0 + 35.0 / 210.0),
                    max_distance: Fix64::ZERO,
                },
                growth: SkillGrowth {
                    cooldown_base: 21.0,
                    cooldown_delta: -0.421,
                    duration_base: 7.0,
                    duration_delta: 0.8,
                    ..DEF_ZERO
                },
            },
            // S010 疾风步（E 键）——spec：CD 30→17（20 级，步长 -0.684）；dur=3.1*jn（基础形态）；
            // 隐身 +200 移速（乘数 1+200/210）；破隐一击（bA 复合 KI）TODO M2 后续。
            SkillId::S010 => SkillDef {
                id,
                tree: SkillTree::E,
                name: "疾风步",
                needs_point: false,
                effect: W098bUtility {
                    kind: W098bUtilKind::Windwalk,
                    speed: Fix64::from_num(1.0 + 200.0 / 210.0),
                    max_distance: Fix64::ZERO,
                },
                growth: SkillGrowth {
                    cooldown_base: 30.0,
                    cooldown_delta: -0.684,
                    duration_base: 3.1,
                    ..DEF_ZERO
                },
            },
            // S011 瞬间移动（R 键）——spec：CD 16→5.5（9 级，步长 -1.3125）；距离 700+70*Yr → L1 770。
            SkillId::S011 => SkillDef {
                id,
                tree: SkillTree::R,
                name: "瞬间移动",
                needs_point: true,
                effect: W098bUtility { kind: W098bUtilKind::Blink, speed: Fix64::ZERO, max_distance: Fix64::from_num(770.0) },
                growth: SkillGrowth {
                    cooldown_base: 16.0,
                    cooldown_delta: -1.3125,
                    max_distance_base: 770.0,
                    max_distance_delta: 70.0,
                    ..DEF_ZERO
                },
            },
            // S012 冲撞（R 键）——spec：CD 16.5→8.0（20 级，步长 -0.447）；速度 Hr=1300/s 恒定；
            // 最大距离 (650+50*Yr)*(1+.1*oi) → L1 770、+55/级；命中 0.5s 定身。
            // 伤害 bA 复合公式（4.6+.8yr / 5+.4Yr 三段）M1 简化为 5+0.4L → L1 5.4（TODO 对齐三段）。
            SkillId::S012 => SkillDef {
                id,
                tree: SkillTree::R,
                name: "冲撞",
                needs_point: true,
                effect: W098bUtility { kind: W098bUtilKind::Dash, speed: Fix64::from_num(1300.0), max_distance: Fix64::from_num(770.0) },
                growth: SkillGrowth {
                    cooldown_base: 16.5,
                    cooldown_delta: -0.447,
                    max_distance_base: 770.0,
                    max_distance_delta: 55.0,
                    damage_base: 5.4,
                    damage_delta: 0.4,
                    ..DEF_ZERO
                },
            },
            // S013 移形换位（R 键）——spec：CD 16→4.0（20 级，步长 -0.6316）；射程 600*(1+.1*oi)=660。
            // 098b 为弹体命中换位（speed 800/radius 40），M1 简化为即时换位（弹体化 TODO）。
            SkillId::S013 => SkillDef {
                id,
                tree: SkillTree::R,
                name: "移形换位",
                needs_point: true,
                effect: W098bUtility { kind: W098bUtilKind::Swap, speed: Fix64::ZERO, max_distance: Fix64::from_num(660.0) },
                growth: SkillGrowth {
                    cooldown_base: 16.0,
                    cooldown_delta: -0.6316,
                    max_distance_base: 660.0,
                    ..DEF_ZERO
                },
            },
            // ===== M2 批次C：场/线控制系 =====
            // S017 致残（Y 键）——spec：CD 25→12.5（20 级，步长 -0.658）；speed 900 / radius 23；
            // durations：残废 (4+0.25L)*jn → L1 4.25；eC 的 ri>0 AoE 分支无属性系统（TODO）；
            // 伤害 MI 公式未解码 → M1 恒 3 占位（TODO）。
            SkillId::S017 => SkillDef {
                id,
                tree: SkillTree::Y,
                name: "致残",
                needs_point: true,
                effect: Warlock098b {
                    proj: W098bProjKind::Straight,
                    speed: Fix64::from_num(900.0),
                    radius: Fix64::from_num(23.0),
                    life: Fix64::from_num(2.0),
                    kb_ji: Fix64::ONE,
                    ignite: None,
                    blast: None,
                    count: 1,
                    spread_step: 0.0,
                    on_hit: W098bOnHit::Cripple,
                },
                growth: SkillGrowth {
                    cooldown_base: 25.0,
                    cooldown_delta: -0.658,
                    damage_base: 3.0,
                    duration_base: 4.25,
                    duration_delta: 0.25,
                    ..DEF_ZERO
                },
            },
            // S018 引力（Y 键）——spec：CD 26 恒定（20 级）；speed 850 / aoe 200 / 漩涡 5*jn。
            // 复用现有 GravityZone 原型（飞行场沿途吸拉）——该臂的 speed/radius/duration/range
            // 读 growth（stats），数值故放 growth；仅 pull_speed 走 effect 字段（spec 未给，占位 300 TODO）。
            // 098b 升级版为「落点原地漩涡」，差异 TODO M2 后续标定。
            SkillId::S018 => SkillDef {
                id,
                tree: SkillTree::Y,
                name: "引力",
                needs_point: true,
                effect: GravityZone {
                    speed: Fix64::ZERO,
                    pull_speed: Fix64::from_num(300.0),
                    radius: Fix64::ZERO,
                    life: 0.0,
                    range: Fix64::ZERO,
                },
                growth: SkillGrowth {
                    cooldown_base: 26.0,
                    // speed 850 是 098b 弹体飞行速度（飞向落点）；GravityZone 原型的 speed 是「场漂移速度」
                    // ——语义不同。贴 098b 升级版（落点原地漩涡 5s）取 0（场不漂移）；飞行段弹体化 TODO。
                    speed_base: 0.0,
                    radius_base: 200.0,
                    duration_base: 5.0,
                    range_base: 1200.0,
                    ..DEF_ZERO
                },
            },
            // S019 锁链（Y 键）——spec：CD 17→16（20 级，步长 -0.0526）；radius 35；
            // speed 未给（VengeanceMissile 类）→ M1 占位 800；命中拉向施法者 + Tied 0.5s
            //（098b 拉拽+链光+S031 附加动作 TODO）；伤害 KI 公式未解码 → 恒 3 占位（TODO）。
            SkillId::S019 => SkillDef {
                id,
                tree: SkillTree::Y,
                name: "锁链",
                needs_point: true,
                effect: Warlock098b {
                    proj: W098bProjKind::Straight,
                    speed: Fix64::from_num(800.0),
                    radius: Fix64::from_num(35.0),
                    life: Fix64::from_num(2.0),
                    kb_ji: Fix64::ONE,
                    ignite: None,
                    blast: None,
                    count: 1,
                    spread_step: 0.0,
                    on_hit: W098bOnHit::ChainPull,
                },
                growth: SkillGrowth {
                    cooldown_base: 17.0,
                    cooldown_delta: -0.0526,
                    damage_base: 3.0,
                    duration_base: 0.5,
                    ..DEF_ZERO
                },
            },
            _ => return None,
        };
        Some(def)
    }

    /// 旧（Unity demo）尺度的原始定义表；仅被 [`Self::def`] 的过渡缩放消费。
    fn raw_def(id: SkillId) -> SkillDef {
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
                    duration_base: 0.5,
                    speed_base: 8.0,
                    push_power_base: 6.0,
                    push_power_delta: 1.0,
                    push_time_base: 0.6,
                    push_damage_base: 8.0,
                    push_damage_delta: 2.0,
                    ..DEF_ZERO
                },
            },
            SkillId::Blink2 => SkillDef {
                id,
                tree: SkillTree::R,
                name: "二段闪",
                needs_point: true,
                effect: Blink2 { max_distance: Fix64::from_num(5.0) },
                growth: SkillGrowth { cooldown_base: 6.0, max_distance_base: 5.0, duration_base: 2.0, ..DEF_ZERO }, // duration = 二段可用窗口
            },
            SkillId::DashSlash => SkillDef {
                id,
                tree: SkillTree::R,
                name: "冲刺斩",
                needs_point: true,
                effect: DashSlash { speed: Fix64::from_num(15.0) },
                growth: SkillGrowth { windup_base: 0.1, recovery_base: 0.1, cooldown_base: 5.0, speed_base: 15.0, ..DEF_ZERO },
            },
            SkillId::BlinkToWall => SkillDef {
                id,
                tree: SkillTree::R,
                name: "闪到墙",
                needs_point: true,
                effect: BlinkToWall { max_distance: Fix64::from_num(6.0) },
                growth: SkillGrowth { cooldown_base: 3.0, max_distance_base: 6.0, ..DEF_ZERO },
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
            // E 树：掷弹 = 滚动火球（持续接触 DoT）；火球（D 树）用 Bullet。
            SkillId::StoneShot => SkillDef {
                id,
                tree: SkillTree::E,
                name: "掷弹",
                needs_point: true,
                effect: RollProjectile {
                    speed: Fix64::from_num(6.0),
                    damage_per_sec: Fix64::from_num(2.0),
                    radius: Fix64::from_num(0.7),
                    range: Fix64::from_num(12.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.15,
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    cooldown_delta: -0.2,
                    damage_base: 2.0,
                    damage_delta: 0.5,
                    speed_base: 6.0,
                    range_base: 12.0,
                    radius_base: 0.7,
                    max_distance_delta: 1.0,
                    ..DEF_ZERO
                },
            },
            // E 树：潜行踢·连推（撞障碍后重新触发）。
            SkillId::StealthPush2 => SkillDef {
                id,
                tree: SkillTree::E,
                name: "潜行踢·连推",
                needs_point: false,
                effect: StealthPush2 {
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
            // E 树：撒弹线（E3 = 到终点爆散射击）。
            SkillId::LineBeam => SkillDef {
                id,
                tree: SkillTree::E,
                name: "撒弹线",
                needs_point: true,
                effect: ScatterBurst {
                    speed: Fix64::from_num(6.0),
                    range: Fix64::from_num(8.0),
                    count: 8,
                    step_rad: std::f64::consts::FRAC_PI_4 * 0.11, // 8 发小步进
                    bullet_speed: Fix64::from_num(6.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 6.0,
                    cooldown_delta: -0.3,
                    range_base: 8.0,
                    speed_base: 6.0,
                    ..DEF_ZERO
                },
            },
            // E 树：撒弹线·E3b（沿途周期性散射击）。
            SkillId::LineExplode => SkillDef {
                id,
                tree: SkillTree::E,
                name: "散射弹线",
                needs_point: true,
                effect: ScatterPeriodic {
                    speed: Fix64::from_num(6.0),
                    range: Fix64::from_num(8.0),
                    count: 10,
                    interval: 0.2,
                    bullet_speed: Fix64::from_num(6.0),
                    turn_rad: 0.1,
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 6.0,
                    cooldown_delta: -0.3,
                    range_base: 8.0,
                    speed_base: 6.0,
                    ..DEF_ZERO
                },
            },
            // D 树：回旋镖火球（D2）
            SkillId::D2Fireball => SkillDef {
                id,
                tree: SkillTree::D,
                name: "回旋镖",
                needs_point: true,
                effect: Boomerang {
                    speed: Fix64::from_num(8.0),
                    accelerate: Fix64::from_num(0.2),
                    radius: Fix64::from_num(1.0),
                    damage: Fix64::from_num(12.0),
                    push_power: Fix64::from_num(8.0),
                    push_time: Fix64::from_num(1.0),
                    life: Fix64::from_num(3.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.12,
                    recovery_base: 0.1,
                    cooldown_base: 2.5,
                    cooldown_delta: -0.2,
                    damage_base: 12.0,
                    damage_delta: 3.0,
                    speed_base: 8.0,
                    push_power_base: 8.0,
                    radius_base: 1.0,
                    push_time_base: 1.0,
                    duration_base: 3.0,
                    ..DEF_ZERO
                },
            },
            // D 树：追踪导弹（锁定点击处最近）
            SkillId::D3Missile => SkillDef {
                id,
                tree: SkillTree::D,
                name: "导弹",
                needs_point: true,
                effect: Missile {
                    speed: Fix64::from_num(7.0),
                    radius: Fix64::from_num(1.6),
                    damage: Fix64::from_num(18.0),
                    push_power: Fix64::from_num(9.0),
                    push_time: Fix64::from_num(1.0),
                    range: Fix64::from_num(12.0),
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
                    push_power_base: 9.0,
                    push_time_base: 1.0,
                    range_base: 12.0,
                    ..DEF_ZERO
                },
            },
            // D 树：双香蕉曲线弹（D4）
            SkillId::D4Fireball => SkillDef {
                id,
                tree: SkillTree::D,
                name: "香蕉弹",
                needs_point: true,
                effect: Banana {
                    count: 2,
                    turn_rad: std::f64::consts::FRAC_PI_4,
                    speed: Fix64::from_num(8.0),
                    radius: Fix64::from_num(1.0),
                    damage: Fix64::from_num(10.0),
                    push_power: Fix64::from_num(5.0),
                    push_time: Fix64::from_num(1.0),
                    life: Fix64::from_num(2.5),
                },
                growth: SkillGrowth {
                    windup_base: 0.15,
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    damage_base: 10.0,
                    damage_delta: 2.0,
                    speed_base: 8.0,
                    push_power_base: 5.0,
                    radius_base: 1.0,
                    push_time_base: 1.0,
                    duration_base: 2.5,
                    ..DEF_ZERO
                },
            },
            // T 树：吸血链镖（T1b）
            SkillId::TLeech => SkillDef {
                id,
                tree: SkillTree::T,
                name: "吸血链镖",
                needs_point: true,
                effect: ChainLeech {
                    speed: Fix64::from_num(12.0),
                    damage: Fix64::from_num(5.0),
                    heal: Fix64::from_num(5.0),
                    range: Fix64::from_num(12.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    damage_base: 5.0,
                    damage_delta: 1.0,
                    speed_base: 12.0,
                    ..DEF_ZERO
                },
            },
            // T 树：扇扫连射（T2）
            SkillId::T2Shot => SkillDef {
                id,
                tree: SkillTree::T,
                name: "扇扫连射",
                needs_point: true,
                effect: Sweep {
                    bullet_speed: Fix64::from_num(8.0),
                    damage: Fix64::from_num(8.0),
                    count: 8,
                    cadence: 0.1,
                    turn_step: 0.1,
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 4.0,
                    damage_base: 8.0,
                    damage_delta: 1.5,
                    speed_base: 8.0,
                    ..DEF_ZERO
                },
            },
            // T 树：扇面齐射（T2b）
            SkillId::T2Volley => SkillDef {
                id,
                tree: SkillTree::T,
                name: "扇面齐射",
                needs_point: true,
                effect: Volley {
                    bullet_speed: Fix64::from_num(8.0),
                    damage: Fix64::from_num(8.0),
                    count: 4,
                    spread_step: std::f64::consts::FRAC_PI_8,
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    damage_base: 8.0,
                    damage_delta: 1.5,
                    speed_base: 8.0,
                    ..DEF_ZERO
                },
            },
            // T 树：跳弹·衰减（T3）
            SkillId::T3Fast => SkillDef {
                id,
                tree: SkillTree::T,
                name: "跳弹",
                needs_point: true,
                effect: JumpDecay {
                    speed: Fix64::from_num(20.0),
                    damage: Fix64::from_num(5.0),
                    range: Fix64::from_num(12.0),
                    ratio_decay: Fix64::from_num(0.2),
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    damage_base: 5.0,
                    damage_delta: 1.0,
                    speed_base: 20.0,
                    ..DEF_ZERO
                },
            },
            // T 树：跳弹·蓄力（T3b）
            SkillId::T3Fast2 => SkillDef {
                id,
                tree: SkillTree::T,
                name: "蓄力跳弹",
                needs_point: true,
                effect: BonusChain {
                    speed: Fix64::from_num(15.0),
                    damage: Fix64::from_num(5.0),
                    range: Fix64::from_num(12.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    damage_base: 5.0,
                    damage_delta: 1.0,
                    speed_base: 15.0,
                    range_base: 12.0,
                    ..DEF_ZERO
                },
            },
            // T 树：转镖吸血（TestLeech）
            SkillId::TestLeech => SkillDef {
                id,
                tree: SkillTree::T,
                name: "转镖",
                needs_point: true,
                effect: TurnLeech {
                    speed: Fix64::from_num(6.0),
                    damage: Fix64::from_num(10.0),
                    heal: Fix64::from_num(10.0),
                    turn_delay: Fix64::from_num(0.3), // 先直线飞 0.3s 再转向最近敌人（转镖手感）
                    range: Fix64::from_num(12.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    damage_base: 10.0,
                    damage_delta: 2.0,
                    speed_base: 6.0,
                    ..DEF_ZERO
                },
            },
            // Y 树：蓝线回拉（Y1）
            SkillId::Y1BlueLine => SkillDef {
                id,
                tree: SkillTree::Y,
                name: "蓝线回拉",
                needs_point: true,
                effect: Tether {
                    damage: Fix64::from_num(2.0),
                    pull_speed: Fix64::from_num(2.0),
                    duration: Fix64::from_num(2.0),
                    beam: false,
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    damage_base: 2.0,
                    damage_delta: 0.4,
                    speed_base: 2.0,
                    duration_base: 2.0,
                    ..DEF_ZERO
                },
            },
            // Y 树：红线回拉+扇伤（Y1b）
            SkillId::Y1BlueLine2 => SkillDef {
                id,
                tree: SkillTree::Y,
                name: "红线回拉",
                needs_point: true,
                effect: Tether {
                    damage: Fix64::from_num(2.0),
                    pull_speed: Fix64::from_num(2.0),
                    duration: Fix64::from_num(2.0),
                    beam: true,
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    damage_base: 2.0,
                    damage_delta: 0.4,
                    speed_base: 2.0,
                    duration_base: 2.0,
                    ..DEF_ZERO
                },
            },
            // Y 树：撞击迟缓（Y2）
            SkillId::Y2Delay => SkillDef {
                id,
                tree: SkillTree::Y,
                name: "撞击迟缓",
                needs_point: true,
                effect: PushShot {
                    speed: Fix64::from_num(10.0),
                    damage: Fix64::from_num(8.0),
                    push_power: Fix64::from_num(9.0),
                    push_time: Fix64::from_num(2.0),
                    range: Fix64::from_num(12.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    damage_base: 8.0,
                    damage_delta: 1.5,
                    speed_base: 10.0,
                    push_power_base: 9.0,
                    push_time_base: 2.0,
                    range_base: 12.0,
                    ..DEF_ZERO
                },
            },
            // Y 树：束缚线（Y2b）
            SkillId::Y2Suite => SkillDef {
                id,
                tree: SkillTree::Y,
                name: "束缚线",
                needs_point: true,
                effect: BindLine {
                    speed: Fix64::from_num(8.0),
                    count: 2,
                    bind_time: 3.0,
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 6.0,
                    speed_base: 8.0,
                    ..DEF_ZERO
                },
            },
            // Y 树：引力场（Y3）
            SkillId::Y3Zone => SkillDef {
                id,
                tree: SkillTree::Y,
                name: "引力场",
                needs_point: true,
                effect: GravityZone {
                    speed: Fix64::from_num(4.0),
                    pull_speed: Fix64::from_num(2.0),
                    radius: Fix64::from_num(2.5),
                    life: 4.0,
                    range: Fix64::from_num(10.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 4.0,
                    speed_base: 4.0,
                    radius_base: 2.5,
                    duration_base: 4.0,
                    range_base: 10.0,
                    ..DEF_ZERO
                },
            },
            // Y 树：星域持续伤（Y3b）
            SkillId::Y3Zone2 => SkillDef {
                id,
                tree: SkillTree::Y,
                name: "星域",
                needs_point: true,
                effect: StarZone {
                    damage_per_sec: Fix64::from_num(2.0),
                    heal_per_sec: Fix64::from_num(2.0),
                    radius: Fix64::from_num(1.6),
                    duration: 4.0,
                    range: Fix64::from_num(6.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 10.0,
                    damage_base: 2.0,
                    damage_delta: 0.3,
                    radius_base: 1.6,
                    duration_base: 4.0,
                    ..DEF_ZERO
                },
            },
            // F 树：蓄力自爆（Test03）
            SkillId::Test03 => SkillDef {
                id,
                tree: SkillTree::F,
                name: "蓄力自爆",
                needs_point: false,
                effect: SelfExplode {
                    radius: Fix64::from_num(2.0),
                    self_stay: Fix64::from_num(1.0),
                    damage: Fix64::from_num(10.0),
                    kick: Fix64::from_num(9.0),
                    kick_time: Fix64::from_num(1.0),
                },
                growth: SkillGrowth {
                    windup_base: 1.0, // 吟唱 1s
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    radius_base: 2.0,
                    damage_base: 10.0,
                    push_power_base: 9.0,
                    push_time_base: 1.0,
                    ..DEF_ZERO
                },
            },
            // G 树：普通爆炸弹（Test01）
            SkillId::Test01 => SkillDef {
                id,
                tree: SkillTree::G,
                name: "爆炸弹",
                needs_point: true,
                effect: PushShot {
                    speed: Fix64::from_num(8.0),
                    damage: Fix64::from_num(10.0),
                    push_power: Fix64::from_num(8.0),
                    push_time: Fix64::from_num(1.0),
                    range: Fix64::from_num(12.0),
                },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    damage_base: 10.0,
                    damage_delta: 2.0,
                    speed_base: 8.0,
                    push_power_base: 8.0,
                    push_time_base: 1.0,
                    range_base: 12.0,
                    ..DEF_ZERO
                },
            },
            // D 树：雷电（TestLightning）——指向性即时射线，命中敌人伤害+推，撞障碍停止。
            SkillId::TestLightning => SkillDef {
                id,
                tree: SkillTree::D,
                name: "雷电",
                needs_point: true,
                effect: Lightning,
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    damage_base: 10.0,
                    damage_delta: 2.0,
                    range_base: 10.0,
                    push_power_base: 6.0,
                    push_power_delta: 1.0,
                    push_time_base: 1.0,
                    ..DEF_ZERO
                },
            },
            // R 树：换位（TestSwap）——点目标，有敌人则互换位置，否则瞬移过去。
            SkillId::TestSwap => SkillDef {
                id,
                tree: SkillTree::R,
                name: "换位",
                needs_point: true,
                effect: Swap { max_distance: Fix64::from_num(6.0) },
                growth: SkillGrowth {
                    windup_base: 0.1,
                    recovery_base: 0.1,
                    cooldown_base: 3.0,
                    max_distance_base: 6.0,
                    max_distance_delta: 1.0,
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

    // ===== 098b 名册数值对账（PORT_098B_DECISIONS.md M1 §9 数值对账） =====
    // 每个断言都注明 port_spec_098b.json / abilities_consolidated_098b.md 的来源；
    // 改动 def 数值前先回知识库对账，改知识库后同步更新此处。

    #[test]
    fn s000_fireball_matches_spec() {
        // spec S000：CD 4.8 恒定 24 级；speed 1000 / radius 25 / life (1+.1*oi)=1.0；
        // consolidated：gX = 6.3+.7*Xv，JI = 1.1*eb（M1 eb=1）；点燃 = (6+1.5L)*jn²，2.5s。
        let def = DefTable::def(SkillId::S000);
        assert_eq!(def.name, "火球");
        let s1 = def.stats_at(1);
        assert!(near(s1.cooldown, 4.8, 1e-3), "L1 CD 应 4.8，实际 {:?}", s1.cooldown);
        assert!(near(s1.damage, 7.0, 1e-3), "L1 gX 应 6.3+0.7×1=7.0，实际 {:?}", s1.damage);
        let s24 = def.stats_at(24);
        assert!(near(s24.cooldown, 4.8, 1e-3), "L24 CD 应仍 4.8（恒定），实际 {:?}", s24.cooldown);
        assert!(near(s24.damage, 6.3 + 0.7 * 24.0, 1e-3), "L24 gX 应 6.3+0.7×24，实际 {:?}", s24.damage);
        assert!(near(s24.extra, 6.0 + 1.5 * 24.0, 1e-3), "L24 点燃总量应 6+1.5×24，实际 {:?}", s24.extra);
        match def.effect {
            SkillEffect::Warlock098b { proj: W098bProjKind::Straight, speed, radius, life, kb_ji, ignite, .. } => {
                assert!(near(speed, 1000.0, 1e-3), "speed 应 1000（spec），实际 {speed:?}");
                assert!(near(radius, 25.0, 1e-3), "radius 应 25（spec），实际 {radius:?}");
                assert!(near(life, 1.0, 1e-3), "life 应 (1+.1*oi)=1.0，实际 {life:?}");
                assert!(near(kb_ji, 1.1, 1e-3), "JI 应 1.1*eb=1.1，实际 {kb_ji:?}");
                assert!(ignite.is_some(), "火球应带点燃");
            }
            ref e => panic!("S000 effect 应为 Warlock098b(Straight)，实际 {e:?}"),
        }
    }

    #[test]
    fn s003_homing_matches_spec() {
        // spec S003：CD 15→9.5（9 级，步长 -0.6875）；speed 900 / radius 29 / life 4.5。
        let def = DefTable::def(SkillId::S003);
        assert_eq!(def.name, "追踪弹");
        let s1 = def.stats_at(1);
        let s9 = def.stats_at(9);
        assert!(near(s1.cooldown, 15.0, 1e-3), "L1 CD 应 15（spec l1），实际 {:?}", s1.cooldown);
        assert!(near(s9.cooldown, 9.5, 1e-2), "L9 CD 应 9.5（spec lmax），实际 {:?}", s9.cooldown);
        match def.effect {
            SkillEffect::Warlock098b { proj: W098bProjKind::Homing, speed, radius, life, ignite, .. } => {
                assert!(near(speed, 900.0, 1e-3), "speed 应 900（spec），实际 {speed:?}");
                assert!(near(radius, 29.0, 1e-3), "radius 应 Dr=29（spec），实际 {radius:?}");
                assert!(near(life, 4.5, 1e-3), "life 应 4.5*(1+.15*oi)=4.5，实际 {life:?}");
                assert!(ignite.is_none(), "追踪弹无点燃");
            }
            ref e => panic!("S003 effect 应为 Warlock098b(Homing)，实际 {e:?}"),
        }
    }

    #[test]
    fn s004_boomerang_matches_spec() {
        // spec S004：CD 16→8.2（9 级，步长 -0.975）；radius 40；consolidated gX = 6.4+.8*Xv。
        let def = DefTable::def(SkillId::S004);
        assert_eq!(def.name, "回旋镖");
        let s1 = def.stats_at(1);
        let s9 = def.stats_at(9);
        assert!(near(s1.cooldown, 16.0, 1e-3), "L1 CD 应 16（spec l1），实际 {:?}", s1.cooldown);
        assert!(near(s9.cooldown, 8.2, 1e-2), "L9 CD 应 8.2（spec lmax），实际 {:?}", s9.cooldown);
        assert!(near(s1.damage, 7.2, 1e-3), "L1 gX 应 6.4+0.8×1=7.2，实际 {:?}", s1.damage);
        assert!(near(s9.damage, 6.4 + 0.8 * 9.0, 1e-3), "L9 gX 应 6.4+0.8×9，实际 {:?}", s9.damage);
        match def.effect {
            SkillEffect::Warlock098b { proj: W098bProjKind::Boomerang, radius, .. } => {
                assert!(near(radius, 40.0, 1e-3), "radius 应 40（spec），实际 {radius:?}");
            }
            ref e => panic!("S004 effect 应为 Warlock098b(Boomerang)，实际 {e:?}"),
        }
    }

    #[test]
    fn s005_s006_s007_s010_match_spec() {
        // S005 反射盾：CD 25→14（9 级）；dur 2.6+0.2L → L1 2.8。
        let d = DefTable::def(SkillId::S005);
        assert_eq!(d.name, "反射盾");
        assert!(near(d.stats_at(1).cooldown, 25.0, 1e-3));
        assert!(near(d.stats_at(9).cooldown, 14.0, 1e-2), "L9 CD 应 14，实际 {:?}", d.stats_at(9).cooldown);
        assert!(near(d.stats_at(1).duration, 2.8, 1e-3), "L1 盾时长应 2.6+0.2=2.8");
        // S006 时光回溯：CD 22→12（8 级）；delay 3.6 恒定。
        let d = DefTable::def(SkillId::S006);
        assert_eq!(d.name, "时光回溯");
        assert!(near(d.stats_at(1).cooldown, 22.0, 1e-3));
        assert!(near(d.stats_at(8).cooldown, 12.0, 1e-2), "L8 CD 应 12，实际 {:?}", d.stats_at(8).cooldown);
        assert!(near(d.stats_at(1).duration, 3.6, 1e-3) && near(d.stats_at(8).duration, 3.6, 1e-3));
        // S007 急行：CD 21→13（20 级）；dur 6.2+0.8L → L1 7.0；移速乘数 1+35/210。
        let d = DefTable::def(SkillId::S007);
        assert_eq!(d.name, "急行");
        assert!(near(d.stats_at(1).duration, 7.0, 1e-3));
        match d.effect {
            SkillEffect::W098bUtility { kind: W098bUtilKind::Haste, speed, .. } => {
                assert!((speed.to_num::<f64>() - (1.0 + 35.0 / 210.0)).abs() < 1e-3, "+35 移速换算乘数");
            }
            ref e => panic!("S007 effect 错：{e:?}"),
        }
        // S010 疾风步：CD 30→17；dur 3.1；隐身+200 移速（乘数 1+200/210）。
        let d = DefTable::def(SkillId::S010);
        assert_eq!(d.name, "疾风步");
        assert!(near(d.stats_at(1).cooldown, 30.0, 1e-3));
        assert!(near(d.stats_at(20).cooldown, 17.0, 1e-1), "L20 CD 应 ≈17，实际 {:?}", d.stats_at(20).cooldown);
        assert!(near(d.stats_at(1).duration, 3.1, 1e-3));
        match d.effect {
            SkillEffect::W098bUtility { kind: W098bUtilKind::Windwalk, speed, .. } => {
                assert!((speed.to_num::<f64>() - (1.0 + 200.0 / 210.0)).abs() < 1e-3);
            }
            ref e => panic!("S010 effect 锂：{e:?}"),
        }
    }

    #[test]
    fn s011_s012_s013_match_spec() {
        // S011 闪现：CD 16→5.5（9 级）；距离 700+70L → L1 770 / L9 1330。
        let d = DefTable::def(SkillId::S011);
        assert_eq!(d.name, "瞬间移动");
        assert!(near(d.stats_at(1).cooldown, 16.0, 1e-3));
        assert!(near(d.stats_at(9).cooldown, 5.5, 1e-2), "L9 CD 应 5.5，实际 {:?}", d.stats_at(9).cooldown);
        assert!(near(d.stats_at(1).max_distance, 770.0, 1e-3), "L1 距离应 700+70");
        assert!(near(d.stats_at(9).max_distance, 700.0 + 70.0 * 9.0, 1e-3), "L9 距离应 700+70×9");
        // S012 冲撞：速度 1300 恒定；最大距离 (650+50L)×1.1 → L1 770；伤害简化 5+0.4L。
        let d = DefTable::def(SkillId::S012);
        assert_eq!(d.name, "冲撞");
        assert!(near(d.stats_at(1).max_distance, 770.0, 1e-3));
        assert!(near(d.stats_at(1).damage, 5.4, 1e-3));
        match d.effect {
            SkillEffect::W098bUtility { kind: W098bUtilKind::Dash, speed, .. } => {
                assert!(near(speed, 1300.0, 1e-3), "冲刺速度应 Hr=1300/s");
            }
            ref e => panic!("S012 effect 错：{e:?}"),
        }
        // S013 换位：CD 16→4（20 级）；射程 660。
        let d = DefTable::def(SkillId::S013);
        assert_eq!(d.name, "移形换位");
        assert!(near(d.stats_at(1).cooldown, 16.0, 1e-3));
        assert!(near(d.stats_at(20).cooldown, 4.0, 1e-1), "L20 CD 应 ≈4，实际 {:?}", d.stats_at(20).cooldown);
        assert!(near(d.stats_at(1).max_distance, 660.0, 1e-3), "射程应 600×1.1");
    }

    #[test]
    fn s017_s018_s019_match_spec() {
        // S017 致残：CD 25→12.5（20 级）；残废 (4+0.25L) → L1 4.25。
        let d = DefTable::def(SkillId::S017);
        assert_eq!(d.name, "致残");
        assert!(near(d.stats_at(1).cooldown, 25.0, 1e-3));
        assert!(near(d.stats_at(20).cooldown, 12.5, 1e-1), "L20 CD 应 ≈12.5，实际 {:?}", d.stats_at(20).cooldown);
        assert!(near(d.stats_at(1).duration, 4.25, 1e-3), "L1 残废应 4+0.25");
        match d.effect {
            SkillEffect::Warlock098b { speed, radius, on_hit: W098bOnHit::Cripple, .. } => {
                assert!(near(speed, 900.0, 1e-3) && near(radius, 23.0, 1e-3));
            }
            ref e => panic!("S017 effect 错：{e:?}"),
        }
        // S018 引力：CD 26 恒定；漩涡半径 200 / 5s。
        let d = DefTable::def(SkillId::S018);
        assert_eq!(d.name, "引力");
        assert!(near(d.stats_at(1).cooldown, 26.0, 1e-3) && near(d.stats_at(20).cooldown, 26.0, 1e-3));
        let s20 = d.stats_at(20);
        assert!(near(s20.speed, 0.0, 1e-3) && near(s20.radius, 200.0, 1e-3), "原地漩涡（speed=0）半径 200 走 growth");
        assert!(near(s20.duration, 5.0, 1e-3), "漩涡应持续 5*jn 秒");
        // S019 锁链：CD 17→16（20 级）；radius 35；拉拽+0.5s 定身。
        let d = DefTable::def(SkillId::S019);
        assert_eq!(d.name, "锁链");
        assert!(near(d.stats_at(1).cooldown, 17.0, 1e-3));
        assert!(near(d.stats_at(20).cooldown, 16.0, 1e-1), "L20 CD 应 ≈16，实际 {:?}", d.stats_at(20).cooldown);
        match d.effect {
            SkillEffect::Warlock098b { radius, on_hit: W098bOnHit::ChainPull, .. } => {
                assert!(near(radius, 35.0, 1e-3));
            }
            ref e => panic!("S019 effect 错：{e:?}"),
        }
    }

    #[test]
    fn warlock098b_defs_bypass_legacy_scaling() {
        // 098b 数值已是 war3 尺度，直通 DefTable::def、绝不经过 legacy_scale_def 缩放
        //（若误缩放，S000 speed 会变成 1000*65.625 之类的荒谬值）。
        match DefTable::def(SkillId::S000).effect {
            SkillEffect::Warlock098b { speed, .. } => {
                assert!(near(speed, 1000.0, 1e-3), "098b speed 不得被 legacy 缩放，实际 {speed:?}");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn s002_lightning_matches_spec() {
        // detailed hb：伤害 6+1×L（L1=7）；射程 (1+0.15*oi)×600=600；spec CD 16.5→12（9 级）。
        let def = DefTable::def(SkillId::S002);
        assert_eq!(def.name, "闪电");
        let s1 = def.stats_at(1);
        let s9 = def.stats_at(9);
        assert!(near(s1.cooldown, 16.5, 1e-3), "L1 CD 应 16.5，实际 {:?}", s1.cooldown);
        assert!(near(s9.cooldown, 12.0, 1e-2), "L9 CD 应 12.0，实际 {:?}", s9.cooldown);
        assert!(near(s1.damage, 7.0, 1e-3), "L1 伤害应 6+1=7，实际 {:?}", s1.damage);
        assert!(near(s9.damage, 6.0 + 9.0, 1e-3), "L9 伤害应 6+9，实际 {:?}", s9.damage);
        match def.effect {
            SkillEffect::W098bBolt { range, .. } => {
                assert!(near(range, 600.0, 1e-3), "射程应 600，实际 {range:?}");
            }
            ref e => panic!("S002 effect 应为 W098bBolt，实际 {e:?}"),
        }
    }

    #[test]
    fn s008_meteor_matches_spec() {
        // spec：speed 400 / radius 72 / life 2s / aoe 200 / CD 20→16.5（20 级）；
        // detailed XB：KI($A+2*Xv, .8)，$A=10 → gX L1=12。
        let def = DefTable::def(SkillId::S008);
        assert_eq!(def.name, "陨石");
        let s1 = def.stats_at(1);
        let s20 = def.stats_at(20);
        assert!(near(s1.cooldown, 20.0, 1e-3), "L1 CD 应 20，实际 {:?}", s1.cooldown);
        assert!(near(s20.cooldown, 16.5, 1e-1), "L20 CD 应 ≈16.5，实际 {:?}", s20.cooldown);
        assert!(near(s1.damage, 12.0, 1e-3), "L1 gX 应 10+2=12，实际 {:?}", s1.damage);
        assert!(near(s20.damage, 10.0 + 2.0 * 20.0, 1e-3), "L20 gX 应 10+2×20，实际 {:?}", s20.damage);
        match def.effect {
            SkillEffect::Warlock098b { speed, radius, life, blast, kb_ji, .. } => {
                assert!(near(speed, 400.0, 1e-3) && near(radius, 72.0, 1e-3) && near(life, 2.0, 1e-3));
                assert!(near(blast.unwrap(), 200.0, 1e-3), "陨石应带 200 爆炸半径");
                assert!(near(kb_ji, 0.8, 1e-3));
            }
            ref e => panic!("S008 effect 应为 Warlock098b，实际 {e:?}"),
        }
    }

    #[test]
    fn s009_s014_s015_s016_match_spec() {
        // S009 分裂弹：CD 30→20（20 级）；radius 50；KI(3, 1.4) 伤害恒定。
        let d = DefTable::def(SkillId::S009);
        assert_eq!(d.name, "分裂弹");
        assert!(near(d.stats_at(1).cooldown, 30.0, 1e-3));
        assert!(near(d.stats_at(20).cooldown, 20.0, 1e-1), "L20 CD 应 ≈20，实际 {:?}", d.stats_at(20).cooldown);
        assert!(near(d.stats_at(1).damage, 3.0, 1e-3) && near(d.stats_at(20).damage, 3.0, 1e-3), "分裂弹伤害恒 3");
        match d.effect {
            SkillEffect::Warlock098b { radius, kb_ji, .. } => {
                assert!(near(radius, 50.0, 1e-3));
                assert!(near(kb_ji, 1.4, 1e-3));
            }
            ref e => panic!("S009 effect 错：{e:?}"),
        }
        // S014 汲取：CD 22→18.5；speed 700 / radius 27；M1 近似 gX=6+0.5L、合并 JI=0.8。
        let d = DefTable::def(SkillId::S014);
        assert_eq!(d.name, "汲取");
        assert!(near(d.stats_at(1).cooldown, 22.0, 1e-3));
        assert!(near(d.stats_at(20).cooldown, 18.5, 1e-1), "L20 CD 应 ≈18.5，实际 {:?}", d.stats_at(20).cooldown);
        match d.effect {
            SkillEffect::Warlock098b { speed, radius, kb_ji, .. } => {
                assert!(near(speed, 700.0, 1e-3) && near(radius, 27.0, 1e-3) && near(kb_ji, 0.8, 1e-3));
            }
            ref e => panic!("S014 effect 错：{e:?}"),
        }
        // S015 火焰喷射：CD 16→7；锥形 5 道 5.5°；radius 22；jI(2.6+0.4L,.65) → L1 gX=3.0。
        let d = DefTable::def(SkillId::S015);
        assert_eq!(d.name, "火焰喷射");
        assert!(near(d.stats_at(1).cooldown, 16.0, 1e-3));
        assert!(near(d.stats_at(20).cooldown, 7.0, 1e-1), "L20 CD 应 ≈7，实际 {:?}", d.stats_at(20).cooldown);
        assert!(near(d.stats_at(1).damage, 3.0, 1e-3));
        match d.effect {
            SkillEffect::Warlock098b { count, spread_step, radius, .. } => {
                assert_eq!(count, 5, "喷火应锥形 5 道");
                assert!((spread_step - 5.5_f64.to_radians()).abs() < 1e-6, "每道偏转 5.5°");
                assert!(near(radius, 22.0, 1e-3));
            }
            ref e => panic!("S015 effect 错：{e:?}"),
        }
        // S016 弹跳弹：CD 20 恒定；speed 900 / radius 35；gc 形态 gX=5+L → L1=6。
        let d = DefTable::def(SkillId::S016);
        assert_eq!(d.name, "弹跳弹");
        assert!(near(d.stats_at(1).cooldown, 20.0, 1e-3));
        assert!(near(d.stats_at(20).cooldown, 20.0, 1e-3), "弹跳弹 CD 恒定 20");
        assert!(near(d.stats_at(1).damage, 6.0, 1e-3));
        match d.effect {
            SkillEffect::Warlock098b { proj: W098bProjKind::Bounce, speed, radius, .. } => {
                assert!(near(speed, 900.0, 1e-3) && near(radius, 35.0, 1e-3));
            }
            ref e => panic!("S016 effect 应为 Warlock098b(Bounce)，实际 {e:?}"),
        }
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
