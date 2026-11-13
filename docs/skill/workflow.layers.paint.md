# workflow.layers.paint: 贴图底色与图案
上级: workflow.layers
下级:
- workflow.layers.paint.methods: 按图案选择投影、抠色、直接画或全彩合成
- workflow.layers.paint.color-logic: 从参考/意图推导配色与分区
- workflow.layers.paint.world: 需要贴结构的世界坐标作画时
- workflow.layers.paint.coverage: 核对计划部件、图案与证据
---
**输入**：本次目标与选区；整套制作另有设计书图案行。无模型改动不等待造型层。
**顺序**：先 mask 叠加确认选区 → 普通换色区按需清 `_mg` B → 底色 → 相关图案 → 细节；指定发光区由 glow 最后处理 B，不覆盖其遮罩。
**方法**：单次换色走 `ref.recolor`；整套先读 `paint.color-logic`。需要笔触/插画就保留其形状和 alpha，不用等宽细线替代。数量/覆盖从参考测量，容差按本项目写明；纯色设计无需硬加图案。
**分辨率**：保留所用原贴图尺度与 mip(`tex import --like`)，按近景细节和资源预算决定工作图大小，不强制所有船用同一尺寸。
**完成**：`paint.coverage` 中相关项有证据，关闭发光仍可读，近景无意外错色/拉伸，远景主图案可辨。工作台与已获授权的港口同角并排；没做港口就保留待验项。
