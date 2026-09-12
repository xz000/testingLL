# tools — 098c 数据解析与交叉校验

这些脚本把 098c 存档里的原始数据（`../098c/out/*`）解析成可对照的表，
用于校验 `game-core/src/skill.rs` 的数值是否与 098c 一致。

## 1. `parse_w3a.py` — 解析 `war3map.w3a`（技能对象数据）

**重要**：本仓库里的 `war3map.w3a` **没有 `W3A!` 魔数头**，直接以 `version = 2` 开头，
且每条修改项的布局是：

```
field(4) + type(4) + level(4) + pad(4) + value + end(4)
```

- `type`：0 = int，1/2 = float，3 = cstring
- `pad`：恒为 0 的 4 字节（**不是**标准 w3a 格式，必须计入）
- `end`：恒为 0 的 4 字节

记录结构：`oldId(4) + newId(4) + nMods(4) + mods`。
脚本用「解析必须**精确**消费到下一个记录头」来验证布局（已用 S000/S012/S013 三个大记录验证通过）。

输出：`../098c/out/w3a_table.json`，形如：

```json
{"S013": {"old": "AHta", "levels": 20,
          "acdn": {"1": 16.0, ..., "11": 14.0, ...},
          "tip":  {"1": {"Cooldown": 16.0}, ...}}}
```

- A 形态 = 等级 1..10，B 形态 = 等级 11..20（098c 用 `SetUnitAbilityLevel(id, lvl+10)` 切形态）
- `acdn` 是**引擎强制**的逐级冷却（权威）；`tip` 是从 tooltip 文本里正则抽出的
  Damage / Range / Cooldown / Duration

## 2. `skill_crosscheck.py` — 与我们的定义对表

先导出我方数值：

```bash
cargo run -q -p game-core --example dump_defs > _ours.tsv
python tools/skill_crosscheck.py
```

（`_ours.tsv` 与本脚本同目录运行时即可。）

输出差异清单：`cooldown` / `damage` / `duration` / `range` 四类，逐技能逐等级。

**注意**：`SkillId::as_u32()` 返回的是 enum **判别值**（S002=39），
与数字后缀无关；`dump_defs.rs` 里维护了 `(后缀, 判别值)` 映射表。

## 3. `gen_w3a_tests.py` — 固化回归测试

把「双方已一致」的 `acdn` / Duration 值写进 `game-core/src/skill.rs` 的
`w3a_cooldown_crosscheck` / `w3a_duration_crosscheck` 两个测试，
防止后续改动悄悄偏离 098c。S007B / S011B（我方未实装 B 形态）会被跳过。

## 已知差异（有意为之）

| 项 | 说明 |
|---|---|
| S007B / S011B | 我方未实装 B 形态（098c 两形态共用同一 handler，仅引擎冷却不同） |
| S000 | 098c `alev = 24`，我方 `max_level = 10`（受研究上限约束） |
| S022 / S023 等 | 非玩家技能 / 已移除 |


## 4. `parse_objects.py` — 解析 `war3map.w3t`（物品）/ `w3b` / `w3h`

物品（`w3t`）提供：`unam`（物品名，如 `Boots 3`）、`iabi`（挂载的物品能力，如 `A007`）、
`igol`（金币价）、`utip`。**所有 `igol` 都是 0** —— 印证 `game-core/src/item.rs` 的说明：
098c 的物品价格写在 JASS 商店表（`bD`/`ED`）里，不在物体数据。

物品的**属性数值**在 `w3a` 的*物品能力*里（`A000/A004/A005/A007/A009/A00D/A00F/A00H`，字段 `Ilif` 等），
由 `parse_w3a.py` 解析；本脚本给出「物品 → 能力」映射。

### 布局坑（**与 w3a 不同**）

本存档这些文件**无** `W3T!` 等魔数头，直接以 `version` 开头：

```
version(4) + n_orig(4) + n_orig×(old(4)+new(4)) + n_custom(4) + records
记录: old(4) + new(4) + nMods(4) + mods
```

| 类型 | 布局 | 备注 |
|---|---|---|
| 数值 (type 0/1/2) | `field(4)+type(4)+level(4)+value(4)` | 16 字节，**无结束符** |
| 字符串 (type 3) | `field(4)+type(4)+cstring+4字节` | **无 level**；末尾 4 字节在 w3t 恒为 0，在 w3u 是对象 id |

对比 `w3a`（见 §1）：数值 `field+type+level+**pad**+value+**end**`、字符串 `field+type+level+cstring+end`。
**两者不能共用同一套 mod 尺寸。**

`w3t`/`w3b`/`w3h` 已通过「精确消费到文件末尾」校验（24/3/2 条记录）。

### `w3u`（单位）：头部语义不同 → 用「候选头扫描 + 精确闭合」

`w3u` 的 `version` 之后那个计数**不是**普通的 `n_orig`（按 `n_orig` 解析会立刻崩），
但记录头 `old(4)+new(4)+nMods(4)` 与 mod 尺寸与 w3t 一致。做法：

1. 扫描所有可能是记录头的偏移（`old`/`new` 可打印或全 0、`nMods` ∈ 1..400、
   紧随其后是合法字段名、再后面 `type` ∈ 0..3）；
2. 逐个解析 `nMods` 个 mod，只有**恰好闭合到另一个候选头或文件末尾**的才保留。

结果：**50/51 条精确闭合** ✅（唯一未闭合的是记录内部的假阳性头）。

**数值位置差异**：w3u 把值放在 **`+8`**（不是 w3t/w3a 的 `+12`/`+16`）：

```
field(4) + type(4) + value(4)   + trailer(4)   # 数值，16 字节
field(4) + type(4) + cstring    + trailer(4)   # 字符串
```

### w3u 交叉校验结论

| 项 | 结果 |
|---|---|
| Warlock 英雄（`hpea→h000`） | `umvs`=**210**、`uhpm`=**100** → 与 `balance.rs` 的 `base_speed`/`max_hp` **完全一致** ✅ |
| 障碍物单位（`obs0..obs6` / `obt0..obt6`） | 单位 `uhpm`=1000 但**未使用**；JASS `constant real nx=40` 才是可摧毁 HP（配 `gv[]` 计数 + "40/40" 飘字）→ 我方 40 ✅ |
| 商店/UI 单位 | `u000`(Merchant)/`u001`(Spells 1)/`u002`(Spells 2)/`u003`(Items)/`u004`(Stone of Jordan)/`u005`(Sell)，`uabi` 指向 `S024`-`S028` 等 |
| Warlock 技能槽 | `uabi = W001,W003,W007,W004,W005,W006,W002,W000`（8 个形态切换按钮） |


## 5. `dump_defs.rs` / 物品对照

- 技能：`cargo run -q -p game-core --example dump_defs > _ours.tsv`
- 物品：见 `game-core/src/item.rs` 的 `w3t_crosscheck_item_bonuses` 测试
  （锁定 `Ilif` 生命加成与速度之靴三档；w3t `unam` 实证 I007=Boots 3、I008=Boots 2）。
