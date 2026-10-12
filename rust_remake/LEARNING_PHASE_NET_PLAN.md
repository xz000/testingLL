# 学习期联机 设计规划（LEARNING_PHASE_NET_PLAN.md）

> 目标：让**学习期（首局配置/商店，`MatchPhase::Learning`）**也支持**离开**、**掉线检测**、**重连/归队**（并可承接 R7e 学习期暂停）。
> 关联：`EXIT_MENU_RECONNECT_PLAN.md`（R7/R7e）· `LEAVE_REJOIN_PLAN.md`。
> **本文只做规划，未改代码。**

---

## 0. 现状审计（代码事实）
- 学习期 = `MatchPhase::Learning`：`meta.begin_first_round_config()` 进入，`learn_remaining = config.first_round_time_secs`（**默认 40s**，098c 「Shop Time initial」），`tick_learning(dt)` 倒数到 0 → `enter_first_round()` → `Fighting`。
- 联机的**所有网络推进只在 `MatchPhase::Fighting` 分支**（`main.rs:6961+`）：
  host 产帧/快照/`auto_drop_idle`、client 上行输入/`step_frame`/掉线计数、`host.poll`/`cli.*` 接收——**学习期分支（`main.rs:6854`）一律不跑**。

### 0.1 由此产生的问题（正好对应你观察到的）
1. **无快照** → 归队握手 `steam_rejoin_handshaking` 等不到 `Snapshot` → 15s 超时回菜单 ⇒ **学习期无法重连**。
2. **无掉线判定** → 学习期掉线/退出不被发现（`auto_drop_idle`、client `stale` 都不跑）。
3. **退出会卡住别人**：进入 Fighting 前 host 走 `HostGather` 需收齐**所有参与者的配置**（`all_cfgs`）；学习期退出者永不发 `PlayerCfg` → host 干等 → **全员卡在开局**（除非该端已被 `mark_dropped`，但学习期不 `poll`，标不了）。
4. 连接在学习期**完全空闲**（无帧），P2P 有被 Steam 判空闲/超时的风险。

---

## 1. 关键设计

### 1.1 学习期需要"轻量网络节拍"（心跳 + 静态快照）
学习期 World **不步进**，所以**不能靠"帧"**做心跳（帧会推进模拟）。改用：
- **host 每 ~N 帧**：`host.poll(rcv)`（收 `RoomState` 心跳 / `Leave` / `ReconnectReq` / `PlayerCfg`）+ `set_snapshot`/`broadcast_snapshot`（world+meta 的**静态**快照，`seq = next_seq` 不变）+ 广播在线集。
- **client 每 ~N 帧**：`send_room_state(...)`（在场/心跳，合包已可靠）+ `step_frame`/`recv_snapshot`（收快照）+ **学习期专用存活计数**（收不到 host 心跳/Snapshot 达阈值 → 判定 host 掉线，走覆盖层/迁移）。
- **注意**：学习期**不要**沿用"收不到**帧**就 stale"的逻辑（学习期本就没有帧）；用"收不到**心跳/快照**"作为存活依据。

### 1.2 归队端要按 `meta.phase` 恢复**阶段**，不能假设一定进 Fighting
`apply_snapshot_world` 已恢复 world+meta。归队后应据 `meta.phase`：
- `Learning` → 把 UI 置回学习/配置态（`pre_game_config` 等），沿用 `meta.learn_remaining`；
- `Fighting` → 现状。
> 现在归队握手隐含假设"恢复即进 Fighting"，学习期归队会错位。

### 1.3 退出/掉线：host 标记 + 排除出本轮配置
- 学习期 host 也要处理 `Packet::Leave`（立即 drop）与 `auto_drop_idle`（阈值**放宽**，例如 300 帧=5s，避免误判正在挑技能的人）；
- 被标 `dropped` 的参与者按现有 `all_cfgs` 语义**排除**（`host_all_cfgs_ignores_dropped_client` 已如此）⇒ host 能收齐其余人 → 正常开局（少一人）。

### 1.4 人数变化 → 开局前重排参与集（关键取舍）
- **方案 A（推荐）**：进入 Fighting 前的 `HostGather` 用**当前在线集**重定 `participants`（复用 `set_participants` + `stage_world_for_participants`）→ 全员按**新人数**开局；
- **方案 B**：保留槽位（离开者站桩 1 人）→ 人数少的一边吃亏。**不推荐**。
> 重排时机 = 从学习期进入 `HostGather` 时。

### 1.5 学习期暂停（R7e，可选/后续）
- host 权威冻结 `learn_remaining`：host 广播"学习期暂停"→ 各端 `tick_learning` 暂停；恢复时下发剩余时间。
- 需扩展暂停通道（现有 `PauseState` 是 Fighting 语义）或新增学习期字段。

---

## 2. 改动点（预估）
| 模块 | 改动 |
|---|---|
| `client/src/main.rs` `MatchPhase::Learning` | 新增**轻量网络节拍**：host（poll + 低频 snapshot + auto_drop + 处理 Leave/ReconnectReq）、client（心跳上行 + 收快照 + 学习期存活计数）|
| host 学 习期 | `set_snapshot`/`broadcast_snapshot`（world+meta，seq 不变）；`auto_drop_idle`（放宽阈值）；在线集广播 |
| 归队 | `apply_snapshot_world` 后按 `meta.phase` 置 UI 态（学习/战斗）|
| 开局重排 | `HostGather` 时按在线集 `set_participants` + `stage_world_for_participants` |
| `game-core/meta.rs` | （R7e 才需要）学习期暂停字段 |
| 协议 | 若**复用**现有 `Snapshot`/`Participants`/`Leave`/`RoomState` ⇒ **可能不升**；若新增包/字段 ⇒ 升版本 |

---

## 3. 分阶段实施（每步 1 commit）
- **E1｜学习期心跳 + 快照**：host 低频 `poll`+`broadcast_snapshot`（seq 不变）；client 心跳上行 + 收快照 + **学习期存活判定**（收不到心跳 → 覆盖层/迁移）。→ 归队端能拿到快照。
- **E2｜归队按阶段恢复**：`apply_snapshot_world` 后据 `meta.phase` 进学习态或战斗态。
- **E3｜学习期退出/掉线**：host 处理 `Leave` + 放宽的 `auto_drop_idle`；被 drop 者由 `all_cfgs` 排除 → **退出不卡局**。
- **E4｜开局人数重排**：`HostGather` 按在线集重排 `participants`/world。
- **E5｜（可选）R7e 学习期暂停**。

---

## 4. 测试
- **单测**：学习期快照往返（`meta.phase==Learning` + `learn_remaining` 保真）；`all_cfgs` 排除 dropped；`set_participants` 重排。
- **真机双账号**：① 学习期 A 退出 → 其余正常开局（少一人）；② 学习期 A 掉线 → 归队**回到学习期**（不是战斗）；③ 学习期房主掉线 → 覆盖层/迁移。

---

## 5. 待确认
1. **学习期掉线阈值**：Fighting 是 3s(180 帧)；学习期放宽到多少？（建议 5s，避免误判正在挑技能者。）
2. **退出后是否重排人数**：方案 **A（重排）** 还是 B（保留槽位）？
3. **本轮是否连 R7e（学习期暂停）一起做**？还是先只做 E1–E4（离开/掉线/归队）？
4. 学习期心跳频率（建议 host 每 30 帧=0.5s 发一次快照；client 每 15 帧上行一次 `RoomState`）。
