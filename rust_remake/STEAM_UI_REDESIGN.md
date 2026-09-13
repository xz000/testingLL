# Steam 大厅界面改造（对齐技能/商店风格）

> 目标：把「选择 Steam 联机 → 进入房间之前」的几个界面，改成与**技能/商店页**一致的风格与交互。
> 本文只做规划；**待确认后逐步实施**。关联：`ROOM_UI_REVIEW.md`、`SETTINGS_ALIGNMENT_AUDIT.md`。
> 代码位置：`client/src/main.rs`（绘制 `draw_menu` / `draw_steam_lobby_list` / `draw_steam_connecting`）、
> `client/src/ui.rs`（`theme`/`text_*`/`HitRegistry`）、`client/src/layout.rs`。

---

## 0. 风格构件（技能/商店页现用，直接复用）
- **遮罩**：全屏 `Color::from_rgba(8, 10, 16, 210)`（学习/商店页）。
- **文字**：`ui::text_center` / `ui::text_left` / `ui::text_right`。
- **颜色**：`ui::theme::{accent, ok, text, text_dim, row_bg, row_hover, row_selected, PAD}`。
- **面板几何**：`panel_y = sh * 0.20`，`panel_h = sh * 0.74`；左摘要 + 右内容分栏（`left_w ≈ sw*0.26`）。
- **页签**：顶部居中一行，`tab_w=150`，激活= `row_selected`+`accent`，悬停= `row_hover`，普通= `row_bg`。
- **鼠标命中**：`ui::HitRegistry<Action>`（`clear()` → 绘制时 `push((rect, action))` → 事件时 `hits_at(mouse)`）。
  现有 `learn_hitboxes: HitRegistry<LearnAction>` 即此模式；大厅可新增一个 `lobby_hitboxes`。
- **底部提示**：一行 18px 蓝灰（如 `回车 加入    R 刷新    Q 返回`）。

## 1. 要改造的界面（现状 → 目标）

### 1.1 大厅主界面（创建 / 加入 / 返回）`draw_menu` 的 Steam 分支
- **现状**：自绘三张卡片（96px 高），与主菜单卡片风格；无鼠标命中登记（在 input 里另算矩形）。
- **目标**：与技能/商店一致的**行列表**：遮罩 + 标题 `Steam 对战` + 三行（`row_bg/row_selected`，悬停高亮）+ 底部提示；
  行左侧动作名、右侧一句话说明；错误信息用 `theme` 的警告色显示在列表下方。
- **输入**：沿用现有 `steam_lobby_act(0/1/2)`；鼠标命中改走 `lobby_hitboxes`（与键盘 `steam_lobby_selection` 同步）。

### 1.2 房间列表 `draw_steam_lobby_list`
- **现状**：居中标题 + 每行 64px 自绘条（`[v]` + 房主/房名 + meta），无右侧详情、无滚动、无鼠标点击。
- **目标**（最想完善的界面）：
  - 顶部：标题 `加入房间` + 右侧刷新/筛选状态（`剩余…`/`搜索中…`/`模式：全部`）。
  - 主体：**可滚动行列表**（每行：房主昵称 · 房名 · `n/上限` · 模式 · `自定义 N 项` · 版本），
    选中 `row_selected`、悬停 `row_hover`；行数多时按 `visible` 裁剪 + 滚动（参考商店 `shop_scroll`）。
  - 右侧（可选）：选中房间的**详情区**（房名/备注/模式/人数/版本/自定义项明细），空列表时显示占位说明。
  - 底部：`回车 加入    R 刷新    F 筛选    Q 返回` + 错误行（`steam_lobby_error`）。
- **输入**：`↑/↓` 选择、`回车` 加入、`R` 刷新、`F` 筛选、`Q` 返回；新增鼠标：单击行=选中、双击/单击=加入（与键盘一致）。

### 1.3 连接中 `draw_steam_connecting`
- **现状**：居中标题 + 状态 + 转圈 + 已等待 + 取消提示（已接近目标）。
- **目标**：换成 `ui::theme` 颜色/字号，包一个居中面板（与商店一致的圆角/边距观感），保留转圈与等待秒数。

### 1.4 建房界面
- 已统一为设置编辑器（`draw_room_cfg_editor`），**本改造不改**，只保证从大厅进入时的过渡一致。

## 2. 共享实现建议
- 新增 `enum LobbyAction { Create, Join, Back, Row(usize), Refresh, Filter, Cancel }`（按界面细分也可）。
- 新增字段 `lobby_hitboxes: ui::HitRegistry<LobbyAction>`（steam-only），在各 `draw_*` 里 `clear()`+`push()`；
  找到命中后在 `update` 的对应分支里执行（与现在键盘动作同路径，保证一致）。
- 抽一个**行绘制小工具**（如 `draw_row(canvas, ctx, rect, selected, hover, main, sub, colors)`），
  供大厅菜单/房间列表复用，避免再次各写各的矩形/颜色。
- 滚动：复用商店的 `visible`/`scroll`/`selection` 三件套模式。

## 3. 分步实施（每步可编译/提交）
- [x] **U1 房间列表 ✅ 已完成**：改用主题风格（遮罩 + `ui::theme` 行背景/悬停/选中 + `ui::text_*`）；
  左列表可滚动（`steam_list_scroll`，可见 9 行）+ 列头 + 悬停高亮；右侧「房间详情」（房名/房主/人数/模式/版本/备注）；
  底部「刷新/筛选/返回」可点按钮；新增 `lobby_hitboxes`（与键盘同一动作路径）+ 鼠标点行=选中并加入（抽 `try_join_selected_lobby`）。
- [x] **U2 大厅主界面 ✅ 已完成**：改为主题行列表（创建/加入/返回）+ `ui::paint_row` 底色 + 悬停/选中高亮 + 右侧快捷键标签；
  鼠标命中改走 `lobby_hitboxes`（新增 `LobbyListAction::{MenuCreate,MenuJoin,MenuBack}`），与键盘 `H/J/Q/回车/空格` 共用 `steam_lobby_act`；错误行在列表下方。
- [x] **U3 连接中 ✅ 已完成**：套 `layout::centered_panel` 居中面板 + `ui::theme` 配色/字号；保留转圈与等待秒数。
- [x] **U4 收尾 ✅ 已完成**：新增共享 `ui::paint_row` / `ui::row_color`（房间列表 / 大厅菜单 / 底部按钮共用）；`keys::keymap` 的 `SteamMenu` 补上 ↑/↓/回车。

## 4. 风险 / 注意
- 输入既有键盘又有鼠标：需保证“鼠标点击”和“键盘回车”走**同一条动作路径**（避免两套逻辑分叉）。
- 列表可能为空/搜索中/筛选无结果：三种占位文案要与现状一致（已经有）。
- 版本不符/满员的行不可加入：保留现有红色标注与拒绝逻辑。
- 不改变任何网络/协议行为，纯呈现层。

## 5. 记录
- 2026-09-13：初版规划（未改代码）。
- 2026-09-13：**U1 房间列表已完成**（主题化 + 滚动 + 悬停 + 右详情 + 鼠标可点；`LobbyListAction`/`lobby_hitboxes`/`steam_list_scroll`）。
  `layout::bg_selected`/`text_normal` 暂无人用 → 标 `#[allow(dead_code)]`（与同文件其他共享项一致）。
- 2026-09-13：**U2/U3/U4 已完成**（大厅主界面行列表 + 鼠标；连接中面板；共享 `ui::paint_row`）。
- 2026-09-13：**设置编辑器鼠标支持**（此前只能键盘）：`draw_room_cfg_editor` 登记 `room_cfg_hitboxes`
  （`RoomCfgAction::{Group,Row,Close}`，行/页签有 hover），`room_cfg_editor_input` 派发；点行=`mouse_activate`
  （行级激活，**不**等同建房回车），点「关闭」= Esc/O；房内顶部「房间设置」徐章也可点击打开。
  新增源码级回归测试 `settings_editor_supports_mouse`。
- 2026-09-13：**编辑器两模式关闭语义区分**（重要修正）：
  - 建房模式：右下 `[创建房间]` / `[取消]` 按钮可鼠标（此前鼠标无法建房）；`RoomCfgAction::Build`。
  - 房内模式：`[保存]`（同 `O`，发布）与 `[不保存]`（同 `Esc`，**回滚**新增的 `room_cfg_snapshot`，不发布）；只读客户端只有 `[关闭]`。
  - 修复源码扫描测试在 **CRLF** 文件上 `find("\n    }")` 永不匹配 → `unwrap_or(剩余全文)` 导致的**假通过**：
    新增 `fn_body()`（兼容 CRLF/LF + 找不到闭合则 panic），修正全部 7 处函数体提取。
  - 新增回归测试 `editor_offers_save_and_discard`。
