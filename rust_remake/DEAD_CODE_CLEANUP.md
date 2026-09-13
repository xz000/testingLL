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

### 段 3：旧建房键盘表单（高风险，待评估）
`steam_lobby_create_update` 中 focus 0..7 的字段输入分支（房名/备注/人数/轮数/…），
正常情况下 `room_cfg_edit` 恒为真、该分支不可达；但与 `steam_create_confirm` 读的缓冲耦合。
- **建议**：先做段 1/2，段 3 单独评估后再动（或干脆保留）。
- **状态**：⬜ 未评估

### 不予清理（有意保留）
- `keys::Screen` / `keymap()`：文档 + 表内守护。
- `ui::theme::warn` / `TITLE`：预留主题项。
- `BOTS`：预留 AI 测试常量。
- net 测试里 `pair()` 的几个 `unused_mut` 警告：测试代码，低价值。

## 记录
- 段 1 完成（2026-09-13）：见上。
- 段 2 完成（2026-09-13）：见上；并顺手修了 `world_ser.rs` 的 mojibake 注释。
- 下一步：段 3（旧建房键盘表单）待评估；或其他主线待办。
