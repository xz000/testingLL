//! 音效播放（`ggez::audio`，底层 rodio）。
//!
//! 设计纪律（见 `AUDIO_PLAN.md`）：
//! - **纯客户端**：只读事件源 → 播放，不进 `World` / 快照，不改 `PROTOCOL_VERSION`。
//! - **缺素材静默降级**：加载失败 / 无声卡 → 该 cue 不发声，绝不 panic / 不阻塞。
//! - **同名替换**：素材文件名即 cue（`AUDIO_PLAN.md` 的「占位名」），替换素材不改代码。
//!
//! 播放采用「每个 cue 一个 `Source`，重复触发即重头播放」的简单策略
//! （重叠播放会让 rodio 的 `Sink` 生命周期难以管理；此游戏短音效足够）。

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
            AudioCue::AnnFinish => "ann_finish.wav",
            AudioCue::AnnResearch => "ann_research.wav",
        }
    }
}

/// 素材目录候选（可执行文件相对路径随启动目录不同，逐个探测）。
const AUDIO_DIRS: &[&str] = &["assets/audio", "client/assets/audio", "../client/assets/audio"];

fn find_asset(file: &str) -> Option<PathBuf> {
    AUDIO_DIRS.iter().map(|d| Path::new(d).join(file)).find(|p| p.exists())
}

/// 音量：`set_volume` 取值会被 rodio 夹到 `0.0..=1.0`。
pub struct AudioBank {
    sources: HashMap<AudioCue, Source>,
    settings: LocalSettings,
}

impl AudioBank {
    /// 加载全部素材；缺失的 cue 静默跳过（只记一行日志）。
    pub fn new(ctx: &Context, settings: &LocalSettings) -> Self {
        let mut sources = HashMap::new();
        for &cue in AudioCue::ALL {
            let Some(path) = find_asset(cue.file()) else {
                continue; // 无素材（占位未生成 / 未替换）：静默
            };
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("[audio] 读取 {path:?} 失败（忽略）：{e}");
                    continue;
                }
            };
            let data = match SoundData::from_bytes(&bytes) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("[audio] 解码 {path:?} 失败（忽略）：{e}");
                    continue;
                }
            };
            match Source::from_data(ctx, data) {
                Ok(src) => {
                    sources.insert(cue, src);
                }
                Err(e) => eprintln!("[audio] 创建音源 {path:?} 失败（忽略）：{e}"),
            }
        }
        let mut bank = Self {
            sources,
            settings: *settings,
        };
        bank.apply_volumes();
        bank
    }

    /// 已成功加载的 cue 数量（无素材时为 0，用于日志/诊断）。
    pub fn loaded_count(&self) -> usize {
        self.sources.len()
    }

    /// 应用本地设置（音量 / 静音）。
    pub fn apply(&mut self, settings: &LocalSettings) {
        self.settings = *settings;
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
