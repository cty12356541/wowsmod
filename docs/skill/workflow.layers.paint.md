# workflow.layers.paint: 层 4 贴图(完整图案)
上级: workflow.layers
下级:
- workflow.layers.paint.methods: 产出图案的具体方法(投影/抠色/直接画/全彩合成)
- workflow.layers.paint.color-logic: 配色逻辑(占比/明度层/冷暖/分区/发光/港口补偿)
- workflow.layers.paint.world: 世界坐标作画(UV 位置烘焙, 贴结构走)
- workflow.layers.paint.coverage: 完成标准 + 部件覆盖清单 + role_share 阈值
---
**目标**: 完整高分辨率图案, 不是底色+几道线。
**输入**: 定稿设计书 §3 图案行、层 3 后的项目、港口换算色。
**配色逻辑**: 底色前读 `paint.color-logic`(量化+分区表+port-albedo); 勿只填几个色号。
**顺序**: 清 `_mg` B(`tex channel <_mg> --channel b --set 0`, 否则被原迷彩盖住)→ 底色(`tex recolor`, 港口换算 `port-albedo`)→ 大图案/立绘 → 细节线。
**密度**: 先在参考上数每舷图案数量与覆盖率写进设计书, 作画时逐项对数。
**分辨率**: 舷侧常为 4096 且必须带完整 mip(`tex import --like` 会生成); 设计图 `--width 8192`。
**完成标准**: 以 `paint.coverage` 为准(部件无原版灰、三维≥3、港口实拍)。另: 数量与覆盖率和设计书相差 ≤20%; 关掉发光仍认得出; 清单里每个图案都在; 近景无拉伸/锯齿、远景主图案可读; 工作台图 + 港口同角度并排(`scripts/ref/compare.py`)。
**自评**: 用户只看白天港口截图, 会觉得好看且是这个主题吗?
