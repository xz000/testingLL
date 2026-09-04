# 复刻 Warlock 0.98b — 知识库

> **工程决策见 `../../rust_remake/PORT_098B_DECISIONS.md`（权威）**：落地工程是既有 `rust_remake/`
> （复用不重写）；TICK 保持 1/60、单位尺度直接切 war3、**移除蓝量系统**——
> 本库 `port_plan_098b.md` 的技术选型（TICK=0.03、hero_mana 等）与决策文档冲突处，一律以决策文档为准。

本文件夹集中存放**复刻 098b 所需要的所有文档**（从 `analysis/deepdive/` 复制而来，原处保留未删）。
按用途分子目录；写代码时按"先看 00 总览 → 查 01 技能/02 物体/03 JASS → 数值对齐 data → 必要时比对 04 CE 参照"的顺序使用。

> 配套工程决策见 `00_总览/architecture_098b.md` 与 `00_总览/port_plan_098b.md`：
> ggez 0.9.3 的 2D 复刻；固定步长 `TICK=0.03` 解耦 60fps 渲染；最终联机走 **Steamworks**（不做局域网）；sim 保持确定性 + 输入可序列化。

---

## 目录结构

### `00_总览/` — 起点（先读这些）
| 文件 | 作用 |
|------|------|
| `README_098b.md` | deepdive 目录索引（所有产物清单 + 进度） |
| `mechanics_098b.md` | **★ 机制总览（移植主参照）**：模式 En1–5 / 启动常量 `ed()` / `-C` 17 项 / 复活 `to=10` / tick=0.03 / 热键槽 / 引擎常量表 / 移植注意 |
| `port_plan_098b.md` | 移植计划：里程碑 M1–M5、验证、风险未知项 |
| `architecture_098b.md` | 架构设计：分层、核心类型、step 管线、技能数据驱动、tick 无关性、输入/传输抽象 |

### `01_技能/` — 逐法术机制
| 文件 | 作用 |
|------|------|
| `abilities_098b.md` | 技能分类目录（ID/名称/英雄/自定义字段数） |
| `abilities_consolidated_098b.md` | **★ 合并总表**：物体数据(名/CD/距离/等级/耗蓝) + JASS(投射物/伤害·AoE公式)，41 技能 |
| `abilities_control_098b.md` | **控制/辅助技能真实机制解码**（天罚/闪电/灾变其实打伤害；反射盾/冲撞/锁链…逐 handler） |
| `abilities_detailed_098b.md` | 逐法术数值表（投射物型号/射程/命中半径/impact 绑定/KI·UO·qI 公式） |
| `abilities_tooltips_098b.md` | 官方中文说明文本（含 S032–S036 升级形态名） |
| `abilities_durations_098b.md` | 逐法术持续时间常量（所有 `kO(...,N*jn,...)` 表达式） |
| `abilities_calibration_098b.md` | tooltip vs 真实值校准（CD≈L1，反射盾≈L2） |
| `_spell_handlers.txt` | 30 个 spell handler + 全部 impact 函数原始体 |

### `02_物体/` — 物品/单位/升级 + 原始导出
| 文件 | 作用 |
|------|------|
| `items_098b.md` / `units_098b.md` / `upgrades_098b.md` | 物品/单位/升级分类目录 |
| `war3map.w3t_098b_org.md` / `w3u` / `w3a` / `w3q` | **原始物体编辑器导出（v2 全字段）**——数值溯源的 ground truth |

### `03_JASS/` — 机制解码（JASS 侧）
| 文件 | 作用 |
|------|------|
| `jass_deobf.md` | **★ JASS 解混淆**：全局变量/函数映射 + SC 调度树 + impact 绑定表 + 区域伤害链/核心伤害 |

### `data/` — 机器可读规格（代码直接消费）
| 文件 | 作用 |
|------|------|
| `port_spec_098b.json` | **★ 28 技能机器可读规格** + 引擎块(tick=0.03, dash_vel=39, hero_movespeed=210, hero_mana=10000)。生成脚本：`ce_old98b/build_port_spec.py` |

---

## 使用建议
- **写某个技能**：`01_技能/abilities_consolidated_098b.md` 看数值 → `01_技能/abilities_control_098b.md` 看机制 → `data/port_spec_098b.json` 取精确 speed/radius/life → 校验 `01_技能/abilities_durations_098b.md`。
- **对数值怀疑**：回 `02_物体/` 原始导出或 `03_JASS/jass_deobf.md` 查公式。
- **手感校准**：`00_总览/mechanics_098b.md` 引擎常量表（`Hr=39/tick` 等）是权威。
- **不在本库**：CE 专用文档（abilities.md/items.md 等无 098b 后缀）、scratch `_*.txt` 探索稿、生成脚本——仍留在 `analysis/deepdive/`。
