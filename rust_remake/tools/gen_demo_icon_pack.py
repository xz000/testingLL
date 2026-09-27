#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成一个「演示图标包」用于测试本地图标包 / 创意工坊（用现有 CJK 字体画汉字）。

- 键表：调用客户端 `--dump-icon-keys` 导出（**权威来源**，避免在 Python 里重复维护 id/名字）。
- 每个键画一张正方形 PNG：按键名散列的底色圆角块 + 名称前 2 个汉字（占位风格）。
- 输出：默认 %APPDATA%/warlock_brawl/icons/DemoIconPack，可用参数覆盖。

用法：
    pip install pillow
    python tools/gen_demo_icon_pack.py [输出目录]

说明：真实图标包只要**同名替换**生成的 PNG/换成自己的图即可，键名见输出目录 `keys.txt`。
"""

import colorsys
import os
import re
import subprocess
import sys

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    sys.exit("需要 Pillow：pip install pillow")

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, ".."))
FONT = os.path.join(REPO, "assets", "fonts", "LXGWWenKaiMonoLite-Medium.ttf")
SIZE = 128


def dump_keys():
    """调客户端 `--dump-icon-keys` 导出键→名；每行 `stem<TAB>name`。"""
    proc = subprocess.run(
        ["cargo", "run", "-q", "-p", "client", "--", "--dump-icon-keys"],
        cwd=REPO,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if proc.returncode != 0:
        sys.exit("导出键表失败：\n" + (proc.stderr or "")[:2000])
    out = []
    for line in proc.stdout.splitlines():
        if not line or "\t" not in line:
            continue
        stem, name = line.split("\t", 1)
        out.append((stem.strip(), name.strip()))
    return out


def bg_color(stem):
    """按键名散列取一个暗色底（同键稳定）。"""
    h = 0
    for ch in stem:
        h = (h * 131 + ord(ch)) & 0xFFFFFFFF
    r, g, b = colorsys.hsv_to_rgb((h % 360) / 360.0, 0.45, 0.32)
    return (int(r * 255), int(g * 255), int(b * 255))


def short_label(name):
    """取短标签：`·` 前的主名，去掉括号备注，取前 2 个字符。"""
    core = name.split("·")[0]
    core = re.sub(r"[（(].*?[)）]", "", core).strip()
    return (core or name)[:2]


def draw_icon(path, stem, name):
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle(
        [2, 2, SIZE - 2, SIZE - 2],
        radius=18,
        fill=bg_color(stem),
        outline=(255, 255, 255, 48),
        width=2,
    )
    label = short_label(name)
    size = int(SIZE * (0.58 if len(label) <= 1 else 0.48))
    font = ImageFont.truetype(FONT, size)
    bbox = d.textbbox((0, 0), label, font=font)
    w, h = bbox[2] - bbox[0], bbox[3] - bbox[1]
    d.text(((SIZE - w) / 2 - bbox[0], (SIZE - h) / 2 - bbox[1]), label, font=font, fill=(255, 255, 255, 255))
    os.makedirs(os.path.dirname(path), exist_ok=True)
    img.save(path)


def build_preview(keys, path, cols=5):
    """把前 N 张图标拼成工坊预览图。"""
    n = min(len(keys), cols * cols)
    cell = 96
    pad = 8
    size = cols * cell + (cols + 1) * pad
    img = Image.new("RGBA", (size, size), (18, 20, 28, 255))
    for i in range(n):
        stem, name = keys[i]
        x = pad + (i % cols) * (cell + pad)
        y = pad + (i // cols) * (cell + pad)
        ic = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
        d = ImageDraw.Draw(ic)
        d.rounded_rectangle([2, 2, SIZE - 2, SIZE - 2], radius=18, fill=bg_color(stem))
        label = short_label(name)
        font = ImageFont.truetype(FONT, int(SIZE * (0.58 if len(label) <= 1 else 0.48)))
        bbox = d.textbbox((0, 0), label, font=font)
        d.text(((SIZE - (bbox[2] - bbox[0])) / 2 - bbox[0], (SIZE - (bbox[3] - bbox[1])) / 2 - bbox[1]),
               label, font=font, fill=(255, 255, 255, 255))
        img.alpha_composite(ic.resize((cell, cell), Image.LANCZOS), (x, y))
    img.convert("RGB").save(path)


def main():
    if len(sys.argv) > 1:
        out = sys.argv[1]
    else:
        base = os.path.join(os.environ.get("APPDATA", "."), "warlock_brawl", "icons")
        out = os.path.join(base, "DemoIconPack")
    if not os.path.isfile(FONT):
        sys.exit(f"找不到字体：{FONT}")

    print("导出键表（cargo run … --dump-icon-keys）…")
    keys = dump_keys()
    if not keys:
        sys.exit("键表为空")

    print(f"输出到：{out}")
    print(f"生成 {len(keys)} 张图标 …")
    for stem, name in keys:
        draw_icon(os.path.join(out, "icons", stem + ".png"), stem, name)

    with open(os.path.join(out, "circle_brawl_pack.ini"), "w", encoding="utf-8") as f:
        f.write(
            "name=Demo Icon Pack\n"
            "author=Circle Brawl (generated)\n"
            "version=1\n"
            "type=icons\n"
            "description=自动生成的演示图标包：汉字 + 底色方块，覆盖全部技能/物品键。\n"
        )
    with open(os.path.join(out, "keys.txt"), "w", encoding="utf-8") as f:
        f.write("\n".join(s for s, _ in keys) + "\n")
    print("生成预览图 preview.png …")
    build_preview(keys, os.path.join(out, "preview.png"))
    print("完成。回游戏「设置 → 图标包」选择 DemoIconPack（或改包名后上传创意工坊）。")


if __name__ == "__main__":
    main()
