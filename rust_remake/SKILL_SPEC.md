# 技能全量规格 — 依据原版 Unity 源码核对

> 本文档是与 `Assets/Scripts/Play/Skills/*`、`Bullets/*` 逐项核对后的**权威机制真值表**，
> 用于指导 `game-core` 全部技能落地。凡标 `⚠` 者为当前 Rust 实现与原版不同的地方。

## 通用系统（源码里反复出现的机制，需在 game-core 落地）

| 系统 | 源码 | 说明 |
|------|------|------|
| 击退 `GetPushed(velo,time)` | RBScript | 受击者失去控制、以定速被推，`time` 后恢复 |
| 踢击 `ColliderScript::StartKick` | ColliderScript | 开启"持续踢击窗口"：窗口内撞到人→击退+伤害，然后关闭 |
| 隐身 `StealthByTime(time,DoLSDS)` | StealthScript | 隐藏自身；`DoLSDS`=同时关闭碰撞（撞到人不判定），用于冲刺斩 |
| 束缚 `DoSkill.GetTied(time)` | DoSkill | `CanSing=false`，无法施法/被清火点，持续 `time` |
| 移动速度附加 `VelotoAdd` / `cook` | MoveScript / CentrallyConstentField / Blue/RedLine | 每物理帧向 `VelotoAdd` 累加一个向量（例如引力拉拽） |
| 反弹反射 `MirrorBy(sp,vp)` | Fix64Vector2 / ShieldScript / BoomerangScript | 按法线反射速度（护盾、回旋镖撞墙用） |
| 障碍（墙/柱子） | 场景碰撞体（LetMeGo 里只有 `CircleCollider2D`、无墙） | 原版没有专门墙体；重写以**独立圆形障碍 `Obstacle`** 表示，配套 **射线-圆求交 `raycast_first`**（R3b 闪墙、将来子弹挡墙/束缚用）与**玩家-圆盘分离**。`raycast_first` 同时命中障碍和其他玩家，贴原版 |
| 点目标地图 `targetshadow` | MoveScript | 右键设移动目标；`SetTarget` 同时触发 R2b 现身、C4 放置假身 |

## 各技能真值表（按 8 棵树）

### C 身法树
| 槽 | 名称 | 原版机制 | 当前 Rust 状态 |
|----|------|----------|----------------|
| C1 | Boost 疾跑 | 5s buff：受击返一半伤害作回血 + 移速随累积回血量成长 | ✅ 完成：Boost buff + `soak_boost` 返半回血，`boost_soaked` 记录成长量 |
| C2 | Shield 护盾 | 2s 反弹：反射撞入的弹体/玩家速度（MirrorBy） | ✅ 完成：Reflect buff + 玩家碰撞镜向 + 直射弹反射 |
| C3 | Shadow 影身 | 放记号（2.5s 有效）、cd 3s；再按传回记号；**到期自动回归** | ✅ 完成：锚点 + `shadow_window` 计时；召回免冷却；**到期自动传回锚点**（原版 `BackToShadow`） |
| C4 | Fake 幻象 | 两段：先按进入待幻 → 点移动目标：本体沿方向瞬移2、原位留 2 假身 | ✅ 完成：`fake_active` 待幻 + `fake_locate`（定位即触发） |

### R 突击树
| 槽 | 名称 | 原版机制 | 当前 Rust |
|----|------|----------|-----------|
| R1 | Blink 闪烁 | 朝目标瞬移 ≤maxdist（目标过近不施法） | ✓ 吻合 |
| R1b | Blink2 二段闪 | 闪一次后 **2s 内可再免cd短闪**(maxdist=4) | ✅ 完成：`blink2_window` 窗口 + 免冷却短闪 |
| R2 | DashStrike 冲锋推 | 朝目标以 SpeedR2 冲锋，撞击→踢飞+伤害（TimeR2=距离/速度） | ✓ 吻合（`push` 模型） |
| R2b | DashSlash 冲刺斩 | 以 LDspeed=15 朝目标**无限距离冲刺+全程隐身**，**给新移动命令才解除**（原版 `IdoDSWL`，非撞墙停） | ✅ 完成：`dash_active`+隐身；新移动命令解除 |
| R3b | BlinkToWall 闪到墙 | 沿目标方向射线找最近碰撞体（玩家/障碍），落其前；无→MaxDist=6 | ✅ 完成：`raycast_first` 命中障碍/玩家；配合圆形障碍系统 |

### E 远程树
| 槽 | 名称 | 原版机制 | 当前 Rust |
|----|------|----------|-----------|
| E1 | Rock 掷石 | 落点延时爆炸 AOE（伤害+击退） | ✅ 吻合 |
| E1b | StoneShot 掷弹 | 直线滚动火球，**接触持续 DoT**(rolldamage*dt) | ✅ 改用 `Rolling` 弹体（接触 DoT） |
| E2 | StealthPush 潜行踢 | 隐身+踢击窗口（maxTimeE2） | ✅ 吻合 |
| E2b | StealthPush2 潜行踢·连推 | 同 E2，撞**障碍**后 0.3s 重新触发踢击（窗口内可反复） | ✅ `ricochet_pending`/`ricochet_kick` 重踢；配合障碍系统 |
| E3 | LineBeam 线·撒弹 | 打出一条线（ST），到终点**爆裂 8 个扇弹** | ✅ `ScatterLine`(Burst) |
| E3b | LineExplode 线·散射 | 打出一条线（SA），沿途**周期性(0.2s)散射弹**并旋转 | ✅ `ScatterLine`(Periodic) |

### D 弹幕树
| 槽 | 名称 | 原版机制 | 当前 Rust |
|----|------|----------|-----------|
| D2 | Fireball 回旋镖 | 火球**持续向施法者加速回飞**（速度矢量 + 拉拽），撞障碍反弹；撞人→爆炸伤+推 | ✅ `Boomerang` 弹体(速度矢量+拉拽)；撞障碍反弹；直接命中伤+击退 |
| D3 | Missile 追踪导弹 | 锁定**点击处最近**敌人，全速直追（velocity=dir*Speed），命中爆炸伤+推 | ✅ `needs_point`=点击处；全速直追；命中推+伤 |
| D4 | Fireball·香蕉 | **±对称曲线飞行**香蕉弹（CCWTurn），撞人爆炸伤+推 | ✅ `Banana` 弹体(角速度曲线)；命中伤+击退 |

### T 吸血/弹幕树
| 槽 | 名称 | 原版机制 | 当前 Rust |
|----|------|----------|-----------|
| T1b | TLeech 吸血链镖 | 镖命中敌人→吸血回己，并**自动跳链最近下一个**（链式吸血） | ✅ `Chain`(heal>0)：命中回血 + 跳跃链式 |
| T2 | T2Shot 扇扫连射 | 朝目标方向 **~8 发按角度微转**（每 0.1s 一发） | ✅ `SweepState` 发射器逐帧依次发射并旋转 |
| T2b | T2Volley 扇面齐射 | 同时喷 **N 发扇形**爆炸弹 | ✅ `Volley`：一次喷出 count 发（fan） |
| T3 | T3Fast 跳弹·衰减 | 高速镖，命中→**跳到最近下一个**，伤害逐跳衰减 | ✅ `Chain`(decay)：命中后跳并衰减倍率 |
| T3b | T3Fast2 跳弹·蓄力 | 命中→爆炸伤+推+生回返镖 + `damageplus`(+0.3)；**回返镖到家→刷新技能冷却**；miss→damageplus 归零 | ✅ `BonusBomb`(直线炸弹)+`Returner`(回返镖)：命中+damageplus/生成回返镖，回返镖到家刷新 cd，射程耗尽未命中→归零 |
| TestLeech | 转镖吸血 | 直射镖，折向最近敌人，命中吸血回己 | ✅ `Chain`(heal>0) 复用（当前即转链） |

### Y 控场树
| 槽 | 名称 | 原版机制 | 当前 Rust |
|----|------|----------|-----------|
| Y1 | Y1BlueLine 蓝线回拉 | 对点击处目标上**蓝线**：持续拉向施法者 + 持续掉血（maxtime） | ✅ `Tether`：锁定点击处近敌，场效应 `pull` 拉向施法者 + DoT |
| Y1b | Y1BlueLine2 红线回拉+扇伤 | 同回拉，但红线**沿路径持续射线扫射伤害**所经过的敌人 | ✅ `Tether(beam)`：额外沿施法者→目标线段扫射 |
| Y2 | Y2Delay 撞击迟缓推 | 直线弹，命中→**推离 + pushtime=2s** | ✅ `PushBullet`：命中伤 + 沿弹-目标方向 `push(power, push_time)` 强推 |
| Y2b | Y2Suite 静默束缚 | 施法者身后两点反向、**收拢成一缕线**，线扫过的人被**束缚 3s（禁施法）** | ✅ `BindLine`：固定线段束缚线上敌人（Tied buff） |
| Y3 | Y3Zone 引力场 | 打出一个飞行场，**持续把附近敌人吸向场中心**+倒计时 4s | ✅ `Gravity`：场效应向各玩家 `pull` 累加吸引力 |
| Y3b | Y3Zone2 星域持续伤 | 目标点放一颗**星**：范围内敌持续掉血、对自己回血，持续 4s | ✅ `Star`：范围内敌 DoT + 施法者回血 |

### F / G / 预留
| 槽 | 名称 | 原版机制 | 当前 Rust |
|----|------|----------|-----------|
| F Test03 | 蓄力自爆 | 吟唱 1s（singing=3）→ SelfExplode：以自身(2半径)炸，自己扣到剩1血、范围内敌-10并推开9 | ❌ 未实现 |
| G Test01 | 普通爆炸弹 | 同 BombExplode 直射（测试占位） | ❌ 未实现 |
| 预留 _SelfExplode | 同上 | — | 预留 |
| 预留 _Reserved | — | — | 预留 |

## 结论：约 6 个已实现技能需**(重)修正**，20+ 个待新增
- 现状 = 12 个"已实现"（含 4 个 C、2 个 R、4 个 E、2 个 D），但**其中 Shield/Boost/StoneShot/D2/D3/LineBeam 与原版机制不符**。
- 需要新增的独特机制：二段闪/无限隐身冲刺/闪墙、链式(吸血/跳弹/衰减)、回旋镖/香蕉曲线、扇面(齐/扫)射、线(回拉/散射/扫射)、引力场、束缚线、星持续伤区、返回弹蓄力、蓄力自爆。
- **给阶段 3 的启示**：需要补的代码逻辑基础 = 统一 Buff 系统、多段弹体（链/曲线/回旋镖）、线/场类区域、击退速度模型、束缚(禁施法)、目标点选择（点击处最近 vs 自身最近）。这些都在 `PlayerInput`/`World` 结构里，**必须先于网络协议定形**。
