# workflow.layers.shape.refine.check: 装配、碰撞与可读性
上级: workflow.layers.shape.refine
下级:
- workflow.layers.shape.refine.check.turret: 炮塔外带件的回转/仰角与贴合检查
---
**输入**：已导回项目、部件计划与设计尺度。
1. 用 `model list` 核对所有目标件、材质与 LOD；检查实际被加载的 visual/vertices 引用，不能只确认 misc 文件存在或命令返回 ok。
2. 按项目预算看 `scripts/model/mesh_budget.py` 的单件/总面数；用日/夜、侧/艉/俯视检查布局和近景表面，远景看轮廓可读。
3. 检查船体、甲板、舰桥与炮管活动包络的意外穿模；炮塔件读 `workflow.layers.shape.refine.check.turret`。碰撞数值需说明基线与覆盖范围，不能拿已带旧外带件的缓存当纯船基线。
4. 同类件保持共同造型语言，可共用生成器/参数族，也可有意变化；不要求所有模型必须使用某个 Robin 脚本或 emissive 材质。
5. 检查记录分别写文件/引用、预览、注册与港口视觉。动画与自定义发光有未验证边界，见 `ref.model.animate`、`ref.glow`。历史出现 ssm 已挂但网格消失，不能把 apply 成功当可见证据。
**完成**：相关清单都有依据；失败回模型/材质修正，未做实机则保持待验证。工作台 STUDIO 与港口光照不同，色彩/发光不可仅靠展示图通过。r32/r34 专用脚本及问题原文移至 `workflow.changelog` 案例归档。
