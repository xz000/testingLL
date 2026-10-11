#!/usr/bin/env python3
"""提取 `098c/out/w3a_parsed.json` 里各技能的 `anam`（名）与 `aub1`/`atp1`（tooltip）。

用法：python tools/dump_tooltips.py [S000 S003 ...]   （无参数=全部 S0xx）
注意：098c tooltip **常与代码不符**（见台账 §14/§15）；本工具仅取原文供参考。
"""
import io
import json
import sys

PATH = r"..\098c\out\w3a_parsed.json"


def field(fields, key, level=0):
    arr = fields.get(key)
    if not arr:
        return None
    for e in arr:
        if e.get("level") == level:
            return e.get("value")
    return arr[0].get("value")


def main():
    d = json.load(io.open(PATH, encoding="utf-8", errors="replace"))
    wanted = sys.argv[1:]
    for sid in sorted(d):
        if not sid.startswith("S"):
            continue
        if wanted and sid not in wanted:
            continue
        f = d[sid]["fields"]
        name = field(f, "anam")
        ub = field(f, "aub1")
        tp = field(f, "atp1")
        lv = field(f, "alev")
        print(f"===== {sid} ({name}) alev={lv} =====")
        if tp:
            print(f"[atp1] {tp}")
        if ub:
            print(f"[aub1] {ub}")
        print()


if __name__ == "__main__":
    main()
