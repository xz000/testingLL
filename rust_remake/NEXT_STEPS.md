# 交接记录 / 下一步（离线续接用）

> 唯一的当前权威续接入口。**2026-08-15 晚**重写为最新快照。
> 阅读顺序：先看本文件 → 缺背景再翻 `RECONNECT.md`（重连）/ `ROADMAP.md`（三功能）/
> `LATENCY_MASKING.md`（手感）/ `ATTRIBUTE_SYSTEM.md`（4.6b）/ `NET_REWRITE.md`（网络重写）/
> `LOCKSTEP_FOUNDATION.md`（基座）/ `UI_MENUS.md`（界面）。

## 当前状态（全绿）
- **单测 109 全绿 = game-core 87 + net 15 + client 5 + net-steam 2**；`cargo build --workspace`、`cargo test --workspace`、`cargo clippy --workspace -- -D warnings` 均绿、工作区干净。
- 技术栈：`game-core`（定点确定性核心）+ `net`（proto/handshake/lockstep 三层）+ `client`（ggez）。定点 `fixed=1.28`、三角 `cordic`；`Balance` 数值收敛层已建。
- 测试计数几乎每次新增/重连切片都在涨（79/14/5）。**续接时以 `cargo test --workspace` 为准，别信本文件里的静态数字。**

## 关键历史（速览，回滚/定位用）
- **tag 丢失 bug**（曾导致网络静默吞输入 + 旧测试假绿）→ 已修 + 防假绿纪律写入 PLAN「测试约定」。真机 4 窗口验证通过。
- **网络层已按「正确 lockstep」完整重写**：proto + handshake + lockstep 三层，删旧 session.rs。帧带 seq、host 等齐、client 严格按序、丢帧自动补发。
- **4.3 Balance**：手感/场地数值权威源收敛（base_speed/accel/decel/radii/hurt 等）。
- **4.4 判定**：当前 lockstep 承载「本地/内网多端逐位一致」，不背"显示上掩盖延迟"；真延迟掩盖（本地预测/回滚）另记 `LATENCY_MASKING.md`。
- **4.6 多局配置同步**：局域网多局经济/升级/洗点确定性同步 ✓（host 收齐广播 PlayerCfgAll、各端 apply）。
- **M0 主菜单 + M1 单机试验场**：`AppState`（MainMenu/Solo/LanHost/LanJoin）；Solo 无 AI + 不动靶子 + sandbox（不缩圈/不判结束）+ 开局配置；Lan 开局也先配置（round1 配置同步）。命令行 `--solo/--host/--join` 直通。

## 重连（RECONNECT.md 方案 A）—— 垂直切片已全部完成 ✅
| 切片 | 内容 | 提交 |
|------|------|------|
| 1 | host 掉线排除（mark_dropped 默认输入占位，不卡全队） | `3274e2e` |
| 2 | unmark_dropped + 快照重建 World + set_start_seq 接回 | `1ae894b` |
| 3a | proto：ReconnectReq / Snapshot / Resync | `c04f226` |
| 3b | game-core World 全量确定性序列化（world_to_bytes/from_bytes） | `5bbd333` |
| 3c | 重连测试改走 Packet::Snapshot 真字节链路 | `31e28e3` |

→ 重连主线（掉线不卡全队 + 真字节快照重连接回 + 两端逐位一致）已闭环、有测试锁死。

## 重连接入 client 真实运行时 ✅（本次会话完成，未提交）
把重连从“只有无头切片”接到真实运行的 ggez client 与 host，全部走真实 UDP：
- **net `lockstep.rs`**：
  - `HostLockstep` 加 `set_snapshot`/`current_snapshot`（周期保存快照）、`client_addr`（掉线后仍保留端点到槽位映射）、
    `idle_ticks` + `client_idle_ticks` + `auto_drop_idle(threshold)`（host 自动判定掉线）；`poll()` 改为：
    收到输入始终重记 peer、清零该端 idle；收到 `ReconnectReq` 按来源端点映射回槽位 → unmark + 回 `Snapshot` + 广播 `Resync(seq)`。
  - `ClientLockstep` 加 `send_reconnect_req` / `recv_snapshot` / `apply_resync`（对齐基线 seq）。
  - 语义锁定：**快照字节反映「已处理完 seq-1 帧」的世界，重连端从 seq 开始继续收帧**（seq = host.next_seq()）。
- **client `netlink.rs`（NetLink）**：加 `stale_ticks` + `bump_stale`/`stale_ticks`（掉线探测）、`try_reconnect`（发 ReconnectReq + 收 Snapshot）、
  `align_after_reconnect`（收 Resync 对齐基线）；`step_frame` 成功时清零 stale。
- **client `main.rs`（真实运行时）**：
  - host 端：每帧 `auto_drop_idle(HOST_DROP_TICKS)`（超时自动掉线不卡全队）；每 `SNAPSHOT_EVERY` 帧 `set_snapshot(world_to_bytes, next_seq)`。
  - client 端：收到权威帧即推进；没收到则 `bump_stale` + 乐观预测；`stale >= CLIENT_STALE_TICKS` → `conn_dropped` 冻结并显示重连 UI；
    按 **R** 触发 `try_reconnect` → 拉快照 → `align_after_reconnect` → 重建 World → 清输入残留 → 恢复 lockstep。
  - 常量：`HOST_DROP_TICKS=180`、`SNAPSHOT_EVERY=30`、`CLIENT_STALE_TICKS=180`。
- **新增测试锁死（防假绿）**：net `reconnect_snapshot_and_resync_roundtrip` + `host_auto_drops_idle_client`；client `netlink_reconnect_flow_resumes_identical_worlds`
  （真实 UDP，host+1 client 掉线→快照重连接回→两端逐位一致）。合起来把“host 自动判活 + 快照应答 + client 掉线重连”整条链路锁住。

→ 重连在真实运行时已闭环（host 自动掉线、快照应答、client 按 R 拉快照重建回接）。

## 单机启动死循环 bug + 修复（本次会话 +1，未提交）
真机/多开排查发现 `--solo` **无法启动**（进程存活但无窗口、无 `[main]` 日志），追查到底：
`main()` 命令行解析里 `--solo` 分支**漏了 `i += 1`**，导致 `while i < args.len()` 死循环，单机永远到不了建窗。
- 修复：补 `i += 1`；把解析抽成纯函数 `parse_app_from_args(&[String]) -> AppState`，加回归单测 `solo_parse_does_not_hang_and_selects_solo`（锁死 `--solo` 不再死循环）。
- 顺带体验改善：配置/学习面板**默认预选第一个技能树**（`learn_tree_key` 默认 C 键树），按数字键绑技能立即可用，不必先想到按字母选树；
  Solo 开局配置加 **15 秒超时自动默认开始**（`PRE_GAME_TIMEOUT_SECS`，显示提示），避免窗口没焦点/按键收不到时单机卡死。

## 局域网体验补完（ROADMAP M2）—— 起步
- **对局结束可回主菜单** ✅（本次）：`MatchPhase::Finished` 结算界面新增“按 Q 返回主菜单”（`reset_to_main_menu` 重建 2 玩家沙盒世界/meta、清空联网与运行状态），不再死屏幕。Esc 暂用不了（ggez0.10 的 `NamedKey::Escape` 需直接 import winit，先只用 Q）。
- 待做：玩家名/开局房间界面、局域网发现（可选）、对接续的下一个体验闭环。

## 吸血/跳弹镖“无限时长·自动二镖”修复（本次，game-core 技能确定性 bug）
- 根因（不是联机）：`ProjectileKind::Chain` 中 `ratio_decay=0`（吸血/转镖）时，`ratio` 永不衰减；且每次命中都 `*life=1.5` 重置、`last_target` 只排除上一目标 → 会在末两个敌人间**无限往返**（看起来“自动发二镖/永远存在”）。
- 修复：`Chain` 增加 `max_chain`/`hit_count`；命中即 `hit_count+1`，达到 `max_chain` 或倍率衰减到 0 → 消失。吸血/转镖 `max_chain=3`，跳弹(T3) `max_chain=8`（其 ratio_decay>0 本就自限，加硬上限作安全网）。同步更新 `world_ser`。
- 加了回归测试 `chain_leech_terminates_not_infinite`（4 敌人围圈、跑 300 帧，链镖必须消失）。

## Steam 环境前置（A2 编译/运行所需）—— ✅ steamworks 可编译，已确认
- 之前猜测 `steamworks 0.15` 解析失败；查出真实稳定版为 **0.13.1**（rsproxy 源就有），改版本后可编译。
- `net-steam` 加 `steam` feature（默认关）+ 可选依赖 `steamworks 0.13`；`cargo build -p net-steam --features net-steam/steam` **编译通过**（steamworks-sys 0.13 + steamworks 0.13.1）。
- 默认（无 feature）`check.ps1` / build / test / clippy 全绿；feature 路径 clippy 也绿。
- 运行时前置已在手：**仓库根 `steam_appid.txt` = 908660**，Steam 客户端已登录。还差双账号用于端到端大厅/对战验收。

## Steam 大厅会话 `SteamSession`（自动发现/加入）—— 已落 + 已测（main.rs 接线待续）
- `net-steam::session::SteamSession`：封装大厅生命周期：
  - `init(app,port)`、`run_callbacks()`、`host_create_lobby(max, matchkey)`（公开大厅 + 写 `matchkey` 元数据）、
    `client_find_and_join(matchkey)`（`request_lobby_list` + 用公开 `lobby_data(matchkey)` 过滤 + `join_lobby`；
    注：steamworks 的 `add_request_lobby_list_string_filter` 需 `LobbyKey`（pub(crate) 字段）无法从外部构造，故改用公开 API 过滤）。
  - `prepare_transport()`（host→listen，client→connect_to(host)）；`table`（成员→槽位）、`identities()`（喂给 `set_client_identities`）、`my_slot`。
- 单锁步/重连复用：`HostLockstep<SteamTransport>` / `ClientLockstep<SteamTransport>`（传输无关，已有 `lockstep_over_steam_peers` 兜底）。
- 编译 + clippy（`--features net-steam/steam`）已绿；默认（无 feature）不触发、门禁不红。
- 待续：`main.rs` 接 `--steam-host/--steam-join` + lockstep 分支（需双机实测）。

## 法力量(MP)技术机制 + 成长点/购买 UI —— 已落 + 已测（数值占位、后续按手感调）
- **MP 机制**（game-core，先机制后数值）：`Attributes` 加 `mana_max`/`mana_regen`；`Player` 加 `mana`/`max_mana`/`mana_regen`（序列化）；
  施法 `handle_casts` 前用 `def.mana_cost()` 查蓝/扣蓝（`spend_mana`），蓝不足禁施；每帧 `regen_mana`；`SkillGrowth` 加 `mana_cost`（默认 0=不耗蓝，Rock 占位 30）。
  现有测试默认不耗蓝 → 保持绿；加 MP 测试（耗蓝/禁施/回蓝/上限）。
- **成长点/购买**：`PlayerProfile` 加 `growth_points` + 方法 `add_growth_points`/`buy_growth_with_gold`/`buy_attribute`；`PlayerConfig` 加 `growth_points`（v3→v4）；`Attributes::add_point`/`current`；`GrowthAttr` 枚举。
- **UI**：`draw_pre_game` 加「成长点/金币/属性」面板；按键 `Z 金币→成长点`、`H/J/K/L/;/U/I` 买各自属性；每局 `settle_round` 发 `GROWTH_PER_ROUND` 成长点。
- 测试：成长点发放/金币兑/买属性记账 -> **109 全绿**。数值(耗蓝/回蓝/价格)均为占位，后续平衡。

## 开局无初始技能 + 4.6b 阶段2（护甲/法抗/击退结算）—— 已落 + 已测
- **开局不带任何默认技能**：删掉 `Game::new` 与 `reset_to_main_menu` 里的“每个键默认绑首个技能”，
  完全由玩家在开局/学习界面按字母选树 + 数字绑技能（单机、局域网/Steam 一致）。默认预选 C 树便于直接按数字。
- **4.6b 阶段2**：`Player` 增加派生因子 `armor_factor`/`spell_factor`/`kb_factor`（随角色序列化）；
  `apply_attributes` 一并设置；`world_ser` 序列化含它们。
  - 伤害：`damage_player` 与 `explode_at` 对玩家造成伤害（`from=Some`）按目标 `armor_factor×spell_factor` 折算；环境伤害不减。
  - 击退：`Player::push` 统一按 `kb_factor` 缩短击退时长（所有击退都过此）。
- 测试：护甲/法抗减少玩家伤害（用 Rock 爆炸对比）、击退抗性缩短短推、序列化往返保留因子 → **107 全绿**。
- 阶段 3（后继）：法力量、成长点/购买 UI、数值调优。

## 4.6b 属性系统（第一步：字段 + 派生前 Hp/移速）—— 已落 + 已测
- 新增 `game-core/src/attribute.rs`：`Attributes`（hp_bonus / speed_bonus / armor / spell_resist / kb_resist，整数点数）+ 派生系数
  （`HP_PER_BONUS`、`SPEED_PER_BONUS`、护甲/法抗/击退折算系数）与**确定性纯函数** `derived_max_hp`/`derived_speed_mult`/`armor_factor` 等。
- 接入：
  - `PlayerProfile` / `PlayerConfig` 加 `attributes` 字段；`PlayerConfig::CONFIG_VERSION` bump v1→v2，编码/解码加 5 个 u32。
  - `Player` 加 `speed_mult`（base_speed 乘它）+ `apply_attributes`（按系数重设 max_hp、保持血比、设移速倍率）；`world_ser` 序列化含 `speed_mult`。
  - `main.rs::teardown_round_end` 复用 `p.apply_attributes(&profile.attributes)` → **单机与联机共用**，属性跨端/跨局确定性一致。
- 测试：attribute 派生确定性 + clamping、PlayerConfig 往返含属性、world 应用/血比/序列化往返 → **106 全绿**。
- 阶段 2（文档后继）：护甲/法抗/击退接入 `events` 伤害与 `push` 结算点；法力值系统单独评估。

## Steam 真实接入（A2 中间：P2P 传输已实现）—— `SteamTransport` 完整实现
- **`SteamTransport`（`steam` feature）现已用 `steamworks 0.13` 实现完整 `Transport`**：
  - `init(app_id, virtual_port)` = `Client::init_app`；`steam_id()` / `local()`=自己的 `Peer::Steam{id}`；`run_callbacks()`。
  - host：`listen()` = `create_listen_socket_p2p`；client：`connect_to(host_steam_id)` = `connect_p2p`（`NetworkingIdentity::new_steam_id`）。
  - `send_to/recv_from`：`Peer::Steam{id}` ↔ `Netconnection`；`recv_from` 内部 `listen.events()` accept 新连接 + 各连接 `receive_messages`，返回 `(peer_id, 数据)` 并映射回 `Peer::Steam`。
  - `create_lobby`（`LobbyMatching::create_lobby`，`LobbyType::Private`）用于大厅链路；`set_player_table` 配 `lobby::LobbyPlayerTable`。
- 类型路径：`steamworks::networking_sockets::{ListenSocket, NetConnection}`、`networking_types::{SendFlags, NetworkingIdentity, NetworkingConfigEntry, ListenSocketEvent}`、`matchmaking::LobbyType`。
- 已验证：默认（无 feature）+ feature 两 path 都 build/clippy 绿；`--ignored` init 测试真机跑通。
- 剩双账号端到端：真机各登一账号 → host `listen`+`create_lobby`，client `join_lobby`+`connect_to(host)`，走 `NetLink::from_transport` 注入，帧同步零改动。

## Steam 真实接入（A2 单账号阶段）—— `SteamTransport::init` 已在真机跑通
- **`transport_steam.rs`（`steam` feature 下编译）**：`SteamTransport::init(app_id)` = `steamworks::Client::init_app`；
  `steam_id()`=本机 SteamID、`local()`=自己的 `Peer::Steam{id}`、`run_callbacks()`=每帧泵回调；`matchmaking()` 句柄备用。
  `send_to/recv_from` 目前返回“尚未接 peer 会话”明确错误（双账号后才用 SteamNetworkingSockets 填）。
- **真机验证通过**：`cargo test -p net-steam --features net-steam/steam -- --ignored init_and_read_own_steam_id` → **ok**（Steam 已登录 + AppID 908660，`Client::init_app` 成功）。
- 这确认：**Steam 客户端在跑 + AppID 有效**，Steam 联机第一步已打通。差双账号做大厅对战收发（A2 后半）。
- 注意：`Client` 每进程仅一个（steamworks 规定）；init 测试是 `#[ignore]`，默认 `cargo test --workspace` 不跑，CI 不依赖 Steam。

## Steam 传输适配 `net-steam`（方案丙：独立可插拔 crate）—— A1 已落
- **workspace 新增第 4 个成员 `net-steam`**（依赖 `net` + `game-core`）。
- **feature 门控**（丙的核心）：`steam` feature 默认关，且**当前先不声明 `steamworks` 依赖**（本环境 rsproxy 无法解析 steamworks）。
  无 Steam 环境 `cargo build --workspace` / `check.ps1` 照常绿；将来 registry 能解时再加 `[features] steam=[dep:steamworks]` + 可选依赖 + `transport_steam.rs` 真实接入。
- **A1 已交付（纯本地可编译可测）**：
  - `lobby.rs`：`SteamLobby`/`LobbyPlayerTable`——**大厅成员名单→玩家槽位+稳定身份**的确定性映射（host 槽 0，其余按 SteamID 升序），
    对接已有 `join_dedups_by_stable_identity`（按身份去重/找回槽）。不含 steamworks、可无 feature 单测。
  - `transport_stub.rs`：`SteamTransport` 实现 `net::Transport`（占位），收发明确报“未启用 feature”错误（不 panic），
    保持前端 `NetLink<T: Transport>` 类型关系可对上。
  - 测试：lobby 映射确定性 + stub 占位（+2，共 101 全绿）。
- **A2（待 `steam` feature + 双账号 + 能解析 steamworks 的环境）**：真实 `SteamAPI_Init` + `LobbyMatching` 大厅 + `SteamNetworkingSockets`，
  用 `NetLink::from_transport` 注入 `SteamTransport`，握手/重连/多局逻辑零改动（本会话已铺好的全部地基：Peer::Steam、NetLink 泛型、稳定身份）。

## 对局分叉 / 右键疑似控双角色的修复（本次）—— 关掉局域网乐观预测
- 日志确认：三端均已 `FIRST FRAME started`、host `emit seq=0`，开局/同步/进对局正常。
- 三端视觉不一致 / “右键像是控两个角色”的根因：**client 的乐观预测（4.7 阶段一）与后续权威帧叠加，
  导致本地 World 与 host 分叉**（预测步进 + 权威步进在同一 tick 都执行，无回滚校正）。局域网/Steam 要“逐位一致”，
  应在收不到权威帧时**等待**而非乐观预测。
- 修复：client 收不到权威帧时改为 `break`（等待，不扣 accumulator），仅按权威帧推进 → 严格 lockstep 确定性。
  （乐观手感需配完整回滚才上，见 `LATENCY_MASKING.md` 阶段二；本阶段保证“本地/内网逐位一致”。）
- “数字绑技能”再次由日志证实生效（`[learn] bind ... digit='1/2/3'`）；界面反馈偏弱，非逻辑问题，后续可加强。

## 局域网开局卡住的根因修复（本次）—— pre-game 块拦截配置同步
- 日志铁证：host 反复打 `pre-game done -> HostGather`（14 次），client1 打多次（重复按空格）；且 client1/client2 的 `[learn] bind ... digit='2'` 证明**数字绑技能其实生效**。
- **根因**：`pre_game_config` 在联网开局保持 `true`，而开局前配置块 `if pre_game_config && app != MainMenu { ... return Ok() }` **每帧 return**，把 `Fighting` 分支里的 `HostGather`/`ClientWait` 配置同步逻辑**挡住了**——所以 host 按空格进 HostGather、client 进 ClientWait 后，配置同步**永远不推进 → 对局卡在开局，无法开始**。
- **修复**：开局前配置块的条件改为 `... && net_cfg == Idle`——一旦进入配置同步（HostGather/ClientWait），本块放行到 `Fighting` 分支的同步处理，收齐后 `pre_game_config=false` 正常开第一局。也顺带消除了 host 反复 `finish_pre_game` 的重复调用。
- 结论：**“按数字选不了技能”是误判**（日志多次证明 bind 生效）；真正卡住的是“无法开始对局”，即上面这个控制流 bug，已修复。

## 局域网开局：玩家准备状态显示（本次）—— 消“以为卡住”的困惑
- 日志确认：“无法选技能”其实不成立（client2 的 `[learn] select/bind` 日志证明按键正常），卡住的是**多窗口焦点 + 每窗要按空格就绪**。
- `HostLockstep` 新增就绪查询：`local_cfg_ready()` / `client_cfg_ready(idx)` / `cfg_ready_count()`。
- `draw_pre_game` 新增「玩家准备状态」面板：
  - host：每个玩家 ✓已就绪 / ○ 等待（等你按空格/等待上报）；还没收齐人时显示“已加入 X/期待 M + 每个窗口先点击再按空格”。
  - client：显示自己「✓已就绪，等待 host 开始」或「○未就绪——请先点击本窗口，再按空格就绪」。
- 顺手修复 `draw_pre_game` 一处“各键当前绑定/”笔误。

## 稳定玩家身份（Steam 重连前提）—— 已落 + 已测 + 真机验证
- **`proto`**：`Join` 改为 `Join { identity: u64 }`，`Ack` 改为 `Ack { my_index, players, identity: u64 }`
  （身份=u64，Steam 将来直接放 SteamID；局域网=客户端随机/指定）。
- **握手**：`HostHandshake` 按**稳定身份**（或退化按来源 `Peer`）去重；同一身份重复 JOIN/重连 → 复用原槽位、回 ACK 返回同序号，
  不同身份各得独立槽。`ClientHandshake` 携带/回显身份。新增测试 `join_dedups_by_stable_identity` 锁死。
- **重连按身份找回槽位**：`ReconnectReq` 现在带 `identity`，`HostLockstep` 用 `client_identities` 优先按身份找回槽（Steam=SteamID），
  不依赖来源端点；`main.rs` 把握手读取的身份 `set_client_identities` 交给 HostLockstep。
- **客户端**：`NetLink` 持有 `identity`（`connect_udp` 随机，`connect_udp_with(host, id)` / `from_transport(t,host,id)` 可指定），
  `my_identity()` 上报；`try_reconnect` 带身份。真机 multi-launch 验证 `join_handshake OK` 且打印 `my stable identity`。
- 注意：client 是 binary crate，`pub` 但未在本 crate 内用到的方法会被 `-D warnings` 判 dead-code，
  需确保 `my_identity`/`connect_udp_with` 有实际调用（init 日志用到了）。

## Steam 前置基础（本次，为换 SteamTransport 铺路）—— 已落 + 已测
- **`Peer` 抽象升级**：`net::transport::Peer` 由单一 `Udp(SocketAddr)` 扩为
  `Udp(SocketAddr)` + `Steam { id: u64, conn: Option<u32> }`（id 作稳定身份/SteamID/重连身份，见 RECONNECT 挂点 2）。
  UDP 传输路径不变（lockstep/handshake 只按 `Peer` 判等/转发，不关心变体）。
- **证明“换传输底层零改动”**：新增头测试 `lockstep_over_steam_peers_preserves_determinism`——用假想 `FakeSteamTransport`
  （以 `Peer::Steam` 为端点的内存邮箱）跑 `HostLockstep + ClientLockstep`，两端按序推进 + 逐位一致。
- **客户端接线传输无关化**：`client::netlink::NetLink` 由 `StdUdpTransport` 硬编码改为泛型 `NetLink<T: Transport>`；
  新增传输无关 `NetLink::from_transport(transport, host_peer)` + 局域网便捷 `NetLinkUdp::connect_udp(host)`。
  `main.rs` 统一用 `netlink::NetLinkUdp`。将来换 Steam：把 `NetLinkUdp` 换成 `NetLink<SteamTransport>`、
  `connect_udp` 换成“用 SteamTransport 构造 + from_transport”即可，握手/收发/重连逻辑零改动。
- 方向性决定：**局域网“房间列表/广播发现”不做**（Steam 大厅天然提供），玩家名按 Steam 昵称（局域网允许缺省）。优先保证 Steam 联机。

## host 提早收人修复（本次会话 +1，未提交）
多开实测发现：LAN host 在「开局配置」阶段从不 poll_join，要等 host 窗口按 Space 进入 Fighting 才开始收 client →
无头/手快时先到的 client 会握手超时（100 次后 panic 退出）。已把 host 收人逻辑抽成 `poll_host_join_phase()`，
并在「开局配置」与 Fighting 两阶段都调用，host 等人时就开始收人。
实测：`multi-launch.ps1 -Players 2` 客户端 **attempt 1 即 join_handshake OK**（原先 100 次超时），两端按 Space 均进入配置同步。

## 三大功能（ROADMAP.md）当前进度
- **单机技能试验场**：✅ 已可用（无 AI + 靶子 + 开局配置；`--solo` 或菜单按 1）。
- **局域网对战**：✅ lockstep 打通（多局配置同步/重连切片/开局配置已在 UDP 验证）；缺"中途退出的真实掉线 UI"（见重连剩余）。
- **Steamworks 对战**：未接；`Transport` 抽象已就绪，靠实现 `SteamTransport` 复用局域网这套（含重连）。

## 界面（UI_MENUS.md）
- M0 主菜单已做（三大入口）。暂停/退出菜单、单机调试辅助（无敌/重置CD/伤害数字）**未做**，规划在 UI_MENUS。

## 待议 / 下一步（按我此前优先级建议排序）
1. **✅ 已办（未提交）**：重连接进 client 真实运行时（见上节“重连接入 client 真实运行时”）。
   剩可后做：真机多窗口手动验证重连手感 / host 掉线处理（快照给候补 / 回大厅）。
2. **暂停/退出菜单 + 单机调试辅助**（体验层，见效快，无网络顾虑；联网暂停=本地暂停交互、不暂停时间，退出按 RECONNECT 离场处理）。
3. **4.6b 属性系统**（`ATTRIBUTE_SYSTEM.md`：法抗/护甲/移速血量成长等；网络层已就绪，只加 PlayerConfig 字段 + 合成）。
4. **4.7 完整回滚重放**（`LATENCY_MASKING.md` 阶段二；决策门：先真机感受乐观预测，跳变明显再做）。
5. **Steamworks**（最终目标必含重连；先局域网验证重连，再接 SteamTransport）。

> 建议先跑一次 `multi-launch.ps1` 真机多开，手动停掉一个 client 看 host 自动掉线 + 其余继续；再手动按 R 重连接回。
> 注意多窗口是给**真人操作**的：键盘输入只发给**有焦点的那一个窗口**，需逐个点选窗口后按 Space 完成各自开局配置，
> 才能让 host 收齐配置并开打。无头环境无法聚焦窗口，故逻辑用 UDP 单测（`host_and_two_clients_sync_over_udp`/`netlink_reconnect_flow_resumes_identical_worlds`）锁死。

## 常用命令
```
cargo test  --workspace                # 回归（97 全绿基线）
cargo clippy --workspace -- -D warnings
cargo run -p client -- --solo          # 单机试验场
powershell -File multi-launch.ps1 -Players 3   # 局域网多开（-Fast 加速局终看多局；可手动停窗看重连）
powershell -File check.ps1             # 一键 build+test+clippy
```

## ⚠ 环境大坑（务必再读）
git 仓库根在**上级 `testingLL/`**，`rust_remake/` 只是子目录。`core.hooksPath` 用绝对路径指到 `rust_remake/.githooks`；pre-commit 用 `$0` 定位项目根。已端到端验证：坏代码拦得住、干净提交放行。
