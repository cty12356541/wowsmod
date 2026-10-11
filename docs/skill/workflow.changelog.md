# workflow.changelog: skill 修订记录
上级: workflow
下级: 无
---
每轮: 日期/层/差距/根因/补法/效果。更早行见 `docs/design-changelog-archive.md`。

| 日期 | 层 | 差在哪 | 根因 | 怎么补 | 效果 |
|---|---|---|---|---|---|
| 10-11 r19 | 3/4 | 配色只抄色号、外带无造型语言 | 引导缺失 | `paint.color-logic`+`shape.form-language`+`color_logic.py`; 挂 benchmark/blender | 本轮 |
| 10-11 r18 | 2/7 | C 脏青灰+无倒角却过自审 | 验收太松 | `map.benchmark` 门槛≥3; 底线加量化配色/外带细节 | BENCHMARK |
| 10-11 r17 | 3 | CLI 粗挂穿模; 薄环 `--height` 爆缩放 | 引导缺失 | `shape.blender`+薄环用 `--scale` | C 外带 |
| 10-11 r16 | 主题 | 社区主题≠立人像 | 引导缺失 | theme 社区对齐句 | 一致 |
| 10-11 r15 | learn | 社区流程散落 | 引导缺失 | learn.makers 五子节点 | 合并 |
| 10-11 r14 | 3/主题 | 人物雕像当完成 | 引导缺失 | 禁雕像; 涂装+道具 | v0.2 |
