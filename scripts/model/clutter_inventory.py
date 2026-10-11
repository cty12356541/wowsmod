"""原壳小突起盘点(box): blender -b --factory-startup -P clutter_inventory.py -- <geometry.glb> <out.json> [max_diag=0.12]
对每个网格对象: 按距离焊接后取连通块, 记录 diag < max_diag 的小块(中心/尺寸/三角数/法向主朝向)。
坐标 = glb(游戏 y 上): x 右舷, y 上, z 艏。只读, 不改网格。"""
import bpy, bmesh, sys, json
from mathutils import Vector
a = sys.argv[sys.argv.index('--') + 1:]; G, OUT = a[0], a[1]; MAXD = float(a[2]) if len(a) > 2 else 0.12
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=G)
res = {}
# 船体半宽剖面(按游戏 z 分 0.1 bin): 所有 Hull_* 顶点 max|x| (y<舷墙高) → 判舷侧件
prof = {}; ys = []
for o in bpy.context.scene.objects:
    if o.type == 'MESH' and o.name.startswith('Hull_'):
        for v in o.data.vertices:
            w = o.matrix_world @ v.co; gz = -w.y; gy = w.z
            k = round(gz * 10); prof[k] = max(prof.get(k, 0), abs(w.x)) if gy < 1.0 else prof.get(k, 0)
for o in bpy.context.scene.objects:
    if o.type != 'MESH': continue
    bm = bmesh.new(); bm.from_mesh(o.data); bm.transform(o.matrix_world)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-4)
    bm.faces.ensure_lookup_table(); seen = set(); comps = []
    for f in bm.faces:
        if f.index in seen: continue
        st = [f]; seen.add(f.index); cf = []
        while st:
            x = st.pop(); cf.append(x)
            for e in x.edges:
                for y in e.link_faces:
                    if y.index not in seen: seen.add(y.index); st.append(y)
        vs = {v for x in cf for v in x.verts}
        mn = Vector((min(v.co.x for v in vs), min(v.co.y for v in vs), min(v.co.z for v in vs)))
        mx = Vector((max(v.co.x for v in vs), max(v.co.y for v in vs), max(v.co.z for v in vs)))
        d = (mx - mn).length
        if d < MAXD:
            n = sum((x.normal * x.calc_area() for x in cf), Vector()); 
            # Blender Z 上, Y = 游戏 z(艏)取反? glTF 导入: 游戏(x,y,z) → Blender(x,-z,y)
            c = (mn + mx) / 2
            gz = -c.y; hb = max(prof.get(round(gz * 10) + j, 0) for j in (-1, 0, 1))
            side = abs(c.x) > hb - 0.06 and abs(n.x) > 0.5 * n.length
            comps.append(dict(side=bool(side), up=bool(n.z > 0.7 * n.length), c=[round(c.x, 3), round(c.z, 3), round(-c.y, 3)], size=[round(mx.x - mn.x, 3), round(mx.z - mn.z, 3), round(mx.y - mn.y, 3)], tris=sum(len(x.verts) - 2 for x in cf)))
    res[o.name] = dict(faces=len(bm.faces), small=len(comps), small_tris=sum(c['tris'] for c in comps), comps=comps)
    bm.free()
json.dump(res, open(OUT, 'w'))
for k, v in res.items(): print('INV', k, v['faces'], v['small'], v['small_tris'])
