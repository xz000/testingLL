//! client 侧的联网连接（纯粹、无 ggez 依赖，可无头单测）。
//!
//! 一个 `NetLink` 对应一个“加入 host 的玩家窗口”：每帧把自己的 `PlayerInput` 编码上行到 host，
//! 收整帧解码成所有玩家的 `PlayerInput` 并喂给本端 `World`（帧同步确定性回放）。

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
}

impl NetLink {
    /// 绑定本端并向 `host` 加入（须与对端 `HostSession::poll_join` 协同，见 crate::netlink::tests）。,
    pub fn connect(host: SocketAddr) -> io::Result<NetLink> {
        let (t, _) = StdUdpTransport::bind_loopback()?;
        Ok(NetLink {
            session: ClientSession::connected(t, 0, 0),
            host: Peer::Udp(host),
            rcv: vec![0u8; 4096],
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

    /// 我的玩家序号（握手后有效）。
    pub fn my_index(&self) -> u8 {
        self.session.my_index
    }

    /// 总玩家人数（握手后有效）。
    pub fn player_count(&self) -> u8 {
        self.session.players
    }

    /// 每帧：把本机输入上行，收整帧解码后喂给 `world`。
    /// 返回是否成功推进了一帧（收到合帧并 step 成功）。
    pub fn step_tick(&mut self, my_input: &PlayerInput, world: &mut World, dt: game_core::fix::Fix64) -> io::Result<bool> {
        // 上行本机输入
        let enc = encode_player_input(my_input);
        self.session.send_input(&enc, &self.host)?;
        // 收整帧（轮询几次直到拿到）
        let mut frame: Option<Vec<(u8, Vec<u8>)>> = None;
        for _ in 0..5 {
            if let Some(f) = self.session.recv_frame(&mut self.rcv)? {
                frame = Some(f);
                break;
            }
        }
        let Some(entries) = frame else {
            return Ok(false);
        };
        // 解码成所有玩家输入（按索引对齐 world.players）
        let n = world.players.len();
        let mut inputs = vec![PlayerInput::default(); n];
        for (idx, bytes) in entries {
            if (idx as usize) < n {
                inputs[idx as usize] = decode_player_input(&bytes).map_err(io::Error::other)?;
            }
        }
        world.step(inputs, dt);
        Ok(true)
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
    /// 这直接驱动 client 所使用的 `NetLink` 路径，是 client 联网逻辑的自动化测试。,
    #[test]
    fn two_client_links_stay_synced() {
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut host = HostSession::new(ht, 2);
        let mut rcv = [0u8; 4096];
        // 建两个 NetLink 并握手
        let mut a = NetLink::connect(host_addr).unwrap();
        let mut b = NetLink::connect(host_addr).unwrap();
        for _ in 0..100 {
            let _ = a.session.send_join(&a.host);
            let _ = b.session.send_join(&b.host);
            host.poll_join(&mut rcv);
            if a.session.recv_join_ack(&mut rcv).unwrap_or(false) && b.session.recv_join_ack(&mut rcv).unwrap_or(false) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(a.my_index(), 0);
        assert_eq!(b.my_index(), 1);

        let mut wa = World::new(2, 55);
        let mut wb = World::new(2, 55);
        let dt = Fix64::from_num(1.0 / 60.0);
        let mut rcv2 = [0u8; 4096];
        for _ in 0..120 {
            // a/b 各发自己输入；host 合帧广播
            let ia = sample_input();
            let ib = sample_input();
            let ea = encode_player_input(&ia);
            let eb = encode_player_input(&ib);
            a.session.send_input(&ea, &a.host).unwrap();
            b.session.send_input(&eb, &b.host).unwrap();
            let collected = host.collect_inputs(&mut rcv2);
            host.broadcast_frame(&collected);
            let _ = a.step_tick(&ia, &mut wa, dt);
            let _ = b.step_tick(&ib, &mut wb, dt);
            // 逐帧核对
            assert_eq!(wa.players, wb.players);
        }
    }
}
