//! 输入包的网络编解码（帧同步用）。
//!
//! 帧同步要求所有客户端对同一输入产生逐位一致的模拟。为此网络包必须**无歧义、有损/无损均可控**地编码
//! 每位玩家每帧的 `PlayerInput`（含 shift 队列 `Vec<Cmd>`）。此处用纯字节编码（大端、定长 where possible），
//! `Fix64` 以其 64 位位模式往返，保证跨进程/跨端逐位一致。
//!
//! 该模块在 `game-core` 内、不依赖网络/引擎，故可被 `net/` crate 与 `client` 复用，也可单测锁定。

use crate::fix::{Fix64, Vec2};
use crate::player::Cmd;
use crate::skill::SkillId;
use crate::world::PlayerInput;

/// 字节缓冲写入器。
pub struct BufWriter {
    bytes: Vec<u8>,
}

impl Default for BufWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl BufWriter {
    pub fn new() -> Self {
        BufWriter { bytes: Vec::new() }
    }
    pub fn u8(&mut self, v: u8) {
        self.bytes.push(v);
    }
    pub fn u32(&mut self, v: u32) {
        self.bytes.extend_from_slice(&v.to_be_bytes());
    }
    pub fn i64(&mut self, v: i64) {
        self.bytes.extend_from_slice(&v.to_be_bytes());
    }
    pub fn fix(&mut self, v: Fix64) {
        self.i64(v.to_bits());
    }
    pub fn vec2(&mut self, v: Vec2) {
        self.fix(v.x);
        self.fix(v.y);
    }
    pub fn opt_vec2(&mut self, v: Option<Vec2>) {
        match v {
            Some(p) => {
                self.u8(1);
                self.vec2(p);
            }
            None => self.u8(0),
        }
    }
    pub fn cmd(&mut self, c: Cmd) {
        match c {
            Cmd::Move(t) => {
                self.u8(0);
                self.vec2(t);
            }
            Cmd::Cast(id, t) => {
                self.u8(1);
                self.u32(id.as_u32());
                self.opt_vec2(t);
            }
            Cmd::Stop => {
                self.u8(2);
            }
        }
    }
    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

/// 字节缓冲读取器（带剩余边界检查）。
pub struct BufReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> BufReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        BufReader { bytes, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], &'static str> {
        if self.pos + n > self.bytes.len() {
            return Err("out of bounds");
        }
        let s = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8, &'static str> {
        Ok(self.take(1)?[0])
    }
    pub fn u32(&mut self) -> Result<u32, &'static str> {
        let a = self.take(4)?;
        Ok(u32::from_be_bytes([a[0], a[1], a[2], a[3]]))
    }
    pub fn i64(&mut self) -> Result<i64, &'static str> {
        let a = self.take(8)?;
        Ok(i64::from_be_bytes([
            a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7],
        ]))
    }
    pub fn fix(&mut self) -> Result<Fix64, &'static str> {
        Ok(Fix64::from_bits(self.i64()?))
    }
    pub fn vec2(&mut self) -> Result<Vec2, &'static str> {
        Ok(Vec2::new(self.fix()?, self.fix()?))
    }
    pub fn opt_vec2(&mut self) -> Result<Option<Vec2>, &'static str> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.vec2()?)),
            _ => Err("bad opt_vec2 tag"),
        }
    }
    pub fn cmd(&mut self) -> Result<Cmd, &'static str> {
        match self.u8()? {
            0 => Ok(Cmd::Move(self.vec2()?)),
            1 => {
                let id = self.u32()?;
                let t = self.opt_vec2()?;
                Ok(Cmd::Cast(SkillId::from_u32(id), t))
            }
            2 => Ok(Cmd::Stop),
            _ => Err("bad cmd tag"),
        }
    }
}

/// 编码一个 `PlayerInput` 位固定帧字节。
pub fn encode_player_input(p: &PlayerInput) -> Vec<u8> {
    let mut w = BufWriter::new();
    w.opt_vec2(p.set_target);
    match p.cast {
        Some((id, t)) => {
            w.u8(1);
            w.u32(id.as_u32());
            w.opt_vec2(t);
        }
        None => w.u8(0),
    }
    w.u32(p.queued.len() as u32);
    for c in p.queued.iter() {
        w.cmd(*c);
    }
    w.finish()
}

/// 解码一个 `PlayerInput`。
pub fn decode_player_input(b: &[u8]) -> Result<PlayerInput, &'static str> {
    let mut r = BufReader::new(b);
    let set_target = r.opt_vec2()?;
    let cast = match r.u8()? {
        0 => None,
        1 => {
            let id = r.u32()?;
            let t = r.opt_vec2()?;
            Some((SkillId::from_u32(id), t))
        }
        _ => return Err("bad cast tag"),
    };
    let n = r.u32()? as usize;
    let mut queued = Vec::with_capacity(n);
    for _ in 0..n {
        queued.push(r.cmd()?);
    }
    Ok(PlayerInput {
        set_target,
        cast,
        queued,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(seed: u64) -> PlayerInput {
        let f = |x: f64| Fix64::from_num(x);
        let mut p = PlayerInput {
            set_target: Some(Vec2::new(f(3.25), f(-1.5))),
            cast: Some((SkillId::Rock, Some(Vec2::new(f(6.0), f(0.0))))),
            queued: vec![
                Cmd::Move(Vec2::new(f(4.0), f(0.0))),
                Cmd::Cast(SkillId::Blink, None),
                Cmd::Stop,
            ],
        };
        let _ = seed;
        // 让字段有变化以覆盖 None 分支
        if seed % 2 == 0 {
            p.set_target = None;
            p.cast = None;
            p.queued = vec![];
        }
        p
    }

    #[test]
    fn roundtrip_preserves_all_fields() {
        for seed in 0..8u64 {
            let p = sample(seed);
            let enc = encode_player_input(&p);
            let dec = decode_player_input(&enc).expect("decoding should succeed");
            assert_eq!(dec, p, "roundtrip mismatch for seed {}", seed);
        }
    }

    #[test]
    fn truncated_input_fails_gracefully() {
        let p = sample(0);
        let enc = encode_player_input(&p);
        // 截掉一部分应报错而非 panic
        let cut = &enc[..enc.len() / 2];
        assert!(decode_player_input(cut).is_err());
    }

    #[test]
    fn two_clients_with_same_inputs_replay_identically() {
        // 帧同步铁证：两台独立 World 用相同输入流，逐位一致。
        use crate::world::World;
        let mut a = World::new(2, 42);
        let mut b = World::new(2, 42);
        let dt = Fix64::from_num(1.0 / 60.0);
        // 构建一组输入流（含即时移动、施法、队列指令），逐帧转发给双方
        let inputs = vec![
            vec![
                PlayerInput { set_target: Some(Vec2::new(Fix64::from_num(5.0), Fix64::ZERO)), ..Default::default() },
                PlayerInput { queued: vec![Cmd::Move(Vec2::new(Fix64::from_num(-5.0), Fix64::ZERO))], ..Default::default() },
            ],
            vec![
                PlayerInput { queued: vec![Cmd::Cast(SkillId::DashSlash, Some(Vec2::new(Fix64::from_num(3.0), Fix64::ZERO)))], ..Default::default() },
                PlayerInput::default(),
            ],
        ];
        for _ in 0..120 {
            for frame in inputs.iter() {
                // 把输入经编解码（模拟网络传输）后再喂给 World，确保无损
                let dec_a: Vec<PlayerInput> = frame.iter().map(|p| decode_player_input(&encode_player_input(p)).unwrap()).collect();
                let dec_b: Vec<PlayerInput> = frame.iter().map(|p| decode_player_input(&encode_player_input(p)).unwrap()).collect();
                a.step(dec_a, dt);
                b.step(dec_b, dt);
            }
        }
        // 逐位一致
        assert_eq!(a.players, b.players, "两个客户端必须回放出完全相同世界");
        assert_eq!(a.arena_radius, b.arena_radius);
    }
}
