import io, re, sys
sys.stdout.reconfigure(errors='replace')
s = io.open(r'c:\users\xvzan\documents\testingLL\098c\out\war3map_pretty.j', encoding='utf-8', errors='replace').read().split('\n')
def show(a, b):
    for i in range(a - 1, min(b, len(s))):
        print(i + 1, s[i].rstrip()[:190].encode('ascii', 'replace').decode())
    print('=' * 55)
show(21335, 21380)
show(25850, 25885)
print('--- kn[7* refs ---')
for i, l in enumerate(s):
    if 'kn[7*' in l.replace(' ', ''):
        print(i + 1, l.strip()[:150].encode('ascii', 'replace').decode())
