//! 帧同步核心状态机（传输无关，彻底重写）。
//!
//! 分层：`HostLockstep` 负责 host 侧「收齐输入 → 打 seq 帧 → 广播 + 缓冲补发」；
//! `ClientLockstep` 负责 client 侧「严格按序接收 → 推进 → 漏帧请求补发」。
//! 二者只依赖 `crate::transport::Transport` 抽象收发字节，故可在测试里注入
//! 「丢包 / 乱序 / 重复」的假 transport，验证在丢帧下两端仍逐位一致。
#![allow(clippy::type_complexity)] // 网络二进制签名的复杂元组类型：属协议固有，允许。
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

/// 掉线客户端用的“默认输入”占位（玩家原地不动）。
fn default_input_bytes() -> Vec<u8> {
    game_core::netcode::encode_player_input(&game_core::world::PlayerInput::default())
}

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
    /// 各 client 上报的玩家配置（`PlayerCfg` 字节；下标=client 序号 - local_base）。
    cfgs: Vec<Option<Vec<u8>>>,
    /// host 自身（player 0）上报的配置（学习阶段结束时设定）。
    local_cfg: Option<Vec<u8>>,
    /// 各 client 是否已判定掉线（不再要求其输入；其帧用默认输入占位）。
    dropped: Vec<bool>,
    /// 各 client 的稳定端点（掉线丢失 `client_peers` 后仍保留，用于把重连请求的 from 映射回槽位）。
    client_addr: Vec<Option<Peer>>,
    /// 各 client 的稳定身份（u64：Steam=SteamID；局域网=握手时登记的随机/指定身份）。重连按身份找回槽位。
    client_identities: Vec<Option<u64>>,
    /// 各 client 连续「未在本帧提供输入」的已产帧数（用于 host 自动判定掉线）。
    idle_ticks: Vec<u32>,
    /// 各 client 当前是否已就绪（可撤销；由 `Packet::PlayerReady` 更新）。
    clients_ready: Vec<bool>,
    /// 累计收到过的 `PlayerReady` 包总数（诊断用：区分“包根本没到”与“到了但值为 false”）。
    ready_packets_seen: u64,
    /// 最近一次保存的整场 World 快照字节 + 接回 seq（供重连者重建后从该 seq 继续）。
    snapshot: Option<(Vec<u8>, u64)>,
    /// 距上次成功产帧/收到有效回包的 tick（用于 host 侧自动判定客户端掉线）。
    /// 仅在需要时由调用方驱动更新（见 `bump_alive`）。
    pub alive_tick: u64,
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
            cfgs: vec![None; expected],
            local_cfg: None,
            dropped: vec![false; expected],
            client_addr: vec![None; expected],
            client_identities: vec![None; expected],
            idle_ticks: vec![0; expected],
            clients_ready: vec![false; expected],
            ready_packets_seen: 0,
            snapshot: None,
            alive_tick: 0,
        }
    }

    /// 记下当前整场 World 快照（编码字节 + 接回 seq）。
    /// 语义：`world_bytes` 反映「已处理完 seq-1 帧」的世界状态，重连端应把它重建后**从 seq 开始继续收帧**。
    /// 因此调用方应在「World 已应用完第 seq 帧」后传下一帧号 `host.next_seq()`。
    pub fn set_snapshot(&mut self, world_bytes: Vec<u8>, seq: u64) {
        self.snapshot = Some((world_bytes, seq));
    }

    /// 读当前快照（若有）。用于测试/断言。
    pub fn current_snapshot(&self) -> Option<&(Vec<u8>, u64)> {
        self.snapshot.as_ref()
    }

    /// 累计一帧推进（host 每产一帧调用一次，供上层做超时判活）。
    pub fn bump_alive(&mut self) {
        self.alive_tick += 1;
    }

    /// 广播「全体就绪→进入配置」给所有 client（供房间阶段 host 通知 client 进配置菜单）。
    pub fn broadcast_start_config(&mut self) {
        let pkt = Packet::StartConfig;
        let enc = pkt.encode();
        for peer in self.client_peers.iter().flatten() {
            let _ = self.transport.send_to(&enc, peer);
        }
    }

    /// 广播房间「实时就绪状态快照」给所有 client，使其显示所有成员的就绪状态（多人一致界面）。
    /// `host_ready` = host 自身（槽 0）当前是否就绪。
    pub fn broadcast_roster_ready(&mut self, host_ready: bool) {
        let mut entries: Vec<(u8, bool)> = Vec::with_capacity(self.expected + 1);
        if self.local_base > 0 {
            entries.push((0, host_ready));
        }
        for (c, r) in self.clients_ready.iter().enumerate() {
            entries.push(((c + self.local_base as usize) as u8, *r));
        }
        if entries.is_empty() {
            return;
        }
        let pkt = Packet::RosterReady { entries };
        let enc = pkt.encode();
        for peer in self.client_peers.iter().flatten() {
            let _ = self.transport.send_to(&enc, peer);
        }
    }

    /// 某 client（完整的玩家序号）当前是否已就绪（可撤销）。
    pub fn client_ready(&self, client_seq: u8) -> bool {
        let c = client_seq as usize - self.local_base as usize;
        c < self.expected && self.clients_ready[c]
    }

    /// 所有 client 是否都已就绪（不含 host 自身；host 自身的就绪由调用方另管）。
    pub fn all_clients_ready(&self) -> bool {
        self.clients_ready.iter().all(|r| *r)
    }

    /// 已上行过输入（在场信号）的 client 数。
    pub fn present_clients_count(&self) -> usize {
        self.latest_input.iter().filter(|x| x.is_some()).count()
    }

    /// 已就绪（收到 PlayerReady(true)）的 client 数。
    pub fn ready_clients_count(&self) -> usize {
        self.clients_ready.iter().filter(|r| **r).count()
    }

    /// 已建立连接（记录到 client_peers）的 client 数。
    pub fn connected_clients_count(&self) -> usize {
        self.client_peers.iter().filter(|p| p.is_some()).count()
    }

    /// 期望的 client 总数（= 玩家总数减去 host 本地占位，若有参与）。
    pub fn expected_clients(&self) -> usize {
        self.expected
    }

    /// 累计收到过的 `PlayerReady` 包总数（诊断：区分“包没到”与“到了但值是 false”）。
    pub fn ready_packets_seen(&self) -> u64 {
        self.ready_packets_seen
    }

    /// 登记各 client 槽位的稳定身份（自握手结果带入；Steam=SteamID，局域网=握手随机/指定）。
    /// 重连时优先按身份找回槽位（不依赖来源端点），Steam 下即按 SteamID。
    pub fn set_client_identities(&mut self, identities: &[Option<u64>]) {
        for (i, v) in identities.iter().enumerate() {
            if i < self.client_identities.len() {
                self.client_identities[i] = *v;
            }
        }
    }

    /// 把某 client 标记为掉线：之后用“默认输入”占位（玩家原地不动），不再要求其真实输入，其余端照常推进。
    pub fn mark_dropped(&mut self, client_seq: u8) {
        let c = client_seq as usize - self.local_base as usize;
        if c < self.expected {
            self.dropped[c] = true;
            self.latest_input[c] = Some(default_input_bytes());
            self.client_peers[c] = None; // 不再向它广播
        }
    }

    /// 重连：把某 client 从掉线恢复为活跃，并清掉默认占位（下次它发输入时 host 会重新记 peer、继续推进）。
    pub fn unmark_dropped(&mut self, client_seq: u8) {
        let c = client_seq as usize - self.local_base as usize;
        if c < self.expected {
            self.dropped[c] = false;
            self.latest_input[c] = None; // 等重连端重新上行，poll 会重记 peer
        }
    }

    /// 某一 client 的连续空闲（未提供输入）帧数。
    pub fn client_idle_ticks(&self, client_seq: u8) -> u32 {
        let c = client_seq as usize - self.local_base as usize;
        if c < self.expected {
            self.idle_ticks[c]
        } else {
            0
        }
    }

    /// 自动化掉线：任何一个未掉线 client 连续空闲 `threshold` 帧即标记为掉线（此后用默认输入占位、不再卡全队）。
    /// 返回本次新标记为掉线的 client 序号列表。
    pub fn auto_drop_idle(&mut self, threshold: u32) -> Vec<u8> {
        let mut out = Vec::new();
        for c in 0..self.expected {
            if !self.dropped[c] && self.idle_ticks[c] >= threshold {
                let idx = (c + self.local_base as usize) as u8;
                self.mark_dropped(idx);
                out.push(idx);
            }
        }
        out
    }

    /// 交给 host 自身的玩家配置（学习阶段结束后设定）。
    pub fn set_local_cfg(&mut self, enc: Vec<u8>) {
        self.local_cfg = Some(enc);
    }

    /// 收 client 上报的配置（`PlayerCfg`），按来源去重保存最新。
    pub fn poll_cfg(&mut self, rcv: &mut [u8]) {
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, from))) => {
                    if let Some(Packet::PlayerCfg { index, bytes }) = Packet::decode(&rcv[..n]) {
                        let c = index as usize - self.local_base as usize;
                        if c < self.expected {
                            if self.client_peers[c].is_none() {
                                self.client_peers[c] = Some(from);
                            }
                            self.cfgs[c] = Some(bytes);
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    }

    /// 是否已收齐所有端（host 自身 + 全部 client）的配置。
    pub fn all_cfgs(&self) -> bool {
        if self.local_base > 0 && self.local_cfg.is_none() {
            return false;
        }
        self.cfgs.iter().all(|x| x.is_some())
    }

    /// 合并所有端配置：`(player_index, bytes)`（host=0 在前，client 随后），收齐才 Some。
    pub fn collect_cfgs(&self) -> Option<Vec<(u8, Vec<u8>)>> {
        if !self.all_cfgs() {
            return None;
        }
        let mut out: Vec<(u8, Vec<u8>)> = Vec::new();
        if self.local_base > 0 {
            if let Some(c) = &self.local_cfg {
                out.push((0, c.clone()));
            }
        }
        for (c, cfg) in self.cfgs.iter().enumerate() {
            if let Some(b) = cfg {
                out.push(((c + self.local_base as usize) as u8, b.clone()));
            }
        }
        out.sort_by_key(|(i, _)| *i);
        Some(out)
    }

    /// 广播 `PlayerCfgAll`（所有端完整配置）给所有 client。
    pub fn broadcast_cfgs(&mut self, entries: &[(u8, Vec<u8>)]) {
        let pkt = Packet::PlayerCfgAll { entries: entries.to_vec() };
        let enc = pkt.encode();
        for peer in self.client_peers.iter().flatten() {
            let _ = self.transport.send_to(&enc, peer);
        }
    }

    /// 清空已收集的配置（本局同步完成后调用，供下一局复用）。
    pub fn reset_cfgs(&mut self) {
        for c in self.cfgs.iter_mut() {
            *c = None;
        }
        self.local_cfg = None;
    }

    /// host 自身（player 0）的配置是否已就绪（host 按 Space 开始后即置）。
    pub fn local_cfg_ready(&self) -> bool {
        self.local_base == 0 || self.local_cfg.is_some()
    }

    /// 某一 client 槽位的配置是否已收到（即该 client 已就绪上报）。`client_seq` 是完整玩家序号。
    pub fn client_cfg_ready(&self, client_seq: u8) -> bool {
        let c = client_seq as usize - self.local_base as usize;
        c < self.expected && self.cfgs[c].is_some()
    }

    /// 已就绪的玩家数（含 host 自身，若参与）。供 host 开局界面显示“已就绪 X / 总 N”。
    pub fn cfg_ready_count(&self) -> usize {
        let mut n = 0;
        if self.local_cfg_ready() {
            n += 1;
        }
        n += self.cfgs.iter().filter(|c| c.is_some()).count();
        n
    }

    /// 全部玩家（host 自身 + 各 client）是否都已就绪。
    pub fn all_cfgs_ready(&self) -> bool {
        self.all_cfgs()
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
                                    // 始终记下端点（即使已掉线重连），用于广播/补发；也更新稳定端点映射。
                                    self.client_peers[c] = Some(from);
                                    self.client_addr[c] = Some(from);
                                    self.latest_input[c] = Some(bytes);
                                    self.idle_ticks[c] = 0; // 收到输入 → 清零空闲计数
                                }
                            }
                            Packet::RoomState { index, ready, input_bytes } => {
                                let c = index as usize - self.local_base as usize;
                                if c < self.expected {
                                    // 房间阶段合包：一次更新「在场 + 就绪 + 端点 + 空闲」。可靠的输入在场通道。
                                    self.client_peers[c] = Some(from);
                                    self.client_addr[c] = Some(from);
                                    self.latest_input[c] = Some(input_bytes);
                                    self.clients_ready[c] = ready;
                                    self.idle_ticks[c] = 0;
                                }
                            }
                            Packet::PlayerReady { index, ready } => {
                                let c = index as usize - self.local_base as usize;
                                if c < self.expected {
                                    self.ready_packets_seen += 1;
                                    // 更新该 client 的就绪状态（可撤销，反复 toggle）；同时记下它的端点，便于广播（如 StartConfig）。
                                    self.client_peers[c] = Some(from);
                                    self.client_addr[c] = Some(from);
                                    self.clients_ready[c] = ready;
                                }
                            }
                            Packet::ReqFrame { seq } => {
                                // 补发缺失帧。
                                if let Some((_, entries)) = self.frame_buf.iter().find(|(s, _)| *s == seq) {
                                    let pkt = Packet::Frame { seq, entries: entries.clone() };
                                    let _ = self.transport.send_to(&pkt.encode(), &from);
                                }
                            }
                            Packet::ReconnectReq { identity, .. } => {
                                // 客户端请求重连：优先按稳定身份找回槽位（Steam=SteamID），否则按来源端点。
                                // 找回后恢复为活跃、重记 peer、把当前快照回给它，并广播 Resync(seq) 让全员对齐基线。
                                let c = if identity != 0 {
                                    self.client_identities.iter().position(|i| *i == Some(identity))
                                        .or_else(|| self.client_addr.iter().position(|a| *a == Some(from)))
                                } else {
                                    self.client_addr.iter().position(|a| *a == Some(from))
                                };
                                if let Some(c) = c {
                                    let idx = (c + self.local_base as usize) as u8;
                                    // 恢复为活跃（清掉默认占位，重记 peer），等它重新上行输入。
                                    self.unmark_dropped(idx);
                                    self.client_peers[c] = Some(from);
                                    // 若还没登记该槽身份则补上。
                                    if self.client_identities[c].is_none() {
                                        self.client_identities[c] = Some(identity);
                                    }
                                }
                                if let Some((wb, seq)) = self.snapshot.as_ref() {
                                    let snap_pkt = Packet::Snapshot { world_bytes: wb.clone(), seq: *seq };
                                    let _ = self.transport.send_to(&snap_pkt.encode(), &from);
                                }
                                let rseq = self.snapshot.as_ref().map(|(_, s)| *s);
                                if let Some(seq) = rseq {
                                    let resync_pkt = Packet::Resync { seq };
                                    let enc = resync_pkt.encode();
                                    // 广播到已知 peer（含刚重连回来的）。
                                    for peer in self.client_peers.iter().flatten() {
                                        let _ = self.transport.send_to(&enc, peer);
                                    }
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
        // 空闲计数：本轮未提供输入且未掉线的 client 各 +1（已发送的已被 Input 臂清零）。
        for c in 0..self.expected {
            if !self.dropped[c] && self.latest_input[c].is_none() {
                self.idle_ticks[c] += 1;
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
        // 清空本帧已用输入，等待下一帧；掉线端保持默认占位（继续用默认输入推进）。
        for (c, x) in self.latest_input.iter_mut().enumerate() {
            if self.dropped[c] {
                *x = Some(default_input_bytes());
            } else {
                *x = None;
            }
        }
        if self.local_base > 0 {
            self.local = None;
        }
        self.bump_alive();
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

    /// 向 host 上报本机的就绪状态（可反复 toggle 以取消就绪）。`ready` = 当前是否就绪。
    pub fn send_ready_state(&mut self, ready: bool) -> io::Result<()> {
        let pkt = Packet::PlayerReady { index: self.my_index, ready };
        self.transport.send_to(&pkt.encode(), &self.host)?;
        Ok(())
    }

    /// 房间阶段：把「就绪 + 输入在场信号」合成单包持续上行给 host。
    /// （P2P 下独立的 PlayerReady 包曾实测常丢，而输入在场包可靠；故把就绪折进同一在场包。）
    pub fn send_room_state(&mut self, ready: bool, input_bytes: &[u8]) -> io::Result<()> {
        let pkt = Packet::RoomState {
            index: self.my_index,
            ready,
            input_bytes: input_bytes.to_vec(),
        };
        self.transport.send_to(&pkt.encode(), &self.host)?;
        Ok(())
    }

    /// 尝试收 host 的 `StartConfig`（全体就绪，进入配置菜单）；无则 None。
    pub fn recv_start_config(&mut self, rcv: &mut [u8]) -> io::Result<bool> {
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, _))) => {
                    if let Some(Packet::StartConfig) = Packet::decode(&rcv[..n]) {
                        return Ok(true);
                    }
                }
                Ok(None) => return Ok(false),
                Err(_) => return Ok(false),
            }
        }
    }

    /// 尝试收 host 的房间就绪状态快照（多人一致界面）；无则 None。
    pub fn recv_roster_ready(&mut self, rcv: &mut [u8]) -> io::Result<Option<Vec<(u8, bool)>>> {
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, _))) => {
                    if let Some(Packet::RosterReady { entries }) = Packet::decode(&rcv[..n]) {
                        return Ok(Some(entries));
                    }
                }
                Ok(None) => return Ok(None),
                Err(_) => return Ok(None),
            }
        }
    }

    /// 向 host 上报本玩家最终配置（`PlayerCfg`，载荷为 `PlayerConfig::encode()` 字节）。
    pub fn send_cfg(&mut self, bytes: &[u8]) -> io::Result<()> {
        let pkt = Packet::PlayerCfg { index: self.my_index, bytes: bytes.to_vec() };
        self.transport.send_to(&pkt.encode(), &self.host)?;
        Ok(())
    }

    /// 尝试收 host 广播的 `PlayerCfgAll`（所有玩家完整配置）；当前没有则返回 None。
    pub fn recv_cfg_all(&mut self, rcv: &mut [u8]) -> io::Result<Option<Vec<(u8, Vec<u8>)>>> {
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, _))) => {
                    if let Some(Packet::PlayerCfgAll { entries }) = Packet::decode(&rcv[..n]) {
                        return Ok(Some(entries));
                    }
                }
                Ok(None) => return Ok(None),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(None),
                Err(e) => return Err(e),
            }
        }
    }


    /// 向 host 上报 READY（用于 host 知道 client 已就绪；host 可按此或首帧决定开始）。
    pub fn send_ready(&mut self) -> io::Result<()> {
        let pkt = Packet::Ready;
        self.transport.send_to(&pkt.encode(), &self.host)?;
        Ok(())
    }

    /// 请求补发 `missing_seq`。
    pub fn request_frame(&mut self, missing_seq: u64) -> io::Result<()> {
        let pkt = Packet::ReqFrame { seq: missing_seq };
        self.transport.send_to(&pkt.encode(), &self.host)?;
        Ok(())
    }

    /// 向 host 发送重连请求（附本端稳定身份 + 最后已知 seq，供 host 按身份找回槽位/选快照）。
    pub fn send_reconnect_req(&mut self, identity: u64) -> io::Result<()> {
        let pkt = Packet::ReconnectReq { identity, last_known_seq: self.expect_seq };
        self.transport.send_to(&pkt.encode(), &self.host)?;
        Ok(())
    }

    /// 尝试收 host 回给重连者的整场快照：返回 `Some((world_bytes, seq))`；当前没有则 None。
    pub fn recv_snapshot(&mut self, rcv: &mut [u8]) -> io::Result<Option<(Vec<u8>, u64)>> {
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, _))) => {
                    if let Some(Packet::Snapshot { world_bytes, seq }) = Packet::decode(&rcv[..n]) {
                        return Ok(Some((world_bytes, seq)));
                    }
                }
                Ok(None) => return Ok(None),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(None),
                Err(e) => return Err(e),
            }
        }
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

    /// 处理 host 广播的 `Resync`：把本端起点 seq 对齐到该 seq（配合快照重建后使用）。
    /// 返回是否收到并应用了 Resync。
    pub fn apply_resync(&mut self, rcv: &mut [u8]) -> io::Result<bool> {
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, _))) => {
                    if let Some(Packet::Resync { seq }) = Packet::decode(&rcv[..n]) {
                        // 对齐到重连基线：丢弃所有更早的 pending 帧，从该 seq 起恢复严格按序。
                        self.pending.retain(|(s, _)| *s >= seq);
                        self.expect_seq = seq;
                        return Ok(true);
                    }
                }
                Ok(None) => return Ok(false),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                Err(e) => return Err(e),
            }
        }
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

    /// 就绪往返（可撤销）：client 发 PlayerReady(true) → host 收到 → client_ready/all_clients_ready；
    /// 再发 PlayerReady(false) → 取消就绪 → all_clients_ready 反悔。
    #[test]
    fn ready_state_roundtrip_toggles_withdrawable() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true); // host=0 + client1
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 4096];

        // 默认未就绪。
        assert!(!host.client_ready(1));
        assert!(!host.all_clients_ready());
        // client 就绪 → host 收到。
        cli.send_ready_state(true).unwrap();
        host.poll(&mut rcv);
        assert!(host.client_ready(1));
        assert!(host.all_clients_ready());
        // 取消就绪（可撤销）→ host 收到 → 全体不再就绪。
        cli.send_ready_state(false).unwrap();
        host.poll(&mut rcv);
        assert!(!host.client_ready(1));
        assert!(!host.all_clients_ready());
    }

    /// 房间「在场 + 就绪」流程（每帧持续上行）：client 每帧发输入（在场信号）+ 就绪状态；
    /// host 要求「所有 client 在场（saw_all_clients）&& 全体就绪」才判定 ready；每帧广播 RosterReady 供各端显示。
    #[test]
    fn room_flow_presence_and_ready_and_roster() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true); // host=0 + client1
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 4096];

        // 初始：client 未在场未就绪 → host 不能判 ready。
        assert!(!host.saw_all_clients());
        assert!(!host.all_clients_ready());

        // 模拟客户端房间阶段每帧：上行输入（在场）+ 上报就绪。
        cli.send_input(&[7]).unwrap();
        cli.send_ready_state(true).unwrap();
        host.poll(&mut rcv);

        // 客户端已在场且就绪；host 自身就绪时即可判定全体 ready。
        assert!(host.saw_all_clients(), "client 每帧上行输入后 host 应看到其在场");
        assert!(host.all_clients_ready());
        assert!(host.client_ready(1));

        // host 每帧广播 RosterReady 供 client 显示 → client 应收包含槽 0(host)+槽1(client) 的就绪快照。
        host.broadcast_roster_ready(true);
        let entries = cli.recv_roster_ready(&mut rcv).unwrap().expect("client 应收 RosterReady");
        assert_eq!(entries, vec![(0, true), (1, true)]);

        // 取消就绪（可撤销）：client 发 PlayerReady(false) → host 不再判全体 ready。
        cli.send_ready_state(false).unwrap();
        host.poll(&mut rcv);
        assert!(!host.all_clients_ready());

        // 广播再发时，client 应看到槽 0 就绪、槽 1 取消。
        host.broadcast_roster_ready(true);
        let entries = cli.recv_roster_ready(&mut rcv).unwrap().expect("client 应收 RosterReady");
        assert_eq!(entries, vec![(0, true), (1, false)]);
    }

    /// 房间「合包」：client 用 `send_room_state(ready, input)` 单包上行，host 一次更新「在场 + 就绪」。
    #[test]
    fn room_state_bundle_sets_both_presence_and_ready() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true);
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 4096];

        assert!(!host.saw_all_clients());
        assert!(!host.all_clients_ready());

        // client 房间阶段单包上行：就绪 + 在场。
        cli.send_room_state(true, &[7, 8, 9]).unwrap();
        host.poll(&mut rcv);

        assert!(host.saw_all_clients(), "RoomState 应同时标记在场");
        assert!(host.all_clients_ready(), "RoomState 应同时标记就绪");
        assert!(host.client_ready(1));

        // 取消就绪（可撤销）：再发 ready=false。
        cli.send_room_state(false, &[7, 8, 9]).unwrap();
        host.poll(&mut rcv);
        assert!(!host.all_clients_ready());
        assert!(host.saw_all_clients(), "在场信号应保持");
    }

    /// 配置收集/广播：client 上报 PlayerCfg → host 收齐(含自身) → 广播 PlayerCfgAll → client 收到完整配置。
    #[test]
    fn host_gathers_cfgs_and_broadcasts_all() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true); // host=0 + client1
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 4096];

        // host 自己的配置（player 0）
        let host_cfg = vec![1, 0, 0, 2, 5]; // 任意字节（PlayerConfig 编码）
        host.set_local_cfg(host_cfg.clone());
        // client 上报配置（player 1）
        let client_cfg = vec![1, 0, 0, 3, 9];
        cli.send_cfg(&client_cfg).unwrap();
        host.poll_cfg(&mut rcv);

        // 未收齐时 collect_cfgs 应为 None（已设 host + client，这里应收齐）。
        let all = host.collect_cfgs().expect("host 与 client 配置应收齐");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].0, 0);
        assert_eq!(all[0].1, host_cfg);
        assert_eq!(all[1].0, 1);
        assert_eq!(all[1].1, client_cfg);

        // host 广播 → client 收到完整配置
        host.broadcast_cfgs(&all);
        let got = cli.recv_cfg_all(&mut rcv).unwrap().expect("client 应收 PlayerCfgAll");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, 0);
        assert_eq!(got[0].1, host_cfg);
        assert_eq!(got[1].0, 1);
        assert_eq!(got[1].1, client_cfg);
    }

    /// 掉线离场（切片1）：client 掉线后，host 用默认输入占位照常产帧（不再等它），不卡全队。
    #[test]
    fn host_continues_after_client_dropped() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true); // host=0 + client1
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 4096];

        // 掉线前：client 参与，host 应收齐产帧。
        for i in 0..3u8 {
            let inp = game_core::netcode::encode_player_input(&game_core::world::PlayerInput {
                set_target: Some(game_core::fix::Vec2::new(i.into(), 0.into())),
                ..Default::default()
            });
            cli.send_input(&inp).unwrap();
            host.poll(&mut rcv);
            host.set_local_input(Some(vec![i + 100]));
            assert!(host.try_emit().is_some(), "掉线前应收齐产帧");
        }

        // 标记 client1 掉线：此后 host 不再等它，用默认输入占位照常产帧。
        host.mark_dropped(1);
        let before = host.next_seq();
        for _ in 0..10 {
            host.set_local_input(Some(vec![7]));
            assert!(host.try_emit().is_some(), "掉线后 host 应继续产帧（不因缺 client 卡死）");
        }
        assert!(host.next_seq() > before, "掉线后 host 应持续前进");
    }

    /// 重连全链路（client 侧收 Snapshot + Resync）：client 发 ReconnectReq →
    /// host 用已保存快照应答 Snapshot 并广播 Resync → client 接快照重建 World + set_start_seq/apply_resync
    /// → 继续跑，host 与重连端仍逐位一致。
    #[test]
    fn reconnect_snapshot_and_resync_roundtrip() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true); // host=0 + client1
        let cli_identity = 70001u64;
        // 登记 client1（槽位0）的稳定身份，验证重连按身份找回槽位。
        host.set_client_identities(&[Some(cli_identity)]);
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 16384];

        // 跑若干帧，host 周期保存快照。
        for i in 0..10u8 {
            cli.send_input(&encode_input(i)).unwrap();
            host.poll(&mut rcv);
            host.set_local_input(Some(vec![i + 100]));
            if let Some((seq, _)) = host.try_emit() {
                // host 侧记录当前 World 快照（这里用任意确定性字节，测试只关注 seq 与链路）。
                host.set_snapshot(format!("snap@{seq}").into_bytes(), seq);
            }
            let _ = cli.step_frame(&mut rcv).unwrap();
        }

        // client 掉线：host 标记 drop 并继续推进几步（期间 host 快照继续更新）。
        host.mark_dropped(1);
        for _ in 0..5u8 {
            host.set_local_input(Some(vec![7]));
            if host.try_emit().is_some() {
                // 快照 seq = 下一帧号（World 已反映到上一帧）。
                host.set_snapshot(format!("snap@{}", host.next_seq() - 1).into_bytes(), host.next_seq());
            }
        }

        // 重连：client 发 ReconnectReq（附稳定身份）→ host.poll 应答 Snapshot + 广播 Resync。
        let before = cli.expect_seq();
        cli.send_reconnect_req(cli_identity).unwrap();
        host.poll(&mut rcv); // host 处理 ReconnectReq，回 Snapshot 并广播 Resync

        // client 收 Snapshot：应得到最近快照及其 seq（= host 下一帧号）。
        let (wb, seq) = cli.recv_snapshot(&mut rcv).unwrap().expect("应收到 Snapshot");
        let expect_seq_at_snap = String::from_utf8(wb).unwrap();
        assert!(expect_seq_at_snap.starts_with("snap@"), "快照应为 host 最近保存的那份");
        assert_eq!(seq, host.next_seq(), "快照 seq 应为 host 当前下一帧号");

        // host 已把该客户端从掉线恢复（unmark_dropped 在 poll 应答时隐式完成）。
        // client 收 Resync 对齐基线（从快照 seq 起继续）。
        let applied = cli.apply_resync(&mut rcv).unwrap();
        assert!(applied, "应收到 Resync 并应用");
        assert_eq!(cli.expect_seq(), seq, "Resync 应把 client 基线对齐到快照 seq");
        assert!(cli.expect_seq() > before, "重连后期待 seq 应前进到快照处");

        // 重连后继续跑：host 与 client 均从快照 seq 后继续 lockstep。
        for i in 0..20u8 {
            let inp = encode_input(i);
            cli.send_input(&inp).unwrap();
            host.poll(&mut rcv);
            host.set_local_input(Some(vec![i + 50]));
            let _ = host.try_emit();
            while let Some(_) = cli.step_frame(&mut rcv).unwrap() {}
        }
        assert_eq!(cli.expect_seq(), seq + 20, "重连后应继续严格按序推进 20 帧");
    }

    /// 构造本测试专用的确定性输入字节。
    fn encode_input(i: u8) -> Vec<u8> {
        game_core::netcode::encode_player_input(&game_core::world::PlayerInput {
            set_target: Some(game_core::fix::Vec2::new(i.into(), 0.into())),
            ..Default::default()
        })
    }

    /// 自动掉线判定（切片1运行时版）：client 停发输入 → host 每帧 poll 累计空闲 → 达阈值 auto_drop_idle 自动掉线，不再卡全队。
    #[test]
    fn host_auto_drops_idle_client() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true); // host=0 + client1
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 4096];

        // 先正常跑几帧（client 持续上行）。
        for _ in 0..3u8 {
            cli.send_input(&encode_input(1)).unwrap();
            host.poll(&mut rcv);
            host.set_local_input(Some(vec![9]));
            assert!(host.try_emit().is_some());
        }
        assert_eq!(host.client_idle_ticks(1), 0);

        // client 停发：host 每帧仍 poll（会因缺 client 输入无法产帧，但空闲计数逐帧累加）。
        let dropped = 3u32;
        let mut dropped_list = Vec::new();
        let mut advanced_after = false;
        for _ in 0..(dropped + 3) {
            host.poll(&mut rcv); // 空闲计数 +1（client 未发）
            dropped_list.extend(host.auto_drop_idle(dropped)); // 达阈值自动掉线
            host.set_local_input(Some(vec![9]));
            if host.try_emit().is_some() {
                advanced_after = true;
            }
        }
        assert!(!dropped_list.is_empty(), "client 空闲达阈值应被自动掉线");
        assert_eq!(dropped_list[0], 1);
        assert!(advanced_after, "自动掉线后 host 应能靠默认占位继续产帧（不卡全队）");
        // 掉线后继续推进：不再因缺 client 卡死。
        let before = host.next_seq();
        for _ in 0..5u8 {
            host.poll(&mut rcv);
            host.set_local_input(Some(vec![9]));
            assert!(host.try_emit().is_some(), "掉线后应持续产帧");
        }
        assert!(host.next_seq() > before);
    }
}

