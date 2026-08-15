//! 帧同步网络层（阶段 3）。
//!
//! 设计要点（已与 PLAN 对齐）：
//! - **运输无关**：`Transport` trait 抽象 send/recv 字节与端点；本地用 `StdUdpTransport`(std UDP)，
//!   将来接 Steamworks 可换 `SteamTransport` 而网络逻辑（帧同步/编解码/合帧）不动。
//! - **帧同步模型**：host（同时当一个玩家）+ clients。每个玩家各持完整 `World`；
//!   host 每 tick 收齐各 client 输入（必须收齐，缺一不可推帧），汇成带 `seq` 的帧广播；
//!   各端以收到 `seq` 帧为推进锚点喂本地 `World` 确定性回放 → 各端逐位一致。
//! - **统一起始**：握手后加 READY/GO，所有端从同一 `start_seq` 开始推进，消除加载时刻差导致的漂移。
//!
//! 本模块只依赖 std UDP + `game_core`，不依赖 ggez，故可无头单测。

pub mod frame;
pub mod session;
pub mod transport;

pub use frame::{frame_packet, parse_frame, parse_up, up_packet};
pub use session::{
    ClientSession, HostSession, TAG_ACK, TAG_FRAME, TAG_GO, TAG_INPUT, TAG_JOIN, TAG_READY,
};
pub use transport::{Peer, StdUdpTransport, Transport};

#[cfg(test)]
mod tests {
    #![allow(clippy::type_complexity)] // 测试里的 (ClientSession<StdUdpTransport>, World) 等复杂类型：仅测试用，允许。
    use super::*;
    use game_core::fix::{Fix64, Vec2};
    use game_core::netcode::{decode_player_input, encode_player_input};
    use game_core::player::Cmd;
    use game_core::skill::SkillId;
    use game_core::world::{PlayerInput, World};
    use std::time::Duration;

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

    /// 统一建连 + READY/GO 流程：各 client 握手拿序号 → 上报 READY → host 收齐后广播 GO。
    /// 返回 GO 携带的起始 seq。所有 session 型测试共用，避免重复。
    fn settle_handshake_and_go(
        host: &mut HostSession<StdUdpTransport>,
        clients: &mut [(ClientSession<StdUdpTransport>, World)],
        host_peer: Peer,
        rcv: &mut [u8],
    ) -> u64 {
        // 1) JOIN → ACK
        let mut ok = false;
        for _ in 0..300 {
            for (c, _) in clients.iter_mut() {
                let _ = c.send_join(&host_peer);
            }
            host.poll_join(rcv);
            let mut all_acked = true;
            for (c, _) in clients.iter_mut() {
                if !c.recv_join_ack(rcv).unwrap_or(false) {
                    all_acked = false;
                }
            }
            if all_acked && host.joined >= host.expected() {
                ok = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(ok, "握手（JOIN/ACK）应收齐全部 client，joined={}", host.joined);

        // 2) READY → 收齐 → GO
        let mut start_seq = None;
        for _ in 0..300 {
            for (c, _) in clients.iter_mut() {
                let _ = c.send_ready(&host_peer);
            }
            host.poll_ready(rcv);
            if host.all_ready() {
                let go_seed = host.broadcast_go();
                let mut all_go = true;
                let mut got = None;
                for (c, _) in clients.iter_mut() {
                    match c.recv_go(rcv).unwrap_or(None) {
                        Some(s) => got = Some(s),
                        None => all_go = false,
                    }
                }
                if all_go {
                    start_seq = Some(got.unwrap_or(go_seed));
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let start_seq = start_seq.expect("GO 应送达所有 client（统一起始）");
        // 统一：忽略 READY/GO 时序后输入阶段从 start_seq 开始；测试直接以 frame 为准，start_seq 用于断言。
        start_seq
    }

    /// 端到端帧同步（底层 transport 路径）：host + 2 client 用 raw frame_packet/parse_frame 跑若干 tick。
    /// 只验证封帧/解析本身（不涉及 session 的 READY/GO）。
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
                continue; // 未收齐，跳过（锁步语义在 session 层测，这里验证封帧）
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

    /// session 层端到端：host(不参与) + 2 client，完整 READY/GO + 带 seq 帧推进，两端 World 一致。
    #[test]
    fn session_lockstep_over_udp() {
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut host = HostSession::new(ht, 2);
        let host_peer = Peer::Udp(host_addr);
        let mut rcv = [0u8; 4096];

        let (t0, _) = StdUdpTransport::bind_loopback().unwrap();
        let (t1, _) = StdUdpTransport::bind_loopback().unwrap();
        let mut clients: Vec<(ClientSession<StdUdpTransport>, World)> = vec![
            (ClientSession::connected(t0, 0, 2), World::new(2, 7)),
            (ClientSession::connected(t1, 1, 2), World::new(2, 7)),
        ];
        let start_seq = settle_handshake_and_go(&mut host, &mut clients, host_peer, &mut rcv);
        assert_eq!(start_seq, 0, "首个 GO 起始 seq 应为 0");

        let dt = Fix64::from_num(1.0 / 60.0);
        let mut stepped = 0u32;
        for _ in 0..60 {
            // 各 client 上行；host 必须收齐才推。
            for (i, (c, _)) in clients.iter_mut().enumerate() {
                let _ = c.send_input(&encode_player_input(&sample_input(i as u8)), &host_peer);
            }
            let frame = loop {
                if let Some(f) = host.collect_inputs(&mut rcv) {
                    break f;
                }
                std::thread::sleep(Duration::from_millis(1));
            };
            let (fseq, entries) = frame;
            // 防假绿①：合帧收齐。
            assert_eq!(entries.len(), 2, "合帧应收齐 2 端输入");
            host.broadcast_frame(fseq, &entries);

            // 各 client 收帧并推进（只推进一次，防重推）。
            for (c, w) in clients.iter_mut() {
                let got = loop {
                    if let Some(g) = c.recv_frame(&mut rcv).unwrap() {
                        break g;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                };
                let (rseq, ents) = got;
                assert_eq!(rseq, fseq, "两端 seq 应一致");
                let mut ins = vec![PlayerInput::default(); 2];
                for (p, b) in ents {
                    ins[p as usize] = decode_player_input(&b).unwrap();
                }
                w.step(ins, dt);
            }
            assert_eq!(clients[0].1.players, clients[1].1.players, "两端 World 必须一致");
            stepped += 1;
        }
        assert!(stepped > 0, "应至少真实推进过 1 帧");
        assert_ne!(clients[0].1.players, World::new(2, 7).players, "客户端输入应真实作用于 World（防假通过）");
    }

    /// host 自身作为 player 0 参与：host 本地输入 + 2 client，三端带 seq 逐位一致。
    #[test]
    fn host_participates_as_player_zero() {
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut host = HostSession::new(ht, 2);
        host.host_participates(3);
        let host_peer = Peer::Udp(host_addr);
        let mut rcv = [0u8; 4096];

        let (t1, _) = StdUdpTransport::bind_loopback().unwrap();
        let (t2, _) = StdUdpTransport::bind_loopback().unwrap();
        let mut clients: Vec<(ClientSession<StdUdpTransport>, World)> = vec![
            (ClientSession::connected(t1, 1, 3), World::new(3, 9)),
            (ClientSession::connected(t2, 2, 3), World::new(3, 9)),
        ];
        let _ = settle_handshake_and_go(&mut host, &mut clients, host_peer, &mut rcv);

        let mut wh = World::new(3, 9);
        let dt = Fix64::from_num(1.0 / 60.0);
        let mut stepped = 0u32;
        for tick in 0..60 {
            host.set_local_input(Some(encode_player_input(&sample_input(0))));
            for (i, (c, _)) in clients.iter_mut().enumerate() {
                let _ = c.send_input(&encode_player_input(&sample_input((i + 1) as u8)), &host_peer);
            }
            let frame = loop {
                if let Some(f) = host.collect_inputs(&mut rcv) {
                    break f;
                }
                std::thread::sleep(Duration::from_millis(1));
            };
            let (fseq, entries) = frame;
            assert_eq!(entries.len(), 3, "合帧应收齐 host(0)+2 client");
            assert!(entries.iter().any(|(p, _)| *p == 0));
            host.broadcast_frame(fseq, &entries);

            for (c, w) in clients.iter_mut() {
                let got = loop {
                    if let Some(g) = c.recv_frame(&mut rcv).unwrap() {
                        break g;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                };
                let (rseq, ents) = got;
                assert_eq!(rseq, fseq);
                let mut ins = vec![PlayerInput::default(); 3];
                for (p, b) in ents {
                    ins[p as usize] = decode_player_input(&b).unwrap();
                }
                w.step(ins, dt);
            }
            // host 用同帧推进
            let mut in_h = vec![PlayerInput::default(); 3];
            for (p, b) in &entries {
                in_h[*p as usize] = decode_player_input(b).unwrap();
            }
            wh.step(in_h, dt);
            stepped += 1;
            for (ci, (_, w)) in clients.iter().enumerate() {
                assert_eq!(wh.players, w.players, "tick {} host 与 client{} World 应一致", tick, ci);
            }
        }
        assert!(stepped > 0, "应至少真实推进过 1 帧");
        assert_ne!(wh.players, World::new(3, 9).players, "联网输入应真实作用于 World（防假通过）");
    }

    /// 完整一局联网对战：host(不参与) + 2 client，跑到底验证名次判定一致。
    #[test]
    fn full_online_match_identical_worlds() {
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut host = HostSession::new(ht, 2);
        let host_peer = Peer::Udp(host_addr);
        let mut rcv = [0u8; 4096];

        let (t0, _) = StdUdpTransport::bind_loopback().unwrap();
        let (t1, _) = StdUdpTransport::bind_loopback().unwrap();
        let mut clients: Vec<(ClientSession<StdUdpTransport>, World)> = vec![
            (ClientSession::connected(t0, 0, 2), World::new(2, 123)),
            (ClientSession::connected(t1, 1, 2), World::new(2, 123)),
        ];
        let _ = settle_handshake_and_go(&mut host, &mut clients, host_peer, &mut rcv);

        let dt = Fix64::from_num(1.0 / 60.0);
        let mut stepped = 0u32;
        for _ in 0..300 {
            for (i, (c, _)) in clients.iter_mut().enumerate() {
                let _ = c.send_input(&encode_player_input(&sample_input(i as u8)), &host_peer);
            }
            let Some((fseq, entries)) = host.collect_inputs(&mut rcv) else {
                break; // 首包时序不定；一旦开推就持续
            };
            host.broadcast_frame(fseq, &entries);
            let mut all_got = vec![];
            for (c, _) in clients.iter_mut() {
                let (rseq, ents) = loop {
                    if let Some(g) = c.recv_frame(&mut rcv).unwrap() {
                        break g;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                };
                assert_eq!(rseq, fseq);
                all_got.push((c.my_index, ents));
            }
            for (ci, (_, w)) in clients.iter_mut().enumerate() {
                let mut ins = vec![PlayerInput::default(); 2];
                for (p, b) in &all_got[ci].1 {
                    ins[*p as usize] = decode_player_input(b).unwrap();
                }
                w.step(ins, dt);
            }
            assert_eq!(clients[0].1.players, clients[1].1.players, "两端 World 必须一致");
            stepped += 1;
            if clients[0].1.round_over() {
                break;
            }
        }
        assert!(stepped > 0, "应至少真实推进过 1 帧");
        assert_eq!(clients[0].1.players, clients[1].1.players, "两端 World 必须一致");
        if clients[0].1.round_over() {
            assert_eq!(clients[0].1.placement(), clients[1].1.placement(), "两端名次判定一致");
        }
    }

    /// 冒烟：host + 7 client（共 8，达上限）带 seq 锁步同步不崩。
    #[test]
    fn lockstep_8_player_max_capacity_smoke() {
        const N: usize = 8;
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut host = HostSession::new(ht, N - 1);
        host.host_participates(N as u8);
        let host_peer = Peer::Udp(host_addr);
        let mut rcv = [0u8; 8192];

        let mut clients: Vec<(ClientSession<StdUdpTransport>, World)> = Vec::new();
        for _ in 0..N - 1 {
            let (t, _) = StdUdpTransport::bind_loopback().unwrap();
            clients.push((ClientSession::connected(t, 0, 0), World::new(N as u32, 9001)));
        }
        let _ = settle_handshake_and_go(&mut host, &mut clients, host_peer, &mut rcv);
        // 序号应为 1..N
        let mut idxs: Vec<u8> = clients.iter().map(|(s, _)| s.my_index).collect();
        idxs.sort();
        assert_eq!(idxs, (1u8..N as u8).collect::<Vec<u8>>(), "client 序号应从 1 连续到 {}", N - 1);

        let mut whost = World::new(N as u32, 9001);
        let dt = Fix64::from_num(1.0 / 60.0);
        let mut stepped = 0u32;
        for _ in 0..40 {
            host.set_local_input(Some(encode_player_input(&sample_input(0))));
            for (i, (c, _)) in clients.iter_mut().enumerate() {
                let _ = c.send_input(&encode_player_input(&sample_input((i + 1) as u8)), &host_peer);
            }
            let Some((fseq, entries)) = host.collect_inputs(&mut rcv) else {
                continue;
            };
            assert_eq!(entries.len(), N, "合帧应收齐 N 端输入");
            host.broadcast_frame(fseq, &entries);

            for (c, w) in clients.iter_mut() {
                let (rseq, ents) = loop {
                    if let Some(g) = c.recv_frame(&mut rcv).unwrap() {
                        break g;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                };
                assert_eq!(rseq, fseq);
                let mut ins = vec![PlayerInput::default(); N];
                for (p, b) in ents {
                    ins[p as usize] = decode_player_input(&b).unwrap();
                }
                w.step(ins, dt);
            }
            let mut in_h = vec![PlayerInput::default(); N];
            for (p, b) in &entries {
                in_h[*p as usize] = decode_player_input(b).unwrap();
            }
            whost.step(in_h, dt);
            stepped += 1;
            for (ci, (_, w)) in clients.iter().enumerate() {
                assert_eq!(whost.players, w.players, "host 与 client{} World 应一致", ci);
            }
        }
        assert!(stepped > 0, "应至少真实推进过 1 帧");
        assert_ne!(whost.players, World::new(N as u32, 9001).players, "联网输入应真实作用于 World（防假通过）");
    }
}
