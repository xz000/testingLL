> **历史归档（2026-10-17）**：E1–E3/E5 已完成并验收；后续迁移专项见 `HOST_MIGRATION_LEARNING_PLAN.md`。

# 学习期联机 设计规划（LEARNING_PHASE_NET_PLAN.md）

> 目标：让**学习期（`MatchPhase::Learning`，首局配置/商店）**也支持**离开**、**掉线检测**、**重连/归队**，并承接 **R7e 学习期暂停**。
> 关联：`EXIT_MENU_RECONNECT_PLAN.md`（R7/R7e）· `LEAVE_REJOIN_PLAN.md`。
> **状态：规划已定稿（2026-10-13），未改代码。**

---

## 0. 现状审计（代码事实）
- 学习期 = `MatchPhase::Learning`：`begin_first_round_config()` 进入，`learn_remaining = config.first_round_time_secs`（**默认 40s**），`tick_learning(dt)` 归零 → `enter_first_round()` → `Fighting`；**每轮之间也有一次 Learning**（买技能）。
- 联机的**所有网络推进只在 `MatchPhase::Fighting` 分支**（`main.rs:6961+`）：host 产帧/快照/`auto_drop_idle`、client 上行/`step_frame`/掉线计数、`host.poll`——**学习期分支（`main.rs:6854`）一律不跑**。

### 0.1 由此产生的问题（已确认）
1. **无快照** → 归队握手 `steam_rejoin_handshaking` 等不到 `Snapshot` → 15s 超时 ⇒ **学习期无法重连**。
2. **无掉线判定** → 学习期掉线/退出不被发现（`auto_drop_idle`/client `stale` 都不跑）。
3. **退出会卡住别人**：进入 Fighting 前 host 走 `HostGather` 需收齐**所有参与者配置**（`all_cfgs`）；退出者永不发 `PlayerCfg` → host 干等 → **全员卡在开局**。
4. 连接在学习期**完全空闲**（无帧），有被 Steam 判空闲的风险。

### 0.2 记分现状（本次核实）
- 上报入口**唯一**：客户端到达 `MatchPhase::Finished` 时的 `steam_record_match_result(...)`（`main.rs:6839`），**各端各自上报本机战绩、无中心裁决**。
- ⇒ **局在打完前散了 → 无人到 `Finished` → 无人上报 = 天然"不计分"**（无需代码）。
- 代码齐全（`net-steam/src/stats.rs` + 上报/成就/排行榜），但被 `appid::stats_enabled() = !IS_DEMO` 门控（**demo 直接早退**），且排行榜需 Steamworks 后台配置 ⇒ **现无生效天梯**。

---

## 1. 设计（已定）

### 1.1 学习期需要"轻量网络节拍"（心跳 + 静态快照）
World 学习期**不步进**，不能靠"帧"做心跳。做法：**把 Fighting 的网络节拍搬到 Learning 也跑一遍**（**同节奏**，不新增频率常量）：
- **host**：每帧 `poll(rcv)`（收 `RoomState` 心跳 / `Leave` / `ReconnectReq` / `PlayerCfg`）+ 沿用 `SNAPSHOT_EVERY=30` 发**静态快照**（world+meta，`seq = next_seq` 不变）+ 在线集广播（含周期兜底）。
- **client**：每帧上行 `RoomState`（心跳/在场）+ `step_frame`/收快照 + **学习期存活计数**。
- **存活判定**：沿用**现有 3s(180 帧) stale 阈值**不变；心跳是**自动**的，与"是否在挑技能"无关，**无需放宽**。
  - ⚠ 学习期**没有帧**，故存活依据应是"**收到 host 心跳/快照**"，不能沿用"收不到**帧**就 stale"。

### 1.2 归队要按 `meta.phase` 恢复**阶段**
`apply_snapshot_world` 已恢复 world+meta。归队后据 `meta.phase`：
- `Learning` → 把 UI 置回学习/配置态（`pre_game_config` 等），沿用 `meta.learn_remaining`；
- `Fighting` → 现状。
> 现归队握手隐含假设"恢复即进 Fighting"，学习期归队会错位。

### 1.3 学习期掉线/离开 = **恒桩子（保留槽位，可归队）**
- host 学习期也处理 `Packet::Leave` 与 `auto_drop_idle`（阈值同 3s）；
- 被 `mark_dropped` 者按现有 `all_cfgs` 语义**排除** ⇒ host 能收齐其余人 → **正常开局、不卡局**；
- **世界玩家数/索引不变**（帧同步红线）：离开者**整局恒为桩子**，可随时按 id 归队。

### 1.4 恒桩子 + 记分（**不做投降**）
- **放弃"投降"功能**：玩家想走就走（留桩子）；避免新包/新标志/协议变更与中途重排的复杂度（删槽位=按 id 全量重编号，风险极大）。
- **记分自然成立**：恒桩子会被打死 → **自然末位/判负**；若只剩一个真人 → 清桩即**判胜**。**零特判**。
- **局散不计分**：最后一人退出 → 无人到 `Finished` → 无人上报（见 §5）。

### 1.5 学习期暂停（R7e）
- host 权威冻结 `learn_remaining`（随心跳/快照广播 `paused` + 恢复时下发剩余时间）；各端 `tick_learning` 暂停期间不推进 `dt`。
- 复用 R7 的仲裁（`PauseReq`/`PauseState`），但需把处理从"仅 Fighting 循环"挪到"学习期 tick 也处理"。
- 判断：E1（心跳）落地后**不难**，排最后做。

---

## 2. 改动点（预估）
| 模块 | 改动 |
|---|---|
| `client/src/main.rs` `MatchPhase::Learning` | 新增**网络节拍**：host（poll + 周期静态快照 + auto_drop + 处理 Leave/ReconnectReq + 在线集）、client（心跳上行 + 收快照 + 学习期存活计数）|
| 归队 | `apply_snapshot_world` 后按 `meta.phase` 置 UI 态（学习/战斗）|
| R7e | 学习期暂停字段（随快照/心跳同步）+ 复用暂停仲裁 |
| 协议 | 复用现有 `Snapshot`/`Participants`/`Leave`/`RoomState`/`PauseState` ⇒ **可能不升**；R7e 若加字段/包则升版本 |

---

## 3. 分阶段实施（每步 1 commit）
| 步 | 内容 | 状态 |
|---|---|---|
| **E1** | **学习期心跳 + 静态快照**：host 每帧 poll + 静态快照（`seq` 不变）+ 在线集 + auto_drop；client 心跳上行 + `pump_learning` 收包 + 学习期存活判定 + 归队握手 | ✅ `522725b`+`c46ac50` |
| **E2** | **归队按阶段恢复**：`apply_snapshot_world` 后据 `meta.phase` 置 `pre_game_config`（主循环/迁移/学习期三处）| ✅ `8a886bb` |
| **E3** | **学习期掉线/离开**：host `poll` + `auto_drop_idle`；被 drop 者由 `all_cfgs` 排除 → **不卡局**；恒桩子、可归队 | ✅（由 E1 覆盖，`8a886bb`）|
| **E5** | **R7e 学习期暂停**（host 权威冻结 `learn_remaining`） | ✅ **已完成**（`7478cf7`；学习期接入暂停协议+冻结倒计时+屏蔽输入） |

> 说明：原 **E4（人数重排）已删除**——重排会破坏归队（槽位没了）且需按 id 全量重编号，**改为一律恒桩子**。
>
> **E5 设计要点**（待拍板）：学习期无帧，R7 的暂停仲裁是“帧驱动”的，直接复用不顺。最简可行：host 当唯一权威 —自持 `learn_paused`，各端在暂停期间**不推进 `tick_learning`**；用**静态快照里的 `meta.learn_remaining`** 对齐（天然携带 host 剩余时间）。需定：暂停入口（复用 R7b 菜单项？）、是否带额度/确认、频率（学习期快照广播从 2.5s 提到 0.5s 以免倒计时跳变）。

---

## 4. 测试

### 4.1 单测
- 学习期快照往返（`meta.phase==Learning` + `learn_remaining` 保真）；`all_cfgs` 排除 dropped；（R7e）暂停冻结。
- 已落：`pump_learning_caches_snapshot_and_reports_liveness`（`net`）。

### 4.2 真机验收清单（E1–E3 + 离开/归队/desync 回归；**最多 3 账号**）
> 前置：**各端都是最新构建（协议 58）**（混版本会判版本不符）；demo 无天梯/成就（不影响测试）。

**A. 基础回归（先确认没退化）**
| # | 账号/步骤 | 预期 |
|---|---|---|
| A1 | host + 2 client 正常开一局（学习期买技能→开打→打完一回合） | 正常；三端**同时**进战斗；无 `DESYNC` |
| A2 | 战斗中某端 Esc→离开 → 再按 **Y** 归队 | 归队成功、控制的是**自己**、无 desync、"已离开"标记消失 |

**B. 学习期·崩溃重开归队（R5c 在学习期，重点）**
| # | 步骤 | 预期 |
|---|---|---|
| B1 | host+A+B 进**学习期**；**A 用任务管理器杀进程** | A 那台重启→主菜单出现"未结束的上一局"提示 |
| B2 | A 按 **Y** | 归队后**回到学习期**（商店可见、有剩余时间）；房主/B 看到 A"已离开"标记 ≤2s 消失 |
> 关键字：`[steam-rejoin] resumed in learning (phase=Learning)`、无 `DESYNC`。

**C. 学习期·主动离开 → 归队**
| # | 步骤 | 预期 |
|---|---|---|
| C1 | A 在学习期 Esc→返回主菜单 | 别人**立即**看到 A"已离开"；**不卡住** |
| C2 | A 回菜单按 **Y** | 归队回学习期 |

**D. 学习期·离开且不回来（E3 关键：验证不卡局）**
| # | 步骤 | 预期 |
|---|---|---|
| D1 | host+A+B；A 学习期离开且**不归队**（或杀进程） | 倒计时结束后 host+B **照常进入战斗**（A 为**站桩**）；**无人卡住** |
| D2 | 同上但 **3 人局**：A 离开，host+B+C 继续 | 剩余三人正常开局 |
> 关键字：`[steam-host] AUTO-DROP client N (learning idle) -> continue`。

**E. 学习期·房主掉线/退出 → 主机迁移（已优化，`4082ae2`）**
| # | 步骤 | 预期 |
|---|---|---|
| E1 | host 在学习期被杀进程/断网/主动退出游戏 | 其余端约 3s（或收到 `Leave` 立即）进入**迁移**：选举新 host 接管，**本局继续**（商店/倒计时照常，不再弹覆盖层结束）|
> 关键字：`[steam-client] host silent in learning for 180 ticks -> migration` → `elected new host` / `TAKEOVER`。
> 实现：学习期 client 掉线判定后置 `steam_migrating`，复用**对战期同一迁移状态机**（`poll_steam_migration` + `steam_do_takeover`，阶段无关：学习期也有缓存快照）；主动“退出游戏”也会先广播 `Leave`。

**F. 过渡一致性（学习期→战斗）**
| # | 步骤 | 预期 |
|---|---|---|
| F1 | 正常过渡 | 三端**同时**开始战斗；无 `DESYNC` |
| F2 | 有人学习期离开后过渡 | 剩余端一致开始；离开者站桩 |

### 4.3 重点观察
1. **归队回学习期**：商店是否真的显示、能否买、剩余时间是否 ≈ 房主（剩余时间**仅在归队那一刻**从快照同步一次，之后各自本地倒数，轻微偏差正常）；
2. **卡死**：日志里有没有**反复** `NO frames ... -> entering reconnect-probe`（老死循环）；
3. **desync**：日志里有没有 `[steam-client] DESYNC at seq=`；
4. **标记**：`已离开` 是否 ≤2s 消失。

### 4.4 出问题要提供
- **所有端**对应时间的 `logs/console-menu-full-*.log`（尤其**房主**那台）；
- 异常**时间点**与当时**操作**；
- 关键字：`DESYNC` · `steam-rejoin` · `AUTO-DROP` · `host silent` · `NO frames`。

---

## 5. 已知边界（记录，不特殊处理）
1. **全员退出 = 本局解散**：归队需要**活着的权威端（host）**；若所有人都退出（最后一次退出会离开大厅、无人在线可选新 host）→ 掉线者归队时 `join` 失败或握手超时 → 判"**上一局已结束**"并清会话。**谁也连不回**。
   - 能连回的情形：掉线但进程仍在（权威端活着），或**还有别人在**（已是/仍是 host）。
2. **解散的局不计分**：无人到 `Finished` → 无人上报（见 §0.2）。

---

## 6. 决策记录（已定）
| 项 | 决定 |
|---|---|
| 投降功能 | **不做**（角色**整局恒为桩子**） |
| 中途删槽位 / 人数重排 | **不做**（风险大、破坏归队） |
| 学习期掉线阈值 | **与对战期一致（3s）**，不需放宽 |
| 学习期频率 | **与对战期同节奏**（每帧 poll/心跳 + `SNAPSHOT_EVERY` 快照），不新增常量 |
| 归队恢复 | 按 `meta.phase`（可能回学习期） |
| 记分 | 走现状：打完才结算；局散不计分；桩子自然末位/剩者胜 |
| R7e | ✅ **已完成**（`7478cf7`；见 §1.5）|

---

## 7. 实测发现的 bug 与修复（2026-10-13，3 账号真机）

> E1–E3 落地后经真机反复测试，暴露并修复了多条**既有 + 新引入**的问题。存档便于回溯。

| # | 现象 | 根因 | 修复（commit）|
|---|---|---|---|
| 1 | 房间里 host/client2 见 3 人、**先加入的 client 只见 2 人** | `steam_refresh_roster` 只在 `steam_host_ls` 存在时跑（**仅 host** 刷新）；client 进房取一次名单快照后不更新 | client 也刷新名单（用房主 SteamID 定槽位）· `8c6ea6d` |
| 2 | **卡在准备界面**（host 一直 `present_clients=1/2`） | `LobbyPlayerTable` 按 **SteamID 升序**排槽 → 后加入者（SteamID 更小）把已有玩家槽位**挤后**；各端 `steam_my_index` 进房后不更新 → **两名 client 撞同一 index** | roster 变化时**重算本端索引**并同步 `cli.set_my_index` · `fe11064` |
| 3 | 进首轮学习**界面空白**（房间列表已消失、商店没出） | `draw_meta_overlay` 的 `Learning` 分支 `profiles.find(player_id==self_index())` 失败即 `return`；**早加入者** world/profiles 按进房时人数建，后被挤后 `self_index` 越界 | 进首局配置按当前人数**重建 world** + 学习界面**兜底**（不空白）· `22c65f5` |
| 4 | 学习期退出**再归队丢购买技能** | 配置同步开始时 host `reset_cfgs()` 把离开者学习期发来的 `PlayerCfg` 一并清掉，而它已 drop、**无法重发** | `reset_cfgs` **保留掉线端** cfg · `cc6d15c`；+ 学习期 cfg **一变就上报**（崩溃/掉线也保住）· `9b0cf07` |
| 5 | **「已离开」标记多机不一致** | 配置同步广播**全量**列表，client 收到后把 `steam_online` 也设全量（=无标记）；而 host 的 `steam_online` 是**在线子集**（=有标记） | 配置同步**只设 `steam_participants`** + host **紧跟广播在线子集** · `227c79f`；host 改用同源 `participant_ids` · `cc6d15c` |
| 6 | 学习期**房主退出→迁移接管建成 1 人局**（`TAKEOVER player 0/1`）+ 对端 `Phase-B STALL` 回菜单 | roster 兜底填 `steam_participants` 写在 `if changed` **内**，而进房时 `steam_roster` 已填好 → `changed=false` 跳过填充 → `steam_participants` 空 | 兜底填充**移出 `if changed`** · `3cde649` |

**相关能力/提示：**
- 学习期 host 掉线/退出 → **走主机迁移继续本局**（不再覆盖层结束）· `4082ae2`；主动退出也先广播 `Leave`。
- 学习期购买**实时上报**（配置一变即 `send_cfg`）· `9b0cf07`。
- 排查用临时诊断 `marker-diag`（`2e9061d`）**已移除**（`093d870`）。

---

## 8. 【待办·规划】学习期房主退出 × 购买保真 / 迁移一致性

> 2026-10-14 真机发现（E1–E3 后）。**未改代码**。

### 8.1 现象
1. **学习期房主退出后，剩余两机“学习期结束→进入对局”的时间不一致**（不是同时开战）。
2. **房主退出前，三机买的技能都未装备**（不只离开者——连一直没离开的也丢了）。
3. 房主在对局期连回来成功，但**它退出前买的技能也没了**。
4. **房主的角色没有标“已离开”**。

### 8.2 设计层根因（已核实）
- **学习期的“购买”只存在于各端本地的 `meta.profiles[i]`**（`profile.purchase_skill` / `buy_item`，`main.rs:2132/2432`），**并没有被应用到 host 的权威 `world`/`meta`**。
- 现有两条“去向”：① 本地 `meta.profiles`；② host 的 `host.cfgs` **缓冲区**（靠 `9b0cf07` 的实时 `send_cfg` / 配置同步收集）——**到配置同步才 `apply_player_cfgs`**。
- 因此：
  - E1 的**静态快照包的是 `world+meta`**（`pack_snapshot`）：host 的 `meta` 里**只有它自己的**购买（因为客户的 cfg 只在 `cfgs` 缓冲，未应用）→ **迁移接管从快照恢复 ⇒ 其他端购买丢**。
  - `HostLockstep::takeover` 是**新建 HostLockstep**，**不携带旧 host 的 `cfgs`** ⇒ 旧 host 一走，它缓冲的（含旧 host 自己的）购买也没了。
  - 旧 host 若从“新 host 的快照”恢复得早，快照里也未必有它的购买 → 归队端也丢。

### 8.3 待核实假设（file:line）
- **H1（主）**：host 未把收到的 cfg 应用到 `meta/world` ⇒ 快照缺别人购买（已很可信）。
- **H2**：`steam_do_takeover` 用 `newer_snapshot(cached, own)`；学习期两者 `seq` 都是 0，**并列时取谁**会影响新 host 是否保留自己的购买 → 看 `net::lockstep::newer_snapshot` 并列规则（`steam.rs` 接管段）。
- **H3（时间不一致）**：迁移后新 host 是否真的进 `HostGather`、旧/新学习倒计时是否对齐（`meta.learn_remaining` 来自快照）、`pre_game_config/stage_first` 是否正确 → 看 `main.rs:7080-7100` 切换 + `7136+` HostGather。
- **H4（无标记）**：迁移后新 host 广播的“在线子集”是否排除旧 host、以及配置同步的全量广播是否把它又盖回去（`mark_online_dirty` 时序）→ `main.rs:7283+`。
- **H5**：客户端 `steam_last_sent_cfg` 缓存（`9b0cf07`）在**迁移后不会重发**（因为内容没变）——但 `ClientWait` 应无条件重发，需确认迁移后是否真的进入 ClientWait。

### 8.4 修复方向（荐：让学习期也“host 权威”）
- **把 host 收到的每个 `PlayerCfg` 立即应用到 `self.meta.profiles[i]` + `world.players[i]`**（相当于学习期就做 `apply_player_cfgs` 的**单端增量版**）。
  - 好处：**静态快照天然带上所有人的购买** ⇒ 迁移/接管/归队都保；旧 host 的购买也在（它本就在 host 的 meta 里）；配置同步仍做最终对齐（冗余但安全）。
  - 代价：新增“增量应用单个 cfg”的函数（复用 `apply_player_cfgs` 的 `progress::PlayerConfig::apply_to`）；注意**不要**重设世界/重生（学习期 world 静止）。
- 备选：`HostLockstep::takeover` 携带 `cfgs`（只解决“旧 host 是 authority 但只缓冲未应用”的**自己**那部分，**别人的仍缺**）——不够。

### 8.5 你提的“对局期心跳”
- 如果指的是“让周期心跳/快照带上**每人当前的购买/配置**”——那就是 **H1 的解决方案**（把 cfg 应用进权威 `meta`，快照自然携带）；**不需要新包/升协议**（复用现有快照的 `meta`）。
- 如果指的是“另发一个 cfg 心跳包”—— **不推荐**，能入 `meta` 就不必新包。

### 8.6 分阶段（已定）
- **M1 核实 H1–H5 + P3/P4**（看两次接管日志、`newer_snapshot`、迁移后是否进 `ClientWait`/配置同步是否跑）。
- **M2 host 学习期增量应用 cfg（核心）**：host 收到 `PlayerCfg` 就 `apply_to(meta.profiles[id])`（不重建/重生）+ **每次应用就广播一份静态快照**（P2）。
- **M3 首局配置同步“种子保留”（P1）**：以迁移后恢复的 meta 为种子，重建后按 `player_id` 保留未覆盖 profile，再叠 `PlayerCfgAll`。
- **M4 迁移后一致性（P3/P4）**：倒计时/`net_cfg` 对齐、旧 host 标记（接管后置 `online_dirty`）、确保迁移后进 `ClientWait` 无条件重发 cfg。
- **M5 真机回归**（3 账号：房主学习期退出→其余**同时**开战、**三人购买+金币全保**；房主归队带购买；旧 host 标“已离开”；无 `DESYNC`）。

### 8.7 金币余额（用户关切）——已有，无需额外做
- **已包含**：`PlayerConfig` 带 `gold` + `gold_spent`（`progress.rs:47-52`；文件头注释即声明“否则各端 `profiles[].gold` 会分叉、锁步破裂”）；`MatchState` 的 `write_profile` 也序列化 `gold`（`meta.rs:1168`）。`local_player_cfg()` = `PlayerConfig::from_profile`，`apply_to` 写回 `gold`。
- ⇒ **§8.4 的“host 增量应用 cfg”会同时同步金币**进 host 的 `meta` → 快照带金币 → 迁移/归队金币都保。**无需额外字段/包**。
- 回合奖励（参与/击杀奖）是**帧同步后确定性发放**，各端算出一致；唯一“自算”的是学习期花费，经 cfg 上报后由各端采纳 → 一致。

### 8.8 决策（已定，2026-10-14）
1. **认可 §8.4 荐方案**：学习期也 host 权威（增量应用 cfg）。
2. **保真范围**：离开者 / 归队者 / **旧 host** 的**购买 + 金币**都要保（要给旧 host 归队机会）。
3. **开战前不做对齐校验**：`StateHash` 从**首帧**就在跑，第一时间能抓到分叉，再校一次是冗余。

### 8.9 迁移后“配置对齐”失败路径与对策（核心）
迁移后有**两个对齐检查点**：① 迁移当刻（新 host 从缓存快照恢复、且其余端 adopt）；② 开战前**配置同步**（`PlayerCfgAll`，最终收敛点）。失败路径：

| # | 失败路径 | 对策 |
|---|---|---|
| **P1** | 首局配置同步会 `stage_world_for_participants`**重建 world+meta（重置 profiles）**，而 `PlayerCfgAll` **只来自活跃 client**——**旧 host 的 cfg 谁也收不到** ⇒ 重建后它的 profile 被重置 → 购买/金币丢 | 配置同步首局**不从零重建**：**以“迁移后恢复的 meta”为种子**，重建后**按 `player_id` 保留当前已有的 profile**，再叠加 `PlayerCfgAll` 覆盖活跃端。迁移后各端 adopt 同一份快照 ⇒ 种子一致 ⇒ 旧 host（及其它 drop 者）保留且全校一致 |
| **P2** | 缓存的旧 host 快照**不新鲜/非聚合** | §8.4：host 应用 cfg + **每次应用就广播快照** ⇒ 缓存=聚合且新鲜 |
| **P3** | 迁移后配置同步**没正常跑/没门槛** → 各按自己倒数开战 | 修迁移后倒计时/`net_cfg`/`stage_first`（M1 核实） |
| **P4** | 某 client 迁移后**不重发 cfg**（`9b0cf07` 的“变了才发”缓存） | 确认迁移后进 `ClientWait` 并**无条件重发**；或迁移后清 `steam_last_sent_cfg` 缓存强制重发 |

> **不**做“开战前对齐校验”（决策 8.8-3）；一致性由 ① 聚合快照（迁移基线）+ ② 配置同步（收敛）+ ③ 开战首帧起 `StateHash`（检测）保证。
