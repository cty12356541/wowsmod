#!/usr/bin/env python3
"""从画师色号索引图(robin_p2_paint.py 输出的 *_idx.png)生成按色类的 recolor 遮罩。
只选指定色类, 白/乳白船体(white/dhwhite/roof/deck/turret)天然不在内 —— 比 --hue-range 安全(白底常带淡紫偏色)。
用法: python3 idx_class_mask.py hull_idx.png out.png glow [gold peri ...] [--feather 2]
然后: wowsmod tex recolor <贴图_a.dds> --project P --mask out.png --to-color <port-albedo 换算色> --chroma-detail 0.2"""
import argparse
import numpy as np
from PIL import Image, ImageFilter

NAMES = ['white', 'peri', 'peri_d', 'navy', 'deck', 'dhwhite', 'turret', 'violet', 'glow', 'gold', 'roof', 'star', 'royal']

ap = argparse.ArgumentParser()
ap.add_argument('idx'); ap.add_argument('out'); ap.add_argument('classes', nargs='+')
ap.add_argument('--feather', type=int, default=2, help='向外扩 + 模糊(像素), 防 mip 边缘漏色')
a = ap.parse_args()
idx = np.asarray(Image.open(a.idx))
if idx.ndim == 3: idx = idx[..., 0]
m = np.zeros(idx.shape, bool)
for c in a.classes: m |= idx == NAMES.index(c)
hard = Image.fromarray((m * 255).astype(np.uint8), 'L')
im = hard
if a.feather:
    im = hard.filter(ImageFilter.MaxFilter(a.feather * 2 + 1)).filter(ImageFilter.GaussianBlur(a.feather * 0.6))
    im = Image.fromarray(np.maximum(np.asarray(im), np.asarray(hard)), 'L')
im.save(a.out)
print(f'{a.out}: {",".join(a.classes)} {m.sum()} px ({100 * m.mean():.2f}%)')
