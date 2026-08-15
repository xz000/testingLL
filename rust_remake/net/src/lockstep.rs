//! 帧同步核心状态机（传输无关，彻底重写）。
//!
//! 分层：`HostLockstep` 负责 host 侧「收齐输入 → 打 seq 帧 → 广播 + 缓冲补发」；
//! `ClientLockstep` 负责 client 侧「严格按序接收 → 推进 → 漏帧请求补发」。
//! 二者只依赖 `crate::transport::Transport` 抽象收发字节，故可在测试里注入
//! 「丢包 / 乱序 / 重复」的假 transport，验证在丢帧下两端仍逐位一致。
//!
//! 正确性要点：
//! - host 必须等齐全部 client 输入才产生第 `seq` 帧（缺失时 `try_emit` 返回 None，不推残缺帧）。
//! - client 严格按 `expect_seq` 顺序推进；收到 `seq > expect_seq`（漏帧）时向 host 发 `ReqFrame` 补发，
//!   补齐前不推进——杜绝「跳 seq」导致的永久分叉。
//! - host 保留最近 K 帧（`frame_buf`），收到 `ReqFrame` 时补发。

use crate::proto::{FrameData, Packet};
use crate::transport::{Peer, Transport};
use std::collections::VecDeque;
use std::io;

/// host 侧帧同步状态机。
pub struct HostLockstep<T: Transport> {
    transport: T,
    /// 0 = host 不参与对局；1 = host 自身占 player 0。client 序号从 `local_base` 起。
    local_base: u8,
    /// 需要收齐的 client 数。
    expected: usize,
    /// 各 client peer（下标=client 序号 - local_base）。
    client_peers: Vec<Option<Peer>>,
    /// 各 client 最新输入（下标=client 序号 - local_base）。
    latest_input: Vec<Option<Vec<u8>>>,
    /// host 自身本地输入（参与时）。
    local: Option<Vec<u8>>,
    /// 下一帧 seq。
    next_seq: u64,
    /// 最近若干帧（含全部玩家输入），供补发。
    frame_buf: VecDeque<(u64, FrameData)>,
    /// frame_buf 保留的帧数。
    pub frame_buf_capacity: usize,
}

impl<T: Transport> HostLockstep<T> {
    /// `total_players` 含 host 自身（host 参与时）；host 不参与时 total 即 client 数。
    pub fn new(transport: T, total_players: usize, host_participates: bool) -> Self {
        let local_base = if host_participates { 1 } else { 0 };
        let expected = total_players.saturating_sub(local_base as usize);
        HostLockstep {
            transport,
            local_base,
            expected,
            client_peers: vec![None; expected],
            latest_input: vec![None; expected],
            local: None,
            next_seq: 0,
            frame_buf: VecDeque::new(),
            frame_buf_capacity: 60,
        }
    }

    /// 交给 host 自身的本地输入（参与对局时）。`None` 表示本 tick 不提供。
    pub fn set_local_input(&mut self, enc: Option<Vec<u8>>) {
        self.local = enc;
    }

    /// host 是否已见过所有 client 的输入至少一次。
    pub fn saw_all_clients(&self) -> bool {
        self.latest_input.iter().all(|x| x.is_some())
    }

    /// 处理 transport 中当前所有包（INPUT / REQ_FRAME）。无副作用推进，只在收 REQ_FRAME 时补发。
    pub fn poll(&mut self, rcv: &mut [u8]) {
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, from))) => {
                    if let Some(pkt) = Packet::decode(&rcv[..n]) {
                        match pkt {
                            Packet::Input { index, bytes } => {
                                let c = index as usize - self.local_base as usize;
                                if c < self.expected {
                                    if self.latest_input[c].is_none() {
                                        // 首次见到该 client → 记住 peer，用于广播/补发。
                                        self.client_peers[c] = Some(from);
                                    }
                                    self.latest_input[c] = Some(bytes);
                                }
                            }
                            Packet::ReqFrame { seq } => {
                                // 补发缺失帧。
                                if let Some((_, entries)) = self.frame_buf.iter().find(|(s, _)| *s == seq) {
                                    let pkt = Packet::Frame { seq, entries: entries.clone() };
                                    let _ = self.transport.send_to(&pkt.encode(), &from);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    }

    /// 若已收齐全部 client（及 host 自身）输入，则合成一帧：入缓冲、广播，清空已用输入，
    /// 返回 `Some((seq, entries))`（供各端包括 host 自身喂给本地 World）；未收齐返回 `None`。
    pub fn try_emit(&mut self) -> Option<(u64, crate::proto::FrameData)> {
        // 若 host 参与，总玩家数 = expected + 1；需 host 本地输入 + 全部 client。
        if !self.latest_input.iter().all(|x| x.is_some()) {
            return None;
        }
        if self.local_base > 0 && self.local.is_none() {
            return None;
        }
        let mut entries: FrameData = Vec::new();
        // host local = player 0。
        if self.local_base > 0 {
            entries.push((0, self.local.clone().unwrap()));
        }
        for (c, inp) in self.latest_input.iter().enumerate() {
            if let Some(bytes) = inp {
                entries.push(((c + self.local_base as usize) as u8, bytes.clone()));
            }
        }
        entries.sort_by_key(|(i, _)| *i);
        let seq = self.next_seq;
        self.next_seq += 1;
        // 广播给所有已知 client。
        let pkt = Packet::Frame { seq, entries: entries.clone() };
        let enc = pkt.encode();
        for peer in self.client_peers.iter().flatten() {
            let _ = self.transport.send_to(&enc, peer);
        }
        // 入缓冲（供补发）。
        self.frame_buf.push_back((seq, entries.clone()));
        while self.frame_buf.len() > self.frame_buf_capacity {
            self.frame_buf.pop_front();
        }
        // 清空本帧已用输入，等待下一帧。
        self.latest_input.iter_mut().for_each(|x| *x = None);
        if self.local_base > 0 {
            self.local = None;
        }
        Some((seq, entries))
    }

    pub fn client_peer(&self, client_seq: u8) -> Option<Peer> {
        let c = client_seq as usize - self.local_base as usize;
        if c < self.expected {
            self.client_peers[c]
        } else {
            None
        }
    }

    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }
}

/// client 侧帧同步状态机。
pub struct ClientLockstep<T: Transport> {
    transport: T,
    /// 本机玩家序号（session 握手后分配）。
    my_index: u8,
    /// 下一帧应推进的 seq。
    expect_seq: u64,
    /// 已收、等按序推进的帧。
    pending: VecDeque<(u64, FrameData)>,
    /// host peer。
    host: Peer,
}

impl<T: Transport> ClientLockstep<T> {
    pub fn new(transport: T, my_index: u8, host: Peer) -> Self {
        ClientLockstep {
            transport,
            my_index,
            expect_seq: 0,
            pending: VecDeque::new(),
            host,
        }
    }

    /// 收到 GO 后设置起点 seq。
    pub fn set_start_seq(&mut self, seq: u64) {
        self.expect_seq = seq;
    }

    /// 把本机输入上行给 host。
    pub fn send_input(&mut self, encoded: &[u8]) -> io::Result<()> {
        let pkt = Packet::Input { index: self.my_index, bytes: encoded.to_vec() };
        self.transport.send_to(&pkt.encode(), &self.host)?;
        Ok(())
    }

    /// 请求补发 `missing_seq`。
    pub fn request_frame(&mut self, missing_seq: u64) -> io::Result<()> {
        let pkt = Packet::ReqFrame { seq: missing_seq };
        self.transport.send_to(&pkt.encode(), &self.host)?;
        Ok(())
    }

    /// 从 transport 收一个 FRAME：入 pending 并尝试消费连续帧。
    /// 返回 `Ok(Some(entries))` 表示推进了一帧；`Ok(None)` 表示当前无可用帧（未推进）。
    pub fn step_frame(&mut self, rcv: &mut [u8]) -> io::Result<Option<FrameData>> {
        // 收当前所有 FRAME（有界轮询一次）。
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, _))) => {
                    if let Some(Packet::Frame { seq, entries }) = Packet::decode(&rcv[..n]) {
                        if seq >= self.expect_seq {
                            // 只缓存 >= expect 的帧；丢弃过时帧。
                            let pos = self.pending.iter().position(|(s, _)| *s >= seq).unwrap_or(self.pending.len());
                            self.pending.insert(pos, (seq, entries));
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        // 尝试推进连续帧。
        Ok(self.try_advance())
    }

    fn try_advance(&mut self) -> Option<FrameData> {
        // 若 pending 里的最小 seq == expect → 推进它 + 后续连续帧。
        while !self.pending.is_empty() {
            let min_seq = self.pending[0].0;
            if min_seq == self.expect_seq {
                let (_, entries) = self.pending.pop_front().unwrap();
                let ret = Some(entries);
                // 期望下一帧。
                self.expect_seq += 1;
                // 若缓冲里现在是连续的下一帧，循环继续消费（一帧步进只返回一帧？见下）。
                // 设计：step_frame 每调用推进一帧；但缓冲里的连续帧可由后续 step_frame 继续消费。
                // 这里我们返回最新推进的一帧。为简单，只在 expect==min 时推一帧。
                return ret;
            } else if min_seq < self.expect_seq {
                // 过时帧，丢弃。
                self.pending.pop_front();
            } else {
                // min_seq > expect_seq：有缺口，需补发。
                // 此时不应推进（保证严格按序）。如果缺口缓存里已有 >expect 的帧，说明丢了 expect 那帧。
                let _ = self.request_frame(self.expect_seq);
                return None;
            }
        }
        None
    }

    pub fn expect_seq(&self) -> u64 {
        self.expect_seq
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::net::SocketAddr;
    use std::rc::Rc;

    /// 内存双端 transport：两端各自有 inbox，send 投到对端 inbox，模拟本机 UDP 即时投递。
    /// `drop_seqs` 表示「首次广播该 seq 帧时丢弃」，用于模拟丢包（补发时不再丢）。
    struct FakeTransport {
        inbox: Rc<RefCell<std::collections::VecDeque<Vec<u8>>>>,
        peer_inbox: Rc<RefCell<std::collections::VecDeque<Vec<u8>>>>,
        peer_addr: SocketAddr,
        drop_seqs: Vec<u64>,
    }

    impl Transport for FakeTransport {
        fn send_to(&mut self, buf: &[u8], _peer: &Peer) -> io::Result<usize> {
            if let Some(pkt) = Packet::decode(buf) {
                if let Packet::Frame { seq, .. } = pkt {
                    let drop = self.drop_seqs.iter().any(|s| *s == seq);
                    // 只丢第一次；后续（补发）放行。
                    self.drop_seqs.retain(|s| *s != seq);
                    if drop {
                        return Ok(buf.len());
                    }
                }
            }
            self.peer_inbox.borrow_mut().push_back(buf.to_vec());
            Ok(buf.len())
        }
        fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<Option<(usize, Peer)>> {
            match self.inbox.borrow_mut().pop_front() {
                Some(bytes) if bytes.len() <= buf.len() => {
                    buf[..bytes.len()].copy_from_slice(&bytes);
                    Ok(Some((bytes.len(), Peer::Udp(self.peer_addr))))
                }
                Some(_) => Ok(None),
                None => Ok(None),
            }
        }
        fn local(&self) -> Peer {
            Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        }
    }

    /// 建一对 host(client_seq_base=1) 与 client 的假 transport。
    fn pair() -> (FakeTransport, FakeTransport) {
        let host_inbox = Rc::new(RefCell::new(std::collections::VecDeque::new()));
        let client_inbox = Rc::new(RefCell::new(std::collections::VecDeque::new()));
        let host_peer = SocketAddr::from(([127, 0, 0, 1], 4000));
        let client_peer = SocketAddr::from(([127, 0, 0, 1], 4001));
        let ht = FakeTransport {
            inbox: host_inbox.clone(),
            peer_inbox: client_inbox.clone(),
            peer_addr: client_peer,
            drop_seqs: Vec::new(),
        };
        let ct = FakeTransport {
            inbox: client_inbox.clone(),
            peer_inbox: host_inbox.clone(),
            peer_addr: host_peer,
            drop_seqs: Vec::new(),
        };
        (ht, ct)
    }

    /// host 收齐 client 输入 + 自身输入后应产帧；client 按序推进。
    #[test]
    fn host_emit_and_client_advance_in_order() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true); // host=0 + client1
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 4096];

        for i in 0..5u8 {
            // client 上行 → host 收 → host 产帧 → client 收帧推进。
            cli.send_input(&[i]).unwrap();
            host.poll(&mut rcv);
            host.set_local_input(Some(vec![i + 100]));
            let (seq, _) = host.try_emit().expect("host 应收齐后逐帧产 seq");
            assert_eq!(seq, i as u64, "host 应逐帧产 seq");
            let advanced = cli.step_frame(&mut rcv).unwrap();
            assert!(advanced.is_some(), "client 应收帧推进");
        }
        assert_eq!(cli.expect_seq(), 5, "client 应已按序推进 5 帧");
    }

    /// 丢帧自愈：host 首次广播 seq=1 时被丢，client 应收齐逐帧推进（请求补发）。
    #[test]
    fn client_recovers_missing_frame_via_request() {
        let (mut ht, ct) = pair();
        ht.drop_seqs = vec![1]; // host 首次广播 seq1 丢包
        let mut host = HostLockstep::new(ht, 2, true);
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 4096];

        // 驱动多轮，让 client 请求缺失帧、host 补发，直到 client 追平 5 帧。
        for round in 0..40 {
            // 每轮：client 发输入 → host 收 → host 产帧 → client 尽量消费缓冲。
            cli.send_input(&[round as u8]).unwrap();
            host.poll(&mut rcv);
            host.set_local_input(Some(vec![round as u8 + 100]));
            let _ = host.try_emit();
            // client 反复 step，直到本次缓冲耗尽（step_frame 返回 None 表示本轮无新推进）。
            while cli.step_frame(&mut rcv).unwrap().is_some() {
                // host 需要处理 client 可能发出的 REQ_FRAME。
                // 注意：这里 client 推进时可能触发 request；下一轮 host.poll 会收到并补发。
            }
            if cli.expect_seq() >= 5 {
                break;
            }
        }
        assert!(cli.expect_seq() >= 5, "丢帧后 client 应依靠请求补发追平（实际推进 seq {}", cli.expect_seq());
        assert_eq!(cli.pending_len(), 0, "client 落点不应有未消费的乱序帧");
    }
}

