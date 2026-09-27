//! Steam 库 / 创意工坊内容目录探测（**纯 std，不含 Steamworks API，可单测**）。
//!
//! 音频包（`audio_pack`）与图标包（`icon_pack`）共用：两者**本地目录不同**，
//! 但「Steam 装在哪、订阅内容落在哪个库」这套定位逻辑完全一样。
//!
//! **只放纯路径函数**——两边的「包」模型（音效整包覆盖 vs 图标逐键覆盖）**刻意不统一**，
//! 避免为一个通用枚举扩大回归面（见 `ICON_PACK_PLAN.md` §11/§12）。

use std::path::{Path, PathBuf};

/// 从任意可执行文件路径**向上找 `steamapps`**，其父目录即 Steam 库根。
///
/// 例：`X:/Steam/steamapps/common/CircleBrawl/client.exe` → `X:/Steam`。
/// 这样能自动匹配**游戏被安装到哪个库**（Linux/Windows 通用）。
pub fn steam_root_from_exe(exe: &Path) -> Option<PathBuf> {
    for anc in exe.ancestors() {
        if anc.file_name().is_some_and(|n| n.eq_ignore_ascii_case("steamapps")) {
            return anc.parent().map(Path::to_path_buf);
        }
    }
    None
}

/// 给定 Steam 库根，返回本作创意工坊内容目录：`<root>/steamapps/workshop/content/<AppID>`。
pub fn workshop_content_root(steam_root: &Path) -> PathBuf {
    steam_root
        .join("steamapps")
        .join("workshop")
        .join("content")
        .join(crate::appid::app_id_str())
}

/// 从 `libraryfolders.vdf` 文本解析所有 Steam **库根**（处理 `\\` 转义）。
///
/// 形如：`"path"\t\t"D:\\SteamLibrary"`。
pub fn parse_library_paths(vdf: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for line in vdf.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("\"path\"") else {
            continue;
        };
        let rest = rest.trim();
        let Some(inner) = rest.strip_prefix('"').and_then(|s| s.rsplit_once('"')) else {
            continue;
        };
        let p = inner.0.replace("\\\\", "\\");
        if !p.is_empty() {
            out.push(PathBuf::from(p));
        }
    }
    out
}

/// 所有已安装 Steam 库的**本作工坊内容根**（含 `steam_root` 自身 + `libraryfolders.vdf` 里的其它库）。
///
/// 这样即使游戏装在非默认盘（如 `D:\SteamLibrary`）也能找到订阅内容。
pub fn workshop_roots(steam_root: &Path) -> Vec<PathBuf> {
    let mut libs = vec![steam_root.to_path_buf()];
    let vdf = steam_root.join("steamapps").join("libraryfolders.vdf");
    if let Ok(text) = std::fs::read_to_string(&vdf) {
        for p in parse_library_paths(&text) {
            if !libs.contains(&p) {
                libs.push(p);
            }
        }
    }
    libs.into_iter().map(|l| workshop_content_root(&l)).collect()
}

/// 探测 Steam 库根（全部基于文件系统/环境变量，**不需要 Steamworks API**）：
/// 1. 从当前可执行文件向上找 `steamapps`（Steam 启动的游戏最常见）；
/// 2. 环境变量 `STEAM_PATH`（玩家自定义）；
/// 3. 常规安装路径 `%ProgramFiles(x86)%/Steam`、`%ProgramFiles%/Steam`。
pub fn detect_steam_root() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = steam_root_from_exe(&exe) {
            return Some(root);
        }
    }
    if let Ok(p) = std::env::var("STEAM_PATH") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    for key in ["ProgramFiles(x86)", "ProgramFiles", "ProgramW6432"] {
        if let Ok(pf) = std::env::var(key) {
            let cand = PathBuf::from(pf).join("Steam");
            if cand.is_dir() {
                return Some(cand);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_paths_detected_from_exe() {
        let exe = PathBuf::from("A")
            .join("Steam")
            .join("steamapps")
            .join("common")
            .join("CircleBrawl")
            .join("client.exe");
        let root = steam_root_from_exe(&exe).expect("应能从 steamapps 祖先推出库根");
        assert_eq!(root, PathBuf::from("A").join("Steam"));
        assert!(steam_root_from_exe(Path::new("not/under/steam.exe")).is_none());
        let ws = workshop_content_root(&root);
        // AppID 由 feature 决定（正式版 908660 / demo 1042120）→ 不写死具体数字。
        assert!(ws.ends_with(
            Path::new("steamapps")
                .join("workshop")
                .join("content")
                .join(crate::appid::app_id_str())
        ));
    }

    #[test]
    fn parse_libraryfolders_vdf_paths() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}\n";
        let paths = parse_library_paths(vdf);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from("C:\\Program Files (x86)\\Steam"));
        assert_eq!(paths[1], PathBuf::from("D:\\SteamLibrary"));
    }
}
