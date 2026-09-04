//! 098b 物品系统数据层（PORT_098B_DECISIONS.md M3）。
//!
//! 真值来源：`port_098b/02_物体/war3map.w3t_098b_org.md`（全字段）+ `items_098b.md`（目录）。
//! 24 个自定义物品，多数为**多级升级链**（速度之靴 1-3 / 斗篷 1-3 / 头盔 1-3 / 坠饰 1-3 /
//! 熔岩靴 1-3 / 怀表 1-2 / 鲜血之剑 1-2 / 守护之盾 1-2），以及单体改件
//! （死亡面具 / 火球法杖 / 乔丹之石戒指）。
//!
//! 数值口径（war3 加法值按基础移速 210 / 基础回复 0.05 换算）：
//! - 移速 ±X（war3 平加）→ `speed_add`，生效为 `BASE + Σadd`；
//! - 受击退 -X% → `kb_resist_frac`，与属性击退抗性取**最大**（098b「头盔效果不叠加」）；
//! - 生命 +X → `hp_add`；回复 +X/s → `regen_add`（叠加到 `Balance.hp_regen` 基础上）；
//! - 怀表：自身增益时长 ×M、受到减益时长 ÷M（`buff_dur_mult`/`debuff_dur_mult`）。
//!
//! 武器改件（面具吸血 / 火球法杖点燃改写 / 鲜血之剑·守护之盾的天罚强化）字段已建模，
//! 战斗钩子 TODO（M3 2c）。
//!
//! 定价缺口：速度之靴/斗篷/熔岩靴各档 `GoldCost=0`——098b 商店实际定价在 JASS 内
//! （未导出），此处按「卖出价 ×2」启发式占位（I00D 等显式 GoldCost 照抄），TODO(shop)。

use crate::balance::Balance;

/// 物品标识（098b 自定义物体 I000–I00N，共 24 个；密集索引供存档/网络编码）。
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ItemId {
    /// I000
    Boots1,
    /// I002
    Amulet2,
    /// I004 死亡面具
    FireMask,
    /// I005
    Cloak1,
    /// I001
    Helm1,
    /// I006
    Helm2,
    /// I007
    Boots2,
    /// I008
    Boots3,
    /// I003
    Cloak3,
    /// I009
    Cloak2,
    /// I00A
    Helm3,
    /// I00B
    Amulet1,
    /// I00C
    Amulet3,
    /// I00D 火球法杖
    FireStaff,
    /// I00E 乔丹之石戒指
    Jordan,
    /// I00F
    BloodSword1,
    /// I00G
    BloodSword2,
    /// I00H
    GuardianShield1,
    /// I00I
    GuardianShield2,
    /// I00J
    LavaBoots1,
    /// I00K
    LavaBoots2,
    /// I00L
    LavaBoots3,
    /// I00M
    PocketWatch1,
    /// I00N
    PocketWatch2,
}

/// 物品升级链家族（同家族内按 tier 升级，买高档替换低档）。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ItemFamily {
    Boots,
    Cloak,
    Helm,
    Amulet,
    LavaBoots,
    PocketWatch,
    BloodSword,
    GuardianShield,
    /// 无链单体改件。
    Standalone,
}

/// 物品聚合效果（同类型数值直接求和；kb 取最大；怀表乘子取最大）。
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct ItemEffects {
    /// 移速平加（war3 单位；速度之靴 +20/30/40、熔岩靴 +15/30/39）。
    pub speed_add: f64,
    /// 移速惩罚（平减；头盔/斗篷的 -5 等，存正数）。
    pub speed_penalty: f64,
    /// 最大生命平加（头盔 +10/15/20、坠饰 +10/20/30）。
    pub hp_add: f64,
    /// 生命回复加成（HP/s；斗篷 +0.3/0.4/0.5、坠饰 3 档 +0.1）。
    pub regen_add: f64,
    /// 生命回复惩罚（死亡面具 -0.3、熔岩靴 -0.1；存正数）。
    pub regen_penalty: f64,
    /// 受击退减免比例（头盔 0.16/0.24/0.32；与属性抗性取 max，不叠加）。
    pub kb_resist_frac: f64,
    /// 生命偷取比例（死亡面具 0.24；战斗钩子 TODO 2c）。
    pub lifesteal: f64,
    /// 受伤点恢复（死亡面具 0.12；战斗钩子 TODO 2c）。
    pub on_damage_heal: f64,
    /// 天罚（S001）伤害加成（鲜血之剑 12/13；TODO 2c）。
    pub smite_bonus: f64,
    /// 天罚受伤减免（守护之盾 0.25/0.75；TODO 2c）。
    pub smite_reduction: f64,
    /// 火球点燃改写（火球法杖；TODO 2c）。
    pub fireball_burn: bool,
    /// 技能可升超过上限的级数（乔丹之石 +2；接入 upgrade 上限 TODO 2c）。
    pub jordan_levels: u8,
    /// 自身增益时长乘数（怀表 1.15/1.25）。
    pub buff_dur_mult: f64,
    /// 受到减益时长除数（怀表 1.15/1.25）。
    pub debuff_dur_div: f64,
    /// 熔岩抵抗激活秒数（熔岩靴 3/4/5；熔岩地形 TODO M5）。
    pub lava_resist_secs: f64,
}

/// 物品定义。
#[derive(Copy, Clone, Debug)]
pub struct ItemDef {
    pub id: ItemId,
    pub family: ItemFamily,
    /// 同家族内的档位（1 起；Standalone 恒 1）。
    pub tier: u8,
    /// 购买价（GoldCost 原值；0 = 定价未解码，见模块注释 TODO(shop)）。
    pub cost: i32,
    /// 卖出价（tooltip 参考值；098b Sellable=0 实际不可卖，仅供占位展示）。
    pub sell: i32,
    pub name: &'static str,
    pub fx: ItemEffects,
}

const fn fx() -> ItemEffects {
    ItemEffects {
        speed_add: 0.0,
        speed_penalty: 0.0,
        hp_add: 0.0,
        regen_add: 0.0,
        regen_penalty: 0.0,
        kb_resist_frac: 0.0,
        lifesteal: 0.0,
        on_damage_heal: 0.0,
        smite_bonus: 0.0,
        smite_reduction: 0.0,
        fireball_burn: false,
        jordan_levels: 0,
        buff_dur_mult: 1.0,
        debuff_dur_div: 1.0,
        lava_resist_secs: 0.0,
    }
}

/// 全目录（顺序 = ItemId::as_u32）。数值逐条注明 w3t 来源。
pub const ITEMS: &[ItemDef] = &[
    // I000 速度之靴 1：+20 移速（卖出 4；GoldCost 0 → 占位 8）
    ItemDef { id: ItemId::Boots1, family: ItemFamily::Boots, tier: 1, cost: 8, sell: 4, name: "速度之靴 1", fx: ItemEffects { speed_add: 20.0, ..fx() } },
    // I002 坠饰 2：+20 生命（ReqUnit R000 升级链）
    ItemDef { id: ItemId::Amulet2, family: ItemFamily::Amulet, tier: 2, cost: 0, sell: 8, name: "坠饰 2", fx: ItemEffects { hp_add: 20.0, ..fx() } },
    // I004 死亡面具：+24% 吸血 +12% 受伤点回复、-0.3 回复惩罚；天罚加倍（TODO 2c）
    ItemDef { id: ItemId::FireMask, family: ItemFamily::Standalone, tier: 1, cost: 6, sell: 10, name: "死亡面具", fx: ItemEffects { lifesteal: 0.24, on_damage_heal: 0.12, regen_penalty: 0.3, ..fx() } },
    // I005 斗篷 1：+0.3 回复 / -5 移速
    ItemDef { id: ItemId::Cloak1, family: ItemFamily::Cloak, tier: 1, cost: 6, sell: 6, name: "斗篷 1", fx: ItemEffects { regen_add: 0.3, speed_penalty: 5.0, ..fx() } },
    // I001 头盔 1：-16% 受击退 +10 生命 -5 移速（不叠加）
    ItemDef { id: ItemId::Helm1, family: ItemFamily::Helm, tier: 1, cost: 10, sell: 8, name: "头盔 1", fx: ItemEffects { kb_resist_frac: 0.16, hp_add: 10.0, speed_penalty: 5.0, ..fx() } },
    // I006 头盔 2：-24% +15 生命 -10 移速
    ItemDef { id: ItemId::Helm2, family: ItemFamily::Helm, tier: 2, cost: 20, sell: 16, name: "头盔 2", fx: ItemEffects { kb_resist_frac: 0.24, hp_add: 15.0, speed_penalty: 10.0, ..fx() } },
    // I007 速度之靴 2：+30 移速（GoldCost 0 → 占位 16）
    ItemDef { id: ItemId::Boots2, family: ItemFamily::Boots, tier: 2, cost: 16, sell: 8, name: "速度之靴 2", fx: ItemEffects { speed_add: 30.0, ..fx() } },
    // I008 速度之靴 3：+40 移速（GoldCost 0 → 占位 24）
    ItemDef { id: ItemId::Boots3, family: ItemFamily::Boots, tier: 3, cost: 24, sell: 12, name: "速度之靴 3", fx: ItemEffects { speed_add: 40.0, ..fx() } },
    // I003 斗篷 3：+0.5 回复 / -5 移速
    ItemDef { id: ItemId::Cloak3, family: ItemFamily::Cloak, tier: 3, cost: 18, sell: 18, name: "斗篷 3", fx: ItemEffects { regen_add: 0.5, speed_penalty: 5.0, ..fx() } },
    // I009 斗篷 2：+0.4 回复 / -5 移速
    ItemDef { id: ItemId::Cloak2, family: ItemFamily::Cloak, tier: 2, cost: 12, sell: 12, name: "斗篷 2", fx: ItemEffects { regen_add: 0.4, speed_penalty: 5.0, ..fx() } },
    // I00A 头盔 3：-32% +20 生命 -15 移速
    ItemDef { id: ItemId::Helm3, family: ItemFamily::Helm, tier: 3, cost: 20, sell: 24, name: "头盔 3", fx: ItemEffects { kb_resist_frac: 0.32, hp_add: 20.0, speed_penalty: 15.0, ..fx() } },
    // I00B 坠饰 1：+10 生命（cost 6）
    ItemDef { id: ItemId::Amulet1, family: ItemFamily::Amulet, tier: 1, cost: 6, sell: 4, name: "坠饰 1", fx: ItemEffects { hp_add: 10.0, ..fx() } },
    // I00C 坠饰 3：+30 生命 +0.1 回复
    ItemDef { id: ItemId::Amulet3, family: ItemFamily::Amulet, tier: 3, cost: 0, sell: 12, name: "坠饰 3", fx: ItemEffects { hp_add: 30.0, regen_add: 0.1, ..fx() } },
    // I00D 火球法杖：火球附加点燃（火球伤 5.5+0.5L→持续 3+0.5L 2.5s；天罚加倍）TODO 2c
    ItemDef { id: ItemId::FireStaff, family: ItemFamily::Standalone, tier: 1, cost: 6, sell: 7, name: "火球法杖", fx: ItemEffects { fireball_burn: true, ..fx() } },
    // I00E 乔丹之石戒指：技能可超上限 +2 级（不可售）
    ItemDef { id: ItemId::Jordan, family: ItemFamily::Standalone, tier: 1, cost: 6, sell: 0, name: "乔丹之石戒指", fx: ItemEffects { jordan_levels: 2, ..fx() } },
    // I00F 鲜血之剑 1：天罚伤害增至 12
    ItemDef { id: ItemId::BloodSword1, family: ItemFamily::BloodSword, tier: 1, cost: 6, sell: 12, name: "鲜血之剑 1", fx: ItemEffects { smite_bonus: 12.0, ..fx() } },
    // I00G 鲜血之剑 2：天罚 13 + 每命中回 4
    ItemDef { id: ItemId::BloodSword2, family: ItemFamily::BloodSword, tier: 2, cost: 6, sell: 24, name: "鲜血之剑 2", fx: ItemEffects { smite_bonus: 13.0, on_damage_heal: 4.0, ..fx() } },
    // I00H 守护之盾：天罚 25% 减伤 50% 减击退，-10 最大生命
    ItemDef { id: ItemId::GuardianShield1, family: ItemFamily::GuardianShield, tier: 1, cost: 6, sell: 12, name: "守护之盾", fx: ItemEffects { smite_reduction: 0.25, hp_add: -10.0, ..fx() } },
    // I00I 守护之盾 2：天罚 75% 减伤，-10 最大生命
    ItemDef { id: ItemId::GuardianShield2, family: ItemFamily::GuardianShield, tier: 2, cost: 6, sell: 12, name: "守护之盾 2", fx: ItemEffects { smite_reduction: 0.75, hp_add: -10.0, ..fx() } },
    // I00J 熔岩靴 1：+15 移速 / 岩浆抵抗 3s / -0.1 回复惩罚（岩浆系统 TODO M5）
    ItemDef { id: ItemId::LavaBoots1, family: ItemFamily::LavaBoots, tier: 1, cost: 5, sell: 5, name: "熔岩靴 1", fx: ItemEffects { speed_add: 15.0, lava_resist_secs: 3.0, regen_penalty: 0.1, ..fx() } },
    // I00K 熔岩靴 2：+30 移速 / 4s
    ItemDef { id: ItemId::LavaBoots2, family: ItemFamily::LavaBoots, tier: 2, cost: 10, sell: 10, name: "熔岩靴 2", fx: ItemEffects { speed_add: 30.0, lava_resist_secs: 4.0, regen_penalty: 0.1, ..fx() } },
    // I00L 熔岩靴 3：+39 移速 / 5s
    ItemDef { id: ItemId::LavaBoots3, family: ItemFamily::LavaBoots, tier: 3, cost: 15, sell: 15, name: "熔岩靴 3", fx: ItemEffects { speed_add: 39.0, lava_resist_secs: 5.0, regen_penalty: 0.1, ..fx() } },
    // I00M 怀表 1：增益时长 ×1.15 / 减益时长 ÷1.15（另降沉默，见 S030）
    ItemDef { id: ItemId::PocketWatch1, family: ItemFamily::PocketWatch, tier: 1, cost: 6, sell: 6, name: "怀表 1", fx: ItemEffects { buff_dur_mult: 1.15, debuff_dur_div: 1.15, ..fx() } },
    // I00N 怀表 2：×1.25
    ItemDef { id: ItemId::PocketWatch2, family: ItemFamily::PocketWatch, tier: 2, cost: 6, sell: 12, name: "怀表 2", fx: ItemEffects { buff_dur_mult: 1.25, debuff_dur_div: 1.25, ..fx() } },
];

/// 携带上限（098b 英雄 6 格）。
pub const ITEM_SLOTS: usize = 6;

impl ItemId {
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    pub fn from_u32(v: u32) -> Option<ItemId> {
        if (v as usize) < ITEMS.len() {
            Some(ITEMS[v as usize].id)
        } else {
            None
        }
    }

    pub fn def(self) -> &'static ItemDef {
        &ITEMS[self.as_u32() as usize]
    }

    /// 该物品的下一档（无则 None）。
    pub fn next_tier(self) -> Option<ItemId> {
        let d = self.def();
        ITEMS
            .iter()
            .find(|x| x.family == d.family && x.tier == d.tier + 1)
            .map(|x| x.id)
    }
}

impl ItemDef {
    /// 同家族的完整升级链（按档位升序；数组顺序=w3t 表序，不保证档位升序）。
    pub fn chain(family: ItemFamily) -> Vec<&'static ItemDef> {
        let mut v: Vec<&'static ItemDef> = ITEMS.iter().filter(|d| d.family == family).collect();
        v.sort_by_key(|d| d.tier);
        v
    }
}

/// 汇总一组物品的聚合效果（kb 取最大、怀表乘子取最大，其余求和）。
pub fn aggregate(items: &[ItemId]) -> ItemEffects {
    let mut out = fx();
    for id in items {
        let f = &id.def().fx;
        out.speed_add += f.speed_add;
        out.speed_penalty += f.speed_penalty;
        out.hp_add += f.hp_add;
        out.regen_add += f.regen_add;
        out.regen_penalty += f.regen_penalty;
        out.kb_resist_frac = out.kb_resist_frac.max(f.kb_resist_frac);
        out.lifesteal += f.lifesteal;
        out.on_damage_heal += f.on_damage_heal;
        out.smite_bonus = out.smite_bonus.max(f.smite_bonus);
        out.smite_reduction = out.smite_reduction.max(f.smite_reduction);
        out.fireball_burn |= f.fireball_burn;
        out.jordan_levels += f.jordan_levels;
        out.buff_dur_mult = out.buff_dur_mult.max(f.buff_dur_mult);
        out.debuff_dur_div = out.debuff_dur_div.max(f.debuff_dur_div);
        out.lava_resist_secs = out.lava_resist_secs.max(f.lava_resist_secs);
    }
    out
}

/// 商店可见的购买入口：各家族最低档 + 无链单体（升级在持有低档时指向下一档）。
pub fn shop_catalog() -> Vec<&'static ItemDef> {
    let mut out: Vec<&'static ItemDef> = Vec::new();
    for d in ITEMS {
        if d.tier == 1 || d.family == ItemFamily::Standalone {
            out.push(d);
        }
    }
    out
}

/// 基础移速（war3）——移速平加换算用。
pub fn base_move_speed() -> f64 {
    Balance::default().base_speed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_24_items_with_unique_ids() {
        assert_eq!(ITEMS.len(), 24);
        for (i, d) in ITEMS.iter().enumerate() {
            assert_eq!(d.id.as_u32(), i as u32, "密集索引应与表序一致");
            assert!(!d.name.is_empty());
        }
        assert_eq!(ItemId::from_u32(23), Some(ItemId::PocketWatch2));
        assert_eq!(ItemId::from_u32(24), None);
    }

    #[test]
    fn upgrade_chains_are_contiguous() {
        // 靴/斗篷/头盔/坠饰/熔岩靴 3 档；怀表/剑/盾 2 档；升级链首尾相接。
        for (family, tiers) in [
            (ItemFamily::Boots, 3),
            (ItemFamily::Cloak, 3),
            (ItemFamily::Helm, 3),
            (ItemFamily::Amulet, 3),
            (ItemFamily::LavaBoots, 3),
            (ItemFamily::PocketWatch, 2),
            (ItemFamily::BloodSword, 2),
            (ItemFamily::GuardianShield, 2),
        ] {
            let chain = ItemDef::chain(family);
            assert_eq!(chain.len(), tiers, "{family:?} 链长度");
            for (i, d) in chain.iter().enumerate() {
                assert_eq!(d.tier as usize, i + 1, "{family:?} 档位应连续");
            }
            // 尾档无下一级；其余有。
            assert!(chain.last().unwrap().id.next_tier().is_none());
            for d in &chain[..chain.len() - 1] {
                assert!(d.id.next_tier().is_some(), "{:?} 应有下一档", d.id);
            }
        }
    }

    #[test]
    fn aggregate_sums_and_takes_max() {
        // 靴 3(+40) + 头盔 3(-32% kb, +20hp) + 怀表 2(×1.25)：kb 取 max 而非相加。
        let items = [ItemId::Boots3, ItemId::Helm3, ItemId::PocketWatch2];
        let f = aggregate(&items);
        assert!((f.speed_add - 40.0).abs() < 1e-9);
        assert!((f.hp_add - 20.0).abs() < 1e-9);
        assert!((f.kb_resist_frac - 0.32).abs() < 1e-9, "头盔不叠加：kb 取最大");
        assert!((f.buff_dur_mult - 1.25).abs() < 1e-9);
        // 乔丹 ×2 → +4 级
        let j = aggregate(&[ItemId::Jordan, ItemId::Jordan]);
        assert_eq!(j.jordan_levels, 4);
    }

    #[test]
    fn shop_catalog_lists_entry_points() {
        // 8 个家族入口 + 3 个单体（面具/法杖/乔丹）= 11。
        let cat = shop_catalog();
        assert_eq!(cat.len(), 11, "商店入口 = 家族 t1 + 单体，实际 {}", cat.len());
        assert!(cat.iter().all(|d| d.tier == 1));
    }
}
