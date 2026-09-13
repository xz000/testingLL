//! 版面骨架：把"四带网格"变成**可计算的函数**，而不是各界面手工摆坐标。
//!
//! 起因（`UI_MASTER_PLAN.md`）：多个界面元素互相压、提示被字段盖住、
//! 覆盖层层次靠"绘制先后顺序"维持 —— 根因是**没有统一的版面模型**。
//!
//! 本模块只做纯计算（给屏幕尺寸 → 各带矩形），因此可以**单测**：
//! 带之间不重叠、提示恒在最底、面板恒在屏内、行在内容带内均匀分布。

use ggez::graphics::{Color, Rect};

/// 屏幕四带（比例固定，随分辨率缩放）。
///
/// ```text
/// ┌──────────────┐ 0.00
/// │  标题带       │ 0.06 起
/// ├──────────────┤ 0.20
/// │  内容带       │ 0.22 起
/// │              │
/// ├──────────────┤ 0.84
/// │  状态带       │ 0.85 起（金币/徽章/警告）
/// ├──────────────┤ 0.89
/// │  （留白）      │
/// ├──────────────┤ 0.92
/// │  提示带       │ 最底：按键说明，**永不与内容重叠**
/// └──────────────┘ 0.98
/// ```
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Bands {
    pub title: Rect,
    pub content: Rect,
    pub status: Rect,
    pub hint: Rect,
}

const TITLE_TOP: f32 = 0.06;
const TITLE_BOTTOM: f32 = 0.20;
const CONTENT_TOP: f32 = 0.22;
const CONTENT_BOTTOM: f32 = 0.84;
const STATUS_TOP: f32 = 0.85;
const STATUS_BOTTOM: f32 = 0.89;
const HINT_TOP: f32 = 0.92;
const HINT_BOTTOM: f32 = 0.98;

// ═══════════ 统一视觉常量（建房表单与设置列表共用） ═══════════
//
// 目的：两个界面的**结构**不同（表单 vs 列表），但**视觉语言**必须一致 ——
// 行高、标签宽、输入框尺寸、选中/悬停/普通三态配色，全部取自这里。
// 此前各界面各自写 `Color::from_rgb(56, 66, 84)` 之类的字面量，改一处就会不一致。

/// 列表/表单行高（设置编辑器行、房间列表行）。
/// 注：旧建房/房间信息表单遗留；段 2 删掉最后一个表单界面后再一并移除。
#[allow(dead_code)]
pub const ROW_H: f32 = 28.0;
/// 表单输入框尺寸与标签宽（建房界面字段）。
#[allow(dead_code)]
pub const FIELD_BOX_H: f32 = 44.0;
#[allow(dead_code)]
pub const FIELD_BOX_W: f32 = 300.0;
#[allow(dead_code)]
pub const FIELD_LABEL_W: f32 = 140.0;
/// 表单字段的纵向间距（两列布局用）。
pub const FIELD_GAP_Y: f32 = 34.0;

/// 普通行/输入框底色。
pub fn bg_normal() -> Color {
    Color::from_rgb(28, 32, 42)
}
/// 悬停底色（鼠标在本行上）。
#[allow(dead_code)]
pub fn bg_hover() -> Color {
    Color::from_rgb(38, 44, 56)
}
/// 选中底色（键盘焦点在本行）。
pub fn bg_selected() -> Color {
    Color::from_rgb(56, 66, 84)
}
/// 选中描边（强调色）。
pub fn border_selected() -> Color {
    Color::from_rgb(255, 210, 120)
}
/// 悬停描边（弱化）。
#[allow(dead_code)]
pub fn border_hover() -> Color {
    Color::from_rgb(90, 104, 126)
}
/// 主按钮底色（如 [创建房间]）。
#[allow(dead_code)]
pub fn btn_primary() -> Color {
    Color::from_rgb(42, 74, 52)
}
/// 主按钮悬停。
#[allow(dead_code)]
pub fn btn_primary_hover() -> Color {
    Color::from_rgb(70, 120, 80)
}
/// 次按钮底色（如 [取消]）。
#[allow(dead_code)]
pub fn btn_secondary() -> Color {
    Color::from_rgb(56, 46, 46)
}
/// 次按钮悬停。
#[allow(dead_code)]
pub fn btn_secondary_hover() -> Color {
    Color::from_rgb(90, 70, 70)
}
/// 普通文字。
pub fn text_normal() -> Color {
    Color::from_rgb(215, 220, 232)
}
/// 强调文字（选中行标签）。
pub fn text_accent() -> Color {
    Color::from_rgb(255, 210, 120)
}
/// 自定义项/警示文字（徽章为"自定义 N 项"时）。
pub fn text_custom() -> Color {
    Color::from_rgb(255, 200, 90)
}
/// 提示/禁用文字。
pub fn text_dim() -> Color {
    Color::from_rgb(150, 160, 175)
}

/// 按屏幕尺寸算出四带。
pub fn bands(width: f32, height: f32) -> Bands {
    let r = |t: f32, b: f32| Rect::new(0.0, height * t, width, height * (b - t));
    Bands {
        title: r(TITLE_TOP, TITLE_BOTTOM),
        content: r(CONTENT_TOP, CONTENT_BOTTOM),
        status: r(STATUS_TOP, STATUS_BOTTOM),
        hint: r(HINT_TOP, HINT_BOTTOM),
    }
}

/// 屏幕内居中的面板（覆盖层用）；`w_frac`/`h_frac` 为占屏比，自动留边距。
pub fn centered_panel(width: f32, height: f32, w_frac: f32, h_frac: f32) -> Rect {
    let w = (width * w_frac).min(width - 32.0);
    let h = (height * h_frac).min(height - 32.0);
    Rect::new((width - w) / 2.0, (height - h) / 2.0, w, h)
}

/// 内容带内第 `i` 行（共 `n` 行，等分；行高不超出内容带）。
pub fn row_in(content: Rect, i: usize, n: usize) -> Rect {
    if n == 0 {
        return Rect::new(content.x, content.y, content.w, 0.0);
    }
    let h = content.h / n as f32;
    Rect::new(content.x, content.y + h * i as f32, content.w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 四带**互不重叠**且顺序正确 —— 这是"提示被字段压住"这类问题的结构性防线。
    #[test]
    fn bands_do_not_overlap_and_hint_is_last() {
        for (w, h) in [(1280.0, 720.0), (1920.0, 1080.0), (800.0, 600.0)] {
            let b = bands(w, h);
            assert!(b.title.y + b.title.h <= b.content.y, "标题带不能压到内容带");
            assert!(b.content.y + b.content.h <= b.status.y, "内容带不能压到状态带");
            assert!(b.status.y + b.status.h <= b.hint.y, "状态带不能压到提示带");
            assert!(b.hint.y + b.hint.h <= h + 0.01, "提示带必须在屏内");
            assert!(b.hint.h > 0.0 && b.content.h > b.hint.h, "内容带应显著大于提示带");
        }
    }

    /// 面板恒在屏内、居中、且不超出边距。
    #[test]
    fn centered_panel_stays_inside_screen() {
        for (w, h) in [(1280.0, 720.0), (800.0, 600.0)] {
            let p = centered_panel(w, h, 0.72, 0.76);
            assert!(p.x >= 0.0 && p.y >= 0.0);
            assert!(p.x + p.w <= w + 0.01 && p.y + p.h <= h + 0.01, "面板不能出屏");
            assert!((p.x - (w - p.w) / 2.0).abs() < 0.01, "应水平居中");
            assert!((p.y - (h - p.h) / 2.0).abs() < 0.01, "应垂直居中");
        }
    }

    /// 统一视觉常量：取值合理、三态配色互不相同（否则"选中/悬停"就看不出来）。
    #[test]
    fn shared_visual_constants_are_distinct() {
        assert!(ROW_H > 0.0 && FIELD_BOX_H >= ROW_H, "输入框不应比行更矮");
        assert!(FIELD_BOX_W > 0.0 && FIELD_LABEL_W > 0.0);
        let trio = [bg_normal(), bg_hover(), bg_selected()];
        for (i, a) in trio.iter().enumerate() {
            for b in &trio[i + 1..] {
                assert_ne!(a, b, "普通/悬停/选中底色必须可区分");
            }
        }
        assert_ne!(border_selected(), border_hover(), "选中与悬停描边应不同");
        assert_ne!(btn_primary(), btn_secondary(), "主/次按钮应可区分");
        assert_ne!(text_normal(), text_dim(), "正文与次要文字应可区分");
    }

    /// 行等分且都在内容带内；`n = 0` 不 panic。
    #[test]
    fn rows_fill_content_band() {
        let c = bands(1280.0, 720.0).content;
        let n = 9;
        let first = row_in(c, 0, n);
        let last = row_in(c, n - 1, n);
        assert!((first.y - c.y).abs() < 0.01, "首行应贴内容带顶");
        assert!(
            (last.y + last.h - (c.y + c.h)).abs() < 0.01,
            "末行应贴内容带底"
        );
        for i in 0..n {
            let r = row_in(c, i, n);
            assert!(r.y >= c.y - 0.01 && r.y + r.h <= c.y + c.h + 0.01, "行应在内容带内");
        }
        let _ = row_in(c, 0, 0); // 不 panic
    }
}
