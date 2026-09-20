//! 本机设置（音量 / 静音）。
//!
//! **纯本地**：不进房间设置串（那是 `MatchConfig`，会同步给全房），
//! 也不进 `World` / 快照。以 `key=value` 文本持久化，避免为客户端引入 serde 依赖。

use std::path::{Path, PathBuf};

use crate::i18n::LangPref;

/// 本地设置。音量内部用 `0.0..=1.0`（UI 展示为 0–100）。
#[derive(Clone, Debug, PartialEq)]
pub struct LocalSettings {
    pub master_volume: f32,
    pub sfx_volume: f32,
    pub music_volume: f32,
    pub muted: bool,
    /// 语言偏好：`Auto`（跟随 Steam）默认；也可手动固定为某语言。
    pub lang: LangPref,
    /// 音效包选择：`builtin`（内置占位素材）或包 id（整包覆盖）。见 `audio_pack.rs`。
    pub sfx_pack: String,
    /// BGM 包选择：`off`（关闭）/ `builtin` / 包 id（单包内含分场景）。
    pub music_pack: String,
    /// 发布到创意工坊时是否**复用上次的物品 id**（更新而非新建）。默认开。
    pub workshop_reuse: bool,
    /// 发布可见性：`true`=公开（Public），`false`=私有（Private）。默认公开。
    pub workshop_public: bool,
    /// 包 id → 已发布的创意工坊物品 id（持久化为 `published.<id>=<fileid>` 行）。
    pub published: Vec<(String, u64)>,
    /// 发布目标：`auto`（音效包优先，否则 BGM 包）或某个**本地**包 id。
    pub publish_pack: String,
    /// 8 个技能槽的自定义按键（下标 = `CastKey::as_u32`，顺序 C/R/E/D/Y/T/F/G）。
    pub skill_keys: [char; 8],
    /// 停止移动 + 清空指令队列（默认 `S`）。
    pub key_stop: BindKey,
    /// 镜头回场地中心（默认 `Space`）。
    pub key_cam_center: BindKey,
    /// 镜头跳到自己（默认 `1`）。
    pub key_cam_self: BindKey,
    /// 镜头跟随自身开关（默认 `2`）。
    pub key_cam_follow: BindKey,
    /// 购买/升级（默认 `=`；`回车` 是全局确认键，另计）。
    pub key_buy: BindKey,
    /// 卖出/取消（默认 `退格`）。
    pub key_sell: BindKey,
    /// 静音（默认 `F10`）。
    pub key_mute: BindKey,
}

/// 技能键默认值（`CastKey::ALL` 顺序：C/R/E/D/Y/T/F/G）。
pub const DEFAULT_SKILL_KEYS: [char; 8] = ['c', 'r', 'e', 'd', 'y', 't', 'f', 'g'];

/// 可绑定的**命名键**白名单：字符无法表达、但确实需要可重映射的功能键。
///
/// `Esc`/`Q`/`Tab`/方向键/鼠标/`Shift` 属于**界面固定键**，不进白名单（见 `KEYBINDS_PLAN.md` §1.2）。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum NamedBind {
    Space,
    Enter,
    Delete,
    Backspace,
    F10,
    Home,
    End,
}

impl NamedBind {
    /// 全部命名键（改键捕获时逐一试探）。
    pub const ALL: [NamedBind; 7] = [
        NamedBind::Space,
        NamedBind::Enter,
        NamedBind::Delete,
        NamedBind::Backspace,
        NamedBind::F10,
        NamedBind::Home,
        NamedBind::End,
    ];

    /// 持久化代号（写入 `settings.txt`）。
    pub fn code(self) -> &'static str {
        match self {
            NamedBind::Space => "space",
            NamedBind::Enter => "enter",
            NamedBind::Delete => "delete",
            NamedBind::Backspace => "backspace",
            NamedBind::F10 => "f10",
            NamedBind::Home => "home",
            NamedBind::End => "end",
        }
    }

    /// 由持久化代号解析。
    pub fn parse(s: &str) -> Option<Self> {
        NamedBind::ALL
            .into_iter()
            .find(|n| n.code().eq_ignore_ascii_case(s.trim()))
    }

    /// 界面显示名（键帽文字，中英通用）。
    pub fn label(self) -> &'static str {
        match self {
            NamedBind::Space => "SPACE",
            NamedBind::Enter => "ENTER",
            NamedBind::Delete => "DELETE",
            NamedBind::Backspace => "BACKSPACE",
            NamedBind::F10 => "F10",
            NamedBind::Home => "HOME",
            NamedBind::End => "END",
        }
    }
}

/// 一个绑定值：字符键或命名键。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BindKey {
    Char(char),
    Named(NamedBind),
}

impl BindKey {
    /// 持久化代号（字符小写单字符；命名键见 `NamedBind::code`）。
    pub fn code(self) -> String {
        match self {
            BindKey::Char(c) => c.to_ascii_lowercase().to_string(),
            BindKey::Named(n) => n.code().to_string(),
        }
    }

    /// 界面显示名。
    pub fn label(self) -> String {
        match self {
            BindKey::Char(c) => c.to_ascii_uppercase().to_string(),
            BindKey::Named(n) => n.label().to_string(),
        }
    }

    /// 由持久化代号解析（非法 → `None`）。
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if t.is_empty() {
            return None;
        }
        if let Some(n) = NamedBind::parse(t) {
            return Some(BindKey::Named(n));
        }
        t.chars()
            .next()
            .filter(|c| LocalSettings::valid_bind_char(*c))
            .map(|c| BindKey::Char(c.to_ascii_lowercase()))
    }
}

/// 绑定的**作用域**：同域内键位唯一，跨域允许重叠（界面固定键永远优先）。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BindScope {
    /// 全局（任何界面都生效）。
    Global,
    /// 对战（施法、停止、镜头）。
    Battle,
    /// 学习期（购买/卖出/成长）。
    Learn,
}

/// 可绑动作：8 技能槽 + 停止移动 + 镜头 3 项 + 购买 + 卖出 + 静音。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BindAction {
    Skill(usize),
    Stop,
    CamCenter,
    CamSelf,
    CamFollow,
    Buy,
    Sell,
    Mute,
}

impl BindAction {
    /// 该动作的作用域（冲突判定按域划分）。
    pub fn scope(self) -> BindScope {
        match self {
            BindAction::Skill(_)
            | BindAction::Stop
            | BindAction::CamCenter
            | BindAction::CamSelf
            | BindAction::CamFollow => BindScope::Battle,
            BindAction::Buy | BindAction::Sell => BindScope::Learn,
            BindAction::Mute => BindScope::Global,
        }
    }

    /// 是否只能绑字符键（技能槽要在学习页用来选技能树，不支持命名键）。
    pub fn char_only(self) -> bool {
        matches!(self, BindAction::Skill(_))
    }
}

/// 全部可绑动作（UI 列表顺序：技能 → 对战 → 镜头 → 商店 → 系统）。
pub const BIND_ACTIONS: [BindAction; 15] = [
    BindAction::Skill(0),
    BindAction::Skill(1),
    BindAction::Skill(2),
    BindAction::Skill(3),
    BindAction::Skill(4),
    BindAction::Skill(5),
    BindAction::Skill(6),
    BindAction::Skill(7),
    BindAction::Stop,
    BindAction::CamCenter,
    BindAction::CamSelf,
    BindAction::CamFollow,
    BindAction::Buy,
    BindAction::Sell,
    BindAction::Mute,
];

/// 各动作默认键 —— **与改动前完全一致**，保证老玩家零迁移。
pub const DEFAULT_KEY_STOP: BindKey = BindKey::Char('s');
pub const DEFAULT_KEY_CAM_CENTER: BindKey = BindKey::Named(NamedBind::Space);
pub const DEFAULT_KEY_CAM_SELF: BindKey = BindKey::Char('1');
pub const DEFAULT_KEY_CAM_FOLLOW: BindKey = BindKey::Char('2');
pub const DEFAULT_KEY_BUY: BindKey = BindKey::Char('=');
pub const DEFAULT_KEY_SELL: BindKey = BindKey::Named(NamedBind::Backspace);
pub const DEFAULT_KEY_MUTE: BindKey = BindKey::Named(NamedBind::F10);

impl Default for LocalSettings {
    fn default() -> Self {
        Self {
            master_volume: 1.0,
            sfx_volume: 1.0,
            music_volume: 1.0,
            muted: false,
            lang: LangPref::Auto,
            sfx_pack: "builtin".to_string(),
            music_pack: "off".to_string(),
            workshop_reuse: true,
            workshop_public: true,
            published: Vec::new(),
            publish_pack: "auto".to_string(),
            skill_keys: DEFAULT_SKILL_KEYS,
            key_stop: DEFAULT_KEY_STOP,
            key_cam_center: DEFAULT_KEY_CAM_CENTER,
            key_cam_self: DEFAULT_KEY_CAM_SELF,
            key_cam_follow: DEFAULT_KEY_CAM_FOLLOW,
            key_buy: DEFAULT_KEY_BUY,
            key_sell: DEFAULT_KEY_SELL,
            key_mute: DEFAULT_KEY_MUTE,
        }
    }
}

fn clamp01(v: f32) -> f32 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(0.0, 1.0)
    }
}

impl LocalSettings {
    /// 实际音效音量（含总音量与静音）。
    pub fn effective_sfx(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.master_volume * self.sfx_volume
        }
    }

    /// 实际音乐音量（含总音量与静音）。BGM 尚未接入，暂未使用。
    #[allow(dead_code)]
    pub fn effective_music(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.master_volume * self.music_volume
        }
    }

    /// 切换静音（`F10`）。返回切换后的状态。
    pub fn toggle_mute(&mut self) -> bool {
        self.muted = !self.muted;
        self.muted
    }

    /// 某包上次发布到的创意工坊物品 id（无则 `None`）。
    #[cfg_attr(not(feature = "steam"), allow(dead_code))]
    pub fn published_id(&self, pack_id: &str) -> Option<u64> {
        self.published.iter().find(|(k, _)| k == pack_id).map(|(_, v)| *v)
    }

    /// 记录某包已发布到的物品 id。
    #[cfg_attr(not(feature = "steam"), allow(dead_code))]
    pub fn set_published(&mut self, pack_id: &str, file_id: u64) {
        if let Some(e) = self.published.iter_mut().find(|(k, _)| k == pack_id) {
            e.1 = file_id;
        } else {
            self.published.push((pack_id.to_string(), file_id));
        }
    }

    /// 清除某包的发布记录（复用失败/想重建时）。
    #[cfg_attr(not(feature = "steam"), allow(dead_code))]
    pub fn clear_published(&mut self, pack_id: &str) {
        self.published.retain(|(k, _)| k != pack_id);
    }

    /// 某槽当前绑定键。
    pub fn skill_key(&self, idx: usize) -> char {
        self.skill_keys.get(idx).copied().unwrap_or('?')
    }

    /// 某可绑动作当前的键。
    pub fn bind_key(&self, a: BindAction) -> BindKey {
        match a {
            BindAction::Skill(i) => BindKey::Char(self.skill_key(i)),
            BindAction::Stop => self.key_stop,
            BindAction::CamCenter => self.key_cam_center,
            BindAction::CamSelf => self.key_cam_self,
            BindAction::CamFollow => self.key_cam_follow,
            BindAction::Buy => self.key_buy,
            BindAction::Sell => self.key_sell,
            BindAction::Mute => self.key_mute,
        }
    }

    /// 设置某可绑动作的键（技能槽只接受字符键，命名键会被忽略）。
    pub fn set_bind(&mut self, a: BindAction, k: BindKey) {
        match a {
            BindAction::Skill(i) => {
                if let BindKey::Char(c) = k {
                    if i < self.skill_keys.len() {
                        self.skill_keys[i] = c.to_ascii_lowercase();
                    }
                }
            }
            BindAction::Stop => self.key_stop = k,
            BindAction::CamCenter => self.key_cam_center = k,
            BindAction::CamSelf => self.key_cam_self = k,
            BindAction::CamFollow => self.key_cam_follow = k,
            BindAction::Buy => self.key_buy = k,
            BindAction::Sell => self.key_sell = k,
            BindAction::Mute => self.key_mute = k,
        }
    }

    /// 该动作是否接受这个绑定值（技能槽不接受命名键；字符键需在白名单内）。
    pub fn accepts(a: BindAction, k: BindKey) -> bool {
        match k {
            BindKey::Char(c) => Self::valid_bind_char(c),
            BindKey::Named(_) => !a.char_only(),
        }
    }

    /// 若 `k` 已被**同一作用域**内的其它可绑动作占用，返回那个动作。
    ///
    /// 跨作用域允许重叠（例：对战的 `c` 与大厅的 `c` 互不影响），
    /// 界面**固定键**不在此表内（它们永远优先，见 `KEYBINDS_PLAN.md` §5）。
    pub fn bind_conflict(&self, a: BindAction, k: BindKey) -> Option<BindAction> {
        let scope = a.scope();
        BIND_ACTIONS
            .iter()
            .copied()
            .find(|b| *b != a && b.scope() == scope && self.bind_key(*b) == k)
    }

    /// 恢复全部可绑键为默认。
    pub fn reset_binds(&mut self) {
        self.skill_keys = DEFAULT_SKILL_KEYS;
        self.key_stop = DEFAULT_KEY_STOP;
        self.key_cam_center = DEFAULT_KEY_CAM_CENTER;
        self.key_cam_self = DEFAULT_KEY_CAM_SELF;
        self.key_cam_follow = DEFAULT_KEY_CAM_FOLLOW;
        self.key_buy = DEFAULT_KEY_BUY;
        self.key_sell = DEFAULT_KEY_SELL;
        self.key_mute = DEFAULT_KEY_MUTE;
    }

    /// 键位是否合法：字母/数字，或几个常用符号（便于绑 `=`/`-` 等）。
    pub fn valid_bind_char(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || matches!(ch, '=' | '-' | '[' | ']' | ';' | '\'' | ',' | '.' | '/')
    }
}

/// 解析 `key=value` 文本；未知键 / 非法值忽略，缺失项用默认。
pub fn parse(text: &str) -> LocalSettings {
    let mut s = LocalSettings::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "master_volume" => {
                if let Ok(x) = v.parse::<f32>() {
                    s.master_volume = clamp01(x);
                }
            }
            "sfx_volume" => {
                if let Ok(x) = v.parse::<f32>() {
                    s.sfx_volume = clamp01(x);
                }
            }
            "music_volume" => {
                if let Ok(x) = v.parse::<f32>() {
                    s.music_volume = clamp01(x);
                }
            }
            "muted" => {
                s.muted = matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on");
            }
            "lang" => {
                s.lang = LangPref::from_code(v);
            }
            "sfx_pack" => s.sfx_pack = v.to_string(),
            "music_pack" => s.music_pack = v.to_string(),
            "publish_pack" => s.publish_pack = v.to_string(),
            "key_stop" => {
                if let Some(k) = BindKey::parse(v) {
                    s.key_stop = k;
                }
            }
            "key_cam_center" => {
                if let Some(k) = BindKey::parse(v) {
                    s.key_cam_center = k;
                }
            }
            "key_cam_self" => {
                if let Some(k) = BindKey::parse(v) {
                    s.key_cam_self = k;
                }
            }
            "key_cam_follow" => {
                if let Some(k) = BindKey::parse(v) {
                    s.key_cam_follow = k;
                }
            }
            "key_buy" => {
                if let Some(k) = BindKey::parse(v) {
                    s.key_buy = k;
                }
            }
            "key_sell" => {
                if let Some(k) = BindKey::parse(v) {
                    s.key_sell = k;
                }
            }
            "key_mute" => {
                if let Some(k) = BindKey::parse(v) {
                    s.key_mute = k;
                }
            }
            "skill_keys" => {
                let mut it = v.split(',').map(|t| t.trim());
                for slot in s.skill_keys.iter_mut() {
                    if let Some(t) = it.next() {
                        if let Some(c) = t.chars().next().filter(|c| LocalSettings::valid_bind_char(*c)) {
                            *slot = c.to_ascii_lowercase();
                        }
                    }
                }
            }
            "workshop_reuse" => {
                s.workshop_reuse = matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on");
            }
            "workshop_public" => {
                s.workshop_public = matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on");
            }
            k if k.starts_with("published.") => {
                if let Ok(id) = v.parse::<u64>() {
                    s.published.push((k["published.".len()..].to_string(), id));
                }
            }
            _ => {}
        }
    }
    s
}

/// 序列化为 `key=value` 文本（固定行序，便于人读/手改）。
pub fn serialize(s: &LocalSettings) -> String {
    let mut out = format!(
        "master_volume={}\nsfx_volume={}\nmusic_volume={}\nmuted={}\nlang={}\nsfx_pack={}\nmusic_pack={}\nworkshop_reuse={}\nworkshop_public={}\npublish_pack={}\nskill_keys={}\nkey_stop={}\nkey_cam_center={}\nkey_cam_self={}\nkey_cam_follow={}\nkey_buy={}\nkey_sell={}\nkey_mute={}\n",
        s.master_volume,
        s.sfx_volume,
        s.music_volume,
        if s.muted { 1 } else { 0 },
        s.lang.code(),
        s.sfx_pack,
        s.music_pack,
        if s.workshop_reuse { 1 } else { 0 },
        if s.workshop_public { 1 } else { 0 },
        s.publish_pack,
        s.skill_keys.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(","),
        s.key_stop.code(),
        s.key_cam_center.code(),
        s.key_cam_self.code(),
        s.key_cam_follow.code(),
        s.key_buy.code(),
        s.key_sell.code(),
        s.key_mute.code()
    );
    for (id, fid) in &s.published {
        out.push_str(&format!("published.{id}={fid}\n"));
    }
    out
}

/// 默认存储路径：`%APPDATA%/warlock_brawl/settings.txt`，取不到则退回当前目录。
pub fn default_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join("warlock_brawl").join("settings.txt");
    }
    PathBuf::from("settings.txt")
}

/// 读取；文件不存在或读取失败 → 默认值。
pub fn load(path: &Path) -> LocalSettings {
    match std::fs::read_to_string(path) {
        Ok(t) => parse(&t),
        Err(_) => LocalSettings::default(),
    }
}

/// 写入；失败只记一行日志（不影响游戏）。
pub fn save(path: &Path, s: &LocalSettings) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(path, serialize(s)) {
        eprintln!("[settings] 保存失败（忽略）：{e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_full_volume_unmuted() {
        let s = LocalSettings::default();
        assert_eq!(s.master_volume, 1.0);
        assert_eq!(s.sfx_volume, 1.0);
        assert_eq!(s.music_volume, 1.0);
        assert!(!s.muted);
    }

    #[test]
    fn roundtrip_preserves_values() {
        let s = LocalSettings {
            master_volume: 0.5,
            sfx_volume: 0.25,
            music_volume: 0.0,
            muted: true,
            lang: LangPref::Fixed(crate::i18n::Lang::En),
            sfx_pack: "MyPack".to_string(),
            music_pack: "BigMusic".to_string(),
            workshop_reuse: false,
            workshop_public: false,
            published: vec![("MyPack".to_string(), 42), ("Other".to_string(), 7)],
            publish_pack: "MyPack".to_string(),
            skill_keys: ['q', 'w', 'e', 'r', 'a', 's', 'd', 'f'],
            key_stop: BindKey::Char('x'),
            key_cam_center: BindKey::Named(NamedBind::Home),
            key_cam_self: BindKey::Char('3'),
            key_cam_follow: BindKey::Char('4'),
            key_buy: BindKey::Char('='),
            key_sell: BindKey::Named(NamedBind::Delete),
            key_mute: BindKey::Char('m'),
        };
        let back = parse(&serialize(&s));
        assert_eq!(back, s);
    }

    #[test]
    fn skill_keys_parse_validate_and_conflict() {
        // 默认：与改动前一致（老玩家零迁移）。
        let d = LocalSettings::default();
        assert_eq!(d.skill_key(0), 'c');
        assert_eq!(d.skill_key(7), 'g');
        assert_eq!(d.bind_key(BindAction::Stop), BindKey::Char('s'));
        assert_eq!(d.bind_key(BindAction::CamCenter), BindKey::Named(NamedBind::Space));
        assert_eq!(d.bind_key(BindAction::CamSelf), BindKey::Char('1'));
        assert_eq!(d.bind_key(BindAction::CamFollow), BindKey::Char('2'));
        assert_eq!(d.bind_key(BindAction::Buy), BindKey::Char('='));
        assert_eq!(d.bind_key(BindAction::Sell), BindKey::Named(NamedBind::Backspace));
        assert_eq!(d.bind_key(BindAction::Mute), BindKey::Named(NamedBind::F10));
        // 非法字符 / 缺项 → 对应槽/字段保持默认
        let s = parse("skill_keys=q,1,@,x\nkey_sell=k\nkey_buy=@\nkey_stop=space\n");
        assert_eq!(s.skill_key(0), 'q');
        assert_eq!(s.skill_key(1), '1');
        assert_eq!(s.skill_key(2), 'e', "非法字符 @ → 该槽保持默认");
        assert_eq!(s.skill_key(3), 'x');
        assert_eq!(s.skill_key(4), 'y', "缺项 → 默认");
        assert_eq!(s.bind_key(BindAction::Sell), BindKey::Char('k'));
        assert_eq!(s.bind_key(BindAction::Buy), BindKey::Char('='), "非法 key_buy 回退默认 =");
        assert_eq!(s.bind_key(BindAction::Stop), BindKey::Named(NamedBind::Space), "命名键可作绑定值");
        // 冲突：**同作用域**内唯一，跨作用域允许重叠
        let mut s = LocalSettings::default();
        s.set_bind(BindAction::Skill(0), BindKey::Char('k'));
        assert_eq!(
            s.bind_conflict(BindAction::Skill(1), BindKey::Char('k')),
            Some(BindAction::Skill(0))
        );
        assert_eq!(s.bind_conflict(BindAction::Skill(0), BindKey::Char('k')), None, "自己不算冲突");
        assert_eq!(
            s.bind_conflict(BindAction::Sell, BindKey::Char('=')),
            Some(BindAction::Buy),
            "与购买键冲突（同为 Learn 域）"
        );
        assert_eq!(
            s.bind_conflict(BindAction::Stop, BindKey::Char('k')),
            Some(BindAction::Skill(0)),
            "技能与停止同属 Battle 域 → 冲突"
        );
        assert_eq!(
            s.bind_conflict(BindAction::Mute, BindKey::Char('c')),
            None,
            "静音是 Global 域 → 跨域不冲突"
        );
        assert_eq!(
            s.bind_conflict(BindAction::Buy, BindKey::Char('c')),
            None,
            "购买是 Learn 域 → 跨域不冲突"
        );
        assert!(LocalSettings::valid_bind_char('a') && LocalSettings::valid_bind_char('7'));
        assert!(LocalSettings::valid_bind_char('=') && LocalSettings::valid_bind_char('-'));
        assert!(!LocalSettings::valid_bind_char('@') && !LocalSettings::valid_bind_char(' '));
        // 技能槽不接受命名键；其它动作可以
        assert!(!LocalSettings::accepts(BindAction::Skill(0), BindKey::Named(NamedBind::Space)));
        assert!(LocalSettings::accepts(BindAction::CamCenter, BindKey::Named(NamedBind::Space)));
        // 重置
        s.reset_binds();
        assert_eq!(s.skill_keys, DEFAULT_SKILL_KEYS);
        assert_eq!(s.bind_key(BindAction::Buy), BindKey::Char('='));
        assert_eq!(s.bind_key(BindAction::Sell), DEFAULT_KEY_SELL);
        assert_eq!(s.bind_key(BindAction::Mute), DEFAULT_KEY_MUTE);
        assert_eq!(s.bind_key(BindAction::Stop), DEFAULT_KEY_STOP);
    }

    #[test]
    fn legacy_settings_without_new_keys_keeps_defaults() {
        // 旧版本 settings.txt（无 key_stop/key_cam_* 行，且 key_sell/key_mute 为空）
        let old = "master_volume=0.5\nskill_keys=q,w,e,r,a,s,d,f\nkey_buy=\nkey_sell=\nkey_mute=\n";
        let s = parse(old);
        assert_eq!(s.bind_key(BindAction::Buy), BindKey::Char('='), "空 key_buy → 默认 =");
        assert_eq!(s.bind_key(BindAction::Sell), DEFAULT_KEY_SELL, "空 key_sell → 默认退格");
        assert_eq!(s.bind_key(BindAction::Mute), DEFAULT_KEY_MUTE, "空 key_mute → 默认 F10");
        assert_eq!(s.bind_key(BindAction::Stop), DEFAULT_KEY_STOP, "缺行 → 默认 S");
        assert_eq!(s.bind_key(BindAction::CamCenter), DEFAULT_KEY_CAM_CENTER);
        assert_eq!(s.skill_key(0), 'q', "旧技能键仍应保留");
    }

    #[test]
    fn lang_defaults_to_auto_and_parses_codes() {
        assert_eq!(LocalSettings::default().lang, LangPref::Auto);
        assert_eq!(parse("lang=en\n").lang, LangPref::Fixed(crate::i18n::Lang::En));
        assert_eq!(parse("lang=zh\n").lang, LangPref::Fixed(crate::i18n::Lang::ZhHans));
        assert_eq!(parse("lang=auto\n").lang, LangPref::Auto);
        assert_eq!(parse("lang=bogus\n").lang, LangPref::Auto, "非法值应回退自动");
    }

    #[test]
    fn parse_clamps_and_ignores_junk() {
        let s = parse("# comment\nmaster_volume=2.0\nsfx_volume=-1\nmusic_volume=abc\nmuted=YES\njunk-line\nx=3\n");
        assert_eq!(s.master_volume, 1.0, "超范围应夹到 1.0");
        assert_eq!(s.sfx_volume, 0.0, "负数应夹到 0.0");
        assert_eq!(s.music_volume, 1.0, "非法值应保持默认");
        assert!(s.muted, "muted=YES 应为真");
    }

    #[test]
    fn effective_volume_respects_mute_and_master() {
        let mut s = LocalSettings {
            master_volume: 0.5,
            sfx_volume: 0.5,
            music_volume: 0.8,
            muted: false,
            ..Default::default()
        };
        assert!((s.effective_sfx() - 0.25).abs() < 1e-6);
        assert!((s.effective_music() - 0.4).abs() < 1e-6);
        s.toggle_mute();
        assert_eq!(s.effective_sfx(), 0.0);
        assert_eq!(s.effective_music(), 0.0);
        assert!(s.muted);
    }

    #[test]
    fn missing_file_loads_defaults() {
        let p = std::path::Path::new("definitely_missing_settings_xyz.txt");
        assert_eq!(load(p), LocalSettings::default());
    }
}
