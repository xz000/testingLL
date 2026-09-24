# 交接说明（HANDOFF）—— 新会话从这里开始

> 用途：本仓库的 098c 移植工作已进入「代码阶段」。新会话**不继承上一会话的上下文**，
> 所以这里固定记录：**文档地图、真值来源、测试基线、协议号、命令与坑**。
> 开场只需说「**按 HANDOFF 继续，先做 P0_RECHECK §F（S005 反射）**」。

---

## 0. 一句话现状

- **P0 五个技能**（S000/S003/S008/S009/S016）已普查 + 已修代码（可复查：`P0_RECHECK.md` 判定 21/22 恰当）。
- **技能普查 16/16 完成**（含 `SC → 入口` 分发表）、**道具 24 格全解**、**经济奖励确认**（起始金待实机）。
- **复审出 5 处击退系数缺口 + 8 项结构性偏差 + 1 个数学错误（S005 反射）** → 现在开始按风险顺序改代码。
- **代码阶段已有进展**：
  - §5 第 1 项（S005 反射）✅ `0baea5d`（协议 26）；
  - §5 第 2 项 `ProjClass`/逐弹体 `xv` ✅ 部分（S009 碎片/B 弹撞柱反弹；**S018 仍缺**）；
  - §5 第 3 项（击退系数 5 处 + S001/S021 距离衰减）✅ `b3a0a97`（协议 **27**，详见 `KNOCKBACK_ALIGNMENT.md`）。
  - §5 第 4 项（S009 父弹固定 3）✅ `06cee9e`（协议 **28**）。
  - **下一步 = §5 第 5 项**：S003 继承施法者速度（`bO(Nb, 900*.03+Qb, …)`），
    随后 S016 提前量+制导 / S016B 魂回飞。代码阶段总台账见 `CODE_PHASE_RECORD.md`。

## 1. 文档地图（先读这三份）

| 文件 | 读什么 |
|---|---|
| `P0_RECHECK.md` | **§F = 下一步的实施规格（S005）**；§A 22 条判定；§B 击退系数缺口表；§C 8 项结构偏差；§D/§E 道具与经济复审与顺序 |
| `SKILL_ALIGNMENT_LEDGER.md` | §3 八张事实表；§7 收尾清单（8 项待实施）；**§9 技能（分发表 + 16 张表）**；**§9b 道具 24 格**；**§9c 经济**；§9d 复审计划 |
| `JASS_FIELDS_098c.md` | 两字母字段/回调语义（`Q/S` 是每 tick、`mI` 的 `lI`=击退%、`yO/oR` DoT、`wR` 废参、`Jv`≠`Gv`、`kv` vs `Kv`、两个“镜面反射”的区别） |
| `P0_AUDIT_RECORD.md` | P0 的“事实 + 改动 + commit”汇总（供回退/复查） |

## 2. 真值来源（**代码是真值**，tooltip 只作交叉验证）

- JASS 反编译：`../098c/out/war3map_pretty.j`（27329 行；**行号在所有文档里被引用，不要用别的文件代替**）
- 技能 tooltip：`../098c/out/w3a_strings.txt`；技能原始数据（含**每技能多档 tooltip/`acdn`/`alev`**）：`../098c/out/w3a_parsed.json`
- 道具：`game-core/src/item.rs`（+ 测试 `w3t_crosscheck_item_bonuses`）；JASS 买/卖处理器 `ED`(20752)、`21080–21560`
- ⚠ 已知 tooltip 与代码冲突的例子：S016 跳衰减（tooltip 20% vs 代码 `×0.75`）、S000 火法杖 DoT（tooltip 3.0 vs 代码 3.5）→ **一律取代码**

## 3. 基线（改动前先确认，改动后必须一致或增长）

- 测试：**client 100 / game-core 273 / net 39 / net-steam 9**；带 steam+gui 的 client **107**
- `PROTOCOL_VERSION = 25`（`game-core/src/lib.rs`）——**只要动了会进状态的字段就必须 +1**，并同步 `world_ser.rs` 的 ser + de + 往返测试
- 命令：`powershell -NoProfile -ExecutionPolicy Bypass -File check.ps1`（= build + test + clippy `-D warnings`，含 steam/gui 变体）
- 提交：默认走 **pre-commit 钩子**（会跑完整回归，输出很长）；
  **纯文档提交**可 `git commit --no-verify`，但仍需先跑一次 `check.ps1` 确认全绿

## 4. 坑（血泪）

1. **改代码用 `edit` 工具**；**不要**用 `powershell Set-Content` 写 `.rs`（会把 UTF-8 中文变成 ANSI → `stream did not contain valid UTF-8`）。
2. clippy 的 client 调用固定为 `cargo clippy -p client --features client/steam,client/gui`（**不要** `--all-targets`，会报测试专用代码）。
3. JASS **标识符大小写敏感**（`z`/`Z`、`rv`/`Rv`、`kv`/`Kv` 都是不同变量）；`Kv` 遍历**只走术士子链**。
4. 两个“镜面反射”别混：
   - `fix::bounce_off(v,n,xv)` = `v − (1+xv)(v·n)n` → **098c 的柱/墙/盾反弹**（法向取反、切向保留）
   - `fix::mirror_by(v,n)` = `2(v·n)n − v` → 只给 **D2 原型护盾**用；**正面撞面时 v 不变**（曾因此让火球/回旋镖穿柱）
5. 098c 的 `mI(nr,Vr,HX,lI)`：`lI` = tooltip 的「xx% 击退」（S015 簇射 `.65` ↔ tooltip「65% knockback」可作锚点）；`HX` 才是伤害。
6. 技能是“**一个能力多档**”：如 S000 共 **24 档**，**13–24 = 带火焰法杖版**（物品 13 靠 `SetUnitAbilityLevel('S000', +12)` 切档），不是两个技能。

## 5. 代码阶段顺序（风险从高到低）

1. ✅ **S005 反射盾**（`0baea5d`）—— 见 `P0_RECHECK.md` §F：反射改 `bounce_off` + **改弹体归属** + 回旋镖转回程 + `Ev` 排除表
2. ✅ **引入 `ProjClass`（`Ev` 等价）**（`0baea5d`）→ S009 碎片/B 父弹 `+1` 已解决；**S018A/B `+1` 仍缺**（`Gravity` 无柱碰撞）
3. ✅ **击退系数 5 处**（`b3a0a97`）：S002 `.95`、S003 AoE `1.3`（自撞 1.0）、S004 `.95`、
   S008A `.75`、S008B `.6`；S001/S021 改**距离衰减** `1−d/1000`、S020 固定 1。
   （遗留：S003 直伤 `.95`、S008B 撞柱 `.8` 未建模；详见 `KNOCKBACK_ALIGNMENT.md` §3）
4. ✅ **S009 父弹固定 3**（`06cee9e`）：新增 `W098b.direct_dmg`，父弹 `Some(3.0)`，碎片仍分级（`FB` 12220）
5. **S003 继承施法者速度**（`bO(Nb, 900*.03+Qb, ...)`）→ **S016 提前量解算 + 跳后制导** → **S016B 魂回飞清 CD**
6. **`jn`（状态时长倍率）** → 再考虑 `Bv`/`Hr`/`cv`
7. **道具 24 项对齐**（台账 §9b；⚠ 我方“力量/坠饰/疾风靴/护腕/吸血之刃”疑为 098b 遗留）
8. **经济**：初始金待用户实机确认；技能价用 `w3q` 对账（我方 `meta.rs` 现为 098b 默认值）

## 6. 每步的完成标准

- 纯函数优先 + 单测覆盖该行为（项目风格）
- `check.ps1` 全绿；协议号按需递增并补 `world_ser` 往返
- 更新 `P0_RECHECK.md` / `SKILL_ALIGNMENT_LEDGER.md` 的**状态列**（把 ⚠/📋 改成 ✅ 并写 commit hash）
