# 文档索引（DOCS_INDEX）

> 用途：本表给出**每份 `.md` 的角色与权威性**，避免新会话把**历史/过时**文档当真值。
> 入口是 **`HANDOFF.md`**；真值永远是 **098c 代码**（`098c/out/war3map.j`）。
>
> **2026-10-17 整理**：根目录只留 **A（真值）+ B（在办）**；已完成/被取代的一律移入 **`archive/`**（顶部带历史横幅）。
> 检索 098c 用 `tools/jass_find.py <regex>` 等（见 `tools/README.md` §7：`jass_find`/`jass_where`/`jass_func`/`dump_tooltips`）。

## A. 权威 / 真值（活，改动须同步）

| 文档 | 角色 |
|---|---|
| `SKILL_ALIGNMENT_LEDGER.md` | **主台账（活）**：逐技能 098c 事实表 + 状态列；§11 基准审计；§12/§13 A2 移动；**§14 JASS↔Rust 全量对齐矩阵**；§15 缺口/近似深挖；§16 F5/F6 规划；§17 血剑 & F 槽；**§18 技能描述审计**；**§19 弹体回调集复核**；**§20 S016A 弹跳弹深挖**；**§21 第二轮逐回调深挖+修复**；**§22 近似/遗留复核结论**；协议史 |
| `SKILL_CODE_AUDIT_098c.md` | **代码基准审计**：逐技能从 098c 代码抽 damage/duration/cooldown/range/speed + 生成标志，与 def 对照 |
| `JASS_FIELDS_098c.md` | 098c 两字母字段/回调语义（`Q/S`、`mI.lI`、`yO,oR`、`Jv`≠`Gv`、`kv` vs `Kv`、`jn`、`Hr`…）|
| `ITEM_AUDIT_098c.md` | 道具 24 格逐项（买/卖/效果）|
| `ECONOMY_RECHECK.md` | 经济奖励（起始金/每回合/计分）|
| `098C_DIFF.md` | **系统级** 098b↔098c 差异（张力/移动/物理/成长/计分/回血/岩浆），A1–A8 已结案 |

> 击退系数明细原在 `archive/KNOCKBACK_ALIGNMENT.md`，已并入台账 **§14.3/§14.5** 并归档。

## B. 在办的计划/设计档（非技能真值）

| 文档 | 角色 |
|---|---|
| `HOST_MIGRATION_LEARNING_PLAN.md` | **学习期房主退出/主机迁移**（M0–M5、S1–S6 已完成；§16 迁移抖振+换主机系统暂停；§17 F 系列；§17.8 报递延迟；§17.9 E1/E2；**§17.10 陈旧 owner 自接管残余风险（暂不修，含成因/证据/P7-lite 方案）**）|
| `ALIGNMENT_REAUDIT_2026-10.md` | **增量对齐审计（只读）**：**F1/F2 已修复（协议 66）**；F3 = S012B 待核；TODO 清扫清单 |
| `RECONNECT.md` | 掉线/重连机制设计 |
| `STEAM_DEMO_PLAN.md` | Steam demo（AppID `1042120`）出包方案（已实现，待 4 项真机验证）|
| `PRESENTATION_PLAN.md` | 表现层（P1–P5 + 击杀信息流已完成；P6 观战未启动）|
| `TRAINING_PLAN.md` | 单机训练场分期（P0 ✅；**P1-lite ✅ 统计 HUD / F5 重置 / 岩浆·缩圈·障碍开关**；AI 反击远期）|
| `I18N.md` | 多语言（中文/English）|

## C. ⚠ 历史归档（`archive/`，**勿当真值**）

> 全部物理位于 **`archive/`** 子目录，顶部带历史横幅；保留作参考/追溯，**不是真值**。

| 文档 | 说明 |
|---|---|
| `archive/EXIT_MENU_RECONNECT_PLAN.md` | 联机 R 系列（Esc 退出菜单/重连/迁移/暂停；已完成；P7 残余见 `HOST_MIGRATION_LEARNING_PLAN.md` §17.10）|
| `archive/LEAVE_REJOIN_PLAN.md` | 主动离开 + 三因归队（L1–L5 已完成并真机验收）|
| `archive/REJOIN_DETECT_PLAN.md` | “连回上一局”检测（H1–H4 已完成并真机验收）|
| `archive/LEARNING_PHASE_NET_PLAN.md` | 学习期联机（E1–E3/E5 已完成并验收）|
| `archive/DISPLAY_CAMERA_PLAN.md` | 显示/相机设置（S1–S4 已完成；手感调整转入当前打磨计划）|
| `archive/SKILL_PER_LEVEL_AUDIT.md` | 逐档表**按 tooltip** 建立，已多处与 098c 代码不符 → 用 `SKILL_CODE_AUDIT_098c.md` |
| `archive/JASS_AUDIT_098c.md` | 早期逐技能事实汇编；数值/状态以台账+代码审计为准 |
| `archive/KNOCKBACK_ALIGNMENT.md` | 击退系数改动记录（已并入台账 §14）|
| `archive/SKILL_STATE_AUDIT.md` / `archive/MAGNITUDE_AUDIT.md` / `archive/P0_AUDIT_RECORD.md` / `archive/P0_RECHECK.md` | 早期技能/数值审计 |
| `archive/UI_AUDIT.md` / `archive/ROOM_UI_REVIEW.md` / `archive/SETTINGS_ALIGNMENT_AUDIT.md` / `archive/AOE_VISUAL_AUDIT.md` | UI / 设置 / AoE 视觉 审计（已完成）|
| `archive/UI_MASTER_PLAN.md` / `archive/LOBBY_UI_PLAN.md` / `archive/STEAM_UI_REDESIGN.md` / `archive/ROOM_SETTINGS_PLAN.md` | UI 计划（已完成 / 被取代）|
| `archive/KEYBINDS_PLAN.md` / `archive/ICON_PACK_PLAN.md` / `archive/AUDIO_PLAN.md` / `archive/AUDIO_SCRIPT.md` | 键位 / 图标 / 音频 计划（已实施；音频素材待替换）|
| `archive/STEAM_MULTIPLAYER_PLAN.md` | Steam 联机三阶段计划（已完成）|
| `archive/CODE_PHASE_RECORD.md` / `archive/FIX_ROADMAP.md` / `archive/GAMEPLAY_FIX_PLAN.md` | 早期代码阶段记录 |
| `archive/HANDOVER.md` | 更早的交接说明（被 `HANDOFF.md` 取代）|
| `archive/FRAME_SYNC_*.md` | 帧同步方案比较/设计（历史决策）|
| `archive/PORT_098B_DECISIONS.md` | **098b 时期**的移植决策（历史；勿按 098b 行事）|
| `archive/RISK_ANALYSIS.md` / `archive/WORKFLOW_NOTES.md` / `archive/DEAD_CODE_CLEANUP.md` | 早期流程/风险/清理 |

> 冲突时：**以 A 类 + `HANDOFF.md` 为准**。
