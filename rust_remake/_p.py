import io, re
s = io.open('../098c/out/war3map.j', encoding='utf-8', errors='replace').read()
for name in ['rC', 'eC', 'oC', 'xC', 'yc', 'wc', 'CC', 'cC', 'BC']:
    m = re.search(r'\nfunction ' + name + r' takes([^\n]*)\n', s)
    if not m:
        print('### NOTFOUND', name); continue
    j = s.find('\nendfunction', m.start())
    print('### ' + name + ' takes' + m.group(1))
    print(s[m.start()+1:j+12])
    print()
