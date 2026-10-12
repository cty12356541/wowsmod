# AGENTS.md — wowsmod 代理入口

按需逐层读取，不要一次性读完整棵树。

1. 构建：`cargo build --release`。Oodle 在运行时从本机 DLL 加载（`WOWSMOD_OODLE`，见 `docs/OODLE.md`）。
2. 环境自检：`wowsmod doctor`，然后 `wowsmod status [项目]`。
3. 技能树入口：`wowsmod help`（等同 `docs/skill/root.md`）。
   - 整套外观 mod → `wowsmod help workflow`
   - 单个命令用法 → `wowsmod help ref`
   - 风格包 → `styles/`；社区方法摘要 → `wowsmod help learn`
   - 整棵树：`wowsmod help --tree`；只看下级：`wowsmod help <id> --children`
4. MCP：`wowsmod mcp`（5 个入口工具，`wowsmod_expand` 展开分组），详见 `wowsmod help ref.mcp`；客户端配置见 README「AI 工作流搭建」。Blender 只以无头方式调用（`ref.blender`），不使用 Blender MCP。
5. 离线也可直接读 `docs/skill/<id>.md`，每个节点开头写有上级/下级。

规则：
- 只写项目目录；游戏目录仅由 `install` 写、`uninstall` 还原。
- 不提交、不分发任何游戏资源、Oodle 二进制、第三方模型/视频帧（见 README 免责声明）。
- 参考素材（如 Robin、Krishna 示例）只以文字引用，由用户自行合法获取。
- 设计/实现背景：`docs/DESIGN.md`；前端源码：`preview_app/web`（`npm ci && npm run build` → 复制 `dist/` 到 `crates/wowsmod/preview_web/`）。
