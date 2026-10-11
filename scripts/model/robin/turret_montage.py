"""炮塔近景 改前/改后/标杆 并排: python3 turret_montage.py <before_dir> <after_dir> <out.jpg> [bench1.jpg ...]"""
import sys
from PIL import Image, ImageDraw
B, A, OUT = sys.argv[1:4]; BEN = sys.argv[4:]
rows = ['fgm1', 'fgs053', 'fgs036', 'fgm3']; W, H = 420, 294
cols = [(B, 'q', 'before q'), (B, 'top', 'before top'), (A, 'q', 'after q'), (A, 'top', 'after top'), (A, 'side', 'after side')]
nc = len(cols) + (1 if BEN else 0); o = Image.new('RGB', (W * nc, H * len(rows) + 24), 'white')
dr = ImageDraw.Draw(o)
for c, (_, _, lab) in enumerate(cols): dr.text((c * W + 6, 6), lab, fill=(0, 0, 0))
if BEN: dr.text((len(cols) * W + 6, 6), 'benchmark', fill=(0, 0, 0))
for r, n in enumerate(rows):
    for c, (d, v, _) in enumerate(cols):
        im = Image.open(f'{d}/{n}_{v}.png').convert('RGB').resize((W, H)); ImageDraw.Draw(im).text((6, 6), n, fill=(255, 255, 0)); o.paste(im, (c * W, 24 + r * H))
    if BEN and r < len(BEN):
        im = Image.open(BEN[r]).convert('RGB'); im.thumbnail((W, H)); o.paste(im, (len(cols) * W, 24 + r * H))
o.save(OUT, quality=88)
