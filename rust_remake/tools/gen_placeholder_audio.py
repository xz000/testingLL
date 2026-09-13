#!/usr/bin/env python3
"""生成占位音效（.wav）—— 仅用于 P2 跑通链路，之后用真实素材同名替换。

用法（在仓库根目录）：
    python tools/gen_placeholder_audio.py

输出目录：client/assets/audio/<cue>.wav
文件名与 client/src/audio.rs 的 AudioCue::file() 及 AUDIO_PLAN.md 的「占位名」一致。
**保持本列表与 Rust 侧同步**（新增 cue 时两处都要加）。

设计：纯 stdlib（wave/struct/math），每个 cue 由名字哈希决定基频，
按类别（combat/flow/ui/shop/ann）给不同时长/包络，听感可区分但都不刺耳。
"""

import math
import os
import struct
import wave

RATE = 22050  # 单声道 16-bit；占位素材无需高采样率

CUES = [
    # A. 战斗核心
    "combat_cast", "combat_release", "combat_hit", "combat_explode",
    "combat_bounce", "combat_shield", "combat_reflect", "combat_heal",
    "combat_kill", "combat_death", "combat_respawn", "combat_pillar_break",
    "combat_lava",
    # B. 流程
    "flow_round_start", "flow_round_end", "flow_config_phase",
    "flow_countdown", "flow_oob_warn", "flow_shrink_warn",
    # C. UI
    "ui_move", "ui_confirm", "ui_cancel", "ui_error", "ui_ready",
    "ui_all_ready", "ui_join", "ui_leave", "ui_host_left", "ui_invite",
    # D. 商店
    "shop_buy", "shop_upgrade", "shop_sell",
    # E. 播报（占位：短双音；真实素材用录音替换）
    "ann_first_blood", "ann_double_kill", "ann_multi_kill", "ann_mega_kill",
    "ann_ultra_kill", "ann_monster_kill", "ann_ludicrous_kill",
    "ann_spree3", "ann_spree4", "ann_spree5", "ann_spree6", "ann_spree7",
    "ann_spree8", "ann_spree9", "ann_spree10", "ann_spree_holy",
    "ann_hattrick", "ann_vampire", "ann_denied", "ann_burnout",
    "ann_silencer", "ann_pancake", "ann_last_second_save",
    "ann_victory", "ann_game_start", "ann_finish", "ann_research",
]


def name_hash(s: str) -> float:
    h = 2166136261
    for ch in s:
        h = (h ^ ord(ch)) * 16777619 & 0xFFFFFFFF
    return h / 0xFFFFFFFF  # 0..1


def envelope(t: float, dur: float) -> float:
    attack = 0.004
    if t < attack:
        return t / attack
    # 指数衰减到 0
    x = (t - attack) / max(dur - attack, 1e-6)
    return math.exp(-4.0 * x) * (1.0 - 0.15 * x)


def tone(name: str) -> bytes:
    r = name_hash(name)
    f0 = 180.0 + r * 620.0  # 180..800 Hz

    if name.startswith("ann_"):
        dur, kind = 0.30, "two"
    elif name.startswith("flow_"):
        dur, kind = 0.26, "two"
    elif name.startswith("ui_"):
        dur, kind = 0.07, "click"
    elif name.startswith("shop_"):
        dur, kind = 0.13, "two"
    elif name.startswith("combat_"):
        dur, kind = 0.15, "blip"
    else:
        dur, kind = 0.15, "blip"

    n = int(RATE * dur)
    out = bytearray()
    for i in range(n):
        t = i / RATE
        env = envelope(t, dur)
        if kind == "click":
            f = f0 * 1.5
            s = math.sin(2 * math.pi * f * t)
        elif kind == "two":
            # 后半段升一个纯五度，像“di-da”提示
            f = f0 if t < dur * 0.5 else f0 * 1.5
            s = math.sin(2 * math.pi * f * t) * 0.8
            s += 0.2 * math.sin(2 * math.pi * f * 2 * t)
        else:  # blip
            f = f0
            s = math.sin(2 * math.pi * f * t)
            s += 0.3 * math.sin(2 * math.pi * f * 2 * t)  # 加二次谐波
            s += 0.15 * math.sin(2 * math.pi * f * 3 * t)
        v = max(-1.0, min(1.0, s * env * 0.6))
        out += struct.pack("<h", int(v * 32767))
    return bytes(out)


def main() -> None:
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out_dir = os.path.join(root, "client", "assets", "audio")
    os.makedirs(out_dir, exist_ok=True)
    for name in CUES:
        path = os.path.join(out_dir, name + ".wav")
        with wave.open(path, "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(RATE)
            w.writeframes(tone(name))
    print(f"wrote {len(CUES)} placeholder wavs to {out_dir}")


if __name__ == "__main__":
    main()
