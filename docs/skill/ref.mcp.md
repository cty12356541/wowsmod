# ref.mcp: AI 工具接入(MCP)
上级: ref
下级: 无
---
`wowsmod mcp` 是一个 stdio MCP 服务器。启动时只暴露 5 个入口工具:

| 工具 | 作用 |
|---|---|
| `wowsmod_status` | 游戏、已装 mod、项目状态 |
| `wowsmod_help` | 读取主题说明(就是本文档系统) |
| `wowsmod_run` | 执行任意 wowsmod 命令(自动加 `--json`) |
| `wowsmod_render` | 出图,直接返回 PNG 图片 |
| `wowsmod_expand` | 按组展开带参数说明的专用工具:texture / model / project / install / assets |

- 展开后服务器会发送 `notifications/tools/list_changed`。不支持该通知的客户端,可以一直用 `wowsmod_run`。
- 配置(`wowsmod ai init <目录>` 会自动写好):
  - Cursor:`.cursor/mcp.json`;
  - Claude Code:`.mcp.json`;
  - 其他工具:命令 `wowsmod mcp`,无参数。
- 规则文件:
  - `AGENTS.md` 是正本;
  - `CLAUDE.md`、`.cursor/rules/wowsmod.mdc`、`GEMINI.md`、`.github/copilot-instructions.md` 都只是指向正本的入口。

## 预热出图
- `wowsmod_render` 连到项目的常驻出图实例(见 `wowsmod help ref.render` 的"预热"一节), 不在 MCP 进程里开浏览器; 第一次调用会先启动实例(冷启动十几秒到几十秒), 之后每张约 1 秒。
- `wowsmod_status` 带项目参数时会在后台预热; MCP 启动时若当前目录是 mod 项目(或设了 `WOWSMOD_PROJECT`)也会自动预热。
- 出错时返回 `[错误码]` 和下一步提示(如 `render_timeout` → 先 `wowsmod render stop <项目>` 再预热)。
