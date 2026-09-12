import struct, sys, io, json, re
sys.stdout.reconfigure(errors='replace')
b = open(r'C:\Users\xvzan\Documents\testingLL\098c\out\war3map.w3a', 'rb').read()


def i32(o):
    return struct.unpack_from('<i', b, o)[0]


def parse_mod(p, pad=True, end=True):
    f = b[p:p + 4]
    if not all(48 <= c <= 122 for c in f):
        raise ValueError('field %r' % f)
    typ = i32(p + 4)
    if typ not in (0, 1, 2, 3):
        raise ValueError('type %d' % typ)
    lvl = i32(p + 8)
    if not (0 <= lvl <= 200):
        raise ValueError('lvl %d' % lvl)
    q = p + 12 + (4 if pad else 0)
    if typ == 0:
        val = i32(q); q += 4
    elif typ in (1, 2):
        val = struct.unpack_from('<f', b, q)[0]; q += 4
    else:
        e = b.find(b'\0', q)
        val = b[q:e].decode('utf-8', 'replace'); q = e + 1
    if end:
        if i32(q) != 0:
            raise ValueError('end %d' % i32(q))
        q += 4
    return q, f.decode(), typ, lvl, val


# 候选头
cand = set()
for p in range(0, len(b) - 16):
    s = b[p:p + 8]
    if not all(32 <= c < 127 for c in s):
        continue
    nmod = i32(p + 8)
    if not (0 < nmod <= 400):
        continue
    f = b[p + 12:p + 16]
    if not all(48 <= c <= 122 for c in f):
        continue
    if i32(p + 16) not in (0, 1, 2, 3):
        continue
    cand.add(p)

ok_recs = {}
for start in sorted(cand):
    old = b[start:start + 4].decode()
    new = b[start + 4:start + 8].decode()
    nmod = i32(start + 8)
    p = start + 12
    fields = {}
    try:
        for _ in range(nmod):
            p, f, typ, lvl, val = parse_mod(p)
            fields.setdefault(f, []).append({'level': lvl, 'type': typ, 'value': val})
    except Exception:
        continue
    if p in cand or p == len(b):
        ok_recs[new] = {'old': old, 'start': start, 'nmod': nmod, 'fields': fields}
print('valid records', len(ok_recs))
print('ids:', ' '.join(sorted(ok_recs)))

TOOL = ['Damage', 'Range', 'Cooldown', 'Duration']


def nums(txt):
    r = {}
    for k in TOOL:
        m = re.search(re.escape(k) + r':\s*\|c[0-9a-fA-F]{8}([0-9.]+)\|r', txt)
        if m:
            r[k] = float(m.group(1))
    return r


table = {}
for aid, rec in ok_recs.items():
    f = rec['fields']
    lv = max((d['value'] for d in f.get('alev', [])), default=None) if 'alev' in f else None
    cd = {str(d['level']): round(d['value'], 2) for d in f.get('acdn', [])}
    tip = {str(d['level']): nums(d['value']) for d in f.get('aub1', [])}
    table[aid] = {'old': rec['old'], 'levels': lv, 'acdn': cd, 'tip': tip}
json.dump({'table': table}, io.open(r'C:\Users\xvzan\Documents\testingLL\098c\out\w3a_table.json', 'w', encoding='utf-8'), ensure_ascii=False)
json.dump(ok_recs, io.open(r'C:\Users\xvzan\Documents\testingLL\098c\out\w3a_raw.json', 'w', encoding='utf-8'), ensure_ascii=False)
print('saved w3a_table.json')
for aid in sorted(table):
    t = table[aid]
    if t['tip']:
        lv = sorted(t['tip'], key=int)
        print(aid, t['old'], 'lev', t['levels'], '| tip', {k: t['tip'][k] for k in lv[:3]})
