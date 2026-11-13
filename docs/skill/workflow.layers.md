# workflow.layers: 按目标选用制作层
上级: workflow
下级:
- workflow.layers.ref: 参考、需求、抽帧与来源
- workflow.layers.map: 部件映射与小样方向
- workflow.layers.shape: 需要改变轮廓或新增模型时
- workflow.layers.paint: 需要底色或图案时
- workflow.layers.glow: 需要发光与材质调整时
- workflow.layers.fx: 需要全局特效且理解影响范围时
- workflow.layers.verify: 工作台检查与获授权港口验收
---
七层是能力索引，不是每次必须全走的流水线。按目标选择相关层，并在项目记录跳过项的理由：简单换色通常是需求→贴图→检查；无模型变化跳过 shape；不做特效就跳过 fx。

**依赖**：先理解参考与意图；小样验证有风险的部分；方向明确后细做。模型改动会影响 UV/贴图，贴图和材质会影响发光；变更后只复查受影响项。整套制作记录在设计书，局部编辑用简短项目记录。

**产物**：参考层留下来源与未知视角；映射层留下逐项计划与选择；造型层留下装配/LOD/碰撞证据；贴图层留下可读的色块图案；发光层留下遮罩与材质；特效层留下全局影响与能力边界；验收层逐项对照意图。

**出图**：`wowsmod render <项目> --warm` 后用工作台/浏览器 render；soft 只作尺寸等辅助。`game-day/game-night` 是近似 look，不是实机证明。先工作台检查，再依已有授权应用/进港；未经授权就停在已完成阶段并列待验项。每层复盘写项目，见 `workflow.review`。
