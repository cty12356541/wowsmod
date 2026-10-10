# blender: Blender 往返

```
wowsmod model export-blender --project <项目> -o ship.glb   # 每个渲染集一个对象, 挂点为空物体
# 在 Blender 里编辑(保持对象名; 新对象的材质名要与现有材质一致), 导出 glTF(.glb)
wowsmod model import-blender --project <项目> ship_edited.glb
```
- 名字相同的对象写回对应渲染集,没有改动的不写。新对象按材质名找模板渲染集新增。
- 需要本机装有 Blender 才能自动化(`wowsmod doctor` 会显示)。也可以手动在 Blender 里操作。
