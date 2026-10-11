# workflow.layers.shape.refine.uv: UV 与贴图烘焙
上级: workflow.layers.shape.refine
下级: 无
---
**输入**: 已 Apply 的 glb 网格; 分区色板(`paint.color-logic`)。
**做法**: 1) UV 按硬边拆岛, 大面优先 texel; 2) 底色/镶边/发光遮罩分通道或分贴图; 3) 需要细节时从高模 bake normal/AO 到低模; 4) 发光遮罩给 `glow`/`mfm`。
**完成标准**: UV 无严重拉伸; 镶边/发光在贴图上可读; 材质名对齐项目 mfm; 港口色走 `port-albedo`。
**道具图集法**(推荐, 知更鸟实测): 一张 1024 共享图集 8 列×128px(渐变羽/白/periwinkle/紫/深蓝/金/发光…, `prop_atlas.py`) + 同版发光遮罩; Blender 材质名 `rb_<列>` → 导出前把 UV 压进该列、合并单材质; 所有件 `import --texture atlas --glow glow`。改色只重出图集, 不动网格。
**不替代**: 船体原版 UV 大面仍用 `tex recolor/composite`; 本节点只管**新网格**。
