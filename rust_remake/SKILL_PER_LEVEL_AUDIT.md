# 战斗 / 技能逐档数值复核（098c）

> 日期：2026-09-24 ｜ 状态：✅ 可达等级全部对齐（无偏差）
>
> 真值来源：`../098c/out/w3a_parsed.json`（技能对象 `acdn`/`adur`/`aare`/`aran` + 逐级 `aub1`
> tooltip 的 Damage/Duration/Range/Force/Missiles/DPS 等），并与 `../098c_20260924/Warlock098c/
> scripts/war3map.j` 的施法分支交叉验证。全部数值以**逐档 tooltip + JASS 公式**为准。

## 方法

1. 从 `w3a_parsed.json` 逐技能、逐等级抽取 `aub1` 里的 `Name: |c…value|r` 数值。
2. 与 `SkillStats::stats_at(level)`（`skill.rs` 的 `SkillGrowth` 表）逐档比对。
3. 把结果固化成回归测试（`skill::tests::w3a_*_crosscheck`），此后改数值即红。

## 覆盖率（回归测试）

| 测试 | 覆盖 |
|---|---|
| `w3a_cooldown_crosscheck` | S000/S002-S019（A/B 两形态）逐档 `acdn` |
| `w3a_duration_crosscheck` | S005/S006/S007/S010/S012B/S014/S017/S018B 逐档 `adur`（tooltip Duration） |
| `w3a_damage_crosscheck` | S000/S002/S003/S004/S008/S009/S010/S012/S015/S018B 逐档 tooltip 伤害 |
| `w3a_per_tick_damage_crosscheck` | S018A(`0.3+0.2L`/tick)、S019A(`0.2×L`/tick)（工程侧存 DPS，÷0.18 门控周期） |
| `w3a_range_crosscheck` | S011(`770+70L`)、S012(`700+50L`) 逐档射程 |
| `s000_fireball_matches_spec` | S000 火球含**法杖档**（L13-24：直伤 5.5→11、DoT 3.0→8.5） |

## 逐档数值结论（均对齐）

- **冷却**：S000 恒定 4.8；S002 `16.5→12`；S003 `15→9.5`；S004 `16→8.2`；S005 `25→14`；
  S006 `22→12`；S007 恒定 21；S008 `20→16.5`；S009 `30→20`；S010 `30→17`；S011 `16→5.5`；
  S012 `16.5→7`；S013 `16→6`；S014 `22→16.5`；S015 `16→9`；S016 `20→13` / B `23.5→18.5`；
  S017 `25→10`；S018 恒定 26；S019 `17→8`。
- **伤害**：S000 `7+0.7L`（法杖档另计）；S002/S003 `7+L`；S004 `7.2+0.8L`；S008 中心 `12+2L`；
  S009 `3+0.5L`（碎片/子弹出伤，父弹固定 3）；S010 `5.4+0.8L`；S012 `5.4+0.4L`；
  S015 `2.6+0.2L`（B 形态 `3+0.4L`）；S018A `0.1+0.2L`/tick；S018B DPS `2.25→8.00`（非线性表）；
  S019A `0.2×L`/tick。
- **时长**：S005 `2.8→4.2`；S006 恒定 3.6；S007 `7.0+0.8L`；S010 恒定 3.1（B 4.0）；
  S012B 3.1；S014 `4+L`；S017 `4.5+0.25L`；S018B 5。
- **射程**：S011 `770+70L`；S012 `700+50L`。
- **其他**：S007 吸收上限 `3+2L`、移速 +35（`KR` 2778/2788）；S018A 拉力 `12+L`；S018B 回复 `1.0+0.2L`；
  S019B 红链闪电 `1.0+0.3L`。

## 结构说明（非偏差）

1. **`alev=20` vs 可达上限**：w3a 里多数技能 `alev=20`（奇数档：S002-5=9、S006=8），
   但实际可达等级由 JASS `SetPlayerTechMaxAllowed` 封顶（见 `ECONOMY_RECHECK.md §4`）→
   我们的 `max_level`（7/6/5 …）用的是**运行时上限**，正确。
2. **B 形态 = 第二段等级**：S008/S009/S010/S011/S012/S013/S015/S017/S018/S019 的 w3a 等级
   11-20 是 **B 形态**（JASS 用 `Da[ri]` 等旗标切段）；我方用独立 `SkillDef`（`warlock098b_def_alt`）建模。
3. **S007 的 L11-18 是死数据**：w3a `S007` 有 20 档，但 JASS `KR`(2788) **只有 A 形态**（速度+吸收），
   无 `Da` 分支 → L11-18（“Damage reduced” 0→87.5%）**永不选用**；我方 `has_alt(S007)=false` 正确。
4. **S000 的 24 档**：13-24 = 带火焰法杖版（物品 13 `SetUnitAbilityLevel('S000',+12)` 切档）。
5. **S014/S016 不在 `w3a_parsed`**（rawcode 非 `S0xx`）：其逐档数值来源为 `w3a_strings.txt` +
   JASS 注释，已由 `s009_s014_s015_s016_match_spec` 等测试守。

## S006 时光回溯 —— 已解决（`384cb88`，协议 37）

**原版 `mana`（`gn[player]`）是什么**：**挨打累积的「张力」**——受伤害时 `dX`(1411) `gn += dmg`，
它只用于**放大你受到的击退**（`mI` 3715：`LI=('d'+gn[受])×…`）。它不是蓝耗资源。

tooltip「you will still take **70%** damage points」是**误译**：`RR`(2609) 实为
`gn = ee + (.8 − .1×zr)×(gn − ee)`（`ee` = 施法瞬间 mana 快照）→
**回溯把窗口内累积的张力保留 70%/60%/50%…**（L1/L2/L3…；越高等级保留越少，越好）。

**实现**：`Player.rewind` 元组加 `mana` 快照 + `mana_keep`；施法时存
`keep=(.8−.1L).max(0)`；回溯时 `hp = max(当前, 快照)`、`mana = 快照 + keep×(当前−快照)`。
（`/098c20260924` JASS `RR` 2609 / `GC` 7790；`ve/ee/xe` = hp/mana/facing 快照。）

**遗留（低优先）**：`momentum`（`Q/S` 速度）在 098c `RR` 也回滚；我方保留当前动量未回滚。
其余无逐档偏差。

## 本次改动

- `2e14f8d`：新增 `w3a_range_crosscheck` / `w3a_per_tick_damage_crosscheck`，并扩充
  `w3a_damage_crosscheck`（S008/S009/S018B）、`w3a_duration_crosscheck`（S006/S007/S018B）。
- 回归：`check.ps1` 全绿（game-core **286**）。
