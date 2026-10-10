# workflow.layers.paint: 层 4 贴图(完整图案)
上级: workflow.layers
下级:
- workflow.layers.paint.methods: 产出图案的具体方法(投影/抠色/直接画/全彩合成)
---
**目标**: 完整高分辨率图案, 不是底色+几道线。
**顺序**: 清 `_mg` B(`tex channel <_mg> --channel b --set 0`, 否则被原迷彩盖住)→ 底色(`tex recolor`, 港口换算 `port-albedo`)→ 大图案/立绘 → 细节线。
**密度**: 先在参考上数每舷图案数量与覆盖率写进设计书, 作画时逐项对数。
**分辨率**: 舷侧常为 4096 且必须带完整 mip(`tex import --like` 会生成); 设计图 `--width 8192`。
**完成标准**: 数量与覆盖率和设计书相差 ≤20%; 关掉发光仍认得出; 清单里每个图案都在; 近景无拉伸/锯齿、远景主图案可读; 工作台图 + 港口同角度并排(`scripts/ref/compare.py`)。
**自评**: 用户只看白天港口截图, 会觉得好看且是这个主题吗?
