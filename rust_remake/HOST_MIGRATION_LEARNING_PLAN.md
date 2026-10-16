# 学习期房主退出 / 主机迁移 —— 规划（HOST_MIGRATION_LEARNING_PLAN）

> 起因：2026-10-14 真机反复出现"房主退出→重开→连回"迁移异常，且近两次修复引入了新回归。
> 本文件先**停止打补丁**，把不变量、已确证故障、根因与分步方案写清楚，再按步实施。

---

## 0. 结论先行

- **本轮新问题是回归**：`elect_new_host` 在 owner 不可用时"回退到在线集最小 SteamID"（commit `c06b296`）
  → 非 owner 端**自选举** → 接管 → 立刻被 `should_self_fence` 判定"owner 不是我" → 退位转客户端
  → 与此同时原新 host 收到 `SUPERSEDED` 也退位 → **双退位、无 host** → 归队握手超时 → 回主菜单。
  **处置：已撤回该回退（恢复"owner 不可用就不猜，返回 0"的原保守语义）。**
- **仍待解的真问题**：`[steam-client] host broadcast Leave -> immediate migration` 的**来源不明**
  （按代码只应来自 `self.host`，但当时"host"是否指对需诊断坐实）。以及 Steam **owner 未移交**时如何安全迁移。

---

## 1. 范围

- 学习期（首局配置 / 局间配置）+ 对局期的**房主退出 → 迁移**；含主动离开、崩溃、重开归队。
- 不含：R7e 学习期暂停（E5 暂缓）。

---

## 2. 不变量（Invariants，违反即为 bug）

- **I1 唯一权威**：同一时刻最多一个 host；由 Steam lobby owner 仲裁。
- **I2 host == lobby owner**：否则该端必须自栅栏退位（`should_self_fence`）。
- **I3 迁移后一致**：各端 `world` / `meta` / 玩家索引一致（否则 DESYNC）。
- **I4 参与者列表 = 世界人数**：`steam_participants.len() == world.players.len()`（含掉线占位）。
- **I5 只认当前 host**：`Leave` / `Takeover` / `Snapshot` 仅当 `from == self.host` 才被采纳。
- **I6 可回退**：迁移失败 → 回主菜单但**保留会话**（可再归队）。

---

## 3. 已确证的故障（附真机日志证据）

| 编号 | 现象 | 证据 | 状态 |
|---|---|---|---|
| F1 | 学习期 `host silent in learning for 180 ticks` 误判（快照仍在到达） | `logs_2@214324` | 观察中（诊断已加） |
| F2 | `cannot determine a valid new host (lobby_owner=旧host) ... returning to menu` | `logs_2@214324` L1556 | 原设计保守，**勿改** |
| F3 | `Phase-B STALL: 收不到新 host 的 Takeover（600 帧）` | `logs_2@221914` | 已修 `ae8e551`（补齐参与列表） |
| F4 | 自选举→接管→自栅栏退位→归队失败连锁 | `logs_2@222651` L135-143 / `logs_3@222710` L144-148 | 已撤回 `c06b296` 的回退 |
| F5 | `host broadcast Leave -> immediate migration` 来源不明 | `logs_2@222651` L~134 | **待诊断** |

F3 修复已确证生效：`logs_3@222710` 出现
`TAKEOVER 补齐参与列表（世界 3 人）：[43985466, 63417] -> [43985466, 63417, 94062]`、`player 1/3`。

---

## 4. 根因分析

- **R1 参与者列表生命周期混乱**：`steam_participants` 在**配置同步前**靠大厅 roster 兜底（可能不全：
  后加入者尚未出现在本端看到的 lobby member 里），配置同步时才由 host 广播的 `PlayerCfgAll.participants` 变权威。
  → 学习期迁移会用不完整的列表（F3）。
- **R2 owner 移交与 host 迁移不同步**：旧 host 非正常退出（崩溃/强杀）时 Steam **不一定移交** lobby owner，
  它仍指向已掉线的旧 host → 按 owner 仲裁就"无合法新 host"（F2）。**不能靠猜**（会 F4）。
- **R3 `Leave` 语义脆弱**：host 广播 `Leave` 让 client **跳过 3s stale + probe 立即迁移**。
  一旦 `self.host` 指针不指向真正的权威（迁移/归队后短暂错位），就会**误触发**迁移（F5）。
- **R4 状态机交织**：迁移 phase A/B、配置同步（ClientWait/HostGather）、学习倒计时、rejoin 握手
  各自有超时与重试，缺少"单一权威源"，产生连锁反应。

---

## 5. 建议方案（按"先诊断、后根治"）

### M1 诊断先行（低风险）
- `host broadcast Leave -> immediate migration` 增加**发送者 id** 与 `self.host`：
  `Leave from {sender} (my host={host})`。
- `NO frames` / `host silent` 触发时补打 `self.host`、最近一次收到包的来源与 tick。
- 目的：坐实 F5（是旧 host 仍在发 Leave？还是 `self.host` 指错？），并决定 M3 细节。

### M2 单一参与者来源（修 R1/F3 根）
- host 在**学习期**除广播在线子集（`online_ids`，用于"已离开"标记）外，**同时广播全量 `participant_ids`**；
  client 收到即更新 `steam_participants`（学习期无帧推进，更新+重建世界在各端一致）。
- 移除/收紧"仅当 `steam_participants` 为空才用 roster 兜底"的逻辑——roster 只用于**大厅 UI**，不再充当对局名单。
- 与 `ae8e551` 的接管补齐形成双保险（补齐作为最后兜底，正常路径不再需要）。

### M3 选举/接管（修 R2/F4，D1/D2：**取消共识，只认 Steam owner**）

> 依据：Steam 在 owner 离开大厅后**自动移交** owner（`session.rs:101` 注释已如此声明）。
> 因此唯一权威 = `lobby_owner`，**不需要共识**。

**规则（按优先级）：**
1. **owner ∈ `steam_online`（在线）→ owner 即合法 host**：本端**重定向/继续等待 owner**，**不选举**。
   （F5 cascade 的正解：`lobby owner` 明明活着，却因 `owner == old_host` 被判无效 → 误选举。）
2. **owner 不在线 / 为 0** → 等待 Steam 移交：周期重查 `lobby_owner`，上限 **10s（D1）**；一旦移交 → 回规则 1。
3. 10s 仍无合法 owner → 回主菜单（**保留会话**，可再试）—— 不猜测、不共识。
4. 接管后仍走 `should_self_fence`（owner 已移交为别人且在线 → 退位）；因取消了共识，
   不会出现“非 owner 自选举→接管即退位”的连锁。

**其它：**
- **退位/被取代路径不得广播 `Leave`**（仅用于“我主动离开本局”），避免诱导他人误迁移。
- 接管前校验：`owner == 本端` 才接管（owner 未移交则不接管）。

### M4 迁移后一致性断言（修 R3/I4）
- 迁移/接管完成后断言 `steam_participants.len() == world.players.len()`，不一致则以 host 侧为准修正并告警。
- 所有采纳点保持 `from == self.host`（I5）。

---

## 6. 分步与提交

1. **M0（已完成 `5b4ec99`）**：撤回 `elect_new_host` 回退回归。
2. **M1（已完成 `748f626`）**：诊断 —— `Leave`/`silent`/`NO frames` 补发送者(`self.host`) 与 leaver 身份。
3. **M2（已完成 `748f626`）**：新增运行期包 `ParticipantsAll`（TAG 23），host 学习期每 2s 广播**全量名单**；
   client 采纳维护 `steam_participants`；大厅 roster 只用于 UI（不再兜底对局名单）。
4. **M3（已完成 `e6ff182`）**：D1 `MIGRATE_NO_OWNER_BAIL_TICKS` 270→**600**（~10s）；选举保持**只认 owner、不共识**；
   确认退位/被取代路径**不发 Leave**（已核：`steam_begin_rejoin_as_client` 不发）。
5. **M4（已完成 `e6ff182`）**：迁移/恢复后 `log_migration_consistency` 自检（`participants != world.players` 即告警）；
   接管侧继续用 `ae8e551` 的补齐兜底。
6. **M5（已完成 `91c83f7`）**：修 `host_left` 跨迁移残留 —— `retarget_host` 清空 `host_left`；
   学习期也消费 `host_left`（与 Fighting 同口径，立即迁移）。真机 M1 日志坐实：旧 host 迁移前发的 `Leave`
   会在重定向后被当成“当前 host 离开”而误迁移。

## 6.1 真机验收结论（2026-10-14，三次 3 账号“房主退出→重开→连回”）

- ✅ **M1 生效**：`host broadcast Leave (leaver=..., sender/host=...)` 抓到 F5 真因（`host_left` 跨迁移残留）；
  修后 `sender/host == leaver` 始终一致，且学习期 Leave 走 `host broadcast Leave in learning ... -> immediate migration`。
- ✅ **M2 生效**：三端 `participants(full) <- host: [43985466, 63417, 94062]` 一致。
- ✅ 迁移链全面通过：选举=owner、`TAKEOVER ... player 1/3`（人数对）、含带进度快照的接管（`migrated to new host (seq=2314)`）；
  **`Phase-B STALL` / `cannot determine a valid new host` / `DESYNC` / `M4 不一致` / `panic` 均为 0**。
- ℹ️ 观察到一次 `learning snapshot from NON-host 43985466`（旧 host 被取代后短暂仍发快照），**已被 `from == self.host` 正确忽略**（见 §10）。

---

## 7. 真机测试矩阵

| 场景 | 期望 |
|---|---|
| 房主正常退出（Leave） | client 立即迁移；新 host = owner；无 DESYNC |
| 房主崩溃（无 Leave） | 3s stale → probe → owner 选举；成功 |
| 房主重开归队 | 归队成功；"已离开"标记消失；购买/金币保留 |
| owner 未移交（旧 host 强杀） | 等待重查 owner；超时才回菜单（**不**自选举） |
| 后加入者（配置同步前迁移） | 参与列表完整（=世界人数）；无 Phase-B STALL |
| 连续多次退出/归队 | 无自选举连锁、无双 host、无 DESYNC |
| 学习期购买 + 迁移 | 三人购买/金币全保 |

---

## 8. 决策（已定）

- **D1（定）**：owner 未移交的等待上限 = **10 秒**（`MIGRATE_NO_OWNER_BAIL_TICKS = 600`），超时回菜单（保留会话）。
- **D2（定，修正）**：**取消共识**。依据 Steam 在 owner 离开后**自动移交** owner（`session.rs:101`）；
  唯一权威 = `lobby_owner`。共识只会引入脑裂风险（真机已验：自选举→接管→自栅栏退位连锁），故不做。
- **关键修复**：`owner == old_host` **不等于** owner 失效——只要 **owner ∈ 在线集**，它就是合法 host（重定向/等待），
  这既修 F5 cascade，也让 F2 从“误选举”变为“等 Steam 移交”。

---

## 9. 状态：已完成（2026-10-14）

M0–M5 全部实施并真机验收通过；本工程为 **A（真值）+ B（在办）** 中现归为**已收尾的联机设计档**，
后续如再动学习期迁移请先更新本文档。

---

## 10. 后续可选：僵尸 host 退位加固（非阻塞，未做）

**现象**：旧 host 被取代后可能**短暂仍持有 host 身份并继续广播周期快照**（真机见
`learning snapshot from NON-host 43985466`）。窗口来自：①新 host 的 `notify_old_host_takeover` 是**一次性单播**、可能丢；
②Steam **owner 尚未移交**时旧 host 的 `should_self_fence`（只认 owner）不触发；③刚从对局退到菜单的状态切换瞬。

**为何无害**：各端**只认 `self.host` 的包**（不变量 I5）→ 旧 host 的快照/帧/Leave 全被丢弃；
旧 host 最终会因 `Takeover` 重发或 owner 移交而自栅栏退位；若已退到菜单则无 host 身份。

**可选加固**（将来若发现问题再做）：
1. ✅ **已做（`2ff05ee`）**：`notify_old_host_takeover` **重发** —— 接管时暂存通知，`poll` 尾部每 30 tick(0.5s) 重发一次、共 12 次(~6s)，
   提高到达率，促旧 host 尽快退位（`HostLockstep` 新增 `old_host_notice*` 字段 + `poll` 重发；net 单测 `notify_old_host_takeover_is_resent`）。
2. ❌ **已撤销（`253f1d9`）**：**旧 host 自判“被遗弃”** —— 曾用 `all_clients_dropped`+`deserted_ticks`
   在全客户端掉线 ~10s 后自退，但**用户体验不佳**（用户定调）：现改为 **host 继续单人对桩子打下去**，不自动退。
   真正的“被取代/P2P 隔离”仍由**自栅栏（按 `lobby_owner`）**处理，或用户自行 Esc 退。
   （`HostLockstep::all_clients_dropped`/`deserted_ticks` API 保留但不接主循环。）
3. 记录/限流：对“非当前 host”的包做丢弃计数，便于诊断僵尸窗口时长。

---

## 11. 网络隔离 owner 的迁移缺口（2026-10-14 真机新发现，**待规划/未开工**）

### 11.1 现象（日志）
- `cannot determine a valid new host (lobby_owner=43985466) after 600 ticks; returning to menu`（多台、多次）。
- 含义：**旧 host 网络被隔离（但并未退出大厅）时，Steam 的 `lobby_owner` 一直停在它身上、不会自动移交**；
  现有“只认 owner”的选举无解 → 10s 用尽 → 回菜单。
- 对比：旧 host **主动退出** 会 `leave_lobby`，Steam 随即移交 owner → 那条路径正常（已验证）。

### 11.2 关键未知：剩余成员能不能改 owner？
- SDK 有 `ISteamMatchmaking::SetLobbyOwner(lobby, newOwner)`：`steamworks::sys` 已 re-export（`lib.rs:10`），
  `SteamAPI_SteamMatchmaking_v009()` + `SteamAPI_ISteamMatchmaking_SetLobbyOwner(...)` 均可用（技术上**可调**）。
- 但 **调用权限/语义未验证**：非 owner 成员能否调？在当前 owner “在线但隔离”时能否成功？
- → **Step 0 小实验（3 账号）先验证**：非 owner 调 `SetLobbyOwner` 把 owner 改给某人，能否成功、`GetLobbyOwner` 是否立即更新。

### 11.3 你提的两个担心（均成立）
1. **共识出的新 host 过不了 owner 判定 → 自栅栏退位**：是的。`should_self_fence` 只认 `lobby_owner`，
   所以若共识 host ≠ Steam owner，它一接管就会被判“owner 不是我”而退位（这正是之前 cascade 的机制）。
   ⇒ 若走“不改 owner”的共识路线，**必须同时改自栅栏口径**（不能只看 owner）。
2. **旧 host 恢复重连后自认 owner**：是的。`host_steam_id()`（`session.rs:1066`）= 玩家表槽0 / `lobby_owner`；
   旧 host 一回来就会把自己当 host。⇒ 需要**恢复协议**（见 11.4 P2）。

### 11.4 两条路线（Step 0 实测结果：**P1 ❌ 不可行**）
- **P1（`SetLobbyOwner` 可用）— ❌ 排除**：实测非 owner 调用 `SetLobbyOwner` 返回 **`ok=false`、owner 不变**
  （`logs_2/3`：`F6 SetLobbyOwner(me=...) ok=false owner_after=<旧owner>`）→ **剩余成员改不了 owner**。
  且 `[netdiag]` 显示房主断网后 owner 也**不变**（Steam 不因“隔离”而移交）→ 只能走 P2。
- **P2（`SetLobbyOwner` 不可用/不可靠）— 采用**：见 §12。

### 11.5 决策点（待你拍板）
- **Step 0**：✅ 已做，结论 **P1 不可行**（`ok=false`）。
- 共识函数：建议 “**`participants`（权威顺序）里最靠前的、仍在线的**者”（天然各端一致）。
- 重连端“默认 client + 读当前 host id”恢复协议：待确认。

---

## 12. P2 方案：基于**大厅聊天（服务端转发）**的 host epoch + 共识（规划中，未开工）

### 12.1 为什么用大厅聊天
- Step 0 已证：**owner 不可由成员强制改**，且隔离时不自移交 → 必须**自建权威**。
- 真机日志有 `[netdiag] LobbyChatMsg recv: ... from=...` → **Steam 大厅聊天 = 服务端转发，不依赖 P2P**（P2P 隔离时仍可用）。
- 代码已有 `send_lobby_chat`（`session.rs`）；注释已预留“epoch 宣告走这条路径”。
- **大厅成员列表**（`lobby_members`）也是 Steam 服务端权威、各端一致 → 可作选举的“成员集”。

### 12.2 机制：host epoch + 确定性继任
- **宣告（可选/低频）**：host 每 ~1s 经大厅聊天发 `HOST <epoch> <hostId>`；各端记录**已知最大 epoch**。
  ⚠ 心跳**不是必需**：大厅阶段已有 **P2P 心跳**（`RosterReady`）用于判“host 沉默”；心跳仅用于**局部分区时的 epoch 消解**。**E1 一条都不发**。
- **丢主判定**：优先用**现有 P2P 心跳超时**（`steam_lobby_silent_ticks`，240 tick）；大厅聊天心跳仅作备用。
- **共识（确定性）**：继任者 = **当前大厅成员里 SteamID 最小、且不是旧 host 的那个**；各端算得一致，
  （若我就是→）我宣告 `HOST <epoch+1> <me>` 并成为权威；其余端采纳。
- **冲突**：**epoch 大者胜**；同 epoch → 取 id 小者。
- **恢复**：旧 host 回来读到 `HOST epoch' > 自忆 epoch`（或 host != me）→ **不再当 host**，作 client 加入。

### 12.3 对现有不变量/代码的影响
- 不变量 I1/I2 从“host = `lobby_owner`”改为“**host = 已知最大 epoch 的宣告者**”；`lobby_owner` 降为**提示**。
- `should_self_fence`：由“owner != me”改为“**已知 host epoch > 自忆 epoch 或 已知 host != me**” → 退位。
- 重连端**默认 client**，读（P2P 或大厅聊天）当前 host id 决定。
- 旧 host 的 `host_left_overlay`/自栅栏路径改为看 epoch。

### 12.4 分期（**先大厅层，不碰游戏内逻辑** —— 按你的建议）
- **E1（纯观察，零风险）**：在大厅/准备阶段实现 epoch 宣告 + 丢主检测 + 继任计算，**只打日志**
  （“我算出的继任者 = X，与本端成员表 = [...]”），**不改实际谁当 host**。目标：3 账号真机验证
  **“剩余成员对继任者的计算结果一致”**，以及大厅聊天在隔离时确实可达。
- **E2**：让 E1 的继任者**真的接管**（大厅准备阶段：接管倒计时/开始游戏权限）+ 旧 host 恢复后自动降为 client。
- **E3**：把同一套 epoch/共识接入**对局期**主机迁移（替换/兑底现有的 owner-选举路径）。

## 13. 大厅层接管（入口：`host_missing`；按你的建议）

### 13.1 入口
- 现状（`main.rs:8823`）：`host_missing`（`steam_lobby_silent_ticks` 超时）→ **直接弹“房主已离开”覆盖层** → 用户只能回菜单。
- 改为：进入**接管流程**（共识选举 + 抢 host）。

### 13.2 核心洞见
> **“主机”是本协议的逻辑角色，不需要 Steam owner 认可。**
> `lobby_owner` 只是“猜 host”的提示；只要各端对继任者一致，继任者就能当 host（用同一大厅、**不改 Steam owner**）。
> 所以“抢房主”是**纯软件层**的事，**无需 `SetLobbyOwner`**。

### 13.3 零消息共识
- `lobby_members`（Steam 服务端权威）各端一致 ⇒ `继任者 = min(成员 \ {旧host})`，各自算、必然一致。
- 只有“宣告/确认”才需发消息（大厅聊天，连发几次）。

### 13.4 流程
1. 各 client 算出 `successor`；
2. 我是 successor → `cli`→`host` 切换（大厅层 host，继续准备/开始），并发宣告；
3. 否则 → `retarget_host(successor)`，**继续等**（不再回菜单）；
4. 旧 host 恢复 → 发现自己是 owner 但别人已在当 host → **以 client 加入**。

### 13.5 分期
- **E1（观察）**：`host_missing` 处只计算并打印 successor + 成员表，**不改行为**。→ 验证三台一致。
- **E2**：真的接管（successor 变 host、其余 retarget）；覆盖层文案改“正在选举新主机…”。
- **E3**：加 epoch/宣告，处理**局部分区** + 旧 host 恢复冲突。

---

## 12.5 待确认（注：继任规则已定为“成员表最小 SteamID、非旧 host”）
- 是否先做 **E1（纯观察日志）**？
- 覆盖层是否保留作兼容（无其他成员时才回菜单）？

---

## 14. 【定案】Step 0 实测结论与采用路线（2026-10-15）

**实验（F6 抢 owner + 自动抢）结论（3 账号真机）**：
- 房主**在场**（哪怕你已收不到它、但它仍在大厅成员表）时：成员 `SetLobbyOwner(me)` **一律 `ok=false`** → **抢不到**。
- 房主被 Steam 移出大厅后（本次实测**约 40s**）：Steam **自动移交 owner** 给某在线成员；之后成员的 `SetLobbyOwner` 才 `ok=true`（但此时 Steam 已改好，抦不到便宜）。
- 旧房主恢复后会重新出现在 `members`，但 **owner 保持为新成员**。

→ 结论：**“抢 owner”比“等 Steam”无优势**，且不必自建共识（§12/§13 暂不需要）。

**采用 A 路线（已实施 `dead18c`）**：
- 大厅：`host_missing` → 覆盖层“正在等待 Steam 移交房主…”，**不再回菜单**；每帧查 `lobby_owner`：移交给我→**接管大厅 host**；给别人→**重定向**；**上限 ~60s**（`STEAM_LOBBY_HOST_TRANSFER_TIMEOUT_TICKS=3600`）。
- 对局期：`MIGRATE_NO_OWNER_BAIL_TICKS` **600→3600（~60s）**（不再 10s 就放弃）。
- 移除临时实验（F6/F7/自动抢/raw-bindings）；保留 `lobby_member_ids` helper。
- UX：覆盖层带倒计时提示；这是**慢路径（~40s）**，取决于 Steam 超时。

**若将来嫌慢**：再考虑 B（§12/§13 自建共识快路径）或 C（reform lobby）。

---

## 15. S 系列收尾（2026-10-15，真机验证通过）

在本轮基础上继续做了 **S1–S6**（按四轮真机反馈逐步修正），现已跑通：
「学习期房主断网 → 客户端等 Steam 移交 → 接管 → 倒计时冻结/恢复 → 配置同步 → 开局；
旧房主恢复 → 刷新大厅状态 → 自栅栏自动降级为 client 归队 → 画面正常 + 头像恢复；全程无 DESYNC」。

| 步骤 | commit | 内容 |
|---|---|---|
| S1 | `409a91d` | **撤掉**“只认 owner 才接管”双保险（判断不确定 + 有卡住合法接管的风险） |
| S2 | `409a91d` | 等 owner 移交上限 **60s → 120s**（`MIGRATE_NO_OWNER_BAIL_TICKS` 与 `STEAM_LOBBY_HOST_TRANSFER_TIMEOUT_TICKS` 均 7200） |
| S3 | `e31f171` | `SteamServersConnected/Disconnected` 回调 → `servers_online`（区分“我掉线”vs“对方掉线”） |
| S4 | `4eb33f1` | **学习期自动冻结**（`steam_learning_paused`）：迁移/归队/本端离线时冻结倒计时 |
| S5 | `4ab4abb` | **重连后刷新大厅状态** `RequestLobbyData`（raw sys；重连刷**一次**，无 burst） |
| S6 | `91ba76f` | 归队后按**权威参与顺序**重建 `steam_roster`（恢复角色头像） |

### 15.1 期间修掉的真机 bug
- **自栅栏失效**：`steam_net_ticks` 仅在大厅/Fighting 递增（学习期不增）→ `%30==1` 不命中；改为**每帧全局单次**递增。
- **对局中头像/ping 不刷新（本会话引入的回归）**：全局递增后 `steam_refresh_network_info` 内部**又递增一次** → 检查时恒为偶数 → 函数永不执行。去掉其内部递增，保留全局单一计数源（`35b572d`）。
- **归队后画面冻结**：归队快照把阶段切到 `Fighting`，但本帧已进入的 Learning 分支继续跑 `tick_learning` → 立即滑进 `ClientWait` 卡死；改为 `phase != Learning` 即返回（`91ba76f`）。
- **归队后无头像**：头像按 `roster.slot == player.id` 取，归队后 `steam_roster` 未按权威参与顺序重建；新增 `steam_rebuild_roster`（`91ba76f`）。
- **大厅“host left”误判**：新加入者在 P2P/心跳建立前就被 4s 门槛判离开；加**加入宽限**（未 heard 时 30s 才算）（`9c86632`）。
- **断网提示缺失**：新增顶部横幅「已与 Steam 断开，正在等待重连…」（`2b40fc7`）。

### 15.2 不变量更新
- “host = `lobby_owner`”在 **A 路线**下保持；**重连后必须 `RequestLobbyData`** 才能拿到真值 owner（否则本端 lobby 状态陈旧、自栅栏不触发）。
- 阶段切换后**不得**再用旧阶段的分支逻辑（尤其 Learning→Fighting）。
- `steam_net_ticks` 只有一个递增点（`update` 全局）。

### 15.3 基线
- `net` 单测 **58**；`client`/`game-core` 不变；协议仍 **58**（S 系列均为运行期行为，不动快照字段）。

## 16. 迁移抖振治理 + 换主机系统暂停（2026-10-16，待真机验收）

### 16.1 真机证据（三机，~4 分钟）
- **迁移抖振**：C 端 20 秒内连迁 4 次、每次重选出**同一个** `lobby_owner`（43985466）；同一窗口 A 端 `AUTO-DROP client 2`、B 端 `AUTO-DROP client 0`；三端共 3 次 `[steam-p2p] session failed: ProblemDetectedLocally / MiscTimeout`。
- **倒数抖跳**：`resync 30.0 -> 1.6` 与 `resync 1.5 -> 29.8` 相程出现（跨轮次采纳了上一轮末尾的过期快照）。
- **误报**：`NO frames for n/a (180 ticks)` —— `n/a` 说明墙钟计时器被停在 0.0（只在“计数”触发）。
- **大 gap 的归因**：6 个 6~37s 的 `[jit] sim gap` 全部是人肉对齐才能分清的**暂停 / 轮次切换(ClientWait) / 迁移**三类，不是 hang。

### 16.2 改动（本轮 6 个 commit）
- **C1 `38a0304`（迁移完成统一复位判活）**：两条迁移出口（阶段 A 收到方向性 Snapshot / 阶段 B 收到 Takeover）原先只清 `steam_cli_stale_ticks`，留下墙钟计时器停在 `0.0`（=暂停判活）→ 判活退化为“按帧数”（受帧率漂移影响）；改为统一 `steam_note_host_alive(now)`，`poll_steam_migration` 增 `now` 参数。
- **C2' `172f38e`（换主机系统暂停）**：
  - 新宿主在 `steam_do_takeover` 里、**广播 `Takeover`/`Snapshot` 之后、第一帧之前** arm 系统暂停（顺序关键：客户端只采纳“当前 host”的 `PauseState`）；
  - 哨兵 = `paused && by == 0`（玩家暂停必须 `by != 0`，故 **不用改包结构就能区分**）；**不消耗任何人的额度、恢复不设 `PAUSE_COOLDOWN_TICKS`**、幂等（叠加第二次迁移只重播）；
  - `PauseState` 增 `present`/`total`（运行期包，不升协议）→ 两端都能显示 `等待玩家连接 x/y`；“就绪” = `client_peers[c].is_some() && !dropped[c]`（**不能用 `present_clients_count()`**：`mark_dropped` 会塞默认输入，掉线者也计“在场”）；
  - UI：暂停覆盖层区分系统暂停（“房主已更换，本局已暂停” + `x/y`），**软门禁**（恢复键未就绪时给二次确认，仍是同一个确认框只换文案）；恢复走现有“任何人可请求 + 3s 倒计时”。
- **C2'-b（同 commit）**：`pump_learning` 的 `PauseState` 补 `from == self.host` 过滤（与对局期一致，防旧 host 在途暂停态在迁移期被采纳）。
- **C3 `cdec03d`（倒数对齐门控 + 只读 meta）**：`host_learn_remaining` → `host_learn: Option<(round, remaining)>`（字段数不增）；新增 `world_ser::snapshot_meta_from_bytes()`（meta 在快照尾部，只读两个长度、跳过 world）→ `pump_learning` 与 `snapshot_phase_is_learning()`（**迁移期间每帧**调用）不再为一点 meta 反序列化整个 World；对齐仅在**同轮 且 不在配置同步**时发生。
- **C4 `7523349`（诊断可读性）**：`[jit] host/client sim gap … reason=paused|migrating|cfg-sync|rejoin|plain`；`AUTO-DROP` 日志补 `steamid`。
- **C5 `eb4041b`**：看门狗连续判定 2→3 次（`-ConsecutiveNeeded`）。

### 16.3 不变量更新
- **“选举”= 读 Steam `lobby_owner` + 校验**（非投票/非自选举）；宽限/暂停耗尽后**不得再选一次**（只会得到同一个 owner），按迁移失败路径处理。
- **只有当前 host 的 `PauseState` 算数**（对局/学习两侧一致）。
- **系统暂停不得污染玩家额度/冷却**；`paused && by == 0` 唯一表示系统暂停。
- **推帧间隔异常必须能自证原因**（新增 `jit_stall_reason`），不允许再出现“只能人肉对齐”的日志。

### 16.4 验收要点（待真机）
1. 换主机后**必然**出现系统暂停覆盖层（学习/对局都测），`x/y` 从 `1/N` 涨到 `N/N`；
2. 换主机窗口内**不再出现重复迁移**（原抖振目标，现由暂停保证）；暂停期间无人被 `AUTO-DROP`；
3. 系统暂停**不消耗**任何人的暂停次数（覆盖层里的“剩余暂停”列表可核对）；恢复后玩家可**立即**暂停（无冷却）；
4. 轮次边界 `learn_remaining resync` 每个边界 ≤ 1 条且同向；
5. 日志：每事件 1 条 `NO frames`、1 条 `elected`；`[jit] … reason=` 可读；无 `DESYNC`/`panic`/`Phase-B STALL`。

## 17. F 系列：迁移期“被误掉线/无法操作”修复（2026-10-17，待真机验收）

### 17.1 真机现象（三机，A=原房主/B=学习期接管的房主/C=报障端）
- **现象**：学习期接管出的新宿主 B 在**对局期退出**后，A 接管；C **无法控制自己的角色**（但能看到别人动），且被标为**“已离开”**。
- 日志链（`logs_2/…000235`）：
  - `219 host broadcast Leave (leaver=63417)` → `221 elected new host=43985466` → **`222 migrated to new host (seq=0)`**（A 的真接管是 `seq=401`）
  - `246 NO frames for 3.0s … (host=63417)`（!! 它其实指向了**已死的 B**）→ `260/261` 第二次才认到 A（`seq=401`）→ 共绕了 **≈ 5.5s**（时间戳 `1792166629927 → 1792166635244`）
  - `logs/…000200`（A 侧）：`282 handover pause armed` / `283 paused=true by=0` → **`317 AUTO-DROP client 2 (steamid=…99873494062)`**（在系统暂停/恢复倒计时期间掉了 C）→ `340 paused=false`
  - `logs_2:265 online set updated -> [43985466]`（在线集里没有 C 自己）

### 17.2 三个根因
| | 根因 | 证据 |
|---|---|---|
| **D1** | host 侧 `auto_drop_idle` 在**系统暂停期间照常计数**（`idle_ticks += 1` 在 `poll()` 尾部，每帧都跑）→ 把“正在迁移、本就静默”的端当掉线 | `AUTO-DROP client 2` 出现在 `paused=true by=0` 窗口内 |
| **D2** | 被 drop 的端**只能靠 `ReconnectReq` 恢复**；普通心跳（`RoomState`）不解除掉线 → “被 drop 但仍在每帧上行的端”永久卡在「已离开 + 输入被默认值替代」 | 修复前 `unmark_dropped` 调用点仅 `ReconnectReq` + 局域网路径 |
| **D3** | 阶段 B **不校验 `Takeover` 来源**，采纳了**缓存里**学习期旧宿主 B 的 Takeover（`seq=0`）→ `retarget_host(B)` + `set_start_seq(0)` | `migrated to new host (seq=0)`，且随后静默判定仍报 `host=63417` |

### 17.3 改动（3 个 commit）
- **F3' `b7215d2`（早 retarget + 交接包只认新宿主 + 定向探测收尾）**
  - 阶段 A→B 交界（采纳 Steam owner 的瞬间）**立刻 `retarget_host(X)`**：“谁是房主”只认 Steam；`Takeover`/`Snapshot` 从此只当“接管基线 + 状态交接”。时机仍守在“阶段 A 对旧宿主探测超时之后”。
  - 阶段 B 只认来源 == X 的交接包（缓存与实时都过滤）→ 旧缓存再也骗不到。
  - 阶段 B 主动每 `RECONNECT_REQ_EVERY` 发定向 `ReconnectReq` 给 X，**优先用 X 的定向 `Snapshot{your_index}` 应答收尾**（不依赖广播被收到）；抽出 `finish_migration_with_snapshot()`（阶段 A/B 同构、消重），扫包时遇到的 `Takeover` 用新增的 `lockstep::cache_takeover()` 存回缓存。
  - 顺带：迁移中的端不再对新宿主静默 → 新宿主 idle 攒不起来；其 `ReconnectReq` 会触发 host 侧现成的 `unmark_dropped`。
- **F2 `e9665c0`（心跳即恢复）**：`RoomState` 臂里若 `dropped[c]` 则 `unmark_dropped`（**必须在写入本帧 `latest_input` 之前**——unmark 会把它置 None）并置 `online_dirty` → “已离开”标记自动撤销。新单测 `room_state_heartbeat_undrops_client`。
- **F1 `ca29028`（系统暂停期间不判空闲）**：放进 `auto_drop_idle` 自身（`paused && by == 0` → 直接返回空），保证三处调用（学习期/对局期/局域网）都不会遗漏且可单测。新单测 `auto_drop_idle_is_skipped_during_handover_pause`。

### 17.4 不变量更新
- **“谁是房主”只认 Steam `lobby_owner`**；`Takeover`/`Snapshot` 只负责“基线 + 状态交接”，**不得**作为“谁是房主”的来源（且只采信所选新宿主的）。
- **迁移/重连期间的静默不是“掉线”**：系统暂停期间不判空闲；被掉的端只要还有正常心跳就应自动恢复。
- `auto_drop_idle` 的调用点不得绕过“系统暂停不判空闲”的约束（已内置）。

### 17.5 验收要点（待真机）
1. 学习期接管出的新宿主在**对局期退出** → 其余端：无“无法操作/被标已离开”；日志里**不应**出现 `AUTO-DROP` 正在迁移的那端；
2. 迁移日志应显示一次“`elected …; retargeted early`” + 一次 `resumed from snapshot … (resume-new-host)`（或广播 Takeover 路径），**不再**出现 `seq=0` 这种被旧缓存带偏的值；
3. 换主机后 `x/y` 能涨到 `N/N`；系统暂停仍不扣任何人的暂停额度；
4. 被 drop 后假如又回来：`online set` 应重新包含它（“已离开”标记自动撤销）；
5. 全域：无 `DESYNC`/`panic`/`Phase-B STALL`。
