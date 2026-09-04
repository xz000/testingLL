//! client 侧的联网连接（纯粹、无 ggez 依赖，可无头单测）。
//!
//! 流程：`join_handshake`(握手拿序号，内部移交 transport 到 ClientLockstep) →
//! 每帧 `upload` 持续上行输入 + `step_frame` 收带 seq 帧推进（首帧即开始，丢帧自动请求补发）。
//! 只有收到帧才推进（`step_frame` 返回 `None` 表示本帧未到，调用方不得盲扣时间/盲推进）。
#![allow(clippy::type_complexity)] // recv_cfg_all 等返回的复杂元组：协议固有，允许。

use game_core::netcode::decode_player_input;
use game_core::world::{PlayerInput, World};
use net::handshake::ClientHandshake;
use net::lockstep::ClientLockstep;
use net::transport::{Peer, StdUdpTransport, Transport};
use std::io;
use std::net::SocketAddr;

/// 生成本进程内近乎唯一的稳定身份（u64 占位）：时间戳异或 pid。Steam 将来直接换成 SteamID，不走这里。
fn random_identity() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let pid = std::process::id() as u64;
    (nanos ^ pid.wrapping_mul(0x9E3779B97F4A7C15)).max(1)
}

/// 联网客户端连接封装（**传输无关**：`T: Transport` 决定底层是 UDP 还是 Steam）。
/// `NetLink` 只按 `Peer` 收发/判等，不关心具体传输；换底层（UDP→Steam）只需换 `T` 与注入方式。
/// 局域网用 `connect_udp`；Steam 将来用 `from_transport` 注入 SteamTransport。
pub struct NetLink<T: Transport> {
    /// 握手阶段持有（transport 在其内）。
    handshake: Option<ClientHandshake<T>>,
    /// 运行阶段持有（transport 移交于此）。
    lockstep: Option<ClientLockstep<T>>,
    host: Peer,
    /// 本端稳定身份（u64：Steam 未来=SteamID；局域网=随机占位）。握手时上报、ACK 回显。
    identity: u64,
    rcv: Vec<u8>,
    pub started: bool,
    my_index: u8,
    players: u8,
    /// 连续未收到权威帧的 tick 计数（用于判断是否掉线/可重连）。每次成功收到帧时清零。
    stale_ticks: u64,
}

pub type NetLinkUdp = NetLink<StdUdpTransport>;

impl NetLinkUdp {
    /// 局域网（UDP）便捷构造：绑定本机随机端口，以 `Peer::Udp(host)` 为 host 端点，随机稳定身份占位。
    pub fn connect_udp(host: SocketAddr) -> io::Result<NetLinkUdp> {
        let identity = random_identity();
        NetLinkUdp::connect_udp_with(host, identity)
    }

    /// 局域网（UDP）+ 指定稳定身份（如昵称 hash / 玩家自定义 id）。
    pub fn connect_udp_with(host: SocketAddr, identity: u64) -> io::Result<NetLinkUdp> {
        let (t, _) = StdUdpTransport::bind_loopback()?;
        NetLink::from_transport(t, Peer::Udp(host), identity)
    }
}

impl<T: Transport> NetLink<T> {
    /// 传输无关构造：指定稳定身份（Steam 传 SteamID；局域网传昵称 hash/自定义 id）。
    pub fn from_transport(transport: T, host: Peer, identity: u64) -> io::Result<NetLink<T>> {
        Ok(NetLink {
            handshake: Some(ClientHandshake::connected_with(transport, identity)),
            lockstep: None,
            host,
            identity,
            rcv: vec![0u8; 4096],
            started: false,
            my_index: 0,
            players: 0,
            stale_ticks: 0,
        })
    }

    /// 本端稳定身份。
    pub fn my_identity(&self) -> u64 {
        self.identity
    }

    /// 握手：发 JOIN 并尝试收 ACK，直到拿到序号/人数；一旦拿到，立即把 transport 移交给
    /// ClientLockstep（此后 client 可持续上行输入，host 收齐输入即可产首帧＝统一起始）。
    pub fn join_handshake(&mut self) -> io::Result<bool> {
        for i in 0..100 {
            let Some(hs) = self.handshake.as_mut() else {
                return Ok(true);
            };
            hs.send_join(&self.host)?;
            if hs.recv_join_ack(&mut self.rcv)? {
                self.my_index = hs.my_index;
                self.players = hs.players;
                // 移交 transport → lockstep。
                let transport = self.handshake.take().unwrap().into_transport();
                self.lockstep = Some(ClientLockstep::new(transport, self.my_index, self.host));
                eprintln!("[netlink] join_handshake OK: my_index={} players={} (attempt {i})", self.my_index, self.players);
                return Ok(true);
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        eprintln!("[netlink] join_handshake TIMEOUT: no ACK after 100 attempts");
        Ok(false)
    }

    pub fn my_index(&self) -> u8 {
        self.my_index
    }

    pub fn player_count(&self) -> u8 {
        self.players
    }

    /// 由调用方在「本 tick 未收到权威帧」时累计，用于探测掉线。
    pub fn bump_stale(&mut self) {
        self.stale_ticks += 1;
    }

    /// 距上次成功收到权威帧的连续 tick 数。
    pub fn stale_ticks(&self) -> u64 {
        self.stale_ticks
    }

    /// 掉线后发起重连：向 host 发 `ReconnectReq`，尝试收 `Snapshot`。
    /// 返回 `Some((world_bytes, seq))`＝拿到整场快照（调用方据此重建 World + 对齐基线）；`None`＝尚未收到。
    pub fn try_reconnect(&mut self) -> io::Result<Option<(Vec<u8>, u64)>> {
        let Some(ls) = self.lockstep.as_mut() else {
            return Ok(None);
        };
        ls.send_reconnect_req(self.identity)?;
        ls.recv_snapshot(&mut self.rcv)
    }

    /// 重连拿到快照后，把本端基线对齐到快照 seq（处理 host 广播的 `Resync`）。
    pub fn align_after_reconnect(&mut self) -> io::Result<bool> {
        let Some(ls) = self.lockstep.as_mut() else {
            return Ok(false);
        };
        ls.apply_resync(&mut self.rcv)
    }

    /// 持续上行本机输入（无论是否 started）。host 靠收齐输入产首帧，从而自然统一起始。
    pub fn upload(&mut self, encoded: &[u8]) -> io::Result<()> {
        let Some(ls) = self.lockstep.as_mut() else {
            return Ok(());
        };
        ls.send_input(encoded)
    }

    /// 上报本玩家最终配置（学习阶段结束/就绪时；载荷为 `PlayerConfig::encode()` 字节）。
    pub fn upload_cfg(&mut self, cfg_bytes: &[u8]) -> io::Result<()> {
        let Some(ls) = self.lockstep.as_mut() else {
            return Ok(());
        };
        ls.send_cfg(cfg_bytes)
    }

    /// 尝试收 host 广播的 `PlayerCfgAll`（所有玩家完整配置）；当前没有则返回 None。
    /// 局域网为满员，故舍弃 participants（收缩仅 Steam 不满员用）。
    pub fn recv_cfg_all(&mut self) -> io::Result<Option<Vec<(u8, Vec<u8>)>>> {
        let Some(ls) = self.lockstep.as_mut() else {
            return Ok(None);
        };
        Ok(ls.recv_cfg_all(&mut self.rcv)?.map(|(e, _parts)| e))
    }

    /// 每帧：只收带 seq 帧并推进 `world`。收到并推进返回 `Some(seq)`，未到返回 `None`。
    /// 首次收到帧时自动置 `started`（首帧即统一起始信号）。上行由调用方逐帧 `upload`。
    pub fn step_frame(
        &mut self,
        world: &mut World,
        dt: game_core::fix::Fix64,
    ) -> io::Result<Option<u64>> {
        let Some(ls) = self.lockstep.as_mut() else {
            return Ok(None);
        };
        let expect_before = ls.expect_seq();
        let n = world.players.len();
        match ls.step_frame(&mut self.rcv)? {
            Some(entries) => {
                self.stale_ticks = 0;
                let became_started = !self.started && { self.started = true; true };
                if became_started {
                    eprintln!("[netlink] FIRST FRAME: started, expect_seq->{expect_before}");
                }
                let mut inputs = vec![PlayerInput::default(); n];
                for (idx, bytes) in entries {
                    if (idx as usize) < n {
                        inputs[idx as usize] = decode_player_input(&bytes).map_err(io::Error::other)?;
                    }
                }
                world.step(inputs, dt);
                Ok(Some(ls.expect_seq() - 1))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::fix::{Fix64, Vec2};
    use game_core::netcode::encode_player_input;
    use game_core::player::Cmd;
    use game_core::skill::SkillId;
    use net::handshake::HostHandshake;
    use net::lockstep::{ClientLockstep, HostLockstep};
    use net::transport::{Peer, StdUdpTransport};
    use std::time::Duration;

    fn sample_input() -> PlayerInput {
        PlayerInput {
            set_target: Some(Vec2::new(Fix64::from_num(3.0), Fix64::ZERO)),
            cast: Some((SkillId::Rock, Some(Vec2::new(Fix64::from_num(6.0), Fix64::ZERO)))),
            queued: vec![Cmd::Move(Vec2::new(Fix64::from_num(4.0), Fix64::ZERO))],
            clear_queue: false,
            stop_move: false,
        }
    }

    /// 无头端到端：host(参与=player0) + 两个 NetLink，真 UDP 跑若干帧，验证三端 World 逐位一致。
    /// 采用“首帧即开始”：host 收齐各端输入后产首帧，client 收到即推进（无需 GO/READY）。
    #[test]
    fn host_and_two_clients_sync_over_udp() {
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut hs = HostHandshake::new(ht, 3, true); // host=0 + 2 client
        let mut a = NetLink::connect_udp(host_addr).unwrap();
        let mut b = NetLink::connect_udp(host_addr).unwrap();
        let mut rcv = [0u8; 8192];

        // 握手（client join_handshake 内部移交 transport → lockstep；host poll_join 收加入）
        for _ in 0..100 {
            let _ = a.handshake.as_mut().unwrap().send_join(&a.host);
            let _ = b.handshake.as_mut().unwrap().send_join(&b.host);
            hs.poll_join(&mut rcv);
            if a.join_handshake().unwrap() && b.join_handshake().unwrap() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(a.my_index(), 1);
        assert_eq!(b.my_index(), 2);
        assert!(hs.joined >= hs.expected(), "host 应收齐 2 client");

        // 运行：host 移交 transport → HostLockstep；三端各持同 seed World。
        let mut host = HostLockstep::new(hs.into_transport(), 3, true);
        let mut whost = World::new(3, 55);
        let mut wa = World::new(3, 55);
        let mut wb = World::new(3, 55);
        let dt = Fix64::from_num(1.0 / 60.0);
        let mut stepped = 0u32;

        for _ in 0..120 {
            // 各端持续上行输入（client 无需先等首帧/GO）
            a.upload(&encode_player_input(&sample_input())).unwrap();
            b.upload(&encode_player_input(&sample_input())).unwrap();
            host.set_local_input(Some(encode_player_input(&sample_input())));
            host.poll(&mut rcv);
            if let Some((_seq, frame)) = host.try_emit() {
                let mut in_h = vec![PlayerInput::default(); 3];
                for (idx, bytes) in &frame {
                    in_h[*idx as usize] = decode_player_input(bytes).unwrap();
                }
                whost.step(in_h, dt);
            }
            // 两端收帧推进（收不到返回 None，不推进）
            let _ = a.step_frame(&mut wa, dt).unwrap();
            let _ = b.step_frame(&mut wb, dt).unwrap();
            if a.started && host.next_seq() > 0 {
                stepped += 1;
            }
            // 三端应逐位一致（若 host 已产帧并广播到两端）
            assert_eq!(whost.players, wa.players, "@@@ host 与 a 应一致 (expect={})", a.lockstep.as_ref().map(|l| l.expect_seq()).unwrap_or(0));
            assert_eq!(whost.players, wb.players, "host 与 b 应一致");
        }
        assert!(stepped > 0, "应至少推进过（防假绿）");
        assert_ne!(whost.players, World::new(3, 55).players, "输入应真实作用于 World");
    }

    /// 把配置条目 `(player_index, PlayerConfig 编码)` 应用到 World 的 `skill_levels`。
    fn apply_cfgs(world: &mut World, entries: &[(u8, Vec<u8>)]) {
        for (idx, bytes) in entries {
            if let Some(cfg) = game_core::progress::PlayerConfig::decode(bytes) {
                if let Some(p) = world.players.get_mut(*idx as usize) {
                    for i in 0..p.skill_levels.len().min(cfg.skill_levels.len()) {
                        p.skill_levels[i] = cfg.skill_levels[i];
                    }
                }
            }
        }
    }

    /// 构造一个只有 `skills[slot] = level`、其余 1 的 PlayerConfig 编码。
    fn make_cfg(slot: usize, level: u32) -> Vec<u8> {
        use game_core::progress::PlayerConfig;
        let mut levels = vec![1u32; 34];
        levels[slot] = level;
        PlayerConfig { skill_levels: levels, key_slots: [None; 8], gold: 0, gold_spent: 0, attributes: game_core::attribute::Attributes::default(), growth_points: 0 , items: Vec::new() }.encode()
    }

    /// 端到端多局：host+2 client 第一局跑帧 → 学习+配置同步（host 收齐广播）→ 各端重建下一局
    /// World（应用同步后的技能等级）→ 第二局继续锁步 → 三端逐位一致、技能等级反映升级。
    /// 这模拟 Dota2 式 learning→ready→下一局，证明跨局技能配置不会造成分叉。
    #[test]
    fn meta_round_sync_keeps_worlds_identical() {
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut hs = HostHandshake::new(ht, 3, true);
        let mut a = NetLink::connect_udp(host_addr).unwrap();
        let mut b = NetLink::connect_udp(host_addr).unwrap();
        let mut rcv = [0u8; 8192];

        for _ in 0..100 {
            let _ = a.handshake.as_mut().unwrap().send_join(&a.host);
            let _ = b.handshake.as_mut().unwrap().send_join(&b.host);
            hs.poll_join(&mut rcv);
            if a.join_handshake().unwrap() && b.join_handshake().unwrap() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(a.my_index(), 1);
        assert_eq!(b.my_index(), 2);

        let mut host = HostLockstep::new(hs.into_transport(), 3, true);
        let mut whost = World::new(3, 55);
        let mut wa = World::new(3, 55);
        let mut wb = World::new(3, 55);
        let dt = Fix64::from_num(1.0 / 60.0);

        // 第一局：跑 30 帧。
        for _ in 0..30 {
            a.upload(&encode_player_input(&sample_input())).unwrap();
            b.upload(&encode_player_input(&sample_input())).unwrap();
            host.set_local_input(Some(encode_player_input(&sample_input())));
            host.poll(&mut rcv);
            if let Some((_, frame)) = host.try_emit() {
                let mut in_h = vec![PlayerInput::default(); 3];
                for (idx, bytes) in &frame {
                    in_h[*idx as usize] = decode_player_input(bytes).unwrap();
                }
                whost.step(in_h, dt);
            }
            let _ = a.step_frame(&mut wa, dt).unwrap();
            let _ = b.step_frame(&mut wb, dt).unwrap();
            assert_eq!(whost.players, wa.players, "第一局 host 与 a 应一致");
            assert_eq!(whost.players, wb.players, "第一局 host 与 b 应一致");
        }

        // 学习结束：各端生成自己的升级配置（本机操作，各不相问）。
        let host_cfg = make_cfg(0, 5); // host(玩家0)把技能0升到5
        let cfg_a = make_cfg(1, 3);    // client A(玩家1)把技能1升到3
        let cfg_b = make_cfg(2, 4);    // client B(玩家2)把技能2升到4
        host.set_local_cfg(host_cfg.clone());
        a.upload_cfg(&cfg_a).unwrap();
        b.upload_cfg(&cfg_b).unwrap();
        host.poll_cfg(&mut rcv);
        assert!(host.all_cfgs(), "host 应收齐自身+两端配置");
        let all = host.collect_cfgs().expect("收齐");
        host.broadcast_cfgs(&all);
        let got_a = a.recv_cfg_all().unwrap().expect("client A 应收到完整配置");
        let got_b = b.recv_cfg_all().unwrap().expect("client B 应收到完整配置");
        assert_eq!(all, got_a, "host/端A 应持有相同完整配置");
        assert_eq!(all, got_b, "host/端B 应持有相同完整配置");

        // 重建下一局 World（同 seed），并应用同步后的技能等级。
        let mut w2host = World::new(3, 55);
        let mut w2a = World::new(3, 55);
        let mut w2b = World::new(3, 55);
        apply_cfgs(&mut w2host, &all);
        apply_cfgs(&mut w2a, &all);
        apply_cfgs(&mut w2b, &all);
        // 技能等级应反映升级：玩家0技能0=5、玩家1技能1=3、玩家2技能2=4（所有端一致）。
        assert_eq!(w2host.players[0].skill_levels[0], 5);
        assert_eq!(w2a.players[0].skill_levels[0], 5);
        assert_eq!(w2a.players[1].skill_levels[1], 3);
        assert_eq!(w2b.players[2].skill_levels[2], 4);
        host.reset_cfgs(); // 下一局复用

        // 第二局：继续锁步跑 30 帧，三端逐位一致（分叉即失败）。
        for _ in 0..30 {
            a.upload(&encode_player_input(&sample_input())).unwrap();
            b.upload(&encode_player_input(&sample_input())).unwrap();
            host.set_local_input(Some(encode_player_input(&sample_input())));
            host.poll(&mut rcv);
            if let Some((_, frame)) = host.try_emit() {
                let mut in_h = vec![PlayerInput::default(); 3];
                for (idx, bytes) in &frame {
                    in_h[*idx as usize] = decode_player_input(bytes).unwrap();
                }
                w2host.step(in_h, dt);
            }
            let _ = a.step_frame(&mut w2a, dt).unwrap();
            let _ = b.step_frame(&mut w2b, dt).unwrap();
            assert_eq!(w2host.players, w2a.players, "第二局 host 与 a 应逐位一致");
            assert_eq!(w2host.players, w2b.players, "第二局 host 与 b 应逐位一致");
        }
    }

    /// 重连垂直切片（切片2）：client 掉线 → host 用默认输入继续 → 存(World,seq)快照 →
    /// 重连者用快照重建 World + set_start_seq 接回 → 继续跑，host 与重连端逐位一致。
    #[test]
    fn reconnect_after_drop_via_snapshot() {
        let (ht, host_addr) = net::transport::StdUdpTransport::bind_loopback().unwrap();
        let (ct, _caddr) = net::transport::StdUdpTransport::bind_loopback().unwrap();
        let mut host = HostLockstep::new(ht, 2, true); // host=0 + client1
        let mut cli = ClientLockstep::new(ct, 1, Peer::Udp(host_addr));
        let mut rcv = [0u8; 8192];
        let dt = Fix64::from_num(1.0 / 60.0);
        let mut whost = World::new(2, 55);
        let mut wcli = World::new(2, 55);

        // A 段：先建立两端一致。
        for _ in 0..30 {
            cli.send_input(&encode_player_input(&sample_input())).unwrap();
            host.poll(&mut rcv);
            host.set_local_input(Some(encode_player_input(&sample_input())));
            if let Some((_, frame)) = host.try_emit() {
                let mut ins = vec![PlayerInput::default(); 2];
                for (idx, b) in &frame { ins[*idx as usize] = decode_player_input(b).unwrap(); }
                whost.step(ins, dt);
            }
            if let Some(ents) = cli.step_frame(&mut rcv).unwrap() {
                let mut ins = vec![PlayerInput::default(); 2];
                for (idx, b) in &ents { ins[*idx as usize] = decode_player_input(b).unwrap(); }
                wcli.step(ins, dt);
            }
            assert_eq!(whost.players, wcli.players, "掉线前两端应一致");
        }

        // 掉线：client 停，host 用默认输入继续推进。
        host.mark_dropped(1);
        for _ in 0..20 {
            host.set_local_input(Some(encode_player_input(&sample_input())));
            if let Some((_, frame)) = host.try_emit() {
                let mut ins = vec![PlayerInput::default(); 2];
                for (idx, b) in &frame { ins[*idx as usize] = decode_player_input(b).unwrap(); }
                whost.step(ins, dt);
            }
        }

        // 快照：host 当前 World -> 字节 -> 封装 Packet::Snapshot（模拟真实回传）。
        let seq_at = host.next_seq();
        let world_bytes = game_core::world_ser::world_to_bytes(&whost);
        let snap_pkt = net::Packet::Snapshot { world_bytes, seq: seq_at };
        let snap_bytes = snap_pkt.encode();
        let snap_back = net::Packet::decode(&snap_bytes).expect("Snapshot 应可编解码");

        // 重连：恢复 active；重连者从字节重建 World 并接到当前 seq。
        host.unmark_dropped(1);
        let world_bytes = match snap_back {
            net::Packet::Snapshot { world_bytes, seq } => {
                cli.set_start_seq(seq);
                world_bytes
            }
            _ => panic!("应收到 Snapshot"),
        };
        wcli = game_core::world_ser::world_from_bytes(&world_bytes).expect("应能从字节重建 World");

        // B 段：重连后继续跑，host 与重连端逐位一致。
        for _ in 0..30 {
            cli.send_input(&encode_player_input(&sample_input())).unwrap();
            host.poll(&mut rcv);
            host.set_local_input(Some(encode_player_input(&sample_input())));
            if let Some((_, frame)) = host.try_emit() {
                let mut ins = vec![PlayerInput::default(); 2];
                for (idx, b) in &frame { ins[*idx as usize] = decode_player_input(b).unwrap(); }
                whost.step(ins, dt);
            }
            if let Some(ents) = cli.step_frame(&mut rcv).unwrap() {
                let mut ins = vec![PlayerInput::default(); 2];
                for (idx, b) in &ents { ins[*idx as usize] = decode_player_input(b).unwrap(); }
                wcli.step(ins, dt);
            }
            assert_eq!(whost.players, wcli.players, "重连后两端应逐位一致");
        }
    }

    /// 端到端重连（真实 UDP，走 NetLink 高层 API）：client 掉线 → host 继续推进 + save 快照 →
    /// client 用 `try_reconnect` 拉快照、`align_after_reconnect` 对齐 → 重建 World → 继续跑，host 与重连端逐位一致。
    #[test]
    fn netlink_reconnect_flow_resumes_identical_worlds() {
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut hs = HostHandshake::new(ht, 2, true); // host=0 + client1
        let mut cli = NetLink::connect_udp(host_addr).unwrap();
        let mut rcv = [0u8; 16384];
        let dt = Fix64::from_num(1.0 / 60.0);
        let mut whost = World::new(2, 55);
        let mut wcli = World::new(2, 55);
        let mut progressed = 0u32;

        // 握手：client 拿序号（my_index=1），host 收齐后移交 HostLockstep。
        for _ in 0..100 {
            let _ = cli.handshake.as_mut().unwrap().send_join(&cli.host);
            hs.poll_join(&mut rcv);
            if cli.join_handshake().unwrap() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(cli.my_index(), 1);
        assert!(hs.joined >= hs.expected(), "host 应收齐 1 client");
        let mut host = HostLockstep::new(hs.into_transport(), 2, true); // host=0 + client1

        // A 段：正常跑 30 帧，两端一致。
        for _ in 0..30 {
            cli.upload(&encode_player_input(&sample_input())).unwrap();
            host.poll(&mut rcv);
            host.set_local_input(Some(encode_player_input(&sample_input())));
            if let Some((_, frame)) = host.try_emit() {
                let mut ins = vec![PlayerInput::default(); 2];
                for (idx, b) in &frame { ins[*idx as usize] = decode_player_input(b).unwrap(); }
                whost.step(ins, dt);
            }
            if cli.step_frame(&mut wcli, dt).unwrap().is_some() {
                progressed += 1;
            }
            assert_eq!(whost.players, wcli.players, "A 段两端应一致");
        }
        assert!(progressed > 0, "A 段应推进过（防假绿）");

        // 掉线：client 停，host 用默认输入继续；并周期保存快照（快照 seq = 下一帧号）。
        host.mark_dropped(1);
        for i in 0..25u8 {
            host.set_local_input(Some(encode_player_input(&sample_input())));
            if host.try_emit().is_some() {
                host.set_snapshot(game_core::world_ser::world_to_bytes(&whost), host.next_seq());
            }
            // 每帧喂世界（用默认占位输入：掉线端原地）。
            let mut ins = vec![PlayerInput::default(); 2];
            ins[0] = sample_input();
            whost.step(ins, dt);
            let _ = i;
        }

        // 重连：client 通过 NetLink 拉快照。
        let _ = cli.try_reconnect().unwrap(); // 首次：发出 ReconnectReq，host 尚未应答
        host.poll(&mut rcv); // host 处理 ReconnectReq -> 回 Snapshot 并广播 Resync
        let (wb, seq) = cli.try_reconnect().unwrap().expect("应能拉到 Snapshot");

        // 对齐基线（处理 Resync）。
        let applied = cli.align_after_reconnect().unwrap();
        assert!(applied, "重连端应收到 Resync 并应用到快照 seq");
        assert_eq!(cli.lockstep.as_ref().unwrap().expect_seq(), seq, "对齐后基线应为快照 seq");

        // 重建 World：从快照字节重建（与 host 当前 World 状态一致的未来起点状态）。
        wcli = game_core::world_ser::world_from_bytes(&wb).expect("快照可解码为 World");
        // host 端 World 也应重建到同一快照（此处 host 一直用 whost 推进；为严格对照，把 host 也重置到快照）。
        whost = game_core::world_ser::world_from_bytes(&wb).unwrap();

        // B 段：重连后继续跑，host 与重连端逐位一致。
        let mut resumed = 0u32;
        for _ in 0..30 {
            cli.upload(&encode_player_input(&sample_input())).unwrap();
            host.poll(&mut rcv);
            host.set_local_input(Some(encode_player_input(&sample_input())));
            if let Some((_, frame)) = host.try_emit() {
                let mut ins = vec![PlayerInput::default(); 2];
                for (idx, b) in &frame { ins[*idx as usize] = decode_player_input(b).unwrap(); }
                whost.step(ins, dt);
            }
            if cli.step_frame(&mut wcli, dt).unwrap().is_some() {
                resumed += 1;
            }
            assert_eq!(whost.players, wcli.players, "重连后两端应逐位一致");
        }
        assert!(resumed > 0, "重连后应继续推进（防假绿）");
    }
}
