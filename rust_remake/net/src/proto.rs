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
pub const TAG_SNAPSHOT: u8 = 10;
pub const TAG_RESYNC: u8 = 11;
pub const TAG_RECONNECT: u8 = 12;
/// 就绪状态变更：client→host，`index` 玩家序号 + 是否就绪（可反复 toggle，供房间界面显示/取消就绪）。
pub const TAG_PLAYER_READY: u8 = 13;
/// host→client：全体就绪，通知 client 去进入开局配置菜单。
pub const TAG_START_CONFIG: u8 = 14;
/// host→client：房间「就绪状态快照」：`entries` 为各玩家 `(玩家序号, 是否就绪)`（含 host 槽 0）。
/// 每端据此显示所有成员的实时就绪状态，保证多人界面一致。
pub const TAG_ROSTER_READY: u8 = 15;
/// client→host：房间阶段的状态包：`index` + 就绪标志 + 输入在场字节（三合一）。
/// 因为 P2P 下 Input 在场已被验证可靠送达、但独立 PlayerReady 常丢，故把就绪折进同一在场包，天然可靠。
pub const TAG_ROOM_STATE: u8 = 16;

/// 一帧内各玩家的 `(玩家序号, 输入字节)`（已拷贝）。
pub type FrameData = Vec<(u8, Vec<u8>)>;

/// 网络层可发送/接收的包。
#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    /// client 申请加入，附本端稳定身份（u64，未来=SteamID；局域网可随机，重连/按身份取槽用）。
    Join { identity: u64 },
    /// host 确认序号：`my_index` + 总人数 + 本端稳定身份回显。
    Ack { my_index: u8, players: u8, identity: u64 },
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
    /// client→host：请求重连，附本端稳定身份（Steam=SteamID；局域网=握手时登记的身份）
    /// 与已知的最后 seq（校验用）。host 按身份找回槽位，而非只靠来源端点。
    ReconnectReq { identity: u64, last_known_seq: u64 },
    /// client→host：就绪状态变更（`index`=玩家序号，`ready`=是否就绪；可反复 toggle 供取消就绪）。
    PlayerReady { index: u8, ready: bool },
    /// host→client：房间就绪状态快照（含 host 槽 0）。`entries` = 各玩家 `(序号, ready)`。
    RosterReady { entries: Vec<(u8, bool)> },
    /// host→client：全体就绪，进入开局配置菜单。`seq` 为 host 的起始帧号（作填充，避免 Steam P2P 丢过小的包）。
    StartConfig { seq: u64 },
    /// client→host：房间阶段状态包：`index`（玩家序号）+ `ready`（是否就绪）+ `build_done`（是否已选好技能/配完开局）+
    /// `input_bytes`（输入在场信号）。把就绪/配完/在场合并成单包，走可靠的连续上行通道（P2P 下 Input 在场实测可靠）。
    /// `build_done` 用于「开局配置阶段」判定该端是否已配完（host 收齐所有端 build_done 才产首帧统一开战）。
    RoomState { index: u8, ready: bool, build_done: bool, input_bytes: Vec<u8> },
    /// host→重连端：整场 World 快照字节 + 接回 seq。
    Snapshot { world_bytes: Vec<u8>, seq: u64 },
    /// host→部分端：对齐基线（各端从此 seq 重新确认一条基线后继续）。
    Resync { seq: u64 },
}

impl Packet {
    /// 编成字节（纯函数，返回独立 Vec）。
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Packet::Join { identity } => {
                let mut v = Vec::with_capacity(9);
                v.push(TAG_JOIN);
                v.extend_from_slice(&identity.to_be_bytes());
                v
            }
            Packet::Ready => vec![TAG_READY],
            Packet::Ack { my_index, players, identity } => {
                let mut v = Vec::with_capacity(10);
                v.push(TAG_ACK);
                v.push(*players);
                v.push(*my_index);
                v.extend_from_slice(&identity.to_be_bytes());
                v
            }
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
            Packet::ReconnectReq { identity, last_known_seq } => {
                let mut v = Vec::with_capacity(17);
                v.push(TAG_RECONNECT);
                v.extend_from_slice(&identity.to_be_bytes());
                v.extend_from_slice(&last_known_seq.to_be_bytes());
                v
            }
            Packet::PlayerReady { index, ready } => {
                vec![TAG_PLAYER_READY, *index, if *ready { 1 } else { 0 }]
            }
            Packet::StartConfig { seq } => {
                let mut v = vec![TAG_START_CONFIG];
                v.extend_from_slice(&seq.to_be_bytes());
                v
            }
            Packet::RosterReady { entries } => {
                let mut v = Vec::with_capacity(1 + 1 + entries.len() * 2);
                v.push(TAG_ROSTER_READY);
                v.push(entries.len() as u8);
                let mut sorted: Vec<(u8, bool)> = entries.clone();
                sorted.sort_by_key(|(i, _)| *i);
                for (i, r) in sorted {
                    v.push(i);
                    v.push(if r { 1 } else { 0 });
                }
                v
            }
            Packet::RoomState { index, ready, build_done, input_bytes } => {
                let mut v = Vec::with_capacity(5 + input_bytes.len());
                v.push(TAG_ROOM_STATE);
                v.push(*index);
                v.push(if *ready { 1 } else { 0 });
                v.push(if *build_done { 1 } else { 0 });
                v.extend_from_slice(&(input_bytes.len() as u16).to_be_bytes());
                v.extend_from_slice(input_bytes);
                v
            }
            Packet::Snapshot { world_bytes, seq } => {
                let mut v = Vec::with_capacity(1 + 2 + world_bytes.len() + 8);
                v.push(TAG_SNAPSHOT);
                v.extend_from_slice(&(world_bytes.len() as u16).to_be_bytes());
                v.extend_from_slice(world_bytes);
                v.extend_from_slice(&seq.to_be_bytes());
                v
            }
            Packet::Resync { seq } => {
                let mut v = Vec::with_capacity(9);
                v.push(TAG_RESYNC);
                v.extend_from_slice(&seq.to_be_bytes());
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
            TAG_JOIN if buf.len() >= 9 => {
                let identity = u64::from_be_bytes(buf[1..9].try_into().ok()?);
                Some(Packet::Join { identity })
            }
            TAG_READY => Some(Packet::Ready),
            TAG_ACK if buf.len() >= 10 => {
                let players = buf[1];
                let my_index = buf[2];
                let identity = u64::from_be_bytes(buf[3..11].try_into().ok()?);
                Some(Packet::Ack { my_index, players, identity })
            }
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
            TAG_SNAPSHOT if buf.len() >= 4 => {
                let len = u16::from_be_bytes([buf[1], buf[2]]) as usize;
                let end = 3usize + len;
                if end + 8 > buf.len() {
                    return None;
                }
                let world_bytes = buf[3..end].to_vec();
                let seq = u64::from_be_bytes(buf[end..end + 8].try_into().ok()?);
                Some(Packet::Snapshot { world_bytes, seq })
            }
            TAG_RESYNC if buf.len() >= 9 => {
                Some(Packet::Resync { seq: u64::from_be_bytes(buf[1..9].try_into().ok()?) })
            }
            TAG_RECONNECT if buf.len() >= 17 => {
                let identity = u64::from_be_bytes(buf[1..9].try_into().ok()?);
                let last_known_seq = u64::from_be_bytes(buf[9..17].try_into().ok()?);
                Some(Packet::ReconnectReq { identity, last_known_seq })
            }
            TAG_PLAYER_READY if buf.len() >= 3 => {
                Some(Packet::PlayerReady { index: buf[1], ready: buf[2] != 0 })
            }
            TAG_START_CONFIG if buf.len() >= 9 => {
                Some(Packet::StartConfig { seq: u64::from_be_bytes(buf[1..9].try_into().ok()?) })
            }
            TAG_ROSTER_READY if buf.len() >= 2 => {
                let count = buf[1] as usize;
                let mut entries = Vec::with_capacity(count);
                let mut pos = 2;
                for _ in 0..count {
                    if pos + 2 > buf.len() {
                        return None;
                    }
                    entries.push((buf[pos], buf[pos + 1] != 0));
                    pos += 2;
                }
                let entries = Packet::RosterReady { entries };
                Some(entries)
            }
            TAG_ROOM_STATE if buf.len() >= 5 => {
                let index = buf[1];
                let ready = buf[2] != 0;
                let build_done = buf[3] != 0;
                let len = u16::from_be_bytes([buf[4], buf[5]]) as usize;
                let end = 6usize + len;
                if end > buf.len() {
                    return None;
                }
                Some(Packet::RoomState { index, ready, build_done, input_bytes: buf[6..end].to_vec() })
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
            Packet::Join { identity: 9001 },
            Packet::Ready,
            Packet::Ack { my_index: 3, players: 8, identity: 9001 },
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
            Packet::ReconnectReq { identity: 123456, last_known_seq: 123 },
            Packet::PlayerReady { index: 2, ready: true },
            Packet::StartConfig { seq: 456 },
            Packet::RosterReady { entries: vec![(0, true), (1, false), (2, true)], },
            Packet::RoomState { index: 1, ready: true, build_done: true, input_bytes: vec![1, 2, 3, 4] },
            Packet::Snapshot { world_bytes: vec![1, 2, 3, 4, 5], seq: 456 },
            Packet::Resync { seq: 789 },
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
        // Ack 需要 10 字节（players+index+identity:u64）。
        assert!(Packet::decode(&[TAG_ACK, 0]).is_none());
        // Join 需要 9 字节（identity:u64）。
        assert!(Packet::decode(&[TAG_JOIN]).is_none());
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
