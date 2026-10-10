# design-glow: 层 5 发光与材质

- 原理与参数: `wowsmod help glow`。发光 = `_mg` B 通道遮罩 × 漫反射颜色, glow 会把 .mfm 切到 emissive 着色器并改 .visual 标识。
- 哪些材质能发光: 用 `wowsmod lineart … --material x` 的报错列出材质; ship_material(_skinned) 类都有 emissive 变体(船体/上层/炮塔/炮管)。
- 一张 `_mg` 的所有发光遮罩要**一次给齐**(多个 `--mask` 取并集), 后一次 glow 会清掉前一次。
- 发光遮罩来源: 层 4 的图案键色、`lineart --crease/--boundary/--contour/--rings/--fill`(按几何/世界位置)。
- 两种发光风格(参考原包时先量 `_mg` B 的最大值与占比): ①大面积微光——B 只有 0.1–0.2(×255 ≈ 25–50), 面积可大, emissivePower 用默认; ②局部强光——小面积 B=1。不要把①做成满值 B, 会整船发白。①用 `glow --b-max 40` / `model import --b-max 40`(满值处 B=40, 软边按比例)。
- 衣物/薄片/头发类额外模型: 材质开 `doubleSided`, 镂空用 `alphaTestEnable`+`alphaReference`。
- 发光色: 霓虹常是近白的浅色核心, 不是纯饱和色; emissivePower 0.3–0.4 起试。
- 动画: `glow --mfm-set emissionAnimationMode=Int:1` 等(效果需港口确认)。

**不复用旧版本遮罩**: 每个遮罩都要来自当前设计书的设计图, 旧遮罩(如整根炮管)会带入与参考不符的发光面积。
**完成标准**: 只有拆解清单指定处发光; 发光像素占可见面积不超过模式规定上限; 白天港口仍看得出发光、夜间不糊成一片。 发光在 `preview` 工作台里检查(soft 只近似发光, 不算)。
**自评**: 关掉发光后的层 4 是否仍成立(发光不能替代图案)?
