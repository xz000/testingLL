#!/usr/bin/env python3
"""给出 098c raw `war3map.j` 某些行号所属的**函数**（或读文件里的行号）。

用法：
    python tools/jass_where.py 5802 5908 5983 ...     # 直接给行号
    python tools/jass_where.py --file -              # 从 stdin 读行号（每行一个）

按 `[\\r\\n]+` 切分，与 `tools/jass_find.py` 的行号口径一致。
"""
import argparse
import re
import sys


def load_lines(path):
    with open(path, "rb") as f:
        return re.split(r"[\r\n]+", f.read().decode("latin-1"))


def owner(lines, n):
    fn = "?"
    for k in range(n, -1, -1):
        if lines[k].startswith("function "):
            fn = lines[k].strip()
            break
    return fn


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("lines", nargs="*", type=int)
    ap.add_argument("--file", default=r"..\098c\out\war3map.j")
    ap.add_argument("--stdin", action="store_true")
    args = ap.parse_args()

    nums = list(args.lines)
    if args.stdin:
        nums += [int(x) for x in sys.stdin.read().split()]

    lines = load_lines(args.file)
    for n in nums:
        if 0 <= n < len(lines):
            print(f"{n}: {owner(lines, n)}  |  {lines[n].strip()}")
        else:
            print(f"{n}: (out of range)")


if __name__ == "__main__":
    main()
