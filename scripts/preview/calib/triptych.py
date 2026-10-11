#!/usr/bin/env python3
"""三联并排: 旧工作台(studio) / 校准后 game-day / 港口实拍 —— 用 fit_game_look.py eval 的出图。
用法: triptych.py <work>/eval <tag> <spec.json> <out_dir>"""
import sys, json
from PIL import Image, ImageDraw, ImageFont
ev, tag, spec, out = sys.argv[1:5]
S = json.load(open(spec)); R = {(r['shot'], r['cls']): r for r in json.load(open(f'{ev}/{tag}.json'))}
try: F = ImageFont.truetype('/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc', 22)
except Exception:
    import glob; fs = glob.glob('/usr/share/fonts/**/*CJK*', recursive=True) + glob.glob('/usr/share/fonts/**/*.ttf', recursive=True)
    F = ImageFont.truetype(fs[0], 22) if fs else ImageFont.load_default()
H = 540
for s in S['shots']:
    n = s['name']
    a = Image.open(f'{ev}/{n}_{tag}_studio.png').convert('RGB'); b = Image.open(f'{ev}/{n}_{tag}_game.png').convert('RGB'); c = Image.open(s['port']).convert('RGB')
    tiles = [im.resize((im.size[0] * H // im.size[1], H)) for im in (a, b, c)]
    rows = [r for k, r in R.items() if k[0] == n]
    ms = sum(r['studio']['de'] for r in rows) / len(rows); mg = sum(r['game']['de'] for r in rows) / len(rows)
    labels = [f'旧工作台 studio  ΔE2000 均值 {ms:.1f}', f'校准后 game-day  ΔE2000 均值 {mg:.1f}', '港口实拍 (菲律宾)']
    W = sum(t.size[0] for t in tiles) + 16; o = Image.new('RGB', (W, H + 44 + 30), (246, 247, 249)); d = ImageDraw.Draw(o); x = 0
    for t, l in zip(tiles, labels):
        o.paste(t, (x, 44)); d.text((x + 10, 10), l, font=F, fill=(36, 40, 48)); x += t.size[0] + 8
    d.text((10, H + 48), '  '.join(f"{r['cls']} {r['studio']['de']:.1f}→{r['game']['de']:.1f}" for r in rows), font=F, fill=(90, 96, 108))
    o.save(f'{out}/triptych_{n}.jpg', quality=88); print(f'{out}/triptych_{n}.jpg')
