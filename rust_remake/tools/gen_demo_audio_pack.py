#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成一个「演示音频包」用于测试本地包功能（无需 Steam）。

- 音效：覆盖 **全部** cue（文件名从 client/src/audio.rs 的 `file()` 表自动读取，保证一致）。
- BGM：4 个场景 menu/lobby/battle/result（循环友好的合成小段）。
- 格式：全部 WAV（16-bit PCM）。*真实包*BGM 建议用 Ogg Vorbis（更小、无缝）；本脚本不引入编码器，用 WAV 保证可行。
- 输出：默认 %APPDATA%/warlock_brawl/audio/DemoPack，可用参数覆盖。

用法：
    python tools/gen_demo_audio_pack.py [输出目录]
"""

import math
import os
import re
import struct
import sys
import wave

RATE = 44100
HERE = os.path.dirname(os.path.abspath(__file__))
AUDIO_RS = os.path.join(HERE, "..", "client", "src", "audio.rs")


def read_cue_names():
    """从 audio.rs 抓所有 `=> "<name>.wav"`。"""
    with open(AUDIO_RS, encoding="utf-8") as f:
        src = f.read()
    names = re.findall(r'=>\s*"([a-z0-9_]+\.wav)"', src)
    # 去重、保序
    seen, out = set(), []
    for n in names:
        if n not in seen:
            seen.add(n)
            out.append(n)
    return out


def clamp(x):
    return 1.0 if x > 1.0 else (-1.0 if x < -1.0 else x)


def hash_f(name, lo, hi):
    h = 2166136261
    for ch in name:
        h = (h ^ ord(ch)) * 16777619 & 0xFFFFFFFF
    return lo + (h % 1000) / 1000.0 * (hi - lo)


def sine(f, t):
    return math.sin(2.0 * math.pi * f * t)


def env(n, i, attack, decay):
    """线attack + 指数decay 包络（0..1）。"""
    a = max(1, int(attack * RATE))
    if i < a:
        return i / a
    k = (i - a) / max(1.0, decay * RATE)
    return math.exp(-3.0 * k)


def sfx_samples(name):
    """按类别合成一个短音效，返回 mono float 列表。"""
    if name.startswith("ann_"):
        # 播报：三音琶音
        base = hash_f(name, 300.0, 520.0)
        dur = 0.55
        notes = [(1.0, 0.0), (1.26, 0.16), (1.5, 0.32)]
        n = int(RATE * dur)
        out = [0.0] * n
        for mult, start in notes:
            s0 = int(start * RATE)
            for i in range(s0, n):
                t = (i - s0) / RATE
                out[i] += 0.35 * sine(base * mult, t) * math.exp(-6.0 * t)
        return out

    if name.startswith("ui_"):
        f = hash_f(name, 700.0, 1400.0)
        n = int(RATE * 0.07)
        return [0.4 * sine(f, i / RATE) * env(n, i, 0.002, 0.05) for i in range(n)]

    if name.startswith("combat_"):
        # 打击：低频砰 + 噪声
        f = hash_f(name, 90.0, 180.0)
        n = int(RATE * 0.18)
        out = []
        seed = 12345
        for i in range(n):
            seed = (1103515245 * seed + 12345) & 0x7FFFFFFF
            noise = (seed / 0x3FFFFFFF) - 1.0
            t = i / RATE
            out.append(0.45 * sine(f, t) * env(n, i, 0.001, 0.08) + 0.25 * noise * env(n, i, 0.001, 0.04))
        return out

    if name.startswith("shop_"):
        f = hash_f(name, 900.0, 1300.0)
        n = int(RATE * 0.16)
        out = [0.0] * n
        for j, mult in enumerate((1.0, 1.5)):
            s0 = int(j * 0.06 * RATE)
            for i in range(s0, n):
                t = (i - s0) / RATE
                out[i] += 0.3 * sine(f * mult, t) * math.exp(-9.0 * t)
        return out

    if name.startswith("flow_"):
        f = hash_f(name, 400.0, 800.0)
        n = int(RATE * 0.4)
        return [0.3 * sine(f, i / RATE) * math.exp(-4.0 * (i / RATE)) for i in range(n)]

    # 兜底：短 blip
    f = hash_f(name, 500.0, 1000.0)
    n = int(RATE * 0.09)
    return [0.35 * sine(f, i / RATE) * env(n, i, 0.002, 0.06) for i in range(n)]


def bgm_samples(scene):
    """各场景一段循环友好的立体声（返回 (left, right) 两个 mono 列表）。"""
    dur = 8.0
    n = int(RATE * dur)
    left = [0.0] * n
    right = [0.0] * n

    def add(i, l, r):
        left[i] += l
        right[i] += r

    if scene == "menu":
        # 缓慢环境 pad
        for i in range(n):
            t = i / RATE
            lfo = 0.6 + 0.4 * math.sin(2 * math.pi * 0.12 * t)
            a = 0.16 * sine(110.0, t) * lfo
            b = 0.12 * sine(164.81, t) * lfo
            c = 0.10 * sine(220.0, t) * (0.5 + 0.5 * math.sin(2 * math.pi * 0.08 * t))
            add(i, a + c, b + c)
    elif scene == "lobby":
        # 轻柔琶音
        seq = [261.63, 329.63, 392.0, 523.25]
        for i in range(n):
            t = i / RATE
            step = int(t / 0.5) % len(seq)
            local = (t % 0.5)
            e = math.exp(-4.0 * local)
            tone = 0.22 * sine(seq[step], t) * e
            pad = 0.06 * sine(130.81, t)
            add(i, tone + pad, tone * 0.9 + pad)
    elif scene == "battle":
        # 驱动节奏 + 低音
        bass = [110.0, 110.0, 146.83, 123.47]
        arp = [440.0, 554.37, 659.25, 554.37]
        for i in range(n):
            t = i / RATE
            beat = t % 0.25
            envb = math.exp(-14.0 * beat)
            b = 0.28 * sine(bass[int(t / 0.5) % len(bass)], t) * envb
            a = 0.14 * sine(arp[int(t / 0.25) % len(arp)], t) * envb
            add(i, b + a * 0.8, b + a)
    elif scene == "result":
        # 解决和弦（渐强后保持）
        freqs = [196.0, 246.94, 293.66, 392.0]
        for i in range(n):
            t = i / RATE
            swell = min(1.0, t / 1.5) * (1.0 - min(1.0, max(0.0, (t - 6.5) / 1.5)))
            s = sum(0.07 * sine(f, t) for f in freqs) * swell
            add(i, s, s * 0.95)
    else:
        raise ValueError(scene)

    # 归一化避免削波
    peak = max(max(abs(x) for x in left), max(abs(x) for x in right), 1e-9)
    if peak > 0.9:
        k = 0.9 / peak
        left = [x * k for x in left]
        right = [x * k for x in right]
    return left, right


def write_wav_mono(path, samples):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        data = bytearray()
        for s in samples:
            data += struct.pack("<h", int(clamp(s) * 32767))
        w.writeframes(bytes(data))


def write_wav_stereo(path, left, right):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with wave.open(path, "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(RATE)
        data = bytearray()
        for l, r in zip(left, right):
            data += struct.pack("<hh", int(clamp(l) * 32767), int(clamp(r) * 32767))
        w.writeframes(bytes(data))


def write_png(path, width, height, pixel):
    """极简 PNG（8-bit RGB）编码器；`pixel(x,y)->(r,g,b)`。无第三方依赖。"""
    import zlib
    import binascii

    raw = bytearray()
    for y in range(height):
        raw.append(0)  # filter type 0
        for x in range(width):
            r, g, b = pixel(x, y)
            raw += bytes((r & 255, g & 255, b & 255))

    def chunk(tag, data):
        out = struct.pack(">I", len(data)) + tag + data
        return out + struct.pack(">I", binascii.crc32(tag + data) & 0xFFFFFFFF)

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", ihdr)
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(png)


def preview_pixel(size):
    """返回一个 pixel 函数：深色渐变 + 同心圆 + 声波条。"""
    cx, cy = size / 2, size / 2

    def pixel(x, y):
        # 背景渐变
        t = (x + y) / (2 * size)
        r = int(18 + 40 * t)
        g = int(22 + 60 * t)
        b = int(34 + 90 * t)
        # 同心圆环
        d = math.hypot(x - cx, y - cy)
        for radius, col in ((0.36 * size, (90, 200, 255)), (0.24 * size, (120, 230, 200))):
            if abs(d - radius) < 3:
                r, g, b = col
        if d < 12:
            r, g, b = (240, 240, 255)
        # 声波条
        if size * 0.62 < y < size * 0.70:
            amp = 26 * math.sin(2 * math.pi * x / (size / 4.0))
            if abs(y - (size * 0.66 + amp)) < 2.5:
                r, g, b = (255, 190, 90)
        return r, g, b

    return pixel


def main():
    if len(sys.argv) > 1:
        out = sys.argv[1]
    else:
        appdata = os.environ.get("APPDATA")
        base = os.path.join(appdata, "warlock_brawl", "audio") if appdata else "audio"
        out = os.path.join(base, "DemoPack")

    cues = read_cue_names()
    if not cues:
        print("!! 未能从 audio.rs 读取 cue 名单", file=sys.stderr)
        sys.exit(1)

    print(f"输出到：{out}")
    print(f"生成 {len(cues)} 个音效 …")
    for name in cues:
        write_wav_mono(os.path.join(out, "sfx", name), sfx_samples(name))

    print("生成 4 段 BGM …")
    for scene in ("menu", "lobby", "battle", "result"):
        l, r = bgm_samples(scene)
        write_wav_stereo(os.path.join(out, "bgm", f"{scene}.wav"), l, r)

    with open(os.path.join(out, "circle_brawl_pack.ini"), "w", encoding="utf-8") as f:
        f.write(
            "name=Demo Pack\n"
            "author=Circle Brawl (generated)\n"
            "version=1\n"
            "type=both\n"
            "description=自动生成的演示包：全部音效为合成音，BGM 4 场景 8 秒循环。\n"
        )
    print("生成预览图 preview.png …")
    write_png(os.path.join(out, "preview.png"), 512, 512, preview_pixel(512))
    with open(os.path.join(out, "README.txt"), "w", encoding="utf-8") as f:
        f.write(
            "这是 tools/gen_demo_audio_pack.py 自动生成的演示音频包。\n"
            "覆盖：全部音效 cue + 4 个 BGM 场景（menu/lobby/battle/result）。\n"
            "真实包建议：音效用 WAV、BGM 用 Ogg Vorbis（更小且无缝循环）；Opus 不支持。\n"
        )
    print("完成。回游戏「设置 → 音效包/BGM 包」选择 Demo Pack，或用「试听当前音效包」。")


if __name__ == "__main__":
    main()
