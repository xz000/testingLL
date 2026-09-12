import json, io, sys
sys.stdout.reconfigure(errors='replace')

w3a = json.load(io.open(r'C:\Users\xvzan\Documents\testingLL\098c\out\w3a_table.json', encoding='utf-8'))['table']

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

# w3a 等级 → 我方等级：A: 1..9 -> 1..9；B: 11..19 -> 1..9
def map_level(lvl):
    if lvl <= 10:
        return 'A', lvl
    return 'B', lvl - 10

TOL = 0.06
report = []
for aid in sorted(w3a):
    t = w3a[aid]
    if not t['tip'] and not t['acdn']:
        continue
    lvls = sorted(set([int(k) for k in t['tip']]) | set([int(k) for k in t['acdn']]))
    for lvl in lvls:
        form, olv = map_level(lvl)
        o = ours.get((aid, form, olv))
        tip = t['tip'].get(str(lvl), {})
        acdn = t['acdn'].get(str(lvl))
        if o is None:
            report.append((aid, form, olv, 'MISSING_OURS', '', ''))
            continue
        if acdn is not None and abs(acdn - o['cooldown']) > TOL:
            report.append((aid, form, olv, 'cooldown', acdn, round(o['cooldown'], 2)))
        if 'Damage' in tip and abs(tip['Damage'] - o['damage']) > TOL:
            report.append((aid, form, olv, 'damage', tip['Damage'], round(o['damage'], 2)))
        if 'Duration' in tip and abs(tip['Duration'] - o['duration']) > TOL:
            report.append((aid, form, olv, 'duration', tip['Duration'], round(o['duration'], 2)))
        if 'Range' in tip:
            rng = tip['Range']
            near = min(abs(rng - o['range']), abs(rng - o['max_distance']))
            if near > TOL:
                report.append((aid, form, olv, 'range', rng, f"range={o['range']:.0f} md={o['max_distance']:.0f}"))

print('=== 差异（我方 vs 098c w3a tooltip/acdn）===')
cur = None
for r in report:
    if r[0] != cur:
        cur = r[0]
        print('---', cur)
    print('  %s L%-2d %-12s 098c=%-8s ours=%s' % (r[1], r[2], r[3], r[4], r[5]))
print('total diffs', len(report))
io.open('_cmp_report.txt', 'w', encoding='utf-8').write('\n'.join('%s\t%s\t%s\t%s\t%s\t%s' % r for r in report))
