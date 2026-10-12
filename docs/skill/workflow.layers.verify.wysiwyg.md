# workflow.layers.verify.wysiwyg: 所见即所得(游戏内预览)
上级: workflow.layers.verify
下级: 无
---
**目标**: 定稿在工作台「游戏内」look 里完成, 港口只做确认, 不靠反复进港试色。
**两种 look**: `game`(默认, 按港口实拍拟合: 强压缩曲线 + 蓝天青海环境光 + 左舷前上方的太阳)/ `studio`(浅色清透影棚, 用于展示)。UI 工具栏「游戏内 | 展台」; 出图用 `wowsmod render <项目> --look game-day|game-night|studio`。
**流程**: 配色/发光在 `--look game-day` 下判断 → 夜景用 `game-night` 粗看 → 定稿后进港确认(`verify.port`)→ 三联(旧/游戏内/港口)进设计书 §5。
**机位对齐港口**: `--query "view=custom&az=<度>&el=<度>&dist=<模型单位>&fov=<度>&tgt=x,y,z"`(az 0=船首, 正=右舷)。
**重新校准**(换港口/时段/游戏版本, 或有了新实拍): 见 `scripts/preview/calib/CALIBRATION.md`。
1. 在 `regions_*.json` 加一个 shot: 港口图 + 像素框(+色相过滤), 工作台机位与色类。
2. `fit_game_look.py --spec <json> --work <dir> basis` → `fit` → `apply --write <前端>/src/engine/gamelook.json` → 重建前端与 release → `eval` → `triptych.py`。
3. 写 ΔE2000 前后(如实写, 不挑区域), 并记一行 changelog。
**已知残差**(game-day, P2 v6, 15 个区; r40c 均值 ≈7.6, 原校准 7.5 / studio 15.6):
- 金色: `goldHue`≈20 + `tmS`≈1.72 后 high/gold ΔE ~5.6(原 ~13); low/gold 仍 ~14 → 定稿金以港口为准。
- 深紫/peri: `periHue`≈10 后 peri 区 ΔE ~4–7; royal/navy 阴影区仍偏亮(ΔE 6–9)。
- 受光白顶略灰(ΔE ~7)。
- 低机位掠射艏白带海青灰, 只能部分复现。
- 水: `waterOpa`≈1.5 抬近场不透明度(仍见水下轮廓)。
- 夜景: `sat`≈0.06 贴近黑桃城近黑白; 亮度粗对齐, **未做分区 ΔE**(无同设计夜景掩码)。
- **外带件明暗**: `contactAO`≈0.28 近似朝下/掠射压暗(补港口 SSAO/檐下); 真·屏幕空间 AO 仍未做。判断外带件明暗仍以港口近景为准。
**完成标准**: 定稿图标明 look; 与港口差异只剩上面这些残差, 或已写进 changelog。
