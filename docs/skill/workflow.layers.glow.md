# workflow.layers.glow: 层 5 发光与材质
上级: workflow.layers
下级: 无(命令细节: `wowsmod help ref.glow`)
---
**目标**: 只在设计指定处发光, 强度与面积受控。
**输入**: §3 发光行(位置/颜色/面积上限)、层 4 的贴图与键色遮罩。
**要点**: 发光 = `_mg` B 遮罩 × 漫反射色; 一张 `_mg` 的遮罩一次给齐(多个 `--mask` 并集)。遮罩来自层 4 键色或 `lineart --crease/--boundary/--contour/--rings/--fill`; 不复用旧版本遮罩。
**两种风格**: 大面积微光(B≈25–50: `--b-max 40`)/ 局部强光(小面积满值)。不要把大面积做成满值, 会整船发白。发光色宜近白浅色核心; emissivePower 0.3–0.4 起试。
**船体发光线**(知更鸟 r22): 画师索引图里设 `glow` 色号(弧块金边内侧一条、甲板金线组内外各一、上层裙线/烟囱环下), 同时输出 `*_glow.png` → `glow --mg <Hull_mg> --mask hull_glow.png --mfm <Hull.mfm> --mfm <Hull_skinned.mfm> --power 0.5`。发光线挨着金线而**不是**金线本身(发光色=漫反射色, 金发光成橙糊)。
**发光色要与主色块色相分开**: 主色 periwinkle 时发光用天青, 薰衣草发光既读不出也被 role_share 记作主色/白。
**发光色要够饱和**(r34): 近白天青 #B8E8FF 在港口夜景读成「白漆」; 改 #58B8FF(饱和天青)+ emissivePower 0.7(船体与外带件同值), 夜景读主题色、白天也看得出。只换发光线色用 `scripts/paint/glow_recolor.py <_a> <glow遮罩> <#色> <out>`, 外带件图集发光列同色(`prop_atlas.py`)。
**额外模型材质**: 薄片/头发/衣物开 `doubleSided`, 镂空用 alphaTest(`model import` 自动随 glTF)。
**完成标准**: 只有清单指定处发光; 发光面积在设计上限内; 白天看得出、夜间不糊成一片(工作台夜景 + 港口)。
**自评**: 关掉发光层 4 仍成立吗? 发光有没有抢掉人物/立绘的主体地位?
