"""原壳小块按「打碎色块/发光主线」排序: clutter_rank.py <pos.npz> <idx.png> <glow.png> <prune_dryrun.json>... > rank.json
对 dry-run 列出的每个 LOD0 小块: 取其包围盒(中心±diag/2+0.01)内的贴图像素(pos 烘焙图), 统计色号种类数 n_idx 与是否含发光 glow;
score = 2*glow + (n_idx-1)。glow=1 → 压在发光线上(优先删); n_idx≥2 → 跨色块边界(打碎色块)。"""
import sys, json, numpy as np
from PIL import Image
P = np.load(sys.argv[1]); pos = P['pos']; cov = P['cov'] > 0
idx = np.array(Image.open(sys.argv[2])); glow = np.array(Image.open(sys.argv[3]).convert('L')) > 127
if idx.ndim == 3: idx = idx[..., 0]
sz = pos.shape[0]
def rs(a): return a if a.shape[0] == sz else np.array(Image.fromarray(a.astype(np.uint8)).resize((sz, sz), Image.NEAREST))
idx = rs(idx); glow = rs(glow.astype(np.uint8)) > 0
m = cov.ravel(); X = pos.reshape(-1, 3)[m]; I = idx.ravel()[m]; Gl = glow.ravel()[m]
order = np.argsort(X[:, 2]); Xs = X[order]; Is = I[order]; Gs = Gl[order]
out = []
for f in sys.argv[4:]:
    d = json.load(open(f))
    for u in d['units']:
        for r in u['renderSets']:
            if r['lodVisual'] != 0: continue
            for c in r['samples']:
                ctr = np.array(c['center']); h = c['diag'] / 2 + 0.01
                lo, hi = np.searchsorted(Xs[:, 2], ctr[2] - h), np.searchsorted(Xs[:, 2], ctr[2] + h)
                sel = np.all(np.abs(Xs[lo:hi] - ctr) <= h, axis=1)
                ii = Is[lo:hi][sel]; gg = Gs[lo:hi][sel]
                n_idx = len(np.unique(ii)); g = bool(gg.any())
                out.append(dict(file=f, set=r['renderSet'], center=c['center'], diag=c['diag'], tris=c['tris'], texels=int(sel.sum()), n_idx=n_idx, glow=g, score=2 * g + max(n_idx - 1, 0)))
out.sort(key=lambda o: -o['score'])
json.dump(out, open('/dev/stdout', 'w'), indent=0)
