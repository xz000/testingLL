> **历史归档（2026-10-17）**：H1–H4 已完成并真机验收；当前状态见 `HANDOFF.md`。

# 「连回上一局」检测机制 —— 优化规划（REJOIN_DETECT_PLAN）

> 起因：现有归队检测是**纯本地文件 + 时间窗/协议版本**过滤，无法判断"上一局是否真的还在"。
> 本文件先梳理现状与弱点，再给出分步优化方案（**先规划，未开工**）。

---

## 1. 现状（实现事实）

| 环节 | 位置 | 行为 |
|---|---|---|
| 落盘 | `persist_rejoin_session` / `broadcast_steam_leave` | 写 `rejoin.txt`：`{lobby_id, match_key, my_steam_id, participants, protocol_version, saved_at_unix}` |
| 检测 | `main.rs:1808`（启动）/ `reset_to_main_menu(true)`（`main.rs:8370`） | `rejoin::load()` 后过滤：`is_fresh(now, rejoin_window)` && `protocol_version == PROTOCOL_VERSION` |
| 提示 | `main.rs:12231` | 「检测到未结束的上一局：按 Y 重新加入，Esc 忽略」 |
| 动作 | `main.rs:6870` | Y → `steam_rejoin_target` + `steam_join_lobby_id` → `enter_steam_mode(join)` → `ReconnectReq` 握手等 `Snapshot` |
| 清除 | `reset_to_main_menu(false)` / 归队失败 | `rejoin::clear()` |

`RejoinSession`（`client/src/rejoin.rs`）字段：`lobby_id / match_key / my_steam_id / participants / protocol_version / saved_at_unix`（`FORMAT_VERSION=1`）。

---

## 2. 弱点（要解决的问题）

- **W1 无法确认"上一局还在不在"**：只靠时间窗；房间已关 / 对局已结束 / 我们已被移出，仍会提示归队（点了才发现失败）。
- **W2 会话信息贫乏**：没有 `phase`/`round`/`host_id`/房间名 → 提示只能泛泛而谈，也无法据此判断"已结束"。
- **W3 协议不匹配静默丢弃**：用户看不到"版本不同，无法连回"。
- **W4 `match_key` 未校验**：不同房间类型/旧数据可能误提示。
- **W5 只在启动/回菜单时检测**：运行中掉线（如"被移出大厅")后若未回菜单，无入口。
- **W6 结束路径可能漏清**：若对局结束未走 `reset_to_main_menu(false)`，会残留提示。

---

## 3. 优化方案（分步，可独立取舍）

### P1 lobby 探活（核心）
显示/保留提示前，用 **Steam 后端**探测该大厅的真实状态（与 P2P 无关）：
- `request_lobby_data(lobby)`（已有）刷一次 → 读 `lobby_owner` / `lobby_member_ids`；
- **判定**：
  - 大厅不存在 / 不属于我们 / 成员仅剩自己 → **不提示**（或提示"上一局已结束"）；
  - 仍有多人在 → 提示"可归队"。
- 实现位置：`reset_to_main_menu(true)`/启动时**异步**探活（不阻塞菜单）；结果更新 `steam_rejoin_prompt`。

### P2 丰富会话（`FORMAT_VERSION` +1）
在 `rejoin.txt` 增加：`phase`、`round`、`host_id`（当前 owner）、`room_name`、`last_seq`、`ended`。
- 提示可显示：**「上一局：第 N 轮 · 3 人 · 房主 <名> · 按 Y 归队」**；
- `ended=true` 时不提示。

### P3 补齐"清会话"路径
- 对局正常结束（`Finished→menu`）已清；
- 补：**全员退出=局散**、**被移出大厅（orphan）**、**归队超时**等路径也清/落。

### P4 协议不匹配**给提示**（不再静默）
- 显示「上一局版本不同（vX≠vY），无法连回」；或提供"忽略"。

### P5 运行中掉线的入口
- 大厅"孤岛→回菜单"、对局"迁移失败→回菜单"时，确保**已落一份最新会话**（多数已由 `persist_rejoin_session` 覆盖），使回菜单即可提示归队。

### P6 `match_key` 校验
- 载入时校验 `match_key` 为本作值，否则忽略（防旧数据/他作数据）。

---

## 4. 建议实施顺序

1. **P6 + P4（最小、纯本地）**：先让"误提示/静默丢弃"变正确/可见。
2. **P2**：丰富会话（为 P1/P3 提供判定依据；改 `FORMAT_VERSION`）。
3. **P1**：lobby 探活（依赖 P2 的 `host_id`；用 Steam 后端，安全）。
4. **P3/P5**：补齐清理与入口。

### 测试要点
- 正常结束后续开 → **不**提示；
- 崩溃重开、房主仍在线 → **提示且能归队**；
- 房间已关 / 全员退出 → **不**提示（或提示"已结束"）；
- 协议不匹配 → **有**明确提示；
- 对局迁移失败回菜单 → 仍可归队。

---

## 5. 待决策
- D1 是否做 **P1 lobby 探活**？（见 §6.4：只能部分可靠，建议降级为辅助）
- D2 是否接受 **`FORMAT_VERSION` +1**（P2；旧 `rejoin.txt` 会被忽略，等价于"首次升级不提示一次"）？
- D3 提示信息是否要显示**轮次/人数/房主名**（P2）？
- D4 协议不匹配时的提示文案与动作（仅提示 / 提供忽略 / 仍然尝试）？

---

## 6. 【修正】基于本地**历史对局记录**的方案（更贴合需求）

### 6.1 为什么改用历史
现有是**单槽文件**（`rejoin.txt`），很多路径会**清/覆盖**它：
- 主动退房 / 大厅“房主已离开”覆盖层里按 Q/Esc/回车 → `steam_leave_room()` → `reset_to_main_menu(false)` → **`rejoin::clear()`**；
- 进入任何新房间 → `persist_rejoin_session()` → **覆盖**；
- 归队失败 → 也 `clear()`。

→ 只要中途做了无关操作，**上一局的归队入口就丢了**（用户反馈：“很容易就没了”）。
历史记录方案：**追加写、有界、不因无关操作丢失**。

### 6.2 存储设计
- 文件：`rejoin_history.txt`（与 settings 同目录）；**append-only + 有界**（保留最近 N=5 条）。
- 每条字段（在现有基础上扩展，`FORMAT_VERSION` 升级）：
  `lobby_id / match_key / my_steam_id / participants / protocol_version / saved_at`
  `+ round / phase / room_name / host_id / last_seq / ended(bool) / ended_reason`。
- “上一条未结束的对局” = 时间最新的 `ended=false` 条目。

### 6.3 “是否已结束”的判定（分级）
1. **观察到正常结束**（`Finished→menu`）→ 标 `ended`（reason=finished）；
2. **观察到局散**（全员退出 / 只剩自己）→ 标 `ended`；
3. **尝试归队失败**（`join_lobby` 报错 / 握手超时）→ 标 `ended`（reason=rejoin_failed）；
4. **超过 `rejoin_window`** → 标“过期”（软，不硬删，仍可手动尝试）。

### 6.4 关于“lobby 探活”（原 P1）的定位

> 更正（实现事实）：建房用 **`LobbyType::Public`**（`session.rs:629`），故我们的大厅**会出现在 Steam 大厅列表**里。
> `matchkey`（键=我们定，值=`remake_arena_v1`）由 host 写入大厅元数据（`session.rs:1057`），
> client 自动加入就是靠它过滤（`session.rs:1012`）。

**两条探活通道：**
1. **仍是大厅成员**：`RequestLobbyData` + `lobby_owner/members` —— 可靠（真机已验证能刷新出真值 owner）；
2. **已退房/崩溃后**：**`RequestLobbyList` + 按 `matchkey` 过滤 + 看列表里是否还有该 `lobby_id`**
   —— **Steam 后端查询、不依赖 P2P**，故**离开后也能探**（比我上次说的可靠）。
   - 注意：**锁房/不可加入**的大厅不会被搜索返回（但那种本来也归队不了，视为“不可归队”合理）。
   - 注意：列表可能有分页/条数限制，需取足够多条。

⇒ 因此 **探活可作为辅助依据**（通道 2 对“已退房”也有效）；但仍建议保底：
**“尝试归队 → 失败即标 ended”**（§6.3-3）作为最终 oracle。

### 6.5 UI
- 主菜单显示：**「上一局：第 N 轮 · M 人 · 房主 <名> · 按 Y 归队 / Esc 忽略」**；
- 不再因“退房/建房/回菜单”而消失；只有**明确结束**或用户 **Esc 忽略**才标 `ended`。

### 6.6 分期（修正后）
- **H1**：本地**历史**骨架（有界追加/读取/标 ended）+ 旧单文件迁移/忽略；
- **H2**：丰富字段（round/phase/room_name/host_id）+ 提示文案；
- **H3**：“结束/局散/归队失败”→ 标 ended 的接线；
- **H4**（可选）：lobby 探活作辅助；启动时清理/标过期。

### 6.7 待决策（修正）
- D5 历史容量 N 取多少？（建议 5）
- D6 `ended` 条目在主菜单是否还可“手动重试归队”？（建议：显示为“已结束（可手动尝试）”，默认不主动提示）
- D7 旧 `rejoin.txt` 如何处理？（迁移为历史首条 / 直接忽略）

---

## 7. H4 详细规划：用 `matchkey` + 大厅列表探活

### 7.1 结论：现成机制可复用（无需新建）
- `MATCH_KEY="matchkey"` / `MATCH_VALUE="remake_arena_v1"`（`session.rs:22/24`）；host 建房写入（`:1057`）；自动加入按它过滤（`:1012`）。
- 现成「列大厅」：`start_list_lobbies(beats)`（`:915`）→ 每帧 `tick_lobby_list()`（`:940`）→
  `LobbyListProgress::Done(Ok(Vec<LobbyInfo>))`；`LobbyInfo { id, owner, members, limit, name, note, mode, version, settings }`。
- **需小改**：`LobbyInfo` 增 `matchkey: Option<String>`（读 `lobby_data(l, MATCH_KEY)`），以便确认“这个大厅是我们的”。

### 7.2 两条探活通道
- **A（仍是成员）**：直接 `SteamTransport::request_lobby_data(lobby)`（已有）+ `session::lobby_member_ids/owner`。适合进程未退（如 orphan 回菜单）。
- **B（已非成员）**：`start_list_lobbies` + `tick_lobby_list` → 结果里找 `id == 目标` 且 `matchkey == MATCH_VALUE`。
  - 适合：崩溃/退房后；**Steam 后端查询、不依赖 P2P**；
  - 注意：**锁房/不可加入的大厅不会出现在列表**（视为“不可归队”，合理）；列表可能被条数上限截断（只需存在性）。

### 7.3 探活状态机（client，只读、无副作用）
- **触发**：主菜单出现 `steam_rejoin_prompt` 时；此后每 ~10s 复检（`RequestLobbyList` 有频率限制）。
- **流程**：1) 仍是成员 → 走 A；2) 否则 → 走 B（`start_list_lobbies(240)` ≈ 4s）；
- **结果**：
  - **alive** → 提示增强为「上一局：<房名> · M 人 · 房主 <名>（第 N 轮）· 按 Y 归队」；
  - **gone** → 把该历史条目标 `ended` 并**隐藏提示**（或显示“上一局已结束”）。

### 7.4 与历史/提示的接线
- 探活只针对**最新未结束**的历史条目；结果写回条目（`ended` / `last_seen_*`）。
- **保底**：即便探活说“在”，仍以**归队尝试**为准（join/握手失败 → `ended`）。

### 7.5 失败与边界
- `RequestLobbyList` 超时/失败 → **不改变** `ended`（保守：仍提示，让用户试）；
- 目标处于**锁房/已满**：列表不含/无法加入 → 视为不可归队（可提示“房间不可加入”）；
- 列表可能含**同 matchkey 的其它房间** → **只按 `lobby_id` 匹配目标**，绝不“随便挑一个”；
- 全程**只读**（不 join、不改大厅数据）。

### 7.6 测试矩阵
| 场景 | 期望 |
|---|---|
| 局仍在（房主在线） | 探活 alive；提示正常；Y 能归队 |
| 局已结束（全员退出/房主解散） | 探活 gone → 标 ended、不提示 |
| 崩溃重开、局仍在 | B 通道 alive → 提示可归队 |
| 进程未退、被移出大厅（orphan） | A 通道 gone → 标 ended |
| 锁房 | 视为不可归队（不提示 / 提示“不可加入”） |
| 网络断 | 探针超时 → 保守仍提示 |

### 7.7 待决策（H4）
- D8 复检间隔（建议 ~10s）与超时（建议 4s）；
- D9 gone 时：完全隐藏 / 显示“上一局已结束（不可归队）”；
- D10 是否需要在提示里显示房名/人数/轮次（依赖 H2 字段）。

---

## 8. 【收尾】H1–H4 已实施并真机验收（2026-10-15/16）

### 8.1 已做
| 步骤 | commit | 内容 |
|---|---|---|
| H1 | `5dd4e35` | **历史记录** `rejoin_history.txt`（追加+有界5、同 lobby 就地更新）；退房/回菜单**保留**，仅结束/失败/忽略才标 ended；旧 `rejoin.txt` 忽略 |
| H2 | `2c936c8` | 提示增强（轮次/人数/房名）+ 协议不匹配说明；忽略键 **Esc → Delete** |
| H1.2 | `c33c216` | 判据改为**“曾进入对局”**（`in_match`，非大厅）；对局中每 ~10s 刷新会话 |
| H4 | `2bf74cc`/`a7e2e5e`/`b2835f2` | **探活 = 直接查该大厅**：`RequestLobbyData(lobby)` → `LobbyDataUpdate{success}` + `matchkey` 复核 |

### 8.2 期间修掉的 bug
- **“1 人房”被当成上一局**（退房后被 Steam 销毁→Y 必失败）→ H1.2 改为按 `in_match` 判定；
- **归队后画面卡死**：归队快照把阶段切到 Fighting，但本帧仍在 Learning 分支跑倒计时→滑进 ClientWait（`91ba76f`）；
- **归队后无头像**：头像按 `roster.slot==player.id` 取，归队后 roster 未按权威参与顺序重建（`91ba76f`）→ 以及 `steam_net_ticks` 双递增致对局中头像/ping 不刷新的回归（`35b572d`）；
- **自动恢复后“无法操作”**：阶段 A 探测采纳了**周期广播快照**（`your_index=0xFF`）→ 本端 index 未纠正（仍是大厅槽位）→ 输入发到错槽位；改为**只采纳带本端 index 的定向快照**（`d09203a`）；
- **探活应答被丢弃**：`LobbyDataUpdate` 对“大厅自身数据”的 `member == 该大厅 id`（不是 0）→ 原 `member==0` 过滤把应答全丢（`a7e2e5e`）。

### 8.3 探活最终方案（为什么不是列表/join）
| 方式 | 满员但还在 | 已散 | 副作用 |
|---|---|---|---|
| 大厅列表搜索 | ❌ 不可加入的房间不出现 | ✅ | 无 |
| 静默 `join_lobby` | ❌ 满员进不去 | ✅ | **会真的进房** |
| **`RequestLobbyData`（只读）** | ✅ | ✅ `success=false` | **无** |

真机已验：`success=true → 仍在`；`success=false → 已不存在 → 标 ended`。节流 ~10s，等待超时→**保守保留**（不误杀）。

### 8.4 待决/未做
- H3 的“**局散**（全员退出/只剩自己）”无独立检测：**现已改为 host 继续单人对桩子打下去**（不自动结束，`253f1d9`）；
  因此这一局的 **lobby 仍在**（host 持有），探活会报“仍在” → 其他玩家的归队提示**仍保留**（按 Y 可回到该局、接管桩子）。
- D9“gone 时是否显示“已结束”文案”（当前：直接隐藏）。
