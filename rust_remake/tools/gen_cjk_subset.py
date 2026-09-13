#!/usr/bin/env python3
"""GenCore 客户端内置 CJK 子集字体生成器。

把完整 `assets/fonts/cjk.ttf`（Noto Sans CJK SC）裁剪成只含**本项目源码实际用到**的字符，
输出 `assets/fonts/cjk-168k.ttf`（随 exe 内嵌，作为找不到完整字体时的回退）。

背景：此前子集是在更早的源码上生成的，后来新增的 UI 文案（如状态图标「燃/灼/弱」）
漏了对应字形，导致回退字体下显示为方框（豆腐块）。**改了 UI 文案后请重跑本脚本**。

用法：
    python tools/gen_cjk_subset.py            # 用默认路径
    python tools/gen_cjk_subset.py --check    # 只检查完整字体是否覆盖源码字符，不改文件

许可：Noto Sans CJK SC 为 SIL OFL 1.1，子集化再分发需保留 OFL 声明（见
`assets/fonts/FONT_LICENSE_168k.md`）。
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

# Windows 控制台默认 GBK，源码里的变体选择符/emoji 会 print 报错：改用 UTF-8 + replace。
try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
except Exception:
    pass

REPO = Path(__file__).resolve().parent.parent
FULL_FONT = REPO / "assets" / "fonts" / "cjk.ttf"
OUT_FONT = REPO / "assets" / "fonts" / "cjk-168k.ttf"

# 扫描这些目录下的源码（.rs），收集其中出现的所有字符。
SOURCE_DIRS = ["client/src", "game-core/src", "net/src", "net-steam/src", "net-lan/src", "tools"]

# 始终包含：可打印 ASCII + 常用中/英标点（避免遗漏代码里拼接/动态产生的字符）。
ALWAYS = (
    "".join(chr(c) for c in range(0x20, 0x7F))
    + "　"
    + "，。、；：！？…—·×÷°"
    + "“”‘’「」『』（）【】《》〈〉〔〕"
    + "－±×≈≤≥≠∞"
)


def collect_chars() -> set[str]:
    chars: set[str] = set(ALWAYS)
    for d in SOURCE_DIRS:
        p = REPO / d
        if not p.exists():
            continue
        for f in p.rglob("*.rs"):
            try:
                chars.update(f.read_text(encoding="utf-8"))
            except OSError as e:
                print(f"[warn] 读取失败 {f}: {e}", file=sys.stderr)
    # 去掉控制字符/换行/制表以外的空白之外的东西不处理（保留空格）。
    return {c for c in chars if c >= " "}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="只检查完整字体覆盖，不改文件")
    ap.add_argument("--full", type=Path, default=FULL_FONT)
    ap.add_argument("--out", type=Path, default=OUT_FONT)
    args = ap.parse_args()

    if not args.full.exists():
        print(f"[error] 找不到完整字体：{args.full}", file=sys.stderr)
        return 2

    try:
        from fontTools import subset
        from fontTools.ttLib import TTFont
    except ImportError:
        print("[error] 需要 fontTools：pip install fonttools", file=sys.stderr)
        return 2

    chars = sorted(collect_chars())
    full = TTFont(args.full)
    cmap = full.getBestCmap()
    missing = [c for c in chars if ord(c) not in cmap and c not in ("\t", "\n", "\r")]
    print(f"[info] 源码字符 {len(chars)} 个；完整字体内缺 {len(missing)} 个：{''.join(missing)}")
    if missing:
        print("[warn] 完整字体也缺这些字形（会在回退字体里显示为方框）。", file=sys.stderr)

    if args.check:
        return 0

    opts = subset.Options()
    opts.layout_features = []            # 客户端用 ab_glyph 逐字渲染，不需要 GSUB/GPOS（可大幅减小）
    opts.name_IDs = []
    opts.notdef_outline = True
    opts.recommended_glyphs = True
    opts.hinting = False                 # 去 hinting（屏幕渲染用不到，减小体积）

    font = subset.load_font(str(args.full), opts)
    subsetter = subset.Subsetter(options=opts)
    subsetter.populate(text="".join(c for c in chars if ord(c) in cmap))
    subsetter.subset(font)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    subset.save_font(font, str(args.out), opts)
    size_kb = args.out.stat().st_size / 1024.0
    print(f"[ok] 已写出 {args.out}（{size_kb:.0f} KB，{len(chars)} 个源码字符）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
