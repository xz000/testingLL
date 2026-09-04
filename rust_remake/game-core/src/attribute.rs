//! 玩家属性快照（4.6b）与派生规则。
//!
//! 属性是 Dota/War3 式可成长战斗数值，作为 **PlayerProfile / PlayerConfig 的组成部分**在跨端确定性同步，
//! 再由 **game-core 一处的纯函数派生**到 `Player` 的战斗数值（最大生命、移速等）。
//!
//! 设计要点（ATTRIBUTE_SYSTEM.md）：
//! - 属性以「整数点数」表达，派生用「加法系数」（本阶段先接 Hp/移速；护甲/法抗/击退留阶段 2 结算点接入）。
//! - 派生必须确定性（纯函数仅由属性算出），只在 game-core 一处合成。
//! - 快照字段进 `PlayerConfig` 时 bump `CONFIG_VERSION`，网络层收发零改动。

/// 一个玩家的战斗属性快照（全部为整数点数；0 = 无加成）。跨端同步、端到端一致。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attributes {
    /// 最大生命加成（点数）。每点 +`HP_PER_BONUS` 比例。
    pub hp_bonus: u32,
    /// 移速加成（点数）。每点 +`SPEED_PER_BONUS` 比例。
    pub speed_bonus: u32,
    /// 护甲（点数）：减伤。阶段 2 在 `events` 伤害结算前按此折算。
    pub armor: u32,
    /// 法术抗性（点数）：减技能/子弹伤害。阶段 2 接入。
    pub spell_resist: u32,
    /// 击退抗性（点数）：缩放 `push_power`/`push_time`。阶段 2 接入。
    pub kb_resist: u32,
    // 蓝量属性（mana_max / mana_regen）已移除（PORT_098B_DECISIONS.md D3，无蓝量系统）：
    // 098b 施法不耗蓝、无回蓝，伤害公式的蓝量项折叠为 balance::DAMAGE_BASE。
}

/// 可购买的属性项（成长点购买索引）。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GrowthAttr {
    Hp,
    Speed,
    Armor,
    SpellResist,
    KbResist,
}

impl Attributes {
    /// 给指定属性 +1 点（购买）。
    pub fn add_point(&mut self, which: GrowthAttr) {
        match which {
            GrowthAttr::Hp => self.hp_bonus += 1,
            GrowthAttr::Speed => self.speed_bonus += 1,
            GrowthAttr::Armor => self.armor += 1,
            GrowthAttr::SpellResist => self.spell_resist += 1,
            GrowthAttr::KbResist => self.kb_resist += 1,
        }
    }

    /// 某属性的当前点数（用于价格表）。
    pub fn current(&self, which: GrowthAttr) -> u32 {
        match which {
            GrowthAttr::Hp => self.hp_bonus,
            GrowthAttr::Speed => self.speed_bonus,
            GrowthAttr::Armor => self.armor,
            GrowthAttr::SpellResist => self.spell_resist,
            GrowthAttr::KbResist => self.kb_resist,
        }
    }
}


// ---------- 派生系数（Balance 收敛层） ----------

/// 每点 hp_bonus 加成最大生命的比例（10%）。
pub const HP_PER_BONUS: f64 = 0.10;
/// 每点 speed_bonus 加成移速的比例（5%）。
pub const SPEED_PER_BONUS: f64 = 0.05;
/// 每点 armor 减伤比例（6%）。
pub const ARMOR_REDUCTION_PER_POINT: f64 = 0.06;
/// 每点 spell_resist 减伤比例（6%）。
pub const SPELL_RESIST_PER_POINT: f64 = 0.06;
/// 每点 kb_resist 削减击退的比例（12%）。
pub const KB_RESIST_PER_POINT: f64 = 0.12;

impl Attributes {
    /// 由属性派生出最大生命值（在 `Balance::default().max_hp` 基础上按加法系数）。
    pub fn derived_max_hp(&self, base_max_hp: f64) -> f64 {
        base_max_hp * (1.0 + self.hp_bonus as f64 * HP_PER_BONUS)
    }

    /// 由属性派生出移速倍率（1.0 = 无加成）。
    pub fn derived_speed_mult(&self) -> f64 {
        1.0 + self.speed_bonus as f64 * SPEED_PER_BONUS
    }

    /// 护甲折算伤害倍率（0..1 内的系数；`min_factor` 下限防负）。阶段 2 用。
    pub fn armor_factor(&self) -> f64 {
        (1.0 - self.armor as f64 * ARMOR_REDUCTION_PER_POINT).max(0.2)
    }

    /// 法抗折算伤害倍率。阶段 2 用。
    pub fn spell_factor(&self) -> f64 {
        (1.0 - self.spell_resist as f64 * SPELL_RESIST_PER_POINT).max(0.2)
    }

    /// 击退抗性：剩余击退比例（push 力/时长乘此）。
    pub fn kb_factor(&self) -> f64 {
        (1.0 - self.kb_resist as f64 * KB_RESIST_PER_POINT).max(0.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_max_hp_is_deterministic_and_proportional() {
        let a = Attributes { hp_bonus: 5, ..Default::default() };
        let base = 100.0;
        // 5 点 × 10% = +50% → 150
        let h = a.derived_max_hp(base);
        assert!((h - 150.0).abs() < 1e-9, "5 点 hp_bonus 应 +50% 生命，got {h}");
        // 无加成 → base
        let z = Attributes::default();
        assert!((z.derived_max_hp(100.0) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn derived_speed_mult_is_deterministic() {
        let a = Attributes { speed_bonus: 4, ..Default::default() };
        // 4 × 5% = +20%
        assert!((a.derived_speed_mult() - 1.2).abs() < 1e-9);
        assert!((Attributes::default().derived_speed_mult() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn reductions_are_clamped_not_negative() {
        let big = Attributes { armor: 999, spell_resist: 999, kb_resist: 999, ..Default::default() };
        assert!(big.armor_factor() >= 0.2);
        assert!(big.spell_factor() >= 0.2);
        assert!(big.kb_factor() >= 0.1);
    }
}
