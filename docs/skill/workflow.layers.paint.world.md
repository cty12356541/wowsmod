# workflow.layers.paint.world: 世界坐标作画(UV 位置烘焙)
上级: workflow.layers.paint
下级: 无(脚本: `scripts/paint/uvpos_bake.py`、`scripts/paint/robin_p2_paint.py` 为范例)
---
**何时**: 图案要贴结构走(干舷比例弧线、甲板边平行线、层高带、炮塔顶中线), 投影 bake-ref 做不到时。
**输入**: 项目 `geometry.glb`(工作台缓存)、原 `_a`(取细节比)。
**步骤**:
1. 烘: `uvpos_bake.py geometry.glb --material '<Hull 正则>' --size 4096 -o hull_pos.npz`(炮塔加 `--node 'HP_FGM_1' --local`)。得 pos/nrm/cov + `upc`(朝上三角命中)。
2. 画: 按世界坐标写色号图(干舷比 f=y/甲板边高, 船长 t, 半宽比 |x|/hw)。左右舷共用 UV → 只用 |x| 设计, 图案须对称。
3. 合成: 色号→贴图色(按色补偿方式)× 原图细节比(lum/blur24, 0.72–1.22, ^0.55), 底色 1px 高斯防台阶; `tex import --like <orig> --project`。
**坑(实测)**:
- 甲板纹素被舱内背面三角复用 → 记成朝下面判成舷侧, 甲板出现白楔: 用 `upc>0` 甲板优先。
- 炮管是蒙皮件, glb 绑定姿态坐标不可用 → 按 UV 矩形(圆柱展开, 炮口熏黑边)单独上色; C035 `_mg` G 高光会把深色反白, 降到 ~120。
- DeckHouse 材质里 y<0.6 的顶面是主甲板铺板, 不是上层顶。
- 未烘到的纹素(>8px)填深色, 别填白。
- 工作台用 `water=full`; 默认剖面显示水下。
**完成标准**: 预览图(world side/top)与工作台同角一致, 无错色块; 进入 `paint.coverage` 验收。
