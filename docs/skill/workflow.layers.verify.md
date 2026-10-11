# workflow.layers.verify: 层 7 验收(工作台 + 港口)
上级: workflow.layers
下级:
- workflow.layers.verify.wysiwyg: 所见即所得——在工作台「游戏内」look 定稿, 港口只确认; 如何重新校准
- workflow.layers.verify.port: 装进游戏、进港、截图、卸载的具体步骤
---
**目标**: 证明成品与设计书/参考一致, 而且好看。
**输入**: 层 3–6 完成的项目、设计书验收清单(`design-template acceptance`)。**完成标准**: 清单逐项打勾 + 审美各项不为「差」+ 港口昼夜证据齐(`verify.port`)。
**并排**: `python scripts/ref/compare.py --out <图> "<参考>:参考" "<工作台图>:工作台" "<港口图>:港口" [--grid 3]`, 每轮存进设计书 §5。
**角度**: 舷侧/艏斜/艉斜/俯视/近景; 港口白天、夜间、远景(LOD)、近景(锯齿)。
**判定**: 清单逐行打勾; 不达标写明退回哪层(轮廓→3, 图案→4, 发光→5, 特效→6, 证据→1); 3 轮仍不达标写原因(多半缺工具能力)。
**审美判定**: `workflow.review` + 对标表均≥3; 任一项差/对标<3 不算通过。
**以游戏为准**: 工作台出图用 `--look game-day/game-night`(`verify.wysiwyg`); 与港口差异大先查原因(`_mg` B 未清、时段光照、LOD、已知残差), 写进 changelog。
