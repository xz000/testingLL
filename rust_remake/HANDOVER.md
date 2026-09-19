# 交接说明（HANDOVER）—— 下次对话从这里开始

> 本文件是**唯一的"下一步"入口**：状态、命令、待办、坑，都在这。
> 细节看各自的专题文档（见文末索引）。

## 一、怎么跑 / 怎么验

```bat
cargo test --workspace                                  :: 基线：356 项
cargo clippy --workspace -- -D warnings                 :: 必须干净
cargo clippy --workspace --features client/steam -- -D warnings
cargo build -p client                                   :: debug
cargo build --release -p client --features client/steam  :: release（联机用这个）
```
- 联机复验走 `powershell -ExecutionPolicy Bypass -File run-steam.ps1`
- **改完必看 exe 时间戳**；或在 exe 里搜新增字符串确认是新版本（详见 `WORKFLOW_NOTES.md`）

## 一·补、发布到 Steam（上传 + 上线）

**脚本**：`publish.ps1` = 编译 release → 收集 staging → 生成 app_build VDF → 调 steamcmd。
- 构建特征：`client/steam,client/gui`。**`gui` feature 只在这里开**：release profile 为帧同步确定性
  强制 `debug-assertions = true`，所以 GUI 子系统**不能**靠 `not(debug_assertions)` 判定
  （旧写法导致发布版弹黑框）；`gui` 控制 `windows_subsystem="windows"`。`check.ps1` 也构建/测试/clippy 这组。
- appid **908660**（Circle Brawl）/ depot **908661**（Circle Brawl Content）。

**推荐流程（本 app 的 `default` 分支拒绝命令行 SetLive）**：
```bat
powershell -ExecutionPolicy Bypass -File publish.ps1 -SteamUser xvzan   :: 只上传，不动线上
:: 然后到 Steamworks → SteamPipe → Builds → 用下拉把新构建设到 default 上线
```
`publish.ps1` **默认就是“只上传、不设分支”**；要自动上线非默认分支用 `-SetLive <branch>`。

**踩过的坑（2026-09-14 排查）**：
1. `SetLive "public"` → `Access Denied`：**根本没有 `public` 分支**；Steam 默认主分支名就是 **`default`**。
2. `SetLive "default"` → `Failure`：**命令行 SetLive 到默认分支被拒**（缓存令牌登录即可复现；
   与手机 Steam 令牌无关）。**非默认分支（beta）的 SetLive 可行** → 内部测试可建 beta 分支全自动。
   网页以 owner 身份把构建设到 default 上线正常。
3. `publish.ps1` 曾硬要求 `steamcmd\config\loginusers.vdf`（本机 steamcmd 把账号记在
   `config.vdf` 的 Accounts 里）→ 已降级为 WARN，并加 `-SteamUser`（非交互）。
4. **上传 ≠ 上线**：构建上传成功不代表已上线；`Builds` 列表里“已包含 Depot”但没设分支就是没上线
   （曾误判为“没传上去”）。

**字体**：外置 `assets/fonts/LXGWWenKaiMonoLite-Medium.ttf`（LXGW 文楷，OFL-1.1），随 exe 分发；
`load_cjk_font` **只从磁盘加载，找不到直接报错**（已移除内联 168k 回退）。

## 二、当前状态（已实现且已验证）

- **设置系统（统一）**：`H`（建房）与 `O`（房内）打开**同一个**设置编辑器；分组 `[A]房间 [Z]经济 [X]玩法 [C]地图 [V]模式`；
  `回车`/`T`=操作当前行（文本/数值进输入，枚举/开关切换），`←/→`=调档；
  **房内**：`O`/`[保存]`=保存并发布、`Esc`/`[不保存]`=**回滚到打开时快照且不发布**；
  **建房**：`回车`/`[创建房间]`=建房、`Esc`/`[取消]`=取消；非房主**只读**；
  **键鼠同源**（点页签/行/按钮与键盘同一条路径）；按钮带快捷键提示；
  房主改设置后**各端自动取消准备**（走大厅设置串 `room_cfg`，无需额外协议）。
- **金币时序（已按 098c 校准）**：首次进配置/商店时发 `初始金+首轮参与奖`；**每轮参与奖在回合结算时发**（进商店前到账）；
  两者都**幂等**；对局未开始时金币为 0。
- **商店/成长（列表 + 详情，2026-09-12 重塑）**：
  - 选中行 → 详情区显示**描述**，下方是**「购买」「卖出」两个按钮**，各带快捷键与**禁用态**
    （不可用时置灰并写明原因：已满级 / 金币不足 / 背包已满 / 未持有）；
  - 行标签**不再有 `[买]`/`[卖]` 前缀**（动作只在按钮上）；`=`/回车 只购买/升级，`退格`/`Delete` 卖出；
  - 纯函数 `shop_rows`（`ShopRow` 行模型）、`shop_buy_block`、`shop_sell_target`、`mastery_block` 均有单测；
  - 成长页同构（无卖出），选中精通看描述 + 购买按钮禁用态；
  - **价格模型（2026-09-19 校正）**：技能购买价 = `learn_cost + jf×10`（`jf = spell_buys.saturating_sub(2).min(3)`，
    买第 3/4/5 个法术时 JASS `Jf` 抬的是**购买研究**，故涨价计入购买价）；技能升级价 = `upgrade_cost + (等级-1)×glvl`
    （火球 R002 `glvl=11`、其余 10）；精通价 = `COSTS[kind] + COST_PER_LEVEL[kind]×已购级`（6/6/6/3）。
    （此前把 `Jf` 涨价错记到升级价，已修正；见 `JASS_AUDIT_098c.md` B 轮。）
  - `keys::CONFIRM_HINT` / `keys::SELL_HINT` + `keys::confirm_just` / `keys::sell_just` 统一提示与判定。
- **大厅 UI**：四带版面骨架（`layout.rs`）、统一调色板、覆盖层统一入口、鼠标可点、
  子界面**清屏**、房间面板显示设置信息块（所有端可见）、房间列表显示「自定义 N 项」。
  已按技能/商店风格重塑（`STEAM_UI_REDESIGN.md` U1–U4）：大厅主菜单/房间列表/连接中均主题化 + 鼠标；
  房间列表**两步式**（点行=选中，`[回车 加入]`/回车才加入）+ 右侧详情 + 滚动 + 悬停。
- **E 键已退休**（房间名/备注并入 `[A]房间`）。
- **输入路由 + 中文 IME（2026-09-12 大修）**：`TextField` + `Game::text_focus()/text_buffer_mut()` 成为 `Ime::Commit`
  的**唯一去处** —— 建房名/备注、设置编辑器 `[A]房间` 的房名/备注都能输入中文（此前设置编辑器收不到 IME）。
  文本态屏蔽字母快捷键（`O`/`M`/`R`/`Q` 不再抢键）；`Ime::Preedit` 组合期间不走 ASCII 白名单（防重复/乱码）；
  长度按**字符数**（40）而非字节。纯函数 `append_text_limited` 有单测。
- **client 看房主设置**：`steam_sync_room_meta()` 每帧从大厅元数据回读房名/备注/人数上限；
  client 按 `O` 可打开**只读**编辑器看到全部 `MatchConfig` 参数，徽章提示 `[O] 查看`。
- **表现层 P1**：伤害/治疗**飘字**（世界坐标、上飘淡出；自伤红、他伤黄、治疗绿）
  + 首杀/击杀/连杀**横幅**（连杀用 098c `streak_label`）。纯客户端、由 hp/alive 差分推导，不进快照；
  世界重建/新一局/复活均正确重置。纯函数 `health_delta_text` 有单测。

- **多语言（i18n，2026-09-15）**：主语言中文 + English；语言由 **Steam 语言设置**驱动
  （`Apps::current_game_language`，见 `net-steam`），也可在「设置 → 语言」手动固定（`Auto/简体中文/English`，
  存 `LocalSettings.lang`）。实现：`client/src/i18n.rs` —— **以中文原文为 key** 查英文表、漏翻回退中文；
  静态文案在 `draw_text`/`ui::text_*` **绘制时统一过一层 `t`**，含变量文案用 `i18n::tf("…{x}…")`。
  英文技能/道具文案优先取自 098c 原文。详见 `I18N.md`。
  命令行语言覆盖：`client.exe --lang en|zh|auto`（不开 Steam 也能测英文）。
- **主菜单卡片点击错位修复**：绘制与点击命中共用 `main_menu_card_rect`（此前两处硬编码不同步），有回归单测。

- **098c 持续伤害/拉拽口径对齐（2026-09-19）**：① DoT 不再每帧涨 `Gn`（成长由每发弹体命中触发）；
  ② 引力/锁链 A 形态的“每 tick”数值按 `je` 门控（每 0.18s）换算为每秒 DPS；
  ③ 引力吸力带距离平方衰减 `Hc×(1−d²/600²)`，量纲 = `Force×5/3`；
  ④ 暗物质是“场”、不与柱碰撞（旧 bug 导致首帧被销毁→看不到效果）；
  ⑤ 岩浆补接触 DoT（buff `rr`）；⑥ 红链沿线切割 `Tether.beam_dps`（协议 20→21）。均有回归单测。

## 三、关键常量与版本

| 项 | 值 |
|---|---|
| 产品名 | 英文 **Circle Brawl** / 中文 **圆圈之战**（Steam AppID 908660；原名 Warlock Brawl / 术士之战） |
| `PROTOCOL_VERSION` | 21 |
| `CONFIG_VERSION` | 15 |
| UI 设计分辨率 | `UI_W=1280 / UI_H=720`（`ui::design_rect` 自适应） |
| 房间设置串 | `MatchConfig::to_meta_string()`，单键 `room_cfg`（`ROOM_SETTINGS_KEY`） |
| 测试基线 | client 83 / game-core 261 / net 39 / net-steam 9（合计 392）；steam client 90 |

## 四、待办（按建议优先级）

1. **表现层 P2 · 音效（进行中）**：`AUDIO_PLAN.md` —— 后端 `ggez::audio`；**占位素材已生成**。
   ✅ P2-0 占位素材（`tools/gen_placeholder_audio.py`）· P2-1 `local_settings`+`AudioBank` ·
   P2-2 主菜单「设置」界面（滑条+键鼠）+ `F10` 全局静音（`M` 已被商店分类键占用）。
   ⬜ 待接：战斗事件信号（Hattrick/Vampire/Silencer/Pancake/Burnout/Denied/LastSecondSave）。
   已接：命中/治疗/死亡/击杀、首杀、连杀 3..10/>10、多重击杀 2..6（9s 窗口）、Ludicrous、
   学习/升级、胜利；**不接** 缩圈/出界/倒计时/回合流程/买卖（098c 无）。
   ✅ **外部音频包 A0+B0+A1+B2（2026-09-19）**：音效包整包覆盖 + BGM 分场景（menu/lobby/battle/result，循环+交叉淡出）
   + 设置页选择/浏览工坊/发布本地包 + 本地/创意工坊目录热重载；方案与包格式见 `AUDIO_PLAN.md` §8。
   Steam 发布/浏览部分需真机验证（UGC，`SteamTransport::{create_workshop_item, submit_workshop_update}`）。
2. **表现层 P1 扩展**：Hattrick / Vampire / Denied / Burnout / Silencer / Pancake / Last-Second-Save 等事件横幅
   （需额外战斗信号）—— 与 P2 的播报音共用信号（098c 触发条件已录入 `AUDIO_PLAN.md` §1）。
3. **死代码清理**（详细分段见 `DEAD_CODE_CLEANUP.md`）：
   - ✅ **段 1**：已删旧建房界面 `draw_steam_create_lobby` / `CreateAction` / `create_hitboxes` / `create_step_field`
     / `create_dispatch` + 鼠标命中块（净 −207 行）。`layout.rs` 遗留表单项暂 `#[allow(dead_code)]`，待段 2 删。
   - ⬜ **段 2**：✅ 已完成（删 `steam_room_edit*` / `draw_steam_room_edit` / `steam_edit_*`、`Screen::RoomEdit`；
     同步改 `on_text_input`/`keys.rs`/`source_scan_tests`；顺手修 `world_ser.rs` mojibake 注释）。
   - ☑ **段 3 完成**（S3-1~S3-4）：旧建房键盘表单 + `steam_create_*` 字段全部删除，创建模式改为统一设置编辑器独占输入；
     `steam_create_confirm`/`finish_enter_steam_mode` 只读 `room_meta`+`match_cfg`/`match_*`（修好了编辑器设置被旧缓冲覆盖）。
     ✅ **已目测通过（用户 2026-09-13）**：建房后客户端收到的大厅参数一致（用户单方目测，非正式双机）。详见 `DEAD_CODE_CLEANUP.md`。
   - **做法**：一次一个可编译单元；`edit` 工具按精确文本删（不再用行号脚本）；每段跑门禁 + 记录。
4. **房间设置与 098c 对齐**（详见 `SETTINGS_ALIGNMENT_AUDIT.md`）：
   - ✅ **阶段 A**：开局丢设置 S1、参与奖时点 S4、`-no reward`(A3)。
   - ✅ **阶段 B**：时长 4→2 合并、删除名次金 `place_rewards`（settings schema → **3**）。
   - ✅ **阶段 C**：伤害/击退/岩浆倍率接入结算；柱子/冰面按我们三档接入；地图形状行置灰锁定。
   - ✅ **无剩余实现项**：`-league` 不做专用开关（用现有金币/点数/柱子/冰面/时长配置项即可组合出等价预设，已裁定）。
   - ✅ **已目测通过（用户 2026-09-13）**：建房→改设置→开局一致（用户单方目测）。
5. ~~`R017` 的小遗漏：`I004` 持有者击退减免按 +3 级计~~ → ✅ **已核/关闭**：`R017` 只是假科技载体（非精通上限）；
   购买精通块是**增量**写法，连乘望远镜相消得 `Hn = 1 - 0.025×总精通`（我们已对齐）。
   `I004` 的 `lf -= 3` 只把端点平移，满级 N=6 时 `Hn≈0.860`（无面具 0.850）→ 影响 **<1% 且方向相反**，**不实现**。详见 `JASS_AUDIT_098c.md` ⑥。
6. **联机卡顿修复**（`FRAME_SYNC_ANALYSIS.md`）：
   - ✅ **快照广播降频**：Steam host 每 30 帧只本地 `set_snapshot`（重连），广播降为每 150 帧（接管）；
     hash/snapshot 复用一份 `world_to_bytes`；接管取 seq 更新者（`newer_snapshot`）。
   - ⏸️ **host 固定节拍 + 输入延迟：已搜置（用户裁定 2026-09-13）**。走经典 RTS 式 lockstep（等齐再推进），**不引入固定 D**。
     草案见 `FRAME_SYNC_INPUT_DELAY_DESIGN.md`（保留为备选）；待用户实测手感后再定后续（候选：渲染插值 → 降 tick → D → 预测/回滚）。
   - ✅ **client 收敛追赶**：`accumulate_tick` 夹到 4 步。输入发送**每模拟 tick 一条**（曾改为“每次 update 一条”，
     实测导致 host 缺输入、sim 掉到 20–30Hz，**已回退**；渲染插值未做）。
   - ✅ **诊断落盘**：新增 `client/src/logging.rs`（带 ms 时间戳，写 `logs/<role>-<epoch>.log`，role 由启动参数判定）；
     关键 net/时序日志已改走它，并加周期 `[stat]`（host 产帧/等输入计数、client 帧号/延迟）。
     `run-steam.ps1` 另将控制台输出 tee 到 `logs/console-*.log`（兜底捕获库内 `eprintln!`）。`logs/` 已 gitignore。
   - ✅ **held-continuous 固定节拍 + 渲染插值（2026-09-13，方案对比后选定；均无协议改动）**：
     `HostLockstep::try_emit` 每 tick 必产帧，缺输入时用上一条输入的**连续量**（丢弃离散动作）/默认；
     `MAX_CATCHUP_STEPS` 4→8；绘制对玩家位置做 `prev→cur` 插值。
     **双机复测已验证**：host sim 由 ~52.5Hz 回到 **60.0Hz**；`emit` 33ms 占比 5~25%→0~3%；
     `waiting for client input` 72→0；client 残余 33/50ms（~2.5%）由插值抹平，肉眼流畅。
7. **玩法修复**（`GAMEPLAY_FIX_PLAN.md`）：
   - ✅ **击退清掉移动目标**：改为 `Game::should_clear_player_target`（仅“已接受 + 未位移/冲刺 + 临近目标”才清，
     击退/冲刺中绝不清）；单测 `player_target_clear_requires_arrival_and_no_displacement`。
   - ✅ **冲撞撞柱与 098c 不一致**：JASS 实证（`war3map_pretty.j` 8640-8730）是**逐轴**响应；`resolve_obstacles`
     已改为“只清指向障碍的那一轴、保留切向 → 沿墙滑行”，不再接触即清 `control`；测试已替换/新增。
     遗留：**mover（弹体）撞柱反弹已按 `xv` 实现**（S000/S004/S009/S014/S016/S018 满反弹=1，S008=.75 衰减；
     其余被挡；见 `skill::pillar_restitution`），逐技能 `xv` 衰减已区分。场地边界 ×0.5 反弹 = **War3 地图可玩区域**
     （`bj_mapInitialPlayableArea`）的引擎限制 → **不做**（我们的边界是岩浆区）。
8. ✅ **平局加赛机制已完成（2026-09-13）**：`MatchState::finish_round` 在终局时若**最高分并列 >1**
   （098c `rI` 的 `ZD>1`）且非死亡竞赛/化身模式 → `total_rounds += 1` 继续 Learning（对应 `AV=AV+1`）；
   客户端检测到总轮数增加 → 播 `Vo` + 「Draw! One more round to decide the battle」横幅。
9. **S010/S012 接触伤害对齐（2026-09-13）**：`CA` 规格完整实现 —— `Player.charging`（`fr`）区分 A/B 形态；
   `bA` 改为以攻方为圆心的 AoE（`splash_damage`，半径 160×(1+.12xi)、×Gn）；
   S012A 撞敌自伤 + `xi>0` 的 `SI` AoE；凤凰弹门控改为只看 B 形态。协议 15→16。
   **招架击退**：量级（4.5→125）与**方向**（双方互相推开）已修正。
   **风步接触吸血已删除**（对齐 098c，其无此机制）→ 协议 16→17。

## 五、文档索引（读哪个）

| 文档 | 内容 |
|---|---|
| `HANDOVER.md` | ← 本文件：状态 / 命令 / 待办 |
| `WORKFLOW_NOTES.md` | **工程坑**（脚本锚点只用 ASCII、探针落文件、验二进制） |
| `JASS_AUDIT_098c.md` | **098c 数值真值台账**（唯一权威，含 JASS 行号证据） |
| `SKILL_STATE_AUDIT.md` | **技能/单位副状态对齐审计**（Hr 燃烧、Fv 链索等缺口 + 工具） |
| `UI_MASTER_PLAN.md` | UI 总规划 + 迁移步骤 + IME 待办 + "开局前发钱"修正记录 |
| `ROOM_SETTINGS_PLAN.md` | 房间设置 17 项与 098c 对照、档位/自定义设计 |
| `LOBBY_UI_PLAN.md` | 大厅重构动机（已被 UI_MASTER_PLAN 取代，保留来龙去脉） |
| `PRESENTATION_PLAN.md` | 表现层 P1–P6 |
| `AUDIO_PLAN.md` | **音效清单（098c 实证）+ 本地设置 + 主菜单设置界面规划（P2）** |
| `AUDIO_SCRIPT.md` | **占位音用途清单 + 播报台词（中文/English）—— 录制用** |
| `I18N.md` | **多语言（i18n）设计与用法**：中文 key + 英文表、Steam 语言驱动、如何加新语言 |
| `FRAME_SYNC_ANALYSIS.md` | **联机卡顿分析**（房间信息轮询 + 帧同步；快照队头阻塞等） |
| `FRAME_SYNC_INPUT_DELAY_DESIGN.md` | **主机固定节拍 + 输入延迟 设计草案**（未实施） |
| `FRAME_SYNC_OPTIONS_COMPARE.md` | **抖动方案对比**（D 帧 vs held-continuous vs 渲染插值） |
| `GAMEPLAY_FIX_PLAN.md` | **玩法修复计划**（击退清移动目标、冲撞撞柱与 098c 差异） |
| `DEAD_CODE_CLEANUP.md` | 死代码清理分段计划与进度 |
| `SETTINGS_ALIGNMENT_AUDIT.md` | **房间/开局设置与 098c 对齐审计 + 清单**（阶段 A/B/C 已完成） |
| `STEAM_UI_REDESIGN.md` | **Steam 大厅/房间列表/编辑器 UI 重塑（U1–U4 已完成）** |
| `UI_AUDIT.md` | 更早的 UI 审视结论 |
| `tools/README.md` + `tools/parse_w3a.py` / `parse_w3q.py` / `parse_objects.py` | 098c 物体数据解析工具 |

## 六、最近提交（新→旧）

`50daa58` HANDOVER 补 Steam 发布流程 ← `2cec2a2` publish 默认只上传 ← `fa79b17` publish SetLive 默认 `default` + `-NoSetLive` ←
`66473e7` publish `-SteamUser` + 不硬卡 loginusers.vdf ← `5e98f75` 发布版 GUI 子系统 feature `gui` ←
`bc17ed2` 头顶状态字下移 3px ← `eb88e47` 头顶状态框描边 1.0 ← `918d7c8` 头顶状态框描边调细 ←
`83bc962` 换字体 LXGW 文楷 ← `b615abd` 删内联 168k 字体回退 ← `9d176cc` HUD 显示熔岩靴 CD ←
`8590df4` 施法结束疾风步 ← `02891d5` 局间同步 mastery/forms + 敌方隐身不可见 ←
`746e946` Blast 固定半径（回退动画）← `a240469` 虔诚治疗环 + 主菜单溢出修复 ←
`f9a0a64` Blast 中心扩散 ← `0defc16` 天罚/虔诚距离衰减 ← `d636f6a` 队列移动标记改青色 ←
`0f6fb2a` 队列标记读模拟队列（修空）← `c813e43` 冲锋/燃烧接触 AoE + 虔诚治疗半径特效 ←
`42dcf02` 爆炸按真实半径绘制 ← `7743711` 状态图标/自身面板/队列标记/商店音效 ←
`a4d042b` P4-2 施法条 + P4-4 自机环/目标标记 ← `d75c769` P4-3 状态图标行 + P3-3 柱子碎裂/爆炸
（更早的提交见 `git log`）
