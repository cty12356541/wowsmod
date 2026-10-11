# root: wowsmod 入口(先判断要做什么)
上级: -
下级:
- workflow: 要设计/做一整套外观 mod(复刻参考 / 角色主题 / 原创)时
- ref: 只需要某个命令的用法(换色、发光、模型、出图、安装、打包…)时
- styles: 设计书 §2 采用了某个风格包时
- learn: 社区制作方法摘要(贴图痛船/外带建模/发光/ModsSDK)
---
wowsmod 是 Lesta《战舰世界》客户端改模命令行工具; 所有命令可加 `--json`, 结果里的 `next` 是建议下一步。

1. 先 `wowsmod status [项目]`: 游戏、已装 mod、SDK 素材、项目状态。
2. 路由: 整套外观 → `wowsmod help workflow`; 单个操作 → `wowsmod help ref`。
3. 读法: `wowsmod help --tree` 看整棵树; `wowsmod help <id>` 读一个节点; `wowsmod help <id> --children` 只看下级。每个节点开头写了上级/下级, 只在需要时往下读。

通用规则: 修改只写进项目目录(`edits/` 可撤销); 游戏目录只由 `install` 写、`uninstall` 完全还原; 游戏运行时 install/uninstall 会拒绝; 改完先出图再装; 进港只看港口, 不进战斗; 训练房需用户授权。
