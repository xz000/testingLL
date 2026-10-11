#!/usr/bin/env python3
"""扫描 w3q：列出每个研究对象的 gglb/gglm/glvl（金币 base/mod/最大级）。

布局特殊，故用「扫字段 + 回溯最近 4 字符 id」的启发式（够用于对价）。
"""
import io
import struct

PATH = r"..\098c_20260924\Warlock098c\war3map.w3q"


def id_before(b, pos, span=260):
    best = None
    for p in range(max(0, pos - span), pos - 3):
        tok = b[p:p + 4]
        if len(tok) == 4 and all(48 <= c <= 57 or 65 <= c <= 90 or 97 <= c <= 122 for c in tok):
            # 常见 id 以 R 开头
            if tok[0:1] == b"R":
                best = tok.decode("latin-1")
    return best


def main():
    b = io.open(PATH, "rb").read()
    rows = {}
    for field in (b"gglb", b"gglm", b"glvl"):
        start = 0
        while True:
            p = b.find(field, start)
            if p < 0:
                break
            start = p + 4
            try:
                val = struct.unpack_from("<i", b, p + 16)[0]
            except struct.error:
                continue
            rid = id_before(b, p)
            if rid:
                rows.setdefault(rid, {})[field.decode()] = val
    for rid in sorted(rows):
        r = rows[rid]
        print(f"{rid}: gglb={r.get('gglb')} gglm={r.get('gglm')} glvl={r.get('glvl')}")


if __name__ == "__main__":
    main()
