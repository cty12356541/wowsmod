# 生成"烟囱顶新网格"示例 GLB(glTF 整船坐标, 与 wowsmod assemble / export-blender 输出一致):
# 椭圆筒形烟囱帽 + 顶上一根细桅杆。仅用 numpy + 标准库。
import json, struct, sys, numpy as np
cx, cy, cz = 0.0, 1.735, 2.03      # 烟囱顶中心(glTF; 由 export-blender 的 MidBack_Chimney 顶部包围盒得到)
rx, rz, h = 0.175, 0.11, 0.045     # 帽半径/高度
seg = 24
P, N, UV, I = [], [], [], []
def quad(a, b, c, d, n, uvs):
    base = len(P)
    for p, uv in zip((a, b, c, d), uvs):
        P.append(p); N.append(n); UV.append(uv)
    I.extend([base, base + 1, base + 2, base, base + 2, base + 3])
def tri(a, b, c, n, uvs):
    base = len(P)
    for p, uv in zip((a, b, c), uvs):
        P.append(p); N.append(n); UV.append(uv)
    I.extend([base, base + 1, base + 2])
# 侧面(外法线), 顶面, 底面
for k in range(seg):
    t0, t1 = 2 * np.pi * k / seg, 2 * np.pi * (k + 1) / seg
    x0, z0, x1, z1 = cx + rx * np.cos(t0), cz + rz * np.sin(t0), cx + rx * np.cos(t1), cz + rz * np.sin(t1)
    tm = (t0 + t1) / 2
    n = np.array([np.cos(tm) / rx, 0, np.sin(tm) / rz]); n = list(n / np.linalg.norm(n))
    u0, u1 = 0.1 + 0.1 * k / seg, 0.1 + 0.1 * (k + 1) / seg
    # 逆时针(从外面看)= glTF 正面
    quad([x0, cy, z0], [x0, cy + h, z0], [x1, cy + h, z1], [x1, cy, z1], n, [[u0, 0.12], [u0, 0.1], [u1, 0.1], [u1, 0.12]])
    quad([x0, cy, z0], [x0, cy + h, z0], [x1, cy + h, z1], [x1, cy, z1], n, [[u0, 0.12], [u0, 0.1], [u1, 0.1], [u1, 0.12]])
    I[-12:] = I[-12:-6]  # 先占位, 下面按法线统一修正绕序
    del P[-4:], N[-4:], UV[-4:]
    tri([cx, cy + h, cz], [x1, cy + h, z1], [x0, cy + h, z0], [0, 1, 0], [[0.15, 0.15], [0.16, 0.14], [0.14, 0.14]])
    tri([cx, cy, cz], [x0, cy, z0], [x1, cy, z1], [0, -1, 0], [[0.15, 0.15], [0.14, 0.14], [0.16, 0.14]])
# 桅杆: 细长方体
w, mh = 0.012, 0.32
x0, x1, y0, y1, z0, z1 = cx - w, cx + w, cy + h, cy + h + mh, cz - w, cz + w
box = {
    (1, 0, 0): [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
    (-1, 0, 0): [[x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [x0, y0, z0]],
    (0, 0, 1): [[x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [x0, y0, z1]],
    (0, 0, -1): [[x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0]],
    (0, 1, 0): [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
}
for n, q in box.items():
    quad(*q, list(n), [[0.2, 0.2], [0.2, 0.21], [0.21, 0.21], [0.21, 0.2]])
P = np.array(P, np.float32); N = np.array(N, np.float32); UV = np.array(UV, np.float32); I = np.array(I, np.uint32).reshape(-1, 3)
# 统一绕序: 让几何法线 (b-a)x(c-a) 与顶点法线同向(glTF 逆时针为正面)
for t in I:
    a, b, c = P[t]
    if np.dot(np.cross(b - a, c - a), N[t[0]]) < 0:
        t[1], t[2] = t[2], t[1]
# 侧面占位产生的未引用顶点无害; 写 GLB
bin_ = b''
views, accs = [], []
def add(arr, target, comp, typ, minmax=False):
    global bin_
    while len(bin_) % 4: bin_ += b'\0'
    off = len(bin_); data = arr.tobytes(); bin_ += data
    views.append(dict(buffer=0, byteOffset=off, byteLength=len(data), target=target))
    a = dict(bufferView=len(views) - 1, componentType=comp, count=int(arr.shape[0] if arr.ndim > 1 else arr.size), type=typ)
    if minmax: a['min'] = arr.min(0).tolist(); a['max'] = arr.max(0).tolist()
    accs.append(a); return len(accs) - 1
ip = add(P, 34962, 5126, 'VEC3', True); inn = add(N, 34962, 5126, 'VEC3'); iuv = add(UV, 34962, 5126, 'VEC2'); ii = add(I.flatten(), 34963, 5125, 'SCALAR')
j = dict(asset=dict(version='2.0', generator='make_cap.py'), scene=0, scenes=[dict(nodes=[0])],
         nodes=[dict(name='FunnelCap', mesh=0)], meshes=[dict(name='FunnelCap', primitives=[dict(attributes=dict(POSITION=ip, NORMAL=inn, TEXCOORD_0=iuv), indices=ii, material=0)])],
         materials=[dict(name='SHIPMAT_PBS_DeckHouse')], accessors=accs, bufferViews=views, buffers=[dict(byteLength=len(bin_))])
js = json.dumps(j).encode()
while len(js) % 4: js += b' '
while len(bin_) % 4: bin_ += b'\0'
out = struct.pack('<III', 0x46546C67, 2, 12 + 8 + len(js) + 8 + len(bin_)) + struct.pack('<II', len(js), 0x4E4F534A) + js + struct.pack('<II', len(bin_), 0x004E4942) + bin_
open(sys.argv[1], 'wb').write(out)
print('verts', len(P), 'tris', len(I), 'bbox', P.min(0), P.max(0))
