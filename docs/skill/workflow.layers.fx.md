# workflow.layers.fx: 层 6 特效(全局贴图, 随主题)
上级: workflow.layers
下级: 无
---
**目标**: 炮口焰/开火烟/烟囱烟/航迹与主题、涂装、外带件同步设计, 且在工作台可见。
**原则(主题同步)**: 贴图全局共享(所有船) → 只做保亮度轻偏色, 不加亮/不加不透明度; 借发光主线色相但不发光; 炮口焰保暖色(开火语义), 烟 alpha ≤ 原版, 不动曳光/着火; 按船的主题感交给船模。
**可改性(莱服)**:
| 特效 | 贴图 | 结论 |
|---|---|---|
| 烟囱烟 | `particles/animated/Smoke_*` | 可行(港口实证) |
| 炮口焰 | `MuzzleFlashes_*` | 需训练房实测 |
| 开火烟 | `trails/shot_smoke`、`shot_hat`(RGB 白, 换色=乘色) | 需训练房实测 |
| 航迹 | `trails/foam*` 是数据贴图(R0/G 遮罩/B .5), 无 RGB 颜色 | **颜色不可改**(已实拆确认); 本主题不改航迹 |
发射器参数在 `content/assets.bin` 不可改; 按船只能动挂点位置(.visual), GP 绑定不改。
**工具**: `fx find --kind` → `fx ship-effects <船> [--gp]`(挂点→粒子) → `fx init <项目> --from 主题.json` → `fx apply <项目>`(跳过数据贴图; 缺 particles pkg 报 missing)。主题 = 项目 `fx.json` 的 `preview`(tint/strength/keep/alpha), 工作台与换色同一公式。特效单独建 loose 项目, 不混进船 mod。
**预览**: `render --look game-day|game-night --query "env=water&speed=full&warm=20"`(航迹+烟囱烟, 烟从 HP_FX_Smoke_* 冒) / `"…&fire=main&fireT=0.09"`(炮口焰; 0.6 看开火烟); `fx=vanilla` 出原版对照。无骨骼几何用估计炮口。
**验证**: 训练房已授权(炮火/开火烟/航迹): 原版与主题各一轮同机位连拍; 结束关游戏, 只删特效文件。航迹颜色以贴图实拆为准(foam*=数据), 训练房只拍对照证明观感未改。模式下拉点 Join 钮内侧 teal 船标(~1397,40@2560x1440), 勿点红钮右外侧灰条。
**训练房实测**(2026-10-14, Robin_FX robin-qingge 66 文件 vs 原版, 同机位连拍): 炮口焰仍暖橙(主题允许), 开火烟与烟囱烟在战斗中均为**黑褐色, 与原版肉眼无差**; 乘色换色对这些烟不可见(在战斗内烟色可能由 ramps/数据贴图驱动)。下一步查 `particles/ramps` 与 Smoke_*_LM/MVEA 的作用, 未证实前设计卡里烟色项标「未生效」。港口里 Smoke_* 换色可见(早期绿烟实证)与战斗内不一致。脚本: fx_training.ps1 需同时加载 common.ps1 + m3_game.ps1(Zoom/RelDrag), PS5.1 下用 UTF-8 读入(`[scriptblock]::Create((gc -Raw -Encoding UTF8 f))`)。
**完成标准**: 设计卡(色/形/强度/呼应/克制/可读性) + 可改性表 + 原版对照图; 列出全局受影响贴图; 未实测项写明。
