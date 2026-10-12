#!/usr/bin/env python3
"""从立绘/参考图提取主色及面积比例(设计语言卡用)。

python scripts/ref/palette.py <图>... [--k 8] [--bg auto|none|#RRGGBB] [--box x,y,w,h] [--out swatch.png] [--json]
- 透明像素忽略; --bg auto: 去掉与四角相近的背景色(立绘常为纯色/白底)。
- 输出: 每个色的 #hex、占比、明度(L*)、饱和度; 多张图合并统计。--out 生成色卡图。
只依赖 Pillow + numpy。
"""
import argparse, json, sys
import numpy as np
from PIL import Image, ImageDraw

def load(p, box, bg):
    im = Image.open(p).convert('RGBA')
    if box:
        x, y, w, h = map(int, box.split(','))
        im = im.crop((x, y, x + w, y + h))
    im.thumbnail((512, 512))
    a = np.asarray(im).reshape(-1, 4).astype(np.float32)
    keep = a[:, 3] > 128
    rgb = a[keep, :3]
    if bg == 'auto':
        c = np.asarray(im)[[0, 0, -1, -1], [0, -1, 0, -1], :3].astype(np.float32)
        ref = np.median(c, axis=0)
        if np.ptp(c, axis=0).max() < 30:
            rgb = rgb[np.linalg.norm(rgb - ref, axis=1) > 30]
    elif bg and bg.startswith('#'):
        ref = np.array([int(bg[i:i + 2], 16) for i in (1, 3, 5)], np.float32)
        rgb = rgb[np.linalg.norm(rgb - ref, axis=1) > 30]
    return rgb

def kmeans(x, k, it=25, seed=0):
    rng = np.random.default_rng(seed)
    c = x[rng.choice(len(x), k, replace=False)]
    for _ in range(it):
        d = ((x[:, None, :] - c[None]) ** 2).sum(-1)
        l = d.argmin(1)
        for j in range(k):
            if (l == j).any():
                c[j] = x[l == j].mean(0)
    return c, l

def lum(c):
    r, g, b = [v / 255 for v in c]
    f = lambda u: u / 12.92 if u <= 0.04045 else ((u + 0.055) / 1.055) ** 2.4
    y = 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)
    return 116 * (y ** (1 / 3) if y > 0.008856 else 7.787 * y + 16 / 116) - 16

def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('images', nargs='+'); ap.add_argument('--k', type=int, default=8)
    ap.add_argument('--bg', default='auto'); ap.add_argument('--box'); ap.add_argument('--out'); ap.add_argument('--json', action='store_true')
    a = ap.parse_args()
    x = np.concatenate([load(p, a.box, a.bg) for p in a.images])
    if len(x) > 200000:
        x = x[np.random.default_rng(1).choice(len(x), 200000, replace=False)]
    c, l = kmeans(x, min(a.k, len(x)))
    n = np.bincount(l, minlength=len(c))
    order = np.argsort(-n)
    rows = []
    for j in order:
        if n[j] == 0:
            continue
        col = [int(round(v)) for v in c[j]]
        mx, mn = max(col), min(col)
        rows.append({'hex': '#%02X%02X%02X' % tuple(col), 'pct': round(100 * n[j] / n.sum(), 1), 'L': round(lum(col), 1), 'sat': round((mx - mn) / mx, 2) if mx else 0})
    if a.out:
        W = 120; im = Image.new('RGB', (W * len(rows), 150), 'white'); d = ImageDraw.Draw(im)
        for i, r in enumerate(rows):
            d.rectangle([i * W, 0, i * W + W - 1, 110], fill=r['hex'])
            d.text((i * W + 6, 116), f"{r['hex']}\n{r['pct']}%  L{r['L']:.0f}", fill='black')
        im.save(a.out)
    if a.json:
        print(json.dumps({'images': a.images, 'colors': rows, 'swatch': a.out}, ensure_ascii=False))
    else:
        for r in rows:
            print(f"{r['hex']}  {r['pct']:5.1f}%  L*{r['L']:5.1f}  sat {r['sat']:.2f}")

if __name__ == '__main__':
    main()
