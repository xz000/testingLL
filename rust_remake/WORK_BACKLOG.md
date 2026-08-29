# 后续工作总索引 WORK_BACKLOG

> 创建 2026-08-25。把之前散落在 `NEXT_STEPS.md` / `PLAN.md` / `ROADMAP.md` / `UI_MENUS.md` /
> `LATENCY_MASKING.md` 里的**所有未完成工作**汇总成一张可执行清单，作为下次接续的统一入口。
> 阅读顺序：`NEXT_STEPS.md`（交接）→ 本文件（后续工作索引）→ 各专项文档（`STEAM_MULTIPLAYER_PLAN.md` 等）。

---

## 0. 当前状态快照（2026-08-29 会话末 · 建房金币配置 + 中文 IME + 无控制台 + 若干修复，最新）
- **本会话完成并提交**（7 个提交：`df99301`…`37a2b5e`，工作区干净，见末尾 git log）：
  - **房主建房可设置金币配置**（走大厅元数据同步，host/client 两端一致）：
    - **初始金币**（第一局开局一次性发放，独立于每轮参与奖；`MatchConfig.starting_gold`，默认 0）；
    - **每轮固定金币**（参与奖，复用 `gold_per_round`）；
    - **单轮名次奖励**：**自动生成**——输一个「第一名」金额，按 ×0.6 向下取整递减到 0，覆盖任意玩家数（`auto_place_rewards`，如 `30`→`[30,18,10,6,3,1]`）；
      仍兼容逗号分隔手动档位（如 `30,20,10`）。每轮发金币/记录单轮排名决定奖励等逻辑本就有（`give_round_gold`/`finish_round`），本次仅补建房入口+同步。
  - **建房界面重绘为两列 8 字段**（左：房名/备注/人数/轮数；右：准备时间/初始金币/每轮金币/名次奖励），
    **方向键二维导航**（↑↓ 同列上下、←→ 换列、Tab=↑），聚焦高亮+字段专属提示。
  - **中文房间名（IME）**：自定义 winit 事件循环替代 `ggez::event::run`（ggez 0.10 不转发 IME 事件），
    接入 `WindowEvent::Ime(Ime::Commit)` → `Game::on_text_input`，建房界面/编辑房间信息的房间名与备注支持中文输入法。
    自定义循环在 `resumed` 里 `set_ime_allowed(true)`，其余事件分发照搬 ggez（键盘/鼠标/触摸/绘制/退出），行为不变。
  - **发布版无命令行窗口**：`client/src/main.rs` 顶部 `#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]`，
    release=GUI 子系统（不弹黑色命令行，PE subsystem=2）、debug=Console（保留日志）。`publish.ps1` 加了一步校验（读 PE subsystem，非 GUI 会警告）。
  - **建房超时与诊断**：创建大厅等待 10s→25s（`STEAM_LOBBY_CREATE_BEATS=500`），等待中每 2.5s 打印进度；
    `CreateLobby` 失败打印具体 `SteamError`（曾遇 **NoConnection=Steam 掉线**）并给出中文原因提示（NoConnection/AccessDenied/服务繁忙）。
  - **修复就绪倒计时闪烁**：client 原按“本帧是否恰好收到 host 的 RosterReady 广播”判定全员就绪，某帧没收包即回退 false，
    导致界面在“按 U 就绪/倒计时”间快速闪烁；改为持久字段 `steam_roster_all_ready` 仅在收到新快照时更新。
- 基线：workspace **143 全绿**（98+31+9+5），steam feature client **7 全绿**（+`auto_place_rewards` 测试），
  build/test/clippy（默认 + steam）全绿；release 冒烟窗口正常启动；publish.ps1 `-BuildOnly` 收集 client.exe+steam_api64.dll 且 GUI 校验通过。
- ⚠ **待真机复验**（需 Steam 双账号）：建房/加入金币配置两端一致、中文 IME 输入、发布版无命令行窗口体验、方向键导航手感、就绪倒计时不再闪烁。

---

## 0. 当前状态快照（2026-08-29 会话末 · Steam 发布脚本 + 对局/建房功能 + 若干修复，最新）
- **本会话完成并提交**（10 个提交：`80387cc`…`471b679`，工作区干净）：
  - **Steam 发布脚本** `publish.ps1`：编译 release → 收集产物(exe+steam_api64.dll) → 生成 VDF → steamcmd 上传。
    已填 DepotID=908661「Circle Brawl Content」(AppID 908660)；支持交互式登录（环境变量优先，否则每次上传输账号+隐藏密码）；
    修复 PS5.1 编码(UTF-8 BOM)与 native stderr `2>&1` 误报；steamcmd 装在仓库根 `testingLL/steamcmd`（git 已忽略）；
    发布版刻意不打包 `steam_appid.txt`。
  - **柱子/玩家出生**：柱子改环状均匀分布+每轮不同(`round_seed` 递增)+不重叠，数量随机 **0~5 可为 0**；
    玩家出生在 0.6×arena 环等分+随机旋转，**每轮结束重生回出生环**。
  - **房主可设定**（存大厅元数据同步，client/冷启动读取对齐）：**总轮数**(建房第4字段, 1~256 默认3)、**局间准备时间**(第5字段, 8~256秒 默认20，`MatchConfig.learn_time_secs` 默认改 20)；
    人数/轮数/准备时间改用**编辑缓冲**（Backspace 可逐位删）。
  - **修复**：新轮状态残留（清 `move_target` + `projectiles`，冷却已由 `Caster::new` 正确重置）；
    **客户端与主机同步就绪倒计时**（不再抢先进配置）；**施法蓄力动画只本地自己显示**（他人看不到前摇圆环）。
  - 基线：workspace **97+31+9+5 全绿**，build/test/clippy（默认 + steam）全绿。

---

## 0. 当前状态快照（2026-08-29 会话末 · 技能系统+界面优化，最新）
- **本会话完成并提交**（10 个提交：`297e20e`…`9b0f7dd`，工作区干净）：
  - **技能系统对照 Unity 原版**：补齐缺失技能 雷电(D1)+换位(R3a)；修复冲锋/雷电参数失效、转镖先直线后转向；
    **系统性技能成长全部接入 stats 驱动**（批A弹体/批B链扇/批C区域线，升级后伤害/射程等真正成长）。workspace **136 全绿**。
  - **界面优化**：主菜单 + Steam 大厅子菜单支持**鼠标点击** + 悬停高亮；大厅子菜单支持**上下箭头**；
    **修复菜单切换逻辑**（选 Steam 直接进创建房间 bug：进入大厅那帧触发键被二次消费）；**单机试验场 Esc 返回主菜单**；
    **配置界面 Esc 返回 + 开始键扩展**(Space/P/回车)。
- 基线：workspace **136 测试全绿**，build/test/clippy（默认 + steam）全绿。

---

## 0. 当前状态快照（2026-08-29 会话末 · 第二批也已落）
- **Steamworks 第二批（Ping / 头像 / 统计 / 成就 / 排行榜）已落**（2026-08-29，待真机复验 + 后台配置）：
  见 `NEXT_STEPS.md`「Steamworks 第二批」节。
- 基线：workspace **132 测试全绿**（+5：`net-steam/src/stats.rs` 战绩规则层），build/test/clippy（默认 + steam）全绿。
- ⚠ **统计/成就/排行榜需要 Steamworks 后台先定义 key**（键名见 `net-steam/src/stats.rs` 顶部注释），
  没配置时只会打日志、不影响游戏；Ping 与头像不需要后台配置，真机直接可见。

---

## 0c. 技能系统对照核对与修复（2026-08-29 会话，最新）
- **对照 Unity 原版全部技能（`Sender.cs` 的 `SkillCode` 枚举）逐技能核对行为**：C/D/E/F/G/R/T/Y 全树 + 测试技能。
- 成果：
  - **补齐缺失**：雷电（D1，默认 D 键）、换位（R3a）两个正式技能（提交 `297e20e`）。
  - **修复 bug**：冲锋/雷电参数脱节（`world.rs` 用 `stats` 但 `growth` 缺 base → 冲锋不移动、雷电推 0 时长）（`80eecb6`）。
  - **修复转镖**：`TurnLeech` 的 `turn_delay` 原被忽略 → 转镖退化为全程自动追踪；新增 `Chain.turn_delay` 恢复“先直线飞再转向最近敌人”的手感（`7c389b0`，含 world_ser 序列化）。
  - **确认回旋镖**：已实现回旋（每帧速度朝施法者加速拉拽 + 撞障碍反弹 + 命中爆炸），符合回旋镖力学，无需改。
- 基线：workspace **136 测试全绿**（技能批次共 +4：雷电/换位/转镖转向），build/test/clippy（默认 + steam）全绿。
- ✅ **系统性技能成长已全部接入（stats 驱动）**：批A(弹体/导弹回旋镖香蕉弹滚动火球撒弹线散射线) + 批B(链/扇/吸血链镖转镖跳弹扇面扇扫蓄力跳弹) + 批C(区域/线/回拉线撞击迟缓爆炸弹束缚线引力场星域自爆)，
  `world.rs::execute_effects` 全部改用 `SkillGrowth` 派生的 `stats`（等级1数值不变，升级后伤害/射程/推击成长），并补全各自 growth base（防“参数脱节→为0”）。
  仅疾跑/护盾的 buff 时长仍走 effect（时长成长意义小，留数值调参）。提交 `1e08c0c`/`a9dbc8d`/`944b369`。

---

## 0b. 上一批（第一批：好友邀请 + Rich Presence，2026-08-29 已落，待真机复验）
- **Steam 联机主线三阶段全部完成并真机验证**：掉线处理+重连 ✅ / 快照广播 ✅ / **主机迁移（连续迁移）✅**。
- **Steamworks 第一批（好友邀请 + Rich Presence）已落**（2026-08-29，待真机双账号复验）：
  房间界面按 I 展开「邀请好友」面板（定向邀请 / Steam 邀请窗口）；
  Rich Presence 按阶段上报（房间/配置/对局）+ `connect` 串 → 好友可一键加入；
  被邀请方支持「游戏在跑 → 回调自动进房」与「游戏未跑 → `+connect_lobby <id>` 冷启动进房」。
  详见 `NEXT_STEPS.md`「Steamworks 第一批」节。
- 基线：workspace **127 测试全绿**（+2 connect 串解析），steam feature 下 client 6 全绿（+1 命令行解析），
  build/test/clippy（默认 + steam）全绿，HEAD=`3b6bbd5` + 未提交改动。
- 架构定案：**Steam 为中心，纯玩家 P2P + 不租服务器 + 帧同步 host 权威**（见 `STEAM_MULTIPLAYER_PLAN.md`）。
- 字体：`assets/fonts/cjk.ttf` 已换为全量 **SourceHanSansCN-VF**（17.7MB，字形齐全；旧 168KB 子集留作 `cjk-168k.ttf`）；
  冒烟确认新字体能加载、窗口正常。

---

## 1. Steamworks 增强（用户确认的优先级顺序）
详见 `STEAM_MULTIPLAYER_PLAN.md` §3 盘点。

| 批次 | 内容 | 价值 | 说明 |
|---|---|---|---|
| **第一批** | **好友邀请 + Rich Presence** | 高 | ✅ **已落（2026-08-29）**，待真机双账号复验（见 `NEXT_STEPS.md`） |
| **第二批** | 成就 / 排行榜 / 头像 / Ping | 中 | ✅ **已落（2026-08-29）**：Ping/头像真机直接可见；统计/成就/排行榜需后台配置 key + 真机复验 |
| **最后** | 云存档 | 中 | 技能树绑定/成长/金币 meta 存云端（数据现成，延后） |

> 已排除：专用服务器（不租）、商店/支付/DRM。
>
> 能力边界（已核实 steamworks 0.13）：**没有** `InviteUserToLobby`，定向邀请只能走
> `Friend::invite_user_to_game(connect 串)`；Rich Presence 用 `status` + `connect` 两个键（未做 `steam_display` 本地化）。

---

## 2. 单机调试辅助（ROADMAP M1，未做）
`UI_MENUS.md` §3 定义：单机试验场加调试工具，作为数值/手感调优的"安全试验场"。
- 工具栏：玩家位置 / HP / 各技能冷却 / 当前施法阶段。
- 快捷键：重置冷却、回满血（无敌开关）、重置位置、"假人目标"测伤害。

---

## 3. 局域网体验补完（ROADMAP M2，部分做了）
局域网是**离线备胎**（Steam 为主），只做必要维护，不投入 Steam 专属增强。
- 玩家名 / 开局房间界面 / 对局结束回主菜单（已做 Q 回主菜单）。
- **局域网房间列表 / 自动发现**：默认**不做**（方向性决定：Steam 大厅天然提供，局域网手动 `--host`/`--join` 即可）。

---

## 4. 游戏手感 / 系统调优（PLAN 阶段 2 遗留）
| 项 | 状态 | 入口 |
|---|---|---|
| 技能成长数值手感调优 | 待做 | `PLAN.md` 阶段 2 |
| **击退模型 Impulse**（瞬时初速度+逐帧减速，替代定速推） | 待做，**不影响协议** | `PLAN.md`「待议/搁置」 |
| shift 施法瞄准线起点（当前位置 vs 最终位置） | 暂缓 | `PLAN.md`「待议/搁置」 |
| 升级流程交互（数字键=选技能绑定 vs 升级）是否困惑 | 待真机确认 | `PLAN.md`「待议/搁置」 |
| shift 冲刺 / 力场手感 | 未定级 | `PLAN.md`「待议/搁置」 |
| 冷却 HUD 换图标贴图 | 阶段 4 美术 | `PLAN.md` 阶段 2 |

---

## 5. 表现层美术（阶段 4）
- Cell-Graph-Risk 扁平几何 / 节点细胞风（目标美术）。
- 粒子、音效、菜单打磨。

---

## 6. 延迟掩盖：完整回滚重放（LATENCY_MASKING 阶段二 / 4.7）
- 决策门：先真机感受乐观预测，跳变明显再做。
- 前提：局域网/Steam 已"严格 lockstep + 快照 + 迁移"，完整回滚需在此基础上加本地预测+回滚。

---

## 7. UI 打磨
- **暂停 / 退出菜单**（联网暂停=本地暂停交互、不暂停时间；退出按离场处理）。
- ~~**中文房间名输入（IME）**~~：✅ 已落（2026-08-29，自定义 winit 事件循环接入 `Ime::Commit`，房间名/备注支持中文）。
- 主菜单 / 各阶段 UI"清爽化"：S1–S5 已做大部分（卡片式、左右分栏），后续按需。
- **多语言（i18n）**：待 UI 基本定型后做（2026-08-29 评估暂缓，用户同意后置）。当前文案为中文+ASCII 硬编码；建议先定 `Lang` 枚举 + `tr(key)` 查表函数逐步抽离，至少加英文一种，暂不铺开多种。联网下语言仅本地显示，不影响协议。

---

## 8. 已知边界 / 未尽（联机）
| 项 | 说明 |
|---|---|
| Steam 迁移：多个 client 同时掉线 | 阶段 3 目前只处理"原 host 掉线"；其他 client 掉线时新 host 会等其输入而卡（可扩展对掉线 client 也 auto_drop） |
| Steam 迁移：不满员对局 | 阶段 3 假设满员，未特别处理不满员迁移 |
| 重连手感真机验证 | 局域网多窗口手动验证重连（host 掉线快照给候补/回大厅） |

---

## 9. 下次建议起点（按价值/依赖排序）
0. ✅ **（已完成，2026-08-29）建房金币配置 + 中文房间名 IME + 发布版无命令行窗口 + 名次奖励自动生成 + 建房超时/诊断 + 就绪倒计时闪烁修复 + 建房界面方向键导航**
   （7 个提交 `df99301`…`37a2b5e`，见顶部快照）。
1. **真机双账号复验本次新功能**（最重要，见顶部快照「⚠ 待真机复验」清单）：金币配置两端一致、中文 IME 输入、方向键导航手感、就绪倒计时不闪烁、发布版无命令行窗口体验。
2. **Steam 发布上传（暂缓中，用户密码暂忘）**：`publish.ps1` 已就绪（DepotID=908661，`-BuildOnly` 已验证产物 client.exe+steam_api64.dll）。
   真上传：设 `STEAM_USER`/`STEAM_PASS`（或交互输入）→ 处理 Steam Guard（建议带 bot 的发布账号/访问令牌）→ `-SetLive beta` 先测 → 确认后 `public`。
3. **真机双账号复验此前功能**：柱子每轮不同/数量随机(可0)、玩家每轮重生出生环、房主设定轮数与准备时间两端对齐、就绪倒计时同步、施法蓄力动画仅本地自己可见。
4. **真机复验 Steamworks 第一批 + 第二批**（清单见 `NEXT_STEPS.md` 两节末尾）：Ping/头像无需后台配置最快；统计/成就/排行榜需先在后台按 `net-steam/src/stats.rs` 建 key/榜。
5. 复验通过后接最后一批：**云存档**（Remote Storage，meta 数据现成）。
6. **学习阶段交互（暂缓）**：选技能树/绑定仍只用字母+数字快捷键，可加鼠标/方向键（用户暂不做，留待）。
7. 技能成长**数值手感调优**（调各技能 `SkillGrowth` 的 base/delta，见 §4，需真机/试验场感受）。
8. 之后按需：**单机调试辅助** → **游戏手感调优**（击退 Impulse 等）。
9. 表现层美术 / UI 打磨 / 延迟回滚 / **多语言 i18n（见 §7）** 作为长期目标。

---

## 10. 常用命令（续接用）
```
cargo test --workspace                  # 回归（当前 98+31+9+5 = 143 全绿基线）
cargo test -p client --features client/steam   # steam 版 client 7 测试
cargo clippy --workspace -- -D warnings
cargo clippy -p client --features client/steam -- -D warnings
powershell -File run-steam.ps1 -Mode menu   # Steam 版主菜单（按 3 进大厅，H 建厅 / J 房间列表）
powershell -File check.ps1                  # 一键 build+test+clippy
```
