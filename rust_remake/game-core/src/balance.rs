//! Balance（数值收敛层）：把散落在 `player` / `world` / `skill` 里的手感/玩法数值统一到一处，
//! 作为权威来源，并由同名常量 / `World` 读取。
//!
//! 目标：调手感不散地改多处魔法字面量，且为「随帧同步版本一起同步、两端一致性校验」打基础
//! （所有端用同一 `Balance` 才能锁步逐位一致）。
//!
//! 用法：
//! - 直接读默认值：`Balance::default().base_speed`
//! - 旧常量名保持兼容：`pub const BASE_SPEED: f64 = Balance::default().base_speed;`
//! - 未来若支持"运行时可调"，`World` 可持有 `Balance` 并随版本校验。
//!
//! **[2026-09-04 尺度切换]** 默认值已从 Unity demo 微缩尺度切到 **war3 单位尺度**
//! （Warlock 0.98b 复刻，见 `PORT_098B_DECISIONS.md` D2）：
//! 距离/半径/速度直接用 war3 单位（`port_spec_098b.json` 的 speed 即单位/秒）；
//! 伤害/HP 尺度不变（098b 英雄 HP=100，与旧值相同）。

/// 玩法/手感数值的权威结构。全 `f64`，`Copy`，可 const 求默认值。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Balance {
    // ---- 玩家基础手感 ----
    /// 基础移动速度（war3 单位/秒）。（098b 英雄移速，port_spec engine.hero_movespeed）
    pub base_speed: f64,
    /// 移动加速度（向目标逼近的速度增量/秒）。占位：保持旧加速时间 210/1312.5≈0.16s 到全速。
    pub accel: f64,
    /// 移动减速度（无目标时刹停的速度削减/秒）。占位：保持旧刹停时间 210/2625≈0.08s。
    pub decel: f64,
    /// 玩家初始/最大生命。（098b h000 术士 HP=100，w3u 导出；与旧值相同 → 伤害数值不随尺度切换变）
    pub max_hp: f64,
    /// 全局生命恢复（HP/秒）。098b `zd()` 每玩家 `Nn=0.05`（每 20s 回 1 血，-C#9 可调）；
    /// 陨石灼烧「烤肉饼」期间清零禁疗（PORT_098B_DECISIONS.md D7）。
    pub hp_regen: f64,
    /// 玩家默认半径（碰撞半径）。h000 未覆盖 Collision → 继承 war3 原版 hpea=32
    /// （2026-09-05 用户体感「地图直径 ≈ 20 个术士并排」交叉验证：碰撞直径 64 × 10 = 半径 640，两项自洽）。
    pub default_radius: f64,

    // ---- 场地 / 世界 ----
    /// 场地初始半径 = 20 个术士并排（碰撞直径 64）× 10 = 640。
    /// 交叉验证：火球弹程 1000 = 1.56 半径（横穿压制技）、闪现 770 = 1.2 半径（非全图）、
    /// 陨石 cast_range 1200 > 640（全图落点）、8 人混战密度合理。2026-09-05 定案（原占位 1200 偏大）。
    pub start_radius: f64,
    /// 缩圈速度（半径减少/秒）。比例口径与原占位一致（1.75%/s × 640）；
    /// 098b 受 war3 地形限制只能整块消失，连续缩圈为本重制版刻意设计（用户确认）。
    pub shrink_speed: f64,
    /// 出界掉血（HP/秒）。（098b 熔岩 Uo×10 = 0.9×10 = 9，mechanics §五）
    pub out_hurt: f64,
    /// 玩家相互挤压损伤（HP/秒）。伤害尺度不变，维持旧值。
    pub overlap_damage: f64,
    /// E3/E3b 扇形子弹伤害。伤害尺度不变，维持旧值。
    pub sabullet_damage: f64,
    /// E3/E3b 扇形子弹射程。按距离因子 ×60 过渡。
    pub sabullet_range: f64,
}

impl Balance {
    /// 默认手感数值（war3 尺度；来源与占位标注见各字段 doc）。
    pub const fn default() -> Self {
        Balance {
            base_speed: 210.0,
            accel: 1312.5,
            decel: 2625.0,
            max_hp: 100.0,
            hp_regen: 0.05,
            default_radius: 32.0,
            start_radius: 640.0,
            shrink_speed: 11.2,
            out_hurt: 9.0,
            overlap_damage: 2.0,
            sabullet_damage: 2.0,
            sabullet_range: 360.0,
        }
    }
}

/// 098b 伤害公式的基值折叠（无蓝量系统的推导，`PORT_098B_DECISIONS.md` D3）。
///
/// 原式（`port_098b/03_JASS/jass_deobf.md` KI/jI）：
/// `kI = ('d' + UnitState(F[hR], MANA)) * gX * ... * .03 * JI`，其中 `'d' = 100`，
/// 蓝量项取的是**受击者**当前蓝。098b 中施法不耗蓝、无回蓝（蓝恒满 10000），
/// 故该项恒为 `(100 + 10000) * 0.03 = 303`，折叠为常量。
///
/// 移植技能时的口径：`kI = DAMAGE_BASE * gX * JI`（其余 Gn/hn/Hn 缩放随技能带入）。
/// 注意：`0.03` 是**几何换算常量**（速率×时间空间化），与逻辑步长 TICK 无关，按字面保留。
pub const DAMAGE_BASE: f64 = (100.0 + 10000.0) * 0.03;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_copy_and_has_expected_fields() {
        let a = Balance::default();
        let b = a; // Copy
        assert_eq!(a, b);
        // war3 尺度（PORT_098B_DECISIONS.md D2 来源表）
        assert_eq!(a.base_speed, 210.0);
        assert_eq!(a.max_hp, 100.0);
        assert_eq!(a.default_radius, 32.0);
        assert_eq!(a.start_radius, 640.0);
        assert_eq!(a.shrink_speed, 11.2);
        assert_eq!(a.out_hurt, 9.0);
    }

    #[test]
    fn damage_base_folds_full_mana_term() {
        // D3：蓝恒满 10000 时 (100+10000)*0.03 的折叠值锁定，防止被误改回含蓝形式。
        assert!((DAMAGE_BASE - 303.0).abs() < 1e-9);
    }
}
