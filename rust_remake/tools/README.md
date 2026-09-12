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
