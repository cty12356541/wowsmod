# workflow.layers.ref.video: 视频参考(抽帧/放大/覆盖率)
上级: workflow.layers.ref
下级: 无
---
**目标**: 参考视频的全角度证据。**输入**: 视频链接/文件。**完成标准**: 关键帧拼图 + 各部位放大图 + 覆盖率数字进设计书 §1。
- 抽帧: `python scripts/ref/ref_frames.py --bv <BV号> --out <目录> --frames 48`(或 `--video <文件>`)→ 关键帧 + 时间戳拼图 + info.json。机房 IP 被拒(412)时用浏览器播放截帧放进 `<目录>/frames/` 后 `--sheet-only`。
- 画质: B 站匿名最高 1080P(qn=80); 匿名 DASH 只有 480P, 别用。
- 放大: 舷侧/艏/艉/炮塔顶/上层/炮管各截一张局部; 在正侧帧上数图案数量。
- 覆盖率: `python scripts/ref/coverage.py <帧> --box …`(框只框船体)量各明度层占比与平均色。
- B 站简介/评论接口: `x/web-interface/archive/desc?bvid=`、`x/v2/reply?type=1&oid=<aid>`(先取 `x/frontend/finger/spi` 的 buvid3)。
