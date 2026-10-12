# workflow.layers.map.preview: 方向预览(粗样, 定稿前)
上级: workflow.layers.map
下级: 无
---
**目标**: 用最少工作让用户在 2~3 个方向里选, 不做细节。
**输入**: 已 `import-sdk` 的基础项目; §3 清单; 港口换算色(`port-albedo`)。
**命令**: `wowsmod design-preview <spec.toml> --init` → 填 `[vars]` 与各 `[[direction]]` steps → `design-preview <spec>`: 复制基础项目、跑 steps、默认 5 机位、拼 sheet。`--only` / `--render-only` 见 help。
**每方向只做**: 大面底色(`ship-color` 或 `tex recolor`) + 主图案一处(`bake-ref` / `tex composite`, 可用官方立绘作**舷侧图案**而非雕像) + **可选**一件外带道具(`model import` 道具形, 尺寸先粗) + 发光一处。
**三档(主题 = 涂装+外带件差异, 无人物雕像)**:
- 克制: 角色色大面 + 少量纹样/小贴花; 无外带件或仅极小徽章。
- 主题强化: 底色 + 主图案(立绘作舷侧画) + 1～2 件标志道具(麦克风/天環环等)。
- 夸张: 更大面积图案 + 更多/更大外带件 + 较强发光。
**远景 fq 必须一眼分出三档**; 禁止用「甲板上站一个人」当差异手段。
**完成标准**: sheet 全出图; fq 缩到 480 宽仍分得出三档; 各方向项目 `model list` 无人物身体装配; 图与 spec 进 §4。
**审美自查**: 主次、配色、道具尺度、远景认主题; 不好看写 §4 并回 `workflow.review`。
