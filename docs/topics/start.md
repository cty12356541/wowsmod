# start: 从零开始 / 任务路由

wowsmod 是 Lesta《战舰世界》(Мир кораблей)客户端的改模命令行工具。所有命令都可以加 `--json`,输出机器可读的结果,其中 `next` 字段给出建议的下一步。

## 第一步
1. `wowsmod status [项目目录]`:查看游戏位置、版本、res_mods 里已装的 mod、本机记录过的 SDK 素材和项目状态。
2. 按任务类型看对应主题(`wowsmod help <主题>`):

| 想做的事 | 主题 |
|---|---|
| 新建 mod 项目(SDK 路线,改整船) | project |
| 改颜色 / 涂装 / 去迷彩 | recolor |
| 让部件发光 | glow |
| 缩放 / 移动 / 删除 / 替换部件,导入新网格 | model |
| 用 Blender 改模型 | blender |
| 看效果(出图) | render |
| 装进游戏 / 卸载 / 冲突检查 | install |
| 打包分发 | pack |
| 游戏更新后迁移 | migrate |
| 报错了 | troubleshoot |
| 文件格式、坐标系、SDK 原理 | formats |
| 给 AI 工具接 MCP | mcp |

## 通用规则
- 所有修改都写在**项目目录**里,每一步记录在 `edits/` 下。游戏目录只由 `install` 写入,`uninstall` 能把它完全还原。
- 游戏运行时不要执行 install / uninstall / migrate,命令会拒绝。
- 改完先 `wowsmod render <项目> --compare` 看图,再 install;实际观感以进港截图为准。
- 不确定时用 `--dry-run`(install / uninstall / migrate / extract 都支持)。
