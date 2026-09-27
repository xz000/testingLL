//! 外部图标包（HUD 技能槽 / 物品栏 / 商店）发现、清单解析与图标键解析。
//!
//! **纯 std + `game_core`（只读技能/物品定义），不依赖 ggez**，可单测。ggez 侧的图像缓存见
//! `icons.rs`（I1 接入）。目录/键约定见 `ICON_PACK_PLAN.md`：
//!
//! ```text
//! <包根>/circle_brawl_pack.ini      # 可选清单（name/author/version/description）
//! <包根>/icons/skill/<id>.png              # id = SkillId::as_u32()
//! <包根>/icons/skill/<id>_b.png            # B 形态（可选）
//! <包根>/icons/skill/<id>_release.png      # 链激活 S031·释放（可选）
//! <包根>/icons/skill/<id>_induce.png       # 链激活 S031·诱导（可选）
//! <包根>/icons/item/<id>.png               # id = ItemId::as_u32()
//! ```
//!
//! 与 `audio_pack` 的差别：音效包是**整包覆盖**，图标包是**逐键覆盖**
//! （有 `icons/skill/36.png` 就换火球，没有的那个键回退文字）——故两者模型**刻意不统一**。

use std::path::{Path, PathBuf};

use game_core::skill::{DefTable, SkillId};

/// 清单文件名（与音频包同名，便于玩家复用认知）。
pub const MANIFEST_NAME: &str = "circle_brawl_pack.ini";
/// 图标包选择值：不选包（保持现状，纯文字；内置默认）。
pub const PACK_NONE: &str = "none";
/// 图标扩展名搜索优先级（PNG 推荐，其余为兼容）。
pub const ICON_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp"];

/// 图标包（只认含 `icons/` 子目录的目录）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconPack {
    /// 稳定 id（目录名；创意工坊即 PublishedFileId）。
    pub id: String,
    pub root: PathBuf,
    pub name: String,
    pub author: String,
    pub version: String,
}

impl IconPack {
    /// UI 显示名：`名称 (作者)`，无作者则只显示名称。
    pub fn display(&self) -> String {
        if self.author.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.author)
        }
    }
}

/// 技能槽的**显示变体**（与 HUD 的中性名 / 形态角标 / 链释放·诱导同源，见 plan §4.3）。
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum SkillVariant {
    /// A 形态 / 普通技能。
    Base,
    /// B 形态。
    Alt,
    /// 链激活时 S031「锁链附加·释放」（`beam=false`）。
    Release,
    /// 链激活时 S031「锁链附加·诱导」（`beam=true`）。
    Induce,
}

/// 一个图标键（技能或物品）。
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum IconKey {
    Skill { id: u32, variant: SkillVariant },
    Item { id: u32 },
}

impl IconKey {
    pub fn skill(id: u32, variant: SkillVariant) -> Self {
        IconKey::Skill { id, variant }
    }

    pub fn item(id: u32) -> Self {
        IconKey::Item { id }
    }
}

/// 清单解析结果（宽松：缺项为空串，未知键忽略）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
}

/// 解析清单文本（`key=value` 行，`#`/`;` 注释，未知键忽略，大小写不敏感）。
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
        match k.trim().to_ascii_lowercase().as_str() {
            "name" => m.name = v.trim().to_string(),
            "author" => m.author = v.trim().to_string(),
            "version" => m.version = v.trim().to_string(),
            "description" => m.description = v.trim().to_string(),
            _ => {}
        }
    }
    m
}

/// 技能槽显示状态 → 图标键（**与 HUD 文字同源**，两者不会漂移）。
///
/// - `chain_here`：仅当槽位技能为 `S019` 且存在激活链时传 `Some(beam)`
///   （`beam=false` = 释放 / `true` = 诱导）；此时槽位实为 **S031**，图标键用 S031 的 id。
/// - 其余：`form_on && has_alt` → B 形态，否则 A 形态。
pub fn slot_state(skill: Option<SkillId>, form_on: bool, chain_here: Option<bool>) -> Option<IconKey> {
    let s = skill?;
    if let Some(beam) = chain_here {
        return Some(IconKey::skill(
            SkillId::S031.as_u32(),
            if beam { SkillVariant::Induce } else { SkillVariant::Release },
        ));
    }
    let variant = if form_on && DefTable::has_alt(s) {
        SkillVariant::Alt
    } else {
        SkillVariant::Base
    };
    Some(IconKey::skill(s.as_u32(), variant))
}

/// 键 → 候选相对文件名（**不含扩展名**），按回退顺序排列。
///
/// - B 形态：`<id>_b` → `<id>`
/// - 链·释放：`63_release` → `63` → `54`（S019 基础图标）
/// - 链·诱导：`63_induce` → `63` → `54_b`（S019 B 图标）
pub fn candidate_stems(key: IconKey) -> Vec<String> {
    match key {
        IconKey::Item { id } => vec![format!("item/{id}")],
        IconKey::Skill { id, variant } => {
            let base = format!("skill/{id}");
            match variant {
                SkillVariant::Base => vec![base],
                SkillVariant::Alt => vec![format!("{base}_b"), base],
                SkillVariant::Release => vec![
                    format!("{base}_release"),
                    base,
                    format!("skill/{}", SkillId::S019.as_u32()),
                ],
                SkillVariant::Induce => vec![
                    format!("{base}_induce"),
                    base,
                    format!("skill/{}_b", SkillId::S019.as_u32()),
                ],
            }
        }
    }
}

/// 在某包根中解析图标键 → 实际文件路径（按回退链 + 扩展名优先级逐级查找）。
pub fn icon_path(root: &Path, key: IconKey) -> Option<PathBuf> {
    for stem in candidate_stems(key) {
        for ext in ICON_EXTS {
            let p = root.join(format!("{stem}.{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// 扫描一个根目录下的所有子目录，识别为图标包（只认含 `icons/` 的目录）。
pub fn discover_root(root: &Path, out: &mut Vec<IconPack>) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("icons").is_dir())
        .collect();
    entries.sort(); // 稳定顺序（UI 列表可预期）
    for dir in entries {
        let manifest = std::fs::read_to_string(dir.join(MANIFEST_NAME))
            .map(|t| parse_manifest(&t))
            .unwrap_or_default();
        let id = dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "?".to_string());
        let name = if manifest.name.is_empty() { id.clone() } else { manifest.name };
        out.push(IconPack {
            id,
            root: dir,
            name,
            author: manifest.author,
            version: manifest.version,
        });
    }
}

/// 扫描多个根目录（后者可为空/不存在，静默跳过）。同名 id 以**先出现的根**为准（本地优先）。
pub fn discover(roots: &[PathBuf]) -> Vec<IconPack> {
    let mut out = Vec::new();
    for root in roots {
        discover_root(root, &mut out);
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|p| seen.insert(p.id.clone()));
    out
}

/// 本地图标包根目录：`%APPDATA%/warlock_brawl/icons`（取不到 APPDATA 则退回当前目录）。
pub fn local_root() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata).join("warlock_brawl").join("icons")
    } else {
        PathBuf::from("icons")
    }
}

/// 图标包根目录列表（优先级 **本地 > 创意工坊**；后者不存在/探测失败则省略）。
pub fn default_roots() -> Vec<PathBuf> {
    let mut roots = vec![local_root()];
    if let Some(sr) = crate::steam_paths::detect_steam_root() {
        roots.extend(crate::steam_paths::workshop_roots(&sr));
    }
    roots
}

/// 按 id 找包。
pub fn find<'a>(packs: &'a [IconPack], id: &str) -> Option<&'a IconPack> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("cb_icon_test_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::create_dir_all(&p);
        p
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn parse_manifest_reads_fields() {
        let m = parse_manifest("# c\nname=Neon Icons\nauthor=Alice\nversion=2\ntype=icons\ndescription=x\n");
        assert_eq!(m.name, "Neon Icons");
        assert_eq!(m.author, "Alice");
        assert_eq!(m.version, "2");
        assert_eq!(m.description, "x");
        assert_eq!(parse_manifest("junk\n").name, "");
    }

    #[test]
    fn slot_state_matches_hud_states() {
        // 普通技能
        assert_eq!(
            slot_state(Some(SkillId::S000), false, None),
            Some(IconKey::skill(SkillId::S000.as_u32(), SkillVariant::Base))
        );
        // B 形态（S009 有多形态）
        assert_eq!(
            slot_state(Some(SkillId::S009), true, None),
            Some(IconKey::skill(SkillId::S009.as_u32(), SkillVariant::Alt))
        );
        // form_on 但技能无第二形态 → 仍 Base（同 HUD：has_alt 判定）
        assert_eq!(
            slot_state(Some(SkillId::S000), true, None),
            Some(IconKey::skill(SkillId::S000.as_u32(), SkillVariant::Base))
        );
        // 链两态 → S031
        assert_eq!(
            slot_state(Some(SkillId::S019), false, Some(false)),
            Some(IconKey::skill(SkillId::S031.as_u32(), SkillVariant::Release))
        );
        assert_eq!(
            slot_state(Some(SkillId::S019), true, Some(true)),
            Some(IconKey::skill(SkillId::S031.as_u32(), SkillVariant::Induce))
        );
        // 空槽
        assert_eq!(slot_state(None, false, None), None);
    }

    #[test]
    fn candidate_stems_have_fallbacks() {
        let alt = candidate_stems(IconKey::skill(SkillId::S009.as_u32(), SkillVariant::Alt));
        assert!(alt[0].ends_with("_b"));
        assert_eq!(alt.len(), 2, "B 形态应回退到基础图标");
        let rel = candidate_stems(IconKey::skill(SkillId::S031.as_u32(), SkillVariant::Release));
        assert_eq!(rel[0], format!("skill/{}_release", SkillId::S031.as_u32()));
        assert!(rel.iter().any(|s| s == &format!("skill/{}", SkillId::S019.as_u32())), "释放应回退到 S019 基础图标");
        let ind = candidate_stems(IconKey::skill(SkillId::S031.as_u32(), SkillVariant::Induce));
        assert!(ind.iter().any(|s| s.ends_with("_b")), "诱导应回退到 S019 B 图标");
        assert_eq!(candidate_stems(IconKey::item(3)), vec!["item/3".to_string()]);
    }

    #[test]
    fn icon_path_follows_fallback_chain() {
        let root = tmp_root("iconpath");
        let key = IconKey::skill(SkillId::S031.as_u32(), SkillVariant::Induce);
        // 只有 S019 B 图标 → 诱导应回退到它
        write(&root.join(format!("skill/{}_b.png", SkillId::S019.as_u32())), b"x");
        let got = icon_path(&root, key).unwrap();
        assert!(got.ends_with(format!("{}_b.png", SkillId::S019.as_u32())), "{got:?}");
        // 补上专属图标 → 优先专属
        write(&root.join(format!("skill/{}_induce.png", SkillId::S031.as_u32())), b"x");
        let got = icon_path(&root, key).unwrap();
        assert!(got.ends_with(format!("{}_induce.png", SkillId::S031.as_u32())), "{got:?}");
        // 完全没有 → None
        assert!(icon_path(&root, IconKey::item(999)).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn discover_only_icons_dirs_and_local_priority() {
        let local = tmp_root("disc_local");
        let ws = tmp_root("disc_ws");
        // 有 icons/ → 是图标包
        write(&local.join("Neon/icons/skill/36.png"), b"x");
        write(&local.join("Neon/circle_brawl_pack.ini"), "name=霓虹\nauthor=Bob\n".as_bytes());
        // 无 icons/ → 跳过
        write(&local.join("Junk/sfx/x.wav"), b"x");
        // 同 id 的工坊包 → 本地优先
        write(&ws.join("Neon/icons/skill/55.png"), b"x");

        let packs = discover(&[local.clone(), ws.clone()]);
        assert_eq!(packs.len(), 1, "只应识别含 icons/ 的包，且同 id 去重");
        assert_eq!(packs[0].id, "Neon");
        assert_eq!(packs[0].name, "霓虹");
        assert_eq!(packs[0].display(), "霓虹 (Bob)");
        assert!(packs[0].root.starts_with(&local), "本地根优先");
        let _ = std::fs::remove_dir_all(&local);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn cycle_id_wraps_and_handles_unknown() {
        let ids = vec!["none".to_string(), "A".to_string(), "B".to_string()];
        assert_eq!(cycle_id(&ids, "none", 1), "A");
        assert_eq!(cycle_id(&ids, "B", 1), "none", "环绕");
        assert_eq!(cycle_id(&ids, "gone", 1), "A");
        assert_eq!(cycle_id(&[], "x", 1), "x");
    }
}
