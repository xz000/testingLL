# 经济系统核对（ECONOMY_RECHECK）—— 用 098c_20260924 复核我们的现状

> 日期：2026-09-24 ｜ 状态：**核对完成，发现 4 处需修**（未改代码，待确认）
>
> **来源**：`../098c_20260924/`（AI 逆向产物，主文档 `WARLOCK_ECONOMY.md`）。
> **方法**：不盲信新文档——凡结论都在**本仓库真值源**上复核：
> - `../098c/out/war3map_pretty.j`（用 `Select-String`，勿用 `findstr`，后者行号错乱）；
> - `../098c_20260924/Warlock098c/war3map.w3q` 原始字节（24 字节 mod 布局，值在 id+16）。
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

## 4. 待确认 / 新文档未直接回答我方模型的点

1. **背包（R000 Inventory）**：w3q 中 `R000` **无任何成本字段**（gglb/glvl 皆 None）。
   我方 `Mastery::COSTS[3]=3`、`CAPS[3]=3` 来源不明，需另行定位（可能走 `bD` 特殊分支或对象数据）。
2. **精通/技能上限是 `glvl`（6/10）还是 w3a 的 `alev`**：两者是“研究级上限 vs 技能档上限”两个轴，
   我方 `Mastery::CAPS=[6,6,6,3]` 与研究上限（6/6/6）吻合；技能上限另走 w3a（8/9 等），暂不动。
3. **Aegis 2（I00I）**：098c **从不创建**（24 件里唯它不可获得）。我方 `GuardianShield2` 可购（cost 9）→ 应移除或标注不可得。
4. **Pendant（坠饰）效果**：098c 仅 3 级 `In+0.01`，我方给了 `hp_add 10/20/30`（疑似 098b 遗留）。
5. **Stone of Jordan**：098c 解锁 `S027`、卖出返还 **0**；我方是“某槽击退突破 +2”。
6. **`qo` 发放对象**：098c 是“回合末对**存活**玩家 `+qo`”；我方按“参与者”。需对齐“存活”口径。

---

## 5. 建议的改动（待确认后实施）

1. `meta.rs` `MatchConfig::default()`：
   - `gold_per_kill=0`、`gold_per_assist=0`、`gold_per_round_win=0`、`gold_per_most_damage=0`；
   - `score_per_round_win=1`（`mo`）。
2. `meta.rs` `Mastery::COST_PER_LEVEL = [1,1,1,/*背包待核*/]`；`mastery_cost = base + 1×已购级`。
3. `skill.rs`：`PURCHASE_COST_PER_LEVEL = 1`；`upgrade_cost_per_level() = 1`（含火球）。
4. 更新受影响测试：`d6_economy_defaults_match_098b`、`no_reward_disables_kill_win_damage_gold_only`、
   `mastery_costs_and_caps_match_w3q`、`mastery_buy_costs_caps_and_backpack`、`spell_costs_escalate_per_098c`、
   `kill_gives_gold`、`most_damage_in_round_gets_po_gold` 等。
5. 更正 `P0_RECHECK.md §D` 的“死常量/奖励默认”两处结论，并同步 `HANDOFF.md`。

> 上述 1–3 会**改变默认手感**（默认几乎无金币奖励、涨价极缓）。因涉及默认值/多项测试，
> **先出核对结论，待你确认后再动代码**。
