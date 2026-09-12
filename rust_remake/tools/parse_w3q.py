#!/usr/bin/env python3
"""w3q（UpgradeData）解析器 —— 与 w3a 同族的对象数据格式。

布局（`098c/out/war3map.w3q` 实证）::

    header: u32 version? + u32 n_orig + u32 n_custom        （无魔数头）
    per custom object:
        char[4] old_id
        char[4] new_id
        u32     n_mods
        per modification:
            char[4] field_id
            u32     type        # 0=int 1=real 2=unreal 3=string
            u32     level
            u32     data_ptr    # **非 0 = 值继承原表，不内联**（旧脚本把它当 pad 忽略 → 读不到值）
            <value>             # type 0..2: u32/f32；type 3: cstring（仅当 data_ptr == 0）
            u32     end         # 0x00000000 结束标记

用法::

    python tools/parse_w3q.py [路径] [--json 输出路径]
"""
import io
import json
import struct
import sys


def parse(data: bytes):
    u32 = lambda o: struct.unpack_from('<I', data, o)[0]
    # 098c 这份 w3q **没有魔数头**，而且头部只有 8 字节：
    #   @0 u32 version(=2)  @4 u32 unknown(=2)  @8 起就是第一个条目（old_id）
    # 旧脚本按标准 12 字节头 + 计数循环解，一开头就错位。
    assert data[:4] != b'W3Q!', 'unexpected magic'
    version = u32(0)
    n_orig = u32(4)
    pos = 8

    objects = []
    while pos + 12 <= len(data):
        old_id = data[pos:pos + 4]
        new_id = data[pos + 4:pos + 8]
        if not all(c in b'0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ\x00' for c in old_id):
            break
        n_mods = u32(pos + 8)
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
                # ptr != 0 → 值继承原表，不内联
            end = u32(pos)
            if end != 0:
                raise ValueError('bad end marker %d at %d (field %s)' % (end, pos, fid))
            pos += 4
            fields.append({'field': fid, 'type': typ, 'level': level, 'ptr': ptr, 'value': value})
        objects.append({
            'old': old_id.decode('latin-1'),
            'new': new_id.decode('latin-1'),
            'fields': fields,
        })
    consumed = pos
    return {'version': version, 'n_orig': n_orig, 'n_custom': len(objects),
            'objects': objects, 'consumed': consumed, 'size': len(data)}


def main():
    src = sys.argv[1] if len(sys.argv) > 1 else r'c:\users\xvzan\documents\testingLL\098c\out\war3map.w3q'
    dst = None
    if '--json' in sys.argv:
        dst = sys.argv[sys.argv.index('--json') + 1]
    data = io.open(src, 'rb').read()
    r = parse(data)
    print('version=%s n_orig=%s n_custom=%s consumed=%d/%d' % (
        r['version'], r['n_orig'], r['n_custom'], r['consumed'], r['size']))
    # 关注成本字段
    keys = ('gglb', 'glvl', 'glmb', 'gnam')
    for o in r['objects']:
        want = [f for f in o['fields'] if f['field'] in keys]
        if not want:
            continue
        per = {}
        for f in want:
            per.setdefault(f['field'], []).append(f['value'])
        print('%-5s(old=%-5s) %s' % (o['new'].strip('\x00'), o['old'], per))
    if dst:
        io.open(dst, 'w', encoding='utf-8').write(json.dumps(r, ensure_ascii=False, indent=1))
        print('wrote', dst)


if __name__ == '__main__':
    main()
