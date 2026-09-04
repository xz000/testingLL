# 098b_CN 逐法术数值表（abilities_detailed_098b）

> 来源：`ce_old98b/war3map.j.dec` 解混淆。法术机制硬编码在 JASS handler + impact 函数里，
> 物体数据（w3a）的同名自定义字段为遗留废数据。伤害统一经 `KI(gI,hR,base,mult)`；
> `KI` 公式含 `100+mana`、`Gn[攻击方]`、`hn[防御方]`、`Hn[血量缩放]` 等系数（见 jass_deobf.md）。
> `eb(id)=1-.1*max(0,智力-其它属性之和)` 为法术强度系数。

## 调度表（SC 内 Sxxx → handler，含升级分支 Fa/Ga/Ha/ga）

| 技能码 | handler | 升级条件 | 名称/备注 |
|---|---|---|---|
| S000 | Rb | — | Fireball |
| S001 | KC | — | ? |
| S002 | hb | — | Lightning |
| S003 | pb | — | ? |
| S004 | ub | — | Boomerang/Glaive |
| S005 | DC | — | ? |
| S006 | fC | — | Time Rewind |
| S007 | jR | Ha | ? (Ha branch empty) |
| S008 | XB | Fa | Meteor (Fa: rolling) |
| S008 | rB | Fa-else | Meteor alt |
| S009 | gB | — | ? |
| S010 | RB | Fa | Wind Walk (Fa: charge) |
| S010 | OB | Fa-else | Wind Walk (invis) |
| S011 | hB | — | ? |
| S012 | wB | Ga | Charge (Ga: Phoenix) |
| S012 | IB | Ga-else | Charge alt |
| S013 | MB | Ga | ? (Ga: drain) |
| S013 | mB | Ga-else | ? |
| S014 | ic | — | Drain (slow) |
| S015 | Ic | — | ? |
| S016 | dc | ga | ? (ga: life drain) |
| S016 | gc | ga-else | ? |
| S017 | bC | ha | ? (ha) |
| S017 | eC | ha-else | ? |
| S018 | mc | ha | Gravity Ball (ha: field) |
| S018 | jc | ha-else | Gravity Ball alt |
| S019 | Tc | ha | ? (ha) |
| S019 | tc | ha-else | ? |
| S020 | MC | — | Vampiric/Multi-hit |
| S021 | pC | — | ? |
| S022 | — | — | unused |
| S023 | — | — | unused |
| S024 | — | — | Form morph u000<->u003 |
| S025 | — | — | Form morph -> u001 |
| S026 | — | — | Form morph -> u002 |
| S027 | — | — | Form morph -> u004 (+T000-T006) |
| S028 | — | — | unused |
| S029 | — | — | unused |
| S030 | — | — | Duration list display |
| S031 | oR | — | S031 lvl1 |
| S031 | Pc | — | S031 lvl>1 |

## 逐法术细节

### S000 → `Rb`  (Fireball)

- 投射物型号: Abilities\\Weapons\\RedDragonBreath\\RedDragonMissile.mdl, fb2.mdl
- 射程/飞行时间 `ev`: `(1+.1*oi[ai])`
- 命中半径 `Rv`: `25`
- 等级来源: `wr[ai]`
- impact 绑定: `Xi`→`ob`, `Ri`→`rb`, `Ii`→`nb`
- impact `nb`:
  - `KI(Er,Xr,5.+.5*Xv[Er],1.1*eb(Vv[Er])`
  - `KI(Er,Xr,5.+.5*Xv[Er],1.1*eb(Vv[Er])`
- impact `ob`:
  - `KI(Er,Xr,6.3+.7*Xv[Er],1.1*eb(Vv[Er])`
- impact `rb`:
  - `KI(Er,Xr,5.+.5*Xv[Er],1.1*eb(Vv[Er])`
  - `KI(Er,Xr,5.+.5*Xv[Er],1.1*eb(Vv[Er])`

### S001 → `KC`  (?)

- 投射物型号: Abilities\\Spells\\Human\\Invisibility\\InvisibilityTarget.mdl, Abilities\\Spells\\Demon\\DarkPortal\\DarkPortalTarget.mdl, Nova_yellow.mdl
- 无投射物 impact；handler 内直接伤害调用: `KI`×1

### S002 → `hb`  (Lightning)

- 无投射物 impact；handler 内直接伤害调用: `Nb`×1
  - 注：`Nb` 为连锁闪电判定，末端经 `LI`/`KI` 结算伤害

### S003 → `pb`  (?)

- 投射物型号: Abilities\\Spells\\NightElf\\SpiritOfVengeance\\SpiritOfVengeanceBirthMissile.mdl
- 射程/飞行时间 `ev`: `4.5*(1+1.5*.1*oi[ai])`
- 命中半径 `Rv`: `Dr`
- 等级来源: `Wr[ai]`
- impact 绑定: `Di`→`Kb`
- impact `Kb`:
  - `KI(Er,Xr,jb(Er)`

### S004 → `ub`  (Boomerang/Glaive)

- 投射物型号: ArcaneGlaive.mdl
- 射程/飞行时间 `ev`: `-Ub/ Wb*.03`
- 命中半径 `Rv`: `40`
- 等级来源: `Wr[ai]`
- impact 绑定: `Bi`→`sb`
- impact `sb`:
  - `KI(Er,Xr,6.4+.8*Xv[Er],mI)`
  - `qI(Er,6.4+.8*Xv[Er],.5*mI)`  → 区域伤害(PI→LI, 半径 pe*(1+.12*ri))

### S005 → `DC`  (?)

- 投射物型号: Abilities\\Spells\\Human\\MagicSentry\\MagicSentryCaster.mdl
- 命中半径 `Rv`: `'x'`
- impact 绑定: `va`→`CC`
- impact `CC`:
  - (无直接 KI/UO；可能经 jI/AC/wc 等二级函数或纯控制效果)

### S006 → `fC`  (Time Rewind)

- 射程/飞行时间 `ev`: `ev[ni]`
- 无投射物 KI；效果在 handler 内直接施加（控制/位移/变身/回放等，非直接伤害）

### S007 → `jR`  (? (Ha branch empty))  [条件 Ha]

- 投射物型号: Abilities\\Spells\\Orc\\Voodoo\\VoodooAuraTarget.mdl
- 无投射物 KI；效果在 handler 内直接施加（控制/位移/变身/回放等，非直接伤害）

### S008 → `XB`  (Meteor (Fa: rolling))  [条件 Fa]

- 投射物型号: Abilities\\Weapons\\BallsOfFireMissile\\BallsOfFireMissile.mdl
- 射程/飞行时间 `ev`: `2*(1+.1*oi[ai])`
- 命中半径 `Rv`: `72`
- 等级来源: `yr[ai]`
- impact 绑定: `Ba`→`nB`
- impact `nB`:
  - `KI(Er,Xr,$A+2*Xv[Er],.8)`

### S008 → `rB`  (Meteor alt)  [条件 Fa-else]

- 投射物型号: Abilities\\Weapons\\BallsOfFireMissile\\BallsOfFireMissile.mdl
- 射程/飞行时间 `ev`: `1.35`
- 等级来源: `yr[ai]`
- 无投射物 KI；效果在 handler 内直接施加（控制/位移/变身/回放等，非直接伤害）

### S009 → `gB`  (?)

- 投射物型号: Abilities\\Weapons\\SpiritOfVengeanceMissile\\SpiritOfVengeanceMissile.mdl
- 射程/飞行时间 `ev`: `GB/ 280`
- 命中半径 `Rv`: `50`
- 等级来源: `yr[ai]`
- impact 绑定: `Yi`→`fB`
- impact `fB`:
  - `KI(Er,Xr,3,1.4)`

### S010 → `RB`  (Wind Walk (Fa: charge))  [条件 Fa]

- 投射物型号: Abilities\\Spells\\Undead\\ReplenishMana\\SpiritTouchTarget.mdl
- 命中半径 `Rv`: `fo`
- impact 绑定: `Ei`→`bA`
- impact `bA`:
  - `KI(Er,Xr,4.6+.8*yr[Vv[Er]]+5.+.4*Yr[Vv[Er]],.75)`
  - `KI(Er,Xr,4.6+.8*yr[Vv[Er]],1)`
  - `KI(Er,Xr,5.+.4*Yr[Vv[Er]],1)`
  - `qI(Er,5.+.4*Yr[Vv[Er]],1)`  → 区域伤害(PI→LI, 半径 pe*(1+.12*ri))

### S010 → `OB`  (Wind Walk (invis))  [条件 Fa-else]

- 投射物型号: Abilities\\Spells\\Undead\\ReplenishMana\\SpiritTouchTarget.mdl
- 命中半径 `Rv`: `fo`
- impact 绑定: `Ei`→`bA`
- impact `bA`:
  - `KI(Er,Xr,4.6+.8*yr[Vv[Er]]+5.+.4*Yr[Vv[Er]],.75)`
  - `KI(Er,Xr,4.6+.8*yr[Vv[Er]],1)`
  - `KI(Er,Xr,5.+.4*Yr[Vv[Er]],1)`
  - `qI(Er,5.+.4*Yr[Vv[Er]],1)`  → 区域伤害(PI→LI, 半径 pe*(1+.12*ri))

### S011 → `hB`  (?)

- 投射物型号: Abilities\\Spells\\Undead\\CarrionSwarm\\CarrionSwarmDamage.mdl, Abilities\\Spells\\Undead\\DeathCoil\\DeathCoilSpecialArt.mdl
- 无投射物 KI；效果在 handler 内直接施加（控制/位移/变身/回放等，非直接伤害）

### S012 → `wB`  (Charge (Ga: Phoenix))  [条件 Ga]

- 投射物型号: units\\human\\phoenix\\phoenix.mdl
- 无投射物 KI；效果在 handler 内直接施加（控制/位移/变身/回放等，非直接伤害）

### S012 → `IB`  (Charge alt)  [条件 Ga-else]

- 射程/飞行时间 `ev`: `Ar/ Hr*.03+3*.03`
- 命中半径 `Rv`: `fo`
- impact 绑定: `Ei`→`bA`
- impact `bA`:
  - `KI(Er,Xr,4.6+.8*yr[Vv[Er]]+5.+.4*Yr[Vv[Er]],.75)`
  - `KI(Er,Xr,4.6+.8*yr[Vv[Er]],1)`
  - `KI(Er,Xr,5.+.4*Yr[Vv[Er]],1)`
  - `qI(Er,5.+.4*Yr[Vv[Er]],1)`  → 区域伤害(PI→LI, 半径 pe*(1+.12*ri))

### S013 → `MB`  (? (Ga: drain))  [条件 Ga]

- 投射物型号: Abilities\\Weapons\\DragonHawkMissile\\DragonHawkMissile.mdl
- 射程/飞行时间 `ev`: `Bb/ 800`
- 命中半径 `Rv`: `40`
- impact 绑定: `si`→`lB`
- impact `lB`:
  - (无直接 KI/UO；可能经 jI/AC/wc 等二级函数或纯控制效果)

### S013 → `mB`  (?)  [条件 Ga-else]

- 投射物型号: Abilities\\Weapons\\SerpentWardMissile\\SerpentWardMissile.mdl
- 射程/飞行时间 `ev`: `Bb/ $6A4`
- 命中半径 `Rv`: `40`
- impact 绑定: `si`→`lB`
- impact `lB`:
  - (无直接 KI/UO；可能经 jI/AC/wc 等二级函数或纯控制效果)

### S014 → `ic`  (Drain (slow))

- 投射物型号: Abilities\\Spells\\Undead\\DeathCoil\\DeathCoilMissile.mdl
- 射程/飞行时间 `ev`: `Ar/ 700`
- 命中半径 `Rv`: `27`
- 等级来源: `zr[ai]`
- impact 绑定: `mi`→`ZB`, `Mi`→`xc`
- impact `xc`:
  - `KI(Er,Xr,yO,.6)`
- impact `ZB`:
  - `KI(Er,Xr,yO,.2)`

### S015 → `Ic`  (?)

- 投射物型号: Abilities\\Weapons\\RedDragonBreath\\RedDragonMissile.mdl, fb2.mdl
- 射程/飞行时间 `ev`: `800*(1+.1*oi[ai])/ 900`
- 命中半径 `Rv`: `22`
- 等级来源: `zr[ai]`
- impact 绑定: `Li`→`nc`
- impact `nc`:
  - `jI(Er,Xr,2.6+.4*Xv[Er],.65)`  → 核心伤害结算(同 KI: 100*gX*JI*.03 含减伤)

### S016 → `dc`  (? (ga: life drain))  [条件 ga]

- 投射物型号: Abilities\\Spells\\Items\\OrbCorruption\\OrbCorruptionMissile.mdl
- 射程/飞行时间 `ev`: `900*(1+.1*oi[ai])/ 900`
- 命中半径 `Rv`: `35`
- 等级来源: `zr[ai]`
- impact 绑定: `Ji`→`Cc`
- impact `Cc`:
  - `KI(Er,Xr,gv[Er]*(5.1+.9*Xv[Er])`

### S016 → `gc`  (?)  [条件 ga-else]

- 投射物型号: Abilities\\Weapons\\IllidanMissile\\IllidanMissile.mdl
- 射程/飞行时间 `ev`: `(750+$96*zr[ai])*(1+.1*oi[ai])/ 900`
- 命中半径 `Rv`: `38`
- 等级来源: `zr[ai]`
- impact 绑定: `Gi`→`Fc`
- impact `Fc`:
  - `KI(Er,Xr,gv[Er]*(5+Xv[Er])`

### S017 → `bC`  (? (ha))  [条件 ha]

- 投射物型号: Abilities\\Weapons\\FaerieDragonMissile\\FaerieDragonMissile.mdl
- 射程/飞行时间 `ev`: `((Ar+$D2)/ 900)*1.046751601`
- 命中半径 `Rv`: `23`
- impact 绑定: `Va`→`NC`, `Va`→`NC`
- impact `NC`:
  - (无直接 KI/UO；可能经 jI/AC/wc 等二级函数或纯控制效果)

### S017 → `eC`  (?)  [条件 ha-else]

- 投射物型号: Abilities\\Weapons\\MurgulMagicMissile\\MurgulMagicMissile.mdl
- 射程/飞行时间 `ev`: `1.1*(1+.1*oi[ai])`
- 命中半径 `Rv`: `39`
- impact 绑定: `ia`→`zc`
- impact `zc`:
  - (无直接 KI/UO；可能经 jI/AC/wc 等二级函数或纯控制效果)

### S018 → `mc`  (Gravity Ball (ha: field))  [条件 ha]

- 投射物型号: Abilities\\Weapons\\BansheeMissile\\BansheeMissile.mdl
- 射程/飞行时间 `ev`: `(SquareRoot(AX(((TX)*1.),((TY)*1.),((K[ni])*1.),((L[ni])*1.))))`
- 无投射物 KI；效果在 handler 内直接施加（控制/位移/变身/回放等，非直接伤害）

### S018 → `jc`  (Gravity Ball alt)  [条件 ha-else]

- 投射物型号: Abilities\\Spells\\Undead\\DarkSummoning\\DarkSummonMissile.mdl
- 射程/飞行时间 `ev`: `900*(1+.1*oi[ai])/ 400`
- 无投射物 KI；效果在 handler 内直接施加（控制/位移/变身/回放等，非直接伤害）

### S019 → `Tc`  (? (ha))  [条件 ha]

- 投射物型号: Abilities\\Weapons\\VengeanceMissile\\VengeanceMissile.mdl
- 命中半径 `Rv`: `35`
- impact 绑定: `Aa`→`qc`
- impact `qc`:
  - (无直接 KI/UO；可能经 jI/AC/wc 等二级函数或纯控制效果)

### S019 → `tc`  (?)  [条件 ha-else]

- 投射物型号: Abilities\\Weapons\\FarseerMissile\\FarseerMissile.mdl
- 命中半径 `Rv`: `35`
- impact 绑定: `Ia`→`Qc`
- impact `Qc`:
  - (无直接 KI/UO；可能经 jI/AC/wc 等二级函数或纯控制效果)

### S020 → `MC`  (Vampiric/Multi-hit)

- 投射物型号: Abilities\\Weapons\\PhoenixMissile\\Phoenix_Missile.mdl, Abilities\\Spells\\Human\\Invisibility\\InvisibilityTarget.mdl, Abilities\\Spells\\Demon\\DarkPortal\\DarkPortalTarget.mdl
- 无投射物 impact；handler 内直接伤害调用: `KI`×3

### S021 → `pC`  (?)

- 投射物型号: Abilities\\Spells\\Other\\Drain\\DrainCaster.mdl, Abilities\\Spells\\Demon\\DarkPortal\\DarkPortalTarget.mdl, Abilities\\Spells\\Human\\HolyBolt\\HolyBoltSpecialArt.mdl, Abilities\\Spells\\Human\\Invisibility\\InvisibilityTarget.mdl, Nova_yellow.mdl
- 无投射物 impact；handler 内直接伤害调用: `KI`×1

### S022 — unused

### S023 — unused

### S024 — Form morph u000<->u003

### S025 — Form morph -> u001

### S026 — Form morph -> u002

### S027 — Form morph -> u004 (+T000-T006)

### S028 — unused

### S029 — unused

### S030 — Duration list display

### S031 → `oR`  (S031 lvl1)

- 无投射物 KI；效果在 handler 内直接施加（控制/位移/变身/回放等，非直接伤害）

### S031 → `Pc`  (S031 lvl>1)

- 无投射物 KI；效果在 handler 内直接施加（控制/位移/变身/回放等，非直接伤害）
