# render: 出图看效果

```
wowsmod render <项目> --compare               # 上=原版, 下=当前; 默认 3/4 视角
wowsmod render <项目> --view all --compare    # side/quarter/top 三个视角
wowsmod render <项目> --texture FSB025_Bourgogne_1945_Hull_a.dds   # 单张贴图: 左原版, 右当前
```
- 输出 PNG 默认在 `<项目>/.render/`,`--json` 结果里有 `out` 路径。MCP 的 `wowsmod_render` 会直接返回图片。
- 内置软件光栅,不需要启动游戏,也不需要 GPU。只渲染漫反射贴图和发光(发光区会提亮),没有法线、AO、PBR、迷彩和海面环境光。颜色只能用来判断“改没改、大致改成什么”,真实观感以进港截图为准。
- 3D 出图需要 SDK 项目。`--compare` 还需要项目里有 SDK 原版副本(import-sdk 时会一起拷入)。
- `--engine preview`(M5 React 预览的 WebGL 无头渲染)目前只留了接口,还没接通。
