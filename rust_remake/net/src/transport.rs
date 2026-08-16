//! 传输层：端到端字节收发（传输无关抽象 + 本地 UDP 实现）。

use std::io;
use std::net::{SocketAddr, UdpSocket};

/// 网络端点（传输无关抽象）。
///
/// - `Udp(SocketAddr)`：本地 StdUdpTransport 的端点（UDP socket 地址）。
/// - `Steam { id, conn }`：为 SteamTransport（`SteamNetworkingSockets`）预留的端点——`id` 是稳定身份
///   （如 `CSteamID.ConvertToUint64()`，也作重连身份用，见 RECONNECT.md 挂点 2），`conn` 是会话句柄。
///
/// 帧同步/握手逻辑一律只按 `Peer` 判等与转发，不关心具体传输，故换 `SteamTransport` 时无需改上层。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Peer {
    Udp(SocketAddr),
    /// Steam 预留端点：稳定身份 id + 可选会话句柄 conn。
    Steam { id: u64, conn: Option<u32> },
}

/// 帧同步所需的底层传输能力：向指定端点 send、从任一端点 recv 字节。
pub trait Transport {
    /// 向 `peer` 发送 `buf`。
    fn send_to(&mut self, buf: &[u8], peer: &Peer) -> io::Result<usize>;
    /// 非阻塞地接收一个包，返回 `(字节数, 来源端点)`；无包时返回 `Ok(None)`。
    fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<Option<(usize, Peer)>>;
    /// 本端在本地网络中的端点（host 客户端互相连接的已知地址用）。
    fn local(&self) -> Peer;
}

/// 基于标准库 `UdpSocket` 的本地传输实现。
pub struct StdUdpTransport {
    sock: UdpSocket,
}

impl StdUdpTransport {
    /// 绑定到 `addr`（如 `0.0.0.0:0` 让系统分配端口）。
    pub fn bind(addr: &str) -> io::Result<Self> {
        let sock = UdpSocket::bind(addr)?;
        sock.set_nonblocking(true)?;
        Ok(StdUdpTransport { sock })
    }

    /// 创建“测试用”传输：绑定到 127.0.0.1 系统分配端口，返回 (transport, 分配的地址)。
    pub fn bind_loopback() -> io::Result<(Self, SocketAddr)> {
        let t = Self::bind("127.0.0.1:0")?;
        let addr = t.local_addr();
        Ok((t, addr))
    }

    fn local_addr(&self) -> SocketAddr {
        self.sock.local_addr().unwrap()
    }
}

impl Transport for StdUdpTransport {
    fn send_to(&mut self, buf: &[u8], peer: &Peer) -> io::Result<usize> {
        match peer {
            Peer::Udp(addr) => self.sock.send_to(buf, *addr),
            // UDP 传输不认识 Steam 端点；由 SteamTransport 才处理该变体。此处不会在正常流程被调用。
            Peer::Steam { .. } => Err(io::Error::other("Peer::Steam used with StdUdpTransport")),
        }
    }

    fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<Option<(usize, Peer)>> {
        match self.sock.recv_from(buf) {
            Ok((n, addr)) => Ok(Some((n, Peer::Udp(addr)))),
            // EWOULDBLOCK / EAGAIN：当前无包
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn local(&self) -> Peer {
        Peer::Udp(self.local_addr())
    }
}
