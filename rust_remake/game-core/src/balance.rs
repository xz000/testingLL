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

/// 玩法/手感数值的权威结构。全 `f64`，`Copy`，可 const 求默认值。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Balance {
    // ---- 玩家基础手感 ----
    /// 基础移动速度（单位/秒）。（原 `player::BASE_SPEED`）
    pub base_speed: f64,
    /// 移动加速度（向目标逼近的速度增量/秒）。（原 `player::ACCEL`）
    pub accel: f64,
    /// 移动减速度（无目标时刹停的速度削减/秒）。（原 `player::DECEL`）
    pub decel: f64,
    /// 玩家初始/最大生命。（原 `player::MAX_HP`）
    pub max_hp: f64,
    /// 玩家默认半径。（原 `player::DEFAULT_RADIUS`）
    pub default_radius: f64,

    // ---- 场地 / 世界 ----
    /// 场地初始半径。（原 `world::START_RADIUS`）
    pub start_radius: f64,
    /// 缩圈速度（半径减少/秒）。（原 `world::SHRINK_SPEED`）
    pub shrink_speed: f64,
    /// 出界掉血（HP/秒）。（原 `world::OUT_HURT`）
    pub out_hurt: f64,
    /// 玩家相互挤压损伤（HP/秒）。（原 `world::OVERLAP_DAMAGE`）
    pub overlap_damage: f64,
    /// E3/E3b 扇形子弹伤害。（原 `world::SABULLET_DAMAGE`）
    pub sabullet_damage: f64,
    /// E3/E3b 扇形子弹射程。（原 `world::SABULLET_RANGE`）
    pub sabullet_range: f64,
}

impl Balance {
    /// 默认手感数值（保持与历史实现完全一致，纯重构不改玩法）。
    pub const fn default() -> Self {
        Balance {
            base_speed: 3.2,
            accel: 20.0,
            decel: 40.0,
            max_hp: 100.0,
            default_radius: 1.0,
            start_radius: 20.0,
            shrink_speed: 0.35,
            out_hurt: 5.0,
            overlap_damage: 2.0,
            sabullet_damage: 2.0,
            sabullet_range: 6.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_copy_and_has_expected_fields() {
        let a = Balance::default();
        let b = a; // Copy
        assert_eq!(a, b);
        assert_eq!(a.base_speed, 3.2);
        assert_eq!(a.accel, 20.0);
        assert_eq!(a.decel, 40.0);
        assert_eq!(a.max_hp, 100.0);
        assert_eq!(a.start_radius, 20.0);
        assert_eq!(a.shrink_speed, 0.35);
    }
}
