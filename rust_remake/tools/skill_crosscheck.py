import json, io, sys
sys.stdout.reconfigure(errors='replace')

W3A_TABLE = r'C:\Users\xvzan\Documents\testingLL\098c\out\w3a_table.json'

w3a = json.load(io.open(W3A_TABLE, encoding='utf-8'))['table']

ours = {}
for ln in io.open('_ours.tsv', encoding='utf-8').read().split('\n')[1:]:
    if not ln.strip():
        continue
    p = ln.split('\t')
    ours[(p[0], p[1], int(p[2]))] = {
        'cooldown': float(p[3]), 'damage': float(p[4]), 'range': float(p[5]),
        'max_distance': float(p[6]), 'duration': float(p[7]), 'radius': float(p[8]),
        'speed': float(p[9]), 'push_damage': float(p[10]),
    }

# 已知/有意差异：不在报告里报为「问题」。
#   (技能, 形态) 或 技能 → 原因
IGNORE_SKILL = {
    'Ash2': '非玩家技能（地图内置）',
    'S128': '非玩家技能',
    'S022': '已移除（098c 无此技能）',
    'S027': '非玩家技能/被动',
    'S032': '形态切换 dummy（无实际效果）',
    'S033': '形态切换 dummy',
    'S034': '形态切换 dummy',
    'S035': '形态切换 dummy',
    'S000': '098c 配了 24 级，我方 max_level=10（受研究上限约束，玩家不可达）',
}
IGNORE_FORM = {
    ('S007', 'B'): 'S007B 在 098c 不可达（形态切换只接线 S032–S035 四棵树，C 树没有）',
}
RANGE_SKIP = {
    ('S011', 'B'): '098c tooltip 写固定 900，JASS `HB` 用 700+70×Wr（同 A）；按 JASS 为准取后者',
}


def map_level(lvl):
    return ('A', lvl) if lvl <= 10 else ('B', lvl - 10)


TOL = 0.06
report = []
ignored = []
for aid in sorted(w3a):
    reason_skill = IGNORE_SKILL.get(aid)
    t = w3a[aid]
    if not t['tip'] and not t['acdn']:
        continue
    lvls = sorted(set(int(k) for k in t['tip']) | set(int(k) for k in t['acdn']))
    for lvl in lvls:
        form, olv = map_level(lvl)
        o = ours.get((aid, form, olv))
        tip = t['tip'].get(str(lvl), {})
        acdn = t['acdn'].get(str(lvl))
        bucket = ignored if (reason_skill or (aid, form) in IGNORE_FORM) else report
        why = reason_skill or IGNORE_FORM.get((aid, form))
        if o is None:
            bucket.append((aid, form, olv, 'MISSING_OURS', '', why or ''))
            continue
        if acdn is not None and abs(acdn - o['cooldown']) > TOL:
            bucket.append((aid, form, olv, 'cooldown', acdn, round(o['cooldown'], 2)))
        if 'Damage' in tip and abs(tip['Damage'] - o['damage']) > TOL:
            bucket.append((aid, form, olv, 'damage', tip['Damage'], round(o['damage'], 2)))
        if 'Duration' in tip and abs(tip['Duration'] - o['duration']) > TOL:
            bucket.append((aid, form, olv, 'duration', tip['Duration'], round(o['duration'], 2)))
        if 'Range' in tip and (aid, form) not in RANGE_SKIP:
            rng = tip['Range']
            near = min(abs(rng - o['range']), abs(rng - o['max_distance']))
            if near > TOL:
                bucket.append((aid, form, olv, 'range', rng,
                               f"range={o['range']:.0f} md={o['max_distance']:.0f}"))


def show(title, rows):
    print('=== %s (%d) ===' % (title, len(rows)))
    cur = None
    for r in rows:
        if r[0] != cur:
            cur = r[0]
            print('---', cur)
        print('  %s L%-2d %-12s 098c=%-8s ours=%s' % (r[1], r[2], r[3], r[4], r[5]))


show('问题（需要处理）', report)
print()
show('已知/有意差异', ignored)
for aid, why in IGNORE_SKILL.items():
    print('  * %s: %s' % (aid, why))
for (aid, f), why in IGNORE_FORM.items():
    print('  * %s%s: %s' % (aid, f, why))
for (aid, f), why in RANGE_SKIP.items():
    print('  * %s%s range: %s' % (aid, f, why))
