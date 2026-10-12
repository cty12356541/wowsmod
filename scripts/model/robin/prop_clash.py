"""外带件 vs 船体静态网格穿插盘点(box, 无游戏):
blender -b --factory-startup -P prop_clash.py -- <geometry.glb> <prop.glb> [<prop.glb> ...]
每件输出: 与各船体对象相交三角对数(BVH overlap)。翼根/底座贴船处少量相交是预期; 比较改前改后, 不应新增对象或数量暴增。"""
import bpy, sys, json
from mathutils.bvhtree import BVHTree
a = sys.argv[sys.argv.index('--') + 1:]
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=a[0])
ship = [o for o in bpy.context.scene.objects if o.type == 'MESH' and not o.name.startswith('rb_')]
dg = bpy.context.evaluated_depsgraph_get()
def bvh(o):
    m = o.evaluated_get(dg).to_mesh(); m.transform(o.matrix_world)
    return BVHTree.FromPolygons([v.co.copy() for v in m.vertices], [p.vertices[:] for p in m.polygons])
SB = [(o.name, bvh(o)) for o in ship]
for f in a[1:]:
    before = set(bpy.context.scene.objects)
    bpy.ops.import_scene.gltf(filepath=f)
    new = [o for o in bpy.context.scene.objects if o not in before and o.type == 'MESH']
    res = {}
    for o in new:
        b = bvh(o)
        for n, s in SB:
            k = len(b.overlap(s))
            if k: res[n] = res.get(n, 0) + k
    print('CLASH', f.split('/')[-1], json.dumps(res, ensure_ascii=False))
