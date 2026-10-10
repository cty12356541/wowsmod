# design-shape: 层 3 大形(额外建模与部件改动)

先定位置与比例, 再做细节。
- 查结构: `wowsmod model list --project <项目>`(渲染集/部件/挂点); 坐标 y 上, 船艏 +z(glTF 导出的 z 要取反), 见 `wowsmod help formats`。
- 改现有部件: `model scale|move|rotate|remove|replace`(`wowsmod help model`)。
- **额外建模(新部件)**: 网格来源按拆解书 §3 标注(原始素材 / 程序生成 / 外部 AI), 无素材的插画级形状不硬凑。
  1. 网格: .glb 或 .obj(带 UV 最好; 没有 UV 会按平面投影)。
  2. `wowsmod model import --project <项目> --mesh <网格> --like <模板渲染集> [--hp HP_x | --part <部件> | --section <分段>] --at x,y,z --rot 度 --scale s`
     - 随炮塔转: `--part <炮塔>` + `--like Turret`; 每个顶点按最近的模板顶点绑骨(炮塔本体 = 只随水平转, 炮管 = 也随俯仰)。对部件的改动作用于该部件的全部实例。
     - 船体固定物件: `--like <分段>_DeckHouse` 或 `--hp HP_*`。
  3. 材质: 每次导入新建 `<名>.mfm` 与 `_a/_mg/_n/_ao` 贴图, 不与原船共用贴图。`--texture <png>` 或 `--color #RRGGBB`; 发光: `--glow <遮罩png>` 或 `--glow-all`, `--power 0.25–0.5`; 发光颜色 = 漫反射颜色。
  4. 默认写进全部 LOD; 碰撞/装甲不变。撤销: `model undo`。
- 检查: 工作台(`wowsmod preview` / `render`)会在有 model 编辑时自动重新装配几何; 近景用 `--query "view=custom&az=40&el=25&dist=7&focus=<部件名, | 写成 %7C>"`, 夜景加 `&light=night` 看发光。soft 图只用于量尺寸。

**完成标准**
- 同角度 render 的轮廓(新增物件位置、长宽比例、数量)与参考帧一致; 每个新增物件在拆解清单里打勾。
- 每个新增物件: 工作台日景与夜景出图都可见; 港口实证截图可见, 并与工作台同角度并排; 远景(far)仍在(LOD)。
- 随炮塔转的物件: 导入结果里的骨骼绑定是炮塔骨骼。工作台和港口都不转炮塔, 所以只能确认绑定, 实际转动要在战斗或训练房里看(需用户授权), 汇报时要写明这一点。

**自评**: 遮住贴图只看剪影, 能认出是参考那艘吗? 新物件在远景会不会消失(LOD)? 新物件有没有穿进原船的面里?
