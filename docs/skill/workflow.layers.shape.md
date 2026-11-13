# workflow.layers.shape: 层 3 大形与额外模型
上级: workflow.layers
下级:
- workflow.layers.shape.form-language: 外带造型语言(剪影/分件/倒角/镶边/比例)先于精修
- workflow.layers.shape.blender: 单件进 Blender 精修(曲率/贴合)时
- workflow.layers.shape.refine: 真正精细改船模全流程(拆标杆→设计→建模→UV→挂回→验收)时
---
**目标**: 先定新增**装饰件**与部件改动的位置、比例、数量, 再做细节。
**输入**: 设计书 §3 的额外模型行(来源/挂点/尺寸); 网格来源: 自建简化几何 / 官方模型**只导出的道具部件** / 外部道具 glb。无素材时不硬凑。
**主题模式硬规则**: 只允许道具形外带件。**禁止**人物全身/半身 mesh 当雕像。复刻若参考含人物, 设计书写明并经用户确认; 默认仍禁。
**做法**:
1. 粗摆: `model scale|move|rotate|remove|replace` / `model import`(PMX 先 `pmx2glb` 去人体材质)。
2. **造型语言** → `workflow.layers.shape.form-language`(对标剪影/倒角/镶边)。
3. **精细外形** → `shape.blender`; **整船级改模** → `shape.refine`(拆标杆几何→对等件)(必读): export-blender → 改网格 → import-blender → 出图验收。
4. 独立动画属实验, 先读 `ref.model.animate`; 随炮挂载不能用环绕动画替代。尺寸用 `mod set --ship-length` + `units` 核对本船比例, 不沿用统一船长百分比。
**完成标准**: 剪影与设计一致; LOD、活动范围与材质检查有依据; 主题无人体装配。日/夜和港口仅在相关且获授权时验证, 未测项写明。
**自评**: 只看剪影主次清吗? 有没有靠甲板上立人像认角色(错)? 该进 Blender 的精细件有没有用贴图糊弄?
