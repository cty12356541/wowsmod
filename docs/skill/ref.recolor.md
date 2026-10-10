# ref.recolor: 换色 / 涂装 / 去迷彩
上级: ref
下级: 无
---
## 关键事实(勃艮第港口实测)
- 船体颜色主要来自 `_a.dds`(漫反射)。
- `_mg.dds` 的 **B 通道是迷彩/涂装遮罩**:B 不为 0 时,游戏会用 camouflages.xml 里的颜色盖掉 `_a`。所以想让新颜色显示出来,要把对应 `_mg` 的 B 清零。
  - **例外**:要做发光的 DeckHouse,它的 `_mg` B 留给 glow 用,不要清零。
- 港口光照会让颜色变亮、变灰。先换算:`wowsmod port-albedo <想在港口看到的颜色>`,结果里的 `to_color` 才是给 recolor 用的颜色。

## 步骤(SDK 项目,以整船换成深海蓝为例)
```
wowsmod --json port-albedo #1E3F66                  # → to_color
wowsmod mod status <项目> --json                     # 看有哪些 *_a.dds
wowsmod tex recolor <贴图名_a.dds> --project <项目> --to-color <to_color> --exclude-hue 0:65 --soft 0.04
wowsmod tex channel <同名_mg.dds> --project <项目> --channel b --set 0   # 去迷彩遮罩
```
- 范围:船体与上层建筑是 `<船体模型>_(Hull|DeckHouse)_a.dds`;炮塔、指挥仪、雷达是 `F[GDR]*_a.dds`。逐个执行即可。
- `--exclude-hue 0:65` 用来保留木甲板、锈迹和船底红。
- 还可以按部件或区域限定:`--glb <export 出的 glb> --material <正则>`、`--uv-rect`、`--mask <png>`。先用 `wowsmod mask ... --overlay x.png` 看选区。
- 效果检查:`wowsmod render <项目> --compare`,或用 `--texture <名>` 单看贴图。
- 撤销:`wowsmod tex` 的编辑会把原贴图留在 `edits/`;也可以重新 import-sdk 到新项目。
