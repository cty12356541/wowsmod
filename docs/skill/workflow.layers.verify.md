# workflow.layers.verify: 分阶段验收
上级: workflow.layers
下级:
- workflow.layers.verify.wysiwyg: 工作台 look、实机残差与校准范围
- workflow.layers.verify.port: 获授权后的安装/进港与证据记录
---
**目标**：用本次验收清单证明设计意图达成，并清楚说明证据范围。简单编辑只核对受影响项；整套制作可用 `design-template acceptance`。
**工作台**：同机位对比参考/小样与当前方案；按改动检查轮廓、图案、材质、发光、LOD/远近及动作/碰撞，关闭发光检查涂装本身。game-day/night 是近似，见 `workflow.layers.verify.wysiwyg`。
**并排**：`python scripts/ref/compare.py --out <图> "<参考>:参考" "<工作台图>:工作台" "<港口图>:港口"`；只有实际存在的证据才加入比较。
**港口**：只在授权范围内按 `workflow.layers.verify.port` 执行。文件哈希、注册日志、视觉和动画分别记录，未做项标待验证；安装成功不能写实机验收通过。
**结论**：各项“通过/有差距/未验证”加依据，审美见 `workflow.review`；数字是项目诊断工具，不用统一评分线。差距回对应层，缺证据回参考层，连续无改善先诊断能力限制。结果与下一步写项目记录，不强制每轮更新全局 changelog。
