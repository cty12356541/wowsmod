# workflow.layers.map.preview: 方向预览(粗样, 定稿前)
上级: workflow.layers.map
下级: 无
---
**目标**: 用最少工作让用户在 2~3 个方向里选, 不做细节。
**输入**: 已 `import-sdk` 的基础项目; §3 清单; 换算好的港口色(`port-albedo`)。
**命令**: `wowsmod design-preview <spec.toml> --init` 写示例 spec → 填 `[vars]`(贴图名/颜色/外部文件)与每个 `[[direction]]` 的 steps(就是普通 wowsmod 命令行)→ `wowsmod design-preview <spec.toml>`: 每方向复制基础项目、跑 steps、出默认 5 机位(昼斜/昼侧/昼艉/夜斜/夜艉), 拼 `<out>/sheet.png` + `index.html`。只重做一个方向: `--only <id>`; 只重出图: `--render-only`。spec 放进设计书目录, 便于复现。
**每方向只做**: 大面底色(`tex recolor`)+ 主图案/立绘一处(`bake-ref` / `tex composite`)+ 额外模型摆位(`model import`, 尺寸先粗)+ 发光一处。
**三档**: 克制(角色色大面底色+少量标志元素, 不加模型; 不要只贴小花导致远景像原版灰船)/ 主题强化(底色+主图案+人物模型或标志道具)/ 夸张(大面积立绘+大件+较多发光)。**远景斜视(fq)必须一眼分出三档**; 小贴花不够时加大色块占比(如 `--glb --box` 舷侧色带)。
**完成标准**: sheet.png 每方向一行全部出图(`preview.json` 每步 ok); fq 缩到 480 宽仍分得出三档; 图与 spec 进设计书 §4。
**审美自查(每张)**: 主次清楚? 色彩与角色配色和谐? 人物/道具比例相称? 远景认得出主题? 不好看就写 §4 备注并回 `workflow.review`。
