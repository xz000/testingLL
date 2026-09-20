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
    /// 购买/升级的额外绑定字符（默认 `=`；`回车` 恒为固定别名）。空串 = 未绑。
    pub key_buy: String,
    /// 卖出/取消的额外绑定字符（默认未绑；`退格/Delete` 恒为固定别名）。
    pub key_sell: String,
    /// 静音的额外绑定字符（默认未绑；`F10` 恒为固定别名）。
    pub key_mute: String,
}

/// 技能键默认值（`CastKey::ALL` 顺序：C/R/E/D/Y/T/F/G）。
pub const DEFAULT_SKILL_KEYS: [char; 8] = ['c', 'r', 'e', 'd', 'y', 't', 'f', 'g'];
/// 购买/升级默认键。
pub const DEFAULT_KEY_BUY: char = '=';

/// 可绑动作（Tier 1+2）：8 技能槽 + 购买/升级 + 卖出/取消 + 静音。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BindAction {
    Skill(usize),
    Buy,
    Sell,
    Mute,
}

/// 全部可绑动作（UI 列表顺序）。
pub const BIND_ACTIONS: [BindAction; 11] = [
    BindAction::Skill(0),
    BindAction::Skill(1),
    BindAction::Skill(2),
    BindAction::Skill(3),
    BindAction::Skill(4),
    BindAction::Skill(5),
    BindAction::Skill(6),
    BindAction::Skill(7),
    BindAction::Buy,
    BindAction::Sell,
    BindAction::Mute,
];

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
            key_buy: DEFAULT_KEY_BUY.to_string(),
            key_sell: String::new(),
            key_mute: String::new(),
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

    /// 某可绑动作当前的键（`None` = 未绑）。
    pub fn bind_char(&self, a: BindAction) -> Option<char> {
        match a {
            BindAction::Skill(i) => self.skill_keys.get(i).copied(),
            BindAction::Buy => self.key_buy.chars().next(),
            BindAction::Sell => self.key_sell.chars().next(),
            BindAction::Mute => self.key_mute.chars().next(),
        }
    }

    /// 设置某可绑动作的键（`None` = 解绑）。
    pub fn set_bind(&mut self, a: BindAction, ch: Option<char>) {
        let s = ch
            .map(|c| c.to_ascii_lowercase().to_string())
            .unwrap_or_default();
        match a {
            BindAction::Skill(i) => {
                if let Some(c) = ch {
                    if i < self.skill_keys.len() {
                        self.skill_keys[i] = c.to_ascii_lowercase();
                    }
                }
            }
            BindAction::Buy => self.key_buy = s,
            BindAction::Sell => self.key_sell = s,
            BindAction::Mute => self.key_mute = s,
        }
    }

    /// 若 `ch` 已被**其它**可绑动作占用，返回那个动作。
    pub fn bind_conflict(&self, a: BindAction, ch: char) -> Option<BindAction> {
        let ch = ch.to_ascii_lowercase();
        BIND_ACTIONS
            .iter()
            .copied()
            .find(|b| *b != a && self.bind_char(*b) == Some(ch))
    }

    /// 恢复全部可绑键为默认。
    pub fn reset_binds(&mut self) {
        self.skill_keys = DEFAULT_SKILL_KEYS;
        self.key_buy = DEFAULT_KEY_BUY.to_string();
        self.key_sell = String::new();
        self.key_mute = String::new();
    }

    /// 键位是否合法：字母/数字，或几个常用符号（便于绑 `=`/`-` 等）。
    pub fn valid_bind_char(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || matches!(ch, '=' | '-' | '[' | ']' | ';' | '\'' | ',' | '.' | '/')
    }
}

/// 解析 `key=value` 文本；未知键 / 非法值忽略，缺失项用默认。
pub fn parse(text: &str) -> LocalSettings {
    let mut s = LocalSettings::default();
    let norm_bind = |v: &str| -> String {
        v.chars()
            .next()
            .filter(|c| LocalSettings::valid_bind_char(*c))
            .map(|c| c.to_ascii_lowercase().to_string())
            .unwrap_or_default()
    };
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
            "key_buy" => {
                let t = norm_bind(v);
                s.key_buy = if t.is_empty() { DEFAULT_KEY_BUY.to_string() } else { t };
            }
            "key_sell" => s.key_sell = norm_bind(v),
            "key_mute" => s.key_mute = norm_bind(v),
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
        "master_volume={}\nsfx_volume={}\nmusic_volume={}\nmuted={}\nlang={}\nsfx_pack={}\nmusic_pack={}\nworkshop_reuse={}\nworkshop_public={}\npublish_pack={}\nskill_keys={}\nkey_buy={}\nkey_sell={}\nkey_mute={}\n",
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
        s.key_buy,
        s.key_sell,
        s.key_mute
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
            key_buy: "=".to_string(),
            key_sell: "z".to_string(),
            key_mute: "m".to_string(),
        };
        let back = parse(&serialize(&s));
        assert_eq!(back, s);
    }

    #[test]
    fn skill_keys_parse_validate_and_conflict() {
        // 默认
        let d = LocalSettings::default();
        assert_eq!(d.skill_key(0), 'c');
        assert_eq!(d.skill_key(7), 'g');
        assert_eq!(d.bind_char(BindAction::Buy), Some('='));
        assert_eq!(d.bind_char(BindAction::Sell), None, "卖出默认未绑（仍可用退格/Delete）");
        assert_eq!(d.bind_char(BindAction::Mute), None, "静音默认未绑（仍可用 F10）");
        // 非法字符/缺项 → 保持默认对应槽
        let s = parse("skill_keys=q,1,@,x\nkey_sell=k\nkey_buy=@\n");
        assert_eq!(s.skill_key(0), 'q');
        assert_eq!(s.skill_key(1), '1');
        assert_eq!(s.skill_key(2), 'e', "非法字符 @ → 该槽保持默认");
        assert_eq!(s.skill_key(3), 'x');
        assert_eq!(s.skill_key(4), 'y', "缺项 → 默认");
        assert_eq!(s.bind_char(BindAction::Sell), Some('k'));
        assert_eq!(s.bind_char(BindAction::Buy), Some('='), "非法 key_buy 回退默认 =");
        // 冲突检测（跨全部可绑动作）
        let mut s = LocalSettings::default();
        s.set_bind(BindAction::Skill(0), Some('k'));
        assert_eq!(s.bind_conflict(BindAction::Skill(1), 'k'), Some(BindAction::Skill(0)));
        assert_eq!(s.bind_conflict(BindAction::Skill(0), 'k'), None, "自己不算冲突");
        assert_eq!(s.bind_conflict(BindAction::Sell, '='), Some(BindAction::Buy), "与购买键冲突");
        assert!(LocalSettings::valid_bind_char('a') && LocalSettings::valid_bind_char('7'));
        assert!(LocalSettings::valid_bind_char('=') && LocalSettings::valid_bind_char('-'));
        assert!(!LocalSettings::valid_bind_char('@') && !LocalSettings::valid_bind_char(' '));
        // 重置
        s.reset_binds();
        assert_eq!(s.skill_keys, DEFAULT_SKILL_KEYS);
        assert_eq!(s.bind_char(BindAction::Buy), Some('='));
        assert_eq!(s.bind_char(BindAction::Sell), None);
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
