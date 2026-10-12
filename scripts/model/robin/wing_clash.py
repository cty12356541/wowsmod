import bpy, sys, math
from mathutils.bvhtree import BVHTree
from mathutils import Vector
d=sys.argv[sys.argv.index('--')+1]
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath='./work/e2e/robin/p2/geometry.glb')
ship=[o for o in bpy.context.scene.objects if o.type=='MESH']
for f in ['rb_wing_R','rb_funnel','rb_halo']:
    bpy.ops.import_scene.gltf(filepath=f'{d}/{f}.glb')
new=[o for o in bpy.context.scene.objects if o.type=='MESH' and o not in ship]
wing=[o for o in new if 'wing' in o.name][0]; others=[o for o in new if o is not wing]
dg=bpy.context.evaluated_depsgraph_get()
def bvh(o):
    m=o.evaluated_get(dg).to_mesh(); m.transform(o.matrix_world)
    return BVHTree.FromPolygons([v.co.copy() for v in m.vertices],[p.vertices[:] for p in m.polygons]), m
wb,wm=bvh(wing); ws=[v.co.copy() for v in wm.vertices]
hits=0; mind=9
for o in ship+others:
    sb,sm=bvh(o); ov=sb.overlap(wb)
    if ov: print('HIT',o.name,len(ov)); hits+=len(ov)
    for v in ws[::7]:
        r=sb.find_nearest(v)
        if r[0] is not None: mind=min(mind,r[3])
print('mesh hits',hits,'min gap to ship/props',round(mind,3))
# 回转+仰角包络(Blender: X=x, Y=game z, Z=y): 圆柱 中心(x,z) 半径 R, 顶 = 炮塔底 y0 + 炮管长(仰 90°) + 0.1
env={'FGS036':[(sx*0.95,z,0.45,0.45,0.45) for sx in (1,-1) for z in (0.80,0.27,-2.27,-2.80)],
     'FGS053':[(0,1.20,0.98,0.55,0.55),(0,-3.10,0.91,0.55,0.55),(0,-3.77,0.72,0.55,0.55)],
     'FGM049':[(0,4.07,0.51,1.15,1.1),(0,2.23,0.75,1.15,1.1),(0,-4.80,0.51,1.15,1.1)]}
for k,L in env.items():
    n=0
    for (x,z,y0,R,bl) in L:
        n+=sum(1 for v in ws if math.hypot(v.x-x,v.y-z)<R+0.05 and v.z<y0+bl+0.1)
    print('env',k,n)
xs=[v.x for v in ws]; print('wing x',round(min(xs),2),round(max(xs),2),'y(up)',round(min(v.z for v in ws),2),round(max(v.z for v in ws),2),'z',round(min(v.y for v in ws),2),round(max(v.y for v in ws),2))
