# workflow.layers.shape.refine.check: 穿模/面数/远近可读
上级: workflow.layers.shape.refine
下级: 无
---
**输入**: 已 hook 的项目; 设计尺度。
**检查**:
1. **面数**: `python scripts/model/mesh_budget.py <glb…>` 各件与合计过预算。
2. **穿模**: `render --warm` day side+stern+overhead; 翼根/环座/罩壳不扎进甲板舰桥; 炮管扫掠无挡。
3. **远近**: fq 缩略图能认主外带; 近景有倒角镶边(非光坨)。
4. **LOD**: 远距物件仍在(勿只留近 LOD)。
**完成标准**: 清单全勾; 失败件回 model 修, 不进 accept。
**脚本**: `mesh_budget.py`(已固化); 穿模以出图目视+设计书勾选为主(自动碰撞待补)。
