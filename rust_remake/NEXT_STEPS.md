# 交接记录 / 下一步（离线续接用）

> 本文件用于在中断后快速恢复上下文。配合 `PLAN.md`（总计划 + 网络手感状态核实）与
> `SKILL_SPEC.md`（依据原版源码核对的技能全量真值表）一起看。
> **📋 帧同步基座稳固计划见 `LOCKSTEP_FOUNDATION.md`**（2026-08-15 新增：把「⚠ 修复」后续收口成可执行步骤）。
> **🧪 测试防假绿约定已写入 `PLAN.md`「🧪 测试约定（防假绿）」**；新增联网测试须对齐（合帧收到 / 真实推进 / 输入生效 / 有界循环）。
> **🛠 本地无人值守回归（2026-08-15 已建成并验证）：`check.ps1` + `.githooks/pre-commit` + `install-hooks.ps1`**
>    - `check.ps1`：一键 build + test(80) + clippy(-D warnings)，失败即非零退出。
>    - `pre-commit`：每次 `git commit` 前自动跑 check.ps1，坏代码提交不出去；`SKIP_HOOKS=1` 可临时跳过。用 `powershell -File install-hooks.ps1` 安装。
>    - ⚠ 大坑（务必记住）：git 仓库根在**上级 `testingLL/`**，`rust_remake/` 只是其子目录。故 `core.hooksPath` 必须用**绝对路径**（指向 `.../rust_remake/.githooks`），且钩子用 `$0` 定位项目根（不能靠 `git rev-parse --show-toplevel`，它会返回 testingLL/）。已端到端验证：真编译错误能拦提交，干净提交放行。
> **🧨 2026-08-15 完整锁步修复（三窗口不同步）见下节「⚠ 2026-08-15 完整锁步修复」**
> **🏁 网络层已按「正确 lockstep」完整重写并收尾：见 `NET_REWRITE.md`（proto+handshake+lockstep 三层，删旧 session.rs；真机 4 窗口验证通过）。**
> **🗺 三大功能规划见 `ROADMAP.md`（单机技能试验场 / 局域网对战 / Steamworks 对战）。技术专项：`LOCKSTEP_FOUNDATION`·`NET_REWRITE`·`LATENCY_MASKING`·`ATTRIBUTE_SYSTEM`。**
> **⬚ 数值收敛层已完成：`game-core::balance::Balance` 统一起手感/场地数值权威源（4.3），详见 `LOCKSTEP_FOUNDATION.md`。**
> **📊 测试：当前 83 全绿（client 1 + game-core 73 + net 9），`cargo test --workspace` / `cargo clippy --workspace -- -D warnings` 均绿。**

## 当前状态（2026-08-13 晚，全部全绿）
> **2026-08-15 重要修复（网络层帧同步 tag 丢失 bug）见下节「⚠ 2026-08-15 修复」**
- **单测 80 全绿**（game-core 72 + net 6 + client 2），`cargo build --workspace`、`cargo clippy --workspace` 均通过无警告。
- **中文显示**：内置 `assets/fonts/cjk.ttf` —— **开源的思源黑体(Noto Sans CJK SC, SIL OFL)**，子集约 941 字形 168KB。客户端 `include_bytes!` 内嵌 + `from_slice` 注册 `cjk` 字体渲染中文。已实测运行正常。
- **场地缩小到 0**：复刻原版 `AreaScript`，半径持续缩到 0（不再停阈值 3.0）。
- 技术栈：workspace = `game-core`（纯逻辑/定点，确定性）+ `client`（ggez）+ `net`（传输无关，本地 UDP / 日后 Steam）。
- 定点数 `fixed =1.28`，三角 `cordic`；确定性基座已就绪。
- 工作区 `rust_remake/` 已纳入 git。**醒来续接的里程碑：git HEAD 应在 `f8b717a`（含 shift 停止修复 + 前摇UI + 瞄准线）；上一提交 `7fcf180` 记录了待议决策；再上一 `8ea8d8d` 是阶段3第一步(netcode+确定性)。**

## ⚠ 2026-08-15 完整锁步修复（三窗口不同步）

**现象**：本地多开 `--host` + `--join` 后，三窗口虽各自操作不同角色，但运行状态不同、加载完成时间不同、画面不同步。

**根因（帧同步正确性的三个硬伤）**：
1. **host 不等齐 N 端就推帧广播**：旧 `collect_inputs` “收到多少返回多少”，某个 client 输入未到就用缺人帧推进并广播 → 各端收到不同内容的帧 → 状态分叉。
2. **client 没收到帧也盲扣时间**：旧客户端 `while accumulator>=TICK` 里 `step_tick` 返回 false 仍 `-1=tick`，导致 client 帧数落后 host → 永久漂移。
3. **无统一起始**：host 一满足 `joined>=expected` 就开推，各端开始时刻不同（加载时间不同）→ 时间轴对不上。

**修复（协议 + 两侧闭环）**：
- **帧带 seq**：`net/src/frame.rs` 的 `frame_packet(seq, entries)` / `parse_frame -> (seq, entries)`。
- **READY/GO 统一起始**：`net/src/session.rs` 新增 `TAG_READY=5` / `TAG_GO=6`；client 握手后 `send_ready`，host `poll_ready`+`all_ready` 后 `broadcast_go` 带起始 seq，client `recv_go` 拿到起点。
- **host 等齐门槛**：`collect_inputs` 只有收齐全部 client 输入才返回 `Some((seq, frame))`；未齐返回 `None`（不推帧不广播）。
- **client 帧锚定推进**：`netlink::step_frame` 收到带 seq 帧才推进并返回 `Some(seq)`；`None` 表示未到，client 端不扣时间、不推进（与 host 帧对齐）。
- **collect_inputs 追加去重**：同一 client 序号保留最新一份输入，防 UDP 重复/乱序重复入帧。
- client/main.rs 联网段同步改造（host 收齐才推 + READY/GO；client 以 seq 锚定、没收到帧不扣时间）。

**验证**：`cargo test --workspace` 81 全绿（含重写后的 `session_lockstep_over_udp`、`host_participates_as_player_zero`、`full_online_match_identical_worlds`、`lockstep_8_player_max_capacity_smoke` 都走 READY/GO + seq）；`cargo clippy --workspace -- -D warnings` 无警告。**仍需你在多开里真机手测确认不再不同步**（用 `multi-launch.ps1 -Players 3`）。

## ⚠ 2026-08-15 修复：网络层帧同步 tag 丢失 + 测试假通过（重点备份）

**现象**：工作区新增的 `client::netlink::tests::full_round_host_and_client_consistent_with_winner` 跑 `cargo test` 无限卡死（数小时无结果）。

**根因（两个同类 bug）`net/src/session.rs`**：`up_packet` 与 `frame_packet` 开头都执行 `out.clear()`；而 `ClientSession::send_input` 与 `HostSession::broadcast_frame` 是先 `push(TAG_INPUT/TAG_FRAME)` 再调用它们 —— 结果 `clear()` 把刚 push 的 tag 首字节抹掉：
- `send_input` 实际发出 `[my_index][payload]`（首字节=玩家序号，而非 TAG_INPUT=3）。
- `broadcast_frame` 实际发出 `[count高字节][...]`（首字节=帧计数而非 TAG_FRAME=4）。

于是对端 `collect_inputs`(按 `rcv[0]==TAG_INPUT`) 与 `recv_frame`(按 `rcv[0]==TAG_FRAME`) 永远匹配不上 → **client 输入在网络上被静默丢弃**。新增的完整对局测试因 host 永远收不到 client 输入而卡在内层 `loop`。

**为何旧 net 测试“全绿”其实在假通过**：`session_lockstep_over_udp` / `host_participates_as_player_zero` / `full_online_match_identical_worlds` / `two_client_links_stay_synced` 在收不到帧时 `if let` 直接跳过 `world.step`，两 World 都停在初始默认状态，`assert_eq!(wa.players, wb.players)` 恒成立 —— **网络其实一行真实输入都没传，测试却“通过”**。这是本次排查最重要的教训：帧同步不变量测试必须证明“确实推进过、输入确实生效”，否则会在协议悄悄坏掉时假绿。

**修复**：`send_input`/`broadcast_frame` 先单独产 body（`up_packet`/`frame_packet` 写入独立 `body`），再把 tag 前缀拼上再发。

**加固**：四类 session 联网测试均加 `collected.len() >= N`、`stepped > 0`、`world != 初始World` 断言 —— 网络若再静默丢输入会立即炸红而非假绿。新增的完整对局测试改为**有界重试循环**（`0..2000` 次 + `expect` 超时 panic），永不再无限 hang。

**验证**：`cargo test --workspace` 80 全绿（含上述非假通过断言）、`cargo clippy --workspace` 无警告。

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
   - ✅ **`--host` 窗口**：`--host <port> [--players N]` 开房作 host（自身=player0），接受 client 加入。`net::HostSession` 支持 `set_local_input`/`host_participates`；自动测试 `net::host_participates_as_player_zero` 验证 host+clients 三端一致。本地多开 `--host` + `--join` 可对战。
   - ⏳ meta 多局联网（当前联网用单 World，多局循环的 meta 状态未跨网络联动）。
   - ⏳ 本地 UDP 多开手测手感。
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
