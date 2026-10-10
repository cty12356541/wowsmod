#!/usr/bin/env python3
"""拆解一个外观 mod 包(只读, 不改任何文件)。

用法:
  python scripts/ref/pack_teardown.py <mod包根目录(含 res_mods 或 PnFMods/<Mod>/<船>)> [--sdk <官方 SDK 船目录>] [--out teardown.md]

输出 markdown: 文件分类、与官方 SDK 船目录的增/改/缺、每个 .visual 的渲染集/LOD/材质标识、
.mfm 的着色器与关键参数、贴图分辨率/格式/mip、_mg 各通道统计(B=发光遮罩)、
.model 里的动画(.ssm/.seq)、Python 脚本、可疑的多余文件(备份/源文件)。
贴图像素统计需要 Pillow + numpy(可选)。原包只作参考: 本脚本不复制任何文件。
"""
import argparse, os, re, struct, sys
from collections import Counter, defaultdict

DX = {71: 'BC1', 72: 'BC1s', 74: 'BC2', 77: 'BC3', 78: 'BC3s', 80: 'BC4', 83: 'BC5', 95: 'BC6H', 98: 'BC7', 99: 'BC7s', 28: 'RGBA8', 87: 'BGRA8'}


def dds_info(p):
    d = open(p, 'rb').read(148)
    if d[:4] != b'DDS ':
        return None
    h, w = struct.unpack('<II', d[12:20])
    mips = struct.unpack('<I', d[28:32])[0]
    cc = d[84:88]
    fmt = cc.decode('latin1')
    if cc == b'DX10':
        fmt = DX.get(struct.unpack('<I', d[128:132])[0], 'dxgi%d' % struct.unpack('<I', d[128:132])[0])
    return w, h, fmt, max(mips, 1)


def mg_stats(p):
    try:
        from PIL import Image
        import numpy as np
        a = np.asarray(Image.open(p).convert('RGBA'))
    except Exception as e:  # noqa
        return 'n/a(%s)' % type(e).__name__
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    return 'R均%.0f G最大%d B最大%d B>0 %.1f%% B>32 %.1f%%' % (r.mean(), g.max(), b.max(), (b > 0).mean() * 100, (b > 32).mean() * 100)


def txt(p):
    return open(p, encoding='utf-8', errors='replace').read()


def visual_info(p):
    s = txt(p)
    rs = re.findall(r'<renderSet>\s*<name>\s*([^<]*?)\s*</name>.*?<nodes>(.*?)</nodes>.*?<identifier>\s*([^<]*?)\s*</identifier>\s*<mfm>\s*([^<]*?)\s*</mfm>', s, re.S)
    lods = [x.strip() for x in re.findall(r'<extent>([^<]*)</extent>', s)]
    return rs, lods


def find_ship_dirs(root):
    out = []
    for dp, dn, fn in os.walk(root):
        if any(f.endswith('.visual') for f in fn) and os.path.basename(dp) in ('ship', 'gun', 'director', 'radar', 'misc'):
            out.append(os.path.dirname(dp))
    return sorted(set(out))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('root')
    ap.add_argument('--sdk', help='官方 SDK 船目录(PnFMods/ModsSDK.zip/<船>), 用于对比增/改/缺')
    ap.add_argument('--out')
    a = ap.parse_args()
    L = []
    w = L.append
    ships = find_ship_dirs(a.root)
    w('# 拆解: %s\n' % os.path.abspath(a.root))
    allf = [os.path.join(dp, f) for dp, _, fn in os.walk(a.root) for f in fn]
    w('文件 %d 个, 共 %.1f MB; 扩展名: %s\n' % (len(allf), sum(os.path.getsize(f) for f in allf) / 1e6, dict(Counter(os.path.splitext(f)[1].lower() for f in allf))))
    py = [f for f in allf if f.endswith('.py')]
    w('## Python / 加载方式')
    for f in py:
        w('- `%s` (%d B): %s' % (os.path.relpath(f, a.root), os.path.getsize(f), ' '.join(txt(f).split())[:200]))
    junk = [f for f in allf if re.search(r'副本|新建文件夹|copy|\.(png|psd|pbm|tga|blend|ma|mb|fbx)$', f, re.I) and 'gui/' not in f.replace('\\', '/')]
    w('\n## 可疑多余文件(备份/源文件, 游戏不读)')
    for f in junk:
        w('- `%s` %.1f MB' % (os.path.relpath(f, a.root), os.path.getsize(f) / 1e6))
    for sd in ships:
        rel = lambda p: os.path.relpath(p, sd)
        files = {rel(os.path.join(dp, f)): os.path.join(dp, f) for dp, _, fn in os.walk(sd) for f in fn}
        w('\n## 船目录 `%s`' % os.path.relpath(sd, a.root))
        if a.sdk:
            sdkf = {os.path.relpath(os.path.join(dp, f), a.sdk) for dp, _, fn in os.walk(a.sdk) for f in fn}
            added = sorted(set(files) - sdkf)
            missing = sorted(sdkf - set(files))
            w('- 对比 SDK: 新增 %d, 同名 %d, SDK 有而包里没有 %d' % (len(added), len(set(files) & sdkf), len(missing)))
            w('- 新增(非贴图): ' + ', '.join('`%s`' % x for x in added if not x.endswith('.dds'))[:4000])
            w('- 缺(SDK 有): ' + str(dict(Counter(os.path.dirname(x) or '.' for x in missing))))
        w('\n### .visual 渲染集 / LOD / 材质标识')
        for k in sorted(f for f in files if f.endswith('.visual')):
            rs, lods = visual_info(files[k])
            ids = sorted({r[2] for r in rs})
            w('- `%s`: 渲染集 %d, LOD extent %s, 标识 %s' % (k, len(rs), lods, ids))
            for r in rs:
                if not r[3].split('/')[-2:-1] or not re.search(r'(ship|gun|director|radar)/', r[3]):
                    w('  - 额外渲染集 `%s` 节点 %s 材质 `%s`' % (r[0], ' '.join(re.sub('<[^>]+>', ' ', r[1]).split()), r[3]))
        w('\n### .model 动画 / skeletonExtender')
        for k in sorted(f for f in files if f.endswith('.model')):
            s = txt(files[k])
            an = re.findall(r'<animation>\s*([^<]*?)\s*</animation>', s)
            if an:
                w('- `%s` 动画: %s' % (k, an))
        for k in sorted(f for f in files if f.endswith('.seq')):
            s = txt(files[k])
            objs = re.findall(r'<resource>\s*([^<]+?)\s*</resource>', s)
            pos = re.findall(r'<name>\s*position\s*</name>.*?<value>\s*([^<]*?)\s*</value>', s, re.S)
            rot = re.findall(r'<outTangent>\s*([^<]*?)\s*</outTangent>', s)
            w('- `%s`: 对象 %s, position %s, 非零 outTangent(角速度) %s' % (k, sorted(set(objs)), pos, [r for r in rot if set(r.replace('0', '').replace('.', '').split()) - {''}]))
        w('\n### .mfm 着色器')
        byfx = defaultdict(list)
        for k in sorted(f for f in files if f.endswith('.mfm')):
            s = txt(files[k])
            fx = re.search(r'<fx>\s*([^<]*?)\s*</fx>', s)
            ep = re.search(r'emissivePower\s*<Float>\s*([\d.]+)', s)
            flags = [n for n in ('doubleSided', 'alphaTestEnable') if n in s]
            byfx[fx.group(1) if fx else '?'].append('%s%s%s' % (os.path.basename(k), ' emissivePower=' + ep.group(1) if ep else '', ' ' + '/'.join(flags) if flags else ''))
        for fx, v in byfx.items():
            w('- `%s` × %d: %s' % (fx, len(v), ', '.join(v)))
        w('\n### 贴图')
        w('| 文件 | 尺寸 | 格式 | mip | _mg 通道 |')
        w('|---|---|---|---|---|')
        for k in sorted(f for f in files if f.lower().endswith('.dds')):
            i = dds_info(files[k])
            if not i:
                continue
            st = mg_stats(files[k]) if k.endswith('_mg.dds') and i[0] >= 512 else ''
            w('| `%s` | %dx%d | %s | %d | %s |' % (k, i[0], i[1], i[2], i[3], st))
    out = '\n'.join(L) + '\n'
    if a.out:
        open(a.out, 'w', encoding='utf-8').write(out)
        print('写入', a.out)
    else:
        sys.stdout.write(out)


if __name__ == '__main__':
    main()
