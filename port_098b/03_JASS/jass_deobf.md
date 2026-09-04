# 098b_CN JASS 解混淆地图（deobfuscation map）

098b_CN 的 `war3map.j`（`ce_old98b/war3map.j.dec`，343 KB / 415 函数）是**混淆/旧框架**代码：变量名是作者起的 1–2 字母短名（非随机打乱），所以"解混淆"= 推断每个全局变量/函数的语义。本文件记录已解码的战斗核心，作为逐法术"深扒"的基础。

## 一、整体架构：自研投射物/击退/伤害引擎

098b_CN **没有**用 WC3 原生技能数据。它的法术是"投射手感"：

- 施法 → `SC()`（cast dispatcher）读 `GetSpellAbilityId()` → 按 `Sxxx` 路由到各法术 handler（如 `S000`→`Rb`）。
- handler 用 `VO()` 生成一个**假人单位**当投射物（型号如 `RedDragonMissile.mdl` / `fb2.mdl`），用 `IO()` 给它速度，用 `xO()` 销毁。
- 投射物飞行中用 `Nb`/`Rb` 系列碰撞回调检测命中，命中调用 **impact handler**（`ob`/`rb`/`nb` 等）算伤害。
- 伤害统一经 `KI()`（算值与击退）→ `FI()`（扣血/飘字/统计/击杀）。

所以：**法术数值 = 物体数据里的静态平衡（名/冷却/距离/等级/耗蓝，见 `w3a_v2.json`） + JASS handler 里硬编码的动态公式（伤害 `KI(...)` / AoE 半径 `UO` / 击退 `CX` / 持续时间按 `jn` 缩放）**。注意：`w3a` 里 `acdn/anam/aran/alev/aman/acas` 是**有值且权威**的（不是废数据）；只有那 244 个通用自定义字段（`Ear1/Eme1/…`）才基本无意义。CD 是原生冷却（`acdn`），代码不在每次施法重置。

## 二、已解码全局变量（战斗核心）

| 变量 | 类型 | 含义（推断+证据） |
|------|------|------------------|
| `F[i]` | unit[] | 单位句柄；`F[Xr]`=目标单位，`F[Er]`=施法者 |
| `K[i]`,`L[i]` | real[] | 单位 X/Y 坐标；`K[Er]`,`L[Er]` 用于计算方向/击退 |
| `Q[i]`,`S[i]` | real[] | **击退速度** X/Y；`KI`/`FI` 里累加 |
| `Vv[i]` | int[] | 单位的内部数组下标（所有数据数组的索引）；`GI=Vv[gI]` |
| `Xv[i]` | int[] | 施法者**当前法术等级**（出现在 `5.+.5*Xv[Er]`） |
| `nv[i]` | int[] | 单位存活/类型标志（`nv[Xr]==1` 视为可命中；`eO` 里 `nv[id]=ZX` 标记投射物类型） |
| `av[i]` | bool[] | 单位"在场"标志 |
| `rv[i]`,`iv[i]` | int[] | 投射物链表 prev/next；`kv`/`Kv` 为链表头/尾 |
| `Gn[i]` | real[] | **攻击方伤害放大系数**（出现在 `kI=...*Gn[GI]*...`） |
| `hn[i]` | real[] | **防御方受伤系数**（`...*hn[hI]*...`） |
| `Hn[i]` | real[] | 血量缩放因子（`df` 里 `Hn[id]=Hn[id]*(1./(1-.025*(ff-1)))`，随属性变化） |
| `An[i]` | real[] | 玩家 i **累计造成的总伤害**（统计） |
| `Jn[idx]` | real[] | 伤害矩阵（攻击方→防御方） |
| `bn[i]` | int[] | 单位 i 的**最后攻击者** |
| `ii/xi/ri/oi[id]` | int[] | **玩家属性点**（int/str/agi/con 类）；`eb` 中 `xb=xi+oi+ri`，`kb=.1*(ii-xb)` |
| `rx[hR]` | int[] | 目标伤害类型码（`==985`/954 触发减伤 0.8/0.4） |
| `fv[hR]`,`Dv[hR]` | bool[] | 目标**无敌/已死**标志（`KI`/`FI` 里跳过真实伤害） |
| `sr[hR]`,`tr[hR]` | bool[] | 目标减伤标志（`FI` 里 `gX=GR(hR,gX)`） |
| `le[hR]` | int[] | 击杀时记录的杀手 id |
| `ev[dX]`,`Rv[dX]` | real[] | 投射物**射程/速度**（`ev`=range，`Rv[Ib]=25` 似速度） |
| `Jv[Ib]`,`Gv[Ib]`,`hv[Ib]` | misc[] | 投射物携带数据（等级/基础能力/impact 类型），`Rb` 里 `set Jv[Ib]=Ai set Gv[Ib]=Oi set hv[Ib]=Xi` |
| `Ae[bf]`,`de[bf]` | real[] | AoE 区域：半径 / 每秒速率（`UO` 设 `Ae[bf]=IX`） |
| `Fa/Ga/Ha/ga/ha[ai]` | bool[] | **玩家升级状态标志**（`SC` 里 `if Fa[ai] then XB() else rB()` 等，决定同法术不同形态） |
| `Vi[ai]` | real[] | 施法时显示的数值（飘字） |
| `Tn` | bool | 训练/无敌模式（`FI` 里 `if Tn or gn[hI] then` 不改血只飘灰字） |
| `gr[ni]`,`Gr[ni]` | bool[] | 施法特效标志（`Gr` 生成 `thrd` 假人并立即 `KillUnit`） |
| `eO(ZX)` | fn | 从空闲链表分配单位/投射物 id（`lv`=链表头，`rv`=next） |
| `xe,ve,ee,oe,re,ie` | misc | 临时/全局单体：`re`=unit，`ie`=lightning，`oe`=effect 等 |

> 标注"推断"的需后续逐函数验证；带明确公式的（如 `Xv`/`Gn`/`hn`/`eb`）把握高。

## 三、已解码核心函数

| 函数 | 语义 | 关键公式/证据 |
|------|------|--------------|
| `KI(gI,hR,gX,JI)` | **伤害+击退计算** | `kI=('d'+UnitState(F[hR],MANA))*gX*Gn[GI]*hn[hI]*.03*Hn[hI]*JI`（`'d'`=100）；`Q[hR]+=kI*dx; S[hR]+=kI*dy`；末尾 `call FI(gI,hR,gX,.05*dx,.05*dy)` |
| `FI(gI,hR,gX,xx,yy)` | **伤害结算** | 飘字 `gX=gX*Gn[GI]*hn[hI]`；`An[GI]+=gX`；`bn[hI]=GI`；若 `HI=life-gX<.5` 则 `OI(hI)`/`gO(le[hR])` 击杀；否则 `SetUnitState(life,HI)` |
| `eb(id)` | **法术强度系数** | `xb=xi+oi+ri; kb=.1*(ii-xb); kb=max(0,kb); return 1-kb`（智力相对其他属性越高，系数越高） |
| `VO(ZX,EO,XO,x,y,OO,RO)` | **生成投射物单位** | `F[dX]=CreateUnit(Player(EO),XO,x,y,OO)`；关寻路；`G[dX]=AddSpecialEffectTarget(RO,...)`；返回 `dX` |
| `IO(dX,AO,x,y)` | **转向/加速投射物** | `Q[dX]+=AO*x/NO; S[dX]+=AO*y/NO`（NO=距离） |
| `xO(id,oO)` | **销毁投射物** | 清 `F/G/H/J`，从链表 `rv/iv` 摘除 |
| `UO(sO,wO,WO,yO,IX)` | **创建 AoE 区域** | `Ae[bf]=IX`（半径）；`de[bf]=yO/IX*.25`（速率）；返回区域 id |
| `XA(dX)` | 入清理队列 | `he[++He]=dX` |
| `fX(FX,gX,id)` | 设物品充能 | 遍历 `Xn[id]` 物品槽，`UserDataType==FX` 则 `SetItemCharges(gX)` |
| `Rb()` | **火球施法 handler** | `VO(2,ai,'e000',...,model)`；`set Jv[Ib]=Ai set Gv[Ib]=Oi set hv[Ib]=Xi`；`IO(Ib,...)` 给初速 |
| `SC()` | **施法总调度** | `qC=GetSpellAbilityId()`；嵌套 if/else 把每个 `Sxxx` 路由到 handler |
| `ob`/`rb`/`nb` | **命中 impact** | `KI(Er,Xr, 6.3+.7*Xv[Er], 1.1*eb(Vv[Er]))` / `KI(...,5.+.5*Xv[Er],...)`；`rb`/`nb` 额外 `UO(...)` 做 AoE，半径 `2.5*(1.2+.2*Xv[Er])` / `3.*(1.2+.2*Xv[Er])` |

## 四、法术调度表（SC 内的 Sxxx → handler）

提取自 `SC`（`qC=='Sxxx' then call HANDLER()`）：

```
S000 -> Rb      S002 -> hb      S004 -> ub      S006 -> fC
S008 -> XB/rB   S010 -> RB/OB   S012 -> wB/IB   S014 -> ic
S016 -> dc/gc   S018 -> mc/...  S020 -> MC      ...(至 S031 链释放)
```

注：很多法术有**升级分支**——`SC` 里 `if Fa[ai] then XB() else rB()`（S008）、`if Ga[ai] then wB() else IB()`（S012）等；`Fa/Ga/Ha/ga/ha[ai]` 是玩家购买的升级标志，决定同法术的不同形态/数值。完整 53 条需把 `SC` 余下部分（S016→S0xx 及 A/M/W 系）也解析出来。

## 五、火球（S000）完整解码（示范）

1. 玩家对 `S000` 施法 → `SC` 路由到 `Rb()`。
2. `Rb()`：`Ib=VO(2,ai,'e000',K[ni],L[ni],facing,"RedDragonMissile.mdl"/"fb2.mdl")`；`Jv[Ib]=Ai; Gv[Ib]=Oi; hv[Ib]=Xi`；`IO(Ib, speed, TX,TY)` 朝目标初速。
3. 飞行中 `Nb` 碰撞回调命中 `Xr` → 调 impact。
4. impact（`rb` 型，小 AoE）：
   - `KI(Er,Xr, 5.+.5*Xv[Er], 1.1*eb(Vv[Er]))` → 基础伤害 `5+0.5×等级`，再乘 `1.1×法术强度`。
   - 若 `nv[Xr]==1`（英雄/有效目标）：`bf=UO(nr,Xr,Vv[Er], yO*ab, 5.*jn[Vv[Er]]*ab)` 生成 AoE，半径 `yO=2.5*(1.2+.2*Xv[Er])`（随等级放大），`ab=jn[caster]/jn[target]` 血量比缩放。
   - `KI(... 5.+.5*Xv[Er] ...)` 对中心目标直接结算。
5. `KI`→`FI`：扣血（含 `100+mana` 魔法强度项）、击退、飘字、统计 `An`，致死则 `OI` 击杀。

→ 火球：**单体+小范围溅射，基础 5+0.5×Lv，半径 2.5×(1.2+0.2×Lv)，伤害含目标法力加成项**。

## 六、法术调度表（SC 内 Sxxx → handler，完整版 2026-09-02）

`SC()` 读 `qC=GetSpellAbilityId()`，用嵌套 if/else 树把 `S000`–`S031` 路由到 handler。升级分支由玩家购买标志决定同一技能的不同形态：

| 技能码 | handler | 升级条件 | 备注 |
|---|---|---|---|
| S000 | Rb | — | 火球（impact: Xi→ob，升级 Ri→rb/Ii→nb 带 AoE 点燃）|
| S001 | KC | — | 直接 KI（handler 内）|
| S002 | hb | — | 连锁闪电，路由到 `Nb`（末端 LI/KI 结算）|
| S003 | pb | — | impact Di→Kb |
| S004 | ub | — | 回力镖/弧光，impact Bi→sb |
| S005 | DC | — | impact va→CC（控制/转换）|
| S006 | fC | — | 时光回溯（无伤害，回放单位状态）|
| S007 | jR | Ha[ai] else 空 | ？|
| S008 | XB / rB | Fa[ai] | 陨石（滚石 XB / 另一种 rB）|
| S009 | gB | — | impact Yi→fB (KI=3,1.4) |
| S010 | RB / OB | Fa[ai] | 疾风步（冲撞 RB / 隐身 OB）impact Ei→bA |
| S011 | hB | — | ？|
| S012 | wB / IB | Ga[ai] | 冲撞（凤凰 wB / 另一种 IB）|
| S013 | MB / mB | Ga[ai] | impact si→lB（区域）|
| S014 | ic | — | 汲取（减速），impact mi→ZB / Mi→xc |
| S015 | Ic | — | impact Li→nc（→jI 伤害）|
| S016 | dc / gc | ga[ai] | 生命汲取，impact Ji→Cc / Gi→Fc |
| S017 | bC / eC | ha[ai] | impact Va→NC / ia→zc |
| S018 | mc / jc | ha[ai] | 引力球（力场 mc / 另一种 jc）|
| S019 | Tc / tc | ha[ai] | impact Aa→qc / Ia→Qc |
| S020 | MC | — | 吸血/多段命中（handler 内直接 KI×3）|
| S021 | pC | — | ？|
| S022–S023 | — | — | 未用 |
| S024–S027 | — | — | 形态变换（u000↔u001/u002/u003/u004，S027 还解锁 T000–T006）|
| S028–S029 | — | — | 未用 |
| S030 | — | — | 持续时间列表显示（聊天）|
| S031 | oR / Pc | 等级==1 else | 特殊 |

## 七、关键机制：impact 绑定（boolexpr 全局 → 函数）

碰撞引擎把投射物的 `hv[dX]`（`boolexpr` 数组）挂到触发器 `Ge` 上，碰撞时 `TriggerAddCondition(Ge,hv[dX]); TriggerEvaluate(Ge)` 调用它。
`hv[Ib]=Xi` 里的 `Xi` 等是全局 `boolexpr`，在初始化里用 `set Xi=Condition(function ob)` 绑定。
**完整绑定表**（左=全局，右=impact 函数）：

```
Xi→ob  Oi→Xb  Ri→rb  Ii→nb  Ni→Eb  bi→Ob  va→CC  ea→dC
si→lB  Si→LB  ti→kB  Bi→sb  di→tb  ci→Tb  Ci→Qb  Di→Kb  fi→lb
Fi→mb  gi→Lb  Gi→Fc  hi→Dc  Hi→fc  ji→Bc  Ji→Cc  ki→cc  Ki→ac
li→Vc  Li→nc  mi→ZB  Mi→xc  pi→YB  Pi→oc  qi→rc  Qi→zB  Ti→oB
ui→xB  Ui→bB  wi→NB  Wi→BB  yi→cB  Yi→fB  zi→dB  Zi→FB
Ia→Qc  Aa→qc  Na→sc  ba→Sc  ia→zc  aa→Zc  na→vC  Va→NC
Xa→Gc  Oa→Hc  Da→kc  Ca→Jc  fa→Kc  da→lc  Ba→nB  ca→VB
xa→QB  oa→sB  ra→PB  (ZE 等其它为系统条件)
```

## 八、伤害/区域二级函数

- `jI(gI,hR,gX,JI)`：**真正结算伤害**（与 `KI` 同源）：`kI='d'(=100)*gX*JI*.03`（含 mana、`Gn`/`hn`/`Hn` 缩放）；`rx[hR]==985` 减伤 0.8，`==954` 减伤 0.4，`Dv` 无敌则 0，`fv` 则按法力折算。
- `qI(gI,gX,mI)` → `PI(gI,gX,mI,x,y)` → `LI(gI,gX,mI,x,y, MI=pe*(1+.12*ri[id]), pI=.15*ri[id])`：**区域伤害**（带距离衰减 `gX*(pI+(1-pI)*NO)`）。许多 impact（sb/nc/lB/CC 等）经此路径。
- `eb(id)=1-.1*max(0,智力-(力+敏+体))`：法术强度系数（出现在 `1.1*eb(...)` 类 mult）。

## 九、逐法术数值表

已生成 **`abilities_detailed_098b.md`**：每个 Sxxx 给出 投射物型号、射程 `ev`、命中半径 `Rv`、等级来源、impact 绑定、以及 `KI`/`UO`/`qI` 的 base/mult 公式。这是 098b_CN 版的 `abilities_detailed.md` 等效物。

> 注：CD/持续时间类数值若不在 handler 硬编码里，可能在 `df`（RESEARCH_FINISH）或全局计时器；需随后核查。
