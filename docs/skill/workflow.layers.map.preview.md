# workflow.layers.map.preview: 方向预览(粗样, 定稿前)
上级: workflow.layers.map
下级: 无
---
**目标**: 用最少工作让用户在 2~3 个方向里选, 不做细节。
**每个方向一个项目副本**(`wowsmod mod init` + `import-sdk`, 或复制基础项目目录), 只做: 大面底色(`tex recolor`)+ 主图案/立绘一处(`bake-ref` 或 `tex composite`)+ 额外模型摆位(`model import`, 尺寸先粗)+ 发光一处。
**三档**: 克制(底色+小面积标志元素, 不加模型)/ 主题强化(底色+主图案+人物模型或标志道具)/ 夸张(大面积立绘+翼/光环等大件+较多发光)。三者差异要一眼可见。
**出图**: 同一机位组 `render --query "view=fq…"` 日景 + 一张夜景, 每方向拼一张(`python scripts/ref/compare.py`), 放进设计书 §4。
**审美自查(每张)**: 构图主次清楚吗? 色彩与角色配色一致且和谐吗? 人物/道具与船比例相称吗? 远景还认得出主题吗? 有不好看的地方就写进 §4 备注并回到 `workflow.review`。
