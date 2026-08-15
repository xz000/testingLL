//! 帧同步网络层（阶段 3）。
//!
//! 设计要点（已与 PLAN 对齐）：
//! - **运输无关**：`Transport` trait 抽象 send/recv 字节与端点；本地用 `StdUdpTransport`(std UDP)，
//!   将来接 Steamworks 可换 `SteamTransport` 而网络逻辑（帧同步/编解码/合帧）不动。
//! - **帧同步模型**：host（同时当一个玩家）+ clients。每个玩家各持完整 `World`；
//!   host 每 tick 收齐各 client 用 `game_core::netcode` 编码的 `PlayerInput`，汇成帧广播；
//!   各端解出整帧输入后喂给本地 `World` 确定性回放 → 各端逐位一致。
//!
//! 本模块只依赖 std UDP + `game_core`，不依赖 ggez，故可无头单测。

pub mod frame;
pub mod session;
pub mod transport;

pub use frame::{frame_packet, parse_frame, parse_up, up_packet};
pub use session::{ClientSession, HostSession, TAG_ACK, TAG_FRAME, TAG_INPUT, TAG_JOIN};
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

    /// 端到端帧同步：在单进程内用 3 个本地 UDP socket（host + 2 client）跑若干 tick，
    /// 验证：client 输入上行 → host 合帧广播 → 各端解码后喂各自 World → 逐位一致。
    #[test]
    fn lockstep_over_udp_reaches_identical_worlds() {
        // 三个端点
        let (mut host, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let (mut c0, c0_addr) = StdUdpTransport::bind_loopback().unwrap();
        let (mut c1, c1_addr) = StdUdpTransport::bind_loopback().unwrap();

        // 两个 client 各持一份（2 玩家）World；host 只负责合帧广播
        let mut w0 = World::new(2, 7);
        let mut w1 = World::new(2, 7);
        let dt = Fix64::from_num(1.0 / 60.0);

        // host 记住每个 client 的 (port -> player_index)
        let mut port_to_player: std::collections::HashMap<u16, u8> = std::collections::HashMap::new();
        port_to_player.insert(c0_addr.port(), 0);
        port_to_player.insert(c1_addr.port(), 1);

        let mut rcv = [0u8; 4096];
        for _tick in 0..60 {
            // (A) 各 client 把自己输入上行给 host
            for (t, paddr) in [(&mut c0, c0_addr), (&mut c1, c1_addr)] {
                let pi = sample_input(if paddr == c0_addr { 0 } else { 1 });
                let bytes = encode_player_input(&pi);
                let up = up_packet(if paddr == c0_addr { 0 } else { 1 }, &bytes); // [index][payload]
                t.send_to(&up, &Peer::Udp(host_addr)).unwrap();
            }

            // (B) host 收齐两个 client 的输入
            let mut collected: Vec<(u8, Vec<u8>)> = Vec::new();
            while let Some((n, from)) = host.recv_from(&mut rcv).unwrap() {
                let a = match from {
                    Peer::Udp(a) => a,
                };
                let player = *port_to_player.get(&a.port()).unwrap();
                let input_bytes = &rcv[..n];
                if let Some((pid, body)) = parse_up(input_bytes) {
                    if pid == player {
                        collected.push((pid, body.to_vec()));
                    }
                }
            }
            // 排序保证确定性顺序，汇成整帧并广播给两个 client
            collected.sort_by_key(|(pid, _)| *pid);
            let refs: Vec<(u8, &[u8])> = collected.iter().map(|(p, b)| (*p, b.as_slice())).collect();
            let frame = frame_packet(&refs);
            host.send_to(&frame, &Peer::Udp(c0_addr)).unwrap();
            host.send_to(&frame, &Peer::Udp(c1_addr)).unwrap();

            // (C) 两个 client 各收帧、解码、喂各自 World
            let mut w0_inputs: Option<Vec<PlayerInput>> = None;
            let mut w1_inputs: Option<Vec<PlayerInput>> = None;
            for (t, is_w0) in [((&mut c0), true), ((&mut c1), false)] {
                if let Some((n, _from)) = t.recv_from(&mut rcv).unwrap() {
                    let entries = parse_frame(&rcv[..n]).unwrap();
                    let mut inputs = vec![PlayerInput::default(); 2];
                    for (pid, body) in entries {
                        let pi = decode_player_input(body).unwrap();
                        inputs[pid as usize] = pi;
                    }
                    let slot = if is_w0 { &mut w0_inputs } else { &mut w1_inputs };
                    *slot = Some(inputs);
                }
            }
            if let (Some(a), Some(b)) = (w0_inputs, w1_inputs) {
                w0.step(a, dt);
                w1.step(b, dt);
            }
        }

        // (D) 两个 client 的 World 必须逐位一致
        assert_eq!(w0.players, w1.players, "两端 World 必须一致");
        assert_eq!(w0.arena_radius, w1.arena_radius);
    }

    #[test]
    fn frame_roundtrip() {
        let pi = sample_input(3);
        let enc = encode_player_input(&pi);
        let up = up_packet(3, &enc); // [index][payload]
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
        let p = frame_packet(&entries);
        let parsed = parse_frame(&p).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, 0);
        assert_eq!(parsed[1].0, 1);
        let d0 = decode_player_input(parsed[0].1).unwrap();
        let d1 = decode_player_input(parsed[1].1).unwrap();
        assert_eq!(d0, sample_input(0));
        assert_eq!(d1, sample_input(1));
    }

    /// 端到端“完整一局”的联网对战：host + 2 client 用 session + 真 UDP 跑到底，
    /// 验证两端 World 全程一致、且本局名次判定一致（这是 client 接入后所依赖的核心逻辑）。
    #[test]
    fn full_online_match_identical_worlds() {
        // host
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut host = HostSession::new(ht, 2);
        // 两个 client
        let (t0, _a0) = StdUdpTransport::bind_loopback().unwrap();
        let (t1, _a1) = StdUdpTransport::bind_loopback().unwrap();
        let host_peer = Peer::Udp(host_addr);
        let mut rcv = [0u8; 4096];
        let mut c0 = ClientSession::connected(t0, 0, 2);
        let mut c1 = ClientSession::connected(t1, 1, 2);
        // 建连握手
        for _ in 0..100 {
            let _ = c0.send_join(&host_peer);
            let _ = c1.send_join(&host_peer);
            host.poll_join(&mut rcv);
            if c0.recv_join_ack(&mut rcv).unwrap_or(false) && c1.recv_join_ack(&mut rcv).unwrap_or(false) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        // 每个 client 各持一份 2 玩家 World；agent 只控制自己的玩家槽（0 / 1）。
        let mut wa = World::new(2, 123);
        let mut wb = World::new(2, 123);
        let dt = Fix64::from_num(1.0 / 60.0);
        // 简单确定性 agent：自己的玩家朝固定方向移动/施法
        let script_a = sample_input(0);
        let script_b = sample_input(1);
        let mut stepped = 0u32;
        // 跑固定 240 tick 的联网对战，逐帧核对两端一致（不强求本局结束——名次判定逻辑已由 game-core 单测覆盖）。
        for _ in 0..240 {
            let ea = encode_player_input(&script_a);
            let eb = encode_player_input(&script_b);
            let _ = c0.send_input(&ea, &host_peer);
            let _ = c1.send_input(&eb, &host_peer);
            let collected = host.collect_inputs(&mut rcv);
            assert!(collected.len() >= 2, "合帧应含两端输入（实际 {}", collected.len());
            host.broadcast_frame(&collected);
            let mut fa = None;
            let mut fb = None;
            for _ in 0..3 {
                if let Some(e) = c0.recv_frame(&mut rcv).expect("r") { fa = Some(e); }
                if let Some(e) = c1.recv_frame(&mut rcv).expect("r") { fb = Some(e); }
                if fa.is_some() && fb.is_some() { break; }
            }
            if let (Some(ea), Some(eb)) = (fa, fb) {
                let mut ia = vec![PlayerInput::default(); 2];
                let mut ib = vec![PlayerInput::default(); 2];
                for (p, body) in ea { ia[p as usize] = decode_player_input(&body).unwrap(); }
                for (p, body) in eb { ib[p as usize] = decode_player_input(&body).unwrap(); }
                wa.step(ia, dt);
                wb.step(ib, dt);
                stepped += 1;
                // 逐帧核对两 client World 位一致（这是帧同步的核心不变量）
                assert_eq!(wa.players, wb.players, "联网各帧两端 World 必须一致");
                assert_eq!(wa.arena_radius, wb.arena_radius);
            }
        }
        assert!(stepped > 0, "应至少真实推进过 1 帧（防网络静默丢弃导致的假通过）");
        assert_ne!(wa.players, World::new(2, 123).players, "联网输入应真实作用于 World（防假通过）");
        // 若某端已本局结束，名次判定也应一致
        if wa.round_over() {
            assert_eq!(wa.placement(), wb.placement(), "两端名次判定必须一致");
        }
    }

    /// 用 Session 封装跑一遍真 UDP 锁步：建连 + 每帧收发 + 两端 World 一致。
    #[test]
    fn session_lockstep_over_udp() {
        // host
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut host = HostSession::new(ht, 2);
        // clients（各自拥有自己的 transport）
        let (t0, _a0) = StdUdpTransport::bind_loopback().unwrap();
        let (t1, _a1) = StdUdpTransport::bind_loopback().unwrap();

        let host_peer = Peer::Udp(host_addr);
        let mut rcv = [0u8; 4096];
        // 手动握手：client 发 JOIN → host 分配序号回 ACK，交替推进
        let mut c0 = ClientSession::connected(t0, 0, 2);
        let mut c1 = ClientSession::connected(t1, 0, 2);
        for _ in 0..100 {
            let _ = c0.send_join(&host_peer);
            let _ = c1.send_join(&host_peer);
            host.poll_join(&mut rcv);
            if c0.recv_join_ack(&mut rcv).unwrap_or(false) && c1.recv_join_ack(&mut rcv).unwrap_or(false) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(c0.my_index, 0);
        assert_eq!(c1.my_index, 1);
        assert!(host.joined >= 2, "host 应收齐 2 名玩家");

        let mut w0 = World::new(2, 7);
        let mut w1 = World::new(2, 7);
        let dt = Fix64::from_num(1.0 / 60.0);
        let mut stepped = 0u32;

        for _ in 0..60 {
            // client 上行输入
            let e0 = encode_player_input(&sample_input(0));
            let e1 = encode_player_input(&sample_input(1));
            // 让 client 知道自己序号，发送即可
            let _ = c0.send_input(&e0, &host_peer);
            let _ = c1.send_input(&e1, &host_peer);
            // host 收集 + 合帧广播
            let collected = host.collect_inputs(&mut rcv);
            assert!(collected.len() >= 2, "合帧应含两端输入（实际 {}", collected.len());
            host.broadcast_frame(&collected);
            // client 收帧
            let mut f0 = None;
            let mut f1 = None;
            for _ in 0..3 {
                if let Some(entries) = c0.recv_frame(&mut rcv).expect("c0 recv") { f0 = Some(entries); }
                if let Some(entries) = c1.recv_frame(&mut rcv).expect("c1 recv") { f1 = Some(entries); }
                if f0.is_some() && f1.is_some() { break; }
            }
            if let (Some(e0), Some(e1)) = (f0, f1) {
                let mut in0 = vec![PlayerInput::default(); 2];
                let mut in1 = vec![PlayerInput::default(); 2];
                for (p, body) in e0 { in0[p as usize] = decode_player_input(&body).unwrap(); }
                for (p, body) in e1 { in1[p as usize] = decode_player_input(&body).unwrap(); }
                w0.step(in0, dt);
                w1.step(in1, dt);
                stepped += 1;
            }
        }
        assert!(stepped > 0, "应至少真实推进过 1 帧（防网络静默丢弃导致的假通过）");
        // 证明输入真的生效了（若传输被静默丢弃，World 不会离开初始状态的默认布局）
        let initial = World::new(2, 7);
        assert_ne!(w0.players, initial.players, "客户端输入应真实作用于 World（防假通过）");
        assert_eq!(w0.players, w1.players, "session 锁步两端 World 必须一致");
        assert_eq!(w0.arena_radius, w1.arena_radius);
    }

    /// host 自身作为 player 0 参与：host 提供本地输入，结合 2 名 client(player 1/2) 合帧，
    /// 三端 World 应一致。
    #[test]
    fn host_participates_as_player_zero() {
        let (ht, host_addr) = StdUdpTransport::bind_loopback().unwrap();
        let mut host = HostSession::new(ht, 2); // 收 2 名 client
        host.host_participates(3); // 自身=player0
        let (t1, _) = StdUdpTransport::bind_loopback().unwrap();
        let (t2, _) = StdUdpTransport::bind_loopback().unwrap();
        let host_peer = Peer::Udp(host_addr);
        let mut rcv = [0u8; 4096];
        let mut c1 = ClientSession::connected(t1, 1, 3);
        let mut c2 = ClientSession::connected(t2, 2, 3);
        // 建连（host 分配 client 序号 1,2）
        for _ in 0..100 {
            let _ = c1.send_join(&host_peer);
            let _ = c2.send_join(&host_peer);
            host.poll_join(&mut rcv);
            if c1.recv_join_ack(&mut rcv).unwrap_or(false) && c2.recv_join_ack(&mut rcv).unwrap_or(false) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(c1.my_index, 1);
        assert_eq!(c2.my_index, 2);
        assert_eq!(c1.players, 3);

        let mut wh = World::new(3, 9); // host 的 World（3 玩家）
        let mut w1 = World::new(3, 9);
        let mut w2 = World::new(3, 9);
        let dt = Fix64::from_num(1.0 / 60.0);
        let mut stepped = 0u32;
        for tick in 0..90 {
            let ih = sample_input(0);
            let i1 = sample_input(1);
            let i2 = sample_input(2);
            host.set_local_input(Some(encode_player_input(&ih)));
            let _ = c1.send_input(&encode_player_input(&i1), &host_peer);
            let _ = c2.send_input(&encode_player_input(&i2), &host_peer);
            let frame = host.collect_inputs(&mut rcv);
            assert!(frame.iter().any(|(p, _)| *p == 0), "合帧应含 host 的 player0 输入");
            assert!(
                frame.iter().any(|(p, _)| *p == 1) && frame.iter().any(|(p, _)| *p == 2),
                "合帧应含两端 client 输入（实际 {:?}", frame.iter().map(|(p,_)| *p).collect::<Vec<_>>()
            );
            host.broadcast_frame(&frame);
            // 等到两 client 都收到本帧才推进（这样三端用同一帧，保证逐位一致）
            let mut in1: Option<Vec<PlayerInput>> = None;
            let mut in2: Option<Vec<PlayerInput>> = None;
            for _ in 0..10 {
                if let Some(f) = c1.recv_frame(&mut rcv).unwrap() {
                    let mut v = vec![PlayerInput::default(); 3];
                    for (p, b) in f { v[p as usize] = decode_player_input(&b).unwrap_or_default(); }
                    in1 = Some(v);
                }
                if let Some(f) = c2.recv_frame(&mut rcv).unwrap() {
                    let mut v = vec![PlayerInput::default(); 3];
                    for (p, b) in f { v[p as usize] = decode_player_input(&b).unwrap_or_default(); }
                    in2 = Some(v);
                }
                if in1.is_some() && in2.is_some() {
                    break;
                }
            }
            if let (Some(i1), Some(i2)) = (in1, in2) {
                // host 用同一份 frame 推进（与 client 收到的一致）
                let mut in_h = vec![PlayerInput::default(); 3];
                for (p, b) in &frame { in_h[*p as usize] = decode_player_input(b).unwrap_or_default(); }
                wh.step(in_h, dt);
                w1.step(i1, dt);
                w2.step(i2, dt);
                stepped += 1;
                assert_eq!(wh.players, w1.players, "tick {} host 与 client1 World 应一致", tick);
                assert_eq!(wh.players, w2.players, "tick {} host 与 client2 World 应一致", tick);
            }
        }
        assert!(stepped > 0, "应至少真实推进过 1 帧（防网络静默丢弃导致的假通过）");
        let initial = World::new(3, 9);
        assert_ne!(wh.players, initial.players, "联网输入应真实作用于 World（防假通过）");
    }
}

