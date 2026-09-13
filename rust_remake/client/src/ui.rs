//! 可复用 UI 原语（U3）：主题 / 文本排版 / 面板 / 可点行 / 命中登记 / 滚动工具。
//!
//! ggez 0.10 没有 UI 框架，本项目此前是纯手写立即模式：颜色/字号/坐标全是字面量，
//! 且「按钮」几何在多处重复硬编码（主菜单卡片就重复了三处，改一处漏两处）。
//! 本模块抽出最小原语，首个使用者是**学习/配置界面**，后续主菜单 / Steam 大厅可逐步迁移。
//!
//! ⚠ 重要：`main::draw_text` **忽略**其 `_centered` 参数、恒按 `center.x` 水平居中，
//! 因此无法做左对齐与列排版。本模块的 [`text_left`] 补上这一能力。
//! **新代码请优先使用本模块，不要再用旧的 `draw_text`。**

use ggez::graphics::{self, Canvas, Color, DrawMode, Mesh, Text, TextFragment};
use ggez::mint::{Point2, Vector2};
use ggez::{Context, GameResult};

// ---------- 逻辑分辨率与自适应缩放（E 方案） ----------
//
// 界面按固定设计空间 1280×720 排版；每屏创建 canvas 后调 [`set_design_coordinates`]，
// ggez 会把设计空间映射到窗口（非 16:9 时以对称外扩矩形做 letterbox，保持比例不变形）。
// 因此所有布局/字号/命中盒都直接用设计坐标，**不再用 `drawable_size()`**。
// 鼠标：`ctx.mouse.position()` 是**物理像素**（与 `drawable_size` 同基准），用 [`mouse_design`] 逆变换。

/// 逻辑设计宽度。
pub const UI_W: f32 = 1280.0;
/// 逻辑设计高度。
pub const UI_H: f32 = 720.0;

/// 窗口像素尺寸 → 屏坐标矩形（设计空间单位）。保证 16:9、居中、不变形。
pub fn design_rect(win_w: f32, win_h: f32) -> graphics::Rect {
    let ww = win_w.max(1.0);
    let wh = win_h.max(1.0);
    // 每窗口像素对应的设计单位数（保持等比）。
    let s = (ww / UI_W).min(wh / UI_H);
    let w = ww / s;
    let h = wh / s;
    graphics::Rect::new((UI_W - w) / 2.0, (UI_H - h) / 2.0, w, h)
}

/// 按当前窗口尺寸设置该 canvas 的屏坐标（即逻辑设计空间 + letterbox）。
pub fn set_design_coordinates(canvas: &mut Canvas, ctx: &Context) {
    let (ww, wh) = ctx.gfx.drawable_size();
    canvas.set_screen_coordinates(design_rect(ww, wh));
}

/// 鼠标位置：窗口物理像素 → 设计空间坐标（命中盒/射线均用这个）。
pub fn mouse_design(ctx: &Context) -> Point2<f32> {
    let (ww, wh) = ctx.gfx.drawable_size();
    let r = design_rect(ww, wh);
    let m = ctx.mouse.position();
    Point2 {
        x: r.x + m.x / ww.max(1.0) * r.w,
        y: r.y + m.y / wh.max(1.0) * r.h,
    }
}

/// 行底色：按选中/悬停优先级取色（技能/商店/大厅/设置共用）。
pub fn row_color(selected: bool, hover: bool) -> Color {
    if selected {
        theme::row_selected()
    } else if hover {
        theme::row_hover()
    } else {
        theme::row_bg()
    }
}

/// 铺一行底色（与技能/商店/大厅/设置一致）。返回该矩形，方便调用方登记命中盒。
pub fn paint_row(
    canvas: &mut Canvas,
    ctx: &Context,
    rect: graphics::Rect,
    selected: bool,
    hover: bool,
) -> GameResult<graphics::Rect> {
    let bg = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), rect, row_color(selected, hover))?;
    canvas.draw(&bg, graphics::DrawParam::new());
    Ok(rect)
}

/// 统一主题。颜色是**函数**而非 const：本 ggez 版本的 `Color::from_rgb*` 不是 const fn，
/// 无法用于常量初始化；尺寸类仍是 `const`。
pub mod theme {
    use ggez::graphics::Color;

    pub fn panel_bg() -> Color {
        Color::from_rgba(16, 19, 26, 236)
    }
    pub fn panel_border() -> Color {
        Color::from_rgb(70, 84, 110)
    }
    pub fn row_bg() -> Color {
        Color::from_rgba(30, 35, 46, 205)
    }
    pub fn row_hover() -> Color {
        Color::from_rgba(58, 68, 88, 225)
    }
    pub fn row_selected() -> Color {
        Color::from_rgba(98, 79, 34, 235)
    }
    pub fn row_disabled() -> Color {
        Color::from_rgba(26, 28, 34, 180)
    }

    pub fn text() -> Color {
        Color::from_rgb(220, 226, 238)
    }
    pub fn text_dim() -> Color {
        Color::from_rgb(150, 162, 184)
    }
    /// 强调色（金）：选中、标题、价格
    pub fn accent() -> Color {
        Color::from_rgb(255, 210, 120)
    }
    pub fn ok() -> Color {
        Color::from_rgb(130, 225, 150)
    }
    #[allow(dead_code)]
    pub fn warn() -> Color {
        Color::from_rgb(230, 120, 120)
    }

    /// 标准行高
    pub const ROW_H: f32 = 28.0;
    /// 面板内边距
    pub const PAD: f32 = 12.0;
    #[allow(dead_code)]
    pub const TITLE: f32 = 28.0;
    pub const BODY: f32 = 18.0;
    pub const SMALL: f32 = 15.0;
}

/// 行 / 控件的视觉状态。
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RowState {
    Normal,
    Hover,
    Selected,
    Disabled,
}

impl RowState {
    fn bg(self) -> Color {
        match self {
            RowState::Normal => theme::row_bg(),
            RowState::Hover => theme::row_hover(),
            RowState::Selected => theme::row_selected(),
            RowState::Disabled => theme::row_disabled(),
        }
    }

    /// 该状态下文字的默认颜色。
    pub fn text_color(self) -> Color {
        match self {
            RowState::Selected => theme::accent(),
            RowState::Disabled => theme::text_dim(),
            _ => theme::text(),
        }
    }
}

/// 文本绘制的公共部分；`centered` 为真时把 `x` 当作**中心**。
#[allow(clippy::too_many_arguments)]
fn draw_text_at(
    canvas: &mut Canvas,
    ctx: &Context,
    s: &str,
    size: f32,
    color: Color,
    x: f32,
    y: f32,
    centered: bool,
) -> GameResult {
    let fragment = TextFragment::new(s).color(color).scale(size).font("cjk".to_string());
    let mut t = Text::new(fragment);
    t.set_bounds(Vector2 { x: 4000.0, y: 200.0 });
    let x = if centered {
        let w = t.measure(&ctx.gfx)?.x;
        x - w / 2.0
    } else {
        x
    };
    canvas.draw(&t, graphics::DrawParam::new().dest(Point2 { x, y }));
    Ok(())
}

/// **左对齐**文本（左上角在 `(x, y)`）——列排版 / 列表项用这个。
pub fn text_left(
    canvas: &mut Canvas,
    ctx: &Context,
    s: &str,
    size: f32,
    color: Color,
    x: f32,
    y: f32,
) -> GameResult {
    draw_text_at(canvas, ctx, s, size, color, x, y, false)
}

/// 水平居中文本（`x` 为**中心**）——标题 / 居中提示用这个。
pub fn text_center(
    canvas: &mut Canvas,
    ctx: &Context,
    s: &str,
    size: f32,
    color: Color,
    center_x: f32,
    y: f32,
) -> GameResult {
    draw_text_at(canvas, ctx, s, size, color, center_x, y, true)
}

/// **自动换行的左对齐文本**（技能描述等长文案用）。
///
/// 按显示宽度估算切行：CJK 记 2 单位、ASCII 记 1 单位，`max_w / (size / 2)` 得每行容量。
/// 逐行调用 `text_left` 绘制，返回**下一行的 y**（调用方直接用它续排后续内容）。
#[allow(clippy::too_many_arguments)]
pub fn text_wrapped(
    canvas: &mut Canvas,
    ctx: &Context,
    s: &str,
    size: f32,
    color: Color,
    x: f32,
    y: f32,
    max_w: f32,
) -> GameResult<f32> {
    let capacity = ((max_w / (size * 0.5)).max(4.0)) as usize;
    let mut line = String::new();
    let mut width = 0usize;
    let mut yy = y;
    for ch in s.chars() {
        let w = if ch.is_ascii() { 1 } else { 2 };
        if width + w > capacity {
            text_left(canvas, ctx, &line, size, color, x, yy)?;
            yy += size + 4.0;
            line.clear();
            width = 0;
        }
        line.push(ch);
        width += w;
    }
    if !line.is_empty() {
        text_left(canvas, ctx, &line, size, color, x, yy)?;
        yy += size + 4.0;
    }
    Ok(yy)
}

/// 右对齐文本（**右下角**在 `(right, y)`）——角落信息 / 价格列用这个。
pub fn text_right(
    canvas: &mut Canvas,
    ctx: &Context,
    s: &str,
    size: f32,
    color: Color,
    right: f32,
    y: f32,
) -> GameResult {
    let fragment = TextFragment::new(s).color(color).scale(size).font("cjk".to_string());
    let mut t = Text::new(fragment);
    t.set_bounds(Vector2 { x: 4000.0, y: 200.0 });
    let w = t.measure(&ctx.gfx)?.x;
    // 让 baseline 与其它文本对齐：draw_text_at 的 y 是 baseline。
    canvas.draw(&t, graphics::DrawParam::new().dest(Point2 { x: right - w, y }));
    Ok(())
}

/// 面板：填充底 + 边框 + 可选标题（标题画在面板顶部内侧）。
pub fn panel(
    canvas: &mut Canvas,
    ctx: &Context,
    r: graphics::Rect,
    title: Option<&str>,
) -> GameResult {
    let fill = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), r, theme::panel_bg())?;
    canvas.draw(&fill, graphics::DrawParam::new());
    let border = Mesh::new_rectangle(&ctx.gfx, DrawMode::stroke(1.5), r, theme::panel_border())?;
    canvas.draw(&border, graphics::DrawParam::new());
    if let Some(t) = title {
        text_left(canvas, ctx, t, theme::BODY, theme::accent(), r.x + theme::PAD, r.y + 8.0)?;
    }
    Ok(())
}

/// 可点行：带可见背景与边框（**不再是无形的 hitbox**），文字左对齐内缩 `PAD`。
///
/// 调用方负责在绘制后用 [`HitRegistry::push`] 登记同一矩形。
pub fn row(
    canvas: &mut Canvas,
    ctx: &Context,
    r: graphics::Rect,
    label: &str,
    size: f32,
    state: RowState,
) -> GameResult {
    let fill = Mesh::new_rectangle(&ctx.gfx, DrawMode::fill(), r, state.bg())?;
    canvas.draw(&fill, graphics::DrawParam::new());
    let border_color = if state == RowState::Selected {
        theme::accent()
    } else if state == RowState::Hover {
        theme::text_dim()
    } else {
        theme::panel_border()
    };
    let border = Mesh::new_rectangle(&ctx.gfx, DrawMode::stroke(1.0), r, border_color)?;
    canvas.draw(&border, graphics::DrawParam::new());
    text_left(
        canvas,
        ctx,
        label,
        size,
        state.text_color(),
        r.x + theme::PAD,
        r.y + (r.h - size) / 2.0 - 2.0,
    )
}

/// 命中登记（泛型化的 `learn_hitboxes`）：**绘制时 `push` 矩形，`update` 里 `hits_at` 派发**。
///
/// 沿用现有「绘制注册 → update 派发」的立即模式（有 1 帧延迟，实测可忽略）。
#[derive(Default)]
pub struct HitRegistry<A> {
    hits: Vec<(graphics::Rect, A)>,
}

impl<A: Copy> HitRegistry<A> {
    pub fn new() -> Self {
        Self { hits: Vec::new() }
    }

    /// 每帧绘制开头清空。
    pub fn clear(&mut self) {
        self.hits.clear();
    }

    /// 登记一个可点区域（与旧 `Vec<(Rect, A)>` 的 `push` 同形，便于迁移）。
    pub fn push(&mut self, hit: (graphics::Rect, A)) {
        self.hits.push(hit);
    }

    /// 返回鼠标位置命中的全部动作（按登记顺序）。
    pub fn hits_at(&self, p: Point2<f32>) -> Vec<A> {
        self.hits
            .iter()
            .filter(|(r, _)| r.contains(Point2 { x: p.x, y: p.y }))
            .map(|(_, a)| *a)
            .collect()
    }
}

/// 把滚动偏移钳制到合法范围（内容不足一屏时为 0）。
pub fn clamp_scroll(len: usize, visible: usize, offset: usize) -> usize {
    if len <= visible {
        0
    } else {
        offset.min(len - visible)
    }
}

/// 当前可见区间 `[start, end)`（已钳制偏移）。
pub fn scroll_window(len: usize, visible: usize, offset: usize) -> (usize, usize) {
    let start = clamp_scroll(len, visible, offset);
    let end = (start + visible).min(len);
    (start, end)
}
