//! 真实 Steam 传输（`steam` feature 下编译；非 vendored，连系统 Steam）。
//!
//! 用 `steamworks` 实现 `net::Transport`，使现有 lockstep / 多局 / 重连代码**零改动**地在 Steam 上运行：
//!   - `SteamTransport::init(app_id)`：`Client::init_app`（连当前登录账号）。
//!   - host 侧 `listen()`：`create_listen_socket_p2p` 开监听；client 侧 `connect_to(steam_id)` 用 `connect_p2p`。
//!   - `Transport::send_to/recv_from`：`Peer::Steam{id}` ↔ `steamworks::networking_types::NetworkingIdentity`；
//!     每帧 `run_callbacks()` 驱动 P2P 事件与收帧。
//!   - 大厅（`client.matchmaking()`）：`create_lobby` / `join_lobby` / `lobby_members` 做“身份→玩家槽位”（配 `lobby::LobbyPlayerTable`）。
//!
//! 单账号即可验证 init/读 SteamID/创建大厅；对战收发需双账号（真机 + 各自登录）。

use net::transport::{Peer, Transport};
use std::collections::HashMap;
use std::io;

use crate::lobby::LobbyPlayerTable;

/// Steam P2P 传输：持有已初始化 Client + 监听/P2P 连接映射。
pub struct SteamTransport {
    client: steamworks::Client,
    /// 若为 host：已创建的监听 socket。
    listen: Option<steamworks::networking_sockets::ListenSocket>,
    /// peer SteamID -> 会话（client 用 connect_p2p 所得，或 host accept 所得）。
    conns: HashMap<u64, steamworks::networking_sockets::NetConnection>,
    /// 我方虚拟端口（host 与 peer 约定一致即可）。
    virtual_port: i32,
    /// 大厅成员→玩家槽位表（host/client 各持一致视角，来自大厅成员名单）。
    _table: Option<LobbyPlayerTable>,
}

impl SteamTransport {
    /// 初始化 Steam（连当前登录账号 + 强制 AppID）。一台机器即可验证成功。
    /// 注意：一个进程只应有一个 `Client`，故应全局单例持有。
    pub fn init(app_id: u32, virtual_port: i32) -> io::Result<SteamTransport> {
        let client = steamworks::Client::init_app(app_id).map_err(|e| {
            io::Error::other(format!(
                "Steam init failed: 请确认 Steam 客户端在运行且已登录、AppID({app_id}) 有效。({e})"
            ))
        })?;
        Ok(SteamTransport {
            client,
            listen: None,
            conns: HashMap::new(),
            virtual_port,
            _table: None,
        })
    }

    /// pump 待处理 Steam 回调（大厅 / 网络连接事件）。建议每帧调用。
    pub fn run_callbacks(&self) {
        self.client.run_callbacks();
    }

    /// 本机 SteamID（u64）。
    pub fn steam_id(&self) -> u64 {
        self.client.user().steam_id().raw()
    }

    /// host：开 P2P 监听，返回本机 SteamID（供 client join / 大厅分发）。
    pub fn listen(&mut self) -> io::Result<u64> {
        let socks = self.client.networking_sockets();
        let ls = socks
            .create_listen_socket_p2p(
                self.virtual_port,
                std::iter::empty::<steamworks::networking_types::NetworkingConfigEntry>(),
            )
            .map_err(|e| io::Error::other(format!("create_listen_socket_p2p failed: {e:?}")))?;
        self.listen = Some(ls);
        Ok(self.steam_id())
    }

    /// client：连接到 host 的 SteamID。
    pub fn connect_to(&mut self, host_steam_id: u64) -> io::Result<()> {
        use steamworks::networking_types::NetworkingIdentity;
        let socks = self.client.networking_sockets();
        let identity = NetworkingIdentity::new_steam_id(steamworks::SteamId::from_raw(host_steam_id));
        let conn = socks
            .connect_p2p(
                identity,
                self.virtual_port,
                std::iter::empty::<steamworks::networking_types::NetworkingConfigEntry>(),
            )
            .map_err(|e| io::Error::other(format!("connect_p2p failed: {e:?}")))?;
        self.conns.insert(host_steam_id, conn);
        Ok(())
    }

    /// 处理 P2P 事件（host accept 新连接 / 断开清理）并收帧。返回 `(来源 SteamID, 数据)` 列表。
    fn pump_p2p(&mut self) -> Vec<(u64, Vec<u8>)> {
        self.client.run_callbacks();
        use steamworks::networking_types::ListenSocketEvent;
        let mut out = Vec::new();
        // host：accept 待连 / 收已连连接。
        if let Some(ls) = self.listen.as_ref() {
            // 注意：不能用 `ls.events()`（阻塞迭代器，会卡住主线程）；用非阻塞 `try_receive_event`。
            while let Some(ev) = ls.try_receive_event() {
                match ev {
                    ListenSocketEvent::Connecting(req) => {
                        let remote_id = req.remote().steam_id();
                        let _ = req.accept();
                        if let Some(id) = remote_id {
                            eprintln!("[steam-p2p] host accepted connection from {}", id.raw());
                        }
                    }
                    ListenSocketEvent::Connected(ev) => {
                        if let Some(id) = ev.remote().steam_id() {
                            let conn = ev.take_connection();
                            eprintln!("[steam-p2p] host connection established with {}", id.raw());
                            self.conns.insert(id.raw(), conn);
                        }
                    }
                    ListenSocketEvent::Disconnected(_) => {}
                }
            }
        }
        // 从每个已建立连接收帧。
        let peer_ids: Vec<u64> = self.conns.keys().copied().collect();
        for pid in peer_ids {
            if let Some(c) = self.conns.get_mut(&pid) {
                match c.receive_messages(32) {
                    Ok(msgs) => {
                        for m in msgs {
                            out.push((pid, m.data().to_vec()));
                        }
                    }
                    Err(_) => {
                        self.conns.remove(&pid);
                    }
                }
            }
        }
        out
    }

    /// 大厅（Matchmaking）句柄；用 `create_lobby`/`join_lobby`/`lobby_members` 做成员→槽位。
    pub fn matchmaking(&self) -> steamworks::Matchmaking {
        self.client.matchmaking()
    }

    /// 设置大厅→玩家槽位表（host 从 `lobby_members` 建表，client 用同样名单建一致表）。
    pub fn set_player_table(&mut self, t: LobbyPlayerTable) {
        self._table = Some(t);
    }
}

impl Transport for SteamTransport {
    fn send_to(&mut self, buf: &[u8], peer: &Peer) -> io::Result<usize> {
        use steamworks::networking_types::SendFlags;
        match peer {
            Peer::Steam { id, .. } => {
                let c = self
                    .conns
                    .get_mut(id)
                    .ok_or_else(|| io::Error::other(format!("no steam connection to SteamID {id}")))?;
                c.send_message(buf, SendFlags::RELIABLE_NO_NAGLE)
                    .map_err(|e| io::Error::other(format!("steam send_message failed: {e:?}")))?;
                Ok(buf.len())
            }
            Peer::Udp(_) => Err(io::Error::other("Peer::Udp 不适用于 SteamTransport")),
        }
    }

    fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<Option<(usize, Peer)>> {
        for (pid, data) in self.pump_p2p() {
            if data.len() <= buf.len() {
                buf[..data.len()].copy_from_slice(&data);
                return Ok(Some((data.len(), Peer::Steam { id: pid, conn: None })));
            }
            // 缓冲区过小则丢弃该包（上层 rcv 一般足够大）。
        }
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

    #[test]
    #[ignore = "需要真机 Steam 登录 + AppID(908660)，且每进程一个 Client"]
    fn init_and_read_own_steam_id() {
        let t = SteamTransport::init(908660, 1337).expect("Steam 初始化应成功");
        let sid = t.steam_id();
        assert!(sid != 0, "本机 SteamID 不应为 0");
        eprintln!("[net-steam] own SteamID={sid} (hex {sid:#x})");
        match t.local() {
            Peer::Steam { id, .. } => assert_eq!(id, sid),
            _ => panic!("local() 应为自己的 Peer::Steam"),
        }
    }
}
