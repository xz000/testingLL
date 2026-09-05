//! 玩家配置快照（跨端确定性同步用）。
//!
//! Dota2 式「学习阶段 → 就绪 → 下一局」需要把每名玩家的个人进度跨端一致地同步给所有端，
//! 否则各端 `World.players[].skill_levels` 与 `profiles[].gold` 会分叉、锁步破裂。
//! 本模块定义 `PlayerConfig` —— 一个可编解码、可 `from_profile` / `apply_to` 的「玩家配置快照」
//! （技能等级 + 键位绑定 + 金币），并兼容未来扩展（开头带版本 + 各段长度前缀，
//! 以后加属性字段只需在快照里增段、由派生规则 synth 到战斗数值，网络层不变）。

use crate::meta::PlayerProfile;
use crate::skill::SkillId;

/// 快照版本：每次改字段布局就 +1，供将来两端一致性校验。
/// v2（4.6b）：加入 attributes。
/// v3：attributes 扩充（mana_max / mana_regen）。
/// v4：加入 growth_points。
/// v5（098b 复刻 M0）：attributes 收缩为 5 个 u32（移除 mana_max / mana_regen，
///     无蓝量系统 PORT_098B_DECISIONS.md D3）。
/// v6（M3）：加入 items（u8 数量 + 每 item u32 id）。
/// v7（B1 精通）：加入 mastery 4 字节（生命/远程/时间/背包）。
pub const CONFIG_VERSION: u8 = 7;
/// 键位槽数量（= CastKey 数量）。
pub const KEY_SLOTS: usize = 8;

/// 简化读写小端/大端 u32/i64 的辅助（长度自管，便于加段扩展）。
fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}
fn put_i64(out: &mut Vec<u8>, v: i64) {
    out.extend_from_slice(&v.to_be_bytes());
}
fn u16_at(buf: &[u8], pos: usize) -> Option<u16> {
    Some(u16::from_be_bytes(buf.get(pos..pos + 2)?.try_into().ok()?))
}
fn u32_at(buf: &[u8], pos: usize) -> Option<u32> {
    Some(u32::from_be_bytes(buf.get(pos..pos + 4)?.try_into().ok()?))
}
fn i64_at(buf: &[u8], pos: usize) -> Option<i64> {
    Some(i64::from_be_bytes(buf.get(pos..pos + 8)?.try_into().ok()?))
}

/// 玩家配置快照：可编解码、可 `from_profile` / `apply_to`。
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerConfig {
    pub skill_levels: Vec<u32>,
    /// 每个键位绑定（索引 = CastKey::as_u32；`None` = 未绑定）。
    pub key_slots: [Option<SkillId>; KEY_SLOTS],
    pub gold: i64,
    pub gold_spent: i64,
    /// 战斗属性（4.6b）。
    pub attributes: crate::attribute::Attributes,
    /// 成长点（4.6b）：用于购买属性（可用金币兑换）。
    pub growth_points: u32,
    /// 持有的物品（M3；升级链同家族替换）。
    pub items: Vec<crate::item::ItemId>,
    /// 精通研究等级（v7，098c D12.3）：[生命, 远程, 时间, 背包]。
    pub mastery: [u8; 4],
}

impl PlayerConfig {
    /// 从 `PlayerProfile` 生成快照。
    pub fn from_profile(p: &PlayerProfile) -> Self {
        let mut key_slots = [None; KEY_SLOTS];
        for (i, slot) in key_slots.iter_mut().enumerate().take(KEY_SLOTS) {
            *slot = p.key_slots[i];
        }
        PlayerConfig {
            skill_levels: p.skill_levels.clone(),
            key_slots,
            gold: p.gold as i64,
            gold_spent: p.gold_spent as i64,
            attributes: p.attributes,
            growth_points: p.growth_points,
            items: p.items.clone(),
            mastery: [p.mastery.life, p.mastery.range, p.mastery.time, p.mastery.backpack],
        }
    }

    /// 把快照应用回 `PlayerProfile`（供本端学习阶段后/收到 host 广播后同步本地档案）。
    /// 只回写可同步字段（等级/绑定/金币），不覆盖击杀/名次等统计（那些本端一致不用同步）。
    pub fn apply_to(&self, p: &mut PlayerProfile) {
        let n = p.skill_levels.len();
        for (i, lv) in self.skill_levels.iter().enumerate().take(n) {
            p.skill_levels[i] = *lv;
        }
        for (dst, src) in p.key_slots.iter_mut().zip(self.key_slots.iter()) {
            *dst = *src;
        }
        p.gold = self.gold as i32;
        p.gold_spent = self.gold_spent as i32;
        p.attributes = self.attributes;
        p.growth_points = self.growth_points;
        p.items = self.items.clone();
        p.mastery = crate::meta::Mastery {
            life: self.mastery[0],
            range: self.mastery[1],
            time: self.mastery[2],
            backpack: self.mastery[3],
        };
    }

    /// 编码为字节（带版本 + 长度前缀，可扩展）。
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(CONFIG_VERSION);
        out.extend_from_slice(&(self.skill_levels.len() as u16).to_be_bytes());
        for lv in &self.skill_levels {
            put_u32(&mut out, *lv);
        }
        // key_slots：先给槽数（应为 8），每个槽 `[present][skill_id u32 if present]`。
        out.push(KEY_SLOTS as u8);
        for s in &self.key_slots {
            match s {
                Some(id) => {
                    out.push(1);
                    put_u32(&mut out, id.as_u32());
                }
                None => out.push(0),
            }
        }
        put_i64(&mut out, self.gold);
        put_i64(&mut out, self.gold_spent);
        // attributes（v5）：固定的 5 个 u32（蓝量属性已移除）。
        put_u32(&mut out, self.attributes.hp_bonus);
        put_u32(&mut out, self.attributes.speed_bonus);
        put_u32(&mut out, self.attributes.armor);
        put_u32(&mut out, self.attributes.spell_resist);
        put_u32(&mut out, self.attributes.kb_resist);
        // growth_points（v4）。
        put_u32(&mut out, self.growth_points);
        // items（v6）：u8 数量 + 每 item u32。
        out.push(self.items.len() as u8);
        for it in &self.items {
            put_u32(&mut out, it.as_u32());
        }
        // mastery（v7）：生命/远程/时间/背包各 1 字节。
        out.extend_from_slice(&self.mastery);
        out
    }

    /// 从字节解码；非法/截断返回 `None`。
    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.is_empty() || buf[0] != CONFIG_VERSION {
            return None;
        }
        let mut pos = 1;
        let count = u16_at(buf, pos)? as usize;
        pos += 2;
        let mut skill_levels = Vec::with_capacity(count);
        for _ in 0..count {
            skill_levels.push(u32_at(buf, pos)?);
            pos += 4;
        }
        let slot_count = *buf.get(pos)? as usize;
        pos += 1;
        if slot_count > KEY_SLOTS {
            return None;
        }
        let mut key_slots = [None; KEY_SLOTS];
        for slot in key_slots.iter_mut().take(slot_count) {
            let present = *buf.get(pos)?;
            pos += 1;
            if present != 0 {
                let id = SkillId::from_u32(u32_at(buf, pos)?);
                pos += 4;
                *slot = Some(id);
            }
        }
        // 剩余的槽默认 None（buf 里可能不写足 8，但编码总会写足；这里兼容更短）。
        let gold = i64_at(buf, pos)?;
        pos += 8;
        let gold_spent = i64_at(buf, pos)?;
        pos += 8;
        // attributes（v5）：5 个 u32（蓝量属性已移除）。
        let attributes = crate::attribute::Attributes {
            hp_bonus: u32_at(buf, pos)?,
            speed_bonus: u32_at(buf, pos + 4)?,
            armor: u32_at(buf, pos + 8)?,
            spell_resist: u32_at(buf, pos + 12)?,
            kb_resist: u32_at(buf, pos + 16)?,
        };
        let growth_points = u32_at(buf, pos + 20)?;
        // items（v6）。
        let n_items = *buf.get(pos + 24)? as usize;
        pos += 25;
        let mut items = Vec::with_capacity(n_items);
        for _ in 0..n_items {
            let id = u32_at(buf, pos)?;
            pos += 4;
            items.push(crate::item::ItemId::from_u32(id)?);
        }
        // mastery（v7）。
        let mastery = [*buf.get(pos)?, *buf.get(pos + 1)?, *buf.get(pos + 2)?, *buf.get(pos + 3)?];
        Some(PlayerConfig {
            skill_levels,
            key_slots,
            gold,
            gold_spent,
            attributes,
            growth_points,
            items,
            mastery,
        })
    }
}

/// 让 gold/spent 以 i64 编解码。

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::{MatchConfig, MatchState};

    fn profile() -> PlayerProfile {
        let ms = MatchState::new(MatchConfig::default(), &[0, 1], 34);
        ms.profiles[0].clone()
    }

    #[test]
    fn encode_decode_roundtrip_preserves_config() {
        let mut p = profile();
        // 手动改一点：升一级、绑个键、改金币、加属性（4.6b）。
        p.gold = 12345;
        p.gold_spent = 500;
        p.attributes = crate::attribute::Attributes { hp_bonus: 3, speed_bonus: 2, ..Default::default() };
        p.mastery = crate::meta::Mastery { life: 2, range: 1, time: 3, backpack: 2 };
        let cfg = PlayerConfig::from_profile(&p);
        let bytes = cfg.encode();
        let dec = PlayerConfig::decode(&bytes).expect("应能解码");
        assert_eq!(dec, cfg, "快照往返应一致");
        assert_eq!(dec.attributes.hp_bonus, 3, "属性应随快照同步");
        assert_eq!(dec.attributes.speed_bonus, 2);
        assert_eq!(dec.mastery, [2, 1, 3, 2], "精通应随快照同步（v7）");
    }

    #[test]
    fn apply_to_writes_synced_fields() {
        let mut p = profile();
        let mut cfg = PlayerConfig::from_profile(&p);
        cfg.skill_levels[3] = 7;
        cfg.gold = 999;
        cfg.apply_to(&mut p);
        assert_eq!(p.gold, 999);
        assert_eq!(p.skill_levels[3], 7);
    }

    #[test]
    fn decode_rejects_bad_version() {
        assert!(PlayerConfig::decode(&[99, 0, 0]).is_none());
    }
}
