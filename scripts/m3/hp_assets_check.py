# assets.bin 交叉核对挂点矩阵: 在 assets.bin(游戏实际加载的原型库)里按 float32 搜索 SDK *_ports.visual 中每个 HP 节点的局部矩阵
import sys, glob, os, re, json, numpy as np, xml.etree.ElementTree as ET
ab = np.fromfile(sys.argv[1], dtype=np.uint8)
sdk = sys.argv[2]
out = sys.argv[3]
F = [ab[k:len(ab) - ((len(ab) - k) % 4)].view('<f4') for k in range(4)]
def nodes(path):
    root = ET.fromstring(open(path, encoding='utf-8', errors='replace').read())
    res = []
    def walk(n, parent, pname):
        t = n.find('transform')
        L = np.array([[float(x) for x in t.find('row%d' % i).text.split()] for i in range(4)]) if t is not None else np.eye(4)
        name = n.find('identifier').text.strip()
        res.append((name, L, pname))
        for c in n.findall('node'):
            walk(c, L @ parent, name)
    for n in root.findall('node'):
        walk(n, np.eye(4), None)
    return res
PRE = []
for f in F:
    i = np.nonzero(f == np.float32(1.0))[0] - 3
    i = i[(i >= 12)]
    PRE.append((i, f[i], f[i + 1], f[i + 2]))
print('candidates', [len(p[0]) for p in PRE], flush=True)
rows = []
for vp in sorted(glob.glob(os.path.join(sdk, 'ship', '*_ports.visual'))):
    for name, L, pname in nodes(vp):
        if not name.startswith('HP_'):
            continue
        tgt = L.astype(np.float32).flatten()
        found = []
        for al in range(4):
            f = F[al]; idx, X, Y, Z = PRE[al]
            sel = idx[(np.abs(X - tgt[12]) < 6e-5) & (np.abs(Y - tgt[13]) < 6e-5) & (np.abs(Z - tgt[14]) < 6e-5)]
            for c in sel:
                m = f[c - 12:c + 4]
                found.append((int(c * 4 + al - 48), float(np.abs(m - tgt).max())))
        best = min(found, key=lambda t: t[1]) if found else None
        rows.append(dict(visual=os.path.basename(vp), hp=name, parent=pname, hits=len(found), best_offset=best[0] if best else None, best_maxdiff=best[1] if best else None))
        print(name, len(found), best)
ok = [r for r in rows if r['best_maxdiff'] is not None and r['best_maxdiff'] < 1e-4]
summary = dict(hardpoints=len(rows), matched_full_matrix_1e4=len(ok), not_found=[r['hp'] for r in rows if r['hits'] == 0], partial=[(r['hp'], r['best_maxdiff']) for r in rows if r['hits'] and r['best_maxdiff'] >= 1e-4])
json.dump(dict(summary=summary, rows=rows), open(out, 'w'), indent=1, ensure_ascii=False)
print(json.dumps(summary, ensure_ascii=False))
