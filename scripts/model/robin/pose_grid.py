"""拼 turret_pose_render.py 输出: python3 pose_grid.py <dir> <out.jpg>"""
import os, sys
from PIL import Image, ImageDraw
d, out = sys.argv[1], sys.argv[2]; fs = sorted(os.listdir(d)); W, H = 500, 350
rows = ['fgm1', 'fgs053', 'fgs036', 'fgm3']; o = Image.new('RGB', (W * 4, H * 4), 'white')
for r, n in enumerate(rows):
    cs = [f for f in fs if f.startswith(n + '_')]; cs.sort(key=lambda f: ('rest' not in f, '_q' not in f, 'side' not in f))
    for c, f in enumerate(cs[:4]):
        im = Image.open(f'{d}/{f}').convert('RGB').resize((W, H)); ImageDraw.Draw(im).text((6, 6), f, fill=(255, 255, 0)); o.paste(im, (c * W, r * H))
o.save(out, quality=85)
