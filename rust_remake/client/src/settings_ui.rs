//! 房间设置编辑器：**行模型**（分组 / 标签 / 取值文本 / 调整）。
//!
//! 设计要点：
//! - **档位 + 自定义并存**：方向键在档位间跳（含 `自定义` 档时继续按则微调），
//!   `Shift+方向` 恒为微调；这样既满足"快速选常用值"，也保留"想怎么调就怎么调"。
//! - 逻辑（本模块）与渲染分离，因此可在**单测**里验证档位、钳制、格式化，
//!   不必靠肉眼看界面（此前多次出现"界面看着对但值不对"的问题）。
//! - 默认值一律等于 098c 全局值（见 `ROOM_SETTINGS_PLAN.md`），`is_custom` 用于高亮。

use game_core::meta::MatchConfig;

/// 设置分组（房间 UI 的四个页签）。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Group {
    Economy,
    Gameplay,
    Map,
    Mode,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Economy, Group::Gameplay, Group::Map, Group::Mode];

    pub fn name(self) -> &'static str {
        match self {
            Group::Economy => "经济",
            Group::Gameplay => "玩法",
            Group::Map => "地图",
            Group::Mode => "模式",
        }
    }

    /// 页签快捷键字母（`J/K/L` 已用于技能/商店/成长页，这里用 `Z/X/C/V`）。
    pub fn hotkey(self) -> char {
        match self {
            Group::Economy => 'z',
            Group::Gameplay => 'x',
            Group::Map => 'c',
            Group::Mode => 'v',
        }
    }
}

/// 一个可编辑设置项。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SettingId {
    // 经济
    StartingGold,
    GoldPerRound,
    GoldPerKill,
    GoldPerAssist,
    GoldPerRoundWin,
    GoldPerMostDamage,
    ScorePerKill,
    ScorePerAssist,
    ScorePerRoundWin,
    // 玩法
    DamageMult,
    KnockbackMult,
    LavaDamageMult,
    FirstRoundSecs,
    BetweenRoundsSecs,
    ShrinkDelaySecs,
    ShrinkRingSecs,
    BaseRegen,
    // 地图
    ArenaShape,
    PillarMode,
    IceMode,
    // 模式
    GoldRewardsEnabled,
}

impl SettingId {
    /// 某分组的行顺序。
    pub fn rows(g: Group) -> &'static [SettingId] {
        use SettingId::*;
        match g {
            Group::Economy => &[
                StartingGold,
                GoldPerRound,
                GoldPerKill,
                GoldPerAssist,
                GoldPerRoundWin,
                GoldPerMostDamage,
                ScorePerKill,
                ScorePerAssist,
                ScorePerRoundWin,
            ],
            Group::Gameplay => &[
                DamageMult,
                KnockbackMult,
                LavaDamageMult,
                BaseRegen,
                FirstRoundSecs,
                BetweenRoundsSecs,
                ShrinkDelaySecs,
                ShrinkRingSecs,
            ],
            Group::Map => &[ArenaShape, PillarMode, IceMode],
            Group::Mode => &[GoldRewardsEnabled],
        }
    }

    pub fn label(self) -> &'static str {
        use SettingId::*;
        match self {
            StartingGold => "初始金币",
            GoldPerRound => "每轮金币",
            GoldPerKill => "击杀金币",
            GoldPerAssist => "助攻金币",
            GoldPerRoundWin => "胜利金币",
            GoldPerMostDamage => "伤害金（回合最高伤害）",
            ScorePerKill => "击杀得点",
            ScorePerAssist => "助攻得点",
            ScorePerRoundWin => "胜利得点",
            DamageMult => "伤害倍率",
            KnockbackMult => "击退倍率",
            LavaDamageMult => "岩浆伤害",
            FirstRoundSecs => "首轮配置期(秒)",
            BetweenRoundsSecs => "局间配置期(秒)",
            ShrinkDelaySecs => "收缩延迟(秒)",
            ShrinkRingSecs => "收缩每环(秒)",
            BaseRegen => "基础回血(HP/s)",
            ArenaShape => "地图形状",
            PillarMode => "柱子",
            IceMode => "冰面",
            GoldRewardsEnabled => "金币奖励总开关",
        }
    }

    /// 一句话说明（详情区显示；写清 098c 出处与语义）。
    pub fn hint(self) -> &'static str {
        use SettingId::*;
        match self {
            StartingGold => "098c `Qo`=20。开局一次性发放。",
            GoldPerRound => "098c 设置 17 `qo`=10。每轮结算时发放。",
            GoldPerKill => "098c 设置 12 `lo`=1。",
            GoldPerAssist => "098c 设置 13 `Lo`=1。",
            GoldPerRoundWin => "098c 设置 15 `Mo`=2。",
            GoldPerMostDamage => "098c 设置 16 `po`=1。发给**本回合伤害最高**者（并列都发）。",
            ScorePerKill => "098c 设置 10 `ko`=1。",
            ScorePerAssist => "098c 设置 11 `Ko`=1。",
            ScorePerRoundWin => "098c 设置 14 `mo`=2。",
            DamageMult => "全局伤害倍率；档位 75/100/125/150%，可自定义。",
            KnockbackMult => "全局击退倍率；档位同上。",
            LavaDamageMult => "岩浆（出界）伤害倍率。**0 = 关闭岩浆**（098c 允许，不推荐）。",
            FirstRoundSecs => "098c 设置 5 `Uo`=40：第一轮的配置期时长。",
            BetweenRoundsSecs => "098c 设置 4 `uo`=30：局间配置期时长。",
            ShrinkDelaySecs => "开局静止期；实际延迟 = 本值 × √存活人数（098c `wo*√sn`）。",
            ShrinkRingSecs => "每越一环所需秒数；速率随存活人数 √ 缩放（我方连续收缩模型）。",
            BaseRegen => "098c 设置 9 `In`=.05/0.1s = 0.5。档位 0.5/0/0.25/0.75/1.0/2.0。",
            ArenaShape => "当前仅圆形；后续可扩正方形/六边形。",
            PillarMode => "关闭 / 随机 / 每局必有。",
            IceMode => "关闭 / 随机 / 每局必有。",
            GoldRewardsEnabled => "关闭后所有金币奖励归零（等价 098c `-no reward`）；点数不受影响。",
        }
    }

    /// 枚举型档位（非枚举返回 `None`）。
    pub fn enum_tiers(self) -> Option<&'static [&'static str]> {
        match self {
            SettingId::PillarMode | SettingId::IceMode => Some(&["关闭", "随机", "每局必有"]),
            SettingId::ArenaShape => Some(&["圆形"]),
            _ => None,
        }
    }

    /// 数值型档位（前端 `nudge` 的快速跳档；未列出的值即"自定义"）。
    pub fn num_tiers(self) -> Option<&'static [f64]> {
        use SettingId::*;
        match self {
            DamageMult | KnockbackMult => Some(&[0.75, 1.0, 1.25, 1.5]),
            LavaDamageMult => Some(&[0.0, 0.5, 1.0, 1.5]),
            BaseRegen => Some(&[0.5, 0.0, 0.25, 0.75, 1.0, 2.0]),
            _ => None,
        }
    }

    /// 数值范围与步长 `(min, max, step)`；非数值返回 `None`。
    pub fn num_range(self) -> Option<(f64, f64, f64)> {
        use SettingId::*;
        match self {
            DamageMult | KnockbackMult => Some((0.25, 3.0, 0.05)),
            LavaDamageMult => Some((0.0, 3.0, 0.05)),
            BaseRegen => Some((0.0, 5.0, 0.05)),
            FirstRoundSecs | BetweenRoundsSecs => Some((5.0, 180.0, 5.0)),
            ShrinkDelaySecs => Some((0.0, 120.0, 1.0)),
            ShrinkRingSecs => Some((1.0, 60.0, 1.0)),
            _ => None,
        }
    }

    /// 整数范围与步长 `(min, max, step)`。
    pub fn int_range(self) -> Option<(i32, i32, i32)> {
        use SettingId::*;
        match self {
            StartingGold => Some((0, 500, 5)),
            GoldPerRound | GoldPerKill | GoldPerAssist | GoldPerRoundWin | GoldPerMostDamage => {
                Some((0, 100, 1))
            }
            ScorePerKill | ScorePerAssist | ScorePerRoundWin => Some((0, 20, 1)),
            _ => None,
        }
    }
}

/// 读取当前值（用于显示与默认值比较）。
pub fn value(cfg: &MatchConfig, id: SettingId) -> f64 {
    use SettingId::*;
    match id {
        StartingGold => cfg.starting_gold as f64,
        GoldPerRound => cfg.gold_per_round as f64,
        GoldPerKill => cfg.gold_per_kill as f64,
        GoldPerAssist => cfg.gold_per_assist as f64,
        GoldPerRoundWin => cfg.gold_per_round_win as f64,
        GoldPerMostDamage => cfg.gold_per_most_damage as f64,
        ScorePerKill => cfg.score_per_kill as f64,
        ScorePerAssist => cfg.score_per_assist as f64,
        ScorePerRoundWin => cfg.score_per_round_win as f64,
        DamageMult => cfg.damage_mult,
        KnockbackMult => cfg.knockback_mult,
        LavaDamageMult => cfg.lava_damage_mult,
        FirstRoundSecs => cfg.first_round_time_secs,
        BetweenRoundsSecs => cfg.between_rounds_time_secs,
        ShrinkDelaySecs => cfg.shrink_delay_secs,
        ShrinkRingSecs => cfg.shrink_ring_secs,
        BaseRegen => cfg.base_regen,
        ArenaShape => cfg.arena_shape as f64,
        PillarMode => cfg.pillar_mode as f64,
        IceMode => cfg.ice_mode as f64,
        GoldRewardsEnabled => cfg.gold_rewards_enabled as i32 as f64,
    }
}

/// 写入值（已按类型取整/钳制由调用方 `nudge` 保证）。
fn set(cfg: &mut MatchConfig, id: SettingId, v: f64) {
    use SettingId::*;
    let iv = v.round() as i32;
    let bv = v.round().clamp(0.0, 255.0) as u8;
    match id {
        StartingGold => cfg.starting_gold = iv,
        GoldPerRound => cfg.gold_per_round = iv,
        GoldPerKill => cfg.gold_per_kill = iv,
        GoldPerAssist => cfg.gold_per_assist = iv,
        GoldPerRoundWin => cfg.gold_per_round_win = iv,
        GoldPerMostDamage => cfg.gold_per_most_damage = iv,
        ScorePerKill => cfg.score_per_kill = iv.max(0) as u32,
        ScorePerAssist => cfg.score_per_assist = iv.max(0) as u32,
        ScorePerRoundWin => cfg.score_per_round_win = iv.max(0) as u32,
        DamageMult => cfg.damage_mult = v,
        KnockbackMult => cfg.knockback_mult = v,
        LavaDamageMult => cfg.lava_damage_mult = v,
        FirstRoundSecs => cfg.first_round_time_secs = v,
        BetweenRoundsSecs => cfg.between_rounds_time_secs = v,
        ShrinkDelaySecs => cfg.shrink_delay_secs = v,
        ShrinkRingSecs => cfg.shrink_ring_secs = v,
        BaseRegen => cfg.base_regen = v,
        ArenaShape => cfg.arena_shape = bv,
        PillarMode => cfg.pillar_mode = bv.min(2),
        IceMode => cfg.ice_mode = bv.min(2),
        GoldRewardsEnabled => cfg.gold_rewards_enabled = v >= 0.5,
    }
}

/// 该设置项是否**偏离默认值**（徽章计数与高亮用）。
pub fn is_custom(cfg: &MatchConfig, id: SettingId) -> bool {
    let d = MatchConfig::default();
    if id == SettingId::GoldRewardsEnabled {
        return cfg.gold_rewards_enabled != d.gold_rewards_enabled;
    }
    (value(cfg, id) - value(&d, id)).abs() > 1e-9
}

/// 方向键调整：`dir = +1/-1`。
///
/// - **枚举**：在档位间环绕。
/// - **数值/整数**：若当前值恰在某个档位上，则跳到**相邻档位**（档位表见 `num_tiers`）；
///   否则按步长微调；均在 `min..max` 内钳制。
/// - **开关**：翻转。
pub fn nudge(cfg: &mut MatchConfig, id: SettingId, dir: i32) {
    if id == SettingId::GoldRewardsEnabled {
        cfg.gold_rewards_enabled = !cfg.gold_rewards_enabled;
        return;
    }
    if let Some(tiers) = id.enum_tiers() {
        let cur = value(cfg, id).round() as i32;
        let n = tiers.len() as i32;
        let next = (cur + dir).rem_euclid(n.max(1));
        set(cfg, id, next as f64);
        return;
    }
    if let Some((min, max, step)) = id.num_range() {
        let cur = value(cfg, id);
        // 档位跳档：当前值命中某档 → 去相邻档；否则步进。
        let target = id.num_tiers().and_then(|t| {
            let idx = t.iter().position(|v| (v - cur).abs() < 1e-6);
            idx.map(|i| {
                let ni = (i as i32 + dir).clamp(0, t.len() as i32 - 1) as usize;
                t[ni]
            })
        });
        let nv = target.unwrap_or(cur + step * dir as f64);
        set(cfg, id, nv.clamp(min, max));
        return;
    }
    if let Some((min, max, step)) = id.int_range() {
        let cur = value(cfg, id).round() as i32;
        let nv = (cur + step * dir).clamp(min, max);
        set(cfg, id, nv as f64);
    }
}

/// 显示文本（含单位与"自定义"标记）。
pub fn value_text(cfg: &MatchConfig, id: SettingId) -> String {
    let v = value(cfg, id);
    if let Some(tiers) = id.enum_tiers() {
        let i = (v.round() as usize).min(tiers.len().saturating_sub(1));
        return tiers.get(i).copied().unwrap_or("?").to_string();
    }
    if id == SettingId::GoldRewardsEnabled {
        return if cfg.gold_rewards_enabled { "开".into() } else { "关".into() };
    }
    if id.num_range().is_some() {
        let is_tier = id
            .num_tiers()
            .map(|t| t.iter().any(|x| (x - v).abs() < 1e-6))
            .unwrap_or(true);
        let tag = if is_tier { "" } else { "（自定义）" };
        // 倍率显示为百分比更直观
        return match id {
            SettingId::DamageMult | SettingId::KnockbackMult | SettingId::LavaDamageMult => {
                if id == SettingId::LavaDamageMult && v == 0.0 {
                    format!("关闭{tag}")
                } else {
                    format!("{:.0}%{tag}", v * 100.0)
                }
            }
            SettingId::ShrinkRingSecs | SettingId::ShrinkDelaySecs | SettingId::FirstRoundSecs
            | SettingId::BetweenRoundsSecs => format!("{v:.0}s{tag}"),
            _ => format!("{v:.2}{tag}"),
        };
    }
    format!("{}", v.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> MatchConfig {
        MatchConfig::default()
    }

    #[test]
    fn defaults_are_not_custom_and_badge_zero() {
        let c = fresh();
        for g in Group::ALL {
            for &id in SettingId::rows(g) {
                assert!(!is_custom(&c, id), "{:?} 默认不应标为自定义", id);
            }
        }
        assert_eq!(c.non_default_setting_count(), 0);
    }

    #[test]
    fn numeric_tiers_jump_and_custom_is_marked() {
        let mut c = fresh();
        // 伤害倍率默认 1.0（档位中点）→ +1 → 1.25
        nudge(&mut c, SettingId::DamageMult, 1);
        assert!((c.damage_mult - 1.25).abs() < 1e-9, "应跳到相邻档 1.25");
        assert!(is_custom(&c, SettingId::DamageMult));
        // 从最高档再 +1 → 钳制在 1.5（档位顶）
        nudge(&mut c, SettingId::DamageMult, 1);
        assert!((c.damage_mult - 1.5).abs() < 1e-9, "应到档位顶 1.5");
        nudge(&mut c, SettingId::DamageMult, 1);
        assert!(c.damage_mult <= 3.0, "钳制在 max");
        // 非档位值（自定义）→ 按步长微调
        c.damage_mult = 1.13;
        nudge(&mut c, SettingId::DamageMult, 1);
        assert!(
            (c.damage_mult - 1.18).abs() < 1e-9,
            "非档位值应步进 0.05，实际 {}",
            c.damage_mult
        );
        assert!(value_text(&c, SettingId::DamageMult).contains("自定义"));
    }

    #[test]
    fn lava_can_be_disabled_and_shown_as_off() {
        let mut c = fresh();
        assert_eq!(value_text(&c, SettingId::LavaDamageMult), "100%");
        // 1.0 → 0.5 → 0.0
        nudge(&mut c, SettingId::LavaDamageMult, -1);
        nudge(&mut c, SettingId::LavaDamageMult, -1);
        assert_eq!(c.lava_damage_mult, 0.0, "岩浆可关（098c 同）");
        assert_eq!(value_text(&c, SettingId::LavaDamageMult), "关闭（自定义）".replace("（自定义）", ""));
        // 再 -1 仍在 0（钳制）
        nudge(&mut c, SettingId::LavaDamageMult, -1);
        assert_eq!(c.lava_damage_mult, 0.0);
    }

    #[test]
    fn enum_rows_cycle_and_wrap() {
        let mut c = fresh();
        assert_eq!(value_text(&c, SettingId::PillarMode), "随机"); // 默认 1
        nudge(&mut c, SettingId::PillarMode, 1);
        assert_eq!(c.pillar_mode, 2, "随机 → 每局必有");
        nudge(&mut c, SettingId::PillarMode, 1);
        assert_eq!(c.pillar_mode, 0, "环绕回关闭");
        nudge(&mut c, SettingId::IceMode, -1);
        assert_eq!(c.ice_mode, 0, "冰面可关");
    }

    #[test]
    fn toggle_and_int_clamp() {
        let mut c = fresh();
        nudge(&mut c, SettingId::GoldRewardsEnabled, 1);
        assert!(!c.gold_rewards_enabled, "开关应翻转");
        assert!(is_custom(&c, SettingId::GoldRewardsEnabled));
        // 整数钳制：初始金币默认 20，-5 → 15；连续 -1 不越 0
        nudge(&mut c, SettingId::StartingGold, -1);
        assert_eq!(c.starting_gold, 15);
        for _ in 0..10 {
            nudge(&mut c, SettingId::StartingGold, -1);
        }
        assert_eq!(c.starting_gold, 0, "不应低于 0");
    }

    #[test]
    fn badge_counts_each_group() {
        let mut c = fresh();
        nudge(&mut c, SettingId::GoldPerKill, 1);
        nudge(&mut c, SettingId::PillarMode, 1);
        nudge(&mut c, SettingId::GoldRewardsEnabled, 1);
        assert_eq!(c.non_default_setting_count(), 3, "三项各记 1");
    }
}
