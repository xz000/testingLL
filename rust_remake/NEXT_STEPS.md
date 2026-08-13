# 交接记录 / 下一步（离线续接用）

> 本文件用于在中断后快速恢复上下文。配合 `PLAN.md`（总计划 + 网络手感状态核实）与
> `SKILL_SPEC.md`（依据原版源码核对的技能全量真值表）一起看。

## 当前状态（2026-08-13，全部全绿）
- **单测 70 全绿**，`cargo build --workspace` 通过，`cargo clippy --workspace` 无警告。
- **中文显示**：内置 `assets/fonts/cjk.ttf` —— **开源的思源黑体(Noto Sans CJK SC, SIL OFL)**，子集约 941 字形 168KB。客户端 `include_bytes!` 内嵌 + `from_slice` 注册 `cjk` 字体渲染中文。已实测运行正常。
- **场地缩小到 0**：复刻原版 `AreaScript`，半径持续缩到 0（不再停阈值 3.0）。
- 技术栈：workspace = `game-core`（纯逻辑/定点，确定性）+ `client`（ggez）。
- 定点数 `fixed =1.28`，三角 `cordic`；确定性基座已就绪（后续帧同步直接用）。
- 工作区 `rust_remake/` 已纳入 git（首提交 `a8926fc`）。

## 已完成（到本时刻）
- **阶段 0/1**：骨架 + 核心单机 demo（缩圈、移动、碰撞、HP）。
- **阶段 2 的一部分**：
  - **通用系统地基**（`player.rs`）：`control` 强制位移、`pull` 附加速度、统一 `Buff` 系统、
    `cur_vel` 移动**渐加速/渐减速**、`mirror_by` 反射、`soak_boost` 生命偷取。
  - **C 树已全部复刻**：C1 疾跑(生命偷取+移速成长)、C2 反弹护盾(Reflect+镜向)、
    C3 影身(锚点+窗口+召回免冷却+到期自动回归)、C4 幻象(两段式：待幻→定位触发留2假身+瞬移)。
  - **障碍系统（方案 A）**：独立圆形障碍 `Obstacle` + `raycast_first`(射线-圆求交，同时命中障碍与其他玩家) + 玩家-圆盘分离；客户端渲染柱子。
  - **R 树已复刻**：R1b 二段闪(`blink2_window` 免冷却短闪)、R2b 冲刺斩(无限时长+隐身，
    新移动命令解除，非撞墙停)、R3b 闪到墙(射线命中障碍/玩家落其前)。R1/R2 早已就绪。
  - **E 树已复刻**：E1b 掷弹=`Rolling`滚动火球(接触 DoT)；E2b 潜行踢·连推=`ricochet`(撞障碍重踢)；
    E3/E3b 撒弹线=`ScatterLine`(Burst/Periodic 扇弹)；E1/E2 早已就绪。
  - **D 树已复刻**：D2 回旋镖=`Boomerang`(速度拉拽+撞障碍反弹)；D3 导弹=`Missile`(锁定点击处最近+击退)；
    D4 香蕉弹=`Banana`(±对称曲线+击退)。
  - **T 树已复刻**：T1b 吸血链镖=`Chain`(命中回血+跳链)；T2 扇扫=`SweepState`发射器；T2b 扇面=`Volley`(fan)；
    T3 跳弹=`Chain`(衰减)；T3b 蓄力跳弹严格复刻=`BonusBomb`(直线炸弹)+`Returner`(回返镖，
    命中+damageplus/生成回返镖→到家刷新cd/miss归零)；TestLeech 转镖=`Chain`。
  - **Y 树已复刻**：Y1/Y1b 回拉线=`Tether`(场效应 pull 拉向施法者+DoT，Y1b 扫射)；Y2 撞击迟缓=`Bullet`；
    Y2b 束缚线=`BindLine`(线段束缚 Tied)；Y3 引力场=`Gravity`(场效应吸附)；Y3b 星域=`Star`(敌 DoT+回血)。
  - **F/G 树已复刻**：F 蓄力自爆=`SelfExplode`(windup 1s AOE)；G 普通爆炸弹=`PushShot`。
  - **8 棵技能树已全部复刻完成** ✅。
  - **shift 指令队列（阶段 2 待做第 5 项）已完成** ✅：`player::Cmd { Move/Cast/Stop }` + `Player` 固定数组队列 + `PlayerInput.queued`（可被网络回放注入）。客户端 **Shift+右键/Shift+技能/S** 排/清队列。`World::step_command_queue` 于空闲时按序执行。阶段 2 待做 5 项全部完成。
- **meta 多局循环**：MatchState/金币/升级/洗点/键绑定 + 客户端学习阶段 UI + 冷却 HUD。

## 当前未完成 / 待办（按优先级）
1. **阶段 2 已全部完成**：8 棵技能树 + shift 指令队列 + 手感层（windup/recovery/加减速）。
2. **阶段 3 帧同步联网（进行中）**：
   - ✅ `game-core::netcode`：`PlayerInput`（含队列）字节编解码 + 确定性回放单测（两 World 逐位一致）。
   - ⏳ `net/` crate：UDP 帧同步（主机收集+广播、客户端收发喂 World）。
   - ⏳ client 接入 net（联网模式，本地 UDP 多开）。
   - 这些技能需要新增的通用机制（按 SKILL_SPEC「通用系统」表）：
     - 链式/跳弹（T1b/T3 吸血链、跳弹衰减）
     - 曲线/回旋镖（D2 回旋镖、D4 香蕉）
     - 扇面齐射/扫射（T2/T2b、E3/E3b）
     - 线/区域（Y1/Y1b 回拉线、Y2b 束缚、Y3 引力场、Y3b 星区）—— 引力/回拉用已有 `pull`
     - 位移（R1b 二段闪、R2b 无限隐身冲刺、R3b 闪到墙）
     - 特殊（F 蓄力自爆）
3. **stage 2 待做第 5 项：shift 指令队列**（War3 式移动+施法完整队列，兼作输入缓冲）。
4. **阶段 3：帧同步联网**（core 确定性已具备；本地 UDP + 输入缓冲 + 本地预测 + 乐观同步）。
   - 前置：确定 `PlayerInput` 最终形态（要等 shift 队列定形）。
5. **阶段 4/5**：美术（Cell-Graph-Risk）、粒子、音效、菜单、房间/结算/打包。

## 已确认的设计决策（回看用）
1. **移动加减速已补齐**（ACCEL=20 / DECEL=40，`cur_vel` 积分），手感数值（加速度/减速度斜率、
   BASE_SPEED）属「纯数值调优」，按用户判断可放阶段 3 之后。
2. **场地障碍已拍板：方案 A**：独立圆形障碍 `Obstacle`（pos+radius）+ `raycast_first`
   射线求交（同时命中障碍与其他玩家）+ 玩家-圆盘分离。R3b 因此可闪墙/闪人。已落地。
3. **SKILL_SPEC.md 是依据原版源码核对的权威机制表**，凡标 `⚠` 的已实现技能与原版机制不符，
   需要按表修正（C/R 树已修完；E/D/T/Y 仍有 `⚠`/`❌`）。

## 常用命令
```
cargo test  -p game-core                # 跑逻辑单测（49 个）
cargo build -p client                   # 编客户端（ggez 窗口）
cargo clippy --workspace                # 静态检查（应无警告）
cargo run  -p client                    # 本机跑 demo（需图形环境）
```

## 如何续接（建议顺序）
1. 先读 `PLAN.md` 的「网络手感/延迟掩盖设计」+ `SKILL_SPEC.md` 复核机制。
2. 进阶段 3：帧同步联网（本地 UDP + 确定性回放 + 输入缓冲/本地预测/乐观同步）。
3. 再做 shift 指令队列（阶段 2 待做第 5 项）。
4. 进阶段 3。
