# 死代码清理计划（2026-09-13）

> 纪律（`WORKFLOW_NOTES.md`）：**一次只删一个可编译单元** → `cargo check`/`cargo clippy` → 通过再下一个；
> 每段完成后在本文档记录。**用 `edit` 工具按精确文本删**（上次按行号脚本因行尾/行号不一致失败）。
> 每段都必须让 `cargo test --workspace` + 两套 clippy 保持全绿。

## 现状：带 `#[allow(dead_code)]` 的对象

| 位置 | 对象 | 说明 |
|---|---|---|
| `client/src/main.rs` | `draw_steam_create_lobby`（fn） | 旧「建房设置」界面（被统一设置编辑器取代），仅定义无调用 |
| `client/src/main.rs` | `enum CreateAction` | 旧建房界面命中动作，仅被 `create_dispatch`/旧绘制用 |
| `client/src/main.rs` | `create_hitboxes` 字段 | 旧建房命中表，仅旧绘制 push、鼠标块读 |
| `client/src/main.rs` | `create_dispatch` / `create_step_field` | 旧建房点击派发/步进，仅被鼠标块与彼此调用 |
| `client/src/steam.rs` 文档 | 提及 `draw_steam_create_lobby` | 注释 |
| `client/src/main.rs` | `BOTS` 常量 | 预留“带 AI 测试”，当前无使用 |
| `client/src/ui.rs` | `theme::warn()` / `theme::TITLE` | 预留主题项 |
| `client/src/keys.rs` | `Screen` / `keymap()` | 键位总表：**声明+表内一致性守卫**，有意保留 |
| `client/src/main.rs` | 旧 `E` 房间信息界面：`steam_room_edit*` / `draw_steam_room_edit` / `steam_edit_*` | `E` 已退休；但被源码扫描测试/`on_text_input` 引用，需一并处理 |

## 分段

### 段 1：旧建房界面「命中/绘制」设施（低风险）
目标符号：`draw_steam_create_lobby`、`enum CreateAction`、`create_hitboxes`（字段+初始化）、`create_dispatch`、`create_step_field`、以及 `steam_lobby_create_update` 里读 `create_hitboxes` 的鼠标块。
- 这些只互相引用；删除后不影响键盘路径与 `steam_create_confirm`。
- 保留：`steam_create_*` 数值/文本缓冲（`steam_create_confirm` 仍在读）。
- **状态**：✅ 已完成（2026-09-13）
  - 删除：`draw_steam_create_lobby`（~170 行）、`enum CreateAction`、`create_hitboxes`（字段+初始化）、
    `create_dispatch`、`create_step_field`、`steam_lobby_create_update` 里的鼠标命中块；同步删了 `steam.rs` 文档里的提及。
  - **连带**：`layout.rs` 里原本只被这个界面用的表单项（`ROW_H`/`FIELD_BOX_*`/`bg_hover`/`border_hover`/`btn_*`）变为未用；
    暂标 `#[allow(dead_code)]`，**待段 2 删掉最后一个表单界面（房间信息编辑）后一并移除**。
  - 门禁：`cargo clippy`（默认 + steam）干净；`cargo test -p client` 37 / steam 43 全绿。

### 段 2：旧 `E` 房间信息界面（中风险）
目标符号：`steam_room_edit` / `steam_room_edit_focus` / `steam_edit_name` / `steam_edit_note` 字段+初始化、`steam_room_edit_update`、`draw_steam_room_edit`、`update`/`draw` 中的调用点、`on_text_input` 的 `TextField::RoomEditName/Note`、`keys.rs` 的 `Screen::RoomEdit`、以及 `source_scan_tests` 中依赖它的两条测试。
- **状态**：✅ 已完成（2026-09-13）
  - 删除：`steam_room_edit_update` / `draw_steam_room_edit`（两函数）、四个字段+初始化、`TextField::RoomEdit*` 两个变体、
    `text_focus`/`text_buffer_mut` 的对应分支、`draw`/`update`/`reset_to_main_menu`/`steam_leave_room` 的调用与重置、
    `steam_lobby_update` 中 4 处 `!self.steam_room_edit` 守卫与文档、`steam.rs` 的 presence 分支与文档。
  - `keys.rs`：删 `Screen::RoomEdit` + keymap 条目 + `ALL`/navigable 引用；删掉 `room_edit_does_not_replace_lobby_heartbeat` 测试；
    `lobby_keys_are_guarded_while_a_subscreen_is_open` 改为按 `!self.room_cfg_edit` 断言；`Screen::Room` 里退休的 `E` 绑定也删了。
  - `layout.rs`：四带骨架（`bands`/`Bands`/带常量）、`FIELD_GAP_Y` 及表单视觉项现在无生产者，统一标 `#[allow(dead_code)]`
    （保留为 UI_MASTER_PLAN 的统一版面模型 + 单测守卫）。
  - **顺手修乱码**：`game-core/src/world_ser.rs` 头部注释等 **5 行**是 GBK 误存的 mojibake（另有 1 行），已还原为正确中文。
  - 门禁：全绿（client 36 / game-core 234 / net 39 / net-steam 9；steam client 42）。

### 段 3：旧建房键盘表单 + `steam_create_*` 缓冲（高风险）——**已评估，待下轮实施**

#### 目标
把「建房」彻底收编到**统一设置编辑器**：删掉不可达的旧表单、`steam_create_*` 镜像字段/缓冲，
让建房只读 `room_meta` + `match_cfg`。

#### 现状事实（已核实）
1. **进入建房**：`steam_lobby_act(0)` 设 `steam_lobby_create=true`、`room_cfg_create_mode=true`、`room_cfg_edit=true`，
   并把 `steam_create_*` 全部填默认，再从 `steam_create_name/note` 写 `room_meta`。
2. **绘制**：`draw_menu` 的 CREATE-BRANCH 只要 `steam_lobby_create` 就画 `draw_room_cfg_editor`（与 `room_cfg_edit` 无关）。
3. **输入**：`steam_lobby_create_update` 顶部用 `O` **切换** `room_cfg_edit`；仅当 `room_cfg_edit && !o_pressed`
   才调 `room_cfg_editor_input`；否则落入 **focus 0..7 的旧表单**（房名/备注/人数/轮数/准备/初始金/每轮金/名次）。
4. **确认**：`steam_create_confirm` 目前：`players/rounds` 取 `room_meta/match_cfg`；但
   `learn/starting_gold/gold_per_round/place` 仍取 **`steam_create_*` 缓冲**，再写回 `steam_create_*` 标量；
   且 `match_regen = steam_create_regen`、`match_mode = steam_create_mode`。
5. **host 建厅**：`finish_enter_steam_mode` Host 分支用 `steam_create_name/note/rounds/learn/starting_gold/
   gold_per_round/place/regen` 写大厅元数据，并把它们回写 `match_*`。

#### 已发现的问题（这一步不只是清死码，还要修 bug）
- **编辑器设置被旧缓冲覆盖**：在创建模式里改「经济/时长/名次/模式/回血」是改 `match_cfg`，
  但建房时 `steam_create_confirm` / `finish_enter_steam_mode` 又用 **`steam_create_*` 默认缓冲**覆写
  `match_mode/match_regen` 与大厅元数据 → **编辑器里改的这些可能不生效**（与注释“一律取 match_cfg”相矛盾）。
- **创建模式按 `O` 会把编辑器卡死**：顶部 O 切换把 `room_cfg_edit` 置 false，但 CREATE-BRANCH 仍画编辑器，
  而 `room_cfg_editor_input` 又因 `!o_pressed` 不被调用 → 屏幕有编辑器但不吃键（旧表单接键，但它已无绘制）。

#### 目标终态
- `steam_lobby_create_update` 缩为：
  ```rust
  fn steam_lobby_create_update(&mut self, ctx: &mut Context) {
      self.room_cfg_editor_input(ctx);   // 统一编辑器独占输入；其回车=建房
      if std::mem::take(&mut self.create_confirm_pending) { self.steam_create_confirm(ctx); }
  }
  ```
  （**去掉**顶层 O toggle 与 M/R/Q/焦点/字段表单；取消建房交给编辑器的 `Esc`/`O`，已在 `room_cfg_editor_input` 中实现。）
- `steam_create_confirm` 只读 `room_meta` + `match_cfg`，不读任何 `steam_create_*`。
- `finish_enter_steam_mode` Host 分支只读 `room_meta` + `match_cfg`（不再读 `steam_create_*`）。
- 删除 `steam_create_*` 字段/缓冲/标量、`Text::CreateName/CreateNote` 变体与 `text_focus`/`text_buffer_mut` 分支。

#### 迁移步骤（每步可独立编译/提交）
- **S3-1（修 bug，先做）✅ 已完成**：`steam_create_confirm` 改为：
  `players=room_meta.player_limit`、其余全部取 `match_cfg`（`total_rounds`/`between_rounds_time_secs`→learn、
  `starting_gold`/`gold_per_round`/`place_rewards`/`game_mode`/`base_regen`）；不再读任何 `steam_create_*_buf`。
  同时把房名/备注与上述标量回写 `steam_create_name/note/rounds/learn/starting_gold/gold_per_round/place`，
  使 `finish_enter_steam_mode`（仍读这些）自然拿到编辑器后的值——**本步不删字段**，风险可控。
- **S3-2 ✅ 已完成**：`steam_lobby_create_update` 瘦身为「委托 `room_cfg_editor_input` + 处理 `create_confirm_pending`」；
  去掉 O toggle、focus 0..7 表单、M/R/Q/Enter 分支（取消建房交给编辑器的 Esc/O）。源码扫描测试 `create_screen_delegates_only_to_the_editor` 守护。
- **S3-3 ✅ 已完成**：删 `steam_create_*` 全部字段/初始化、`TextField::CreateName/CreateNote` 及 `text_focus`/`text_buffer_mut` 分支；
  `finish_enter_steam_mode` 改读 `room_meta` + `match_*`；`steam_lobby_act(0)` 只设 `room_meta` 默认；
  `match_cfg` 初始化带上命令行设定（`--mode/--regen` 等，cfg 分 Steam/非 Steam）；删除 4 个无用常量
  （`STEAM_MIN/MAX_LEARN_SECS`、`STEAM_REGEN_CHOICES`、`STEAM_DEFAULT_PLACE_REWARD`）。
- **S3-4 ✅ 已完成**：更新 `HANDOVER.md`/本文档。

#### 风险与验证（待执行）
- **必须双机实测**：`H` 建房 → 编辑器里改经济/时长/名次/模式/回血 → 回车建房 → 看客户端收到的大厅元数据/`room_cfg` 是否一致。
- 建议抽一个**纯函数**（如 `build_match_cfg(room_meta, match_cfg) -> (u8, MatchConfig)`）并加单测，把“建房只读这两处”变成可测契约。
- 关注 `create_confirm_pending` 与 `steam_lobby_create` 的时序（编辑器回车置 pending → 回调 confirm）。
- 回归：`cargo test --workspace` + 两套 clippy；源码扫描测试（CREATE-BRANCH 仍应画 `draw_room_cfg_editor`，不受影响）。

#### 状态
- ☑ **S3-1~S3-4 全部完成**：段 3 结束。创建模式输入完全收编到统一设置编辑器；
  `steam_create_*` 字段/缓冲已全部删除；`steam_create_confirm`/`finish_enter_steam_mode` 只认 `room_meta`+`match_cfg`/`match_*`。
- ⚠️ **仍待双机实测**：`H` 建房 → 编辑器里改设置 → 回车建房 → 客户端收到的大厅参数应一致。

### 不予清理（有意保留）
- `keys::Screen` / `keymap()`：文档 + 表内守护。
- `ui::theme::warn` / `TITLE`：预留主题项。
- `BOTS`：预留 AI 测试常量。
- net 测试里 `pair()` 的几个 `unused_mut` 警告：测试代码，低价值。

## 记录
- 段 1 完成（2026-09-13）：见上。
- 段 2 完成（2026-09-13）：见上；并顺手修了 `world_ser.rs` 的 mojibake 注释。
- 段 3 评估完成（2026-09-13）：见上；**含一个真 bug（编辑器设置被旧缓冲覆盖）**。
- 段 3 **S3-1 完成**（2026-09-13）：`steam_create_confirm` 改读 `room_meta`+`match_cfg`，并把房名/备注与标量回写，
  修好了“编辑器改的经济/时长/名次/模式/回血/房名被建房默认值覆盖”。
- 段 3 **S3-2/S3-3/S3-4 完成**（2026-09-13）：旧建房键盘表单/`steam_create_*` 字段/`TextField::CreateName/CreateNote` 全部删除；
  `finish_enter_steam_mode` 改读 `room_meta`+`match_*`；删 4 个无用常量。段 3 结束。
  ⚠️ 待双机实测。
