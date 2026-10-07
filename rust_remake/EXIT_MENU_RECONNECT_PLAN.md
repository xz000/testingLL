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
