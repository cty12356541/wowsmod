#!/usr/bin/env python3
"""生成一个合成测试 PMX(2.0, UTF-16LE, 2 字节索引), 用于验证 `wowsmod model pmx2glb` + `model import`.
内容全部由本脚本生成(无第三方版权): 一个立方体"身体"(贴图 body.png)、一块双面"披风"(带透明的 cape.png)、
一个 diffuse alpha=0 的隐藏材质(应被跳过)。材质名用中文, 检查命名回退。
用法: python3 make_test_pmx.py <输出目录>   (需要 Pillow)
"""
import os, struct, sys
from PIL import Image, ImageDraw

out = sys.argv[1] if len(sys.argv) > 1 else "pmx_test"
os.makedirs(os.path.join(out, "tex"), exist_ok=True)
# 贴图
b = Image.new("RGB", (256, 256), (230, 230, 240))
d = ImageDraw.Draw(b)
for i in range(0, 256, 32):
    d.rectangle([i, 0, i + 15, 255], fill=(120, 220, 210))
b.save(os.path.join(out, "tex", "body.png"))
c = Image.new("RGBA", (256, 256), (0, 0, 0, 0))
d = ImageDraw.Draw(c)
d.polygon([(128, 8), (248, 248), (8, 248)], fill=(255, 120, 200, 255))
c.save(os.path.join(out, "tex", "cape.png"))

def s(t):
    e = t.encode("utf-16-le")
    return struct.pack("<i", len(e)) + e

V, F = [], []  # 顶点(pos, nrm, uv), 面索引
def quad(p0, p1, p2, p3, n):
    k = len(V)
    for p, uv in zip((p0, p1, p2, p3), ((0, 1), (1, 1), (1, 0), (0, 0))):
        V.append((p, n, uv))
    # PMX(DirectX 约定)正面为顺时针
    F.extend([k, k + 2, k + 1, k, k + 3, k + 2])

# 立方体 (MMD 单位, 高 20 ≈ 1.6 m), 面朝 -z(MMD 角色朝向)
x0, x1, y0, y1, z0, z1 = -4, 4, 0, 20, -3, 3
cube = [
    ((x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0), (0, 0, -1)),
    ((x1, y0, z1), (x0, y0, z1), (x0, y1, z1), (x1, y1, z1), (0, 0, 1)),
    ((x0, y0, z1), (x0, y0, z0), (x0, y1, z0), (x0, y1, z1), (-1, 0, 0)),
    ((x1, y0, z0), (x1, y0, z1), (x1, y1, z1), (x1, y1, z0), (1, 0, 0)),
    ((x0, y1, z0), (x1, y1, z0), (x1, y1, z1), (x0, y1, z1), (0, 1, 0)),
]
for q in cube:
    quad(*q)
n_body = len(F)
quad((-6, 2, 4), (6, 2, 4), (6, 18, 4), (-6, 18, 4), (0, 0, -1))  # 披风(背后, 双面)
n_cape = len(F) - n_body
quad((-1, 21, 0), (1, 21, 0), (1, 23, 0), (-1, 23, 0), (0, 0, -1))  # 隐藏件
n_hidden = len(F) - n_body - n_cape

o = bytearray(b"PMX " + struct.pack("<f", 2.0) + bytes([8, 0, 0, 2, 1, 1, 2, 1, 1]))
o += s("测试人形") + s("test figure") + s("wowsmod 合成测试模型, 无第三方版权") + s("")
o += struct.pack("<i", len(V))
for p, n, uv in V:
    o += struct.pack("<8f", *p, *n, *uv) + bytes([0]) + struct.pack("<h", 0) + struct.pack("<f", 1.0)
o += struct.pack("<i", len(F)) + b"".join(struct.pack("<H", i) for i in F)
o += struct.pack("<i", 2) + s("tex\\body.png") + s("tex\\cape.png")
o += struct.pack("<i", 3)
for name, diff, flag, tex, cnt in [("身体", (1, 1, 1, 1), 0, 0, n_body), ("披风", (1, 1, 1, 1), 1, 1, n_cape), ("隐藏", (1, 1, 1, 0), 0, -1, n_hidden)]:
    o += s(name) + s("") + struct.pack("<4f", *diff) + struct.pack("<7f", 0, 0, 0, 5, .5, .5, .5)
    o += bytes([flag]) + struct.pack("<5f", 0, 0, 0, 1, 1) + struct.pack("<b", tex) + struct.pack("<b", -1)
    o += bytes([0, 1, 0]) + s("") + struct.pack("<i", cnt)
o += struct.pack("<i", 0)  # 骨骼 0(读取器只读到材质为止)
open(os.path.join(out, "test_figure.pmx"), "wb").write(o)
print(os.path.join(out, "test_figure.pmx"), len(V), "verts", len(F) // 3, "tris")
