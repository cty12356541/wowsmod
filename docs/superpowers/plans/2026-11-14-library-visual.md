# 制作库视觉统一实施计划

Spec: `docs/superpowers/specs/2026-11-14-library-visual-design.md`

## Task 1: 复用工作台视觉规范

所有权：`preview_app/web/src/styles/app.css` 的 library 相关规则，必要时 `components/Library.tsx` 的展示结构；不得更改 API、store 或模拟代码。阅读 `docs/DESIGN.md` 与既有工作台控件，移除 library 专属暗色 token，重用基础控件，统一字级、边框、间距、标题和状态。保留所有业务行为、多语言及无障碍标签。根代理拥有生成产物，worker 不提交 `preview_web`。

## Task 2: 验证与交付

Typecheck、现有 library 测试；不为纯 CSS 添加镜像测试。独立视觉验收首页、单船方案页、筛选/表单状态及窄屏溢出，与工作台对照。根代理统一构建、保护已导入库数据、更新本地服务并在右侧浏览器展示。

并行约束：共享工作树，保留其他智能体改动。模拟与皮肤绑定另有所有者；仅 stage 明确拥有的文件。
