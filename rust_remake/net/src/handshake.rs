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
    /// 各 player 是否已 READY。
    ready: Vec<bool>,
    pub joined: usize,
    pub go_sent: bool,
}

impl<T: Transport> HostHandshake<T> {
    pub fn new(transport: T, total_players: usize, host_participates: bool) -> Self {
        let local_base = if host_participates { 1 } else { 0 };
        HostHandshake {
            transport: Some(transport),
            total: total_players,
            local_base,
            peers: vec![None; total_players],
            ready: vec![false; total_players],
            joined: 0,
            go_sent: false,
        }
    }

    /// 期望加入的 client 数。
    pub fn expected(&self) -> usize {
        self.total - self.local_base as usize
    }

    /// 收 JOIN → 分配序号 → 回 ACK。返回是否已收齐所有 client。
    pub fn poll_join(&mut self, rcv: &mut [u8]) -> bool {
        let expected = self.expected();
        let (local_base, total) = (self.local_base, self.total);
        let Some(transport) = self.transport.as_mut() else {
            return false;
        };
        while self.joined < expected {
            match transport.recv_from(rcv) {
                Ok(Some((n, from))) => {
                    if let Some(Packet::Join) = Packet::decode(&rcv[..n]) {
                        let idx = local_base + self.joined as u8; // 分配序号
                        if (idx as usize) < self.peers.len() {
                            self.peers[idx as usize] = Some(from);
                        }
                        self.joined += 1;
                        let ack = Packet::Ack { my_index: idx, players: total as u8 };
                        let _ = transport.send_to(&ack.encode(), &from);
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        self.joined >= expected
    }

    /// 收 READY，标记对应 client 已就绪。
    pub fn poll_ready(&mut self, rcv: &mut [u8]) {
        let Some(transport) = self.transport.as_mut() else {
            return;
        };
        loop {
            match transport.recv_from(rcv) {
                Ok(Some((n, from))) => {
                    if let Some(Packet::Ready) = Packet::decode(&rcv[..n]) {
                        if let Some(i) = self.peers.iter().position(|p| *p == Some(from)) {
                            if i < self.ready.len() {
                                self.ready[i] = true;
                            }
                        }
                    }
                }
                Ok(None) => break,
                _ => break,
            }
        }
    }

    /// 是否所有 client 都已 READY。
    pub fn all_ready(&self) -> bool {
        let start = self.local_base as usize;
        (start..self.total).all(|i| self.ready[i])
    }

    /// 广播 GO（带起始 seq=0）给所有 client。
    pub fn broadcast_go(&mut self) -> u64 {
        let Some(transport) = self.transport.as_mut() else {
            return 0;
        };
        let pkt = Packet::Go { start_seq: 0 };
        let enc = pkt.encode();
        let peers: Vec<Peer> = self.peers.iter().flatten().copied().collect();
        for p in peers {
            let _ = transport.send_to(&enc, &p);
        }
        self.go_sent = true;
        0
    }

    /// 释放 transport，交由上层（HostLockstep）使用。
    pub fn into_transport(mut self) -> T {
        self.transport.take().expect("transport already taken")
    }

    /// 取某 player 的 peer（供补发/广播由 lockstep 内部记录，这里供可选用）。
    pub fn peer_of(&self, player_index: u8) -> Option<Peer> {
        self.peers.get(player_index as usize).copied().flatten()
    }
}

/// client 侧建连/统一握手。
pub struct ClientHandshake<T: Transport> {
    transport: Option<T>,
    pub my_index: u8,
    pub players: u8,
}

impl<T: Transport> ClientHandshake<T> {
    pub fn connected(transport: T) -> Self {
        ClientHandshake {
            transport: Some(transport),
            my_index: 0,
            players: 0,
        }
    }

    /// 发 JOIN。
    pub fn send_join(&mut self, host: &Peer) -> io::Result<()> {
        let Some(t) = self.transport.as_mut() else {
            return Err(io::Error::other("transport taken"));
        };
        t.send_to(&Packet::Join.encode(), host)?;
        Ok(())
    }

    /// 收 ACK（拿序号+人数）。
    pub fn recv_join_ack(&mut self, rcv: &mut [u8]) -> io::Result<bool> {
        let Some(t) = self.transport.as_mut() else {
            return Ok(false);
        };
        match t.recv_from(rcv) {
            Ok(Some((n, _))) => {
                if let Some(Packet::Ack { my_index, players }) = Packet::decode(&rcv[..n]) {
                    self.my_index = my_index;
                    self.players = players;
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

    /// 发 READY。
    pub fn send_ready(&mut self, host: &Peer) -> io::Result<()> {
        let Some(t) = self.transport.as_mut() else {
            return Err(io::Error::other("transport taken"));
        };
        t.send_to(&Packet::Ready.encode(), host)?;
        Ok(())
    }

    /// 收 GO（拿起始 seq）。未到返回 None。
    pub fn recv_go(&mut self, rcv: &mut [u8]) -> io::Result<Option<u64>> {
        let Some(t) = self.transport.as_mut() else {
            return Ok(None);
        };
        match t.recv_from(rcv) {
            Ok(Some((n, _))) => {
                if let Some(Packet::Go { start_seq }) = Packet::decode(&rcv[..n]) {
                    Ok(Some(start_seq))
                } else {
                    Ok(None)
                }
            }
            Ok(None) => Ok(None),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 释放 transport，交由上层（ClientLockstep）使用。
    pub fn into_transport(mut self) -> T {
        self.transport.take().expect("transport already taken")
    }
}
