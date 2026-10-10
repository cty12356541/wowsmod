# ref.render.warm: 预热常驻出图实例
上级: ref.render
下级: 无
---
冷启动每次都要开浏览器、加载几何与贴图, 要十几秒到几十秒。要连续看图时先预热:
```
wowsmod render --warm <项目>           # 启动常驻实例(后台进程 + 无头浏览器, 已加载好几何/贴图)
wowsmod render status <项目>           # 看是否就绪: pid、已出图数、空闲时长
wowsmod render <项目> --compare        # 自动连到常驻实例, 每张约 1 秒
wowsmod render stop <项目>             # 用完关掉(不关也会空闲 15 分钟自动退出)
wowsmod render status --all            # 本机所有常驻实例; stop --all 全关
```
- 状态文件在 `<项目>/.wowsmod/render_warm.json`, 日志 `<项目>/.wowsmod/render_warm.log`。
- 改了项目(贴图、mfm、几何)后再 render: 实例按文件哈希增量重载, 只重算改动部分, 一般 1~3 秒。
- 空闲时长: `--idle <分钟>` 或环境变量 `WOWSMOD_RENDER_IDLE_MIN`(默认 15)。
- `--cold` 强制不用常驻实例; `--engine soft` / `--texture` 不走常驻实例。
- 自愈: 实例卡住或浏览器崩溃, 会自动重开浏览器重试一次; 还不行就重启实例再试一次。仍失败时返回明确错误码:
  - `render_timeout`(超时, 默认 `--timeout 60` 秒): 先 `wowsmod render stop <项目>` 再 `--warm`;
  - `browser_crashed` / `warm_start_failed`: 看 `.wowsmod/render_warm.log`, 或用 `--engine soft` 兜底;
  - `browser_not_found`: 安装 Edge/Chrome 或 `--browser <路径>`;
  - `bad_param`: 检查 `--query` 参数。
- 冷启动出图的结果里会提示 "下次先 wowsmod render --warm <项目>"(`--json` 里 `warm: false` + `hint`)。
- MCP: `wowsmod_render` 不在 MCP 进程里渲染, 而是自动启动并连接这个常驻实例(实例挂了不影响 MCP); `wowsmod_status` 带项目时会顺便在后台预热。
