#!/usr/bin/env python3
"""并排对比图(验收层通用): 参考帧 | 工作台出图 | 港口实拍 …, 每格带标签, 统一高度。

用法:
  python compare.py --out cmp.png  ref.jpg:参考 t=34s  wb.png:工作台  port.png:港口 [--crop 图序号=x,y,w,h ...] [--height 540]
  --crop 的坐标是相对比例(0..1), 例如 --crop 2=0.3,0.3,0.5,0.5 表示第 2 张只取中间区域(放大局部)。
  --grid N: 每行 N 张(默认全部一行)。
"""
import argparse, sys
from PIL import Image, ImageDraw, ImageFont

import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cjkfont import font  # 系统中文字体(WOWSMOD_CJK_FONT 可覆盖)

ap = argparse.ArgumentParser()
ap.add_argument("items", nargs="+", help="图片路径:标签")
ap.add_argument("--out", required=True)
ap.add_argument("--height", type=int, default=540)
ap.add_argument("--crop", action="append", default=[])
ap.add_argument("--grid", type=int, default=0)
a = ap.parse_args()
crops = {}
for c in a.crop:
    i, r = c.split("=", 1); crops[int(i)] = [float(v) for v in r.split(",")]
tiles = []
F = font(max(16, a.height // 22))
for n, it in enumerate(a.items, 1):
    path, _, label = it.partition(":")
    im = Image.open(path).convert("RGB")
    if n in crops:
        x, y, w, h = crops[n]; W, H = im.size
        im = im.crop((int(x*W), int(y*H), int((x+w)*W), int((y+h)*H)))
    im = im.resize((max(1, im.width * a.height // im.height), a.height), Image.LANCZOS)
    d = ImageDraw.Draw(im)
    tb = d.textbbox((0, 0), label or path, font=F)
    d.rectangle([0, 0, tb[2] + 16, tb[3] + 12], fill=(0, 0, 0))
    d.text((8, 4), label or path, fill=(255, 230, 80), font=F)
    tiles.append(im)
g = a.grid or len(tiles)
rows = [tiles[i:i+g] for i in range(0, len(tiles), g)]
W = max(sum(t.width for t in r) + 6*(len(r)-1) for r in rows)
H = len(rows) * a.height + 6*(len(rows)-1)
out = Image.new("RGB", (W, H), (20, 20, 20))
y = 0
for r in rows:
    x = 0
    for t in r:
        out.paste(t, (x, y)); x += t.width + 6
    y += a.height + 6
out.save(a.out); print(a.out, out.size)
