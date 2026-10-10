# design-paint: 层 4 贴图(完整图案, 不是线条)

顺序: **底色 → 大图案(块面/插画/纹样) → 细节线条**。禁止只有底色+线条交差。
1. 查 UV: `wowsmod lineart <项目> --material <材质> --like <_a 贴图> --boundary --overlay uv.png` 看 UV 岛分布; 舷侧通常是几条长条带。
2. 产出图案(任选, 可组合):
   - 同角度投影: 在 `render --view side|top|--yaw/--pitch --width 8192 --json` 的同框图上画(手绘/AI 生成/参考帧描摹), `wowsmod bake-ref … --ref <图> --material <材质> --like <_a> [--mirror-x]` 投影进 UV(带遮挡)。多角度各烘一次再合并。
   - 键色抠遮罩: `bake-ref --key "#RRGGBB:tol=mask.png"` → `tex recolor --mask`。
   - 直接在 UV 上画: 用 uv.png 做底, 画完 `tex import --like <_a>`。
3. 底色: `tex recolor <_a> --to-color <色>`(保留明暗细节); 港口光照会压暗/提亮, 见 `wowsmod help recolor`。
4. 分辨率: 舷侧贴图常为 4096; 设计图用 `--width 8192`; 近景有锯齿就 `--feather`。
5. 清迷彩/涂装遮罩: `tex channel <_mg> --channel b --set 0`(否则原迷彩盖住你的颜色)。

**完成标准**: 关掉发光(或看 `render` 的漫反射)仍能认出这套涂装; 拆解清单里的每个图案都在对应部件上; 近景无明显拉伸/锯齿, 远景主图案可读。 检查图来自 `preview` 工作台或浏览器 `render`, 不是 soft。
**自评**: 这是"图案"还是"几道线"? 用户只看白天港口截图会觉得像参考吗?
