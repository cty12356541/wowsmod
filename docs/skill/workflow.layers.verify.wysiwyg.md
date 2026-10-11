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
**已知残差**(game-day, P2 v6, 15 个区, 均值 15.6→7.5):
- 金色 ΔE 13–16(港口更黄、更橄榄)→ 金色以港口为准。
- 深紫偏冷、偏淡(ΔE 7–9)。
- 受光白顶略灰(ΔE 7)。
- 低机位掠射时艏上沿白带在港口呈海青灰, 只能部分复现。
- 水比港口透明。
- 夜景只粗调(港口黑白调色), 未做 ΔE 验收。
**完成标准**: 定稿图标明 look; 与港口差异只剩上面这些残差, 或已写进 changelog。
