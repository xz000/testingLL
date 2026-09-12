//! 版面骨架：把"四带网格"变成**可计算的函数**，而不是各界面手工摆坐标。
//!
//! 起因（`UI_MASTER_PLAN.md`）：多个界面元素互相压、提示被字段盖住、
//! 覆盖层层次靠"绘制先后顺序"维持 —— 根因是**没有统一的版面模型**。
//!
//! 本模块只做纯计算（给屏幕尺寸 → 各带矩形），因此可以**单测**：
//! 带之间不重叠、提示恒在最底、面板恒在屏内、行在内容带内均匀分布。

use ggez::graphics::Rect;

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
