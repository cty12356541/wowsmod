"""r42 分区 ΔE2000: 港口实拍 vs 预览(机位近似对齐, 不要求像素对齐)。
船体按颜色类(白/紫/藏青/青/金)过滤取色并比较占比(日景"洗白"程度), 翅膀取框内最亮 40%, 光环取最暗 40% 像素。
用法: python3 r42_regional.py regions_r42.json <port_dir> <prev_dir> [--json]"""
import json, sys
import numpy as np
from cmetrics import load, region, lab, de2000, hsv

def stats(img, box, cls):
    x0, y0, x1, y1 = box; n = (x1 - x0) * (y1 - y0)
    rgb, cnt = region(img, box, cls)
    return rgb, cnt / n

def bright(img, box, frac=0.4):
    """frac>0: 框内最亮 frac 像素均值; frac<0: 最暗 |frac|(光环是细环, 暗端=环体, 亮端=天空)"""
    x0, y0, x1, y1 = box; p = img[y0:y1, x0:x1].reshape(-1, 3)
    L = p @ np.array([0.2126, 0.7152, 0.0722])
    if frac > 0: return p[L >= np.percentile(L, 100 * (1 - frac))].mean(0)
    return p[L <= np.percentile(L, -100 * frac)].mean(0)

def main():
    spec = json.load(open(sys.argv[1])); pd, vd = sys.argv[2], sys.argv[3]
    rows = []; tot = []
    for name, s in spec['shots'].items():
        P = load(f'{pd}/{name}.png', (1280, 720)); V = load(f'{vd}/{name}.png', (1280, 720))
        for c, f in spec['classes'].items():
            a, fa = stats(P, s['port']['hull'], f); b, fb = stats(V, s['prev']['hull'], f)
            d = float(de2000(lab(a), lab(b))) if a is not None and b is not None else None
            rows.append((name, c, d, fa, fb, a, b))
            if d is not None: tot.append(d)
        for k in ('wing', 'halo'):
            fr = 0.4 if k == "wing" else -0.4; a = bright(P, s["port"][k], fr); b = bright(V, s["prev"][k], fr); d = float(de2000(lab(a), lab(b)))
            rows.append((name, k, d, None, None, a, b)); tot.append(d)
    if '--json' in sys.argv:
        print(json.dumps({'mean': float(np.mean(tot)), 'rows': [(r[0], r[1], r[2]) for r in rows]})); return
    print(f"{'shot':11s} {'region':7s} {'ΔE00':>6s} {'占比port':>8s} {'占比prev':>8s}  port_rgb -> prev_rgb")
    for n, c, d, fa, fb, a, b in rows:
        fs = lambda v: '' if v is None else f'{100*v:7.2f}%'
        rs = lambda v: '-' if v is None else ','.join(str(int(x)) for x in v)
        print(f"{n:11s} {c:7s} {('%6.1f' % d) if d is not None else '   n/a'} {fs(fa):>8s} {fs(fb):>8s}  {rs(a)} -> {rs(b)}")
    print(f'mean ΔE00 = {np.mean(tot):.2f} (n={len(tot)})')

if __name__ == '__main__': main()
