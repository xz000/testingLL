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
- D1 是否做 **P1 lobby 探活**？（会让"误提示"消失，但需要异步探测+状态机）
- D2 是否接受 **`FORMAT_VERSION` +1**（P2；旧 `rejoin.txt` 会被忽略，等价于"首次升级不提示一次"）？
- D3 提示信息是否要显示**轮次/人数/房主名**（P2）？
- D4 协议不匹配时的提示文案与动作（仅提示 / 提供忽略 / 仍然尝试）？
