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
