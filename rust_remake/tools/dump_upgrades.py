#!/usr/bin/env python3
"""从 `098c/out/objects.json` 提取 w3q 升级/研究价格（技能购买/升级价的真值）。

w3q 字段：`gglb`=基础金价, `gglm`=每级金价增量, `glvl`=最大等级, `glmb`=基础木, `glmm`=每级木。

用法：python tools/dump_upgrades.py [id...]   （无参数=全列）
"""
import io
import json
import sys

PATH = r"..\098c\out\objects.json"

# 098c `kf` 研究 id → 技能（购买研究 = learn_cost；升级研究 = upgrade_cost）
PURCHASE = {
    "R002": "S000",
    "R009": "S002", "R005": "S003", "R00Z": "S004",
    "R00F": "S010", "R00A": "S008", "R00H": "S009",
    "R004": "S011", "R007": "S012", "R013": "S013",
    "R003": "S016", "R001": "S015", "R00E": "S014",
    "R006": "S007", "R00C": "S005", "R011": "S006",
    "R00B": "S019", "R00G": "S018", "R008": "S017",
}
UPGRADE = {
    "R002": "S000", "R00P": "S002", "R00O": "S003", "R010": "S004",
    "R00T": "S005", "R012": "S006", "R00S": "S007", "R00R": "S008",
    "R00U": "S009", "R00X": "S010", "R00V": "S011", "R00W": "S012",
    "R014": "S013", "R00L": "S014", "R00J": "S015", "R00K": "S016",
    "R00M": "S017", "R00N": "S018", "R00Q": "S019",
}


def val(entry, field, default=None):
    v = entry.get(field, default)
    if isinstance(v, dict):
        # {level: value} 形如 {"1": 5.0}
        return v.get("1", v)
    return v


def main():
    data = json.load(io.open(PATH, encoding="utf-8", errors="replace"))
    up = data["upgrades"]
    if isinstance(up, dict):
        items = up.items()
    else:
        items = [(e.get("new") or e.get("id"), e) for e in up]
    wanted = sys.argv[1:]
    for rid, e in items:
        if wanted and rid not in wanted:
            continue
        role = ""
        if rid in PURCHASE:
            role += f" purchase->{PURCHASE[rid]}"
        if rid in UPGRADE:
            role += f" upgrade->{UPGRADE[rid]}"
        print(
            f"{rid}: gglb={val(e,'gglb')} gglm={val(e,'gglm')} glvl={val(e,'glvl')}"
            f" glmb={val(e,'glmb')} glmm={val(e,'glmm')}{role}"
        )


if __name__ == "__main__":
    main()
