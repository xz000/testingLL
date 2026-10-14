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
1. ✅ **已做（`<本次>`）**：`notify_old_host_takeover` **重发** —— 接管时暂存通知，`poll` 尾部每 30 tick(0.5s) 重发一次、共 12 次(~6s)，
   提高到达率，促旧 host 尽快退位（`HostLockstep` 新增 `old_host_notice*` 字段 + `poll` 重发；net 单测 `notify_old_host_takeover_is_resent`）。
2. 旧 host 自栅栏更积极（一旦观察到“他人 Takeover / owner 已变 / 自己的权威回声消失”即退位）；
3. 记录/限流：对“非当前 host”的包做丢弃计数，便于诊断僵尸窗口时长。
