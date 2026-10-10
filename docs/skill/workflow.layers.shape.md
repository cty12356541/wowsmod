# workflow.layers.shape: 层 3 大形与额外模型
上级: workflow.layers
下级: 无(命令细节: `wowsmod help ref.model`、`ref.model.import`、`ref.model.animate`)
---
**目标**: 先定新增物件与部件改动的位置、比例、数量, 再做细节。
**输入**: 设计书 §3 的额外模型行(来源/挂点/尺寸); 网格来源按清单(官方模型 / 外部 glb / 自建), 无素材的插画级形状不硬凑。
**做法**: 现有部件 `model scale|move|rotate|remove|replace`; 新部件 `model import`(人物/道具多材质、PMX 先 `pmx2glb`); 会动的物件 `model animate`。坐标 y 上、船艏 +z; 人物先按真实身高 × 船的比例定 `--height`, 再看剪影调。
**完成标准**: 同角度工作台剪影与设计一致; 每个新物件日景/夜景可见、远景(LOD)仍在、不穿模、不挡炮管转动; 港口实拍与工作台同角度并排。随炮塔转的物件只能确认骨骼绑定(港口/工作台不转炮塔), 要写明。
**自评**: 遮住贴图只看剪影, 主次清楚吗? 人物是不是太小看不见、或大到喧宾夺主?
