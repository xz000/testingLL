import io, re

src = io.open('../098c/out/war3map_pretty.j', encoding='utf-8', errors='replace').read()
lines = src.split('\n')

def fn_line(name):
    m = re.search(r'^function ' + re.escape(name) + r' takes', src, re.M)
    return (src[:m.start()].count('\n') + 1) if m else -1

seen = {}
for i, l in enumerate(lines):
    m = re.search(r"if\s+SC\s*==\s*'(S\d{3})'\s*then", l)
    if not m:
        continue
    skill = m.group(1)
    if skill in seen:
        continue
    # then 分支：本行之后首个 call Fn()
    then_fn = None
    else_fn = None
    for j in range(i + 1, min(i + 14, len(lines))):
        if lines[j].strip() == 'else':
            c = None
            for k in range(j + 1, min(j + 4, len(lines))):
                c = re.search(r'call\s+([A-Za-z][A-Za-z0-9_]*)\s*\(\s*\)', lines[k])
                if c:
                    else_fn = c.group(1)
                    break
            break
        if then_fn is None:
            c = re.search(r'call\s+([A-Za-z][A-Za-z0-9_]*)\s*\(\s*\)', lines[j])
            if c:
                then_fn = c.group(1)
    seen[skill] = (then_fn, else_fn, i + 1)

print('=== 技能 → 施法函数（二分 if 链：if SC==Sx then A() else B()，B 属于链中的邻居技能）===')
for s in sorted(seen, key=lambda x: int(x[1:])):
    t, e, ln = seen[s]
    print(f'{s}: then={t}(L{fn_line(t) if t else "-"})  else={e}(L{fn_line(e) if e else "-"})   [dispatch L{ln}]')

print()
print('=== 已知的奇数技能反向映射（from else branches）===')
inv = {}
for s, (t, e, ln) in seen.items():
    if e:
        inv.setdefault(e, []).append(s)
for fn, owners in sorted(inv.items()):
    print(f'{fn}  <- else of {owners}')

known = {'Ab': 'S000', 'jb': 'S002', 'Ub': 'S004', 'GC': 'S006', 'OB': 'S008', 'IB': 'S010',
         'WB': 'S012', 'ac': 'S014', 'Dc': 'S016', 'Mc': 'S018', 'qC': 'S020'}
print()
print('=== 函数行号（用于逐技能读代码）===')
for fn, s in sorted(known.items(), key=lambda kv: int(kv[1][1:])):
    print(f'{s}: {fn}()  line {fn_line(fn)}')
