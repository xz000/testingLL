#!/usr/bin/env python3
"""098c `war3map.j` raw 检索器（供「与 JASS 对齐」核对用）。

用法：
    python tools/jass_find.py <regex> [--ctx N] [--file PATH]

- raw `.j` 用 CR 作行分隔（LF 仅 85 处），`findstr`/多数编辑器会数错行 → 本工具按 `[\\r\\n]+` 切分，
  行号与历史文档里 `Select-String` 的口径一致（`mI`≈7051、`DA`≈8340）。
- 只读、确定性。输出 `行号: 文本` 与可选的上下文行。
"""
import argparse
import re
import sys

DEFAULT = r"..\098c\out\war3map.j"


def load_lines(path):
    with open(path, "rb") as f:
        data = f.read()
    text = data.decode("latin-1")
    return re.split(r"[\r\n]+", text)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("pattern")
    ap.add_argument("--ctx", type=int, default=0, help="匹配行前后各打印 N 行")
    ap.add_argument("--file", default=DEFAULT)
    ap.add_argument("--max", type=int, default=200, help="最多打印多少条匹配")
    args = ap.parse_args()

    lines = load_lines(args.file)
    try:
        rx = re.compile(args.pattern)
    except re.error as e:
        print(f"bad regex: {e}", file=sys.stderr)
        return 2

    hits = 0
    for i, line in enumerate(lines):
        if rx.search(line):
            hits += 1
            if hits > args.max:
                print(f"... (truncated at {args.max} hits)")
                break
            lo = max(0, i - args.ctx)
            hi = min(len(lines), i + args.ctx + 1)
            for j in range(lo, hi):
                mark = ">" if j == i else " "
                print(f"{mark}{j:6d}: {lines[j].strip()}")
            if args.ctx:
                print("  " + "-" * 40)
    print(f"# {hits} hit(s) for /{args.pattern}/", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
