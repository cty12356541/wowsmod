# ref.variants: 制作库与同船多方案
上级: ref
下级: 无
---
```
wowsmod library add <项目目录>
wowsmod library scan <指定制作根目录>
wowsmod library list
wowsmod library update <id> --name "<方案显示名>" --status review --favorite true --tag "<标签>"
wowsmod library relocate <id> <新目录>
wowsmod library remove <id>
wowsmod preview --library
wowsmod apply <项目目录> --dry-run
```
这是制作库新入口；先以相应命令 `--help` 核对所用构建，不支持时不能编造替代参数。扫描只针对明确目录，不扫全盘。remove 仅取消库登记，不删除项目/资产，不卸载游戏；relocate 保留库 ID，需给出实际新路径。

**身份与状态**：库 ID/规范路径识别方案，`mod.name`、同名目录或 `.install` 不能替代身份；不同路径同名项目分别保留。制作状态 working/review/complete/archived 由用户维护，和当前应用、文件异常、需版本验证等安装信息分开；“已完成”不证明实机验收。失效目录重新定位，未知船/无目标船不猜国家、等级或舰种。

**选择与应用**：在同船方案中选择/打开只改变工作台；确认项目路径与面包屑后再编辑。第一版切换完整重载，多个标签页各自指向明确项目。只有“应用到游戏”或明确 apply 执行才写游戏；先核对 dry-run 的客户端、构建、当前→新方案，再按授权 `wowsmod apply <项目目录>`。不会自动开游戏。

**同船互斥**：一次应用一个 SDK 方案，用事务替换，不用 `--allow-same-ship` 叠装。其他船/全局资源仍有冲突检查。文件被外部改动、恢复未完成或游戏运行时先解决原因；版本不同/未知标待验证。安装和证据边界见 `ref.install`。
