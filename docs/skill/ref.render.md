# ref.render: 出图
上级: ref
下级:
- ref.render.warm: 连续出图前预热常驻实例 / 出图报错
---
```
wowsmod render <项目> --compare                 # 默认: Edge/Chrome 无头 WebGL, 分屏对比 base|head
wowsmod render <项目> --query "view=fq&compare=split&base=base&w=1600&h=900"
wowsmod render <项目> --sheet                   # 四视角 × base/head 联系表
wowsmod render <项目> --look game-day           # 游戏内预览(港口拟合); game-night / studio(展台)
wowsmod render <项目> --engine soft --compare   # 无浏览器时的软件光栅兜底
wowsmod preview <项目>                          # 启动本机预览页(可交互)
```
- 默认引擎是系统 Edge/Chrome(API.md §11): 内嵌与 `preview` 相同的服务, 用 CDP 调 `window.__render`。
- 找不到浏览器: 退出码 3、`browser_not_found`, 提示安装或用 `--browser` / `--engine soft`。
- `--look`: 默认 game(白天); 定稿与验收用 game-day, 展示图用 studio。详见 `workflow.layers.verify.wysiwyg`。机位可加 `fov=`、`tgt=x,y,z`。
- 特效: `--query "env=water&speed=full&warm=20"`(航迹+烟囱烟) / `"…&fire=main&fireT=0.09"`(炮口焰); `fx=project`(默认, 项目 fx.json) | `vanilla` | JSON。见 `workflow.layers.fx`。
- 输出默认写到 `<项目>/renders/`。`--json` 含绝对路径 `out` 与 meta。
- MCP 的 `wowsmod_render` 直接返回 PNG 图片内容; 找不到浏览器时返回文本说明(不算错误)。
- 软件光栅(`--engine soft`)只有漫反射+发光近似, 用于没浏览器的环境。真实观感以 WebGL 出图和进港截图为准。

## 设计用: 任意角度与世界→像素映射
- `--yaw <度> --pitch <度>`(soft): 任意机位, 如 `--yaw 35 --pitch 14` 接近进港斜视。
- `--json` 输出的 `frame` 给出世界→像素映射, 用来在同框设计图上按世界坐标作画, 再 `wowsmod bake-ref` 投影进 UV(见 `wowsmod help workflow.replicate`)。
