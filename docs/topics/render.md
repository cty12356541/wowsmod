# render: 出图看效果

```
wowsmod render <项目> --compare                 # 默认: Edge/Chrome 无头 WebGL, 分屏对比 base|head
wowsmod render <项目> --query "view=fq&compare=split&base=base&w=1600&h=900"
wowsmod render <项目> --sheet                   # 四视角 × base/head 联系表
wowsmod render <项目> --engine soft --compare   # 无浏览器时的软件光栅兜底
wowsmod preview <项目>                          # 启动本机预览页(可交互)
```
- 默认引擎是系统 Edge/Chrome(API.md §11): 内嵌与 `preview` 相同的服务, 用 CDP 调 `window.__render`。
- 找不到浏览器: 退出码 3、`browser_not_found`, 提示安装或用 `--browser` / `--engine soft`。
- 输出默认写到 `<项目>/renders/`。`--json` 含绝对路径 `out` 与 meta。
- MCP 的 `wowsmod_render` 直接返回 PNG 图片内容; 找不到浏览器时返回文本说明(不算错误)。
- 软件光栅(`--engine soft`)只有漫反射+发光近似, 用于没浏览器的环境。真实观感以 WebGL 出图和进港截图为准。
