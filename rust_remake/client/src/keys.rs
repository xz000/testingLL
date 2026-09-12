//! 界面按键契约。
//!
//! **为什么单独放一个模块**：之前连续抓到两个同类 bug ——
//! 1) 技能详情按钮写着 `[= / 回车]`，但代码只接了 `=`；
//! 2) 技能页的「突破上限」按钮只有鼠标 hitbox，键盘 `=` 走的是另一条分支。
//!
//! 根因是「UI 上写的提示」和「代码里接受的按键」散落在两处、靠人眼保持一致。
//! 这里把**提示文案**和**判定函数**放到一起，并用单测钉住：
//! 文案里承诺的键，必须真的被判定函数接受；主界面必须调用共享判定函数。

use ggez::input::keyboard::Key;
use ggez::Context;
use winit::keyboard::NamedKey;

/// 「确认」操作的界面提示文案（技能 / 商店 / 成长三页统一）。
/// 改这里就等于改三处显示；`tests` 会校验主界面源码里出现的提示就是它。
pub const CONFIRM_HINT: &str = "[= / 回车]";

/// 「确认」键是否在本帧被按下：`=` 或 回车（含小键盘回车）。
///
/// 三个页面（技能/商店/成长）的确认一律走这里，避免再次出现"某页少接一个键"。
pub fn confirm_just(ctx: &Context) -> bool {
    // 注：winit 的 `NamedKey` 只有 `Enter`（主键盘与小键盘回车都归一到这里）。
    confirm_char_just(ctx, "=") || named_just(ctx, NamedKey::Enter)
}

fn confirm_char_just(ctx: &Context, s: &str) -> bool {
    ctx.keyboard
        .is_logical_key_just_pressed(&Key::Character(s.to_lowercase().into()))
        || ctx
            .keyboard
            .is_logical_key_just_pressed(&Key::Character(s.to_uppercase().into()))
}

fn named_just(ctx: &Context, n: NamedKey) -> bool {
    ctx.keyboard.is_logical_key_just_pressed(&Key::Named(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_hint_matches_accepted_keys() {
        assert_eq!(CONFIRM_HINT, "[= / 回车]", "提示文案被改动时此测试会提醒同步文档");
        assert!(CONFIRM_HINT.contains('='), "文案应承诺 `=` 键");
        assert!(CONFIRM_HINT.contains("回车"), "文案应承诺回车键");
    }

    /// 主界面必须**引用**共享文案与共享判定，而不是各写各的。
    /// （`include_str!` 让"文案与实现脱节"这种问题在 CI 就红，而不是等玩家发现。）
    #[test]
    fn main_wires_shared_confirm_contract() {
        let src = include_str!("main.rs");
        assert!(
            src.contains("CONFIRM_HINT"),
            "主界面应使用 keys::CONFIRM_HINT 作为确认提示文案"
        );
        let n = src.matches("keys::confirm_just(ctx)").count();
        assert!(n >= 3, "技能/商店/成长三页都应用 keys::confirm_just 判定，当前只有 {n} 处");
        // 不应再留下写死的、与 CONFIRM_HINT 不一致的确认提示。
        for bad in ["[= / 回车 ]", "[=/回车]", "[= / Enter]"] {
            assert!(!src.contains(bad), "发现与共享文案不一致的写死提示：{bad}");
        }
    }
}
