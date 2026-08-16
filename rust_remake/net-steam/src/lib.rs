//! net-steam —— Steam 传输适配（可插拔附件 crate）。
//!
//! 目标：把 Steam 联机做成 `net::Transport` 的可选实现，从而**复用 `net` 的 lockstep / 多局 /
//! 重连 / 身份**逻辑零改动。配合前端已做好的抽象：
//!   - `net::transport::Peer::Steam{ id, conn }` 已预留；`net::transport::Transport` 是传输无关 trait。
//!   - `client::netlink::NetLink<T: Transport>` 传输无关，可注入 `SteamTransport`。
//!   - 握手/重连已按稳定身份（u64=SteamID）去重与找回槽位。
//!
//! **feature 门控**：本 crate 的 `steam` feature 默认【关】，因此无 Steam 环境（纯局域网 / CI）也能
//! `cargo build --workspace`。开 `--features net-steam/steam` 才引入 `steamworks` 依赖并编译真实接入。
//! 运行期要真机 Steam 客户端登录 + AppID（`steam_appid.txt`）+ 至少双账号才能端到端验收大厅与收发。
//!
//! 演进（LOCKSTEP_FOUNDATION / RECONNECT / ROADMAP.M3 全线目标）：
//!   1. [A1，本文件已落] 接口骨架 + 大厅→玩家槽位映射约定 + 占位实现（默认可编译、可测）。
//!   2. [A2，待 `steam` feature + 双账号] 真实 `SteamAPI_Init` + `LobbyMatching` + `SteamNetworkingSockets`，实现 `Transport`。

pub mod lobby;
pub mod transport_stub;

pub use net::transport::{Peer, Transport};

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认路径：`SteamTransport` 存在、实现 `Transport`、且明确报“未集成”错误（不 panic）。
    #[test]
    fn stub_transport_errors_cleanly_not_panics() {
        let mut t = transport_stub::SteamTransport::default();
        let peer = Peer::Steam { id: 9001, conn: None };
        let r = t.send_to(b"hi", &peer);
        assert!(r.is_err(), "默认路径应返回“未集成”错误而非 panic");
        let mut buf = [0u8; 64];
        let recv = t.recv_from(&mut buf);
        assert!(recv.is_err());
    }
}
