# ref.install: 显式应用、安装与卸载
上级: ref
下级: 无
---
```
wowsmod apply <项目> --dry-run           # 查看目标客户端和替换计划
wowsmod apply <项目>                     # 按授权应用，同船 SDK 方案替换
wowsmod conflicts <项目或包>...          # 只查冲突
wowsmod install <项目或.wsmod> --dry-run # 传统安装计划
wowsmod installed --verify              # 已登记文件的哈希检查
wowsmod uninstall <项目或mod名或包>       # 按登记卸载
```
选择、打开和预览方案都不安装；`apply` 执行前确认项目路径、当前→新方案、目标游戏根目录与构建(可显式 `--game <目录>`)。先看 dry-run，再按用户已有授权执行写入；应用不会自动启动游戏。
- 同船 SDK 一次启用一个方案，用 apply 切换；不用 `--allow-same-ship` 叠装。其他船/全局资源仍检查冲突，不能擅自卸载。制作库入口见 `ref.variants`。
- 默认目标为当前 `bin/<构建>/res_mods`；其他安装选项以该命令 `--help` 为准，不把项目 `.install` 存在当当前应用凭据。
- 应用准备资源/冲突计划后事务替换；失败或中断按事务恢复，报恢复失败时先保留现场，不手工删除备份/登记或绕过检查。游戏运行、已安装文件被外部修改等情况先解决明确原因。
- 传统卸载默认保留手改文件；`--force` 会改变保护行为，不能作为例行步骤。最后一个 mod 卸载也不能据此宣称外部改动已还原。
- `installed --verify` 只证明登记文件完整；SDK 注册查日志，视觉/动画按 `workflow.layers.verify.port` 验收。旧构建或未知构建表示待验证，不直接判不兼容。
新入口需要含 apply 的构建；若 `--help` 不支持，先核对版本，不以同船强制叠装替代。
