# workflow.layers.paint.color-logic: 配色逻辑(量化+分区)
上级: workflow.layers.paint
下级: 无(工具: `palette.py`、`color_logic.py`、`part_colors.py`、`role_share.py`、`port-albedo`)
---
**何时**: 层 2 语言卡 / 层 4 底色前; 对标例子或主题立绘已齐。勿只抄色号、勿整图糊取色。
**方法(硬)**: **角色本色优先, 搭配逻辑来自标杆** — 主/辅/点缀 hex 必须来自角色部位取色(发/衣/环/翅/饰); 分区占比、L*跨度、主色块面积、暗色压边、发光线比例抄标杆 `role_share`, 不抄标杆色相。
**步骤**:
1. **部位取色**: `part_colors.py extract` 对立绘裁发/衣/环/翅/饰; 禁止背景色进主辅。
2. **标杆角色占比**: `role_share.py` 标杆关键帧 → 白底/主色块/暗色/发光/中灰%。
3. **推导板**: `part_colors.py derive` 立绘部位→船体分区(船壳/水线/甲板/上层/炮塔/外带); 主识别色做主色块(15–25%), 暗色≥15%, L_span≥50, 发光≤5%。
4. **港口**: 各屏上色 `port-albedo` → `--to-color`。
5. **对标**: 同角并排 + `role_share` 我方; 中灰>30% 或主色块≈0 → 不过。`map.benchmark` 配色维≥3。
**完成标准**: 有部位板+推导图+分区表+port-albedo; 选定理由含「本色来源+标杆占比」。
**自评**: 去掉点缀还能认出角色本色吗? 是否粉脏灰糊成一色、L 拉不开?
