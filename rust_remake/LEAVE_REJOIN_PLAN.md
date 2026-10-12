# 主动离开 + 三因归队 设计规划（LEAVE_REJOIN_PLAN.md）

> **目标**：对局中「主动离开」（房主/玩家）立即生效、不按掉线等待；且**主动离开 / 掉线 / 崩溃**三因都能**手动重新加入上一局**。世界玩家**数量/索引不变**（帧同步红线）。
>
> **状态（2026-10-11 修订）**：本版基于**真实代码现状**重写（上一版把 R5c/R6b 误标为"未做"）。
> **四项决策已定**（用户 2026-10-11）：
> - **Q1 = B**：主动离开**退出大厅**，但**保留归队会话**，归队走"按 id 重进大厅"（复用崩溃归队路径）。
> - **Q2 = 做**：新增 `Packet::Leave`，主动离开**立即生效**（房主立即触发迁移；非房主被 host 立即标记）。
> - **Q3**：归队会话**以"对局是否仍可加入"为准**（不设 30 分钟硬过期；仅留一个宽松的卫生上限）。
> - **Q4**：回主菜单**只提示、手动归队**（与现有 R5c 一致）。
>
> 关联：`EXIT_MENU_RECONNECT_PLAN.md`（E1 已落）· `archive/STEAM_MULTIPLAYER_PLAN.md` · `client/src/rejoin.rs`。

---

## 0. 现状审计（**准确版**，2026-10-11）

### 0.1 已经实现（无需再做）
| 能力 | 实现 | 位置 |
|---|---|---|
| **崩溃/掉线→归队会话** | `rejoin.txt`：`{lobby_id, match_key, my_steam_id, participants, protocol_version, saved_at}`；进房成功时 `persist_rejoin_session()` 写盘 | `rejoin.rs`；`main.rs:9289/9295` |
| **崩溃重开→主菜单提示→归队** | 启动读回（新鲜+协议一致）→ 主菜单"检测到未结束的上一局：回车/Y 重新加入，Esc 忽略" | `main.rs:1759/11521/6537` |
| **归队握手** | 按 id 重进大厅（`steam_join_lobby_id`）→ `steam_rejoin_handshaking`：节流重发 `ReconnectReq` 等 `Snapshot` → 重建 world+meta；超时 `REJOIN_TIMEOUT_TICKS=900`(15s) | `main.rs:7222/9101/9275` |
| **原房主转 client 归队（R6b）** | 被取代/自栅栏的旧 host 用 transport 重建 `ClientLockstep` 连新 host 归队 | `steam.rs`/`main.rs:9316` |
| **掉线检测（client）** | `steam_cli_stale_ticks>=180`(3s) → 迁移状态机（先 probe 原 host `MIGRATE_PROBE_TICKS=90`(1.5s)，再按 lobby_owner 选举） | `steam.rs:56` |
| **host 判 client 掉线** | `auto_drop_idle(180)` → `mark_dropped`（站桩占位，世界继续） | `net` |
| **房间阶段房主离开** | `steam_lobby_silent_ticks>=240` → "房主已离开"覆盖层 | `main.rs:8401` |

### 0.2 真正的缺口（本规划要做的）
1. **主动离开 = 全量拆除**：`Escape→返回主菜单` → `leave_match()` → `reset_to_main_menu()`，它**退出大厅** + **`rejoin::clear()`（删会话）** + 清空全部状态 ⇒ **主动离开者无法归队**、主菜单不提示。
2. **无 `Packet::Leave`**：房主要等 3s(stale)+1.5s(probe) 才被判定离开；非房主离开、host 要等 180 tick(3s) 才 `mark_dropped`。
3. **归队会话 30 分钟硬过期**（`rejoin::MAX_AGE_SECS`）⇒ 长局中途崩溃会失效。

---

## 1. 不变式
1. 世界 `players` 数量/顺序固定 —— 离场/掉线/归队都用 `mark_dropped` 站桩占位。
2. 所有"回主菜单"路径须**区分**"我离开本局（保留会话，可归队）"vs"干净退出/失败回退（清会话）"。
3. 主动离开的广播是 **best-effort** —— 丢包由现有 3s 兜底。
4. `Packet::Leave` 是**运行期包、不进 World** ⇒ **不升 `PROTOCOL_VERSION`**（改 `proto.rs` 补 roundtrip）。

---

## 2. 设计

### 2.1 归队会话生命周期（Q3）
**以"能否真的加入"为准，而非固定时长。**
- **写**：进房成功时（已有）；**并在迁移/参与集变化后刷新**（`participants` 会变，新增补丁）。
- **保留（不 clear）**：对局中主动离开、迁移失败、归队握手超时 → 回菜单仍可再试。
- **清除**：① 用户显式忽略提示（Esc）；② 用户"干净退出房间"（房间阶段 `Q` = `steam_leave_room`）；③ **对局确实结束**——由"重进大厅失败（join 报错）"或"提示被忽略"判定；④ 格式/协议不符（本来就会忽略）。
- **过期**：**去掉 30 分钟硬门限**；仅保留一个**宽松卫生上限**（建议 `12h`，可调，防止关了游戏隔几天还弹提示）。真正判定 = **归队时 `join_lobby` 成功且能收到 `Snapshot`**。
- **主菜单提示刷新**：现在只在**启动**读盘 ⇒ 增加"每次回到主菜单时"重新装载（同进程主动离开后立刻可提示）。

### 2.2 `Packet::Leave { identity }`（Q2）
- `net/src/proto.rs`：新增变体 + 编解码 + `roundtrip_all_packets`。**不升协议**。
- **client→host**（非房主主动离开）：host 收到 → **立即** `mark_dropped(identity)`（不等 180 tick），并且**从 `steam_online`/参与集移除**（否则**未来迁移可能把已离开者选为新 host**）。
- **host→all**（房主主动离开）：各 client 收到 → **跳过 3s stale + 跳过 probe**，直接进入选举迁移；收不到 → 现有 3s 兜底。
- 增强（可选）：房主离开前**再广播一次最终 `Snapshot`**，让新 host 的缓存快照更准（现有接管已依赖缓存快照）。

### 2.3 主动离开流程（Q1=B, Q4）
```
对局中 Esc →「返回主菜单」二次确认
  ├─（房主）广播 Packet::Leave{me}（best-effort）  /（玩家）向 host 发 Packet::Leave{me}
  ├─ persist_rejoin_session()（**保留**，不 clear）
  ├─ leave_lobby（退出大厅）
  ├─ 回主菜单：重置对局状态，但 **回填 steam_rejoin_prompt = Some(session)**
  └─ 主菜单立即显示「重新加入上一局」（手动回车/Y 归队）
归队 = 复用现有 R5c：join_lobby(id) → rejoin 握手 → Snapshot → 续打
```
- 房主主动离开：其二次确认文案已区分（"房主离开会让其他人接管"，`EXIT_MENU` E1 已落）。
- 归队失败（大厅已关/超时）：大厅已关 → 清会话 + 提示"上一局已结束"；仅超时 → **保留**会话（可重试）。

### 2.4 归队（三因统一）
- **掉线（进程在）**：走现有 `ReconnectReq` 重连/迁移。
- **崩溃（进程没了）**：现有 R5c（读 `rejoin.txt` → 重进大厅 → 握手）。
- **主动离开（新）**：同崩溃路径（会话被保留 ⇒ 主菜单提示）。
- 归队后：host 按稳定身份（SteamID）找回槽位 + `unmark_dropped`（整局保槽）。

### 2.5 槽位/窗口
- **整局保槽**（现状）。主动离开者站桩占位、可被击杀（Q/L-D3 已同意）。
- `steam_online`/参与集须在"主动离开"时更新，避免误选举。

---

## 3. 改动点
| 模块 | 改动 |
|---|---|
| `client/src/rejoin.rs` | 30min 硬过期 → 宽松卫生上限（12h）；保留 `saved_at` 供显示 |
| `client/src/main.rs` | ① `reset_to_main_menu` 拆口径（清/留会话）；② 回主菜单重载 rejoin 提示；③ 主动离开路径（发 `Leave` + 保留会话 + 回填提示）；④ 迁移 bail / 归队超时改为**保留**会话；⑤ 归队 join 失败 → 清会话+"上一局已结束"；⑥ host 处理 `Leave` 立即 drop + 移出参与集；⑦ client 收 host `Leave` → 立即迁移 |
| `net/src/proto.rs` | 新增 `Packet::Leave { identity }` + roundtrip |
| `net/src/lockstep.rs` | host `Leave` → 立即 drop 的口子（复用 `mark_dropped`） |
| **不改 `World`/`world_ser`/模拟** ⇒ **不升 `PROTOCOL_VERSION`** |

---

## 4. 分阶段实施（每步 1 commit、可编译、有单测）
| 步 | 内容 | 依赖 | 真机 |
|---|---|---|---|
| **L1** | **会话生命周期**：宽松过期、回菜单重载提示、`reset_to_main_menu` 拆"清/留"、迁移 bail/归队超时保留、归队 join 失败清 | — | 可 |
| **L2** | **`Packet::Leave` + 立即生效**：host 立即 drop/移出参与集；client 收房主 Leave 直接迁移；3s 兜底保留 | — | ✅ |
| **L3** | **主动离开接线**：Esc→返回主菜单（发 Leave + 保留会话 + 回填提示 + leave_lobby） | L1,L2 | ✅ |
| **L4** | i18n/文案 + 双账号验收清单 + 文档收口 | L1–L3 | ✅ |

> 顺序：先会话生命周期（地面）→ 再协议包 → 再接线 → 最后验收。

---

## 5. 测试策略
- **纯函数/单测**：`rejoin` 过期边界（新上限）；`reset` 清/留两径；host `Leave` → `mark_dropped` 且移出参与集；client 收房主 `Leave` → 进入迁移（跳过 probe）。
- **`net` 单测**：`proto` roundtrip（新 `Leave`）；复用 `host_auto_drops_idle_client` / `host_all_cfgs_ignores_dropped_client` 思路。
- **真机双账号**（每步）：
  - L2：非房主按返回主菜单 → 房主**立即**显示其站桩/已离开（不再等 3s）；房主离开 → 其余**立即**开始迁移（不再等 3s+1.5s）。
  - L3：主动离开者回菜单**立即**看到"重新加入上一局" → 回车回到原局（同一槽位）。
  - L1：归队握手超时后**会话仍在**、可再点；对局结束后重进失败 → 提示"上一局已结束"并清会话。

---

## 6. 风险与边界
1. **帧同步红线**：一切离场/归队不得改世界玩家数/索引。
2. **Leave 丢包**：best-effort；**必须保留 3s 兜底**（L2 不删旧机制）。
3. **误选举**：host 收到 `Leave` 后必须把该身份移出 `steam_online`/参与集，否则未来迁移可能选到已离开者。
4. **归队失败区分**：join 报错（大厅已关）= 对局结束 → 清；握手超时 = 网络问题 → 留（可重试）。
5. **P2P 广播的时序**：房主"发 Leave 后立即退房"，需确保可靠通道把 Leave 送出去（Steam 可靠包会排队；若有疑虑，加"退房后仍泵一帧回调"）。
6. **归队会话语义**：`protocol_version` 不符自动忽略（已有）；跨版本客户端不会误入。

---

## 7. 待确认的小点
1. 卫生上限取 **12h** 可以吗？（纯卫生；真正判定是"重进大厅是否成功"。）
2. **离开二次确认文案**是否要区分"房主/玩家"（玩家版可写"离开后可重新加入同一局"）？
3. 主动离开者**站桩占位**时，UI 是否要在他身上标"已离开"（与"掉线中"区分）？还是两者外观一致即可？（L2 已能拿到"立即离开"与"3s 掉线"的区分信号，做标识成本很低。）

> 以上确认后，我按 §4 的 **L1→L2→L3→L4** 逐 commit 实施（每步 `check.ps1` + 真机验收清单）。
