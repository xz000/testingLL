import io, json, re

p = r'game-core\src\skill.rs'
s = io.open(p, encoding='utf-8').read()

# 1) 去掉上次生成（有整数 literal 问题）的测试块
marks = [x for x in [s.find('    /// w3a 逐级冷却交叉校验'), s.find('    /// w3a 逐级持续时间交叉校验')] if x > 0]
if marks:
    start = min(marks)
    last = max(x for x in [s.find('fn w3a_cooldown_crosscheck', start), s.find('fn w3a_duration_crosscheck', start)] if x > 0)
    end = s.find('\n    }\n', last)
    assert end > 0
    s = s[:start] + s[end + len('\n    }\n') + 1:]
assert 'w3a_cooldown_crosscheck' not in s and 'w3a_duration_crosscheck' not in s

t = json.load(io.open(r'C:\Users\xvzan\Documents\testingLL\098c\out\w3a_table.json', encoding='utf-8'))['table']
ours = {}
for ln in io.open('_ours.tsv', encoding='utf-8').read().split('\n')[1:]:
    if ln.strip():
        q = ln.split('\t')
        ours[(q[0], q[1], int(q[2]))] = q

SKIP = {'S007B'}  # S007B 在 098c 里不可达（C 树无形态切换菜单），故不实装


def num(v):
    r = '%g' % float(v)
    if '.' not in r and 'e' not in r and 'E' not in r:
        r += '.0'
    return r


cd_map = {}
for aid in sorted(t):
    if not aid.startswith('S') or len(aid) != 4:
        continue
    per_form = {}
    for lv_s, cd in sorted(t[aid]['acdn'].items(), key=lambda kv: int(kv[0])):
        lv = int(lv_s)
        f = 'A' if lv <= 10 else 'B'
        ol = lv if lv <= 10 else lv - 10
        if aid + f in SKIP:
            continue
        per_form.setdefault(f, []).append((ol, cd))
    for f, lst in per_form.items():
        # 仅当该形态**每一级**都一致时写入（否则索引会错位）
        ok = True
        for ol, cd in lst:
            o = ours.get((aid, f, ol))
            if not o or abs(float(o[3]) - cd) > 0.06:
                ok = False
                break
        if ok and len(lst) >= 2:
            cd_map[(aid, f)] = lst

out = []
out.append('    /// w3a 逐级冷却交叉校验（真值源：098c `war3map.w3a` 的 `acdn` 与 tooltip「Cooldown」）。')
out.append('    ///')
out.append('    /// 由脚本从 w3a 导出，只写入**双方已一致**的项；S007B/S011B（我方未实装 B 形态）除外。')
out.append('    #[test]')
out.append('    fn w3a_cooldown_crosscheck() {')
out.append('        let table: &[(SkillId, bool, &[f64])] = &[')
for (aid, f), lst in cd_map.items():
    if len(lst) < 2:
        continue
    vals = ', '.join(num(v) for _, v in sorted(lst))
    out.append('            (SkillId::%s, %s, &[%s]),' % (aid, 'true' if f == 'B' else 'false', vals))
out.append('        ];')
out.append('        for (id, alt, cds) in table {')
out.append('            let d = DefTable::def_for(*id, *alt);')
out.append('            for (i, want) in cds.iter().enumerate() {')
out.append('                let got = d.stats_at(i as u32 + 1).cooldown.to_num::<f64>();')
out.append('                assert!(')
out.append('                    (got - want).abs() < 0.06,')
out.append('                    "{id:?} alt={alt} L{}: got {got}, want {want} (098c w3a acdn)",')
out.append('                    i + 1')
out.append('                );')
out.append('            }')
out.append('        }')
out.append('    }')
out.append('')
du_map = {}
for aid in sorted(t):
    if not aid.startswith('S') or len(aid) != 4:
        continue
    per_form = {}
    for lv_s, tip in sorted(t[aid]['tip'].items(), key=lambda kv: int(kv[0])):
        if 'Duration' not in tip:
            continue
        lv = int(lv_s)
        f = 'A' if lv <= 10 else 'B'
        ol = lv if lv <= 10 else lv - 10
        per_form.setdefault(f, []).append((ol, tip['Duration']))
    for f, lst in per_form.items():
        ols = sorted(ol for ol, _ in lst)
        contig = ols == list(range(1, len(ols) + 1))
        ok = contig
        for ol, d in lst:
            o = ours.get((aid, f, ol))
            if not o or abs(float(o[7]) - d) > 0.06:
                ok = False
                break
        if ok and len(lst) >= 2:
            du_map[(aid, f)] = lst

out.append('    /// w3a 逐级持续时间交叉校验（真值源：tooltip「Duration」）。')
out.append('    #[test]')
out.append('    fn w3a_duration_crosscheck() {')
out.append('        let table: &[(SkillId, bool, &[f64])] = &[')
for (aid, f), lst in du_map.items():
    vals = ', '.join(num(v) for _, v in sorted(lst))
    out.append('            (SkillId::%s, %s, &[%s]),' % (aid, 'true' if f == 'B' else 'false', vals))
out.append('        ];')
out.append('        for (id, alt, ds) in table {')
out.append('            let d = DefTable::def_for(*id, *alt);')
out.append('            for (i, want) in ds.iter().enumerate() {')
out.append('                let got = d.stats_at(i as u32 + 1).duration.to_num::<f64>();')
out.append('                assert!(')
out.append('                    (got - want).abs() < 0.06,')
out.append('                    "{id:?} alt={alt} L{}: got {got}, want {want} (098c w3a duration)",')
out.append('                    i + 1')
out.append('                );')
out.append('            }')
out.append('        }')
out.append('    }')
out.append('')

anchor = 'mod tests {\n    use super::*;\n'
assert s.count(anchor) == 1
s = s.replace(anchor, anchor + '\n' + '\n'.join(out) + '\n')
io.open(p, 'w', encoding='utf-8', newline='').write(s)
print('skills', len(cd_map))
