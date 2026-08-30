//! 建连握手层：只负责 JOIN / ACK / READY / GO 生命周期，产出"可开始推进"的信号。
//! 每帧收发不在这里（由 `lockstep` 负责）。传输仍为 `T: Transport` 抽象。
//!
//! 职责：
//! - `HostHandshake`：收 client JOIN → 确认序号（ACK）→ 收 READY → 收齐后广播 GO（带起始 seq）。
//! - `ClientHandshake`：发 JOIN → 收 ACK（拿序号+人数）→ 发 READY → 收 GO（拿起始 seq）。
//!
//! 完成后通过 `into_transport()` 把 transport 移交给 `lockstep` 继续使用。

use crate::proto::Packet;
use crate::transport::{Peer, Transport};
use std::io;

/// host 侧建连/统一握手。
pub struct HostHandshake<T: Transport> {
    transport: Option<T>,
    /// 总玩家数（含 host 自身，若参与）。
    total: usize,
    /// host 是否参与（占 player 0）。
    local_base: u8,
    /// 各 player 的 peer（仅 client 有值；下标=player index）。
    peers: Vec<Option<Peer>>,
    /// 各 player 的稳定身份（u64，Steam=SteamID；局域网=客户端随机）。用于按身份去重/按身份重连取槽。
    identities: Vec<Option<u64>>,
    pub joined: usize,
}

impl<T: Transport> HostHandshake<T> {
    pub fn new(transport: T, total_players: usize, host_participates: bool) -> Self {
        let local_base = if host_participates { 1 } else { 0 };
        HostHandshake {
            transport: Some(transport),
            total: total_players,
            local_base,
            peers: vec![None; total_players],
            identities: vec![None; total_players],
            joined: 0,
        }
    }

    /// 期望加入的 client 数。
    pub fn expected(&self) -> usize {
        self.total - self.local_base as usize
    }

    /// 收 JOIN → 按已登记的稳定身份去重/分配序号 → 回 ACK（附身份回显）。返回是否已收齐所有（去重后的）client。
    /// 关键：同一身份重复发 JOIN 时【不重复计数/分配】，而是重发已分配的 ACK，
    /// 否则 client 疯狂重发的 JOIN 会撑爆 joined/序号，导致其他 client 收不到 ACK。
    /// 兼容：Steam 下身份=SteamID，局域网=客户端随机；早期 client 若不带身份（identity=0）回退到按来源 Peer 去重。
    pub fn poll_join(&mut self, rcv: &mut [u8]) -> bool {
        let expected = self.expected();
        let (local_base, total) = (self.local_base, self.total);
        let Some(transport) = self.transport.as_mut() else {
            return false;
        };
        loop {
            match transport.recv_from(rcv) {
                Ok(Some((n, from))) => {
                    if let Some(Packet::Join { identity }) = Packet::decode(&rcv[..n]) {
                        // 带身份：按身份找已占用的槽位（重连/重复 JOIN 复用该槽）。
                        let existing = if identity != 0 {
                            self.identities.iter().position(|i| *i == Some(identity))
                        } else {
                            None
                        };
                        // 无身份（旧客户端）→ 退回按来源 Peer 找已占槽位。
                        let existing = existing.or_else(|| self.peers.iter().position(|p| *p == Some(from)));
                        if let Some(idx) = existing {
                            self.peers[idx] = Some(from);
                            let ack = Packet::Ack { my_index: idx as u8, players: total as u8, identity };
                            let _ = transport.send_to(&ack.encode(), &from);
                            continue;
                        }
                        // 新加入：若还有空位则分配；否则忽略（已满）。
                        if self.joined < expected {
                            let idx = local_base + self.joined as u8;
                            if (idx as usize) < self.peers.len() {
                                self.peers[idx as usize] = Some(from);
                                self.identities[idx as usize] = Some(identity);
                            }
                            self.joined += 1;
                            let ack = Packet::Ack { my_index: idx, players: total as u8, identity };
                            let _ = transport.send_to(&ack.encode(), &from);
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        self.joined >= expected
    }

    /// 取某 player 的稳定身份（若有）。
    pub fn identity_of(&self, player_index: u8) -> Option<u64> {
        self.identities.get(player_index as usize).copied().flatten()
    }

    /// 释放 transport，交由上层（HostLockstep）使用。
    pub fn into_transport(mut self) -> T {
        self.transport.take().expect("transport already taken")
    }

}

/// client 侧建连/统一握手。
pub struct ClientHandshake<T: Transport> {
    transport: Option<T>,
    /// 本端稳定身份（u64：Steam=SteamID；局域网=客户端随机/调用方指定）。
    identity: u64,
    pub my_index: u8,
    pub players: u8,
}

impl<T: Transport> ClientHandshake<T> {
    /// 用 0 身份（未指定）构造。旧调用方若不想关心身份可用此；但新代码建议用 `connected_with`。
    pub fn connected(transport: T) -> Self {
        ClientHandshake::connected_with(transport, 0)
    }

    /// 带稳定身份构造（Steam 传 SteamID，局域网传调用方生成/指定的 id）。
    pub fn connected_with(transport: T, identity: u64) -> Self {
        ClientHandshake {
            transport: Some(transport),
            identity,
            my_index: 0,
            players: 0,
        }
    }

    /// 发 JOIN（附本端稳定身份）。
    pub fn send_join(&mut self, host: &Peer) -> io::Result<()> {
        let Some(t) = self.transport.as_mut() else {
            return Err(io::Error::other("transport taken"));
        };
        t.send_to(&Packet::Join { identity: self.identity }.encode(), host)?;
        Ok(())
    }

    /// 收 ACK（拿序号+人数+身份回显）。
    pub fn recv_join_ack(&mut self, rcv: &mut [u8]) -> io::Result<bool> {
        let Some(t) = self.transport.as_mut() else {
            return Ok(false);
        };
        match t.recv_from(rcv) {
            Ok(Some((n, _))) => {
                if let Some(Packet::Ack { my_index, players, identity }) = Packet::decode(&rcv[..n]) {
                    self.my_index = my_index;
                    self.players = players;
                    self.identity = identity;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Ok(None) => Ok(false),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// 释放 transport，交由上层（ClientLockstep）使用。
    pub fn into_transport(mut self) -> T {
        self.transport.take().expect("transport already taken")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::StdUdpTransport;
    use std::time::Duration;

    /// 多个 client 疯狂重发 JOIN 时，host 必须按来源 Peer 去重：每个 Peer 只分配一次序号并各回 ACK，
    /// 且能收齐 expected 个【不同】client（不会因重复 JOIN 撑爆 joined 或漏发 ACK）。
    #[test]
    fn poll_join_dedups_peers_and_acks_all_clients() {
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut host = HostHandshake::new(ht, 4, true); // host=0 + 3 client
        let host_peer = Peer::Udp(host_addr);

        // 3 个 client，各自独立 transport + handshake。
        let mut clients: Vec<ClientHandshake<StdUdpTransport>> = Vec::new();
        for _ in 0..3 {
            let (t, _) = StdUdpTransport::bind_loopback().unwrap();
            clients.push(ClientHandshake::connected(t));
        }
        let mut rcv = [0u8; 4096];

        // 循环：所有 client 反复发 JOIN，host poll_join，每 client 收 ACK，直到全部就绪。
        let deadline = 300;
        for _ in 0..deadline {
            for c in clients.iter_mut() {
                let _ = c.send_join(&host_peer);
            }
            host.poll_join(&mut rcv);
            let all_acked = clients.iter_mut().all(|c| c.recv_join_ack(&mut rcv).unwrap_or(false));
            if all_acked && host.joined >= host.expected() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }

        // 断言：host 收齐 3 个不同 client，joined==expected。
        assert_eq!(host.joined, host.expected(), "应只计入 3 个不同 client（重复 JOIN 不该撑爆）");
        // 每个 client 应有不重复的正确序号。
        let mut idxs: Vec<u8> = clients.iter().map(|c| c.my_index).collect();
        idxs.sort();
        let mut uniq: Vec<u8> = idxs.clone();
        uniq.dedup();
        assert_eq!(uniq.len(), 3, "三个 client 序号应互不相同");
        assert_eq!(idxs, vec![1, 2, 3], "host 参与时 client 序号应为 1,2,3");
        for c in clients.iter() {
            assert_eq!(c.players, 4, "client 应知道总人数 4");
        }
    }

    /// Steam-向前：稳定身份按 token 去重。同一身份重连/重复 JOIN → 复用它原有槽位（回 ACK 同序号），
    /// 不同身份 → 各得不同槽位。这保证 Steam 下以 SteamID 作身份、掉线重连不会占新槽。
    #[test]
    fn join_dedups_by_stable_identity() {
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut host = HostHandshake::new(ht, 4, true); // host=0 + 3 client
        let host_peer = Peer::Udp(host_addr);

        // 两个不同身份 + 第三个与第一个同身份（模拟重连）。
        let mut c1 = ClientHandshake::connected_with(StdUdpTransport::bind_loopback().unwrap().0, 70001);
        let mut c2 = ClientHandshake::connected_with(StdUdpTransport::bind_loopback().unwrap().0, 70002);
        // 重连者：独立 transport，但身份与 c1 相同。
        let mut c1r = ClientHandshake::connected_with(StdUdpTransport::bind_loopback().unwrap().0, 70001);
        let mut rcv = [0u8; 4096];

        // 三个都 JOIN → host 应收齐 2 个不同身份（c1/c1r 同身份算一个），c1 与 c1r 拿到相同序号。
        for hand in [&mut c1, &mut c2, &mut c1r] {
            let _ = hand.send_join(&host_peer);
        }
        host.poll_join(&mut rcv);
        let mut ok_all = true;
        for hand in [&mut c1, &mut c2, &mut c1r] {
            ok_all = ok_all && hand.recv_join_ack(&mut rcv).unwrap_or(false);
        }
        assert!(ok_all, "都应收到 ACK");
        assert_eq!(host.joined, host.expected() - 1, "同身份占一个槽，应收齐 2 个不同身份");
        assert_eq!(c1.my_index, c1r.my_index, "同身份重连应复用相同槽位");
        assert_ne!(c1.my_index, c2.my_index, "不同身份应分到不同槽位");
        assert_eq!(host.identity_of(c1.my_index), Some(70001), "host 应记住该槽的身份");
        assert_eq!(host.identity_of(c2.my_index), Some(70002));
    }
}
