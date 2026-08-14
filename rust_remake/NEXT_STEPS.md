# 交接记录 / 下一步（离线续接用）

> 本文件用于在中断后快速恢复上下文。配合 `PLAN.md`（总计划 + 网络手感状态核实）与
> `SKILL_SPEC.md`（依据原版源码核对的技能全量真值表）一起看。

## 当前状态（2026-08-13 晚，全部全绿）
- **单测 78 全绿**（game-core 72 + net 5 + client 1），`cargo build --workspace`、`cargo clippy --workspace` 均通过无警告。
- **中文显示**：内置 `assets/fonts/cjk.ttf` —— **开源的思源黑体(Noto Sans CJK SC, SIL OFL)**，子集约 941 字形 168KB。客户端 `include_bytes!` 内嵌 + `from_slice` 注册 `cjk` 字体渲染中文。已实测运行正常。
- **场地缩小到 0**：复刻原版 `AreaScript`，半径持续缩到 0（不再停阈值 3.0）。
- 技术栈：workspace = `game-core`（纯逻辑/定点，确定性）+ `client`（ggez）。（`net/` crate 待建）
- 定点数 `fixed =1.28`，三角 `cordic`；确定性基座已就绪。
- 工作区 `rust_remake/` 已纳入 git。**醒来续接的里程碑：git HEAD 应在 `f8b717a`（含 shift 停止修复 + 前摇UI + 瞄准线）；上一提交 `7fcf180` 记录了待议决策；再上一 `8ea8d8d` 是阶段3第一步(netcode+确定性)。**

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
1. **阶段 2 已全部完成** ✅：8 棵技能树 + shift 指令队列 + 手感层（windup/recovery/加减速）+ 冷却 HUD。
2. **阶段 3 帧同步联网（进行中，下一步入口）**：
   - ✅ `game-core::netcode`：`PlayerInput`（含队列/clear_queue/stop_move）字节编解码 + 确定性回放单测（两 World 逐位一致）。
   - ✅ **`net/` crate**：`Transport`(trait) + `StdUdpTransport` + `frame`(上行/整帧) + `session`(`HostSession`/`ClientSession` 建连握手+每帧合帧广播)。真 UDP 三端锁步单测：`lockstep_over_udp_reaches_identical_worlds`、`session_lockstep_over_udp`（两端 World 逐位一致）。
   - ✅ **client 接入 net（联网加入模式）**：`--join <host:port>` 加入 host；`client::netlink::NetLink` 每帧上行本机输入/收整帧喂 World（可无头单测 `two_client_links_stay_synced`）。联网时禁用本地 AI、按握手序号作为本机玩家。
   - ⏳ host 窗口（`--host`，host 也当玩家窗口）+ meta 多局联网 + 本地 UDP 多开手测。
   - ⏳ （后续）Steamworks 接入：用 `Steam Networking` 替换底层 Transport（实现 `Transport` trait 即可）。

## 阶段 3 网络架构选型（已确认，2026-08-13 晚）
- **模型：主机-客户端（host 同时当一个玩家）**，即“房主开房当 host + 其余 client 连入”。
  - 与 Steamworks 常见形态（房间：房主=host，玩家加入）无缝衔接。
  - 便于以后过渡到专用服务器 / Steam 中继。
- **人数：设计上限 8 人**（`MatchConfig` 可配；本地测试默认 2~4）。大逃杀收缩圈向，8 人合适。
- **`net/` 层做「传输无关」抽象**：定义 `trait Transport { send/recv }`，本地用 `StdUdpTransport`，
  将来接 Steamworks 时用 `SteamTransport` 替换，网络逻辑（帧同步/编解码）不动。
- 帧同步：host 每 tick 收齐各 client 输入（`netcode` 编解码）→ 广播整合包 → 各端喂本地 `World`。
- 验证：`net/` 层写“单进程内 host + 两 client（不同端口）”的本机 UDP 单测，验证收发与回放一致；
  两个 ggez 窗口对战仍需有图形环境手动跑 `cargo run -p client` 验证。

## 待议 / 搁置决策（详见 PLAN.md「待议 / 搁置决策」）
- 升级流程 UI 是否改（数字=绑定 vs `=`=升级）。
- shift 施法瞄准线起点（当前位置 vs 最终移动位）。
- **击退模型**：当前定速 push(`GetPushed`)，可改 `Impulse` 初速度+减速；**不影响网络协议，阶段 3 后可做**。

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
