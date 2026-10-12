"""可选外部路径: 用 Blender + mmd_tools 把 PMX 转成 GLB(内置的 `wowsmod model pmx2glb` 不够用时再用,
例如需要先在 Blender 里摆姿势/删部件/烘焙 morph)。

依赖与许可(都不随 wowsmod 分发, 由用户自行安装):
- Blender(GPL-2.0-or-later), https://www.blender.org
- mmd_tools 插件(GPL-3.0), https://github.com/MMD-Blender/blender_mmd_tools
  Blender 4.2+ 从"扩展"安装(模块名 bl_ext.blender_org.mmd_tools); 旧版本装 zip 插件(模块名 mmd_tools)。
本脚本只通过命令行调用它们, 不链接、不复制其代码。

用法:
  blender -b -P pmx_to_glb_blender.py -- <in.pmx> <out.glb> [scale=0.08] [pose.vmd]
说明:
- scale 与 pmx2glb 的 --unit-m 同义(MMD 1 单位 ≈ 0.08 m)。
- 给 pose.vmd 时应用第 0 帧姿势后导出(静态网格)。
- 导出前把形变烘焙进网格(export_apply), 不导出骨骼动画; wowsmod model import 只用静态网格与材质。
- 状态: 本脚本未在 box 上运行验证(box 没有 Blender); 首次使用请检查输出, 并在修订记录里补一行。
"""
import sys

import bpy
import addon_utils

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
if len(argv) < 2:
    raise SystemExit("用法: blender -b -P pmx_to_glb_blender.py -- <in.pmx> <out.glb> [scale] [pose.vmd]")
src, dst = argv[0], argv[1]
scale = float(argv[2]) if len(argv) > 2 else 0.08
vmd = argv[3] if len(argv) > 3 else None

enabled = False
for mod in ("bl_ext.blender_org.mmd_tools", "mmd_tools"):
    try:
        addon_utils.enable(mod, default_set=True)
        if hasattr(bpy.ops, "mmd_tools") and hasattr(bpy.ops.mmd_tools, "import_model"):
            enabled = True
            break
    except Exception as e:  # noqa: BLE001
        print(f"启用 {mod} 失败: {e}")
if not enabled:
    raise SystemExit("没有可用的 mmd_tools 插件(GPL-3.0, 需自行安装)")

bpy.ops.wm.read_factory_settings(use_empty=True)
for mod in ("bl_ext.blender_org.mmd_tools", "mmd_tools"):
    try:
        addon_utils.enable(mod, default_set=True)
    except Exception:  # noqa: BLE001
        pass
bpy.ops.mmd_tools.import_model(filepath=src, scale=scale, types={"MESH", "ARMATURE", "MORPHS"})
if vmd:
    bpy.ops.mmd_tools.import_vmd(filepath=vmd)
    bpy.context.scene.frame_set(0)
bpy.ops.export_scene.gltf(filepath=dst, export_format="GLB", export_apply=True, export_animations=False, export_skins=False, export_morph=False)
print("写出", dst)
