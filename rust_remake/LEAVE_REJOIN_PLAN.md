# 主动离开 + 三因归队 设计规划（LEAVE_REJOIN_PLAN.md）

> 目标（用户 2026-10-11）：
> 1. **主动离开（房主/玩家）立即生效**，不按掉线等待；
> 2. **不论主动离开 / 掉线 / 死机崩溃，都要能主动连回来**；
> 3. 世界玩家**数量/索引不变**（帧同步红线：否则两端 desync）。
>
> 本文只做规划，**未改代码**。关联：`EXIT_MENU_RECONNECT_PLAN.md`（E1 退出菜单已落；R1/R2/R4 已落；**R5/R6/E5 未落**）· `RECONNECT.md` · `archive/STEAM_MULTIPLAYER_PLAN.md`。
> 阅读：§0 现状 → §1 不变式 → §2 决策 → §3 设计 → §4 状态机 → §5 改动点 → §6 分阶段 → §7 测试 → §8 风险 → §9 待拍板。

---

## 0. 现状审计（代码事实，2026-10-11）

### 0.1 「离开」路径
| 入口 | 现状 | 位置 |
|---|---|---|
| 对局中 Esc →「返回主菜单」 | `EscapeChoice::LeaveMatch → leave_match() → reset_to_main_menu()` —— **全量拆除**：世界重置为 Solo、丢 `net_link`/`net_host_ls`、**`leave_lobby`（退出 Steam 大厅）**、清全部 Steam 状态 | `main.rs` 4761 / 7735 |
| Esc →「退出游戏」 | `ctx.request_quit()` | `main.rs` 4733 |
| 大厅/房间界面 `Q` | `steam_leave_room()` | — |
| 窗口关闭 | `quit_event` | — |

> **关键**：`leave_match` **不区分房主/玩家**、**不广播**、**不保留会话**、**直接退出大厅** ⇒ 主动离开者**席位消失**，且**没有归队入口**。

### 0.2 「掉线 / 死机 / 归队」路径
| 环节 | 现状 |
|---|---|
| host 判 client 掉线 | `auto_drop_idle(HOST_DROP_TICKS=180≈3s)` → `mark_dropped`（该端用**默认输入站桩占位**，世界继续、不卡全队）|
| client 判掉线 | `steam_cli_stale_ticks≥180` → 冻结世界 → **主机迁移**（Steam）|
| 归队（进程还在） | `ReconnectReq{identity,last_known_seq}` → host 按**稳定身份**找槽 + `unmark_dropped` ✓ |
| **崩溃重开** | ❌ **无法发现对局**：新进程不知道 `lobby_id`/host/自己槽（**缺会话持久化**）|
| 快照 | 已带 `MatchState`(meta)（R2，协议 40）✓；重连/迁移都用它 |
| `Packet` | **无 `Leave`/`Bye`**（主动离开无广播）|
| 大厅语义 | `lobby_owner()` 可靠、房主离开后**自动移交**（V1 实测）；`leave_lobby` **释放席位** |

---

## 1. 不变式（不可违反）

1. **世界 `players` 数量/顺序固定** —— 离场/掉线/归队都**不改**（用 `mark_dropped` 站桩占位）。
2. **`reset_to_main_menu` 的"全量拆除"不能用于"暂时离开"** —— 必须与"离场"分离。
3. **归队依赖快照（含 meta）** —— 已在（R2）。
4. **主动离开的广播是 best-effort** —— 丢包由现有 180 帧机制兜底。

---

## 2. 关键决策（待拍板；带推荐）

| # | 问题 | 选项 | 推荐 |
|---|---|---|---|
| **L-D1** | 主动离开者是否**退出 Steam 大厅**？ | **A** 不退出（保留席位，只在**对局层**标"已离开"）→ 归队=一键 `ReconnectReq`；**B** 退出大厅 → 归队=**重进大厅**(by 持久化 id)+`ReconnectReq` | **A**（归队路径最短、无需处理满员/关房；大厅本就是"可能回来的会话"容器） |
| **L-D2** | 房主**主动**离开 | **立即**广播 `Leave` + **立即触发迁移**（不等 3s stale）；收不到 Leave 的端仍走 3s 兜底 | 采用 |
| **L-D3** | 离场者角色状态 | **站桩占位**（同掉线，可被击杀）；"离场即无敌/幽灵化"= 玩法改动 | 站桩（保持现状语义） |
| **L-D4** | 槽位窗口 | **整局保槽**（不设硬超时；UI 标"掉线中/已离开"）| 采用（同 `EXIT_MENU…` CR-Q3） |
| **L-D5** | 归队入口 | ① 掉线覆盖层「重新连接」（已有）② **主菜单「重新加入上一局」**（会话在且对局未结束）③ 崩溃重开后**自动提示** | 全做 |
| **L-D6** | 主动离开者立刻回来 | **可以**（同一局内随时）；回来后清除"已离开"标记（`unmark`）| 采用 |

> **L-D1 取 A 的影响面**：Leaver 客户端回主菜单/或留在"已离开"界面时，**保持 Steam client + 大厅连接**（后台存活）→ 大厅成员列表里他还在（房间阶段 UI 若能显示则应标"已离开对局"）。Hmm — 若嫌"幽灵成员"别扭，可改为：**回主菜单即在大厅里保持"旁观态"**，别人看到的是"他在房间/已离局"。**细节见 §3.4。**

---

## 3. 设计

### 3.1 `Packet::Leave { identity }`（运行期包）
- 方向：**leaver → host**（玩家主动离开）；**host → all**（房主主动离开，best-effort）。
- host 收到 → **立即** `mark_dropped(identity)`（不等 180 帧）+ 记 `left_since`（UI 显示"已离开"）。
- **不进 `World` 状态 ⇒ 不升 `PROTOCOL_VERSION`**；改 `net/src/proto.rs` 需补 `roundtrip_all_packets`。
- 另可保留一个**语义化**名字（`Leave`/`Bye`），建议 `Leave`。

### 3.2 玩家主动离开（client）—— **L-D1=A** 版
```
Esc →「离开本局」二次确认
  ├─ 发 Packet::Leave{identity}（best-effort）
  ├─ 本地：保留会话描述（写 session 文件：lobby_id/host_id/my_id/last_seq/participants）
  ├─ 清对局状态（world→solo、丢 lockstep、清输入残留）
  ├─ **不 leave_lobby**（L-D1=A）；Steam client + 大厅保持连接（后台）
  └─ 回主菜单（若会话仍有效 → 显示「重新加入上一局」）
归队：点「重新加入上一局」→（直连仍在大厅）重建 lockstep → ReconnectReq → Snapshot(含 meta) → apply_resync → 回对局
```
> 若执行 **「退出游戏」**（关进程）：写会话文件仍保留（供崩溃重开归队）。

### 3.3 房主主动离开（host）—— **L-D2**
```
Esc →「离开本局」二次确认
  ├─ 广播 Packet::Leave{identity=我}（best-effort；尽力让所有人立即迁移）
  ├─ leave_lobby（房主走了，大厅 owner 会自动移交 → 其余端 owner 变化）
  └─ 本地：写会话文件（host_id=我、我的新角色=未来可能作为 client 归队）→ 回主菜单
其余端：
  ├─ 收到 Leave → 立即进入迁移（跳过 3s stale 等待）
  └─ 未收到 → 现有 3s stale / owner 变化兜底
原房主归队：作为 **client** 归队（见 §3.5 / R6=CR3）
```

### 3.4 「已离开」在大厅/房间中的表现（L-D1=A 的细节）
- `lobby_owner()` 仅房主**真正离开大厅**才变；玩家主动离开（A）时 owner 不变 ✓。
- 其他端大厅列表仍含 leaver（因他没 leave）→ **房间/结算 UI** 若要区分，用 **host 广播的 `left_since`/`dropped` 集合**（可选 `Participants` 或新状态包）。
- 若嫌复杂：**选项 A'** = 主动离开时 **`leave_lobby`**，但**本地会话文件**仍存 → 归队走 §3.5 的"重进大厅"。※ 这条与 B 等价，只是保留了自动重试能力。**细节待拍板。**

### 3.5 统一的归队（三因）
```
三因：主动离开(A) / 掉线 / 崩溃
共同点：都想回**原来那局**
路径（统一）：
  1. 有会话 且 lobby 仍存在 且 对局未结束
  2. 重建客户端 lockstep（若已在大厅：直连；否则 join_lobby(by id)）
  3. 发 ReconnectReq{identity=SteamID, last_known_seq}
  4. 收 Snapshot(含 meta) → apply_resync → 回对局
  5. host 端：按身份找槽 + unmark_dropped（整局保槽）
失败分支：大厅没了 / 对局结束 → 提示"上一局已结束" → 清会话
```
- **进程还在**（掉线/主动离开）：`steam_cli_ls` 可能仍在（掉线）；主动离开(A) 保持大厅 → 直连最快。
- **进程没了**（崩溃）：读 `session.json` → `join_lobby(id)` → 建 lockstep → ReconnectReq。

### 3.6 会话持久化（G2）—— 崩溃归队的前提
- 文件：`session.json`（与 `local_settings` 同目录），字段：
  `{ lobby_id: Option<u64>, my_steam_id: u64, host_id: u64, last_seq: u64, participants: Vec<u64>, saved_at_unix: u64 }`。
- **写**：进入对局时、迁移后、每次 `last_seq` 显著前进（节流，如每 2s）。
- **清**：主动「退出房间/解散房间」（真正结束会话）、对局正常结束、`join_lobby` 失败、或 `saved_at` 超龄（如 >2h）。
- 归队前**校验**：`lobby_data`/大厅仍存在 + 对局未结束（`last_seq` 仍在推进 / `Snapshot` 能取到）。

### 3.7 host 端槽位/窗口
- 保槽：`World` 固定 ✓；新增 `left_since: Vec<Option<u64>>`（帧号）区分"掉线"vs"主动离开"（仅 UI）。
- 窗口：**整局**（L-D4）；不物理移除（§1 不变式 1）。

---

## 4. 状态机（client 视角）

```
[房间就绪] ─start─▶ [对局]
   │                    │
   │      Esc→「离开本局」二次确认
   │                    ▼
   │            [主动离开] ──(best-effort Leave 广播)──▶ 其他人: 立即标"已离开"
   │                    │
   │        (写会话; L-D1=A: 保留大厅席位)
   │                    ▼
   │               [主菜单]  ←── 若会话有效: 显示「重新加入上一局」
   │                    │
   │                    └── 点「重新加入上一局」──┐
   │                                              ▼
   │                                   [重连中] ReconnectReq/迁移 ──成功──▶ [对局]
   │                                              │失败(关房/结束)
   │                                              ▼
   │                                        [清会话] 回主菜单
   │  （掉线/崩溃同理并入同一条归队路径）
   ▼
[退出游戏]（写会话保留）
```

---

## 5. 改动点

| 模块 | 改动 |
|---|---|
| `net/src/proto.rs` | 新增 `Packet::Leave { identity }` + 编解码 + `roundtrip_all_packets`。**不升协议** |
| `net/src/lockstep.rs` | host 侧处理 `Leave` → 立即 `mark_dropped`（复用现有 `mark_dropped`/`Participants`）|
| `net-steam` | 封装 `join_lobby(by id)`（已有 join 路径可复用）；`lobby_owner` 已有 |
| `client/src/main.rs` | ① `leave_match()` 分流：房主/玩家、广播、会话保留、`reset_to_main_menu` 拆分（对局拆除 vs 大厅拆除）；② 主菜单「重新加入上一局」入口 + 崩溃重开读会话；③ 迁移动机加"收到 Leave 立即迁移"；④ 掉线覆盖层接入三因的统一归队 |
| `client/src/session.rs`（新，或并入 `local_settings`）| `session.json` 读写/清除 |
| **不改 `World`/`world_ser`/模拟** → **不升 `PROTOCOL_VERSION`** |

---

## 6. 分阶段实施（每步 1 commit、可编译、有单测）

> 顺序按"低风险 → 高风险、每步可独立验收"。

| 步 | 内容 | 依赖 | 双账号真机 |
|---|---|---|---|
| **L1** | `Packet::Leave` + host **立即**标记（掉线兜底不变）；玩家主动离开发 Leave | — | 可（观察"立即已离开"）|
| **L2** | 房主主动离开 → **立即触发迁移**（广播 + 迁移动机） | L1 | ✅ |
| **L3** | **会话持久化**（session 文件）+ 主菜单「**重新加入上一局**」（进程还在场景）| — | ✅ |
| **L4** | **崩溃重开归队**（client）：读会话 → join_lobby(by id) → ReconnectReq → Snapshot | L3 + R2 | ✅ |
| **L5** | **原房主归队为 client**（R6=CR3 的 U2：自动尝试 + 覆盖层 + 超时回退）| L4 + R4 | ✅ |
| **L6** | （可选）房间/结算 UI 标"已离开" + i18n 文案 | L1 | 可 |

> 与 `EXIT_MENU_RECONNECT_PLAN.md` 的对应：**E5 = L1+L2**；**R5(CR2) = L3+L4**；**R6(CR3) = L5**。

---

## 7. 测试策略

- **纯函数单测**：归队决策树（`rejoin_decision(has_session, lobby_alive, match_running) -> Rejoin|Forget`）、会话文件 roundtrip、`left_since` 状态、`Leave → 立即 dropped`。
- **`net` 单测**：`proto` roundtrip（新增 Leave）；host 收 Leave 后 `all_cfgs`/帧不再等该端（复用 `host_all_cfgs_ignores_dropped_client` 思路）。
- **无头 e2e**（`netlink.rs` 模式）：A 主动离开 → 发 Leave → B 立即标已离开 → A `ReconnectReq` → 两端逐位一致。
- **真机双账号**（每步验收）：
  - L1：A 按「离开本局」→ B 视角**立即**显示"A 已离开"（不再等 3s）。
  - L2：房主 A 离开 → B **立即**开始迁移（不等 3s）→ B 接管。
  - L3/L4：A 主动离开/杀进程后 → 重开/主菜单点「重新加入上一局」→ 回到原局（同一槽位）。
  - L5：A 曾为房主、离开后 B 接管 → A 归队为 client，拿到 B 的快照续打。

---

## 8. 风险与边界

1. **帧同步红线**：一切"离开/归队"都不得改 `World` 玩家数/索引（用 `mark_dropped`）。
2. **Leave 丢包**：P2P 下 best-effort；**必须保留 180 帧兜底**（L1 不删旧机制）。
3. **L-D1=A 的"幽灵成员"**：主动离开者留在大厅 → 房间 UI 需按 `left_since` 标"已离开"；否则观感别扭（→ L6 / 或选 A'）。
4. **重进大厅失败**：满员/关房/对局结束 → 提示并清会话（不要卡在"重连中"）。
5. **崩溃重开的身份**：Steam=SteamID 稳定 ✓；LAN 随机（D4 不持久化）→ 本文归队**只保证 Steam**。
6. **会话文件陈旧/串局**：校验 `lobby_id` + 对局存活 + `saved_at` 超龄。
7. **主机迁移 × 归队交叠**：A 掉线期间 host 又走 → 迁移后 A 向**新 host** 归队（需真机验证）。

---

## 9. 待拍板（确认后开工）

1. **L-D1**：主动离开**是否退出大厅**？推荐 **A（不退出，保留席位）**；若要避免幽灵成员可选 **A'（退出大厅但保留会话文件→归队重进）**。
2. **L-D2**：房主离开**立即迁移** —— 同意？
3. **L-D3**：离场者**站桩占位**（可被击杀） —— 同意？（幽灵化属玩法改动，另议）
4. **L-D5**：主菜单「**重新加入上一局**」入口 —— 同意？
5. **实施顺序**：**L1→L2→L3→L4→L5**（推荐，先正确性后 UX）还是你想先做 L3/L4（崩溃归队）？

> 拍板后我会按 §6 逐 commit 实施（每步跑 `check.ps1` + 附真机验收清单）。
