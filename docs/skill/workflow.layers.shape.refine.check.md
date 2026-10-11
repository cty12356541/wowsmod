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
**出图**: 工作台 `focus=<部件>` 时 `dist` 无效, 近景用 `fill`(>1 拉近, 舯近景 MidFront 1.9)。Read 工具按路径缓存图 → 新图换新文件名再看。
**打分**: `role_share.py --bg-corners`(船贴画边时四边 k-means 会把船色当背景剔掉, 同图分数乱跳)。
**布局**(见 `shape.refine.layout`): 已答布局自问(视觉中心/互抢/压船或太弱/剪影读主题); 标杆仅参考。r22 根因: 只看远景可读就过线。
**完成标准**: 清单全勾; 失败件回 model 修, 不进 accept。
**脚本**: `mesh_budget.py`(已固化); 穿模以出图目视+设计书勾选为主(自动碰撞待补)。
**预览光照**: 审美判断用接近港口白天的光(暖太阳+天空环境, `box_preview.py` PREVIEW_LIGHT=day); 工作台 STUDIO 灰光会把乳白/淡紫读成灰、发光不可见, 只用于剪影/布局。
