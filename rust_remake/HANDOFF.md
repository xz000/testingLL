# 交接说明（HANDOFF）—— 新会话从这里开始

> 用途：本仓库是 **Warlock 098c 的 Rust 复刻**（原为 098b 移植，正逐步迁移到 098c）。
> 新会话**不继承上一会话上下文**，这里固定记录：**文档地图、真值来源、测试基线、协议号、命令与坑**。
> 开场只需说「**按 HANDOFF 继续**」，并说明要做 Steam 主线还是技能对齐。

---

## 0. 一句话现状（2026-10-10）

- **技能对齐**：本会话完成一轮**逐技能「098c 代码」基准审计**（`SKILL_CODE_AUDIT_098c.md`），
  修掉若干偏差/占位（S015A/S017A/S017B/S018B/S014B/S007/S010/S020/S021/S008A + 火法杖窗口 + 治疗 `PX→DX`）。
  **审计已无已知未闭合项**。
- **联机（Steam）**：R 系列全部完成——掉线/迁移 UI、崩溃重开归队(R5c)、被取代 host 归队(R6b)、
  联机暂停(R7a–d)、有界追赶(R8)、防脑裂。见 `EXIT_MENU_RECONNECT_PLAN.md`。
- **协议号 `PROTOCOL_VERSION = 52`**（`game-core/src/lib.rs`；变更史见其上方注释）。
- **A2 移动模型已收口（2026-10-10）**：冰面阻尼时间化 `0.98^(5/9)`；**击退并入走位**（`Player.kb_vel` 与自走并存相加，可边走边滑；协议 49）；仅普通加速时间**有意保留**（对帧同步手感更好）。见台账 §12/§13。
- **关键教训**：本仓库基线**既非 098b 也非 098c**，而是「098b 事实 + 098c tooltip + 占位」的混合物；
  **098c 的 tooltip 也常与 098c 代码不符** ⇒ **唯一真值是 098c 代码**。

## 1. 文档地图（按权威性）

> 全量文档（40+）的角色/权威性见 **`DOCS_INDEX.md`**；下表只列核心。

| 文件 | 权威性 | 读什么 |
|---|---|---|
| `SKILL_ALIGNMENT_LEDGER.md` | **主台账（活）** | 逐技能 §3/§9 事实表 + 状态列（✅/🔧/⚠/📋）；§11 基准审计；§12/§13 A2 移动；**§14 JASS↔Rust 全量对齐矩阵** |
| `SKILL_CODE_AUDIT_098c.md` | **权威（代码基准）** | 逐技能按 **098c 代码** 抽出的 damage/duration/cooldown/range/speed 公式，与 def 对照 |
| `JASS_FIELDS_098c.md` | 权威（字段语义） | 两字母字段/回调语义（`Q/S`=每 tick、`mI` 的 `lI`=击退%、`yO/oR` DoT、`wR` 废参、`Jv`≠`Gv`、`kv` vs `Kv`、两种“镜面反射”） |
| `098C_DIFF.md` | 系统级差异 | 098b↔098c 大系统差异（魔法张力/移动/物理/成长/计分/回血/岩浆）。**A1–A8 全部结案**；**A2 移动模型已收口**（详台账 §12/§13）|
| `ITEM_AUDIT_098c.md` / `ECONOMY_RECHECK.md` | 权威 | 道具 24 格 / 经济奖励 |
| `SKILL_ALIGNMENT_LEDGER.md` §14 | 权威 | **击退系数逐技能 + JASS↔Rust 全量对齐矩阵**（原 `archive/KNOCKBACK_ALIGNMENT.md` 已并入并归档）|
| `EXIT_MENU_RECONNECT_PLAN.md` | 设计档（联机） | Esc 退出菜单 / 重连 / 迁移 / 暂停（R 系列） |
| `archive/CODE_PHASE_RECORD.md` | 记录 | 代码阶段改动汇总（commit hash） |
| ⚠ `archive/SKILL_PER_LEVEL_AUDIT.md` | **已被取代** | 逐档表**按 tooltip** 建立，已多处与 098c 代码不符 → 以 `SKILL_CODE_AUDIT_098c.md` 为准 |
| `archive/*.md`（`HANDOVER`/`P0_*`/`FIX_ROADMAP`/`FRAME_SYNC_*`/`PORT_098B_DECISIONS` …） | 历史 | 已移入 `archive/` 并带历史横幅；可查但**勿当真值** |

## 2. 真值来源（**代码是真值**，tooltip 只作交叉验证）

- JASS 反编译：`../098c/out/war3map.j`（337KB **raw**，**不要**用 `war3map_pretty.j`——它会把 `elseif` 拆坏）。
- ⚠ **行号口径**：raw 用 `CR` 分行，`findstr`/多数编辑器会数错行；历史文档行号来自旧 `Select-String`，**与 raw 不可比**。
  用 **`tools/jass_find.py <regex>`**（按 `[\r\n]+` 切分）检索；引用一律以**函数名**为准。
- 技能 tooltip：`../098c/out/w3a_strings.txt`；原始数据：`../098c/out/w3a_parsed.json`。
- 098b（**仅用于对照历史，不作真值**）：`../ce/ce_old98b/war3map.j.dec`、`../ce/ce_extract/*`。
- ⚠ 已知 **098c tooltip ≠ 098c 代码** 的例子（一律取**代码**）：
  - S018B 力场 DPS：tooltip `2.25…8.00`“非线性” vs 代码 `1+1.25L`（线性）。
  - S017A 缠绕时长：tooltip `4.5…6.25` vs 代码 `4+0.25·abilityLevel`=`4.25…6.0`。
  - S014B 削弱伤害：tooltip `5→11` vs 代码 `4+L`=`5…12`。
  - S000 火法杖 DoT：tooltip 3.0 vs 代码 3.5；S016 跳衰减：tooltip 20% vs 代码 `×0.75`。

## 3. 基线（改动前先确认，改动后必须一致或增长）

- 测试：**client 119 / game-core 331 / net 47 / net-steam 9**（默认）；带 `client/steam,client/gui` 的变体见 `check.ps1`。
- `PROTOCOL_VERSION = 52`——**动了会进快照的字段 / 改变世界演化的数值逻辑都必须 +1**，并同步 `world_ser.rs` 的 ser + de + 往返测试。
- 命令：`powershell -NoProfile -ExecutionPolicy Bypass -File check.ps1`（= build + test + clippy `-D warnings`，含 steam/gui 变体）。
- 提交：默认走 **pre-commit 钩子**（跑完整回归）；**纯文档提交**可 `git commit --no-verify`，但仍需先跑一次 `check.ps1` 确认全绿。

## 4. 坑（血泪）

1. **改代码用 `edit` 工具**；**不要**用 `powershell Set-Content` 写 `.rs`（会把 UTF-8 中文弄成 ANSI）。
2. clippy 的 client 调用固定为 `cargo clippy -p client --features client/steam,client/gui`（**不要** `--all-targets`）。
3. JASS **标识符大小写敏感**（`z`/`Z`、`rv`/`Rv`、`kv`/`Kv`、`DX`/`dX`、`SI`/`sI` 都是不同变量/函数）；`Kv` 遍历**只走术士子链**。
4. 两个“镜面反射”别混：
   - `fix::bounce_off(v,n,xv)` = `v − (1+xv)(v·n)n` → **098c 的柱/墙/盾反弹**（法向取反、切向保留）。
   - `fix::mirror_by(v,n)` = `2(v·n)n − v` → 只给 **D2 原型护盾**用；**正面撞面时 v 不变**（曾因此让火球/穿柱）。
5. 098c 的 `mI(nr,Vr,HX,lI)`：`lI` = 击退系数（tooltip 的「xx% 击退」），`HX` 才是伤害。
6. 技能是“**一个能力多档**”：如 S000 共 **24 档**，**13–24 = 带火焰法杖版**（物品 13 靠 `SetUnitAbilityLevel('S000',+12)` 切档）。
7. `war3map.j` **raw** 才是控制流真值；`war3map_pretty.j` 的 `elseif` 被拆坏（已实证误读 `aR`）。
8. 改「会进快照的字段」→ 必须升协议 + 改 `world_ser.rs` ser/de + 往返测试；`BuffKind`/`W098bOnHit` 是**按 tag 显式编码**的（加变体要新 tag + 升协议）。

## 5. 下一步候选（按需要挑）

1. **Steam 真机验收**（协议已到 48，两端都需更新）——验收点见本会话 commit 说明（火法杖窗口、力场、S017A/B、S015A、平加移速、陨石砸柱、治疗扣张力等）。
2. **继续技能代码审计**：把 `SKILL_CODE_AUDIT_098c.md` 里「未抽查」的小项补齐（如 S002 的 `cX` 注入点、S012A 时长）。
3. **R7e 学习期暂停**（需学习期心跳 + 冻结本地 `tick_learning`，工作量大，待设计）。
4. 其它 Steam 主线（回合/胜负/计分流程、UI/表现、实机行为验证）。

## 6. 每步的完成标准

- 纯函数优先 + 单测覆盖该行为（项目风格）。
- `check.ps1` 全绿；协议号按需递增并补 `world_ser` 往返。
- 更新 `SKILL_ALIGNMENT_LEDGER.md` / `SKILL_CODE_AUDIT_098c.md` 的**状态列**（把 ⚠/📋 改成 ✅ 并写 commit hash）。
