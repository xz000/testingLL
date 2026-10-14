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

### M3 选举/接管严格化（修 R2/F4）
- **接管前先校验**：`lobby_owner == 本端` 才允许 `steam_do_takeover`；否则拒绝接管、继续等待
  （避免"非 owner 接管→自栅栏退位"的连锁）。
- 选举只接受 owner（现状）；owner 未移交时：**等待并周期重查 owner**（有上限，如 ~10s），
  超时才回菜单（保留会话）。**禁止**任何"最小 ID 猜测"回退。
- 退位/被取代路径**不得广播 `Leave`**（`Leave` 仅用于真正的"我主动离开本局"），避免诱导他人误迁移。

### M4 迁移后一致性断言（修 R3/I4）
- 迁移/接管完成后断言 `steam_participants.len() == world.players.len()`，不一致则以 host 侧为准修正并告警。
- 所有采纳点保持 `from == self.host`（I5）。

---

## 6. 分步与提交

1. **M0（已完成）**：撤回 `elect_new_host` 回退回归。
2. **M1 诊断**：Leave/超时补 sender 与 host 指针日志。
3. **M2 单一参与者来源**：host 学习期广播全量名单 + client 采纳。
4. **M3 严格化**：接管前 owner 校验 + owner 未移交的等待上限 + 退位不广播 Leave。
5. **M4 断言**：迁移后人数/索引一致性校验。

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

## 8. 待决策点

- D1：owner 未移交的**等待上限**取多少秒后回菜单？（建议 10s）
- D2：是否允许"非 owner、但各端在线集一致"的共识式自选举？（建议**否**，保持 owner 仲裁单一权威）
