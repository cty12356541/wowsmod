# workflow.layers.shape.refine: 精细改船模全流程
上级: workflow.layers.shape
下级:
- workflow.layers.shape.refine.teardown: 拆标杆几何改动清单时
- workflow.layers.shape.refine.layout: 外带布局量化/主次/剪影并排时
- workflow.layers.shape.refine.design: 定剪影/比例/挂点方案时
- workflow.layers.shape.refine.model: Blender 建模/倒角/拓扑时
- workflow.layers.shape.refine.uv: UV 展开与贴图烘焙时
- workflow.layers.shape.refine.hook: 导回 import/挂点/动画时
- workflow.layers.shape.refine.check: 穿模/面数/远近可读检查时
- workflow.layers.shape.refine.clean: 原壳小突起打碎色块/发光线, 要清理降噪时
- workflow.layers.shape.refine.accept: 与标杆同角并排验收时
---
**何时**: 主题要「船模也改」(非只涂色); 对标例子有大外带/罩壳/改轮廓; 用户反馈「船模没改」。
**目标**: 拆→设计→建模→UV→挂回→检查→同角验收, 每步有输入与完成标准; 能脚本化的固化。
**硬规则**: 主题禁人物雕像; 只道具/罩壳/外带; 大尺度远景可读(fq 能认翅膀/环)。
**顺序**(勿跳): teardown → design → **layout(量化挂点/尺度/主次, 剪影并排过线)** → model → uv → hook → check →(clean: 原壳降噪, 按需)→ accept。layout 未过线不进贴图细修。粗摆仍走 `shape`/`form-language`; 本树管「真正精细」。
**工具**: `model export-blender`/`import`; `scripts/model/mesh_budget.py`; `scripts/model/part_export.md`(流程); Blender 5.x 用关键字参数 `transform_apply`。
**完成**: accept 同角≥3 过线; changelog 记本轮几何差与补法。
