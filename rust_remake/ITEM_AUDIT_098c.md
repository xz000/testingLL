# 道具 24 项对齐审计（098c）

> 日期：2026-09-24 ｜ 状态：✅ 逐项核实完成（未决项：无）
>
> 真值来源（**代码为准**，两份独立解包一致）：
> `../098c_20260924/Warlock098c/scripts/war3map.j`（`bD` 购买 10544+ / `ED` 卖出 10401+）、
> `war3map.w3u`（商店单位 `ugol` 买价）、`war3map.w3t`（物品 `iabi` 自带能力 / `unam`）、
> `war3map.w3a`（能力字段如 `Ilif`）。
> 我方实现：`game-core/src/item.rs`；经济另有 `ECONOMY_RECHECK.md`。

## 机制要点（核实）

- **买价 = 商店单位的 `w3u` 字段 `ugol`**（不是 `w3t` 的 `igol`，后者本图全为 0）。每家族**每步同价**，重复购买即升级。
- **物品自带能力**（`w3t.iabi`，引擎自动施加，如 `A007` 的 `Ilif=+10` 生命）**与** `bD` 脚本效果（移速 `gR`、`In` 回复、`Hn` 击退倍率、`jn` 时长等）**是两套**，需分别对齐。
- **卖出 = `ED`**：返还固定金 + 反向扣除脚本效果；`I00E`（乔丹之石）**不返还也不删除**。
- `I00I`（Aegis 2）**不可获得**（`UnitAddItemById`/`bD` 全图不创建）→ 我方已移除。
- `PocketWatch` 的自带能力 `S030` 是**空壳**（无字段）；其效果纯由 `bD`/`ED` 设 `jn`。

## 逐项对照（22 件可获物品 + 乔丹 + Aegis2）

| idx | 物品 | 商店/买价 | 卖出 | 自带能力(w3a) | `bD` 脚本效果 | 我方 `item.rs` | 状态 |
|---|---|---|---:|---|---|---|---|
| 0/1/2 | I000/I008/I007 Boots 1-3 | h00B / 5 | 4/8/12 | — | `gR +20/+10/+10`（累计 20/30/40） | `speed_add 20/30/40` | ✅ |
| 3/4/5 | I00B/I002/I00C Pendant 1-3 | h00A / 5 | 4/8/12 | `A007/A004/A00H` `Ilif` 10/20/30 | 仅 3 阶 `In+0.01` | `hp_add 10/20/30` + 3 阶 `regen_add 0.1` | ✅ |
| 6/7/8 | I001/I006/I00A Helm 1-3 | h005 / 9 | 8/16/24 | `A007/A00D/A004` `Ilif` 10/15/20 | `gR -5`×档、`Hn *= .84/.76/.68` | `hp_add` + `speed_penalty 5/10/15` + `kb_resist .16/.24/.32` | ✅ |
| 9/10/11 | I005/I009/I003 Cape 1-3 | h002 / 4 | 3/6/9 | — | `In +0.02/+0.01/+0.01` | `regen_add 0.2/0.3/0.4` | ✅ |
| **12** | **I004 Mask of Death** | h001 / 12 | 10 | — | `vi += 3`（吸血 8%×3）、`In -= .03` | `lifesteal 0.24` + `regen_penalty 0.3` + `scourge_double`（天罚下 `vi×2`） | ✅（**已去**无依据的 `on_damage_heal 0.12`） |
| **13** | **I00D Staff of Fireball** | h006 / 7 | 6 | — | `SetUnitAbilityLevel('S000', +12)` | `fireball_burn` + 火球等级 | ✅ |
| 14 | I00E Stone of Jordan | h004 / 5 | **0**（且不删除） | — | 加戒指 + `S027` 解锁 | 特殊路径（每槽 +2 上限，5 金/次） | ✅（用户确认：**有意优化的流程**） |
| **15/16** | **I00F/I00G Blood Sword 1-2** | h007 / 8 | 7/24 | — | `Zr += 1`、`S001` 等级 +1；天罚回血 `DX((Zr+1)*n)` | `smite_bonus 1/2`；`on_damage_heal 2/3`（**仅天罚**） | ✅（**已修**：回血不再进通用伤害路径） |
| 17 | I00H Aegis | h008 / 13 | 12 | `A000` `Ilif -10` | `jX(id)` 护盾（`HC`：`hn/=4/3`、`Hn/=2`，窗口 5s） | `smite_reduction 0.25` + `aegis` + `kb_reduction 0.5` + `hp_add -10` | ✅ |
| 18 | I00I Aegis 2 | —（**不可得**） | 24 | `A000` | 无 | **已移除** | ✅ 不收录 |
| **19/20/21** | **I00J/I00K/I00L Lava Treads 1-3** | h00C / 7 | 5/10/15 | — | 被动 `To *= .96/.94/.92`（−4/6/8%）+ `gR +15/+12/+12`（累计 15/27/39）+ `In -= .01`；激活 `kC`：`To /= 8`（−87.5%）3/4/5s、CD25s | `speed_add 15/27/39` + `regen_penalty 0.1` + **被动 `lava_passive_mult .96/.94/.92`** + 激活 `lava_resist_frac 0.875`/`secs 3/4/5` | ✅（**已补**被动 `To`） |
| **22/23** | **I00M/I00N Pocket Watch 1-2** | h00E / 7 | 6/12 | `S030`（空壳） | `jn *= 1.15 / ×1.25` | `buff_dur_mult 1.15/1.25`（`jn`，见下） | ✅ |

> 乔丹之石（idx 14）不在 `ITEMS` 表（不占物品栏，走技能页突破流程）。

## `jn`（怀表）说明

`jn` 是**状态时长倍率**：自身增益 ×`jn[自己]`，敌方减益 ×`jn[施法者]/jn[目标]`。
怀表 1/2 = ×1.15/×1.25。实现见 `Player::jn()` / `add_debuff`（commit `37bc636`）。

## 本次修正（commit `56cd720`，协议 36）

1. **Lava Treads**：补**被动** `To` 倍率 `.96/.94/.92`（此前只实现了激活 87.5%）。
2. **Mask of Death**：去除无依据的 `on_damage_heal 0.12`（098c 只有 `vi+3`→24% 吸血 + `In-0.03`）。
3. **Blood Sword**：回血 `(Zr+1)×n` 是**天罚专属**（`mC` 的 `DX`），`on_damage_heal` 不再进通用伤害路径（普通攻击/技能不再回血）。

## 测试

- `world::tests::lava_boots_passive_resist`（被动 −4/−8%）
- `world::tests::blood_sword_heals_only_on_smite`
- `world::tests::item_effects_lava_passive_and_mask`（数据断言）
- 既有：`item::tests::w3t_crosscheck_item_bonuses`、`catalog_has_unique_dense_ids`（22 件）、`upgrade_chains_are_contiguous`、`meta::tests::buy_item_*` / `sell_item_*`
