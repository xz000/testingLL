#!/usr/bin/env python3
"""对 098c 弹体生成函数抽取**回调集 + 生成标志**，便于逐技能核对。

对每个给定函数，打印 `OO(2,..)` 新建对象后设置的：
  hv/jv/Gv/Jv（碰撞/每帧/到寿/销毁 回调）、Av（碰撞掩码）、Rv（半径）、xv（撞柱）、
  ev（寿命）、Q/S（初速）、gv/Xv（数值槽/等级）。

用法：python tools/jass_callbacks.py Ab Pb Ub ...
"""
import re
import sys

PATH = r"..\098c\out\war3map.j"
KEYS = ["Av", "Rv", "xv", "ev", "gv", "Xv", "Q", "S", "hv", "jv", "Gv", "Jv", "Ev", "Nv", "bv"]


def load():
    return re.split(r"[\r\n]+", open(PATH, "rb").read().decode("latin-1"))


def aliases(lines):
    """`set X=Condition(function Y)` → {X: Y}（hv/jv/Gv/Jv 存的是这些 boolexpr 别名）。"""
    m = {}
    for l in lines:
        r = re.match(r"set (\w+)\s*=\s*Condition\(function (\w+)\)", l)
        if r:
            m[r.group(1)] = r.group(2)
    return m


def func(lines, name):
    i = next((k for k, l in enumerate(lines) if l.startswith("function " + name + " ")), None)
    if i is None:
        return None
    j = next(k for k in range(i + 1, len(lines)) if lines[k] == "endfunction")
    return lines[i:j + 1]


def main():
    lines = load()
    al = aliases(lines)
    for name in sys.argv[1:]:
        body = func(lines, name)
        print(f"===== {name} =====")
        if body is None:
            print("  (not found)")
            continue
        for l in body:
            m = re.match(r"set (" + "|".join(KEYS) + r")\[([^\]]+)\]\s*=(.*)", l)
            if m:
                v = m.group(3).strip()
                res = f"  → {al[v]}" if v in al else ""
                print(f"  {m.group(1)}[{m.group(2)}] = {v}{res}")
        # 也打印 OO 生成与 SetUnitScale
        for l in body:
            if "OO(2" in l or "SetUnitScale" in l or "bO(" in l or "CO(" in l:
                print(f"  · {l.strip()}")
        print()


if __name__ == "__main__":
    main()
