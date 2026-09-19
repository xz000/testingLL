//! 音效播放（`ggez::audio`，底层 rodio）。
//!
//! 设计纪律（见 `AUDIO_PLAN.md`）：
//! - **纯客户端**：只读事件源 → 播放，不进 `World` / 快照，不改 `PROTOCOL_VERSION`。
//! - **缺素材静默降级**：加载失败 / 无声卡 → 该 cue 不发声，绝不 panic / 不阻塞。
//! - **同名替换**：素材文件名即 cue（`AUDIO_PLAN.md` 的「占位名」），替换素材不改代码。
//!
//! 播放采用「每个 cue 一个 `Source`，重复触发即重头播放」的简单策略
//! （重叠播放会让 rodio 的 `Sink` 生命周期难以管理；此游戏短音效足够）。

use crate::audio_pack;
use crate::local_settings::LocalSettings;
use ggez::audio::{SoundData, SoundSource, Source};
use ggez::Context;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 全部音效 cue。文件名与 `AUDIO_PLAN.md` 的「占位名」一致。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AudioCue {
    // A. 战斗核心（自制）
    CombatCast,
    CombatRelease,
    CombatHit,
    CombatExplode,
    CombatBounce,
    CombatShield,
    CombatReflect,
    CombatHeal,
    CombatKill,
    CombatDeath,
    CombatRespawn,
    CombatPillarBreak,
    CombatLava,
    // B. 回合 / 对局流程（自制）
    FlowRoundStart,
    FlowRoundEnd,
    FlowConfigPhase,
    FlowCountdown,
    FlowOobWarn,
    FlowShrinkWarn,
    // C. 大厅 / 房间 UI（自制）
    UiMove,
    UiConfirm,
    UiCancel,
    UiError,
    UiReady,
    UiAllReady,
    UiJoin,
    UiLeave,
    UiHostLeft,
    UiInvite,
    // D. 商店 / 成长（自制）
    ShopBuy,
    ShopUpgrade,
    ShopSell,
    // E. 098c 播报语音（重录/替换）
    AnnFirstBlood,
    AnnDoubleKill,
    AnnMultiKill,
    AnnMegaKill,
    AnnUltraKill,
    AnnMonsterKill,
    AnnLudicrousKill,
    AnnSpree3,
    AnnSpree4,
    AnnSpree5,
    AnnSpree6,
    AnnSpree7,
    AnnSpree8,
    AnnSpree9,
    AnnSpree10,
    AnnSpreeHoly,
    AnnHattrick,
    AnnVampire,
    AnnDenied,
    AnnBurnout,
    AnnSilencer,
    AnnPancake,
    AnnLastSecondSave,
    AnnVictory,
    AnnGameStart,
    /// 平局加赛专属 sting（098c 复用 `Vo` GameFound；本作独立，见 `AUDIO_PLAN.md` §1.6）。
    AnnDraw,
    AnnFinish,
    AnnResearch,
}

impl AudioCue {
    /// 全部 cue（用于加载 / 单测遍历）。
    pub const ALL: &'static [AudioCue] = &[
        AudioCue::CombatCast,
        AudioCue::CombatRelease,
        AudioCue::CombatHit,
        AudioCue::CombatExplode,
        AudioCue::CombatBounce,
        AudioCue::CombatShield,
        AudioCue::CombatReflect,
        AudioCue::CombatHeal,
        AudioCue::CombatKill,
        AudioCue::CombatDeath,
        AudioCue::CombatRespawn,
        AudioCue::CombatPillarBreak,
        AudioCue::CombatLava,
        AudioCue::FlowRoundStart,
        AudioCue::FlowRoundEnd,
        AudioCue::FlowConfigPhase,
        AudioCue::FlowCountdown,
        AudioCue::FlowOobWarn,
        AudioCue::FlowShrinkWarn,
        AudioCue::UiMove,
        AudioCue::UiConfirm,
        AudioCue::UiCancel,
        AudioCue::UiError,
        AudioCue::UiReady,
        AudioCue::UiAllReady,
        AudioCue::UiJoin,
        AudioCue::UiLeave,
        AudioCue::UiHostLeft,
        AudioCue::UiInvite,
        AudioCue::ShopBuy,
        AudioCue::ShopUpgrade,
        AudioCue::ShopSell,
        AudioCue::AnnFirstBlood,
        AudioCue::AnnDoubleKill,
        AudioCue::AnnMultiKill,
        AudioCue::AnnMegaKill,
        AudioCue::AnnUltraKill,
        AudioCue::AnnMonsterKill,
        AudioCue::AnnLudicrousKill,
        AudioCue::AnnSpree3,
        AudioCue::AnnSpree4,
        AudioCue::AnnSpree5,
        AudioCue::AnnSpree6,
        AudioCue::AnnSpree7,
        AudioCue::AnnSpree8,
        AudioCue::AnnSpree9,
        AudioCue::AnnSpree10,
        AudioCue::AnnSpreeHoly,
        AudioCue::AnnHattrick,
        AudioCue::AnnVampire,
        AudioCue::AnnDenied,
        AudioCue::AnnBurnout,
        AudioCue::AnnSilencer,
        AudioCue::AnnPancake,
        AudioCue::AnnLastSecondSave,
        AudioCue::AnnVictory,
        AudioCue::AnnGameStart,
        AudioCue::AnnDraw,
        AudioCue::AnnFinish,
        AudioCue::AnnResearch,
    ];

    /// 资源文件名（相对 `assets/audio/`）。
    pub fn file(self) -> &'static str {
        match self {
            AudioCue::CombatCast => "combat_cast.wav",
            AudioCue::CombatRelease => "combat_release.wav",
            AudioCue::CombatHit => "combat_hit.wav",
            AudioCue::CombatExplode => "combat_explode.wav",
            AudioCue::CombatBounce => "combat_bounce.wav",
            AudioCue::CombatShield => "combat_shield.wav",
            AudioCue::CombatReflect => "combat_reflect.wav",
            AudioCue::CombatHeal => "combat_heal.wav",
            AudioCue::CombatKill => "combat_kill.wav",
            AudioCue::CombatDeath => "combat_death.wav",
            AudioCue::CombatRespawn => "combat_respawn.wav",
            AudioCue::CombatPillarBreak => "combat_pillar_break.wav",
            AudioCue::CombatLava => "combat_lava.wav",
            AudioCue::FlowRoundStart => "flow_round_start.wav",
            AudioCue::FlowRoundEnd => "flow_round_end.wav",
            AudioCue::FlowConfigPhase => "flow_config_phase.wav",
            AudioCue::FlowCountdown => "flow_countdown.wav",
            AudioCue::FlowOobWarn => "flow_oob_warn.wav",
            AudioCue::FlowShrinkWarn => "flow_shrink_warn.wav",
            AudioCue::UiMove => "ui_move.wav",
            AudioCue::UiConfirm => "ui_confirm.wav",
            AudioCue::UiCancel => "ui_cancel.wav",
            AudioCue::UiError => "ui_error.wav",
            AudioCue::UiReady => "ui_ready.wav",
            AudioCue::UiAllReady => "ui_all_ready.wav",
            AudioCue::UiJoin => "ui_join.wav",
            AudioCue::UiLeave => "ui_leave.wav",
            AudioCue::UiHostLeft => "ui_host_left.wav",
            AudioCue::UiInvite => "ui_invite.wav",
            AudioCue::ShopBuy => "shop_buy.wav",
            AudioCue::ShopUpgrade => "shop_upgrade.wav",
            AudioCue::ShopSell => "shop_sell.wav",
            AudioCue::AnnFirstBlood => "ann_first_blood.wav",
            AudioCue::AnnDoubleKill => "ann_double_kill.wav",
            AudioCue::AnnMultiKill => "ann_multi_kill.wav",
            AudioCue::AnnMegaKill => "ann_mega_kill.wav",
            AudioCue::AnnUltraKill => "ann_ultra_kill.wav",
            AudioCue::AnnMonsterKill => "ann_monster_kill.wav",
            AudioCue::AnnLudicrousKill => "ann_ludicrous_kill.wav",
            AudioCue::AnnSpree3 => "ann_spree3.wav",
            AudioCue::AnnSpree4 => "ann_spree4.wav",
            AudioCue::AnnSpree5 => "ann_spree5.wav",
            AudioCue::AnnSpree6 => "ann_spree6.wav",
            AudioCue::AnnSpree7 => "ann_spree7.wav",
            AudioCue::AnnSpree8 => "ann_spree8.wav",
            AudioCue::AnnSpree9 => "ann_spree9.wav",
            AudioCue::AnnSpree10 => "ann_spree10.wav",
            AudioCue::AnnSpreeHoly => "ann_spree_holy.wav",
            AudioCue::AnnHattrick => "ann_hattrick.wav",
            AudioCue::AnnVampire => "ann_vampire.wav",
            AudioCue::AnnDenied => "ann_denied.wav",
            AudioCue::AnnBurnout => "ann_burnout.wav",
            AudioCue::AnnSilencer => "ann_silencer.wav",
            AudioCue::AnnPancake => "ann_pancake.wav",
            AudioCue::AnnLastSecondSave => "ann_last_second_save.wav",
            AudioCue::AnnVictory => "ann_victory.wav",
            AudioCue::AnnGameStart => "ann_game_start.wav",
            AudioCue::AnnDraw => "ann_draw.wav",
            AudioCue::AnnFinish => "ann_finish.wav",
            AudioCue::AnnResearch => "ann_research.wav",
        }
    }
}

/// 素材目录候选（可执行文件相对路径随启动目录不同，逐个探测）。
const AUDIO_DIRS: &[&str] = &["assets/audio", "client/assets/audio", "../client/assets/audio"];
/// 内置 BGM 目录候选（当前无内置 BGM，找到就用）。
const BGM_DIRS: &[&str] = &["assets/bgm", "client/assets/bgm", "../client/assets/bgm"];

fn find_asset(file: &str) -> Option<PathBuf> {
    AUDIO_DIRS.iter().map(|d| Path::new(d).join(file)).find(|p| p.exists())
}

/// 内置 BGM：`<dir>/<scene>.<ext>`（按 [`audio_pack::BGM_EXTS`] 顺序）。
fn find_builtin_bgm(scene: audio_pack::MusicScene) -> Option<PathBuf> {
    for d in BGM_DIRS {
        for ext in audio_pack::BGM_EXTS {
            let p = Path::new(d).join(format!("{}.{ext}", scene.key()));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// 读文件 → 解码 → `Source`；任一步失败返回 `None`（只记一行日志，不 panic）。
fn load_source(ctx: &Context, path: &Path) -> Option<Source> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[audio] 读取 {path:?} 失败（忽略）：{e}");
            return None;
        }
    };
    let data = match SoundData::from_bytes(&bytes) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[audio] 解码 {path:?} 失败（忽略，可能是 Opus 等不支持的格式）：{e}");
            return None;
        }
    };
    match Source::from_data(ctx, data) {
        Ok(src) => Some(src),
        Err(e) => {
            eprintln!("[audio] 创建音源 {path:?} 失败（忽略）：{e}");
            None
        }
    }
}

/// BGM 播放状态（支持交叉淡入淡出）。
struct Music {
    source: Source,
    scene: audio_pack::MusicScene,
    cur: f32,
    target: f32,
}

/// BGM 淡入淡出速率（音量/秒）。
const FADE_PER_SEC: f32 = 1.5;

/// 音量：`set_volume` 取值会被 rodio 夹到 `0.0..=1.0`。
pub struct AudioBank {
    /// 音效：cue → 音频源（来源可为内置占位或选定音频包）。
    sources: HashMap<AudioCue, Source>,
    /// 当前 BGM（None = 无）。
    music: Option<Music>,
    /// 正在淡出的旧 BGM（交叉淡出用）。
    music_out: Vec<Music>,
    settings: LocalSettings,
    /// 当前生效的音效包根（None = 内置）。
    sfx_pack_root: Option<PathBuf>,
    /// 当前生效的 BGM 包根（None = 内置）。
    music_pack_root: Option<PathBuf>,
}

impl AudioBank {
    /// 加载全部素材；缺失的 cue 静默跳过（只记一行日志）。
    ///
    /// 若 `settings.sfx_pack` 指向一个有效音效包，则**整包覆盖**内置素材；否则用内置。
    pub fn new(ctx: &Context, settings: &LocalSettings, packs: &[audio_pack::Pack]) -> Self {
        let mut bank = Self {
            sources: HashMap::new(),
            music: None,
            music_out: Vec::new(),
            settings: settings.clone(),
            sfx_pack_root: None,
            music_pack_root: None,
        };
        bank.resolve_packs(packs);
        bank.load_sfx(ctx);
        bank.apply_volumes();
        bank
    }

    /// 根据当前选择解析出音效/BGM 包的根目录（无效/类型不匹配 → None）。
    fn resolve_packs(&mut self, packs: &[audio_pack::Pack]) {
        self.sfx_pack_root = audio_pack::find(packs, &self.settings.sfx_pack)
            .filter(|p| p.kind.has_sfx())
            .map(|p| p.root.clone());
        self.music_pack_root = audio_pack::find(packs, &self.settings.music_pack)
            .filter(|p| p.kind.has_bgm())
            .map(|p| p.root.clone());
    }

    /// 重新应用包选择并重载（设置里改包后调用）。旧 BGM 会淡出。
    pub fn reload(&mut self, ctx: &Context, settings: &LocalSettings, packs: &[audio_pack::Pack]) {
        self.settings = settings.clone();
        self.resolve_packs(packs);
        self.load_sfx(ctx);
        self.apply_volumes();
        if let Some(m) = self.music.take() {
            self.music_out.push(Music { target: 0.0, ..m });
        }
    }

    /// 加载音效（先试音频包，再回退内置）。
    fn load_sfx(&mut self, ctx: &Context) {
        self.sources.clear();
        for &cue in AudioCue::ALL {
            let stem = cue.file().trim_end_matches(".wav");
            let path = self
                .sfx_pack_root
                .as_ref()
                .and_then(|r| audio_pack::resolve_sfx(r, stem))
                .or_else(|| find_asset(cue.file()));
            if let Some(p) = path {
                if let Some(src) = load_source(ctx, &p) {
                    self.sources.insert(cue, src);
                }
            }
        }
    }

    /// 已成功加载的 cue 数量（无素材时为 0，用于日志/诊断）。
    pub fn loaded_count(&self) -> usize {
        self.sources.len()
    }

    /// 应用本地设置（音量 / 静音）。
    pub fn apply(&mut self, settings: &LocalSettings) {
        self.settings = settings.clone();
        self.apply_volumes();
    }

    fn apply_volumes(&mut self) {
        let v = self.current_volume();
        for src in self.sources.values_mut() {
            src.set_volume(v);
        }
    }

    fn current_volume(&self) -> f32 {
        self.settings.effective_sfx()
    }

    /// 播放一个 cue（静音 / 未加载 → 无操作）。
    pub fn play(&mut self, cue: AudioCue) {
        let vol = self.current_volume();
        if vol <= 0.0 {
            return;
        }
        if let Some(src) = self.sources.get_mut(&cue) {
            src.set_volume(vol);
            src.play();
        }
    }

    /// 每帧推进 BGM：场景切换 + 交叉淡入淡出。`scene` 由 [`audio_pack::scene_for`] 推导。
    pub fn update(&mut self, ctx: &Context, dt: f32, scene: audio_pack::MusicScene) {
        let target = self.settings.effective_music();
        let same = self.music.as_ref().map(|m| m.scene) == Some(scene);
        if !same {
            if let Some(m) = self.music.take() {
                self.music_out.push(Music { target: 0.0, ..m });
            }
            if target > 0.0 {
                if let Some(src) = self.start_music(ctx, scene) {
                    self.music = Some(Music { source: src, scene, cur: 0.0, target });
                }
            }
        }
        if let Some(m) = self.music.as_mut() {
            m.target = target;
        }
        ramp_music(self.music.as_mut(), dt);
        for m in self.music_out.iter_mut() {
            m.target = 0.0;
            ramp_music(Some(m), dt);
        }
        self.music_out.retain(|m| m.cur > 0.0 || m.target > 0.0);
    }

    /// 为某场景创建循环 BGM 音源（先试 BGM 包，再回退内置；都没有 → None）。
    fn start_music(&self, ctx: &Context, scene: audio_pack::MusicScene) -> Option<Source> {
        let path = self
            .music_pack_root
            .as_ref()
            .and_then(|r| audio_pack::resolve_bgm(r, scene))
            .or_else(|| find_builtin_bgm(scene))?;
        let mut src = load_source(ctx, &path)?;
        src.set_repeat(true);
        src.set_volume(0.0);
        src.play();
        Some(src)
    }
}

/// 把一个 BGM 的音量向 `target` 逼近（线性淡入淡出）。
fn ramp_music(m: Option<&mut Music>, dt: f32) {
    if let Some(m) = m {
        let step = FADE_PER_SEC * dt.max(0.0);
        if m.cur < m.target {
            m.cur = (m.cur + step).min(m.target);
        } else if m.cur > m.target {
            m.cur = (m.cur - step).max(m.target);
        }
        m.source.set_volume(m.cur);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_cues_have_unique_wav_names() {
        let mut files: Vec<&str> = AudioCue::ALL.iter().map(|c| c.file()).collect();
        files.sort_unstable();
        let n = files.len();
        files.dedup();
        assert_eq!(files.len(), n, "存在重名的音效文件");
        assert!(files.iter().all(|f| f.ends_with(".wav")), "音效文件名应为 .wav");
    }

    #[test]
    fn cue_list_is_nonempty_and_covers_categories() {
        assert!(AudioCue::ALL.contains(&AudioCue::CombatHit));
        assert!(AudioCue::ALL.contains(&AudioCue::UiConfirm));
        assert!(AudioCue::ALL.contains(&AudioCue::ShopBuy));
        assert!(AudioCue::ALL.contains(&AudioCue::AnnFirstBlood));
    }
}
