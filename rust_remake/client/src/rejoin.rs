//! R5a / H1：重连会话描述 + **本地历史对局记录**（崩溃重开后「重新加入上一局」用）。
//!
//! 旧实现是**单槽文件** `rejoin.txt`：进房/离场写、干净离场**删** → 很容易被无关操作清掉
//! （退房、进新房间都会覆盖/删除）→ 归队入口"很容易就没了"。
//!
//! 现改为**历史记录** `rejoin_history.txt`：**追加写 + 有界**（保留最近 `HISTORY_MAX` 条），
//! 只把"最新一条**未结束**"的会话作为归队候选；结束/忽略用 `mark_ended` 打标（不删）。
//!
//! 本模块只做**序列化 + 读写 + 历史操作 + 新鲜度判定**（纯逻辑、可单测）；调用点在 `main.rs`。仅 Steam 使用。

use std::path::{Path, PathBuf};

/// 行格式版本（改格式时 +1；旧行直接忽略）。H1 起为 2。
pub const FORMAT_VERSION: u32 = 2;
/// 历史最多保留的条数（超出丢弃最旧的）。
pub const HISTORY_MAX: usize = 5;

/// 一份「上一局」的重连会话描述（历史记录里的一行）。
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
    /// 是否已结束（正常结束 / 局散 / 归队失败 / 用户忽略）→ 不再作为归队候选。
    pub ended: bool,
    /// 当时轮次（提示用）。
    pub round: u32,
    /// 当时的房主 SteamID（0=未知；提示用）。
    pub host_id: u64,
    /// 房间名（提示用；可为空）。
    pub room_name: String,
}

impl RejoinSession {
    /// 是否仍新鲜：`now - saved_at <= max_age`。`max_age = None` 表示**不限**；时钟回拨（`now < saved_at`）视为不新鲜。
    pub fn is_fresh(&self, now: u64, max_age: Option<u64>) -> bool {
        if now < self.saved_at_unix {
            return false;
        }
        match max_age {
            Some(max) => now - self.saved_at_unix <= max,
            None => true,
        }
    }

    /// 作为「归队候选」是否可用：未结束 + 新鲜 + 协议一致 + match_key 为本作值。
    pub fn is_candidate(&self, now: u64, max_age: Option<u64>, match_value: &str) -> bool {
        !self.ended
            && self.match_key == match_value
            && self.protocol_version == game_core::PROTOCOL_VERSION
            && self.is_fresh(now, max_age)
    }

    /// 单行序列化（`\t` 分隔固定字段；`room_name` 去掉制表/换行）。
    pub fn serialize_line(&self) -> String {
        let parts: Vec<String> = self.participants.iter().map(|p| p.to_string()).collect();
        let room = self.room_name.replace(['\t', '\n', '\r'], " ");
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            FORMAT_VERSION,
            self.lobby_id,
            self.match_key,
            self.my_steam_id,
            self.protocol_version,
            self.saved_at_unix,
            if self.ended { 1 } else { 0 },
            self.round,
            self.host_id,
            room,
            parts.join(",")
        )
    }

    /// 解析单行；版本不符/字段缺失/数字非法 → `None`。
    pub fn parse_line(line: &str) -> Option<RejoinSession> {
        let f: Vec<&str> = line.trim_end().split('\t').collect();
        if f.len() < 11 {
            return None;
        }
        if f[0].parse::<u32>().ok()? != FORMAT_VERSION {
            return None;
        }
        let participants = if f[10].is_empty() {
            Vec::new()
        } else {
            f[10]
                .split(',')
                .map(|s| s.parse::<u64>())
                .collect::<Result<Vec<_>, _>>()
                .ok()?
        };
        Some(RejoinSession {
            lobby_id: f[1].parse::<u64>().ok()?,
            match_key: f[2].to_string(),
            my_steam_id: f[3].parse::<u64>().ok()?,
            protocol_version: f[4].parse::<u32>().ok()?,
            saved_at_unix: f[5].parse::<u64>().ok()?,
            ended: f[6] == "1",
            round: f[7].parse::<u32>().ok()?,
            host_id: f[8].parse::<u64>().ok()?,
            room_name: f[9].to_string(),
            participants,
        })
    }
}

/// 默认路径：与 `settings.txt` 同目录的 `rejoin_history.txt`（旧 `rejoin.txt` **不再读取**）。
pub fn default_path() -> PathBuf {
    match crate::local_settings::default_path().parent() {
        Some(dir) => dir.join("rejoin_history.txt"),
        None => PathBuf::from("rejoin_history.txt"),
    }
}

/// 读取全部历史（按文件顺序 = 由旧到新）；坏行忽略；不存在 → 空。**校验格式版本为 `FORMAT_VERSION`。**
pub fn load_all(path: &Path) -> Vec<RejoinSession> {
    let Ok(t) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    t.lines().filter_map(RejoinSession::parse_line).collect()
}

/// 写回全部历史（每行一条）。失败只记一行日志。
fn write_all(path: &Path, list: &[RejoinSession]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let body: String = list.iter().map(|s| s.serialize_line()).collect::<Vec<_>>().join("\n");
    if let Err(e) = std::fs::write(path, body) {
        eprintln!("[rejoin] 保存历史失败（忽略）：{e}");
    }
}

/// 追加（或**就地更新**）一条历史：
/// - 若最后一条是**同一 lobby** → 用它替换（避免同一局反复写刷屏）；
/// - 否则追加；
/// - 始终**只保留最近 `HISTORY_MAX` 条**。
pub fn push(path: &Path, s: &RejoinSession) {
    let mut list = load_all(path);
    match list.last_mut() {
        Some(last) if last.lobby_id == s.lobby_id => *last = s.clone(),
        _ => list.push(s.clone()),
    }
    if list.len() > HISTORY_MAX {
        let drop_n = list.len() - HISTORY_MAX;
        list.drain(0..drop_n);
    }
    write_all(path, &list);
}

/// 取「最新一条**未结束**」的会话（用于归队候选；不做新鲜度/协议过滤，由调用方判断）。
pub fn latest_active(path: &Path) -> Option<RejoinSession> {
    load_all(path).into_iter().rev().find(|s| !s.ended)
}

/// 把指定 `lobby_id` 的历史条目标记为**已结束**（不删）。不存在则无操作。
pub fn mark_ended(path: &Path, lobby_id: u64) {
    let mut list = load_all(path);
    let mut changed = false;
    for s in list.iter_mut().filter(|s| s.lobby_id == lobby_id && !s.ended) {
        s.ended = true;
        changed = true;
    }
    if changed {
        write_all(path, &list);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(lobby: u64, ended: bool, saved: u64) -> RejoinSession {
        RejoinSession {
            lobby_id: lobby,
            match_key: "remake_arena_v1".to_string(),
            my_steam_id: 76561198043985466,
            participants: vec![76561199873494062, 76561198043985466],
            protocol_version: game_core::PROTOCOL_VERSION,
            saved_at_unix: saved,
            ended,
            round: 3,
            host_id: 76561199873494062,
            room_name: "my room".to_string(),
        }
    }

    #[test]
    fn line_roundtrip() {
        let s = sample(7, false, 1000);
        let back = RejoinSession::parse_line(&s.serialize_line()).expect("应能解析");
        assert_eq!(s, back);
    }

    #[test]
    fn line_rejects_bad() {
        assert!(RejoinSession::parse_line("").is_none(), "空行");
        assert!(RejoinSession::parse_line("1\t2\t3").is_none(), "字段不足");
        // 版本不符（把首字段改成 1）。
        let s = sample(7, false, 1000);
        let bad = format!("1\t{}", s.serialize_line().splitn(2, '\t').nth(1).unwrap());
        assert!(RejoinSession::parse_line(&bad).is_none(), "版本不符应拒绝");
    }

    #[test]
    fn sanitizes_room_name_newlines() {
        let mut s = sample(7, false, 1000);
        s.room_name = "a\tb\nc".to_string();
        let line = s.serialize_line();
        assert!(!line[..line.len() - 1].contains('\n'), "行内不应有换行");
        assert_eq!(RejoinSession::parse_line(&line).unwrap().room_name, "a b c");
    }

    #[test]
    fn candidate_filters() {
        let now = 10_000;
        let ok = sample(7, false, now);
        assert!(ok.is_candidate(now, Some(100), "remake_arena_v1"));
        let mut ended = ok.clone();
        ended.ended = true;
        assert!(!ended.is_candidate(now, Some(100), "remake_arena_v1"), "已结束不是候选");
        let mut pm = ok.clone();
        pm.protocol_version = 0;
        assert!(!pm.is_candidate(now, Some(100), "remake_arena_v1"), "协议不一致不是候选");
        let mut mk = ok.clone();
        mk.match_key = "other".into();
        assert!(!mk.is_candidate(now, Some(100), "remake_arena_v1"), "matchkey 不符不是候选");
        assert!(!ok.is_candidate(now, Some(1), "remake_arena_v1") || ok.saved_at_unix == now);
    }

    #[test]
    fn history_push_update_truncate_and_mark() {
        let dir = std::env::temp_dir().join(format!("warlock_rejoin_hist_{}", std::process::id()));
        let path = dir.join("rejoin_history.txt");
        let _ = std::fs::remove_file(&path);
        // 同一 lobby 连推两次 → 只 1 条（就地更新）。
        push(&path, &sample(1, false, 100));
        push(&path, &sample(1, false, 200));
        assert_eq!(load_all(&path).len(), 1, "同一 lobby 就地更新");
        // 不同 lobby → 追加；超过 HISTORY_MAX 丢弃最旧。
        for i in 2..=(HISTORY_MAX as u64 + 3) {
            push(&path, &sample(i, false, 100 + i));
        }
        let all = load_all(&path);
        assert_eq!(all.len(), HISTORY_MAX, "有界");
        assert_eq!(all.last().unwrap().lobby_id, HISTORY_MAX as u64 + 3, "保留最新的");
        // latest_active 取最新未结束。
        assert_eq!(latest_active(&path).unwrap().lobby_id, HISTORY_MAX as u64 + 3);
        // 标记已结束（不删）→ latest_active 回退到上一条。
        mark_ended(&path, HISTORY_MAX as u64 + 3);
        assert_eq!(load_all(&path).len(), HISTORY_MAX, "标记不删除");
        assert_eq!(latest_active(&path).unwrap().lobby_id, HISTORY_MAX as u64 + 2);
        let _ = std::fs::remove_file(&path);
    }
}
