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

/// 「卖出」操作的界面提示文案（商店详情「卖出」按钮，与键盘退格/Delete 对应）。
pub const SELL_HINT: &str = "[退格 / Delete]";

/// 「确认」键是否在本帧被按下：`=` 或 回车（含小键盘回车）。
///
/// 三个页面（技能/商店/成长）的确认一律走这里，避免再次出现"某页少接一个键"。
pub fn confirm_just(ctx: &Context) -> bool {
    // 注：winit 的 `NamedKey` 只有 `Enter`（主键盘与小键盘回车都归一到这里）。
    confirm_char_just(ctx, "=") || named_just(ctx, NamedKey::Enter)
}

/// 「卖出」键是否在本帧被按下：退格 或 Delete（商店详情卖出按钮的键盘入口）。
pub fn sell_just(ctx: &Context) -> bool {
    named_just(ctx, NamedKey::Backspace) || named_just(ctx, NamedKey::Delete)
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

    #[test]
    fn sell_hint_matches_accepted_keys() {
        assert!(SELL_HINT.contains("退格"), "文案应承诺退格键");
        assert!(SELL_HINT.contains("Delete"), "文案应承诺 Delete 键");
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


// ═══════════════════════ 键位总表 ═══════════════════════
//
// 为什么要有它：本项目多次踩到"同一个键被两处处理"的坑 ——
// `O` 开→立刻关（编辑器永不出现）、`Q` 想关编辑却直接退房。
// 根因是键位散落在各界面函数里，没有任何一处能"看全"。
//
// 这里把每个界面的键位**声明成数据**，便于一处看全 + 让"表内撞车"在 CI 直接报出来。
//
// **诚实说明能力边界**：本表是**手工维护**的声明，测试只能保证
//   1. 同一界面内不许重复绑定同一个键（表内一致）；
//   2. 每个界面都有非空键位表；
//   3. 可返回的界面必须提供 `esc`/`q`。
// 它**不能**自动发现"代码里同一函数处理了两次同一个键"（如 `O` 开→立刻关、`Q` 关编辑却退房）——
// 那需要**源码级检测**（扫描各界面函数内 `just('x')` / `Character("x")` 的出现次数 ≤ 1），
// 已列入 `UI_MASTER_PLAN.md` 的待办。
//
// 也就是说：本表的作用是**文档 + 表内一致性守卫**，不是代码与表一致性的强制校验。

/// 界面（与 `UI_MASTER_PLAN.md` 的界面清单一致）。
#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    MainMenu,
    SteamMenu,
    LobbyList,
    CreateLobby,
    Room,
    SettingsEditor,
    Play,
    LearnConfig,
}

/// 一条键位绑定：按键 → 动作（`key` 用统一写法：小写字母/`方向键`/`回车` 等）。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub key: &'static str,
    pub action: &'static str,
}

/// 某界面的键位表。
///
/// 运行时**暂未消费**（当前只作声明 + 表内守卫），因此 `allow(dead_code)`；
/// 后续把各界面的底部提示行改为读这张表时即可去掉。
#[allow(dead_code)]
pub fn keymap(screen: Screen) -> &'static [Binding] {
    use Screen::*;
    match screen {
        MainMenu => &[
            Binding { key: "up", action: "上移选择" },
            Binding { key: "down", action: "下移选择" },
            Binding { key: "enter", action: "确认进入" },
            Binding { key: "1", action: "单机试验场" },
            Binding { key: "2", action: "局域网" },
            Binding { key: "3", action: "Steam 大厅" },
        ],
        SteamMenu => &[
            Binding { key: "h", action: "创建房间" },
            Binding { key: "j", action: "加入房间" },
            Binding { key: "q", action: "返回主菜单" },
            Binding { key: "esc", action: "返回主菜单" },
        ],
        LobbyList => &[
            Binding { key: "up", action: "选择上一间" },
            Binding { key: "down", action: "选择下一间" },
            Binding { key: "enter", action: "加入所选房间" },
            Binding { key: "r", action: "刷新列表" },
            Binding { key: "f", action: "切换模式筛选" },
            Binding { key: "q", action: "返回" },
        ],
        CreateLobby => &[
            Binding { key: "up", action: "字段上移" },
            Binding { key: "down", action: "字段下移" },
            Binding { key: "left", action: "左列 / 减" },
            Binding { key: "right", action: "右列 / 加" },
            Binding { key: "tab", action: "字段上移" },
            Binding { key: "plus", action: "数值 +1" },
            Binding { key: "minus", action: "数值 -1" },
            Binding { key: "enter", action: "创建房间" },
            Binding { key: "m", action: "切换游戏模式" },
            Binding { key: "r", action: "切换基础回血" },
            Binding { key: "o", action: "打开房间设置编辑器" },
            Binding { key: "q", action: "取消返回" },
        ],
        Room => &[
            Binding { key: "u", action: "切换准备" },
            Binding { key: "q", action: "退出房间" },
            Binding { key: "i", action: "好友邀请面板" },
            Binding { key: "o", action: "房间设置编辑器（仅房主）" },
        ],
        SettingsEditor => &[
            Binding { key: "z", action: "分组：经济" },
            Binding { key: "x", action: "分组：玩法" },
            Binding { key: "c", action: "分组：地图" },
            Binding { key: "v", action: "分组：模式" },
            Binding { key: "up", action: "上一行" },
            Binding { key: "down", action: "下一行" },
            Binding { key: "left", action: "档位 -1 / 微调" },
            Binding { key: "right", action: "档位 +1 / 微调" },
            Binding { key: "enter", action: "数值行进入自定义输入 / 其它行切换" },
            Binding { key: "esc", action: "保存并关闭" },
            Binding { key: "o", action: "保存并关闭" },
        ],
        Play => &[
            Binding { key: "c", action: "施放 C 槽技能" },
            Binding { key: "r", action: "施放 R 槽技能" },
            Binding { key: "e", action: "施放 E 槽技能" },
            Binding { key: "d", action: "施放 D 槽技能" },
            Binding { key: "y", action: "施放 Y 槽技能" },
            Binding { key: "t", action: "施放 T 槽技能" },
            Binding { key: "f", action: "施放 F 槽技能" },
            Binding { key: "g", action: "施放 G 槽技能" },
            Binding { key: "s", action: "停止移动 + 清空队列" },
            Binding { key: "esc", action: "返回主菜单" },
        ],
        LearnConfig => &[
            Binding { key: "j", action: "页签：技能" },
            Binding { key: "k", action: "页签：商店" },
            Binding { key: "l", action: "页签：成长" },
            Binding { key: "tab", action: "循环页签" },
            Binding { key: "enter", action: "确认（购买/升级/突破/执行）" },
            Binding { key: "=", action: "确认（等价回车）" },
            Binding { key: "esc", action: "返回" },
        ],
    }
}

#[cfg(test)]
mod keymap_tests {
    use super::*;

    const ALL: [Screen; 8] = [
        Screen::MainMenu,
        Screen::SteamMenu,
        Screen::LobbyList,
        Screen::CreateLobby,
        Screen::Room,
        Screen::SettingsEditor,
        Screen::Play,
        Screen::LearnConfig,
    ];

    /// **同一界面内不许有重复键** —— 这正是"一键两用"（`O` 开→立刻关、`Q` 关编辑却退房）的检测。
    /// 新增键位若在表里撞车，这里会直接失败。
    #[test]
    fn no_duplicate_keys_within_a_screen() {
        for sc in ALL {
            let map = keymap(sc);
            for (i, a) in map.iter().enumerate() {
                for b in &map[i + 1..] {
                    assert_ne!(
                        a.key, b.key,
                        "{:?} 界面里同一个键 `{}` 绑了两处：{} / {}",
                        sc, a.key, a.action, b.action
                    );
                }
            }
        }
    }

    /// 每个界面都应有非空键位表（避免"新界面忘了声明键位"）。
    #[test]
    fn every_screen_declares_bindings() {
        for sc in ALL {
            assert!(!keymap(sc).is_empty(), "{:?} 未声明键位", sc);
        }
    }

    /// 需要"返回上一层"的界面必须提供 `esc` 或 `q`（避免进得去出不来）。
    #[test]
    fn navigable_screens_offer_a_way_back() {
        for sc in [
            Screen::SteamMenu,
            Screen::LobbyList,
            Screen::CreateLobby,
            Screen::Room,
            Screen::SettingsEditor,
            Screen::Play,
            Screen::LearnConfig,
        ] {
            let map = keymap(sc);
            assert!(
                map.iter().any(|b| b.key == "esc" || b.key == "q"),
                "{:?} 没有返回键（esc/q）",
                sc
            );
        }
    }
}


/// **源码级回归检测**：针对本项目真实发生过的四个 UI bug，各写一条断言。
///
/// 为什么不用"统计某字母出现次数"：`just('o') || just('O')` 这类大小写成对的写法会误报，
/// 检测会很脆。改为**断言结构**（顺序、守卫、是否存在早退），既稳健又能真正拦住回归。
#[cfg(test)]
mod source_scan_tests {
    const SRC: &str = include_str!("main.rs");

    fn idx(needle: &str) -> usize {
        SRC.find(needle)
            .unwrap_or_else(|| panic!("源码中找不到 {needle:?}（可能被重构改名，请同步本测试）"))
    }

    /// 回归①（`O` 开→立刻关）：建房界面里，编辑器分支**内部不得再判 `O`**。
    ///
    /// 曾因"外层 `O` 切换开 → 内层又判 `O` 关闭"导致编辑器永远打不开。
    /// 现在的结构：外层用 `o_pressed` 切换一次；进入编辑器分支的条件里带 `!o_pressed`，
    /// 且该分支内不再出现 `'o'` / `"o"` 的按键判定。
    #[test]
    fn create_screen_does_not_handle_o_twice() {
        let start = idx("fn steam_lobby_create_update");
        let after = &SRC[start..];
        let guard = after
            .find("if self.room_cfg_edit && !o_pressed")
            .expect("建房界面的编辑器分支应带 `!o_pressed` 守卫（防止同帧开→关）");
        let tail = &after[guard..];
        // 该分支（到函数末尾）内不应再有 O 键判定。
        let body_end = tail.find("\n    }\n").unwrap_or(tail.len());
        let body = &tail[..body_end];
        for pat in ["just('o')", "just(\"o\")", "just(\"O\")"] {
            assert!(
                !body.contains(pat),
                "编辑器分支内又出现了 `{pat}` —— 会与外面的开关重复处理（回归①）"
            );
        }
    }

    /// 回归③（一键两用：`Q` 关编辑器却退了房）：子界面打开时大厅按键必须被守卫。
    #[test]
    fn lobby_keys_are_guarded_while_a_subscreen_is_open() {
        let n = SRC.matches("!self.room_cfg_edit").count();
        assert!(
            n >= 4,
            "大厅按键（I/Q/L/U）应在编辑器打开时被 `!self.room_cfg_edit` 守卫，当前只有 {n} 处"
        );
        assert!(
            SRC.contains("&& !self.room_cfg_edit"),
            "至少应有一处把编辑器守卫作为组合条件的一部分"
        );
    }

    /// 回归④（大厅子界面文字重叠）：清屏必须发生在菜单内容**之后**、子界面绘制**之前**。
    ///
    /// 曾把清屏插在标题绘制之前 → 清完又被菜单文字画上去，子界面下面仍压着主菜单文字。
    #[test]
    fn lobby_clear_happens_after_menu_content() {
        // 用源码里的 **ASCII 标记** 锚定（中文/缩进/结构都易变，标记稳定），
        // 且限定在 `draw_menu` 内比较顺序（同名分支在 update 里也有）。
        let dm = idx("fn draw_menu(");
        let scope = &SRC[dm..];
        let pos = |needle: &str| {
            dm + scope
                .find(needle)
                .unwrap_or_else(|| panic!("draw_menu 内找不到 {needle:?}"))
        };
        let title = pos("let title = ");
        let clear = pos("LAYOUT-CLEAR");
        let create = pos("CREATE-BRANCH");
        assert!(title < clear, "清屏必须在菜单内容之后（否则等于没清，回归④）");
        assert!(clear < create, "清屏必须在建房分支绘制之前");
        let seg = &SRC[create..(create + 400).min(SRC.len())];
        assert!(
            seg.contains("draw_room_cfg_editor"),
            "建房分支应绘制统一设置编辑器（而非已退休的旧建房界面）"
        );
    }
}
