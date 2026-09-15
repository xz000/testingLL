//! 多语言（i18n）。
//!
//! **设计**：以**中文原文为 key** 查英文表。这样做的好处：
//! - 主语言是中文：中文路径**零改动**（`t("中文")` 直接原样返回），迁移是纯增量；
//! - 漏翻不会变空白：查不到就回退中文，界面永远可读；
//! - key 即文案，改中文时英文表一眼能看出是否过期。
//!
//! **语言来源优先级**：Steam 游戏语言（[`Lang::from_steam_code`]）→ 本地设置覆盖
//! （`LocalSettings::lang`）→ 默认简体中文。见 `main.rs` 的启动与 `steam_lobby_*` 会话建立处。
//!
//! **用法**：
//! - 静态文案：`i18n::t("进攻")`；
//! - 含变量的文案：`i18n::tf("剩余 {n} 秒", &[("n", format!("{n}"))])`（中文里也写成 `{n}`，见 [`tf`]）；
//! - **不要**把玩家输入/Steam 昵称等用户数据交给 `t`（中文玩家名可能与词条撞车）。
//!
//! 英文技能/道具文案优先取 **098c 原始地图**（`098c/out/w3a_strings.txt`）里的原文，
//! 其余为直译；一致性由 [`tests::en_table_has_no_duplicate_keys`] 等单测钉住。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// 支持的语言。
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum Lang {
    /// 简体中文（主语言，默认）。
    #[default]
    ZhHans,
    /// 英语。
    En,
}

impl Lang {
    /// 全部语言（设置界面循环切换的顺序）。
    pub const ALL: [Lang; 2] = [Lang::ZhHans, Lang::En];

    /// 持久化 / 展示用的稳定短码（也用于本地设置 `lang=`）。
    pub fn code(self) -> &'static str {
        match self {
            Lang::ZhHans => "zh",
            Lang::En => "en",
        }
    }

    /// 该语言的**原生名**（设置界面行显示用；本身不翻译）。
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::ZhHans => "简体中文",
            Lang::En => "English",
        }
    }

    /// 由本地设置短码解析（见 [`code`](Lang::code)）。
    pub fn from_code(code: &str) -> Option<Lang> {
        match code.trim().to_ascii_lowercase().as_str() {
            "zh" | "zh-hans" | "zh_cn" | "schinese" | "chinese" => Some(Lang::ZhHans),
            "en" | "english" => Some(Lang::En),
            _ => None,
        }
    }

    /// 由 **Steam 语言码**解析（`Apps::current_game_language()` / `Utils::ui_language()`）。
    ///
    /// 目前只支持中/英：简体中文 → `ZhHans`，其余 Steam 语言（english/german/japanese/…）
    /// 一律回退到英语（英文用户最多，且是当前唯一的第二语言）。
    /// 返回 `None` 仅用于「我们完全没听过这个码」的情况，让调用方保留现有语言。
    #[cfg_attr(not(feature = "steam"), allow(dead_code))]
    pub fn from_steam_code(code: &str) -> Option<Lang> {
        let c = code.trim().to_ascii_lowercase();
        if c.is_empty() {
            return None;
        }
        match c.as_str() {
            "schinese" | "tchinese" | "chinese" => Some(Lang::ZhHans),
            // 其余已知 Steam 语言码统一回退英文（Steam API 的取值见 ISteamApps/ISteamUtils）。
            "english" | "german" | "french" | "spanish" | "latam" | "italian" | "russian"
            | "portuguese" | "brazilian" | "polish" | "dutch" | "turkish" | "japanese"
            | "koreana" | "thai" | "vietnamese" | "swedish" | "danish" | "norwegian"
            | "finnish" | "czech" | "hungarian" | "romanian" | "greek" | "bulgarian"
            | "ukrainian" | "indonesian" => Some(Lang::En),
            _ => None,
        }
    }
}

/// 语言偏好（本地设置里存这个，而不是直接存 `Lang`）。
///
/// `Auto`（默认）= 跟随 Steam 语言设置；`Fixed` = 玩家在游戏内设置里手动指定的语言。
/// 这样既满足「用 Steam 语言切换」，又能在 Steam 语言不合意时手动覆盖（且不会被 Steam 刷回去）。
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum LangPref {
    #[default]
    Auto,
    Fixed(Lang),
}

impl LangPref {
    /// 全部可选值（设置界面循环顺序）：自动 → 各语言。
    pub fn all() -> Vec<LangPref> {
        let mut v = vec![LangPref::Auto];
        v.extend(Lang::ALL.iter().copied().map(LangPref::Fixed));
        v
    }

    /// 持久化短码：`auto` / `zh` / `en`。
    pub fn code(self) -> &'static str {
        match self {
            LangPref::Auto => "auto",
            LangPref::Fixed(l) => l.code(),
        }
    }

    /// 设置界面显示名（自动项本身要翻译，由调用方用 `t("自动（跟随 Steam）")`；
    /// 固定语言显示其原生名）。
    pub fn display(self) -> &'static str {
        match self {
            LangPref::Auto => "自动（跟随 Steam）",
            LangPref::Fixed(l) => l.native_name(),
        }
    }

    /// 解析持久化短码；非法值回退 `Auto`。
    pub fn from_code(code: &str) -> LangPref {
        match code.trim().to_ascii_lowercase().as_str() {
            "auto" | "" => LangPref::Auto,
            other => match Lang::from_code(other) {
                Some(l) => LangPref::Fixed(l),
                None => LangPref::Auto,
            },
        }
    }

    /// 循环到下一个选项。
    pub fn next(self) -> LangPref {
        let all = LangPref::all();
        let i = all.iter().position(|&p| p == self).unwrap_or(0);
        all[(i + 1) % all.len()]
    }

    /// 结合（可能有的）Steam 语言得出实际语言。
    pub fn resolve(self, steam: Option<Lang>) -> Lang {
        match self {
            LangPref::Auto => steam.unwrap_or(Lang::ZhHans),
            LangPref::Fixed(l) => l,
        }
    }
}

/// 当前语言（全局）。UI 单线程访问；`AtomicU8` 仅用于满足 `static` 要求。
static CURRENT: AtomicU8 = AtomicU8::new(Lang::ZhHans as u8);

/// 取当前语言。
pub fn lang() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::En,
        _ => Lang::ZhHans,
    }
}

/// 设置当前语言（启动时按 Steam/本地设置调用；设置界面切换时调用）。
pub fn set_lang(l: Lang) {
    CURRENT.store(l as u8, Ordering::Relaxed);
}

/// 翻译一条**静态**中文文案（按指定语言，纯函数、不改全局状态；单测用这个）。
pub fn t_for(lang: Lang, zh: &str) -> &str {
    if lang == Lang::ZhHans {
        return zh;
    }
    en_table().get(zh).copied().unwrap_or(zh)
}

/// 翻译一条**静态**中文文案（用当前语言）；查不到回退原文（中文路径原样返回）。
pub fn t(zh: &str) -> &str {
    t_for(lang(), zh)
}

/// 含变量文案的翻译+占位替换（按指定语言，纯函数）。
pub fn tf_for(lang: Lang, zh: &str, args: &[(&str, String)]) -> String {
    let mut s = t_for(lang, zh).to_string();
    for (k, v) in args {
        if s.contains('{') {
            s = s.replace(&format!("{{{k}}}"), v);
        }
    }
    s
}

/// 翻译一条**含变量**的中文文案（用当前语言），并做 `{名字}` 占位替换。
///
/// 与 `format!` 的区别：`format!` 要求格式串是字面量，无法用运行时查表结果；
/// 因此这里在中文原文与英文译文里都写 `{名字}`，由本函数做字面替换。
/// 数值/单位请先格式化成字符串再传入，例如：
/// `i18n::tf("剩余 {secs} 秒", &[("secs", format!("{secs:.1}"))])`。
#[cfg_attr(not(feature = "steam"), allow(dead_code))]
pub fn tf(zh: &str, args: &[(&str, String)]) -> String {
    tf_for(lang(), zh, args)
}

/// 英文词条表（`key=中文原文` → `value=英文`）。
///
/// 英文技能/道具文案优先取自 098c 原始地图导出（`098c/out/w3a_strings.txt`）。
const EN: &[(&str, &str)] = &[
    // ---- 通用 ----
    ("开", "On"),
    ("关", "Off"),
    ("是", "Yes"),
    ("否", "No"),
    ("返回", "Back"),
    // ---- 主菜单 ----
    ("圆圈之战 Circle Brawl", "Circle Brawl"),
    ("—— 选择对战模式 ——", "— Select Game Mode —"),
    ("单机技能试验场", "Single-player Sandbox"),
    ("无 AI 自由试技能与数值（进入后配置技能开始）", "Freely test skills and stats with no AI (configure skills to begin)"),
    ("局域网对战", "LAN Match"),
    ("同机/内网：命令行 --host <port> / --join <host:port>", "Same PC / LAN: launch with --host <port> to host or --join <host:port> to join"),
    ("Steam 在线对战", "Steam Online"),
    ("联网与好友实时对抗（进入 Steam 大厅）", "Real-time matches with friends online (opens the Steam lobby)"),
    ("设置", "Settings"),
    ("本机音量与静音（F10 快捷静音）", "Local volume and mute (F10 toggles mute)"),
    ("↑/↓ 选择    回车 确认    或直接按数字键", "↑/↓ Select    Enter Confirm    or press a number key"),
    ("Steam 对战 · 大厅", "Steam · Lobby"),
    ("本端版本 v{v}，仅同版本可联机", "Client version v{v}; only matching versions can play together"),
    ("创建房间", "Create Room"),
    ("选房间名与玩家人数，然后进入房间", "Pick a room name and player count, then enter the room"),
    ("加入房间", "Join Room"),
    ("从房间列表选择并加入", "Pick a room from the list and join"),
    ("返回主菜单", "Back to Main Menu"),
    ("回到主菜单选择", "Return to the main menu to choose"),
    ("↑/↓ 选择    回车 确认    H/J/Q 快捷键", "↑/↓ Select    Enter Confirm    H/J/Q shortcuts"),
    ("Steam 未启用", "Steam Disabled"),
    ("需要 --features client/steam 构建", "Build with --features client/steam"),
    ("局域网对战需命令行启动：--host <port> 创建，或 --join <host:port> 加入（GUI 暂未接入）", "LAN play must be launched from the command line: --host <port> to host, or --join <host:port> to join (not yet available in the GUI)"),
    // ---- 本机设置 ----
    ("设置（本机）", "Settings (Local)"),
    ("音量与静音仅影响本机，不影响联机", "Volume and mute are local only and do not affect multiplayer"),
    ("主音量", "Master Volume"),
    ("音效音量", "SFX Volume"),
    ("音乐音量", "Music Volume"),
    ("静音", "Mute"),
    ("语言", "Language"),
    ("自动（跟随 Steam）", "Auto (follow Steam)"),
    ("开（静音）", "On (Muted)"),
    ("返回  [Esc]", "Back  [Esc]"),
    ("↑/↓ 选择 · ←/→ 调值 · 回车/点击 调整 · Esc/Q 返回", "↑/↓ Select · ←/→ Adjust · Enter/Click Apply · Esc/Q Back"),
    ("当前：已静音（F10 切换）", "Muted now (F10 toggles)"),
    ("F10 一键静音", "F10 toggles mute"),
    ("（自定义）", " (custom)"),
    ("[= / 回车]", "[= / Enter]"),
    ("[退格 / Delete]", "[Backspace / Delete]"),
    ("回车", "Enter"),
    ("F 筛选", "F Filter"),
    ("已刷新好友列表", "Refreshed friend list"),
    ("steam 会话丢失", "Steam session lost"),
    // ---- 不可购买/禁用原因（内嵌到按钮文案） ----
    ("已满级", "maxed"),
    ("背包已满", "bag full"),
    ("金币不足", "not enough gold"),
    ("{name}  （已满级）", "{name}  (maxed)"),
    ("{name}  （不可购买）", "{name}  (unavailable)"),
    ("[版本不符 {ver:?}]", "[incompatible {ver:?}]"),
    ("关闭{tag}", "Off{tag}"),
    ("{n} 人", "{n} players"),
    // ---- 房间设置编辑器：分组 ----
    ("房间", "Room"),
    ("经济", "Economy"),
    ("玩法", "Gameplay"),
    ("地图", "Map"),
    ("模式", "Mode"),
    // ---- 房间设置编辑器：行标签 ----
    ("房间名", "Room Name"),
    ("备注", "Note"),
    ("人数上限（只读）", "Player Limit (read-only)"),
    ("初始金币", "Starting Gold"),
    ("每轮金币", "Gold per Round"),
    ("击杀金币", "Gold per Kill"),
    ("助攻金币", "Gold per Assist"),
    ("胜利金币", "Gold per Round Win"),
    ("伤害金（回合最高伤害）", "Damage Gold (most damage)"),
    ("击杀得点", "Score per Kill"),
    ("助攻得点", "Score per Assist"),
    ("胜利得点", "Score per Round Win"),
    ("伤害倍率", "Damage Multiplier"),
    ("击退倍率", "Knockback Multiplier"),
    ("岩浆伤害", "Lava Damage"),
    ("首轮配置期(秒)", "First Round Setup (s)"),
    ("局间配置期(秒)", "Between Rounds Setup (s)"),
    ("收缩延迟(秒)", "Shrink Delay (s)"),
    ("收缩总时长(秒)", "Shrink Total (s)"),
    ("基础回血(HP/s)", "Base Regen (HP/s)"),
    ("地图形状", "Arena Shape"),
    ("柱子", "Pillars"),
    ("冰面", "Ice"),
    ("总轮数", "Total Rounds"),
    ("游戏模式", "Game Mode"),
    ("金币奖励总开关", "Gold Rewards (master)"),
    // ---- 房间设置编辑器：枚举档位 ----
    ("随机", "Random"),
    ("每局必有", "Every round"),
    ("轮次", "Rounds"),
    ("死亡竞赛", "Deathmatch"),
    ("化身", "Avatar"),
    ("国王", "King"),
    ("最后生还", "Last Man Standing"),
    ("圆形", "Circle"),
    // ---- 房间设置编辑器：说明（详情区） ----
    ("大厅里显示的房间名（改完关闭编辑器即生效）。", "Room name shown in the lobby (applies when you close the editor)."),
    ("大厅备注，可留空。", "Lobby note; may be left empty."),
    ("本场打几轮（1~50）。", "Rounds in this match (1–50)."),
    ("Steam 建房时固定，之后不可改；想关门用「锁房」（就绪界面 L）。", "Fixed when the Steam room is created; use “Lock Room” (L on the ready screen) to close it."),
    ("098c `Qo`=20。开局一次性发放。", "098c `Qo`=20. Granted once at match start."),
    ("098c 设置 17 `qo`=10。每轮结算时发放。", "098c setting 17 `qo`=10. Granted at each round settlement."),
    ("098c 设置 12 `lo`=1。", "098c setting 12 `lo`=1."),
    ("098c 设置 13 `Lo`=1。", "098c setting 13 `Lo`=1."),
    ("098c 设置 15 `Mo`=2。", "098c setting 15 `Mo`=2."),
    ("098c 设置 16 `po`=1。发给**本回合伤害最高**者（并列都发）。", "098c setting 16 `po`=1. Granted to the player(s) with the **most damage this round** (ties all get it)."),
    ("098c 设置 10 `ko`=1。", "098c setting 10 `ko`=1."),
    ("098c 设置 11 `Ko`=1。", "098c setting 11 `Ko`=1."),
    ("098c 设置 14 `mo`=2。", "098c setting 14 `mo`=2."),
    ("全局伤害倍率；档位 75/100/125/150%，可自定义。", "Global damage multiplier; presets 75/100/125/150%, customizable."),
    ("全局击退倍率；档位同上。", "Global knockback multiplier; same presets as above."),
    ("岩浆（出界）伤害倍率。**0 = 关闭岩浆**（098c 允许，不推荐）。", "Lava (out-of-bounds) damage multiplier. **0 = lava off** (allowed by 098c, not recommended)."),
    ("098c 设置 5 `Uo`=40：第一轮的配置期时长。", "098c setting 5 `Uo`=40: setup time for the first round."),
    ("098c 设置 4 `uo`=30：局间配置期时长。", "098c setting 4 `uo`=30: setup time between rounds."),
    ("开局静止期；实际延迟 = 本值 × √存活人数（098c `wo*√sn`）。", "Initial grace period; actual delay = this value × √(alive players) (098c `wo*√sn`)."),
    ("满员时从开始收缩到缩到 0 的总时长；实际 = 本值 × √(存活/初始)，连续收缩（非按环）。", "Total time from shrink start to zero at a full lobby; actual = this value × √(alive/initial), shrinking continuously (not per ring)."),
    ("098c 设置 9 `In`=.05/0.1s = 0.5。档位 0.5/0/0.25/0.75/1.0/2.0。", "098c setting 9 `In`=.05 per 0.1s = 0.5. Presets 0.5/0/0.25/0.75/1.0/2.0."),
    ("**仅圆形**（暂锁定，置灰）；后续版本再扩正方形/六边形。", "**Circle only** (locked/greyed for now); squares/hexagons come later."),
    ("关闭 / 随机 / 每局必有。", "Off / Random / Every round."),
    ("1 轮次 · 2 死亡竞赛 · 3 化身 · 4 国王 · 5 最后生还。改动会取消全员准备。", "1 Rounds · 2 Deathmatch · 3 Avatar · 4 King · 5 Last Man Standing. Changing this un-readies everyone."),
    ("关闭后击杀/胜利/最高伤害金归零（等价 098c `-no reward`）；点数、助攻金、每轮金不变。", "When off, kill/win/most-damage gold becomes zero (equivalent to 098c `-no reward`); points, assist gold and per-round gold are unchanged."),
    // ---- 对局 HUD ----
    ("检测到帧同步分歧(desync)：本端状态与房主不一致，请退出重连", "Desync detected: your state differs from the host — please leave and reconnect"),
    ("你是化身 — 独占一队，全场皆是敌人", "You are the Avatar — alone on your team, everyone else is an enemy"),
    ("你是国王 — 受伤与岩浆 -10%", "You are the King — damage and lava taken -10%"),
    ("视角: 方向键/中键拖拽 平移 · 滚轮缩放 · Home 场地中心 · End 跳到自己", "Camera: Arrow keys / middle-drag pan · Wheel zoom · Home center · End jump to self"),
    ("金币 {n}", "Gold {n}"),
    ("模式：{mode}（{align}）", "Mode: {mode} ({align})"),
    ("两队", "2 teams"),
    ("精通 命{life} 范{range} 射{time} 包{bag}", "Mastery HP{life} AoE{range} Rng{time} Bag{bag}"),
    ("精通  命{life} 范{range} 射{time} 包{bag}", "Mastery  HP{life} AoE{range} Rng{time} Bag{bag}"),
    ("死斗：目标 {n} 分", "Deathmatch: first to {n}"),
    ("死斗  目标 {n} 分", "Deathmatch  first to {n}"),
    ("第 {round} / {total} 局", "Round {round} / {total}"),
    ("{title}     化 = 化身", "{title}     A = Avatar"),
    ("{title}     王 = 国王", "{title}     K = King"),
    ("·化", "·A"),
    ("·王", "·K"),
    ("玩家", "Player"),
    ("分数", "Score"),
    ("击杀", "Kills"),
    ("伤害", "Damage"),
    ("存活", "Alive"),
    ("出局", "Out"),
    ("{name}{role} (我)", "{name}{role} (me)"),
    ("玩家{id}", "Player{id}"),
    // ---- 房间设置徒章 / 就绪界面 ----
    ("房间设置：默认（原版）", "Room settings: Default (vanilla)"),
    ("房间设置：自定义 {n} 项 ⚠", "Room settings: {n} custom ⚠"),
    ("房间设置：自定义 {n} 项", "Room settings: {n} custom"),
    ("   [O] 编辑/查看", "   [O] Edit/View"),
    ("   [O] 查看", "   [O] View"),
    ("只读：只有房主可以修改房间设置", "Read-only: only the host can change room settings"),
    ("{label}：暂锁定（{value}）", "{label}: locked ({value})"),
    ("创建房间 · 设置   （自定义 {n} 项）", "Create Room · Settings   ({n} custom)"),
    ("房间设置   （自定义 {n} 项）", "Room Settings   ({n} custom)"),
    ("[输入 {buf}_]", "[Type {buf}_]"),
    ("[只读] A/Z/X/C/V 分组 · ↑↓ 选择 · Esc 或 O 关闭", "[Read-only] A/Z/X/C/V groups · ↑↓ select · Esc or O to close"),
    ("输入中：回车 提交 · Esc 取消本次输入（提交/取消后再按 Esc/O 关闭）", "Editing: Enter to submit · Esc to cancel this input (then Esc/O to close)"),
    ("A/Z/X/C/V 分组 · ↑↓ 选择 · ←→ 档位 · T 或 Shift+回车 输入 · 回车/右下按钮 创建房间 · Esc 取消", "A/Z/X/C/V groups · ↑↓ select · ←→ tier · T or Shift+Enter to type · Enter/bottom-right to create · Esc cancel"),
    ("A/Z/X/C/V 分组 · ↑↓ 选择 · ←→ 档位 · 回车/T 编辑当前行 · O 保存并关闭 · Esc 不保存", "A/Z/X/C/V groups · ↑↓ select · ←→ tier · Enter/T edit row · O save & close · Esc discard"),
    ("取消", "Cancel"),
    ("关闭", "Close"),
    ("保存", "Save"),
    ("不保存", "Don't Save"),
    ("房间 - 等待所有人就绪", "Room — waiting for everyone to ready up"),
    ("房间：{name}    人数 {n}    版本 v{v}", "Room: {name}    Players {n}    Version v{v}"),
    ("[锁]", "[Locked]"),
    ("[开]", "[Open]"),
    ("备注：{note}", "Note: {note}"),
    ("流程：全员就绪 → 倒计时 → 技能配置 → 配好后自动开战", "Flow: all ready → countdown → skill setup → battle starts automatically"),
    ("即将开始（不可取消）", "Starting soon (cannot cancel)"),
    ("按 U 可取消", "Press U to cancel"),
    ("人数不足（已入 {n}）：房主已确认，{secs} 秒后进配置（{hint}）", "Fewer players ({n} in): host confirmed, entering setup in {secs}s ({hint})"),
    ("全员就绪：{secs} 秒后进配置（结束前按 U 可取消）", "All ready: entering setup in {secs}s (press U to cancel before then)"),
    ("人数不足（已入 {n}）：当前全员就绪，按回车 开始倒计时", "Fewer players ({n} in): all ready now, press Enter to start the countdown"),
    ("已就绪：等其他人就绪（人数不足时由你按回车开始）", "Ready: waiting for others (press Enter to start when underfull)"),
    ("已就绪：等其他人就绪", "Ready: waiting for others"),
    ("▶ 按 U 就绪（再按 U 取消）", "▶ Press U to ready (U again to cancel)"),
    ("O 房间设置（含 L 锁定）    I 邀请好友    Q 退出房间", "O Room settings (L locks)    I Invite friends    Q Leave room"),
    ("U 就绪/取消    O 查看设置    I 邀请好友    Q 退出房间", "U Ready/Unready    O View settings    I Invite friends    Q Leave room"),
    ("== 就绪状态 ==", "== Ready Status =="),
    ("（我）", "(me)"),
    ("== 邀请好友 ==", "== Invite Friends =="),
    ("（暂无好友 / 正在拉取…）", "(No friends / loading…)"),
    ("（已在房间）", "(In room)"),
    ("（在线）", "(Online)"),
    ("（离线）", "(Offline)"),
    ("↑/↓ 选择    回车 邀请    A Steam 邀请窗口    R 刷新    I/Q 收起", "↑/↓ select    Enter invite    A Steam invite dialog    R refresh    I/Q close"),
    ("已打开 Steam 邀请窗口（可勾选多位好友）", "Opened the Steam invite dialog (you can pick several friends)"),
    ("未刷新好友列表", "Friend list not refreshed"),
    ("尚未在房间里，无法邀请", "Not in a room yet; cannot invite"),
    ("{name} 已经在房间里了", "{name} is already in the room"),
    ("已邀请 {name}{note}", "Invited {name}{note}"),
    ("（离线，邀请会等到其上线）", " (offline; the invite waits until they come online)"),
    // ---- 重连 / 连接中 / 房主离开 ----
    ("正在重连…", "Reconnecting…"),
    ("连接已断开", "Connection lost"),
    ("按 R 从 host 拉取快照重连", "Press R to pull a snapshot from the host and reconnect"),
    ("正在创建房间…", "Creating room…"),
    ("正在加入房间…", "Joining room…"),
    ("正在连接 Steam 大厅，请稍候", "Connecting to the Steam lobby, please wait"),
    ("按 Q / Esc 取消", "Press Q / Esc to cancel"),
    ("已等待 {secs}s", "Waited {secs}s"),
    ("房主已离开房间", "The host left the room"),
    ("房主已退出或断开，本场无法继续", "The host left or disconnected; this match cannot continue"),
    ("按 Q / 回车 / Esc 返回主菜单", "Press Q / Enter / Esc to return to the main menu"),
    // ---- 房间列表 ----
    ("搜索中…", "Searching…"),
    ("全部", "All"),
    ("模式：[{mode}]    共 {n} 个", "Mode: [{mode}]    {n} total"),
    ("正在向 Steam 查询公开房间，请稍候", "Querying public rooms on Steam, please wait"),
    ("（无此模式的房间）", "(No rooms for this mode)"),
    ("按 F 切换筛选条件，或 R 重新搜索", "Press F to change the filter, or R to search again"),
    ("（暂无可加入的房间）", "(No joinable rooms right now)"),
    ("让好友先创建房间，或按 R 重新搜索", "Have a friend create a room, or press R to search again"),
    ("房间 / 房主", "Room / Host"),
    ("人数 · 模式", "Players · Mode"),
    ("房间详情", "Room Details"),
    ("房间名：", "Name: "),
    ("房主：", "Host: "),
    ("人数：", "Players: "),
    ("模式：", "Mode: "),
    ("版本：", "Version: "),
    ("兼容", "Compatible"),
    ("{ver:?}（不兼容）", "{ver:?} (incompatible)"),
    ("备注：", "Note: "),
    ("房主", "Host"),
    ("回车 加入", "Enter Join"),
    ("R 刷新", "R Refresh"),
    ("Q 返回", "Q Back"),
    ("选中的房间已满，请换一个", "The selected room is full; pick another"),
    ("版本不符（房主 {host:?}，本端 {mine}），无法加入", "Version mismatch (host {host:?}, local {mine}); cannot join"),
    ("加入失败：{err}", "Join failed: {err}"),
    ("进入房间失败：{err}", "Failed to enter the room: {err}"),
    ("未命名房间", "Unnamed Room"),    ("我的房间", "My Room"),
    ("{name}的房间", "{name}'s Room"),
    ("延迟 -- ms", "Ping -- ms"),
    ("延迟 {ms} ms", "Ping {ms} ms"),
    // ---- Steam presence / 上报 ----
    ("房间「{name}」{n}/{limit} 等待中", "Room \"{name}\" {n}/{limit} waiting"),
    ("正在配置技能", "Configuring skills"),
    ("对局中（第 {round} 局）", "In match (round {round})"),
    ("成就已上报：{names}", "Achievements submitted: {names}"),
    ("、", ", "),
    ("战绩上报未生效（需在 Steamworks 后台配置统计/成就）", "Stats upload had no effect (configure stats/achievements in Steamworks)"),
    ("已从邀请加入房间", "Joined from an invite"),
    // ---- 学习 / 配置 ----
    ("蓄力中", "Casting"),
    ("Shift+右键 排移动 / Shift+技能(Shift+左键点目标) 排施法 / S 清空指令队列", "Shift+Right-click queues a move / Shift+skill (Shift+Left-click target) queues a cast / S clears the queue"),
    ("开局配置", "Match Setup"),
    ("第 {round} / {total} 局结束 - 学习阶段", "Round {round} / {total} over — Learning Phase"),
    ("自由配置 · 空格 / 回车 开始", "Free setup · Space / Enter to start"),
    ("剩余 {secs}s", "{secs}s left"),
    ("金币 {gold}   击杀 {kills}   最佳名次 #{rank}", "Gold {gold}   Kills {kills}   Best #{rank}"),
    ("[J]技能", "[J] Skills"),
    ("[K]商店", "[K] Shop"),
    ("[L]成长", "[L] Growth"),
    ("配装总览", "Loadout"),
    ("技能槽", "Skill Slots"),
    ("[{key}] 未绑定", "[{key}] Unbound"),
    ("持有物品 ({n}/{cap})", "Items ({n}/{cap})"),
    ("（空）", "(empty)"),
    ("  ↑ 升 {name} · {cost}G", "  ↑ To {name} · {cost}G"),
    ("  (满级)", "  (maxed)"),
    ("（技能整场锁定，购买后同树其余技能不可再选）", "(Skills lock for the whole match; buying one locks the rest of its tree)"),
    ("{tree} 树 — 点技能查看详情，再购买 / 升级", "{tree} tree — click a skill for details, then buy / upgrade"),
    ("{i} {name}  ✓已购", "{i} {name}  ✓ owned"),
    ("{i} {name}  （同树已锁定）", "{i} {name}  (tree locked)"),
    ("{i} {name}  ({cost}G 金币不足)", "{i} {name}  ({cost}G not enough gold)"),
    ("{name}  Lv{lv} / {cap}  （已购买，乔丹 +{jb}）", "{name}  Lv{lv} / {cap}  (owned, Jordan +{jb})"),
    ("{name}  Lv{lv} / {cap}  （已购买）", "{name}  Lv{lv} / {cap}  (owned)"),
    ("Lv{lv} / 上限{cap}  伤害 {dmg}  冷却 {cd}s  射程 {rng}", "Lv{lv} / cap {cap}  Dmg {dmg}  CD {cd}s  Range {rng}"),
    ("二形态  形态A：{a}   ⇄   形态B：{b}", "Dual form  A: {a}   ⇄   B: {b}"),
    ("当前出战：{name}", "Active: {name}"),
    ("二形态：无", "Dual form: none"),
    ("切换为 {name}  (B)", "Switch to {name}  (B)"),
    ("，已突破 ×{n}", ", +{n} breaks"),
    ("突破上限 +2（乔丹之石 · {price}G{extra}）  {hint}", "Raise cap +2 (Jordan Stone · {price}G{extra})  {hint}"),
    ("已满级 Lv{lv}", "Maxed Lv{lv}"),
    ("升级到 Lv{lv} ({cost}G)  [= / 回车]", "Upgrade to Lv{lv} ({cost}G)  [= / Enter]"),
    ("同树已锁定，不可购买", "Tree locked; cannot buy"),
    ("购买 ({cost}G)  [= / 回车]", "Buy ({cost}G)  [= / Enter]"),
    ("购买 ({cost}G) — 金币不足", "Buy ({cost}G) — not enough gold"),
    ("← 点击上方技能查看详情 / 购买 / 切形态", "← Click a skill above for details / buy / switch form"),
    ("← 在左侧「配装总览」里点一个技能槽来配置", "← Click a skill slot in the Loadout panel to configure"),
    ("物品栏 {n}/{cap}    金币 {gold}G", "Items {n}/{cap}    Gold {gold}G"),
    ("{hint} 购买 {name}（{cost}G）", "{hint} Buy {name} ({cost}G)"),
    ("{hint} 购买 {name}（{cost}G）— {reason}", "{hint} Buy {name} ({cost}G) — {reason}"),
    ("{hint} 购买 — 已满级", "{hint} Buy — maxed"),
    ("{hint} 卖出 {name}（+{gold}G）", "{hint} Sell {name} (+{gold}G)"),
    ("{hint} 卖出 — 未持有该物品", "{hint} Sell — not owned"),
    ("← 点上方物品查看详情 / 购买 / 卖出", "← Click an item above for details / buy / sell"),
    ("（无）", "(none)"),
    ("共 {n} 条 · 滚轮/↑↓ 滚动 {range} · 数字选中", "{n} rows · wheel/↑↓ scroll {range} · number to select"),
    ("金币 {n}G", "Gold {n}G"),
    ("← 点上方精通查看详情 / 购买", "← Click a mastery above for details / buy"),
    ("（技能上限突破在「技能」页详情里用乔丹之石，不在此页）", "(Skill cap breaks use the Jordan Stone on the Skills page, not here)"),
    ("字母选树 · 数字选技能看详情 · = 或回车 购买/升级/乔丹突破 · B 切形态 · J/K/L 翻页", "Letters pick a tree · numbers pick a skill for details · = or Enter buy/upgrade/Jordan break · B switch form · J/K/L tabs"),
    ("B/N/M 选分类 · 数字选中 · = 购买/升级 · 退格 卖出 · 滚轮/↑↓ 滚动 · J/K/L 翻页", "B/N/M categories · numbers select · = buy/upgrade · Backspace sell · wheel/↑↓ scroll · J/K/L tabs"),
    ("数字选精通 · = 或回车 购买 · J/K/L 翻页（技能上限突破在「技能」页）", "Numbers pick a mastery · = or Enter buy · J/K/L tabs (skill cap breaks are on the Skills page)"),
    ("{hint} 购买（{cost}G）", "{hint} Buy ({cost}G)"),
    ("{hint} 购买（{cost}G）— {reason}", "{hint} Buy ({cost}G) — {reason}"),
    // ---- 结算 / 播报 / 自身状态 ----
    ("对局结束", "Match Over"),
    ("最终得分排名", "Final Score Ranking"),
    ("#{rank}  {name}  {score} 分", "#{rank}  {name}  {score} pts"),
    ("{name}  金币{gold}  击杀{kills}  伤害{dmg}  最佳名次#{rank}", "{name}  Gold{gold}  Kills{kills}  Dmg{dmg}  Best#{rank}"),
    ("Steam 统计：场次 {matches}    胜场 {wins}    击杀 {kills}", "Steam stats: {matches} matches    {wins} wins    {kills} kills"),
    ("排行榜：暂无数据（需在 Steamworks 后台建榜）", "Leaderboard: no data (create it in the Steamworks backend)"),
    ("排行榜 TOP5", "Leaderboard TOP5"),
    ("按 Q 返回主菜单", "Press Q to return to the main menu"),
    ("{name} 阵亡", "{name} was eliminated"),
    ("{who} 击杀了 {victim}  ·  {label} ×{count}", "{who} killed {victim}  ·  {label} ×{count}"),
    ("{who} 击杀了 {victim}", "{who} killed {victim}"),
    ("音频已静音 [F10]", "Audio muted [F10]"),
    ("音频已开启 [F10]", "Audio on [F10]"),
    ("状态", "Status"),
    ("待命", "Idle"),
    ("生命 {cur} / {max}", "HP {cur} / {max}"),
    ("施法中 · {name} {secs}s", "Casting · {name} {secs}s"),
    ("收招 · {name} {secs}s", "Recovering · {name} {secs}s"),
    ("熔岩靴 冷却 {secs}s", "Lava Boots CD {secs}s"),
    ("熔岩靴 就绪", "Lava Boots ready"),
    ("物品 —", "Item —"),
    // ---- 状态图标（头顶/自身面板单字） ----
    ("格", "P"), ("燃", "B"), ("疾", "H"), ("迅", "S"), ("隐", "I"),
    ("盾", "Sh"), ("反", "Rf"), ("岩", "Rk"), ("守", "Gd"), ("镜", "Mr"),
    ("灼", "Sc"), ("束", "Bd"), ("默", "Si"), ("饼", "Pk"), ("慢", "Sl"), ("弱", "Wk"),
    // ---- 技能树名称 ----
    ("身法", "Mobility"), ("突击", "Assault"), ("远程", "Ranged"), ("弹幕", "Barrage"),
    ("控场", "Control"), ("吸血", "Leech"), ("秘法", "Mystic"), ("奥术", "Arcane"),
    // ---- 连杀播报（098b） ----
    ("大杀特杀", "Killing Spree"), ("主宰比赛", "Dominating"), ("迈向胜利", "Mega Kill"),
    ("狂暴了", "Unstoppable"), ("无法阻挡", "Wicked Sick"), ("变态了", "Monster Kill"),
    ("接近神了", "Godlike"), ("超越神了", "Beyond Godlike"),
    // ---- 技能名（098c 名册；英文优先取 098c 原文，其余直译） ----
    ("火球", "Fireball"), ("追踪弹", "Homing Missile"), ("回旋镖", "Boomerang"),
    ("闪电", "Lightning"), ("陨石", "Meteor"), ("岩浆", "Magma"),
    ("分裂弹", "Splitter"), ("分裂弹·目标", "Splitter ·Target"), ("分裂弹·区域", "Splitter ·Area"),
    ("汲取", "Drain"), ("汲取·减速", "Drain ·Slow"), ("汲取·削弱", "Drain ·Weaken"),
    ("火焰喷射", "Flame Spray"), ("火焰喷射·流射", "Flame Spray ·Stream"), ("火焰喷射·簇射", "Flame Spray ·Burst"),
    ("弹跳弹", "Bouncing Shot"), ("弹跳弹·充能", "Bouncing Shot ·Recharge"),
    ("反射盾", "Parry"), ("时光回溯", "Time Rewind"), ("急行", "Haste"),
    ("疾风步", "Windwalk"), ("疾风步·冲锋", "Windwalk ·Charge"), ("疾风步·隐身", "Windwalk ·Invisibility"),
    ("瞬间移动", "Blink"), ("瞬间移动·镜像", "Blink ·Mirror"),
    ("冲撞", "Charge"), ("冲撞·突击", "Charge ·Assault"), ("冲撞·凤凰", "Charge ·Phoenix"),
    ("移形换位", "Relocate"), ("移形换位·置换", "Relocate ·Swap"), ("移形换位·搬运", "Relocate ·Carry"),
    ("禁锢", "Bind"), ("禁锢·缠绕", "Bind ·Entangle"), ("禁锢·沉默", "Bind ·Silence"),
    ("引力", "Gravity"), ("引力·暗物质", "Gravity ·Dark Matter"), ("引力·力场", "Gravity ·Force Field"),
    ("锁链", "Chain"), ("锁链·钩引", "Chain ·Hook"), ("锁链·红链", "Chain ·Red Chain"),
    ("天罚", "Smite"), ("灾变", "Catastrophe"), ("虔诚", "Devotion"),
    ("电弧（非 098c·未实装）", "Arc (not in 098c · unimplemented)"),
    ("镜像分身", "Mirror Image"),
    ("物品（未实装）", "Item (unimplemented)"),
    ("怀表（未实装：沉默缩短）", "Pocket Watch (unimplemented: shorter silence)"),
    ("锁链附加（未实装）", "Chain Add-on (unimplemented)"),
    ("切换键（未实装）", "Toggle Key (unimplemented)"),
    // 遗留（Unity 版旧技能，已退役但枚举保留）
    ("疾跑", "Sprint"), ("护盾", "Shield"), ("影身", "Shadow Form"), ("幻象", "Illusion"),
    ("闪烁", "Blink"), ("冲锋", "Charge"), ("二段闪", "Double Blink"), ("冲刺斩", "Dash Slash"),
    ("闪到墙", "Blink to Wall"), ("掷石", "Stone Shot"), ("潜行踢", "Stealth Kick"),
    ("掷弹", "Bomb Toss"), ("潜行踢·连推", "Stealth Kick ·Chain Push"),
    ("撒弹线", "Scatter Line"), ("散射弹线", "Spread Line"), ("导弹", "Missile"),
    ("香蕉弹", "Banana Shot"), ("吸血链镖", "Tether Leech"),
    ("扇扫连射", "Sweep Volley"), ("扇面齐射", "Fan Volley"),
    ("跳弹", "Bounce Shot"), ("蓄力跳弹", "Charged Bounce"), ("转镖", "Turning Blade"),
    ("蓝线回拉", "Blue-Line Pull"), ("红线回拉", "Red-Line Pull"), ("撞击迟缓", "Impact Slow"),
    ("束缚线", "Tether"), ("引力场", "Gravity Field"), ("星域", "Star Zone"),
    ("雷电", "Lightning"), ("换位", "Swap"), ("未实现", "Not Implemented"),
    // ---- 技能描述 ----
    ("术士的通用火球：命中造成伤害。击中柱子会反弹。", "The warlock's basic fireball: damages on hit and bounces off pillars."),
    ("天罚（F）：对目标区域降下天罚，是术士的固定技能之一。", "Smite (F): calls down judgment on a target area; one of the warlock's fixed skills."),
    ("短时间内可以反弹大部分飞行法术和冲撞。可挣脱锁链。", "Briefly reflects most flying spells and charges. Can break chains."),
    ("时光回到使用法术 3.5 秒前的位置，并恢复当时的血量、动量与击退值；可挣脱锁链和消除减益效果。", "Rewinds you 3.5s to your position then, restoring that HP, momentum and knockback; breaks chains and clears debuffs."),
    ("短时间内移速提高，可吸收伤害并转化为自身速度（吸收 50% 伤害，每点伤害转 15 速度）。可挣脱锁链。", "Briefly increases move speed and absorbs damage into speed (absorbs 50% of damage, each point becomes 15 speed). Breaks chains."),
    ("（098c 未实装：S011 瞬移的隐藏 B 形态占位）", "(Not in 098c: placeholder for Blink's hidden B form)"),
    ("召唤一道闪电攻击敌人。", "Calls down a bolt of lightning to strike the enemy."),
    ("发射一颗会跟踪敌人的光球。施法者碰撞跟踪球会使其爆炸，对周围敌人造成伤害并令自身加速 2.25 秒。", "Launches an orb that homes in on enemies. If the caster touches it, it explodes, damaging nearby enemies and hasting the caster for 2.25s."),
    ("投掷出一个被魔法强化过的回旋镖，它最终会飞回施法者。", "Throws a magically strengthened boomerang that eventually returns to the caster."),
    ("（非 098c 技能：098c 名册无电弧，已停用）", "(Not a 098c skill: no Arc in the 098c roster; retired)"),
    ("召唤流星来攻击对手。两种模式：陨石（从天而降砸向对手）/ 岩浆（向前滚动，被飞弹击中后变大，命中使术士减速）。", "Calls down a meteor. Two forms: Meteor (drops from the sky onto the enemy) / Magma (rolls forward, grows when hit by missiles, slows the warlock on hit)."),
    ("投掷出一个会分裂为许多小飞弹的光球。两种模式：分裂弹·目标（在目标地爆炸后继续向前分裂）/ 分裂弹·区域（飞向目标途中不断分裂散发）。", "Throws an orb that splits into many small missiles. Two forms: Splitter ·Target (explodes at the target, then keeps splitting forward) / Splitter ·Area (splits continuously while flying)."),
    ("短时间内隐身并增加移动速度。两种模式：疾风步·冲锋（撞向敌人产生伤害并结束隐身）/ 疾风步·隐身（撞到敌人不打断隐身）。", "Briefly turns invisible and faster. Two forms: Windwalk ·Charge (charging an enemy deals damage and ends invisibility) / Windwalk ·Invisibility (bumping an enemy doesn't break it)."),
    ("获悉空间折叠的奥秘，自由施展空间移动。两种模式：瞬间移动（闪现到目标点，随等级提升距离）/ 瞬间移动·镜像（效果相同，但冷却曲线更快）。", "Masters folded space to teleport at will. Two forms: Blink (blink to the target point, range grows with level) / Blink ·Mirror (same effect, faster cooldown curve)."),
    ("短时间内利用法术激增移动速度。两种模式：冲撞·突击（加速撞向目标，命中首个敌人造成伤害）/ 冲撞·凤凰（冲刺中可用移动指令转向，可无限转向）。", "Briefly surges with speed. Two forms: Charge ·Assault (dashes into the target, damaging the first enemy hit) / Charge ·Phoenix (can steer with move commands during the dash, unlimited turning)."),
    ("用精神力标记远处的空间，快速移动至目标地。两种模式：移形换位·置换（发射弹体，命中敌人后与其互换位置）/ 移形换位·搬运（发射弹体，到达后把你传送过去；撞到柱子则与柱子换位）。", "Marks distant space and quickly moves there. Two forms: Relocate ·Swap (fires a bolt; on hitting an enemy, swap places) / Relocate ·Carry (fires a bolt and teleports you to it; hits a pillar swap with it)."),
    ("释放一发可吸血的飞弹。两种模式：汲取·减速（减慢敌方或提高友方速度）/ 汲取·削弱（降低敌方伤害 50%）。", "Fires a lifestealing missile. Two forms: Drain ·Slow (slows enemies or speeds up allies) / Drain ·Weaken (reduces enemy damage by 50%)."),
    ("喷射出多个火球。两种模式：火焰喷射·流射（连续喷射出所有火球）/ 火焰喷射·簇射（一次性喷射出所有火球）。", "Sprays multiple fireballs. Two forms: Flame Spray ·Stream (sprays all fireballs in sequence) / Flame Spray ·Burst (sprays all fireballs at once)."),
    ("发射一个弹力飞弹。两种模式：弹跳弹（在目标间弹跳，每次命中伤害递减 20%）/ 弹跳弹·充能（成功命中则刷新本技能冷却）。", "Fires a bouncing missile. Two forms: Bouncing Shot (bounces between targets, each hit deals 20% less) / Bouncing Shot ·Recharge (a successful hit refreshes the cooldown)."),
    ("禁锢对手，使其无法移动或释放法术。两种模式：禁锢·缠绕（拟态能量化作蔓藤束缚敌人）/ 禁锢·沉默（沉默对手法术并驱散增益减益）。", "Binds the enemy, preventing movement or casting. Two forms: Bind ·Entangle (mimicked energy becomes vines that hold the enemy) / Bind ·Silence (silences spells and dispels buffs/debuffs)."),
    ("掌握引力的奥秘，牵引对方的行动。两种模式：引力·暗物质（对周围飞弹与术士产生吸力并造成伤害）/ 引力·力场（伤害并减速其中敌人，己方进入则回复生命）。", "Masters gravity to pull foes. Two forms: Gravity ·Dark Matter (pulls in nearby missiles and warlocks, dealing damage) / Gravity ·Force Field (damages and slows enemies inside; allies entering are healed)."),
    ("在你和目标之间创造一股力量，连接后对目标造成伤害。两种模式：锁链·钩引（把术士拉向你或把自己拉向柱子）/ 锁链·红链（把你拉向敌人，并附加可切割敌人的红色闪电）。", "Creates a force between you and the target that damages it once connected. Two forms: Chain ·Hook (pull a warlock to you, or pull yourself to a pillar) / Chain ·Red Chain (pull yourself to the enemy and add red lightning that can cut enemies)."),
    ("（暂无描述）", "(No description)"),
    // ---- 道具：家族 / 商店分类 / 精通 ----
    ("速度之靴", "Boots of Speed"), ("头盔", "Helm"), ("斗篷", "Cloak"), ("坠饰", "Amulet"),
    ("怀表", "Pocket Watch"), ("守护之盾", "Guardian Shield"), ("熔岩靴", "Lava Boots"),
    ("鲜血之剑", "Blood Sword"), ("独立物品", "Standalone Item"),
    ("机动", "Mobility"), ("防御续航", "Defense & Sustain"), ("攻击特殊", "Offense & Special"),
    ("生命精通", "Life Mastery"), ("范围精通", "Area Mastery"), ("射程精通", "Range Mastery"), ("背包研究", "Backpack Study"),
    ("造成伤害时回复生命（吸血 +8%/级）。", "Heals you when you deal damage (lifesteal +8% per level)."),
    ("火球爆炸范围与击退力度 +12%/级（需有落点爆炸）。", "Fireball blast radius and knockback +12% per level (requires an impact blast)."),
    ("法术射程与持续 +10%/级（火球系 +15%/级）。", "Spell range and duration +10% per level (fireball family +15%)."),
    ("物品栏容量提升（3 → 6 → 10 格）。", "Increases inventory slots (3 → 6 → 10)."),
    // ---- 道具名称 ----
    ("速度之靴 1", "Boots of Speed 1"), ("速度之靴 2", "Boots of Speed 2"), ("速度之靴 3", "Boots of Speed 3"),
    ("坠饰 1", "Amulet 1"), ("坠饰 2", "Amulet 2"), ("坠饰 3", "Amulet 3"),
    ("斗篷 1", "Cloak 1"), ("斗篷 2", "Cloak 2"), ("斗篷 3", "Cloak 3"),
    ("头盔 1", "Helm 1"), ("头盔 2", "Helm 2"), ("头盔 3", "Helm 3"),
    ("怀表 1", "Pocket Watch 1"), ("怀表 2", "Pocket Watch 2"),
    ("熔岩靴 1", "Lava Boots 1"), ("熔岩靴 2", "Lava Boots 2"), ("熔岩靴 3", "Lava Boots 3"),
    ("死亡面具", "Death Mask"), ("火球法杖", "Fire Staff"),
    ("鲜血之剑 1", "Blood Sword 1"), ("鲜血之剑 2", "Blood Sword 2"),
    ("守护之盾 2", "Guardian Shield 2"),
    // ---- 道具描述 ----
    ("移速 +20；可再升级 2 次", "Move speed +20; upgradable 2 more times"),
    ("生命 +20", "HP +20"),
    ("吸血 24%+受伤回 12%（天罚下翻倍）；回复-0.3/s", "Lifesteal 24% + heal 12% of damage taken (doubled under Smite); regen -0.3/s"),
    ("回复 +0.2/s；可升 2 次", "Regen +0.2/s; upgradable 2 times"),
    ("击退-16% 生命+10 移速-5；不叠加；可升 2 次", "Knockback -16% HP +10 speed -5; doesn't stack; upgradable 2 times"),
    ("击退-24% 生命+15 移速-10；不叠加；可升 1 次", "Knockback -24% HP +15 speed -10; doesn't stack; upgradable once"),
    ("移速 +30；可升 1 次", "Move speed +30; upgradable once"),
    ("移速 +40（满级）", "Move speed +40 (max)"),
    ("回复 +0.4/s（满级）", "Regen +0.4/s (max)"),
    ("回复 +0.3/s；可升 1 次", "Regen +0.3/s; upgradable once"),
    ("击退-32% 生命+20 移速-15；不叠加（满级）", "Knockback -32% HP +20 speed -15; doesn't stack (max)"),
    ("生命 +10；可升 2 次", "HP +10; upgradable 2 times"),
    ("生命 +30 回复 +0.1/s（满级）", "HP +30 regen +0.1/s (max)"),
    ("火球附加点燃(3+0.5Lv/2.5s) 直伤降 5.5+0.5Lv；天罚加倍", "Fireball adds ignite (3+0.5Lv over 2.5s); direct damage -5.5+0.5Lv; doubled under Smite"),
    ("天罚伤害 +1；命中每敌回 2 血；可升 1 次", "Smite damage +1; heal 2 HP per enemy hit; upgradable once"),
    ("天罚伤害 +2；命中每敌回 3 血", "Smite damage +2; heal 3 HP per enemy hit"),
    ("火球命中充能：天罚后5s 受伤-25% 击退-50%；生命-10", "Charged by fireball hits: for 5s after Smite, damage taken -25% and knockback -50%; HP -10"),
    ("火球命中充能：天罚后5s 受伤-75% 击退-50%；生命-10", "Charged by fireball hits: for 5s after Smite, damage taken -75% and knockback -50%; HP -10"),
    ("移速+15；熔岩上天罚激活：熔岩伤-87.5%×3s CD25s；回复-0.1/s；可升 2 次", "Speed +15; Smite on lava activates: lava damage -87.5% for 3s, 25s CD; regen -0.1/s; upgradable 2 times"),
    ("移速+27；熔岩抵抗窗口 4s；回复-0.1/s；可升 1 次", "Speed +27; 4s lava resistance window; regen -0.1/s; upgradable once"),
    ("移速+39；熔岩抵抗窗口 5s；回复-0.1/s（满级）", "Speed +39; 5s lava resistance window; regen -0.1/s (max)"),
    ("增益时长+15% 受沉默-15%；可升 1 次", "Buff duration +15%, silence taken -15%; upgradable once"),
    ("增益时长+25% 受沉默-25%", "Buff duration +25%, silence taken -25%"),
];

/// 惰性构建的哈希索引（只在英文模式下首次调用时构建一次）。
fn en_table() -> &'static HashMap<&'static str, &'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| EN.iter().copied().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 语言切换对 `t`/`tf` 的影响（纯函数版，不动全局状态，可与其它测试并行）。
    #[test]
    fn language_switch_drives_t_and_tf() {
        // 中文：原样返回
        assert_eq!(t_for(Lang::ZhHans, "设置"), "设置");

        // 英文：查表；缺词条回退中文（永不空白）
        assert_eq!(t_for(Lang::En, "设置"), "Settings", "应有英文词条");
        assert_eq!(
            t_for(Lang::En, "这条文案故意没翻译"),
            "这条文案故意没翻译",
            "缺词条应回退中文，不能返回空"
        );
        // `tf` 即使词条缺失也应完成占位替换
        assert_eq!(
            tf_for(Lang::En, "剩余 {secs} 秒", &[("secs", format!("{:.1}", 3.0))]),
            "剩余 3.0 秒"
        );
        // 已有译文的词条：英文 + 占位替换
        assert_eq!(
            tf_for(Lang::En, "本端版本 v{v}，仅同版本可联机", &[("v", "20".to_string())]),
            "Client version v20; only matching versions can play together"
        );
    }

    #[test]
    fn steam_code_mapping_covers_zh_and_english_and_unknown() {
        assert_eq!(Lang::from_steam_code("schinese"), Some(Lang::ZhHans));
        assert_eq!(Lang::from_steam_code("tchinese"), Some(Lang::ZhHans));
        assert_eq!(Lang::from_steam_code("english"), Some(Lang::En));
        assert_eq!(Lang::from_steam_code("japanese"), Some(Lang::En));
        assert_eq!(Lang::from_steam_code("ENGLISH"), Some(Lang::En), "应大小写不敏感");
        assert_eq!(Lang::from_steam_code(""), None);
        assert_eq!(Lang::from_steam_code("klingon"), None);
    }

    #[test]
    fn from_code_roundtrips_local_setting_code() {
        for l in Lang::ALL {
            assert_eq!(Lang::from_code(l.code()), Some(l));
        }
        assert_eq!(Lang::from_code("garbage"), None);
    }

    /// 词条 key 不得重复（重复意味着后一条永远不生效，且容易两条译文打架）。
    #[test]
    fn en_table_has_no_duplicate_keys() {
        let mut seen = std::collections::HashSet::new();
        for (zh, _) in EN {
            assert!(seen.insert(*zh), "英文表里 key 重复：{zh}");
        }
    }

    /// 每条英文译文都非空（空串会让界面变空白）。
    #[test]
    fn en_table_values_are_nonempty() {
        for (zh, en) in EN {
            assert!(!en.trim().is_empty(), "词条 {zh:?} 的英文为空");
        }
    }

    #[test]
    fn lang_pref_cycles_auto_then_languages_and_resolves() {
        // 循环：Auto → 简体中文 → English → Auto
        let mut p = LangPref::Auto;
        p = p.next();
        assert_eq!(p, LangPref::Fixed(Lang::ZhHans));
        p = p.next();
        assert_eq!(p, LangPref::Fixed(Lang::En));
        p = p.next();
        assert_eq!(p, LangPref::Auto, "应循环回自动");
        // 解析：Auto 跟随 Steam，固定语言无视 Steam
        assert_eq!(LangPref::Auto.resolve(Some(Lang::En)), Lang::En, "自动应跟随 Steam");
        assert_eq!(LangPref::Auto.resolve(None), Lang::ZhHans, "无 Steam 时回退中文");
        assert_eq!(
            LangPref::Fixed(Lang::ZhHans).resolve(Some(Lang::En)),
            Lang::ZhHans,
            "手动固定应无视 Steam"
        );
    }

    #[test]
    fn lang_pref_code_roundtrip_and_junk_falls_back_to_auto() {
        for p in LangPref::all() {
            assert_eq!(LangPref::from_code(p.code()), p);
        }
        assert_eq!(LangPref::from_code("nonsense"), LangPref::Auto);
        assert_eq!(LangPref::from_code(""), LangPref::Auto);
    }

    /// 源码扫查：所有 `i18n::t("…")` / `i18n::tf("…")` 的**字面量 key** 必须在词条表里。
    ///
    /// 这是「全量覆盖」的守卫 —— 新增一处显式翻译却忘了补词条时，CI 直接红。
    /// 只能扫字面量（`i18n::t(name)` 这类动态 key 靠回退中文兼容，不在本测试范围）。
    #[test]
    fn every_explicit_t_key_in_source_is_translated() {
        let sources: &[(&str, &str)] = &[
            ("main.rs", include_str!("main.rs")),
            ("steam.rs", include_str!("steam.rs")),
            ("settings_ui.rs", include_str!("settings_ui.rs")),
            ("keys.rs", include_str!("keys.rs")),
            ("layout.rs", include_str!("layout.rs")),
        ];
        let known: std::collections::HashSet<&str> = EN.iter().map(|(k, _)| *k).collect();
        let mut missing = Vec::new();
        for (file, src) in sources {
            for pat in ["i18n::t(\"", "i18n::tf(\""] {
                let mut rest = *src;
                while let Some(pos) = rest.find(pat) {
                    let after = &rest[pos + pat.len()..];
                    if let Some(end) = after.find('"') {
                        let key = &after[..end];
                        if !known.contains(key) {
                            missing.push(format!("{file}: {key}"));
                        }
                    }
                    rest = &rest[pos + pat.len()..];
                }
            }
        }
        assert!(
            missing.is_empty(),
            "以下显式翻译 key 缺少英文词条（请在 EN 表中补充）：\n{}",
            missing.join("\n")
        );
    }
}
