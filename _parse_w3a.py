import re
txt = open('098c/out/w3a_strings.txt', encoding='utf-8', errors='replace').read()
txt = re.sub(r'\|c[0-9a-fA-F]{6}', '', txt)
txt = txt.replace('|r', '').replace('|n', '\n')
records = txt.split('=====')

def nums(s):
    out = {}
    for key in ('Damage', 'Cooldown', 'Duration', 'Range', 'Max AoE damage', 'DPS factor', 'Damage over time'):
        m = re.search(key + r':\s*([0-9][0-9.\-]*)', s)
        if m:
            out[key] = m.group(1)
    m = re.search(r'\bcd:\s*([0-9.]+)', s)
    if m:
        out['cd'] = m.group(1)
    return out

interesting = []
for r in records:
    r = r.strip()
    if not r:
        continue
    lines = [l for l in r.split('\n') if l.strip()]
    if not lines:
        continue
    desc = lines[0].strip()
    n = nums(r)
    if n:
        interesting.append((desc, n))

for desc, n in interesting:
    print(f"{desc[:72]:72} | {n}")
