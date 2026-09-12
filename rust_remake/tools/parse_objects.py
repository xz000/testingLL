"""解析 098c 的 war3map.w3t（物品）/ w3b（装饰物）/ w3h（增益）对象数据。

真值用途：
- `w3t`：`unam`（物品名）、`iabi`（挂载的物品能力，如 `A007`）、`igol`（金币价）、`utip`。
  注意：**所有 `igol` 都是 0** —— 与 `game-core/src/item.rs` 的说明一致：098c 的物品价格
  是写在 JASS 商店表（`bD`/`ED`）里的，不是物体数据。
- 物品的**属性数值**在 `w3a` 的*物品能力*里（`A000/A004/A005/A007/A009/A00D/A00F/A00H`），
  由 `parse_w3a.py` 解析；本脚本负责给出 物品 → 能力 的映射。

文件结构（本存档**无** `W3T!` 等魔数头，直接以 version 开头）：

    version(4) + n_orig(4) + n_orig×(old(4)+new(4)) + n_custom(4) + records

记录：`old(4) + new(4) + nMods(4) + mods`

修改项布局（与 w3a **不同**，务必注意）：

- 数值（type 0/1/2）：`field(4) + type(4) + level(4) + value(4)` —— 16 字节，**无结束符**
- 字符串（type 3）：`field(4) + type(4) + cstring + 4字节` —— **无 level**；
  末尾那 4 字节在 w3t 里恒为 0，在 w3u 里是**对象 id**（如 `hpea`）。
  （w3a 则是数值 `field+type+level+pad+value+end`、字符串 `field+type+level+cstring+end`。）

校验方式：解析必须**精确**消费到文件末尾。w3t/w3b/w3h 已通过。

用法：`python tools/parse_objects.py`（输出 `../098c/out/war3map.w3t.json` 等）。
"""

import struct
import io
import json
import sys

OUT = r'C:\Users\xvzan\Documents\testingLL\098c\out'


def parse(path):
    b = open(path, 'rb').read()

    def i32(o):
        return struct.unpack_from('<i', b, o)[0]

    p = 4
    n_orig = i32(p)
    p += 4
    orig = []
    for _ in range(n_orig):
        orig.append((b[p:p + 4].decode('latin1'), b[p + 4:p + 8].decode('latin1')))
        p += 8
    n_custom = i32(p)
    p += 4
    recs = {}
    for _ in range(n_custom):
        old = b[p:p + 4].decode('latin1')
        new = b[p + 4:p + 8].decode('latin1')
        nmod = i32(p + 8)
        p += 12
        fields = {}
        for _ in range(nmod):
            f = b[p:p + 4].decode('latin1')
            typ = i32(p + 4)
            if typ == 3:
                e = b.find(b'\0', p + 8)
                val = b[p + 8:e].decode('utf-8', 'replace')
                q = e + 1 + 4
                lvl = 0
            elif typ in (0, 1, 2):
                lvl = i32(p + 8)
                q = p + 12
                val = i32(q) if typ == 0 else struct.unpack_from('<f', b, q)[0]
                q += 4
            else:
                raise ValueError('type %d at %d' % (typ, p))
            p = q
            fields.setdefault(f, []).append({'level': lvl, 'value': val})
        recs[new] = {'old': old, 'fields': fields}
    return {'version': i32(0), 'n_orig': n_orig, 'orig': orig, 'recs': recs,
            'consumed': p, 'size': len(b)}


def main():
    for fn in ['war3map.w3t', 'war3map.w3b', 'war3map.w3h']:
        r = parse(OUT + '\\' + fn)
        ok = 'EXACT' if r['consumed'] == r['size'] else 'MISMATCH'
        print('%-16s recs=%d consumed=%d/%d %s'
              % (fn, len(r['recs']), r['consumed'], r['size'], ok))
        json.dump(r, io.open(OUT + '\\' + fn + '.json', 'w', encoding='utf-8'),
                  ensure_ascii=False)
        if fn == 'war3map.w3t':
            print('%-6s %-6s %-24s %-8s %s' % ('id', 'old', 'unam', 'iabi', 'igol'))
            for k in sorted(r['recs']):
                f = r['recs'][k]['fields']

                def g(n):
                    v = f.get(n)
                    return v[0]['value'] if v else ''
                print('%-6s %-6s %-24s %-8s %s'
                      % (k, r['recs'][k]['old'], str(g('unam'))[:24],
                         str(g('iabi'))[:8], g('igol')))




# ---------------------------------------------------------------------------
# w3u（单位）专用：头部计数语义与 w3t 不同，改用「候选记录头扫描 + 精确闭合」。
#
# 另注意：w3u 的数值 mod 把**值放在 +8**（w3t/w3a 在更后面），即
#   field(4) + type(4) + value(4) + trailer(4)   —— 16 字节
#   field(4) + type(4) + cstring   + trailer(4)
# 例：Warlock 英雄（hpea→h000）`umvs`=210、`uhpm`=100（与我方 balance.rs 一致）。
# ---------------------------------------------------------------------------


def _candidates(b):
    def printable(bs):
        return all(32 <= c < 127 for c in bs)

    def fieldname(bs):
        return all(97 <= c <= 122 or 48 <= c <= 57 for c in bs)

    cand = set()
    for p in range(0, len(b) - 16):
        if not printable(b[p:p + 4]):
            continue
        if not (printable(b[p + 4:p + 8]) or b[p + 4:p + 8] == b'\0\0\0\0'):
            continue
        nmod = struct.unpack_from('<i', b, p + 8)[0]
        if not (0 < nmod <= 400):
            continue
        if not fieldname(b[p + 12:p + 16]):
            continue
        if struct.unpack_from('<i', b, p + 16)[0] not in (0, 1, 2, 3):
            continue
        cand.add(p)
    return cand


def _rec(b, start):
    def i32(o):
        return struct.unpack_from('<i', b, o)[0]

    old = b[start:start + 4].decode('latin1')
    new = b[start + 4:start + 8].decode('latin1')
    nmod = i32(start + 8)
    p = start + 12
    fields = {}
    for _ in range(nmod):
        f = b[p:p + 4].decode('latin1')
        typ = i32(p + 4)
        if typ == 3:
            e = b.find(b'\0', p + 8)
            val = b[p + 8:e].decode('utf-8', 'replace')
            q = e + 1 + 4
        elif typ in (0, 1, 2):
            q = p + 8
            val = i32(q) if typ == 0 else struct.unpack_from('<f', b, q)[0]
            q += 8
        else:
            raise ValueError('type %d' % typ)
        p = q
        fields.setdefault(f, []).append({'level': 0, 'value': val})
    return {'old': old, 'new': new, 'fields': fields, 'end': p}


def parse_units(path):
    b = open(path, 'rb').read()
    cand = _candidates(b)
    recs = {}
    for start in sorted(cand):
        try:
            r = _rec(b, start)
        except Exception:
            continue
        if r['end'] in cand or r['end'] == len(b):
            recs['%s->%s' % (r['old'], r['new'])] = {'start': start, **r}
    return {'recs': recs, 'candidates': len(cand), 'size': len(b)}


def dump_units():
    r = parse_units(OUT + r'\war3map.w3u')
    print('war3map.w3u     closed=%d/%d' % (len(r['recs']), r['candidates']))
    json.dump(r, io.open(OUT + r'\war3map.w3u.json', 'w', encoding='utf-8'), ensure_ascii=False)
    keys = ['unam', 'umvs', 'uhpm', 'uabi', 'umdl']
    for k in sorted(r['recs'], key=lambda x: r['recs'][x]['start']):
        f = r['recs'][k]['fields']
        parts = []
        for kk in keys:
            if kk in f:
                parts.append('%s=%s' % (kk, str(f[kk][0]['value'])[:26]))
        print('  %-14s %s' % (k, ' | '.join(parts)))


if __name__ == '__main__':
    sys.stdout.reconfigure(errors='replace')
    main()
    dump_units()
