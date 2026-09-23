# P0 技能普查记录（098c 对齐）—— 事实 · 改动 · 证据

> 本文件是 **P0 五个技能**（S000/S003/S008/S009/S016）+ 两个跨技能修正的**汇总记录**：
> 「098c 代码事实 → 我们改了什么（文件/字段/行为）→ commit」。
> 逐技能的四栏明细表（098c 代码事实 / 我方实现 / 状态）在 `SKILL_ALIGNMENT_LEDGER.md` §3。
> 字段/回调语义在 `JASS_FIELDS_098c.md`。**所有结论必须能在 `098c/out/war3map_pretty.j` 里按行号复查。**

**记录原则**：代码是真值；tooltip 只作交叉验证（已知它与代码冲突时以代码为准，见 S016）；每条结论带 `函数名(行号)`。

---

## 0. 跨技能修正（先于逐技能）

| # | 问题 | 098c 证据 | 改动 | commit |
|---|---|---|---|---|
| C1 | **通用 bug**：命中检测里 `else if blast.is_some() { None }`（原意是「陨石飞行途中不结算」）导致**任何带爆炸的弹体都会穿过敌人**（xi>0 的火球就是这么漏掉的，调试时逐帧打印位置发现） | `iB` 11614 / `oB` 11562（陨石走 `Gv` 不在命中段） | 删除该分支 | `83c710b` |
| C2 | **撞柱反弹系数 `xv` 全技能重核**：原表把 S000/S009/S018 当作 1.0（反弹） | 逐技能扫 `set xv[Nb]=…`：**1** = S004(11497)/S014A(13306)/S014B(13344)/S016(13742)/S016B(13918)+SoV(12162/12332)+14012/14144；**.75** = S008B(11838)；**未设= -1（撞柱即毁）** = S000/S003/S009/S013/S002/S019… | `pillar_restitution` 收窄为 `S004/S014/S016 = 1`、`S008 = .75`、其余 0；测试同步 | `83c710b` |
| C3 | **`fix::bounce_off` 新增纯函数**：098c 柱面/墙反弹是 `v' = v − (1+xv)(v·n)n`（`WA` 8980）；原实现误用 `mirror_by`（保留法向/翻转切向 → 正面撞柱原速穿柱） | `WA`(8980)、`RA==6` 墙(8658/8676) | 新增 `bounce_off`（含单测），柱面分支改用它；`mirror_by` 文档标注「仅 D2 原型护盾用」 | `83c710b` |
| C4 | **弹体新增字段**（供上述修正）：`blast_floor` / `blast_dmg` / `blast_on_expiry` / `is_fireball` / `weaken_armed` | 各技能行号见下表 | `world.rs` W098b 变体 + `world_ser` 序列化；**PROTOCOL_VERSION 23→24→25** | `3a1f22b` `83c710b` `3f610d3` |
| C5 | **`xv`/`kv`/`Kv` 考古**：`Kv` 只走**术士**（定论，见 `JASS_FIELDS_098c.md` §3.6）；`wR` 第二参数是废参数；`Jv`(销毁) 与 `Gv`(到寿) 职责常不同 | `eO` 3443 / `oO` 3520 / `wR` 5249 | 文档化；`rc`/`oc` 的「只搜玩家」结论被证明正确 | `465ef90` |

---

## 1. S000 火球 ✅

**098c 事实**（生成 `Ab` 10537；命中 `ib` 10283 / 带杖 `ab` 10323、`Eb` 10394；销毁 `Xb` 10465 / `Ob` 10489；到寿 `Rb` 10513 / `Ib` 10525）

| 事实 | 我们改了什么 | 文件 |
|---|---|---|
| 弹体 `speed 1000` / `Rv 25` / `ev=(1+.1*ei)`（**+10%**，不是 +15%） | ei 权重从 0.15 → **0.10**（从「火球三系 +15%」分组里移出） | `world.rs`（`ei_mult` 调用处） |
| 命中：`mI(nr,Vr,6.3+0.7L,1)` → **击退 1.0、无点燃** | `kb_ji` 1.1 → **1.0**；`ignite` `Some(7.5)` → **None**；`extra_base/delta` 改为 3.5/0.5（= 法杖燃烧总量 3+0.5L） | `skill.rs` S000 def |
| 火焰法杖（物品 13 `iV[$D+24*ri]`）：直伤 **5+0.5L**、燃烧总量 `2.5*(1.2+0.2L)` = 3+0.5L / 2.5s | 直伤 5.5 → **5.0**；燃烧总量改用 `stats.extra`（=3.5+0.5(L−1) = 3+0.5L ✓） | `world.rs`（法杖分支） |
| xi>0 的 AoE：`sI`→`pI`，半径 **`160×(1+0.12xi)`**（`pe=$A0=160`）、衰减 `×(0.15xi+(1−0.15xi)(1−d/r))` | 半径 `45√(14+xi)` → **`160×(1+0.12xi)`**；新增 `DmgFalloff::FloorMul(floor)`（地板 `0.15xi`） | `world.rs` + `DmgFalloff` |
| **触发时机**：AoE 写在 **`Jv`（销毁回调 `Xb`）** → 命中/撞柱**必炸**；**到寿（`Gv=Rb`）不炸** | 原「到寿炸、直中不炸」→ 反转为「命中/撞柱炸、到寿不炸」（`blast_on_expiry=false`；命中分支不再抑制；撞柱分支补 push） | `world.rs` |
| 撞柱：`Ab` 未设 `xv` → **不反弹**，`ib` 对柱结算 `6.3+0.7L` + `IA`（所以也炸） | `pillar_restitution(S000)` 1.0 → **0**；撞柱分支也触发 AoE | `skill.rs` / `world.rs` |
| 守护盾充能只在这三个火球处理器里（10293/10335/10406） | 新字段 `is_fireball`（S000 与分身火球为 true），不再用「Straight+Ki+有点燃」当指纹 | `world.rs` |

**测试**：重写 `fireball_is_stopped_by_pillar_and_damages_it`、`mastery_xi_gives_fireball_ground_blast_on_hit_only`（新）、`no_xi_means_no_fireball_blast`（新）、`s000_fireball_matches_spec`、`mastery_ei_extends_projectile_life`。**commit `83c710b`**

---

## 2. S003 追踪弹 ✅

**098c 事实**（生成 `Pb` 11215；命中 `Ci=lb` 10961；销毁 `di=Lb` 11098；每帧 `fi=mb` 11118 / `Di=Mb` 11161；伤害 `kb` 10941）

| 事实 | 我们改了什么 | 文件 |
|---|---|---|
| **伤害随飞行时间成长**：`Kb=4.5(1+.15*ei)−2.25−ev`，`Kb>0 → (6+L) + 2.5*Kb/2.25`（L1：7 → 9.5） | 新增纯函数 `homing_missile_damage(base, life_total, remaining)`（+单测）；命中伤害改用它 | `world.rs` |
| 命中**术士** = AoE：`tI(nr,kb(nr),1.3, 200*.39*√(14+xi))` → 半径 **78√(14+xi)**、kb 1.3、地板 .5；**本体不直伤**（非术士才 `mI(kb,.95)`） | spawn 注入 `blast = 78√(14+xi)`、`blast_floor=.5`；命中分支对 `Homing` 跳过直伤/直击退（交给 AoE） | `world.rs` |
| **`Nv=true` 且 `bv=true`** → 可撞队友与施法者自己 | 命中检测对 `Homing` 改用 `nearest_hit_any_incl_owner` | `world.rs` |
| **自撞 = Burn out**：施法者 `gR(+100)`（4*jn 后 `Jb` 归还）、`Q[Vr]=.2*Q[Vr]`、**不自伤** | 新增 `homing_burnouts` 队列（速度 ×0.2 延后写回）+ `SpeedSteal(+100)` buff | `world.rs` |
| 命中他人：`gR(Vr, hR+50)`（4*jn）；伤害走 AoE 且 AoE 过滤同队 → **命中队友 = 纯加速不伤害** | `speed_steals.push((victim, 50.0, 4.0))`；同队伤害被 AoE 队伍过滤自然排除 | `world.rs` |
| 生成位置：施法者前方 `qb = 2 + Rv[施法者] + Cr`（否则因 `bv=true` **生成瞬间自爆**） | 命中(AoE)与生成偏移一起加；spawn 的 `pos` 对 `Homing` 加 `dir*(2+Rv+radius)` | `world.rs` |
| 到寿无 `Gv` → `iO(true)` 不炸；`ev=4.5(1+1.5*.1*ei)`（+15% ✓） | `blast_on_expiry=false` ✓ / ei 权重 0.15 ✓（分组正确） | — |

**测试**：+`s003_damage_grows_with_flight_time` / +`s003_hitting_own_caster_burns_out` / +`s003_can_hit_teammate_for_speed_buff`。**commit `3f610d3`**

---

## 3. S016 弹跳弹 ✅

**A 形态**（生成 `Gc` 13896；命中 `gc` 13833；销毁回调 `Jv=Fc` 13739；每帧 `jv=mb` 11118）**B 形态·充能**（`Dc` 13720 / `dc` 13679 / `hi=cc` 13633 / `Nc` 13615）

| 事实 | 我们改了什么 | 文件 |
|---|---|---|
| **跳衰减 `set gv[nr]=.75*gv[nr]`**（下限 .2）——**tooltip 写 20%，代码是 25%** | `×0.8` → **`×0.75`**；测试断言同步（`s016_bounce_jumps_with_decay`） | `world.rs` |
| 到地板后：`if ev>.75 and gv<=.2 then MI(...)` → **只推不伤** | 新增：`at_floor && remaining>0.75` 时撤回本次伤害事件（保留击退） | `world.rs` |
| 首次命中后 `Nv=bv=true` → 后续跳可撞同队/自己 | 新增 `nearest_hit_any_opt_skip(...,incl_owner,skip)`；首跳后启用（`target.is_some()` 作判定） | `world.rs` |
| `mI(...,1.15)` | `kb_ji` 1.0 → **1.15** | `skill.rs` |
| 半径 `Rv=38`、`ev=(750+150L)(1+.1ei)/900`、`xv=1` | 均已一致 ✓（注释修正为 ×0.75） | — |

**测试**：`s016_bounce_jumps_with_decay` 断言改为 ×0.75。**commit `8430992`**

---

## 4. S009 分裂 ✅（近似）

**A 形态**（`GB` 12314 / 命中 `Wi=FB` 12233 / 到点 `yi=DB` 12178 / 碎片 `dB` 12150 / 碎片每帧 `wi=CB` 12070 / 小子弹 `Ti=BB`）**B 形态**（每帧 `Ui=cB` 12002）

| 事实 | 我们改了什么 | 文件 |
|---|---|---|
| A：`hB=700*(1+.1*ei)`，`ev=min(点击距离,hB)/700`（速度 700）、`Rv=50` | **射程封顶 `700×(1+.1ei)`**（原来无上限） | `world.rs`（S009A 分支） |
| B：速度 **280**、`ev=hB/280=2.5*(1+.1ei)`、`Rv=50`、`xv=1` | 600 → **280**；life → `2.5*(1+.1ei)` | `world.rs`（新增 S009B 分支） |
| 小子弹（tooltip「minor missiles」）：速度 600、`Rv=21`、`ev=1.2*(1+.1ei)`、`hv=BB` → `mI(2.5+0.5L,.65)` | 半径 15→**21**、寿命 0.8→**1.2×(1+.1ei)**、击退→**0.65**（原误代入父弹 1.4） | `world.rs`（两处 `spawn_bullets.push`） |
| 链条：父命中固定 3 → 到点喷 1 碎片（250/s, 0.48s）→ 碎片每 0.12s 螺旋喷 2 枚 | 本作压缩为「到点喷 N 枚小子弹」——**已在代码注释写明简化点**；父弹固定伤与碎片层列入 §7 收尾清单 | — |

**测试**：既有 `s009_form_splitter_target_burst_and_area_emit` 仍通过。**commit `7acd388`**

---

## 5. S008 陨石 / 岩浆 ✅（2 项待办）

**A 陨石**（`iB` 11614；落点 `Gv=ti=oB` 11562；每帧 `rB` 11607）**B 岩浆**（`OB` 11814；命中 `Na`；`Jv=Gv=ba` → `EB` 11792 爆炸）

| 事实 | 我们状态 | 文件/待办 |
|---|---|---|
| A：`ev=1.35`、速度 = `点击距离/1.35`、`Gv=oB` | ✓ 一致（`DelayedBlast`） | — |
| A 落点：半径 `210√(1+.25xi)`、伤害 `(12+2L)×(1−d/(400+40xi))`、kb **.75** | ✓ 半径/伤害/分母一致；kb 取 def 值（待核是否 .75） | §7-1 之后一起核 |
| **A 落点用**小写 `kv`（全表）+ `Dv/fv` → **能砸柱** | ✗ 我方 `explode_at` 只扫玩家 | **§7-1 待办** |
| B：`ev=2*(1+.1ei)`、400/s、`Rv=72`、`xv=.75` | ✓ 一致 | — |
| B 爆炸：`3+1.5L`、半径 `200√((128+40xi+Rv)/200)`、地板 .5、kb .6、**`Jv` 与 `Gv` 都炸** | ✓ 伤害（def 4.5+1.5(L−1) = 3+1.5L）、半径、地板、`blast_on_expiry=true`、命中分支也 push | `world.rs`（S000 那笔里已接好） |
| B 撞柱伤应为 `10+2L`（`Na` 尾部 `mI(nr,Vr,$A+2*Xv,.8)`） | ✗ 我方扣 `gx`（3+1.5L） | **§7-2 待办** |

**测试**：既有 `s008_meteor_*` / `s008_magma_*` 通过。**commit `de4a9d6`（普查）+ `2f8d35e`（收尾清单）**

---

## 6. 测试与协议基线（本会话结束时）

- 测试：**game-core 273 / client 100 / net 39 / net-steam 9**（steam+gui client 107）；`check.ps1`（build+test+clippy -D warnings）全绿。
- `PROTOCOL_VERSION`：**25**（本会话 23→24→25，新增 `weaken_armed` / `blast_floor` / `blast_dmg` / `blast_on_expiry` / `is_fireball`）。
- 每个 commit 都经 pre-commit 钩子（完整回归）后才落库；commit body 里保留了当时的完整改动说明与证据。

---

## 7. 仍未做的（已文档化，见 `SKILL_ALIGNMENT_LEDGER.md` §7 收尾清单）

S008 砸柱 / S008B 撞柱 10+2L / S009 父弹固定 3 / S009 碎片层 / S003 继承施法者速度 / S016 提前量与跳后制导 / S016B 魂回飞清 CD / 通用 `jn`·`Bv`·`Hr`·`cv`。
