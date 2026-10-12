# Blender 后台脚本(M3 往返示例): 导入 wowsmod model export-blender 的 GLB, 把 MidBack 烟囱三件(主体/上层/线框)
# 以烟囱底为基准沿竖直方向拉高 FACTOR 倍, 再导出 GLB(带自定义属性)。
# 用法: blender.exe --background --python m3_roundtrip.py -- in.glb out.glb [factor]
import sys, bpy
from mathutils import Vector

argv = sys.argv[sys.argv.index('--') + 1:]
src, dst = argv[0], argv[1]
factor = float(argv[2]) if len(argv) > 2 else 1.35
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=src)
names = ['MidBack_Chimney', 'MidBack_DeckHouse_Chimney', 'MidBack_wire_Chimney']
objs = [o for o in bpy.data.objects if o.type == 'MESH' and o.name.split('.')[0] in names]
print('targets', [o.name for o in objs])
if not objs:
    raise SystemExit('没有找到烟囱对象')
# 世界坐标(Blender Z 向上)下烟囱的最低点
zmin = min((o.matrix_world @ v.co).z for o in objs for v in o.data.vertices)
for o in objs:
    mw = o.matrix_world
    inv = mw.inverted()
    for v in o.data.vertices:
        w = mw @ v.co
        w.z = zmin + (w.z - zmin) * factor
        v.co = inv @ w
    o.data.update()
print('zmin', zmin, 'factor', factor)
bpy.ops.export_scene.gltf(filepath=dst, export_format='GLB', export_extras=True, export_yup=True, export_apply=False)
print('exported', dst)
