# RENDER_WARM_REPORT — 常驻出图(预热)进度

更新: 2026-10-10 20:25 (Asia/Shanghai)

## 插队任务: 克利希娜 mod 不在预览工作台 (已修, cf19dc6)
- 根因: `preview::ensure_steps_json` 在项目没有 `edits/steps.json` 时, 把 mock 种子(preview_app/mock/data/FSB510, 勃艮第"深海涂装"故事)拷进真实项目。
  `out/e2e/bourgogne_krillishna` 因此时间线显示「基线 / 整船换深海蓝 / 烟囱加蓝色发光」3 步(最后一步引用不存在的 028, 时间显示 1月1日), 而不是克利希娜自己的 9 次改动。
  模型/贴图本身(head 扫描)是对的, 所以看起来像"别的 mod"。
- 修复: 不再写项目; 识别并忽略已被拷入的种子 steps.json; steps.json 与 edits 对不上时补齐未引用的 edit、丢掉空步骤; 每步标题改为可读(如「发光 Hull_mg #00E5FF 强度 0.35」)。
- 截图脚本: `scripts/preview/workbench_shot.py <URL> <目录> 1920x1080 <前缀>`(总览 / 分屏 / 时间线最新步 / 暗色影棚发光)。
- 实测(只读快照 out/wb_snap/*, 不碰对方项目): 3 个项目页面 3.2–3.3 s 加载完成, 无控制台错误; 发光与新贴图正常显示。

## 常驻出图(进行中)
- 前端: render.ts 新增 `window.__renderReload()`(增量: 重新取 manifest, 几何按哈希、贴图按内容哈希复用)、`__renderApi=2`; 重建产物与原 app.js 逐字节可复现。
- cdp.rs: 每次读都带截止时间(旧实现只在收到消息后才检查超时, 套接字 180 s 读超时); 崩溃/断开检测; 可复用的 `Page`; cdp.py 回退加 180 s 超时。
- state.rs: 按 (大小, mtime) 记忆文件哈希、游戏贴图 vpath 哈希、一次性文件名索引; 几何缓存文件变了才重读; `reload()` / `fingerprint()`。
- http.rs: `/__wm/*` 控制端点钩子。
- 待做: warm.rs 守护进程 + `render --warm/status/stop` + MCP 后台预热 + 文档 + Windows 计时。

## 之前超时的根因(初查)
- E2E 18:03 `browser render TIMEOUT skipped`: 出图本身 ~12 s 已完成, 但 `render_pngs` join tiny_http 线程永久阻塞(只 connect 不发请求不会唤醒 `incoming_requests`), 进程不退出 → 脚本超时。de872a4 已用真实 HTTP 请求唤醒 + 3 s join 兜底修复。
- 其余隐患(本任务修): CDP 读无截止时间、前端贴图 fetch 无超时、cdp.py 回退无超时、MCP 出图在进程内同步执行(任何一处挂起 = MCP 挂起)。

## 完成(2026-10-10 20:55 Asia/Shanghai)
提交: 055369c(常驻服务主体), 7d576cc(增量重载提速 + 浏览器随进程退出 + 计时脚本), f62d811(文档)。均只 add 自己改的文件; 对方未提交的 design.rs / m2cmd.rs 未动。

### 新命令
| 命令 | 作用 |
|---|---|
| `wowsmod render --warm <项目> [--idle 分钟]` | 后台起常驻出图实例(独立进程 `render serve` + 无头 Edge, 已加载几何/贴图); 已在跑则直接返回 |
| `wowsmod render status <项目>` / `status --all` | 就绪/启动中/卡死/已退出; pid、浏览器 pid、已出图数、空闲时长、预热耗时 |
| `wowsmod render stop <项目>` / `stop --all` | 优雅停止(8 s 不退则强杀进程树) |
| `wowsmod render <项目> ...` | 实例就绪时自动连它(`--json` 里 `warm: true` + timing); `--cold` 强制冷启动 |
| `--timeout <秒>`(默认 60) | 单张超时; 超时/浏览器崩溃 → 实例内重开浏览器重试一次 → 客户端重启实例再试一次 |
- 状态文件 `<项目>/.wowsmod/render_warm.json`, 日志 `.wowsmod/render_warm.log`, 登记 `%TEMP%/wowsmod-render-warm/`。
- 空闲退出: 默认 15 分钟(`--idle` / `WOWSMOD_RENDER_IDLE_MIN`), 实测 `--idle 1` 到点退出, 浏览器一起关, 状态文件清掉。
- 错误码: `render_timeout` / `browser_crashed` / `warm_start_failed` / `browser_not_found` / `bad_param` / `render_failed`, 每个带下一步提示; 退出码 2 = 参数, 3 = 找不到浏览器, 4 = 其他。
- MCP: `wowsmod_render` 不再在 MCP 进程内渲染, 改为确保实例 + HTTP 请求(实例挂了 MCP 不受影响, 自动重启实例); `wowsmod_status` 带项目时后台预热; MCP 启动时当前目录是项目(或 `WOWSMOD_PROJECT`)也预热。
- Python 兜底(cdp.py)加 180 s 超时; 明确的超时/参数错误不再落到 Python 兜底重跑。
- 无头浏览器放进 kill-on-close Job: 强杀常驻进程(或冷启动 CLI)后浏览器跟着退出(实测 0 个残留)。

### Windows 实测(Krillishna V2 副本 out/warm_test/kv2, 1600x900 分屏 base|head, GPU)
| 场景 | 耗时 |
|---|---|
| 冷启动单张(`--cold`, 含起服务 + 浏览器 + 加载) | 7.9 s(提速前 16.0 s) |
| 预热 `render --warm` | 3.6 s(项目状态 0.4 s + 浏览器 1.3 s + 首张 1.8 s) |
| 预热后单张(CLI 全程) | 0.13 s(服务端 0.10 s) |
| 连续 10 张(4 个视角轮换) | 共 0.82 s, 平均 0.08 s, 最慢 0.11 s |
| 改一张贴图后(tex recolor 船体)第一张, 含增量重载 | 0.50 s(后端重载 0.06 s, 4 张新贴图 / 66; 前端 0.24 s) |
| 增量重载后下一张 | 0.09 s |
| MCP `wowsmod_render`(实例已热) | 0.10 / 0.12 / 0.10 s |
| MCP 首次调用自动起实例 | 3.7 s, 之后 0.09 s |
| 浏览器被杀后下一张(自愈) | 3.4 s, 成功, browser_restarts=1 |
| 常驻进程被杀后 MCP 调用(自动重启实例) | 6.1 s, 成功 |
目标"预热后单张 ≤2 s"达成(约 0.1 s)。
- 增量重载第一次实测 8.0 s → 定位到 `resolve_tex` 对每张贴图线性扫描全部游戏索引条目; 改成一次建好的文件名索引后 0.06 s(冷启动也因此从 16 s 降到 8 s)。
- 计时脚本: `scripts/render/warm_bench.ps1 <exe> <项目副本> <贴图.dds>`, `scripts/render/mcp_bench.py <exe> <项目> [次数]`。

### 超时根因(总结)
1. 18:03 E2E 的 TIMEOUT: 图 ~12 s 就出完了, 卡在收尾 join tiny_http 线程(只 connect 不发请求唤不醒 `incoming_requests`), 进程不退出 → 脚本超时(de872a4 已修)。
2. 潜在问题(本次修掉): CDP 读只在收到消息后才检查超时(实际靠套接字 180 s); cdp.py 兜底无超时; MCP 在自己进程里同步出图(一卡全卡)。现在每次读都有截止时间, 超时有明确错误码并自动重试一次。

### 顺手修
- 顶部「null 级」: 后端从 GameParams 取等级/国家/类型(后台线程解析, 按版本缓存到 %TEMP%/wowsmod_shipmeta_*.json), 取不到就不显示; 实测显示「Bourgogne 10 级 · 战列舰」。
- 步骤标题里贴图缩写改为部件中文名(按 parts.json 材质归属 + 名字关键字): 如「换色 船体 → #1E1F23」「发光 主炮/副炮炮管 #D0F6FF 强度 0.35」「换色 上层建筑」。
- 前端源码同步到 D:\WoWSModding\preview_app\web(render.ts、TitleBar.tsx)和 dist/assets/app.js。

### 文档(只追加)
- AGENTS.md(由 `wowsmod ai init` 生成, ai.rs): 流程第 3 步后加一句「要出图先预热: wowsmod render --warm <项目> …」。
- `wowsmod help render`: 新增「预热: 常驻出图实例」一节; `render --help` 末尾与冷启动结果提示「下次先 wowsmod render --warm <项目>」(已热时不再提示)。
- 追加段落: docs/topics/mcp.md、design.md、design-verify.md、start.md。
