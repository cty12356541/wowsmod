# workflow.layers.paint.methods: 产出图案的方法
上级: workflow.layers.paint
下级: 无
---
- 查 UV: `wowsmod lineart <项目> --material <材质> --like <_a> --boundary --overlay uv.png`。原包通常沿用官方 UV 重画。
- 同角度投影: 在 `render --view side|top --width 8192 --json` 同框图上画/贴官方立绘/描摹参考帧 → `wowsmod bake-ref … --ref <图> --material <材质> --like <_a> [--mirror-x]`(带遮挡); 多角度各烘一次。
- 键色抠遮罩: `bake-ref --key "#RRGGBB:tol=mask.png"` → `tex recolor --mask`。
- 直接在 UV 上画: uv.png 做底, 画完 `tex import --like <_a>`。
- 全彩图案(立绘/外部 AI 图): 透明底 PNG → `bake-ref`(保留 alpha)→ `tex composite <_a> --over <uv图> [--keep-detail 0.35]`。
- 笔刷: 两头收尖、粗细变化、配中灰阴影笔刷; 不要等宽线。近景锯齿加 `--feather`。
