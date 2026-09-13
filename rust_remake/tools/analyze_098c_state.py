#!/usr/bin/env python3
"""098c 状态数组审计助手（只读）。

扫描 war3map_pretty.j，对给定的「按单位/玩家索引的全局数组」输出：
- 引用总数；
- 若干 `set X[...] =` 赋值样例（含行号）；
- 若干读取样例（含行号）。

用法：
    python tools/analyze_098c_state.py name1 name2 ...
输出到 stdout（可重定向到文件）。
"""
import re
import sys
from pathlib import Path

JASS = Path(__file__).resolve().parents[2] / "098c" / "out" / "war3map_pretty.j"


def main() -> None:
    names = sys.argv[1:]
    if not names:
        print("usage: analyze_098c_state.py NAME...")
        return
    if not JASS.exists():
        print(f"not found: {JASS}")
        return
    lines = JASS.read_text(encoding="utf-8", errors="replace").splitlines()

    for name in names:
        set_pat = re.compile(r"set\s+" + re.escape(name) + r"\[[^\]]*\]\s*=")
        read_pat = re.compile(r"\b" + re.escape(name) + r"\[[^\]]*\]")
        sets, reads, total = [], [], 0
        for i, line in enumerate(lines, 1):
            if read_pat.search(line):
                total += 1
                if set_pat.search(line):
                    if len(sets) < 5:
                        sets.append(f"  set@{i}: {line.strip()}")
                elif len(reads) < 5:
                    reads.append(f"  rd @{i}: {line.strip()}")
        print(f"===== {name} (refs {total}) =====")
        print("\n".join(sets))
        print("\n".join(reads))


if __name__ == "__main__":
    main()
