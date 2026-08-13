# Rust 重写计划 — 帧同步圆球竞技场

将 7 年前的 Unity/C# 帧同步圆球竞技场 demo 重写为 **Rust + ggez** 完成品。
美术风格目标：**Cell-Graph-Risk** 式扁平几何 / 节点细胞风。

原版技术要点（需要复刻的确定性基座）：
- `FixMath`（`Fix64` / `Fix64Vector2` 定点数）—— 帧同步的确定性基石
- 帧同步：`NetWriter` 收集本帧输入（右键移动目标 / 左键施放 / 按键技能），广播给所有玩家，每帧确定性回放
- 核心玩法：圆球移动、收缩大逃杀场地、HP / 技能冷却、C/D/E/R/T/Y 技能树、子弹弹体

## 目录约定
- `game-core/` — 纯逻辑 crate（定点数、确定性模拟、玩法逻辑），不依赖 ggez
- `client/`   — ggez 应用（渲染 / 输入 / UI）

## 数学基石（已确定：采用成熟 crate）
- **定点算术**：`fixed = "=1.28.0"`（Q32.32，即 `I32F32`）。
  + 注意：本机 rustc 是 1.88，`fixed` 最新版 1.31 需 rust 1.93，故锁定 `=1.28.0`。
- **确定性三角**：`cordic = "0.1"`（nalgebra 作者 sebcrozet 所写，`fixed` 官方文档推荐），
  CORDIC 纯整数实现，跨平台确定。
- 旧手写 `fix64.rs` / `trig.rs` 已删除，统一到 `game-core/src/fix.rs`。
- `Fix64` = `I32F32`；`Vec2` 为 2D 定点向量。

## 阶段跟踪

### 阶段 2 技能系统：从“固定键位”改为“键位→技能树→选技能绑定+学习”（按原版）

原版键位（ClickCatcher）：**C / D / E / F / G / R / T / Y**（S = 停手）。每个键对应一棵技能树，
在学习阶段从该树的可选技能里选一个绑定，再花钱升级；可洗点（全额/比例退款）重选。

**技能树 → 可选技能（原版清单，作为数值/机制依据）：**
- **C 树**：C1 疾跑（自加速）、C2 护盾、C3 影身（两段传送）、C4 幻象（留假身）
- **D 树**：D2 火球、D3 追踪导弹、D4 火球（另一种）
- **E 树**：E1 掷石（aoe 爆炸）、E1b 飞弹、E2 / E2b 潜行推击、E3 / E3b 远程持续线/弹
- **F 树**：TestSkill03 等（测试类，后续归并）
- **G 树**：TestSkill01 等（测试类）
- **R 树**：R1 闪烁、R1b 二段闪、R2 冲锋推（撞击踢击）、R2b 冲刺斩、R3(TestSkill02) / R3b 闪身到墙
- **T 树**：T1b 吸血弹、T2 / T2b 连射弹、T3 / T3b 高速弹（+TestSkillLeech 吸血）
- **Y 树**：Y1 / Y1b 蓝线抽吸、Y2 / Y2b 延时/套件、Y3 / Y3b 持续掉血区

实施顺序：
1. **`game-core` 技能标识重构（已完成）**：`SkillId` 扩展到原版全部技能（34 个槽），每个技能归属一棵树（`SkillId::tree()`），
   每棵树列出可选技能（`SkillTree::skills_in_tree()`）。已实现技能保留真实效果；未实现的以 `_Unimplemented` 占位
   （可绑定/学习，但暂不落地效果）。`Caster.cooldowns` / 玩家的 `skill_levels` 扩到全槽数（34）。单测 30 全绿。
2. **键位绑定 + 档案洗点（已完成）**：`PlayerProfile` 增加 `key_slots[8]`（每个键绑定哪个技能）+ `gold_spent`；
   提供 `bind_skill`/`unbind_skill`/`respec(比例退款)`（可全额或按比例）。客户端学习阶段：按字母键选中树 →
   数字键从该树选技能绑到该键 → `=` 升级该键绑定技能 → `X` 洗点（全额）。开局默认每个键绑首技能。
   单测 32 全绿（含绑定 / 全额洗点 / 比例洗点）。
3. **学习阶段 UI**：进阶为“选技能→绑键→升级”的交互。
4. **冷却提示 UI**（技能图标冷却遮罩/倒计时）。
5. **shift 指令队列**（预排移动/施法，兼作输入缓冲）。

- [x] **阶段 0 — 环境与骨架**：workspace + `game-core` crate + `client` ggez 空窗口（已验证可运行）
- [x] **阶段 1 — 核心玩法单机 demo**：Fix64 定点数、圆球移动（右键设目标）、收缩场地、HP 与基础碰撞伤害。可玩的缩圈 demo。
  > `game-core`：`World`（确定性 step）、`Player`、`Rng`（确定性 LCG）、10 个单测全绿。
  > `client`：固定 60Hz 步进 + 右键移动玩家 + 确定性 AI 机器人 + 画绳圈/圆球/HP 条。
- [ ] **阶段 2 — 技能系统（进行中）**：
  > **已完成（框架 + 三棵树链路）**：
  > - `game-core/skill.rs`：`SkillDef`/`SkillGrowth`（等级成长）、`SkillStats`、`Caster` 施法状态机（
  >   前摇 windup / 后摇 recovery / cooldown / 打断 interrupt）、`CastError`。
  > - `game-core/world.rs`：`PlayerInput.cast`、skill 执行（Blink 瞬移、Rock 延时爆炸、Boost 加速）、
  >   `Projectile` 飞行物 + 爆炸结算（伤害+击退）。
  > - `game-core/meta.rs`：多局 meta 数据模型 —— `MatchConfig`/`PlayerProfile`/`MatchState`/
  >   `MatchPhase`；金币经济（每轮参与奖 + 击杀奖励 + 名次奖励）、技能升级记账、多局循环推进。
  > - `game-core/world.rs`：本局名次结算（`placement()`）、击杀记录（`take_kills()`）、
  >   `round_over()`/`reset_round()`。
  > - **竞技场边界机制（复刻原版 HPScript/AreaScript）**：球可出圈（不再硬钳制），出圈掉血（`mag > radius`，
  >   5/秒）+ 轻微回收拉力；缩圈加快到 0.35/s、最小半径 3，压迫感与终局推进明显。
  > - 单测 26 全绿（含 meta 经济/升级/多局 + 出界/重置集成）。
  > **已完成（通用系统地基 + C 树落地）**：
> - **运动/受力模型重构（B 阶段地基）**：`Player.control`（强制位移/击退，原版 `GetPushed`）、`Player.pull`（逐帧附加速度，原版 `VelotoAdd`）、统一 `Buff` 系统（Speed/Reflect/Stealth/Tied/Boost）、`mirror_by` 向量反射；移动统一进 `step_velocity`+`tick_buffs`+`step_area_forces`。
> - **C 树全部落地（复刻原版）**：C1 疾跑=生命偷取+移速成长（`soak_boost` 返半回血）；C2 护盾=反弹（Reflect buff，玩家碰撞 + 直射弹镜向反射）；C3 影身=锚点+窗口计时、召回免冷却+到期自动回归；C4 幻象=两段式（待幻→定位触发留 2 假身+瞬移）。
> - **障碍系统（方案 A）**：新增独立圆形障碍 `Obstacle` + `raycast_first`（射线-圆求交）+ 玩家-圆盘分离；`raycast_first` 同时命中障碍与其他玩家，用于 R3b 闪墙。客户端渲染柱子。
> - **R 树全部落地（复刻原版）**：R1b 二段闪（`blink2_window` 免冷却短闪）、R2b 冲刺斩（无限时长+隐身，**新移动命令解除**，非撞墙停）、R3b 闪到墙（射线命中障碍/玩家落其前，无则 MaxDist）。
> - **E 树全部落地（复刻原版）**：E1b 掷弹=滚动火球 `Rolling`(接触 DoT)；E2b 潜行踢·连推 `ricochet`(撞障碍重踢)；E3/E3b 撒弹线 `ScatterLine`(Burst/Periodic 扇弹)；E1/E2 已就绪。
> - **D 树全部落地（复刻原版）**：D2 回旋镖 `Boomerang`(速度矢量拉拽+撞障碍反弹，直接命中伤+击退)；D3 导弹 `Missile`(锁定点击处最近，全速直追+击退)；D4 香蕉弹 `Banana`(±对称曲线，命中伤+击退)。
> - **T 树全部落地（复刻原版）**：新增 `Chain` 弹体（吸血链镖/跳弹衰减/转镖）；T2 扇扫 `SweepState` 发射器；T2b 扇面齐射 `Volley`；**T3b 蓄力跳弹严格复刻**：`BonusBomb`(直线炸弹)+`Returner`(回返镖) —— 命中 +damageplus/生回返镖，回返镖到家刷新技能冷却(`Caster::reset_cooldown`)，射程耗尽未命中→damageplus 归零。
> - **Y 树全部落地（复刻原版）**：Y1/Y1b 回拉线 `Tether`（场效应 `pull` 拉向施法者 + DoT，Y1b 沿线段扫射）；Y2 撞击迟缓 `PushBullet`（命中伤 + 沿方向强推 push_time）；Y2b 束缚线 `BindLine`（线段束缚 Tied 禁施法）；Y3 引力场 `Gravity`（场效应吸附）；Y3b 星域 `Star`（范围敌 DoT + 施法者回血）。此时 `step_area_forces`（原先空的场效应钩子）正式接入。
> - **F/G 树落地**：F Test03 蓄力自爆 `SelfExplode`（windup 1s 后自身 AOE，自扣残血、范围内敌伤+踢开）；G Test01 普通爆炸弹 `PushShot`。
> - **至此 8 棵树全部按原版复刻完成**。
> - **竞技场缩到 0（修正）**：复刻原版 `AreaScript`，场地半径持续缩到 0（不再停在旧阈值 3.0）。`arena_shrinks_to_zero` 测试锁定。
> - **中文字体（修正）**：ggez 默认字体无 CJK 字形；已内置 `assets/fonts/cjk.ttf`（SimHei），`Game::new` 注册 `cjk` 字体，`draw_text` 用 `TextFragment::font("cjk")` 渲染中文。
> - **直射弹 Bullet（E 树掷弹 StoneShot / D 树火球 D2Fireball）**：沿施法方向直线飞出，命中最近目标造成伤害（或被反弹护盾反射）。
> - **追踪导弹 Missile（D 树 D3Missile）**：每帧朝最近敌人 `turn_toward` 转向，命中即 AOE 爆炸。
> - **客户端**：弹体/导弹/激光线/障碍渲染、护盾外圈、技能冷却 HUD（对应阶段 2 待做第 4 项）。
> - 单测 63 全绿（含护盾反弹、疾跑返半回血、幻象两段式、影身召回、障碍推出、二段闪、冲刺斩解除、闪墙、滚动火球 DoT、撒弹线爆散、连推重踢、回旋镖、香蕉弹、导弹、吸血链、跳弹、扇扫/扇射、蓄力跳弹、回拉线、撞击迟缓、束缚线、星域、自爆、爆炸弹）。
> **待做**：技能成长数值手感调优；冷却 HUD 换图标贴图（阶段 4 美术）；**`shift` 指令队列（阶段 2 待做第 5 项，全技能树已就绪后开始）**。
  > **持续性技能已实现（4/6 槽）**：`Player` 增加 `kick`/`charge`/`shadow_anchor`/`stealth` 状态；
  > - **冲锋 DashStrike**：受控朝方向高速移动 + 撞击踢击（伤害+击退）
  > - **潜行踢 StealthPush**：隐身（半透明）+ 接触踢击
  > - **幻象 Fake**：留 3 个假身假圆（Decoy Projectile，可持续 3s）
  > - **影身 Shadow**：两阶段（放锚 → 再施放传回，0.9s 冷却共用）
  > 客户端：潜行半透明渲染、假身渲染；`execute_effects` 不再占位。单测 30 全绿。
  > **客户端多局循环已完成**：`Game` 持有 `MatchState`，按 `MatchPhase` 分支；
  > Fighting 推进 World，本局结束 → 结算击杀/名次 → 进 Learning（显示金币/名次/升级菜单、学习倒计时）；
  > Learning 结束 → 同步技能等级 → `reset_round` 下一局；Finished 显示终局结算。
  > **已完成（框架 + 三棵树的链路）**：
  > - `game-core/skill.rs`：`SkillDef`/`SkillGrowth`（等级成长）、`SkillStats`、`Caster` 施法状态机（
  >   前摇 windup / 后摇 recovery / cooldown / 打断 interrupt）、`CastError`。
  > - `game-core/world.rs`：`PlayerInput.cast`、skill 执行（Blink 瞬移、Rock 延时爆炸、Boost 加速）、
  >   `Projectile` 飞行物 + 爆炸结算（伤害+击退）。
  > - `client`：按键施法（C/R/E/F/T/G + S 停步）、点目标瞄准（左键）、石头渲染。
  > - 单测 17 全绿（含端到端：掷石炸到受害者 / 闪烁瞬移）。
  > **待做**：Shadow 两阶段、Fake 幻象、DashStrike 冲锋、StealthPush 潜行踢（需持续状态/接触）；
  > 技能冷却/成长的前摇表现接入；学习阶段金币升级 UI；结算判定。
  > 设计基线（已确认）：
  > - 技能有 **前摇 windup / 后摇 recovery**，均影响结算：前摇期间被打断则施法失败
  > - **移动规则（已按 DOTA2/War3 手感定）**：前摇期间不能移动；后摇期间可移动但不能立刻再放技能；
  >   施法开始会**取消当前移动命令**（瞬移后不会继续走向旧目标）；客户端按技能键即清掉持久移动目标。
  >   （已用单测锁定：`blink_cancels_previous_movement_order`、`cannot_walk_while_casting`）
  > - 技能成长：`SkillDef 基础数值 + 等级 × 成长系数`，金币购买/升级（原版等级系统未完成，重写补齐）
  > - 升级界面在多局间的学习阶段（MainSkillMenu）
  > - 金币来源（术士之战式）：击杀 + 局内存活 + 每轮固定发放 多种方式
  > - 最终实现全部技能＋子弹；**初期先做 C(位移)+R(近战推)+E(远程线) 三棵树**跑通链路
  > - 手感层：移动加速度/减速度、技能前/后摇，用于掩饰网络延迟（阶段3配合输入缓冲+本地预测+乐观同步）

## 网络手感 / 延迟掩盖设计（阶段 3 前置）—— 实现状态核实
> 本节逐条核对代码与文档，区分「已实现」与「设计目标」，避免"文档误当作已做"。
### ✅ 已实现于 `game-core`
- **技能前摇 windup / 后摇 recovery**（`skill.rs`：`CastPhase::{Windup,Recovery}`、`try_cast`/`advance`/`interrupt`）。
  + 前摇被打断→施法失败；`step_velocity` 里前摇期间禁止自走（清目标）。已用单测锁定。
- **施法取消移动命令**（`handle_casts` 施法即 `move_target = None`；客户端按键清持久移动目标）。
- **统一受力/移动模型**（`control` 强制位移、`pull` 附加速度、`buff`），供击退/冲锋/力场类技能低成本接入。
- **移动加速度 / 减速度**：`Player::cur_vel` 自走速度积分，起步以 `ACCEL` 渐加速、松手/到达以 `DECEL` 渐减速刹停。已用单测锁定 `self_walk_accelerates_gradually` / `self_walk_decelerates_to_stop`。
### ⏳ 设计目标 / 待实现（勿误当作已做）
- **输入缓冲 / 本地预测 / 乐观同步**：属于阶段 3 帧同步的延迟掩盖，时机在阶段 3。
- **shift 指令队列**：阶段 2 待做第 5 项（预排移动/施法，兼作输入缓冲）。
- [ ] **阶段 3 — 帧同步联网**：核心逻辑确定性模拟 + UDP 广播输入 + 本机多开联调（输入缓冲 + 本地预测 + 乐观同步，不停摆等待最慢玩家）
- [ ] **阶段 4 — 表现层打磨**：Cell-Graph-Risk 美术、粒子、音效、菜单
- [ ] **阶段 5 — 完成品**：房间系统、结算、材质打磨、打包

## 已确认决策
- Rust 工程放 `rust_remake/`，不改动原 Unity 项目
- 美术先用纯色圆 + 色块 + 简单网格跑通玩法，视觉放阶段 4
- 网络先不上 Steam，用本地 UDP 做帧同步，Steam 之后再说
