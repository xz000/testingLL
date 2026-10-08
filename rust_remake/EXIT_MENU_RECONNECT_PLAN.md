# 对局内退出菜单 + 重连 规划（EXIT_MENU_RECONNECT_PLAN.md）

> 创建 2026-09-27（v2）。目标：把「对局中按 Esc 立即退回主菜单」改成**确认式退出菜单**，并补全**重连**的界面与入口。
> 本文只做规划，**未改代码**。关联：`RECONNECT.md` · `STEAM_MULTIPLAYER_PLAN.md` · `UI_MASTER_PLAN.md` · `FRAME_SYNC_*`。
> 阅读顺序：§0 现状 → §1 术语 → §2 决策 → §3 状态机 → §4 退出菜单 → §5 重连 → §6 Q3/Q4 权衡 → §7 改动点 → §8 实施 → §9 测试 → §10 风险 → §11 待确认。

---

## 0. 现状审计（代码事实，2026-09-27）

### 0.1 退出相关
| 场景 | 当前行为 | 位置（`client/src/main.rs`） |
|---|---|---|
| 对局中（`MatchPhase::Fighting`）按 Esc | **立即 `reset_to_main_menu()`**，无确认、无菜单 | ~6241 |
| 结算（`MatchPhase::Finished`） | 只有按 `Q` → `reset_to_main_menu()`；**Esc 无反应** | ~6127 |
| 学习/局间配置（`MatchPhase::Learning`） | **没有退出对局的入口**（未见 Esc/Q 处理，见 §11-Q8） | — |
| 开局技能配置（`pre_game_config` 覆盖层） | 是否可退出待确认 | — |
| Steam 房间/就绪 | `Q` → `steam_leave_room()`；Esc 用于取消「进行中的建房/加入」 | `steam_lobby_update` ~7308 |
| host 已离开覆盖层 | Esc/Q/Enter → `steam_leave_room()`（**仅 Steam**，LAN 无对应） | ~5890 |
| 窗口关闭（CloseRequested） | `quit_event`（默认直接退出进程） | `GameApp::window_event` ~11307 |
| `reset_to_main_menu()` | **全量拆除**：`leave_lobby` + 丢 `Host/ClientLockstep` + 清全部 Steam 状态 | ~7062 |

> 关键：`reset_to_main_menu` 把「离场」与「回菜单」合二为一，没有单独语义。

### 0.2 掉线 / 重连相关
| 环节 | 当前实现 | 位置 |
|---|---|---|
| host 判 client 掉线 | `auto_drop_idle(HOST_DROP_TICKS=180≈3s)`（**LAN 与 Steam 都用**）→ `mark_dropped`：该端用**默认输入**（原地站桩）占位，其余继续，不卡全队 | `main.rs` 6371 / 6674；`net/src/lockstep.rs` `mark_dropped`/`auto_drop_idle` |
| host 找回槽位 | `ReconnectReq{identity}` → 按稳定身份找槽位 + `unmark_dropped`；有每端限速 `RECONNECT_RESP_INTERVAL=30` | `net/src/lockstep.rs` ~674 |
| 快照 | host 本地 `set_snapshot` 每 `SNAPSHOT_EVERY=30` 帧（≈0.5s）；**广播**每 `SNAPSHOT_BROADCAST_EVERY=150` 帧（≈2.5s，仅 Steam） | `main.rs` |
| **LAN** client 判掉线 | `stale_ticks>=CLIENT_STALE_TICKS(180≈3s)` → `conn_dropped=true` → 冻结世界 → `draw_reconnect_overlay` → 按 `R` → `poll_reconnect` | `main.rs` 6767 / 2875 / 4403 |
| **Steam** client 判掉线 | `steam_cli_stale_ticks>=180` → `steam_migrating=true` → **主机迁移** | `main.rs` ~6597 / `steam.rs::poll_steam_migration` |
| Steam 主机迁移 | 已完整实现（三阶段，真机复验通过） | `STEAM_MULTIPLAYER_PLAN.md` |
| 帧缓冲 | `frame_buf_capacity=60` 帧（≈1s）→ 超过 1s 的缺口靠**快照**而非帧补发 | `net/src/lockstep.rs` |

### 0.3 关键数值（用于窗口/节奏判断）
- 单局默认 `total_rounds=3`；`first_round_time_secs=40s`；`between_rounds_time_secs=30s`（`game-core/src/meta.rs`）。
- 掉线端角色：`default_input_bytes()` = **原地不动**，仍在世界里、仍可被击杀（站桩靶子）。
- ⚠ **分叉（需决策）**：`steam::poll_steam_reconnect`（R 键拉快照重连）依赖 `conn_dropped`，但**Steam 分支从未把它置 `true`**（全仓库只有 `steam.rs:164` 置 `false`）→ 该函数**不可达/近似死代码**；Steam 实际走迁移。文档里「Steam 按 R 重连」与现状不符。

---

## 1. 术语与语义

| 术语 | 含义 | 对他人影响 |
|---|---|---|
| **继续 Resume** | 关掉菜单回到对局 | 无 |
| **离场 abandon（主动）** | 主动退出本局 | host 离开→迁移/中断；client 离开→host 判离场 |
| **掉线 drop（被动）** | 网络断开，保留槽位一段时间 | host 用默认输入占位继续 |
| **重连 reconnect** | 掉线者在窗口内回来接续 | 无（接回原槽位） |
| **迁移 migrate** | host 掉线，其余端选新 host | 需快照 + 选举（Steam 已做） |

---

## 2. 决策记录（Decision Log）

| # | 问题 | 决策 | 状态 |
|---|---|---|---|
| **D1** | 退出菜单「返回主菜单 / 退出游戏」是否二次确认 | **要做二次确认**（防误触断送整局） | ✅ 已定（2026-09-27） |
| **D2** | Steam client 掉线语义 | **A：先重连、再迁移**（原 host 有应答→重连接回；超时→迁移） | ✅ 已定（2026-09-27） |
| **D3** | 重连窗口时长 + 是否自动重试 | **60s 软窗口 + 每 ~3s 自动重试**；过期不物理移除，仅 UI 标「已离开」 | ✅ 已定（2026-09-27） |
| **D4** | LAN 身份持久化 / LAN host 迁移 | **都不做**（LAN 是备胎；持久化会破坏同机多开；LAN 无一致成员视图，无法确定性选举） | ✅ 已定（2026-09-27） |
| **D5** | LAN 优先级 | **先不在 LAN 上投入**：Steam 主线优先；LAN 只保证**不回归**，不做重连/迁移增强 | ✅ 已定（2026-09-27） |
| **D6** | 退出菜单覆盖阶段 | **纳入**开局配置 / 学习 / 结算阶段（顺手补上 Learning 无出口的缺口） | ✅ 已定（2026-09-27） |

---

## 3. 对局生命周期 & 掉线状态机

### 3.1 客户端（client）视角
```
[房间就绪] ──start──▶ [开局配置] ──▶ [对局 Fighting] ⇄ [局间 Learning] ──▶ [结算 Finished]
                                            │
                              连续 180 帧无权威帧
                                            ▼
                                    [掉线/Dropped]
                                    ├─ 自动+手动发 ReconnectReq（每 ~3s）
                                    │      ├─ 原 host 应答 Snapshot ──▶ [重连成功] → 回 [Fighting]
                                    │      └─ 探测超时 → [进入迁移] → 选新 host → 接管/重定向 → [Fighting]
                                    └─ 玩家选择「返回主菜单」──▶ [主菜单]
```

### 3.2 host 视角
```
其他 client 连续 180 帧无输入
        ▼
   auto_drop_idle → mark_dropped（默认输入占位，世界继续；角色原地站桩）
        │
        ├─ 收到该身份 ReconnectReq → unmark_dropped（接回，世界继续）
        └─ 超过 RECONNECT_WINDOW（D3）→ 仅 UI 标记「已离开」（**不物理移除**，见 §5.2）
```

> 设计约束：**玩家数量/索引在 `World` 里固定**，重连/离场都**不改变 `players` 数量与顺序**（否则两端 desync）。窗口过期只做状态标记。

---

## 4. 退出菜单设计（深入）

### 4.1 触发点与覆盖范围
- 定义 `in_match()`：`app ∈ {Solo, LanHost, LanJoin, SteamHost, SteamJoin}` 且**不在**大厅/房间/菜单界面。
- 在 `update` **最前部**统一拦截：`in_match()` 且 `!escape_menu` 且按 `Esc` → `escape_menu = true`，并**立即返回**（吞掉本帧输入）。
- 菜单打开后，所有游戏输入（技能/移动/镜头/购买）暂停；`Esc`（或「继续」）关闭。
- 覆盖所有阶段：Fighting / Learning / Finished / 开局配置（这样也顺手补上 Learning 无出口的缺口，§11-Q8）。

### 4.2 帧同步红线：本地暂停 vs 世界继续
**多人：绝不能在本地真暂停世界**（本地停 → 与权威分叉）。
- client 打开菜单：**继续按 tick 上行「默认输入」占位**，并继续 `step_frame` 吃权威帧推进世界（画面在菜单后继续动）。等价「本地玩家暂时挂机」。
- host 打开菜单：**继续产帧**（用默认输入占位自己的输入），否则会拖停全队。
- 因此网络循环**不因菜单而跳过**；菜单只门控「本地输入采集 + 技能/购买动作」。
- **单机/训练场**：本地是唯一权威 → 可真暂停（`escape_menu` 时跳过 step）。
- 实现建议：抽 `fn menu_gate_local_input(&self) -> bool`；多人为 true 时 `local_player_input()` 返回 `Default`、并屏蔽 `poll_input`/商店/学习点击。

### 4.3 菜单项矩阵
| 模式 | 继续 | 重新连接 | 返回主菜单 | 退出游戏 |
|---|---|---|---|---|
| 单机 / 训练场 | ✅（并恢复真暂停） | — | ✅ | ✅ |
| LAN client | ✅ | 掉线时显示 | ✅（离开本局） | ✅ |
| LAN host | ✅ | — | ✅ + ⚠「会中断所有人」 | ✅ |
| Steam client | ✅ | 掉线时显示 | ✅（离开本局，其余继续） | ✅ |
| Steam host | ✅ | — | ✅ + ⚠「会触发其他人主机迁移」 | ✅ |

### 4.4 二次确认流程（D1）
- 「返回主菜单」「退出游戏」为**危险项**：第一次确认后进入「再按一次确认」态（`escape_menu_confirm`），显示红色提示；移动选择到别处或按 Esc 取消。
- 文案按角色区分（房主 vs 参与者）。
- 「继续」「重新连接」不需要确认。

### 4.5 UI / 实现
- 复用 `ui::theme` / `ui::paint_row` / `layout::centered_panel`（与商店/设置/大厅一致）。
- 新增字段：`escape_menu: bool`、`escape_menu_selection: usize`、`escape_menu_confirm: Option<EscapeChoice>`。
- 鼠标命中：`ui::HitRegistry<MenuAction>`（同房间列表/设置编辑器模式）。
- `keys.rs`：`Screen::PauseMenu`（供 `every_screen_declares_bindings` / `navigable_screens_offer_a_way_back` 守卫）。
- 源码级守卫测试：断言 Fighting 分支**不再**在 Esc 处直接 `reset_to_main_menu`。

### 4.6 语义拆分（E2）
- `leave_match()`（离场）：`reset_to_main_menu()` 现有全量拆除 + （可选）先发 `Packet::Leave`。
- 「继续」只清 `escape_menu`，不动网络/世界。
- 单机 `leave_match()` 同现有 reset。

---

## 5. 重连设计（深入）

### 5.1 决策树（client 掉线后）
```
无权威帧 180 帧
   │
   ├─ 单机? —— 不适用（单机不掉线）
   │
   ├─ LAN: 发 ReconnectReq 重试 N 次
   │        ├─ host 应答 Snapshot → 重连成功（接回原槽位）
   │        └─ 窗口耗尽 → 提示「连接未恢复 / 房主可能已离开」→ 返回主菜单
   │
   └─ Steam: [D2-A] 先探测原 host
            ├─ 原 host 应答 → 重连成功
            └─ MIGRATE_PROBE_TICKS 超时 → 迁移（现有 poll_steam_migration 阶段 A→B）
```
> D2-A 与现有 `poll_steam_migration` 阶段 A 完全一致（它本就先发 `ReconnectReq` 再选举）→ 实施上就是**删掉冗余的 `poll_steam_reconnect`**，把「掉线 UI + 自动/手动重试」接到迁移状态机上。

### 5.2 host 端：窗口与槽位
- 保留现状 `auto_drop_idle` + `unmark_dropped`（已支持接回）。
- 新增 `dropped_since: Vec<u32>`：记录各 client 掉线后的帧数。
- 超过 `RECONNECT_WINDOW`：**不物理移除**（world 固定），仅在 host 侧状态标 `left[]=true`，UI 显示「已离开」。若将来需要「离场广播」，再加包。
- 窗口内若重连：清 `dropped_since`、`unmark_dropped`。
- 理由：物理移除会改变参与集/索引 → 与「world player 数固定 + 帧同步」冲突（见 §3.2 约束）。

### 5.3 client 端：自动 + 手动
- 掉线后进入 `[掉线]` 覆盖层（升级现有 `draw_reconnect_overlay`）：
  - 显示：已等待时长、剩余窗口倒计时、状态（「正在重连…／host 已离开，尝试接管…」）。
  - **自动重试**（D3）：每 ~3s 自动发一次 `ReconnectReq`（host 侧 `RECONNECT_RESP_INTERVAL=30` 有节流，客户端不必每帧发）。
  - 手动：「R / 重连」立即再试一次；「返回主菜单」离场。
- 成功：重建 `World` → `apply_resync` → 清本地输入残留（复用现有 `poll_reconnect` 逻辑）。

### 5.4 Steam 归并细节（D2-A）
- 删除 `steam::poll_steam_reconnect`（及其 `conn_dropped` 门控），掉线统一走 `steam_migrating` 状态机。
- `poll_steam_migration` 阶段 A 已是「发 ReconnectReq 等应答/超时」；只需在其外层加「自动重试节流 + UI 状态」。
- 修正 `RECONNECT.md` / `STEAM_MULTIPLAYER_PLAN.md` 里过时描述。
- 需保留「原 host 还活着」的早退（收到来自旧 host 的任意包即恢复），现状已有。

### 5.5 LAN 的处理（无迁移，D4/D5）
- **按 D5：先不在 LAN 上投入。** LAN 现状保持**不回归**即可，不做重连/迁移增强。
- 已记录的现状缺口（**本轮不修，留待需要时**）：LAN client 一掉线就进 `conn_dropped` + 重连覆盖层，若 host 真没了会一直等；理想是加窗口超时 + 「房主可能已离开」覆盖层。
- 明确**不做** LAN 主机迁移（§6.2 理由）。
- E1/E2 的退出菜单是**传输无关**的，会自然惠及 LAN（不算额外 LAN 投入）。

### 5.6 身份
- **Steam**：SteamID 稳定 → 崩溃重启后可重新 join + 拉快照接回。
- **LAN**：`random_identity()` 每次进程随机 → 崩溃重启找不回（D4：建议不持久化，见 §6.2）。
- 网络抖动（进程未退）：identity 不变，重连天然可用（主流场景）。

---

## 6. Q3 / Q4 权衡分析（帮你拍板）

### 6.1 Q3：重连窗口时长 + 自动重试

**约束/事实**
- 掉线端角色**原地站桩**、仍可被击杀 → 窗口越长，掉线者越可能被「白打」，但接回后仍在局内（不会少一个人）。
- 单局默认 3 局、局间 30s、首局 40s → 一个自然「等待节拍」约 30–60s。
- 帧缓冲只 60 帧（1s），重连**依赖快照**（本地 0.5s 一份，广播 2.5s 一份），窗口长短不影响能否接回（取最新快照即可）。

**方案对比**
| 方案 | 优点 | 缺点 |
|---|---|---|
| 窗口 30s | 短平快，尽快让局面确定 | 可能只够一次网络抖动；跨一个局间（30s）就到期 |
| **窗口 60s（建议）** | 覆盖「一局 + 一个局间」；给足重连/重启游戏时间 | 掉线者站桩被击杀的窗口略长 |
| 窗口 = 整局（不设硬期） | 实现最简（host 反正一直保槽） | 剩余玩家一直等一个不会回来的人；UI 无「结束」信号 |

**自动重试**：建议**要**（每 ~3s 一次）。理由：手按 `R` 反直觉、容易漏；自动重试只是把现有 `ReconnectReq` 周期化，host 端已有限速保护。手动按钮保留（失败后可立刻再试）。

**建议结论（D3）**：**60s 软窗口 + 每 ~3s 自动重试**；窗口过期不物理移除，仅 UI 标「已离开」并停止自动重试；玩家可在覆盖层随时「返回主菜单」。

### 6.2 Q4：LAN 身份持久化 / LAN host 迁移

**LAN 身份持久化的障碍**
- 身份存 `local_settings` 是**每用户一份**，而仓库有 `multi-launch.ps1` **同机多开**测试 → 多个实例会**共用同一身份** → 重连/握手去重会互相冲突（把两个实例当成同一人）。
- 规避要按「端口/实例」分文件或命令行显式 `--identity`，增加复杂度。
- 收益有限：Steam（主线）天然稳定身份；LAN 是备胎，崩溃重启接回是低频场景。

**LAN host 迁移的障碍**
- 迁移依赖「全体成员一致视图」来选举同一新 host；Steam 用大厅（`lobby_members`）保证一致性，**LAN 无中心成员视图**，client 之间互不知晓 → 做不到确定性选举（`STEAM_MULTIPLAYER_PLAN.md` 的核心洞察）。
- 工程量大、且与「Steam 为核心、LAN 为备胎」定位相悖。

**建议结论（D4）**：
1. **不做** LAN 身份持久化（保留随机；文档记录为已知限制）。
2. **不做** LAN host 迁移；改为「host 丢失 → 覆盖层提示 → 返回菜单」（补齐现有体验缺口）。
3. 若日后 LAN 需要，可作为独立课题（需先解决 LAN 成员一致视图）。

---

## 7. 协议 / 代码改动点
- **E1–E3 可不改协议**。
- **优雅离场（可选，E5）**：`Packet::Leave{identity}` → host 立即判离场（不必等 180 帧）。
  - `Packet` 是运行期协议、**不进 `World` 状态** → **不升 `PROTOCOL_VERSION`**；改 `proto.rs` 需补 `roundtrip_all_packets`。
- `client`：`escape_menu` 系列字段 + 绘制 + 输入门控；`reset_to_main_menu` 语义拆分。
- `net`：`dropped_since` 窗口计时（host）；可选「已离场」广播。
- **不改 `world_ser` / 模拟** → 正常**不升协议号**。

---

## 8. 分阶段实施（细化）

> 每步：可编译 / 有单测 / 可提交；每步跑 `check.ps1`。顺序按「低风险→高风险、不阻塞在真机」。

### E1 退出菜单（UI + 继续/离开，不碰重连）
- `main.rs`：新增 `escape_menu`/`selection`/`confirm` 字段与 `EscapeChoice` 枚举；在 `update` 顶部拦截；`draw_escape_menu`（主题风格 + 鼠标命中）。
- 单机真暂停；多人只门控本地输入（§4.2）。
- 危险项二次确认（D1）。
- 测试：选择循环/确认门槛纯函数；`keys.rs` `PauseMenu` 守卫；源码级守卫「Fighting 不再直接 reset」。

### E2 退出语义拆分 + 输入恢复
- 抽 `leave_match()`；确保开关菜单后本地输入/镜头正常恢复。
- 测试：多人中反复开关菜单，两端逐位一致（复用 lockstep 无头测试思路）。

### E3 重连 UI 统一升级
- 把 `draw_reconnect_overlay` 升级为：等待时长 / 剩余窗口 / 自动重试 / 「重连」「返回主菜单」。
- 抽传输无关的掉线显示状态（LAN + Steam 共用）。
- 测试：重试节拍纯逻辑（`should_retry(now, last)` 类）。

### E4 Steam 重连归并（D2-A）
- 删 `poll_steam_reconnect`；掉线 UI/手动重试接到 `poll_steam_migration` 阶段 A。
- 修正 `RECONNECT.md` / `STEAM_MULTIPLAYER_PLAN.md` 过时描述。
- 测试：迁移阶段 A 早退/超时分支纯逻辑；真机复验（双账号，调网络）。

> **实施顺序（按 D5「Steam 优先」调整）**：E1 → E2 → **E4** → E3 →（可选）E5。
> E3 的「掉线覆盖层升级」聚焦 **Steam**；LAN 沿用现状（只保证不被 E1/E2 弄回归）。

### E5（可选）优雅离场 + 窗口过期
- `Packet::Leave{identity}`；host `dropped_since` 窗口 → 标「已离开」。
- 测试：`proto` roundtrip；host 收到 Leave 立即判离场。

---

## 9. 测试策略
- **纯函数单测**（首选，符合项目风格）：菜单状态机、二次确认门槛、重试节拍、窗口判定、决策树分支。
- **`net` 单测**：已有 `host_auto_drops_then_client_reconnects_resumes`、`host_continues_after_client_dropped`、`reconnect_snapshot_and_resync_roundtrip` → 补「窗口过期后仍不 desync」（只是状态标记）。
- **源码级守卫**（沿用 `keys::source_scan_tests` 风格）：Esc 不再直接 reset；菜单覆盖所有 match 阶段。
- **无头 e2e**（`netlink.rs` 已有模式）：三端真 UDP，A 掉线重连接回后三方逐位一致。
- **真机复验**：Steam 双账号（host 在→client 重连；host 退→迁移）；LAN 双机（client 断网重连；host 退→覆盖层）。

---

## 10. 风险与边界
1. **帧同步红线**：菜单绝不暂停多人世界；host 打开菜单仍产帧（最容易踩）。
2. **菜单吃掉输入**：模态期间要吞掉后续帧输入，避免一帧内既开又关。
3. **掉线端站桩**：窗口内该角色仍存活可被击杀 → 需接受（或未来讨论「掉线即无敌/幽灵化」，属玩法改动，本规划不做）。
4. **迁移与重连交叠**：掉线窗口内 host 又掉线 → 迁移后掉线者归队（现有 `retarget_host` + cached snapshot，需真机验证）。
5. **窗口状态标记的一致性**：host 标「已离开」若需让其他 client 显示，要广播（可选，E5）。
6. **LAN host 丢失文案**：需与「本机网络断」区分；用「重试失败超时」来收敛。

---

## 11. 决策已确认（2026-09-27）
- **Q3 → D3**：60s 软窗口 + 每 ~3s 自动重试。（§6.1）
- **Q4 → D4**：LAN 身份持久化不做、LAN host 迁移不做。（§6.2）
- **Q8 → D6**：开局配置 / 学习 / 结算阶段均纳入退出菜单。
- **D5**：先不在 LAN 上投入，Steam 主线优先。

下一步：可以开始 **E1**（退出菜单 UI + 继续/离开，不碰重连）。

## 12. 本规划范围外（可另议）
- 掉线角色「幽灵化/无敌」等**玩法**改动。
- 观战、投降/投票结束、真正的暂停（多人）。
- LAN 成员一致视图 / LAN 主机迁移。
- 语音、Steam 富状态扩展等（见 `STEAM_MULTIPLAYER_PLAN.md`）。

## 13. 记录
- 2026-09-27：v1 初版（审计 + 设计骨架）。
- 2026-09-27：**v2** 锁定 D1/D2；补 §0.3 关键数值、§3 状态机、§4.2 帧同步红线、§5 深入重连、§6 Q3/Q4 权衡、§7–§9 改动点/实施/测试。
- 2026-09-27：**v2.1** 锁定 D3/D4，新增 D5（LAN 不投入）/D6（菜单覆盖全阶段）；实施顺序调整为 E1→E2→E4→E3→(E5)。
- 2026-09-27：E1/E2/E4 已落（`6bab72d`/`274e8b8`/`4e356d8`）；新增 §14（E3 深入：现状时序 + 问题 + 优化建议）。
- 2026-09-27：E3 三问已定（§14.7：采纳全部 S1–S6；允许 Esc 放弃；超时横幅+回菜单）；新增 §15（脑裂与 Steamworks 工具）、§16（老 host 归队可行性）。脑裂/归队均作为后续独立专题，**尚未开工**。
- 2026-10-08：**R0 实测（§18.1）**：V1 ✅ 房主离开后大厅存活、owner 自动移交（→ B 可行；C 升为主方案）；V2 ⚠ 连接状态不可靠（→ **放弃 A**，改 R3' = lobby_owner 选举）。R0 探针已提交（`7f5f209`/`2e2d571`）。
- 2026-10-08：**B 修订为「owner 写大厅元数据 epoch」**（聊天内容因 `Client: !Send` 无法在回调中读取，见 §15.2）；
  实施顺序改为**正确性优先** `R2 → R3' → R4 → R1 → R5/R6`（§19.1）。
- 2026-10-08：**R2 已落**（快照带 meta）：新增 `MatchState::to_bytes/from_bytes` + `world_ser::pack_snapshot/snapshot_from_bytes`；
  客户端全部快照读写点（重连/迁移/接管/广播）改为携带并恢复 meta；`PROTOCOL_VERSION` 39→40；补 2 单测。
- 2026-10-08：**R3' 已落**（选举 = `lobby_owner()`）：`net-steam::session::lobby_owner` + 纯函数 `steam::elect_new_host`
  （owner 合法则选 owner，否则回退最小 SteamID）+ 2 单测。各端读同一后端 owner → 选举天然一致。
- 2026-10-08：**R2 真机复验时发现并修复一个真 bug**（`logs/console-menu-full-20261008-222141.log`）：主机迁移后旧 host 掉线，
  下一回合 `HostGather` 永远等它的 `PlayerCfg`（`all_cfgs` 未排除 `dropped`）→ 卡住。修：`net::lockstep::all_cfgs` 对 `dropped` 端不再要求 cfg
  （及其角色保持掉线前配置）；补单测 `host_all_cfgs_ignores_dropped_client`。

---

## 14. E3 深入：Steam 掉线/重连/迁移的时序与 UI 优化

> 结论先行：**保持现有「先探测重连、再选举迁移」的快速时序**（不要改成 60s 才迁移，那会让全队干等），
> 把优化集中在**反馈（UI）**与**少量鲁棒性/带宽细节**上。D3 的「60s」只适用于「LAN 主动重连」口径，**不套到 Steam 迁移**。

### 14.1 现状时序（精确到 tick，约 60 tick/s）
| 阶段 | 触发/时长 | 行为 | 玩家可见 |
|---|---|---|---|
| 正常 | — | 每帧收权威帧推进 | 正常 |
| **静默掉线** | `steam_cli_stale_ticks >= CLIENT_STALE_TICKS(180≈3s)` | 无权威帧 → 世界**冻结**（不推进） | **无任何提示（黑箱）** ← 最大问题 |
| **探测原 host** | 阶段 A，`MIGRATE_PROBE_TICKS(60≈1s)`；**每帧**发 `ReconnectReq` | 收到**来自旧 host 的任意包**即恢复；超时→选新 host | **无提示** |
| **选举后接管** | 阶段 B，`MIGRATE_BAIL_TICKS(600≈10s)`（从进入迁移起算） | 本端是新 host→接管；否则等 `Takeover` | **无提示** |
| 失败 | 阶段 B 超时 | **静默** `reset_to_main_menu` | 突然回主菜单，懵 |

关键常量：`CLIENT_STALE_TICKS=180`、`MIGRATE_PROBE_TICKS=60`、`MIGRATE_BAIL_TICKS=600`、
`HOST_DROP_TICKS=180`（host 判 client 掉线、默认输入占位）、`RECONNECT_RESP_INTERVAL=30`（host 限速应答）、
`SNAPSHOT_EVERY=30`（本地快照）、`SNAPSHOT_BROADCAST_EVERY=150`（Steam 广播快照）。

### 14.2 问题清单
- **P1（UX，最大）**：从「静默掉线」到「迁移完成/失败」全程**无任何 UI**，世界冻结但玩家不知发生了什么。
- **P2**：`draw_reconnect_overlay` 只在 `conn_dropped`（**仅 LAN**）时画；Steam 迁移期完全不画。
- **P3**：阶段 B 超时**静默**回主菜单（无 toast/横幅）。
- **P4（带宽）**：阶段 A **每帧**发 `ReconnectReq`（60/s）；host 已限速回应，客户端发送端也可节流。
- **P5（鲁棒性）**：探测窗口仅 **1s**，一次 relay 抖动 >1s 就误判「host 掉线」→ 不必要的迁移（有快照兜底，但有脑裂风险与开销）。
- **P6（语义）**：阶段 A/B 共用 `steam_migrate_ticks`，进入阶段 B 不复位 → B 的「600 帧」实际是「从进迁移起 600 帧」（B 只剩 ~540 帧）。
- **P7（已知风险，非本次修）**：**非对称断链脑裂**——若 client↔旧 host 断、但 client↔其他 client 通，则 client 选自己为新 host 并广播 `Takeover`，旧 host 收不到 `Takeover`（`superseded` 不生效）→ 可能双权威。需后续专题。

### 14.3 优化建议（供拍板；均为增量、不动协议）
- **S1 早期抖动提示**：静默超过 `STALE_HINT_TICKS(30≈0.5s)` 就显示**非模态**小提示（顶部横幅）：
  「正在等待房主…（{t}s）」；一收到帧即消失。→ 直接消除 P1 的前 3s 黑箱。
- **S2 迁移模态覆盖层**（P1/P2）：`steam_migrating` 期间画覆盖层，按子阶段显示：
  - 阶段 A：「正在尝试重新连回房主…（{t}s）」
  - 阶段 B（本端非新 host）：「房主已离开，正在选拔新主机…（{t}s）」
  - 阶段 B（本端是新 host）：「正在接管对局…」
  - 统一加 spinner + 已等待秒数；底部提示「Esc 返回主菜单（放弃本局）」。
- **S3 失败有反馈（P3）**：阶段 B 超时时 `push_banner("连接未能恢复，已返回主菜单")` 再 reset（或进入 Failed 覆盖层等按键）。
- **S4 节流重连请求（P4）**：阶段 A 每 `RECONNECT_REQ_EVERY(15≈0.25s)` 帧发一次（而非每帧）。
- **S5 探测窗口 1s → 1.5s（P5）**：`MIGRATE_PROBE_TICKS=90`，容忍一次 relay 抖动，仍远快于 60s。
- **S6 阶段计时独立（P6）**：进入阶段 B 时把 `steam_migrate_ticks` 复位（或在阶段 B 用独立计数），使「10s 接管窗口」名副其实。
- **S7 风格统一**：覆盖层/横幅文案与 E1 退出菜单同一套 `ui::theme` 与 i18n。

> 建议采纳：**S1+S2+S3+S4+S6（必做）**，**S5（可做，1.5s 小步）**。S7 随实现。
> 明确**不做**：把迁移等 60s（D3-60s 不套 Steam），也不做掉线角色幽灵化（玩法改动，§12）。

### 14.4 UI 状态映射（实现用）
```
无权威帧：
  stale_ticks < 30         → 正常（仍显示上一帧画面）
  30 ≤ stale_ticks < 180   → S1 顶部非模态横幅「正在等待房主…」
  stale_ticks ≥ 180        → 进入迁移（S2 覆盖层）
迁移中 steam_migrating：
  new_host_id == 0         → 「正在尝试重新连回房主…」
  new_host_id == my_id     → 「正在接管对局…」
  new_host_id == 其他      → 「房主已离开，正在选拔新主机…」
阶段 B 超时                → S3 横幅 + 返回主菜单
```

### 14.5 实现要点 / 改动点
- `client/src/main.rs`：
  - 新增常量 `STALE_HINT_TICKS=30`（或复用/新增）；Steam 分支与 `draw_scene` 据此画 S1 横幅。
  - `draw_scene`：新增 `if self.steam_migrating { draw_migration_overlay }`（Steam）；S1 徽标。
  - 新增 `draw_migration_overlay`（或扩展 `draw_reconnect_overlay` 带状态参数），风格对齐 E1。
  - 阶段 B 超时分枝：加 `push_banner` 后再 reset。
- `client/src/steam.rs`：
  - 阶段 A `ReconnectReq` 节流（S4）；阶段 B 入口 `steam_migrate_ticks` 复位（S6）。
- `client/src/keys.rs`：无需新屏幕（`PauseMenu` 已覆盖「Esc 返回」）。
- **不改协议 / `world_ser`** → 不升 `PROTOCOL_VERSION`。

### 14.6 测试
- **纯函数单测**：S1 阈值判定、S2 状态→文案映射（`migration_overlay_state(migrating, new_host_id, my_id)` 类）。
- **源码级守卫**：迁移覆盖层存在；阶段 B 不再静默 reset。
- **真机复验（待双端）**：host 短暂卡顿 >1.5s → 先探测、能重连接回（不误迁移）；host 真退 → 覆盖层显示各阶段、最终接管/或超时横幅回菜单。

### 14.7 待确认（E3）—— 已定（2026-09-27）
- **E3-Q1** ✅ 采纳全部：S1+S2+S3+S4+S6（必做）+ S5（探测窗口 1s→1.5s）。
- **E3-Q2** ✅ 迁移覆盖层**允许 Esc 主动放弃**→返回主菜单。
- **E3-Q3** ✅ 阶段 B 超时用 **横幅 + 自动回菜单**。

---

## 15. 脑裂（split-brain）与授权仲裁 —— Steamworks 可用工具

> 背景（问题 P7/§14.2）：**非对称断链**时（client↔旧 host 断、但 client↔其他 client 通），
> client 会选举并广播 `Takeover`，而旧 host 收不到 `Takeover`（`superseded` 不生效）→ **双权威/脑裂**。

### 15.1 Steamworks 可用工具（已查证 0.13.1 实际 API）
| 能力 | API | 对防脑裂的价值 |
|---|---|---|
| **每 peer 连接状态** | `ISteamNetworkingMessages::GetSessionConnectionInfo(...).state` → `NetworkingConnectionState::{None,Connecting,FindingRoute,Connected,ClosedByPeer,ProblemDetectedLocally}`（另有 `realtime.connection_state()`） | **高**：区分「链路真的断了」与「只是应用层没帧」。比“3s 无帧”可靠得多。 |
| **大厅唯一 owner** | `ISteamMatchmaking::GetLobbyOwner` → `lobby_owner()`（文档原文：*“There is guaranteed to always be one and only one lobby member who is the owner.”*） | **中高**：Steam 保证“恰好一个 owner”，可作权威/仲裁回退。⚠ 0.13 **无 `SetLobbyOwner`**；owner 离开后是否移交未文档化（大厅可能被销毁）。 |
| **大厅聊天消息** | `ISteamMatchmaking::SendLobbyChatMsg`（经 **Steam 后端**广播，非 P2P relay；≤ 4KB，带宽有限） | **高**：走**另一条传输路径**，即使 P2P relay 不对称断链也能到达全体大厅成员 → 适合做「权威宣告 / epoch / 心跳」。 |
| **会话失败回调** | `session_failed_callback`（已注册） | 低中：辅助信号。 |
| 主机迁移 / 授权仲裁 API | **不存在** | — |

### 15.2 建议方案（**已按 2026-10-08 实测修订**）
- ~~**A. 连接状态门控选举**~~ **【已放弃】**：V2 实测表明 `get_session_connection_info().state` 在 peer 被
  杀后**长时间仍为 `Connected`**（~7s 后才 `None`），而帧级检测 ~4s 就完成接管。用它门控只会拖慢/阻断迁移。
  最多只能作“反向确认”：`None`/`ClosedByPeer` 一定死；`Connected` **不**等于活。
- **C. 以 `lobby_owner()` 为权威（【升级为主方案】）**：V1 实测证明 **房主离开后 Steam 会自动把 owner
  移交给留下的成员（几乎即时）**，且 `lobby_owner()` 是**全员一致**的权威。故可把“新 host = 当前 `lobby_owner()`”
  作为选举，**替代**现在的“最小 SteamID”启发式——确定性 + 由 Steam 仲裁、天然无脑裂。
- **B. 权威 fencing（走大厅元数据，【修订】）**：新 host 把 **`authority = "epoch:hostid"` 写入大厅元数据**
  （`set_lobby_data`，**只有 owner 能写**）；各端定期 `lobby_data` 读取，只认最高 epoch，hostid 变了就 retarget。
  旧 host 只要还连得上 Steam 后端，读到更高 epoch 即**退位**。
  > **为何不用大厅聊天**：聊天内容只能在回调作用域内读（`chat_id` 出回调即失效），而回调闭包要求 `Send`，
  > 但 `steamworks::Client` 内含 `Manager`（原始指针）为 **`!Send`**，无法带进回调 → 解析聊天内容做不了。
  > 而**元数据读写无需回调**，且只有 owner 能写——恰好与 **C（host=owner）** 合体。

> 修订后的推荐：**C（election = lobby_owner）为主 + B（owner 写大厅元数据 epoch）为兜底**；**A 不做**。
> 真正的网络分区无 API 能完全根除；C 提供一致权威，B 把“双权威窗口”降到极小。
> V1 实测：owner 离开后大厅**仍存活**且 owner **自动移交** → 元数据通道可用。

### 15.3 決策（防脑裂）—— 已定（2026-10-08）
- **SP-Q1**：**C（选举 = `lobby_owner()`）为主 + B（owner 写大厅元数据 epoch）为兜底**；**A 撤回**（V2 证明连接状态不可靠）。
- 0.13.1 已含所需 API（`lobby_owner`/`set_lobby_data`/`lobby_data`），**无需升级组件版本**。

### 15.4 A+B+C 详细设计
**A. 连接状态门控选举**
- net-steam 新增 `peer_connection_state(transport, peer_id) -> Option<NetworkingConnectionState>`（封装 `get_session_connection_info().state`）。
- 迁移阶段 A 超时后**不立即选举**，先看旧 host 的 state：
  - `Connected` → 链路仍在，**延长探测**（可能是 host 应用层卡顿，不是掉线）；
  - `ClosedByPeer` / `ProblemDetectedLocally` / `None` → 确认掉线 → 选举。
- 也可用 `realtime.connection_state()` 双重确认。

**B. 权威 fencing（大厅元数据，【修订】）**
- 引入**权威代次** `Authority { epoch: u64, host_id: u64 }`；对局开始 epoch=0（host=建房者，也是 owner）。
- **当前 host = lobby owner**（见 C），故它**有权限** `set_lobby_data(ROOM_AUTH_KEY, "epoch:hostid")`：
  - 开局写 `0:<host>`；每次迁移写 `epoch+1:<新host>`。
- 各端（含旧 host）定期（~1–2s）`lobby_data(ROOM_AUTH_KEY)` 读取：
  - 只认**最高 epoch**；若 `hostid` ≠ 当前权威 peer → `retarget_host` 并向该 host 请求快照/对齐。
  - 旧 host 读到更高 epoch → `superseded` 退位（不再产帧）。
- 优点：**无需解析聊天内容**（绕开 `!Send` 阻塞）、后端一致性由 Steam 保证、与 owner 移交天然契合。
- 协议：`Packet::Takeover` 可选加 `epoch` 字段（运行期包，不进 World → 不升 `PROTOCOL_VERSION`；改了 `proto.rs` 补 roundtrip）。

**C. 选举 = `lobby_owner()`（主方案）**
- V1 实测：房主离开后 Steam **自动且近乎即时**把 owner 移交给留下的成员 → 新 host 定义为**当前 `lobby_owner()`**，
  **替代**“最小 SteamID”启发式。全员读到的 owner 一致 → 天然无脑裂，且新 host 具备写元数据权限（支撑 B）。
- 边界：owner 必须仍是**本局参与者/在线**（正常成立；若加入者中途进大厅而非参与对局需单独处理）。

> ✅ 前提已由 V1 实测确认（大厅在 owner 离开后仍存活、owner 自动移交、跨成员聊天双向）。

---

## 16bis. 崩溃重连（Crash Reconnect）—— host 与 client 均可重开回归

> 用户目标（2026-09-27）：**最终要实现“进程崩溃后重开→回到原对局”**；可能需要「手动暂停 / 等待掉线者重连」。
> 这是一项大工程，需先解决下列**架构缺口**。

### 16bis.1 关键缺口
- **G1【最关键：快照不含 meta】】** `world_ser` 只序列化 `World`；`MatchState`（回合/阶段/金币/profiles/技能绑定）**不在快照**。
  重连只重建 `self.world`（`poll_steam_migration`/旧 `poll_steam_reconnect`），meta 保留本端旧值 → 跨回合/学习阶段重连会**meta 落后/分歧**。
  → 需把 `MatchState` 也序列化进快照（`Packet::Snapshot` 携带 `world_bytes + meta_bytes`）。**这是状态字段 → 必须升 `PROTOCOL_VERSION` + 补往返测试**。
- **G2【重开后如何发现对局】** 崩溃重开 = 新进程，不知道 lobby id / 当前 host / 自己槽位。
  → 需**本地持久化会话描述**（lobby_id / my_id / host_id / participants / epoch），重开时读回 → 重进 lobby → 拉快照归队。干净退出/离场时清除。
- **G3【host 崩溃后以 client 归队】** 见 §16：需 `HostLockstep::into_transport()` + main.rs “被取代→转 client 归队”路径；
  且新 host 要能**宣告自己**（靠 B 的 epoch 心跳 / lobby 后端）。
- **G4【社交/暂停语义】** 重连窗口内其他人怎么办：
  - 现状：host 用**默认输入占位**（该角色站桩可被击杀），其余继续——**不暂停**；
  - 可选：**协调暂停**（host 停产帧，各端冻结，同时**抑制 stale→迁移**）等崩溃者回来；触发方式（host 手动 / 投票 / 掉线自动+超时）需定。
- **G5【大厅存活性】（同 B 前提）** 原 host 离开/崩溃后大厅是否还在，决定 G2/G3 能否用 lobby 发现。

### 16bis.2 分阶段（建议）
- **CR0 真机验证**（§18 V1）：owner 离开后大厅/大厅聊天是否存活；`get_session_connection_info` 在断链/掉线的实际取值。
- **CR1 快照带 meta（G1）**：写 `MatchState` 序列化（建议 `meta_ser` 对称 `world_ser`）+ 快照包扩展 + 往返测试 + `PROTOCOL_VERSION++`。
  → **也直接修复现有迁移/重连的 meta 隐患**（不是只为崩溃重连）。
- **CR2 会话持久化 + client 崩溃重开归队（G2）**：本地会话文件 + 重开提示「重新加入上一局」+ 重进 lobby + `ReconnectReq` + 快照（含 meta）+ `apply_resync`。
- **CR3 host 崩溃重开 / 被取代归队为 client（G3）**：`into_transport` + 转 client 路径 + 权威发现（B 的 epoch）。
- **CR4（可选）协调暂停 / 重连窗口 UX（G4）**：控制消息 `Pause{on}` + 暂停期间抑制迁移 + 界面倒计时。

### 16bis.3 影响的模块
- `game-core`：`meta_ser`（新）+ `PROTOCOL_VERSION`。
- `net`：`Packet::Snapshot` 携带 meta；`Takeover` 携 epoch；`HostLockstep::into_transport`；补无头测试。
- `net-steam`：`set_lobby_data`/`lobby_data` 封装（owner 写权威、全员读）；`lobby_owner()` 封装；连接状态封装（诊断用）。
- `client`：会话描述持久化（新文件，类似 `local_settings`）；重开时「重新加入上一局」入口；归队路径；暂停 UI（CR4）。

### 16bis.4 待決（崩溃重连）—— 部分已定
- **CR-Q1** ✅：先做 **CR0（验证）+ CR1（快照带 meta）**。
- **CR-Q2** 🟡：暂停参照 WC3/Dota2（见 §20），待选模式。
- **CR-Q3** ✅：host **整局保槽**（不硬过期，仅 UI 标“掉线中”）；client 前 60s 自动重试，之后允许手动/崩溃重开再接。
- **CR-Q4** ✅：**LAN 不做**（D4/D5 重申，专注 Steam）。

---

## 18. 关键验证清单（真机，未做前不写代码的部分）
- **V1（gates B / G5）**：房主（lobby owner）离开后：`lobby_members()` 是否仍返回其余成员？其余成员能否 `send_lobby_chat_message` 并收到 `LobbyChatMsg`？`get_lobby_owner()` 返回什么（0？移交？）？
- **V2（gates A）**：断开网络/杀进程时，`get_session_connection_info(peer).state` 的实际取值序列（Connected→?；多久变 `ProblemDetectedLocally`/`ClosedByPeer`）。
- **V3（gates 迁移正确性）**：跨回合/学习阶段触发迁移后，两端 `meta.round`/金币/技能是否一致（验证 G1 影响面）。

### 18.1 实测结果（2026-10-08，`--netdiag`，账号 xvzan 作“留下的观察者”）
日志：`logs/console-menu-full-20261008-214336.log`。场景：xvzan 作 client 加入 `...062` 的房；开局后 **host `...062` 被杀**。

**V1（房主离开后大厅）—— ✅ 通过（重大利好）**
- 房主离开后，`owner` **自动移交给留下的成员**：`owner=...062 → ...466`；`members=[...062,...466] → [...466]`。
- 大厅**仍存活**：`lobby=109775244388432648` 一直打印；`lobby-chat send ok=true`；`LobbyChatMsg` 回调照常触发。
- owner 移交**几乎即时**（与帧骤停同一时刻，≤0.5s）。
→ **B（epoch fencing 走大厅后端）可行**；而且 **C 从“辅助”升级为“强方案”**：Steam 自动移交 owner，`lobby_owner()` 是一个**全员一致、且几乎即时**的权威信号。

**V2（断链时连接状态）—— ⚠ 状态不可靠**
- 房主被杀后 `conn[...062]` **长时间仍为 `Connected`**；直到**接管完成后** (~kill 后 ~7s) 才变 `Some(None)`。
- 而帧级检测在 **~4s**（3s stale + 1s 探测）就完成了接管（日志：`host sim gap 4072ms`）。
→ **A（用连接状态门控选举“Connected 就继续等”）会拖慢/阻断迁移 → 应放弃 A**。
  （状态最多只能作“反向确认”：`None`/`ClosedByPeer` 一定死；`Connected` **不能**当“还活着”。）

**V1 补充实测（2026-10-08，`logs/console-menu-full-20261008-214839.log`，两端均 `--netdiag`）—— ✅ 跨成员聊天确认**
- 房主在时：`LobbyChatMsg recv` 同时出现 `from=...466`（自己）与 `from=...062`（对方），**双向互通**。
- 房主离开后：`owner=...062 → ...466`、`members=[...466]`、`send ok=true`、大厅 id 不变。
→ B（epoch 走大厅后端）完全可行；C（`lobby_owner()` 作权威）仍有强实证。
（注：本局只有 2 账号，owner 走后只剩 1 人，故“owner 走后剩下 N≥2 人互通”未单独测，但后端路径已证双向可用。）

---

## 19. 重规划（双账号可测；**每步一个 commit**，另一端 `git pull` 后真机验收）

> 前提已变：**现在可以双账号测试**，且**只做 Steam**（LAN 冻结）。因此把「先验后改」具体化为工具链：
> **每步一个 commit**，commit 信息里附「真机验收清单」，你在另一账号拉取后按单跑。

### 19.1 路线（每项 = 一个可提交、可验收的步骤）
| 步骤 | 内容 | 依赖 | 需要双账号？ |
|---|---|---|---|
| **R0 ✅** | **诊断探针**（已落 `7f5f209`） | — | ✅ 已跑 V1/V2 |
| **R2** | **CR1**：快照带 `MatchState`（meta），修跨回合重连隐患（**升 `PROTOCOL_VERSION`**） | — | ✅（V3） |
| **R3'** | **C**：选举改用 `lobby_owner()`（Steam 仲裁的新 owner 即新 host） | — | ✅ |
| **R4** | **B**：owner 写**大厅元数据** epoch 权威（防脑裂兜底） | R3' | ✅ |
| **R1** | **E3**：掉线/迁移 UI + 时序优化（S1–S6，无协议改动） | — | 可单机看 UI；双账号更佳 |
| **R5** | **CR2**：会话持久化 + client 崩溃重开归队 | R2 | ✅ |
| **R6** | **CR3**：host 崩溃 / 被取代 → 转 client 归队 | R4/R5 | ✅ |
| **R7**（可选） | **暂停**（CR-Q2 定后） | R1 | ✅ |

> ~~R3（A：连接状态门控选举）~~ **【已撤回：V2 证明状态不可靠】**。实施顺序改为**正确性优先**：
> `R2 → R3' → R4 → R1 → R5/R6`（先堵正确性与脑裂风险，UI 放后；UI 对着稳定状态机只写一次）。

### 19.2 为何先做 R0 / 为何改正确性优先
- R0 已用双账号实测回答 V1/V2（见 §18.1），并**改变了方案**（放弃 A、C 升主、B 改元数据）。
- 用户选择**正确性优先**：先把 R2（跨回合 meta 分歧）+ R3'/R4（脑裂）堵上，再做 R1 的 UI。
  UI 后做的好处：对着**最终稳定**的迁移/选举状态机写一次，不返工。

### 19.3 每步的 commit 约定
- 一个 commit 只做一件事；message 末尾附 `真机验收：…`（列 2–4 条可操作步骤）。
- 纯代码/文档不限；涉及协议（R2 `PROTOCOL_VERSION++`）要在 message 里标出。

### 19.4 R0（诊断探针）使用与验收 —— ✅ 已落（见 §19.1 R0 行）
**开启**：启动加 `--netdiag`（如 `client.exe --netdiag` / `run-steam.ps1` 里加参数）。默认关闭，不影响行为。
**日志关键行**（`client.log` / stderr）：
- `[netdiag] lobby=<id> owner=<id> members=[...] me=<id>` —— 每 ~2.5s（看大厅是否存活 / owner 是否变）。
- `[netdiag] conn[<peer>]=<State>` —— 每 ~0.5s（看断链时状态迁移；`Connected`/`ProblemDetectedLocally`/`ClosedByPeer`/`None`）。
- `[netdiag] lobby-chat send ok=<bool>` —— 本端广播大厅聊天是否成功。
- `[netdiag] LobbyChatMsg recv: lobby=.. from=.. type=..` —— **收到**大厅聊天（证明后端路径通）。

**验收（V1）**：双账号在房间/对局中；**房主退出/杀进程**后，观察余下账号：
- `lobby=` 行是否还在打印（大厅是否存活）；`owner=` 是否变成别人/0；`members=[...]` 是否还列得出来；
- 余下账号（若有不止一个）能否继续 `lobby-chat send ok=true` 且对方收到 `LobbyChatMsg recv`。
→ 这直接决定 **B（epoch fencing）能不能做**。

**验收（V2）**：两端对局中，一端断网/杀进程；记录另一端 `conn[peer]` 的**状态序列与时延**（多久从 `Connected` 变其它）。→ 决定 **A** 的阈值。

---

## 20. 参照：War3 / Dota2 怎么处理掉线、暂停、重连

> 大致行为（供设计参照，细节以各自版本为准）：

| 维度 | 魔兽争霸 3（经典自定义） | Dota 2 | 我们（host 权威 lockstep + 快照） |
|---|---|---|---|
| 网络架构 | P2P 锁步（无专用服务器） | **专用服务器权威**（状态同步） | host 权威 lockstep + 周期快照（**近似“把 host 当小服务器”**） |
| 掉线是否自动暂停 | **不自动**：掉线方单位采立，有文字提示 | **自动暂停**（约 2 分钟窗口，提示“X 已断线”） | 待定（CR-Q2） |
| 重连 | 经典自定义基本都是“掉线即退出”，无可靠重连 | **自动重连 + 从服务器状态同步** | host 快照重连/迁移（已部分做） |
| 放弃判定（abandon） | 掉线基本即算离开 | 约 5 分钟无重连算 abandon | host **整局保槽**（CR-Q3） |
| 手动暂停 | **有**（弹提示；正式比赛有限制） | **有**（有暂停预算/次数限制） | 可选（R7） |

**分析（对我们）**：我们的架构实际上是“轻量服务器”（host=小服务器、快照=状态同步），
**在能力上更接近 Dota2**（能自动重连），而不是纯 WC3。所以：
- 重连/状态同步：照 Dota2（我们已有快照），继续完善（meta 快照 + 发现/归队）。
- 暂停（CR-Q2）：WC3 与 Dota2 **都有手动暂停**；Dota2 额外有“掉线自动暂停”。
- 防滥用：**借用 Dota2 的“暂停预算/次数限制”**（避免一个人反复冻结全场）。

### 20.1 CR-Q2 三选
| 选项 | 含义 | 优点 | 缺点 |
|---|---|---|---|
| **P0 无暂停** | 掉线坐立，其余继续 | 最简、无滥用 | 等待体验差 |
| **P1 仅 host 手动暂停/继续** | host 按键（或投票）暂停 | WC3/Dota2 都这么做；可控 | 依赖 host 自觉；需 UI |
| **P2 掉线自动暂停 + 有限窗口** | 检测掉线就冻结等重连，超时继续 | Dota2 式，等待友好 | **可能被滥用**；需暂停预算/次数限制 |

**建议**：本轮 **P0**（继续用“掉线坐立占位”）；把 **P1** 作为 R7 轻量项（host 手动，带提示）；
**P2** 等崩溃重连（R5/R6）到位且加了暂停预算后再评估。

### 20.2 待确认（重规划）
- **RP-Q1**：按 §19.1 路线（R0→R1→…）推进，每步一 commit + 验收清单？
- **RP-Q2**：CR-Q2 先按 **P0**，P1 列入 R7，P2 待定？

---

## 16. 老 host 被接管后能否重新连回来？

### 16.1 现状
- 旧 host 收到新 host 的 `Takeover`（`notify_old_host_takeover` 单发）→ `HostLockstep::is_superseded()` = true
  → main.rs 直接 `reset_to_main_menu()`（**丢失本局**）。
- 若旧 host **根本收不到** `Takeover`（非对称断链）→ 它继续产帧（僵尸权威）→ 脑裂（§15）。
- 若旧 host 只是短暂卡顿、网络恢复：它仍在产帧，但客户端已迁移；它只能靠收到 `Takeover` 才知道退位——目前退到主菜单。

### 16.2 “作为 client 归队”可行性分析
**结论：技术可行**，接线大部分已存在：
- 新 host 的 `HostLockstep` 的 `client_identities` 已含旧 host 的 SteamID（`takeover` 按 `participants` 建），
  且 `auto_drop_idle` 已把它标为 `dropped`；旧 host 发 `ReconnectReq{SteamID}` → `unmark_dropped` **即可接回**。
- 旧 host 从收到的 `Takeover{participants}` 就能得知新 host 的 SteamID。
- 世界索引：原始 `participants` 不变 → 旧 host 用原 index 续打，不会重排。
- **缺的接线**：
  1. `HostLockstep` 没有 `into_transport()`（只有 `transport_ref()`）→ 需新增，才能把同一条 `SteamTransport`
     从 HostLockstep 取出、重建为 `ClientLockstep`（参考 `takeover()` 反向）。
  2. main.rs 需新增“被取代 → 转 client 归队”路径：取出 transport → 建 `ClientLockstep{host=新host}` →
     `send_reconnect_req` → 收 `Snapshot` → 重建 World → `apply_resync` → 续打。

### 16.3 分情况
| 情况 | 能否归队 | 说明 |
|---|---|---|
| host 短暂卡顿/分区后恢复（进程未退） | ✅ 可（若实现 16.2） | 目前是退主菜单，改善空间大 |
| host 进程崩溃后重开 | ⚠ 需额外流程 | SteamID 稳定，可重开→重进大厅；但需把“重进大厅”接到“拉快照归队”而非新建对局 |
| 非对称断链（收不到 Takeover） | ❌ 当前不行 | 同 §15；需 B 方案的 lobby-chat epoch 才能可靠感知被取代 |
| host 主动离场（退出菜单） | ❌ 不应归队 | 属主动 abandon（§2.2），不重连 |

### 16.4 待決（老 host 归队）
- **RH-Q1**：把「**被接管的新旧 host 转为 client 归队**」作为一个独立步骤（E6？）实现？还是先只改进提示（退主菜单前告知“你已被接管”）？
- **RH-Q2**：若实现归队，崩溃重开后的“重进大厅→拉快照归队”也要一并做吗（更复杂）还是先只做“进程未退的短暂断链归队”？

### 16.5 与 E3 的关系
- E3 本轮**只做 UI/时序优化**（§14），不碰归队与防脑裂；归队（§16）/防脑裂（§15）/崩溃重连（§16bis）作为后续独立专题。
- **建议排期**：E3（UI/时序）→ CR0/V1–V3（验证）→ CR1（快照带 meta）→ A+C → B（epoch fencing）→ CR2/CR3（崩溃重连）→ CR4（可选暂停）。
