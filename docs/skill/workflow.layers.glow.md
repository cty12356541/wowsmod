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
**外带件夜景描边**(知更鸟 P5.1): 不发光的外带件(翼/光环/披风等)在港口夜景会随整体去饱和变成灰白剪影, 只剩船体发光线可读。关键外带件给**克制的发光描边**: 只取外缘细带(羽片两侧外缘约 7% 宽, 不铺满整面), 强度**低于船体主发光线**(遮罩值 0.5–0.7 × 主线, 同 emissivePower; 金环/金饰 ≤0.35-0.55, 发光色=漫反射金→暖金, 再高就成橙黄糊团)。描边色与船体发光线同一色相族(#58B8FF), 用道具专用 `_a`/遮罩, 不改共享图集。工具 `scripts/model/robin/prop_glow_rim.py` → `tex import <wing_atlas> --like <prop>_a.dds` + `glow --mg <prop>_mg.dds --mask <rim遮罩> --mfm <prop>.mfm`。验收: 白天前后差 <1% 像素, 夜景 `channel=emissive` 能看到羽缘线。
**外带件挂点与发光**(知更鸟 C2/halo 港口实测): 外带件挂 `*_DeckHouse` 节点 + `SHIPMAT_EMISSIVE_PBS_DeckHouse` 时改发光无效。renderSet `<node>` 改分段船体节点(如 `MidFront`)、identifier 改 `SHIPMAT_EMISSIVE_PBS_Hull`(全部 LOD)后, 翅膀夜景侧面转亮白; 光环同法仍暗(材质贴图与翅膀同构 → 疑法线/几何)。外带件默认挂船体节点, 每件夜景单独验收; 亮度随角度变, 「被照亮」≠「自发光」。
**完成标准**: 只有清单指定处发光; 发光面积在设计上限内; 白天看得出、夜间不糊成一片(工作台夜景 + 港口)。
**自评**: 关掉发光层 4 仍成立吗? 发光有没有抢掉人物/立绘的主体地位?
