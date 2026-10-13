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
| **E5** | **R7e 学习期暂停**（host 权威冻结 `learn_remaining`） | ⏸ 待做 |

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

**E. 学习期·房主掉线**
| # | 步骤 | 预期 |
|---|---|---|
| E1 | host 在学习期被杀进程/断网 | client 约 3s 后出现「房主已离开」覆盖层；可 Esc/Q 关闭回菜单 |
> 关键字：`[steam-client] host silent in learning for 180 ticks -> host-left overlay`。

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
| R7e | **做**，排最后（E5）|
