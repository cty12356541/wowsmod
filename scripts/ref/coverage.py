#!/usr/bin/env python3
"""区域明度分层统计(层 1 量化参考 / 层 7 对照港口): 暗底 / 中灰块面 / 浅色 / 发光 各占多少、平均色。

用法: python coverage.py <图> --box x,y,w,h(比例 0..1) [--bins 60,140,200] [--save crop.png]
同一部件(如舷侧水线以上条带)在参考帧和港口截图上各量一次, 差距写进设计书 §5。
"""
import argparse
import numpy as np
from PIL import Image
ap = argparse.ArgumentParser()
ap.add_argument("image"); ap.add_argument("--box", required=True)
ap.add_argument("--bins", default="60,140,200"); ap.add_argument("--save")
a = ap.parse_args()
im = Image.open(a.image).convert("RGB"); W, H = im.size
x, y, w, h = [float(v) for v in a.box.split(",")]
c = im.crop((int(x*W), int(y*H), int((x+w)*W), int((y+h)*H)))
if a.save: c.save(a.save)
px = np.asarray(c).astype(float); L = 0.2126*px[..., 0] + 0.7152*px[..., 1] + 0.0722*px[..., 2]
edges = [0] + [float(b) for b in a.bins.split(",")] + [256]
names = ["暗底", "中灰块面", "浅色", "发光/白"] if len(edges) == 5 else [f"bin{i}" for i in range(len(edges)-1)]
for n, lo, hi in zip(names, edges, edges[1:]):
    m = (L >= lo) & (L < hi)
    col = "#%02X%02X%02X" % tuple(int(v) for v in px[m].mean(0)) if m.any() else "-"
    print(f"{n:6s} L[{lo:.0f},{hi:.0f}) 占比 {m.mean()*100:5.1f}%  平均色 {col}")
