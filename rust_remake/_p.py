import io, re
s = io.open('../098c/out/war3map.j', encoding='utf-8', errors='replace').read()
for name in ['pC']:
    m = re.search(r'\nfunction ' + name + r' takes([^\n]*)\n', s)
    j = s.find('\nendfunction', m.start())
    print('### ' + name)
    print(s[m.start()+1:j+12])
