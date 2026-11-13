# workflow.layers.shape.refine.check.turret: 炮塔外带件检查
上级: workflow.layers.shape.refine.check
下级: 无
---
**何时**：改炮塔网格、加壳/装甲板/点缀时。
- 挂点：`model import --part <实际部件名> --like Turret` 的参数以 help 为准；核对 mesh 在正确部件实例中，不能仅放船体上冒充随炮。
- 活动：按 GameParams 的回转与仰角范围检查炮门、炮管、邻炮塔/船体/邻壳，记录采样步长和未覆盖范围。用原船同姿态作基线，别把原有接触与新增碰撞混淆。
- 贴合：从实际炮塔轮廓推导间隙、外伸与连接，远近看是否像有意设计；不沿用另一船的固定间隙数字。
- 造型：按主题决定分件、倒角、法线/AO、金属与发光；关闭发光仍能读懂。白/peri/royal、局部板或整圈壳都不是通用要求。
- 工具：`scripts/model/turret_check.py`、`turret_pose_render.py` 辅助碰撞/姿态；使用无旧外带件基线，核对工具是否真的支持该骨骼/部件。静态预览不证明实际随转。
**完成**：每类改动炮塔有部件/动作/碰撞/视觉证据；获授权后港口复核相应近景，未测战斗或实际活动就写未验证。Robin T3/T4 塑料盒感、缝隙与面板细节试验见 `workflow.changelog`，不是所有炮塔的审美模板。
