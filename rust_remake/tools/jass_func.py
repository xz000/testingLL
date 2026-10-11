#!/usr/bin/env python3
"""打印 098c raw `war3map.j` 的整函数（按名字）。

用法：
    python tools/jass_func.py Na EB OB
    python tools/jass_func.py --contains mI      # 名字包含即可（模糊）
"""
import argparse
import re


def load_lines(path):
    with open(path, "rb") as f:
        return re.split(r"[\r\n]+", f.read().decode("latin-1"))


def print_func(lines, name, exact=True):
    starts = []
    for k, l in enumerate(lines):
        if not l.startswith("function "):
            continue
        head = l[len("function "):].split(" takes")[0].strip()
        if (head == name) if exact else (name in head):
            starts.append((k, head))
    if not starts:
        print(f"!! no function {name}")
        return
    for k, head in starts:
        j = next((m for m in range(k + 1, len(lines)) if lines[m] == "endfunction"), len(lines) - 1)
        print(f"==== {head} ({k}-{j}) ====")
        for m in range(k, j + 1):
            print(f"{m}: {lines[m]}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("names", nargs="+")
    ap.add_argument("--file", default=r"..\098c\out\war3map.j")
    ap.add_argument("--contains", action="store_true")
    args = ap.parse_args()
    lines = load_lines(args.file)
    for n in args.names:
        print_func(lines, n, exact=not args.contains)


if __name__ == "__main__":
    main()
