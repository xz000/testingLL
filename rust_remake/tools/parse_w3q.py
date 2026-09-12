#!/usr/bin/env python3
"""w3q（UpgradeData）解析器 —— 098c 实证布局。

**头部（8 字节，无魔数）**::

    @0 u32 version            # 本文件 = 2
    @4 u32 unknown            # 本文件 = 2

**条目（顺序排到文件尾，无计数循环）**::

    char[4] old_id            # 非零 → 修改原表对象；全 0 → 新增自定义对象
    char[4] new_id            # 自定义 id（原表修改条目这里是 0）
    u32     n_mods
    per modification:
        char[4] field_id      # 如 gnam / gglb / glvl / glmb
        u32     type          # 0=int 1=real 2=unreal 3=string
        u32     level         # 逐级：1,2,3,…
        u32     data_ptr      # **非 0 = 值继承原表、不内联**（不是 padding！）
        <value>               # type 0..2: u32/f32；type 3 且 ptr==0: cstring
        u32     end           # **可选**：为 0 时视为结束标记吃掉（字符串值后面通常没有）

用法::

    python tools/parse_w3q.py [路径] [--json 输出路径]
"""
import io
import json
import struct
import sys

IDCHARS = b'0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ'


def _looks_like_id(b):
    return len(b) == 4 and all(c in IDCHARS or c == 0 for c in b)


def parse(data: bytes):
    u32 = lambda o: struct.unpack_from('<I', data, o)[0]
    version = u32(0)
    unknown = u32(4)
    pos = 8
    objects = []
    while pos + 12 <= len(data):
        old_id = data[pos:pos + 4]
        new_id = data[pos + 4:pos + 8]
        n_mods = u32(pos + 8)
        if not _looks_like_id(old_id) or n_mods > 4096:
            break
        pos += 12
        fields = []
        for _ in range(n_mods):
            fid = data[pos:pos + 4].decode('latin-1')
            typ = u32(pos + 4)
            level = u32(pos + 8)
            ptr = u32(pos + 12)
            pos += 16
            value = None
            if typ == 0:
                value = u32(pos)
                pos += 4
            elif typ in (1, 2):
                value = struct.unpack_from('<f', data, pos)[0]
                pos += 4
            elif typ == 3:
                if ptr == 0:
                    end = data.find(b'\x00', pos)
                    value = data[pos:end].decode('latin-1')
                    pos = end + 1
                # ptr != 0 → 继承原表
            # 结束标记可选：仅当"4 字节 0 + 后面像字段 id"时才吃掉。
            # （实证：字符串值之后直接就是下一个条目的 old_id，没有 4 字节 0。）
            if pos + 8 <= len(data) and u32(pos) == 0 and _looks_like_id(data[pos + 4:pos + 8]):
                pos += 4
            fields.append({'field': fid, 'type': typ, 'level': level, 'ptr': ptr, 'value': value})
        objects.append({'old': old_id.decode('latin-1'), 'new': new_id.decode('latin-1'), 'fields': fields})
    return {'version': version, 'unknown': unknown, 'n': len(objects),
            'objects': objects, 'consumed': pos, 'size': len(data)}


def main():
    src = sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith('--') \
        else r'c:\users\xvzan\documents\testingLL\098c\out\war3map.w3q'
    dst = None
    if '--json' in sys.argv:
        dst = sys.argv[sys.argv.index('--json') + 1]
    data = io.open(src, 'rb').read()
    r = parse(data)
    print('version=%s n=%s consumed=%d/%d' % (r['version'], r['n'], r['consumed'], r['size']))
    keys = ('gglb', 'glvl', 'glmb', 'gnam')
    shown = 0
    for o in r['objects']:
        want = [f for f in o['fields'] if f['field'] in keys]
        if not want:
            continue
        per = {}
        for f in want:
            per.setdefault(f['field'], []).append(f['value'] if f['ptr'] == 0 else 'INHERIT')
        print('%-5s(old=%-5s) %s' % (o['new'].replace('\x00', ''), o['old'].replace('\x00', ''), per))
        shown += 1
        if shown >= 12:
            break
    if dst:
        io.open(dst, 'w', encoding='utf-8').write(json.dumps(r, ensure_ascii=False, indent=1))
        print('wrote', dst)


if __name__ == '__main__':
    main()
