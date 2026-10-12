"""知更鸟外带道具 Blender 程序化建模(headless):
  blender -b -P robin_props_blender.py -- <out_dir> [prop ...]
坐标: Blender X=游戏 x(右舷), Y=游戏 z(艏向), Z=游戏 y(上); glTF 导出后 = 游戏坐标(z 取反由导出器处理, 与 export 的 glb 一致)。
单位 = visual 单位(1≈13.9 m)。每件: 建网格 → 按材质名 rb_<列> 压 UV 进共享图集列 → 合并 → 三角化 → 导出 glb + 预览图。
禁人物雕像: 这里只有羽翼/光环/罩壳/麦克风/缎带等道具形。
"""
import bpy, bmesh, math, sys, os, json
from mathutils import Vector, Matrix, Euler

argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
OUT = argv[0] if argv else os.getcwd()
WANT = set(argv[1:])
COLS = ['feather_wp', 'feather_pv', 'white', 'peri', 'violet', 'navy', 'gold', 'glow']

def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)

def mat(name):
    m = bpy.data.materials.get('rb_' + name) or bpy.data.materials.new('rb_' + name)
    return m

def new_obj(name, bm, mats):
    me = bpy.data.meshes.new(name); bm.to_mesh(me); bm.free()
    ob = bpy.data.objects.new(name, me); bpy.context.collection.objects.link(ob)
    for m in mats: me.materials.append(mat(m))
    return ob

# ---------- 基元 ----------
def feather(L, W, th=0.018, curve=0.10, nL=16, nW=4, colname='feather_wp', tipsharp=0.55, profile=None):
    """羽片: 透镜截面, 沿 +X 长 L, 宽 W(Y 向), 在 XY 平面内向 -Y 弯; UV u=横向 v=沿长。"""
    bm = bmesh.new(); uvl = bm.loops.layers.uv.new('UVMap')
    top, bot = [], []
    for i in range(nL + 1):
        t = i / nL
        w = W * (math.sin(math.pi * min(t, 0.999) ** 0.85) ** tipsharp) * (1.0 - 0.25 * t) + 0.004
        if profile == 'vane':   # A5 真羽: 羽根窄→1/4 处最宽→末端收尖(不对称: 后缘更宽)
            w = 1.35 * W * min(1.0, t / 0.22) ** 0.6 * (1.0 - t) ** 0.85 + 0.003
        cx = L * t; cy = -curve * L * t * t
        rt, rb = [], []
        for j in range(nW + 1):
            s = j / nW; yy = (s - 0.5) * w
            h = th * (1 - (2 * s - 1) ** 2) * ((1 - 0.85 * t) if profile == 'vane' else (1 - 0.6 * t))   # A6: 真羽尾端更薄
            rt.append((bm.verts.new((cx, cy + yy, h)), s, t)); rb.append((bm.verts.new((cx, cy + yy, -h)), s, t))
        top.append(rt); bot.append(rb)
    def quad(a, b, c, d):
        f = bm.faces.new([a[0], b[0], c[0], d[0]])
        for lp, src in zip(f.loops, [a, b, c, d]): lp[uvl].uv = (src[1], src[2])
        return f
    for i in range(nL):
        for j in range(nW):
            quad(top[i][j], top[i + 1][j], top[i + 1][j + 1], top[i][j + 1])
            quad(bot[i][j], bot[i][j + 1], bot[i + 1][j + 1], bot[i + 1][j])
    # 两侧边缝合(同点 h=0 已重合 → 合并)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    ob = new_obj('feather', bm, [colname])
    return ob

def torus(R, r, segR=96, segr=12, col='gold'):
    bm = bmesh.new(); uvl = bm.loops.layers.uv.new('UVMap')
    rows = []
    for i in range(segR):
        a = 2 * math.pi * i / segR; row = []
        for j in range(segr):
            b = 2 * math.pi * j / segr
            rr = R + r * math.cos(b)
            row.append(bm.verts.new((rr * math.cos(a), rr * math.sin(a), r * math.sin(b))))
        rows.append(row)
    for i in range(segR):
        for j in range(segr):
            f = bm.faces.new([rows[i][j], rows[(i + 1) % segR][j], rows[(i + 1) % segR][(j + 1) % segr], rows[i][(j + 1) % segr]])
            for lp in f.loops: lp[uvl].uv = (0.5, 0.5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return new_obj('torus', bm, [col])

def spike(L, r, col='peri', seg=8, flat=0.35):
    """放射刺: 菱形截面锥, 沿 +X; 根半径 r; flat=厚度比。"""
    bm = bmesh.new(); uvl = bm.loops.layers.uv.new('UVMap')
    ring = [bm.verts.new((0, r * math.cos(2 * math.pi * k / seg), r * flat * math.sin(2 * math.pi * k / seg))) for k in range(seg)]
    tip = bm.verts.new((L, 0, 0)); base = bm.verts.new((-r * 0.3, 0, 0))
    for k in range(seg):
        for f in (bm.faces.new([ring[k], ring[(k + 1) % seg], tip]), bm.faces.new([ring[(k + 1) % seg], ring[k], base])):
            for lp in f.loops: lp[uvl].uv = (0.5, 0.5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return new_obj('spike', bm, [col])

def sphere(r, col='gold', seg=24):
    bm = bmesh.new(); bm.loops.layers.uv.new('UVMap'); bmesh.ops.create_uvsphere(bm, u_segments=seg, v_segments=seg // 2, radius=r, calc_uvs=True)
    return new_obj('sphere', bm, [col])

def octagram(r, th, col='gold'):
    bm = bmesh.new(); uvl = bm.loops.layers.uv.new('UVMap')
    pts = []
    for k in range(16):
        a = math.pi / 8 * k; rr = r if k % 2 == 0 else r * 0.42
        pts.append((rr * math.cos(a), rr * math.sin(a)))
    t = [bm.verts.new((x, y, th)) for x, y in pts]; b = [bm.verts.new((x, y, -th)) for x, y in pts]
    ct = bm.verts.new((0, 0, th * 1.8)); cb = bm.verts.new((0, 0, -th * 1.8))
    n = len(pts)
    for k in range(n):
        for f in (bm.faces.new([t[k], t[(k + 1) % n], ct]), bm.faces.new([b[(k + 1) % n], b[k], cb]),
                  bm.faces.new([t[k], b[k], b[(k + 1) % n], t[(k + 1) % n]])):
            for lp in f.loops: lp[uvl].uv = (0.5, 0.5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return new_obj('octagram', bm, [col])

def xform(ob, loc=(0, 0, 0), rot=(0, 0, 0), scale=(1, 1, 1), order='XYZ'):
    ob.matrix_world = Matrix.Translation(loc) @ Euler([math.radians(a) for a in rot], order).to_matrix().to_4x4() @ Matrix.Diagonal((*scale, 1))
    return ob

def join(objs, name):
    bpy.ops.object.select_all(action='DESELECT')
    for o in objs: o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.join(); ob = bpy.context.view_layer.objects.active; ob.name = name
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    return ob

def mirror_x(ob, name):
    c = ob.copy(); c.data = ob.data.copy(); bpy.context.collection.objects.link(c)
    c.matrix_world = Matrix.Diagonal((-1, 1, 1, 1)) @ ob.matrix_world
    bpy.ops.object.select_all(action='DESELECT'); c.select_set(True); bpy.context.view_layer.objects.active = c
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    bm = bmesh.new(); bm.from_mesh(c.data); bmesh.ops.reverse_faces(bm, faces=bm.faces); bm.to_mesh(c.data); bm.free()
    c.name = name
    return c

# ---------- 图集 UV 压列 + 导出 ----------
def atlas_bake_uv(ob):
    me = ob.data
    if not me.uv_layers: me.uv_layers.new(name='UVMap')
    uv = me.uv_layers.active.data
    for poly in me.polygons:
        mname = me.materials[poly.material_index].name[3:]
        pnl = mname.startswith('pnl_')   # T3b: 面板细节区(图集该列上带 v∈[0.62,0.98], 下带镜像: 倒角明暗/AO/铆钉/中缝)
        if pnl: mname = mname[4:]
        col = COLS.index(mname) if mname in COLS else 2
        for li in poly.loop_indices:
            u, v = uv[li].uv
            if col <= 1: uu = (col + 0.12 + 0.76 * u) / 8.0; vv = 0.03 + 0.94 * v   # v2: 内缩防 mip 串列
            elif pnl: uu = (col + 0.08 + 0.84 * min(max(u, 0), 1)) / 8.0; vv = 0.62 + 0.36 * min(max(v, 0), 1)
            else: uu = (col + 0.5) / 8.0; vv = 0.5   # T3b: 纯色取中带(翻转不变; 上/下带为镜像面板细节)
            uv[li].uv = (uu, vv)
    # 合成单材质
    me.materials.clear(); me.materials.append(bpy.data.materials.get('rb_atlas') or bpy.data.materials.new('rb_atlas'))
    for p in me.polygons: p.material_index = 0

def export(objs, name):
    ob = join(objs, name)
    atlas_bake_uv(ob)
    bpy.ops.object.select_all(action='DESELECT'); ob.select_set(True); bpy.context.view_layer.objects.active = ob
    bpy.ops.object.mode_set(mode='EDIT'); bpy.ops.mesh.select_all(action='SELECT'); bpy.ops.mesh.quads_convert_to_tris(); bpy.ops.object.mode_set(mode='OBJECT')
    # 平滑法线(按角度)
    try: bpy.ops.object.shade_auto_smooth(angle=math.radians(40))
    except Exception: bpy.ops.object.shade_smooth()
    path = os.path.join(OUT, name + '.glb')
    bpy.ops.export_scene.gltf(filepath=path, export_format='GLB', use_selection=True, export_apply=True, export_yup=True)
    tris = sum(len(p.vertices) - 2 for p in ob.data.polygons)
    bb = [ob.matrix_world @ Vector(c) for c in ob.bound_box]
    mn = [min(v[i] for v in bb) for i in range(3)]; mx = [max(v[i] for v in bb) for i in range(3)]
    # 报告用游戏坐标: x, y=Blender Z, z=Blender Y
    return dict(name=name, glb=path, tris=tris, game_min=[mn[0], mn[2], mn[1]], game_max=[mx[0], mx[2], mx[1]])

# ---------- 道具 ----------
def tube(pts, radii, col='white', seg=10):
    """沿折线的圆管(翼臂/缎带边), 每点半径 radii[i]。"""
    bm = bmesh.new(); uvl = bm.loops.layers.uv.new('UVMap'); rings = []
    P = [Vector(p) for p in pts]
    for i, p in enumerate(P):
        d = (P[min(i + 1, len(P) - 1)] - P[max(i - 1, 0)]).normalized()
        a = d.orthogonal().normalized(); b = d.cross(a).normalized()
        rings.append([bm.verts.new(p + radii[i] * (math.cos(2 * math.pi * k / seg) * a + math.sin(2 * math.pi * k / seg) * b)) for k in range(seg)])
    for i in range(len(P) - 1):
        for k in range(seg):
            f = bm.faces.new([rings[i][k], rings[i + 1][k], rings[i + 1][(k + 1) % seg], rings[i][(k + 1) % seg]])
            for lp in f.loops: lp[uvl].uv = (0.5, 0.5)
    for r in (rings[0], rings[-1][::-1]):
        f = bm.faces.new(r)
        for lp in f.loops: lp[uvl].uv = (0.5, 0.5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return new_obj('tube', bm, [col])

def arm_point(span, s):
    """翼前缘臂(局部 XY): 根→腕(0.42)→梢, 腕处前凸、梢略后掠。"""
    x = span * s
    y = 0.10 * span * math.sin(math.pi * min(s / 0.42, 1.0) * 0.5) * (1 - max(0, s - 0.42) / 0.58 * 0.9) - 0.12 * span * max(0, s - 0.42) ** 1.6
    return (x, y, 0.0)

def wing(span=2.6, n=11, base_col='feather_wp', cover_col='feather_pv'):
    """鸟翼(局部): 根在原点, 前缘臂沿 +X; 羽根沿臂分布, 次级羽朝后(−Y)、初级羽(梢部)朝外后且最长 —— 天使翼剪影。"""
    parts = []
    for k in range(n):
        s = 0.06 + 0.92 * k / (n - 1)
        x, y, _ = arm_point(span, s)
        ang = -100 + 80 * s ** 1.3                     # 根部朝后偏内, 梢部朝外后
        L = span * (0.32 + 0.28 * s ** 1.5)           # 初级羽最长
        W = 0.22 + 0.05 * (1 - s)                      # L1: 随翼展缩小, 仍相邻重叠
        f = feather(L, W, th=0.024, curve=0.06, colname=base_col, tipsharp=0.35)
        xform(f, loc=(x, y, -0.006 * k), rot=(0, 0, ang)); parts.append(f)
    for k in range(n - 3):                              # 覆羽: 沿臂短羽, 上层偏紫
        s = 0.05 + 0.75 * k / (n - 4)
        x, y, _ = arm_point(span, s)
        f = feather(span * (0.17 + 0.08 * s), 0.24, th=0.022, curve=0.04, colname=cover_col, tipsharp=0.35)
        xform(f, loc=(x, y, 0.035), rot=(0, 0, -95 + 70 * s)); parts.append(f)
    pts = [arm_point(span, i / 20) for i in range(21)]
    parts.append(tube([(x, y + 0.02, 0.02) for x, y, _ in pts], [0.05 * (1 - 0.7 * i / 20) + 0.012 for i in range(21)], 'white'))
    parts.append(tube([(x, y + 0.055, 0.03) for x, y, _ in pts[:18]], [0.012] * 18, 'gold'))
    parts.append(tube([(x, y + 0.075, 0.025) for x, y, _ in pts[:16]], [0.010] * 16, 'glow'))   # c2: 0.006→0.010
    # 根关节: 金球 + 金环 + 发光细环 + 八芒星
    parts.append(sphere(0.09, 'gold'))
    tr = torus(0.12, 0.022, 48, 10, 'gold'); xform(tr, rot=(0, 90, 0)); parts.append(tr)
    tg = torus(0.145, 0.008, 48, 6, 'glow'); xform(tg, rot=(0, 90, 0)); parts.append(tg)
    o = octagram(0.13, 0.012, 'gold'); xform(o, loc=(0.0, 0.0, 0.11)); parts.append(o)
    return parts

def place(parts, loc, rot):
    """局部→船: rot=(展向抬升 dihedral, 后掠 sweep, 翼面横滚 roll) 依次: 先绕 X(roll) 再绕 Y(抬升) 再绕 Z(后掠)。"""
    roll, dih, sweep = rot
    M = Matrix.Translation(loc) @ Matrix.Rotation(math.radians(-sweep), 4, 'Z') @ Matrix.Rotation(math.radians(-dih), 4, 'Y') @ Matrix.Rotation(math.radians(roll), 4, 'X')
    for p in parts: p.matrix_world = M @ p.matrix_world
    return parts

def blade(L, W, th, main='feather_wp', sub='feather_pv', sub_k=0.62, glow=True):
    """厚羽刃(局部 +X 长, +Y 前缘): 主羽(厚透镜) + 内叠副刃(下层, 深色, 短) + 金前缘管 + 天青发光槽。"""
    parts = []
    f = feather(L, W, th=th, curve=0.05, nL=18, nW=4, colname=main, tipsharp=0.45); parts.append(f)
    g = feather(L * sub_k, W * 0.78, th=th * 0.8, curve=0.04, nL=12, nW=4, colname=sub, tipsharp=0.4)
    xform(g, loc=(0.02 * L, -0.10 * W, -th * 1.3)); parts.append(g)
    def edge(t, side):
        w = W * (math.sin(math.pi * min(t, 0.999) ** 0.85) ** 0.45) * (1.0 - 0.25 * t) + 0.004
        return (L * t, -0.05 * L * t * t + side * w * 0.5, 0.0)
    pts = [edge(i / 20, 1) for i in range(1, 19)]
    parts.append(tube([(x, y + 0.004, z + th * 0.3) for x, y, z in pts], [th * 0.55 * (1 - 0.6 * i / 18) + 0.006 for i in range(18)], 'gold', seg=6))
    if glow:
        gp = [(L * t, -0.05 * L * t * t + 0.12 * W * (1 - t), th * 1.0 + 0.004) for t in [0.06 + 0.8 * i / 16 for i in range(17)]]
        parts.append(tube(gp, [0.010] * 17, 'glow', seg=5))
    return parts

def aim(parts, root, d, up_hint, roll=0.0):
    """把局部 +X 对准方向 d, 局部 +Y(前缘)取 up_hint 去掉 d 分量后再绕 d 转 roll°。"""
    D = Vector(d).normalized(); U = Vector(up_hint); U = (U - D * D.dot(U)).normalized()
    U = (Matrix.Rotation(math.radians(roll), 3, D) @ U).normalized()
    return frame_place(parts, root, tuple(D), tuple(U))

def dvec(el, sw, out=1.0):
    """右舷方向: 仰角 el, 后掠 sw(°)。Blender: X 外, −Y 艉, Z 上。"""
    e, w = math.radians(el), math.radians(sw)
    return (out * math.cos(e) * math.cos(w), -math.cos(e) * math.sin(w), math.sin(e))

def plume(L, W, th, col, curve=0.14, edge=True, shaft=True, glow=False, round_tip=False):
    """A5 羽片: 弯弧(向后缘弯) + 末端收尖 + 根部窄; 金只在前缘(细, 渐细)和羽轴(细)。round_tip=覆羽圆头。"""
    parts = []
    f = feather(L, W, th=th, curve=curve, nL=20, nW=4, colname=col, tipsharp=0.8 if round_tip else 0.45,
                profile=None if round_tip else 'vane'); parts.append(f)
    def wv(t):
        if round_tip: return W * (math.sin(math.pi * min(t, 0.999) ** 0.85) ** 0.8) * (1.0 - 0.25 * t) + 0.004
        return 1.35 * W * min(1.0, t / 0.22) ** 0.6 * (1.0 - t) ** 0.85 + 0.003
    if edge:
        pts = [(L * t, -curve * L * t * t + 0.5 * wv(t) + 0.003, th * 0.2) for t in [0.08 + 0.84 * i / 16 for i in range(17)]]
        parts.append(tube(pts, [0.014 * (1 - 0.7 * i / 16) + 0.003 for i in range(17)], 'gold', seg=5))  # P6: 金前缘略粗(白天可读)
    if shaft:
        sp = [(L * t, -curve * L * t * t, th * (1 - (0.6 * t)) + 0.004) for t in [0.02 + 0.85 * i / 16 for i in range(17)]]
        # P6: 发光羽轴加粗(白天发光占比目标≈标杆 7%)
        parts.append(tube(sp, [0.016 * (1 - 0.7 * i / 16) + 0.003 for i in range(17)], 'glow' if glow else 'gold', seg=5))  # P5.1: 羽轴略粗于 P5, 不及 P6
    return parts

def shoulder_plate(p0, p1, height, th, col='white', taper=0.55):
    """贴舷楼的装甲肩板(P6): p0 贴墙内缘中心 → p1 外缘; 外端收窄(taper), 略上扬。
    默认实色 white(不用 pnl_ 重 AO, 避免港口读成灰方块)。"""
    p0, p1 = Vector(p0), Vector(p1)
    nrm = (p1 - p0).normalized()
    along = Vector((0, -1, 0)); along = (along - nrm * along.dot(nrm)).normalized()
    up = nrm.cross(along).normalized()
    if up.z < 0: up = -up; along = -along
    half_L0 = abs(p1.y - p0.y) * 0.55 + 0.42   # 内端半长(船长向)
    half_L1 = half_L0 * taper                      # 外端收窄
    half_H0 = height * 0.5; half_H1 = height * 0.38
    bm = bmesh.new(); uvl = bm.loops.layers.uv.new('UVMap')
    def ring(c, half_L, half_H, du):
        o = c + nrm * du
        return [bm.verts.new(o + along * (sL * half_L) + up * (sH * half_H))
                for sL, sH in [(-1, -1), (1, -1), (1, 1), (-1, 1)]]
    rin = ring(p0, half_L0, half_H0, 0.0)
    rout = ring(p1, half_L1, half_H1, th)
    def face(vs, u0=0, u1=1, v0=0, v1=1):
        f = bm.faces.new(vs)
        uvs = [(u0, v0), (u1, v0), (u1, v1), (u0, v1)]
        for lp, uv in zip(f.loops, uvs): lp[uvl].uv = uv
    face([rout[0], rout[1], rout[2], rout[3]])
    face([rin[0], rin[3], rin[2], rin[1]])
    for i in range(4):
        j = (i + 1) % 4
        face([rin[i], rin[j], rout[j], rout[i]], 0, 1, 0, 0.2)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return new_obj('shoulder', bm, [col])


def build_wings():
    """P6「装甲根 + 羽末梢」: 贴舷楼大型肩甲 + 小套筒(无大球) + 羽片从根部长出。
    取代 A7 悬浮光点挂饰 —— 标杆好看核心是外带件从船身长出来、成片有厚度。
    MODE=plate(默认) | float(旧悬浮) | attach(A5 薄壁+套筒, 无肩甲)。"""
    P = []
    MODE = os.environ.get('ROBIN_WING_MODE', 'float')  # P5.1: 回到悬浮结构(用户否决 P6 大肩甲)
    TK = float(os.environ.get('ROBIN_WING_TK', '0.45'))
    if MODE == 'float':
        DZ = float(os.environ.get('ROBIN_WING_DZ', '-0.60')); GAP = float(os.environ.get('ROBIN_WING_GAP', '0.45'))
        sh = Vector((1.19 + GAP, -0.80, 2.10 + DZ))
        tip = sh + Vector((0.62, -0.38, 0.22))
        LS = float(os.environ.get('ROBIN_WING_LS', '0.88')); SW = 6  # P5.1: 略放大(碰撞须 0)
    elif MODE == 'plate':
        # P6: 肩甲贴墙(x≈0.50) → 套筒外口(x≈1.08) → 翼骨外延; 挂点舷楼中部偏上, 避开副炮回转包络
        DZ = float(os.environ.get('ROBIN_WING_DZ', '-0.25'))
        wall = Vector((0.50, -0.70, 1.88 + DZ))          # 贴舷楼外墙
        sh = Vector((1.08, -0.82, 2.05 + DZ))            # 套筒外口 / 翼骨根(=咬合后外伸)
        tip = sh + Vector((1.20, -0.42, 0.28))           # P6b: 翼骨更外展(俯视剪影更宽)
        LS = float(os.environ.get('ROBIN_WING_LS', '1.05')); SW = 2
    else:  # attach
        DZ = float(os.environ.get('ROBIN_WING_DZ', '-0.40'))
        wall = Vector((0.47, -0.64, 1.98 + DZ))
        sh = Vector((0.82, -0.72, 2.10 + DZ)); tip = Vector((1.75, -1.05, 2.38 + DZ))
        LS = 0.86; SW = 0
    arm = lambda t: sh + (tip - sh) * t
    if MODE == 'float':
        ax = (sh - tip).normalized()
        core = sphere(1.0, 'glow', 20); xform(core, loc=tuple(sh), scale=(0.075, 0.075, 0.075)); P.append(core)
        for R, r, col in [(0.15, 0.016, 'gold'), (0.20, 0.008, 'glow'), (0.25, 0.006, 'gold')]:
            ring = torus(R, r, 48, 6, col); P.append(ring)
            q = Vector((0, 0, 1)).rotation_difference(ax)
            ring.matrix_world = Matrix.Translation(sh) @ q.to_matrix().to_4x4(); bpy.ops.object.select_all(action='DESELECT')
            ring.select_set(True); bpy.context.view_layer.objects.active = ring; bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
        p0 = Vector((1.22, -0.80, 1.05))
        for i in range(1, 7):
            t = i / 7.0; pt = p0 + (sh - p0) * t + Vector((0, 0, -0.10 * math.sin(math.pi * t)))
            d = sphere(1.0, 'glow', 10); rr = 0.012 + 0.014 * t; xform(d, loc=tuple(pt), scale=(rr, rr, rr)); P.append(d)
    elif MODE == 'plate':
        # 肩甲: 白实色收窄板(从墙长出) + peri 下层叠板; 避免 pnl_ 重 AO 读成灰方块
        p_out = wall.lerp(sh, 0.62) + Vector((0.02, 0, 0.08))
        P.append(shoulder_plate(wall, p_out, height=0.70, th=0.050, col='white', taper=0.50))
        p_out2 = wall.lerp(sh, 0.48) + Vector((0.01, 0.06, -0.10))
        P.append(shoulder_plate(wall + Vector((0.015, 0.06, -0.12)), p_out2, height=0.42, th=0.036, col='peri', taper=0.45))
        # 小套筒(护腕): 肩甲外缘 → 翼骨根, 张口流线, 无大球
        n = 10
        sleeve_pts = [tuple(wall.lerp(sh, 0.50 + 0.50 * i / n)) for i in range(n + 1)]
        P.append(tube(sleeve_pts, [0.100 - 0.040 * (i / n) ** 0.7 for i in range(n + 1)], 'white', seg=14))
        P.append(tube([sleeve_pts[int(0.7 * n)], sleeve_pts[int(0.85 * n)]], [0.072, 0.072], 'gold', seg=14))
        P.append(tube([sleeve_pts[int(0.35 * n)], sleeve_pts[int(0.48 * n)]], [0.088, 0.088], 'glow', seg=14))
        # 肩甲上下沿发光缝 + 外缘金边
        for sy in (-1, 1):
            a = wall + Vector((0.02, sy * 0.40, -0.32)); b = p_out + Vector((0.02, sy * 0.22, 0.30))
            P.append(tube([tuple(a), tuple(b)], [0.012, 0.012], 'glow', seg=6))
        P.append(tube([tuple(p_out + Vector((0.03, -0.22, 0.32))), tuple(p_out + Vector((0.03, 0.22, 0.32)))], [0.009, 0.009], 'gold', seg=6))
        # 细斜撑
        br0 = Vector((0.55, -0.60, 1.20 + DZ)); br1 = wall.lerp(sh, 0.65) + Vector((0, 0, -0.08))
        P.append(tube([tuple(br0 + (br1 - br0) * (i / 8)) for i in range(9)], [0.038 + 0.012 * i / 8 for i in range(9)], 'white', seg=10))
        P.append(tube([tuple(br0 + (br1 - br0) * (i / 8) + Vector((0.05, 0, 0))) for i in range(9)], [0.011] * 9, 'glow', seg=5))
    else:
        wall = Vector((0.47, -0.64, 1.98 + DZ))
        n = 8
        P.append(tube([tuple(wall + (sh - wall) * (i / n)) for i in range(n + 1)], [0.17 - 0.06 * (i / n) ** 0.7 for i in range(n + 1)], 'white', seg=16))
        P.append(tube([tuple(wall + (sh - wall) * t) for t in (0.86, 0.94)], [0.118, 0.118], 'gold', seg=16))
        P.append(tube([tuple(wall + (sh - wall) * t) for t in (0.30, 0.36)], [0.160, 0.160], 'glow', seg=16))
        br0 = Vector((0.50, -0.58, min(1.25, 1.25 + DZ + 0.15))); br1 = wall + (sh - wall) * 0.5 + Vector((0, 0, -0.10))
        P.append(tube([tuple(br0 + (br1 - br0) * (i / 8)) for i in range(9)], [0.055 + 0.015 * i / 8 for i in range(9)], 'white', seg=10))
        P.append(tube([tuple(br0 + (br1 - br0) * (i / 8) + Vector((0.06, 0, 0))) for i in range(9)], [0.008] * 9, 'glow', seg=5))
    # 翼骨: 细白, 下沿发光(P6 加粗)
    m = 14
    P.append(tube([tuple(arm(i / m)) for i in range(m + 1)], [0.090 * (1 - 0.55 * i / m) + 0.022 for i in range(m + 1)], 'white', seg=12))
    P.append(tube([tuple(arm(i / m) + Vector((0, 0.0, -0.075 * (1 - 0.6 * i / m)))) for i in range(m + 1)], [0.012] * (m + 1), 'glow', seg=5))  # P5.1
    UP = lambda tilt: (0, 1.0, math.tan(math.radians(tilt)))
    # 初级飞羽 ×6
    for t, L, el, sw, W in [(0.50, 1.45, 4, 46, 0.30), (0.60, 1.70, 7, 38, 0.31), (0.70, 1.95, 10, 30, 0.32),
                            (0.80, 2.20, 13, 22, 0.33), (0.90, 2.40, 16, 15, 0.33), (1.00, 2.50, 19, 8, 0.32)]:
        L *= LS; sw += SW
        b = plume(L, W * 0.9, 0.055 * TK, 'feather_wp', curve=0.13, glow=True)
        aim(b, tuple(arm(t) + Vector((0, 0, 0.03))), dvec(el, sw), UP(35), roll=0); P += b
    # 次级 ×6
    for k, t in enumerate([0.0, 0.10, 0.20, 0.30, 0.40, 0.50]):
        L = (1.10 + 0.06 * k) * (1.0 if MODE != 'plate' else 1.05); sw = 72 - 6 * k; el = -6 + k
        b = plume(L, 0.27, 0.05 * TK, 'feather_pv', curve=0.12, edge=(k % 2 == 0))
        aim(b, tuple(arm(t) + Vector((0, -0.04, -0.07))), dvec(el, sw), UP(30), roll=0); P += b
    # 覆羽
    for k in range(7):
        t = 0.02 + 0.15 * k
        f = plume(0.72 - 0.03 * k, 0.26, 0.06 * TK, 'white', curve=0.08, edge=False, shaft=False, round_tip=True)
        aim(f, tuple(arm(t) + Vector((0, 0.02, 0.07))), dvec(6, 40 - 3 * k), UP(38), roll=0); P += f
    # 裙摆 ×3: plate 模式略收仰角, 少侵入副炮包络
    for k, (L, el, sw) in enumerate([(1.30, -12 if MODE == 'plate' else -16, 34), (1.12, -20 if MODE == 'plate' else -26, 46), (0.92, -28 if MODE == 'plate' else -36, 58)]):
        b = plume(L * (1.05 if MODE == 'plate' else 1.0), 0.25, 0.05 * TK, 'feather_wp', curve=0.12, edge=(k == 0), glow=(k == 0))
        aim(b, tuple(sh + Vector((-0.05, -0.06 - 0.08 * k, -0.14))), dvec(el, sw), UP(45), roll=0); P += b
    for p in P: p.select_set(False)
    right = join(P, 'wing_R')
    left = mirror_x(right, 'wing_L')
    return [right, left]


def build_halo():
    objs = []
    R = 0.52   # v2: 标杆环 R≈0.027L≈0.5(次级)
    objs.append(torus(R, 0.034, 128, 14, 'gold'))
    objs.append(torus(R - 0.075, 0.012, 128, 8, 'glow'))
    objs.append(torus(R + 0.07, 0.010, 128, 8, 'glow'))
    for k in range(16):
        a = 360 * k / 16; big = k % 2 == 0
        s = spike(0.34 if big else 0.18, 0.05 if big else 0.032, 'peri' if big else 'white')
        xform(s, loc=(R * math.cos(math.radians(a)), R * math.sin(math.radians(a)), 0), rot=(0, 0, a)); objs.append(s)
    for k in range(4):   # 四个金色菱钻
        a = 45 + 90 * k
        o = octagram(0.07, 0.012, 'gold'); xform(o, loc=(R * math.cos(math.radians(a)), R * math.sin(math.radians(a)), 0.03)); objs.append(o)
    h = join(objs, 'halo')
    # 舰桥顶上方(游戏 0, 3.22, -0.18), 向艏微俯 10°
    h.matrix_world = Matrix.Translation((0, -0.18, 3.20)) @ Matrix.Rotation(math.radians(10), 4, 'X')
    bpy.ops.object.select_all(action='DESELECT'); h.select_set(True); bpy.context.view_layer.objects.active = h
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    return [h]

def superellipse(hw, hl, cz, th, n=4.0):
    """俯视超椭圆轮廓点(角度 th, 0=炮口 +Y), 返回 (x, y)。"""
    c, s_ = math.cos(th), math.sin(th)
    r = (abs(c / hl) ** n + abs(s_ / hw) ** n) ** (-1.0 / n)
    return (r * s_, cz + r * c)

def turret_shell(hw, hl, cz, y0, y1, flare, open_deg=38, fin_len=0.5, scallops=10, seg=72, wave=None):
    """炮塔软弧罩壳(炮塔局部, 随炮转): 侧后围裙(上白下 periwinkle)+波浪下摆+金边+发光线+后角耳羽鳍。前方 ±open_deg 开口留炮管。"""
    parts = []
    wave = 0.03 * hw if wave is None else wave   # 下摆波浪幅度随尺寸
    a0, a1 = math.radians(open_deg), math.radians(360 - open_deg)
    ths = [a0 + (a1 - a0) * i / seg for i in range(seg + 1)]
    def ring(y, off, wave=0.0):
        pts = []
        for th in ths:
            x, yy = superellipse(hw + off, hl + off, cz, th)
            pts.append((x, yy, y + wave * math.cos(scallops * (th - a0))))
        return pts
    levels = [(y1, 0.012, 'white'), ((y0 + y1) / 2 + 0.01, 0.018 + flare * 0.35, 'white'), ((y0 + y1) / 2, 0.02 + flare * 0.45, 'peri'), (y0, 0.02 + flare, 'peri')]
    bm = bmesh.new(); uvl = bm.loops.layers.uv.new('UVMap')
    rows = []
    for k, (yy, off, col) in enumerate(levels):
        outer = ring(yy, off, wave=wave if k == len(levels) - 1 else 0.0)
        rows.append([bm.verts.new(p) for p in outer])
    inner = [[bm.verts.new((p.co.x * 0.985, cz + (p.co.y - cz) * 0.985, p.co.z)) for p in r] for r in rows]
    mats = ['white', 'peri']
    def face(vs, mi):
        f = bm.faces.new(vs); f.material_index = mi
        for lp in f.loops: lp[uvl].uv = (0.5, 0.5)
    for k in range(len(rows) - 1):
        mi = 0 if levels[k + 1][2] == 'white' else 1
        for i in range(seg):
            face([rows[k][i], rows[k + 1][i], rows[k + 1][i + 1], rows[k][i + 1]], mi)
            face([inner[k][i + 1], inner[k + 1][i + 1], inner[k + 1][i], inner[k][i]], mi)
    for i in range(seg):   # 上下缘封边
        face([rows[0][i], rows[0][i + 1], inner[0][i + 1], inner[0][i]], 0)
        face([rows[-1][i + 1], rows[-1][i], inner[-1][i], inner[-1][i + 1]], 1)
    for k in range(len(rows) - 1):   # 开口两端封边
        for e in (0, seg):
            face([rows[k][e], inner[k][e], inner[k + 1][e], rows[k + 1][e]] if e == 0 else [rows[k][e], rows[k + 1][e], inner[k + 1][e], inner[k][e]], 0 if levels[k + 1][2] == 'white' else 1)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    sh = new_obj('shell', bm, mats); parts.append(sh)
    hem = [(p[0] * 1.004, cz + (p[1] - cz) * 1.004, p[2]) for p in ring(y0, 0.02 + flare, wave)]
    parts.append(tube(hem, [0.006 + 0.01 * hw] * len(hem), 'gold', seg=6))
    gl = ring(y0 + 0.035 * hw / 0.5 + 0.012, 0.02 + flare * 0.82, wave)
    parts.append(tube([(p[0] * 1.01, cz + (p[1] - cz) * 1.01, p[2]) for p in gl], [0.004 + 0.012 * hw] * len(gl), 'glow', seg=6))   # c2: 加粗
    top = ring(y1, 0.014)
    parts.append(tube(top, [0.008] * len(top), 'gold', seg=6))
    if fin_len > 0:   # 后角耳羽鳍: 两片, 向后上外
        for sx in (1, -1):
            x, yy = superellipse(hw, hl, cz, math.radians(180 - 38 * sx))
            for k, (L, up) in enumerate([(fin_len, 38), (fin_len * 0.72, 22)]):
                f = feather(L, 0.11 + 0.03 * (1 - k), th=0.014, curve=-0.12, colname='feather_wp', tipsharp=0.45)
                # 立起(绕X 90, 羽面竖直) → 上翘 up → 指向后(−Y)并外摆 22°
                xform(f, loc=(x, yy, y1 - 0.02 - 0.03 * k), rot=(90, -up, -90 + 22 * sx), order='XYZ'); parts.append(f)
        o = octagram(0.06 * hw / 0.5 + 0.02, 0.008, 'gold'); xform(o, loc=(0, cz - hl - 0.005, (y0 + y1) / 2 + 0.02), rot=(90, 0, 0)); parts.append(o)
    return parts

# ---------- 炮塔装甲件 v2(T1) ----------
# v1 软弧罩壳的问题(炮塔审查 r31): 超椭圆环外扩+喇叭口 → 与炮塔本体有 0.05–0.12 间隙、比本体长 0.1–0.3(像托盘/漂壳);
# 下摆金边+发光线+顶金线一圈 → 边沿橙紫条纹; 后角耳羽鳍有被后炮塔炮管扫到的风险。
# v2: 由 geometry.glb 实测的炮塔平面轮廓(半宽 w(z), 已排除炮管/玻璃)生成贴合装甲板:
#     侧/后围板按段分件(分件缝)、顶沿内倒角; 白为主, 侧中板 periwinkle, 后板 violet; 金/发光只做后板徽记+短条点缀; 前方炮门全开。
TURRET_PLAN = {   # 半轮廓 (z 前向, 半宽 w), 从后中线 → 前炮门边; H=围板高(≈0.7 炮塔高); g=贴合间隙
    # T4: 倒角更大(ch↑)、板略厚(th↑ 仍向外)、缝更深; g 不变保贴合; 仍无前板/无整盒
    'FGM049': dict(pts=[(-0.600, 0.0), (-0.600, 0.30), (-0.565, 0.500), (0.080, 0.505), (0.180, 0.440), (0.300, 0.370), (0.385, 0.300)],
                   H=0.165, g=0.008, th=0.018, ch=0.028, panels=[None, 'white', 'peri', 'white', 'peri'], band=0.24,
                   roof=dict(x=0.27, y0=-0.53, y1=-0.14, z=0.243, th=0.011, gap=0.036, ch=0.018)),
    'FGS053': dict(pts=[(-0.235, 0.0), (-0.235, 0.07), (-0.185, 0.170), (-0.130, 0.215), (0.020, 0.235), (0.100, 0.210), (0.170, 0.160), (0.215, 0.105)],
                   H=0.125, g=0.006, th=0.014, ch=0.020, panels=[None, 'white', 'peri', 'white', 'peri'], band=0.24,
                   roof=dict(x=0.095, y0=-0.150, y1=0.012, z=0.172, th=0.008, gap=0.022, ch=0.012, accents=False), rear_glow=False),   # T3: FGS_11 炮管回转 50°/310° 扫到 FGS_10 顶板点缀 → 副炮顶板不加点缀
    'FGS036': dict(pts=[(-0.180, 0.0), (-0.180, 0.06), (-0.155, 0.120), (-0.100, 0.145), (0.100, 0.150), (0.145, 0.115), (0.165, 0.070)],
                   H=0.150, g=0.005, th=0.011, ch=0.018, y0=0.045, panels=[None, 'white', 'peri', 'white', 'peri'], band=0.24,
                   roof=None),   # 不加顶板: 后排副炮(FGS_4/6)静止时炮管离本炮塔顶仅 ~0.01, 顶板会被扫到(turret_check)   # y0: 前副炮朝后时炮管从后副炮围板下缘下方掠过
}

def _resample(poly, n):
    """折线按弧长等分 n 段, 返回 n+1 点。"""
    L = [0.0]
    for i in range(1, len(poly)): L.append(L[-1] + (Vector(poly[i]) - Vector(poly[i - 1])).length)
    out = []
    for k in range(n + 1):
        t = L[-1] * k / n; j = max(1, next((i for i in range(1, len(L)) if L[i] >= t), len(L) - 1))
        f = 0 if L[j] == L[j - 1] else (t - L[j - 1]) / (L[j] - L[j - 1])
        out.append(Vector(poly[j - 1]).lerp(Vector(poly[j]), f))
    return out, L[-1]

SEAL = float(os.environ.get("RB_SEAL", "0.006"))   # T3: 内缘插入炮塔本体的深度(封住上下沿间隙 → 港口 SSAO 不再压出黑缝)
def armor_panel(seg_pts, g, th, H, ch, col, y0=0.012, rivets=True):
    """一段局部装甲板(T4): seg_pts 平面折线(Blender X, Y), 法向外; 外面=轮廓+g+th; 上下沿内缘插入本体 SEAL(封缝);
    更大上/下倒角; 外表面凸框(边框抬高)制造分件立体; 可选几何铆钉; UV u=沿弧长, v=高(→ 图集 pnl_)。
    返回 [主面板, …铆钉] 列表(调用方一并 join)。"""
    bm = bmesh.new(); uvl = bm.loops.layers.uv.new('UVMap')
    n = len(seg_pts)
    nors = []
    for i in range(n):
        a = seg_pts[max(i - 1, 0)]; b = seg_pts[min(i + 1, n - 1)]
        t = (b - a).normalized(); nrm = Vector((t.y, -t.x)); nors.append(nrm)   # 右手: 沿逆时针走时外法向
    L = [0.0]
    for i in range(1, n): L.append(L[-1] + (seg_pts[i] - seg_pts[i - 1]).length)
    # T4: 倒角更大(下 0.65ch / 上 ch 全用), 外表面加一层凸框(step)
    cb = min(ch * 0.65, (H - y0) * 0.18)
    step = max(th * 0.28, 0.0035)   # 凸框外抬量(远距可读, 不胀到邻炮)
    rows = []; uvs = []
    for i, p in enumerate(seg_pts):
        nn = nors[i]
        pin = p - nn * SEAL
        pout = p + nn * (g + th)
        pmid = p + nn * (g + th - cb)
        # 凸框外缘(外抬 step); 中段内凹区仍在 pout
        pframe = p + nn * (g + th + step)
        # 截面 8 点: 内底 / 下外起点 / 下倒角外 / 凸框下沿 / 凸框上沿 / 上倒角起点 / 上倒角顶 / 内顶
        z_lo = y0 + cb; z_hi = H - ch
        z_f0 = z_lo + 0.10 * (z_hi - z_lo); z_f1 = z_hi - 0.10 * (z_hi - z_lo)
        rows.append([
            bm.verts.new((pin.x, pin.y, y0)),
            bm.verts.new((pmid.x, pmid.y, y0)),
            bm.verts.new((pout.x, pout.y, z_lo)),
            bm.verts.new((pframe.x, pframe.y, z_f0)),
            bm.verts.new((pframe.x, pframe.y, z_f1)),
            bm.verts.new((pout.x, pout.y, z_hi)),
            bm.verts.new((pout.x - nn.x * ch * 0.75, pout.y - nn.y * ch * 0.75, H)),
            bm.verts.new((pin.x, pin.y, H)),
        ])
        uvs.append(L[i] / max(L[-1], 1e-6))
    hv = [0.0, 0.0, cb, z_f0 - y0, z_f1 - y0, H - y0 - ch, H - y0, H - y0]
    vv = [min(max(h / max(H - y0, 1e-6), 0), 1) for h in hv]
    m = len(rows[0])
    for i in range(n - 1):
        for k in range(m):
            k2 = (k + 1) % m
            f = bm.faces.new([rows[i][k], rows[i + 1][k], rows[i + 1][k2], rows[i][k2]])
            for lp in f.loops:
                j = 0 if lp.vert in rows[i] else 1; kk = (rows[i] if j == 0 else rows[i + 1]).index(lp.vert)
                lp[uvl].uv = (uvs[i + j], vv[kk])
    for r in (rows[0], rows[-1][::-1]):
        f = bm.faces.new(r)
        for lp in f.loops: lp[uvl].uv = (0.5, 0.5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    out = [new_obj('armor', bm, ['pnl_' + col])]
    # T4 几何铆钉: 上/下凸框各一排, 沿弧长等距; 半径随板厚, 略外露
    if rivets and L[-1] > 1e-4:
        rr = max(0.0022, min(0.0045, th * 0.28))
        n_riv = max(3, min(9, int(L[-1] / 0.055) + 1))
        for tr in (0.14, 0.86):
            zz = y0 + tr * (H - y0)
            for k in range(n_riv):
                t = (0.12 + 0.76 * k / max(n_riv - 1, 1))
                # 沿折线插值位置
                target = t * L[-1]; j = 1
                while j < len(L) and L[j] < target: j += 1
                f = 0 if L[j] == L[j - 1] else (target - L[j - 1]) / (L[j] - L[j - 1])
                p = seg_pts[j - 1].lerp(seg_pts[j], f)
                nn = nors[min(j, n - 1)].lerp(nors[max(j - 1, 0)], 1 - f).normalized()
                c = p + nn * (g + th + step + rr * 0.55)
                o = sphere(rr, col=col, seg=8)
                xform(o, loc=(c.x, c.y, zz)); out.append(o)
    return out

def turret_armor(pts, H, g, th, ch, panels, seam=0.022, seg_per=6, y0=0.012):
    """整圈围板: 右半轮廓镜像成 左炮门边→后→右炮门边, 按 panels(从后中向前, 左右对称)分件, 件间留缝。
    T4: 默认缝 0.022(更深分件可读)。"""
    right = [Vector((w, z)) for z, w in pts]                       # 后中 → 右前
    left = [Vector((-v.x, v.y)) for v in reversed(right)]          # 左前 → 后中
    loop = left[:-1] + right                                       # 左前 → 后 → 右前(逆时针: 外法向向外)
    npan = len(panels) * 2 - 1                                     # 后中一件居中
    pts_all, total = _resample(loop, npan * seg_per)
    parts = []
    # 颜色从前到后: 前侧板→…→后中板(panels[-1]=后)
    seq = panels[:-1] + [panels[-1]] + list(reversed(panels[:-1]))
    for k in range(npan):
        seg = pts_all[k * seg_per:(k + 1) * seg_per + 1]
        # 缝: 两端各收 seam/2
        a0 = seg[0] + (seg[1] - seg[0]).normalized() * seam * 0.5; a1 = seg[-1] + (seg[-2] - seg[-1]).normalized() * seam * 0.5
        seg = [a0] + seg[1:-1] + [a1]
        pc = seq[k]
        if pc is None: continue   # T3: 前侧不做板(保留原炮塔前脸细节)
        col, hk = (pc if isinstance(pc, tuple) else (pc, 1.0))
        parts.extend(armor_panel(seg, g, th, H * hk, ch, col, y0=y0))
    return parts, pts_all

def roof_plate(x, y0, y1, z, th, gap, ch, col='white', accents=True):
    """顶板: 左右两块倒角薄板, 中缝 gap 露出贴图的 royal 中带; 四角斜切, 上表面内收 ch(倒角)。"""
    out = []
    for sx in (1, -1):
        xa, xb = gap / 2, x; c = min(ch * 2.5, (xb - xa) * 0.3)
        P = [(xa, y0), (xb - c, y0), (xb, y0 + c), (xb, y1 - c), (xb - c, y1), (xa, y1)]
        bm = bmesh.new(); uvl = bm.loops.layers.uv.new('UVMap')
        cx = (xa + xb) / 2; cy = (y0 + y1) / 2
        lo = [bm.verts.new((sx * px, py, z)) for px, py in P]
        hi = [bm.verts.new((sx * (px + (cx - px) * ch / max(1e-6, abs(cx - px) + (xb - xa) / 2)), py + (cy - py) * ch / max(1e-6, abs(cy - py) + (y1 - y0) / 2), z + th)) for px, py in P]
        n = len(P); fs = [bm.faces.new(lo[::-1]), bm.faces.new(hi)]
        for i in range(n): fs.append(bm.faces.new([lo[i], lo[(i + 1) % n], hi[(i + 1) % n], hi[i]]))
        for f in fs:
            for lp in f.loops: lp[uvl].uv = (0.5, 0.5)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        for f in fs:   # T3: 顶面走面板细节区(倒角明暗/铆钉), uv 按平面坐标
            for lp in f.loops: lp[uvl].uv = ((abs(lp.vert.co.x) - xa) / max(xb - xa, 1e-6), (lp.vert.co.y - y0) / max(y1 - y0, 1e-6))
        out.append(new_obj('roof', bm, ['pnl_' + col]))
        # T3 点缀: 每块后部小金八芒星 + 后沿短发光缝(不沿边整圈)
        if not accents: continue
        s_ = (xb - xa)
        o = octagram(0.16 * s_, 0.0035, 'gold'); xform(o, loc=(sx * (xa + xb) / 2, y0 + 0.22 * (y1 - y0), z + th + 0.0015)); out.append(o)
        out.append(tube([(sx * (xa + 0.25 * s_), y0 + 0.06 * (y1 - y0), z + th * 0.9), (sx * (xa + 0.75 * s_), y0 + 0.06 * (y1 - y0), z + th * 0.9)], [0.0035 + 0.01 * s_] * 2, 'glow', seg=6))
    return out

def build_shells():
    out = []
    for k, prm in TURRET_PLAN.items():
        pts = prm['pts']; H = prm['H']; g = prm['g']; th = prm['th']; ch = prm['ch']
        y0 = max(prm.get('y0', 0.012), float(os.environ.get('RB_BAND', prm.get('band', 0.0))) * H)   # T3: 只做中上带(下沿抬离炮座/后悬, 露出原炮塔下部细节)
        ps, loop = turret_armor(pts, H, g, th, ch, prm['panels'], y0=y0)
        if prm.get('roof'): ps += roof_plate(**prm['roof'])
        rz = pts[0][0] - g - th                          # 后板外表面 z
        s_ = (pts[2][1] if k == 'FGM049' else pts[3][1])  # 尺度参考
        # 后板徽记: 金八芒星 + 下方短发光缝(少而精)
        o = octagram(0.10 * s_ + 0.012, 0.004 + 0.004 * s_, 'gold'); xform(o, loc=(0, rz - 0.003, H * 0.55), rot=(90, 0, 0)); ps.append(o)
        if prm.get('rear_glow', True): gl = tube([(-0.18 * s_, rz - 0.004, y0 + 0.22 * (H - y0)), (0.18 * s_, rz - 0.004, y0 + 0.22 * (H - y0))], [0.006 + 0.006 * s_] * 2, 'glow', seg=6); ps.append(gl)  # P6: 后板发光缝加粗
        # 侧中板(peri)上沿短金条: 每侧一段, 只在该板中部 60%
        for sx in (1, -1):
            zc = 0.5 * (pts[3][0] + pts[4][0]) if len(pts) > 4 else pts[3][0]
            wz = max(w for z, w in pts) + g + th * 0.6
            L = 0.18 * (pts[-1][0] - pts[0][0])
            gt = tube([(sx * wz, zc - L / 2, H - ch * 0.5), (sx * wz, zc + L / 2, H - ch * 0.5)], [0.003 + 0.002 * s_] * 2, 'gold', seg=5); ps.append(gt)
            # T4: 侧板上沿发光短缝(克制但远距可读; 略细于 P6)
            gls = tube([(sx * (wz + 0.004), zc - L / 2, H - ch * 0.35), (sx * (wz + 0.004), zc + L / 2, H - ch * 0.35)], [0.0035 + 0.0025 * s_] * 2, 'glow', seg=5); ps.append(gls)
        out.append(join(ps, 'shell_' + k))
    return out

# ---------- 批 C: 艏翼饰 / 中桅大麦克 / 烟囱缎带罩 / 艉披风罩 ----------
def frame_place(parts, loc, ax_x, ax_y):
    """局部 X→ax_x, 局部 Y→ax_y(Blender 坐标, 自动正交化), Z=X×Y。"""
    X = Vector(ax_x).normalized(); Y = Vector(ax_y); Y = (Y - X * X.dot(Y)).normalized(); Z = X.cross(Y)
    M = Matrix(((X.x, Y.x, Z.x, loc[0]), (X.y, Y.y, Z.y, loc[1]), (X.z, Y.z, Z.z, loc[2]), (0, 0, 0, 1)))
    for p in parts: p.matrix_world = M @ p.matrix_world
    return parts

WING_TK = float(os.environ.get('ROBIN_WING_TK', '0.45'))
def wing_fan(root, S, d0, up, n_pri=3, n_sec=1, n_cov=1, glow_k=0):
    """主翼羽片语言的缩放小翼(同一生成器 plume、同厚宽比/弯弧/收尖/分层/金前缘/发光羽轴):
    root 翼根, S=相对主翼的尺度, d0=(el, sw) 基准方向(右舷), up=前缘朝向提示。
    初级 n_pri(白→长, 第 glow_k 片发光羽轴) + 次级 n_sec(periwinkle, 叠下) + 覆羽 n_cov(圆头白, 盖羽根)。"""
    P = []; el0, sw0 = d0; R = Vector(root)
    for k in range(n_pri):   # 主翼初级 L 1.45–2.50, W≈0.30, th 0.055·TK, curve 0.13
        L = (2.2 - 0.35 * k) * S; W = 0.30 * 0.9 * (S ** 0.5) * 1.0
        b = plume(L, W, 0.055 * WING_TK * (W / 0.27), 'feather_wp', curve=0.13, glow=True)   # P5: 与主翼同, 全部初级发光羽轴
        P += aim(b, tuple(R + Vector((0, -0.04 * k * S, 0.02 * k * S))), dvec(el0 - 7 * k, sw0 + 8 * k), up) or b
    for k in range(n_sec):
        L = 1.2 * S; W = 0.27 * (S ** 0.5)
        b = plume(L, W, 0.05 * WING_TK * (W / 0.27), 'feather_pv', curve=0.12, edge=True)
        P += aim(b, tuple(R + Vector((0, -0.04 * S, -0.06 * S))), dvec(el0 - 18 - 8 * k, sw0 + 22 + 8 * k), up) or b
    for k in range(n_cov):
        W = 0.26 * (S ** 0.5)
        f = plume(0.72 * S, W, 0.06 * WING_TK * (W / 0.26), 'white', curve=0.08, edge=False, shaft=False, round_tip=True)
        P += aim(f, tuple(R + Vector((0, 0.02 * S, 0.05 * S))), dvec(el0 - 6, sw0 + 14), up) or f
    return P

def build_bow():
    """艏饰: 与主翼同一造型语言 wing_fan(plume); P5.1 尺度 S=0.40, 无肩甲(保持悬浮语系)。"""
    S = float(os.environ.get('ROBIN_BOW_S', '0.40'))
    P = wing_fan((0.14, 8.24, 0.66), S, (30, 70), (0, 0.5, 1.0), n_pri=3, n_sec=1, n_cov=1)
    for p in P: p.select_set(False)
    r = join(P, 'bowp_R'); l = mirror_x(r, 'bowp_L')
    o = octagram(0.16, 0.02, 'gold'); xform(o, loc=(0, 8.78, 0.72), rot=(90, 0, 0))
    g = torus(0.21, 0.012, 64, 8, 'glow'); xform(g, loc=(0, 8.77, 0.72), rot=(90, 0, 0))
    t = torus(0.19, 0.022, 64, 8, 'gold'); xform(t, loc=(0, 8.775, 0.72), rot=(90, 0, 0))
    return [join([r, l, o, g, t], 'bow_crest')]


def build_mic():
    """中桅大麦克(复古演唱会话筒, 程序化, 非 PMX): 网罩头+金箍+白柄+发光环, 立在主桅顶 (游戏 0, ~3.0, −1.45), 前倾 12°。"""
    objs = []
    R = 0.09   # L1: 大麦克球头突兀, 缩成桅顶小饰
    head = sphere(R, 'white', 32); xform(head, loc=(0, 0, 0.42), scale=(1, 1, 1.2)); objs.append(head)
    for k, zz in enumerate([-0.10, -0.03, 0.04, 0.11]):   # 网罩纹: 横环 periwinkle
        rr = R * math.sqrt(max(0.05, 1 - (zz / (R * 1.18)) ** 2)) + 0.004
        t = torus(rr, 0.006, 64, 6, 'peri'); xform(t, loc=(0, 0, 0.42 + zz)); objs.append(t)
    for k in range(8):   # 竖向网罩筋(金), 读成话筒网罩
        a = math.pi * k / 8
        pts = [(R * 1.02 * math.sin(t) * math.cos(a), R * 1.02 * math.sin(t) * math.sin(a), 0.42 + R * 1.2 * math.cos(t)) for t in [math.pi * i / 16 for i in range(17)]]
        objs.append(tube(pts, [0.007] * 17, 'gold', seg=5))
    band = torus(R * 0.98, 0.026, 64, 10, 'gold'); xform(band, loc=(0, 0, 0.30)); objs.append(band)
    gl = torus(R * 1.05, 0.008, 64, 6, 'glow'); xform(gl, loc=(0, 0, 0.27)); objs.append(gl)
    body = tube([(0, 0, 0.29), (0, 0, 0.18), (0, 0, 0.05), (0, 0, -0.10)], [0.105, 0.09, 0.075, 0.07], 'white', seg=24); objs.append(body)
    ring = torus(0.078, 0.014, 48, 8, 'gold'); xform(ring, loc=(0, 0, -0.02)); objs.append(ring)
    o = octagram(0.07, 0.01, 'gold'); xform(o, loc=(0, 0.095, 0.14), rot=(90, 0, 0)); objs.append(o)
    m = join(objs, 'mic')
    m.matrix_world = Matrix.Translation((0, -1.45, 2.78)) @ Matrix.Rotation(math.radians(-10), 4, 'X')
    bpy.ops.object.select_all(action='DESELECT'); m.select_set(True); bpy.context.view_layer.objects.active = m
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    return [m]

def build_funnel():
    """烟囱缎带罩: 绕烟囱/桅塔(游戏 z −1.15..−2.3, x ±0.36)一圈缎带(上下金边+发光线), 前方蝴蝶结, 后方两条飘带拖向艉下。"""
    objs = []
    hw, hl, cz = 0.40, 0.58, -1.72
    seg = 72; y0, y1 = 1.68, 1.84   # L1: 更矮更贴烟囱
    ths = [2 * math.pi * i / seg for i in range(seg + 1)]
    def ring(yy, off):
        return [(*superellipse(hw + off, hl + off, cz, th, n=3.0), yy) for th in ths]
    bm = bmesh.new(); uvl = bm.loops.layers.uv.new('UVMap')
    lo = [bm.verts.new(p) for p in ring(y0, 0.03)]; hi = [bm.verts.new(p) for p in ring(y1, 0.0)]
    li = [bm.verts.new(p) for p in ring(y0, 0.015)]; hi2 = [bm.verts.new(p) for p in ring(y1, -0.015)]
    for i in range(seg):
        for vs in ([lo[i], lo[i + 1], hi[i + 1], hi[i]], [li[i + 1], li[i], hi2[i], hi2[i + 1]], [hi[i], hi[i + 1], hi2[i + 1], hi2[i]], [lo[i + 1], lo[i], li[i], li[i + 1]]):
            f = bm.faces.new(vs)
            for lp in f.loops: lp[uvl].uv = (0.5, 0.5)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-6); bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    objs.append(new_obj('band', bm, ['peri']))
    for yy, off, rr, col in [(y0, 0.035, 0.014, 'gold'), (y1, 0.006, 0.012, 'gold'), ((y0 + y1) / 2, 0.022, 0.006, 'glow')]:
        objs.append(tube(ring(yy, off), [rr] * (seg + 1), col, seg=6))
    # 蝴蝶结(前): 两个羽形环瓣 + 金结
    fx, fy = superellipse(hw + 0.03, hl + 0.03, cz, 0.0, n=3.0)
    for sx in (1, -1):
        f = feather(0.30, 0.20, th=0.03, curve=0.0, colname='feather_pv', tipsharp=0.6)
        xform(f, loc=(0, fy + 0.02, (y0 + y1) / 2), rot=(90, 0, 0 if sx > 0 else 180)); objs.append(f)
    k = sphere(0.05, 'gold', 16); xform(k, loc=(0, fy + 0.03, (y0 + y1) / 2)); objs.append(k)
    # 飘带(后): 两条从后缘拖向后下, 宽 0.12, 波浪
    bx, by = superellipse(hw + 0.03, hl + 0.03, cz, math.pi, n=3.0)
    for sx in (1, -1):
        pts = []
        for i in range(13):
            t = i / 12
            pts.append((sx * (0.08 + 0.25 * t + 0.03 * math.sin(5 * t)), by - 0.9 * t, y0 + 0.05 - 0.55 * t ** 1.3))
        objs += ribbon(pts, 0.13, 'peri', hem='gold')
    return [join(objs, 'funnel')]

def ribbon(pts, width, col, hem=None, up=(0, 0, 1), taper=True):
    """沿折线的带状薄片(两面), 宽向 = up 方向, 末端尖。返回零件列表(带面 + 下缘金边)。"""
    P = [Vector(p) for p in pts]; U = Vector(up)
    bm = bmesh.new(); uvl = bm.loops.layers.uv.new('UVMap'); top, bot = [], []
    n = len(P)
    for i, p in enumerate(P):
        t = i / (n - 1); w = width * ((1 - t ** 3) if taper else 1.0) + 0.003
        d = (P[min(i + 1, n - 1)] - P[max(i - 1, 0)]).normalized(); nn = d.cross(U).normalized() * 0.006
        top.append((bm.verts.new(p + nn), bm.verts.new(p - nn))); bot.append((bm.verts.new(p - U * w + nn), bm.verts.new(p - U * w - nn)))
    for i in range(n - 1):
        for a, b_ in ((0, 0), (1, 1)):
            vs = [top[i][a], top[i + 1][a], bot[i + 1][b_], bot[i][b_]]
            if a == 1: vs = vs[::-1]
            f = bm.faces.new(vs)
            for lp in f.loops: lp[uvl].uv = (0.5, 0.5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    out = [new_obj('ribbon', bm, [col])]
    if hem:
        hp = [P[i] - U * (width * ((1 - (i / (n - 1)) ** 3) if taper else 1.0) + 0.003) for i in range(n)]
        out.append(tube(hp, [0.010 * (1 - 0.7 * i / (n - 1)) for i in range(n)], hem, seg=6))
    return out

def build_cape():
    """艉披风罩(c2): 两幅披风贴艉舷外侧(离舷 0.03), 从甲板边(z −6.3)垂下、向艉渐宽, 过艉封板后收成燕尾尖(到 z −9.35);
    两面 periwinkle/紫(c1 白内衬从艉视角成大白刀片), 下摆金边+发光线, 上缘白滚边, 中段金八芒星。避开 FGM_3 回转区。"""
    HB = [(-6.0, 0.84), (-6.5, 0.79), (-7.0, 0.72), (-7.5, 0.64), (-8.0, 0.55), (-8.4, 0.44), (-8.7, 0.30), (-8.95, 0.22)]
    def hb(z):
        if z >= HB[0][0]: return HB[0][1]
        for (z0, b0), (z1, b1) in zip(HB, HB[1:]):
            if z1 <= z <= z0: return b0 + (b1 - b0) * (z0 - z) / (z0 - z1)
        return None
    objs = []; n = 28; sx = 1
    pts, wid = [], []
    for i in range(n + 1):
        t = i / n; z = -6.3 - 3.05 * t
        h = hb(z)
        if h is None:   # 过艉: 沿舷线外推后收尖
            u = (-8.95 - z) / 0.40; x = 0.22 + 0.06 * u
        else: x = h + 0.03
        yy = 0.47 - 0.02 * t - (0.12 * ((-8.95 - z) / 0.40) if z < -8.95 else 0)
        pts.append((sx * x, z, yy))
        wid.append(0.05 + 0.20 * math.sin(math.pi * min(t / 0.80, 1.0) * 0.5) - (0.22 * (t - 0.80) / 0.20 if t > 0.80 else 0))
    bm = bmesh.new(); uvl = bm.loops.layers.uv.new('UVMap'); rows = []
    for p, w in zip(pts, wid):
        w = max(w, 0.006); o = Vector((sx * 0.010, 0, 0)); P = Vector(p); D = Vector((0, 0, w))
        rows.append([bm.verts.new(P + o), bm.verts.new(P - o), bm.verts.new(P - D + o), bm.verts.new(P - D - o)])
    for i in range(n):
        a, b_ = rows[i], rows[i + 1]
        for vs, mi in (([a[0], a[2], b_[2], b_[0]], 0), ([a[1], b_[1], b_[3], a[3]], 1), ([a[0], b_[0], b_[1], a[1]], 0), ([a[2], a[3], b_[3], b_[2]], 0)):
            f = bm.faces.new(vs); f.material_index = mi
            for lp in f.loops: lp[uvl].uv = (0.5, 0.5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    hem = [(r[2].co.x + sx * 0.004, r[2].co.y, r[2].co.z) for r in rows]
    objs.append(new_obj('cape', bm, ['peri', 'violet']))
    objs.append(tube(hem, [0.012] * len(hem), 'gold', seg=6))
    objs.append(tube([(h[0] + sx * 0.006, h[1], h[2] + 0.03) for h in hem[:-3]], [0.008] * (len(hem) - 3), 'glow', seg=6))
    objs.append(tube([(p[0] + sx * 0.004, p[1], p[2]) for p in pts], [0.011] * len(pts), 'white', seg=6))
    m = n // 2
    o = octagram(0.075, 0.010, 'gold'); xform(o, loc=(pts[m][0] + sx * 0.02, pts[m][1], pts[m][2] - wid[m] * 0.5), rot=(0, 90 * sx, 0)); objs.append(o)
    r = join(objs, 'cape_R'); l = mirror_x(r, 'cape_L')
    return [r, l]

def preview(name, objs):
    """工作台引擎快速预览(俯+侧), 只看道具本身。"""
    scn = bpy.context.scene
    scn.render.engine = 'BLENDER_WORKBENCH'; scn.display.shading.color_type = 'MATERIAL'
    scn.render.resolution_x, scn.render.resolution_y = 900, 600
    cam = bpy.data.objects.get('cam') or bpy.data.objects.new('cam', bpy.data.cameras.new('cam'))
    if cam.name not in scn.objects: scn.collection.objects.link(cam)
    scn.camera = cam
    for tag, loc, rot in [('top', (0, -1, 12), (0, 0, 0)), ('qtr', (7, 5, 5), (62, 0, 125)), ('side', (12, -1, 2.2), (88, 0, 90))]:
        cam.location = loc; cam.rotation_euler = [math.radians(a) for a in rot]
        scn.render.filepath = os.path.join(OUT, f'{name}_{tag}.png'); bpy.ops.render.render(write_still=True)

BUILD = {'wings': build_wings, 'halo': build_halo, 'shells': build_shells, 'bow': build_bow, 'mic': build_mic, 'funnel': build_funnel, 'cape': build_cape}
if __name__ == '__main__':
    os.makedirs(OUT, exist_ok=True); report = []
    for key, fn in BUILD.items():
        if WANT - {'preview'} and key not in WANT: continue
        reset(); objs = fn()
        if 'preview' in WANT: preview(key, objs)
        for o in objs:
            report.append(export([o], 'rb_' + o.name))
    json.dump(report, open(os.path.join(OUT, 'props_report.json'), 'w'), indent=1)
    print('PROPS_REPORT', json.dumps(report))
