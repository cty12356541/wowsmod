#!/usr/bin/env python3
"""发光线换色(不重画整张涂装): glow_recolor.py <in_a.png> <glow_mask.png> <#RRGGBB> <out.png>
遮罩(白=发光, 尺寸不同按最近邻缩放)内像素换成目标色; 其余不变。发光色 = _a 该处颜色(emissive 着色器),
近白色(如 #B8E8FF)在港口读成「白漆」, 要读成主题色须提高饱和度(T3: #58B8FF)。"""
import sys, numpy as np
from PIL import Image
a = Image.open(sys.argv[1]).convert('RGB'); m = Image.open(sys.argv[2]).convert('L')
if m.size != a.size: m = m.resize(a.size, Image.NEAREST)
A = np.array(a); M = np.array(m) > 127
h = sys.argv[3].lstrip('#'); c = np.array([int(h[i:i + 2], 16) for i in (0, 2, 4)], np.uint8)
A[M] = c
Image.fromarray(A).save(sys.argv[4])
print('recolored', int(M.sum()), 'px ->', '#' + h)
