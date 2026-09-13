# 交接说明（HANDOVER）—— 下次对话从这里开始

> 本文件是**唯一的"下一步"入口**：状态、命令、待办、坑，都在这。
> 细节看各自的专题文档（见文末索引）。

## 一、怎么跑 / 怎么验

```bat
cargo test --workspace                                  :: 基线：313 项
cargo clippy --workspace -- -D warnings                 :: 必须干净
cargo clippy --workspace --features client/steam -- -D warnings
cargo build -p client                                   :: debug
cargo build --release -p client --features client/steam  :: release（联机用这个）
```
- 联机复验走 `powershell -ExecutionPolicy Bypass -File run-steam.ps1`
- **改完必看 exe 时间戳**；或在 exe 里搜新增字符串确认是新版本（详见 `WORKFLOW_NOTES.md`）

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

## 三、关键常量与版本

| 项 | 值 |
|---|---|
| `PROTOCOL_VERSION` | 16 |
| `CONFIG_VERSION` | 15 |
| UI 设计分辨率 | `UI_W=1280 / UI_H=720`（`ui::design_rect` 自适应） |
| 房间设置串 | `MatchConfig::to_meta_string()`，单键 `room_cfg`（`ROOM_SETTINGS_KEY`） |
| 测试基线 | client 54 / game-core 241 / net 39 / net-steam 9；steam client 61 |

## 四、待办（按建议优先级）

1. **表现层 P2 · 音效（进行中）**：`AUDIO_PLAN.md` —— 后端 `ggez::audio`；**占位素材已生成**。
   ✅ P2-0 占位素材（`tools/gen_placeholder_audio.py`）· P2-1 `local_settings`+`AudioBank` ·
   P2-2 主菜单「设置」界面（滑条+键鼠）+ `F10` 全局静音（`M` 已被商店分类键占用）。
   ⬜ 待接：战斗事件信号（Hattrick/Vampire/Silencer/Pancake/Burnout/Denied/LastSecondSave）。
   已接：命中/治疗/死亡/击杀、首杀、连杀 3..10/>10、多重击杀 2..6（9s 窗口）、Ludicrous、
   学习/升级、胜利；**不接** 缩圈/出界/倒计时/回合流程/买卖（098c 无）。
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
5. `R017` 的小遗漏：`I004` 持有者击退减免按 +3 级计（`JASS_AUDIT_098c.md`）
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
     遗留：`xv>0` 的 mover 反弹（弹体类）与场地边界 ×0.5 反弹未做。
8. **平局加赛机制缺失**（待做）：098c 平局时**加赛一轮**（`rI` war3map_pretty.j:22488-22492：
   `AV=AV+1; StartSound(Vo)` + 广播 `"Draw! One more round to decide the battle"`）。
   我们目前只有固定 `total_rounds`，**无平局判定、无追加轮**。
   待定义：平局条件（如同分/并列第一）→ 追加一轮 → 结算；暂已用 `Vo` 音效（`ann_game_start`/`ann_finish`）。
9. **S010/S012 接触伤害对齐（2026-09-13）**：`CA` 规格完整实现 —— `Player.charging`（`fr`）区分 A/B 形态；
   `bA` 改为以攻方为圆心的 AoE（`splash_damage`，半径 160×(1+.12xi)、×Gn）；
   S012A 撞敌自伤 + `xi>0` 的 `SI` AoE；凤凰弹门控改为只看 B 形态。协议 15→16。
   仍有意保留：风步接触吸血（098c 无）。

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

`fc1fd6b` 编辑器按钮快捷键提示 + 房间列表两步式 ← `465429e` 编辑器保存/不保存 + 鼠标建房 + 修 CRLF 假通过 ←
`9edecb7` 设置编辑器鼠标支持 ← `992a8f6` Steam 大厅菜单/连接中主题化 ← `dce5bdb` Steam 房间列表主题化 ←
`f1255e5` 占位符保留 + Steam UI 计划 ← `16a097d` 删弃用测试技能收尾 ← `928de7a` 删 Test01/Test03 ←
`6dbcede` net 测试警告清理 ← `7cd4cbe` 快照广播降频/复用 hash/取新基线 ←
`ca25399` 快照体积更正 ← `58e31db` 帧同步卡顿分析 ← `2f207cf` 表现层 P1 ←
`6d9d3f2` client 看房主参数 ← `b21d81f` 输入路由+中文 IME ← `f4835c6` 商店升级后保持高亮 ←
`811ad8d` 商店/成长列表+详情 ← `c854027` HANDOVER 刷新 ←
`565728e` 商店退格卖出可用化 ← `7270497` HANDOVER ← `961182d` 商店一行两动作
← `065c4ff` 源码扫描测试抗重构 ← `74edf9d` 金币时序 ← `12f1946` 工程坑文档
← `31bf70b` 人数/提示 ← `30e6ea5` 非房主只读 ← `5ce9ee1` 开局发钱（初版）
← `7ebacee` 客户端同步 meta.config ← `e75579d` 房间信息界面职责拆分
← `fb62684` 统一设置（双数据源）← `254b044` H 走统一编辑器 ← `aa7899e` 建房界面收敛
