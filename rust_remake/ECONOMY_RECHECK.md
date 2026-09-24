# 经济系统核对（ECONOMY_RECHECK）—— 用 098c_20260924 复核我们的现状

> 日期：2026-09-24 ｜ 状态：✅ **核对完成且已实施**（commit `aa80a80`，协议 31→32）
> 已改：奖励金默认全 0、胜利点数 mo=1、精通/技能每级增量=gglm=1（§5，`aa80a80`）；
> 背包增量=0（`ab752c2`）；移除不可获得的 Aegis2（`0888d4a`，协议 33）。
> §4 五项目前已**逐条 JASS 核实完毕**（其中 R000/Pendant **推翻了新文档的说法**）；仅剩低优先存疑项。
>
> **来源**：`../098c_20260924/`（AI 逆向产物，主文档 `WARLOCK_ECONOMY.md`）。
> **方法**：不盲信新文档——凡结论都在**两份 JASS 与对象数据**上直接复核：
> - **新文件夹自带的 JASS**：`../098c_20260924/Warlock098c/scripts/war3map.j`（13616 行，本文 **§1b** 的行号）；
> - **本仓库解包**：`../098c/out/war3map_pretty.j`（用 `Select-String`，勿用 `findstr`，后者行号错乱）；
> - `../098c_20260924/Warlock098c/war3map.w3q`（升级，24 字节 mod，值在 id+16）与 `war3map.w3u`（单位 `ugol`）。
> 两条 JASS 独立解包、行号不同，但结论完全一致。
>
> ⚠ 新目录的 `war3map.j` 是同一张图的另一次解包（13616 行），与 `war3map_pretty.j`（27329 行）
> 逻辑一致，随机命名相同（`Qo/qo/lo/Mo/po` 等可跨文件对照）。

---

## 1. 核对结论一览

| # | 项 | 098c 真值（证据） | 我方现状 | 判定 |
|---|---|---|---|---|
| 1 | 初始金 `Qo` | **20**（pretty.j 216 / 18139：`set Qo=20`） | `starting_gold=20` | ✅ 一致 |
| 2 | 每回合金 `qo` | **10**（215 / 18137 `set qo=$A`） | `gold_per_round=10` | ✅ 一致 |
| 3 | 击杀金 `lo` | **0**（18125 `set lo=0`） | `gold_per_kill=1` | ❌ 应 0 |
| 4 | 助攻金 `Lo` | **0**（18127 `set Lo=0`） | `gold_per_assist=1` | ❌ 应 0 |
| 5 | 胜利金 `Mo` | **0**（18131 `set Mo=0`） | `gold_per_round_win=2` | ❌ 应 0 |
| 6 | 伤害金 `po` | **0**（18133 `set po=0`） | `gold_per_most_damage=1` | ❌ 应 0 |
| 7 | 击杀/助攻**点数** | ko=1 / Ko=1（18123，globals 同为 1） | `score_per_kill=1`,`score_per_assist=1` | ✅ 一致 |
| 8 | 胜利**点数** `mo` | **1**（18129 `set mo=1`） | `score_per_round_win=2` | ❌ 应 1 |
| 9 | 精通**每级增量** | `gglm`=**1**（w3q R00D/R00I/R00Y 均 `gglm=1`；`glvl`=6 是**上限**） | `COST_PER_LEVEL=[6,6,6,3]` | ❌ 应 1 |
| 10 | 技能购买价**每级增量** | `gglm`=**1**（w3q R009/R005/R00C… 均 1） | `PURCHASE_COST_PER_LEVEL=10` | ❌ 应 1 |
| 11 | 技能升级价**每级增量** | `gglm`=**1** | `upgrade_cost_per_level()`=10（火球 11） | ❌ 应 1 |
| 12 | 技能/精通**基础价** | `gglb`：R009=11 / R005=10 / R00C=13 / R00P=8 / R00D=6 / R00I=7 / R00Y=5 | `learn_cost`/`upgrade_cost`/`Mastery::COSTS` 与之一致 | ✅ 一致 |
| 13 | 技能/物品**不耗木材** | `glmb`(木base)=0、`glmm`=0（w3q 实测） | 我方无木材 | ✅ 一致 |
| 14 | 物品**买价** | `w3u` 单位 `ugol`（非 `w3t` 的 `igol`）：Boots5/Cape4/Pendant5/Helm9/Staff7/Mask12/Blood8/Aegis13/Lava7/Pocket7/Stone5 | `item.rs` cost 全部一致 | ✅ 一致 |
| 15 | 物品**卖出返还** | `ED` 给金：Boots4/8/12、Cape3/6/9、Pendant4/8/12、Helm8/16/24、Staff6、Mask10、Blood7/24、Aegis12、Lava5/10/15、Pocket6/12 | `item.rs` sell 全部一致 | ✅ 一致 |
| 16 | `ED` = 卖出处理器 | pretty.j 20759 `function ED`；`-sell`@24942 | 我方也按卖出 | ✅ 一致 |
| 17 | `bD` = 购买处理器 | pretty.j 21068 `GetTrainedUnitType()` | 我方按购买 | ✅ 一致 |
| 18 | 技能价经 `kf`（`GetResearched`） | 25537 | 我方按 w3q 建模 | ✅ 一致 |

---

### 1b. 英文版 JASS（`098c_20260924/.../war3map.j`）直接证据

| 事实 | 新 JASS 行 | 原文 |
|---|---|---|
| 开局默认经济 | **9129–9137** | `set ko=1 / lo=0 / Lo=0 / mo=1 / Mo=0 / po=0 / Po=0 / qo=$A / Qo=20` |
| 每回合发金 | **11433 / 11516 / 11997 / 12303** | `...,GOLD, 当前+qo)` |
| 开局设初始金 | **11479 / 11673 / 11772 / 12046 / 12348 / 12538** | `...,GOLD, Qo)` |
| 击杀/助攻金 | **3330–3331 / 3342 / 3361–3365** 等 | `...+lo` / `+Lo` |
| 胜利/伤害金 | **2881 / 2910 / 2925 … 3095** | `...+Mo` / `+po` |
| 卖出返还 | **10403–10512** | `+4/+8/+$C`（靴）、`+3/+6/+9`（斗篷）……与 `item.rs` 一致 |
| 背包退款 | **10538** | `...+AD` |
| 涨价 `Jf` | **12680–12698** | 对 18 个购买研究逐个 `AddPlayerTechResearched(+1)` |
| 涨价触发 | **12867 / 12898** | `oi[id]=oi[id]+1`；`if oi==6 then elseif oi>2 then …Jf` |
| 购买处理器 | **10544** `function bD`（`GetTrainedUnitType`） | 训练隐藏单位 → 给物品 |
| 卖出处理器 | **10401** `function ED`（`-sell`） | 移除物品 + 返还金 + 反扣被动 |
| `-no reward` | **9357–9359** | `set Mo=0 / po=0 / lo=0`（**不动 `Lo`**） |

**物品买价（直接读 `war3map.w3u` 的 `ugol`）**：Pendant 5 / Boots 5 / Mask 12 / Cape 4 / Helm 9 /
Staff 7 / Stone 5 / Blood 8 / Aegis 13 / Lava 7 / Pocket 7 / Sell 0 —— **与 `item.rs` 逐项一致**。
（`w3t` 的 `igol` 未被脚本读取；全图无 `SetItemGoldCost`/`AddUnitToStock`。）

---

## 2. 我们**之前判定错误**的地方（须在文档中更正）

`P0_RECHECK.md §D` 有两处结论被本次核对推翻：

1. **「`qo`/`Qo` 是死常量、从未被读取」——错。**
   `Select-String` 在 `war3map_pretty.j` 中大量命中：
   - 215/216 全局声明 `qo=$A` / `Qo=20`；
   - **22904/23074/24058/24686** `SetPlayerState(...,PLAYER_STATE_RESOURCE_GOLD, 当前+qo)`（回合末发金）；
   - **22997/23397/23601/24157/24777/25177** `...,Qo`（开局设初始金）。
   ⇒ `Qo`=初始金(20)、`qo`=每回合金(10)。（当时误用 `qo[`/`Qo[` 数组写法搜索，标量自然搜不到。）

2. **奖励金默认值取自 `globals` 占位初值（lo=1/Lo=1/Mo=2/po=1）——错。**
   模式初始化函数（pretty.j **18123–18135**）把它们**覆盖**为：
   `ko=1, lo=0, Lo=0, mo=1, Mo=0, po=0, Po=0`（并 `qo=$A, Qo=20`）。
   `globals` 里的值只是未生效的默认声明。`-no reward`（18611–18615）再显式把 `Mo/po/lo` 置 0。

> 换言之：**098c 默认几乎没有击杀/胜利/伤害金币收入**，主要收入就是 `Qo=20`（开局一次）+ `qo=10`（每回合）。

---

## 3. 关键证据：w3q 字段名

用 `UpgradeMetaData.slk`（官方字段定义）+ 原图 `war3map.w3q` 实测：

| 字段 | 含义 | R00D | R00I | R00Y | R002 | R009 | R00P |
|---|---|---:|---:|---:|---:|---:|---:|
| `gglb` | 金币 **base** | 6 | 7 | 5 | 5 | 11 | 8 |
| `gglm` | 金币 **mod/级（增量）** | **1** | **1** | **1** | **1** | **1** | **1** |
| `glmb` | 木材 base | 0 | 0 | 0 | 0 | 0 | 0 |
| `glmm` | 木材 mod | 0 | 0 | 0 | 0 | 0 | 0 |
| `glvl` | **最大等级** | 6 | 6 | 6 | 11 | 10 | 10 |

⇒ 成本 = `gglb + gglm×(等级-1)` = **base + 1/级**。
**我方误把 `glvl`（最大等级）当成了每级增量**（`COST_PER_LEVEL` / `PURCHASE_COST_PER_LEVEL` /
`upgrade_cost_per_level` 用了 6/10/11）。这是 §1 第 9–11 行的根源。

> 附带：`Jf` 在买第 3/4/5 个法术时给**所有**科技 `AddPlayerTechResearched(+1)`，
> 使后续价格按 `gglm`(=**+1 金**)上升（不是 +10）。我方 `spell_cost_step × 10` 同样偏大 10 倍。

---

## 4. 逐项核实（JASS/对象数据）与处理 —— 2026-09-24 已全部核实

> 直接读英文版 JASS/`w3q`/`w3t`/`w3a`；**两处推翻了新文档的陈述**（R000、Pendant）。

1. **背包 R000**：w3q 原始字节 = **`gglb=3, gglm=0(缺失), glvl=3`**（`war3map.j` `kf` 12715 `IncUnitAbilityLevel(S128)`）
   ⇒ **固定 3 金/次、上限 3 级**。⚠ **新文档 §6 的 `R000=None` 是错的**（其解析器边界 bug）。
   → 已修：`COST_PER_LEVEL[3]=0`（`ab752c2`）。**上限保留我方改良的 10 格**（用户决定）。
2. **Aegis 2（I00I）**：`war3map.j` 全部 `UnitAddItemById` + `bD h008` 分支**从不创建 I00I** ⇒ 不可获得。
   → 已**移除**（忠于原版，`0888d4a`；`ITEMS` 22 件，协议 33）。
3. **Pendant `I00B/I002/I00C`**：生命加成来自**物品自带能力 w3a `Ilif`**（I00B→`A007`=10 / I002→`A004`=20 /
   I00C→`A00H`=30，引擎自动施加；`bD` 只额外给 I00C `In+0.01`）。⚠ **新文档 §2.5 的“无被动”是误导**（只看了 `bD`）。
   → 我方 `hp_add 10/20/30` + I00C `regen_add 0.1` **本就正确，无需改**。
4. **Stone of Jordan**：`bD h004` → 加 `I00E` + `iV[312+id]` + 解锁 `S027`；`Hf`(12614) 按 `T000..T006` 选一热键组
   → `SetPlayerTechMaxAllowed(Rxx, **+2**)`，一次性。→ 我方「每槽 +2、5 金/次」**功能吻合，不动**。
5. **`qo` 发放**：`aI`(11380–11433) 对 `bn[i]`（参战玩家）在回合切换时 `+qo` → 我方按“参与者”发，**口径一致，无需改**。

### 已解决：`glvl` vs 实际上限

**`glvl`（w3q 对象字段）不是实际生效的等级上限**：`kf` 在购买分支用 `SetPlayerTechMaxAllowed` **覆盖**它。
逐槽（`kn[7*id+k]`）对照：

| 槽 | 购买科技→技能 | `kf` 设的升级研究上限 | 实际档数=1+上限 | 我方 `max_level` |
|---|---|---|---|---|
| G/S000 | `R002`→S000 | init `R002`=**9**（9960） | 10 | 10 ✓ |
| D(+1) | R009→S002 / R005→S003 / R00Z→S004 | R00P/R00O/R010=**6** | 7 | 7 ✓ |
| E(+2) | R00F→S010 / R00A→S008 / R00H→S009 | R00X/R00R/R00U=**5** | 6 | 6 ✓ |
| R(+3) | R004→S011=6 / R007→S012=6 / R013→S013=**5** | 5–6 | 7/7/6 | 7/7/6 ✓ |
| T(+4) | R003→S016 / R001→S015 / R00E→S014 | R00K/R00J/R00L=**5** | 6 | 6 ✓ |
| Y(+6) | R006→S007 / R00C→S005 / R011→S006 | R00S/R00T/R012=**5** | 6 | 6 ✓ |
| G(+5) | R00B→S019=6 / R00G→S018=**4** / R008→S017=5 | 4–6 | 7/5/6 | 7/5/6 ✓ |

⇒ **我方 `max_level` 已与 JASS 一致**（买来即 1 级 + 可升 N 次）；乔丹之石（`Hf` 的 `+2`）也在其上加。
**精通/背包无 `SetPlayerTechMaxAllowed` 覆盖**，故直接用 w3q `glvl`：精通 6/6/6、背包 3 → 我方 `CAPS=[6,6,6,3]` ✓。
所以此前的“`glvl` vs `alev` 两轴”存疑已解：**技能上限用 JASS 运行时上限，精通/背包用 `glvl`，两者我们都用对了**。

- **Stone**：卖出返还 0、需先买戒指并用 `S027` 选择——我方未建模该 UI 流程（用户确认：这是**有意优化的流程**，不动）。

---

## 5. 已实施的改动（commit `aa80a80`，协议 32）

1. ✅ `meta.rs` `MatchConfig::default()`：
   - `gold_per_kill=0`、`gold_per_assist=0`、`gold_per_round_win=0`、`gold_per_most_damage=0`；
   - `score_per_round_win=1`（`mo`）；默认几乎无击杀/助攻/胜利/伤害金币。
2. ✅ `meta.rs` `Mastery::COST_PER_LEVEL = [1,1,1,1]`（背包暂同 1；其 base=3 仍来源不明，列入 §4-1）。
3. ✅ `skill.rs`：`PURCHASE_COST_PER_LEVEL = 1`；`upgrade_cost_per_level() = 1`（含火球）。
4. ✅ `spell_cost_step` 改为 JASS 语义（`oi==6` 跳过、`>=7` 恢复触发）。
5. ✅ 更新受影响测试（`economy_defaults_match_098c`（重命名）、`no_reward_...`、
   `mastery_costs_and_caps_match_w3q`、`mastery_buy_costs_caps_and_backpack`、`spell_costs_escalate_per_098c`、
   `most_damage_in_round_gets_po_gold`、`en2_deathmatch_early_win_and_ranking` 等）。

回归：`check.ps1` 全绿（client 100 / game-core 280 / net 39 / net-steam 9 / steam+gui 107）。
