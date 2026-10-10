# design: 外观设计共用(先判断模式)

先判断模式: 有参考图/视频 → `wowsmod help replicate`; 给了主题(角色/作品) → `wowsmod help theme`; 只有风格描述 → `wowsmod help original`。

## 部件分区表(例: 勃艮第 FSB510, 其他船用 `lineart --material x` 报错列表看材质名)
| 区 | 材质(.mfm) | 贴图 |
|---|---|---|
| 舷侧+甲板+船底 | FSB025_…_Hull(_skinned) | Hull_a/_mg(4096) |
| 上层建筑 | FSB025_…_DeckHouse(_skinned) | DeckHouse_a/_mg |
| 主炮塔+炮管(4 根共一套 UV) | FGM049_…_skinned | FGM049_a/_mg |
| 炮管根部/套筒 | C035_Gun_Barrel_skinned | C035_a/_mg |
| 副炮/高炮 | FGS036 / FGS053 / FGA056 / FGA014 | 各自 _a |
世界坐标: y 向上, 水线 y≈0, 船艏在 -z; 炮塔实例已摆好(lineart/bake-ref 用的整船)。

## 发光规则
- 发光 = `_mg` B 通道遮罩 × 漫反射颜色; glow 会把 .mfm 切到 emissive 着色器并改 .visual 标识(含 TL2_ 炮塔标识)。
- 支持: ship_material(_skinned).fx → ship_emissive_material(_skinned).fx(船体、上层、炮塔、炮管都可)。
- 同一张 _mg 的所有发光遮罩一次给齐; 发光色偏白青(#B8F2FF)比饱和青更像霓虹; emissivePower 0.3–0.4。

## 港口光照补偿
港口白天会把暗色压得更黑: 想要屏幕上 #0B0E14 级别的近黑, 贴图用 #1E1F23–#232529(`wowsmod help recolor` 的 port-albedo)。块面浅灰在港口会更亮, 比参考暗一档。

## 新命令速查
- `render --engine soft --view side|--yaw Y --pitch P --width W --json`: 输出 `frame`(世界→像素)
- `bake-ref`: 同框设计图 → 某材质 UV(遮挡判断, --mirror-x 两舷, --key 键色抠遮罩)
- `lineart`: --crease 折边 / --boundary 外轮廓 / --contour 等值线 / --rings 圆环 / --normal-edges 板缝 / --fill 按世界盒子整块选

## 自检清单(render 与进港都过一遍)
1. 同角度和参考/概念并排, 元素是否都在、位置是否对
2. 没有规则网格条纹、没有跨部件断开的明显接缝
3. 发光不糊成整块(上层建筑 lineart 覆盖率 ≤ 4%)
4. 水线以下没有放重要元素
5. 远景缩小仍可读; 近景无拉伸模糊
6. 进港截图白天正常; python 日志 registerShipMod done 且无新错误
7. 结束 `uninstall`, res_mods 还原
