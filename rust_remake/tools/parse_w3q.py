#!/usr/bin/env python3
"""解析 `098c/out/war3map.w3q`（升级/研究对象）→ 打印每个研究的 gglb/gglm/glvl。

w3q 布局（据 `ECONOMY_RECHECK.md`：24 字节数值 mod，值在 field+16）：
    version(4) n_orig(4) n_orig*(old4,new4) n_custom(4) records
    记录: old(4) new(4) nMods(4) mods
    数值 mod(24): field(4) type(4) level(4) pad(4) value(4) end(4)
    字符串 mod: field(4) type(4) level(4) cstring end(4)
"""
import io
import struct

PATH = r"..\098c\out\war3map.w3q"


def cstr(b, p):
    e = b.index(b"\x00", p)
    return b[p:e].decode("latin-1"), e + 1


def main():
    b = io.open(PATH, "rb").read()
    p = 0
    version = struct.unpack_from("<I", b, p)[0]
    p += 4
    n_orig = struct.unpack_from("<I", b, p)[0]
    p += 4 + n_orig * 8
    n_custom = struct.unpack_from("<I", b, p)[0]
    p += 4
    print(f"version={version} n_orig={n_orig} n_custom={n_custom}")
    rows = []
    for _ in range(n_custom):
        old = b[p:p + 4].decode("latin-1")
        new = b[p + 4:p + 8].decode("latin-1")
        nmods = struct.unpack_from("<I", b, p + 8)[0]
        p += 12
        f = {}
        for _ in range(nmods):
            field = b[p:p + 4].decode("latin-1")
            typ = struct.unpack_from("<I", b, p + 4)[0]
            level = struct.unpack_from("<I", b, p + 8)[0]
            if typ in (0, 1, 2):
                val = struct.unpack_from("<i", b, p + 16)[0]
                if typ in (1, 2):
                    val = struct.unpack_from("<f", b, p + 16)[0]
                p += 24
            else:
                s, p = cstr(b, p + 12)
                s2, p = cstr(b, p)
                val = s2
            f.setdefault(field, {})[level] = val
        rows.append((new, f))
    for new, f in rows:
        gglb = f.get("gglb", {})
        gglm = f.get("gglm", {})
        glvl = f.get("glvl", {})
        base = next(iter(gglb.values()), None)
        mod = next(iter(gglm.values()), None)
        mx = next(iter(glvl.values()), None)
        if base is not None or mod is not None or mx is not None:
            print(f"{new}: gglb={base} gglm={mod} glvl={mx}")


if __name__ == "__main__":
    main()
