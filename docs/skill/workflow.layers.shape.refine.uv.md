# workflow.layers.shape.refine.uv: UV 与贴图烘焙
上级: workflow.layers.shape.refine
下级: 无
---
**输入**: 已 Apply 的 glb 网格; 分区色板(`paint.color-logic`)。
**做法**: 1) UV 按硬边拆岛, 大面优先 texel; 2) 底色/镶边/发光遮罩分通道或分贴图; 3) 需要细节时从高模 bake normal/AO 到低模; 4) 发光遮罩给 `glow`/`mfm`。
**完成标准**: UV 无严重拉伸; 镶边/发光在贴图上可读; 材质名对齐项目 mfm; 港口色走 `port-albedo`。
**道具图集法**(推荐, 知更鸟实测): 一张 1024 共享图集 8 列×128px(渐变羽/白/periwinkle/紫/深蓝/金/发光…, `prop_atlas.py`) + 同版发光遮罩; Blender 材质名 `rb_<列>` → 导出前把 UV 压进该列、合并单材质; 所有件 `import --texture atlas --glow glow`。改色只重出图集, 不动网格。
**不替代**: 船体原版 UV 大面仍用 `tex recolor/composite`; 本节点只管**新网格**。
**图集必须翻转安全**(r35, 实拍教训): Blender→glTF→游戏链路 v 是否翻转没法从盒内预览确认; T3 把纯色放 v=0.25、面板细节放上半, 进游戏后纯色件(艇饰白羽、炮塔纯色面)采到了细节区 → 灰/紫块+同心圆+青色短划的马赛克, 面板反而采到纯色(显平)。规则: 纯色一律取 **v=0.5** 中带; 有方向的细节放上带并在下带**上下镜像**一份(`prop_atlas.py` T3b), 这样翻不翻都一样。改图集布局后先在港口拍一张近景验。
**图集与 glb 同批次(P5)**: apply 必须从 glb 同目录取图集(`$R\props\prop_atlas.png`); 旧脚本取 `$R\prop_atlas.png`, 换了 glb 没换图集, T3b 修复进游戏仍是旧图集马赛克。改图集后解出已装 `_a.dds` 核对布局。羽片渐变也要翻转安全(关于 v=0.5 对称)。
