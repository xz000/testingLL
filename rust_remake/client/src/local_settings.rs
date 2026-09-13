//! 本机设置（音量 / 静音）。
//!
//! **纯本地**：不进房间设置串（那是 `MatchConfig`，会同步给全房），
//! 也不进 `World` / 快照。以 `key=value` 文本持久化，避免为客户端引入 serde 依赖。

use std::path::{Path, PathBuf};

/// 本地设置。音量内部用 `0.0..=1.0`（UI 展示为 0–100）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalSettings {
    pub master_volume: f32,
    pub sfx_volume: f32,
    pub music_volume: f32,
    pub muted: bool,
}

impl Default for LocalSettings {
    fn default() -> Self {
        Self {
            master_volume: 1.0,
            sfx_volume: 1.0,
            music_volume: 1.0,
            muted: false,
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
            _ => {}
        }
    }
    s
}

/// 序列化为 `key=value` 文本（固定行序，便于人读/手改）。
pub fn serialize(s: &LocalSettings) -> String {
    format!(
        "master_volume={}\nsfx_volume={}\nmusic_volume={}\nmuted={}\n",
        s.master_volume,
        s.sfx_volume,
        s.music_volume,
        if s.muted { 1 } else { 0 }
    )
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
        };
        let back = parse(&serialize(&s));
        assert_eq!(back, s);
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
