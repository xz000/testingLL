//! R5a：重连会话描述（崩溃重开后「重新加入上一局」用）。
//!
//! 崩溃重开 = 新进程，不知道上一局的 `lobby_id` / 自己身份 / 参与者。故在进房/开局时把一份
//! 极小的**会话描述**写到磁盘（与 `settings.txt` 同目录的 `rejoin.txt`），重开后据此重进大厅、拉快照归队。
//!
//! 本模块只做**序列化 + 读写 + 新鲜度判定**（纯逻辑、可单测）；写/读/清的**调用点**在 `main.rs`（R5c）。
//! 仅 Steam 使用。

// R5a 先落地序列化/读写；写/读/清的调用点在 R5c 接入。在此之前允许这些 API 尚未被使用。
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// 会话文件格式版本（改格式时 +1，旧格式直接忽略）。
pub const FORMAT_VERSION: u32 = 1;
/// 会话有效期（秒）：超过则视为过期忽略（避免重开到很久以前的旧局）。
pub const MAX_AGE_SECS: u64 = 30 * 60;

/// 一份「上一局」的重连会话描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejoinSession {
    /// 大厅 id（`LobbyId::raw()`）。
    pub lobby_id: u64,
    /// 大厅 matchkey 值（本作用 `remake_arena_v1`）。
    pub match_key: String,
    /// 本机 SteamID（稳定身份，归队时用）。
    pub my_steam_id: u64,
    /// 本局参与者 SteamID（按玩家槽位序），供归队时核对/索引。
    pub participants: Vec<u64>,
    /// 联机兼容版本（`game_core::PROTOCOL_VERSION`）。
    pub protocol_version: u32,
    /// 保存时刻（Unix 秒）。
    pub saved_at_unix: u64,
}

impl RejoinSession {
    /// 是否仍新鲜（`now - saved_at <= MAX_AGE_SECS`）。时钟回拨（`now < saved_at`）视为不新鲜。
    pub fn is_fresh(&self, now: u64) -> bool {
        now >= self.saved_at_unix && now - self.saved_at_unix <= MAX_AGE_SECS
    }

    /// 紧凑 `key=value` 文本（行分隔；人类可读，便于调试）。
    pub fn serialize(&self) -> String {
        let parts: Vec<String> = self.participants.iter().map(|p| p.to_string()).collect();
        format!(
            "v={}\nlobby={}\nmatch={}\nme={}\nprotocol={}\nsaved_at={}\nparticipants={}\n",
            FORMAT_VERSION,
            self.lobby_id,
            self.match_key,
            self.my_steam_id,
            self.protocol_version,
            self.saved_at_unix,
            parts.join(",")
        )
    }

    /// 解析；缺字段/版本不符/数字非法 → `None`（不 panic）。
    pub fn parse(text: &str) -> Option<RejoinSession> {
        let mut version = None;
        let mut lobby_id = None;
        let mut match_key = None;
        let mut my_steam_id = None;
        let mut protocol_version = None;
        let mut saved_at_unix = None;
        let mut participants = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let (k, v) = line.split_once('=')?;
            match k {
                "v" => version = v.parse::<u32>().ok(),
                "lobby" => lobby_id = v.parse::<u64>().ok(),
                "match" => match_key = Some(v.to_string()),
                "me" => my_steam_id = v.parse::<u64>().ok(),
                "protocol" => protocol_version = v.parse::<u32>().ok(),
                "saved_at" => saved_at_unix = v.parse::<u64>().ok(),
                "participants" => {
                    participants = v
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.parse::<u64>())
                        .collect::<Result<Vec<_>, _>>()
                        .ok()?;
                }
                _ => {}
            }
        }
        if version != Some(FORMAT_VERSION) {
            return None;
        }
        Some(RejoinSession {
            lobby_id: lobby_id?,
            match_key: match_key?,
            my_steam_id: my_steam_id?,
            participants,
            protocol_version: protocol_version?,
            saved_at_unix: saved_at_unix?,
        })
    }
}

/// 默认路径：与 `settings.txt` 同目录的 `rejoin.txt`（取不到则退回当前目录）。
pub fn default_path() -> PathBuf {
    match crate::local_settings::default_path().parent() {
        Some(dir) => dir.join("rejoin.txt"),
        None => PathBuf::from("rejoin.txt"),
    }
}

/// 写入（best-effort；失败只记一行日志）。`saved_at_unix` 由调用方填（便于测试注入时间）。
pub fn save(path: &Path, s: &RejoinSession) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(path, s.serialize()) {
        eprintln!("[rejoin] 保存失败（忽略）：{e}");
    }
}

/// 读取；不存在/格式非法 → `None`。
pub fn load(path: &Path) -> Option<RejoinSession> {
    let t = std::fs::read_to_string(path).ok()?;
    RejoinSession::parse(&t)
}

/// 清除（干净离场时调用；不存在/失败忽略）。
pub fn clear(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RejoinSession {
        RejoinSession {
            lobby_id: 109775244404366707,
            match_key: "remake_arena_v1".to_string(),
            my_steam_id: 76561198043985466,
            participants: vec![76561199873494062, 76561198043985466],
            protocol_version: 40,
            saved_at_unix: 1_791_475_000,
        }
    }

    #[test]
    fn serialize_parse_roundtrip() {
        let s = sample();
        let back = RejoinSession::parse(&s.serialize()).expect("应能解析");
        assert_eq!(s, back);
    }

    #[test]
    fn parse_rejects_missing_or_wrong_version() {
        assert!(RejoinSession::parse("").is_none(), "空文本应拒绝");
        assert!(RejoinSession::parse("v=2\nlobby=1\nmatch=m\nme=2\nprotocol=40\nsaved_at=1\n").is_none(), "版本不符应拒绝");
        // 缺 lobby。
        assert!(RejoinSession::parse("v=1\nmatch=m\nme=2\nprotocol=40\nsaved_at=1\n").is_none());
        // participants 非法数字。
        assert!(
            RejoinSession::parse("v=1\nlobby=1\nmatch=m\nme=2\nprotocol=40\nsaved_at=1\nparticipants=1,x\n")
                .is_none()
        );
    }

    #[test]
    fn parse_allows_empty_participants() {
        let s = RejoinSession::parse("v=1\nlobby=7\nmatch=m\nme=9\nprotocol=40\nsaved_at=5\nparticipants=\n")
            .expect("空参与集也应可解析");
        assert!(s.participants.is_empty());
    }

    #[test]
    fn freshness_boundaries() {
        let s = sample(); // saved_at = 1_791_475_000
        assert!(s.is_fresh(s.saved_at_unix), "刚存 = 新鲜");
        assert!(s.is_fresh(s.saved_at_unix + MAX_AGE_SECS), "正好过期边界内 = 新鲜");
        assert!(!s.is_fresh(s.saved_at_unix + MAX_AGE_SECS + 1), "超过有效期 = 过期");
        assert!(!s.is_fresh(s.saved_at_unix - 1), "时钟回拨 = 不新鲜");
    }

    #[test]
    fn save_load_clear_roundtrip_via_fs() {
        let dir = std::env::temp_dir().join(format!("warlock_rejoin_test_{}", std::process::id()));
        let path = dir.join("rejoin.txt");
        clear(&path);
        assert!(load(&path).is_none(), "清除后应无");
        let s = sample();
        save(&path, &s);
        assert_eq!(load(&path), Some(s), "保存后可读回");
        clear(&path);
        assert!(load(&path).is_none(), "清除后应无");
    }
}
