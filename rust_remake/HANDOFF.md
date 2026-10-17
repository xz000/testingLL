# 交接说明（HANDOFF）—— 新会话从这里开始

> 用途：本仓库是 **Warlock 098c 的 Rust 复刻**（原为 098b 移植，正逐步迁移到 098c）。
> 新会话**不继承上一会话上下文**，这里固定记录：**文档地图、真值来源、测试基线、协议号、命令与坑**。
> 开场只需说「**按 HANDOFF 继续**」，并说明要做 Steam 主线还是技能对齐。

---

## 0. 一句话现状（2026-10-17）

- **技能对齐**：
  - 前段：逐技能「098c 代码」基准审计（`SKILL_CODE_AUDIT_098c.md`；S015A/S017A/S017B/S018B/S014B/S007/S010/S020/S021/S008A + 火法杖窗口 + 治疗 `PX→DX`）。
  - **本轮（2026-10-11）再完成一轮 JASS/描述对齐**（台账 §14–§18）：F5（S003 命中分支）、F6（沉默 `AC` 逐技能槽）、熔岩靴 `kC`、**F 槽不可购买**、S005 `Ev` 反射类别、S008A 不被暗物质拉、沉默 UI（逐槽置灰/紫环）、**全部技能描述逐条对代码订正并简练化**。**协议 48→54**。
  - **二轮深挖（§19–§22）**：§19 弹体回调集全技能复核（`hv/jv/Gv/Jv` 全对位）；§20 S016A 弹跳弹（差异1/2/3）；**§21 第二轮逐回调语义深挖并修复**——S000/S002 `xi` AoE 排除直击者、S009 `gB`（命中/撞柱/撞弹体）、S013A 落点传送、S010/S012 招架（+100 移速/`jn`）、S004/S003 `xi` AoE、6 处注释订正；**§22 近似项复核**（S005/S006/S007/S018B/S020 —— S007 精确、S018B 效果等价、余为功能等价）。
  - **审计已无已知未闭合项**（§14.5 全结案 + §22 近似复核无真缺口）。
- **联机（Steam）**：R / M / F / E / H / L 系列全部收口（掉线/迁移 UI、崩溃重开归队、被取代 host 归队、联机暂停、有界追赶、主动离开+三因归队、学习期联机与主机迁移）。**E1/E2 后 SDK 断言 14→2**（`interfacemap` 0；剩余 2 条在迁移瞬间/进程退出）。旧设计文档见 `archive/EXIT_MENU_RECONNECT_PLAN.md`，当前迁移细节见 `HOST_MIGRATION_LEARNING_PLAN.md`。
- **协议号 `PROTOCOL_VERSION = 69`**（`game-core/src/lib.rs`；变更史见其上方注释）。v56 = §21/§22 行为对齐；**v57 = 快照补全模拟状态**（`World` 房间配置标量 + `Player` 11 状态字段纳入 `world_ser`，修重连后 DESYNC，`a223c5a`）；**v58 = 归队索引修正**（`Packet::Snapshot` 增 `your_index`，host 重连应答下发该端当前世界索引，修迁移后“控制错角色 + host 等不到输入而卡死”，`0648241`）；**v59 = 点目标施法范围核准**（S008 陨石 `max_distance=1200` 并在权威模拟 clamp；S009/S014 按 `700*(1+.1*时间精通)`；见台账 §23）；**v60 = 位移技能与击退对齐**（S012A 冲撞替换旧击退；S013 换位/搬运不再清空 `kb_vel`；见台账 §24）；**v61 = S013 Stop 语义**（`IssueImmediateOrder` 只清移动指令，自定义 Q/S/U/w 继续 ⇒ 换位/搬运不再清 `control`）；**v62 = S012A 冲撞时长按点击距离**（`ev=Rr/Gr*.03+3*.03`，不再固定跑满 max_distance）；**v63 = S012A 的 U/w 是衰减补偿**（098c `Q*=.96` 后 `+52/s`，净 1300/s；我们的 constant-control 不再额外加 52，只清旧击退）；**v64 = S012A 去掉 +3 tick 尾部**（用户选 B：时长 = min(点击距离,最大距离)/1300，冲撞停在点击点附近；有意偏离 098c 手调余量）；**v65 = S012A 正常结束保留 20% 余速**（098c `CR`：Q=.2Q，地面/冰面都有；冰面按 0.98/tick 长滑；撞人/撞墙仍急停）；**v66 = 时间精通范围统一**（S012/S013 的 `(1+.1×时间精通)` 同时进入指示线与 `cast_range_limit`；S016 指示线也乘；修高精通时实际距离被截短）；**v67 = S012B 凤凰重写**（新增 `PlayerInput.phoenix_order` 点地边沿；固定 `3.1×jn`；每次新点地追加 `18×0.5^(v/20)`；B 无距离 clamp、结束保留速度、移除直接接触伤害）；**v68 = Phoenix 效果优先**（删除专用 Q/S 摩擦分支；Phoenix 只负责转向窗口+bO 冲量，速度/位置复用现有普通地面/冰面移动）；**v69 = 冷却按技能等级/形态**（`begin_cooldown` 取 `stats_at(level).cooldown`，修“菜单冷却变短但对局不变”）。
- **A2 移动模型已收口（2026-10-10）**：冰面阻尼时间化 `0.98^(5/9)`；**击退并入走位**（`Player.kb_vel` 与自走并存相加，可边走边滑；协议 49）；仅普通加速时间**有意保留**（对帧同步手感更好）。见台账 §12/§13。
- **关键教训**：本仓库基线**既非 098b 也非 098c**，而是「098b 事实 + 098c tooltip + 占位」的混合物；
  **098c 的 tooltip 也常与 098c 代码不符** ⇒ **唯一真值是 098c 代码**。

## 1. 文档地图（按权威性）

> 全量文档（40+）的角色/权威性见 **`DOCS_INDEX.md`**；下表只列核心。

| 文件 | 权威性 | 读什么 |
|---|---|---|
| `SKILL_ALIGNMENT_LEDGER.md` | **主台账（活）** | 逐技能 §3/§9 事实表 + 状态列（✅/🔧/⚠/📋）；§11 基准审计；§12/§13 A2 移动；**§14 JASS↔Rust 全量对齐矩阵** |
| `SKILL_CODE_AUDIT_098c.md` | **权威（代码基准）** | 逐技能按 **098c 代码** 抽出的 damage/duration/cooldown/range/speed 公式，与 def 对照 |
| `JASS_FIELDS_098c.md` | 权威（字段语义） | 两字母字段/回调语义（`Q/S`=每 tick、`mI` 的 `lI`=击退%、`yO/oR` DoT、`wR` 废参、`Jv`≠`Gv`、`kv` vs `Kv`、两种“镜面反射”） |
| `098C_DIFF.md` | 系统级差异 | 098b↔098c 大系统差异（魔法张力/移动/物理/成长/计分/回血/岩浆）。**A1–A8 全部结案**；**A2 移动模型已收口**（详台账 §12/§13）|
| `ITEM_AUDIT_098c.md` / `ECONOMY_RECHECK.md` | 权威 | 道具 24 格 / 经济奖励 |
| `SKILL_ALIGNMENT_LEDGER.md` §14 | 权威 | **击退系数逐技能 + JASS↔Rust 全量对齐矩阵**（原 `archive/KNOCKBACK_ALIGNMENT.md` 已并入并归档）|
| `archive/EXIT_MENU_RECONNECT_PLAN.md` | 历史（联机 R 系列） | Esc 退出菜单 / 重连 / 迁移 / 暂停；P7 残余见 `HOST_MIGRATION_LEARNING_PLAN.md` §17.10 |
| `archive/CODE_PHASE_RECORD.md` | 记录 | 代码阶段改动汇总（commit hash） |
| ⚠ `archive/SKILL_PER_LEVEL_AUDIT.md` | **已被取代** | 逐档表**按 tooltip** 建立，已多处与 098c 代码不符 → 以 `SKILL_CODE_AUDIT_098c.md` 为准 |
| `archive/*.md`（`HANDOVER`/`P0_*`/`FIX_ROADMAP`/`FRAME_SYNC_*`/`PORT_098B_DECISIONS` …） | 历史 | 已移入 `archive/` 并带历史横幅；可查但**勿当真值** |

## 2. 真值来源（**代码是真值**，tooltip 只作交叉验证）

- JASS 反编译：`../098c/out/war3map.j`（337KB **raw**，**不要**用 `war3map_pretty.j`——它会把 `elseif` 拆坏）。
- ⚠ **行号口径**：raw 用 `CR` 分行，`findstr`/多数编辑器会数错行；历史文档行号来自旧 `Select-String`，**与 raw 不可比**。
  用 **`tools/jass_find.py <regex>`**（按 `[\r\n]+` 切分）检索；引用一律以**函数名**为准。
- 技能 tooltip：`../098c/out/w3a_strings.txt`；原始数据：`../098c/out/w3a_parsed.json`。
- 098b（**仅用于对照历史，不作真值**）：`../ce/ce_old98b/war3map.j.dec`、`../ce/ce_extract/*`。
- ⚠ 已知 **098c tooltip ≠ 098c 代码** 的例子（一律取**代码**）：
  - S018B 力场 DPS：tooltip `2.25…8.00`“非线性” vs 代码 `1+1.25L`（线性）。
  - S017A 缠绕时长：tooltip `4.5…6.25` vs 代码 `4+0.25·abilityLevel`=`4.25…6.0`。
  - S014B 削弱伤害：tooltip `5→11` vs 代码 `4+L`=`5…12`。
  - S000 火法杖 DoT：tooltip 3.0 vs 代码 3.5；S016 跳衰减：tooltip 20% vs 代码 `×0.75`。

## 3. 基线（改动前先确认，改动后必须一致或增长）

- 测试：**client 128 / game-core 353 / net 62 / net-steam 9**（默认）；带 `client/steam,client/gui` 的变体 **client 141**（见 `check.ps1`）。
- `PROTOCOL_VERSION = 69`——**动了会进快照的字段 / 改变世界演化的数值逻辑都必须 +1**，并同步 `world_ser.rs` 的 ser + de + 往返测试。（运行期包如 `Packet::Leave`/`ParticipantsAll` **不进 World ⇒ 不升协议**。）
- 命令：`powershell -NoProfile -ExecutionPolicy Bypass -File check.ps1`（= build + test + clippy `-D warnings`，含 steam/gui 变体）。
- 提交：默认走 **pre-commit 钩子**（跑完整回归）；**纯文档提交**可 `git commit --no-verify`，但仍需先跑一次 `check.ps1` 确认全绿。

## 4. 坑（血泪）

1. **改代码用 `edit` 工具**；**不要**用 `powershell Set-Content` 写 `.rs`（会把 UTF-8 中文弄成 ANSI）。
2. clippy 的 client 调用固定为 `cargo clippy -p client --features client/steam,client/gui`（**不要** `--all-targets`）。
3. JASS **标识符大小写敏感**（`z`/`Z`、`rv`/`Rv`、`kv`/`Kv`、`DX`/`dX`、`SI`/`sI` 都是不同变量/函数）；`Kv` 遍历**只走术士子链**。
4. 两个“镜面反射”别混：
   - `fix::bounce_off(v,n,xv)` = `v − (1+xv)(v·n)n` → **098c 的柱/墙/盾反弹**（法向取反、切向保留）。
   - `fix::mirror_by(v,n)` = `2(v·n)n − v` → 只给 **D2 原型护盾**用；**正面撞面时 v 不变**（曾因此让火球/穿柱）。
5. 098c 的 `mI(nr,Vr,HX,lI)`：`lI` = 击退系数（tooltip 的「xx% 击退」），`HX` 才是伤害。
6. 技能是“**一个能力多档**”：如 S000 共 **24 档**，**13–24 = 带火焰法杖版**（物品 13 靠 `SetUnitAbilityLevel('S000',+12)` 切档）。
7. `war3map.j` **raw** 才是控制流真值；`war3map_pretty.j` 的 `elseif` 被拆坏（已实证误读 `aR`）。
8. 改「会进快照的字段」→ 必须升协议 + 改 `world_ser.rs` ser/de + 往返测试；`BuffKind`/`W098bOnHit` 是**按 tag 显式编码**的（加变体要新 tag + 升协议）。

## 5. 下一步候选（按需要挑）

1. **真机验收（优先）**：协议 **58**；**重点**：S014 回血球回飞 / S016A 纯弹道 / `kb_vel` 击退可走 / 沉默逐槽 / `kC` 岩浆盾 / F 槽不可购，**以及本轮 §21 改的 S000/S002（`xi` AoE 不含直击者）、S003（`xi` 销毁 AoE）、S004（`xi` 命中 AoE）、S009（命中/撞柱出碎片）、S013A（到点传送）、S010/S012（招架 +100 移速）**。**两端都需更新**。
2. **R7e 学习期暂停**：✅ **已完成**（`7478cf7`）——学习期接入暂停协议（host 仲裁 `request_pause/tick_pause`+广播 `PauseState`；client `send_pause_req`），暂停时**冻结学习倒计时**（`steam_learning_paused()` 含 `steam_paused_now()`）并**屏蔽学习期输入/购买**；学习期也可按暂停键。**已随 2026-10-17 真机轮验收**。
2b. **归队 `join_lobby` 超时重试**：✅ 已实现（`e6bfae1`）——`start_join_by_id` 单次尝试超时（`ErrorKind::TimedOut`）**自动重试**（默认 5s × 最多 3 次；`STEAM_LOBBY_JOIN_BEATS/ATTEMPTS`），超时**不再**把上一局标 `ended`（保留会话，提示“连接超时，可按 Y 重试”）；并加 `[steam-pause]` 诊断日志。起因：旧 host 退出→其余迁移后发起暂停→旧 host 按 Y 归队时 `join_lobby` 4s 超时被误判“该局已结束”（**与暂停无关：`join_lobby` 在所有阶段/所有机器都相同**）。**已随 2026-10-17 真机轮验收**。
3. 其它 Steam 主线（回合/胜负/计分流程、UI/表现、实机行为验证）。
3b. **心跳/判活机制（已统一）**：学习期与对局期**共用**同一条上行 `send_room_state`、同一个 `steam_cli_stale_ticks` 计数器与 `CLIENT_STALE_TICKS` / `HOST_DROP_TICKS` 阈值、以及“只信 host”的拓扑假设；差异只有**下行心跳载体**（学习期=在线集广播，对局期=权威帧），这是“学习期没有 tick”的必然结果，不是不一致。已做的对齐（`9a2cc48`）：①判活改**墙钟时间**为主（`STEAM_HOST_SILENT_SECS=3.0`，旧帧数计数作兼容兼底），不再受窗口失焦/卡帧导致的 update 频率漂移影响；②学习期 host 的在线集广播由“**每帧**”（~60 个可靠包/s）降为“**变化时 + 每 2s 兜底**”（`STEAM_ONLINE_BCAST_SECS`），后者兼作下行心跳；③两端保留相同的暂停/离线/迁移豁免（学习期统一用 `steam_learning_paused()`）。仍为帧数计（未改）：host 侧 `auto_drop_idle(HOST_DROP_TICKS)`（对局期按 sim tick 计，稳健；学习期按 update 计，低频时会拉长）。
4. **剩余已知边缘**（不修，仅记录，见台账 §22.2）：S006 回溯 HP 为无条件还原（098c `max`）；S006 未还原 `ev`/面向；S020 的物品分支（`HC`/`LC`/`Zr`）近似。098c 的 `un`（热身）门控未建模。
4b. **迁移抖振治理 + 换主机系统暂停**（`38a0304`/`cdec03d`/`7523349`/`172f38e`/`eb4041b`，**2026-10-17 真机轮验收通过**）：
   - **C1** 迁移完成统一复位判活（原先只清帧数计数、墙钟计时器停在 `0.0` → 日志里 `NO frames for n/a`）；
   - **C2'** **换主机系统暂停**：新宿主接管时（广播 `Takeover` 之后、第一帧之前）arm “系统暂停”（哨兵 `paused && by == 0`）——**不扣任何人额度、恢复不设冷却、幂等**；`PauseState` 带 `present/total`（运行期包，不升协议）→ 覆盖层显示「房主已更换，本局已暂停」+「等待玩家连接 x/y」+ **软门禁**（未就绪按恢复键先二次确认），恢复沿用“任何人可请求 + 3s 倒计时”。**它同时替掉了原计划的“8s 热身”**（暂停状态下客户端天然不判 host 静默）。
   - **C2'-b** `pump_learning` 的 `PauseState` 补 `from == self.host`（与对局期一致）；
   - **C3** 学习倒数对齐加**同轮 + 非配置同步**门控（修 `resync 30.0->1.6` 紧接 `1.5->29.8` 的抖跳）；顺手新增 `world_ser::snapshot_meta_from_bytes()`（只解 meta）——`pump_learning` 与 `snapshot_phase_is_learning()`（迁移期**每帧**）不再为一点 meta 反序列化整个 World；
   - **C4** `[jit] … sim gap` 带 `reason=paused|migrating|cfg-sync|rejoin|plain`；`AUTO-DROP` 日志补 `steamid`；**C5** 看门狗连续判定 2→3 次。
   - 详见 `HOST_MIGRATION_LEARNING_PLAN.md` **§16**（含真机证据、不变量、验收要点）。
4c. **F 系列：“迁移期被误掉线 / 无法操作”修复**（`b7215d2`/`e9665c0`/`ca29028`，**2026-10-17 真机轮验收通过**）——真机（学习期接管出的新宿主在对局期退出）现象：一台端“无法控制角色、但能看到别人动、且被标已离开”。三个根因与修法：
   - **F3'** 早 retarget（采纳 Steam owner 的瞬间就 `retarget_host(X)`，“谁是房主”只认 Steam；`Takeover`/`Snapshot` 只当“基线+状态交接”）+ 阶段 B **只认来源==X 的交接包**（修“采纳了缓存里旧宿主的 Takeover(seq=0) → 指回死宿主、绕 5.5s”）+ 定向 `ReconnectReq`→定向 `Snapshot` 收尾（不依赖广播）；
   - **F2** `RoomState` 即恢复（修“被 drop 后不靠 `ReconnectReq` 永不恢复”：“已离开”标记随之自动撤销）；
   - **F1** 系统暂停期间不判空闲（内置在 `auto_drop_idle`；修“接管后边等边把正在迁移的端踢掉”）。
   - 详见 `HOST_MIGRATION_LEARNING_PLAN.md` **§17**（含日志链、不变量、验收要点）。
4d. **已知残余风险（记录，暂不修）**：
   - **陈旧 owner 自接管**：原 P7（client 自选 host）已由 M3/R3'+F3'-c 结构性消除；但 `lobby_owner` 是 Steam 本地缓存读，迁移期无 `RequestLobbyData` 刷新，`steam_do_takeover` 前**不再复核 owner** → “曾经是 owner 且缓存未刷新”的端理论上仍会自接管（低频、**至今无真机证据**）。完整分析/代码位置/成立条件/将来若修的 P7-lite（只做发送端确认，**不做接收端 allowlist**）见 `HOST_MIGRATION_LEARNING_PLAN.md` **§17.10**。
   - **SDK 断言**：E1+E2 后断言 14→2（`!IsLocked` 仅剩“迁移瞬间 1 条 + 进程退出 1 条”）；`interfacemap` 0；已接受为不可完全避免的 SDK 噪声（§17.9）。
5. **已结案/归档计划（摘要；原文在 `archive/`）**：
   - **`archive/LEAVE_REJOIN_PLAN.md`**：主动离开（房主/玩家）**立即**生效 + **三因（主动/掉线/崩溃）都能归队**。**L1–L5 已完成**（`4980321`/`4019e5c`/`3a8f386`/`9888ba6`/`3e52f2d`）：会话时长设置项(入云)、`reset_to_main_menu(keep_rejoin)`、`Packet::Leave`+统一在线集、"已离开"标记、接线+文案。**已真机验证**（3 账号：对战期/学习期的离开→归队、房主迁移、控制角色/标记一致、无 DESYNC；清单见该文档 §4.1）。协议 **58**（`Leave` 为运行期包；57=快照补全、58=归队索引，修归队 DESYNC/卡死）。
   - **`archive/DISPLAY_CAMERA_PLAN.md`**：分辨率（自适应+全屏跟随）/全屏（Desktop 默认、True 附加）/边缘滚屏（玩家开关）/相机平移速度；**S1–S4 已完成**（`d0a6675`/`55e2919`/`7c35928`/`f7a10d2`），client 125 单测全绿；手感微调归入当前打磨计划。
   - **`archive/LEARNING_PHASE_NET_PLAN.md`**：学习期联机。**E1–E3 已完成并真机验证**（心跳+静态快照 / 按 `meta.phase` 恢复 / 掉线·离开=恒桩子·可归队·不卡局）；**学习期房主退出/掉线已改走主机迁移**（`4082ae2`）；**E5（R7e 学习期暂停）已完成**（`7478cf7`）。真机暴露并修复的一系列问题见该文档 **§7「实测发现的 bug 与修复」**（房间槽位重算、学习界面空白、配置同步抹 cfg、`steam_participants` 兜底、标记不一致、学习期丢购买等，均含 commit）。
   - **`archive/REJOIN_DETECT_PLAN.md`**：**“连回上一局”检测**专项。**H1–H4 已完成并真机验收**：本地**历史记录**(退房/回菜单不再丢)、提示含轮次/人数/房名、判据改为“**曾进入对局**”、忽略键 **Delete**、**探活=直接 `RequestLobbyData` + `LobbyDataUpdate.success` + matchkey 复核**（满员/不可加入也能查；修复“应答被 `member==0` 过滤丢掉”）。期间还修：归队后画面卡死/无头像、自动恢复“无法操作”（采纳广播快照致 index 未纠正）。
   - **`HOST_MIGRATION_LEARNING_PLAN.md`**：学习期房主退出/**主机迁移**专项。**M0–M5 全部完成并真机验收**（3 账号“房主退出→重开→连回”）：撤自选举回归 `5b4ec99`；M1 诊断 + M2 全量名单权威下发 `748f626`；M3(D1=10s 等 owner 移交、取消共识) + M4 断言 `e6ff182`；M5 修 `host_left` 跨迁移残留 `91c83f7`。验收：`Phase-B STALL`/`cannot determine`/`DESYNC`/`M4`/`panic` 均 0；❌ 不做共识（唯一权威 = Steam `lobby_owner`）。**后续 S1–S6 也已收尾**（见其 **§14/§15**）：S1/S2 撤守卫+等移交 120s、S3 `SteamServersConnected/Disconnected`、S4 学习期自动冻结、S5 重连 `RequestLobbyData` 刷新大厅、S6 归队重建头像名单；并修“`steam_net_ticks` 双递增致对局中头像不刷新”的回归与“归队后 ClientWait 冻结”。
6. **当前主线：打磨（2026-10-17 拍板）**：
   - **顺序**：A（文档收口 + `tools/soak-report.ps1`，✅ `36b231b`/`d361510`）→ **P5**（缩圈/出界预警，✅ `1a68e9e`）→ **训练场 P1-lite**（统计 HUD + `F5` 重置 + 岩浆/缩圈/障碍开关，✅ `1c1690d`）→ **Kill feed**（✅ 同 commit）；
   - **已全部完成**（2026-10-17 打磨批次）；后续按真机反馈再排。
   - **追加修复**：Ludicrous Kill 判定改用实时伤害矩阵 `world.round_damage_of(victim)<=0`（原用 `meta.damage_this_round`，对局中恒为 0 → 几乎每次击杀都触发，`8d606c5`）；`fd6a481` 目标点清除改为位移线段穿越；`bbb1d28` 位移技能与击退对齐（协议 60，见台账 §24）；`338e9bc` S013 Stop 只清移动指令、保留 `control`（协议 61）；`ff89904` S012A 冲撞时长按点击距离（协议 62）；`299b9f6` 冲撞不再额外加 52/s（协议 63）；`ddb71da` 去掉 +3 tick 尾部（协议 64，用户选 B）；`22dac12` 冲撞正常结束保留 20% 余速（协议 65）；`ade52bc` 时间精通范围统一（协议 66，F1/F2）；`16ef5bb` 普通投射物指示线补时间精通；`651b208` S012B 凤凰重写（协议 67）；`1469184` 凤凰速度按 Q/S 每 tick 衰减（手感修正）；`61f0ea2` Phoenix 改 cur_vel/普通移动分支（方案 C，恢复 Y/z）；`cf09a08` HUD 增加「凤」状态图标；`4eb332f` Phoenix 效果优先（协议 68，删除专用摩擦分支）；`0374bc5` 冷却按技能等级/形态（协议 69）。
   - **音频**：创意工坊/本地包通道已完整，缺真实素材；素材到位后优先**替换 `client/assets/audio/*.wav`**（正式版与 demo 都生效），工坊包留给玩家自定义；
   - **暂缓**：P7-lite、多帧打包、观战/回放、AI 反击、En4/Scourge 模式。

## 6. 每步的完成标准

- 纯函数优先 + 单测覆盖该行为（项目风格）。
- `check.ps1` 全绿；协议号按需递增并补 `world_ser` 往返。
- 更新 `SKILL_ALIGNMENT_LEDGER.md` / `SKILL_CODE_AUDIT_098c.md` 的**状态列**（把 ⚠/📋 改成 ✅ 并写 commit hash）。
