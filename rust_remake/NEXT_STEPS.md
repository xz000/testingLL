# 交接记录 / 下一步（离线续接用）

> 唯一的当前权威续接入口。**2026-08-15 晚**重写为最新快照。
> 阅读顺序：先看本文件 → 缺背景再翻 `RECONNECT.md`（重连）/ `ROADMAP.md`（三功能）/
> `LATENCY_MASKING.md`（手感）/ `ATTRIBUTE_SYSTEM.md`（4.6b）/ `NET_REWRITE.md`（网络重写）/
> `LOCKSTEP_FOUNDATION.md`（基座）/ `UI_MENUS.md`（界面）。

> ⚠ **Steam 双机「不能自动进对战 + client 无法操控」根因分析与修复，见下方「Steam 根因分析」节（2026-08-17：已完成代码实现，待真机双机验证）。**

## 当前状态（全绿）
- **单测 113 全绿 = game-core 87 + net 19 + client 5 + net-steam 2**；`cargo build --workspace`、`cargo test --workspace`、`cargo clippy --workspace -- -D warnings` 均绿、工作区干净。
    - feature 路径（`client/steam`、`net-steam/steam`）build + clippy 也绿。
- 技术栈：`game-core`（定点确定性核心）+ `net`（proto/handshake/lockstep 三层）+ `client`（ggez）。定点 `fixed=1.28`、三角 `cordic`；`Balance` 数值收敛层已建。
- 测试计数几乎每次新增/重连切片都在涨（79/14/5）。**续接时以 `cargo test --workspace` 为准，别信本文件里的静态数字。**

## Steam 房间就绪（重点待续）——已重写干净流程 + 双机测试卡点记录
**目标流程（你确认的）**：房间界面（成员昵称+就绪状态，多人一致）→ 按 o/空格 toggle 就绪（**可撤销**）→ 全员就绪 → 5 秒倒计时（有人取消则重置）→ 进配置菜单（技能/点数）→ 各自配完 → 开打。不带 host 手动开始。局域网先不动。
**已实现（提交 `c96d39f` 等）**：
- net：`Packet::PlayerReady{index,ready}`（client→host 可撤销就绪）、`Packet::StartConfig`（host→client 进配置）；`HostLockstep::clients_ready`/`client_ready`/`all_clients_ready`/`broadcast_start_config`；`ClientLockstep::send_ready_state`/`recv_start_config`；PlayerReady poll 也记 client 端点（供广播）。net 16 测试绿。
- client：`steam_in_lobby` 房间阶段 + `steam_lobby_update`（o toggle 就绪、client send_ready_state、host 判定全就绪→倒计时→StartConfig、各端进配置菜单 pre_game_config）；`draw_steam_ready_overlay` 显示成员昵称+就绪+倒计时；`finish_pre_game` Steam 直接开打（不再用 net_cfg HostGather/ClientWait）；Fighting Steam 分支简化（host 收齐输入才产帧，client 跟帧）。
**双机测试结果（最新）**：
- 两端都能进房间阶段、toggle 就绪（日志 `[steam-lobby] local ready = true/false/...`）、P2P 连接建立（host `[steam-p2p] host accepted connection`）。
- **但 host 未能判定“全员就绪”→ 没倒计时/没进配置/没开打**。根因猜测：host 靠 `all_clients_ready()`（收 client 的 `PlayerReady` 包）判定，但 client 只在按 o 那帧 `send_ready_state` 一次，且**房间阶段 client 没有像局域网那样持续上行输入**——P2P 包一次性发送可能因连接建立时序丢包，host 收不到 client ready。
- **已修（2026-08-16）**：房间阶段 client 改为**每帧持续上行输入（在场信号，对齐局域网 upload）+ 上报当前就绪（幂等）**；host 判定升级为 `本地就绪 && saw_all_clients() && all_clients_ready()`（所有 client 在场 && 全体就绪）；新增 **`RosterReady` 广播**（host 每帧把各成员就绪状态快照发给所有 client），让每个端都显示所有成员就绪（多人一致界面，不再只有 host 能看别人）。用 UDP 单测 `room_flow_presence_and_ready_and_roster` 锁死。
- 还需双机实测确认：房间就绪（含可撤销）→ 配置 → 对战不再卡在“host 判不了全员就绪”。
**关键对照（务必先读局域网多人开始逻辑再改）**：
- **局域网核心**：client 在 `ClientWait` 后**每帧持续 `upload` 上行输入**（无条件），host `try_emit` **收齐所有 client 输入才产首帧** → 全员一起开始（天然多人）。局域网没有显式“按 o 就绪”，是“连上即在场即开始”。
- 你要的“按 o 就绪可撤销”是在这个自动开始机制前加一个显式房间阶段。
**下一步（新会话从这里）——照局域网可靠机制对齐（已完成 ✅，本次会话）**：
1. ✅ 房间阶段 client **每帧持续 `send_input`（在场信号，同局域网 upload）**，按 o 时才发 `send_ready_state(true/false)`（就绪开关）。
2. ✅ host 房间阶段 `poll` 收客户端（持续在场 + PlayerReady 就绪），判定 = 本地就绪 && 所有 client 在场（saw_all_clients()）&& 所有 client 就绪（all_clients_ready()） → 倒计时 → StartConfig 进配置。
3. ✅ host→client 的“成员就绪状态快照”（`RosterReady` 广播）让每个端房间界面都显示**所有成员**就绪（多人一致界面）。
4. 待双机验证：房间就绪（含可撤销）→ 配置 → 对战。局域网保持不动。

## Steam 联机根因：P2P 连接未建立前 SendMessage 被静默丢弃 —— 已加 established 门 + 诊断（本次会话，未提交）
**真机测试再卡（2026-08-16）**：即便已上“持续在场+幂等就绪+RosterReady”，host 仍判不了全员就绪。查 steamworks 源码：
`SendMessageToConnection` 在连接**尚未 ESTABLISHED** 时返回 `k_EResultIgnored`（消息直接被丢弃），而我们 `.ok()` 静默吞错，
于是 host 永远收不到 client 的 Input/PlayerReady（P2P 握手建立与首包发送的时序竞态）。
- **修复**：`SteamTransport::send_to` 现在**先 `run_callbacks()` 推进握手**，且显式检查该端连接状态；非 `Connected(Established)` 时返回明确
  “not established yet”错误（不静默丢），上层 `.ok()` 丢弃该次、但**每帧持续重发**，连接建立后自然送达 → 不再因时序丢包。
- **诊断**：host 房间阶段每 ~120 帧节流打一条 `[steam-host] waiting: local_ready / present_clients / ready_clients / alive_conns / expected`，
  真机卡点能从日志直接看出“等了谁”（在场/就绪/连接各几何）。`HostLockstep` 新增 `present_clients_count`/`ready_clients_count`/`connected_clients_count`/`expected_clients`。
- 待双机重测：看 `[steam-p2p] host connection ESTABLISHED` + 上述 waiting 计数，确认 host 能收到 client 在场/就绪并倒计时。

## Steam 大厅/加入界面 + 可显示字符替换（本次会话，未提交）
- **主菜单 Steam 入口变为真界面**：按 `3` 进 Steam 大厅子菜单，`H/空格` = 建 2 人厅、`J` = **自动按 matchkey 加入（无需输房间号）**、`Q` 返回。
  `Game::enter_steam_mode`（自菜单重建 Steam 会话/lockstep/world/meta，复用 `--steam-host/--steam-join` 同套构建代码）。
  `run-steam.ps1 -Mode menu` 可起 Steam 版菜单（自动 stage DLL/appid）。
- **替换 UI 里可能打不出来的字符**（CJK 字体可能缺字形）：`◀`→`[已选]`、`✓`/`○`→`[v]`/`[ ]`、`→`→`->`、`——`/`──`→`-`、`·`→`/`、`…`→`...`。
  （技能名里的 `·`（如“潜行踢·连推”）为 U+00B7 常见字符，保留。）

## Steam 双机重测：P2P 在场已通、但“就绪”仍不达（本次会话诊断位）
**2026-08-16 双机重测（established 门 + 持续在场已生效）**：
- host 日志铁证：`[steam-p2p] host connection ESTABLISHED` 后，`present_clients=1/1`（client 的输入在场已送达 host）、
  但 `ready_clients=0/1` 恒为 0；即使 client 按 o 且 `[steam-client] local ready=true`。
- **结论**：Steam P2P 的 **Input 包能到 host，PlayerReady 包不到**——同一条连接、同样 `send_to`，只有包内容不同。
  疑似 `send_ready_state` 方向/编码或某处把 ready 值当 false 发；也可能是传输层对 3 字节小包的处理。
- **本轮加了决定性诊断**：
  - client：`[steam-client] sent ready=true to host`（发送成功才打）/ `[steam-client] send_ready_state failed: ...`（建立门失败才打，节流一次）。
  - host：`ready_pkts=N`（累计收到的 PlayerReady 包总数）。可区分“包没到（ready_pkts=0）”还是“包到了但值是 false（ready_pkts>0 且 ready_clients=0）”。
  - `[steam-lobby] roster ready snapshot: [...]`（client 收到的实时就绪快照，确认 host->client 广播是否通）。
- **另修**：client 房间阶段**先 `recv_start_config` 再 `recv_roster_ready`**（避免 RosterReady 读取器把 StartConfig 当非目标包消费掉 -> 进不了配置菜单）。
- **根治候选已实施：`RoomState` 合包**（既然独立 PlayerReady 包 P2P 下常丢、而 Input 在场包被验证可靠送达）：
  新增 `Packet::RoomState { index, ready, input_bytes }`（TAG 16），client 房间阶段用 `send_room_state(ready, presence_input)` 单包持续上行，
  host 收到一次即同时更新「在场 + 就绪 + 端点 + 空闲」→ 就绪与在场天然同路、同可靠性。
  net 单测 +1：`room_state_bundle_sets_both_presence_and_ready`（锁在场+就绪同包、可撤销）。协议往返含 RoomState。
- 下一步（新会话从这做起）：跑双机，host 若 `present=1 && ready=1` 则应能过“全家就绪→倒计时→StartConfig”；
  若仍 `ready=0` 则看 client 的 `send_room_state failed` 与 host 的 `ready_pkts` 定位传输层问题。

## Steam 双机再测：`RoomState` 生效、host 已 “all ready -> broadcast StartConfig”（本次会话）
**2026-08-16 三测（RoomState 合包生效）**：host 日志 `present_clients=1/1 ready_clients=1/1`（就绪已在场包同路送达）、`[steam-host] all ready -> broadcast StartConfig`。
- 说明「就绪折进在场包」已解决“host 判不了全员就绪”。剩最后一段：**host→client 的 StartConfig 是否被 client 收到并进入配置菜单**（client 日志本截到 `sent room_state ready=false` 未显示进配置）。
- **修（防 StartConfig 被吞）**：改用 `ClientLockstep::recv_room_inbox` **单次排空**入包并对 StartConfig / RosterReady 分类，
  杜绝“先读 RosterReady 的循环把 StartConfig 当非目标包消费掉”的竞态。client 房间阶段改走它。
  net 单测 +1：`room_inbox_classifies_start_config_and_roster_together`（StartConfig 与 RosterReady 同队列不互吞）。
- 下一步：再双机测，看 client 是否打 `[steam-client] host says all ready -> config menu` 进配置；进而配置→对战。

## Steam 四测：client 仍卡“进配置”——起始小包（StartConfig 仅 1 字节）被 Steam P2P 丢（本次会话）
**2026-08-16 四测**：host 已到 `[steam] config done -> start match` / `emit seq=0`；client 能收到 `roster ready snapshot: [(0,true),(1,true)]`
（即 host→client 的 RosterReady 广播可靠），但**没打 StartConfig 进配置** → 对“client 收到大包(RosterReady)却不收小包(StartConfig)”的规律加重证据。
- 确认**传输层丢小包规律**：且大（Input/RosterReady，几十字节）都能到；独立小包（PlayerReady 3B、StartConfig 1B）被丢。Steam P2P 小消息易丢。
- **修（双保险）**：
  1. **StartConfig 加负载**：`Packet::StartConfig { seq }`（变 9 字节，不再是小包）；`broadcast_start_config` 连发 3 拍。
  2. **client 用可靠 RosterReady 自触发进配置**（关键）：尽管 StartConfig 仍可能丢，client 收到「宿主广播的就绪快照显示所有玩家已就绪」时
     自触发 `entered_config` 进配置菜单（不再依赖可能被丢的 StartConfig）。这是主路径；StartConfig 改为冗余/兜底。
- net 单测仍绿（`room_inbox_classifies_start_config_and_roster_together` 已适配 StartConfig{seq}）。
- 下一步：再双机测，client 应打 `roster shows all 2 players ready -> config menu` 进配置；两端配完按 o/空格开打。

## Steam 五测：房间→配置→开始都通了，但“对战不同步”（本次会话，诊断位）
**2026-08-16 五测**：两人都能 `config done -> start match`（房间/配置/开始链已通），但进对战后 host 只 `emit seq=0`、且两边状态似各自独立——
两者应是 **Fighting 阶段的帧交换未建立**：host 需收 client 每帧输入才能连续产 seq 帧；client 需收 host 帧才推进。
- 已在两端 Feeding 循环加诊断日志：
  - host：`emit seq=N, n_entries=`（前 30 帧）/ 以及“trying to emit but waiting for client input (present=)” （节流，若产不出帧）。
  - client：`frame -> seq=N`（推进到哪帧）；及旧的 `send_room_state failed`。
- 用这些日志可定位：若 host 卡在 seq=0 且一直“waiting for client input” → client 输入没到 host；
  若 host emit seq 连续而 client 不打 `frame -> seq` → host→client 帧投递断；若两者都连续 → 是 world/配置分叉而非投递。
- 另自查过：Steam host 与 client 均用同 seed/同 2 玩家建世界、`teardown_round_end` 同步，理论上逐位一致，只差帧交换。

## Steam 六测关键：host 产帧但 client 一帧没收——建议根因=配置期连接被 Steam 拆除（本次会话，已修保活 + 诊断）
**2026-08-16 六测**：host `emit seq=0..22`（自跑 23 帧，n_entries=2）后卡 `waiting for client input (present=0)`；
client 完全不打 `frame -> seq`（**一帧 host 广播都没收到**）。且 client 的移动在 host 上可见（host 在跑），但 client 画面不动。
- **结论**：host→client 的 Frame 广播从一开始就没送达；约 23 帧后 client→host 的输入也断（present=0）。两方向同断 → **P2P 连接断了**。
- 断点成因：**房间→配置→开打之间，配置阶段两端都不 pump 回调/无流量，Steam P2P 连接被拆除/timeout**；开打后 host 空转产帧（send 失败被吞），client 收不到一切。
- **修**：`steam_config_keepalive()` —— 配置阶段每帧 `send_room_state`(client) 或 `poll`+`broadcast_roster_ready`(host) 作心跳，持续 pump 回调 + 双向保活，防止连接在配置期死掉。
- **顺带加重诊断**（`SteamTransport`）：`send_to` 失败（no-conn / not-established / send_message）与 `receive_messages` 失败（会 remove conn）都打前 10 条，
  能直接看出“连接是否被拆、为何发不出”。
- 下一步：再双机测，看 `[steam-p2p] send_to/...` 是否还在报错；若配置期保活后连接得住，对战时应有 `[steam-client] frame -> seq=0,1,2...` 且两端逐位一致。

## Steam 七测：保活生效、client 收到帧但仍有输入断流——切 `send_room_state` 上对战斗输入（本次会话）
**2026-08-16 七测**：配置期保活后 **连接不再被拆**——client 开始打 `frame -> seq=1, seq=2`（收到 host 帧了，对比上次一帧不收有进展）。
但 host 仍卡 `waiting for client input (present=0)`（跑 ~30 帧后 client 输入断流），client 也还是“画面不动”（卡在等缺失帧/没推进）。
- 归因：client 对战斗输入用 `send_input`（独立 `Packet::Input`，Steam P2P 下实测间歇丢、跑一会儿就断），导致 host `try_emit` 收不齐、停产帧；
  client 因缺帧无法连续推进。而 `RoomState` 合包（在场+就绪+输入）在房间阶段被证明稳定送达。
- **修**：Steam client 对战斗（Fighting）也改 `send_input` → **`send_room_state(ready, input)`**（走已证明可靠的合包通道），
  host 的 `RoomState` 处理已会把 `input_bytes` 写进 `latest_input` → `try_emit` 照常收齐产帧。只改 client 一处，host 无需动。
- 下一步：再双机测，host 应不再卡 `waiting for client input`（present 持续 =1），client 应连续 `frame -> seq=0,1,2...` 且两端角色都动起来。





## Steam 根因分析 + 修复落地（2026-08-17：代码已实现，待真机双机验证）：两个症状的机理 + 修复方案
**2026-08-17 接手复盘**。双机最新观察复述：
- 症状 A：**不能自动进对战**——要机器上“分别按 o”，两台才各自进对战（各自手动、不同步）。
- 症状 B：**client 画面角色无法操控**，但 host 能看到两端角色都在动。

> 读码结论：这两条不是两个独立 bug，而是「**对战开始没有同步**」这一个根因的两个表象；症状 B 还叠加一个「配置期 keepalive 吞帧但没步进世界 → expect_seq 与 world 分叉」的确定性 bug。

---
### 症状 A 机理：开始对战的“第二道手动确认”没去掉，且各端独立触发
当前 Steam 进对战实际是**两段手动按 o**：
1. `steam_in_lobby`：按 `o` toggle 就绪（可撤销）。全员就绪 → 5 秒倒计时 → `StartConfig` → 进 `pre_game_config` 配置菜单。
2. `pre_game_config`（`main.rs` 的开局配置块）：`done = 空格 || o` → `finish_pre_game()`（Steam 分支只 `teardown_round_end(); pre_game_config=false`）→ 进 `Fighting`。
- 第 2 段是**各端自己按 `o` 才各自离开配置菜单进对局**，host 与 client 之间没有任何“统一开始”信号 —— 这就是“两台机器分别按 o、分别进入对战”的来源。
- 反观**局域网**：没有配置菜单按 o 这段；host 在 `Fighting` 里 `try_emit` **收齐各端输入才产 seq=0 首帧**，首帧即统一开始信号 → 天然同步开始。这正是你要“参考的开始对战机制”。

### 症状 B 机理：不同步开始 → 先开的一边跑、后开的一边 world 被 keepalive 吞帧而分叉/冻结
- Steam client 在 `Fighting` 分支**只收权威帧步进 `self.world`**（`cli.step_frame → world.step`）。host 在 `Fighting` 分支 `try_emit`（有 host local + 全部 client 输入）就产帧步进自己 world —— 所以 **host 的 world（含两端输入）在跑 = host 看到两只都在动**（吻合观察）。
- 若 client 因“分别按 o”比 host 晚进 `Fighting`，则 host 已开始 `emit seq=0,1,2,...`；仍在 `pre_game_config` 的 client 每帧 `steam_config_keepalive()` 里调 `cli.step_frame(&mut krcv)` 收帧，但**丢弃返回（不 step world）** —— 而 `step_frame`/`try_advance` 会推进 `expect_seq`。
  → client 的 `expect_seq` 被前冲、world 却停在原地 → 进 `Fighting` 后从「n 帧之后」的输入继续步进同一个初始 world，**与 host 的 world 永久错位（滞后 n 帧、越拉越僵），画面看起来就像自己的角色不动、不可操控**。
- 若“后开的其实是 host”则会反过来（host 停、仅 client 在跑）——总之**谁先离开配置谁先跑，后开的那个 world 分叉/冻结**。这就是每次“分别按 o”结果不稳定的原因。

### 症状 B 的次因（历史已修大半，需再确认）
- 早期“host 产帧但 client 一帧不收 / 输入断流”主要是「配置期连接被 Steam 拆除（无回调/无流量）」与「独立小包(P2P 下发送时未 established/时序)被吞」。
  - 已修：配置期 `steam_config_keepalive` 保活；`SteamTransport::send_to` 加 established 门 + `run_callbacks`；client 对战斗输入也改走可靠的 `send_room_state`。
  - 待确认：**帧(host→client)与输入(client→host)两条方向在真机双机是否都稳定送达**。诊断日志已齐：host `[steam-host] emit seq=N, n_entries` / `trying to emit but waiting (present=)`；client `[steam-client] frame -> seq=N` / `send_room_state failed`。

---
## 修复思路（按“先跨通、再加缓冲”排序）
### 修 1（核心正确性，必做）：去掉“各端按 o 才开打”，改**局域网式统一开始**
- 不再让 `pre_game_config` 里的 `done = 空格||o` 触发各自 `finish_pre_game`。
- 把「配置完成 / 就绪」折进**每帧上行**（用现有 `RoomState`，client 在配置菜单结束时置 `ready=true` 并以 `send_room_state(ready, input)` 每帧持续上报，天然同路可靠）。
- host 在 `pre_game_config` 阶段**照现逻辑每帧 `poll` 收各端 ready**，等「本地 + 所有 client 都 ready」→ 才开始 `try_emit`（seq=0 首帧）→ 各端 `Fighting` 一起以同一 seq 开始。
- client 端 `pre_game_config` 结束时**不再自己 `finish_pre_game`**，而是保持等待 host 产帧 → 收到 seq=0 首帧才切 `Fighting` 并开步进。
- 效果：与局域网一致，首帧=统一开始；两台不再分道扬镳，症状 A/B 的“不同步”根因直接消失。

### 修 2（正确性，防复发）：`steam_config_keepalive` 不得吞帧推进 `expect_seq`
- 配置期/等待期 client 只在“收心跳/推进握手”时 `run_callbacks` + 读包，但**不要用 `step_frame` 消费 FRAME**（否则 `try_advance` 把 `expect_seq` 前冲、world 不跟着走 → 分叉）。
- 可加一个“不推进的收包”方法（或直接不读 FRAME，只收 RosterReady 等配置期包），保证 `expect_seq` 只在 `Fighting` 真正对战时才动。

### 修 3（想要的体验，在修 1/2 打底后做）：全员就绪 → 3~5 秒缓冲 → 自动进对战，期间可取消就绪
- 复用现有就绪判定 + 倒计时：`all_ready = 本地就绪 && saw_all_clients && all_clients_ready`，`all_ready` 时 `steam_countdown = 5.0` 递减。
- **取消机会**：倒计时期间任何一端按 `o` 取消就绪（client `send_room_state(false)` / host 本地 toggle false），host 检测非全就绪 → 重置 `steam_countdown = 5.0`；倒计时归零（且未被取消）才放行统一开始。
- **给各端显示倒计时与谁取消**：现有 `RosterReady` 广播只含每人 ready 布尔；“倒计时/是否已放行”建议再带一个共享阶段（如 `STARTING(N)`）或跟首帧联动。若嫌复杂，可先只用“就绪快照 + host 放行后自然产首帧”的简单版跑通。
- 简单版保底：不弄倒计时 UI，只要“全员就绪即统一开始”（等同局域网机制）也算达到你要的“自动进对战”。

### 修 4（可选加固）：`steam_lobby_update` 的倒计时边界 bug
- 现 `if o_pressed || self.steam_countdown <= 0.0 { broadcast StartConfig }`——**若全员就绪恰好发生在“按下 o 的当帧”** 会跳过倒计时秒进配置；`steam_countdown` 初值在 `enter_steam_mode` 为 `0.0`（非全就绪帧才重置 5.0），与倒计时叠加易出“秒进/永不进”边界。改统一：`all_ready` 才真正开始倒计时，且 `o` 在该阶段只做“取消”，不再做“秒进”。

---
## 修复落地（2026-08-17 已实现，steam build/test/clippy 全绿；逻辑用 net 单测 U 锁死，真机待验）
- **修 1 ✅ 对局开始统一（局域网式）**：
  - `RoomState` 新增 `build_done` 字段（client→host 每帧上报“配完”）；proto 编解码含之（`proto.rs`）。
  - `HostLockstep` 记录各 client `build_done`，新增 `all_clients_build_done`/`client_build_done`/`reset_clients_build_done`（`lockstep.rs`）。
  - `steam_config_update`：配置阶段玩家按空格/o 置 `build_done`；host 等「本端 + 所有 client 配完」→ 进入 HostGather；client 配完 → 进入 ClientWait。
  - Fighting 分支新增 Steam 的 HostGather/ClientWait 配置同步（复用现成 `poll_cfg`/`set_local_cfg`/`collect_cfgs`/`broadcast_cfgs`/`send_cfg`/`recv_cfg_all`）：收齐 PlayerCfg → 广播 PlayerCfgAll → 各端 apply → teardown → host 产 seq=0 首帧统一开战、client 收首帧开战。
  - 效果：去掉“各端按 o 各自进对局”（症状 A）；并**同步了各端技能配置**（两端 world 逐位一致，堵住配置分叉）。
- **修 2 ✅ 配置期不吞帧推进 expect_seq**：新增 `ClientLockstep::pump_frames`（只缓存不推进）；`steam_config_update` 配置等待期用 `pump_frames` 保活（不再 `step_frame` 前冲锚点）——堵住症状 B 的“后开一边 world 分叉/冻结”。
- **修 3 ✅ 缓冲 + 取消 + 锁定**：host 房间阶段全员就绪 → 5 秒倒计时（`STEAM_READY_COUNTDOWN_SECS`），倒计时内有人取消（ready=false / host 按 o）则重置；**最后 2 秒 `STEAM_COUNTDOWN_LOCK_SECS` 锁定，忽略取消**（host 权威执行，防临界竞态）。
- **修 4 ✅ 倒计时边界**：去掉旧的“on o_pressed 秒进”；倒计时由 `steam_was_all_ready` 状态机控制，只在真正全员就绪后才启动、清零才放行。
- **UI**：房间层显示“N 秒后进配置（可取消）”；配置层显示各端配好状态 + “我已配好等待全员配完统一开始”。
- **net 单测 2 新增**：`client_pump_frames_caches_without_advancing_expect`、`steam_config_gather_then_unified_start_identical`（配置→统一开战链）；workspace 116 全绿（client 5 + game-core 87 + net 22 + net-steam 2）。
- **实机双机再测（2026-08-17）：配置同步阶段连接断开（ClosedByPeer）卡点，已补修**：
  - 现象：房间就绪/倒计时/进配置都通了；host 日志 `[steam-host] all ready countdown zero -> broadcast StartConfig` → 两端都打了 `[steam] build done -> waiting ...`，但 **host 一直没进 HostGather**（没打 `all players configured`），随后 host 侧连接反复 `state=Ok(ClosedByPeer)`（对端连接断了）。
  - 根因①（host 判定卡住）：host 进配置时 `reset_clients_build_done()` 会把 client 已上报的 build_done 清零；而 client 一旦配完就进入 ClientWait（原本不再上行 build_done）→ host 永远收不到 client build_done=true → 一直不进 HostGather。
  - 根因②（连接断/保活不足）：进入配置同步（ClientWait/HostGather）后，client 只发 `send_cfg`、host 只 `poll_cfg`，没有每帧上行/广播保活；一旦同步拖久，P2P 连接易被 Steam 断开/对端异常关连 → ClosedByPeer。
  - **补修**：`reset_clients_build_done()` 不再在进配置时调用（build_done 由玩家按 o 自然置位+续报，host 不筛漏）；client 在 ClientWait 阶段每帧 `send_room_state(ready, build_done, input)` 续报 build_done + 保活；host 在 HostGather 阶段同时 `poll`（收 RoomState）+ `poll_cfg` + 每帧 `broadcast_roster_ready` 保活；并加节流诊断 `[steam-host] config waiting: ... clients_build_done=…`。
  - 真机待验：双机配置→统一开战能推进，host 打 `all players configured -> config sync` + `synced N player configs`，client 打 `got N player configs` + 连续 `frame -> seq=0,1,2`。
- **根因确认 + Steam 传输可靠发送补发（2026-08-17，基于 steamworks 官方发送文档）**：
  - 对照：局域网不用按键也能开始、Steam 却卡——根源在**传输层**不是快捷键。查 steamworks 头文件 `SendMessageToConnection`：RELIABLE 消息只要 send 返回成功就保证送达；但**连接尚未 ESTABLISHED / 无效时 send 返回错误**（`k_EResultInvalidState`/`k_EResultNoConnection`/`k_EResultLimitExceeded`/包过大 `InvalidParam`）。旧 `transport_steam.rs::send_to` 遇错 `return Err`，上层一律 `let _=` 静默吞 → **一次性关键包（StartConfig/PlayerCfg/PlayerCfgAll/ReconnectReq）在建立时序里被拒即永久丢失** → client 收不到 → 卡死/分叉。
  - **修复（`net-steam/src/transport_steam.rs`）**：`send_to` 遇“未建立/暂不可发/发送失败”不再报错丢弃，而是**入队待补发**（新增 `pending_sends` 补发队列 + `flush_pending`，每个 `send_to`/`recv_from` 前自动把已 ESTABLISHED 的积压消息按 FIFO 补发；RELIABLE 有序，故有历史积压时不直接发新包而追加队尾）。host 端 `Disconnected` 事件清理该 peer 的 conns+pending；队列有 1024 条上限防膨胀。这样建立前发出的关键包在连接建立后会自动补发一次，client 必达。
  - 配套：**键位拆分**——`o` 从 Steam 流程彻底移除；房间就绪用 `U`，配置确认配好用 `P`（空格在本环境实测不可靠，LAN 也能开始说明不是按键问题，但 Steam 的确认键换可靠字符键）；非 Steam(LAN/Solo) 开始键保留空格+P 克底。
  - 状态：workspace 116 测试全绿；steam feature build/clippy 全绿。待真机双机验证配置→统一开战（host 打 `all players configured` / `synced N`，client 打 `got N` + `frame -> seq=0,1,2`）。

## 验收 / 下一步（新会话从这里做）
1. **真机双机**：确认 client 不再需要“手动进对局”——房间全就绪 → 倒计时（最后 2 秒按 o 无效）→ 进配置 → 各自配完 → **自动统一进对局**；client 连续 `[steam-client] frame -> seq=0,1,2`，两端角色都动、可操控、逐位一致。
2. 观察登录：`[steam-host] synced N player configs -> pre-game` / `[steam-client] got N player configs` / `[steam-client] build done -> config sync`。若 host 卡“等待配置”，看 `[steam-host] config waiting: ... clients_build_done=…`（等哪个 client 配完）；若某端连不上看 `[steam-p2p] ... not established` / `ClosedByPeer`。
3. 局域网/Solo（非 steam feature 路径）不受影响，build/test/clippy 全绿。
4. 若体验要再调：把锁定改为“最后 N 秒”（现在 2 秒），或调整倒计时长度。
> 已知边界（本次不处理）：Steam 多局（round 2+）的经济/升级配置同步仍沿用旧逻辑（首局走新统一开始；后局依赖既有每帧通道），如需逐位同步多局配置需后续接入与局域网一致的完整同步（本次只修“开局不能自动进对战”）。


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

## `main.rs` Steam 接线（--steam-host / --steam-join + lockstep 分支）—— 已落编译，待双机实测
- `client` 加可选依赖 `net-steam` + `steam` feature（默认关，check.ps1 不触发）。
- `AppState` 加 `SteamHost{players}` / `SteamJoin`；`Game` 加 `steam_host_ls` / `steam_cli_ls` / `steam_my_index`。
- `Game::new`：`SteamHost` → `SteamSession::init(APP_ID=908660, virtual_port=1337)` + `host_create_lobby` + `prepare_transport`(listen) + `HostLockstep<SteamTransport>`；
  `SteamJoin` → `init` + `client_find_and_join` + `prepare_transport`(connect) + `ClientLockstep<SteamTransport>`。
- `update.Fighting` 加 Steam 分支：host 产帧步世界（乐观预测关，严格 lockstep + 快照）；client 上行 + 严格按权威帧推进。
- `self_index()` 用 `steam_my_index`；CLI 支持 `--steam-host [--players N]` / `--steam-join`。
- 编译：`cargo build -p client --features client/steam` + feature clippy 全绿；默认（无 feature）build/test/clippy 全绿（109）。
- ⚠ 运行期前置（已在本机验证）：Steam 客户端登录 + **`steam_api64.dll` 必须与 `client.exe` 同在 `target\debug\`**（放错误位置会进程无输出直接退，exit 1）。
  `steam_appid.txt` 也放到 exe 旁边（`init_app` 已强制 AppID 908660 作双保险）。
  本机 `client.exe --steam-host` 已跑通：`[steam-host] lobby=<id>, my slot=0`（建厅成功）。
- **本机单账号也验证 join 自动搜厅跑通**：`--steam-join` init + request_lobby_list + 加入同账号 host 大厅成功（`[steam-join] lobby=<id>`；`my slot=0` 是同账号伪象，双机不同账号时 client=槽1）。
- fallback：`--steam-join <lobby_id>` 手动用 host 打印的 LobbyId 加入（`SteamSession::join_lobby_by_id`，用 `LobbyId::from_raw`）。
- **辅助脚本 `run-steam.ps1`**：`run-steam -Mode host [-Players N]` / `run-steam -Mode join`——自动 `--features client/steam` 构建 + 把 `steam_api64.dll`/`steam_appid.txt` 拷到 exe 旁再启动（双机各跑一个）。
- 待双机：host 一台 `run-steam.ps1 -Mode host`，client 另一台 `run-steam.ps1 -Mode join`（各登不同账号，自动搜加 matchkey 大厅）。
- 待双机：host 一台 `client --steam-host`，client 另一台 `client --steam-join`（各登不同账号，自动按 matchkey 搜加大厅）。

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
cargo test  --workspace                # 回归（111 全绿基线）
cargo clippy --workspace -- -D warnings
cargo run -p client -- --solo          # 单机试验场
powershell -File multi-launch.ps1 -Players 3   # 局域网多开（-Fast 加速局终看多局；可手动停窗看重连）
powershell -File run-steam.ps1 -Mode menu       # Steam 版主菜单（按 3 进大厅，H 建厅 / J 自动加入）
powershell -File run-steam.ps1 -Mode host -Players 2   # host 建厅
powershell -File run-steam.ps1 -Mode join        # client 自动按 matchkey 加入（无需输房间号）
powershell -File check.ps1             # 一键 build+test+clippy
```

## ⚠ 环境大坑（务必再读）
git 仓库根在**上级 `testingLL/`**，`rust_remake/` 只是子目录。`core.hooksPath` 用绝对路径指到 `rust_remake/.githooks`；pre-commit 用 `$0` 定位项目根。已端到端验证：坏代码拦得住、干净提交放行。
