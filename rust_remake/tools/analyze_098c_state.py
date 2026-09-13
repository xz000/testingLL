#!/usr/bin/env python3
"""098c 状态数组 / 技能函数审计助手（只读）。

两种模式：
1) 数组模式：`python tools/analyze_098c_state.py Hr Fv nv ...`
   对每个数组名输出：引用总数、`set X[...] =` 赋值样例、读取样例。
2) 函数模式：`python tools/analyze_098c_state.py @AB @WB @mC ...`
   对每个函数名（**大小写敏感**）输出其函数体内的 `set <数组>[...] =` 写入行与 `call <fn>(` 调用行，
   便于把一个技能实现所写的「副状态」列全。

真值文件：../098c/out/war3map_pretty.j
"""
import re
import sys
from pathlib import Path

JASS = Path(__file__).resolve().parents[2] / "098c" / "out" / "war3map_pretty.j"


def load_lines():
    if not JASS.exists():
        print(f"not found: {JASS}")
        sys.exit(1)
    return JASS.read_text(encoding="utf-8", errors="replace").splitlines()


def array_mode(lines, names):
    for name in names:
        set_pat = re.compile(r"set\s+" + re.escape(name) + r"\[[^\]]*\]\s*=")
        read_pat = re.compile(r"\b" + re.escape(name) + r"\[[^\]]*\]")
        sets, reads, total = [], [], 0
        for i, line in enumerate(lines, 1):
            if read_pat.search(line):
                total += 1
                if set_pat.search(line):
                    if len(sets) < 6:
                        sets.append(f"  set@{i}: {line.strip()}")
                elif len(reads) < 6:
                    reads.append(f"  rd @{i}: {line.strip()}")
        print(f"===== {name} (refs {total}) =====")
        print("\n".join(sets))
        print("\n".join(reads))


def function_body(lines, name):
    """返回 (start_line, body_lines)，大小写敏感匹配 `function <name> takes`。"""
    start_pat = re.compile(r"^function " + re.escape(name) + r" takes")
    for i, line in enumerate(lines):
        if start_pat.match(line):
            body = []
            for j in range(i + 1, len(lines)):
                if lines[j].startswith("function "):
                    break
                body.append((j + 1, lines[j]))
            return i + 1, body
    return None, []


def fn_mode(lines, names):
    set_arr = re.compile(r"set\s+([A-Za-z0-9_]+)\[[^\]]*\]\s*=")
    call_pat = re.compile(r"call\s+([A-Za-z0-9_]+)\(")
    for name in names:
        start, body = function_body(lines, name)
        if start is None:
            print(f"===== @{name} NOT FOUND =====")
            continue
        print(f"===== @{name} (start {start}) =====")
        writes = []
        calls = []
        for lineno, line in body:
            m = set_arr.search(line)
            if m:
                writes.append(f"  set {m.group(1)} @{lineno}: {line.strip()}")
            for c in call_pat.findall(line):
                calls.append(c)
        seen = []
        for c in calls:
            if c not in seen:
                seen.append(c)
        print("  -- state writes --")
        print("\n".join(writes) if writes else "  (none)")
        print(f"  -- calls: {' '.join(seen)}")


def main() -> None:
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return
    lines = load_lines()
    arr = [a for a in args if not a.startswith("@")]
    fns = [a[1:] for a in args if a.startswith("@")]
    if arr:
        array_mode(lines, arr)
    if fns:
        fn_mode(lines, fns)


if __name__ == "__main__":
    main()
