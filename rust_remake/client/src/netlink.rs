//! client 侧的联网连接（纯粹、无 ggez 依赖，可无头单测）。
//!
//! 流程：`join_handshake`(ClientHandshake 握手拿序号) → `ready`+`recv_go`(统一起始) →
//! `step_frame`(ClientLockstep 按 seq 严格推进，丢帧自动请求补发)。
//! 只有收到帧才推进（`step_frame` 返回 `None` 表示本帧未到，调用方不得盲扣时间/盲推进）。

use game_core::netcode::{decode_player_input, encode_player_input};
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
    pub start_seq: u64,
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
            start_seq: 0,
            my_index: 0,
            players: 0,
        })
    }

    /// 握手：发 JOIN 并尝试收 ACK，直到拿到序号/人数。
    pub fn join_handshake(&mut self) -> io::Result<bool> {
        for _ in 0..100 {
            let Some(hs) = self.handshake.as_mut() else {
                return Ok(true);
            };
            hs.send_join(&self.host)?;
            if hs.recv_join_ack(&mut self.rcv)? {
                self.my_index = hs.my_index;
                self.players = hs.players;
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

    /// 上报 READY。
    pub fn ready(&mut self) -> io::Result<()> {
        let Some(hs) = self.handshake.as_mut() else {
            return Ok(());
        };
        hs.send_ready(&self.host)
    }

    /// 收 GO：收到则记录 started+start_seq，并把 transport 移交给 ClientLockstep。
    pub fn recv_go(&mut self) -> io::Result<bool> {
        let Some(hs) = self.handshake.as_mut() else {
            return Ok(true);
        };
        if let Some(seq) = hs.recv_go(&mut self.rcv)? {
            let transport = self.handshake.take().unwrap().into_transport();
            self.lockstep = Some(ClientLockstep::new(transport, self.my_index, self.host));
            self.started = true;
            self.start_seq = seq;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 只上行本机输入（不推进），供测试自行驱动 host 收包时使用。
    #[cfg(test)]
    pub fn upload(&mut self, encoded: &[u8]) -> io::Result<()> {
        let Some(ls) = self.lockstep.as_mut() else {
            return Ok(());
        };
        ls.send_input(encoded)
    }

    /// 每帧：上行本机输入 + 收带 seq 帧推进。收到并推进返回 `Some(seq)`，未到返回 `None`。
    pub fn step_frame(
        &mut self,
        my_input: &PlayerInput,
        world: &mut World,
        dt: game_core::fix::Fix64,
    ) -> io::Result<Option<u64>> {
        let Some(ls) = self.lockstep.as_mut() else {
            return Ok(None);
        };
        ls.send_input(&encode_player_input(my_input))?;
        let n = world.players.len();
        match ls.step_frame(&mut self.rcv)? {
            Some(entries) => {
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
    #[test]
    fn host_and_two_clients_sync_over_udp() {
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut hs = HostHandshake::new(ht, 3, true); // host=0 + 2 client
        let mut a = NetLink::connect(host_addr).unwrap();
        let mut b = NetLink::connect(host_addr).unwrap();
        let mut rcv = [0u8; 8192];

        // 握手
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

        // READY + GO
        for _ in 0..100 {
            let _ = a.ready();
            let _ = b.ready();
            hs.poll_ready(&mut rcv);
            if hs.all_ready() {
                hs.broadcast_go();
            }
            if a.recv_go().unwrap() && b.recv_go().unwrap() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(a.started && b.started, "两端都应收 GO");

        // 运行：host 移交 transport → HostLockstep；三端各持同 seed World。
        let mut host = HostLockstep::new(hs.into_transport(), 3, true);
        let mut whost = World::new(3, 55);
        let mut wa = World::new(3, 55);
        let mut wb = World::new(3, 55);
        let dt = Fix64::from_num(1.0 / 60.0);
        let mut stepped = 0u32;

        for _ in 0..120 {
            // 各端上行输入
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
            // 两端收帧推进
            let _ = a.step_frame(&sample_input(), &mut wa, dt).unwrap();
            let _ = b.step_frame(&sample_input(), &mut wb, dt).unwrap();
            if a.started {
                stepped += 1;
            }
            // 三端应逐位一致（若 host 已产帧并广播到两端）
            assert_eq!(whost.players, wa.players, "host 与 a 应一致");
            assert_eq!(whost.players, wb.players, "host 与 b 应一致");
        }
        assert!(stepped > 0, "应至少推进过（防假绿）");
        assert_ne!(whost.players, World::new(3, 55).players, "输入应真实作用于 World");
    }
}
