# Warlock 098b_CN — 深挖目录（deepdive）

数据来源：`098b_CN`（部分可提取版本）。物体编辑器数据（w3t/w3u/w3a/w3q）为 **v2 格式**，已由 `ce_obj/parse_v2.py` 完整解析；但 `war3map.j` 为 **混淆/压缩后的旧框架 JASS**（单/双字母变量名、无空格），无法像 CE 1.10B 那样直接逐函数读机制。

## 当前进度（截至 2026-09-02）

- ✅ **v2 物体数据解析全部完成**（这是之前一直卡住的点）。四种文件均 `end==total` 完美对齐：
  - `war3map.w3t`（物品）：原版 0，自定义 **24**
  - `war3map.w3u`（单位）：原版 2，自定义 **38**
  - `war3map.w3a`（技能）：原版 5（ARal/Apiv/ANcl/Amls/Afbt），自定义 **53**
  - `war3map.w3q`（升级）：原版 2，自定义 **53**
- ✅ **原始 dump 落盘**（`analysis/war3map.w3*_098b_org.md`）：逐条列出每个对象的全部修改字段（4CC 已尽量解码为 WC3 标准字段名；Warlock 自定义字段如 `Ear1/Eme1/Ncl5/Owk1` 保留原始 4CC）。
- ✅ **分类目录落盘**（`analysis/deepdive/*_098b.md`）：技能/物品/单位/升级的逐条表格（ID、基础 ID、名称、关键数值、自定义字段数）。

## ★ 关键结构结论（影响"深扒"方式）—— 已修正为「混合模型」

1. **098b_CN 是混合模型**：物品/单位/技能/升级的**静态平衡数值在物体编辑器里，动态公式在 JASS 里**（旧结论"物体数据是废数据"已证伪，见下）。
   - **物体数据（`w3a_v2.json`）确实带**：`anam`(中文名：火球/天罚/闪电/…)、`acdn`(冷却，逐等级)、`aran`(施法距离)、`alev`(最大等级)、`aman`(耗蓝)、`acas`(吟唱)。这些**有值、是权威来源**。
   - **JASS（`war3map.j.dec`）带**：伤害公式 `KI`/`jI`、AoE 半径 `UO`、击退 `CX`/`IO`、以及按 `jn[玩家]` 缩放的持续时间。
   - ⚠️ `w3a` 里那 244 个 `S000` 的"通用"自定义字段（`Ear1/Eme1/Ncl5/Owk1` 等）大多无意义，但 `acdn/anam/aran/alev/aman/acas` **不是废数据**，不要整体当成遗留。
   - CD 是原生冷却：代码只在死亡/训练模式重置时调 `UnitResetCooldown`，**不是每次施法重置** → 游戏内 CD 就是 `acdn`（且很多技能 CD 随等级递增做平衡）。
2. **JASS 框架更老**：无 `FireballCast`/`Object_Create`/`DamageTargetNormal` 等 CE 1.10B 的清晰函数；函数名被压成 1–2 字母（`eb`/`ob`/`rb`/`nb`/`Vb`…），触发/回调靠全局数组与 `ExecuteFunc` 串接。
3. **技能通过 `UnitAddAbility` 在研发完成时挂到英雄上**（JASS 里 `df` 函数处理 `RESEARCH_FINISH`：如 `R000`→`IncUnitAbilityLevel('S000')`，`R00D/R00I/R00Y`→属性点累加 `xi/ri/oi` 并改血量上限 `Hn`）。
4. **升级 = 法术槽位/等级的科技购买**：`R00x`(属性/法术类) 与 `T000–T006`(乔丹之石等一次性道具) 用 `GetPlayerTechMaxAllowed` + `SetPlayerTechMaxAllowed` 做"每类只能买 N 次"的限制。

## 与 CE 1.10B 解析方式的差异（重要）

| 维度 | CE 1.10B | 098b_CN |
|------|----------|---------|
| JASS 可读性 | 完整可读（1199 函数，命名清晰） | 混淆压缩（单字母变量，旧框架） |
| 技能数值来源 | 物体编辑器 + 可读 JASS 回调 | **混合**：静态平衡(CD/名/距离/等级/耗蓝)在物体数据 `w3a_v2.json`，动态公式(伤害/AoE/击退/持续时间)在混淆 JASS |
| 物体数据格式 | v3 | **v2** |
| 可提取性 | 全量可提取 | 仅物体数据可提取；JASS 机制需解混淆 |

→ 结论：098b_CN 的"逐法术机制说明"不能像 CE 那样靠物体编辑器字段 + 读函数完成，必须 **先解混淆 JASS（映射全局变量名），再逐法术函数追溯**。这是下一步的大头工作。

## 已生成文件

| 文件 | 内容 |
|------|------|
| `../war3map.w3t_098b_org.md` | 24 件物品全部字段 |
| `../war3map.w3u_098b_org.md` | 2 原版 + 38 自定义单位全部字段 |
| `../war3map.w3a_098b_org.md` | 5 原版 + 53 自定义技能全部字段 |
| `../war3map.w3q_098b_org.md` | 2 原版 + 53 自定义升级全部字段 |
| `abilities_098b.md` | 技能分类目录（ID/名称/英雄/自定义字段数） |
| `items_098b.md` | 物品分类目录 |
| `units_098b.md` | 单位分类目录 |
| `upgrades_098b.md` | 升级分类目录 |
| `ce_obj/parse_v2.py` | v2 物体数据解析器（修复后，已验证对齐） |
| `ce_obj/emit_org.py` / `emit_catalog.py` | 上述 md 的生成脚本 |

## 下一步（待确认方向）

- ~~**A. 解混淆 JASS**~~：✅ 已完成（见 `jass_deobf.md`）。全局数组已反推；`SC` 调度树（S000–S031 + 升级分支 Fa/Ga/Ha/ga）完整解码；impact 绑定机制（`hv[]`=boolexpr 全局，经 `Condition(function X)` 绑定到 impact 函数）已破解；逐法术数值表已落盘（`abilities_detailed_098b.md`）。
- ~~**CD / 持续时间来源**~~：✅ 已查清 —— CD 在物体数据 `acdn`（原生冷却，非每次施法重置），中文名在 `anam`。已合并成总表 `abilities_consolidated_098b.md`。
- ~~**控制/辅助技能真实机制**~~：✅ 已解码并落盘 `abilities_control_098b.md`（天罚/闪电/灾变其实打伤害；虔诚是奶+伤双效；反射盾/时光回溯/急行/瞬移/冲撞/移形换位/喷火/致残/引力/锁链/S031 均为真实控制/位移机制）。
- **B. 物体数据"清单级"文档细化**（命名/分组、与 CE 对齐）。
- **C. 与 CE 1.10B 字段级 diff**（已有 `../compare_versions.md` 框架）。
- ~~**D. 技能说明文本（tooltip）**~~：✅ 已提取并落盘 `abilities_tooltips_098b.md`（内联在 `w3a_v2.json` 的 `aub1`，`war3map.wts` 仅含地图信息文本）。
- ~~**E. 逐法术持续时间常量**~~：✅ 已汇总并落盘 `abilities_durations_098b.md`（所有 `kO(...,N×jn,...)` / `TimerStart` 计时器表达式）。
- ~~**G. tooltip 校准表**~~：✅ 已落盘 `abilities_calibration_098b.md`（结论：tooltip CD≈L1 值，仅反射盾例外；持续/伤害均为静态近似）。
- ~~**F. 与 CE 1.10B 字段级 diff**~~：✅ 已落盘 `compare_ce_098b.md`（以**能力 CODE S000–S031** 为对齐键；28 个技能中 21 个有字段差异；`../compare_versions.md` 已同步更新）。脚本 `ce_old98b/build_version_diff.py`。
- ~~**H. diff 扩展到物品 w3t / 单位 w3u / 升级 w3q**~~：✅ 已落盘 `compare_ce_098b_objects.md`（脚本 `ce_old98b/build_obj_diff.py`）。物品 24/24、单位 35/38、升级 52/53 命中同码条目；10 / 19 / 34 条有差异。亮点：术士英雄(h000/h003) 魔法上限 098b 10000 vs CE 200、移速 210 vs 220；商店(u000) 与形态单位(u001–u004) 技能列表实现不同；各法术升级的前置(`greq`)结构不同。
- **I.（可选）深挖 CE 1.10B 的 JASS 机制**作为移植主参照（CE 的 `war3map.j` 明文可读，1199 函数）。

## 新增解混淆产物（2026-09-02）

| 文件 | 内容 |
|------|------|
| `jass_deobf.md` | 全局变量/函数语义映射 + **完整 SC 调度树** + impact 绑定表 + 区域伤害链(qI→PI→LI)/核心伤害 jI |
| `abilities_detailed_098b.md` | **逐法术数值表**（32 个 Sxxx：投射物型号/射程 ev/命中半径 Rv/等级来源/impact 绑定/KI·UO·qI 的 base·mult 公式） |
| `abilities_consolidated_098b.md` | **逐法术总表（合并版）**：物体数据(名/CD/距离/等级/耗蓝) + JASS(投射物/伤害·AoE公式)，41 个技能；控制/辅助技已补全真实机制 |
| `abilities_control_098b.md` | **控制/辅助技能真实机制解码**：天罚/闪电/灾变(有伤害)、反射盾/时光回溯/急行/瞬移/冲撞/移形换位/喷火/致残/引力/锁链/S031 的逐 handler + impact 函数解读 |
| `port_spec_098b.json` | **机器可读移植规格**（28 技能）：物体数据(CD/等级/施法距离/AoE) + 校验过的投射物数值(speed 单位/秒, radius, life 秒, impact) + 引擎块(tick=0.03, dash_vel=39, hero_movespeed=210, hero_mana=10000)。脚本 `ce_old98b/build_port_spec.py` |
| `port_plan_098b.md` | **移植计划**（ggez 0.9.3 2D 复刻）：固定步长 TICK=0.03 + 60fps 插值、实体/模块架构、tick 无关性规则、M1–M5 里程碑、验证、风险未知项 |
| `architecture_098b.md` | **架构设计（纯文档）**：分层依赖图、核心 Rust 类型(实体/英雄/投射物/Buff)、step 管线数据流、数据驱动技能+impact 注册表、tick 无关性规则、渲染插值、输入抽象+传输抽象(`Transport` trait 为 Steamworks 留口，明确不做 LAN)、模块文件清单、保真度核对清单 |
| `abilities_tooltips_098b.md` | **官方中文说明文本**：直接读 `w3a_v2.json` 内联的 `aub1`（含 S032–S036 升级形态名：引力 黑洞/力场、锁链 勾取/引导 等） |
| `abilities_durations_098b.md` | **逐法术持续时间常量**：所有 `kO(...,N×jn,...)` 计时器表达式（致残 (4+0.25×级)×jn、引力漩涡 5×jn、火球点燃 (6+1.5×级+xi)×jn×jn 等） |
| `abilities_calibration_098b.md` | **tooltip 校准表**：物体数据 `aub1` 静态 CD/持续 vs 真实 `acdn`(L1…最高级) / JASS 表达式（结论：tooltip≈L1 值，反射盾例外） |
| `compare_ce_098b.md` | **与 CE 1.10B 字段级 diff（技能）**：以能力 CODE(S000–S031) 对齐，比 CD / AoE(aare) / 距离 / 耗蓝 / 等级上限；28 个技能 21 个有差异 |
| `mechanics_098b.md` | **★ 098b 游戏机制总览（移植主参照）**：模式(En1–5) / 启动常量(`ed()`默认值、-C 17项、命令) / 核心循环(购物·击杀·复活) / 场地地形柱子 / 技能槽位热键与升级分支 / 投射物碰撞引擎(tick=0.03) / 伤害管线 / 物品升级 / 单位 / 引擎常量表 / 移植注意 |
| `_spell_handlers.txt` | 30 个 spell handler + 全部 impact 函数原始体 |
| `ce_old98b/extract_dispatch.py`、`build_spell_table.py`、`build_abilities_detailed.py`、`build_consolidated.py` | 上述产物的可复现提取脚本 |
