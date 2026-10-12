# ref.blender: Blender 往返命令
上级: ref
下级: 无
---
**何时/调什么/验收**: 先读 `workflow.layers.shape.blender`(精细外形必进; 主题只改道具网格)。
```
wowsmod model export-blender --project <项目> -o ship.glb
# Blender: 保持对象名; 新对象材质名对齐现有材质; 导出 glb
wowsmod model import-blender --project <项目> ship_edited.glb
```
- 同名对象写回对应渲染集; 未改的不写。新对象按材质名找模板渲染集新增。
- 本机需 Blender(`wowsmod doctor` 显示); 亦可手动操作后只跑 import-blender。
