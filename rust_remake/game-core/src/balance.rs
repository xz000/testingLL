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
    /// 移动减速度（无目标时刹停的速度削减/秒）。0.08s 刹停（灵敏受控）。
    /// 098c 的 ov=0.96 阻尼是冰面 -ice/TimeShift 特定状态参数，非普通地面（D9 批次2 回退）。
    pub decel: f64,
    /// 玩家初始/最大生命。（098b h000 术士 HP=100，w3u 导出；与旧值相同 → 伤害数值不随尺度切换变）
    pub max_hp: f64,
    /// 全局生命恢复（HP/秒）。098c `uhpr=0`——回复全靠物品/技能（D9 批次3 改 0；
    /// 098b 曾有 Nn=0.05，两版本口径不同，基准转 098c）。
    pub hp_regen: f64,
    /// 玩家默认半径（碰撞半径）。098c `do=30`（术士碰撞半径，JASS 常量原文；D9 批次2）。
    /// 场地半径 640 保持（用户体感「20 术士并排」×直径 60 ÷ 2 = 600，与 640 同量级自洽）。
    pub default_radius: f64,

    // ---- 场地 / 世界 ----
    /// 场地初始半径 = 20 个术士并排（碰撞直径 64）× 10 = 640。
    /// 交叉验证：火球弹程 1000 = 1.56 半径（横穿压制技）、闪现 770 = 1.2 半径（非全图）、
    /// 陨石 cast_range 1200 > 640（全图落点）、8 人混战密度合理。2026-09-05 定案（原占位 1200 偏大）。
    pub start_radius: f64,
    /// 每环宽度（098c 地形格 128 码；缩圈按环步进）。
    pub ring_width: f64,
    /// 每环缩圈时长基数（098c wo=10s，-C 6；实际间隔 = wo×√存活数）。
    pub shrink_ring_secs: f64,
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
    /// 098c 场地初始半径：`(9 + 人数/2) 环 × 128 码`（ta=9+qn/2，EA 每 wo×√存活 烧一环）。
    /// 2 人=1280、4 人=1408、10 人=1792；全场吞没 ≈ 环数×10×√人数 秒（2 人约 141 秒）。
    pub fn start_radius_for(player_count: u32) -> f64 {
        let b = Self::default();
        // 098c ta 为整环数：9 + qn/2（整除）——1 人 9 环、2 人 10 环、4 人 11 环。
        (9.0 + (player_count / 2) as f64) * b.ring_width
    }

    pub const fn default() -> Self {
        Balance {
            base_speed: 210.0,
            accel: 1312.5,
            decel: 2625.0,
            max_hp: 100.0,
            hp_regen: 0.0,
            default_radius: 30.0,
            // 098c 场地半径按人数：start_radius_for(n) = (9+n/2 环)×128 码；此值为缺省（1 人 9 环）。
            start_radius: 1152.0,
            ring_width: 128.0,
            shrink_ring_secs: 10.0,
            shrink_speed: 11.2,
            out_hurt: 9.0,
            overlap_damage: 2.0,
            sabullet_damage: 2.0,
            sabullet_range: 360.0,
        }
    }
}

// DAMAGE_BASE（098b 蓝恒满折叠=303）已随 D9 基准转向 098c 退役：
// 击退初速 = (100 + 目标当前魔法) × gX × JI（动态，见 world::warlock_ki_knockback）。

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
        assert_eq!(a.default_radius, 30.0);
        assert_eq!(a.start_radius, 1152.0, "缺省（1 人）应为 9 环×128");
        assert_eq!((a.ring_width, a.shrink_ring_secs), (128.0, 10.0), "098c EA：128 码/环、wo=10s");
        assert_eq!(a.shrink_speed, 11.2);
        assert_eq!(a.out_hurt, 9.0);
    }

    // DAMAGE_BASE 折叠测试已随 D9（098c 动态击退）删除——击退基数现为 (100+目标魔法) 动态值。
}
