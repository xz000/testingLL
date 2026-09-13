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
            Binding { key: "up", action: "上移选择" },
            Binding { key: "down", action: "下移选择" },
            Binding { key: "enter", action: "确认所选" },
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

    /// 取 `fn <name>` 的函数体文本（到该函数闭合大括号为止）。
    ///
    /// 注意：`main.rs` 是 **CRLF** 换行，`include_str!` 不做规范化 —— 早期测试用
    /// `find("\n    }\n")` 实际**永远匹配不到**，`unwrap_or(剩余全文)` 把“整段后续代码”当成函数体，
    /// 使 `!contains(...)` 断言恒真（假通过）。这里同时兼容 CRLF / LF，且“找不到闭合”改为 panic。
    fn fn_body(name: &str) -> &'static str {
        let start = idx(name);
        let scope = &SRC[start..];
        let end = scope
            .find("\n    }\r\n")
            .or_else(|| scope.find("\n    }\n"))
            .unwrap_or_else(|| panic!("找不到 {name:?} 的闭合大括号"));
        &scope[..end]
    }

    /// 回归①（`O` 开→立刻关）+ 段 3：创建模式输入已收编到统一编辑器。
    ///
    /// 旧的两列键盘表单与"外层 `O` 切换"已删除；`steam_lobby_create_update` 只做
    /// 「委托 `room_cfg_editor_input` + 处理 `create_confirm_pending`」，
    /// 不得再出现独立的 O/方向键/字段缓冲判定（否则会与编辑器抢输入）。
    #[test]
    fn create_screen_delegates_only_to_the_editor() {
        let body = fn_body("fn steam_lobby_create_update");
        assert!(
            body.contains("self.room_cfg_editor_input(ctx)"),
            "创建模式必须委托统一编辑器处理输入（room_cfg_editor_input）"
        );
        for pat in [
            "steam_create_focus",
            "just('o')",
            "steam_create_players_buf",
            "just('m')",
        ] {
            assert!(
                !body.contains(pat),
                "旧建房表单残留 `{pat}`，会与统一编辑器抢输入（段 3 回归）"
            );
        }
    }

    /// 回归（方案 B）：设置编辑器（`O`）的回车统一为“操作当前行”，不再“有时关闭/有时切换”。
    /// 关闭统一 `O`/`Esc`；创建模式下**裸回车**=建房（`Shift+回车` 仍用于编辑当前行）。
    #[test]
    fn settings_editor_enter_activates_row_not_closes() {
        let body = fn_body("fn room_cfg_editor_input");
        // 旧的“回车也关闭”兜底已移除。
        assert!(
            !body.contains("just_named(NamedKey::Enter) || just_named(NamedKey::Escape) || just(\"o\")"),
            "编辑器不应再让回车直接关闭（方案 B：回车=操作当前行，O/Esc=关闭）"
        );
        // 只读/无操作行应有可见反馈（room_cfg_hint）。
        assert!(
            body.contains("room_cfg_hint"),
            "只读/无操作行应用 room_cfg_hint 给出反馈"
        );
        // 创建模式建房需排除 Shift+回车。
        assert!(
            body.contains("active_modifiers.shift_key()"),
            "创建模式建房应排除 Shift+回车（Shift+回车用于编辑当前行）"
        );
    }

    /// 回归：建房成功后必须清掉 `room_cfg_create_mode`，否则房内按 O 仍走“创建模式”
    /// （回车=建房→直接关闭编辑器、底部显示“回车 创建房间”）。
    #[test]
    fn build_clears_create_mode() {
        let body = fn_body("fn steam_create_confirm");
        assert!(
            body.contains("room_cfg_create_mode = false"),
            "建房成功应清掉创建模式标志"
        );
    }

    /// 回归：编辑房名/备注后必须随 `publish_room_cfg` 发布（否则客户端/房间列表读到建房时的旧值）。
    #[test]
    fn publish_room_cfg_pushes_room_name_and_note() {
        let body = fn_body("fn publish_room_cfg");
        assert!(body.contains("ROOM_NAME_KEY"), "publish_room_cfg 应发布房名");
        assert!(body.contains("ROOM_NOTE_KEY"), "publish_room_cfg 应发布备注");
    }

    /// 回归：就绪界面不得再提示已退休的 `E 编辑房间`（设置入口统一为 `O`）。
    #[test]
    fn room_ready_hint_uses_o_not_e_for_settings() {
        assert!(!SRC.contains("E 编辑房间"), "就绪界面仍写着已退休的 E 编辑房间");
        assert!(
            SRC.contains("O 房间设置") || SRC.contains("O 查看设置"),
            "应提示 O 打开/查看设置"
        );
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

    /// 回归：设置编辑器（建房 / 房内 `O`）必须兼容鼠标。
    ///
    /// `draw_room_cfg_editor` 每帧 `clear` 并登记 `room_cfg_hitboxes`（页签/行/保存/不保存/建房）；
    /// `room_cfg_editor_input` 必须 `hits_at` 派发，且行操作用 `mouse_activate`（≠建房回车）。
    #[test]
    fn settings_editor_supports_mouse() {
        let dbody = fn_body("fn draw_room_cfg_editor");
        assert!(dbody.contains("room_cfg_hitboxes.clear()"), "绘制前应清空命中盒");
        for pat in [
            "RoomCfgAction::Group",
            "RoomCfgAction::Row",
            "RoomCfgAction::Save",
            "RoomCfgAction::Discard",
            "RoomCfgAction::Build",
            "RoomCfgAction::Cancel",
        ] {
            assert!(dbody.contains(pat), "编辑器绘制应登记 {pat} 命中盒");
        }

        let ibody = fn_body("fn room_cfg_editor_input");
        assert!(ibody.contains("room_cfg_hitboxes.hits_at"), "输入应派发命中盒");
        assert!(ibody.contains("mouse_activate"), "鼠标点行应走行级激活语义");
        assert!(ibody.contains("mouse_build"), "建房应有鼠标确认（不然鼠标用户无法建房）");
        assert!(ibody.contains("mouse_save"), "应有鼠标「保存」");
        assert!(ibody.contains("mouse_discard"), "应有鼠标「不保存」");
    }

    /// 回归：房内编辑器关闭必须区分「保存」与「不保存」。
    ///
    /// 曾经只有一种关闭（直接发布）→ 想反悔也没办法。现在：`O`/保存=发布；`Esc`/不保存=回滚快照，不发布。
    #[test]
    fn editor_offers_save_and_discard() {
        // 绘制：两种选择都在。
        assert!(SRC.contains("\"保存\""), "应有「保存」按钮");
        assert!(SRC.contains("\"不保存\""), "应有「不保存」按钮");
        assert!(SRC.contains("\"创建房间\""), "应有「创建房间」按钮");
        // 回滚方法存在，且只回滚不发布。
        let body = fn_body("fn discard_room_cfg");
        assert!(body.contains("room_cfg_snapshot.take()"), "放弃应从快照回滚");
        assert!(!body.contains("publish_room_cfg"), "「不保存」不得发布设置");
        // 打开时记录快照。
        assert!(
            SRC.contains("self.room_cfg_snapshot = Some((self.match_cfg.clone(), self.room_meta.clone()))"),
            "打开编辑器时应记录 (match_cfg, room_meta) 快照"
        );
    }
}
