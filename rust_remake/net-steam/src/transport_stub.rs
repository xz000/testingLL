//! 默认（未开 `steam` feature）的占位 `SteamTransport`。
//!
//! 目的：在**无 Steam 环境**下让 `net-steam` 也能编译、`Transport` trait 也真实存在，
//! 从而前端（`client::netlink::NetLink<net-steam::...>` 等）的类型关系能先对得上；
//! 但任何实际收发都返回明确的“未集成/未启用 feature”错误（不会 panic）。
//! 真实接入（`SteamAPI_Init` / `LobbyMatching` / `SteamNetworkingSockets`）在 `transport_steam.rs`（`steam` feature）。

use net::transport::{Peer, Transport};
use std::io;

/// 默认路径的 Steam 传输占位。
#[derive(Default, Clone, Debug)]
pub struct SteamTransport {
    // 预留：将来放 `steamworks::Client` / 会话句柄等。
    _marker: (),
}

impl Transport for SteamTransport {
    fn send_to(&mut self, _buf: &[u8], _peer: &Peer) -> io::Result<usize> {
        Err(io::Error::other(
            "net-steam: SteamTransport 未启用 —— 请用 `--features net-steam/steam` 编译并配合真机 Steam",
        ))
    }
    fn recv_from(&mut self, _buf: &mut [u8]) -> io::Result<Option<(usize, Peer)>> {
        Err(io::Error::other(
            "net-steam: SteamTransport 未启用 —— 请用 `--features net-steam/steam` 编译并配合真机 Steam",
        ))
    }
    fn local(&self) -> Peer {
        Peer::Steam { id: 0, conn: None }
    }
}
