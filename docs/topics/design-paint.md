# design-paint: 层 4 贴图(完整图案, 不是线条)

顺序: **清 `_mg` B → 底色 → 大图案(块面/插画/纹样) → 细节线条**。禁止只有底色+线条交差。
0. **先清迷彩遮罩**: `tex channel <_mg> --channel b --set 0`。B>0 的区域在游戏/工作台里会被原迷彩盖住(工作台显示成原版浅灰), 不清就判断不了颜色。
1. 查 UV: `wowsmod lineart <项目> --material <材质> --like <_a 贴图> --boundary --overlay uv.png` 看 UV 岛分布; 舷侧通常是几条长条带。
2. 产出图案(任选, 可组合):
   - 同角度投影: 在 `render --view side|top|--yaw/--pitch --width 8192 --json` 的同框图上画(手绘/AI 生成/参考帧描摹), `wowsmod bake-ref … --ref <图> --material <材质> --like <_a> [--mirror-x]` 投影进 UV(带遮挡)。多角度各烘一次再合并。
   - 键色抠遮罩: `bake-ref --key "#RRGGBB:tol=mask.png"` → `tex recolor --mask`。
   - 直接在 UV 上画: 用 uv.png 做底, 画完 `tex import --like <_a>`。
   - 全彩图案(含外部 AI 生成图): 透明底 PNG → `bake-ref`(保留 alpha) → `tex composite <_a> --over <uv图> [--keep-detail 0.35]` 合成, 保留底图明暗。
   - 笔刷: 两头收尖、有粗细变化, 配一层中灰"阴影笔刷"; 不要等宽线。
3. 底色: `tex recolor <_a> --to-color <色>`(保留明暗细节); 港口光照会压暗/提亮, 见 `wowsmod help recolor`。
4. 分辨率: 舷侧贴图常为 4096; 设计图用 `--width 8192`; 近景有锯齿就 `--feather`。
5. 密度: 先在参考的正侧帧上数"每舷笔刷/色块/圆环数量"和覆盖率写进设计书, 作画时逐项对数。

**完成标准**: 图案数量与覆盖率和设计书记录相差 ≤ 20%; 关掉发光(或看 `render` 的漫反射)仍能认出这套涂装; 拆解清单里的每个图案都在对应部件上; 近景无明显拉伸/锯齿, 远景主图案可读。 检查图来自 `preview` 工作台或浏览器 `render`(不是 soft), 并已进港实拍与参考同角度并排(`scripts/ref/compare.py`)。
**自评**: 这是"图案"还是"几道线"? 用户只看白天港口截图会觉得像参考吗?
