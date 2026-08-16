//! 真实 Steam 传输（`steam` feature 下编译；非 vendored，连系统 Steam）。
//!
//! **当前阶段（单账号即可验证）**：
//!   - `SteamTransport::init(app_id)`：`steamworks::Client::init_app` —— 验证“Steam 客户端在跑 + AppID 有效 +
//!     有权启动应用”。这台机器一台账号即可确认成功。
//!   - `steam_id()` / `local()`：读到本机 SteamID，映射为 `Peer::Steam{ id }`（对应前端稳定身份/大厅槽位）。
//!   - `run_callbacks()`：每帧 pump Steam 回调（大厅/连接事件需要）。
//!
//! **待双账号（A2 后半）**：用 `client.networking_sockets()` 的 `SteamNetworkingSockets` 把每个
//! `Peer::Steam{ id }` 映射到真实 peer 会话，实现 `send_to/recv_from` 的真实收发。当前 send/recv 返回明确
//! “尚未接 peer 会话”的错误（不 panic），保证默认可编译、逻辑无害。

use net::transport::{Peer, Transport};
use std::io;

/// 真实 Steam 传输：持有已初始化的 `steamworks::Client`（其 Drop 会 SteamAPI_Shutdown）。
pub struct SteamTransport {
    client: steamworks::Client,
}

impl SteamTransport {
    /// 初始化 Steam（连当前登录账号 + 强制 AppID）。一台机器即可验证成功。
    /// 注意：一个进程只应有一个 `Client`（steamworks 规定）。
    pub fn init(app_id: u32) -> io::Result<SteamTransport> {
        let client = steamworks::Client::init_app(app_id).map_err(|e| {
            io::Error::other(format!(
                "Steam init failed: 请确认 Steam 客户端在运行且已登录、AppID({app_id}) 有效。({e})"
            ))
        })?;
        Ok(SteamTransport { client })
    }

    /// pump 待处理的 Steam 回调（大厅/网络事件）。建议每帧调用一次。
    pub fn run_callbacks(&self) {
        self.client.run_callbacks();
    }

    /// 本机 SteamID（u64）。稳定身份/大厅槽位的来源。
    pub fn steam_id(&self) -> u64 {
        self.client.user().steam_id().raw()
    }

    /// 大厅（Matchmaking）句柄；将来用 `create_lobby` / `join_lobby` + `lobby_members` 建玩家表。
    pub fn matchmaking(&self) -> steamworks::Matchmaking {
        self.client.matchmaking()
    }
}

impl Transport for SteamTransport {
    fn send_to(&mut self, _buf: &[u8], _peer: &Peer) -> io::Result<usize> {
        Err(io::Error::other(
            "SteamTransport.send_to 尚未接通 peer 会话 —— 需双账号 + SteamNetworkingSockets 实现 Peer::Steam→会话映射",
        ))
    }
    fn recv_from(&mut self, _buf: &mut [u8]) -> io::Result<Option<(usize, Peer)>> {
        // 暂无真实连接；A2 用 networking_sockets 收帧填这里。
        Ok(None)
    }
    fn local(&self) -> Peer {
        Peer::Steam {
            id: self.client.user().steam_id().raw(),
            conn: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单账号可验证：`--features net-steam/steam` + 真机 Steam 登录时，手动 `cargo test -- --ignored` 跑这个。
    /// （默认忽略，避免无 Steam 的 CI 也跑；one Client per process）。
    #[test]
    #[ignore = "需要真机 Steam 登录 + AppID(908660)"]
    fn init_and_read_own_steam_id() {
        let t = SteamTransport::init(908660).expect("Steam 客户端在跑且 AppID 有效时初始化应成功");
        t.run_callbacks();
        let sid = t.steam_id();
        assert!(sid != 0, "本机 SteamID 不应为 0");
        eprintln!("[net-steam] own SteamID = {sid} (hex {sid:#x})");
        match t.local() {
            Peer::Steam { id, .. } => assert_eq!(id, sid),
            _ => panic!("local() 应返回自己的 Peer::Steam"),
        }
    }
}
