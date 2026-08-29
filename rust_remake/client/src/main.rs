//! ggez 客户端 —— 阶段 1：核心玩法单机 demo。
//!
//! - 玩家圆：**右键**设置移动目标点，圆球匀速走过去，到达即停
//! - 场地逐渐收缩，出界扣血；球被挤到边缘/相互重叠会受压损血
//! - 若干机器人（确定性 AI）在同一场地游走，演示多人对抗氛围
//!
//! 玩法逻辑全部在 `game-core` 的 `World` 中，本文件只负责输入采集与渲染。

use game_core::fix::{cos, sin, Fix64, Vec2};
use game_core::meta::{MatchConfig, MatchPhase, MatchState};
use game_core::rng::Rng;
use game_core::skill::SkillId;
use game_core::world::{PlayerInput, World};
use ggez::event;
use ggez::graphics::{self, Canvas, Color, DrawMode, Mesh};
// 让 `SteamTransport::send_stats()`（trait 默认实现由 net-steam 覆盖）在泛型代码里可直接调用。
#[cfg(feature = "steam")]
use net::transport::Transport as _;
use ggez::mint::Point2;
use ggez::{Context, GameResult};

mod netlink;

/// 机器人数量（不含玩家本人）。当前 Solo/局域网均无本地 AI；保留该常量供将来“带 AI 测试”模式复用。
#[allow(dead_code)]
const BOTS: u32 = 7;
/// 固定步长模拟（帧率）
const TICK: f64 = 1.0 / 60.0;
/// 玩家本人 = id 0
const PLAYER_ID: u32 = 0;

/// host：客户端连续空闲这么多帧判定为掉线（自动 mark_dropped，不卡全队）。约 3 秒。
const HOST_DROP_TICKS: u32 = 180;
/// host：每隔多少帧保存一次世界快照（供重连）。约 0.5 秒。
const SNAPSHOT_EVERY: u64 = 30;
/// client：连续多少帧未收到权威帧判定为“掉线/等待重连”（进入重连 UI）。约 3 秒。
const CLIENT_STALE_TICKS: u64 = 180;
/// Steam（client）：进入迁移后，探测“host 是否还在”的帧数（发 ReconnectReq 等 Snapshot 应答）。约 1 秒。
/// 超时无应答则判定 host 掉线，开始选举新 host。
#[cfg(feature = "steam")]
const MIGRATE_PROBE_TICKS: u64 = 60;/// 单机开局配置超时：等这么久没按开始就用默认配置自动开始第一轮（避免窗口没焦点/按键收不到导致卡死）。
const PRE_GAME_TIMEOUT_SECS: f64 = 60.0;
/// 每局发放的成长点（4.6b，占位数值，后续平衡）。
const GROWTH_PER_ROUND: u32 = 3;
/// 用金币兑换 1 成长点的成本（占位）。
const GOLD_PER_GROWTH: i32 = 20;
/// 某属性已有 `cur` 点时，买下一级成长点的成本（便宜斜坡，占位）。
fn growth_attr_cost(cur: u32) -> u32 {
    cur + 1 // base 1，每级 +1
}

/// Steamworks 应用 AppID（对应根目录 `steam_appid.txt` = 908660）。
#[cfg(feature = "steam")]
const APP_ID: u32 = 908660;/// Steam P2P 虚拟端口（host/peer 约定一致）。
#[cfg(feature = "steam")]
const STEAM_VIRTUAL_PORT: i32 = 1337;
/// Steam 对局确定性世界种子（各端一致，重建 world 用）。
#[cfg(feature = "steam")]
const STEAM_SEED: u64 = 20260812;
/// Steam 房间：全员就绪后进入配置/开战的缓冲倒计时（秒）。
#[cfg(feature = "steam")]
const STEAM_READY_COUNTDOWN_SECS: f32 = 5.0;
/// Steam 房间：倒计时最后这么多秒**不允许取消就绪**（锁定），防止临界竞态（避免有人卡最后一瞬取消导致不同步）。
#[cfg(feature = "steam")]
const STEAM_COUNTDOWN_LOCK_SECS: f32 = 2.0;
/// Steam 建房：玩家人数上限允许的最大值（steamworks 支持到 250，这里给竞技场设实际可玩的上限）。
#[cfg(feature = "steam")]
const STEAM_MAX_PLAYERS: u8 = 64;
/// Steam 建房：默认玩家数（创建房间界面的初始值）。
#[cfg(feature = "steam")]
const STEAM_DEFAULT_PLAYERS: u8 = 2;
/// Steam 建房：总轮数上限允许的最大值（256 基本无限制）。
#[cfg(feature = "steam")]
const STEAM_MAX_ROUNDS: u32 = 256;
/// Steam 建房：默认总轮数（创建房间界面的初始值，与 MatchConfig 默认一致）。
#[cfg(feature = "steam")]
const STEAM_DEFAULT_ROUNDS: u32 = 3;
/// Steam 房间列表：两次刷新（`request_lobby_list`）之间的最小间隔（秒）。
/// Steam 对大厅搜索接口有限速（Steam 官方建议每秒至多一次）；频繁触发会拿到空/陈旧结果（今实测 `1->0->1` 漂忽）。
#[cfg(feature = "steam")]
const LOBBY_REFRESH_COOLDOWN_SECS: f64 = 4.0;
/// Steam 房间（client）：连续这么多帧收不到 host 的 `RosterReady` 广播，即判定 host 已离开（自动退出房间回主菜单）。约 4 秒。
#[cfg(feature = "steam")]
const STEAM_LOBBY_SILENT_TIMEOUT_TICKS: u32 = 240;
/// Rich Presence：相同内容的最小重写间隔（秒）。内容变化时立即写，否则节流（Steam 对频繁 set 有限速）。
#[cfg(feature = "steam")]
const STEAM_PRESENCE_INTERVAL_SECS: f64 = 3.0;

/// 学习阶段里，数字键 1..N 用于从“选中的树”选择/绑定技能。
/// 这里定义 8 个键字母 → CastKey 的映射。
const KEY_LETTERS: [(&str, game_core::skill::CastKey); 8] = [
    ("c", game_core::skill::CastKey::C),
    ("r", game_core::skill::CastKey::R),
    ("e", game_core::skill::CastKey::E),
    ("d", game_core::skill::CastKey::D),
    ("y", game_core::skill::CastKey::Y),
    ("t", game_core::skill::CastKey::T),
    ("f", game_core::skill::CastKey::F),
    ("g", game_core::skill::CastKey::G),
];

/// 联网多局：学习阶段结束后、进入下一局前的“配置同步”阶段。
#[derive(Clone, Copy, PartialEq)]
enum NetCfgSync {
    /// 不处于同步（单机 / Fighting / Finished）。
    Idle,
    /// host：正在收齐各端配置（含自身），齐后广播 PlayerCfgAll 并完成。
    HostGather,
    /// client：已上传配置，正在等 host 广播 PlayerCfgAll。
    ClientWait,
}

/// 顶层应用状态（主菜单 / 各对战模式）。命令行可直通某模式，也可进主菜单选择。
#[derive(Clone, Copy, PartialEq, Debug)]
enum AppState {
    /// 主菜单：从三大入口选择。
    MainMenu,
    /// 单机技能试验场（无 AI，一个玩家自由测技能/数值）。
    Solo,
    /// 局域网：开房间（host，自身=player0）。
    LanHost { port: u16, total: u8 },
    /// 局域网：加入（client）。
    LanJoin { addr: std::net::SocketAddr },
    /// Steam：开厅作 host（自身=player0，其余由大厅成员按 SteamID 排序）。
    #[cfg(feature = "steam")]
    SteamHost { players: u8 },
    /// Steam：按 matchkey 自动加入 host 大厅（client）；`Some(lobby_id)` = 手动指定 host 打印的 LobbyId（fallback）。
    #[cfg(feature = "steam")]
    SteamJoin { lobby_id: Option<u64> },
}

/// 升级到某个等级的价格（简单坡度，后期可调）
fn upgrade_cost(current_level: u32) -> i32 {
    (current_level * 5 + 5) as i32
}

struct Game {
    /// 当前小局的战斗世界
    world: World,
    /// 多局 meta 状态（经济/升级/周期）
    meta: MatchState,
    /// 玩家本人待发送的移动目标（右键设置；成功发给 World 后由 World 保留）
    player_target: Option<Vec2>,
    /// 待发送的施法命令（左键确认后产生，直到世界进入前摇才清）
    pending_cast: Option<(SkillId, Option<Vec2>)>,
    /// 当前等左键确认的点目标技能（技能键按下后稳定保持，直到左键确认/右键/S 取消）
    pending_skill: Option<SkillId>,
    /// shift 键按住时压入的待执行指令队列（每帧把队首注入 PlayerInput.queued）
    queued_cmds: std::collections::VecDeque<game_core::player::Cmd>,
    /// shift 键按住点目标技能时，等左键确认的技能
    pending_shift_skill: Option<SkillId>,
    /// 本帧是否要给 World 下发"清空命令队列"信号（S / 普通即时操作打断）
    pending_clear_signal: bool,
    /// 本帧是否要给 World 下发"停止移动"信号（S 停手）
    pending_stop_signal: bool,
    /// 学习阶段当前选中的键（用于从该键的树里选技能/升级）
    learn_tree_key: Option<game_core::skill::CastKey>,
    /// 机器人的当前目标点
    bot_targets: Vec<Option<Vec2>>,
    /// 机器人的确定性随机源
    bot_rngs: Vec<Rng>,
    /// 累计未消费的模拟时间
    accumulator: f64,
    /// 世界坐标 → 屏幕坐标的缩放
    scale: f32,
    /// 相机偏移（竞技场中心在画面中央）
    offset: Point2<f32>,
    /// 联网模式：加入 host 后用于每帧收发/喂 World；`None` = 单机（含本地 AI 机器人）。
    net_link: Option<netlink::NetLinkUdp>,
    /// Steam 联机：host 端帧同步（feature=steam）。
    #[cfg(feature = "steam")]
    steam_host_ls: Option<net::lockstep::HostLockstep<net_steam::SteamTransport>>,
    /// Steam 联机：client 端帧同步（feature=steam）。
    #[cfg(feature = "steam")]
    steam_cli_ls: Option<net::lockstep::ClientLockstep<net_steam::SteamTransport>>,
    /// Steam 联机：本机在大厅里的玩家槽位（`self_index` 用）。
    #[cfg(feature = "steam")]
    steam_my_index: u8,
    /// Steam 联机：本机 SteamID（重连时作稳定身份发给 host 找回槽位）。
    #[cfg(feature = "steam")]
    steam_my_id: u64,
    /// Steam（client）：连续未收到权威帧的 tick 数（Steam 战斗端掉线探测；对齐局域网 stale_ticks）。
    #[cfg(feature = "steam")]
    steam_cli_stale_ticks: u64,
    /// Steam：本局参与玩家的稳定身份（SteamID）按 new index 排列（host 广播 `Participants` 后保存，**原始、不变**）。
    /// world index → SteamID 的映射：对局开始时确定，迁移时据此定位本端 world index 与掉线 host 的 index。
    #[cfg(feature = "steam")]
    steam_participants: Vec<u64>,
    /// Steam：当前**在线**参与玩家 SteamID 集（初始 = steam_participants；每次迁移排除掉线 host，并随 Takeover 同步）。
    /// 用于选举新 host（排除已掉线的 host，避免把已掉线的旧 host 再选出）。
    #[cfg(feature = "steam")]
    steam_online: Vec<u64>,
    /// Steam（client）：是否正处于「主机迁移」流程中（host 掉线 → 选举/接管/重定向）。
    #[cfg(feature = "steam")]
    steam_migrating: bool,
    /// Steam（client）：迁移流程累计 tick（探测 host 存活 / 等待 Takeover）。
    #[cfg(feature = "steam")]
    steam_migrate_ticks: u64,
    /// Steam（client）：选举出的新 host SteamID（0 = 尚未决定）。
    #[cfg(feature = "steam")]
    steam_new_host_id: u64,
    /// Steam（host）：接管后是否仍在「持续广播 Takeover」（直到首个在线 client 连上、产出首帧才停）。
    /// 避免晚进入迁移的 client 错过单次 Takeover 广播。
    #[cfg(feature = "steam")]
    steam_host_broadcasting_takeover: bool,
    /// Steam：全体就绪后的倒计时（秒）。
    #[cfg(feature = "steam")]
    steam_countdown: f32,
    /// Steam：是否仍在「房间/就绪」阶段（进配置菜单前）。
    #[cfg(feature = "steam")]
    steam_in_lobby: bool,
    /// Steam：本机是否已就绪（按 o toggle，可撤销）。
    #[cfg(feature = "steam")]
    steam_local_ready: bool,
    /// Steam：房间成员名单（槽位, 昵称, SteamID）。host 每 ~0.5s 经 `steam_refresh_roster` 刷新（显示新进成员），
    /// client 加入时构建；房间就绪界面按此列出成员。
    #[cfg(feature = "steam")]
    steam_roster: Vec<(u8, String, u64)>,
    /// Steam：本机最近一次收到的 host 房间「就绪状态快照」（多人一致界面；client 侧显示各成员就绪用）。
    #[cfg(feature = "steam")]
    steam_roster_ready: Vec<(u8, bool)>,
    /// Steam：全体就绪是否为真（host 计算；房间/就绪界面显示用）。
    #[cfg(feature = "steam")]
    steam_all_ready: bool,
    /// Steam：本机最近一次上报给 host 的就绪值（用于节流打印发送结果/变更）。
    #[cfg(feature = "steam")]
    steam_last_sent_ready: Option<bool>,
    /// Steam：房间阶段等待满员/就绪的累计帧数（用于节流打印诊断，避免每帧刷屏）。
    #[cfg(feature = "steam")]
    steam_lobby_wait_ticks: u32,
    /// Steam：本端是否已在「开局配置」阶段配好技能/配置（配完即置 true，host 据此收集各端 build_done 统一开战）。
    #[cfg(feature = "steam")]
    steam_build_done: bool,
    /// Steam（host）：房间阶段是否已经历过「全员就绪」状态（用于倒计时只在真正全员就绪后才开始，避免边界"秒进/永不进"）。
    #[cfg(feature = "steam")]
    steam_was_all_ready: bool,
    /// Steam：client 最近推进到的帧号（诊断：确认是否在收 host 权威帧）。
    #[cfg(feature = "steam")]
    steam_cli_last_seq: u64,
    /// Steam：主菜单内是否处于「大厅选择」子菜单（H 创建 / J 加入 / Q 返回）。
    #[cfg(feature = "steam")]
    steam_lobby_menu: bool,
    /// Steam 大厅子菜单（创建/加入/返回）的选中项（支持上下箭头 + 鼠标）。
    #[cfg(feature = "steam")]
    steam_lobby_selection: usize,
    /// Steam：是否处于「建房设置」界面（房间名/备注/人数，回车创建 / Q 取消）。
    #[cfg(feature = "steam")]
    steam_lobby_create: bool,
    /// Steam：是否处于「房间列表」界面（浏览公开大厅，方向键选中+回车加入 / R 刷新 / Q 返回）。
    #[cfg(feature = "steam")]
    steam_lobby_list: bool,
    /// Steam 建房设置：房间名当前输入。
    #[cfg(feature = "steam")]
    steam_create_name: String,
    /// Steam 建房设置：备注当前输入（可空）。
    #[cfg(feature = "steam")]
    steam_create_note: String,
    /// Steam 建房设置：当前聚焦字段（0=房间名，1=备注，2=人数）。
    #[cfg(feature = "steam")]
    steam_create_focus: usize,
    /// Steam：房间列表（缓存的公开大厅信息，供浏览选房）。
    #[cfg(feature = "steam")]
    steam_list_lobbies: Vec<net_steam::session::LobbyInfo>,
    /// Steam：房间列表当前选中项。
    #[cfg(feature = "steam")]
    steam_list_selection: usize,
    /// Steam：是否为房间列表拉取过一次（true=已在加载/已加载，避免反复 request_lobby_list）。
    #[cfg(feature = "steam")]
    steam_list_requested: bool,
    /// Steam：房间列表上次刷新的时间戳（秒，用于刷新节流，避免 Steam 搜索限速）。
    #[cfg(feature = "steam")]
    steam_list_last_refresh: f64,
    /// Steam：整个大厅流程持有的一次性 Steam 会话（进入大厅时 init 一次，建房/加入消费之；避免重复 init 单实例 steamworks）。
    #[cfg(feature = "steam")]
    steam_sess: Option<net_steam::session::SteamSession>,
    /// Steam：本机昵称（建房默认房间名/大厅展示用；进入大厅时读取并缓存）。
    #[cfg(feature = "steam")]
    steam_my_display_name: String,
    /// Steam：客户端要加入的指定大厅 LobbyId（从房间列表选中时设置；`enter_steam_mode` client 分支优先用其加入）。
    #[cfg(feature = "steam")]
    steam_join_lobby_id: Option<u64>,
    /// Steam：房主是否处于「编辑房间信息」子界面（房间就绪界面按 E 进入，回车保存 / Q 取消）。
    #[cfg(feature = "steam")]
    steam_room_edit: bool,
    /// Steam 房主编辑：当前聚焦字段（0=房间名，1=备注）。
    #[cfg(feature = "steam")]
    steam_room_edit_focus: usize,
    /// Steam 房主编辑：房间名当前编辑内容。
    #[cfg(feature = "steam")]
    steam_edit_name: String,
    /// Steam 房主编辑：备注当前编辑内容。
    #[cfg(feature = "steam")]
    steam_edit_note: String,
    /// Steam：当前所在房间的 LobbyId（编辑房间信息/锁房用；host/client 进入房间后设置）。
    #[cfg(feature = "steam")]
    steam_lobby_id: Option<u64>,
    /// Steam：当前房间是否已锁定（他人不能再加入；host 用 set_lobby_joinable 控制，本端记录状态）。
    #[cfg(feature = "steam")]
    steam_room_locked: bool,
    /// Steam：host 房间阶段刷新成员名单（roster）的节流计数器（client 加入后 host 才能看到并显示新成员）。
    #[cfg(feature = "steam")]
    steam_roster_refresh_ticks: u32,
    /// Steam：client 连续收不到 host 广播（RosterReady 心跳）的帧数；超过 `STEAM_LOBBY_SILENT_TIMEOUT_TICKS` 判定 host 已离开。
    #[cfg(feature = "steam")]
    steam_lobby_silent_ticks: u32,
    /// Steam：host 上次刷新 roster 看到的成员数（用于检测“有玩家离开”并提示 host，避免误以为卡住）。
    #[cfg(feature = "steam")]
    steam_last_roster_len: usize,
    /// Steam（host）：当前是否处于“不满员但在线者都已就绪、等待 host 手动开始”的状态（供界面提示）。
    #[cfg(feature = "steam")]
    steam_manual_start_pending: bool,
    /// 联网模式：开房作 host，建连/握手阶段（自身=player 0）。
    net_host: Option<net::handshake::HostHandshake<net::transport::StdUdpTransport>>,
    /// 联网模式：开房作 host，运行阶段。
    net_host_ls: Option<net::lockstep::HostLockstep<net::transport::StdUdpTransport>>,
    /// 联网：是否已完成 READY/GO 统一起始（可开始推进）。host=已广播 GO；client=已收 GO。
    net_ready: bool,
    /// 联网多局：学习结束后「配置同步」阶段（见 `NetCfgSync`）。
    net_cfg: NetCfgSync,
    /// 开局前的技能配置阶段（第一局开始前先选/升级技能）。
    pre_game_config: bool,
    /// 顶层应用状态（主菜单 / 各模式）。
    app: AppState,
    /// 客户端是否已因长时间收不到帧而进入“掉线/重连”状态（显示重连界面）。
    conn_dropped: bool,
    /// 客户端是否正在发起重连（已按 R，正等 host 快照）。
    reconnect_attempting: bool,
    /// 主机端累计产帧数（用于周期保存快照）。
    host_frame_count: u64,
    /// 单机开局配置剩余的等待秒数（超时自动用默认配置开始，避免“按键无反应卡死”）。
    pre_game_timer: f64,
    /// 主菜单当前选中项（方向键 ↑/↓ 移动 + 回车确认；数字键直选同步更新）。
    menu_selection: usize,
    /// 主菜单「Steam 大厅」子界面里，创建房间的玩家人数上限（2..=STEAM_MAX_PLAYERS）。
    #[cfg(feature = "steam")]
    steam_create_players: u8,
    /// Steam 建房设置：总轮数（1..=STEAM_MAX_ROUNDS）。
    #[cfg(feature = "steam")]
    steam_create_rounds: u32,
    /// Steam 建房设置：玩家人数输入缓冲（字符串编辑，支持 Backspace 逐位删）。
    #[cfg(feature = "steam")]
    steam_create_players_buf: String,
    /// Steam 建房设置：总轮数输入缓冲（字符串编辑，支持 Backspace 逐位删）。
    #[cfg(feature = "steam")]
    steam_create_rounds_buf: String,
    /// 当前场次的总轮数（host 建房设定 / client 从大厅元数据读取，两端一致）。
    #[cfg(feature = "steam")]
    match_rounds: u32,
    /// Steam：房间界面是否展开「邀请好友」面板（I 开关；不是模态，房间网络逻辑照常每帧跑）。
    #[cfg(feature = "steam")]
    steam_friend_list: bool,
    /// Steam：好友列表（展开面板时刷新一次；R 手动刷新）。
    #[cfg(feature = "steam")]
    steam_friends: Vec<net_steam::session::FriendInfo>,
    /// Steam：好友列表当前选中项。
    #[cfg(feature = "steam")]
    steam_friend_selection: usize,
    /// Steam：上次邀请动作的反馈文案（界面提示，如「已邀请 xxx」）。
    #[cfg(feature = "steam")]
    steam_friend_hint: String,
    /// Steam：Rich Presence 上次写入的时刻（秒，用于节流）。
    #[cfg(feature = "steam")]
    steam_presence_last: f64,
    /// Steam：Rich Presence 上次写入的内容（`状态|connect`），内容不变则不重复写。
    #[cfg(feature = "steam")]
    steam_presence_text: String,
    /// Steam：主菜单上是否已尝试过初始化会话（失败也不再每帧重试，避免刷屏 + 反复 SteamAPI_Init）。
    #[cfg(feature = "steam")]
    steam_session_tried: bool,
    /// Steam：各成员的 ping（毫秒），键 = SteamID；每 ~0.5s 刷新一次（帧同步对延迟敏感，值得显性展示）。
    #[cfg(feature = "steam")]
    steam_pings: Vec<(u64, i32)>,
    /// Steam：头像缓存（SteamID → 32x32 贴图）。拉过一次就缓存，Steam 首次常拉不到，下次自动重试。
    #[cfg(feature = "steam")]
    steam_avatars: Vec<(u64, graphics::Image)>,
    /// Steam：ping/头像刷新的节流计数（每 30 帧一次）。
    #[cfg(feature = "steam")]
    steam_net_ticks: u32,
    /// Steam：排行榜句柄槽（查找是异步回调，结果由 `run_callbacks` 推进后写回）。
    #[cfg(feature = "steam")]
    steam_lb_slot: net_steam::session::Shared<Option<net_steam::steamworks::Leaderboard>>,
    /// Steam：榜单前几名（下载同样是异步回调写回）。
    #[cfg(feature = "steam")]
    steam_lb_rows: net_steam::session::Shared<Vec<net_steam::session::LeaderboardRow>>,
    /// Steam：是否已对本场发起过排行榜查找（每会话只找一次）。
    #[cfg(feature = "steam")]
    steam_lb_requested: bool,
    /// Steam：本场战绩是否已上报（进入 Finished 只上报一次，避免每帧重复写）。
    #[cfg(feature = "steam")]
    steam_stats_recorded: bool,
    /// Steam：结算界面展示的统计快照（场次/胜场/击杀）。
    #[cfg(feature = "steam")]
    steam_stats_snapshot: Option<net_steam::session::StatsSnapshot>,
    /// Steam：成就/榜单提示条（文案 + 到期时刻，秒）。
    #[cfg(feature = "steam")]
    steam_toast: (String, f64),
}

impl Game {
    fn new(ctx: &mut Context, app: AppState) -> GameResult<Self> {
        // 注册中文字体：用 include_bytes 内嵌，避免资源路径/VFS 解析问题。
        let font = ggez::graphics::FontData::from_slice(include_bytes!("../../assets/fonts/cjk.ttf"))?;
        ctx.gfx.add_font("cjk", font);

        // 联网：加入 host 或开房作 host；否则单机（含本地 AI 机器人）。
        let mut net_link: Option<netlink::NetLinkUdp> = None;
        let mut net_host: Option<net::handshake::HostHandshake<net::transport::StdUdpTransport>> = None;
        let net_host_ls: Option<net::lockstep::HostLockstep<net::transport::StdUdpTransport>> = None;
        #[cfg(feature = "steam")]
        let mut steam_host_ls: Option<net::lockstep::HostLockstep<net_steam::SteamTransport>> = None;
        #[cfg(feature = "steam")]
        let mut steam_cli_ls: Option<net::lockstep::ClientLockstep<net_steam::SteamTransport>> = None;
        #[cfg(feature = "steam")]
        let mut steam_my_index: u8 = 0;
        #[cfg(feature = "steam")]
        let steam_my_id: u64 = 0;
        #[cfg(feature = "steam")]
        let steam_cli_stale_ticks: u64 = 0;
        #[cfg(feature = "steam")]
        let steam_participants: Vec<u64> = Vec::new();
        #[cfg(feature = "steam")]
        let steam_online: Vec<u64> = Vec::new();
        #[cfg(feature = "steam")]
        let steam_migrating: bool = false;
        #[cfg(feature = "steam")]
        let steam_migrate_ticks: u64 = 0;
        #[cfg(feature = "steam")]
        let steam_new_host_id: u64 = 0;
        #[cfg(feature = "steam")]
        let steam_host_broadcasting_takeover: bool = false;
        #[cfg(feature = "steam")]
        let mut steam_roster: Vec<(u8, String, u64)> = Vec::new();
        #[cfg(feature = "steam")]
        let steam_in_lobby_flag = matches!(app, AppState::SteamHost { .. }) || matches!(app, AppState::SteamJoin { .. });
        // 主菜单/单机试验场：仅 1 个玩家且无 AI；Solo 也是 1 玩家无 AI。
        #[cfg(feature = "steam")]
        let mut init_rounds: u32 = STEAM_DEFAULT_ROUNDS;
        let mut player_count: u32 = 1;
        match app {
            AppState::MainMenu => {}
            AppState::Solo => {}
            #[cfg(feature = "steam")]
            AppState::SteamHost { players } => {
                let mut sess = net_steam::session::SteamSession::init(APP_ID, STEAM_VIRTUAL_PORT)
                    .map_err(ggez::GameError::from)?;
                let lobby = sess.host_create_lobby(players.max(1) as u32, 200).map_err(ggez::GameError::from)?;
                sess.prepare_transport().map_err(ggez::GameError::from)?;
                steam_my_index = sess.my_slot();
                eprintln!("[steam-host] lobby={:?}, my slot={}", lobby.raw(), sess.my_slot());
                // 房间成员名单（昵称）：identities + Friends 昵称。
                {
                    let fr = sess.transport.friends();
                    for (slot, id) in sess.identities() {
                        let name = fr.get_friend(net_steam::steamworks::SteamId::from_raw(id)).name();
                        steam_roster.push((slot, name, id));
                    }
                }
                // 建 HostLockstep<SteamTransport>：总玩家数= host 请求的 players（不是当前唯一成员 1）。
                let n = players.max(1) as usize;
                // 传给 set_client_identities 的身份必须是 client（不含 host 槽 0）：sess.identities() 含 host，需跳过。
                let ids: Vec<Option<u64>> = sess.identities().iter().skip(1).map(|(_, v)| Some(*v)).collect();
                let transport = sess.into_transport();
                let mut host_ls = net::lockstep::HostLockstep::new(transport, n, true);
                host_ls.set_client_identities(&ids);
                steam_host_ls = Some(host_ls);
                player_count = n as u32;
            }
            #[cfg(feature = "steam")]
            AppState::SteamJoin { lobby_id } => {
                let mut sess = net_steam::session::SteamSession::init(APP_ID, STEAM_VIRTUAL_PORT)
                    .map_err(ggez::GameError::from)?;
                let lobby = match lobby_id {
                    Some(id) => sess.join_lobby_by_id(id, 240).map_err(ggez::GameError::from)?,
                    None => sess.client_find_and_join(240).map_err(ggez::GameError::from)?,
                };
                sess.prepare_transport().map_err(ggez::GameError::from)?;
                eprintln!("[steam-join] lobby={:?}, my slot={}", lobby.raw(), sess.my_slot());
                let total = sess.table.as_ref().map(|t| t.total_players()).unwrap_or(2);
                init_rounds = sess.lobby_rounds().unwrap_or(STEAM_DEFAULT_ROUNDS);
                let host_id = sess.host_steam_id().unwrap_or(0);
                let my_slot = sess.my_slot();
                steam_my_index = my_slot;
                // 房间成员名单（昵称）。
                {
                    let fr = sess.transport.friends();
                    for (slot, id) in sess.identities() {
                        let name = fr.get_friend(net_steam::steamworks::SteamId::from_raw(id)).name();
                        steam_roster.push((slot, name, id));
                    }
                }
                let transport = sess.into_transport();
                steam_cli_ls = Some(net::lockstep::ClientLockstep::new(
                    transport,
                    my_slot,
                    net::transport::Peer::Steam { id: host_id, conn: None },
                ));
                player_count = total.max(2) as u32;
            }            AppState::LanJoin { addr } => {
                let mut link: netlink::NetLinkUdp =
                    netlink::NetLink::connect_udp(addr).map_err(ggez::GameError::from)?;
                eprintln!("[client] connecting to {addr}, my stable identity = {}", link.my_identity());
                for _ in 0..60 {
                    if link.join_handshake().map_err(ggez::GameError::from)? {
                        break;
                    }
                }
                player_count = link.player_count().max(1) as u32;
                net_link = Some(link);
            }
            AppState::LanHost { port, total } => {
                let t =
                    net::transport::StdUdpTransport::bind(&format!("0.0.0.0:{port}")).map_err(ggez::GameError::from)?;
                let hs = net::handshake::HostHandshake::new(t, total.max(1) as usize, true);
                player_count = total.max(1) as u32;
                net_host = Some(hs);
            }
        }
        let seed = 20260812u64;
        // Solo 试验场（含主菜单入口）：世界含「你 + 1 个不动靶子」→ 不判结束；meta 只记录你。
        // 这样无论从菜单按 1 进 Solo 还是 --solo 直通，世界都已是 sandbox。
        let mut world = match app {
            AppState::Solo | AppState::MainMenu => {
                let mut w = World::new(2, seed); // player0=你, player1=靶子
                w.sandbox = true;
                eprintln!("[solo] world players={} sandbox={}", w.players.len(), w.sandbox);
                w
            }
            _ => World::new(player_count.max(1), seed),
        };
        let meta_ids: Vec<u32> = match app {
            AppState::Solo | AppState::MainMenu => vec![0],
            _ => (0..player_count).collect(),
        };
        // 整场对抗：3 小局，所有玩家都纳入档案
        let mut meta = MatchState::new(MatchConfig::default(), &meta_ids, 8);
        // 观察/调试 `FASTROUND=1`：缩小场地加速局终、缩短学习时间、多开几局，便于用 netlogs 看多局循环。
        if std::env::var("FASTROUND").is_ok() {
            world.arena_radius = game_core::fix::Fix64::from_num(3.0);
            meta.config.learn_time_secs = 1.0;
            meta.config.total_rounds = 4;
        }
        // 开局不带任何默认技能：完全由玩家在配置/学习界面按字母选树 + 数字绑技能（4.6b/从零选择）。

        // 当前所有模式（Solo/Lan）都不带本地 AI 机器人：Solo 无对手，Lan 是真人玩家。
        let bot_rngs: Vec<Rng> = Vec::new();
        let bot_targets: Vec<Option<Vec2>> = Vec::new();

        let (w, h) = ctx.gfx.drawable_size();
        Ok(Game {
            world,
            meta,
            player_target: None,
            pending_cast: None,
            pending_skill: None,
            queued_cmds: std::collections::VecDeque::new(),
            pending_shift_skill: None,
            pending_clear_signal: false,
            pending_stop_signal: false,
            // 默认首选一棵技能树（第一个键 C），让“按数字键绑技能”立即可用，不必先想到去按字母键选树。
            learn_tree_key: game_core::skill::CastKey::ALL.first().copied(),
            bot_targets,
            bot_rngs,
            accumulator: 0.0,
            scale: 1.0,
            offset: Point2 { x: w / 2.0, y: h / 2.0 },
            net_link,
            net_host,
            net_host_ls,
            #[cfg(feature = "steam")]
            steam_host_ls,
            #[cfg(feature = "steam")]
            steam_cli_ls,
            #[cfg(feature = "steam")]
            steam_my_index,
            #[cfg(feature = "steam")]
            steam_my_id,
            #[cfg(feature = "steam")]
            steam_cli_stale_ticks,
            #[cfg(feature = "steam")]
            steam_participants,
            #[cfg(feature = "steam")]
            steam_online,
            #[cfg(feature = "steam")]
            steam_migrating,
            #[cfg(feature = "steam")]
            steam_migrate_ticks,
            #[cfg(feature = "steam")]
            steam_new_host_id,
            #[cfg(feature = "steam")]
            steam_host_broadcasting_takeover,
            #[cfg(feature = "steam")]
            steam_countdown: 0.0,
            #[cfg(feature = "steam")]
            steam_lobby_wait_ticks: 0,
            #[cfg(feature = "steam")]
            steam_cli_last_seq: 0,
            #[cfg(feature = "steam")]
            steam_last_sent_ready: None,
            #[cfg(feature = "steam")]
            steam_in_lobby: steam_in_lobby_flag,
            #[cfg(feature = "steam")]
            steam_local_ready: false,
            #[cfg(feature = "steam")]
            steam_roster,
            #[cfg(feature = "steam")]
            steam_roster_ready: Vec::new(),
            #[cfg(feature = "steam")]
            steam_all_ready: false,
            #[cfg(feature = "steam")]
            steam_build_done: false,
            #[cfg(feature = "steam")]
            steam_was_all_ready: false,
            #[cfg(feature = "steam")]
            steam_lobby_menu: false,
            #[cfg(feature = "steam")]
            steam_lobby_selection: 0,
            #[cfg(feature = "steam")]
            steam_lobby_create: false,
            #[cfg(feature = "steam")]
            steam_lobby_list: false,
            #[cfg(feature = "steam")]
            steam_create_name: "我的房间".to_string(),
            #[cfg(feature = "steam")]
            steam_create_note: String::new(),
            #[cfg(feature = "steam")]
            steam_create_focus: 0,
            #[cfg(feature = "steam")]
            steam_list_lobbies: Vec::new(),
            #[cfg(feature = "steam")]
            steam_list_selection: 0,
            #[cfg(feature = "steam")]
            steam_list_requested: false,
            #[cfg(feature = "steam")]
            steam_list_last_refresh: -999.0,
            #[cfg(feature = "steam")]
            steam_sess: None,
            #[cfg(feature = "steam")]
            steam_my_display_name: String::new(),
            #[cfg(feature = "steam")]
            steam_join_lobby_id: None,
            #[cfg(feature = "steam")]
            steam_room_edit: false,
            #[cfg(feature = "steam")]
            steam_room_edit_focus: 0,
            #[cfg(feature = "steam")]
            steam_edit_name: String::new(),
            #[cfg(feature = "steam")]
            steam_edit_note: String::new(),
            #[cfg(feature = "steam")]
            steam_lobby_id: None,
            #[cfg(feature = "steam")]
            steam_room_locked: false,
            #[cfg(feature = "steam")]
            steam_roster_refresh_ticks: 0,
            #[cfg(feature = "steam")]
            steam_lobby_silent_ticks: 0,
            #[cfg(feature = "steam")]
            steam_last_roster_len: 0,
            #[cfg(feature = "steam")]
            steam_manual_start_pending: false,
            #[cfg(feature = "steam")]
            steam_friend_list: false,
            #[cfg(feature = "steam")]
            steam_friends: Vec::new(),
            #[cfg(feature = "steam")]
            steam_friend_selection: 0,
            #[cfg(feature = "steam")]
            steam_friend_hint: String::new(),
            #[cfg(feature = "steam")]
            steam_presence_last: -999.0,
            #[cfg(feature = "steam")]
            steam_presence_text: String::new(),
            #[cfg(feature = "steam")]
            steam_session_tried: false,
            #[cfg(feature = "steam")]
            steam_pings: Vec::new(),
            #[cfg(feature = "steam")]
            steam_avatars: Vec::new(),
            #[cfg(feature = "steam")]
            steam_net_ticks: 0,
            #[cfg(feature = "steam")]
            steam_lb_slot: net_steam::session::shared(None),
            #[cfg(feature = "steam")]
            steam_lb_rows: net_steam::session::shared(Vec::new()),
            #[cfg(feature = "steam")]
            steam_lb_requested: false,
            #[cfg(feature = "steam")]
            steam_stats_recorded: false,
            #[cfg(feature = "steam")]
            steam_stats_snapshot: None,
            #[cfg(feature = "steam")]
            steam_toast: (String::new(), 0.0),
            net_ready: false,
            net_cfg: NetCfgSync::Idle,
            app,
            pre_game_config: app != AppState::MainMenu,
            conn_dropped: false,
            reconnect_attempting: false,
            host_frame_count: 0,
            pre_game_timer: PRE_GAME_TIMEOUT_SECS,
            menu_selection: 0,
            #[cfg(feature = "steam")]
            steam_create_players: STEAM_DEFAULT_PLAYERS,
            #[cfg(feature = "steam")]
            steam_create_rounds: STEAM_DEFAULT_ROUNDS,
            #[cfg(feature = "steam")]
            steam_create_players_buf: STEAM_DEFAULT_PLAYERS.to_string(),
            #[cfg(feature = "steam")]
            steam_create_rounds_buf: STEAM_DEFAULT_ROUNDS.to_string(),
            #[cfg(feature = "steam")]
            match_rounds: init_rounds,
        })
    }

    fn update_camera(&mut self, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        // 令初始场地（半径约 20）约占较短边的 45%
        self.scale = sw.min(sh) * 0.45 / 20.0;
        self.offset.x = sw / 2.0;
        self.offset.y = sh / 2.0;
        Ok(())
    }

    /// 屏幕坐标 → 世界坐标。
    fn screen_to_world(&self, x: f32, y: f32) -> Vec2 {
        Vec2::new(
            Fix64::from_num((x - self.offset.x) / self.scale),
            Fix64::from_num((y - self.offset.y) / self.scale),
        )
    }

    /// 学习阶段交互：
    /// - 按字母键 → 选中该键对应的技能树（learn_tree_key）
    /// - 按数字键 1..N → 把选中的树里的第 N 个技能绑定到该键
    /// - 按 `=` → 升级当前键绑定的技能
    /// - 按 `X` → 洗点（全额退款 + 清空绑定）
    fn poll_learning(&mut self, ctx: &Context) {
        use ggez::input::keyboard::Key;

        // 字母键：选中技能树
        for (letter, key) in KEY_LETTERS {
            if ctx
                .keyboard
                .is_logical_key_just_pressed(&Key::Character(letter.into()))
            {
                eprintln!("[learn] select tree '{letter}' (key=Key::Character)");
                self.learn_tree_key = Some(key);
            }
        }

        let learn_key = self.learn_tree_key;

        // 数字键：绑定选中树里的第 N 个技能
        if let Some(key) = learn_key {
            for (i, skill) in key.tree().skills_in_tree().iter().enumerate() {
                let digit = char::from(b'1' + i as u8); // '1','2',...
                if ctx
                    .keyboard
                    .is_logical_key_just_pressed(&Key::Character(digit.to_string().into()))
                {
                    eprintln!("[learn] bind tree={} digit='{}' -> {}", key.letter(), digit, game_core::skill::DefTable::def(*skill).name);
                    if let Some(profile) = self
                        .meta
                        .profiles
                        .iter_mut()
                        .find(|pr| pr.player_id == PLAYER_ID)
                    {
                        profile.bind_skill(key, *skill);
                    }
                }
            }
        }

        // `=` 键：升级当前选中键绑定的技能
        if ctx.keyboard.is_logical_key_just_pressed(&Key::Character("=".into())) {
            eprintln!("[learn] '=' pressed, learn_tree_key={learn_key:?}");
            if let Some(key) = learn_key {
                if let Some(profile) = self
                    .meta
                    .profiles
                    .iter_mut()
                    .find(|pr| pr.player_id == PLAYER_ID)
                {
                    if let Some(skill) = profile.bound_skill(key) {
                        let cost = upgrade_cost(profile.skill_level(skill));
                        eprintln!("[learn] upgrade {} cost={}", game_core::skill::DefTable::def(skill).name, cost);
                        profile.upgrade_skill(skill, cost);
                    }
                }
            }
        }

        // `X`：洗点（全额退款）
        if ctx.keyboard.is_logical_key_just_pressed(&Key::Character("x".into())) {
            if let Some(profile) = self
                .meta
                .profiles
                .iter_mut()
                .find(|pr| pr.player_id == PLAYER_ID)
            {
                profile.respec(1.0);
            }
            self.learn_tree_key = None;
        }
    }

    /// 4.6b 成长点/属性购买输入：
    /// - `Z`：用金币换 1 成长点。
    /// - `H`=Hp、`J`=Speed、`K`=Armor、`L`=法抗、`;`=击退、`U`=法力上限、`I`=回蓝。
    fn poll_growth_buy(&mut self, ctx: &Context) {
        use ggez::input::keyboard::Key;
        let me = self.self_index();
        let just = |k: &str| {
            ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.into()))
                || ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.to_uppercase().into()))
        };
        let Some(profile) = self.meta.profiles.iter_mut().find(|pr| pr.player_id == me) else {
            return;
        };
        if just("z") && profile.buy_growth_with_gold(GOLD_PER_GROWTH) {
            eprintln!("[attr] 金币→成长点：金币 {}", profile.gold);
        }
        let mut buy = |g: game_core::attribute::GrowthAttr| {
            let cur = profile.attributes.current(g);
            let cost = growth_attr_cost(cur);
            if profile.buy_attribute(g, cost) {
                eprintln!("[attr] 买 {g:?} +1（点 {}", profile.attributes.current(g));
            }
        };
        if just("h") { buy(game_core::attribute::GrowthAttr::Hp); }
        if just("j") { buy(game_core::attribute::GrowthAttr::Speed); }
        if just("k") { buy(game_core::attribute::GrowthAttr::Armor); }
        if just("l") { buy(game_core::attribute::GrowthAttr::SpellResist); }
        if just(";") { buy(game_core::attribute::GrowthAttr::KbResist); }
        if just("u") { buy(game_core::attribute::GrowthAttr::ManaMax); }
        if just("i") { buy(game_core::attribute::GrowthAttr::ManaRegen); }
    }

    /// 本局进行中：结算击杀、名次，进入学习阶段。
    fn settle_round(&mut self) {
        for (killer, _victim) in self.world.take_kills() {
            self.meta.register_kill(killer);
        }
        let placement = self.world.placement();
        self.meta.finish_round(placement);
        // 4.6b：每局给所有玩家发成长点（用于买属性）。
        for profile in self.meta.profiles.iter_mut() {
            profile.add_growth_points(GROWTH_PER_ROUND);
        }
    }

    /// 进入下一局前：把玩家的技能等级从档案同步到世界，并重置世界。
    fn teardown_round_end(&mut self) {
        // 把 meta.profiles 全量同步到 world.players，使所有端下一局的技能等级一致。
        // （联网下 profiles 已经由 host 广播的完整配置统一；单机下按本地各玩家档案设置。）
        for (profile, p) in self.meta.profiles.iter().zip(self.world.players.iter_mut()) {
            for i in 0..p.skill_levels.len().min(profile.skill_levels.len()) {
                p.skill_levels[i] = profile.skill_levels[i];
            }
            // 4.6b：把玩家属性（Hp/移速等）派生到战斗数值（确定性纯函数，跨端/跨局一致）。
            p.apply_attributes(&profile.attributes);
        }
        self.world.reset_round();
        self.player_target = None;
        self.pending_cast = None;
        self.pending_skill = None;
        self.accumulator = 0.0;
    }

    /// 把 host 广播的完整玩家配置（`PlayerCfgAll` entries）应用回本地 `meta.profiles`。
    fn apply_player_cfgs(&mut self, entries: &[(u8, Vec<u8>)]) {
        for (player_index, bytes) in entries {
            if let Some(cfg) = game_core::progress::PlayerConfig::decode(bytes) {
                if let Some(profile) = self.meta.profiles.iter_mut().find(|pr| pr.player_id == *player_index as u32) {
                    cfg.apply_to(profile);
                }
            }
        }
    }

    /// 对局开始时把 world 与 meta 重建为“本局参与玩家数” `p`（不满员时两端角色数量由此一致）：
    /// 参与者被收缩为连续 player 0..p-1（`self.steam_my_index` 由调用方同步更新）。首局开局用 seed 从零建。
    #[cfg(feature = "steam")]
    fn stage_world_for_participants(&mut self, p: usize, seed: u64) {
        self.world = game_core::world::World::new(p.max(1) as u32, seed);
        let cfg = game_core::meta::MatchConfig { total_rounds: self.match_rounds, ..Default::default() };
        self.meta = game_core::meta::MatchState::new(
            cfg,
            &(0..p.max(1)).map(|i| i as u32).collect::<Vec<u32>>(),
            8,
        );
        eprintln!("[steam] staged world for {p} participant player(s)");
    }

    /// 每帧统一轮询输入（键盘 + 鼠标都用 ggez 的 just-pressed 边沿检测）。
    fn poll_input(&mut self, ctx: &Context) {
        use ggez::input::keyboard::Key;
        use ggez::input::mouse::MouseButton;

        // 玩家档案（本帧只读绑定的技能）
        let me = self.self_index();
        let bound_for = |key: game_core::skill::CastKey| -> Option<SkillId> {
            self.meta
                .profiles
                .iter()
                .find(|pr| pr.player_id == me)
                .and_then(|p| p.bound_skill(key))
        };

        // shift 按住 = 预排队列模式（winit ModifiersState::shift_key()）。
        let shift = ctx.keyboard.active_modifiers.shift_key();

        // 1) 技能键：按下 → 施放该键绑定的技能（shift 时入列）
        // 注意：winit 对 shift+字母会给大写逻辑字符，故按大小写都匹配，否则 shift+技能无法触发。
        for (letter, key) in KEY_LETTERS {
            let lower = letter.to_string();
            let upper = letter.to_uppercase().to_string();
            let just = ctx.keyboard.is_logical_key_just_pressed(&Key::Character(lower.into()))
                || ctx.keyboard.is_logical_key_just_pressed(&Key::Character(upper.into()));
            if just {
                if let Some(skill) = bound_for(key) {
                    if game_core::skill::DefTable::def(skill).needs_point {
                        if shift {
                            self.player_target = None; // 队列操作：放弃即时移动目标，避免覆盖队列移动
                            self.pending_shift_skill = Some(skill);
                        } else {
                            self.pending_skill = Some(skill); // 等左键确认
                        }
                    } else if shift {
                        self.player_target = None;
                        self.queued_cmds.push_back(game_core::player::Cmd::Cast(skill, None));
                    } else {
                        self.pending_cast = Some((skill, None)); // 无需目标，直接施放
                        self.pending_clear_signal = true; // 普通施法打断已排的队列
                    }
                }
            }
        }
        // S: 停止移动 + 清空 shift 队列（含 World 里的队列）—— 同样大小写都匹配
        let s_pressed = ctx.keyboard.is_logical_key_pressed(&Key::Character("s".into()))
            || ctx.keyboard.is_logical_key_pressed(&Key::Character("S".into()));
        if s_pressed {
            self.player_target = None;
            self.pending_skill = None;
            self.pending_cast = None;
            self.pending_shift_skill = None;
            self.queued_cmds.clear();
            self.pending_clear_signal = true; // 让 World 也清空其命令队列
            self.pending_stop_signal = true;  // 让 World 停止当前移动
        }

        // 2) 左键：确认点目标技能（cursor 位置作为落点）
        if ctx.mouse.button_just_pressed(MouseButton::Left) {
            let m = ctx.mouse.position();
            let world = self.screen_to_world(m.x, m.y);
            if let Some(skill) = self.pending_skill.take() {
                self.player_target = None;
                self.pending_cast = Some((skill, Some(world)));
                self.pending_clear_signal = true; // 普通施法打断已排的队列
            } else if let Some(skill) = self.pending_shift_skill.take() {
                self.player_target = None;
                self.queued_cmds.push_back(game_core::player::Cmd::Cast(skill, Some(world)));
            }
        }

        // 3) 右键：设置移动目标（shift 时入列；普通右键即时移动会打断队列）
        if ctx.mouse.button_just_pressed(MouseButton::Right) {
            self.pending_skill = None;
            self.pending_cast = None;
            self.pending_shift_skill = None;
            let m = ctx.mouse.position();
            let world = self.screen_to_world(m.x, m.y);
            if shift {
                self.player_target = None; // 放弃即时移动，改为排队列
                self.queued_cmds.push_back(game_core::player::Cmd::Move(world));
            } else {
                self.queued_cmds.clear(); // 普通即时移动：打断并清空之前排的队列
                self.pending_clear_signal = true; // 也让 World 清空其队列
                self.player_target = Some(world);
            }
        }
    }

    /// 本局运行模式已并入 `AppState`（主菜单 / Solo / 局域网主机 / 局域网加入）。
    /// Steam 模式是否激活（host 或 client 任一）。默认构建恒 false。
    fn steam_active(&self) -> bool {
        #[cfg(feature = "steam")]
        {
            self.steam_host_ls.is_some() || self.steam_cli_ls.is_some()
        }
        #[cfg(not(feature = "steam"))]
        {
            false
        }
    }

    /// 本机玩家在该次对局中的序号：单机/host 恒为 0，加入者为握手分配到的 `my_index`；Steam 用大厅槽位。
    fn self_index(&self) -> u32 {
        #[cfg(feature = "steam")]
        if self.steam_cli_ls.is_some() || self.steam_host_ls.is_some() {
            return self.steam_my_index as u32;
        }
        match &self.net_link {
            Some(l) => l.my_index() as u32,
            None => PLAYER_ID,
        }
    }

    /// 生成「本机玩家最终配置快照」的编码字节（学习阶段结束/就绪时上报给 host）。
    fn local_player_cfg(&self) -> Vec<u8> {
        let me = self.self_index();
        match self.meta.profiles.iter().find(|pr| pr.player_id == me) {
            Some(p) => game_core::progress::PlayerConfig::from_profile(p).encode(),
            None => Vec::new(), // 异常：不应发生；空配置
        }
    }

    /// 生成本（本机玩家）这一帧要下达的命令（移动 / 施法 / shift 队列 / 清队 / 停止）。
    /// 单机模式把它放到 `PLAYER_ID`；联网模式由 `NetLink` 上行给 host、按我的序号归位。
    fn local_player_input(&mut self) -> PlayerInput {
        let set_target = self.player_target;
        let cast = self.pending_cast;
        let queued = self.queued_cmds.drain(..).collect();
        let clear_queue = self.pending_clear_signal;
        let stop_move = self.pending_stop_signal;
        self.pending_clear_signal = false;
        self.pending_stop_signal = false;
        PlayerInput {
            set_target,
            cast,
            queued,
            clear_queue,
            stop_move,
        }
    }

    /// 生成本（模拟）帧内所有玩家的输入（单机：本机玩家 + 本地 AI 机器人）。
    fn compute_inputs(&mut self) -> Vec<PlayerInput> {
        let mut inputs: Vec<PlayerInput> = self
            .world
            .players
            .iter()
            .map(|_| PlayerInput::default())
            .collect();

        // 玩家本人
        let me = self.local_player_input();
        inputs[PLAYER_ID as usize] = me;

        // 机器人确定性 AI：需要新目标时从自身随机源挑一个场地内的点。
        let arena = self.world.arena_radius;
        for (i, input) in inputs.iter_mut().enumerate().skip(1) {
            let bot_idx = i - 1;
            let needs_new = match self.bot_targets[bot_idx] {
                None => true,
                Some(t) => {
                    let d = self.world.players[i].pos - t;
                    d.length_squared() <= Fix64::from_num(0.1)
                }
            };
            if needs_new {
                let r = arena * Fix64::from_num(0.8);
                let a = self.bot_rngs[bot_idx].next_fix() * Fix64::from_num(std::f64::consts::TAU);
                let t = Vec2::new(r * cos(a), r * sin(a));
                self.bot_targets[bot_idx] = Some(t);
            }
            input.set_target = self.bot_targets[bot_idx];
        }

        inputs
    }

    /// 客户端掉线后的重连流程：按 R 发起重连 → 向 host 拉快照 → 重建 World 并对齐基线 → 恢复。
    /// 在该状态下不推进世界（冻结，避免与 host 分叉），只等待重连成功或玩家放弃。
    fn poll_reconnect(&mut self, ctx: &mut Context, link: &mut netlink::NetLinkUdp) {
        use ggez::input::keyboard::Key;
        let r_pressed = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("r".into()))
            || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("R".into()));
        if !self.reconnect_attempting && !r_pressed {
            return; // 未按 R，不发起重连，保持空闲等待。
        }
        if !self.reconnect_attempting {
            self.reconnect_attempting = true;
            eprintln!("[client] reconnect flow: sending ReconnectReq...");
        }
        match link.try_reconnect() {
            Ok(Some((world_bytes, seq))) => {
                eprintln!("[client] got Snapshot seq={seq}, rebuilding World ({n} bytes)", n = world_bytes.len());
                link.align_after_reconnect().ok();
                match game_core::world_ser::world_from_bytes(&world_bytes) {
                    Some(w) => {
                        self.world = w;
                        // 清空本地输入残留，避免把掉线期间的输入误带到接回后。
                        self.player_target = None;
                        self.pending_cast = None;
                        self.pending_skill = None;
                        self.queued_cmds.clear();
                        self.pending_shift_skill = None;
                        self.pending_clear_signal = false;
                        self.pending_stop_signal = false;
                        self.conn_dropped = false;
                        self.reconnect_attempting = false;
                        // 重连一次成功即进入等待；重连接完毕后靠下一帧的权威帧驱动（stale 已归零）。
                        eprintln!("[client] reconnected: World rebuilt from snapshot, resuming lockstep");
                    }
                    None => {
                        eprintln!("[client] failed to decode snapshot, retrying on next keypress");
                        self.reconnect_attempting = false;
                    }
                }
            }
            Ok(None) => {
                // 尚未收到快照：保持等待（下帧再试）。
            }
            Err(e) => {
                eprintln!("[client] reconnect error: {e:?}");
                self.reconnect_attempting = false;
            }
        }
    }

    /// Steam client 战斗端掉线后的重连入口（对齐局域网 `poll_reconnect`，但直接操作 `steam_cli_ls`）。
    /// 按 R 触发：发 `ReconnectReq`(带本机 SteamID) → host 应答 `Snapshot` → 重建 World → `apply_resync` 对齐续打。
    #[cfg(feature = "steam")]
    fn poll_steam_reconnect(&mut self, ctx: &Context, cli: &mut net::lockstep::ClientLockstep<net_steam::SteamTransport>) {
        use ggez::input::keyboard::Key;
        let r_pressed = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("r".into()))
            || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("R".into()));
        if !self.reconnect_attempting && !r_pressed {
            return; // 未按 R，不发起重连，保持空闲等待。
        }
        if !self.reconnect_attempting {
            self.reconnect_attempting = true;
            eprintln!("[steam-client] reconnect flow: sending ReconnectReq...");
        }
        // 发重连请求（带本机 SteamID 作稳定身份，host 按身份找回槽位）。
        if cli.send_reconnect_req(self.steam_my_id).is_err() {
            eprintln!("[steam-client] reconnect send failed");
            self.reconnect_attempting = false;
            return;
        }
        let mut rcv = vec![0u8; 8192];
        match cli.recv_snapshot(&mut rcv) {
            Ok(Some((world_bytes, seq))) => {
                eprintln!("[steam-client] got Snapshot seq={seq}, rebuilding World ({n} bytes)", n = world_bytes.len());
                cli.apply_resync(&mut rcv).ok();
                match game_core::world_ser::world_from_bytes(&world_bytes) {
                    Some(w) => {
                        self.world = w;
                        // 清空本地输入残留，避免把掉线期间的输入误带到接回后。
                        self.player_target = None;
                        self.pending_cast = None;
                        self.pending_skill = None;
                        self.queued_cmds.clear();
                        self.pending_shift_skill = None;
                        self.pending_clear_signal = false;
                        self.pending_stop_signal = false;
                        self.steam_cli_stale_ticks = 0;
                        self.conn_dropped = false;
                        self.reconnect_attempting = false;
                        eprintln!("[steam-client] reconnected: World rebuilt from snapshot, resuming lockstep");
                    }
                    None => {
                        eprintln!("[steam-client] failed to decode snapshot, retrying on next keypress");
                        self.reconnect_attempting = false;
                    }
                }
            }
            Ok(None) => {
                // 尚未收到快照：保持等待（下帧再试）。
            }
            Err(e) => {
                eprintln!("[steam-client] reconnect error: {e:?}");
                self.reconnect_attempting = false;
            }
        }
    }

    /// Steam（client）主机迁移状态机：每帧在「收不到权威帧、疑似 host 掉线」后调用。
    /// 分两阶段：
    ///  A) 探测 host 是否还在：发 `ReconnectReq` 等 Snapshot 应答；收到则（host 还在）恢复对局；超时则判定 host 掉线。
    ///  B) 判定 host 掉线后：用 `steam_participants`（排除旧 host、SteamID 最小者）确定性选举同一新 host。
    ///     - 本端是新 host → `steam_do_takeover`（ClientLockstep 转 HostLockstep，广播 Takeover+Snapshot 接管）。
    ///     - 本端不是 → 等待新 host 的 `Takeover`，收到后重定向 + 用其快照重建 + `apply_resync` 对齐续打。
    #[cfg(feature = "steam")]
    fn poll_steam_migration(
        &mut self,
        mut cli: net::lockstep::ClientLockstep<net_steam::SteamTransport>,
        rcv: &mut [u8],
    ) -> GameResult<Option<net::lockstep::ClientLockstep<net_steam::SteamTransport>>> {
        self.steam_migrate_ticks = self.steam_migrate_ticks.saturating_add(1);
        // —— 阶段 A：探测 host 是否还在（尚未决定新 host）。
        if self.steam_new_host_id == 0 {
            let _ = cli.send_reconnect_req(self.steam_my_id);
            let old_host = cli.host_peer();
            // 只接受「来自旧 host」的包作为 host 还活着的证据：
            // 否则新 host（接管后）广播的 Snapshot 会被误判成“旧 host 还活着” → 恢复却不重定向 → 永远连旧 host。
            if let Ok(Some((from, pkt))) = cli.recv_packet(rcv) {
                if from == old_host {
                    if let net::Packet::Snapshot { world_bytes, seq } = pkt {
                        cli.apply_resync(rcv).ok();
                        if let Some(w) = game_core::world_ser::world_from_bytes(&world_bytes) {
                            self.world = w;
                            self.clear_transient_input();
                            eprintln!("[steam-client] host alive, resumed from snapshot seq={seq}");
                        }
                        self.steam_migrating = false;
                        self.steam_migrate_ticks = 0;
                        return Ok(Some(cli));
                    }
                    // 来自旧 host 的其它包（如 Frame）也说明 host 还在：恢复，交给下一帧正常循环推进（丢帧由 lockstep 补发）。
                    self.steam_migrating = false;
                    self.steam_migrate_ticks = 0;
                    eprintln!("[steam-client] host alive (heartbeat from old host), resuming");
                    return Ok(Some(cli));
                }
                // 来自非旧 host（如新 host）的包：忽略，继续探测/等待。
            }
            if self.steam_migrate_ticks >= MIGRATE_PROBE_TICKS {
                // 判定 host 掉线 → 确定性选举新 host（排除当前 host、SteamID 最小者）。
                // 候选集用 `steam_online`（已排除历次掉线的 host），避免把已掉线的旧 host 再选出。
                let old_host_id = match old_host {
                    net::transport::Peer::Steam { id, .. } => id,
                    _ => 0,
                };
                let candidates: Vec<u64> = self.steam_online.iter().filter(|&&id| id != old_host_id).copied().collect();
                let new_host_id = candidates.iter().min().copied().unwrap_or(0);
                self.steam_new_host_id = new_host_id;
                eprintln!(
                    "[steam-client] host gone (probe timeout), elected new host={new_host_id} (I {}), online={:?}",
                    if new_host_id == self.steam_my_id { "am new host" } else { "am client" },
                    self.steam_online
                );
            }
            return Ok(Some(cli));
        }
        // —— 阶段 B：已决定新 host。我是新 host → 接管（消费 cli）；否则等 Takeover。
        if self.steam_new_host_id == self.steam_my_id {
            self.steam_do_takeover(cli, rcv)?;
            Ok(None)
        } else {
            // 优先用 fighting 阶段缓存的 Takeover，否则从传输收（新 host 会持续广播直到首个 client 连上）。
            let takeover = cli.take_latest_takeover().or_else(|| cli.recv_takeover(rcv).ok().flatten());
            if let Some((from, seq, online)) = takeover {
                // 收到新 host 的 Takeover → 重定向 + 用其快照重建 + 对齐续打；并同步更新在线参与集。
                self.steam_online = online; // 排除掉线 host 后的在线参与集（供下一次迁移选举）
                cli.retarget_host(from);
                if let Ok(Some((wb, _))) = cli.recv_snapshot(rcv) {
                    if let Some(w) = game_core::world_ser::world_from_bytes(&wb) {
                        self.world = w;
                        self.clear_transient_input();
                    }
                }
                cli.apply_resync(rcv).ok();
                self.steam_migrating = false;
                self.steam_migrate_ticks = 0;
                self.steam_new_host_id = 0;
                eprintln!("[steam-client] migrated to new host (seq={seq}), resuming lockstep");
                Ok(Some(cli))
            } else {
                Ok(Some(cli))
            }
        }
    }

    /// 迁移接管：本端被选为新 host。把原 client lockstep 转为 host lockstep，从缓存快照续打，
    /// 广播 `Takeover`+`Snapshot` 让其余端重定向并对齐。
    /// 用**原始** `steam_participants` 定位 world index（对局开始时确定、迁移不变）与掉线旧 host 的 index；
    /// 用 `steam_online`（排除掉线 host）作为选举/广播的新在线集，保证下一次迁移仍能正确选新 host。
    #[cfg(feature = "steam")]
    fn steam_do_takeover(&mut self, cli: net::lockstep::ClientLockstep<net_steam::SteamTransport>, _rcv: &mut [u8]) -> GameResult {
        // 取本端缓存的快照重建 world（迁移基线）。
        let snap = cli.cached_snapshot();
        let old_host_id = match cli.host_peer() {
            net::transport::Peer::Steam { id, .. } => id,
            _ => 0,
        };
        // 本端 world index = 在原始参与列表中的位置（对局开始时确定，迁移不变）。
        let my_index = self.steam_participants.iter().position(|&id| id == self.steam_my_id).unwrap_or(0) as u8;
        let total = self.steam_participants.len().max(1);
        if let Some((wb, _)) = &snap {
            if let Some(w) = game_core::world_ser::world_from_bytes(wb) {
                self.world = w;
            }
        }
        // 更新在线参与集：排除掉线的旧 host（供下一次迁移选举）。
        let new_online: Vec<u64> = self.steam_online.iter().filter(|&&id| id != old_host_id).copied().collect();
        self.steam_online = new_online.clone();
        // 其余参与端（按原始 world index）；不在新在线集里的玩家（历次掉线的 host）用默认输入占位。
        let mut other_indices = Vec::new();
        let mut peers = Vec::new();
        let mut dropped = Vec::new();
        let mut identities = Vec::new();
        for i in 0..total {
            let iu = i as u8;
            if iu != my_index {
                other_indices.push(iu);
                let gone = !new_online.contains(&self.steam_participants[i]); // 掉线占位
                peers.push(if gone {
                    None
                } else {
                    Some(net::transport::Peer::Steam { id: self.steam_participants[i], conn: None })
                });
                dropped.push(gone);
                identities.push(Some(self.steam_participants[i]));
            }
        }
        let mut host = net::lockstep::HostLockstep::takeover(
            cli,
            my_index,
            total,
            other_indices,
            peers,
            dropped,
            identities,
        );
        // 广播 Takeover（带更新后的在线参与集）+ Snapshot（接管基线）给其余在线端。
        let seq = host.next_seq();
        if let Some((wb, _)) = &snap {
            host.broadcast_takeover(seq, new_online.clone());
            host.broadcast_snapshot(wb.clone(), seq);
        }
        self.steam_my_index = my_index;
        eprintln!("[steam-host] TAKEOVER: I am new host (player {my_index}/{total}), resume seq={seq}, online={new_online:?}");
        self.steam_host_ls = Some(host);
        self.steam_cli_ls = None;
        self.steam_migrating = false;
        self.steam_migrate_ticks = 0;
        self.steam_new_host_id = 0;
        // 接管后持续广播 Takeover，直到首个在线 client 连上（产帧成功）才停，避免晚进入迁移的 client 错过。
        self.steam_host_broadcasting_takeover = true;
        Ok(())
    }

    /// 清空本机临时的输入/目标残留（重连/迁移重建世界后用，避免把掉线期间的输入误带到接回后）。
    #[cfg(feature = "steam")]
    fn clear_transient_input(&mut self) {
        self.player_target = None;
        self.pending_cast = None;
        self.pending_skill = None;
        self.queued_cmds.clear();
        self.pending_shift_skill = None;
        self.pending_clear_signal = false;
        self.pending_stop_signal = false;
    }

    fn draw_scene(&mut self, ctx: &mut Context) -> GameResult {
        self.update_camera(ctx)?;
        let mut canvas = Canvas::from_frame(ctx, Color::from_rgb(18, 22, 34));

        // 瞄准指示：从玩家到鼠标的画一条线（点目标技能待左键确认）。
        if self.pending_skill.is_some() || self.pending_shift_skill.is_some() {
            if let Some(p) = self.world.players.get(self.self_index() as usize) {
                let pfx = p.pos.x.to_num::<f32>() * self.scale + self.offset.x;
                let pfy = p.pos.y.to_num::<f32>() * self.scale + self.offset.y;
                let mouse = ctx.mouse.position();
                let aimline = Mesh::new_line(
                    &ctx.gfx,
                    &[
                        Point2 { x: pfx, y: pfy },
                        Point2 { x: mouse.x, y: mouse.y },
                    ],
                    2.0,
                    Color::from_rgba(255, 220, 120, 190),
                )?;
                canvas.draw(&aimline, graphics::DrawParam::new());
                let aimdot = Mesh::new_circle(
                    &ctx.gfx,
                    DrawMode::fill(),
                    Point2 { x: mouse.x, y: mouse.y },
                    5.0,
                    0.5,
                    Color::from_rgba(255, 220, 120, 220),
                )?;
                canvas.draw(&aimdot, graphics::DrawParam::new());
            }
        }

        // 场地绳圈（逐渐收缩）
        let ar = self.world.arena_radius.to_num::<f32>();
        let fence = Mesh::new_circle(
            &ctx.gfx,
            DrawMode::stroke(3.0),
            self.offset,
            ar * self.scale,
            0.5,
            Color::from_rgb(120, 130, 160),
        )?;
        canvas.draw(&fence, graphics::DrawParam::new());

        // 障碍（圆形柱子：实心浅色圆 + 描边）
        for o in self.world.obstacles.iter() {
            let ox = o.pos.x.to_num::<f32>() * self.scale + self.offset.x;
            let oy = o.pos.y.to_num::<f32>() * self.scale + self.offset.y;
            let or = (o.radius.to_num::<f32>() * self.scale).max(3.0);
            let pillar = Mesh::new_circle(
                &ctx.gfx,
                DrawMode::fill(),
                Point2 { x: ox, y: oy },
                or,
                0.5,
                Color::from_rgb(70, 76, 96),
            )?;
            canvas.draw(&pillar, graphics::DrawParam::new());
            let pillar_edge = Mesh::new_circle(
                &ctx.gfx,
                DrawMode::stroke(2.0),
                Point2 { x: ox, y: oy },
                or,
                0.5,
                Color::from_rgb(120, 130, 160),
            )?;
            canvas.draw(&pillar_edge, graphics::DrawParam::new());
        }

        // 玩家圆与 HP 条
        let me_idx = self.self_index();
        for p in self.world.players.iter() {
            if !p.alive {
                continue;
            }
            let fx = p.pos.x.to_num::<f32>() * self.scale + self.offset.x;
            let fy = p.pos.y.to_num::<f32>() * self.scale + self.offset.y;
            let r = p.radius.to_num::<f32>() * self.scale;
            let mut color = player_color(p.id, me_idx);
            // 潜行：半透明（潜行踢 / 隐蔽效果）
            if p.stealth() {
                color.a = 0.4;
            }
            let ball = Mesh::new_circle(
                &ctx.gfx,
                DrawMode::fill(),
                Point2 { x: fx, y: fy },
                r.max(2.0),
                0.5,
                color,
            )?;
            canvas.draw(&ball, graphics::DrawParam::new());

            // 护盾：外圈浅色圆环（护盾激活时）
            if p.shield() {
                let sring = Mesh::new_circle(
                    &ctx.gfx,
                    DrawMode::stroke(3.0),
                    Point2 { x: fx, y: fy },
                    r + 6.0,
                    0.5,
                    Color::from_rgba(120, 210, 255, 190),
                )?;
                canvas.draw(&sring, graphics::DrawParam::new());
            }

            // 施法前摇提示：头顶一个不断消失的圆环（越接近完成越细小）
            if let game_core::skill::CastPhase::Windup { remaining, .. } = p.caster.phase() {
                let ring = Mesh::new_circle(
                    &ctx.gfx,
                    DrawMode::stroke(3.0),
                    Point2 { x: fx, y: fy },
                    r + 8.0 + (remaining.to_num::<f32>() * 120.0), // 前摇开始时大，结束时小
                    0.3,
                    Color::from_rgba(255, 180, 80, 200),
                )?;
                canvas.draw(&ring, graphics::DrawParam::new());
            }

            // HP 条
            let bw = (r * 2.0).max(16.0);
            let ratio = (p.hp / p.max_hp).to_num::<f32>().clamp(0.0, 1.0);
            let y_bar = fy - r - 12.0;
            let bg = Mesh::new_rectangle(
                &ctx.gfx,
                DrawMode::fill(),
                graphics::Rect::new(fx - bw / 2.0, y_bar, bw, 5.0),
                Color::from_rgba(10, 12, 18, 190),
            )?;
            let fg = Mesh::new_rectangle(
                &ctx.gfx,
                DrawMode::fill(),
                graphics::Rect::new(fx - bw / 2.0, y_bar, bw * ratio, 5.0),
                hp_color(ratio),
            )?;
            canvas.draw(&bg, graphics::DrawParam::new());
            canvas.draw(&fg, graphics::DrawParam::new());
        }

        // 飞行物（石头：显影半径提示延时区；幻象假身：淡色假圆）
        for pr in self.world.projectiles.iter() {
            let px = pr.pos.x.to_num::<f32>() * self.scale + self.offset.x;
            let py = pr.pos.y.to_num::<f32>() * self.scale + self.offset.y;
            match pr.kind {
                game_core::world::ProjectileKind::Rock { radius, .. } => {
                    let color = if pr.alive {
                        Color::from_rgba(230, 120, 60, 200)
                    } else {
                        Color::from_rgba(120, 60, 30, 160)
                    };
                    let pmesh = Mesh::new_circle(
                        &ctx.gfx,
                        DrawMode::stroke(2.0),
                        Point2 { x: px, y: py },
                        radius.to_num::<f32>() * self.scale,
                        0.5,
                        color,
                    )?;
                    canvas.draw(&pmesh, graphics::DrawParam::new());
                    let dot = Mesh::new_circle(
                        &ctx.gfx,
                        DrawMode::fill(),
                        Point2 { x: px, y: py },
                        4.0,
                        0.5,
                        Color::from_rgb(240, 140, 70),
                    )?;
                    canvas.draw(&dot, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Decoy { radius, .. } => {
                    // 幻象假身：淡色、接近玩家颜色的圆
                    let dec = Mesh::new_circle(
                        &ctx.gfx,
                        DrawMode::fill(),
                        Point2 { x: px, y: py },
                        radius.to_num::<f32>() * self.scale,
                        0.5,
                        Color::from_rgba(150, 160, 200, 120),
                    )?;
                    canvas.draw(&dec, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Bullet { dir, radius, .. } => {
                    // 直射弹：亮色小球，沿 dir 方向画一个速度小尾巴
                    let dirv = dir;
                    let tip_x = px + dirv.x.to_num::<f32>() * 6.0;
                    let tip_y = py + dirv.y.to_num::<f32>() * 6.0;
                    let tail = Mesh::new_line(
                        &ctx.gfx,
                        &[Point2 { x: px, y: py }, Point2 { x: tip_x, y: tip_y }],
                        2.0,
                        Color::from_rgba(255, 200, 80, 160),
                    )?;
                    canvas.draw(&tail, graphics::DrawParam::new());
                    let b = Mesh::new_circle(
                        &ctx.gfx,
                        DrawMode::fill(),
                        Point2 { x: px, y: py },
                        (radius.to_num::<f32>() * self.scale).max(4.0),
                        0.5,
                        Color::from_rgb(255, 210, 90),
                    )?;
                    canvas.draw(&b, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::PushBullet { dir, radius, .. } => {
                    // 撞击迟缓弹：暖橙龟球 + 大效果提示
                    let d = dir;
                    let tx = px + d.x.to_num::<f32>() * 8.0;
                    let ty = py + d.y.to_num::<f32>() * 8.0;
                    let tail = Mesh::new_line(&ctx.gfx, &[Point2 { x: px, y: py }, Point2 { x: tx, y: ty }], 3.0, Color::from_rgba(255, 150, 60, 190))?;
                    canvas.draw(&tail, graphics::DrawParam::new());
                    let b = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, (radius.to_num::<f32>() * self.scale).max(5.0), 0.5, Color::from_rgb(255, 160, 80))?;
                    canvas.draw(&b, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Missile { dir, radius, .. } => {
                    // 追踪导弹：深色大球 + 朝向小三角
                    let m = Mesh::new_circle(
                        &ctx.gfx,
                        DrawMode::fill(),
                        Point2 { x: px, y: py },
                        (radius.to_num::<f32>() * self.scale).max(5.0),
                        0.5,
                        Color::from_rgb(230, 90, 90),
                    )?;
                    canvas.draw(&m, graphics::DrawParam::new());
                    let d = dir;
                    let dx = d.x.to_num::<f32>();
                    let dy = d.y.to_num::<f32>();
                    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
                    let (ux, uy) = (dx / len, dy / len);
                    let tip = Point2 { x: px + ux * 12.0, y: py + uy * 12.0 };
                    let back = Point2 { x: px, y: py };
                    let line = Mesh::new_line(
                        &ctx.gfx,
                        &[back, tip],
                        3.0,
                        Color::from_rgb(240, 120, 60),
                    )?;
                    canvas.draw(&line, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Beam { dir, length, width, .. } => {
                    // 激光线：从 pr.pos 朝 dir 延伸 length 的亮色线段
                    let d = dir;
                    let fx = d.x.to_num::<f32>();
                    let fy = d.y.to_num::<f32>();
                    let f = length.to_num::<f32>() * self.scale;
                    let tip = Point2 { x: px + fx * f, y: py + fy * f };
                    let beam = Mesh::new_line(
                        &ctx.gfx,
                        &[Point2 { x: px, y: py }, tip],
                        (width.to_num::<f32>() * self.scale).max(3.0),
                        Color::from_rgba(150, 230, 255, 200),
                    )?;
                    canvas.draw(&beam, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Rolling { dir, radius, .. } => {
                    // 滚动火球：暖色球 + 旋转小拖尾（示意在滚动）
                    let d = dir;
                    let tx = px + d.x.to_num::<f32>() * 10.0;
                    let ty = py + d.y.to_num::<f32>() * 10.0;
                    let tail = Mesh::new_line(&ctx.gfx, &[Point2 { x: px, y: py }, Point2 { x: tx, y: ty }], 3.0, Color::from_rgba(255, 140, 60, 180))?;
                    canvas.draw(&tail, graphics::DrawParam::new());
                    let ball = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, (radius.to_num::<f32>() * self.scale).max(5.0), 0.5, Color::from_rgb(240, 120, 60))?;
                    canvas.draw(&ball, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::ScatterLine { dir, .. } => {
                    // 撒弹线：亮蓝移动小球 + 前方指示
                    let d = dir;
                    let tipx = px + d.x.to_num::<f32>() * 14.0;
                    let tipy = py + d.y.to_num::<f32>() * 14.0;
                    let tl = Mesh::new_line(&ctx.gfx, &[Point2 { x: px, y: py }, Point2 { x: tipx, y: tipy }], 4.0, Color::from_rgba(120, 200, 255, 220))?;
                    canvas.draw(&tl, graphics::DrawParam::new());
                    let dot = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, 6.0, 0.5, Color::from_rgb(90, 180, 255))?;
                    canvas.draw(&dot, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Boomerang { vel, radius, .. } => {
                    // 回旋镖：绿色球 + 沿速度方向的拖尾
                    let dv = vel;
                    let tx = px + dv.x.to_num::<f32>() * 0.4;
                    let ty = py + dv.y.to_num::<f32>() * 0.4;
                    let tail = Mesh::new_line(&ctx.gfx, &[Point2 { x: px, y: py }, Point2 { x: tx, y: ty }], 3.0, Color::from_rgba(120, 230, 140, 200))?;
                    canvas.draw(&tail, graphics::DrawParam::new());
                    let b = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, (radius.to_num::<f32>() * self.scale).max(5.0), 0.5, Color::from_rgb(90, 200, 110))?;
                    canvas.draw(&b, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Banana { dir, radius, .. } => {
                    // 香蕉弹：黄绿色曲线弹
                    let d = dir;
                    let tx = px + d.x.to_num::<f32>() * 12.0;
                    let ty = py + d.y.to_num::<f32>() * 12.0;
                    let tail = Mesh::new_line(&ctx.gfx, &[Point2 { x: px, y: py }, Point2 { x: tx, y: ty }], 3.0, Color::from_rgba(255, 220, 90, 200))?;
                    canvas.draw(&tail, graphics::DrawParam::new());
                    let b = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, (radius.to_num::<f32>() * self.scale).max(5.0), 0.5, Color::from_rgb(240, 200, 60))?;
                    canvas.draw(&b, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Chain { dir, .. } => {
                    // 链镖/跳弹：红色/品红亮球 + 朝向小拖尾
                    let d = dir;
                    let tx = px + d.x.to_num::<f32>() * 10.0;
                    let ty = py + d.y.to_num::<f32>() * 10.0;
                    let tail = Mesh::new_line(&ctx.gfx, &[Point2 { x: px, y: py }, Point2 { x: tx, y: ty }], 3.0, Color::from_rgba(255, 90, 140, 220))?;
                    canvas.draw(&tail, graphics::DrawParam::new());
                    let dot = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, 5.0, 0.5, Color::from_rgb(235, 90, 160))?;
                    canvas.draw(&dot, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::BonusBomb { dir, .. } => {
                    // 蓄力炸弹：橙红火球
                    let d = dir;
                    let tx = px + d.x.to_num::<f32>() * 10.0;
                    let ty = py + d.y.to_num::<f32>() * 10.0;
                    let tail = Mesh::new_line(&ctx.gfx, &[Point2 { x: px, y: py }, Point2 { x: tx, y: ty }], 3.0, Color::from_rgba(255, 160, 60, 220))?;
                    canvas.draw(&tail, graphics::DrawParam::new());
                    let dot = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, 6.0, 0.5, Color::from_rgb(255, 140, 60))?;
                    canvas.draw(&dot, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Returner { .. } => {
                    // 回返镖：青色小回镖
                    let dot = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, 5.0, 0.5, Color::from_rgb(120, 230, 220))?;
                    canvas.draw(&dot, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Tether { .. } => {
                    // 回拉线：蓝紫节点
                    let dot = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, 5.0, 0.5, Color::from_rgb(120, 140, 255))?;
                    canvas.draw(&dot, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Gravity { radius, .. } => {
                    // 引力场：半透明浅紫圈
                    let r = (radius.to_num::<f32>() * self.scale).max(8.0);
                    let ring = Mesh::new_circle(&ctx.gfx, DrawMode::stroke(2.0), Point2 { x: px, y: py }, r, 0.4, Color::from_rgba(170, 130, 255, 190))?;
                    canvas.draw(&ring, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::Star { radius, .. } => {
                    // 星域：金色星形节点 + 半径
                    let r = (radius.to_num::<f32>() * self.scale).max(6.0);
                    let ring = Mesh::new_circle(&ctx.gfx, DrawMode::stroke(2.0), Point2 { x: px, y: py }, r, 0.4, Color::from_rgba(255, 220, 120, 190))?;
                    canvas.draw(&ring, graphics::DrawParam::new());
                    let dot = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x: px, y: py }, 5.0, 0.5, Color::from_rgb(255, 220, 120))?;
                    canvas.draw(&dot, graphics::DrawParam::new());
                }
                game_core::world::ProjectileKind::BindLine { from, end, .. } => {
                    // 束缚线：一条束形线段
                    let fx = from.x.to_num::<f32>() * self.scale + self.offset.x;
                    let fy = from.y.to_num::<f32>() * self.scale + self.offset.y;
                    let ex = end.x.to_num::<f32>() * self.scale + self.offset.x;
                    let ey = end.y.to_num::<f32>() * self.scale + self.offset.y;
                    let line = Mesh::new_line(&ctx.gfx, &[Point2 { x: fx, y: fy }, Point2 { x: ex, y: ey }], 4.0, Color::from_rgba(200, 120, 255, 200))?;
                    canvas.draw(&line, graphics::DrawParam::new());
                }
            }
        }

        // 多局 meta 覆盖层（学习阶段 / 整场结束）
        self.draw_meta_overlay(&mut canvas, ctx)?;

        #[cfg(feature = "steam")]
        if self.steam_in_lobby {
            if self.steam_room_edit {
                self.draw_steam_room_edit(&mut canvas, ctx)?;
            } else {
                self.draw_steam_ready_overlay(&mut canvas, ctx)?;
            }
        }

        // 客户端掉线/重连覆盖层
        if self.conn_dropped {
            self.draw_reconnect_overlay(&mut canvas, ctx)?;
        }

        // Steam 提示条（成就上报等）：画在最上层，几秒后自动消失。
        #[cfg(feature = "steam")]
        {
            let now = ctx.time.time_since_start().as_secs_f64();
            if !self.steam_toast.0.is_empty() && now < self.steam_toast.1 {
                let (sw, sh) = ctx.gfx.drawable_size();
                draw_text(&mut canvas, ctx, &self.steam_toast.0, 22.0, Color::from_rgb(255, 215, 120), Point2 { x: sw / 2.0, y: sh * 0.08 }, true)?;
            }
        }

        canvas.finish(ctx)?;
        Ok(())
    }

    /// Steam 房间/就绪界面：列出成员昵称 + 就绪状态，按 o 就绪/取消，全就绪倒计时。
    #[cfg(feature = "steam")]
    fn draw_steam_ready_overlay(&mut self, canvas: &mut Canvas, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        let dim = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), graphics::Rect::new(0.0, 0.0, sw, sh), Color::from_rgba(8, 10, 16, 225))?;
        canvas.draw(&dim, graphics::DrawParam::new());
        let cx = sw / 2.0;
        draw_text(canvas, ctx, "房间 - 等待所有人就绪", 40.0, Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.18 }, true)?;
        // 房间名 + 人数 + 锁状态（host 读 matchmaking，client 用本地记录）。
        let (rname, rnote) = self.steam_current_room_info();
        let n_in = self.steam_roster.len();
        let lock_txt = if self.steam_room_locked { "[锁]" } else { "[开]" };
        let mut roomline = format!("房间：{rname}    人数 {n_in}");
        if self.steam_host_ls.is_some() {
            roomline.push_str(&format!("   {lock_txt}"));
        }
        draw_text(canvas, ctx, &roomline, 20.0, Color::from_rgb(190, 200, 215), Point2 { x: cx, y: sh * 0.18 + 40.0 }, true)?;
        if !rnote.is_empty() {
            draw_text(canvas, ctx, &format!("备注：{rnote}"), 18.0, Color::from_rgb(170, 180, 195), Point2 { x: cx, y: sh * 0.18 + 66.0 }, true)?;
        }
        let flow_y = if rnote.is_empty() { sh * 0.18 + 66.0 } else { sh * 0.18 + 92.0 };
        draw_text(canvas, ctx, "流程：全员就绪 → 倒计时 → 技能配置 → 配好后自动开战", 18.0, Color::from_rgb(160, 172, 190), Point2 { x: cx, y: flow_y }, true)?;
        if self.steam_all_ready {
            draw_text(canvas, ctx, &format!("全员就绪：{:.0} 秒后进配置（结束前按 U 可取消）", self.steam_countdown.max(0.0)), 28.0, Color::from_rgb(90, 220, 130), Point2 { x: cx, y: flow_y + 48.0 }, true)?;
        } else if self.steam_host_ls.is_some() && self.steam_manual_start_pending {
            // 不满员但在线者都就绪：不自动倒计时，由 host 按回车手动开始。
            draw_text(canvas, ctx, &format!("人数不足（已入 {n_in}）：当前全员就绪，按回车 手动开始"), 26.0, Color::from_rgb(255, 220, 120), Point2 { x: cx, y: flow_y + 48.0 }, true)?;
        } else {
            draw_text(canvas, ctx, "▶ 按 U 就绪（再按 U 取消）", 28.0, Color::from_rgb(255, 240, 120), Point2 { x: cx, y: flow_y + 48.0 }, true)?;
        }
        // host 附加“编辑房间”入口，显示在底部。
        if self.steam_host_ls.is_some() {
            draw_text(canvas, ctx, "E 编辑房间名/备注与锁定    I 邀请好友    Q 退出房间", 19.0, Color::from_rgb(160, 200, 255), Point2 { x: cx, y: sh * 0.90 }, true)?;
        } else {
            draw_text(canvas, ctx, "U 就绪/取消    I 邀请好友    Q 退出房间", 19.0, Color::from_rgb(160, 200, 255), Point2 { x: cx, y: sh * 0.90 }, true)?;
        }
        draw_text(canvas, ctx, "== 就绪状态 ==", 20.0, Color::from_rgb(200, 210, 220), Point2 { x: cx, y: sh * 0.42 }, true)?;
        let mut y = sh * 0.42 + 32.0;
        let roster_lookup = |slot: u8, fallback: bool| -> bool {
            self.steam_roster_ready
                .iter()
                .find(|(s, _)| *s == slot)
                .map(|(_, r)| *r)
                .unwrap_or(fallback)
        };
        let card_w = (sw * 0.46).min(520.0);
        let card_x = cx - card_w / 2.0;
        for (slot, name, id) in self.steam_roster.iter() {
            let is_me = *slot == self.steam_my_index;
            let (ready, col) = if is_me {
                (self.steam_local_ready, if self.steam_local_ready { Color::from_rgb(90, 220, 130) } else { Color::from_rgb(220, 220, 225) })
            } else if let Some(host) = self.steam_host_ls.as_ref() {
                // host 本机：直接读 HostLockstep 里的各 client 就绪。
                let r = host.client_ready(*slot);
                (r, if r { Color::from_rgb(90, 220, 130) } else { Color::from_rgb(220, 220, 225) })
            } else {
                // client 本机：用 host 广播的就绪状态快照（多人一致界面）。
                let r = roster_lookup(*slot, false);
                (r, if r { Color::from_rgb(90, 220, 130) } else { Color::from_rgb(220, 220, 225) })
            };
            // 成员卡片背景（就绪偏绿、未就绪深灰），与主菜单/配置界面卡片视觉一致。
            let bg_col = if ready { Color::from_rgb(34, 58, 44) } else { Color::from_rgb(30, 34, 44) };
            let bg = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), graphics::Rect::new(card_x, y - 6.0, card_w, 44.0), bg_col)?;
            canvas.draw(&bg, graphics::DrawParam::new());
            let mark = if ready { "[v]" } else { "[ ]" };
            let me_tag = if is_me { "（我）" } else { "" };
            draw_text(canvas, ctx, &format!("  {mark}  {name}{me_tag}"), 26.0, col, Point2 { x: card_x + 90.0, y }, true)?;
            // 延迟：画在卡片右端（没测到显示“--”）。
            let ping_txt = match self.steam_ping_of(*id) {
                Some(ms) => format!("{ms} ms"),
                None => "-- ms".to_string(),
            };
            draw_text(canvas, ctx, &ping_txt, 18.0, Color::from_rgb(150, 175, 205), Point2 { x: card_x + card_w - 60.0, y: y + 2.0 }, true)?;
            // 头像：画在卡片左侧外面（卡片内的昵称是居中排的，塞进去会压字），没拉到就留空。
            self.steam_draw_avatar(canvas, *id, card_x - 36.0, y - 4.0, 30.0);
            y += 46.0;
        }
        // 「邀请好友」面板（按 I 展开）：画在成员列表下方（`y` 即成员列表末尾），卡片样式与成员列表一致。
        #[cfg(feature = "steam")]
        if self.steam_friend_list {
            self.draw_steam_friend_panel(canvas, ctx, y)?;
        }
        Ok(())
    }

    /// 绘制「邀请好友」面板：好友昵称 + 在线/离线 + 是否已在房间；选中项高亮，行数按剩余空间自适应（最多 4 行）。
    /// `roster_end_y` 是成员列表画完后的 y，面板从它下面开始，避免与成员列表叠在一起。
    #[cfg(feature = "steam")]
    fn draw_steam_friend_panel(&self, canvas: &mut Canvas, ctx: &Context, roster_end_y: f32) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;
        let title_y = (roster_end_y + 14.0).max(sh * 0.58);
        let hint_y = sh * 0.83; // 面板操作提示
        let card_w = (sw * 0.46).min(520.0);
        let card_x = cx - card_w / 2.0;
        draw_text(canvas, ctx, "== 邀请好友 ==", 20.0, Color::from_rgb(200, 210, 220), Point2 { x: cx, y: title_y }, true)?;
        if self.steam_friends.is_empty() {
            draw_text(canvas, ctx, "（暂无好友 / 正在拉取…）", 20.0, Color::from_rgb(170, 178, 194), Point2 { x: cx, y: title_y + 34.0 }, true)?;
        } else {
            const ROW_H: f32 = 38.0;
            // 面板能放几行取决于离底部提示还剩多少空间（人数多时自然少放几行）。
            let fit = ((hint_y - (title_y + 30.0)) / ROW_H).floor() as usize;
            let max_rows = fit.clamp(1, 4);
            let n = self.steam_friends.len();
            // 选中项滚出可视窗口时，窗口跟着走（保证选中行总是可见）。
            let start = if n <= max_rows {
                0
            } else {
                (self.steam_friend_selection + 1).saturating_sub(max_rows).min(n - max_rows)
            };
            let mut y = title_y + 34.0;
            for i in start..(start + max_rows).min(n) {
                let f = &self.steam_friends[i];
                let selected = i == self.steam_friend_selection;
                let bg_col = if selected { Color::from_rgb(52, 60, 74) } else { Color::from_rgb(30, 34, 44) };
                let bg = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), graphics::Rect::new(card_x, y - 6.0, card_w, 36.0), bg_col)?;
                canvas.draw(&bg, graphics::DrawParam::new());
                let mark = if selected { "[v]" } else { "[ ]" };
                let tag = if f.in_lobby {
                    "（已在房间）"
                } else if f.online {
                    "（在线）"
                } else {
                    "（离线）"
                };
                let col = if selected { Color::WHITE } else { Color::from_rgb(205, 210, 222) };
                let name = if f.name.is_empty() { f.id.to_string() } else { f.name.clone() };
                draw_text(canvas, ctx, &format!("  {mark}  {name}{tag}"), 20.0, col, Point2 { x: card_x + 60.0, y }, true)?;
                // 头像（与成员列表同一位置：卡片左侧外面）。
                self.steam_draw_avatar(canvas, f.id, card_x - 36.0, y - 4.0, 30.0);
                y += 38.0;
            }
        }
        draw_text(canvas, ctx, "↑/↓ 选择    回车 邀请    A Steam 邀请窗口    R 刷新    I/Q 收起", 17.0, Color::from_rgb(160, 200, 255), Point2 { x: cx, y: hint_y }, true)?;
        if !self.steam_friend_hint.is_empty() {
            draw_text(canvas, ctx, &self.steam_friend_hint, 18.0, Color::from_rgb(255, 220, 120), Point2 { x: cx, y: hint_y + 24.0 }, true)?;
        }
        Ok(())
    }

    /// 客户端掉线/重连提示覆盖层：提醒玩家已掉线，按 R 重连。
    fn draw_reconnect_overlay(&mut self, canvas: &mut Canvas, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        let dim = Mesh::new_rectangle(
            &ctx.gfx,
            DrawMode::fill(),
            graphics::Rect::new(0.0, 0.0, sw, sh),
            Color::from_rgba(8, 8, 12, 220),
        )?;
        canvas.draw(&dim, graphics::DrawParam::new());
        let cx = sw / 2.0;
        let status = if self.reconnect_attempting {
            "正在重连…"
        } else {
            "连接已断开"
        };
        draw_text(canvas, ctx, status, 42.0, Color::from_rgb(255, 190, 90), Point2 { x: cx, y: sh * 0.38 }, true)?;
        draw_text(canvas, ctx, "按 R 从 host 拉取快照重连", 24.0, Color::from_rgb(200, 205, 220), Point2 { x: cx, y: sh * 0.38 + 70.0 }, true)?;
        Ok(())
    }

    /// 渲染学习阶段 / 整场结束的信息覆盖层（无依赖文本，用简笔几何表示）。
    fn draw_meta_overlay(&mut self, canvas: &mut Canvas, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();

        match self.meta.phase {
            MatchPhase::Fighting => {
                // 延迟指示（Steam 联机才有 ping；没测出来显示“--”）。
                #[cfg(feature = "steam")]
                if self.steam_cli_ls.is_some() || self.steam_host_ls.is_some() {
                    let host_id = self.steam_participants.first().copied().unwrap_or(0);
                    let mine = if self.steam_cli_ls.is_some() {
                        self.steam_ping_of(host_id)
                    } else {
                        // host：显示到各 client 里最差的一个（帧同步等最慢的那端）。
                        let my_id = self.steam_my_id;
                        self.steam_pings
                            .iter()
                            .filter(|(id, _)| *id != my_id)
                            .map(|(_, ms)| *ms)
                            .max()
                    };
                    let txt = match mine {
                        Some(ms) => format!("延迟 {ms} ms"),
                        None => "延迟 -- ms".to_string(),
                    };
                    draw_text(canvas, ctx, &txt, 18.0, Color::from_rgb(150, 175, 205), Point2 { x: 76.0, y: sh - 116.0 }, true)?;
                }
                // 技能冷却 HUD：底部一排 8 个键位槽，显示绑定技能图标/名称 + 冷却遮罩
                let self_idx = self.self_index();
                if let (Some(me), Some(me_player)) = (
                    self.meta.profiles.iter().find(|p| p.player_id == self_idx),
                    self.world.players.get(self_idx as usize),
                ) {
                    let slot_w = 56.0;
                    let slot_h = 56.0;
                    let gap = 12.0;
                    let n = game_core::skill::CastKey::ALL.len() as f32;
                    let total_w = n * slot_w + (n - 1.0) * gap;
                    let x0 = (sw - total_w) / 2.0;
                    let y0 = sh - slot_h - 24.0;
                    for (i, key) in game_core::skill::CastKey::ALL.iter().enumerate() {
                        let bx = x0 + i as f32 * (slot_w + gap);
                        let rect = graphics::Rect::new(bx, y0, slot_w, slot_h);
                        // 底板
                        let bg = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), rect, Color::from_rgba(15, 18, 26, 210))?;
                        canvas.draw(&bg, graphics::DrawParam::new());
                        let border = Mesh::new_rectangle(&ctx.gfx, DrawMode::stroke(2.0), rect, Color::from_rgba(90, 100, 120, 230))?;
                        canvas.draw(&border, graphics::DrawParam::new());

                        let skill = me.bound_skill(*key);
                        let slot_center = Point2 { x: bx + slot_w / 2.0, y: y0 + 22.0 };
                        // 技能名
                        let label = match skill {
                            Some(s) => game_core::skill::DefTable::def(s).name,
                            None => "—",
                        };
                        draw_text(canvas, ctx, key.letter(), 16.0, Color::from_rgb(200, 200, 215), Point2 { x: bx + 6.0, y: y0 + 4.0 }, true)?;
                        draw_text(canvas, ctx, label, 15.0, Color::WHITE, slot_center, true)?;

                        // 冷却遮罩 + 倒计时
                        if let Some(s) = skill {
                            let rem = me_player.caster.cooldown_remaining(s);
                            if rem > Fix64::ZERO {
                                let def = game_core::skill::DefTable::def(s);
                                let total = Fix64::from_num(def.growth.cooldown_base.max(0.1));
                                if total > Fix64::ZERO {
                                    let frac = (rem / total).to_num::<f32>().clamp(0.0, 1.0);
                                    let shade_h = slot_h * frac;
                                    let shade = Mesh::new_rectangle(
                                        &ctx.gfx,
                                        DrawMode::fill(),
                                        graphics::Rect::new(bx, y0, slot_w, shade_h),
                                        Color::from_rgba(30, 40, 60, 180),
                                    )?;
                                    canvas.draw(&shade, graphics::DrawParam::new());
                                    draw_text(canvas, ctx, &format!("{:.1}", rem.to_num::<f32>()), 20.0, Color::from_rgb(120, 200, 255), Point2 { x: bx + slot_w / 2.0, y: y0 + slot_h / 2.0 }, true)?;
                                }
                            }
                            // 前摇提示：该技能正在蓄力（Windup）时高亮描边 + 提示字
                            if let game_core::skill::CastPhase::Windup { id, .. } = me_player.caster.phase() {
                                if id == s {
                                    let wring = Mesh::new_rectangle(&ctx.gfx, DrawMode::stroke(3.0), rect, Color::from_rgb(255, 180, 80))?;
                                    canvas.draw(&wring, graphics::DrawParam::new());
                                    draw_text(canvas, ctx, "蓄力中", 15.0, Color::from_rgb(255, 200, 120), Point2 { x: bx + slot_w / 2.0, y: y0 + slot_h - 14.0 }, true)?;
                                }
                            }
                        }
                    }
                }
                // 操作提示：shift 连招队列
                draw_text(
                    canvas, ctx,
                    "Shift+右键 排移动 / Shift+技能(Shift+左键点目标) 排施法 / S 清空指令队列",
                    16.0, Color::from_rgba(170, 180, 200, 220),
                    Point2 { x: sw / 2.0, y: sh - 92.0 }, true)?;
            }
            MatchPhase::Learning => {
                // 半透明遮罩
                let dim = Mesh::new_rectangle(
                    &ctx.gfx,
                    DrawMode::fill(),
                    graphics::Rect::new(0.0, 0.0, sw, sh),
                    Color::from_rgba(8, 10, 16, 210),
                )?;
                canvas.draw(&dim, graphics::DrawParam::new());

                let cx = sw / 2.0;
                let mut y = sh * 0.18;

                let title = format!("第 {} / {} 局结束 - 学习阶段", self.meta.round, self.meta.config.total_rounds);
                draw_text(canvas, ctx, &title, 34.0, Color::from_rgb(255, 210, 120), Point2 { x: cx, y }, true)?;
                y += 64.0;

                // 我的档案：金币 / 击杀 / 最佳名次
                if let Some(me) = self.meta.profiles.iter().find(|p| p.player_id == self.self_index()) {
                    let info = format!(
                        "金币 {}   击杀 {}   最佳名次 #{}",
                        me.gold, me.total_kills, me.best_placement
                    );
                    draw_text(canvas, ctx, &info, 26.0, Color::WHITE, Point2 { x: cx, y }, true)?;
                    y += 50.0;

                    // 提示操作
                    draw_text(
                        canvas, ctx,
                        "选键改技能：按 字母(C/R/E/D/Y/T/F/G) 选中该树 -> 数字键选技能 -> 按 = 升级，X 洗点",
                        20.0, Color::from_rgb(170,180,200), Point2 { x: cx, y }, true)?;
                    y += 44.0;

                    // 每个键：树名 + 已绑定技能
                    for key in game_core::skill::CastKey::ALL {
                        let bound = me.bound_skill(key);
                        let lv = bound.map(|s| me.skill_level(s)).unwrap_or(0);
                        let bound_txt = match bound {
                            Some(s) => format!("{} @Lv{lv}", game_core::skill::DefTable::def(s).name),
                            None => "未绑定".to_string(),
                        };
                        let color = if self.learn_tree_key == Some(key) {
                            Color::from_rgb(255, 210, 120)
                        } else {
                            Color::from_rgb(210, 215, 225)
                        };
                        let line = format!("[{}] {}树   {}", key.letter(), key.tree().name_zh(), bound_txt);
                        draw_text(canvas, ctx, &line, 21.0, color, Point2 { x: cx, y }, true)?;
                        y += 32.0;
                    }

                    // 被选中的树的技能选项
                    if let Some(key) = self.learn_tree_key {
                        y += 18.0;
                        draw_text(canvas, ctx, &format!("{} 树的技能（按数字选）：", key.letter()), 20.0, Color::from_rgb(255,210,120), Point2 { x: cx, y }, true)?;
                        y += 34.0;
                        for (i, skill) in key.tree().skills_in_tree().iter().enumerate() {
                            let line = format!("  {}  {}", i + 1, game_core::skill::DefTable::def(*skill).name);
                            draw_text(canvas, ctx, &line, 18.0, Color::from_rgb(220,220,230), Point2 { x: cx, y }, true)?;
                            y += 28.0;
                        }
                        y += 20.0;
                    }

                    y += 20.0;
                    draw_text(canvas, ctx, &format!("剩余学习时间：{:.0} 秒", self.meta.learn_remaining.max(0.0)), 22.0, Color::from_rgb(150,220,160), Point2 { x: cx, y }, true)?;
                }
            }
            MatchPhase::Finished => {
                // 终局结算
                let dim = Mesh::new_rectangle(
                    &ctx.gfx,
                    DrawMode::fill(),
                    graphics::Rect::new(0.0, 0.0, sw, sh),
                    Color::from_rgba(8, 10, 16, 230),
                )?;
                canvas.draw(&dim, graphics::DrawParam::new());
                let cx = sw / 2.0;
                let mut y = sh * 0.2;
                draw_text(canvas, ctx, "对局结束", 40.0, Color::from_rgb(255,190,90), Point2 { x: cx, y }, true)?;
                y += 70.0;
                // 按最佳名次排序展示所有玩家
                let mut sorted: Vec<_> = self.meta.profiles.iter().collect();
                sorted.sort_by_key(|p| p.best_placement);
                for p in sorted.iter() {
                    let line = format!("玩家{}  金币{}  击杀{}  最佳名次#{}", p.player_id, p.gold, p.total_kills, p.best_placement);
                    draw_text(canvas, ctx, &line, 24.0, Color::WHITE, Point2 { x: cx, y }, true)?;
                    y += 40.0;
                }
                y += 30.0;
                // Steam：统计（后台配置后才有值）+ 榜单前几名 + 成就提示。
                #[cfg(feature = "steam")]
                if let Some(s) = self.steam_stats_snapshot {
                    let f = |v: Option<i32>| v.map(|n| n.to_string()).unwrap_or_else(|| "--".to_string());
                    let line = format!(
                        "Steam 统计：场次 {}    胜场 {}    击杀 {}",
                        f(s.matches),
                        f(s.wins),
                        f(s.kills)
                    );
                    draw_text(canvas, ctx, &line, 20.0, Color::from_rgb(170, 190, 215), Point2 { x: cx, y }, true)?;
                    y += 34.0;
                    // 榜单前 5（异步下载；没下载到就显示一行提示）。
                    let rows = self.steam_lb_rows.lock().unwrap().clone();
                    if rows.is_empty() {
                        draw_text(canvas, ctx, "排行榜：暂无数据（需在 Steamworks 后台建榜）", 18.0, Color::from_rgb(140, 150, 168), Point2 { x: cx, y }, true)?;
                        y += 30.0;
                    } else {
                        draw_text(canvas, ctx, "排行榜 TOP5", 20.0, Color::from_rgb(255, 210, 120), Point2 { x: cx, y }, true)?;
                        y += 30.0;
                        for r in rows.iter().take(5) {
                            let name = self
                                .steam_transport()
                                .map(|t| t.friends().get_friend(net_steam::steamworks::SteamId::from_raw(r.steam_id)).name())
                                .unwrap_or_default();
                            let who = if name.is_empty() { format!("{}", r.steam_id) } else { name };
                            draw_text(canvas, ctx, &format!("#{}  {}  {} 分", r.rank, who, r.score), 18.0, Color::from_rgb(205, 212, 225), Point2 { x: cx, y }, true)?;
                            y += 26.0;
                        }
                    }
                }
                draw_text(canvas, ctx, "对局结束 - 按 Q 返回主菜单", 22.0, Color::from_rgb(150, 200, 255), Point2 { x: cx, y: y + 30.0 }, true)?;
            }
        }
        Ok(())
    }
}

impl event::EventHandler for Game {
    fn update(&mut self, ctx: &mut Context) -> GameResult {
        let dt = ctx.time.delta().as_secs_f64();

        // Steam 房间/就绪/编辑阶段：房主按 E 进「编辑房间信息」界面；否则进房间就绪界面。
        #[cfg(feature = "steam")]
        if self.steam_in_lobby {
            if self.steam_room_edit {
                return self.steam_room_edit_update(ctx, dt);
            }
            return self.steam_lobby_update(ctx, dt);
        }

        // 主菜单：方向键 ↑/↓ 选中 + 回车确认 + 数字快捷；局域网（S3 完成前）保留命令行提示。
        if self.app == AppState::MainMenu {
            // 好友从 Steam 好友列表点「加入游戏」→ 回调进房（需先有会话；Steam 没跑则静默跳过）。
            #[cfg(feature = "steam")]
            {
                self.steam_ensure_session();
                self.steam_poll_join_requests(ctx);
                // 被邀请进房后本帧不再处理菜单输入。
                if self.steam_in_lobby || self.steam_cli_ls.is_some() || self.steam_host_ls.is_some() {
                    self.accumulator = 0.0;
                    return Ok(());
                }
            }
            use ggez::input::keyboard::Key;
            use winit::keyboard::NamedKey;
            use ggez::input::mouse::MouseButton;
            let just = |k: char| ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.to_string().into()));
            let just_named = |n: NamedKey| ctx.keyboard.is_logical_key_just_pressed(&Key::Named(n));
            const MENU_COUNT: usize = 3;
            // 大厅子界面（主/建房设置/房间列表）内不响应主菜单的方向键/数字。
            #[cfg(feature = "steam")]
            let in_lobby_menu = self.steam_lobby_menu || self.steam_lobby_create || self.steam_lobby_list;
            #[cfg(not(feature = "steam"))]
            let in_lobby_menu = false;
            // 建房设置界面输入（房间名/备注/人数，回车创建 / Q 取消）。
            #[cfg(feature = "steam")]
            if self.steam_lobby_create {
                self.steam_lobby_create_update(ctx);
                self.accumulator = 0.0;
                return Ok(());
            }
            // 房间列表界面输入（浏览公开大厅，方向键+回车加入 / R 刷新 / Q 返回）。
            #[cfg(feature = "steam")]
            if self.steam_lobby_list {
                self.steam_lobby_list_update(ctx);
                self.accumulator = 0.0;
                return Ok(());
            }
            // Steam 大厅主界面（创建 / 加入 / 返回）：独立处理并返回，不落到下面主菜单动作块。
            // 放在主菜单动作处理【之前】，避免“刚进入大厅那一帧”把触发键（回车/鼠标）又当成大厅里的选择再触发一次（会直接进创建房间）。
            #[cfg(feature = "steam")]
            if self.steam_lobby_menu {
                // 上下箭头切换选中。
                if just_named(NamedKey::ArrowUp) {
                    self.steam_lobby_selection = (self.steam_lobby_selection + 2) % 3;
                } else if just_named(NamedKey::ArrowDown) {
                    self.steam_lobby_selection = (self.steam_lobby_selection + 1) % 3;
                }
                // 鼠标点击卡片：命中即选中并执行。
                let mut clicked = false;
                if ctx.mouse.button_just_pressed(MouseButton::Left) {
                    let (sw, sh) = ctx.gfx.drawable_size();
                    let card_w = (sw * 0.62).min(560.0);
                    let card_h = 96.0;
                    let card_x = sw / 2.0 - card_w / 2.0;
                    let y0 = sh * 0.34;
                    let gap = 26.0;
                    let p = ctx.mouse.position();
                    for i in 0..3 {
                        let y = y0 + i as f32 * (card_h + gap);
                        if graphics::Rect::new(card_x, y, card_w, card_h).contains(p) {
                            self.steam_lobby_selection = i;
                            self.steam_lobby_act(i);
                            clicked = true;
                        }
                    }
                }
                if !clicked {
                    if just('h') || just('H') || just(' ') {
                        self.steam_lobby_act(0);
                    } else if just('j') || just('J') {
                        self.steam_lobby_act(1);
                    } else if just('q') || just('Q') {
                        self.steam_lobby_act(2);
                    } else if just_named(NamedKey::Enter) || just('\r') {
                        self.steam_lobby_act(self.steam_lobby_selection);
                    }
                }
                self.accumulator = 0.0;
                return Ok(());
            }
            // 方向键移动选中（在 Steam 大厅子菜单里不干扰，只影响主菜单选中）。
            if !in_lobby_menu {
                if just_named(NamedKey::ArrowUp) {
                    self.menu_selection = (self.menu_selection + MENU_COUNT - 1) % MENU_COUNT;
                    eprintln!("[menu] select={}", self.menu_selection);
                } else if just_named(NamedKey::ArrowDown) {
                    self.menu_selection = (self.menu_selection + 1) % MENU_COUNT;
                    eprintln!("[menu] select={}", self.menu_selection);
                }
            }
            // 执行选中项对应的动作（回车 / 数字直选共用）。
            let mut act: Option<usize> = None;
            // 鼠标点击主菜单卡片（与键盘共用 menu_selection + act）。
            if !in_lobby_menu && ctx.mouse.button_just_pressed(MouseButton::Left) {
                let (sw, sh) = ctx.gfx.drawable_size();
                let card_w = (sw * 0.62).min(560.0);
                let card_h = 96.0;
                let card_x = sw / 2.0 - card_w / 2.0;
                let y0 = sh * 0.34;
                let gap = 26.0;
                let p = ctx.mouse.position();
                for i in 0..3 {
                    let y = y0 + i as f32 * (card_h + gap);
                    if graphics::Rect::new(card_x, y, card_w, card_h).contains(p) {
                        self.menu_selection = i;
                        act = Some(i);
                    }
                }
            }
            if in_lobby_menu {
                act = None;
            } else if just_named(NamedKey::Enter) || just('\r') {
                act = Some(self.menu_selection);
            } else if just('1') {
                self.menu_selection = 0;
                act = Some(0);
            } else if just('2') {
                self.menu_selection = 1;
                act = Some(1);
            } else if just('3') {
                self.menu_selection = 2;
                act = Some(2);
            }
            if let Some(sel) = act {
                match sel {
                    0 => {
                        // 单机试验场：world/meta 在构造时已是 1 玩家无 AI，直接切换即可。
                        eprintln!("[menu] -> Solo");
                        self.app = AppState::Solo;
                        self.pre_game_config = true; // 先进开局配置
                    }
                    1 => {
                        eprintln!("[menu] 局域网建设中：需命令行 --host <port> / --join <host:port>");
                    }
                    2 => {
                        // 进入 Steam 大厅选择子菜单（H 创建 / J 加入 / Q 返回）。
                        #[cfg(feature = "steam")]
                        {
                            eprintln!("[menu] -> Steam lobby menu");
                            self.steam_lobby_menu = true;
                            self.steam_lobby_create = false;
                            self.steam_lobby_list = false;
                            // 进入大厅前初始化一次 Steam 会话（读本机昵称；建房/加入复用，避免重复 init 单实例）。
                            if self.steam_sess.is_none() {
                                match net_steam::session::SteamSession::init(APP_ID, STEAM_VIRTUAL_PORT) {
                                    Ok(s) => {
                                        self.steam_my_display_name = s.transport.friends()
                                            .get_friend(net_steam::steamworks::SteamId::from_raw(s.transport.steam_id()))
                                            .name();
                                        self.steam_sess = Some(s);
                                        eprintln!("[steam] session ready, display name='{}'", self.steam_my_display_name);
                                    }
                                    Err(e) => {
                                        eprintln!("[steam] session init failed: {e:?}");
                                        self.steam_sess = None;
                                    }
                                }
                            }
                        }
                        #[cfg(not(feature = "steam"))]
                        eprintln!("[menu] Steam 未启用（需 --features client/steam 构建）");
                    }
                    _ => {}
                }
            }
            self.accumulator = 0.0;
            return Ok(());
        }

        // 开局前的技能配置（Solo 试验场 / 局域网）：选/升级技能，按 Space/O 开始第一局。
        // 开局前的技能配置（Solo 试验场 / 局域网）：选/升级技能，按 Space/O 开始第一局。
        // 注意：一旦进入配置同步（net_cfg != Idle，例如 host 按空格后 HostGather / client 上报后 ClientWait），
        // 本块必须【放行】到下面 Fighting 分支的同步逻辑，否则会一直 return、同步永不推进 → 卡死。
        if self.pre_game_config && self.app != AppState::MainMenu && self.net_cfg == NetCfgSync::Idle {
            // 局域网 host：开局配置阶段就同步接收 client 加入（不必等按了 Space 才开始收人），
            // 否则先到的 client 会因 host 未 poll_join 而握手超时。
            self.poll_host_join_phase();
            // Steam：配置阶段独立处理（心跳 + 选技能 + 配完确认 + 统一开战判定）。
            // 不再让各端按 o 各自进对局（修 1）：host 收齐所有端 build_done 才产 seq=0 首帧统一开战；
            // client 收到 host 首帧才进对局（首帧=统一开始信号）。开战后 `pre_game_config` 置 false，本块自动放行到 Fighting。
            #[cfg(feature = "steam")]
            if self.steam_cli_ls.is_some() || self.steam_host_ls.is_some() {
                self.steam_config_update(ctx)?;
                self.accumulator = 0.0;
                return Ok(());
            }
            use ggez::input::keyboard::Key;
            use winit::keyboard::NamedKey;
            // 配置界面：按 Esc 返回主菜单（单机/局域网）。
            if ctx.keyboard.is_logical_key_just_pressed(&Key::Named(NamedKey::Escape)) {
                eprintln!("[pre-game] Esc -> back to main menu");
                self.reset_to_main_menu();
                self.accumulator = 0.0;
                return Ok(());
            }
            self.poll_learning(ctx);
            self.poll_growth_buy(ctx);
            // 空格（确认）开始第一轮；空格在本环境实测不可靠，故加 P 兜底，再补回车。
            let done = ctx.keyboard.is_logical_key_just_pressed(&Key::Character(" ".into()))
                || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("p".into()))
                || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("P".into()))
                || ctx.keyboard.is_logical_key_just_pressed(&Key::Named(NamedKey::Enter))
                || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("\r".into()));
            // 单机：超时自动用默认配置开始（防止窗口无焦点/按键收不到导致卡死）。
            let auto_done = if self.app == AppState::Solo && self.net_link.is_none() && self.net_host.is_none() && self.net_host_ls.is_none() {
                self.pre_game_timer -= dt;
                self.pre_game_timer <= 0.0
            } else {
                self.pre_game_timer = PRE_GAME_TIMEOUT_SECS; // 联网：不自动开始，重置计时供其它判断用
                false
            };
            if done {
                self.finish_pre_game();
            } else if auto_done {
                eprintln!("[solo] pre-game timeout -> auto-start with defaults");
                self.finish_pre_game();
            }
            self.accumulator = 0.0;
            return Ok(());
        }

        match self.meta.phase {
            MatchPhase::Finished => {
                // 整场对抗结束：不再模拟
                self.accumulator = 0.0;
                // Steam：整场结束上报一次战绩（统计 + 成就 + 排行榜），内部有“只上报一次”保护。
                #[cfg(feature = "steam")]
                if self.steam_active() {
                    self.steam_record_match_result(ctx.time.time_since_start().as_secs_f64());
                }
                // 允许回主菜单：按 Q。
                use ggez::input::keyboard::Key;
                let q = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("q".into()))
                    || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("Q".into()));
                if q {
                    eprintln!("[meta] finished -> back to main menu");
                    self.reset_to_main_menu();
                }
                Ok(())
            }
            MatchPhase::Learning => {
                // 学习阶段：轮询购买升级输入 + 计时
                self.poll_learning(ctx);
                self.poll_growth_buy(ctx);
                let now = self.meta.tick_learning(dt.min(0.25));
                // 若学习结束，准备进入下一局：联网需先做「配置同步」
                if self.meta.phase == MatchPhase::Fighting {
                    if self.net_link.is_some() {
                        eprintln!("[meta] round {} learning done -> ClientWait (config sync)", self.meta.round);
                        self.net_cfg = NetCfgSync::ClientWait;
                    } else if self.net_host_ls.is_some() {
                        eprintln!("[meta] round {} learning done -> HostGather (config sync)", self.meta.round);
                        self.net_cfg = NetCfgSync::HostGather;
                    } else {
                        eprintln!("[meta] round {} learning done -> next round (single)", self.meta.round);
                        self.teardown_round_end();
                    }
                }
                let _ = now;
                Ok(())
            }
            MatchPhase::Fighting => {
                // 单机试验场：按 Esc 随时返回主菜单（无任何联网对局时才生效）。
                #[cfg(not(feature = "steam"))]
                let solo_no_net = self.net_link.is_none() && self.net_host.is_none();
                #[cfg(feature = "steam")]
                let solo_no_net = self.net_link.is_none()
                    && self.net_host.is_none()
                    && self.steam_cli_ls.is_none()
                    && self.steam_host_ls.is_none();
                if solo_no_net {
                    use ggez::input::keyboard::Key;
                    use winit::keyboard::NamedKey;
                    if ctx.keyboard.is_logical_key_just_pressed(&Key::Named(NamedKey::Escape)) {
                        eprintln!("[solo] Esc -> back to main menu");
                        self.reset_to_main_menu();
                        self.accumulator = 0.0;
                        return Ok(());
                    }
                }
                // 每帧轮询输入（技能键 / 鼠标）
                self.poll_input(ctx);
                self.accumulator += dt.min(0.25);
                let ticking = Fix64::from_num(TICK);
                #[cfg(feature = "steam")]
                {
                    // Rich Presence：对局中 → “对局中（第 N 局）”（带 connect，好友可加入同一房间）。
                    self.steam_refresh_presence(ctx.time.time_since_start().as_secs_f64());
                    // 对局中也刷新 ping（HUD 显示到对端的延迟，卡顿来源一眼可辨）。
                    self.steam_refresh_network_info(ctx);
                    if let Some(mut host) = std::mem::take(&mut self.steam_host_ls) {
                        // Steam host：开局配置·配置同步阶段——收齐各端 PlayerCfg(含自身) → 广播 PlayerCfgAll → 统一开战。
                        // （与局域网 HostGather 同构；用可靠的 RoomState/每帧上行通道 + cfg 包，host 收齐即广播。）
                        if self.net_cfg == NetCfgSync::HostGather {
                            // 保活：HostGather 阶段也收 client 心跳（RoomState，更新在场/配好）+ 广播就绪快照当心跳，双向保活。
                            let mut k_rcv = vec![0u8; 4096];
                            host.poll(&mut k_rcv);
                            host.broadcast_roster_ready(self.steam_local_ready);
                            let mut g_rcv = vec![0u8; 8192];
                            host.poll_cfg(&mut g_rcv);
                            let cfg_bytes = self.local_player_cfg();
                            let local_cfg_ready = !cfg_bytes.is_empty();
                            if local_cfg_ready {
                                host.set_local_cfg(cfg_bytes);
                            }
                            // 诊断（节流）：HostGather 里等“谁”。本地配置是否就绪 + 已收到几个客户端配置 + 在场/配好 + 传输收发统计。
                            self.steam_lobby_wait_ticks = self.steam_lobby_wait_ticks.wrapping_add(1);
                            if self.steam_lobby_wait_ticks % 60 == 1 {
                                let st = host.transport_ref().send_stats();
                                let tc = host.transport_ref().recv_tag_counts();
                                eprintln!(
                                    "[steam-host] HostGather waiting: local_cfg={local_cfg_ready} cfgReady={} pres={} bd={}/{} exp={} stats(recv={},queued={}) tags(total={},skill={},room={}) cfgSeen={}",
                                    host.cfg_ready_count(),
                                    host.present_clients_count(),
                                    host.build_done_clients_count(),
                                    host.expected_clients(),
                                    host.expected_clients(),
                                    st.2,
                                    st.1,
                                    tc.0,
                                    tc.1,
                                    tc.2,
                                    host.player_cfg_packets_seen(),
                                );
                            }
                            if host.all_cfgs() {
                                // 提前记住参与玩家数（host 只读）与首局标志，归还 host 后据此重建 world。
                                let p = host.participants_count();
                                let stage_first = self.pre_game_config;
                                let all = host.collect_cfgs().expect("all_cfgs 已确保收齐");
                                host.broadcast_cfgs(&all);
                                // 广播本局参与玩家 SteamID（按 new index），供各端在 host 掉线时确定性选举新 host。
                                let pids = host.participant_ids(self.steam_my_id);
                                host.broadcast_participants(&pids);
                                let stage = if stage_first { "pre-game" } else { "next round" };
                                eprintln!("[steam-host] synced {} player configs -> {stage} (round {}), participants={p}, ids={pids:?}", all.len(), self.meta.round);
                                host.reset_cfgs();
                                self.steam_host_ls = Some(host); // 归还 host，释放 borrow 后重建 world
                                // 首局开局：把 world/meta 重建为本局参与玩家数（不满员时两端角色一致）；此后多局沿用。
                                if stage_first {
                                    self.stage_world_for_participants(p, STEAM_SEED);
                                }
                                self.apply_player_cfgs(&all);
                                self.teardown_round_end();
                                self.net_cfg = NetCfgSync::Idle;
                                self.pre_game_config = false;
                                self.accumulator = 0.0;
                                return Ok(()); // 同步阶段不推进战斗
                            }
                            self.steam_host_ls = Some(host);
                            self.accumulator = 0.0;
                            return Ok(()); // 同步阶段不推进战斗
                        }
                        // Steam host：配置同步完成后这里直接产帧（收齐各端输入才产，seq=0 即统一开始）。
                        let mut hrcv = vec![0u8; 8192];
                        let mut takeover_bcast = self.steam_host_broadcasting_takeover;
                        while self.accumulator >= TICK {
                            let me = self.local_player_input();
                            host.set_local_input(Some(game_core::netcode::encode_player_input(&me)));
                            host.poll(&mut hrcv);
                            // 迁移接管后：持续广播 Takeover（基线=快照 seq + 更新后的在线参与集），直到首个在线 client 连上产帧才停。
                            if takeover_bcast {
                                if let Some((_, bseq)) = host.current_snapshot() {
                                    host.broadcast_takeover(*bseq, self.steam_online.clone());
                                }
                            }
                            // 掉线判定（Steam 战斗端）：某 client 连续未上行超时 → 自动 drop（默认输入占位），其余端继续，不空转等它。
                            for dropped_idx in host.auto_drop_idle(HOST_DROP_TICKS) {
                                eprintln!("[steam-host] AUTO-DROP client {dropped_idx} (idle timeout) -> game continues");
                            }
                            if let Some((seq, frame)) = host.try_emit() {
                                takeover_bcast = false; // 首个在线 client 已连上，停止广播 Takeover
                                if seq < 30 {
                                    eprintln!("[steam-host] emit seq={seq}, n_entries={}", frame.len());
                                }
                                let n = self.world.players.len();
                                let mut inputs = vec![PlayerInput::default(); n];
                                for (idx, bytes) in frame {
                                    if (idx as usize) < n {
                                        inputs[idx as usize] =
                                            game_core::netcode::decode_player_input(&bytes).unwrap_or_default();
                                    }
                                }
                                self.world.step(inputs, ticking);
                                // 周期快照（重连用 + 广播给所有 client，供「host 掉线接管」用）。
                                self.host_frame_count += 1;
                                if self.host_frame_count % SNAPSHOT_EVERY == 0 {
                                    host.broadcast_snapshot(game_core::world_ser::world_to_bytes(&self.world), host.next_seq());
                                }
                                self.accumulator -= TICK;
                            } else {
                                // 诊断（节流）：尝试产帧但没收到 client 输入 → 说明 host→client 帧/输入链可能断。
                                self.steam_lobby_wait_ticks = self.steam_lobby_wait_ticks.wrapping_add(1);
                                if self.steam_lobby_wait_ticks % 120 == 1 {
                                    eprintln!("[steam-host] trying to emit but waiting for client input (present={})", host.present_clients_count());
                                }
                                break;
                            }
                        }
                        self.steam_host_broadcasting_takeover = takeover_bcast;
                        self.steam_host_ls = Some(host);
                    } else if let Some(mut cli) = std::mem::take(&mut self.steam_cli_ls) {
                        // Steam client：开局配置·配置同步阶段——上报我的 PlayerCfg，等 host 广播 PlayerCfgAll 后完成。
                        if self.net_cfg == NetCfgSync::ClientWait {
                            // 保活：配置同步阶段也每帧上行（就绪 + 配好 + 在场输入），既防止 P2P 空闲被拆，
                            // 也持续向 host 续报 build_done=true（host 端判定“所有端配完”始终成立）。
                            let ref_enc = game_core::netcode::encode_player_input(&self.local_player_input());
                            let _ = cli.send_room_state(self.steam_local_ready, self.steam_build_done, &ref_enc);
                            // 上报我的 PlayerCfg（client 每帧发一次，确保 host 无论如何进入 HostGather 都能收到；
                            // Steam 可靠通道保证送达，重发仅为覆盖“host 尚未开始收集”的时序）。
                            let cfg_bytes = self.local_player_cfg();
                            let cfg_len = cfg_bytes.len();
                            let send_ok = if cfg_bytes.is_empty() {
                                false
                            } else {
                                cli.send_cfg(&cfg_bytes).is_ok()
                            };
                            // 诊断（节流）：确认 ClientWait 每帧真的在发 PlayerCfg（大小/发送是否成功/直发还是入队补发队列）。
                            self.steam_lobby_wait_ticks = self.steam_lobby_wait_ticks.wrapping_add(1);
                            if self.steam_lobby_wait_ticks % 60 == 1 {
                                let st = cli.transport_ref().send_stats();
                                eprintln!(
                                    "[steam-client] ClientWait sending cfg len={cfg_len} ok={send_ok} expect_seq={} stats(direct={},queued={},recv={})",
                                    cli.expect_seq(),
                                    st.0,
                                    st.1,
                                    st.2,
                                );
                            }
                            let mut c_rcv = vec![0u8; 8192];
                            if let Some((all, participants)) = cli.recv_cfg_all(&mut c_rcv)? {
                                let stage_first = self.pre_game_config;
                                let stage = if stage_first { "pre-game" } else { "next round" };
                                eprintln!("[steam-client] got {} player configs -> {stage} (round {}), participants={}", all.len(), self.meta.round, participants.len());
                                // 收 host 广播的本局参与玩家 SteamID（按 new index），供 host 掉线时确定性选举新 host。
                                if let Ok(Some(ids)) = cli.recv_participants(&mut c_rcv) {
                                    self.steam_participants = ids.clone();
                                    self.steam_online = ids.clone(); // 初始在线集 = 全参与
                                    eprintln!("[steam-client] got participants ids={ids:?}");
                                }
                                self.steam_cli_ls = Some(cli); // 归还 cli，释放 borrow 后重建 world
                                // 首局开局：用 host 广播的参与玩家数重建 world/meta（不满员时两端角色一致），
                                // 并把本机索引更新为“在参与列表中的位置”（原 index 可能因缺席而收缩）。
                                if stage_first && !participants.is_empty() {
                                    let p = participants.len();
                                    let me_orig = self.steam_my_index;
                                    let me_new = participants.iter().position(|&x| x == me_orig).unwrap_or(0) as u8;
                                    self.stage_world_for_participants(p, STEAM_SEED);
                                    self.steam_my_index = me_new;
                                    eprintln!("[steam-client] reindexed me orig={me_orig} -> new={me_new}");
                                }
                                self.apply_player_cfgs(&all);
                                self.teardown_round_end();
                                self.net_cfg = NetCfgSync::Idle;
                                self.pre_game_config = false;
                                self.accumulator = 0.0;
                                return Ok(()); // 同步阶段不推进战斗
                            }
                            self.steam_cli_ls = Some(cli);
                            self.accumulator = 0.0;
                            return Ok(()); // 同步阶段不推进战斗
                        }
                        // Steam client：就绪/配置已完成；这里上行输入 + 严格按权威帧推进（乐观预测关）。
                        // 上行用 `send_room_state`（合包，Steam P2P 下实测可靠）；`send_input` 单独发送曾实测间歇丢。
                        let mut c_rcv = vec![0u8; 8192];
                        // 已判定掉线：冻结世界，进入重连入口（按 R 拉快照重建），不再推进世界（避免与 host 分叉）。
                        if self.conn_dropped {
                            self.poll_steam_reconnect(ctx, &mut cli);
                            self.steam_cli_ls = Some(cli);
                            self.accumulator = 0.0;
                            return Ok(());
                        }
                        // 正处于「主机迁移/重连探测」流程：推进迁移状态机（探测 host 存活 / 选举 / 接管 / 重定向），
                        // 不推进世界（避免与最终权威分叉）。
                        if self.steam_migrating {
                            let leftover = self.poll_steam_migration(cli, &mut c_rcv)?;
                            // 若是新 host 已消费 cli（转为 host_ls），则不归还；否则归还 cli。
                            if let Some(cli) = leftover {
                                self.steam_cli_ls = Some(cli);
                            }
                            self.accumulator = 0.0;
                            return Ok(());
                        }
                        while self.accumulator >= TICK {
                            let me = self.local_player_input();
                            let enc = game_core::netcode::encode_player_input(&me);
                            let _ = cli.send_room_state(self.steam_local_ready, self.steam_build_done, &enc);
                            if let Some(ents) = cli.step_frame(&mut c_rcv).ok().flatten() {
                                self.steam_cli_stale_ticks = 0; // 收到权威帧 → 清零掉线计数
                                let n = self.world.players.len();
                                let mut inputs = vec![PlayerInput::default(); n];
                                for (idx, bytes) in ents {
                                    if (idx as usize) < n {
                                        inputs[idx as usize] =
                                            game_core::netcode::decode_player_input(&bytes).unwrap_or_default();
                                    }
                                }
                                self.world.step(inputs, ticking);
                                // 诊断：打印推进到哪一帧（前若干帧/变化时不刷屏）。
                                let last = cli.expect_seq().saturating_sub(1);
                                if self.steam_cli_last_seq != last {
                                    let n_ents = self.world.players.len();
                                    eprintln!("[steam-client] frame -> seq={last}, n_ents={n_ents}");
                                    self.steam_cli_last_seq = last;
                                }
                                self.accumulator -= TICK;
                            } else {
                                // 本帧无权威帧：累计掉线计数，超阈值进入「主机迁移/重连探测」（host 可能掉线）。
                                self.steam_cli_stale_ticks = self.steam_cli_stale_ticks.saturating_add(1);
                                if self.steam_cli_stale_ticks >= CLIENT_STALE_TICKS {
                                    eprintln!("[steam-client] NO frames for {} ticks -> entering reconnect-probe / host-migration", self.steam_cli_stale_ticks);
                                    self.steam_migrating = true;
                                    self.steam_migrate_ticks = 0;
                                    self.steam_new_host_id = 0;
                                    self.steam_cli_stale_ticks = 0;
                                    self.accumulator = 0.0;
                                    break;
                                }
                                break; // 等权威帧（不扣 accumulator，避免时间凭空流逝导致分叉）
                            }
                        }
                        self.steam_cli_ls = Some(cli);
                    }
                }
                // 非 Steam 模式才走 UDP host/client/单机 分支（Steam 已在上面的块里推进了世界）。
                if !self.steam_active() {
                // 联网 · 开房作 host：接收 client 加入，全部到齐后移交 HostLockstep（不强制 READY/GO，
                // 由“host 收齐输入即产首帧”自然统一起始。）。
                self.poll_host_join_phase();
                if let Some(mut host) = std::mem::take(&mut self.net_host_ls) {
                    // 多于局：学习结束后的「配置同步」阶段——收齐各端配置(含自身) → 广播 PlayerCfgAll → 完成。
                    if self.net_cfg == NetCfgSync::HostGather {
                        let mut g_rcv = vec![0u8; 8192];
                        host.poll_cfg(&mut g_rcv);
                        let cfg_bytes = self.local_player_cfg();
                        if !cfg_bytes.is_empty() {
                            host.set_local_cfg(cfg_bytes);
                        }
                        if host.all_cfgs() {
                            let all = host.collect_cfgs().expect("all_cfgs 已确保收齐");
                            host.broadcast_cfgs(&all);
                            let stage = if self.pre_game_config { "pre-game" } else { "next round" };
                            eprintln!("[meta] host synced {} player configs -> {stage} (round {})", all.len(), self.meta.round);
                            self.apply_player_cfgs(&all);
                            self.teardown_round_end();
                            host.reset_cfgs(); // 为下一局复用
                            self.net_cfg = NetCfgSync::Idle;
                            self.pre_game_config = false;
                        }
                        self.net_host_ls = Some(host);
                        return Ok(()); // 同步阶段不推进战斗
                    }
                    // 联网 · host 运行：等齐 N 端输入才产 seq 帧（try_emit Some），用同帧喂自己 world 并广播。
                    let mut host_rcv = vec![0u8; 4096];
                    while self.accumulator >= TICK {
                        let me = self.local_player_input();
                        host.set_local_input(Some(game_core::netcode::encode_player_input(&me)));
                        host.poll(&mut host_rcv);
                        // 掉线判定：任一 client 空闲超时才自动 mark_dropped（不卡全队）。
                        for dropped_idx in host.auto_drop_idle(HOST_DROP_TICKS) {
                            eprintln!("[host] AUTO-DROP client {dropped_idx} (idle timeout) -> game continues");
                        }
                        if let Some((seq, frame)) = host.try_emit() {
                            if seq == 0 {
                                eprintln!("[host] emit seq=0: started, n_entries={}", frame.len());
                            }
                            let n = self.world.players.len();
                            let mut inputs = vec![PlayerInput::default(); n];
                            for (idx, bytes) in frame {
                                if (idx as usize) < n {
                                    inputs[idx as usize] =
                                        game_core::netcode::decode_player_input(&bytes).unwrap_or_default();
                                }
                            }
                            self.world.step(inputs, ticking);
                            // 周期保存快照（供掉线者重连时拉取当前状态接回）。
                            self.host_frame_count += 1;
                            if self.host_frame_count % SNAPSHOT_EVERY == 0 {
                                let wb = game_core::world_ser::world_to_bytes(&self.world);
                                host.set_snapshot(wb, host.next_seq());
                            }
                            self.accumulator -= TICK;
                        } else {
                            break; // 未收齐本帧：停在此 tick，下一帧 host.poll 会再收、补发缺失帧
                        }
                    }
                    self.net_host_ls = Some(host);
                } else if let Some(mut link) = std::mem::take(&mut self.net_link) {
                    // 多于局：学习结束后的「配置同步」阶段——上报我的配置，等 host 广播 PlayerCfgAll 后完成。
                    if self.net_cfg == NetCfgSync::ClientWait {
                        let cfg_bytes = self.local_player_cfg();
                        if !cfg_bytes.is_empty() {
                            link.upload_cfg(&cfg_bytes)?;
                        }
                        if let Some(all) = link.recv_cfg_all()? {
                            let stage = if self.pre_game_config { "pre-game" } else { "next round" };
                            eprintln!("[meta] client got {} player configs -> {stage} (round {})", all.len(), self.meta.round);
                            self.apply_player_cfgs(&all);
                            self.teardown_round_end();
                            self.net_cfg = NetCfgSync::Idle;
                            self.pre_game_config = false;
                        }
                        self.net_link = Some(link);
                        return Ok(()); // 同步阶段不推进战斗
                    }
                    // 联网：加入者 —— 每帧持续上行输入（让 host 能收齐并产首帧），并收帧推进。
                    // 收到首帧即开始（started 首帧凭底）；丢帧由 lockstep 自动补发，不会永久不同步。
                    // 若已判定掉线（conn_dropped）：冻结世界，等待重连入口（按 R）。
                    if self.conn_dropped {
                        // 只做重连尝试，不推进世界（避免与 host 分叉）。
                        self.poll_reconnect(ctx, &mut link);
                        self.net_link = Some(link);
                        self.accumulator = 0.0;
                        return Ok(());
                    }
                    while self.accumulator >= TICK {
                        let me = self.local_player_input();
                        let enc = game_core::netcode::encode_player_input(&me);
                        // 无条件上行（无论是否已收到首帧）。
                        link.upload(&enc)?;
                        // 收到权威帧则按权威推进（严格 lockstep，保证逐位一致）。
                        // 未收到帧【不乐观预测】——等待 host 的权威帧即可。乐观预测（4.7 阶段一）会与后续
                        // 权威帧叠加、导致本地 World 与 host 分叉（若要乐观手感需配完整回滚，见 LATENCY_MASKING 阶段二）。
                        if link.step_frame(&mut self.world, ticking)?.is_some() {
                            self.accumulator -= TICK;
                        } else {
                            link.bump_stale();
                            if link.stale_ticks() >= CLIENT_STALE_TICKS {
                                // 太久没收到权威帧 → 判定掉线，进入重连界面。
                                eprintln!("[client] NO frames for {} ticks -> connection dropped, waiting for reconnect (R)",
                                    link.stale_ticks());
                                self.conn_dropped = true;
                                return Ok(());
                            }
                            // 本 tick 不推进，等权威帧补齐（帧会由 lockstep 补发/排队，稍后追上）。
                            // 注意：不扣 accumulator，下一帧继续尝试收帧，避免时间凭空流逝导致分叉。
                            break;
                        }
                    }
                    self.net_link = Some(link);
                } else {
                    // 单机：Solo 试验场用「本机输入 + 其余(靶子)默认」；否则带 AI 机器人。
                    let is_solo = self.app == AppState::Solo;
                    while self.accumulator >= TICK {
                        if is_solo {
                            let me = self.self_index();
                            let n = self.world.players.len();
                            let mut inputs = vec![PlayerInput::default(); n];
                            if (me as usize) < n {
                                inputs[me as usize] = self.local_player_input();
                            }
                            self.world.step(inputs, ticking);
                        } else {
                            let inputs = self.compute_inputs();
                            self.world.step(inputs, ticking);
                        }
                        self.accumulator -= TICK;
                    }
                }
                } // end 非 Steam 分支
                // 一旦世界已进入前摇（施法被接受），清除待发送的施法命令，避免重复发送。
                if self.pending_cast.is_some() {
                    if let Some(p) = self.world.players.get(PLAYER_ID as usize) {
                        if p.caster.is_windup() {
                            self.pending_cast = None;
                        }
                    }
                }
                // 本局结束 → 结算并进入学习阶段
                if self.world.round_over() {
                    self.settle_round();
                }
                Ok(())
            }
        }
    }

    fn mouse_button_down_event(
        &mut self,
        _ctx: &mut Context,
        _button: ggez::input::mouse::MouseButton,
        _x: f32,
        _y: f32,
    ) -> GameResult {
        // 鼠标点击统一在 `poll_input` 里轮询处理；事件回调保持默认（无操作）。
        Ok(())
    }

    /// 诊断（本次会话加）：打印收到的键盘事件 + 窗口焦点变化。
    /// 作用：区分“按键根本没到本进程（窗口无焦点/多窗口）”与“到了但逻辑没处理”。
    /// ggez 的键盘由 winit 事件分发，**窗口无焦点时不发 KeyboardInput**，直接观察日志即可定位。
    fn key_down_event(&mut self, _ctx: &mut Context, input: ggez::input::keyboard::KeyInput, repeated: bool) -> GameResult {
        eprintln!("[input] key_down focused logical={:?} repeated={repeated}", input.event.logical_key);
        Ok(())
    }

    fn focus_event(&mut self, _ctx: &mut Context, gained: bool) -> GameResult {
        eprintln!("[input] window focus gained={gained}");
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        if self.app == AppState::MainMenu {
            return self.draw_menu(ctx);
        }
        if self.pre_game_config {
            return self.draw_pre_game(ctx);
        }
        self.draw_scene(ctx)
    }
}

impl Game {
    /// 完成开局前的技能配置：第一局前同步各端 build（局域网走 HostGather/ClientWait），单机直接开打。
    /// 联网 host：接收 client 加入，全部到齐后把 `net_host` 移交为 `net_host_ls`。
    /// 在“开局配置”阶段与 Fighting 阶段都调用，确保 host 在等人时就开始收人（否则先到的 client 会握手超时）。
    fn poll_host_join_phase(&mut self) {
        if let Some(mut hs) = std::mem::take(&mut self.net_host) {
            let mut host_rcv = vec![0u8; 4096];
            hs.poll_join(&mut host_rcv);
            if hs.joined >= hs.expected() {
                eprintln!("[host] ALL {} clients joined -> hand to HostLockstep", hs.joined);
                let n = self.world.players.len();
                // 读各 client 槽位的稳定身份（Steam=SteamID，局域网=握手随机/指定），交付给 HostLockstep 供重连按身份找回。
                let expected_clients = n.saturating_sub(1); // host 参与占 player 0
                let identities: Vec<Option<u64>> = (0..expected_clients)
                    .map(|c| hs.identity_of((1 + c) as u8))
                    .collect();
                let transport = hs.into_transport();
                let mut host_ls = net::lockstep::HostLockstep::new(transport, n, true);
                host_ls.set_client_identities(&identities);
                self.net_host_ls = Some(host_ls);
                self.net_ready = true;
            } else {
                self.net_host = Some(hs); // 尚未收齐 client，继续等
            }
        }
    }

    /// 把整场对抗（Finished）退回主菜单：放弃当前网络连接，重建为 MainMenu 的沙盒世界/meta，并清空所有运行状态。
    fn reset_to_main_menu(&mut self) {
        let seed = 20260812u64;
        // 主菜单与 Solo 共用“2 玩家 + 靶子 + sandbox”世界（不判结束 / 不缩圈）。
        let mut w = game_core::world::World::new(2, seed);
        w.sandbox = true;
        self.world = w;
        self.meta = game_core::meta::MatchState::new(game_core::meta::MatchConfig::default(), &[0], 8);
        // 开局不带默认技能：玩家从零在配置界面选。
        self.app = AppState::MainMenu;
        // 放弃联网连接（UDP socket / 握手 / 帧同步关闭）。
        self.net_link = None;
        self.net_host = None;
        self.net_host_ls = None;
        self.net_ready = false;
        self.net_cfg = NetCfgSync::Idle;
        // 放弃 Steam 会话（P2P 连接 / lockstep / 房间状态），回主菜单后重建。
        #[cfg(feature = "steam")]
        {
            // 先清 Rich Presence：会话还活着（lockstep 仍持有 transport）时才写得到，
            // 一旦下面把 lockstep 丢掉，Steam Client 就没了，好友会一直看到「加入游戏」。
            self.steam_clear_presence();
            self.steam_host_ls = None;
            self.steam_cli_ls = None;
            self.steam_in_lobby = false;
            self.steam_local_ready = false;
            self.steam_build_done = false;
            self.steam_was_all_ready = false;
            self.steam_countdown = 0.0;
            self.steam_roster_ready = Vec::new();
            self.steam_all_ready = false;
            self.steam_roster = Vec::new();
            self.steam_lobby_id = None;
            self.steam_room_edit = false;
            self.steam_room_locked = false;
            self.steam_room_edit_focus = 0;
            self.steam_edit_name = String::new();
            self.steam_edit_note = String::new();
            self.steam_lobby_menu = false;
            self.steam_lobby_create = false;
            self.steam_lobby_list = false;
            self.steam_list_requested = false;
            self.steam_list_lobbies = Vec::new();
            self.steam_join_lobby_id = None;
            self.steam_roster_refresh_ticks = 0;
            self.steam_cli_stale_ticks = 0;
            self.steam_participants = Vec::new();
            self.steam_online = Vec::new();
            self.steam_migrating = false;
            self.steam_migrate_ticks = 0;
            self.steam_new_host_id = 0;
            self.steam_host_broadcasting_takeover = false;
            // 邀请面板/好友列表状态复位（presence 已在本块开头清掉）。
            self.steam_friend_list = false;
            self.steam_friends = Vec::new();
            self.steam_friend_selection = 0;
            self.steam_friend_hint = String::new();
            // ping/头像缓存（换房间就不该复用上一房的成员数据）。
            self.steam_pings = Vec::new();
            self.steam_avatars = Vec::new();
            self.steam_net_ticks = 0;
            // 本场战绩上报标记/提示条复位（下一场重新上报）。
            self.steam_stats_recorded = false;
            self.steam_stats_snapshot = None;
            self.steam_toast = (String::new(), 0.0);
            // 进房时 `steam_sess` 会被消费掉（传输归 lockstep），回到主菜单后可再初始化一次
            // （否则好友邀请与房间列表在主菜单上会永久失效）。
            self.steam_session_tried = false;
        }
        // 清空运行状态。
        self.pre_game_config = false; // 主菜单不进入开局配置；选了模式后再进。
        self.conn_dropped = false;
        self.reconnect_attempting = false;
        self.host_frame_count = 0;
        self.pre_game_timer = PRE_GAME_TIMEOUT_SECS;
        self.learn_tree_key = game_core::skill::CastKey::ALL.first().copied();
        self.bot_targets = Vec::new();
        self.bot_rngs = Vec::new();
        self.player_target = None;
        self.pending_cast = None;
        self.pending_skill = None;
        self.queued_cmds.clear();
        self.pending_shift_skill = None;
        self.pending_clear_signal = false;
        self.pending_stop_signal = false;
        self.accumulator = 0.0;
    }

    /// 读取当前房间名与备注，host 从 matchmaking 读，无房间或非 host 时返回默认，返回二元组。
    #[cfg(feature = "steam")]
    fn steam_current_room_info(&self) -> (String, String) {
        let t = match self.steam_host_ls.as_ref() {
            Some(ls) => ls.transport_ref(),
            None => return ("未命名房间".to_string(), String::new()),
        };
        let Some(lid) = self.steam_lobby_id else {
            return ("未命名房间".to_string(), String::new());
        };
        let mm = t.matchmaking();
        let lobby = net_steam::steamworks::LobbyId::from_raw(lid);
        let name = mm
            .lobby_data(lobby, net_steam::session::ROOM_NAME_KEY)
            .unwrap_or_else(|| "未命名房间".to_string());
        let note = mm.lobby_data(lobby, net_steam::session::ROOM_NOTE_KEY).unwrap_or_default();
        (name, note)
    }

    /// 当前可用的 Steam 传输：进房后归 lockstep 持有（`into_transport`），进房前在 `steam_sess` 里。
    /// 好友邀请 / Rich Presence 只需 `&SteamTransport`（它持有唯一的 `steamworks::Client`）。
    #[cfg(feature = "steam")]
    fn steam_transport(&self) -> Option<&net_steam::SteamTransport> {
        if let Some(ls) = self.steam_host_ls.as_ref() {
            return Some(ls.transport_ref());
        }
        if let Some(ls) = self.steam_cli_ls.as_ref() {
            return Some(ls.transport_ref());
        }
        self.steam_sess.as_ref().map(|s| &s.transport)
    }

    /// 写 Rich Presence（内容变化立即写；不变则按 `STEAM_PRESENCE_INTERVAL_SECS` 节流，Steam 对频繁 set 有限速）。
    #[cfg(feature = "steam")]
    fn steam_set_presence(&mut self, now: f64, status: &str, connect: Option<&str>) {
        let key = format!("{status}|{}", connect.unwrap_or(""));
        let changed = key != self.steam_presence_text;
        if !changed && now - self.steam_presence_last < STEAM_PRESENCE_INTERVAL_SECS {
            return;
        }
        let Some(t) = self.steam_transport() else { return };
        net_steam::session::set_presence(t, status, connect);
        self.steam_presence_text = key;
        self.steam_presence_last = now;
        if changed {
            eprintln!("[steam-presence] status='{status}' connect={connect:?}");
        }
    }

    /// 清空 Rich Presence（回主菜单/退出房间：好友不再看到「加入游戏」）。
    #[cfg(feature = "steam")]
    fn steam_clear_presence(&mut self) {
        if self.steam_transport().is_none() {
            return;
        }
        if self.steam_presence_text.is_empty() {
            return;
        }
        if let Some(t) = self.steam_transport() {
            net_steam::session::clear_presence(t);
        }
        self.steam_presence_text = String::new();
        self.steam_presence_last = -999.0;
        eprintln!("[steam-presence] cleared");
    }

    /// 按当前所处阶段刷新 Rich Presence（每帧调用，内部节流）：主菜单/无房间 → 清空；
    /// 房间 → 房名 + 人数 + 等待中；配置阶段 → 配置中；对局中 → 对局中（第 N 局）。
    /// 处于房间里时带 `connect` 串 → 好友在 Steam 好友列表里看到「加入游戏」，点了能直接进同一房间。
    #[cfg(feature = "steam")]
    fn steam_refresh_presence(&mut self, now: f64) {
        // 没有 Steam 传输（未初始化 / 非 Steam 模式）→ 无事可做。
        if self.steam_transport().is_none() {
            return;
        }
        let in_room = self.steam_lobby_id.is_some();
        if !in_room {
            self.steam_clear_presence();
            return;
        }
        let connect = net_steam::lobby::format_connect_string(self.steam_lobby_id.unwrap_or(0));
        let status = if self.steam_room_edit {
            "正在设置房间".to_string()
        } else if self.steam_in_lobby {
            let (name, _) = self.steam_current_room_info();
            let n = self.steam_roster.len();
            let limit = self.world.players.len().max(n);
            format!("房间「{name}」{n}/{limit} 等待中")
        } else if self.pre_game_config {
            "正在配置技能".to_string()
        } else {
            format!("对局中（第 {} 局）", self.meta.round)
        };
        self.steam_set_presence(now, &status, Some(&connect));
    }

    /// 某位成员的 ping（毫秒）；没测到返回 `None`（界面显示“--”，不要显示 0 误导）。
    #[cfg(feature = "steam")]
    fn steam_ping_of(&self, id: u64) -> Option<i32> {
        self.steam_pings.iter().find(|(k, _)| *k == id).map(|(_, ms)| *ms)
    }

    /// 节流刷新网络信息：各成员 ping + 补拉缺失头像（每 30 帧一次）。
    /// 头像只补没缓存过的（Steam 首次进房常拉不到，下一轮自动重试）。
    #[cfg(feature = "steam")]
    fn steam_refresh_network_info(&mut self, ctx: &Context) {
        self.steam_net_ticks = self.steam_net_ticks.wrapping_add(1);
        if self.steam_net_ticks % 30 != 1 {
            return;
        }
        // 先把要查的 SteamID 抄出来（避免 `steam_transport()` 的借用挡住后面的 &mut self）。
        // 含房间成员 +（邀请面板展开时）好友列表里的人，好让两边都能显示头像。
        let member_ids: Vec<u64> = self.steam_roster.iter().map(|(_, _, id)| *id).collect();
        let mut ids = member_ids.clone();
        if self.steam_friend_list {
            for f in self.steam_friends.iter() {
                if !ids.contains(&f.id) {
                    ids.push(f.id);
                }
            }
        }
        let Some(t) = self.steam_transport() else { return };
        let my_id = t.steam_id();
        // ping：只查房间成员里的别人（自己到自己是 0，没意义；好友没建会话也测不出来）。
        let mut pings = Vec::new();
        for id in member_ids.iter().copied().filter(|id| *id != my_id) {
            if let Some(ms) = net_steam::session::ping_to(t, id) {
                pings.push((id, ms));
            }
        }
        // 头像：只补缺失的。先把字节取出来（此时仍在借用 t），等 t 用完了再写回 self。
        let mut fetched: Vec<(u64, Vec<u8>, u32)> = Vec::new();
        for id in ids {
            if self.steam_avatars.iter().any(|(k, _)| *k == id) {
                continue;
            }
            if let Some((rgba, side)) = net_steam::session::avatar_rgba(t, id, net_steam::session::AvatarSize::Small) {
                fetched.push((id, rgba, side));
            }
        }
        // t 到此不再使用 → 可以改 self 了。
        self.steam_pings = pings;
        for (id, rgba, side) in fetched {
            let img = graphics::Image::from_pixels(
                &ctx.gfx,
                &rgba,
                graphics::ImageFormat::Rgba8UnormSrgb,
                side,
                side,
            );
            self.steam_avatars.push((id, img));
        }
    }

    /// 画某位成员的头像（有缓存才画）；返回是否画了，调用方据此调整文字缩进。
    #[cfg(feature = "steam")]
    fn steam_draw_avatar(&self, canvas: &mut Canvas, id: u64, x: f32, y: f32, size: f32) -> bool {
        let Some((_, img)) = self.steam_avatars.iter().find(|(k, _)| *k == id) else {
            return false;
        };
        let s = size / 32.0; // 缓存的是 32x32 小头像
        canvas.draw(img, graphics::DrawParam::new().dest(Point2 { x, y }).scale([s, s]));
        true
    }

    /// 排行榜句柄：每会话只查找一次（Steam 的查找是异步回调，结果写回 `steam_lb_slot`）。
    /// 建房后待在房间时就会查好，整场结束要用时直接取。
    #[cfg(feature = "steam")]
    fn steam_ensure_leaderboard(&mut self) {
        if self.steam_lb_requested {
            return;
        }
        let Some(t) = self.steam_transport() else { return };
        net_steam::session::request_leaderboard(t, net_steam::stats::LEADERBOARD, &self.steam_lb_slot);
        self.steam_lb_requested = true;
    }

    /// 整场结束（进入 Finished）时把战绩上报 Steam：统计 + 成就 + 排行榜，只上报一次。
    /// 统计/成就/排行榜都要在 Steamworks 后台先定义 key，没配置时只会打日志、不影响游戏。
    #[cfg(feature = "steam")]
    fn steam_record_match_result(&mut self, now: f64) {
        if self.steam_stats_recorded {
            return;
        }
        self.steam_stats_recorded = true;
        let Some(t) = self.steam_transport() else { return };
        // 本场战绩摘要：从我方档案取（击杀/最佳名次/存活局数），人数与局数从 world/meta 取。
        let me = self.self_index();
        let (kills, best_placement, rounds_survived) = self
            .meta
            .profiles
            .iter()
            .find(|p| p.player_id == me)
            .map(|p| (p.total_kills, p.best_placement, p.rounds_survived))
            .unwrap_or((0, 0, 0));
        let summary = net_steam::stats::MatchSummary {
            kills,
            best_placement,
            players: self.world.players.len().max(1) as u32,
            rounds: self.meta.round.max(1),
            rounds_survived,
        };
        let report = net_steam::session::record_match_result(t, summary);
        // 排行榜：句柄查到了就上传分数；没查到（后台没建榜/还没回调）就跳过。
        let lb = self.steam_lb_slot.lock().unwrap().clone();
        if let Some(lb) = lb.as_ref() {
            net_steam::session::upload_leaderboard_score(t, lb, report.score);
        }
        // 结算界面要展示：读回统计 + 拉一次榜单前 5（都是异步/只读，失败不影响）。
        let snap = net_steam::session::stats_snapshot(t);
        if let Some(lb) = lb.as_ref() {
            net_steam::session::request_leaderboard_top(t, lb, 5, &self.steam_lb_rows);
        }
        // t 到此不再使用 → 写回 self。
        self.steam_stats_snapshot = Some(snap);
        let msg = if !report.achievements.is_empty() {
            let names: Vec<&str> = report
                .achievements
                .iter()
                .map(|k| net_steam::stats::achievement_label(k))
                .collect();
            format!("成就已上报：{}", names.join("、"))
        } else if report.had_failure {
            "战绩上报未生效（需在 Steamworks 后台配置统计/成就）".to_string()
        } else {
            String::new()
        };
        if !msg.is_empty() {
            self.steam_toast = (msg, now + 6.0);
        }
    }

    /// 刷新好友列表（展开邀请面板时调一次；R 手动刷新）。
    #[cfg(feature = "steam")]
    fn steam_refresh_friends(&mut self) {
        let lobby = self.steam_lobby_id;
        let Some(t) = self.steam_transport() else { return };
        let friends = net_steam::session::list_friends(t, lobby);
        self.steam_friends = friends;
        if self.steam_friend_selection >= self.steam_friends.len() {
            self.steam_friend_selection = self.steam_friends.len().saturating_sub(1);
        }
    }

    /// 「邀请好友」面板的输入（房间界面按 I 展开，非模态——房间网络逻辑每帧照常跑）：
    /// ↑/↓ 选择、**回车 邀请选中好友**、A 打开 Steam 邀请窗口（可勾多位）、R 刷新、I/Q 收起面板。
    #[cfg(feature = "steam")]
    fn steam_friend_list_update(&mut self, ctx: &Context) {
        use ggez::input::keyboard::Key;
        use winit::keyboard::NamedKey;
        let just = |k: char| ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.to_string().into()));
        let just_named = |n: NamedKey| ctx.keyboard.is_logical_key_just_pressed(&Key::Named(n));
        // 收起面板（I 或 Q；Q 第一次只收起面板，再按才退出房间）。
        if just('i') || just('I') || just('q') || just('Q') {
            self.steam_friend_list = false;
            return;
        }
        if just('r') || just('R') {
            self.steam_refresh_friends();
            self.steam_friend_hint = "已刷新好友列表".to_string();
            return;
        }
        if just('a') || just('A') {
            match self.steam_lobby_id {
                Some(lid) => {
                    if let Some(t) = self.steam_transport() {
                        net_steam::session::open_invite_dialog(t, lid);
                        self.steam_friend_hint = "已打开 Steam 邀请窗口（可勾选多位好友）".to_string();
                    }
                }
                None => self.steam_friend_hint = "尚未在房间里，无法邀请".to_string(),
            }
            return;
        }
        let n = self.steam_friends.len();
        if n == 0 {
            return;
        }
        if just_named(NamedKey::ArrowDown) {
            self.steam_friend_selection = (self.steam_friend_selection + 1) % n;
        } else if just_named(NamedKey::ArrowUp) {
            self.steam_friend_selection = (self.steam_friend_selection + n - 1) % n;
        }
        if just_named(NamedKey::Enter) || just('\r') {
            let sel = self.steam_friends[self.steam_friend_selection].clone();
            if sel.in_lobby {
                self.steam_friend_hint = format!("{} 已经在房间里了", sel.name);
                return;
            }
            match self.steam_lobby_id {
                Some(lid) => {
                    if let Some(t) = self.steam_transport() {
                        net_steam::session::invite_friend(t, lid, sel.id);
                        self.steam_friend_hint = format!("已邀请 {}{}", sel.name, if sel.online { "" } else { "（离线，邀请会等到其上线）" });
                    }
                }
                None => self.steam_friend_hint = "尚未在房间里，无法邀请".to_string(),
            }
        }
    }

    /// 主菜单：best-effort 初始化一次 Steam 会话，好让好友从 Steam 好友列表点「加入游戏」时
    /// 我们这边的 `GameLobbyJoinRequested` 回调能收到（回调只在 `run_callbacks` 时泵出，必须有 Client）。
    /// 失败（Steam 未运行/未登录）不影响单机与局域网，只是收不到邀请。
    #[cfg(feature = "steam")]
    fn steam_ensure_session(&mut self) {
        if self.steam_sess.is_some() || self.steam_session_tried {
            return;
        }
        self.steam_session_tried = true;
        match net_steam::session::SteamSession::init(APP_ID, STEAM_VIRTUAL_PORT) {
            Ok(s) => {
                self.steam_my_display_name = s
                    .transport
                    .friends()
                    .get_friend(net_steam::steamworks::SteamId::from_raw(s.transport.steam_id()))
                    .name();
                eprintln!("[steam] session ready, display name='{}'", self.steam_my_display_name);
                self.steam_sess = Some(s);
            }
            Err(e) => eprintln!("[steam] session init failed (邀请将不可用): {e:?}"),
        }
    }

    /// 处理好友从 Steam 发起的「加入游戏」请求（主菜单/大厅界面每帧调用）：
    /// 需要已初始化会话（pump 回调才拿得到）、且当前不在房间里；命中则按 lobby id 直接进房。
    #[cfg(feature = "steam")]
    fn steam_poll_join_requests(&mut self, ctx: &mut Context) {
        if let Some(s) = self.steam_sess.as_ref() {
            s.run_callbacks();
        }
        let Some(req) = self.steam_sess.as_ref().and_then(|s| s.take_join_request()) else {
            return;
        };
        if self.steam_in_lobby || self.steam_host_ls.is_some() || self.steam_cli_ls.is_some() {
            eprintln!("[steam-invite] ignoring join request: already in a room");
            return;
        }
        eprintln!("[steam-invite] friend {} invited us to lobby {}", req.from, req.lobby);
        self.steam_join_lobby_id = Some(req.lobby);
        self.steam_lobby_menu = false;
        self.steam_lobby_create = false;
        self.steam_lobby_list = false;
        self.steam_friend_hint = "已从邀请加入房间".to_string();
        self.enter_steam_mode(ctx, false, 2, None, None);
    }

    /// 房主「编辑房间信息」子界面输入：改房间名/备注，回车保存（写回 matchmaking 元数据），Q 取消；
    /// 附带 `L` 锁定/解锁房间（`set_lobby_joinable`；人数上限建房时固定，用锁房代替“开房后改人数”）。
    #[cfg(feature = "steam")]
    fn steam_room_edit_update(&mut self, ctx: &Context, _dt: f64) -> GameResult {
        use ggez::input::keyboard::Key;
        use winit::keyboard::NamedKey;
        let just = |k: char| ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.to_string().into()));
        let just_named = |n: NamedKey| ctx.keyboard.is_logical_key_just_pressed(&Key::Named(n));
        // 锁房：L 切换（host 且记录到字段），即时 set_lobby_joinable。
        if just('l') || just('L') {
            if let Some(ls) = self.steam_host_ls.as_ref() {
                if let Some(lid) = self.steam_lobby_id {
                    let mm = ls.transport_ref().matchmaking();
                    let lobby = net_steam::steamworks::LobbyId::from_raw(lid);
                    self.steam_room_locked = !self.steam_room_locked;
                    mm.set_lobby_joinable(lobby, !self.steam_room_locked);
                    eprintln!("[steam-room] set joinable={} (locked={})", !self.steam_room_locked, self.steam_room_locked);
                }
            }
        }
        // 字段切换 0=房间名 1=备注（↑/↓ 或 Tab）。
        if just_named(NamedKey::ArrowUp) || just_named(NamedKey::ArrowDown) || just_named(NamedKey::Tab) {
            self.steam_room_edit_focus = (self.steam_room_edit_focus + 1) % 2;
        }
        if just('q') || just('Q') {
            self.steam_room_edit = false;
            return Ok(());
        }
        if just_named(NamedKey::Enter) || just('\r') {
            // 保存：写回房间名/备注。
            if let Some(ls) = self.steam_host_ls.as_ref() {
                if let Some(lid) = self.steam_lobby_id {
                    let mm = ls.transport_ref().matchmaking();
                    let lobby = net_steam::steamworks::LobbyId::from_raw(lid);
                    let name = if self.steam_edit_name.trim().is_empty() {
                        "未命名房间"
                    } else {
                        self.steam_edit_name.trim()
                    };
                    mm.set_lobby_data(lobby, net_steam::session::ROOM_NAME_KEY, name);
                    mm.set_lobby_data(lobby, net_steam::session::ROOM_NOTE_KEY, self.steam_edit_note.trim());
                    eprintln!("[steam-room] saved name='{name}' note='{}'", self.steam_edit_note.trim());
                }
            }
            self.steam_room_edit = false;
            return Ok(());
        }
        // 文本输入：聚焦字段 0=名 1=备注。
        if just_named(NamedKey::Backspace) {
            let buf = if self.steam_room_edit_focus == 0 { &mut self.steam_edit_name } else { &mut self.steam_edit_note };
            buf.pop();
            return Ok(());
        }
        let buf = if self.steam_room_edit_focus == 0 { &mut self.steam_edit_name } else { &mut self.steam_edit_note };
        if buf.len() < 80 {
            const CHARS: &str = " abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.(),;:!?'\"-_#@%&*+=/";
            for c in CHARS.chars() {
                if just(c) {
                    buf.push(c);
                    return Ok(());
                }
            }
        }
        self.accumulator = 0.0;
        Ok(())
    }

    /// 绘制「编辑房间信息」界面（房主）：房间名/备注 两字段 + 锁房状态。
    #[cfg(feature = "steam")]
    fn draw_steam_room_edit(&self, canvas: &mut Canvas, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;
        draw_text(canvas, ctx, "编辑房间信息", 36.0, Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.26 }, true)?;
        let labels = ["房间名", "备注"];
        let vals = [self.steam_edit_name.clone(), self.steam_edit_note.clone()];
        let mut y = sh * 0.42;
        let label_w = 200.0;
        let box_w = 420.0;
        let box_h = 56.0;
        let left = cx - box_w / 2.0 - 40.0;
        for i in 0..2 {
            let selected = i == self.steam_room_edit_focus;
            draw_text(canvas, ctx, labels[i], 24.0, Color::from_rgb(220, 224, 235), Point2 { x: left + (label_w + box_w) / 2.0, y: y + box_h / 2.0 - 16.0 }, true)?;
            let bg_col = if selected { Color::from_rgb(52, 60, 74) } else { Color::from_rgb(30, 34, 44) };
            let bg = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), graphics::Rect::new(left + label_w, y, box_w, box_h), bg_col)?;
            canvas.draw(&bg, graphics::DrawParam::new());
            let disp = if vals[i].is_empty() {
                if i == 0 { "（输入房间名）".to_string() } else { "（可选）".to_string() }
            } else {
                format!("  {}", vals[i])
            };
            draw_text(canvas, ctx, &disp, 22.0, if vals[i].is_empty() { Color::from_rgb(120, 130, 150) } else { Color::WHITE }, Point2 { x: left + label_w + box_w / 2.0, y: y + box_h / 2.0 - 14.0 }, true)?;
            y += box_h + 30.0;
        }
        let lock_txt = if self.steam_room_locked { "[v] 已锁定（他人不能加入）" } else { "[ ] 未锁定（可加入）" };
        draw_text(canvas, ctx, &format!("房间锁：{lock_txt}（按 L 切换）"), 22.0, if self.steam_room_locked { Color::from_rgb(235, 150, 90) } else { Color::from_rgb(140, 200, 160) }, Point2 { x: cx, y: y + 30.0 }, true)?;
        draw_text(canvas, ctx, "人数上限建房时固定（steamworks 限制），用房间锁控制新入", 17.0, Color::from_rgb(150, 160, 178), Point2 { x: cx, y: y + 62.0 }, true)?;
        draw_text(canvas, ctx, "回车 保存    Q 取消    L 锁房", 20.0, Color::from_rgb(160, 200, 255), Point2 { x: cx, y: sh * 0.90 }, true)?;
        Ok(())
    }

    /// host 在房间阶段刷新成员名单（roster）：client 加入后 host 才能看到并显示新成员。
    /// 从 matchmaking 读 `lobby_members` → `LobbyPlayerTable`（host=槽0，其余按 SteamID 升序）保持槽位与 lockstep 一致。
    #[cfg(feature = "steam")]
    fn steam_refresh_roster(&mut self) {
        let Some(lid) = self.steam_lobby_id else { return; };
        let Some(ls) = self.steam_host_ls.as_ref() else { return; };
        let t = ls.transport_ref();
        let host_id = t.steam_id();
        let mm = t.matchmaking();
        let lobby = net_steam::steamworks::LobbyId::from_raw(lid);
        let members: Vec<net_steam::lobby::SteamID> = mm.lobby_members(lobby).iter().map(|s| net_steam::lobby::SteamID(s.raw())).collect();
        let table = net_steam::lobby::LobbyPlayerTable::new(net_steam::lobby::SteamID(host_id), members);
        let fr = t.friends();
        let mut roster = Vec::with_capacity(table.total_players());
        for (slot, id) in table.identities_in_order() {
            let name = fr.get_friend(net_steam::steamworks::SteamId::from_raw(id.0)).name();
            roster.push((slot, name, id.0));
        }
        // 仅当成员集合变化时更新（避免每帧重建 + 日志）；并检测“有玩家离开”给 host 提示。
        let changed = roster.len() != self.steam_roster.len()
            || roster.iter().zip(self.steam_roster.iter()).any(|(a, b)| a != b);
        let new_len = roster.len();
        let prev_len = self.steam_last_roster_len;
        self.steam_last_roster_len = new_len;
        if changed {
            if prev_len > new_len && prev_len > 1 {
                eprintln!("[steam-host] a player left the room (members {prev_len} -> {new_len}), waiting");
            } else {
                eprintln!("[steam-host] roster updated: {new_len} member(s)");
            }
            self.steam_roster = roster;
        }
    }

    /// 退出房间：`leave_lobby`（让 Steam 后端不再占席）+ 清理会话回到主菜单。
    #[cfg(feature = "steam")]
    fn steam_leave_room(&mut self) {
        if let Some(lid) = self.steam_lobby_id {
            let lobby = net_steam::steamworks::LobbyId::from_raw(lid);
            if let Some(host) = self.steam_host_ls.as_ref() {
                host.transport_ref().matchmaking().leave_lobby(lobby);
            } else if let Some(cli) = self.steam_cli_ls.as_ref() {
                cli.transport_ref().matchmaking().leave_lobby(lobby);
            }
        }
        eprintln!("[steam] leave room -> main menu");
        self.steam_room_edit = false;
        self.steam_lobby_id = None;
        self.steam_room_locked = false;
        self.reset_to_main_menu();
    }

    /// Steam 房间/就绪阶段（每帧）：client 每帧上行「就绪+在场」合包；host poll 收各端、全员就绪倒数计时进配置。
    /// 房主按 E 进入编辑房间信息界面（见 `steam_room_edit_update`）。
    #[cfg(feature = "steam")]
    fn steam_lobby_update(&mut self, ctx: &Context, dt: f64) -> GameResult {
        use ggez::input::keyboard::Key;
        // 「邀请好友」面板展开时由面板优先吃键（I/Q 收起、↑↓ 选择、回车 邀请、A 开 Steam 邀请窗口、R 刷新）。
        // 面板是**非模态**的：下面房间的网络逻辑（上行/广播/倒计时）照常每帧跑，
        // 否则 host 打开面板挑人时 client 会因收不到 host 心跳而判定「host 已离开」自动退房。
        let panel_open = self.steam_friend_list;
        // I：展开/收起「邀请好友」面板（展开时拉一次好友列表）。
        let i_pressed = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("i".into()))
            || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("I".into()));
        if i_pressed && !panel_open {
            self.steam_friend_list = true;
            self.steam_friend_hint = String::new();
            self.steam_refresh_friends();
            eprintln!("[steam-invite] friend list opened ({} friends)", self.steam_friends.len());
            self.accumulator = 0.0;
            return Ok(());
        }
        if panel_open {
            self.steam_friend_list_update(ctx);
        }
        // 房间阶段只用 [U] 就绪/取消就绪；不再用 o/空格（避免 o 多重语义、避免与“配置确认”混淆）。
        let ready_pressed = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("u".into()))
            || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("U".into()));
        // Q：退出房间（leave_lobby + 回主菜单）。面板展开时 Q 只收起面板（由面板处理），避免误退出。
        let q_pressed = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("q".into()))
            || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("Q".into()));
        if q_pressed && !panel_open {
            self.steam_leave_room();
            self.accumulator = 0.0;
            return Ok(());
        }
        // host 按 E 进入「编辑房间信息」子界面（改房间名/备注；人数上限建房时固定，走锁房代替）。
        let e_pressed = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("e".into()))
            || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("E".into()));
        if e_pressed && !panel_open && self.steam_host_ls.is_some() {
            let (cur_name, cur_note) = self.steam_current_room_info();
            self.steam_edit_name = cur_name;
            self.steam_edit_note = cur_note;
            self.steam_room_edit_focus = 0;
            self.steam_room_edit = true;
            self.accumulator = 0.0;
            return Ok(());
        }
        // 倒计时锁定窗口：仅 host 端维护 `steam_was_all_ready`/`steam_countdown`；client 端恒为 false/0 → locked=false。
        // 锁定窗口内忽略「按 U 取消就绪」（防止有人卡在最后两秒取消导致不同步）。
        let locked = self.steam_was_all_ready && self.steam_countdown <= STEAM_COUNTDOWN_LOCK_SECS;
        if ready_pressed && !locked && !panel_open {
            self.steam_local_ready = !self.steam_local_ready;
            eprintln!("[steam-lobby] local ready = {}", self.steam_local_ready);
        } else if ready_pressed && locked {
            eprintln!("[steam-lobby] ignoring ready-cancel during locked countdown");
        }
        let mut entered_config = false;
        // 本机当前输入（房间阶段就用它做「在场信号」，对齐局域网 upload；与对局开始后一致）。
        let presence_enc = game_core::netcode::encode_player_input(&self.local_player_input());
        if let Some(cli) = self.steam_cli_ls.as_mut() {
            // client：房间阶段用「就绪+在场+配好」合包持续上行（`RoomState`），走已证实可靠的输入在场通道。
            // 房间阶段 build_done 恒为 false（进配置后才置 true）。
            let room_res = cli.send_room_state(self.steam_local_ready, self.steam_build_done, &presence_enc);
            if let Err(e) = room_res {
                if self.steam_last_sent_ready.is_none() {
                    eprintln!("[steam-client] send_room_state failed: {e:?}");
                    self.steam_last_sent_ready = Some(self.steam_local_ready);
                }
            } else if self.steam_last_sent_ready != Some(self.steam_local_ready) {
                eprintln!("[steam-client] sent room_state ready={} to host", self.steam_local_ready);
                self.steam_last_sent_ready = Some(self.steam_local_ready);
            }
            // host 离开检测：每帧先累计“沉默”帧数；收不到 host 广播（RosterReady 心跳）累计，收到即清零。
            self.steam_lobby_silent_ticks = self.steam_lobby_silent_ticks.saturating_add(1);
            // 单次排空读 host 房间入包：StartConfig（进配置）与 RosterReady（界面）一次分类，绝不互吞。
            let mut rcv = [0u8; 8192];
            let mut roster_all_ready = false; // 本帧 host 广播的就绪快照是否“全员就绪”
            if let Ok((got_cfg, roster)) = cli.recv_room_inbox(&mut rcv) {
                if got_cfg {
                    eprintln!("[steam-client] host says all ready -> config menu");
                    entered_config = true;
                }
                if let Some(entries) = roster {
                    self.steam_lobby_silent_ticks = 0; // 收到 host 广播 → 心跳正常。
                    self.steam_roster_ready = entries.clone();
                    eprintln!("[steam-client] roster ready snapshot: {entries:?}");
                    let roster_cnt = self.world.players.len();
                    roster_all_ready = entries.len() >= roster_cnt && entries.iter().all(|(_, r)| *r);
                }
            }
            // client 端就绪倒计时：与 host 一致的缓冲，避免“一看到全员就绪就抢先进配置”。
            // 正常路径由 host 倒计时归零广播 StartConfig（got_cfg）触发；此处兜底：若 StartConfig 小包被丢，
            // 用可靠 RosterReady 启动同样长度的倒计时，归零后同样进配置，保证两端同时开始。
            let locked = self.steam_was_all_ready && self.steam_countdown <= STEAM_COUNTDOWN_LOCK_SECS;
            if !roster_all_ready && !locked {
                self.steam_was_all_ready = false;
                self.steam_countdown = 0.0;
            } else if roster_all_ready && !self.steam_was_all_ready {
                self.steam_was_all_ready = true;
                self.steam_countdown = STEAM_READY_COUNTDOWN_SECS;
                eprintln!("[steam-client] all ready -> countdown {}", self.steam_countdown);
            }
            self.steam_all_ready = roster_all_ready || locked;
            if self.steam_was_all_ready {
                self.steam_countdown = (self.steam_countdown - dt.min(0.25) as f32).max(0.0);
                if self.steam_countdown <= 0.0 {
                    eprintln!("[steam-client] ready countdown zero -> config menu (StartConfig fallback)");
                    entered_config = true;
                }
            }
        } else if let Some(host) = self.steam_host_ls.as_mut() {
            // host：每帧 poll 收客户端（持续在场 + PlayerReady）；要求所有 client 在场 && 全体就绪。
            let mut rcv = [0u8; 8192];
            host.poll(&mut rcv);
            let all_present = host.saw_all_clients(); // 所有 expected client 都已上行过输入（满员在场）
            let all_clients_ready = host.all_clients_ready();
            let present = host.present_clients_count();
            let expected = host.expected_clients();
            let full = present >= expected; // 是否满员
            // 满员 && 全员（含 host）就绪 → 自动倒计时启动（现有）。
            let full_ready = self.steam_local_ready && all_present && all_clients_ready;
            // 不满员但“当前在场的都就绪” → 不自动倒计时，由 host 手动确认开始（人不满开打由 host 拍板）。
            let underfull_ready = !full && self.steam_local_ready && present > 0 && host.ready_clients_count() == present;
            self.steam_manual_start_pending = underfull_ready;
            // 每帧广播就绪状态快照，让各端都能看到所有成员的就绪状态（多人一致界面）。
            host.broadcast_roster_ready(self.steam_local_ready);
            // —— 满员路径：全员就绪 → 缓冲倒计时，可取消，最后 LOCK 秒锁定；归零自动启动。
            if !full_ready && !locked {
                self.steam_was_all_ready = false;
                self.steam_countdown = 0.0;
            } else if full_ready && !self.steam_was_all_ready {
                self.steam_was_all_ready = true;
                self.steam_countdown = STEAM_READY_COUNTDOWN_SECS;
            }
            self.steam_all_ready = full_ready || locked;
            if self.steam_was_all_ready {
                self.steam_countdown = (self.steam_countdown - dt.min(0.25) as f32).max(0.0);
                if self.steam_countdown <= 0.0 {
                    // 缓冲归零 → 统一广播 StartConfig 进配置（满员：参与集=全在场）。
                    let mask = host.present_mask();
                    host.set_participants(&mask);
                    let n = mask.iter().filter(|&&b| b).count();
                    eprintln!("[steam-host] full ready countdown zero -> start with {n} participant client(s), mask={mask:?}");
                    host.broadcast_start_config();
                    entered_config = true;
                }
            }
            // —— 不满员路径：当前在场都就绪 → 提示并由 host 手动开始（回车）。人数不足时不自动倒计时。
            // 不满员也提示“只来了 X/上限 Y，全员就绪，host 按回车开始”。
            // 面板展开时回车归面板（邀请好友），这里让位，避免“想邀请却开局”。
            if underfull_ready && !panel_open {
                use winit::keyboard::NamedKey;
                let enter = ctx.keyboard.is_logical_key_just_pressed(&ggez::input::keyboard::Key::Named(NamedKey::Enter))
                    || ctx.keyboard.is_logical_key_just_pressed(&ggez::input::keyboard::Key::Character("\r".into()));
                if enter {
                    let mask = host.present_mask();
                    host.set_participants(&mask);
                    let n = mask.iter().filter(|&&b| b).count();
                    eprintln!("[steam-host] host manually starts underfull match: {present}/{expected} clients, {n} participant(s), mask={mask:?}");
                    host.broadcast_start_config();
                    entered_config = true;
                }
            }
            if !full_ready && !locked {
                // 节流诊断：每 ~120 帧打一次，说明“等了谁”（在场/就绪各几何），便于真机定位 Steam 联机卡点。
                self.steam_lobby_wait_ticks = self.steam_lobby_wait_ticks.wrapping_add(1);
                if self.steam_lobby_wait_ticks % 120 == 1 {
                    let pres = host.present_clients_count();
                    let rdy = host.ready_clients_count();
                    let alive = host.connected_clients_count();
                    let pkts = host.ready_packets_seen();
                    let exp = host.expected_clients();
                    eprintln!(
                        "[steam-host] waiting: local_ready={} present_clients={pres}/{exp} ready_clients={rdy}/{exp} alive_conns={alive} ready_pkts={pkts} underfull_ready={underfull_ready} full_ready={full_ready}",
                        self.steam_local_ready
                    );
                }
            }
        }
        // client：host 提前离开（超过 N 帧收不到 host 广播）→ 自动退出房间回主菜单（host 不应让 client 永久卡在等待）。
        let host_missing = self.steam_cli_ls.is_some()
            && self.steam_lobby_silent_ticks >= STEAM_LOBBY_SILENT_TIMEOUT_TICKS;
        if host_missing {
            eprintln!(
                "[steam-client] host left (no heartbeat for {} ticks) -> leave room",
                self.steam_lobby_silent_ticks
            );
            self.steam_leave_room();
            self.accumulator = 0.0;
            return Ok(());
        }
        if entered_config {
            self.steam_in_lobby = false;
            self.pre_game_config = true; // 进开局配置菜单（技能/点数）。
            // 本端进入配置：build_done 由玩家在配置阶段重新按 o 确认（重新收集）。
            // （不再对 host 侧 client build_done 做 reset：client 会在其进入配置、按 o 后再次上报 build_done=true，
            //  避免“host 进配置晚于 client 已配完、reset 把已上报的 build_done 清掉导致 host 永远等不到”。）
            self.steam_build_done = false;
            self.net_cfg = NetCfgSync::Idle;
        }
        // host：节流刷新成员名单（client 加入后 host 界面才能显示新成员）。
        self.steam_roster_refresh_ticks = self.steam_roster_refresh_ticks.wrapping_add(1);
        if self.steam_roster_refresh_ticks % 30 == 1 {
            self.steam_refresh_roster();
        }
        // 节流刷新 ping 与头像（房间界面显示延迟/头像）。
        self.steam_refresh_network_info(ctx);
        // 排行榜句柄：待在房间时就查好（异步回调），整场结束要用。
        self.steam_ensure_leaderboard();
        // Rich Presence：房间里 → “房间「名」n/m 等待中” + connect 串（好友可一键加入）。
        self.steam_refresh_presence(ctx.time.time_since_start().as_secs_f64());
        self.accumulator = 0.0;
        Ok(())
    }

    /// 大厅子菜单（创建/加入/返回）执行选中项。0=创建 1=加入 2=返回。
    #[cfg(feature = "steam")]
    fn steam_lobby_act(&mut self, sel: usize) {
        match sel {
            0 => {
                eprintln!("[menu] Steam -> create-lobby setup");
                // 进入建房设置：重置字段；默认房间名用「昵称的房间」（若昵称已知）。
                let disp = self.steam_my_display_name.clone();
                self.steam_create_name = if disp.is_empty() {
                    "我的房间".to_string()
                } else {
                    format!("{disp}的房间")
                };
                self.steam_create_note = String::new();
                self.steam_create_focus = 0;
                self.steam_create_players = STEAM_DEFAULT_PLAYERS;
                self.steam_create_rounds = STEAM_DEFAULT_ROUNDS;
                self.steam_create_players_buf = STEAM_DEFAULT_PLAYERS.to_string();
                self.steam_create_rounds_buf = STEAM_DEFAULT_ROUNDS.to_string();
                self.steam_lobby_create = true;
            }
            1 => {
                eprintln!("[menu] Steam -> join lobby list");
                self.steam_lobby_list = true;
                self.steam_list_requested = false;
                self.steam_list_lobbies = Vec::new();
                self.steam_list_selection = 0;
            }
            2 => {
                self.steam_lobby_menu = false;
            }
            _ => {}
        }
    }

    /// 建房设置界面输入：四个字段（房间名/备注/人数）。
    /// - ↑/↓ 或 Tab 切换字段；在文本字段可输入 ascii+空格+常用标点、Backspace 删末字符；人数字段 `+`/`-` 或直接输数字（2..=STEAM_MAX_PLAYERS）。
    /// - 回车=创建房间（用现有 steam_sess 建厅+写房间元数据）；Q=放弃返回大厅主界面。
    #[cfg(feature = "steam")]
    fn steam_lobby_create_update(&mut self, ctx: &mut Context) {
        use ggez::input::keyboard::Key;
        use winit::keyboard::NamedKey;
        let just = |k: char| ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.to_string().into()));
        let just_named = |n: NamedKey| ctx.keyboard.is_logical_key_just_pressed(&Key::Named(n));
        let parse_num = |s: &str, fallback: u32| s.parse::<u32>().unwrap_or(fallback);
        // 切换字段 0=房名 1=备注 2=人数 3=轮数。
        if just_named(NamedKey::ArrowUp) || just_named(NamedKey::Tab) {
            self.steam_create_focus = (self.steam_create_focus + 3) % 4;
        } else if just_named(NamedKey::ArrowDown) {
            self.steam_create_focus = (self.steam_create_focus + 1) % 4;
        }
        if just('q') || just('Q') {
            self.steam_lobby_create = false; // 返回大厅主界面
            return;
        }
        match self.steam_create_focus {
            0 | 1 => {
                // 文本字段：房名 / 备注。
                if just_named(NamedKey::Backspace) {
                    let buf = if self.steam_create_focus == 0 { &mut self.steam_create_name } else { &mut self.steam_create_note };
                    buf.pop();
                    return;
                }
                let buf = if self.steam_create_focus == 0 { &mut self.steam_create_name } else { &mut self.steam_create_note };
                if buf.len() >= 80 {
                    return;
                }
                // 可打印 ascii 字符（字母大小写/数字/空格/常用标点）。
                const CHARS: &str = " abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.(),;:!?'\"-_#@%&*+=/";
                for c in CHARS.chars() {
                    if just(c) {
                        buf.push(c);
                        return;
                    }
                }
            }
            2 => {
                // 人数字段：数字输入 + Backspace 逐位删 + +/- 步进（2..=STEAM_MAX_PLAYERS）。
                if just('+') {
                    let v = parse_num(&self.steam_create_players_buf, STEAM_DEFAULT_PLAYERS as u32);
                    self.steam_create_players_buf = (v as i64 + 1).clamp(2, STEAM_MAX_PLAYERS as i64).to_string();
                } else if just('-') {
                    let v = parse_num(&self.steam_create_players_buf, STEAM_DEFAULT_PLAYERS as u32);
                    self.steam_create_players_buf = (v as i64 - 1).max(2).to_string();
                } else if just_named(NamedKey::Backspace) {
                    self.steam_create_players_buf.pop();
                } else {
                    for d in '0'..='9' {
                        if just(d) && self.steam_create_players_buf.len() < 3 {
                            self.steam_create_players_buf.push(d);
                        }
                    }
                }
            }
            3 => {
                // 轮数字段：数字输入 + Backspace 逐位删 + +/- 步进（1..=STEAM_MAX_ROUNDS）。
                if just('+') {
                    let v = parse_num(&self.steam_create_rounds_buf, STEAM_DEFAULT_ROUNDS);
                    self.steam_create_rounds_buf = (v as i64 + 1).clamp(1, STEAM_MAX_ROUNDS as i64).to_string();
                } else if just('-') {
                    let v = parse_num(&self.steam_create_rounds_buf, STEAM_DEFAULT_ROUNDS);
                    self.steam_create_rounds_buf = (v as i64 - 1).max(1).to_string();
                } else if just_named(NamedKey::Backspace) {
                    self.steam_create_rounds_buf.pop();
                } else {
                    for d in '0'..='9' {
                        if just(d) && self.steam_create_rounds_buf.len() < 4 {
                            self.steam_create_rounds_buf.push(d);
                        }
                    }
                }
            }
            _ => {}
        }
        // 回车=创建房间（从编辑缓冲解析出最终值；空缓冲回退默认）。
        if just_named(NamedKey::Enter) || just('\r') {
            let players = parse_num(&self.steam_create_players_buf, STEAM_DEFAULT_PLAYERS as u32)
                .clamp(2, STEAM_MAX_PLAYERS as u32) as u8;
            let rounds = parse_num(&self.steam_create_rounds_buf, STEAM_DEFAULT_ROUNDS).clamp(1, STEAM_MAX_ROUNDS);
            self.steam_create_players = players;
            self.steam_create_rounds = rounds;
            let name = self.steam_create_name.clone();
            let note = self.steam_create_note.clone();
            eprintln!("[steam] create lobby: players={players} rounds={rounds} name='{name}' note='{note}'");
            self.steam_lobby_create = false;
            self.steam_lobby_menu = true;
            self.steam_list_requested = false;
            self.steam_list_lobbies = Vec::new();
            self.steam_list_selection = 0;
            self.enter_steam_mode(ctx, true, players, Some(&name), Some(&note));
        }
    }

    /// 房间列表界面输入：首次进入拉一次公开大厅列表，供浏览选房加入。
    /// - ↑/↓ 选择；回车=加入选中的大厅；R=重新刷新；Q=返回大厅主界面。
    #[cfg(feature = "steam")]
    fn steam_lobby_list_update(&mut self, ctx: &mut Context) {
        use ggez::input::keyboard::Key;
        use winit::keyboard::NamedKey;
        let just = |k: char| ctx.keyboard.is_logical_key_just_pressed(&Key::Character(k.to_string().into()));
        let just_named = |n: NamedKey| ctx.keyboard.is_logical_key_just_pressed(&Key::Named(n));
        // 刷新节流：首次进入刷新一次；按 R 刷新需距上次 ≥ LOBBY_REFRESH_COOLDOWN_SECS（Steam 搜索接口限速，避免列表漂忽 1->0->1）。
        let first = !self.steam_list_requested;
        let now = ctx.time.time_since_start().as_secs_f64();
        let want_refresh = first || just('r') || just('R');
        if want_refresh && (first || now - self.steam_list_last_refresh >= LOBBY_REFRESH_COOLDOWN_SECS) {
            self.steam_list_requested = true;
            self.steam_list_last_refresh = now;
            if let Some(sess) = self.steam_sess.as_ref() {
                match sess.client_list_lobbies(120) {
                    Ok(mut list) => {
                        // 人数已满的大厅仍显示但不可选（steamworks 加入会失败）；这里仅排序展示。
                        list.sort_by_key(|l| (l.members >= l.limit, l.members));
                        self.steam_list_lobbies = list;
                        if self.steam_list_selection >= self.steam_list_lobbies.len() {
                            self.steam_list_selection = self.steam_list_lobbies.len().saturating_sub(1);
                        }
                        eprintln!("[steam-list] {} lobbies found", self.steam_list_lobbies.len());
                    }
                    Err(e) => {
                        eprintln!("[steam-list] list failed: {e:?}");
                        self.steam_list_lobbies = Vec::new();
                    }
                }
            }
        } else if want_refresh {
            eprintln!("[steam-list] 刷新太快（Steam 搜索限速），请几秒后再按 R");
        }
        if just('q') || just('Q') {
            self.steam_lobby_list = false;
            return;
        }
        if self.steam_list_lobbies.is_empty() {
            return; // 没有可加入房间（或加载中），等待/提示。
        }
        let n = self.steam_list_lobbies.len();
        if just_named(NamedKey::ArrowDown) {
            self.steam_list_selection = (self.steam_list_selection + 1) % n;
        } else if just_named(NamedKey::ArrowUp) {
            self.steam_list_selection = (self.steam_list_selection + n - 1) % n;
        }
        if just_named(NamedKey::Enter) || just('\r') {
            let sel = self.steam_list_selection;
            if sel < self.steam_list_lobbies.len() && self.steam_list_lobbies[sel].members < self.steam_list_lobbies[sel].limit {
                let lobby_id = self.steam_list_lobbies[sel].id;
                eprintln!("[steam] join lobby by id {lobby_id}");
                self.steam_lobby_list = false;
                self.steam_lobby_menu = true;
                self.steam_join_lobby_id = Some(lobby_id); // enter_steam_mode client 分支优先按其加入
                self.enter_steam_mode(ctx, false, 2, None, None);
            } else {
                eprintln!("[steam-list] 选中的房间已满或无效");
            }
        }
    }

    /// 从主菜单进入 Steam 大厅模式：重建真实 Steam 会话（建厅/加入）+ lockstep + 世界/战绩，
    /// 然后停在「房间/就绪界面」（`steam_in_lobby=true`）。`is_host`=创建大厅，否则加入；
    /// `players` 仅 host 用（请求的玩家总数，含 host）；`room_name`/`room_note` 仅 host 建厅时写进房间元数据（可空）。
    #[cfg(feature = "steam")]
    fn enter_steam_mode(&mut self, _ctx: &mut Context, is_host: bool, players: u8, room_name: Option<&str>, room_note: Option<&str>) {
        let seed = 20260812u64;
        let res = (|| -> std::io::Result<()> {
            // 复用进入大厅主界面时初始化的一次性 Steam 会话（避免重复 init 单实例 steamworks）。
            let mut sess = self
                .steam_sess
                .take()
                .ok_or_else(|| std::io::Error::other("steam 会话未初始化（未进入 Steam 大厅？）"))?;
            if is_host {
                let lobby = sess.host_create_lobby(players.max(1) as u32, 200)?;
                self.steam_lobby_id = Some(lobby.raw());
                sess.host_set_room_info(room_name, room_note)?;
                sess.host_set_rounds(self.steam_create_rounds)?;
                self.match_rounds = self.steam_create_rounds;
                sess.prepare_transport()?;
                self.steam_my_index = sess.my_slot();
                self.steam_my_id = sess.transport.steam_id();
                eprintln!("[steam-host] lobby={:?}, my slot={}", lobby.raw(), sess.my_slot());
                // 房间成员名单（昵称）。
                let fr = sess.transport.friends();
                let mut roster = Vec::new();
                for (slot, id) in sess.identities() {
                    let name = fr.get_friend(net_steam::steamworks::SteamId::from_raw(id)).name();
                    roster.push((slot, name, id));
                }
                self.steam_roster = roster;
                // 建 HostLockstep<SteamTransport>：总玩家数 = 请求的 players。
                let n = players.max(1) as usize;
                // 传给 set_client_identities 的身份必须是 client（不含 host 槽 0）：sess.identities() 含 host，需跳过。
                let ids: Vec<Option<u64>> = sess.identities().iter().skip(1).map(|(_, v)| Some(*v)).collect();
                let transport = sess.into_transport();
                let mut host_ls = net::lockstep::HostLockstep::new(transport, n, true);
                host_ls.set_client_identities(&ids);
                self.steam_host_ls = Some(host_ls);
                self.steam_cli_ls = None;
                self.app = AppState::SteamHost { players };
                // 世界/战绩：玩家数 = 请求数。
                self.world = game_core::world::World::new(n.max(1) as u32, seed);
                let cfg = game_core::meta::MatchConfig { total_rounds: self.match_rounds, ..Default::default() };
                self.meta = game_core::meta::MatchState::new(
                    cfg,
                    &(0..n.max(1)).map(|i| i as u32).collect::<Vec<u32>>(),
                    8,
                );
            } else {
                // 从房间列表选中了具体大厅 → 按其加入；否则沿用 matchkey 自动搜索加入。
                let lobby = match self.steam_join_lobby_id.take() {
                    Some(id) => sess.join_lobby_by_id(id, 240)?,
                    None => sess.client_find_and_join(240)?,
                };
                self.steam_lobby_id = Some(lobby.raw());
                sess.prepare_transport()?;
                eprintln!("[steam-join] lobby={:?}, my slot={}", lobby.raw(), sess.my_slot());
                self.steam_my_id = sess.transport.steam_id();
                let total = sess.table.as_ref().map(|t| t.total_players()).unwrap_or(2);
                let host_rounds = sess.lobby_rounds().unwrap_or(STEAM_DEFAULT_ROUNDS);
                self.match_rounds = host_rounds;
                let host_id = sess.host_steam_id().unwrap_or(0);
                let my_slot = sess.my_slot();
                self.steam_my_index = my_slot;
                let fr = sess.transport.friends();
                let mut roster = Vec::new();
                for (slot, id) in sess.identities() {
                    let name = fr.get_friend(net_steam::steamworks::SteamId::from_raw(id)).name();
                    roster.push((slot, name, id));
                }
                self.steam_roster = roster;
                let transport = sess.into_transport();
                self.steam_cli_ls = Some(net::lockstep::ClientLockstep::new(
                    transport,
                    my_slot,
                    net::transport::Peer::Steam { id: host_id, conn: None },
                ));
                self.steam_host_ls = None;
                self.app = AppState::SteamJoin { lobby_id: None };
                let n = total.max(2);
                self.world = game_core::world::World::new(n.max(1) as u32, seed);
                let cfg = game_core::meta::MatchConfig { total_rounds: self.match_rounds, ..Default::default() };
                self.meta = game_core::meta::MatchState::new(
                    cfg,
                    &(0..n.max(1)).map(|i| i as u32).collect::<Vec<u32>>(),
                    8,
                );
            }
            Ok(())
        })();
        if let Err(e) = res {
            eprintln!("[steam-menu] failed to enter steam mode: {e:?}");
            // 失败则退回主菜单（保留沙盒世界）。
            self.steam_lobby_menu = false;
            self.steam_in_lobby = false;
            return;
        }
        // 进入房间/就绪界面（无需再手动输入房间号）。
        self.steam_lobby_menu = false;
        self.steam_in_lobby = true;
        self.steam_local_ready = false;
        self.steam_build_done = false;
        self.steam_was_all_ready = false;
        self.steam_roster_ready = Vec::new();
        self.steam_all_ready = false;
        self.steam_countdown = 0.0;
        self.pre_game_config = false; // 由房间就绪 → StartConfig → true
        self.net_cfg = NetCfgSync::Idle;
        self.net_link = None;
        self.net_host = None;
        self.net_host_ls = None;
        self.net_ready = false;
        self.conn_dropped = false;
        self.reconnect_attempting = false;
        self.host_frame_count = 0;
        self.steam_cli_stale_ticks = 0;
        self.steam_participants = Vec::new();
        self.steam_online = Vec::new();
        self.steam_migrating = false;
        self.steam_migrate_ticks = 0;
        self.steam_new_host_id = 0;
        self.steam_host_broadcasting_takeover = false;
        // 邀请面板/好友列表状态复位；进房后立刻上报一次 Rich Presence（好友可一键加入）。
        self.steam_friend_list = false;
        self.steam_friends = Vec::new();
        self.steam_friend_selection = 0;
        self.steam_friend_hint = String::new();
        self.steam_presence_text = String::new(); // 强制立即写（不节流）
        self.steam_presence_last = -999.0;
        // 会话已被消费（成功）/ 已丢弃（失败），下次回主菜单允许再初始化一次。
        self.steam_session_tried = false;
        // 新一场：允许重新上报战绩（enter_steam_mode 是进新房间的入口）。
        self.steam_stats_recorded = false;
        self.steam_stats_snapshot = None;
        self.steam_toast = (String::new(), 0.0);
        self.accumulator = 0.0;
    }

    /// Steam 配置阶段统一入口：每帧
    /// - 心跳：pump 回调 + 双向收发，防止 P2P 空闲被拆；
    /// - 配置输入：本端未配完时才允许选技能/买成长，按空格/o 确认配完（build_done）；
    /// - 开战判定（修 1，局域网式统一开始）：
    ///   - host：本端配完 && 所有 client 配完 → 产帧开战；
    ///   - client：本端配完 && 收到 host 首帧（pump_frames 感知，不推进 expect_seq）→ 进入对局。
    ///
    /// 返回非必要：本函数自行把 `pre_game_config` 置 false 切换对局；上层据此提前 return。
    #[cfg(feature = "steam")]
    fn steam_config_update(&mut self, ctx: &Context) -> GameResult {
        use ggez::input::keyboard::Key;
        // 先取心跳字节（借用 self.meta/self.world 前），避免与 cli/host 的 &mut 借用冲突。
        let k = game_core::netcode::encode_player_input(&self.local_player_input());

        // 配置输入：本端未配完才允许；配完即锁定（不再响应选技能/成长）。
        // 确认配好用 [P]（配置界面 p 空闲、语义明确；空格在本环境实测不可靠，且不再用 o）。
        if !self.steam_build_done {
            self.poll_learning(ctx);
            self.poll_growth_buy(ctx);
            let confirm = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("p".into()))
                || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("P".into()));
            if confirm {
                self.steam_build_done = true;
                eprintln!("[steam] build done -> waiting for all players configured, then start match");
            }
        }

        let mut enter_sync: Option<NetCfgSync> = None;
        if let Some(mut cli) = std::mem::take(&mut self.steam_cli_ls) {
            // client：上行（在场/就绪/配好）心跳 + pump host 包（驱动回调/保活/缓存 host 帧）。
            let _ = cli.send_room_state(self.steam_local_ready, self.steam_build_done, &k);
            let mut krcv = vec![0u8; 4096];
            if self.steam_build_done {
                // 本端已配完：进入 ClientWait，上报我的 PlayerCfg 并以 host 广播的 PlayerCfgAll 统一开战。
                eprintln!("[steam-client] build done -> config sync (ClientWait)");
                enter_sync = Some(NetCfgSync::ClientWait);
            } else {
                // 未配完：pump 保活（不推进 expect_seq，避免分叉）。
                let _ = cli.pump_frames(&mut krcv).unwrap_or(false);
            }
            self.steam_cli_ls = Some(cli);
        } else if let Some(mut host) = std::mem::take(&mut self.steam_host_ls) {
            // host：收各端心跳 + 广播就绪快照当心跳，双向保活。
            let mut krcv = vec![0u8; 4096];
            host.poll(&mut krcv);
            host.broadcast_roster_ready(self.steam_local_ready);
            if self.steam_build_done && host.all_clients_build_done() {
                // 本端 + 所有 client 都配完：进入 HostGather，收齐配置并广播 PlayerCfgAll 后统一开战。
                eprintln!("[steam-host] all players configured -> config sync (HostGather)");
                enter_sync = Some(NetCfgSync::HostGather);
            } else if self.steam_build_done {
                // 诊断（节流）：我配完了但还没收齐 client，打印“等谁”的配好/在场计数，便于真机定位。
                self.steam_lobby_wait_ticks = self.steam_lobby_wait_ticks.wrapping_add(1);
                if self.steam_lobby_wait_ticks % 120 == 1 {
                    let done = host.build_done_clients_count();
                    let pres = host.present_clients_count();
                    let exp = host.expected_clients();
                    eprintln!(
                        "[steam-host] config waiting: my_build_done={} clients_build_done={done}/{exp} present={pres}/{exp}",
                        self.steam_build_done
                    );
                }
            }
            self.steam_host_ls = Some(host);
        }

        if let Some(sync) = enter_sync {
            self.steam_enter_config_sync(sync);
        }
        // Rich Presence：配置阶段 → “正在配置技能”（仍带 connect，好友仍可加入房间）。
        self.steam_refresh_presence(ctx.time.time_since_start().as_secs_f64());
        Ok(())
    }

    /// Steam 进入配置同步阶段（对齐局域网 HostGather/ClientWait）：把 phase 切到 Fighting、设 net_cfg，
    /// 让下一帧走 Fighting 分支的 cfg 同步（收齐 PlayerCfg、广播、两端 apply 后自然统一开战）。
    /// 这样既同步了各端技能配置（两端 world 逐位一致），又同步了对局开始。
    #[cfg(feature = "steam")]
    fn steam_enter_config_sync(&mut self, sync: NetCfgSync) {
        if self.meta.phase != MatchPhase::Fighting {
            self.meta.enter_first_round();
        }
        self.net_cfg = sync;
        // 保持 pre_game_config=true；配置同步完成后（Fighting 分支里）再置 false。
    }

    fn finish_pre_game(&mut self) {
        self.meta.enter_first_round(); // Fighting，round 保持 1
        #[cfg(feature = "steam")]
        if self.steam_host_ls.is_some() || self.steam_cli_ls.is_some() {
            // Steam：对局开始由 `steam_config_update` 的「所有端配完统一开始」驱动（修 1），
            // 这里仅作兜底（异常路径）：进入配置同步，下一帧由 Fighting 分支的 cfg 同步完成开战。
            let sync = if self.steam_host_ls.is_some() { NetCfgSync::HostGather } else { NetCfgSync::ClientWait };
            eprintln!("[steam] pre-game finish fallback -> entering config sync");
            self.steam_enter_config_sync(sync);
            return;
        }
        if self.net_link.is_some() {
            eprintln!("[meta] pre-game done -> ClientWait config sync");
            self.net_cfg = NetCfgSync::ClientWait;
        } else if self.net_host_ls.is_some() {
            eprintln!("[meta] pre-game done -> HostGather config sync");
            self.net_cfg = NetCfgSync::HostGather;
        } else {
            self.teardown_round_end();
            self.pre_game_config = false;
            eprintln!("[meta] pre-game config done -> round {} Fighting", self.meta.round);
        }
    }

    /// 开局前配置面板：显示当前绑定/等级/金币，提示按 Space 开始第一轮。
    fn draw_pre_game(&self, ctx: &mut Context) -> GameResult {
        let mut canvas = graphics::Canvas::from_frame(ctx, graphics::Color::from_rgb(18, 20, 26));
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;
        draw_text(&mut canvas, ctx, "开局 - 配置技能", 46.0, graphics::Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.12 }, true)?;
        // Steam：配置阶段为「所有玩家配完统一开始」；否则为局域网/单机的按空格开始。
        #[cfg(feature = "steam")]
        if self.steam_cli_ls.is_some() || self.steam_host_ls.is_some() {
            if self.steam_build_done {
                draw_text(&mut canvas, ctx, "[v] 我已配好，等待所有玩家配完统一开始...", 22.0, graphics::Color::from_rgb(90, 220, 130), Point2 { x: cx, y: sh * 0.12 + 60.0 }, true)?;
            } else {
                draw_text(&mut canvas, ctx, "选择技能后按 P 确认配好", 22.0, graphics::Color::from_rgb(150, 200, 255), Point2 { x: cx, y: sh * 0.12 + 60.0 }, true)?;
            }
        } else {
            draw_text(&mut canvas, ctx, "按 Space/P 开始第一轮，Esc 返回主菜单", 22.0, graphics::Color::from_rgb(150, 200, 255), Point2 { x: cx, y: sh * 0.12 + 60.0 }, true)?;
        }
        #[cfg(not(feature = "steam"))]
        draw_text(&mut canvas, ctx, "按 Space/P 开始第一轮，Esc 返回主菜单", 22.0, graphics::Color::from_rgb(150, 200, 255), Point2 { x: cx, y: sh * 0.12 + 60.0 }, true)?;
        // 准备状态面板：显示各玩家已加入/已就绪，避免“以为卡住”。
        let me = self.self_index();
        if self.app != AppState::Solo {
            let mut r = sh * 0.12 + 96.0;
            draw_text(&mut canvas, ctx, "== 玩家准备状态 ==", 20.0, graphics::Color::from_rgb(200, 210, 220), Point2 { x: cx, y: r }, true)?;
            r += 28.0;
            if let Some(host) = self.net_host_ls.as_ref() {
                let total = self.world.players.len();
                for i in 0..total {
                    let (name, ready) = if i == 0 {
                        ("host(你)".to_string(), host.local_cfg_ready())
                    } else {
                        (format!("玩家{i}"), host.client_cfg_ready(i as u8))
                    };
                    let (txt, col) = if ready {
                        (format!("  {name}  [v] 已就绪"), Color::from_rgb(90, 220, 130))
                    } else if (i as u32) == me {
                        (format!("  {name}  [ ] 等你按空格"), Color::from_rgb(240, 200, 70))
                    } else {
                        (format!("  {name}  [ ] 等待上报"), Color::from_rgb(170, 175, 185))
                    };
                    draw_text(&mut canvas, ctx, &txt, 18.0, col, Point2 { x: cx, y: r }, true)?;
                    r += 26.0;
                }
            } else if let Some(hs) = self.net_host.as_ref() {
                draw_text(&mut canvas, ctx, &format!("  已加入 {}/{} 个玩家", hs.joined, hs.expected()), 18.0, Color::from_rgb(170, 175, 185), Point2 { x: cx, y: r }, true)?;
                draw_text(&mut canvas, ctx, "  等所有玩家加入后：每个窗口先点击再按空格就绪", 17.0, Color::from_rgb(140, 160, 180), Point2 { x: cx, y: r + 26.0 }, true)?;
            } else {
                // LAN client：显示自身是否已就绪。
                let ready = self.net_cfg == NetCfgSync::ClientWait;
                let (txt, col) = if ready {
                    ("  [v] 已就绪，等待 host 开始...".to_string(), Color::from_rgb(90, 220, 130))
                } else {
                    ("  [ ] 未就绪 - 请先点击本窗口，再按空格就绪".to_string(), Color::from_rgb(240, 200, 70))
                };
                draw_text(&mut canvas, ctx, &txt, 18.0, col, Point2 { x: cx, y: r }, true)?;
            }
            // Steam：显示各端配好（build_done）状态，等待全员配完统一开始。
            #[cfg(feature = "steam")]
            if let Some(host) = self.steam_host_ls.as_ref() {
                let total = self.world.players.len();
                for i in 0..total {
                    let (name, done) = if i == 0 {
                        ("host(你)".to_string(), self.steam_build_done)
                    } else {
                        (format!("玩家{i}"), host.client_build_done(i as u8))
                    };
                    let (txt, col) = if done {
                        (format!("  {name}  [v] 已配好"), Color::from_rgb(90, 220, 130))
                    } else if (i as u32) == self.self_index() {
                        (format!("  {name}  [ ] 选技能后按 P"), Color::from_rgb(240, 200, 70))
                    } else {
                        (format!("  {name}  [ ] 配好中"), Color::from_rgb(170, 175, 185))
                    };
                    draw_text(&mut canvas, ctx, &txt, 18.0, col, Point2 { x: cx, y: r }, true)?;
                    r += 26.0;
                }
            } else if self.steam_cli_ls.is_some() {
                let (txt, col) = if self.steam_build_done {
                    ("  [v] 我已配好，等待全员配完统一开始...".to_string(), Color::from_rgb(90, 220, 130))
                } else {
                    ("  [ ] 选技能后按 P 确认配好".to_string(), Color::from_rgb(240, 200, 70))
                };
                draw_text(&mut canvas, ctx, &txt, 18.0, col, Point2 { x: cx, y: r }, true)?;
            }
        }
        if self.app == AppState::Solo {
            draw_text(&mut canvas, ctx, &format!("（单机：{:.0} 秒后自动用默认配置开始）", self.pre_game_timer.max(0.0)), 17.0, graphics::Color::from_rgb(140, 160, 180), Point2 { x: cx, y: sh * 0.12 + 92.0 }, true)?;
        }
        // 下方分左右两栏：左=技能树与键位绑定，右=成长点与属性购买；底部一条操作提示。
        let lcx = sw * 0.30; // 左栏中心
        let rcx = sw * 0.72; // 右栏中心
        let col_top = sh * 0.22;
        // —— 右栏：成长点 / 属性购买面板。
        if let Some(pr) = self.meta.profiles.iter().find(|p| p.player_id == self.self_index()) {
            let mut gy = col_top;
            draw_text(&mut canvas, ctx, "== 成长 / 属性 ==", 20.0, Color::from_rgb(130, 220, 255), Point2 { x: rcx, y: gy }, true)?;
            gy += 34.0;
            draw_text(&mut canvas, ctx, &format!("成长点 {}    金币 {}", pr.growth_points, pr.gold), 22.0, Color::from_rgb(220, 230, 245), Point2 { x: rcx, y: gy }, true)?;
            gy += 34.0;
            let a = &pr.attributes;
            draw_text(&mut canvas, ctx, &format!("生命 +{}%", a.hp_bonus * 10), 19.0, Color::from_rgb(200, 210, 220), Point2 { x: rcx, y: gy }, true)?;
            gy += 28.0;
            draw_text(&mut canvas, ctx, &format!("移速 +{}%", a.speed_bonus * 5), 19.0, Color::from_rgb(200, 210, 220), Point2 { x: rcx, y: gy }, true)?;
            gy += 28.0;
            draw_text(&mut canvas, ctx, &format!("护甲 -{}%  法抗 -{}%", a.armor * 6, a.spell_resist * 6), 18.0, Color::from_rgb(200, 210, 220), Point2 { x: rcx, y: gy }, true)?;
            gy += 28.0;
            draw_text(&mut canvas, ctx, &format!("击退 -{}%  法力 +{}  回蓝 +{}/s", a.kb_resist * 12, a.mana_max * 25, a.mana_regen), 18.0, Color::from_rgb(200, 210, 220), Point2 { x: rcx, y: gy }, true)?;
            gy += 34.0;
            draw_text(&mut canvas, ctx, "购买：Z 金币换点", 17.0, Color::from_rgb(160, 180, 200), Point2 { x: rcx, y: gy }, true)?;
            gy += 26.0;
            draw_text(&mut canvas, ctx, "H生命 J移速 K护甲", 17.0, Color::from_rgb(160, 180, 200), Point2 { x: rcx, y: gy }, true)?;
            gy += 26.0;
            draw_text(&mut canvas, ctx, "L法抗 ;击退 U蓝上 I回蓝", 17.0, Color::from_rgb(160, 180, 200), Point2 { x: rcx, y: gy }, true)?;
        }
        // —— 左栏：技能树与键位绑定。
        let me = self.self_index();
        if let Some(pr) = self.meta.profiles.iter().find(|p| p.player_id == me) {
            let mut y = col_top;
            draw_text(&mut canvas, ctx, "== 技能 配置 ==", 20.0, Color::from_rgb(255, 210, 120), Point2 { x: lcx, y }, true)?;
            y += 34.0;
            let gold_line = format!("金币：{}    击杀：{}    最佳名次：#{}", pr.gold, pr.total_kills, pr.best_placement);
            draw_text(&mut canvas, ctx, &gold_line, 20.0, Color::from_rgb(220, 224, 232), Point2 { x: lcx, y }, true)?;
            y += 34.0;
            // 当前选中树：高亮字样，提醒按了字母 C/R/E... 已选中哪棵/可选技能。
            if let Some(sel) = self.learn_tree_key {
                let sel_line = format!("[{}] {} 树（当前选中）", sel.letter(), sel.tree().name_zh());
                draw_text(&mut canvas, ctx, &sel_line, 22.0, Color::from_rgb(255, 210, 120), Point2 { x: lcx, y }, true)?;
                y += 34.0;
                for (i, skill) in sel.tree().skills_in_tree().iter().enumerate() {
                    let star = if pr.bound_skill(sel) == Some(*skill) { "  [已选]" } else { "" };
                    draw_text(&mut canvas, ctx, &format!("  {} {} {}", i + 1, game_core::skill::DefTable::def(*skill).name, star), 19.0, Color::from_rgb(215, 220, 230), Point2 { x: lcx, y }, true)?;
                    y += 28.0;
                }
                y += 10.0;
            } else {
                draw_text(&mut canvas, ctx, "（按字母 C/R/E/D/Y/T/F/G 选树）", 18.0, Color::from_rgb(170, 175, 185), Point2 { x: lcx, y }, true)?;
                y += 32.0;
            }
            draw_text(&mut canvas, ctx, "各键当前绑定：", 19.0, Color::from_rgb(225, 228, 235), Point2 { x: lcx, y }, true)?;
            y += 30.0;
            for key in game_core::skill::CastKey::ALL {
                let bound = pr.bound_skill(key);
                let lv = bound.map(|s| pr.skill_level(s)).unwrap_or(0);
                let txt = match bound {
                    Some(s) => format!("[{}] {}  @Lv{}", key.letter(), game_core::skill::DefTable::def(s).name, lv),
                    None => format!("[{}] （未绑定）", key.letter()),
                };
                let highlight = self.learn_tree_key == Some(key);
                draw_text(&mut canvas, ctx, &txt, 20.0, if highlight { Color::from_rgb(255, 210, 120) } else { Color::from_rgb(225, 228, 235) }, Point2 { x: lcx, y }, true)?;
                y += 28.0;
            }
        }
        draw_text(&mut canvas, ctx, "字母C/R/E/D/Y/T/F/G选树  数字绑技能  =升级  X洗点", 18.0, graphics::Color::from_rgb(160, 170, 185), Point2 { x: cx, y: sh * 0.92 }, true)?;
        canvas.finish(ctx)?;
        Ok(())
    }


    /// 主菜单：标题 + 三个入口（单机试验场 / 局域网 / Steam 大厅）；按 3 进入 Steam 大厅选择子菜单。
    fn draw_menu(&self, ctx: &mut Context) -> GameResult {
        let mut canvas = graphics::Canvas::from_frame(ctx, graphics::Color::from_rgb(18, 20, 26));
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;

        // 标题区
        let title = "帧同步圆球竞技场";
        draw_text(&mut canvas, ctx, title, 54.0, graphics::Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.14 }, true)?;
        draw_text(&mut canvas, ctx, "—— 选择对战模式 ——", 22.0, graphics::Color::from_rgb(200, 205, 215), Point2 { x: cx, y: sh * 0.14 + 64.0 }, true)?;

        // 卡片通用尺寸
        let card_w = (sw * 0.62).min(560.0);
        let card_h = 96.0;
        let card_x = cx - card_w / 2.0;
        let y0 = sh * 0.34;
        let gap = 26.0;

        #[cfg(feature = "steam")]
        let in_lobby_menu = self.steam_lobby_menu || self.steam_lobby_create || self.steam_lobby_list;
        #[cfg(not(feature = "steam"))]
        let in_lobby_menu = false;

        // Steam 大厅：建房设置 / 房间列表 / 大厅主界面三种子界面。
        if in_lobby_menu {
            // 建房设置界面。
            #[cfg(feature = "steam")]
            if self.steam_lobby_create {
                self.draw_steam_create_lobby(&mut canvas, ctx)?;
                canvas.finish(ctx)?;
                return Ok(());
            }
            // 房间列表界面。
            #[cfg(feature = "steam")]
            if self.steam_lobby_list {
                self.draw_steam_lobby_list(&mut canvas, ctx)?;
                canvas.finish(ctx)?;
                return Ok(());
            }
            // 大厅主界面：创建 / 加入 / 返回，同样卡片样式。
            #[cfg(feature = "steam")]
            {
                let subs: [(&str, &str); 3] = [
                    ("创建房间", "选房间名与玩家人数，然后进入房间"),
                    ("加入房间", "从房间列表选择并加入（S2 接入房间列表）"),
                    ("返回主菜单", "回到主菜单选择"),
                ];
                draw_text(&mut canvas, ctx, "Steam 对战 - 大厅", 34.0, graphics::Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.27 }, true)?;
                draw_text(&mut canvas, ctx, "H 创建    J 加入    Q 返回", 20.0, graphics::Color::from_rgb(200, 205, 215), Point2 { x: cx, y: sh * 0.36 }, true)?;
                let mpos = ctx.mouse.position();
                for (i, (name, desc)) in subs.iter().enumerate() {
                    let y = y0 + (i as f32) * (card_h + gap);
                    // 高亮：键盘选中最亮；鼠标悬停中亮；其他深灰。
                    let selected = i == self.steam_lobby_selection;
                    let hover = !selected && graphics::Rect::new(card_x, y, card_w, card_h).contains(mpos);
                    let bg_color = if selected {
                        Color::from_rgb(52, 60, 74)
                    } else if hover {
                        Color::from_rgb(40, 46, 58)
                    } else {
                        Color::from_rgb(30, 34, 44)
                    };
                    // 卡片背景
                    let bg = Mesh::new_rectangle(
                        &ctx.gfx, DrawMode::fill(),
                        graphics::Rect::new(card_x, y, card_w, card_h),
                        bg_color,
                    )?;
                    canvas.draw(&bg, graphics::DrawParam::new());
                    draw_text(&mut canvas, ctx, name, 30.0, graphics::Color::from_rgb(235, 238, 245), Point2 { x: cx, y: y + card_h * 0.5 - 16.0 }, true)?;
                    draw_text(&mut canvas, ctx, desc, 17.0, graphics::Color::from_rgb(150, 155, 168), Point2 { x: cx, y: y + card_h * 0.5 + 18.0 }, true)?;
                }
            }
            #[cfg(not(feature = "steam"))]
            {
                draw_text(&mut canvas, ctx, "Steam 未启用", 34.0, graphics::Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.36 }, true)?;
                draw_text(&mut canvas, ctx, "需要 --features client/steam 构建", 20.0, Color::from_rgb(200, 205, 215), Point2 { x: cx, y: sh * 0.44 }, true)?;
            }
            draw_text(&mut canvas, ctx, "H 创建    J 加入    Q 返回", 18.0, graphics::Color::from_rgb(160, 168, 182), Point2 { x: cx, y: sh * 0.90 }, true)?;
            canvas.finish(ctx)?;
            return Ok(());
        }

        // 主菜单三个入口卡片
        let items: [(u8, &str, &str); 3] = [
            (1, "单机技能试验场", "无 AI 自由试技能与数值（进入后配置技能开始）"),
            (2, "局域网对战", "建设中：需命令行 --host <port> / --join <host:port>"),
            (3, "Steam 在线对战", "联网与好友实时对抗（进入 Steam 大厅）"),
        ];
        let mpos = ctx.mouse.position();
        for (i, (num, name, desc)) in items.iter().enumerate() {
            let y = y0 + (i as f32) * (card_h + gap);
            let selected = i == self.menu_selection;
            let hover = !selected && graphics::Rect::new(card_x, y, card_w, card_h).contains(mpos);
            // 选中卡片：高亮背景条；悬停：中亮；未选中：深灰背景。
            let bg_color = if selected {
                Color::from_rgb(52, 60, 74)
            } else if hover {
                Color::from_rgb(40, 46, 58)
            } else {
                Color::from_rgb(28, 31, 38)
            };
            let bg = Mesh::new_rectangle(
                &ctx.gfx, DrawMode::fill(),
                graphics::Rect::new(card_x, y, card_w, card_h),
                bg_color,
            )?;
            canvas.draw(&bg, graphics::DrawParam::new());
            let mark = if selected { "[v]" } else { "[ ]" };
            let name_col = if selected { Color::WHITE } else { Color::from_rgb(210, 214, 225) };
            draw_text(&mut canvas, ctx, &format!("[{num}]  {mark}{name}"), 30.0, name_col, Point2 { x: cx, y: y + card_h * 0.5 - 16.0 }, true)?;
            draw_text(&mut canvas, ctx, desc, 17.0, Color::from_rgb(150, 156, 172), Point2 { x: cx, y: y + card_h * 0.5 + 20.0 }, true)?;
        }

        // 底部操作提示条
        draw_text(&mut canvas, ctx, "↑/↓ 选择    回车 确认    或直接按数字键", 18.0, graphics::Color::from_rgb(160, 168, 182), Point2 { x: cx, y: sh * 0.92 }, true)?;
        canvas.finish(ctx)?;
        Ok(())
    }

    /// 绘制「建房设置」界面：房间名 / 备注 / 人数 三字段，当前聚焦字段高亮。
    #[cfg(feature = "steam")]
    fn draw_steam_create_lobby(&self, canvas: &mut Canvas, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;
        draw_text(canvas, ctx, "创建房间", 38.0, Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.14 }, true)?;
        draw_text(canvas, ctx, "Tab / ↑↓ 切换字段 · 回车 创建 · Q 返回", 20.0, Color::from_rgb(180, 190, 205), Point2 { x: cx, y: sh * 0.14 + 48.0 }, true)?;

        let labels = ["房间名", "备注", "玩家人数", "总轮数"];
        let hints = [
            "直接输入文字，Backspace 删除",
            "可留空；直接输入文字",
            "+/- 步进，或直接输数字（2 ~ 64）",
            "+/- 步进，或直接输数字（1 ~ 256）",
        ];
        let vals = [
            self.steam_create_name.clone(),
            self.steam_create_note.clone(),
            self.steam_create_players_buf.clone(),
            self.steam_create_rounds_buf.clone(),
        ];
        let box_w = 400.0;
        let box_h = 50.0;
        let label_w = 180.0;
        let total_left = cx - (label_w + box_w) / 2.0;
        let row_h = box_h + 48.0;
        let mut y = sh * 0.28;
        for i in 0..4 {
            let selected = i == self.steam_create_focus;
            // 输入框
            let bg_col = if selected { Color::from_rgb(56, 66, 84) } else { Color::from_rgb(28, 32, 42) };
            let bg = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), graphics::Rect::new(total_left + label_w, y, box_w, box_h), bg_col)?;
            canvas.draw(&bg, graphics::DrawParam::new());
            // 聚焦字段：金色高亮边框
            if selected {
                let border = Mesh::new_rectangle(&ctx.gfx, DrawMode::stroke(2.0), graphics::Rect::new(total_left + label_w, y, box_w, box_h), Color::from_rgb(255, 210, 120))?;
                canvas.draw(&border, graphics::DrawParam::new());
            }
            // 标签
            let label_col = if selected { Color::from_rgb(255, 210, 120) } else { Color::from_rgb(215, 220, 232) };
            draw_text(canvas, ctx, labels[i], 24.0, label_col, Point2 { x: total_left + label_w / 2.0, y: y + box_h / 2.0 - 15.0 }, true)?;
            // 值
            let disp = if i == 0 && vals[0].is_empty() {
                "（输入房间名）".to_string()
            } else if i == 1 && vals[1].is_empty() {
                "（可留空）".to_string()
            } else if i == 2 && vals[2].is_empty() {
                "（默认 2）".to_string()
            } else if i == 3 && vals[3].is_empty() {
                "（默认 3）".to_string()
            } else {
                vals[i].clone()
            };
            let val_col = if vals[i].is_empty() { Color::from_rgb(120, 130, 150) } else { Color::WHITE };
            draw_text(canvas, ctx, &disp, 22.0, val_col, Point2 { x: total_left + label_w + box_w / 2.0, y: y + box_h / 2.0 - 14.0 }, true)?;
            // 聚焦字段下方：该字段专属操作提示
            if selected {
                draw_text(canvas, ctx, &format!("▶ {}", hints[i]), 17.0, Color::from_rgb(150, 200, 255), Point2 { x: cx, y: y + box_h + 14.0 }, true)?;
            }
            y += row_h;
        }
        draw_text(canvas, ctx, "Tab / ↑↓ 切换字段 · 回车 创建房间 · Q 取消", 20.0, Color::from_rgb(160, 200, 255), Point2 { x: cx, y: sh * 0.90 }, true)?;
        Ok(())
    }

    /// 绘制「房间列表」界面：公开大厅列表（房主昵称/房名/人数/备注），当前选中高亮。
    #[cfg(feature = "steam")]
    fn draw_steam_lobby_list(&self, canvas: &mut Canvas, ctx: &Context) -> GameResult {
        let (sw, sh) = ctx.gfx.drawable_size();
        let cx = sw / 2.0;
        draw_text(canvas, ctx, "加入房间", 36.0, Color::from_rgb(255, 210, 120), Point2 { x: cx, y: sh * 0.22 }, true)?;
        draw_text(canvas, ctx, "↑/↓ 选择，回车加入，R 刷新", 20.0, Color::from_rgb(180, 190, 205), Point2 { x: cx, y: sh * 0.22 + 50.0 }, true)?;
        if self.steam_list_lobbies.is_empty() {
            draw_text(canvas, ctx, "（暂无可加入的房间）", 28.0, Color::from_rgb(170, 178, 194), Point2 { x: cx, y: sh * 0.5 }, true)?;
            draw_text(canvas, ctx, "让好友先创建房间，或按 R 重新搜索", 18.0, Color::from_rgb(150, 160, 178), Point2 { x: cx, y: sh * 0.5 + 48.0 }, true)?;
        } else {
            let mut y = sh * 0.34;
            let head_w = (sw * 0.8).min(760.0);
            let head_x = cx - head_w / 2.0;
            for (i, l) in self.steam_list_lobbies.iter().enumerate() {
                // 房主昵称（临时查 friends；取不到则“房主”）。
                let owner_name = self
                    .steam_sess
                    .as_ref()
                    .map(|s| {
                        let id = net_steam::steamworks::SteamId::from_raw(l.owner);
                        s.transport.friends().get_friend(id).name()
                    })
                    .unwrap_or_else(|| "房主".to_string());
                let full = format!("{}   {}", owner_name, l.name);
                let meta = format!("人数 {}/{}    {}", l.members, l.limit, l.note);
                let selected = i == self.steam_list_selection;
                let bg_col = if selected { Color::from_rgb(52, 60, 74) } else { Color::from_rgb(28, 31, 38) };
                let bg = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), graphics::Rect::new(head_x, y, head_w, 64.0), bg_col)?;
                canvas.draw(&bg, graphics::DrawParam::new());
                let mark = if selected { "[v]" } else { "[ ]" };
                let name_col = if selected { Color::WHITE } else { Color::from_rgb(210, 214, 225) };
                draw_text(canvas, ctx, &format!("{mark}{full}"), 24.0, name_col, Point2 { x: cx, y: y + 20.0 }, true)?;
                draw_text(canvas, ctx, &meta, 16.0, Color::from_rgb(150, 156, 172), Point2 { x: cx, y: y + 44.0 }, true)?;
                y += 74.0;
            }
        }
        draw_text(canvas, ctx, "回车 加入    R 刷新    Q 返回", 18.0, Color::from_rgb(160, 200, 255), Point2 { x: cx, y: sh * 0.90 }, true)?;
        Ok(())
    }
}


fn player_color(id: u32, me: u32) -> Color {
    if id == me {
        Color::from_rgb(90, 200, 160)
    } else {
        const PALETTE: [[u8; 3]; 10] = [
            [240, 120, 100],
            [100, 150, 240],
            [245, 200, 90],
            [180, 120, 220],
            [90, 210, 230],
            [240, 160, 60],
            [150, 220, 120],
            [230, 130, 200],
            [160, 180, 90],
            [200, 140, 110],
        ];
        let c = PALETTE[(id as usize) % PALETTE.len()];
        Color::from_rgb(c[0], c[1], c[2])
    }
}

fn hp_color(ratio: f32) -> Color {
    if ratio > 0.5 {
        Color::from_rgb(90, 220, 130)
    } else if ratio > 0.25 {
        Color::from_rgb(240, 200, 70)
    } else {
        Color::from_rgb(235, 80, 70)
    }
}

/// 在屏幕上居中绘制文本（用 ggez 内置默认字体）。
fn draw_text(
    canvas: &mut Canvas,
    ctx: &Context,
    text: &str,
    size: f32,
    color: Color,
    center: Point2<f32>,
    _centered: bool,
) -> GameResult {
    use ggez::graphics::{Text, TextFragment};
    use ggez::mint::Vector2;
    let fragment = TextFragment::new(text).color(color).scale(size).font("cjk".to_string());
    let mut t = Text::new(fragment);
    t.set_bounds(Vector2 { x: 2000.0, y: 200.0 });
    // 手动粗略居中：先量尺寸
    let sz = t.measure(&ctx.gfx)?;
    let dest = Point2 {
        x: center.x - sz.x / 2.0,
        y: center.y,
    };
    canvas.draw(&t, graphics::DrawParam::new().dest(dest));
    Ok(())
}

/// 本局运行模式已并入 `AppState`（主菜单 / Solo / 局域网主机 / 局域网加入）。
/// 解析命令行（可选直通入口）：`--join <host:port>` / `--host <port>` [--players N] / `--solo`；
/// 无匹配选项 → 返回主菜单。抽成纯函数便于回归测试（曾因 `--solo` 分支漏 `i += 1` 导致死循环）。
fn parse_app_from_args(args: &[String]) -> AppState {
    let mut app = AppState::MainMenu;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--solo" => {
                app = AppState::Solo;
                i += 1;
            }
            "--join" if i + 1 < args.len() => {
                if let Ok(a) = args[i + 1].parse() {
                    app = AppState::LanJoin { addr: a };
                }
                i += 2;
            }
            "--host" if i + 1 < args.len() => {
                let port: u16 = args[i + 1].parse().unwrap_or(0);
                app = AppState::LanHost { port, total: 4 };
                i += 2;
            }
            #[cfg(feature = "steam")]
            "--steam-host" => {
                app = AppState::SteamHost { players: 4 };
                i += 1;
            }
            #[cfg(feature = "steam")]
            "--steam-join" => {
                app = AppState::SteamJoin { lobby_id: None };
                i += 1;
                // 可选：后面跟一个 LobbyId（fallback 手动加入）。
                #[cfg(feature = "steam")]
                if i < args.len() && args[i].parse::<u64>().is_ok() {
                    if let AppState::SteamJoin { lobby_id } = &mut app {
                        *lobby_id = args[i].parse::<u64>().ok();
                    }
                    i += 1;
                }
            }
            // Steam 冷启动加入：好友接受邀请时，若本游戏未在运行，Steam 会用我们自己定义的
            // connect 串启动它（`+connect_lobby <id>`）——这里解析出来直接进那个大厅。
            #[cfg(feature = "steam")]
            "+connect_lobby" if i + 1 < args.len() => {
                if let Ok(id) = args[i + 1].parse::<u64>() {
                    app = AppState::SteamJoin { lobby_id: Some(id) };
                }
                i += 2;
            }
            "--players" if i + 1 < args.len() => {
                #[cfg(feature = "steam")]
                if let AppState::SteamHost { players } = &mut app {
                    *players = args[i + 1].parse().unwrap_or(4);
                }
                if let AppState::LanHost { total, .. } = &mut app {
                    *total = args[i + 1].parse().unwrap_or(4);
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    app
}

fn main() -> GameResult {
    // 解析命令行（可选直通入口）：--join <host:port> / --host <port> [--players N] / --solo。
    // 若无任一参数 → 进主菜单选择。
    let args: Vec<String> = std::env::args().collect();
    let app = parse_app_from_args(&args);
    eprintln!("[main] app = {:?} args = {:?}", app, args[1..].to_vec());
    eprintln!("[main] building ggez context (window)...");

    let (mut ctx, event_loop) = ggez::ContextBuilder::new("frame-sync-arena", "remake")
        .window_setup(ggez::conf::WindowSetup::default().title("帧同步圆球竞技场 — 阶段1"))
        .window_mode(
            ggez::conf::WindowMode::default()
                .dimensions(1280.0, 720.0)
                .resizable(true),
        )
        .build()?;

    let game = Game::new(&mut ctx, app)?;
    event::run(ctx, event_loop, game)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_parse_does_not_hang_and_selects_solo() {
        // 回归：`--solo` 曾因漏 `i += 1` 在命令行解析里死循环，导致单机无法启动。
        let s = |x: &[&str]| x.iter().map(|v| v.to_string()).collect::<Vec<_>>();
        assert_eq!(parse_app_from_args(&s(&["exe", "--solo"])), AppState::Solo);
        // 无参数 → 主菜单
        assert_eq!(parse_app_from_args(&s(&["exe"])), AppState::MainMenu);
        // host + players
        assert_eq!(
            parse_app_from_args(&s(&["exe", "--host", "9001", "--players", "6"])),
            AppState::LanHost { port: 9001, total: 6 }
        );
        // join
        assert_eq!(
            parse_app_from_args(&s(&["exe", "--join", "127.0.0.1:5199"])),
            AppState::LanJoin { addr: "127.0.0.1:5199".parse().unwrap() }
        );
    }

    /// 好友接受邀请、而本游戏没在运行时，Steam 会用我们的 connect 串冷启动游戏
    /// （`+connect_lobby <id>`）——解析后应直接进那个大厅（`SteamJoin { lobby_id: Some(id) }`）。
    #[cfg(feature = "steam")]
    #[test]
    fn steam_connect_lobby_arg_selects_join_with_lobby_id() {
        let s = |x: &[&str]| x.iter().map(|v| v.to_string()).collect::<Vec<_>>();
        assert_eq!(
            parse_app_from_args(&s(&["exe", "+connect_lobby", "109775241105637376"])),
            AppState::SteamJoin { lobby_id: Some(109775241105637376) }
        );
        // id 非法 → 不误进房间（留在主菜单）。
        assert_eq!(
            parse_app_from_args(&s(&["exe", "+connect_lobby", "not-a-number"])),
            AppState::MainMenu
        );
        // 缺参数 → 主菜单（不能死循环 / 不能误解析后面的参数）。
        assert_eq!(parse_app_from_args(&s(&["exe", "+connect_lobby"])), AppState::MainMenu);
    }
}
