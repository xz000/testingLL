# 显示 / 相机 设置 规划（DISPLAY_CAMERA_PLAN.md）

> 目标（用户 2026-10-11）：**分辨率**、**全屏模式**、**全屏下鼠标贴边的镜头移动（边缘滚屏）**、**相机平移速度设置**。
> **状态（2026-10-11）：S1–S4 全部实施完成**（`d0a6675` S1 / `55e2919` S2 / `7c35928` S3 / `f7a10d2` S4），client 124 单测全绿。真机手感验收待做。
>
> 本文只做规划。关联：`HANDOFF.md` · `DOCS_INDEX.md`。
> 阅读：§0 现状 → §1 事实（ggez API）→ §2 决策 → §3 实施顺序 → §4 改动点 → §5 测试 → §6 风险。

---

## 0. 现状审计（代码事实）

| 项 | 现状 | 位置 |
|---|---|---|
| 窗口 | **固定 `1280×720`**、`resizable(true)`；**无**分辨率/全屏设置 | `client/src/main.rs` `ContextBuilder`（~12380） |
| 渲染 | **固定逻辑分辨率 + letterbox**（`ui::UI_W/UI_H`），窗口变化只缩放画面 | `ui` 模块 |
| 缩放 | 滚轮缩放 `zoom`（`ZOOM_MIN/ZOOM_MAX`）+ 光标锚点 | `main.rs` ~1860–1890 |
| 相机平移 | **仅键盘方向键**，速度 = **常量** `PAN_SPEED: f32 = 1.1`（屏幕对角/秒） | `main.rs` 1856 / 1911 |
| 边缘滚屏 | ❌ 无 | — |
| 设置界面 | `Screen::Settings`（`SetRow`）：音量/静音/语言/音频包/图标包/工坊/发布/按键 —— **无显示类行** | `main.rs` ~547 |

---

## 1. 事实：ggez 0.10 能做什么（**已查源码**）

`ggez-0.10.0/src/graphics/context.rs` / `conf.rs`：

| API | 作用 |
|---|---|
| `ctx.gfx.set_fullscreen(FullscreenType)` | 运行时切全屏（内部转 `set_mode`，**失败自动回退**）|
| `ctx.gfx.set_mode(WindowMode)` | 设 尺寸/全屏/可缩放/无边框… |
| `ctx.gfx.set_resizable(bool)` / `set_window_position` / `set_drawable_size` | — |
| `ctx.gfx.window()` → `&winit::window::Window` | 取 `inner_size()` / `current_monitor()` / `available_monitors()` / `scale_factor()` |

`FullscreenType`（**三种**，非两种）：
| 值 | 映射 | 约束 |
|---|---|---|
| `Windowed` | `set_fullscreen(None)` + 装饰/尺寸/resizable | 无 |
| **`Desktop`** | `Fullscreen::Borderless(None)` | **无**（用**当前显示器原生分辨率**，`mode.dimensions` 被忽略）；多显示器友好；ggez 推荐 |
| **`True`** | `Fullscreen::Exclusive(video_mode)` | **硬约束**：必须能在 `current_monitor().video_modes()` 里找到**完全匹配** `mode.width×height` 的模式，否则 ggez **返回 `Err`（"resolution … is not supported"）**；多显示器/alt-tab 体验较差 |

> ⚠ `set_mode` 对 `WindowError` 会**回退**到旧 `fullscreen_type`（即独占切换失败不会崩，但会静默失败）——实现时要主动读取结果/回退。

**⇒ "两种全屏难度差别"**：**API 难度相同**（都是一个枚举值）；差别在**约束**——
`Desktop` 零约束、省事稳；`True` 要**额外枚举 `video_modes()` 得到合法分辨率列表**、且**只在用户从合法列表里选**时才尝试，失败要回退。**独占是"附加项"，无边框是"默认"。**

---

## 2. 决策

| # | 问题 | 决策 | 说明 |
|---|---|---|---|
| **D1** | "分辨率" = 什么？ | **窗口/后台缓冲尺寸**（渲染仍是固定逻辑分辨率 + letterbox） | 与现有 letterbox 架构一致，改动最小 |
| **D2** | 分辨率**自适应 + 全屏跟随**？ | ✅ **都做**：窗口化给预设 **+「自适应」**（按显示器工作区比例选一档不超出的尺寸）；全屏下分辨率行显示 **"跟随显示器"并禁用** | 见 D3 |
| **D3** | 全屏类型 | 默认 **`Desktop`（无边框全屏）**；附加 **`True`（独占）**（仅枚举 `video_modes()` 合法档） | D2 的"跟随显示器"= Desktop；True 时可选档 |
| **D4** | 边缘滚屏 | **玩家开关**（`edge_scroll: bool`）+ 边距 + 速度；**窗口化也生效**（**不区分全屏/窗口**，由玩家自己开） | ✅ 采纳你的想法 |
| **D5** | 相机平移速度 | `cam_pan_speed`（倍率 **0.25×–3.0×**，默认 1.0），作用于**方向键平移**（也作用于边缘滚屏）| |

> 关于 D2「自适应」：本项目**画面本身已是自适应**（逻辑分辨率 + letterbox，任意窗口都不变形）；此处的"自适应分辨率"仅指**窗口尺寸**档（等价"最大化/适配工作区"）。

---

## 3. 实施顺序（每步 1 commit）

> 三步共享同一骨架（`LocalSettings` 加字段 → 设置界面加"显示"组行 → 启动时应用 → 运行时应用）。

| 步 | 内容 | 状态 |
|---|---|---|
| **S1** | **相机平移速度设置**：`cam_pan_speed` + 设置行 + `PAN_SPEED * cam_pan_speed`（也作用于边缘滚屏） | ✅ `d0a6675` |
| **S2** | **分辨率**：`window_resolution`（`1280×720/1600×900/1920×1080/Adaptive`）；建窗 `WindowMode.dimensions` + 运行时 `set_mode`；自适应 按显示器**物理尺寸**算 16:9 | ✅ `55e2919` |
| **S3** | **全屏**：`fullscreen`（`Windowed/Borderless/Exclusive`）；全屏时分辨率行→"跟随显示器"；`Exclusive` 以显示器**物理尺寸**匹配 video mode，失败回退无边框 | ✅ `7c35928` |
| **S4** | **边缘滚屏**：`edge_scroll` 开关（窗口化也生效）；鼠标入 `EDGE_SCROLL_MARGIN=8` 设计单位持续平移（复用 `cam_pan_speed`）| ✅ `f7a10d2` |

> 实际实现与规划差异：① `edge_margin`/`edge_speed` 未做成设置项，边距用常量 8、速度复用 `cam_pan_speed`（后续想加再说）。② 自适应尺寸用显示器**物理**像素（`WindowMode.dimensions` 即物理像素，未除 `scale_factor`）。
>
> **Steam 云存档范围**：`cam_pan_speed` + `edge_scroll`（纯偏好）**入云**；`window_resolution` + `fullscreen`（机器相关）**仅本地**。云文件 `settings_cloud.txt`（旧 `keybinds.txt` 自动迁移）。

---

## 4. 改动点

- **`client/src/local_settings.rs`**：新增
  - `window_resolution: Resolution { W1280x720, W1600x900, W1920x1080, Adaptive }`
  - `fullscreen: FullscreenPref { Windowed, Borderless, Exclusive }`
  - `edge_scroll: bool`、`edge_margin: f32`（默认 ~8px）、`cam_pan_speed: f32`（默认 1.0，夹 [0.25,3.0]）
  （沿用现有 `LocalSettings` 的读写/roundtrip/clamp 风格 + 对应单测）
- **`client/src/main.rs`**：
  - `ContextBuilder.window_mode(...)` 按设置（尺寸 + 全屏 + 可缩放）。
  - `Screen::Settings` 新增 **"显示"** 组（`SetRow::{Resolution, Fullscreen, EdgeScroll, CamPanSpeed}`）——`label/value/activate/adjust` 四处都要加（照现有音量行抄）。
  - 应用：`apply_display_settings(&mut ctx)`（`set_mode`/`set_fullscreen`）；启动时一次、改设置时一次。
  - 相机更新（~1911）：`let pan = PAN_SPEED * self.local_settings.cam_pan_speed;` + 边缘滚屏分支（鼠标在边距内 → 累加方向）。
  - i18n 文案（中英）。
- **不改 `game-core` / 协议 / 世界** → **不升 `PROTOCOL_VERSION`**（纯客户端本地设置）。

---

## 5. 测试

- **纯函数单测**（项目风格）：
  - `resolution_dimensions(Res, monitor) -> (w,h)`（含 Adaptive）。
  - `edge_scroll_dir(mouse, w, h, margin) -> (dx,dy)`（边界/角落/中心不滚）。
  - `cam_pan_speed` 夹紧（0.25–3.0）。
  - `fullscreen` 偏好 ↔ `FullscreenType` 映射。
- **真机**：切 窗口/无边框/独占；切各分辨率 + 自适应；开边缘滚屏手感（窗口 + 全屏）；调平移速度。

---

## 6. 风险与边界

1. **切模式后要重算 letterbox/坐标**：`set_mode` 会清屏/重置 viewport → 切完调 `set_screen_coordinates`（或让现有 `resize_event` 走一遍）。
2. **`Exclusive` 不支持的分辨率**：ggez 返回 `Err` 并回退 → UI 要提示"该分辨率不被显示器支持"，且**只列 `video_modes()` 合法档**。
3. **多显示器**：`Borderless`/`Exclusive` 默认用 `current_monitor()`；跨显示器拖窗口后切全屏需以**当时所在显示器**为准。
4. **全屏下 UI 命中**：现有全屏命中用**逻辑坐标**（`ui::mouse_design`）→ 应天然正确，但需真机复核。
5. **设置持久化**：与 `local_settings` 同文件，注意旧文件缺字段时的默认值（现有 `legacy_settings_without_new_keys_keeps_defaults` 测试应扩展覆盖新字段）。

---

## 7. 与"离开/归队"的关系
两者**完全独立**（本文件纯客户端显示/输入；`LEAVE_REJOIN_PLAN.md` 纯联机会话）。可**并行**，但建议**先把显示三步做掉**（低风险、立即可用），再做联机那三步（高风险、需双账号真机）。
