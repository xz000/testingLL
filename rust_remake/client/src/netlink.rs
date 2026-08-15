//! client 侧的联网连接（纯粹、无 ggez 依赖，可无头单测）。
//!
//! 流程：`join_handshake`(握手拿序号，内部移交 transport 到 ClientLockstep) →
//! 每帧 `upload` 持续上行输入 + `step_frame` 收带 seq 帧推进（首帧即开始，丢帧自动请求补发）。
//! 只有收到帧才推进（`step_frame` 返回 `None` 表示本帧未到，调用方不得盲扣时间/盲推进）。

use game_core::netcode::decode_player_input;
use game_core::world::{PlayerInput, World};
use net::handshake::ClientHandshake;
use net::lockstep::ClientLockstep;
use net::transport::{Peer, StdUdpTransport};
use std::io;
use std::net::SocketAddr;

/// 联网客户端连接封装。
pub struct NetLink {
    /// 握手阶段持有（transport 在其内）。
    handshake: Option<ClientHandshake<StdUdpTransport>>,
    /// 运行阶段持有（transport 移交于此）。
    lockstep: Option<ClientLockstep<StdUdpTransport>>,
    host: Peer,
    rcv: Vec<u8>,
    pub started: bool,
    my_index: u8,
    players: u8,
}

impl NetLink {
    pub fn connect(host: SocketAddr) -> io::Result<NetLink> {
        let (t, _) = StdUdpTransport::bind_loopback()?;
        Ok(NetLink {
            handshake: Some(ClientHandshake::connected(t)),
            lockstep: None,
            host: Peer::Udp(host),
            rcv: vec![0u8; 4096],
            started: false,
            my_index: 0,
            players: 0,
        })
    }

    /// 握手：发 JOIN 并尝试收 ACK，直到拿到序号/人数；一旦拿到，立即把 transport 移交给
    /// ClientLockstep（此后 client 可持续上行输入，host 收齐输入即可产首帧＝统一起始）。
    pub fn join_handshake(&mut self) -> io::Result<bool> {
        for _ in 0..100 {
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
                return Ok(true);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(false)
    }

    pub fn my_index(&self) -> u8 {
        self.my_index
    }

    pub fn player_count(&self) -> u8 {
        self.players
    }

    /// 持续上行本机输入（无论是否 started）。host 靠收齐输入产首帧，从而自然统一起始。
    pub fn upload(&mut self, encoded: &[u8]) -> io::Result<()> {
        let Some(ls) = self.lockstep.as_mut() else {
            return Ok(());
        };
        ls.send_input(encoded)
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
        let n = world.players.len();
        match ls.step_frame(&mut self.rcv)? {
            Some(entries) => {
                if !self.started {
                    self.started = true;
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
    use net::lockstep::HostLockstep;
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

    /// 无头端到端：host(参与=player0) + 两个 NetLink，真 UDP 跑若干帧，验证三端 World 逐位一致。
    /// 采用“首帧即开始”：host 收齐各端输入后产首帧，client 收到即推进（无需 GO/READY）。
    #[test]
    fn host_and_two_clients_sync_over_udp() {
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut hs = HostHandshake::new(ht, 3, true); // host=0 + 2 client
        let mut a = NetLink::connect(host_addr).unwrap();
        let mut b = NetLink::connect(host_addr).unwrap();
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
}
