"""生成 model import 用的简单测试网格(OBJ, glTF 约定: y 上, 右手; 单位同 visual, 约 1 = 30 m)。

  python test_mesh.py halo  --radius 0.3 --tube 0.02 --spikes 12 -o halo.obj   # 水平光环(+放射短刺)
  python test_mesh.py wing  --length 0.6 --chord 0.18 --thick 0.01 -o wing.obj # 后掠刀翼(根部在原点, 沿 +x 伸出; --mirror 得 -x 侧)
  python test_mesh.py masks --kind wing -o wing   # 生成 wing_a.png / wing_glow.png(按上面网格的 UV)
"""
import argparse, math

def write_obj(path, verts, uvs, faces):
    with open(path, "w") as f:
        for v in verts: f.write("v %.6f %.6f %.6f\n" % v)
        for t in uvs: f.write("vt %.6f %.6f\n" % t)
        for fc in faces: f.write("f " + " ".join("%d/%d" % (i + 1, i + 1) for i in fc) + "\n")

def halo(a):
    V, T, F = [], [], []
    n, m = 96, 12
    for i in range(n + 1):
        u = 2 * math.pi * i / n
        for j in range(m + 1):
            w = 2 * math.pi * j / m
            r = a.radius + a.tube * math.cos(w)
            V.append((r * math.cos(u), a.tube * math.sin(w), r * math.sin(u)))
            T.append((i / n, j / m))
    for i in range(n):
        for j in range(m):
            k = i * (m + 1) + j
            F.append((k, k + 1, k + m + 2, k + m + 1))
    # 放射短刺(四棱锥), UV 放在 v=0.5 一行
    for s in range(a.spikes):
        u = 2 * math.pi * (s + 0.5) / a.spikes
        c, d = math.cos(u), math.sin(u)
        r0, r1, h = a.radius + a.tube, a.radius + a.tube + a.spike_len, a.tube * 0.9
        px, pz = -d, c
        base = len(V)
        for (sx, sy) in [(-1, 0), (0, 1), (1, 0), (0, -1)]:
            V.append((r0 * c + px * h * sx, h * sy, r0 * d + pz * h * sx)); T.append(((s + 0.5) / a.spikes, 0.5))
        V.append((r1 * c, 0.0, r1 * d)); T.append(((s + 0.5) / a.spikes, 0.5))
        for q in range(4):
            F.append((base + q, base + (q + 1) % 4, base + 4))
    return V, T, F

def wing(a):
    # 平面轮廓: 根部弦长 chord, 尖端收尖, 后掠; 上下两面 + 边
    prof = []
    N = 24
    for i in range(N + 1):
        t = i / N
        x = a.length * t
        lead = a.sweep * a.length * t * t          # 前缘后掠(-z 为后)
        c = a.chord * (1 - t) ** 0.7
        y = a.dihedral * x
        prof.append((x, y, -lead, -lead - c, t))
    V, T, F = [], [], []
    for side, dy in [(0, a.thick / 2), (1, -a.thick / 2)]:
        for (x, y, zl, zt, t) in prof:
            V.append((x, y + dy, zl)); T.append((t, 0.0))
            V.append((x, y + dy, zt)); T.append((t, 1.0))
    n = len(prof)
    for i in range(N):
        a0, b0, a1, b1 = 2 * i, 2 * i + 1, 2 * i + 2, 2 * i + 3
        F.append((a0, a1, b1, b0))
        o = 2 * n
        F.append((o + a0, o + b0, o + b1, o + a1))
        F.append((a0, o + a0, o + a1, a1))       # 前缘
        F.append((b0, b1, o + b1, o + b0))       # 后缘
    # 上面按"-z 为后"写的; glTF 约定里船艏是 -z(游戏 +z), 翼要向后掠 → z 取反, 面翻转保持朝外
    V = [(x, y, -z) for (x, y, z) in V]; F = [tuple(reversed(f)) for f in F]
    return V, T, F

def masks(a):
    from PIL import Image, ImageDraw, ImageFilter
    S = 512
    if a.kind == "wing":
        alb = Image.new("RGB", (S, S), (30, 34, 40))
        g = Image.new("L", (S, S), 0)
        d, dg = ImageDraw.Draw(alb), ImageDraw.Draw(g)
        # 前缘(v=0)一条亮边 + 3 条沿展向的发光线; u=0 根部 → u=1 翼尖
        for v0, w in [(0.0, 26), (0.35, 10), (0.6, 8), (0.82, 6)]:
            y = int(v0 * S)
            d.rectangle([0, y, S, y + w], fill=(150, 240, 255)); dg.rectangle([0, y, S, y + w], fill=255)
        g = g.filter(ImageFilter.GaussianBlur(3))
    else:
        alb = Image.new("RGB", (S, S), (150, 240, 255))
        g = Image.new("L", (S, S), 255)
    alb.save(a.out + "_a.png"); g.save(a.out + "_glow.png")

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("kind", choices=["halo", "wing", "masks"])
    ap.add_argument("-o", "--out", required=True)
    ap.add_argument("--radius", type=float, default=0.3); ap.add_argument("--tube", type=float, default=0.02)
    ap.add_argument("--spikes", type=int, default=12); ap.add_argument("--spike-len", type=float, default=0.06)
    ap.add_argument("--length", type=float, default=0.6); ap.add_argument("--chord", type=float, default=0.18)
    ap.add_argument("--thick", type=float, default=0.012); ap.add_argument("--sweep", type=float, default=0.35)
    ap.add_argument("--dihedral", type=float, default=0.25)
    ap.add_argument("--kind", dest="mkind", default="wing")
    ap.add_argument("--mirror", action="store_true", help="左右镜像(x 取反)")
    a = ap.parse_args()
    if a.kind == "masks":
        a.kind = a.mkind; masks(a)
    else:
        V, T, F = halo(a) if a.kind == "halo" else wing(a)
        if a.mirror:  # 左右镜像(x 取反, 面翻转保持朝外)
            V = [(-x, y, z) for (x, y, z) in V]; F = [tuple(reversed(f)) for f in F]
        write_obj(a.out, V, T, F)
        print(a.out, len(V), "verts", len(F), "faces")
