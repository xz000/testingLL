//! 外部音频包（音效 / BGM）发现与解析：**纯 `std`，不依赖 ggez / Steam，可单测**。
//!
//! 设计约定（见 `AUDIO_PLAN.md` §7）：
//! - 一个「音频包」= 一个目录，内含 `sfx/<cue>.<ext>`（音效）与/或 `bgm/<scene>.<ext>`（BGM）。
//! - 可选清单 `circle_brawl_pack.ini`（`name/author/version/type/description`）；缺失则按目录布局推断类型。
//! - **音效包整包覆盖**（单槽）；**BGM 单包内含分场景**（场景：`menu/lobby/battle/result`）。
//! - 选择在 `LocalSettings`（`sfx_pack` / `music_pack`），值 = `builtin` / `off` / 包 id。
//!
//! 目录来源：
//! - 本地：`%APPDATA%/warlock_brawl/audio/<id>/`（玩家手动放，**不依赖 Steam**）
//! - 创意工坊：`<Steam>/steamapps/workshop/content/908660/<id>/`（`detect_steam_root` 定位，不需 Steamworks API）

use std::path::{Path, PathBuf};

/// 包清单文件名。
pub const MANIFEST_NAME: &str = "circle_brawl_pack.ini";
/// 音效扩展名搜索优先级（WAV 优先，见 `AUDIO_PLAN.md` §7.2）。
pub const SFX_EXTS: &[&str] = &["wav", "ogg", "flac", "mp3"];
/// BGM 扩展名搜索优先级（Ogg Vorbis 优先，无缝循环）。
pub const BGM_EXTS: &[&str] = &["ogg", "flac", "wav", "mp3"];

/// 选择值：跟随内置占位素材。
pub const PACK_BUILTIN: &str = "builtin";
/// 选择值：关闭该路音频（BGM 常用）。
pub const PACK_OFF: &str = "off";

/// 包类型。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PackKind {
    Sound,
    Music,
    Both,
}

impl PackKind {
    pub fn has_sfx(self) -> bool {
        matches!(self, PackKind::Sound | PackKind::Both)
    }

    pub fn has_bgm(self) -> bool {
        matches!(self, PackKind::Music | PackKind::Both)
    }
}

/// BGM 场景（单 BGM 包内按此命名文件）。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MusicScene {
    Menu,
    Lobby,
    Battle,
    Result,
}

impl MusicScene {
    /// 全部场景（生成说明文件时遍历用）。
    pub const ALL: [MusicScene; 4] =
        [MusicScene::Menu, MusicScene::Lobby, MusicScene::Battle, MusicScene::Result];

    /// 文件名（无扩展名）。
    pub fn key(self) -> &'static str {
        match self {
            MusicScene::Menu => "menu",
            MusicScene::Lobby => "lobby",
            MusicScene::Battle => "battle",
            MusicScene::Result => "result",
        }
    }
}

/// 由顶层状态推导当前 BGM 场景（纯函数，便于单测）。
///
/// - 主菜单（含其上的设置界面）→ [`MusicScene::Menu`]
/// - 对局已结束（`MatchPhase::Finished`）→ [`MusicScene::Result`]
/// - 开局配置期（尚未开始第一轮 / 首次商店）→ [`MusicScene::Lobby`]
/// - 其余（对局进行中、轮间商店）→ [`MusicScene::Battle`]
pub fn scene_for(is_menu: bool, pre_game_config: bool, finished: bool) -> MusicScene {
    if is_menu {
        MusicScene::Menu
    } else if finished {
        MusicScene::Result
    } else if pre_game_config {
        MusicScene::Lobby
    } else {
        MusicScene::Battle
    }
}

/// 一个已发现的音频包。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pack {
    /// 稳定 id（目录名；创意工坊即 PublishedFileId）。
    pub id: String,
    pub root: PathBuf,
    pub name: String,
    pub author: String,
    pub version: String,
    pub kind: PackKind,
}

impl Pack {
    /// UI 显示名：`名称 (作者)`，无作者则只显示名称。
    pub fn display(&self) -> String {
        if self.author.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.author)
        }
    }
}

/// 清单位（解析结果；缺失项为默认空串 / `None` 类型）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub author: String,
    pub version: String,
    pub kind: Option<PackKind>,
    pub description: String,
}

/// 解析清单文本（宽松）：`key=value` 行，`#`/`;` 注释，未知键忽略，大小写不敏感。
pub fn parse_manifest(text: &str) -> Manifest {
    let mut m = Manifest::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim().to_string();
        match k.as_str() {
            "name" => m.name = v,
            "author" => m.author = v,
            "version" => m.version = v,
            "description" => m.description = v,
            "type" | "kind" => {
                m.kind = match v.to_ascii_lowercase().as_str() {
                    "sound" | "sfx" => Some(PackKind::Sound),
                    "music" | "bgm" => Some(PackKind::Music),
                    "both" | "audio" => Some(PackKind::Both),
                    _ => None,
                }
            }
            _ => {}
        }
    }
    m
}

/// 该目录下是否存在子目录 `name`。
fn has_subdir(root: &Path, name: &str) -> bool {
    root.join(name).is_dir()
}

/// 在 `root/<subdir>/<stem>.<ext>` 中按 `exts` 顺序找第一个存在的文件。
pub fn resolve(root: &Path, subdir: &str, stem: &str, exts: &[&str]) -> Option<PathBuf> {
    for ext in exts {
        let p = root.join(subdir).join(format!("{stem}.{ext}"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// 解析某包内的音效文件（`<root>/sfx/<stem>.<ext>`，WAV 优先）。
pub fn resolve_sfx(root: &Path, stem: &str) -> Option<PathBuf> {
    resolve(root, "sfx", stem, SFX_EXTS)
}

/// 解析某包内的 BGM 文件（`<root>/bgm/<scene>.<ext>`，Ogg Vorbis 优先）。
pub fn resolve_bgm(root: &Path, scene: MusicScene) -> Option<PathBuf> {
    resolve(root, "bgm", scene.key(), BGM_EXTS)
}

/// 包封面 / 创意工坊预览图：`<root>/preview.<png|jpg|jpeg|gif>`（可选）。
/// 发布到创意工坊时作为物品预览图（部分 App 要求提交时带预览）。
#[cfg_attr(not(feature = "steam"), allow(dead_code))]
pub fn preview_path(root: &Path) -> Option<PathBuf> {
    for ext in ["png", "jpg", "jpeg", "gif"] {
        let p = root.join(format!("preview.{ext}"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// 该包提供的**音效数量**（`stems` 中能在 `sfx/` 解析到的个数；用于设置页详情）。
pub fn sfx_coverage(root: &Path, stems: &[&str]) -> usize {
    stems.iter().filter(|s| resolve_sfx(root, s).is_some()).count()
}

/// 该包提供的 **BGM 场景列表**（只列实际有文件的场景，用于设置页详情）。
pub fn bgm_scenes(root: &Path) -> Vec<&'static str> {
    MusicScene::ALL
        .iter()
        .filter(|s| resolve_bgm(root, **s).is_some())
        .map(|s| s.key())
        .collect()
}

/// 生成一个**示例包**（结构 + 清单 + README + 两个静音 WAV），返回包目录。
/// 便于玩家照拄；已有则覆盖文件（不删目录）。
pub fn write_example_pack(root: &Path) -> std::io::Result<PathBuf> {
    let dir = root.join("ExamplePack");
    std::fs::create_dir_all(dir.join("sfx"))?;
    std::fs::create_dir_all(dir.join("bgm"))?;
    std::fs::write(
        dir.join(MANIFEST_NAME),
        "name=Example Pack\nauthor=You\nversion=1\ntype=both\ndescription=示例包：把 sfx/ 与 bgm/ 里的文件换成你的音频。\n",
    )?;
    // 两个静音 WAV 占位（换成你自己的文件即可；推荐音效 WAV、BGM Ogg）。
    std::fs::write(dir.join("sfx/ui_confirm.wav"), silent_wav(0.1))?;
    std::fs::write(dir.join("bgm/menu.wav"), silent_wav(0.5))?;
    std::fs::write(
        dir.join("README.txt"),
        "ExamplePack（示例音频包）\n\n  sfx/<音效名>.wav  音效（推荐 WAV，回退 ogg/flac/mp3）\n  bgm/<场景>.ogg    场景：menu / lobby / battle / result（推荐 Ogg Vorbis）\n\n清单 type 仅作提示，能力以目录为准。\n",
    )?;
    Ok(dir)
}

/// 生成一个静音 WAV（16-bit PCM，单声道）的最小字节串（占位用）。
fn silent_wav(secs: f32) -> Vec<u8> {
    let rate = 44100u32;
    let samples = (rate as f32 * secs.max(0.0)) as u32;
    let data_len = samples * 2;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&1u16.to_le_bytes()); // mono
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&(rate * 2).to_le_bytes()); // byte rate
    v.extend_from_slice(&2u16.to_le_bytes()); // block align
    v.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    v.resize(44 + data_len as usize, 0);
    v
}

/// 扫描一个根目录下的所有子目录，识别为音频包。
///
/// 规则：
/// - 只认**含 `sfx/` 或 `bgm/` 子目录**的目录（无音频内容则跳过，避免误把杂物当包）。
/// - **能力（kind）以实际目录为准**：有 `sfx/` → 提供音效；有 `bgm/` → 提供 BGM；两者→`Both`。
///   清单 `type` 只作**校验/提示**（与目录不一致时打日志），不再“覆盖”真实能力
///   （否则会出现“有 bgm 却不出现在 BGM 列表”或“列在 BGM 列表却放不出 BGM”）。
/// - 清单缺失/损坏 → 用目录名当显示名。
pub fn discover_root(root: &Path, out: &mut Vec<Pack>) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    entries.sort(); // 稳定顺序（UI 列表可预期）
    for dir in entries {
        let has_sfx = has_subdir(&dir, "sfx");
        let has_bgm = has_subdir(&dir, "bgm");
        if !has_sfx && !has_bgm {
            continue;
        }
        let manifest = std::fs::read_to_string(dir.join(MANIFEST_NAME))
            .map(|t| parse_manifest(&t))
            .unwrap_or_default();
        let inferred = match (has_sfx, has_bgm) {
            (true, true) => PackKind::Both,
            (false, true) => PackKind::Music,
            _ => PackKind::Sound,
        };
        // 清单 type 与目录不一致时提示（不改变能力）。
        if let Some(declared) = manifest.kind {
            if declared != inferred {
                eprintln!(
                    "[audio] 包 {dir:?} 清单 type={declared:?} 与目录 {inferred:?} 不一致 → 以目录为准"
                );
            }
        }
        let kind = inferred;
        let id = dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "?".to_string());
        let name = if manifest.name.is_empty() { id.clone() } else { manifest.name };
        out.push(Pack {
            id,
            root: dir,
            name,
            author: manifest.author,
            version: manifest.version,
            kind,
        });
    }
}

/// 扫描多个根目录（后者可为空/不存在，静默跳过）。同名 id 以**先出现的根**为准（本地优先）。
pub fn discover(roots: &[PathBuf]) -> Vec<Pack> {
    let mut out = Vec::new();
    for root in roots {
        discover_root(root, &mut out);
    }
    // 去重：同 id 只保留第一个（本地根在前 → 本地覆盖工坊）。
    let mut seen = std::collections::HashSet::new();
    out.retain(|p| seen.insert(p.id.clone()));
    out
}

/// 本作 Steam AppID（与 `steam::APP_ID` 一致）：创意工坊内容目录用。
pub const APP_ID: &str = "908660";

/// 本地音频包根目录：`%APPDATA%/warlock_brawl/audio`（取不到 APPDATA 则退回当前目录）。
pub fn local_root() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata).join("warlock_brawl").join("audio")
    } else {
        PathBuf::from("audio")
    }
}

/// 确保本地音频包根目录存在（不存在则创建）。返回该目录。
pub fn ensure_local_root() -> std::io::Result<PathBuf> {
    let root = local_root();
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

/// 在 `root` 写一份玩家说明 `README.txt`（列出全部音效 cue 名与 BGM 场景名）。
///
/// `cue_stems` 为**不含扩展名**的音效文件名（= `AudioCue::file` 去 `.wav`）。
pub fn write_readme(root: &Path, cue_stems: &[&str]) -> std::io::Result<()> {
    let mut s = String::new();
    s.push_str("Circle Brawl / 圆圈之战 本地音频包说明\n");
    s.push_str("========================================\n\n");
    s.push_str("每个子目录 = 一个音频包，结构：\n");
    s.push_str("  <包名>/\n");
    s.push_str("    circle_brawl_pack.ini   # 可选：name / author / version / type=(sound|music|both)\n");
    s.push_str("    sfx/<音效名>.<wav|ogg|flac|mp3>   # 音效（推荐 WAV）\n");
    s.push_str("    bgm/<场景>.<ogg|flac|wav|mp3>     # BGM（推荐 Ogg Vorbis）\n\n");
    s.push_str("扩展名优先级：\n  音效 wav > ogg > flac > mp3\n  BGM  ogg > flac > wav > mp3\n");
    s.push_str("注意：不支持 Opus（.opus 会被忽略）。\n\n");
    s.push_str("BGM 场景名：\n");
    for scene in MusicScene::ALL {
        s.push_str(&format!("  {}\n", scene.key()));
    }
    s.push_str("\n音效文件名（sfx/ 下，扩展名任选）：\n");
    for c in cue_stems {
        s.push_str(&format!("  {c}\n"));
    }
    s.push_str("\n放好后回游戏「设置 → 音效包 / BGM 包」选择，改包即时生效。\n");
    std::fs::write(root.join("README.txt"), s)
}

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

/// 给定 Steam 库根，返回本作创意工坊内容目录：`<root>/steamapps/workshop/content/908660`。
pub fn workshop_content_root(steam_root: &Path) -> PathBuf {
    steam_root
        .join("steamapps")
        .join("workshop")
        .join("content")
        .join(APP_ID)
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

/// 音频包根目录列表（优先级顺序：**本地 > 创意工坊**；后者不存在/探测失败则省略）。
pub fn default_roots() -> Vec<PathBuf> {
    let mut roots = vec![local_root()];
    if let Some(sr) = detect_steam_root() {
        roots.extend(workshop_roots(&sr));
    }
    roots
}

/// 按 id 找包。
pub fn find<'a>(packs: &'a [Pack], id: &str) -> Option<&'a Pack> {
    packs.iter().find(|p| p.id == id)
}

/// 在选项 id 列表中循环移动（找不到当前项时从头算）。空列表返回 `cur`。
pub fn cycle_id(ids: &[String], cur: &str, delta: i32) -> String {
    if ids.is_empty() {
        return cur.to_string();
    }
    let idx = ids.iter().position(|v| v == cur).unwrap_or(0) as i32;
    let n = ids.len() as i32;
    let ni = (idx + delta).rem_euclid(n);
    ids[ni as usize].clone()
}

/// 发布到创意工坊所需的元数据（由本地包推导）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishMeta {
    pub title: String,
    pub description: String,
    /// 工坊标签：`Sound` / `Music`（合集包两个都有）。
    pub tags: Vec<String>,
}

/// 由包信息推导发布元数据（纯函数，便于单测）。非 Steam 构建下暂未使用。
#[cfg_attr(not(feature = "steam"), allow(dead_code))]
pub fn publish_meta(pack: &Pack) -> PublishMeta {
    let mut tags = Vec::new();
    if pack.kind.has_sfx() {
        tags.push("Sound".to_string());
    }
    if pack.kind.has_bgm() {
        tags.push("Music".to_string());
    }
    let what = match (pack.kind.has_sfx(), pack.kind.has_bgm()) {
        (true, true) => "sound + music",
        (false, true) => "music",
        _ => "sound",
    };
    PublishMeta {
        title: pack.name.clone(),
        description: format!("{} — Circle Brawl audio pack ({what}).", pack.name),
        tags,
    }
}

/// 该包根是否位于 `root` 下（用于判断“本地包”，创意工坊包不可再发布）。
#[cfg_attr(not(feature = "steam"), allow(dead_code))]
pub fn is_under(root: &Path, pack_root: &Path) -> bool {
    pack_root.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("cb_audio_test_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn parse_manifest_reads_fields_and_kind() {
        let m = parse_manifest("# c\nname=Neon\nauthor=Alice\nversion=2\ntype=both\ndescription=x\ny=z\n");
        assert_eq!(m.name, "Neon");
        assert_eq!(m.author, "Alice");
        assert_eq!(m.version, "2");
        assert_eq!(m.kind, Some(PackKind::Both));
        assert_eq!(m.description, "x");
        // 宽松：未知键忽略、非法 type → None
        assert_eq!(parse_manifest("junk\ntype=weird\n").kind, None);
    }

    #[test]
    fn discover_infers_kind_and_skips_empty_dirs() {
        let root = tmp_root("discover");
        let a = root.join("PackA");
        write(&a.join("sfx/ui_confirm.wav"), b"x");
        let b = root.join("PackB");
        write(&b.join("bgm/battle.ogg"), b"x");
        write(&b.join(MANIFEST_NAME), b"name=MusicPack\ntype=music\n");
        let c = root.join("Junk");
        std::fs::create_dir_all(&c).unwrap(); // 无音频子目录 → 跳过

        let packs = discover(&[root.clone()]);
        assert_eq!(packs.len(), 2, "只应识别含音频的两包");
        let pa = find(&packs, "PackA").unwrap();
        assert_eq!(pa.kind, PackKind::Sound, "有 sfx 推断为音效包");
        assert_eq!(pa.name, "PackA", "无清单用目录名");
        let pb = find(&packs, "PackB").unwrap();
        assert_eq!(pb.kind, PackKind::Music);
        assert_eq!(pb.name, "MusicPack");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 能力**以目录为准**：清单 `type` 与目录不一致时不影响能力（仅日志）。
    #[test]
    fn discover_capability_follows_dirs_not_manifest_type() {
        let root = tmp_root("kind_cap");
        // 清单写 sound，但目录里同时有 bgm/ → 能力应为 Both（bgm 可见）。
        let a = root.join("A");
        write(&a.join("sfx/x.wav"), b"x");
        write(&a.join("bgm/menu.ogg"), b"x");
        write(&a.join(MANIFEST_NAME), b"name=A\ntype=sound\n");
        // 清单写 both，但目录里只有 sfx/ → 能力应为 Sound（不放 BGM 列表）。
        let b = root.join("B");
        write(&b.join("sfx/y.wav"), b"x");
        write(&b.join(MANIFEST_NAME), b"name=B\ntype=both\n");

        let packs = discover(&[root.clone()]);
        let pa = find(&packs, "A").unwrap();
        assert_eq!(pa.kind, PackKind::Both, "有 bgm/ 就应提供 BGM（清单 type 不覆盖）");
        assert!(pa.kind.has_bgm() && pa.kind.has_sfx());
        let pb = find(&packs, "B").unwrap();
        assert_eq!(pb.kind, PackKind::Sound, "无 bgm/ 就不应提供 BGM");
        assert!(!pb.kind.has_bgm());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_prefers_wav_for_sfx_and_ogg_for_bgm() {
        let root = tmp_root("resolve");
        let pack = root.join("P");
        write(&pack.join("sfx/combat_hit.ogg"), b"x");
        write(&pack.join("sfx/combat_hit.wav"), b"x");
        write(&pack.join("bgm/battle.wav"), b"x");
        write(&pack.join("bgm/battle.ogg"), b"x");
        assert!(resolve_sfx(&pack, "combat_hit").unwrap().ends_with("combat_hit.wav"));
        assert!(resolve_bgm(&pack, MusicScene::Battle).unwrap().ends_with("battle.ogg"));
        assert!(resolve_sfx(&pack, "missing").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn later_roots_do_not_override_earlier_same_id() {
        let local = tmp_root("local");
        let ws = tmp_root("ws");
        write(&local.join("Same/sfx/ui_move.wav"), b"x");
        write(&ws.join("Same/sfx/ui_move.wav"), b"x");
        let packs = discover(&[local.clone(), ws.clone()]);
        assert_eq!(packs.len(), 1, "同 id 去重");
        assert!(packs[0].root.starts_with(&local), "本地根优先");
        let _ = std::fs::remove_dir_all(&local);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn scene_for_maps_states() {
        use MusicScene::*;
        assert_eq!(scene_for(true, false, false), Menu, "主菜单 → menu");
        assert_eq!(scene_for(true, true, false), Menu, "主菜单优先");
        assert_eq!(scene_for(false, false, true), Result, "结束 → result");
        assert_eq!(scene_for(false, true, false), Lobby, "开局配置 → lobby");
        assert_eq!(scene_for(false, false, false), Battle, "对局中 → battle");
        assert_eq!(scene_for(false, true, true), Result, "结束优先于配置");
    }

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
        assert!(ws.ends_with(Path::new("steamapps").join("workshop").join("content").join("908660")));
    }

    #[test]
    fn default_roots_start_with_local() {
        let roots = default_roots();
        assert!(!roots.is_empty());
        assert_eq!(roots[0], local_root(), "本地根必须排在最前（优先级最高）");
    }

    #[test]
    fn parse_libraryfolders_vdf_paths() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}\n";
        let paths = parse_library_paths(vdf);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from("C:\\Program Files (x86)\\Steam"));
        assert_eq!(paths[1], PathBuf::from("D:\\SteamLibrary"));
    }

    #[test]
    fn publish_meta_tags_match_kind() {
        let mk = |kind| Pack {
            id: "P".into(),
            root: PathBuf::from("P"),
            name: "Neon".into(),
            author: String::new(),
            version: String::new(),
            kind,
        };
        let s = publish_meta(&mk(PackKind::Sound));
        assert_eq!(s.tags, vec!["Sound"]);
        assert_eq!(s.title, "Neon");
        assert!(s.description.contains("sound"));
        assert_eq!(publish_meta(&mk(PackKind::Music)).tags, vec!["Music"]);
        assert_eq!(publish_meta(&mk(PackKind::Both)).tags, vec!["Sound", "Music"]);
    }

    #[test]
    fn write_readme_lists_cues_and_scenes() {
        let root = tmp_root("readme");
        write_readme(&root, &["combat_hit", "ui_confirm"]).unwrap();
        let t = std::fs::read_to_string(root.join("README.txt")).unwrap();
        assert!(t.contains("combat_hit"), "应列出音效名");
        assert!(t.contains("ui_confirm"));
        for scene in MusicScene::ALL {
            assert!(t.contains(scene.key()), "应列出场景 {}", scene.key());
        }
        assert!(t.contains("Opus"), "应提示 Opus 不支持");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn preview_path_finds_cover() {
        let root = tmp_root("preview");
        assert!(preview_path(&root).is_none());
        write(&root.join("preview.png"), b"x");
        assert!(preview_path(&root).unwrap().ends_with("preview.png"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn coverage_counts_sfx_and_scenes() {
        let root = tmp_root("coverage");
        write(&root.join("sfx/combat_hit.wav"), b"x");
        write(&root.join("sfx/ui_confirm.ogg"), b"x");
        write(&root.join("bgm/battle.ogg"), b"x");
        assert_eq!(sfx_coverage(&root, &["combat_hit", "ui_confirm", "missing"]), 2);
        assert_eq!(bgm_scenes(&root), vec!["battle"]);
        assert_eq!(sfx_coverage(&root, &[]), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn write_example_pack_creates_discoverable_pack() {
        let root = tmp_root("example");
        let dir = write_example_pack(&root).unwrap();
        assert!(dir.ends_with("ExamplePack"));
        assert!(dir.join(MANIFEST_NAME).is_file());
        assert!(dir.join("sfx/ui_confirm.wav").is_file());
        assert!(dir.join("bgm/menu.wav").is_file());
        // 能被发现，且能力为 Both（有 sfx/ 与 bgm/）。
        let packs = discover(&[root.clone()]);
        let p = find(&packs, "ExamplePack").unwrap();
        assert_eq!(p.kind, PackKind::Both);
        // 静音 WAV 能解出（至少能被 resolve 解析到）。
        assert!(resolve_sfx(&dir, "ui_confirm").is_some());
        assert!(resolve_bgm(&dir, MusicScene::Menu).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cycle_id_wraps_and_handles_unknown() {
        let ids = vec!["builtin".to_string(), "A".to_string(), "B".to_string()];
        assert_eq!(cycle_id(&ids, "builtin", 1), "A");
        assert_eq!(cycle_id(&ids, "B", 1), "builtin", "环绕");
        assert_eq!(cycle_id(&ids, "builtin", -1), "B", "反向环绕");
        assert_eq!(cycle_id(&ids, "gone", 1), "A", "未知当前值 → 从下一个算");
        assert_eq!(cycle_id(&[], "x", 1), "x", "空列表保持原值");
    }
}
