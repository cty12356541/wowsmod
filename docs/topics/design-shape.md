# design-shape: 层 3 大形(额外建模与部件改动)

先定位置与比例, 再做细节。
- 查结构: `wowsmod model list --project <项目>`(渲染集/部件/挂点); 坐标 y 上, 船艏方向见 `wowsmod help formats`。
- 改现有部件: `model scale|move|rotate|remove|replace`(`wowsmod help model`)。
- 新增网格: `model add --glb <网格> --like <渲染集>`, 或 Blender 往返(`wowsmod help blender`)。新网格用已有材质时会共用那张贴图(UV 要放在该贴图的空白区或新画区域)。
- 随炮塔转的物件挂在炮塔部件上; 船体固定物件挂船体渲染集。
- 对齐: `wowsmod render <项目> --engine soft --yaw <度> --pitch <度> --json` 的 `frame` 把世界坐标换算成像素, 用来量尺寸(只作测量)。
- 检查: 在 `wowsmod preview <项目>` 工作台 / 浏览器 `render --compare` 里与参考同角度帧并排; 完成标准以工作台出图为准, 不以 soft 图为准。

**完成标准**: 同角度 render 的轮廓(新增物件位置、长宽比例、数量)与参考帧一致; 每个新增物件在拆解清单里打勾。
**自评**: 遮住贴图只看剪影, 能认出是参考那艘吗? 新物件在远景会不会消失(LOD)?
