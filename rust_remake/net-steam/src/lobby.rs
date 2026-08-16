//! 大厅→玩家槽位映射（Steam-forward 的核心设计契约，**无 Steam 也可测**）。
//!
//! Steam 联机与本地 UDP 的关键差异：
//! - UDP：host 靠 `poll_join` 逐个收 client、按到达顺序分 `my_index`，并靠来源端点去重。
//! - Steam：host 用 `LobbyMatching::create_lobby` 开房，玩家用 `join_lobby` 加入。Steam 大厅本身
//!   就维护一份**成员名单（CSteamID）**，所以我们**不需要**轮询握手来“发现谁在”；直接拿名单 + 每个成员的
//!   稳定身份（SteamID）映射到玩家槽位即可。这正好对上已有的 `join_dedups_by_stable_identity`（按身份去重/找回槽）。
//!
//! `SteamLobby` 在这里定义“成员名单 → 玩家槽位 + 稳定身份”的纯函数映射，并给出 host / 普通玩家判定，
//! 供给前端（`net::handshake` 或直接 `HostLockstep`/`ClientLockstep`）使用。它不依赖 `steamworks`，
//! 因此没有 `steam` feature 也能开发与单测。

use std::collections::BTreeMap;

/// 大厅成员的稳定身份（此处用 u64 表示：将来即 `CSteamID::ConvertToUint64()`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SteamID(pub u64);

/// 从大厅成员名单派生的“本局玩家表”。
#[derive(Debug, Clone)]
pub struct LobbyPlayerTable {
    /// 实例：host 先放 0，然后按大厅内部成员顺序排。key = 玩家槽位，value = 稳定身份。
    slots: BTreeMap<u8, SteamID>,
    /// host 的稳定身份。
    host: SteamID,
}

impl LobbyPlayerTable {
    /// 由大厅成员名单构建玩家表。`members` 不含身份重复（SteamID 天然唯一）。
    /// 排序规则：host 恒为槽 0，其余按 `members` 里非 host 的身份升序排 → 确定性、端端一致。
    pub fn new(host: SteamID, mut members: Vec<SteamID>) -> LobbyPlayerTable {
        members.retain(|m| *m != host);
        members.sort();
        let mut slots = BTreeMap::new();
        slots.insert(0, host);
        for (i, m) in members.iter().enumerate() {
            slots.insert((i + 1) as u8, *m);
        }
        LobbyPlayerTable { slots, host }
    }

    /// 是否是大厅房主（host）。
    pub fn is_host(&self, id: SteamID) -> bool {
        id == self.host
    }

    /// 某稳定身份对应的玩家槽位（没有则 None —— 说明不在本局名单里，应拒绝其输入）。
    pub fn slot_of(&self, id: SteamID) -> Option<u8> {
        self.slots.iter().find(|(_, v)| **v == id).map(|(k, _)| *k)
    }

    /// 玩家数（含 host）。
    pub fn total_players(&self) -> usize {
        self.slots.len()
    }

    /// 各 slot 的稳定身份。
    pub fn identities_in_order(&self) -> Vec<(u8, SteamID)> {
        self.slots.iter().map(|(k, v)| (*k, *v)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lobby_table_maps_members_to_slots_deterministically() {
        let host = SteamID(100);
        let mut members = vec![SteamID(3), SteamID(1), SteamID(2)];
        let table = LobbyPlayerTable::new(host, members.clone());

        assert_eq!(table.total_players(), 4, "host + 3");
        assert!(table.is_host(SteamID(100)));
        assert!(!table.is_host(SteamID(1)));
        // host 槽 0；其余按身份升序 → 1/2/3。双重保证：任一端都给出一致序列。
        assert_eq!(table.slot_of(SteamID(100)), Some(0));
        assert_eq!(table.slot_of(SteamID(1)), Some(1));
        assert_eq!(table.slot_of(SteamID(2)), Some(2));
        assert_eq!(table.slot_of(SteamID(3)), Some(3));
        assert_eq!(table.slot_of(SteamID(999)), None, "不在名单里的身份应无槽位");
        let order = table.identities_in_order();
        assert_eq!(order, vec![(0, SteamID(100)), (1, SteamID(1)), (2, SteamID(2)), (3, SteamID(3))]);

        // 成员顺序不同，结果也应一致（确定性）。
        members.sort();
        let table2 = LobbyPlayerTable::new(host, members);
        assert_eq!(table.identities_in_order(), table2.identities_in_order());
    }
}
