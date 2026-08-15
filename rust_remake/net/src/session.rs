//! 帧同步会话：建连 + 每帧合帧的 host/client 两端封装。
//!
//! 这是 `frame.rs`（帧封装）之上的会话层。
//! 帧同步正确性的三个关键约束都落在本层：
//!   1) host 必须【收齐全部 N 端输入】才推帧广播（不许用残缺帧）——否则各端收到不同内容的帧。
//!   2) client 必须以【收到带 seq 的帧】为推进锚点，没收到就不推（不许靠本机 clock 盲推）。
//!   3) 加 READY/GO 统一起始：client 加入后先报 READY，host 收齐后广播 GO（带起始 seq），
//!      所有端从同一起点、按同一序列推进，消除“不同窗口加载完成时间不同”导致的漂移。
//!
//! 包类型（首字节 tag）：
//! - `TAG_JOIN=1`   client→host 空串，申请加入。
//! - `TAG_ACK=2`    host→client 我的序号 + 总人数。
//! - `TAG_INPUT=3`  client→host 本机输入（= `frame::up_packet` 体）。
//! - `TAG_FRAME=4`  host→client 整帧（= `frame::frame_packet` 体，含 seq + 所有玩家输入）。
//! - `TAG_READY=5`  client→host 空串，表示“我已加入并准备好”。
//! - `TAG_GO=6`     host→client `[start_seq: u64]`，命令各端从该 seq 开始推进。
#![allow(clippy::type_complexity)] // 网络二进制签名的复杂元组类型：属协议固有，允许。

use crate::frame::{frame_packet, parse_frame, up_packet};
use crate::transport::{Peer, Transport};
use std::io;

pub const TAG_JOIN: u8 = 1;
pub const TAG_ACK: u8 = 2;
pub const TAG_INPUT: u8 = 3;
pub const TAG_FRAME: u8 = 4;
pub const TAG_READY: u8 = 5;
pub const TAG_GO: u8 = 6;

/// 最大支持玩家数（与 PLAN 对齐：8 人上限）。
pub const MAX_PLAYERS: u8 = 8;

/// 一帧内各玩家的 `(玩家序号, 输入字节)`（已拷贝）。
pub type FrameData = Vec<(u8, Vec<u8>)>;

/// host 端会话：等待 N 个 client 加入 + READY；每帧收齐输入后带 seq 合帧广播。
/// host 自身也可作为一个玩家（玩家序号 0），通过 `set_local_input` 提供本机输入。
pub struct HostSession<T: Transport> {
    transport: T,
    expected: usize,
    acked: Vec<Option<Peer>>, // 下标=玩家序号（0 若是 host 自身则不含 peer）
    pub joined: usize,
    /// host 自身的玩家输入（player 0，已编码）；`None` 表示 host 不参与对局。
    local: Option<Vec<u8>>,
    /// host 参与对局时为 1（自身占 player 0），否则为 0；client 序号从该基址分配。
    local_base: u8,
    /// 下一帧序号。
    next_seq: u64,
    /// 各 client 是否已上报 READY（下标与 acked 对齐）。
    ready: Vec<bool>,
}

impl<T: Transport> HostSession<T> {
    pub fn new(transport: T, expected_players: usize) -> Self {
        HostSession {
            transport,
            expected: expected_players,
            acked: vec![None; expected_players],
            joined: 0,
            local: None,
            local_base: 0,
            next_seq: 0,
            ready: vec![false; expected_players],
        }
    }

    /// host 也作为 player 0 参与对局（并把总玩家人数传入以正确配号）。
    pub fn host_participates(&mut self, total_players: u8) {
        self.local_base = 1; // host=player0
        self.expected = (total_players as usize).saturating_sub(1);
        if self.acked.len() < total_players as usize {
            self.acked.resize(total_players as usize, None);
            self.ready.resize(total_players as usize, false);
        }
    }

    /// 设置 host 自身的玩家 0 输入（已编码）。`None` 表示本帧不提供。
    pub fn set_local_input(&mut self, enc: Option<Vec<u8>>) {
        self.local = enc;
    }

    /// 建连阶段：轮询一次，收 join 并给 client 分配/确认序号。
    /// 返回是否已收齐所有 client。
    pub fn poll_join(&mut self, rcv: &mut [u8]) -> bool {
        while self.joined < self.expected {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, from))) if n >= 1 && rcv[0] == TAG_JOIN => {
                    let idx = self.local_base + self.joined as u8; // 若 host=player0，client 从 1 起
                    if (idx as usize) < self.acked.len() {
                        self.acked[idx as usize] = Some(from);
                    }
                    self.joined += 1;
                    // 发 ack：给该 client 序号 + 总人数
                    let total = self.local_base + self.expected as u8;
                    let mut ack = Vec::with_capacity(4);
                    ack.push(TAG_ACK);
                    ack.push(total);
                    ack.push(idx);
                    let _ = self.transport.send_to(&ack, &from);
                }
                Ok(_) => {} // 忽略非 join 包
                Err(_) => break,
            }
        }
        self.joined >= self.expected
    }

    /// READY 阶段：收任意 client 的 READY（TAG_READY），刷新其 ready 标记。
    /// 通过 `from` 地址在 acked 里反查是对应哪个 client 序号。
    pub fn poll_ready(&mut self, rcv: &mut [u8]) {
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, from))) if n >= 1 && rcv[0] == TAG_READY => {
                    if let Some(i) = self.acked.iter().position(|p| *p == Some(from)) {
                        if i < self.ready.len() {
                            self.ready[i] = true;
                        }
                    }
                }
                Ok(None) => break,
                _ => break,
            }
        }
    }

    /// 是否所有 client 都已 READY（即可以统一起始）。
    pub fn all_ready(&self) -> bool {
        // 序号 local_base..local_base+expected 都是 client；host 自身不参与 ready 门槛。
        self.ready.iter().enumerate().all(|(i, r)| i < self.local_base as usize || *r)
    }

    /// 广播 GO 给所有 client：`[TAG_GO][当前 next_seq]`，命令各端从该 seq 开始推进。
    /// 返回 GO 携带的起始 seq。
    pub fn broadcast_go(&mut self) -> u64 {
        let mut pkt = Vec::with_capacity(9);
        pkt.push(TAG_GO);
        pkt.extend_from_slice(&self.next_seq.to_be_bytes());
        for peer in self.acked.iter().flatten() {
            let _ = self.transport.send_to(&pkt, peer);
        }
        self.next_seq
    }

    /// 每帧：收集各 client 上行输入 + host 自身输入。
    /// 【收齐门槛】只有收齐全部 N-1(host 参与时) 或 expected 个 client 输入才返回帧；
    /// 未收齐返回 `None`（调用方不应推帧/广播）。收齐时自动递增 next_seq 并返回 `(seq, frame)`。
    pub fn collect_inputs(&mut self, rcv: &mut [u8]) -> Option<(u64, Vec<(u8, Vec<u8>)>)> {
        let mut latest: std::collections::HashMap<u8, Vec<u8>> = std::collections::HashMap::new();
        let mut seen_clients: Vec<bool> = vec![false; self.expected];
        let mut client_count = 0usize;
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, _))) if n >= 1 && rcv[0] == TAG_INPUT => {
                    if let Some((idx, body)) = crate::frame::parse_up(&rcv[1..n]) {
                        // 只统计 client 序号（local_base..local_base+expected），并对同 client 去重（保留最新）。
                        let c = idx as usize;
                        if c >= self.local_base as usize
                            && c < self.local_base as usize + self.expected
                            && !seen_clients[c - self.local_base as usize]
                        {
                            seen_clients[c - self.local_base as usize] = true;
                            client_count += 1;
                        }
                        latest.insert(idx, body.to_vec());
                    }
                }
                Ok(None) => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        // 收齐门槛：本帧必须覆盖全部 client（含 host 自身，若参与）。
        if client_count != self.expected {
            return None;
        }
        let mut out: Vec<(u8, Vec<u8>)> = latest.into_iter().collect();
        // host 自身作为 player 0（若有本地输入）。
        if self.local_base > 0 {
            if let Some(enc) = self.local.clone() {
                out.push((0, enc));
            }
        }
        out.sort_by_key(|(idx, _)| *idx);
        let seq = self.next_seq;
        self.next_seq += 1;
        Some((seq, out))
    }

    /// 每帧：把已收齐的整帧（带 seq）广播给所有已加入 client（跳过 None 槽）。
    pub fn broadcast_frame(&mut self, seq: u64, entries: &[(u8, Vec<u8>)]) {
        let refs: Vec<(u8, &[u8])> = entries.iter().map(|(i, b)| (*i, b.as_slice())).collect();
        let body = frame_packet(seq, &refs); // [seq][count][entries...]
        let mut pkt = Vec::with_capacity(1 + body.len());
        pkt.push(TAG_FRAME);
        pkt.extend_from_slice(&body);
        for peer in self.acked.iter().flatten() {
            let _ = self.transport.send_to(&pkt, peer);
        }
    }

    pub fn expected(&self) -> usize {
        self.expected
    }
}

/// client 端会话：连入 host，每帧发本机输入、收整帧。
pub struct ClientSession<T: Transport> {
    transport: T,
    pub my_index: u8,
    pub players: u8,
}

impl<T: Transport> ClientSession<T> {
    /// 由已建连（已拿到序号/人数）的 transport 构造一个 client 会话。
    pub fn connected(transport: T, my_index: u8, players: u8) -> ClientSession<T> {
        ClientSession {
            transport,
            my_index,
            players,
        }
    }

    /// 握手用：向 host 发一条 JOIN 申请包。
    pub fn send_join(&mut self, host: &Peer) -> io::Result<()> {
        self.transport.send_to(&[TAG_JOIN], host)?;
        Ok(())
    }

    /// 握手用：尝试读一条 ACK；若收到则填好 my_index/players 并返回 true。
    /// 注意需要 `transport` 可变，故将接收逻辑放在 `recv_join_ack`。
    pub fn recv_join_ack(&mut self, rcv: &mut [u8]) -> io::Result<bool> {
        match self.transport.recv_from(rcv) {
            Ok(Some((n, _))) if n >= 3 && rcv[0] == TAG_ACK => {
                self.players = rcv[1];
                self.my_index = rcv[2];
                Ok(true)
            }
            Ok(_) => Ok(false),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// 每帧：把本机输入（已编码）发给 host。
    pub fn send_input(&mut self, encoded: &[u8], host: &Peer) -> io::Result<()> {
        let body = up_packet(self.my_index, encoded); // [index][payload]（纯函数返回值）
        let mut up = Vec::with_capacity(1 + body.len());
        up.push(TAG_INPUT);
        up.extend_from_slice(&body);
        self.transport.send_to(&up, host)?;
        Ok(())
    }

    /// 准备阶段：向 host 发 READY，表示已加入并准备好开始。
    pub fn send_ready(&mut self, host: &Peer) -> io::Result<()> {
        self.transport.send_to(&[TAG_READY], host)?;
        Ok(())
    }

    /// 尝试收 GO：返回 host 给的下一个预扣 seq。当前没有 GO 则返回 None。
    /// 收到 GO 后，client 应以该 seq 为推进起点。
    pub fn recv_go(&mut self, rcv: &mut [u8]) -> io::Result<Option<u64>> {
        match self.transport.recv_from(rcv) {
            Ok(Some((n, _))) if n >= 9 && rcv[0] == TAG_GO => {
                let mut s = [0u8; 8];
                s.copy_from_slice(&rcv[1..9]);
                Ok(Some(u64::from_be_bytes(s)))
            }
            Ok(_) => Ok(None),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 每帧：尝试收一帧（TAG_FRAME），返回 `(使帧的 seq, 各 (玩家序号, 输入字节))`；
    /// 当前无包返回 None。seq 供 client 锚定推进/去重/缓冲。
    pub fn recv_frame(&mut self, rcv: &mut [u8]) -> io::Result<Option<(u64, FrameData)>> {
        match self.transport.recv_from(rcv) {
            Ok(Some((n, _))) if n >= 1 && rcv[0] == TAG_FRAME => {
                let (seq, entries) = parse_frame(&rcv[1..n])?;
                let owned: FrameData = entries.into_iter().map(|(i, b)| (i, b.to_vec())).collect();
                Ok(Some((seq, owned)))
            }
            Ok(None) => Ok(None),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
}
