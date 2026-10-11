"""box 布局预览(无游戏): 船 geometry.glb(贴 P2 贴图) + 外带 glb, 工作台按标杆关键帧机位出图。
blender -b --factory-startup -P box_preview.py -- <ship.glb> <props_dir> <tex_dir> <out_dir> [tag]
坐标: geometry.glb 艏 −z, 导入后 Blender Y=艏 —— 与道具(Blender Y=游戏 z)同系, 直接叠放。
"""
import bpy, sys, os, math, glob
argv = sys.argv[sys.argv.index('--') + 1:]
SHIP, PROPS, TEX, OUT = argv[:4]; TAG = argv[4] if len(argv) > 4 else 'v'
os.makedirs(OUT, exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
def img(path):
    return bpy.data.images.load(path, check_existing=True)
def texmat(name, path):
    m = bpy.data.materials.new(name); m.use_nodes = True
    nt = m.node_tree; bsdf = nt.nodes.get('Principled BSDF')
    t = nt.nodes.new('ShaderNodeTexImage'); t.image = img(path); nt.links.new(t.outputs['Color'], bsdf.inputs['Base Color'])
    nt.nodes.active = t
    return m
def flatmat(name, rgb):
    m = bpy.data.materials.new(name); m.diffuse_color = (*rgb, 1); return m
bpy.ops.import_scene.gltf(filepath=SHIP)
MAP = [('Hull_wire', None), ('Hull', 'hull_a_p2_half.png'), ('DeckHouse', 'dh_a_p2_half.png'), ('FGM049', 'FGM049_a_p2_half.png'),
       ('FGS036', 'FGS036_a_p2_half.png'), ('FGS053', 'FGS053_a_p2_half.png')]
cache = {}
for ob in [o for o in bpy.context.scene.objects if o.type == 'MESH']:
    for i, sl in enumerate(ob.material_slots):
        n = sl.material.name if sl.material else ''
        hit = None
        for key, f in MAP:
            if key in n: hit = (key, f); break
        if hit is None or hit[1] is None:
            m = cache.get('grey') or flatmat('grey', (0.35, 0.36, 0.42)); cache['grey'] = m
            if 'Grid' in n or 'alpha' in n: m = cache.get('deckgrid') or flatmat('deckgrid', (0.18, 0.13, 0.38)); cache['deckgrid'] = m
        else:
            m = cache.get(hit[0]) or texmat(hit[0], os.path.join(TEX, hit[1])); cache[hit[0]] = m
        sl.material = m
atlas = texmat('atlas', os.path.join(PROPS, '..', 'prop_atlas.png') if os.path.exists(os.path.join(PROPS, '..', 'prop_atlas.png')) else os.path.join(PROPS, 'prop_atlas.png'))
for f in sorted(glob.glob(os.path.join(PROPS, 'rb_*.glb'))):
    nm = os.path.basename(f)[:-4]
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=f)
    new = [o for o in bpy.data.objects if o not in before and o.type == 'MESH']
    for o in new:
        if nm.startswith('rb_shell'):   # 随炮件: 摆到各炮塔挂点(游戏坐标 → Blender (x, z, y))
            continue
        for sl in o.material_slots: sl.material = atlas
        if not o.material_slots: o.data.materials.append(atlas)
    # 炮塔壳: 按挂点复制
    if nm.startswith('rb_shell'):
        HP = {'rb_shell_FGM049': [(0, 0.51, 4.07, 0), (0, 0.75, 2.23, 0), (0, 0.51, -4.80, 180)],
              'rb_shell_FGS053': [(0, 0.98, 1.20, 0), (0, 0.91, -3.10, 180), (0, 0.72, -3.77, 180)],
              'rb_shell_FGS036': [(sx * 0.95, 0.45, z, (0 if z > 0 else 180)) for sx in (1, -1) for z in (0.80, 0.27, -2.27, -2.80)]}[nm]
        src = new[0]
        for sl in src.material_slots: sl.material = atlas
        if not src.material_slots: src.data.materials.append(atlas)
        for k, (x, y, z, yaw) in enumerate(HP):
            o = src if k == 0 else src.copy()
            if k: bpy.context.collection.objects.link(o)
            o.location = (x, z, y); o.rotation_euler = (0, 0, math.radians(yaw))
# 水面(遮住水线下, 否则船底深藏青把明度统计拉暗)
bpy.ops.mesh.primitive_plane_add(size=80, location=(0, 0, 0.0)); wp = bpy.context.active_object; wp.name = 'water'
wp.data.materials.append(flatmat('water', (0.10, 0.20, 0.26)))
scn = bpy.context.scene
scn.render.engine = 'BLENDER_WORKBENCH'
scn.display.shading.light = 'STUDIO'; scn.display.shading.color_type = 'TEXTURE'
scn.display.shading.show_shadows = True; scn.display.shading.show_cavity = False
scn.display.shading.background_type = 'VIEWPORT'
scn.render.resolution_x, scn.render.resolution_y = 960, 540
scn.render.film_transparent = False
w = bpy.data.worlds.new('w'); scn.world = w; w.color = (0.20, 0.30, 0.36)
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); scn.collection.objects.link(cam); scn.camera = cam
cam.data.lens = 35
from mathutils import Vector
def look(loc, tgt, lens=35):
    cam.location = loc; cam.data.lens = lens
    d = Vector(tgt) - Vector(loc); cam.rotation_euler = d.to_track_quat('-Z', 'Y').to_euler()
# 机位: 对位标杆 d_010(艏向俯)/d_020(高俯斜, 艏在右)/d_007(舷斜低, 艏在左)/侧/顶
VIEWS = {
 'bow_high': ((0.0, 24.0, 13.0), (0, 0.5, 1.0), 62),
 'high_oblique': ((-14.0, -12.0, 17.0), (0.5, 0.5, 0.8), 48),
 'quarter_low': ((-13.0, 6.5, 3.6), (0.0, 0.5, 1.6), 30),
 'side': ((-30.0, 0.0, 3.0), (0, 0, 1.2), 58),
 'top': ((0.0, 0.0, 34.0), (0, 0.01, 0), 60),
}
for k, (loc, tgt, lens) in VIEWS.items():
    look(loc, tgt, lens); scn.render.filepath = os.path.join(OUT, f'{TAG}_{k}.png'); bpy.ops.render.render(write_still=True)
print('PREVIEW_DONE')
