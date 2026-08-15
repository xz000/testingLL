//! 帧同步网络层（阶段 3）。
//!
//! 设计要点（已与 PLAN 对齐）：
//! - **运输无关**：`Transport` trait 抽象 send/recv 字节与端点；本地用 `StdUdpTransport`(std UDP)，
//!   将来接 Steamworks 可换 `SteamTransport` 而网络逻辑（帧同步/编解码/合帧）不动。
//! - **分层**：`proto`（包格式与编解码）+ `handshake`（建连/READY/GO）+ `lockstep`（帧同步状态机）。
//!   帧同步模型：host（同时当一个玩家）+ clients。每个玩家各持完整 `World`；
//!   host 每 tick 收齐各 client 输入（必须收齐，缺一不推帧），汇成带 `seq` 的帧广播；
//!   各端以收到 `seq` 帧为推进锚点喂本地 `World` 确定性回放 → 各端逐位一致。
//! - **统一起始**：`handshake` 建连；host 收齐输入即产首帧（首帧＝统一起始），丢帧由 lockstep 补发。
//!
//! 本模块只依赖 std UDP + `game_core`，不依赖 ggez，故可无头单测。
//! （注：旧的 `session.rs` 合并了建连与每帧收发、且无丢帧补偿，已由 handshake+lockstep 取代并移除。）

pub mod frame;
pub mod handshake;
pub mod lockstep;
pub mod proto;
pub mod transport;

pub use frame::{frame_packet, parse_frame, parse_up, up_packet};
pub use handshake::{ClientHandshake, HostHandshake};
pub use lockstep::{ClientLockstep, HostLockstep};
pub use proto::{FrameData, Packet, TAG_ACK, TAG_FRAME, TAG_GO, TAG_INPUT, TAG_JOIN, TAG_READY, TAG_REQ_FRAME};
pub use transport::{Peer, StdUdpTransport, Transport};

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::fix::{Fix64, Vec2};
    use game_core::netcode::{decode_player_input, encode_player_input};
    use game_core::player::Cmd;
    use game_core::skill::SkillId;
    use game_core::world::{PlayerInput, World};

    fn sample_input(pid: u8) -> PlayerInput {
        let f = |x: f64| Fix64::from_num(x);
        let mut p = PlayerInput {
            set_target: Some(Vec2::new(f(pid as f64), f(2.0))),
            cast: Some((SkillId::Rock, Some(Vec2::new(f(6.0), f(0.0))))),
            queued: vec![Cmd::Move(Vec2::new(f(4.0), f(0.0))), Cmd::Stop],
            clear_queue: false,
            stop_move: false,
        };
        if pid % 2 == 0 {
            p.set_target = None;
            p.queued = vec![];
        }
        p
    }

    /// 端到端帧同步（底层 frame_packet/parse_frame 路径）：host + 2 client 用 raw 帧跑若干 tick，
    /// 验证封帧/解析本身能驱动两端 World 一致（不涉及 handshake/lockstep 层）。
    #[test]
    fn lockstep_over_udp_reaches_identical_worlds() {
        let (mut host, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let (mut c0, c0_addr) = StdUdpTransport::bind_loopback().unwrap();
        let (mut c1, c1_addr) = StdUdpTransport::bind_loopback().unwrap();

        let mut w0 = World::new(2, 7);
        let mut w1 = World::new(2, 7);
        let dt = Fix64::from_num(1.0 / 60.0);

        let mut port_to_player: std::collections::HashMap<u16, u8> = std::collections::HashMap::new();
        port_to_player.insert(c0_addr.port(), 0);
        port_to_player.insert(c1_addr.port(), 1);

        let mut rcv = [0u8; 4096];
        let mut seq = 0u64;
        for _tick in 0..30 {
            // (A) 各 client 上行
            for (t, paddr) in [(&mut c0, c0_addr), (&mut c1, c1_addr)] {
                let pi = sample_input(if paddr == c0_addr { 0 } else { 1 });
                let bytes = encode_player_input(&pi);
                let up = up_packet(if paddr == c0_addr { 0 } else { 1 }, &bytes);
                t.send_to(&up, &Peer::Udp(host_addr)).unwrap();
            }
            // (B) host 收齐并合帧
            let mut collected: Vec<(u8, Vec<u8>)> = Vec::new();
            while let Some((n, from)) = host.recv_from(&mut rcv).unwrap() {
                let a = match from {
                    Peer::Udp(a) => a,
                };
                let player = *port_to_player.get(&a.port()).unwrap();
                if let Some((pid, body)) = parse_up(&rcv[..n]) {
                    if pid == player {
                        collected.push((pid, body.to_vec()));
                    }
                }
            }
            if collected.len() < 2 {
                continue;
            }
            collected.sort_by_key(|(pid, _)| *pid);
            let refs: Vec<(u8, &[u8])> = collected.iter().map(|(p, b)| (*p, b.as_slice())).collect();
            let frame = frame_packet(seq, &refs);
            host.send_to(&frame, &Peer::Udp(c0_addr)).unwrap();
            host.send_to(&frame, &Peer::Udp(c1_addr)).unwrap();
            // (C) 各 client 收帧
            let mut w0_in = None;
            let mut w1_in = None;
            for (t, is_w0) in [((&mut c0), true), ((&mut c1), false)] {
                if let Some((n, _)) = t.recv_from(&mut rcv).unwrap() {
                    let (_, entries) = parse_frame(&rcv[..n]).unwrap();
                    let mut ins = vec![PlayerInput::default(); 2];
                    for (pid, body) in entries {
                        ins[pid as usize] = decode_player_input(body).unwrap();
                    }
                    if is_w0 {
                        w0_in = Some(ins);
                    } else {
                        w1_in = Some(ins);
                    }
                }
            }
            if let (Some(a), Some(b)) = (w0_in, w1_in) {
                w0.step(a, dt);
                w1.step(b, dt);
                seq += 1;
            }
        }
        assert_eq!(w0.players, w1.players, "两端 World 必须一致");
        assert_eq!(w0.arena_radius, w1.arena_radius);
    }

    #[test]
    fn frame_roundtrip() {
        let pi = sample_input(3);
        let enc = encode_player_input(&pi);
        let up = up_packet(3, &enc);
        let (idx, body) = parse_up(&up).unwrap();
        assert_eq!(idx, 3);
        let dec = decode_player_input(body).unwrap();
        assert_eq!(dec, pi);
    }

    #[test]
    fn frame_packet_roundtrip() {
        let e0 = encode_player_input(&sample_input(0));
        let e1 = encode_player_input(&sample_input(1));
        let entries: Vec<(u8, &[u8])> = vec![(0, &e0), (1, &e1)];
        let p = frame_packet(42, &entries);
        let (seq, parsed) = parse_frame(&p).unwrap();
        assert_eq!(seq, 42);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, 0);
        assert_eq!(parsed[1].0, 1);
        let d0 = decode_player_input(parsed[0].1).unwrap();
        let d1 = decode_player_input(parsed[1].1).unwrap();
        assert_eq!(d0, sample_input(0));
        assert_eq!(d1, sample_input(1));
    }
}
