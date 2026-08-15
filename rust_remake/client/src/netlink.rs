//! client 侧的联网连接（纯粹、无 ggez 依赖，可无头单测）。
//!
//! 一个 `NetLink` 对应一个“加入 host 的玩家窗口”：先 `join_handshake` 握手拿序号，再经
//! `ready` + `recv_go` 完成 READY/GO 统一起始，之后每帧用 `step_frame` 把本机 `PlayerInput`
//! 上行、收 host 广播的带 `seq` 整帧、推进本端 `World`。
//!
//! 关键约束：只有收到帧才推进（`step_frame` 返回 `None` 表示本帧未到，调用方不得盲扣时间/盲推进），
//! 且按收到帧的 `seq` 锚定推进——与 host 用同一帧回放，保证两端逐位一致。

use game_core::netcode::{decode_player_input, encode_player_input};
use game_core::world::{PlayerInput, World};
use net::session::ClientSession;
use net::transport::{Peer, StdUdpTransport};
use std::io;
use std::net::SocketAddr;

/// 联网客户端连接封装。
pub struct NetLink {
    session: ClientSession<StdUdpTransport>,
    host: Peer,
    rcv: Vec<u8>,
    /// 是否已收到 GO（统一起始）。
    pub started: bool,
    /// GO 携带的起始 seq。
    pub start_seq: u64,
}

impl NetLink {
    /// 绑定本端并向 `host` 加入（须与对端 `HostSession::poll_join` 协同，见 crate::netlink::tests）。
    pub fn connect(host: SocketAddr) -> io::Result<NetLink> {
        let (t, _) = StdUdpTransport::bind_loopback()?;
        Ok(NetLink {
            session: ClientSession::connected(t, 0, 0),
            host: Peer::Udp(host),
            rcv: vec![0u8; 4096],
            started: false,
            start_seq: 0,
        })
    }

    /// 握手：发 JOIN 并尝试收 ACK，直到拿到我的序号/人数。
    pub fn join_handshake(&mut self) -> io::Result<bool> {
        for _ in 0..100 {
            let _ = self.session.send_join(&self.host);
            if self.session.recv_join_ack(&mut self.rcv)? {
                return Ok(true);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(false)
    }

    /// 准备：向 host 上报 READY（表示已加入并准备好开始）。
    pub fn ready(&mut self) -> io::Result<()> {
        self.session.send_ready(&self.host)
    }

    /// 尝试收 GO：收到则记录 started+start_seq 并返回 true；未到返回 false。
    pub fn recv_go(&mut self) -> io::Result<bool> {
        if let Some(seq) = self.session.recv_go(&mut self.rcv)? {
            self.started = true;
            self.start_seq = seq;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 我的玩家序号（握手后有效）。
    pub fn my_index(&self) -> u8 {
        self.session.my_index
    }

    /// 总玩家人数（握手后有效）。
    pub fn player_count(&self) -> u8 {
        self.session.players
    }

    /// 每帧：把本机输入上行，收带 seq 的整帧；收到则推进 `world` 并返回 `Some(seq)`，
    /// 没收到帧返回 `None`（调用方本 tick 不应推进、不应盲扣 accumulator，以保持与 host 帧对齐）。
    pub fn step_frame(
        &mut self,
        my_input: &PlayerInput,
        world: &mut World,
        dt: game_core::fix::Fix64,
    ) -> io::Result<Option<u64>> {
        let enc = encode_player_input(my_input);
        self.session.send_input(&enc, &self.host)?;
        // 有界轮询收一帧；只推进一次（seq 锚定，避免重推）。
        for _ in 0..8 {
            if let Some((seq, entries)) = self.session.recv_frame(&mut self.rcv)? {
                let n = world.players.len();
                let mut inputs = vec![PlayerInput::default(); n];
                for (idx, bytes) in entries {
                    if (idx as usize) < n {
                        inputs[idx as usize] = decode_player_input(&bytes).map_err(io::Error::other)?;
                    }
                }
                world.step(inputs, dt);
                return Ok(Some(seq));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::fix::{Fix64, Vec2};
    use game_core::player::Cmd;
    use game_core::skill::SkillId;
    use net::session::HostSession;
    use net::transport::StdUdpTransport;
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

    /// 无头：host + 两个 NetLink（客户端逻辑）跑真 UDP，验证各端 World 一致。
    #[test]
    fn two_client_links_stay_synced() {
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut host = HostSession::new(ht, 2);
        let mut rcv = [0u8; 4096];
        let host_peer = Peer::Udp(host_addr);

        // 建两个 NetLink 并握手（交替驱动：client 发 JOIN，host poll_join，client 收 ACK）
        let mut a = NetLink::connect(host_addr).unwrap();
        let mut b = NetLink::connect(host_addr).unwrap();
        for _ in 0..100 {
            let _ = a.session.send_join(&a.host);
            let _ = b.session.send_join(&b.host);
            host.poll_join(&mut rcv);
            if a.session.recv_join_ack(&mut rcv).unwrap_or(false)
                && b.session.recv_join_ack(&mut rcv).unwrap_or(false)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        // READY + GO
        for _ in 0..100 {
            let _ = a.ready();
            let _ = b.ready();
            host.poll_ready(&mut rcv);
            if host.all_ready() {
                host.broadcast_go();
            }
            if a.recv_go().unwrap() && b.recv_go().unwrap() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(a.started && b.started, "两端都应收 GO");
        assert_eq!(a.my_index(), 0);
        assert_eq!(b.my_index(), 1);

        let mut wa = World::new(2, 55);
        let mut wb = World::new(2, 55);
        let dt = Fix64::from_num(1.0 / 60.0);
        let mut stepped = 0u32;
        for _ in 0..120 {
            let ia = sample_input();
            let ib = sample_input();
            // host 收集：每轮重发 a/b 输入，直到 collect_inputs 收齐两端（等齐门槛）。
            let (fseq, entries) = loop {
                a.session.send_input(&encode_player_input(&ia), &host_peer).unwrap();
                b.session.send_input(&encode_player_input(&ib), &host_peer).unwrap();
                if let Some(f) = host.collect_inputs(&mut rcv) {
                    break f;
                }
                std::thread::sleep(Duration::from_millis(1));
            };
            assert_eq!(
                entries.len(),
                2,
                "合帧应含两端输入（实际 len={}, idxs={:?}）",
                entries.len(),
                entries.iter().map(|(p, _)| *p).collect::<Vec<_>>()
            );
            host.broadcast_frame(fseq, &entries);
            if a.step_frame(&ia, &mut wa, dt).unwrap().is_some()
                && b.step_frame(&ib, &mut wb, dt).unwrap().is_some()
            {
                stepped += 1;
            }
            assert_eq!(wa.players, wb.players, "两端 World 必须一致");
        }
        assert!(stepped > 0, "应至少真实推进过 1 帧（防网络静默丢弃导致的假通过）");
        assert_ne!(wa.players, World::new(2, 55).players, "联网输入应真实作用于 World（防假通过）");
    }

    /// 完整一局：host 作为 player0(残血冲刺出圈死亡)，一个 NetLink 作为 player1(存活)。
    /// 跑到底，验证 host 自身 World 与 client World 全程一致、且能分出胜者。
    #[test]
    fn full_round_host_and_client_consistent_with_winner() {
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut host = HostSession::new(ht, 1);
        host.host_participates(2); // host=player0, 1 名 client=player1
        let host_peer = Peer::Udp(host_addr);
        let mut rcv = [0u8; 4096];
        let mut client = NetLink::connect(host_addr).unwrap();
        // 握手（交替驱动：client JOIN，host poll，client 收 ACK）
        for _ in 0..100 {
            let _ = client.session.send_join(&client.host);
            host.poll_join(&mut rcv);
            if client.session.recv_join_ack(&mut rcv).unwrap_or(false) {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(client.my_index(), 1);
        // READY + GO
        for _ in 0..100 {
            let _ = client.ready();
            host.poll_ready(&mut rcv);
            if host.all_ready() {
                host.broadcast_go();
            }
            if client.recv_go().unwrap() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(client.started);

        let mut whost = World::new(2, 77);
        let mut wcli = World::new(2, 77);
        whost.players[0].hp = Fix64::ONE;
        wcli.players[0].hp = Fix64::ONE;
        let dt = Fix64::from_num(1.0 / 60.0);
        let p0_in = PlayerInput {
            cast: Some((SkillId::DashSlash, Some(Vec2::new(Fix64::from_num(1000.0), Fix64::from_num(1000.0))))),
            ..Default::default()
        };
        let p1_in = PlayerInput::default();

        let mut ticks = 0;
        while !wcli.round_over() && ticks < 600 {
            // host 收集（等齐 p0 local + p1）并带 seq 推进+广播。
            host.set_local_input(Some(encode_player_input(&p0_in)));
            client.session.send_input(&encode_player_input(&p1_in), &host_peer).unwrap();
            let (fseq, frame) = loop {
                if let Some(f) = host.collect_inputs(&mut rcv) {
                    break f;
                }
                std::thread::sleep(Duration::from_millis(1));
            };
            let n = whost.players.len();
            let mut ins = vec![PlayerInput::default(); n];
            for (idx, bytes) in &frame {
                if (*idx as usize) < n {
                    ins[*idx as usize] = decode_player_input(bytes).ok().unwrap_or_default();
                }
            }
            whost.step(ins, dt);
            host.broadcast_frame(fseq, &frame);
            // client 只推进一次（收到带 seq 帧）。
            let stepped = client.step_frame(&p1_in, &mut wcli, dt).unwrap().is_some();
            assert!(stepped, "tick {} client 应收到并推进本帧", ticks);
            assert_eq!(whost.players, wcli.players, "tick {} host 与 client World 应一致", ticks);
            assert_eq!(whost.arena_radius, wcli.arena_radius);
            ticks += 1;
        }
        assert!(wcli.round_over(), "对局应能打完（出局者死亡）");
        assert!(!wcli.players[0].alive, "出界者(player0)应死亡");
        assert_eq!(whost.placement(), wcli.placement(), "两端名次判定一致");
        assert_eq!(whost.placement()[0], 1, "胜者应为 player1");
    }
}
