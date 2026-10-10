# M6 报告: AI 接入(渐进式披露) + 预览/出图

状态:**已完成核心验收**(2026-10-10 Asia/Shanghai)。M4 见 M4_REPORT.md。本里程碑把 M5 前端嵌入 `wowsmod.exe`,并落地渐进式披露 / MCP / 出图。

## 1. 渐进式披露

| 机制 | 说明 |
|---|---|
| `AGENTS.md` | `wowsmod ai init <目录>` 写出,约 800 字,只放规则与任务路由 |
| 各工具入口 | `CLAUDE.md` / `GEMINI.md` / `.cursor/rules/wowsmod.mdc` / `.github/copilot-instructions.md` 只指向 AGENTS.md;`.cursor/mcp.json` 与 `.mcp.json` 配置 `wowsmod mcp` |
| `wowsmod help [主题]` | 13 个主题文档内嵌于 exe(`docs/topics/*.md`),按需阅读 |
| `--help` 末尾 | 每条命令有「下一步/相关主题」 |
| JSON `next` | 成功与失败结果都带建议下一步;人类模式打印「建议下一步」 |
| MCP 入口工具 | 仅 5 个:`wowsmod_status` / `help` / `run` / `render` / `expand`;`expand` 按组展开专用工具并通知 `tools/list_changed` |

## 2. 预览与出图(M5 集成)

| 命令 | 作用 |
|---|---|
| `wowsmod preview <项目> [--port 8960] [--no-open]` | 本机 HTTP(`127.0.0.1`),嵌入 `preview_web/`(M5 dist),提供 `/api/v1/*`(含 `/sim`),默认打开浏览器 |
| `wowsmod render <项目> …` | 默认引擎 `browser`:内嵌同一服务 + 系统 Edge/Chrome 无头 + CDP(`?render=wait` → `WM_RENDER_READY` → `window.__render`);`--engine soft` 为软件光栅兜底 |

实现要点(对照 `preview_app/BACKEND_CHECKLIST.md` / `API.md` §11 / `REPORT.md` §3):

- 静态产物:`rust-embed` 嵌入 `crates/wowsmod/preview_web/`
- HTTP:`tiny_http` 多线程处理(前端并行拉 geometry + 贴图)
- CDP:`preview/cdp.rs`(对照 `preview_app/tools/cdp.py`);成功路径优先 `dataURL`(`data=true`);原生失败时回退同机 `cdp.py`
- 找不到浏览器:退出码 **3** + JSON `error.code=browser_not_found`(显式 `--browser` 无效路径会**立即**失败,不再白开 PreviewState)
- MCP `wowsmod_render` 直接返回 PNG(`content[].type=image`, base64 + `mimeType=image/png`)
- 几何:优先项目 `.wowsmod/cache/geometry.glb`;否则种子(勃艮第 FSB510,含 LOD/骨骼)~16.5 MB
- `/api/v1/sim`、`/parts`、`/steps`、`/textures`、`/blob/{hash}.png`、`/geometry/{hash}.glb` 已实现

## 3. Windows 实机验收(D:\WoWSModding\wowsmod, deepsea6 / FSB510_Bourgogne)

### 3.1 `wowsmod preview`

| 接口 | 结果 |
|---|---|
| `GET /api/v1/health` | 200 `ok`, server=wowsmod |
| `GET /api/v1/project` | head=`s2`, game=D:\Korabli build 8867689 |
| `GET /api/v1/state` | 200 |
| `GET /api/v1/steps` | 200,~28 KB(s0/s1/s2) |
| `GET /api/v1/parts` | 200,~39 KB |
| `GET /api/v1/model` | 200, geometry.hash=`e0f8507ffdd592c5` |
| `GET /api/v1/geometry/e0f8507ffdd592c5.glb` | 200,**16475704** bytes |
| `GET /api/v1/sim` | 200,~11 KB |
| `GET /api/v1/textures` | 200,~20 KB |
| `GET /` | 200,嵌入 index.html |

### 3.2 `wowsmod render`

| 方式 | 产物 | 大小 | 耗时(约) |
|---|---|---|---|
| `--engine soft --compare` | `out\m6\soft_compare.png` | 870720 | <5 s |
| Python `cdp.py` 对跑中的 preview | `out\m6\cdp_py.png` | 216343 | ~3 s |
| **原生** `wowsmod --json render … --query "view=fq&compare=split&base=base&w=1280&h=720"` | `out\m6\render_fq_split.png` | **723650** | **~52 s**(含起服务+Edge 无头) |

原生 JSON 摘要:`kind=browser`, `engine=browser`, meta 含 `dataURL`(PNG)。

### 3.3 `browser_not_found`

- `--browser C:\no\such\msedge.exe` → 退出码 **3**, JSON `error.code=browser_not_found`(fail-fast,不加载项目几何)。

### 3.4 AI 说明字数(脚本模拟,非第三方 AI 实跑)

见 `scripts/m6/mcp_client.mjs`。仅 AGENTS.md + initialize.instructions + 初始 tools/list 时约 **2356** 字进入上下文;按任务再读 start/project/recolor/glow 主题。第三方 Cursor/Claude/Codex 实跑需用户本机触发(避免消耗用户额度)。

**建议用户自测提示词(深海+夜光):**

```
用 wowsmod 在 deepsea6 项目上:1) status 确认游戏与项目;2) help topic=glow 与 recolor;
3) 按主题做深海涂装+夜光;4) wowsmod_render compare 出图核对;5) 满意再 install。
只读 AGENTS.md 与 help 主题,不要一次灌全文档。
```

## 4. 关键路径

- 代码:`crates/wowsmod/src/{guide,ai,mcp,render}.rs`,`preview/{mod,http,state,cdp}.rs`,`preview_web/`,`docs/topics/*`
- 报告:`M4_REPORT.md`,`M6_REPORT.md`
- Windows 出图:`D:\WoWSModding\wowsmod\out\m6\{soft_compare,cdp_py,render_fq_split}.png`
- 提交号:783a9ca

## 5. 已知限制

- 无种子船的完整 LOD/骨骼几何仍依赖 `assemble --full` 后处理(无炮塔骨骼)。
- 中间历史步骤贴图回看需要 `.wowsmod/objects` 内容寻址;目前 base/head 完整。
- 撤回/恢复 API 返回 501。
- 改动命令 `--json` 自动附带改前/改后/分屏三图路径:渲染管线已具备(`render` / MCP),编辑命令侧按需调用 `preview::render_pngs`(未强制每次改动都出三图,以免拖慢批处理)。
- 原生 CDP 遇协议差异会回退 `preview_app/tools/cdp.py`(同机有 Python 时)。

## 6. 相对 PLAN 的勾选

- [x] M4 稳健安装/卸载/打包/迁移(见 M4_REPORT)
- [x] M6 渐进式披露(AGENTS/help/next/MCP 5 工具)
- [x] M5 前端嵌入 + `preview` + `/api/v1`(含 sim)
- [x] `render` CDP 无头出图 + soft 兜底
- [x] browser_not_found 退出码 3
- [x] MCP render 返回 PNG
- [x] Windows 实机 preview + render 跑通并写入本报告
- [ ] 第三方 AI 客户端实跑深海+夜光(待用户本机,提示词见上)


提交: 783a9ca (本仓库 main)

