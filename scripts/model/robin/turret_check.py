"""炮塔外带件自查(box, 无游戏): 贴合 / 炮管仰角 / 回转碰撞。
blender -b --factory-startup -P turret_check.py -- <ship geometry.glb> <props_dir> [out.json]
- 壳为炮塔局部(Blender X=x, Y=前, Z=上), 按挂点放置; 炮塔网格取自 geometry.glb 的 HP_* 对象(含炮管)。
- 炮管 = Gun_Barrel 材质(FGS053 无独立炮管材质: 取 |x|<0.09 且 z>0.22 的顶点)。俯仰轴 = 炮管最后端 z、炮管平均高度。
- 有 WOWSMOD_GP 时只查射界内的回转角(horizSector)与最大仰角(vertSector)。
- 只记「壳独有」碰撞: 同姿态下原炮塔本体(含炮管)也撞的, 视为游戏射界/回转限制本就禁止, 不计。
- 检查: A 自身炮管 0..最大仰角 vs 自身壳; B 贴合(壳内表面到炮塔本体距离)与外伸; C 回转 0..350°/10° 时
  壳 vs 船体静态分段、vs 邻近炮塔(本体+炮管, 取 0°/同向/180°)及其壳; 邻炮管扫过本壳。"""
import bpy, sys, json, math
from mathutils import Vector, Matrix
from mathutils.bvhtree import BVHTree
a = sys.argv[sys.argv.index('--') + 1:]; SHIP, PROPS = a[0], a[1]; OUTJ = a[2] if len(a) > 2 else None
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=SHIP)
objs = [o for o in bpy.context.scene.objects if o.type == 'MESH']
MAXEL = {'FGM049': 35, 'FGS053': 75, 'FGS036': 80}
# GameParams 射界(可选, WOWSMOD_GP=船条目 JSON): horizSector 为相对静止朝向的角度, 游戏角 = −Blender 偏航(负=左舷侧)
import os
SECT = {}; VMAX = {}
gp = os.environ.get('WOWSMOD_GP')
if gp and os.path.exists(gp):
    def _walk(o):
        if isinstance(o, dict):
            for k, v in o.items():
                if k.startswith('HP_') and isinstance(v, dict) and 'horizSector' in v and k not in SECT:
                    SECT[k] = v['horizSector']; VMAX[k] = (v.get('vertSector') or [0, None])[1]
                _walk(v)
    _walk(json.load(open(gp)))
def allowed(hp, yaw):
    if hp not in SECT: return True
    a = ((-yaw + 180) % 360) - 180; lo, hi = SECT[hp]
    return lo - 1e-6 <= a <= hi + 1e-6
def kind(n):
    for k in MAXEL:
        if k in n: return k
TUR = {}
for o in objs:
    if o.name.startswith('HP_'):
        k = kind(o.name)
        if not k: continue
        me = o.data; mw = o.matrix_world; org = mw.translation.copy()
        bar = set()
        for p in me.polygons:
            mn = me.materials[p.material_index].name
            if 'Barrel' in mn: bar.update(p.vertices)
        V = [mw @ v.co for v in me.vertices]
        # 初始朝向: 炮管质心在挂点前(+Y)还是后
        bi = bar or {i for i, v in enumerate(V) if abs(v.x - org.x) < 0.09 and abs(v.y - org.y) > 0.22}
        cy = sum(V[i].y for i in bi) / max(1, len(bi)) - org.y
        yaw0 = 0.0 if cy > 0 else 180.0
        # 本地(相对挂点, 去掉初始朝向)
        R0 = Matrix.Rotation(math.radians(-yaw0), 3, 'Z')
        L = [R0 @ (v - org) for v in V]
        if not bar:   # FGS053: 无独立炮管材质
            bar = {i for i, v in enumerate(L) if abs(v.x) < 0.09 and v.y > 0.22}
        polys = [tuple(p.vertices) for p in me.polygons]
        TUR[o.name.split('|')[0]] = dict(k=k, org=org, yaw0=yaw0, L=L, bar=bar, polys=polys)
STATIC = [o for o in objs if not o.name.startswith('HP_')]
def bvh_world(verts, polys): return BVHTree.FromPolygons(verts, polys)
SB = []
for o in STATIC:
    mw = o.matrix_world; SB.append((o.name, bvh_world([mw @ v.co for v in o.data.vertices], [tuple(p.vertices) for p in o.data.polygons])))
SH = {}
for k in MAXEL:
    bpy.ops.import_scene.gltf(filepath=f'{PROPS}/rb_shell_{k}.glb')
    ob = [o for o in bpy.context.selected_objects if o.type == 'MESH'][0]
    SH[k] = ([ob.matrix_world @ v.co for v in ob.data.vertices], [tuple(p.vertices) for p in ob.data.polygons])
    bpy.data.objects.remove(ob)
def place(L, org, yaw):
    R = Matrix.Rotation(math.radians(yaw), 3, 'Z'); return [R @ v + org for v in L]
def turret_world(t, yaw, pitch=0.0):
    L = t['L']; bar = t['bar']
    if pitch and bar:
        bz = min(L[i].y for i in bar); by = sum(L[i].z for i in bar) / len(bar)
        Rp = Matrix.Rotation(math.radians(pitch), 3, 'X'); piv = Vector((0, bz, by))
        L = [(Rp @ (v - piv) + piv) if i in bar else v for i, v in enumerate(L)]
    return place(L, t['org'], yaw)
res = {'turrets': {}}
for hp, t in sorted(TUR.items()):
    k = t['k']; sv, sp = SH[k]; r = {}
    # B 贴合: 壳顶点到本体(去炮管)最近距离
    body_idx = [i for i in range(len(t['L'])) if i not in t['bar']]
    bodyP = [p for p in t['polys'] if not any(i in t['bar'] for i in p)]
    bt = bvh_world(t['L'], bodyP)
    d = sorted(bt.find_nearest(v)[3] for v in sv)
    r['fit_gap_median'] = round(d[len(d) // 2], 4); r['fit_gap_p90'] = round(d[int(len(d) * 0.9)], 4)
    bx = max(abs(t['L'][i].x) for i in body_idx); bzr = min(t['L'][i].y for i in body_idx)
    r['overhang_x'] = round(max(abs(v.x) for v in sv) - bx, 4); r['overhang_rear'] = round(bzr - min(v.y for v in sv), 4)
    # A 自身炮管仰角
    sb_loc = bvh_world(sv, sp); hitA = []
    for el in range(0, int(VMAX.get(hp) or MAXEL[k]) + 1, 5):
        Lw = turret_world(dict(t, org=Vector((0, 0, 0))), 0.0, el)
        bp = [p for p in t['polys'] if all(i in t['bar'] for i in p)]
        if not bp: bp = [p for p in t['polys'] if any(i in t['bar'] for i in p)]
        if sb_loc.overlap(bvh_world(Lw, bp)): hitA.append(el)
    r['barrel_elev_hits'] = hitA
    # C 回转: 壳 vs 静态分段 + 邻炮塔
    hitS = {}; hitN = {}
    nb = [h for h, u in TUR.items() if h != hp and (u['org'] - t['org']).length < 2.6]
    body_polys = bodyP
    for yaw in range(0, 360, 10):
        if not allowed(hp, yaw): continue
        shw = place(sv, t['org'], t['yaw0'] + yaw); B = bvh_world(shw, sp)
        Bb = bvh_world(turret_world(t, t['yaw0'] + yaw), t['polys'])   # 基线: 原炮塔本体(含炮管)同姿态
        for n, sbv in SB:
            if B.overlap(sbv) and not Bb.overlap(sbv): hitS.setdefault(n, []).append(yaw)   # 只记「壳独有」
        for h in nb:
            u = TUR[h]
            for uy in (0, yaw, 180, -yaw):
                if not allowed(h, uy): continue
                tw = turret_world(u, u['yaw0'] + uy); TB = bvh_world(tw, u['polys'])
                if B.overlap(TB) and not Bb.overlap(TB): hitN.setdefault(h, set()).add((yaw, uy))
                usv, usp = SH[u['k']]
                if B.overlap(bvh_world(place(usv, u['org'], u['yaw0'] + uy), usp)): hitN.setdefault(h + '_shell', set()).add((yaw, uy))
    r['static_hits'] = {n: v for n, v in hitS.items()}
    r['neighbor_hits'] = {n: sorted(v)[:12] + (['...'] if len(v) > 12 else []) for n, v in hitN.items()}
    r['neighbor_hit_counts'] = {n: len(v) for n, v in hitN.items()}
    res['turrets'][hp] = r
    print('TC', hp, k, json.dumps(r, ensure_ascii=False, default=list)[:600])
if OUTJ: json.dump(res, open(OUTJ, 'w'), ensure_ascii=False, indent=1, default=list)
