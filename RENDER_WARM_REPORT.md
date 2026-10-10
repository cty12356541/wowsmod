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
