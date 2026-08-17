//! 真实 Steam 传输（`steam` feature 下编译；非 vendored，连系统 Steam）。
//!
//! 用 `steamworks` 的 **ISteamNetworkingMessages**（面向 SteamID 的消息接口，UDP 风格）实现 `net::Transport`，
//! 使现有 lockstep / 多局 / 重连代码**零改动**地在 Steam 上运行：
//!   - `SteamTransport::init(app_id)`：`Client::init_app`（连当前登录账号）+ 注册自动接受入站会话。
//!   - `Transport::send_to/recv_from`：`Peer::Steam{id}` ↔ `NetworkingIdentity`；
//!     直接向指定 SteamID 发消息（`SendMessageToUser`），底层会话隐式建立、断后 `AutoRestartBrokenSession`
//!     自动重启，**无需自己管理 listen/accept/connect 连接状态**（官方文档：只在乎两台 peer 互通、
//!     不在乎 client/server 角色时，用 messages 比 sockets 合适）。
//!   - 大厅（`client.matchmaking()`）：`create_lobby` / `join_lobby` / `lobby_members` 做“身份→玩家槽位”（配 `lobby::LobbyPlayerTable`）。
//!
//! 为什么不用 ISteamNetworkingSockets（连接导向）：
//!   真机日志显示 sockets 连接 ESTABLISHED 后又 DISCONNECTED、反复断连重连——因为 sockets 需要你自己
//!   维护 listen/accept/connect + 连接状态，极易在握手/回调解耦处出错或随网络波动断掉。messages 接口
//!   隐式管理会话、`k_nSteamNetworkingSend_AutoRestartBrokenSession` 断后自动重启，天然消除此类卡点。
//! 可靠语义：RELIABLE 消息只要 send 成功就保证送达（同 host 同 channel 有序恰好一次）；会话尚未建立时
//!   send 会返回错误——所以配合一个补发队列（`pending_sends`），失败不丢、会话建立后自动补发。

use net::transport::{Peer, Transport};
use std::collections::{HashMap, VecDeque};
use std::io;

/// Steam 消息传输：持有已初始化 Client。
pub struct SteamTransport {
    client: steamworks::Client,
    /// 可靠发送补发队列：`send_to` 因「会话尚未建立 / 暂不可发」而失败时，不丢包而是入队，
    /// 待会话可用后在 `flush_pending` 里按序重发（RELIABLE send 成功即保证送达）。
    /// 键=peer SteamID；值=按发送顺序的待发消息。
    pending_sends: HashMap<u64, VecDeque<Vec<u8>>>,
    /// 诊断：本运输已打的 send 失败日志数（节流，避免刷屏）。
    send_fail_logs: u32,
}

impl SteamTransport {
    /// 初始化 Steam（连当前登录账号 + 强制 AppID）+ 注册自动接受入站会话。
    /// 注意：一个进程只应有一个 `Client`，故应全局单例持有。
    pub fn init(app_id: u32, _virtual_port: i32) -> io::Result<SteamTransport> {
        let client = steamworks::Client::init_app(app_id).map_err(|e| {
            io::Error::other(format!(
                "Steam init failed: 请确认 Steam 客户端在运行且已登录、AppID({app_id}) 有效。({e})"
            ))
        })?;
        // 自动接受所有入站会话（SendMessageToUser 会隐式建会话；对端需 accept，否则消息进不来）。
        client.networking_messages().session_request_callback(|req| {
            req.accept();
        });
        // 会话失败/对端关闭时打日志（诊断“怎么又断了”，实际由 AutoRestartBrokenSession 自动重启）。
        client.networking_messages().session_failed_callback(|_info| {
            eprintln!("[steam-p2p] networking session failed (will auto-restart on next send)");
        });
        Ok(SteamTransport {
            client,
            pending_sends: HashMap::new(),
            send_fail_logs: 0,
        })
    }

    /// pump 待处理 Steam 回调（大厅 / 会话建立）。建议每帧调用。
    pub fn run_callbacks(&self) {
        self.client.run_callbacks();
    }

    /// 本机 SteamID（u64）。
    pub fn steam_id(&self) -> u64 {
        self.client.user().steam_id().raw()
    }

    /// 向指定 SteamID 发一条可靠消息。会话未建立/暂不可发时入队待补发；AUTO_RESTART 使坏会话自动重启。
    fn send_reliable(&mut self, id: u64, data: &[u8]) -> bool {
        use steamworks::networking_types::{NetworkingIdentity, SendFlags};
        let flags = SendFlags::RELIABLE_NO_NAGLE | SendFlags::AUTO_RESTART_BROKEN_SESSION;
        let identity = NetworkingIdentity::new_steam_id(steamworks::SteamId::from_raw(id));
        self.client
            .networking_messages()
            .send_message_to_user(identity, flags, data, 0)
            .is_ok()
    }

    /// 把可靠补发队列里、且会话当前可发（send 成功）的消息按序补发。
    /// 用 AUTO_RESTART_BROKEN_SESSION 后无需自行判断“已建立”——send 成功即送达；失败则留在队首等下一帧。
    fn flush_pending(&mut self) {
        let keys: Vec<u64> = self.pending_sends.keys().copied().collect();
        for id in keys {
            // 用 while let 尽量清空：发送成功才 pop；失败则 break 保留在队首等下帧补发（FIFO 顺序）。
            while let Some(head) = self.pending_sends.get(&id).and_then(|q| q.front().cloned()) {
                let ok = {
                    // 若该 peer 之前排队的那条补发失败，先立即重试它；同时每轮先 run_callbacks 推进会话。
                    self.client.run_callbacks();
                    self.send_reliable(id, &head)
                };
                if !ok {
                    break; // 会话仍不可发：留队，下帧再补
                }
                self.pending_sends.get_mut(&id).map(|q| q.pop_front());
            }
            if self.pending_sends.get(&id).is_some_and(|q| q.is_empty()) {
                self.pending_sends.remove(&id);
            }
        }
    }

    /// 把一条待发消息入队（带长度上限防无限增长：长期不可发时丢最老，避免内存膨胀）。
    const PENDING_MAX: usize = 1024;
    fn push_pending(&mut self, id: u64, bytes: &[u8]) {
        let q = self.pending_sends.entry(id).or_default();
        if q.len() >= Self::PENDING_MAX {
            q.pop_front();
            if self.send_fail_logs < 10 {
                self.send_fail_logs += 1;
                eprintln!("[steam-p2p] pending outbox overflow for {id}, dropping oldest");
            }
        }
        q.push_back(bytes.to_vec());
    }

    /// 从「指定 SteamID 频道」读取收到的消息（UDP 风格，接收方无需维护连接）。
    fn recv_messages_once(&mut self, batch: usize) -> Vec<(u64, Vec<u8>)> {
        let mut out = Vec::new();
        self.client.run_callbacks();
        let msgs = self.client.networking_messages().receive_messages_on_channel(0, batch);
        for m in msgs {
            if let Some(sid) = m.identity_peer().steam_id() {
                out.push((sid.raw(), m.data().to_vec()));
            }
        }
        out
    }

    /// 某 Steam 端点的会话当前是否为 Connected（诊断用；收发不依赖它）。
    pub fn is_established(&self, id: u64) -> bool {
        use steamworks::networking_types::{NetworkingIdentity, NetworkingConnectionState};
        let identity = NetworkingIdentity::new_steam_id(steamworks::SteamId::from_raw(id));
        let (state, _, _) = self.client.networking_messages().get_session_connection_info(&identity);
        state == NetworkingConnectionState::Connected
    }

    /// 大厅（Matchmaking）句柄；用 `create_lobby`/`join_lobby`/`lobby_members` 做成员→槽位。
    pub fn matchmaking(&self) -> steamworks::Matchmaking {
        self.client.matchmaking()
    }

    /// 好友（Friends）句柄；用 `get_friend(id).name()` 拿 Steam 昵称。
    pub fn friends(&self) -> steamworks::Friends {
        self.client.friends()
    }
}

impl Transport for SteamTransport {
    fn send_to(&mut self, buf: &[u8], peer: &Peer) -> io::Result<usize> {
        // 先把上一轮“会话未建/暂不可发”而积压的可靠消息补发出去。
        self.flush_pending();
        match peer {
            Peer::Steam { id, .. } => {
                // 若该 peer 还有未补发成功的历史可靠消息，此刻直接发新 buf 会乱序（RELIABLE 有序）；
                // 统一追加到队尾，让 flush 按 FIFO 顺序发出。
                if self.pending_sends.get(id).is_some_and(|q| !q.is_empty()) {
                    self.push_pending(*id, buf);
                    return Ok(buf.len());
                }
                match self.send_reliable(*id, buf) {
                    true => Ok(buf.len()),
                    false => {
                        // 会话尚未建立 / 暂不可发 / 发送失败：入队待补发，不丢失。
                        if self.send_fail_logs < 10 {
                            self.send_fail_logs += 1;
                            eprintln!("[steam-p2p] send_to {id}: not sendable yet (session establishing?) -> queued for re-send");
                        }
                        self.push_pending(*id, buf);
                        Ok(buf.len())
                    }
                }
            }
            Peer::Udp(_) => Err(io::Error::other("Peer::Udp 不适用于 SteamTransport")),
        }
    }

    fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<Option<(usize, Peer)>> {
        // 先推进会话/补发，再收帧。
        self.flush_pending();
        for (pid, data) in self.recv_messages_once(32) {
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
