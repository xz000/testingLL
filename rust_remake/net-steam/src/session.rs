//! `SteamSession` —— 大厅生命周期 + 把 `SteamTransport` 接到 lockstep 的高层封装。
//!
//! 职责（供 `main.rs` 的 `--steam-host` / `--steam-join` 使用）：
//!   - host：`init` → `host_create_lobby`（公开大厅写 `matchkey` 元数据）→ `listen()` 开 P2P 监听。
//!   - client：`init` → `find_and_join`（按 `matchkey` 搜索并加入 host 的大厅）→ `connect_to(host_steamid)`。
//!   - 两者都可从大厅成员名单 `LobbyPlayerTable` 得到“成员→玩家槽位 + 稳定身份”，喂给 `set_client_identities`。
//!
//! 复用已就绪的地基：`SteamTransport`（P2P 收发）、`LobbyPlayerTable`（成员→槽位）、
//! `net::lockstep::HostLockstep`/`ClientLockstep`（传输无关），故帧同步/多局/重连零改动。

use crate::stats::{MatchSummary, STAT_KILLS, STAT_MATCHES, STAT_WINS};
use crate::transport_steam::SteamTransport;
use crate::lobby::{format_connect_string, parse_connect_string, LobbyPlayerTable, SteamID};
use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// 大厅 matchkey（会写到大厅元数据，client 按此过滤搜索）。
pub const MATCH_KEY: &str = "matchkey";
/// 默认大厅 key 值（同一房间名）。
pub const MATCH_VALUE: &str = "remake_arena_v1";
/// 大厅元数据：房间名称（键）。
pub const ROOM_NAME_KEY: &str = "room_name";
/// 大厅元数据：房间备注（键）。
pub const ROOM_NOTE_KEY: &str = "room_note";
/// 大厅元数据：房间总轮数（host 建房时写入；加入者据此对齐 MatchConfig.total_rounds）。
pub const ROOM_ROUNDS_KEY: &str = "room_rounds";
/// Rich Presence 键：好友列表里显示的自定义状态文案（无本地化配置时 Steam 直接显示它）。
pub const PRESENCE_STATUS_KEY: &str = "status";
/// Rich Presence 键：`connect` 会让好友看到「加入游戏」按钮，值由
/// [`crate::lobby::format_connect_string`] 生成、由 [`crate::lobby::parse_connect_string`] 解析。
pub const PRESENCE_CONNECT_KEY: &str = "connect";

/// 好友通过 Steam 发起的「加入游戏」请求（回调线程写入，主循环每帧取用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoinRequest {
    /// 要加入的大厅 id。
    pub lobby: u64,
    /// 发起请求的好友 SteamID（可能为 0 / 无效，例如从 Steam 界面直接加入）。
    pub from: u64,
}

// ---------------------------------------------------------------------------
// 好友邀请 / Rich Presence
//
// 这些能力只用到 `SteamTransport`（它持有唯一的 `steamworks::Client`），所以做成**自由函数**：
// 进房后 `SteamSession` 会被 `into_transport()` 消费掉、传输归 lockstep 持有，
// 房间里的邀请/状态上报仍要能用（对 `&SteamTransport` 调用即可）。
// ---------------------------------------------------------------------------

/// 列出 Steam 好友（供「邀请好友」界面）。在线优先、其次昵称升序；`in_lobby` 标记是否已在房间里。
/// 昵称需先 `request_user_information` 刷新（异步生效；首次可能拿到空昵称，界面重开即正常）。
pub fn list_friends(transport: &SteamTransport, lobby: Option<u64>) -> Vec<FriendInfo> {
    use steamworks::FriendFlags;
    let fr = transport.friends();
    let in_lobby: Vec<u64> = lobby
        .map(|l| {
            transport
                .matchmaking()
                .lobby_members(steamworks::LobbyId::from_raw(l))
                .iter()
                .map(|s| s.raw())
                .collect()
        })
        .unwrap_or_default();
    let mut out: Vec<FriendInfo> = fr
        .get_friends(FriendFlags::IMMEDIATE)
        .into_iter()
        .map(|f| {
            let id = f.id().raw();
            fr.request_user_information(f.id(), true);
            let online = f.state() != steamworks::FriendState::Offline;
            FriendInfo {
                id,
                name: f.name(),
                online,
                in_lobby: in_lobby.contains(&id),
            }
        })
        .collect();
    out.sort_by(|a, b| b.online.cmp(&a.online).then_with(|| a.name.cmp(&b.name)));
    out
}

/// 邀请一位好友进房间：`invite_user_to_game(connect 串)`。
/// 好友的游戏若在运行 → 收到 `GameRichPresenceJoinRequested`（据此自动加入大厅）；
/// 未运行 → Steam 用 `+connect_lobby <id>` 启动它（命令行解析见 client）。
pub fn invite_friend(transport: &SteamTransport, lobby: u64, friend_id: u64) {
    let connect = format_connect_string(lobby);
    transport
        .friends()
        .get_friend(steamworks::SteamId::from_raw(friend_id))
        .invite_user_to_game(&connect);
    eprintln!("[steam-invite] invited friend {friend_id} to lobby {lobby} via '{connect}'");
}

/// 打开 Steam 覆盖层的邀请窗口（让房主勾选多位好友一次性邀请）。
pub fn open_invite_dialog(transport: &SteamTransport, lobby: u64) {
    transport
        .friends()
        .activate_invite_dialog(steamworks::LobbyId::from_raw(lobby));
    eprintln!("[steam-invite] opened Steam invite dialog for lobby {lobby}");
}

/// 头像尺寸（Steam 三档固定尺寸，RGBA 字节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvatarSize {
    /// 32x32
    Small,
    /// 64x64
    Medium,
    /// 184x184
    Large,
}

impl AvatarSize {
    /// 该档头像的像素边长（Steam 固定；返回的字节数 = 边长² × 4）。
    pub fn side(self) -> u32 {
        match self {
            AvatarSize::Small => 32,
            AvatarSize::Medium => 64,
            AvatarSize::Large => 184,
        }
    }
}

/// 取某位 Steam 用户的头像（RGBA 字节 + 边长）。`None` = Steam 还没把头像拉下来（首次进房常见，稍后重试即可）。
pub fn avatar_rgba(transport: &SteamTransport, steam_id: u64, size: AvatarSize) -> Option<(Vec<u8>, u32)> {
    let f = transport.friends().get_friend(steamworks::SteamId::from_raw(steam_id));
    let bytes = match size {
        AvatarSize::Small => f.small_avatar(),
        AvatarSize::Medium => f.medium_avatar(),
        AvatarSize::Large => f.large_avatar(),
    };
    bytes.map(|b| (b, size.side()))
}

/// 到某 peer 的当前 ping（毫秒）。`None` = 还没建立会话 / Steam 暂无测量值
/// （帧同步对延迟敏感，没测出来时界面应显示“--”而不是 0）。
pub fn ping_to(transport: &SteamTransport, peer: u64) -> Option<i32> {
    use steamworks::networking_types::NetworkingIdentity;
    let identity = NetworkingIdentity::new_steam_id(steamworks::SteamId::from_raw(peer));
    let (_state, _info, realtime) = transport.networking_messages().get_session_connection_info(&identity);
    let ping = realtime?.ping();
    (ping > 0).then_some(ping)
}

// ---------------------------------------------------------------------------
// 统计 / 成就 / 排行榜
//
// 规则（该记什么）在 `stats.rs`（纯函数、有单测）；这里只负责**写给 Steam**。
// 全部 best-effort：这些 key 都要在 Steamworks 后台先定义，没配置时 set 会失败，
// 游戏不该因此报错，只打一条日志（真接入时按日志去后台补配置即可）。
// ---------------------------------------------------------------------------

/// 本机的 Steam 统计快照（best-effort 读回；后台没定义该统计 → `None`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatsSnapshot {
    /// 累计场次。
    pub matches: Option<i32>,
    /// 累计胜场。
    pub wins: Option<i32>,
    /// 累计击杀。
    pub kills: Option<i32>,
}

/// 一次战绩上报的结果（供界面提示：解锁了哪些成就、上了多少分、是否有写入失败）。
#[derive(Debug, Clone)]
pub struct MatchRecordReport {
    /// 本地判定应解锁的成就键（Steam 侧是否真解锁取决于后台是否定义了这些 key）。
    pub achievements: Vec<&'static str>,
    /// 本次要上传的排行榜分数。
    pub score: i32,
    /// 是否有任一项写失败（后台未配置 / stats 未就绪）。
    pub had_failure: bool,
}

/// 排行榜一行。
#[derive(Debug, Clone)]
pub struct LeaderboardRow {
    /// 全球名次（1 起）。
    pub rank: i32,
    pub steam_id: u64,
    pub score: i32,
}

/// 异步结果槽：Steam 的查找/下载走 call-result 回调（靠 `run_callbacks` 推进），
/// 回调签名是 `FnOnce + 'static`，借用不到这里的 transport，所以把结果写回调用方持有的共享槽。
pub type Shared<T> = Arc<Mutex<T>>;

/// 新建一个结果槽。
pub fn shared<T>(v: T) -> Shared<T> {
    Arc::new(Mutex::new(v))
}

/// 读回本机统计（需要先在 Steamworks 后台定义同名统计）。
pub fn stats_snapshot(transport: &SteamTransport) -> StatsSnapshot {
    let us = transport.user_stats();
    StatsSnapshot {
        matches: us.get_stat_i32(STAT_MATCHES).ok(),
        wins: us.get_stat_i32(STAT_WINS).ok(),
        kills: us.get_stat_i32(STAT_KILLS).ok(),
    }
}

/// 把一场战绩写进 Steam 统计 + 成就（统计是绝对值，所以先读旧值再加增量）。
pub fn record_match_result(transport: &SteamTransport, summary: MatchSummary) -> MatchRecordReport {
    use crate::stats::{achievements_for, leaderboard_score};
    let us = transport.user_stats();
    let mut had_failure = false;

    let deltas: [(&str, i32); 3] = [
        (STAT_MATCHES, 1),
        (STAT_WINS, if summary.won() { 1 } else { 0 }),
        (STAT_KILLS, summary.kills as i32),
    ];
    for (key, delta) in deltas {
        if delta == 0 {
            continue;
        }
        let cur = us.get_stat_i32(key).unwrap_or(0);
        if us.set_stat_i32(key, cur + delta).is_err() {
            had_failure = true;
            eprintln!("[steam-stats] set {key} failed（后台未定义该统计，或 stats 未就绪）");
        }
    }
    let achievements = achievements_for(summary);
    for key in achievements.iter() {
        if us.achievement(key).set().is_err() {
            had_failure = true;
            eprintln!("[steam-stats] unlock {key} failed（后台未定义该成就，或 stats 未就绪）");
        }
    }
    if us.store_stats().is_err() {
        had_failure = true;
        eprintln!("[steam-stats] store_stats failed");
    }
    eprintln!(
        "[steam-stats] recorded: kills={} best=#{} players={} rounds={} achievements={:?}",
        summary.kills, summary.best_placement, summary.players, summary.rounds, achievements
    );
    MatchRecordReport {
        achievements,
        score: leaderboard_score(summary),
        had_failure,
    }
}

/// 异步查找排行榜：结果写进 `slot`（`run_callbacks` 推进后可读）。
pub fn request_leaderboard(transport: &SteamTransport, name: &str, slot: &Shared<Option<steamworks::Leaderboard>>) {
    let name = name.to_string();
    let slot = slot.clone();
    // 回调要 'static，会把 `name` 移进去；查找接口又要一个 &str，所以先克隆一份给同步调用。
    transport
        .user_stats()
        .find_leaderboard(&name.clone(), move |res| match res {
            Ok(Some(lb)) => {
                eprintln!("[steam-lb] leaderboard '{name}' found");
                *slot.lock().unwrap() = Some(lb);
            }
            Ok(None) => eprintln!("[steam-lb] leaderboard '{name}' 不存在（需在 Steamworks 后台建榜）"),
            Err(e) => eprintln!("[steam-lb] find '{name}' failed: {e:?}"),
        });
}

/// 上传排行榜分数（KeepBest：只保留最好成绩）。
pub fn upload_leaderboard_score(transport: &SteamTransport, lb: &steamworks::Leaderboard, score: i32) {
    transport.user_stats().upload_leaderboard_score(
        lb,
        steamworks::UploadScoreMethod::KeepBest,
        score,
        &[],
        move |res| match res {
            Ok(Some(u)) => eprintln!(
                "[steam-lb] score {score} uploaded (rank {} -> {}, changed={})",
                u.global_rank_previous, u.global_rank_new, u.was_changed
            ),
            Ok(None) => eprintln!("[steam-lb] upload {score} rejected（后台未配置该榜？）"),
            Err(e) => eprintln!("[steam-lb] upload failed: {e:?}"),
        },
    );
}

/// 异步下载榜单前 `n` 名，结果写进 `slot`（`run_callbacks` 推进后可读）。
pub fn request_leaderboard_top(
    transport: &SteamTransport,
    lb: &steamworks::Leaderboard,
    n: usize,
    slot: &Shared<Vec<LeaderboardRow>>,
) {
    use steamworks::LeaderboardDataRequest;
    let slot = slot.clone();
    transport.user_stats().download_leaderboard_entries(
        lb,
        LeaderboardDataRequest::Global,
        1,
        n.max(1),
        0,
        move |res| match res {
            Ok(entries) => {
                let rows: Vec<LeaderboardRow> = entries
                    .into_iter()
                    .map(|e| LeaderboardRow {
                        rank: e.global_rank,
                        steam_id: e.user.raw(),
                        score: e.score,
                    })
                    .collect();
                eprintln!("[steam-lb] downloaded {} rows", rows.len());
                *slot.lock().unwrap() = rows;
            }
            Err(e) => eprintln!("[steam-lb] download failed: {e:?}"),
        },
    );
}

/// 写 Rich Presence：`status` 是好友列表里看到的状态文案；`connect` 非空时好友多出「加入游戏」按钮。
pub fn set_presence(transport: &SteamTransport, status: &str, connect: Option<&str>) {
    let fr = transport.friends();
    fr.set_rich_presence(PRESENCE_STATUS_KEY, Some(status));
    fr.set_rich_presence(PRESENCE_CONNECT_KEY, connect);
}

/// 清空 Rich Presence（回主菜单/退出房间时调用，避免好友仍看到「加入游戏」）。
pub fn clear_presence(transport: &SteamTransport) {
    transport.friends().clear_rich_presence();
}

/// 「邀请好友」界面里一位好友的展示信息。
#[derive(Debug, Clone)]
pub struct FriendInfo {
    pub id: u64,
    /// Steam 昵称。
    pub name: String,
    /// 是否在线（含在线/忙/离开等“收得到邀请”的状态）。
    pub online: bool,
    /// 是否已经在当前房间里（已在房间里的人不必再邀请）。
    pub in_lobby: bool,
}

/// 一次大厅会话的封装。
pub struct SteamSession {
    pub transport: SteamTransport,
    /// 我方所在大厅（host 创建后 / client 加入后）。
    pub lobby: Option<steamworks::LobbyId>,
    /// 大厅玩家表（成员→槽位 + 身份）。host/client 各持一致视角。
    pub table: Option<LobbyPlayerTable>,
    /// 待处理的「加入游戏」请求（Steam 回调写入；`take_join_request` 取走）。
    /// 回调只在 `run_callbacks()` 时被泵出，所以持有会话的一端要每帧 pump。
    join_requests: Arc<Mutex<VecDeque<JoinRequest>>>,
}

/// 房间列表里一间公开大厅的展示信息（加入前即可读取；房主昵称由调用方用 Friends 补）。
pub struct LobbyInfo {
    pub id: u64,
    /// 房主 SteamID（房间列表显示“谁建的房”）。
    pub owner: u64,
    /// 当前已加入人数。
    pub members: usize,
    /// 人数上限（建房时固定）。
    pub limit: usize,
    /// 房间名（元数据 `room_name`；缺省用“未命名房间”）。
    pub name: String,
    /// 房间备注（元数据 `room_note`，可空）。
    pub note: String,
}

impl SteamSession {
    /// 初始化 Steam 会话（每进程一个）。`virtual_port` 为 P2P 虚拟端口（host/peer 需一致）。
    pub fn init(app_id: u32, virtual_port: i32) -> io::Result<SteamSession> {
        let mut transport = SteamTransport::init(app_id, virtual_port)?;
        let join_requests = Arc::new(Mutex::new(VecDeque::new()));
        // 好友从好友列表点「加入游戏」（game 已在跑）→ Steam 给大厅 id。
        {
            let q = join_requests.clone();
            transport.register_callback(move |cb: steamworks::GameLobbyJoinRequested| {
                let req = JoinRequest { lobby: cb.lobby_steam_id.raw(), from: cb.friend_steam_id.raw() };
                eprintln!("[steam-invite] GameLobbyJoinRequested: lobby={} from={}", req.lobby, req.from);
                q.lock().unwrap().push_back(req);
            });
        }
        // 好友接受了我们发出的 invite（`invite_user_to_game` 的 connect 串）→ 解析出大厅 id。
        {
            let q = join_requests.clone();
            transport.register_callback(move |cb: steamworks::GameRichPresenceJoinRequested| {
                let Some(lobby) = parse_connect_string(&cb.connect) else {
                    eprintln!("[steam-invite] ignoring foreign connect string: {:?}", cb.connect);
                    return;
                };
                let req = JoinRequest { lobby, from: cb.friend_steam_id.raw() };
                eprintln!("[steam-invite] GameRichPresenceJoinRequested: lobby={lobby} from={}", req.from);
                q.lock().unwrap().push_back(req);
            });
        }
        Ok(SteamSession {
            transport,
            lobby: None,
            table: None,
            join_requests,
        })
    }

    /// 取走一个待处理的「加入游戏」请求（无则 `None`）。回调只在 `run_callbacks()` 时泵出，取之前应先 pump。
    pub fn take_join_request(&self) -> Option<JoinRequest> {
        self.join_requests.lock().unwrap().pop_front()
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

    /// 设置/修改房间名与备注（大厅元数据）。`None` = 不改该项。开房后可随时调用（编辑房间信息）。
    pub fn host_set_room_info(&self, name: Option<&str>, note: Option<&str>) -> io::Result<()> {
        let Some(l) = self.lobby else {
            return Err(io::Error::other("host_set_room_info: 尚未建厅"));
        };
        let mm = self.transport.matchmaking();
        if let Some(n) = name {
            let n = n.trim();
            let n = if n.is_empty() { "未命名房间" } else { n };
            mm.set_lobby_data(l, ROOM_NAME_KEY, n);
        }
        if let Some(n) = note {
            mm.set_lobby_data(l, ROOM_NOTE_KEY, n.trim());
        }
        Ok(())
    }

    /// 设置房间总轮数（写进大厅元数据，供加入者读取对齐）。
    pub fn host_set_rounds(&self, rounds: u32) -> io::Result<()> {
        let Some(l) = self.lobby else {
            return Err(io::Error::other("host_set_rounds: 尚未建厅"));
        };
        self.transport
            .matchmaking()
            .set_lobby_data(l, ROOM_ROUNDS_KEY, &rounds.to_string());
        Ok(())
    }

    /// 读取房间总轮数（加入者用；host 未设置时回退 None，由调用方给默认值）。
    pub fn lobby_rounds(&self) -> Option<u32> {
        let l = self.lobby?;
        self.transport
            .matchmaking()
            .lobby_data(l, ROOM_ROUNDS_KEY)
            .and_then(|s| s.parse().ok())
    }

    /// 列出 Steam 好友（供「邀请好友」界面）。见 [`list_friends`]。
    pub fn list_friends(&self) -> Vec<FriendInfo> {
        list_friends(&self.transport, self.lobby.map(|l| l.raw()))
    }

    /// 邀请一位好友进当前房间。见 [`invite_friend`]。
    pub fn invite_friend(&self, friend_id: u64) -> io::Result<()> {
        let Some(l) = self.lobby else {
            return Err(io::Error::other("invite_friend: 尚未加入/创建房间"));
        };
        invite_friend(&self.transport, l.raw(), friend_id);
        Ok(())
    }

    /// 打开 Steam 覆盖层的邀请窗口。见 [`open_invite_dialog`]。
    pub fn open_invite_dialog(&self) -> io::Result<()> {
        let Some(l) = self.lobby else {
            return Err(io::Error::other("open_invite_dialog: 尚未加入/创建房间"));
        };
        open_invite_dialog(&self.transport, l.raw());
        Ok(())
    }

    /// 写 Rich Presence。见 [`set_presence`]。
    pub fn set_presence(&self, status: &str, connect: Option<&str>) {
        set_presence(&self.transport, status, connect);
    }

    /// 清空 Rich Presence。见 [`clear_presence`]。
    pub fn clear_presence(&self) {
        clear_presence(&self.transport);
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

    /// 列出当前可加入的公开大厅（供「房间列表」界面浏览选房）。
    /// 只跑一次 `request_lobby_list` 回调；对每个大厅读人数/上限/房主/房名/备注（加入前即可读）。
    /// 返回空列表表示暂无可加入房间（host 未建厅或都已满）。
    pub fn client_list_lobbies(&self, beats: u32) -> io::Result<Vec<LobbyInfo>> {
        use steamworks::LobbyId;
        let mm = self.transport.matchmaking();
        let count = Arc::new(AtomicBool::new(false));
        let cands = Arc::new(std::sync::Mutex::new(Vec::<LobbyId>::new()));
        {
            let count = count.clone();
            let cands = cands.clone();
            mm.request_lobby_list(move |res| {
                if let Ok(l) = res {
                    *cands.lock().unwrap() = l;
                }
                count.store(true, Ordering::SeqCst);
            });
        }
        for _ in 0..beats {
            self.run_callbacks();
            std::thread::sleep(std::time::Duration::from_millis(50));
            if count.load(Ordering::SeqCst) {
                break;
            }
        }
        let ids = cands.lock().unwrap().clone();
        let mut out = Vec::with_capacity(ids.len());
        for l in ids {
            let members = mm.lobby_member_count(l);
            let limit = mm.lobby_member_limit(l).unwrap_or(2);
            let owner = mm.lobby_owner(l).raw();
            let name = mm.lobby_data(l, ROOM_NAME_KEY).unwrap_or_else(|| "未命名房间".to_string());
            let note = mm.lobby_data(l, ROOM_NOTE_KEY).unwrap_or_default();
            out.push(LobbyInfo {
                id: l.raw(),
                owner,
                members,
                limit,
                name,
                note,
            });
        }
        Ok(out)
    }

    /// 在 messages 接口下为“无”需额外准备：`SendMessageToUser` 会隐式建立会话、`AutoRestartBrokenSession`
    /// 断后自动重启，收发直接按 SteamID 走，无需 listen/connect。保留此空操作以兼容旧调用点。
    pub fn prepare_transport(&mut self) -> io::Result<()> {
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
