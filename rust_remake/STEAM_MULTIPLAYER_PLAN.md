# Steam 联机规划（Steam 为中心，纯玩家 P2P）

> 创建 2026-08-25。核心决策记录 + 三阶段路线 + Steamworks 能力盘点。
> 阅读顺序：`NEXT_STEPS.md`（交接）→ 本文件（Steam 联机主线）→ `RECONNECT.md`（重连机制）→
> `ROADMAP.md` / `NET_REWRITE.md`（局域网/网络重写，作为备胎参考）。

---

## 0. 架构决策（已确认）

- **不租用任何专用服务器**。游戏完全依赖玩家的机器 + Steam 网络（Steam Relay / NAT 穿透）。
- **对局同步模型 = 帧同步 + host 权威**：由某台玩家机器充当 host/房主，其余端连它。
  这是唯一不需要服务器、且最配「朋友开房对战」主场景的形态。
- **Steam 是核心，局域网是备胎**：局域网只用于无法联网时的内网对战，保持现状、轻度维护，
  不投入 Steam 专属增强（快照广播/主机迁移等），除非某功能在传输层统一、代价可忽略才顺手带上。
- **Steam 版不与局域网"看齐"**：Steam 版应尽量用上 Steamworks 的强处（relay 跨网、大厅成员一致视图、
  SteamID 稳定身份、周边生态），做到局域网做不到的能力（如主机迁移）。

### Steamworks 强 / 不强（据此设计，不越界）
- **强**：跨公网 relay + NAT 穿透（局域网必须同一内网）；大厅（房间/房主/成员一致视图/元数据/锁房/列表）；
  SteamID 稳定身份与认证；周边生态（好友/成就/云存档/排行榜）。
- **不强**：不托管专用服务器（`GameServer` 只是让你把自己租的机器接入，非免费云托管）；
  不提供「对局权威/房主的完美迁移」（0.13 无 `TransferLobbyOwnership`）。"host 掉线"只能靠我们自己架构解决。

### 关键洞察：为什么 Steam 版能做主机迁移而局域网不能
- 局域网 host 掉线只能"全队回大厅"，因为没有中心视图、client 之间互不知晓，无法达成一致。
- **Steam 的大厅（Lobby）提供所有成员一致的成员列表视图**（`lobby_members`，Steam 权威），
  恰好解决"client 之间无法确定在线者集合"的选举一致性问题。
- 加上 host 把快照广播给所有 client → **任何端都持有最新快照、谁都能接任** → 主机迁移可行。

---

## 1. 主线目标：主机迁移（host 掉线自动接管，对局继续）

把「host 掉线」从当前的「全队回大厅（安全但中断）」升级为「自动选新房主接管、对局继续」。
分三阶段，按依赖顺序推进，每阶段加传输无关单测锁死、不破坏现有 119 全绿基线。

---

## 2. 三阶段路线

### 阶段 1：Steam 战斗端掉线处理 + 重连（地基）✅ 已落（2026-08-25）
- **host**：Steam Fighting 分支接入 `auto_drop_idle(HOST_DROP_TICKS)`——某 client 真掉线 → 自动用默认输入占位，
  其余端继续，不再 `waiting for client input` 空转。
- **client**：Steam Fighting 分支接入掉线检测（连续收不到权威帧）→ 显示重连 UI → 按 R 发 `ReconnectReq` →
  拉快照重建 `self.world` → `apply_resync` 对齐继续。
- **新增**：`steam_my_id`（本机 SteamID，重连身份）、`steam_cli_stale_ticks`（Steam client 掉线探测）；
  `poll_steam_reconnect` 重连入口；host 分支 `auto_drop_idle` 接入。
- **复用**：`HostLockstep`/`ClientLockstep` 传输无关，局域网已验证。
- **单测 +1**：`host_auto_drops_then_client_reconnects_resumes`（锁死「自动掉线 + 重连回接续打」整条链路）。
  workspace 120 全绿，build/test/clippy（默认+steam）全绿。
- **待真机复验**：Steam 对战中某 client 掉线 → host 日志 `[steam-host] AUTO-DROP client N`、其余端继续；
  掉线端按 R 拉快照接回、两端逐位一致。
- **为什么先做**：迁移时新 host 要给快照、其余端要拉快照对齐，这套能力就是重连——它是迁移的地基。

### 阶段 2：快照广播（让每个端都有"接任能力"）✅ 已落（2026-08-25）
- **net**：`HostLockstep::broadcast_snapshot`（本地保存 + 广播给所有 client）；`ClientLockstep` 加 `latest_snapshot` 缓存 + `take_latest_snapshot`，在 `step_frame`/`pump_frames` 收包循环里顺带缓存（不应用、不推进）。
- **host**：Steam 战斗分支周期 `set_snapshot` → `broadcast_snapshot`（每 `SNAPSHOT_EVERY` 帧）。
- **单测 +1**：`host_broadcasts_snapshot_client_caches_it`（锁死「广播→缓存→可取走」）。workspace 121 全绿。
- **结果**：任何端都持有最新世界快照，谁都能当新 host——为迁移铺路。
- **待真机复验**：对局中任意 client 持有的快照与 host 一致；观察带宽开销（每 0.5s 一份完整世界快照）是否可接受。

### 阶段 3：主机迁移（Steam 专属增强）✅ 已落 + 真机复验通过（2026-08-25）
> 真机：host 掉线 → 自动选新 host 接管；新 host 再掉线 → 继续选下一新 host 接管，**连续迁移正常**。
> 日志确认：`elected new host=...` → `TAKEOVER: player X/N` + `online=[...]`（只含在线者）逐级收敛。
分三步提交：① 重构 `HostLockstep` 支持 host 任意 index（`host_index`+`client_indices`+`slot_of`）；② net 层迁移 API
（`Packet::Takeover`/`Participants`、`HostLockstep::takeover`/`broadcast_participants`/`broadcast_takeover`、
`ClientLockstep` `into_transport`/`retarget_host`/`recv_takeover`/`recv_participants`）；③ client 迁移状态机。
- **参与者广播**：host 对局配置同步完成时 `broadcast_participants`（按 new index 的 SteamID）；client 收存 `steam_participants`。
- **检测/探测**：client 连续收不到权威帧（`CLIENT_STALE_TICKS`）→ 进入迁移；先发 `ReconnectReq` 探测 host 是否还在，
  收到 Snapshot 则（host 在）恢复；`MIGRATE_PROBE_TICKS` 超时无应答则判定 host 掉线。
- **选举**：排除旧 host + `steam_participants` 中 SteamID 最小者为新 host（确定性、全员一致）。
- **接管**：本端是新 host → `HostLockstep::takeover`（ClientLockstep 转 host，host_index=本端 index，原 host player0 掉线占位，
  从缓存快照续打）→ 广播 `Takeover`+`Snapshot`。
- **重定向**：其余端收 `Takeover` → `retarget_host`（新 host peer）→ 用其快照重建 world → `apply_resync` 对齐续打。
- **单测 +3**：`host_takeover_resumes_from_cached_snapshot_seq` / `host_takeover_nonzero_index_emits_correct_frames` /
  `client_retarget_host_after_migration`。workspace 124 全绿，build/test/clippy（默认+steam）全绿。
- **待真机复验**：Steam 双机对局中 host 直接退出/掉线 → 其余端自动选新 host 接管、对局不断、两端逐位一致。
- **已知边界**：只处理「原 host 掉线」（new index 0）；其他参与 client 同时掉线时，迁移后新 host 会等其输入而卡（后续可扩展为对掉线 client 也 auto_drop）；不满员对局的迁移未特别处理。

### 实施顺序
阶段 1 → 阶段 2 → 阶段 3。阶段 1 同时补掉当前已知缺陷（Steam 对战中 client 掉线 host 空转）。

---

## 3. Steamworks 能力盘点

> 实现时以 `steamworks 0.13` 的实际 API 路径为准，此处记录用途与优先级。

### 已在使用（当前代码已用）
| 能力 | 用途 | 说明 |
|---|---|---|
| `ISteamMatchmaking` 大厅 | 建房/加入/离开、`lobby_members`、`lobby_owner`、`set_lobby_data`/`lobby_data`、`set_lobby_joinable`、`request_lobby_list` | 房间与成员一致视图 |
| `ISteamFriends`（昵称） | `get_friend(id).name()` 显示成员昵称 | 大厅 UI |
| `ISteamFriends`（邀请） | `activate_invite_dialog(lobby)` 覆盖层邀请窗口；`Friend::invite_user_to_game(connect)` 定向邀请 | 好友邀请（第一批，2026-08-29） |
| `ISteamFriends`（Rich Presence） | `set_rich_presence("status"\|"connect")` / `clear_rich_presence()` | 好友看到状态 +「加入游戏」（第一批，2026-08-29） |
| 回调 `GameLobbyJoinRequested` / `GameRichPresenceJoinRequested` | 好友点「加入游戏」/接受邀请时本机收到 → 按 lobby id 自动进房 | 好友邀请（第一批，2026-08-29） |
| `ISteamNetworkingMessages::GetSessionConnectionInfo` | 到 peer 的实时 ping | Ping 显示（第二批，2026-08-29） |
| `ISteamFriends::GetSmallFriendAvatar` + `ISteamUtils::GetImageRGBA` | 玩家头像（32/64/184 RGBA） | 房间/好友列表头像（第二批） |
| `ISteamUserStats`（统计/成就/排行榜） | `set_stat_i32` / `achievement().set()` / `store_stats()` / `find_leaderboard` / `upload_leaderboard_score` | 战绩上报（第二批，需后台 key） |
| `ISteamNetworkingMessages` | `SendMessageToUser`/`ReceiveMessagesOnChannel` relay P2P 传输 | 对局传输 |
| `Client::init_app` + `SteamId` | 初始化 / 稳定身份 | 全局 |

### 建议在本游戏中使用（按用户 2026-08-25 确认的优先级排序）

**第一批（先准备，紧跟主机迁移主线之后）—— ✅ 已落（2026-08-29），待真机双账号复验**
| 能力 | 用途 | 价值 |
|---|---|---|
| **好友邀请（Invite）** | 房主从房间界面邀请 Steam 好友加入大厅/对局 | 高：「朋友对战」刚需，Steam 原生 |
| **Rich Presence + JoinGame** | 好友列表/聊天显示「正在玩 XX 游戏、在对局中」，好友可一键加入 | 高：提升组队便利 |

> 实现要点（踩坑记录，别重复踩）：
> - **0.13 没有 `InviteUserToLobby`**，定向邀请只能 `Friend::invite_user_to_game(connect 串)`；
>   `activate_invite_dialog(lobby)` 另给一个「勾多人」的覆盖层窗口（两条路都留了）。
> - **回调句柄必须持有**：`Client::register_callback` 返回的 `CallbackHandle` 一 drop 就注销，
>   所以 `SteamTransport` 里存了 `callback_handles`（见 `transport_steam.rs::register_callback`）。
> - **进房后 `SteamSession` 已被消费**（`into_transport()`，传输归 lockstep），所以邀请/presence 的 API
>   做成以 `&SteamTransport` + lobby id 为参数的**自由函数**（`net-steam/src/session.rs`）。
> - connect 串统一为 `+connect_lobby <lobby_id>`（`lobby.rs` 的 `format_/parse_connect_string`，有单测）；
>   游戏在跑 → 回调进房；没跑 → Steam 用它冷启动游戏（`parse_app_from_args` 解析，有单测）。

**第二批（然后考虑）—— ✅ 已落（2026-08-29），待真机复验（统计/成就/排行榜还需后台配置 key）**
| 能力 | 用途 | 价值 |
|---|---|---|
| **成就 + 统计 + 排行榜（UserStats）** | 击杀数/名次/胜场/成就解锁/排行榜 | 中：对局结算已有 placement/kills，长期目标感 |
| **玩家头像（Avatar）** | 房间/结算界面显示玩家头像（`GetLargeFriendAvatar` + 图像取 RGBA） | 中：比昵称更直观 |
| **Ping / 连接质量（NetworkingUtils）** | 房间/对局显示到 host 的延迟，帮助判断卡顿来源 | 中：帧同步对延迟敏感，值得显性展示 |

> 实现要点（踩坑记录，别重复踩）：
> - **Ping 不走 `NetworkingUtils`**（那儿只有中继网络状态），而是 `ISteamNetworkingMessages::GetSessionConnectionInfo`
>   返回的实时信息里的 `ping()`；测不到是 `None`，界面要显示“--”而不是 0。
> - **头像是固定尺寸**（32/64/184），steamworks 只给 RGBA 字节不给尺寸，边长按档位常量补。
> - **统计/成就/排行榜的 key 必须先建在 Steamworks 后台**，否则 `set_stat_i32` / `achievement().set()` 直接失败；
>   所以规则（该记什么）放在无 steam 也能编译的 `stats.rs` 并配单测，写入全部 best-effort。
> - Steam 的 find/download 是 **call-result 回调**（`FnOnce + 'static`，拿不到 transport），
>   结果只能写回调用方持有的 `Shared<T>` 槽位，靠 `run_callbacks` 推进后取用。

**最后做**
| 能力 | 用途 | 价值 |
|---|---|---|
| **云存档（Remote Storage）** | 把技能树绑定/成长/金币等 meta 进度存云端，换机器不丢 | 中：meta 已有 `PlayerProfile`/金币/成长，数据现成，但延后 |

**实现顺序（用户定）**：先好友邀请 + Rich Presence → 再成就/排行榜/头像/Ping → 最后云存档。

### 后期可选（记录，不在当前范围）
| 能力 | 用途 | 说明 |
|---|---|---|
| 语音聊天 | 内建语音 | 工程较大，后置评估 |
| Steam Input（手柄） | 手柄支持 | 若做手柄再上 |
| Steam UGC（创意工坊） | 自定义地图/技能 | 超出当前范围 |

### 已排除（不适用）
| 能力 | 原因 |
|---|---|
| `ISteamGameServer`（专用服务器/服务器浏览器） | 不租服务器，纯玩家 P2P |
| 商店 / 支付 / DRM / HTML 覆盖层 | 与本游戏无关 |

---

## 4. 待办（写文档后的下一步）
1. 真机复验「人不满启动角色数一致」+ S5 房间界面（上一轮遗留，需双机）。
2. ~~**开始三阶段主线：阶段 1 → 阶段 2 → 阶段 3**~~ ✅ 三阶段已完成并真机复验（2026-08-25）。
3. ~~Steamworks 第一批：好友邀请 + Rich Presence~~ ✅ 已落（2026-08-29），**待真机双账号复验**
   （复验清单见 `NEXT_STEPS.md`「Steamworks 第一批」节末）。
4. ~~Steamworks 第二批：成就 / 排行榜 / 头像 / Ping~~ ✅ 已落（2026-08-29），**待真机复验 + 后台配置 key**
   （复验清单见 `NEXT_STEPS.md`「Steamworks 第二批」节末）。
5. 复验通过后接最后一批：**云存档**（Remote Storage）。
