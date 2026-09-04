# 098b_CN 逐法术总表（consolidated）

> **混合模型**：物体数据负责【名称 / 冷却 CD(acdn) / 施法距离(aran) / 最大等级(alev) / 耗蓝(aman) / 吟唱(acas)】；
> JASS 代码负责【弹道模型 / 伤害数值(KI) / 范围(AoE) / 击退 / 持续时间(按 jn[玩家] 缩放)】。
> 修正旧结论：物体数据**不是**废数据——名字和 CD 都在里面；只有伤害/AoE/击退在代码里。
> 控制/辅助技的真实机制见 `abilities_control_098b.md`（原先标"(控制/位移/无直接伤害)"的多为误判：天罚/闪电/灾变其实打伤害，虔诚是奶+伤双效）。

| 技能 | 名称 | handler | 升级 | 等级 | 冷却CD(s) | 距离 | 耗蓝 | 弹道模型 | 伤害/AoE 公式 (JASS) |
|---|---|---|---|---|---|---|---|---|---|
| S000 | 火球 | Rb | — | 24 | 4.8 | 99999.0 | - | Abilities\\Weapons\\RedDragonBreath\\RedDragonMissile.mdl, fb2.mdl | KI(Er,Xr,6.3+.7*Xv[Er],1.1*eb(Vv[Er]) ; KI(Er,Xr,5.+.5*Xv[Er],1.1*eb(Vv[Er]) ; KI(Er,Xr,5.+.5*Xv[Er],1.1*eb(Vv[Er]) ; KI(Er,Xr,5.+.5*Xv[Er],1.1*eb(Vv[Er]) ; KI(Er,Xr,5.+.5*Xv[Er],1.1*eb(Vv[Er]) |
| S001 | 天罚 | KC | — | - | 3.0 | - | - | InvisibilityTarget / DarkPortalTarget / Nova_yellow | 近身AoE爆发：半径250内敌人均受 KI 伤害，随距离衰减（**有伤害**） |
| S002 | 闪电 | hb | — | 9 | 16.5..12.0 (各等级) | - | - |  | 闪电光束：伤害 6+等级，射程 (1+0.15×oi)×600（**有伤害**） |
| S003 | 追踪弹 | pb | — | 9 | 15.0..9.5 (各等级) | - | - | Abilities\\Spells\\NightElf\\SpiritOfVengeance\\SpiritOfVengeanceBirthMissile.mdl | KI(Er,Xr,jb(Er) |
| S004 | 回旋镖 | ub | — | 9 | 16.0..8.2 (各等级) | - | - | ArcaneGlaive.mdl | KI(Er,Xr,6.4+.8*Xv[Er],mI) ; qI(Er,6.4+.8*Xv[Er],.5*mI)→区域 |
| S005 | 反射盾 | DC | — | 9 | 25.0..14.0 (各等级) | 0.0 | - | MagicSentryCaster | 反弹护盾：持续 (2.6+0.2×vi)×jn 秒，期间反弹来袭弹体（va=CC 反弹 / ea=dC 跟随） |
| S006 | 时光回溯 | fC | — | 8 | 22.0..12.0 (各等级) | 0.0 | - |  | 延时回溯：记录当前位置，3.6×jn 秒后闪回该处（ER 还原 K/L/Q/S…） |
| S007 | 急行 | jR | Ha | 20 | 21.0..13.0 (各等级) | 0.0 | - | VoodooAuraTarget | 疾跑：+35 移速、+攻速（tr=3+2×vi），持续 (6.2+0.8×vi)×jn 秒 |
| S008 | 陨石 | XB | Fa | 20 | 20.0..16.5 (各等级) | 1200.0 | - | Abilities\\Weapons\\BallsOfFireMissile\\BallsOfFireMissile.mdl | KI(Er,Xr,$A+2*Xv[Er],.8) |
| S008 | 陨石 | rB | Fa-else | 20 | 20.0..16.5 (各等级) | 1200.0 | - | Abilities\\Weapons\\BallsOfFireMissile\\BallsOfFireMissile.mdl | (控制/位移/无直接伤害) |
| S009 | 分裂弹 | gB | — | 20 | 30.0..20.0 (各等级) | 99999.0 | - | Abilities\\Weapons\\SpiritOfVengeanceMissile\\SpiritOfVengeanceMissile.mdl | KI(Er,Xr,3,1.4) |
| S010 | 疾风步 | RB | Fa | 20 | 30.0..17.0 (各等级) | - | - | Abilities\\Spells\\Undead\\ReplenishMana\\SpiritTouchTarget.mdl | KI(Er,Xr,4.6+.8*yr[Vv[Er]]+5.+.4*Yr[Vv[Er]],.75) ; KI(Er,Xr,4.6+.8*yr[Vv[Er]],1) ; KI(Er,Xr,5.+.4*Yr[Vv[Er]],1) ; qI(Er,5.+.4*Yr[Vv[Er]],1)→区域 |
| S010 | 疾风步 | OB | Fa-else | 20 | 30.0..17.0 (各等级) | - | - | Abilities\\Spells\\Undead\\ReplenishMana\\SpiritTouchTarget.mdl | KI(Er,Xr,4.6+.8*yr[Vv[Er]]+5.+.4*Yr[Vv[Er]],.75) ; KI(Er,Xr,4.6+.8*yr[Vv[Er]],1) ; KI(Er,Xr,5.+.4*Yr[Vv[Er]],1) ; qI(Er,5.+.4*Yr[Vv[Er]],1)→区域 |
| S011 | 瞬间移动 | hB | — | 9 | 16.0..5.5 (各等级) | 99999.0 | - | CarrionSwarmDamage / DeathCoilSpecialArt | 闪现：距离 700+70×Yr，超距则传最大射程；低血落特殊地形触发"死里逃生" |
| S012 | 冲撞 | wB | Ga | 20 | 16.5..8.5 (各等级) | 99999.0 | - | units\\human\\phoenix\\phoenix.mdl | 冲撞(升级)：相位状态 3.1×jn 秒，移动即触发冲撞击退（uB/IO） |
| S012 | 冲撞 | IB | Ga-else | 20 | 16.5..8.5 (各等级) | 99999.0 | - |  | 冲撞：朝目标冲刺，最大 (650+50×Yr)×(1+0.1×oi)，命中 KI 伤害+CX 击退（Ei=bA） |
| S013 | 移形换位 | MB | Ga | 20 | 16.0..4.0 (各等级) | 99999.0 | - | DragonHawkMissile | 移形换位：与目标交换位置（si=lB 互换 K/L），射程 600×(1+0.1×oi) |
| S013 | 移形换位 | mB | Ga-else | 20 | 16.0..4.0 (各等级) | 99999.0 | - | SerpentWardMissile | 移形换位：与目标交换位置，射程 900×(1+0.1×oi) |
| S014 | 汲取 | ic | — | 20 | 22.0..19.5 (各等级) | 99999.0 | - | Abilities\\Spells\\Undead\\DeathCoil\\DeathCoilMissile.mdl | KI(Er,Xr,yO,.2) ; KI(Er,Xr,yO,.6) |
| S015 | 火焰喷射 | Ic | — | 20 | 15.0..7.0 (各等级) | 99999.0 | - | RedDragonBreath / fb2 | 喷火：点脉冲（每0.08s）或升级锥形5弹，AoE 火焰伤害 jI×0.65（Li=nc） |
| S016 | 弹跳弹 | dc | ga | 20 | 20.0..21.0 (各等级) | 99999.0 | - | Abilities\\Spells\\Items\\OrbCorruption\\OrbCorruptionMissile.mdl | KI(Er,Xr,gv[Er]*(5.1+.9*Xv[Er]) |
| S016 | 弹跳弹 | gc | ga-else | 20 | 20.0..21.0 (各等级) | 99999.0 | - | Abilities\\Weapons\\IllidanMissile\\IllidanMissile.mdl | KI(Er,Xr,gv[Er]*(5+Xv[Er]) |
| S017 | 致残 | bC | ha | 20 | 21.0..12.5 (各等级) | 99999.0 | - | FaerieDragonMissile | 致残(升级)：双链弹，Va=NC 两弹连线解析 |
| S017 | 致残 | eC | ha-else | 20 | 21.0..12.5 (各等级) | 99999.0 | - | MurgulMagicMissile | 致残：残废 debuff（wc）+ 小范围 AoE（ia=zc） |
| S018 | 引力 | mc | ha | 20 | 26.0 | 99999.0 | - | BansheeMissile | 引力(升级)：生成持续 5×jn 秒的吸附漩涡场（da=lc, Gv=Da） |
| S018 | 引力 | jc | ha-else | 20 | 26.0 | 99999.0 | - | DarkSummonMissile | 引力：把敌人拉向落点（Gv=Oa=Hc 清理） |
| S019 | 锁链 | Tc | ha | 20 | 14.5..16.0 (各等级) | 99999.0 | - | VengeanceMissile | 锁链(升级)：拉目标向己，AoE 友/己+移速（Aa=qc）；解锁 S031 |
| S019 | 锁链 | tc | ha-else | 20 | 14.5..16.0 (各等级) | 99999.0 | - | FarseerMissile | 锁链：拉目标向己（Ia=Qc）；解锁 S031 |
| S020 | 灾变 | MC | — | 1 | 3.0/2.0 | - | - | DarkPortalTarget / InvisibilityTarget / Phoenix_Missile | 灾变：逐级强化 AoE 终极（Ur 0→1→2，伤害 $B→$C→$E，半径 300/300/400，每级+50移速）（**有伤害**） |
| S021 | 虔诚 | pC | — | - | 3.0 | - | - | InvisibilityTarget / DrainCaster / HolyBolt / DarkPortalTarget / Nova_yellow | 虔诚：打敌(250,随距衰减)+奶友(500,+60移速,+DX×0.5血/-DX×0.5蓝) 双效光环（**有伤害+治疗**） |
| S022 | ? | — | — | - | - | - | - |  |  |
| S023 | ? | — | — | - | - | - | - |  |  |
| S024 | 物品 | — | — | - | - | - | - |  |  |
| S025 | 法术 1 | — | — | - | - | - | - |  |  |
| S026 | 法术 2 | — | — | - | - | - | - |  |  |
| S027 | 法术 3 | — | — | - | 0.0 | - | - |  |  |
| S028 | ? | — | — | - | - | - | - |  |  |
| S029 | ? | — | — | - | - | - | - |  |  |
| S030 | 怀表 item details | — | — | 1 | - | 0.0 | - |  |  |
| S031 | 锁链附加动作 | oR | — | 2 | 1.0 | 0.0 | - |  | 链锁收尾：销毁链光/移除单位，复位 S019 开、S031 关 |
| S031 | 锁链附加动作 | Pc | — | 2 | 1.0 | 0.0 | - |  | 链锁附加：定身被拉目标（jO + 3×jn 秒） |

## 说明
- 冷却 `acdn` 多为**随等级递增**（高等级=高伤害+长冷却，平衡取舍）。例如闪电 S002 各等级 12.5→25s。
- `aran`=99999 表示"全图任意点"的目标型施法；`amcs`(导弹速度)=0 因为弹道是代码自绘的假人单位。
- 持续时间（点燃/减速等）不在此表，写在 JASS impact 里并按 `jn[ai]` 缩放（如 火球点燃 2.5×jn、反射盾 (2.6+.2×vi)×jn）。
- S024–S027 为形态变换（切换 u000/u001/u002/u003/u004 模型），S032–S036 为切换快捷键的辅助技能，S030 为信息显示。
- 升级分支由玩家购买标志决定：Fa(S008/S010)、Ga(S012/S013)、Ha(S007)、ha(S017–S019)、ga(S016)。