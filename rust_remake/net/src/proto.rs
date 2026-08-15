//! 网络层字节协议：定义所有包的类型与编解码（传输无关）。
//!
//! 设计：每类包用 `Packet` 枚举表达，`encode` 编成字节、`decode` 从字节还原。
//! 所有解析带边界检查，非法包返回 `None`（上层忽略）。
//! 本层只依赖纯字节，不依赖 socket / ggez，可无头单测。
//!
//! 包类型（tag 首字节）：
//! - `TAG_JOIN=1`     client→host 空串，申请加入。
//! - `TAG_ACK=2`      host→client `[total][my_index]`。
//! - `TAG_INPUT=3`    client→host `[index][input_bytes]`。
//! - `TAG_FRAME=4`    host→client `[seq:u64][count:u16][(idx)(len)(bytes)...]`。
//! - `TAG_READY=5`    client→host 空串，准备就绪。
//! - `TAG_GO=6`       host→client `[start_seq:u64]` 统一起始。
//! - `TAG_REQ_FRAME=7` client→host `[missing_seq:u64]` 请求补发缺失帧。
#![allow(clippy::type_complexity)] // 网络二进制签名固有，允许。

use crate::frame::{frame_packet, parse_frame, up_packet};

pub const TAG_JOIN: u8 = 1;
pub const TAG_ACK: u8 = 2;
pub const TAG_INPUT: u8 = 3;
pub const TAG_FRAME: u8 = 4;
pub const TAG_READY: u8 = 5;
pub const TAG_GO: u8 = 6;
pub const TAG_REQ_FRAME: u8 = 7;
pub const TAG_SKILL: u8 = 8;
pub const TAG_SKILL_ALL: u8 = 9;

/// 一帧内各玩家的 `(玩家序号, 输入字节)`（已拷贝）。
pub type FrameData = Vec<(u8, Vec<u8>)>;

/// 网络层可发送/接收的包。
#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    /// client 申请加入（空）。
    Join,
    /// host 确认序号：`my_index` + 总人数。
    Ack { my_index: u8, players: u8 },
    /// client 本机输入：`index` + 已编码字节。
    Input { index: u8, bytes: Vec<u8> },
    /// host 广播整帧：`seq` + 全玩家输入。
    Frame { seq: u64, entries: FrameData },
    /// client 准备就绪（空）。
    Ready,
    /// host 统一起始：各端从 `start_seq` 开始推进。
    Go { start_seq: u64 },
    /// client 请求补发缺失帧：`seq` 为缺失的那一帧。
    ReqFrame { seq: u64 },
    /// client→host：上报本玩家最终配置快照（编码后的 `game_core::progress::PlayerConfig` 字节）。
    /// 学习阶段结束/就绪时发送。
    PlayerCfg { index: u8, bytes: Vec<u8> },
    /// host→所有端：广播下一局所有玩家的完整配置快照（各端据此确定性初始化下一局）。
    PlayerCfgAll { entries: Vec<(u8, Vec<u8>)> },
}

impl Packet {
    /// 编成字节（纯函数，返回独立 Vec）。
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Packet::Join => vec![TAG_JOIN],
            Packet::Ready => vec![TAG_READY],
            Packet::Ack { my_index, players } => vec![TAG_ACK, *players, *my_index],
            Packet::Go { start_seq } => {
                let mut v = Vec::with_capacity(9);
                v.push(TAG_GO);
                v.extend_from_slice(&start_seq.to_be_bytes());
                v
            }
            Packet::Input { index, bytes } => {
                let mut v = Vec::with_capacity(2 + bytes.len());
                v.push(TAG_INPUT);
                v.push(*index);
                v.extend_from_slice(bytes);
                v
            }
            Packet::Frame { seq, entries } => {
                let mut v = Vec::with_capacity(1);
                v.push(TAG_FRAME);
                let refs: Vec<(u8, &[u8])> = entries.iter().map(|(i, b)| (*i, b.as_slice())).collect();
                v.extend(frame_packet(*seq, &refs));
                v
            }
            Packet::ReqFrame { seq } => {
                let mut v = Vec::with_capacity(9);
                v.push(TAG_REQ_FRAME);
                v.extend_from_slice(&seq.to_be_bytes());
                v
            }
            Packet::PlayerCfg { index, bytes } => {
                let mut v = Vec::with_capacity(1 + 1 + 2 + bytes.len());
                v.push(TAG_SKILL);
                v.push(*index);
                v.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
                v.extend_from_slice(bytes);
                v
            }
            Packet::PlayerCfgAll { entries } => {
                let mut v = Vec::with_capacity(1 + 2 + entries.iter().map(|(_, b)| 1 + 2 + b.len()).sum::<usize>());
                v.push(TAG_SKILL_ALL);
                v.extend_from_slice(&(entries.len() as u16).to_be_bytes());
                for (idx, bytes) in entries {
                    v.push(*idx);
                    v.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
                    v.extend_from_slice(bytes);
                }
                v
            }
        }
    }

    /// 从字节解码（带边界检查）；非法/截断返回 `None`。
    pub fn decode(buf: &[u8]) -> Option<Packet> {
        if buf.is_empty() {
            return None;
        }
        match buf[0] {
            TAG_JOIN => Some(Packet::Join),
            TAG_READY => Some(Packet::Ready),
            TAG_ACK if buf.len() >= 3 => Some(Packet::Ack {
                my_index: buf[2],
                players: buf[1],
            }),
            TAG_GO if buf.len() >= 9 => {
                Some(Packet::Go { start_seq: u64::from_be_bytes(buf[1..9].try_into().ok()?) })
            }
            TAG_INPUT if buf.len() >= 2 => Some(Packet::Input {
                index: buf[1],
                bytes: buf[2..].to_vec(),
            }),
            TAG_FRAME => {
                let (seq, entries) = parse_frame(&buf[1..]).ok()?;
                let owned: FrameData = entries.into_iter().map(|(i, b)| (i, b.to_vec())).collect();
                Some(Packet::Frame { seq, entries: owned })
            }
            TAG_REQ_FRAME if buf.len() >= 9 => {
                Some(Packet::ReqFrame { seq: u64::from_be_bytes(buf[1..9].try_into().ok()?) })
            }
            TAG_SKILL if buf.len() >= 4 => {
                let index = buf[1];
                let len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
                let end = 4usize + len;
                if end > buf.len() {
                    return None;
                }
                Some(Packet::PlayerCfg { index, bytes: buf[4..end].to_vec() })
            }
            TAG_SKILL_ALL if buf.len() >= 3 => {
                let count = u16::from_be_bytes([buf[1], buf[2]]) as usize;
                let mut entries = Vec::with_capacity(count);
                let mut pos = 3;
                for _ in 0..count {
                    if pos + 3 > buf.len() {
                        return None;
                    }
                    let idx = buf[pos];
                    pos += 1;
                    let len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
                    pos += 2;
                    if pos + len > buf.len() {
                        return None;
                    }
                    entries.push((idx, buf[pos..pos + len].to_vec()));
                    pos += len;
                }
                Some(Packet::PlayerCfgAll { entries })
            }
            _ => None,
        }
    }
}

/// 把单个玩家输入包编码为「input 体」（不带 tag 的 `[index][bytes]`），供判等/测试用。
/// （等价于 `Packet::Input.encode()[1..]`。）
pub fn input_body(index: u8, bytes: &[u8]) -> Vec<u8> {
    up_packet(index, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_packets() {
        let cases: Vec<Packet> = vec![
            Packet::Join,
            Packet::Ready,
            Packet::Ack { my_index: 3, players: 8 },
            Packet::Go { start_seq: 7 },
            Packet::Input { index: 2, bytes: vec![1, 2, 3] },
            Packet::Frame {
                seq: 42,
                entries: vec![(0, vec![9; 4]), (1, vec![8]), (2, vec![7, 7])],
            },
            Packet::ReqFrame { seq: 41 },
            Packet::PlayerCfg { index: 2, bytes: vec![1, 2, 3, 4] },
            Packet::PlayerCfgAll {
                entries: vec![(0, vec![10, 20]), (1, vec![30]), (2, vec![])],
            },
        ];
        for p in cases {
            let enc = p.encode();
            let dec = Packet::decode(&enc).expect("应能解码");
            assert_eq!(dec, p, "协议往返应一致: {p:?}");
        }
    }

    #[test]
    fn decode_rejects_truncated() {
        // Go 需要 9 字节，给 2 字节应拒绝。
        assert!(Packet::decode(&[TAG_GO, 0, 1]).is_none());
        // Ack 需要 3 字节。
        assert!(Packet::decode(&[TAG_ACK, 0]).is_none());
        // 未知 tag。
        assert!(Packet::decode(&[99, 1, 2, 3]).is_none());
    }

    #[test]
    fn frame_entries_roundtrip() {
        let frame = Packet::Frame {
            seq: 5,
            entries: vec![(0, vec![1, 2]), (3, vec![4])],
        };
        let enc = frame.encode();
        let dec = Packet::decode(&enc).unwrap();
        assert_eq!(dec, frame);
    }
}
