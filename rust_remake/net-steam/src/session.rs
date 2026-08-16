//! `SteamSession` —— 大厅生命周期 + 把 `SteamTransport` 接到 lockstep 的高层封装。
//!
//! 职责（供 `main.rs` 的 `--steam-host` / `--steam-join` 使用）：
//!   - host：`init` → `host_create_lobby`（公开大厅写 `matchkey` 元数据）→ `listen()` 开 P2P 监听。
//!   - client：`init` → `find_and_join`（按 `matchkey` 搜索并加入 host 的大厅）→ `connect_to(host_steamid)`。
//!   - 两者都可从大厅成员名单 `LobbyPlayerTable` 得到“成员→玩家槽位 + 稳定身份”，喂给 `set_client_identities`。
//!
//! 复用已就绪的地基：`SteamTransport`（P2P 收发）、`LobbyPlayerTable`（成员→槽位）、
//! `net::lockstep::HostLockstep`/`ClientLockstep`（传输无关），故帧同步/多局/重连零改动。

use crate::transport_steam::SteamTransport;
use crate::lobby::{LobbyPlayerTable, SteamID};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// 大厅 matchkey（会写到大厅元数据，client 按此过滤搜索）。
pub const MATCH_KEY: &str = "matchkey";
/// 默认大厅 key 值（同一房间名）。
pub const MATCH_VALUE: &str = "remake_arena_v1";

/// 一次大厅会话的封装。
pub struct SteamSession {
    pub transport: SteamTransport,
    /// 我方所在大厅（host 创建后 / client 加入后）。
    pub lobby: Option<steamworks::LobbyId>,
    /// 大厅玩家表（成员→槽位 + 身份）。host/client 各持一致视角。
    pub table: Option<LobbyPlayerTable>,
}

impl SteamSession {
    /// 初始化 Steam 会话（每进程一个）。`virtual_port` 为 P2P 虚拟端口（host/peer 需一致）。
    pub fn init(app_id: u32, virtual_port: i32) -> io::Result<SteamSession> {
        let transport = SteamTransport::init(app_id, virtual_port)?;
        Ok(SteamSession {
            transport,
            lobby: None,
            table: None,
        })
    }

    /// 驱动回调（大厅列表/加入/连接事件需要每帧 pump）。
    pub fn run_callbacks(&self) {
        self.transport.run_callbacks();
    }

    /// host：创建公开大厅（max_members 含 host），写入 matchkey，返回 LobbyId。
    /// 需要 `run_callbacks` 驱动回调后才完成；`max_wait_beats` 限制等待拍数。
    pub fn host_create_lobby(&mut self, max_members: u32, beats: u32) -> io::Result<steamworks::LobbyId> {
        use steamworks::{LobbyId, LobbyType};
        let mm = self.transport.matchmaking();
        let done = Arc::new(AtomicBool::new(false));
        let slot = Arc::new(std::sync::Mutex::new(None::<Result<LobbyId, ()>>));
        {
            let done = done.clone();
            let slot = slot.clone();
            mm.create_lobby(LobbyType::Public, max_members, move |res| {
                *slot.lock().unwrap() = Some(res.map_err(|_| ()));
                done.store(true, Ordering::SeqCst);
            });
        }
        for _ in 0..beats {
            self.run_callbacks();
            std::thread::sleep(std::time::Duration::from_millis(50));
            if done.load(Ordering::SeqCst) {
                break;
            }
        }
        let lobby = slot
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| io::Error::other("lobby create timeout (需 run_callbacks 驱动回调)"))?
            .map_err(|_| io::Error::other("lobby create failed"))?;
        // 写 matchkey 供 client 搜索。
        mm.set_lobby_data(lobby, MATCH_KEY, MATCH_VALUE);
        self.lobby = Some(lobby);
        // host 自己是首个成员 → 建玩家表（host=自己）。
        let host_id = self.transport.steam_id();
        let members: Vec<SteamID> = mm.lobby_members(lobby).iter().map(|s| SteamID(s.raw())).collect();
        self.table = Some(LobbyPlayerTable::new(SteamID(host_id), members));
        Ok(lobby)
    }

    /// client：用 host 打印的 LobbyId 直接加入（自动搜厅失败时的 fallback）。
    pub fn join_lobby_by_id(&mut self, lobby_id: u64, beats: u32) -> io::Result<steamworks::LobbyId> {
        use steamworks::LobbyId;
        let mm = self.transport.matchmaking();
        let lobby = LobbyId::from_raw(lobby_id);
        eprintln!("[steam-sess] joining lobby by id {lobby_id} ...");
        let join_done = Arc::new(AtomicBool::new(false));
        let join_res = Arc::new(std::sync::Mutex::new(None::<Result<LobbyId, ()>>));
        {
            let join_done = join_done.clone();
            let join_res = join_res.clone();
            mm.join_lobby(lobby, move |r| {
                *join_res.lock().unwrap() = Some(r.map_err(|_| ()));
                join_done.store(true, Ordering::SeqCst);
            });
        }
        for _ in 0..beats {
            self.run_callbacks();
            std::thread::sleep(std::time::Duration::from_millis(50));
            if join_done.load(Ordering::SeqCst) {
                break;
            }
        }
        let lobby = join_res
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| io::Error::other("join lobby by id timeout"))?
            .map_err(|_| io::Error::other("join lobby by id failed"))?;
        eprintln!("[steam-sess] joined lobby by id {:?}", lobby.raw());
        self.lobby = Some(lobby);
        let host_id = mm.lobby_owner(lobby).raw();
        let members: Vec<SteamID> = mm.lobby_members(lobby).iter().map(|s| SteamID(s.raw())).collect();
        self.table = Some(LobbyPlayerTable::new(SteamID(host_id), members));
        Ok(lobby)
    }

    /// client：按 `matchkey` 大厅元数据过滤公开大厅并加入。返回 LobbyId。
    /// 注意：steamworks 的 `add_request_lobby_list_string_filter` 需要 `LobbyKey`（pub(crate) 字段）无法从本 crate 构造，
    /// 故改为 request_lobby_list 后用 `lobby_data(matchkey)` 过滤（公开 API）。
    pub fn client_find_and_join(&mut self, beats: u32) -> io::Result<steamworks::LobbyId> {
        use steamworks::LobbyId;
        let mm = self.transport.matchmaking();
        // 搜大厅列表。
        eprintln!("[steam-sess] requesting lobby list (matchkey)...");
        let list_done = Arc::new(AtomicBool::new(false));
        let candidates = Arc::new(std::sync::Mutex::new(Vec::<LobbyId>::new()));
        {
            let list_done = list_done.clone();
            let candidates = candidates.clone();
            mm.request_lobby_list(move |res| {
                if let Ok(l) = res {
                    *candidates.lock().unwrap() = l;
                }
                list_done.store(true, Ordering::SeqCst);
            });
        }
        for _ in 0..beats {
            self.run_callbacks();
            std::thread::sleep(std::time::Duration::from_millis(50));
            if list_done.load(Ordering::SeqCst) {
                break;
            }
        }
        // 过滤：找 matchkey 匹配的大厅。
        let lobby = candidates
            .lock()
            .unwrap()
            .iter()
            .find(|l| mm.lobby_data(**l, MATCH_KEY).as_deref() == Some(MATCH_VALUE))
            .copied()
            .ok_or_else(|| io::Error::other("未找到 matchkey 匹配的大厅（host 是否已 `--steam-host` 并建厅？）"))?;
        eprintln!("[steam-sess] found host lobby {:?}, joining...", lobby.raw());
        // 加入。
        let join_done = Arc::new(AtomicBool::new(false));
        let join_res = Arc::new(std::sync::Mutex::new(None::<Result<LobbyId, ()>>));
        {
            let join_done = join_done.clone();
            let join_res = join_res.clone();
            mm.join_lobby(lobby, move |r| {
                *join_res.lock().unwrap() = Some(r.map_err(|_| ()));
                join_done.store(true, Ordering::SeqCst);
            });
        }
        for _ in 0..beats {
            self.run_callbacks();
            std::thread::sleep(std::time::Duration::from_millis(50));
            if join_done.load(Ordering::SeqCst) {
                break;
            }
        }
        let lobby = join_res
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| io::Error::other("join lobby timeout"))?
            .map_err(|_| io::Error::other("join lobby failed"))?;
        self.lobby = Some(lobby);
        // 建玩家表：host=owner，其余成员按 SteamID 排序。
        let host_id = mm.lobby_owner(lobby).raw();
        let members: Vec<SteamID> = mm.lobby_members(lobby).iter().map(|s| SteamID(s.raw())).collect();
        self.table = Some(LobbyPlayerTable::new(SteamID(host_id), members));
        Ok(lobby)
    }

    /// host：开 P2P 监听；client：连到 host。据此把当前运输接到 lockstep。
    pub fn prepare_transport(&mut self) -> io::Result<()> {
        if let Some(host_id) = self.host_steam_id() {
            // 我是 host（自己 owner）→ listen；否则 connect 到 host。
            if self.transport.steam_id() == host_id {
                self.transport.listen()?;
            } else {
                self.transport.connect_to(host_id)?;
            }
        }
        Ok(())
    }

    /// 本局 host 的 SteamID（从玩家表或大厅 owner）。
    pub fn host_steam_id(&self) -> Option<u64> {
        if let Some(t) = &self.table {
            // 槽 0 是 host。
            return t.identities_in_order().first().map(|(_, id)| id.0);
        }
        if let Some(l) = self.lobby {
            return Some(self.transport.matchmaking().lobby_owner(l).raw());
        }
        None
    }

    /// 当前会话对应的玩家槽位（host=0，其余按成员排序）。
    pub fn my_slot(&self) -> u8 {
        let id = self.transport.steam_id();
        self.table
            .as_ref()
            .and_then(|t| t.slot_of(SteamID(id)))
            .unwrap_or(0)
    }

    /// 全部身份（按槽位序），供 `HostLockstep::set_client_identities` 用（含 host 的槽 0）。
    pub fn identities(&self) -> Vec<(u8, u64)> {
        self.table
            .as_ref()
            .map(|t| t.identities_in_order().into_iter().map(|(k, SteamID(v))| (k, v)).collect())
            .unwrap_or_default()
    }

    /// 消费本会话，归还底层 `SteamTransport`（供 `HostLockstep`/`ClientLockstep` 持有）。
    pub fn into_transport(self) -> SteamTransport {
        self.transport
    }}
