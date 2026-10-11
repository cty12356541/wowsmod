"""炮塔转塔/仰角姿态图 (box 预览装配几何无 yaw/pitch 骨骼, render turret= 不生效时用)。
blender -b --factory-startup -P turret_pose_render.py -- <geometry.glb> <props_dir> <outdir>
每类炮塔: 静止 / 转 90°(射界内一侧)+最大仰角, 壳按炮塔局部同姿态放置。Workbench 贴图色, 船体灰模。
env WOWSMOD_GP: 射界与最大仰角。"""
import bpy, sys, os, json, math
from mathutils import Vector, Matrix
a = sys.argv[sys.argv.index('--') + 1:]; SHIP, PROPS, OUT = a[0], a[1], a[2]
os.makedirs(OUT, exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=SHIP)
MAXEL = {'FGM049': 35, 'FGS053': 75, 'FGS036': 80}
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
    g = ((-yaw + 180) % 360) - 180; lo, hi = SECT[hp]; return lo - 1e-6 <= g <= hi + 1e-6
def kind(n):
    for k in MAXEL:
        if k in n: return k
grey = bpy.data.materials.new('clay'); grey.diffuse_color = (0.80, 0.80, 0.82, 1)
dark = bpy.data.materials.new('barrel'); dark.diffuse_color = (0.25, 0.28, 0.45, 1)
TUR = {}
for o in list(bpy.context.scene.objects):
    if o.type != 'MESH': continue
    o.data = o.data.copy(); me = o.data   # glTF 实例共享网格: 先复制再改顶点
    bar_m = [i for i, m in enumerate(me.materials) if m and 'Barrel' in m.name]
    me.materials.clear(); me.materials.append(grey); me.materials.append(dark)
    for p in me.polygons: p.material_index = 1 if p.material_index in bar_m else 0
    k = kind(o.name) if o.name.startswith('HP_') else None
    if not k: continue
    mw = o.matrix_world.copy(); org = mw.translation.copy()
    V = [mw @ v.co for v in me.vertices]
    bar = {p_v for p in me.polygons if p.material_index == 1 for p_v in p.vertices}
    bi = bar or {i for i, v in enumerate(V) if abs(v.x - org.x) < 0.09 and abs(v.y - org.y) > 0.22}
    cy = sum(V[i].y for i in bi) / max(1, len(bi)) - org.y
    yaw0 = 0.0 if cy > 0 else 180.0
    R0 = Matrix.Rotation(math.radians(-yaw0), 3, 'Z'); L = [R0 @ (v - org) for v in V]
    if not bar: bar = {i for i, v in enumerate(L) if abs(v.x) < 0.09 and v.y > 0.22}
    for p in me.polygons: p.material_index = 1 if all(i in bar for i in p.vertices) else 0
    o.matrix_world = Matrix.Identity(4)
    TUR[o.name.split('|')[0]] = dict(o=o, k=k, org=org, yaw0=yaw0, L=L, bar=bar)
SH = {}
for k in MAXEL:
    bpy.ops.import_scene.gltf(filepath=f'{PROPS}/rb_shell_{k}.glb')
    SH[k] = [o for o in bpy.context.selected_objects if o.type == 'MESH'][0]; SH[k].hide_render = True
SHELLS = {}
for hp, t in TUR.items():
    c = SH[t['k']].copy(); c.data = SH[t['k']].data; c.hide_render = False
    bpy.context.scene.collection.objects.link(c); SHELLS[hp] = c
def pose(hp, rel, el):
    t = TUR[hp]; L = t['L']; bar = t['bar']
    if el and bar:
        bz = min(L[i].y for i in bar); by = sum(L[i].z for i in bar) / len(bar)
        Rp = Matrix.Rotation(math.radians(el), 3, 'X'); piv = Vector((0, bz, by))
        L = [(Rp @ (v - piv) + piv) if i in bar else v for i, v in enumerate(L)]
    R = Matrix.Rotation(math.radians(t['yaw0'] + rel), 3, 'Z')
    for v, l in zip(t['o'].data.vertices, L): v.co = R @ l + t['org']
    t['o'].data.update()
    SHELLS[hp].matrix_world = Matrix.Translation(t['org']) @ R.to_4x4()
def side(hp):
    for r in (-90, 90):
        if allowed(hp, r): return r
    return 0
sc = bpy.context.scene
sc.render.engine = 'BLENDER_WORKBENCH'
sh = sc.display.shading; sh.light = 'STUDIO'; sh.color_type = 'TEXTURE'; sh.show_shadows = True; sh.show_cavity = True
sh.cavity_type = 'BOTH'
sc.world = bpy.data.worlds.new('w'); sc.render.resolution_x, sc.render.resolution_y = 1000, 700
sc.render.film_transparent = False
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam
cam.data.lens_unit = 'FOV'; cam.data.angle = math.radians(35)
def shoot(name, tgt, az, el, dist):
    tgt = Vector(tgt); d = Vector((math.sin(math.radians(az)) * math.cos(math.radians(el)), math.cos(math.radians(az)) * math.cos(math.radians(el)), math.sin(math.radians(el))))
    cam.location = tgt + d * dist
    cam.rotation_euler = (tgt - cam.location).to_track_quat('-Z', 'Y').to_euler()
    sc.render.filepath = f'{OUT}/{name}.png'; bpy.ops.render.render(write_still=True)
# (类, 炮塔, 目标点 Blender, 方位(0=从船首看,90=右舷), 距离)
SHOTS = [('fgm1', 'HP_FGM_1', (0, 4.0, 0.75), 3.4), ('fgs053', 'HP_FGS_1', (0, 1.2, 1.1), 2.4),
         ('fgs036', 'HP_FGS_3', (0.95, 0.65, 0.55), 2.0), ('fgm3', 'HP_FGM_3', None, 3.4)]
for n, hp, tg, dist in SHOTS:
    if hp not in TUR: continue
    if tg is None: o = TUR[hp]['org']; tg = (o.x, o.y, o.z + 0.25)
    r = side(hp); el = VMAX.get(hp) or MAXEL[TUR[hp]['k']]
    for h in TUR: pose(h, 0, 0)
    shoot(f'{n}_rest_q', tg, 50 if TUR[hp]['yaw0'] == 0 else 130, 28, dist)
    for h in TUR:   # 全舰同向转塔(射界内), 便于看邻炮
        pose(h, r if allowed(h, r) else 0, el if h == hp else 0)
    shoot(f'{n}_y{abs(r)}el{int(el)}_q', tg, 50 if TUR[hp]['yaw0'] == 0 else 130, 28, dist)
    shoot(f'{n}_y{abs(r)}el{int(el)}_side', tg, 90, 6, dist)
    shoot(f'{n}_y{abs(r)}el{int(el)}_top', tg, 0.1, 85, dist)
print('POSE done')
