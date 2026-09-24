# 代码阶段改动记录（Running log）

> 用途：`HANDOFF.md §5`「代码阶段顺序（风险从高到低）」的**逐项落地台账**——
> 每完成一项记：做了什么、对不对齐 098c、动了哪些文件、协议号、commit、遗留。
> 详细设计另见各专题文档（如 `KNOCKBACK_ALIGNMENT.md`、`P0_RECHECK.md §F`）。

真值来源：`../098c/out/war3map_pretty.j`。
⚠ 行号一律用 **`Select-String`** 取（`findstr` 对该 `.j` 计数错乱）。

---

## 进度总表

| # | 项目（HANDOFF §5 顺序） | 状态 | commit | 协议 | 详情 |
|---|---|---|---|---|---|
| 1 | S005 反射盾 | ✅ | `0baea5d` | 25→26 | §「S005」|
| 2 | `ProjClass`（`Ev` 等价）+ 逐弹体 `xv` | ✅ 部分 | `0baea5d` | 26 | §「ProjClass/xv」|
| 3 | 击退系数 5 处 + S001/S021 距离衰减 | ✅ | `b3a0a97` | 26→27 | `KNOCKBACK_ALIGNMENT.md` |
| 4 | S009 父弹固定 3 | ✅ | `06cee9e` | 27→28 | §「S009 父弹」|
| 5 | S003 继承施法者速度 | ✅ | `5d1f6c1` | 28→29 | §「S003 速度」|
| 6 | S016 提前量解算 + 跳后制导 | ✅ | `31b5397` | 29→30 | §「S016」|
| 7 | S016B 魂回飞清 CD | ✅ | `5cb07b3` | 30→31 | §「S016B」|
| 8 | `jn` 状态时长倍率 | ⬜ | — | — | — |
| 9 | 道具 24 项对齐 | ⬜ | — | — | `SKILL_ALIGNMENT_LEDGER §9b` |
| 10 | 经济默认值对齐 | ✅ 部分 | `aa80a80` | 31→32 | `ECONOMY_RECHECK.md` |

---

## 1. S005 反射盾（`0baea5d`，协议 26）

**对齐 098c `fC`（15054，`Select-String`）。** 详细规格见 `P0_RECHECK.md §F`。

- **反射数学**：`Bullet` 与 `W098b` 两路 `mirror_by` → `fix::bounce_off(v,n,1)` = `v − 2(v·n)n`
  （旧 `mirror_by` 数学相反：正面击中穿盾）。
- **改归属**：反射后 `owner = 盾主`（`Vv[Vr]=Vv[nr]`）。
- **新增 `ProjClass`**（`Ev` 等价，挂 `W098b`）：`NoReflect`（S009/S018）不反射；
  `Magma`/`RedChain`/`Twin` 反射保留归属；`Boomerang` 转 `Home`；`Inert` 不参与。
- **推出重叠** `K[Vr]=K[nr]-r*dx`。
- 测试：`shield_reflects_w098b_and_transfers_owner`、`shield_does_not_reflect_s009`；
  `shield_reflects_and_expires` 断言 `v'=-v` + 改属盾主。

## 2. `ProjClass` + 逐弹体 `xv`（`0baea5d`，部分）

- `ProjClass` 随 S005 引入（见上）。
- **逐弹体撞柱反弹系数 `xv`**：S009 碎片（`dB` 12160 `set xv=1`）与 **B 形态父弹**（`GB` 12341 `xv=1`）
  在生成处逐弹体设 `pillar_rest=1`；A 形态父弹仍被柱挡。
- **遗留**：S018A/B 的 `xv=1` 未做——`ProjectileKind::Gravity` 的运动分支**没有障碍碰撞**（直穿），
  需先给 `Gravity` 加柱碰撞。

## 3. 击退系数（`b3a0a97`，协议 27）

见专题文档 **`KNOCKBACK_ALIGNMENT.md`**（含逐技能 JASS 行号、`KbAttn` 设计、遗留项、系数全表）。
要点：S002/S004 `.95`、S008A `.75`、S008B `.6`、S003 AoE `1.3`（自撞 `1.0`）；
S001/S021 击退 `1-d/1000`、S020 固定 1。

## 4. S009 父弹固定 3（`06cee9e`，协议 28）

**对齐 098c `FB`（12220）：`mI(nr,Vr,3,1.4)`。**

- **问题**：我方 S009 父弹直伤用了 `gx`（= `stats.damage` = `2.5+.5×L`，分级 3.0→6.5），
  命中直伤偏高；而分级伤害其实只属于**碎裂出的碎片**（`CB` 12070：`mI(nr,Vr,2.5+.5*Xv,.65)`）。
- **改法**：`ProjectileKind::W098b` 新增 `direct_dmg: Option<Fix64>`；
  - S009 父弹（A/B 两形态）→ `Some(3.0)`；
  - 碎片生成处传 `*gx`（保持分级）；
  - 命中分支：`let hit_dmg = if homing {…} else { direct_dmg.unwrap_or(*gx) }`。
- **文件**：`world.rs`（字段 + 5 处构造 + 命中）、`world_ser.rs`（`direct_dmg` 入快照）、`lib.rs`（协议）。
- 测试：`s009_parent_direct_hit_is_fixed_3`（L1 与 L8 直伤相等）。
- **遗留**：S009 碎片层仍是**压缩近似**（到点一次喷 N 枚小子弹；098c 是碎片再螺旋喷 2 枚，
  见 `SKILL_ALIGNMENT_LEDGER §7` 第 4 项）。

---

## 5. S003 继承施法者前向速度（`5d1f6c1`，协议 29）

**对齐 098c `Pb`（11285）：`bO(Nb, 900*.03 + Qb, K[Nb]+dx, L[Nb]+dy)`。**

- `Qb = dx*Q[ii] + dy*S[ii]`（施法者速度在**开火方向**上的投影），`if Qb<0 then Qb=0`
  ⇒ 只算**前向**分量（后退不减速）。
- `900*.03` 是每 tick 距离（= 900/s）⇒ 弹速（每秒）= **900 + 前向速度**。
- **改法**：S003 生成时 `speed += max(0, caster_vel·dir)`；施法者速度取
  `dash_vel`（冲刺）/ `control.vel`（击退/强制位移）/ `cur_vel`（自走）+ `pull`（场效应）。
- 只对 **S003** 生效（`proj==Homing` 还包含 S014 回血球，后者不应继承）。
- 测试：`s003_inherits_caster_forward_speed`（注入 `cur_vel=(210,0)`，断言弹速 > 900）。

## 6. S016 提前量解算 + 跳后每帧制导（`31b5397`，协议 30）

**对齐 098c `Fc`（13764，重定向）/ `mb`（11118，每帧制导）。**

- **重定向提前量**（`Fc`）：选好新目标后，用目标速度解算拦截速度：
  `dir = 单位(目标−弹)`；`tvel = 目标当前速度`；`cross = tvel.x*dir.y + tvel.y*dir.x`（照搬 098c 写法）；
  `disc = speed² − cross²`；若 `disc≥0`：`lead = √disc − tvel·dir`，`vel = tvel + lead*dir`（模长 = speed）；
  否则回退直瞄（`CO(nr,900*.03,…)`）。旧实现只是直瞄当前位置。
- **每帧制导**（`mb`）：重定向后每帧 `vel = .98*vel + .02*(speed·dir→目标)`（低通转向）；目标死亡即清。
- **新增字段** `W098b.chase: Option<u32>`（= 098c `Fv[nr]`），重定向时置为**新目标**；
  原 `target` 仍作“已命中受害者”（下一跳跳过）。
- 新增辅助 `player_velocity(&Player)`（冲刺/强制位移/自走 + 场效应）；S003 继承速度也改用它。
- 测试：`s016_bounce_homes_toward_chase_target`（注入朝 +x 的弹跳弹、目标在 +y，断言一帧后速度向 +y 偏转）。

## 7. S016B 魂回飞清 CD（`5cb07b3`，协议 31）

**对齐 098c `dc`（13679 命中）/ `cc`（13633 魂回飞）/ `Nc`（13615 清 CD）。**

- **旧行为**：击中即 `reset_cooldown(S016)`。
- **新行为**：命中**任意术士**后，在命中点生成一枚「魂」（`dc`）；魂以 **400/s**（`$C`=12/tick）
  追施法者；**抵达 64 内**才清 `S016` 冷却并销毁（`cc`）。魂不参与碰撞。
- **新增** `W098bOnHit::SoulReturn`（+ `world_ser` 映射 12）；魂用 `Homing` 运动 + 到达分支清 CD。
- **顺带修正**：S016B 击退系数 `0.8 → 1.15`（`dc`：`mI(nr,Vr,gv*(5.1+.9*Xv),1.15)`）。
- 测试：`s016b_recharge_refreshes_cooldown` 增加中间断言——“命中瞬间仍不清 CD（魂还在回飞）”，
  跑到魂抵达后才 <18。

## 10. 经济默认值对齐（`aa80a80`，协议 32）

**来源**：`../098c_20260924/`（英文版 JASS/对象数据）；详细核对见 **`ECONOMY_RECHECK.md`**。

- **奖励金默认→0**（`lo/Lo/Mo/po`），胜利点数 `mo`→1（`war3map.j` 9129–9137 模式初始化）。
- **每级增量**：精通/技能均用 w3q `gglm`=**1**（旧误把 `glvl`（最大等级）当增量，用了 6/10/11）。
- `spell_cost_step` 改为 JASS 语义（`oi==6` 跳过、`≥7` 恢复触发）。
- 已核对**一致**：`Qo=20`/`qo=10`、物品买价（`w3u ugol`）、卖出返还、`bD`/`ED`、零木材。
- **遗留**：Aegis2 不可得、Pendant 效果（我方叠 hp）、Stone of Jordan 语义、`qo` 只发存活者。

---

## 提交与基线

- 基线：`check.ps1` 全绿（最新：client 100 / game-core 280 / net 39 / net-steam 9 / steam+gui 107）。
- 提交：代码走 pre-commit 钩子（= `check.ps1`）；纯文档用 `--no-verify`（仍先跑过一次 `check.ps1`）。
