# workflow.layers.shape.refine.model: Blender 建模
上级: workflow.layers.shape.refine
下级: 无
---
**输入**: design 方案; 可选官方部件(`export-blender`/`geom`)或 PMX 道具(先 `pmx2glb` 去人体)。
**做法**: 1) 参考几何进场(官方 hull 只作比例尺, 不整船重导); 2) 阻塞体定比例; 3) 拓扑四边为主, 倒角/镶边/厚度分层; 4) 枢轴放环心/翼根; 5) Apply 旋转缩放; 三角化导出 glb。
**面数预算**(单件静态): 大翼 3k–12k tris; 环/小饰 <2k; 炮塔壳 2k–8k; 全船外带合计建议 <40k(留 LOD)。超了减面再导。
**Blender 5.x**: `bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)` 用关键字。生成贴图用系统 Python(嵌入式无 PIL)。
**完成标准**: 每件有倒角+镶边或分色环; `mesh_budget.py` 面数过线; 无非流形/反法线; 对象名稳定。
**坑**: 薄环勿 `--height`(按薄轴爆); 用 `--scale`。细节见 `shape.blender`。
