# workflow.changelog: skill 修订记录
上级: workflow
下级: 无
---
每轮: 日期/层/差距/根因/补法/效果。更早行见 `docs/design-changelog-archive.md`。

| 日期 | 层 | 差在哪 | 根因 | 怎么补 | 效果 |
|---|---|---|---|---|---|
| 10-11 r24 | 3/4 | 一度照抄标杆尺度/位置 | 把标杆当硬门槛 | **skill 是辅助不是钉死**: layout 改为原则+参考区间+自问; ≥2 方案自评; accept/benchmark/color-logic 同步定位; 船体学原则(大块硬边分区+发光走结构)用自有「追光」图案 | 方案A2 |
| 10-11 r23 | 3 | P3 布局: 翼压船、麦/艏鳍/艉披风抢戏, 却曾过自检 | check 只验「远景可读」+role_share, **无布局量化/主次/剪影并排** | 新增 `shape.refine.layout`+`sil_pair.py`; L1/L2 缩翼删麦删披风; 布局过线后再修羽纹 | 进行中 |
| 10-11 r22 | 3/5 | P3 船模: 翼像扇/棕榈、发光占比 0、锚色糊、舯近景机位远 | 单点放射翼/发光色=主色相/锚是 DeckHouse 子网格/focus 忽略 dist | 前缘臂翼+图集法+确定性重建; 罩壳/艏翼/麦克/缎带/披风; 天青发光线+`--mask`; 金锚; `fill`; `--bg-corners` | 阶段3 |
| 10-11 r21 | 4 | P2 贴图: 甲板白楔、炮管白、金在港口发绿、近景台阶 | 甲板 UV 被背面三角复用/蒙皮炮管坐标无效/港口 G 增益/最近邻放大 | `paint.world`+`paint.coverage`; bake 记 `upc`; 炮管按 UV 矩形; 金全补偿; 1px 抗锯齿; port 标定改 3 点 | 阶段2 |
| 10-11 r20 | 3/4 | P1 配色不符+船模几乎没改却曾过自审 | 无部位本色/无几何对标/验收同角缺失 | `part_colors`+`role_share`; 本色优先写入 color-logic; 新建 `shape.refine` 七子节点; 拆解书几何清单 | 阶段1 |
| 10-11 r19 | 3/4 | 配色只抄色号、外带无造型语言 | 引导缺失 | `paint.color-logic`+`shape.form-language`+`color_logic.py`; 挂 benchmark/blender | 本轮 |
| 10-11 r18 | 2/7 | C 脏青灰+无倒角却过自审 | 验收太松 | `map.benchmark` 门槛≥3; 底线加量化配色/外带细节 | BENCHMARK |
| 10-11 r17 | 3 | CLI 粗挂穿模; 薄环 `--height` 爆缩放 | 引导缺失 | `shape.blender`+薄环用 `--scale` | C 外带 |
