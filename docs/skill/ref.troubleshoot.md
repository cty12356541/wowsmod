# ref.troubleshoot: 常见问题
上级: ref
下级: 无
---
| 现象 | 处理 |
|---|---|
| 没有找到游戏 | `wowsmod doctor`;用 `--game <游戏根目录>` 指定(该目录含 res_packages 和 bin) |
| 不是 mod 项目 | 先 `wowsmod mod init`;路径要指向含 mod.toml 的目录 |
| 不是 SDK 路线项目 / 还没 import-sdk | `wowsmod help ref.project` |
| 游戏正在运行 | 退出游戏后重试 |
| 冲突 | `wowsmod conflicts <项目>` 查看;先卸载另一个,或改用另一个项目 |
| 进港看不到改动 | 确认 `wowsmod installed --verify` 通过,且游戏是重启后进的港口。SDK mod 要看 python.log 里有没有 registerShipMod;颜色没变时检查 `_mg` 的 B 通道(`help ref.recolor`) |
| 改完想撤销 | 模型用 `model undo`;已安装的 mod 用 `uninstall` |
| 不确定某件事 | 不要猜,先用 `--dry-run` 或 `render` 验证,并如实说明不确定 |

日志位置:游戏 `profile/python.log`;wowsmod 的每次编辑记录在项目的 `edits/` 下。


## Oodle / 许可证
- `doctor` 显示 Oodle 后端为「未编译」:未找到本机 Oodle 运行库: 设 `WOWSMOD_OODLE=<oo2core*.dll 或目录>` 或放在 exe 同目录(见 docs/OODLE.md)。
- 发布给用户的 exe 不要链接 GPL 移植的解码器;优先本机游戏已有的 Oodle 或用户自备 `oo2core*.dll`。
- 不要把从游戏目录复制的 DLL 打进 .wsmod 包。

## Windows PowerShell 与 --json
PowerShell 管道会按 OEM 代码页弄坏 UTF-8 JSON。不要 `wowsmod --json ... | Out-File`。
用全局 `--json-out <文件>`(隐含 `--json`, 结果按 UTF-8 写文件, 失败也写错误 JSON)。以 `-` 开头的参数值写成 `--box=-2,…`。
| 换色后出现粉/怪色斑 | 在已换过色的层上再 recolor 时用 `--chroma-detail 0`(原色斑会被放大成杂色) |
| `--glb --box` 色带没出现 | `--exclude-hue` 把上一步刷的浅黄/米白也排除了: 收窄成只排木甲板(如 5:45) |
