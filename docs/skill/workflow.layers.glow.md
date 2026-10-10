# workflow.layers.glow: 层 5 发光与材质
上级: workflow.layers
下级: 无(命令细节: `wowsmod help ref.glow`)
---
**目标**: 只在设计指定处发光, 强度与面积受控。
**要点**: 发光 = `_mg` B 遮罩 × 漫反射色; 一张 `_mg` 的遮罩一次给齐(多个 `--mask` 并集)。遮罩来自层 4 键色或 `lineart --crease/--boundary/--contour/--rings/--fill`; 不复用旧版本遮罩。
**两种风格**: 大面积微光(B≈25–50: `--b-max 40`)/ 局部强光(小面积满值)。不要把大面积做成满值, 会整船发白。发光色宜近白浅色核心; emissivePower 0.3–0.4 起试。
**额外模型材质**: 薄片/头发/衣物开 `doubleSided`, 镂空用 alphaTest(`model import` 自动随 glTF)。
**完成标准**: 只有清单指定处发光; 发光面积在设计上限内; 白天看得出、夜间不糊成一片(工作台夜景 + 港口)。
**自评**: 关掉发光层 4 仍成立吗? 发光有没有抢掉人物/立绘的主体地位?
