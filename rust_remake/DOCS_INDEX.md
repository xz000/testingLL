# 文档索引（DOCS_INDEX）

> 用途：仓库有 40+ 份 `.md`。本表给出**每份文档的角色与权威性**，避免新会话把**历史/过时**文档当真值。
> 入口仍是 **`HANDOFF.md`**；真值永远是 **098c 代码**（`098c/out/war3map.j`）。

## A. 权威 / 真值（活，改动须同步）

| 文档 | 角色 |
|---|---|
| `SKILL_ALIGNMENT_LEDGER.md` | **主台账（活）**：逐技能 098c 事实表 + 状态列；§11 基准审计；协议史 |
| `SKILL_CODE_AUDIT_098c.md` | **代码基准审计**：逐技能从 098c 代码抽 damage/duration/cooldown/range/speed 与 def 对照 |
| `JASS_FIELDS_098c.md` | 098c 两字母字段/回调语义（`Q/S`/`mI.lI`/`yO,oR`/`Jv`≠`Gv`/`kv` vs `Kv` 等） |
| `ITEM_AUDIT_098c.md` | 道具 24 格逐项（买/卖/效果） |
| `ECONOMY_RECHECK.md` | 经济奖励（起始金/每回合/计分） |
| `KNOCKBACK_ALIGNMENT.md` | 击退系数逐技能 |
| `098C_DIFF.md` | **系统级** 098b↔098c 差异（魔法张力/移动/物理/成长/计分/回血/岩浆），A1–A8 已结案 |

## B. 活跃设计/计划档（联机与表现，非技能真值）

| 文档 | 角色 |
|---|---|
| `EXIT_MENU_RECONNECT_PLAN.md` | 联机：Esc 退出菜单 / 重连 / 迁移 / 暂停（R 系列）设计与记录 |
| `STEAM_MULTIPLAYER_PLAN.md` / `STEAM_DEMO_PLAN.md` / `STEAM_UI_REDESIGN.md` | Steam 联机/UI 计划 |
| `RECONNECT.md` | 重连相关 |
| `ROOM_SETTINGS_PLAN.md` / `ROOM_UI_REVIEW.md` / `LOBBY_UI_PLAN.md` | 房间/大厅设置 |
| `KEYBINDS_PLAN.md` | 键位系统 |
| `ICON_PACK_PLAN.md` / `AUDIO_PLAN.md` / `AUDIO_SCRIPT.md` | 图标包 / 音频 |
| `I18N.md` | 多语言 |
| `TRAINING_PLAN.md` | 训练模式 |
| `SETTINGS_ALIGNMENT_AUDIT.md` | 房间设置对齐审计 |
| `AOE_VISUAL_AUDIT.md` | AoE 视觉审计 |
| `PRESENTATION_PLAN.md` / `UI_MASTER_PLAN.md` | 表现/UI 总纲 |

## C. ⚠ 已被取代 / 历史（**勿当真值**）

| 文档 | 说明 |
|---|---|
| ⚠ `archive/SKILL_PER_LEVEL_AUDIT.md` | 逐档表**按 tooltip** 建立，已多处与 098c 代码不符 → 用 `SKILL_CODE_AUDIT_098c.md` |
| ⚠ `JASS_AUDIT_098c.md` | 早期逐技能事实汇编；状态/数值以台账 + 代码审计为准（仅作函数/字段查阅） |
| `archive/P0_AUDIT_RECORD.md` / `archive/P0_RECHECK.md` | P0 阶段（S000/3/8/9/16）记录；多数已并入台账/代码 |
| `archive/CODE_PHASE_RECORD.md` / `archive/FIX_ROADMAP.md` / `archive/GAMEPLAY_FIX_PLAN.md` | 早期代码阶段记录 |
| `archive/HANDOVER.md` | 更早的交接说明（被 `HANDOFF.md` 取代） |
| `archive/SKILL_STATE_AUDIT.md` / `archive/MAGNITUDE_AUDIT.md` / `archive/UI_AUDIT.md` / `archive/DEAD_CODE_CLEANUP.md` | 早期审计 |
| `archive/FRAME_SYNC_*.md` | 帧同步方案比较/设计（历史决策） |
| `archive/PORT_098B_DECISIONS.md` | **098b 时期**的移植决策（历史；勿按 098b 行事） |
| `archive/RISK_ANALYSIS.md` / `archive/WORKFLOW_NOTES.md` | 早期流程/风险 |

> 说明：C 类中的 `archive/*` 已物理移入 **`archive/`** 子目录（顶部带历史横幅），保留作参考。
> 若发现与本表冲突，**以 A 类 + HANDOFF 为准**。
