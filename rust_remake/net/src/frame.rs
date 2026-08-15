//! 帧同步的帧封装：定义“客户端→主机”的单个输入包，以及“主机→客户端”的整帧广播包。
//!
//! 帧内只携带字节 blob（每个 `PlayerInput` 已由 `game_core::netcode` 编码），这里只做组装/拆解。
#![allow(clippy::type_complexity)] // 网络二进制签名的复杂元组类型：属协议固有，允许。

use std::io;

/// 把“单个玩家的输入包”编码为上行包：`[player_index: u8][input_bytes...]`，返回独立缓冲。
/// `input_bytes` 长度由下层包边界决定（UDP 每包即一条，无需长度前缀）。
///
/// 纯函数：不接收外部 buffer、不做原地 clear，调用方拿返回值即可，避免“先 push tag 再被 clear”的隐患。
pub fn up_packet(player_index: u8, input_bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + input_bytes.len());
    out.push(player_index);
    out.extend_from_slice(input_bytes);
    out
}

/// 解析上行包：返回 `(player_index, input_bytes 切片)`。
pub fn parse_up(buf: &[u8]) -> Option<(u8, &[u8])> {
    if buf.is_empty() {
        return None;
    }
    let idx = buf[0];
    let rest = &buf[1..];
    if rest.is_empty() {
        return None;
    }
    Some((idx, rest))
}

/// 把“整帧”：若干 `(player_index, input_bytes)` 编码为下行广播包，返回独立缓冲。
/// 格式：`[seq: u64][count: u16][(idx: u8)(len: u16)(bytes)...]`。
/// `seq` 为帧序号，供 client 锚定推进（同一起点 + 丢帧检测 + 缓冲对齐）。
///
/// 纯函数：不接收外部 buffer、不做原地 clear。
pub fn frame_packet(seq: u64, entries: &[(u8, &[u8])]) -> Vec<u8> {
    let total: usize = 8 + 2 + entries.iter().map(|e| 1 + 2 + e.1.len()).sum::<usize>();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_be_bytes());
    for (idx, bytes) in entries {
        out.push(*idx);
        out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

/// 解析下行广播包，返回 `(frame_seq, 逐条 (player_index, input_bytes))`。
pub fn parse_frame(buf: &[u8]) -> io::Result<(u64, Vec<(u8, &[u8])>)> {
    if buf.len() < 8 + 2 {
        return Err(io::Error::other("frame too short"));
    }
    let mut seq_bytes = [0u8; 8];
    seq_bytes.copy_from_slice(&buf[0..8]);
    let seq = u64::from_be_bytes(seq_bytes);
    let buf = &buf[8..];
    let count = u16::from_be_bytes([buf[0], buf[1]]) as usize;
    let mut pos = 2;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if pos + 3 > buf.len() {
            return Err(io::Error::other("frame truncated"));
        }
        let idx = buf[pos];
        pos += 1;
        let len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
        pos += 2;
        if pos + len > buf.len() {
            return Err(io::Error::other("frame entry truncated"));
        }
        out.push((idx, &buf[pos..pos + len]));
        pos += len;
    }
    Ok((seq, out))
}
