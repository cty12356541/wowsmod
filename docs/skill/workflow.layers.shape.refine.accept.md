# workflow.layers.shape.refine.accept: 同角并排验收
上级: workflow.layers.shape.refine
下级: 无
---
**输入**: check 通过的日/夜多角图; 标杆同视角帧。
**做法**: 舷/舷斜/侧/俯/夜 各一张, 与标杆同角并排(`compare.py` 或拼图); 几何维单独打分(翼/环/罩壳/剪影/无原灰)。
**定位**: 标杆并排是参照不是模板; 造型/位置可以不同, 写明设计理由即可。
**过线**: 几何维≥3/5 且用户未否; fq 可辨主外带; 相对 teardown「对等位」均有实物(允许造型不同、尺度同级)。
**失败**: 回 design/model, **勿**只加贴花充数。写 changelog: 差在哪/根因/补法。
**与** `map.benchmark` **关系**: accept 专管几何; 配色维仍走 color-logic + role_share。
