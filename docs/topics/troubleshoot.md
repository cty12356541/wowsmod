# troubleshoot: 常见问题

| 现象 | 处理 |
|---|---|
| 没有找到游戏 | `wowsmod doctor`;用 `--game <游戏根目录>` 指定(该目录含 res_packages 和 bin) |
| 不是 mod 项目 | 先 `wowsmod mod init`;路径要指向含 mod.toml 的目录 |
| 不是 SDK 路线项目 / 还没 import-sdk | `wowsmod help project` |
| 游戏正在运行 | 退出游戏后重试 |
| 冲突 | `wowsmod conflicts <项目>` 查看;先卸载另一个,或改用另一个项目 |
| 进港看不到改动 | 确认 `wowsmod installed --verify` 通过,且游戏是重启后进的港口。SDK mod 要看 python.log 里有没有 registerShipMod;颜色没变时检查 `_mg` 的 B 通道(`help recolor`) |
| 改完想撤销 | 模型用 `model undo`;已安装的 mod 用 `uninstall` |
| 不确定某件事 | 不要猜,先用 `--dry-run` 或 `render` 验证,并如实说明不确定 |

日志位置:游戏 `profile/python.log`;wowsmod 的每次编辑记录在项目的 `edits/` 下。
