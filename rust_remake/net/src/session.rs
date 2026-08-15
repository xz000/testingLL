//! 帧同步会话：建连 + 每帧合帧的 host/client 两端封装。
//!
//! 这是 `frame.rs`（帧封装）之上的会话层，把"建连握手、host 收输入合帧广播、client 收发"
//! 封装成可直接被 client/host 复用的结构。传输仍由 `Transport` 抽象，将来可换 Steam。
//!
//! 包类型（首字节 tag）：
//! - `TAG_JOIN=1`   client→host 空串，申请加入。
//! - `TAG_ACK=2`    host→client 我的序号 + 总人数。
//! - `TAG_INPUT=3`  client→host 本机输入（= `frame::up_packet` 体）。
//! - `TAG_FRAME=4`  host→client 整帧（= `frame::frame_packet` 体，含所有玩家输入）。

use crate::frame::{frame_packet, parse_frame, up_packet};
use crate::transport::{Peer, Transport};
use std::io;

pub const TAG_JOIN: u8 = 1;
pub const TAG_ACK: u8 = 2;
pub const TAG_INPUT: u8 = 3;
pub const TAG_FRAME: u8 = 4;

/// 最大支持玩家数（与 PLAN 对齐：8 人上限）。
pub const MAX_PLAYERS: u8 = 8;

/// 一帧内各玩家的 `(玩家序号, 输入字节)`（已拷贝）。
pub type FrameData = Vec<(u8, Vec<u8>)>;

/// host 端会话：等待 N 个 client 加入；每帧收齐输入后合帧广播。
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
        }
    }

    /// host 也作为 player 0 参与对局（并把总玩家人数传入以正确配号）。
    pub fn host_participates(&mut self, total_players: u8) {
        self.local_base = 1; // host=player0
        self.expected = (total_players as usize).saturating_sub(1);
        if self.acked.len() < total_players as usize {
            self.acked.resize(total_players as usize, None);
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

    /// 每帧：收集各 client 上行输入（TAG_INPUT）+ host 自身（player 0）输入，
    /// 返回 `(玩家序号, 输入字节)` 列表。轮询直到当前无包。
    pub fn collect_inputs(&mut self, rcv: &mut [u8]) -> Vec<(u8, Vec<u8>)> {
        let mut out: Vec<(u8, Vec<u8>)> = Vec::new();
        if let Some(enc) = self.local.take() {
            out.push((0, enc)); // host 自身作为 player 0
        }
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, _))) if n >= 1 && rcv[0] == TAG_INPUT => {
                    if let Some((idx, body)) = crate::frame::parse_up(&rcv[1..n]) {
                        out.push((idx, body.to_vec()));
                    }
                }
                Ok(None) => break, // 本轮无更多包
                Ok(_) => {}         // 非输入包，忽略
                Err(_) => break,
            }
        }
        out.sort_by_key(|(idx, _)| *idx);
        out
    }

    /// 每帧：把整帧广播给所有已加入 client（跳过 None 槽）。
    pub fn broadcast_frame(&mut self, entries: &[(u8, Vec<u8>)]) {
        let refs: Vec<(u8, &[u8])> = entries.iter().map(|(i, b)| (*i, b.as_slice())).collect();
        let mut body = Vec::with_capacity(2048);
        frame_packet(&refs, &mut body); // [count][entries...]
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
        let mut body = Vec::with_capacity(64 + encoded.len());
        up_packet(self.my_index, encoded, &mut body); // [index][payload]
        let mut up = Vec::with_capacity(1 + body.len());
        up.push(TAG_INPUT);
        up.extend_from_slice(&body);
        self.transport.send_to(&up, host)?;
        Ok(())
    }

    /// 每帧：尝试收一帧（TAG_FRAME），返回各 `(玩家序号, 输入字节)`（已拷贝，不依赖 rcv）；
    /// 当前无包返回 None。
    pub fn recv_frame(&mut self, rcv: &mut [u8]) -> io::Result<Option<FrameData>> {
        match self.transport.recv_from(rcv) {
            Ok(Some((n, _))) if n >= 1 && rcv[0] == TAG_FRAME => {
                let entries = parse_frame(&rcv[1..n])?;
                let owned = entries.into_iter().map(|(i, b)| (i, b.to_vec())).collect();
                Ok(Some(owned))
            }
            Ok(None) => Ok(None),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
}
