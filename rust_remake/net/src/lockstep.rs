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
    /// 本局实际参与对局的 client 槽位集合（`None`=全部 expected 槽位都参与，兼容满员/局域网）。
    /// 用于“人不满也启动”：建房上限里只有部分 client 进场就绪，host 开局时把“实际就绪者”设为参与集，
    /// 产帧与就绪/配置判定仅对参与集要求，其余 vacant 槽位排除在局外。
    active: Option<Vec<bool>>,
    /// 本局参与玩家的【原 player index】有序列表（host=0 恒在首，其余按参与 client 槽升序）。
    /// new(=本局世界内) index 即“在该列表中的位置”，产帧/配置/self_index 都用 new index，保证不满员时两端角色数量与编号一致。
    participants_orig: Vec<u8>,
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
    /// 各 client 当前是否已配好技能/配置（开局配置阶段由 `Packet::RoomState.build_done` 更新）。
    /// host 收齐所有端(含自身) `build_done` 才产首帧统一开战（对齐局域网机制，消除“各端分别按 o 进对局不同步”）。
    clients_build_done: Vec<bool>,
    /// 累计收到过的 `PlayerReady` 包总数（诊断用：区分“包根本没到”与“到了但值为 false”）。
    ready_packets_seen: u64,
    /// 累计收到过并被 `poll`/`poll_cfg` 处理记进 `cfgs` 的 `PlayerCfg` 包数（诊断用）。
    player_cfg_packets_seen: u64,
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
            active: None,
            participants_orig: Vec::new(),
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
            clients_build_done: vec![false; expected],
            ready_packets_seen: 0,
            player_cfg_packets_seen: 0,
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

    /// 把当前整场 World 快照广播给所有 client（并本地保存），让每个端都持有最新快照、具备「host 掉线时接管」的能力。
    /// 语义同 `set_snapshot`：`world_bytes` 反映「已处理完 seq-1 帧」的世界状态，接任者应重建后从 seq 继续。
    /// 调用方应在「World 已应用完第 seq 帧」后传下一帧号 `host.next_seq()`。
    pub fn broadcast_snapshot(&mut self, world_bytes: Vec<u8>, seq: u64) {
        self.snapshot = Some((world_bytes.clone(), seq));
        let pkt = Packet::Snapshot { world_bytes, seq };
        let enc = pkt.encode();
        for peer in self.client_peers.iter().flatten() {
            let _ = self.transport.send_to(&enc, peer);
        }
    }

    /// 累计一帧推进（host 每产一帧调用一次，供上层做超时判活）。
    pub fn bump_alive(&mut self) {
        self.alive_tick += 1;
    }

    /// 广播「全体就绪→进入配置」给所有 client（供房间阶段 host 通知 client 进配置菜单）。
    pub fn broadcast_start_config(&mut self) {
        let pkt = Packet::StartConfig { seq: self.next_seq };
        let enc = pkt.encode();
        // 连发 3 拍增强投递可靠性（Steam P2P 曾实测丢过的小包重发即达）。
        for _ in 0..3 {
            for peer in self.client_peers.iter().flatten() {
                let _ = self.transport.send_to(&enc, peer);
            }
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

    /// 所有【参与本局】的 client 是否都已就绪（不含 host 自身；host 自身的就绪由调用方另管）。
    /// 未参与的 vacant 槽位不要求。
    pub fn all_clients_ready(&self) -> bool {
        (0..self.expected).all(|c| !self.is_active(c) || self.clients_ready[c])
    }

    /// 某 client（完整的玩家序号）当前是否已配好技能/配置（开局配置阶段）。
    pub fn client_build_done(&self, client_seq: u8) -> bool {
        let c = client_seq as usize - self.local_base as usize;
        c < self.expected && self.clients_build_done[c]
    }

    /// 所有【参与本局】的 client 是否都已配好技能/配置（不含 host 自身；host 自身是否配好由调用方另管）。
    pub fn all_clients_build_done(&self) -> bool {
        (0..self.expected).all(|c| !self.is_active(c) || self.clients_build_done[c])
    }

    /// 已配好技能/配置（收到 RoomState.build_done=true）的 client 数。
    pub fn build_done_clients_count(&self) -> usize {
        self.clients_build_done.iter().filter(|b| **b).count()
    }

    /// 重置所有 client 的「配好」标志为 false（进入开局配置阶段时调用，重新收集各端 build_done）。
    pub fn reset_clients_build_done(&mut self) {
        for b in self.clients_build_done.iter_mut() {
            *b = false;
        }
    }

    /// 已上行过输入（在场信号）的 client 数。
    pub fn present_clients_count(&self) -> usize {
        self.latest_input.iter().filter(|x| x.is_some()).count()
    }

    /// 各 client 槽位当前是否在场（有输入），返回长度=expected 的掩码。供开启一局前 `set_participants` 用。
    pub fn present_mask(&self) -> Vec<bool> {
        (0..self.expected).map(|c| self.latest_input[c].is_some()).collect()
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

    /// 某 client 槽位（下标）是否参与本局对局（`active=None` 时全部参与）。
    fn is_active(&self, c: usize) -> bool {
        match &self.active {
            None => true,
            Some(v) => v.get(c).copied().unwrap_or(false),
        }
    }

    /// 参与玩家【原 index → 本局 new index】：new = 在 `participants_orig` 中的位置；未设参与集时退化为 identity。
    fn orig_to_new(&self, orig: u8) -> u8 {
        if self.participants_orig.is_empty() {
            orig // 未设参与集（满员/局域网）：new = orig
        } else {
            self.participants_orig.iter().position(|&x| x == orig).unwrap() as u8
        }
    }

    /// 设置本局参与对局的 client 槽位集合（“人不满也启动”：只让实际就绪者参与，其余 vacant 槽位排除），
    /// 并据此算出参与玩家【原 player index】有序列表（host=0 恒在首，其余按参与 client 槽升序）。
    /// 长度必须严格等于 `expected`，否则不生效并返回 false。调用时机：host 判定“可开局”时（进配置/开战前）。
    pub fn set_participants(&mut self, active: &[bool]) -> bool {
        if active.len() != self.expected {
            return false;
        }
        self.active = Some(active.to_vec());
        // 参与玩家原 index：host(=local_base 0) + 参与 client 的 orig player index（槽序升序）。
        let mut orig: Vec<u8> = Vec::with_capacity(self.expected + self.local_base as usize);
        if self.local_base > 0 {
            orig.push(0); // host = player 0
        }
        for (c, &on) in active.iter().enumerate() {
            if on {
                orig.push((c + self.local_base as usize) as u8);
            }
        }
        self.participants_orig = orig;
        true
    }

    /// 参与玩家的原 player index 列表（host=0 在首；new/本局 index = 在该列表中的位置）。供广播给 client 对齐下标。
    pub fn participants_orig(&self) -> &[u8] {
        &self.participants_orig
    }

    /// 本局参与玩家总数（host + 参与 client 数）。
    pub fn participants_count(&self) -> usize {
        if self.participants_orig.is_empty() {
            // 未设参与集前：默认全 expected + host。
            self.expected + self.local_base as usize
        } else {
            self.participants_orig.len()
        }
    }

    /// 当前参与的 client 槽位数（诊断/界面用）。
    pub fn active_clients_count(&self) -> usize {
        (0..self.expected).filter(|&c| self.is_active(c)).count()
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
                        self.player_cfg_packets_seen += 1;
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

    /// 是否已收齐所有【参与本局】的端（host 自身 + 参与 client）的配置（未参与的 vacant 槽位不要求）。
    pub fn all_cfgs(&self) -> bool {
        if self.local_base > 0 && self.local_cfg.is_none() {
            return false;
        }
        (0..self.expected).all(|c| !self.is_active(c) || self.cfgs[c].is_some())
    }

    /// 合并所有【参与本局】端配置：`(player_index, bytes)`（host=0 在前，参与 client 随后），收齐才 Some。
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
        for c in 0..self.expected {
            if self.is_active(c) {
                if let Some(b) = &self.cfgs[c] {
                    let orig = (c + self.local_base as usize) as u8;
                    let new = self.orig_to_new(orig);
                    out.push((new, b.clone()));
                }
            }
        }
        out.sort_by_key(|(i, _)| *i);
        Some(out)
    }

    /// 广播 `PlayerCfgAll`（所有端完整配置 + 本局参与玩家原 index 列表）给所有 client。
    pub fn broadcast_cfgs(&mut self, entries: &[(u8, Vec<u8>)]) {
        // 未设参与集（满员/局域网）时 participants 退化为全量 orig（identity）。
        let parts: Vec<u8> = if self.participants_orig.is_empty() {
            (0..(self.expected + self.local_base as usize)).map(|i| i as u8).collect()
        } else {
            self.participants_orig.clone()
        };
        let pkt = Packet::PlayerCfgAll {
            entries: entries.to_vec(),
            participants: parts,
        };
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

    /// 诊断：累计被 `poll`/`poll_cfg` 处理并记进 `cfgs` 的 PlayerCfg 包数（用于判断 PlayerCfg 是否真的到达并解码）。
    pub fn player_cfg_packets_seen(&self) -> u64 {
        self.player_cfg_packets_seen
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

    /// host 是否已见过所有【参与本局】的 client 的输入至少一次（未参与的 vacant 槽位不要求）。
    pub fn saw_all_clients(&self) -> bool {
        (0..self.expected).all(|c| !self.is_active(c) || self.latest_input[c].is_some())
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
                            Packet::RoomState { index, ready, build_done, input_bytes } => {
                                let c = index as usize - self.local_base as usize;
                                if c < self.expected {
                                    // 房间阶段合包：一次更新「在场 + 就绪 + 配好 + 端点 + 空闲」。可靠的输入在场通道。
                                    self.client_peers[c] = Some(from);
                                    self.client_addr[c] = Some(from);
                                    self.latest_input[c] = Some(input_bytes);
                                    self.clients_ready[c] = ready;
                                    self.clients_build_done[c] = build_done;
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
                            Packet::PlayerCfg { index, bytes } => {
                                // 配置同步（HostGather/ClientWait）期间，client 会把 PlayerCfg 与心跳（RoomState）混在同一
                                // 批次上行；这里若把 PlayerCfg 当无关包丢弃，随后调用的 `poll_cfg` 就什么都收不到 → host 永远
                                // 收不齐配置 → 卡在配置同步。故 poll 也必须把 PlayerCfg 记进 `cfgs`（与 `poll_cfg` 语义一致）。
                                self.player_cfg_packets_seen += 1;
                                let c = index as usize - self.local_base as usize;
                                if c < self.expected {
                                    if self.client_peers[c].is_none() {
                                        self.client_peers[c] = Some(from);
                                    }
                                    self.client_addr[c] = Some(from);
                                    self.cfgs[c] = Some(bytes);
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
        // 若 host 参与，总玩家数 = expected + 1；需 host 本地输入 + 所有【参与】的 client 输入（未参与的 vacant 槽位不要求）。
        if !(0..self.expected).all(|c| !self.is_active(c) || self.latest_input[c].is_some()) {
            return None;
        }
        if self.local_base > 0 && self.local.is_none() {
            return None;
        }
        let mut entries: FrameData = Vec::new();
        // host local = 本局 new index 0（原 player 0）。
        if self.local_base > 0 {
            entries.push((0, self.local.clone().unwrap()));
        }
        for c in 0..self.expected {
            if self.is_active(c) {
                if let Some(bytes) = &self.latest_input[c] {
                    // 参与玩家收缩为本局连续 index：new index = 在 participants_orig 中该 orig index 的位置。
                    let orig = (c + self.local_base as usize) as u8;
                    let new = self.orig_to_new(orig);
                    entries.push((new, bytes.clone()));
                }
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

    /// 只读访问底层传输（诊断用，如取 `send_stats()`）。
    pub fn transport_ref(&self) -> &T {
        &self.transport
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
    /// host 主动广播的最新媒体快照（`(world_bytes, seq)`，seq=应重建后继续的下一帧号）。
    /// 供「host 掉线时接管」使用（阶段 3）；在正常收帧循环里顺带缓存、不应用、不推进。
    latest_snapshot: Option<(Vec<u8>, u64)>,
}

impl<T: Transport> ClientLockstep<T> {
    pub fn new(transport: T, my_index: u8, host: Peer) -> Self {
        ClientLockstep {
            transport,
            my_index,
            expect_seq: 0,
            pending: VecDeque::new(),
            host,
            latest_snapshot: None,
        }
    }

    /// 取走缓存的最新媒体快照（`(world_bytes, seq)`）。无则 None。
    pub fn take_latest_snapshot(&mut self) -> Option<(Vec<u8>, u64)> {
        self.latest_snapshot.take()
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

    /// 房间阶段：把「就绪 + 配好 + 输入在场信号」合成单包持续上行给 host。
    /// （P2P 下独立的 PlayerReady 包曾实测常丢，而输入在场包可靠；故把就绪/配好折进同一在场包。）
    /// `build_done` 表示本端是否已在开局配置阶段配好技能/配置（host 收齐所有端才统一开战）。
    pub fn send_room_state(&mut self, ready: bool, build_done: bool, input_bytes: &[u8]) -> io::Result<()> {
        let pkt = Packet::RoomState {
            index: self.my_index,
            ready,
            build_done,
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
                    if let Some(Packet::StartConfig { .. }) = Packet::decode(&rcv[..n]) {
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

    /// 一次性读 host 房间阶段的入包，并分类返回：`(是否收到 StartConfig, 最新 RosterReady 快照)`。
    /// 与分开的 `recv_start_config`/`recv_roster_ready` 不同，这里**单次排空队列**逐步分类，
    /// 不会出现“先读 RosterReady 的循环把 StartConfig 当非目标包消费掉”导致进不了配置菜单。
    pub fn recv_room_inbox(&mut self, rcv: &mut [u8]) -> io::Result<(bool, Option<Vec<(u8, bool)>>)> {
        let mut start_config = false;
        let mut roster = None;
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, _))) => {
                    if let Some(pkt) = Packet::decode(&rcv[..n]) {
                        match pkt {
                            Packet::StartConfig { .. } => start_config = true,
                            Packet::RosterReady { entries } => roster = Some(entries),
                            _ => {}
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        Ok((start_config, roster))
    }

    /// 向 host 上报本玩家最终配置（`PlayerCfg`，载荷为 `PlayerConfig::encode()` 字节）。
    pub fn send_cfg(&mut self, bytes: &[u8]) -> io::Result<()> {
        let pkt = Packet::PlayerCfg { index: self.my_index, bytes: bytes.to_vec() };
        self.transport.send_to(&pkt.encode(), &self.host)?;
        Ok(())
    }

    /// 尝试收 host 广播的 `PlayerCfgAll`（所有玩家完整配置 + 参与列表）；当前没有则返回 None。
    /// 返回 `(entries, participants)`。
    pub fn recv_cfg_all(&mut self, rcv: &mut [u8]) -> io::Result<Option<(Vec<(u8, Vec<u8>)>, Vec<u8>)>> {
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, _))) => {
                    if let Some(Packet::PlayerCfgAll { entries, participants }) = Packet::decode(&rcv[..n]) {
                        return Ok(Some((entries, participants)));
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

    /// 配置/等待期专用：只 pump 回调并缓存 host 广播的 FRAME 到 `pending`，但**不推进 `expect_seq`**、不步进世界。
    /// 返回 `Ok(true)` 表示收到了 ≥ expect 的新帧（即 host 已开始产帧，本端可据此进入对局）。
    /// 与 `step_frame` 的区别：`step_frame` 收帧后还会 `try_advance` 更新 `expect_seq`（配合步进 world）；
    /// 而配置期我们只想让连接保活、且不把 `expect_seq` 前冲（否则进入对局后 start 锚点错位 → world 与 host 分叉）。
    pub fn pump_frames(&mut self, rcv: &mut [u8]) -> io::Result<bool> {
        let mut got = false;
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, _))) => {
                    if let Some(pkt) = Packet::decode(&rcv[..n]) {
                        match pkt {
                            Packet::Frame { seq, entries } => {
                                if seq >= self.expect_seq {
                                    let pos = self.pending.iter().position(|(s, _)| *s >= seq).unwrap_or(self.pending.len());
                                    self.pending.insert(pos, (seq, entries));
                                    got = true;
                                }
                            }
                            Packet::Snapshot { world_bytes, seq } => {
                                // 顺带缓存 host 主动广播的最新媒体快照（不应用、不推进），供「host 掉线接管」用。
                                self.latest_snapshot = Some((world_bytes, seq));
                            }
                            _ => {}
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        Ok(got)
    }

    /// 从 transport 收一个 FRAME：入 pending 并尝试消费连续帧。
    /// 返回 `Ok(Some(entries))` 表示推进了一帧；`Ok(None)` 表示当前无可用帧（未推进）。
    pub fn step_frame(&mut self, rcv: &mut [u8]) -> io::Result<Option<FrameData>> {
        // 收当前所有 FRAME（有界轮询一次）。
        loop {
            match self.transport.recv_from(rcv) {
                Ok(Some((n, _))) => {
                    if let Some(pkt) = Packet::decode(&rcv[..n]) {
                        match pkt {
                            Packet::Frame { seq, entries } => {
                                if seq >= self.expect_seq {
                                    // 只缓存 >= expect 的帧；丢弃过时帧。
                                    let pos = self.pending.iter().position(|(s, _)| *s >= seq).unwrap_or(self.pending.len());
                                    self.pending.insert(pos, (seq, entries));
                                }
                            }
                            Packet::Snapshot { world_bytes, seq } => {
                                // 顺带缓存 host 主动广播的最新媒体快照（不应用、不推进），供「host 掉线接管」用。
                                self.latest_snapshot = Some((world_bytes, seq));
                            }
                            _ => {}
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

    /// 只读访问底层传输（诊断用，如取 `send_stats()`）。
    pub fn transport_ref(&self) -> &T {
        &self.transport
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
        cli.send_room_state(true, false, &[7, 8, 9]).unwrap();
        host.poll(&mut rcv);

        assert!(host.saw_all_clients(), "RoomState 应同时标记在场");
        assert!(host.all_clients_ready(), "RoomState 应同时标记就绪");
        assert!(host.client_ready(1));
        assert!(!host.client_build_done(1), "未上报 build_done 应为 false");

        // 取消就绪（可撤销）：再发 ready=false。
        cli.send_room_state(false, false, &[7, 8, 9]).unwrap();
        host.poll(&mut rcv);
        assert!(!host.all_clients_ready());
        assert!(host.saw_all_clients(), "在场信号应保持");
    }

    /// 房间「合包」的 build_done：client 上报 build_done=true → host 判定该端已配完；也可重置收集。
    #[test]
    fn room_state_build_done_gates_config_gather() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true);
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 4096];

        assert!(!host.all_clients_build_done());
        cli.send_room_state(true, true, &[7, 8, 9]).unwrap();
        host.poll(&mut rcv);
        assert!(host.client_build_done(1), "上报 build_done=true 后应置位");
        assert!(host.all_clients_build_done(), "全部 client 配完");
        assert_eq!(host.build_done_clients_count(), 1);

        // 重置（进入下一阶段重新收集）。
        host.reset_clients_build_done();
        assert!(!host.all_clients_build_done());
    }

    /// 配置期 keepalive：`pump_frames` 只缓存不推进 expect_seq（host 已产帧时能感知开始，但锚点不错位）。
    #[test]
    fn client_pump_frames_caches_without_advancing_expect() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true); // host=0 + client1
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 4096];

        // host 产一帧（需要 local + 全部 client 输入）。
        host.set_local_input(Some(vec![1, 2, 3]));
        cli.send_room_state(true, true, &[9, 9, 9]).unwrap();
        host.poll(&mut rcv);
        let emitted = host.try_emit().expect("应能产第一帧");
        let seq0 = emitted.0;
        assert_eq!(seq0, 0);

        // 配置期 client 用 pump_frames：应感知到 host 已开始（返回 true），但 expect_seq 仍为 0。
        let mut prcv = [0u8; 4096];
        let got = cli.pump_frames(&mut prcv).expect("pump 不应出错");
        assert!(got, "pump_frames 应感知到 host 开始产帧");
        assert_eq!(cli.expect_seq(), 0, "pump_frames 不得推进 expect_seq");

        // 进入对局后 step_frame 才消费 seq=0，且 expect_seq 推进到 1。
        let frame = cli.step_frame(&mut prcv).expect("step 不应出错").expect("应从 pending 消费 seq=0");
        assert_eq!(cli.expect_seq(), 1);
        let n_ents = frame.len();
        assert_eq!(n_ents, 2, "首帧应含两端输入");
    }

    /// 房间入包单次排空分类：StartConfig 与 RosterReady 混在队列里也不会互吞，都能被正确识别。
    #[test]
    fn room_inbox_classifies_start_config_and_roster_together() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true);
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 4096];

        // host 广播：先 roster，再 StartConfig（模拟同一帧到件、且 roster 先到）。
        let roster_pkt = Packet::RosterReady { entries: vec![(0, true), (1, true)] };
        let start_pkt = Packet::StartConfig { seq: 0 };
        // 直接把两个包一次性投递给 client（借用 pair 的 transport 投递方向：cli 发→host；host 发需从 host 端投）。
        // 这里我们用 host 的 transport.send_to 会投到 client 的 peer_inbox。
        host.transport.send_to(&roster_pkt.encode(), &Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4001)))).unwrap();
        host.transport.send_to(&start_pkt.encode(), &Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4001)))).unwrap();

        let (got_cfg, roster) = cli.recv_room_inbox(&mut rcv).unwrap();
        assert!(got_cfg, "StartConfig 应被识别");
        assert_eq!(roster, Some(vec![(0, true), (1, true)]), "RosterReady 也应被识别");
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
        let (got, parts) = cli.recv_cfg_all(&mut rcv).unwrap().expect("client 应收 PlayerCfgAll");
        assert_eq!(parts, vec![0, 1], "参与列表应含 host 与 client 原 index");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, 0);
        assert_eq!(got[0].1, host_cfg);
        assert_eq!(got[1].0, 1);
        assert_eq!(got[1].1, client_cfg);
    }

    /// Steam 配置→统一开战综合链路（修 1 的 net 侧）：
    /// client 上报 RoomState(build_done=true)+PlayerCfg，host 认齐 build_done → 收集 cfg + 广播 PlayerCfgAll →
    /// client 收到并应用 → host 产 seq=0 首帧 → client 收帧推进且 expect_seq 由 0 起（开战锚点一致）。
    /// 这锁死“所有端配完 → 统一开始”，消除“各端分别按 o 进对局不同步”。
    #[test]
    fn steam_config_gather_then_unified_start_identical() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true); // host=0 + client1
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 4096];

        // —— 配置阶段：client 每帧 RoomState(build_done=true, 在场输入) + send_cfg ——
        cli.send_room_state(true, true, &[7, 8, 9]).unwrap();
        host.poll(&mut rcv);
        assert!(host.all_clients_build_done(), "host 看到该 client 已配完");

        // host 进入配置同步：收 client cfg + 设自身 cfg → 收齐 → 广播。
        let host_cfg = vec![1, 0, 0, 2, 5];
        let client_cfg = vec![1, 0, 0, 3, 9];
        host.set_local_cfg(host_cfg.clone());
        cli.send_cfg(&client_cfg).unwrap();
        host.poll_cfg(&mut rcv);
        let all = host.collect_cfgs().expect("配置应收齐");
        assert_eq!(all.len(), 2);
        host.broadcast_cfgs(&all);
        let (got, _parts) = cli.recv_cfg_all(&mut rcv).unwrap().expect("client 应收 PlayerCfgAll");
        assert_eq!(got.len(), 2);

        // —— 开战：host 产 seq=0 首帧（统一开始信号），client 严格从 expect_seq=0 推进 ——
        assert_eq!(cli.expect_seq(), 0, "开战前 client 锚点应为 0");
        host.set_local_input(Some(vec![1, 2, 3]));
        let emitted = host.try_emit().expect("host 应能产首帧");
        assert_eq!(emitted.0, 0, "首帧 seq 应为 0");
        assert_eq!(emitted.1.len(), 2, "首帧应含两端输入");

        // client 用 pump（不推进）+ step_frame（推进）：首帧入 pending 后按序消费 seq=0。
        let got_frame = cli.pump_frames(&mut rcv).expect("pump 不应出错");
        assert!(got_frame, "应该收到 host 首帧");
        assert_eq!(cli.expect_seq(), 0, "pump 不得推进 expect_seq");
        let frame = cli.step_frame(&mut rcv).expect("step 不应出错").expect("应从 pending 消费 seq=0");
        assert_eq!(cli.expect_seq(), 1, "开战后推进到 seq=1");
        assert_eq!(frame.len(), 2);
    }

    /// Steam host 在 HostGather 阶段把“心跳收包”与“配置收集”同一批次处理时不吞 PlayerCfg：
    /// client 每帧 RoomState(心跳) + PlayerCfg(配置) 混批上行，host 先 `poll`（保活/收心跳）再 `poll_cfg`（收配置）；
    /// 若 `poll` 把 PlayerCfg 当无关包丢弃，则 poll_cfg 收不到、host 永远收不齐配置 → 卡在配置同步。
    /// 锁死 `poll` 必须把 PlayerCfg 一并记进 `cfgs`（与 poll_cfg 同语义）。
    #[test]
    fn host_poll_does_not_swallow_player_cfg_from_heartbeat_batch() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true); // host=0 + client1
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 8192];

        // 模拟 ClientWait 阶段 client 每帧：RoomState(心跳, build_done=true) + PlayerCfg(我的配置)。
        let client_cfg = vec![1, 0, 0, 3, 9];
        let host_cfg = vec![1, 0, 0, 2, 5];
        host.set_local_cfg(host_cfg);
        for _ in 0..3 {
            // 同帧先心跳后配置（P2P 下二者可能混在同一批次到达）。
            cli.send_room_state(true, true, &[7, 8, 9]).unwrap();
            cli.send_cfg(&client_cfg).unwrap();
            // host 的 HostGather：先 poll（收心跳保活 + 不应吞 cfg），再 poll_cfg（收集配置）。
            host.poll(&mut rcv);
            host.poll_cfg(&mut rcv);
        }
        // 配置必须被完整收进 cfgs（即便 host 每帧先 poll 再 poll_cfg）。
        assert!(host.all_cfgs(), "poll 不吞 PlayerCfg 时 host 应收齐配置");
        let all = host.collect_cfgs().expect("配置应收齐");
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|(i, b)| *i == 1 && *b == client_cfg), "client 的 PlayerCfg 应被记录");
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

    /// 阶段 1（Steam 战斗端等效）完整链路：对战中 client 停发 → host `auto_drop_idle` 自动掉线并靠默认占位继续产帧（不空转）
    /// → client 发 `ReconnectReq`(带身份) 重连 → host 恢复该槽 + 回 `Snapshot` + 广播 `Resync` → client 重建对齐 →
    /// 两端继续逐位推进。锁死「host 自动掉线 + 掉线重连接回」整条链路（Steam 版接入复用此传输无关逻辑）。
    #[test]
    fn host_auto_drops_then_client_reconnects_resumes() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true); // host=0 + client1
        let cli_identity = 70001u64;
        host.set_client_identities(&[Some(cli_identity)]);
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 16384];

        // A 段：正常跑几帧，两端同步。
        for _ in 0..5u8 {
            cli.send_input(&encode_input(1)).unwrap();
            host.poll(&mut rcv);
            host.set_local_input(Some(vec![9]));
            assert!(host.try_emit().is_some());
        }
        assert_eq!(host.client_idle_ticks(1), 0);

        // B 段：client 停发，host 空闲超阈值自动掉线并继续产帧（默认占位，不卡全队）；期间周期保存快照。
        let dropped = 3u32;
        let mut dropped_list = Vec::new();
        for _ in 0..(dropped + 5) {
            host.poll(&mut rcv);
            dropped_list.extend(host.auto_drop_idle(dropped));
            host.set_local_input(Some(vec![9]));
            if host.try_emit().is_some() {
                host.set_snapshot(format!("snap@{}", host.next_seq() - 1).into_bytes(), host.next_seq());
            }
        }
        assert!(!dropped_list.is_empty(), "client 空闲超阈值应被自动掉线");
        assert_eq!(dropped_list[0], 1);

        // C 段：client 重连（发 ReconnectReq 附身份）→ host.poll 处理（恢复槽 + 回 Snapshot + 广播 Resync）。
        let before = cli.expect_seq();
        cli.send_reconnect_req(cli_identity).unwrap();
        host.poll(&mut rcv);
        let (wb, seq) = cli.recv_snapshot(&mut rcv).unwrap().expect("应收到 Snapshot");
        assert!(String::from_utf8(wb).unwrap().starts_with("snap@"), "应为 host 最近保存的快照");
        assert_eq!(seq, host.next_seq(), "快照 seq 应为 host 当前下一帧号");
        assert_eq!(cli.expect_seq(), before, "收到 Snapshot 但未 Resync 前基线不变");
        let applied = cli.apply_resync(&mut rcv).unwrap();
        assert!(applied, "应收到 Resync 对齐基线");
        assert_eq!(cli.expect_seq(), seq, "Resync 应把 client 基线对齐到快照 seq");

        // D 段：重连后两端继续 lockstep（host 已 unmark_dropped，需重新收到 client 输入才产帧）。
        let mut advanced = 0u32;
        for i in 0..20u8 {
            cli.send_input(&encode_input(i)).unwrap();
            host.poll(&mut rcv);
            host.set_local_input(Some(vec![i + 50]));
            if host.try_emit().is_some() {
                advanced += 1;
            }
            while let Some(_) = cli.step_frame(&mut rcv).unwrap() {}
        }
        assert!(advanced > 0, "重连后应能继续产帧（防假绿）");
        assert_eq!(cli.expect_seq(), seq + 20, "重连后应继续严格按序推进 20 帧");
    }

    /// 阶段 2（快照广播）：host `broadcast_snapshot` 把快照广播给所有 client，client 在正常收帧循环
    /// （step_frame）里顺带缓存到 `latest_snapshot`（不应用、不推进），`take_latest_snapshot` 可取走。
    /// 锁死「每端都持有最新快照、具备 host 掉线接管能力」。
    #[test]
    fn host_broadcasts_snapshot_client_caches_it() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 2, true); // host=0 + client1
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 16384];

        // 先让 client 发一次输入，host 登记该 client peer（否则 broadcast_snapshot 无人可广播）。
        cli.send_input(&encode_input(1)).unwrap();
        host.poll(&mut rcv);

        // host 广播一份快照（seq=10，字节任意确定性内容）。
        host.broadcast_snapshot(b"snap@10".to_vec(), 10);
        assert_eq!(host.current_snapshot(), Some(&(b"snap@10".to_vec(), 10)), "host 本地也应保存最新快照");

        // client 正常 step_frame 收包：应缓存该快照到 latest_snapshot，但 expect_seq 不变（不推进）。
        assert_eq!(cli.expect_seq(), 0);
        let _ = cli.step_frame(&mut rcv).unwrap(); // 消费到 Snapshot（无 Frame，返回 None 不推进）
        assert_eq!(cli.expect_seq(), 0, "缓存快照不得推进 expect_seq");
        let cached = cli.take_latest_snapshot().expect("client 应缓存 host 广播的快照");
        assert_eq!(cached, (b"snap@10".to_vec(), 10), "缓存内容与 host 广播一致");

        // 取走后清空；再广播新快照覆盖旧值。
        assert!(cli.take_latest_snapshot().is_none(), "take 后应清空");
        host.broadcast_snapshot(b"snap@11".to_vec(), 11);
        let _ = cli.step_frame(&mut rcv).unwrap();
        assert_eq!(cli.take_latest_snapshot(), Some((b"snap@11".to_vec(), 11)), "新快照覆盖旧缓存");
    }

    /// 「人不满也启动」：建房上限 3（host+2 client），但只有 client1 进场就绪；host 设参与集 `[client1 参与, client2 不参与]`，
    /// 产帧 / 就绪 / 配置判定仅对参与集要求，vacant 槽位排除在局外。锁死“人不满但全员（现有者）就绪也能启动”。
    #[test]
    fn participants_underfull_start_only_active() {
        let (ht, ct) = pair();
        let mut host = HostLockstep::new(ht, 3, true); // host=0 + client1 + client2（expected=2）
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 4096];

        // 未设参与集前（满员判定）：缺 client2 不能视为全在场、无法产帧。
        assert!(!host.saw_all_clients(), "满员判定：client2 缺席时不全在场");
        cli.send_input(&encode_input(1)).unwrap();
        host.poll(&mut rcv);
        host.set_local_input(Some(vec![9]));
        assert!(host.try_emit().is_none(), "满员判定下应因缺 client2 无法产帧");

        // 设参与集：只 client1 参与 → 只要求 client1。
        assert!(host.set_participants(&[true, false]));
        assert!(host.saw_all_clients(), "按参与集只在场的 client1 应视为全在场");
        assert_eq!(host.active_clients_count(), 1);

        // 产帧：只含 player0(host) + player1(client1)，不含 player2。
        let (seq, entries) = host.try_emit().expect("按参与集应能产帧");
        let players: Vec<u8> = entries.iter().map(|(i, _)| *i).collect();
        assert_eq!(seq, 0);
        assert_eq!(players, vec![0, 1], "产帧只应含参与玩家 host 与 client1");
        assert!(!players.contains(&2), "未参与的 client2 不应出现在帧里");

        // 就绪仅对参与集：client1 就绪 → all_clients_ready true（不要求 client2）。
        assert!(!host.all_clients_ready());
        cli.send_ready_state(true).unwrap();
        host.poll(&mut rcv);
        assert!(host.all_clients_ready(), "参与集就绪即可（client2 不参与不要求）");

        // 配置同步仅对参与集：host local cfg + client1 cfg → all_cfgs true（不要求 client2）。
        host.set_local_cfg(vec![0]);
        cli.send_cfg(&vec![1]).unwrap();
        let mut rcv2 = [0u8; 4096];
        host.poll_cfg(&mut rcv2);
        assert!(host.all_cfgs(), "参与集配置就绪即可（client2 不参与不要求）");
        let merged = host.collect_cfgs().unwrap();
        let players_cfg: Vec<u8> = merged.iter().map(|(i, _)| *i).collect();
        assert_eq!(players_cfg, vec![0, 1], "合并配置只应含参与玩家");
    }

    /// 稀疏参与收缩：只 host + client2 参与（client1 缺席），参与玩家【原 index】收缩为连续 [0,2]→new 0,1，
    /// 使 host/client 能以“本局参与数”建等量角色（不满员时两端角色数量一致）。
    #[test]
    fn participants_sparse_reindex() {
        let (ht, ct) = pair();
        // host=0 + client1(槽1) + client2(槽2)，只 client2(slot 1，orig player 2) 参与。
        let mut host = HostLockstep::new(ht, 3, true);
        // 模拟 client2（实际 slot=1，orig player 2）的 ClientLockstep（my_index=2）。
        let mut cli = ClientLockstep::new(ct, 2, Peer::Udp(std::net::SocketAddr::from(([127, 0, 0, 1], 4000))));
        let mut rcv = [0u8; 4096];

        assert!(host.set_participants(&[false, true])); // client1 不参与、client2 参与
        assert_eq!(host.participants_orig(), &[0, 2], "参与玩家原 index：host + client2");
        assert_eq!(host.participants_count(), 2);

        // 只给参与者输入（client2 上行 + host 本地）→ 应能产帧，且 new index 连续 [0,1]。
        cli.send_input(&encode_input(7)).unwrap();
        host.poll(&mut rcv);
        host.set_local_input(Some(vec![9]));
        let (_, entries) = host.try_emit().expect("按参与集应能产帧");
        let players: Vec<u8> = entries.iter().map(|(i, _)| *i).collect();
        assert_eq!(players, vec![0, 1], "产帧 new index 应收缩为连续 0,1（不含缺席的 player1）");

        // 配置同步只对参与集；broadcast 的 participants 也应反映收缩。
        host.set_local_cfg(vec![0]);
        cli.send_cfg(&vec![1]).unwrap();
        host.poll_cfg(&mut rcv);
        let merged = host.collect_cfgs().unwrap();
        assert_eq!(merged.len(), 2);
        let p: Vec<u8> = merged.iter().map(|(i, _)| *i).collect();
        assert_eq!(p, vec![0, 1]);
        host.broadcast_cfgs(&merged);
        let (got, parts) = cli.recv_cfg_all(&mut rcv).unwrap().expect("client 应收 PlayerCfgAll");
        assert_eq!(parts, vec![0, 2], "广播的参与列表应为原 index 收缩");
        assert_eq!(got.len(), 2);
    }
}

