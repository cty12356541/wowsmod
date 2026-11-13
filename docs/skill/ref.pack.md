# ref.pack: 打包与分发边界
上级: ref
下级: 无
---
```
wowsmod pack <项目> [-o <文件或目录>]
wowsmod install <包.wsmod> --dry-run
```
标准 wsmod 是 zip，含 `wowsmod-package.json`(文件/哈希)、mod.toml、assets/res_mods 与 README.txt。相同项目重复打包结果确定；实际安装仍需授权和冲突检查。

**SDK 必需文件保留**：`PnFMods/<名>/Main.py` 是注册入口，属于项目内容，必须随 SDK 包保留；不能按“所有脚本都排除”删除它。根 PnFModsLoader.py 由安装按需提供。当前打包过滤拦截 exe/dll/pyd/so/dylib/bat/cmd/ps1/msi 与 Oodle、pfsunpack/mkut 等专有组件，不表示会删掉全部 Python 文件。

**原版资源**：项目里的二进制占位在标准包中保留，由安装从使用者目标客户端补齐；已修改的真实资源仍在包中。`--full` 另附补齐的 res_mods 树，只供个人备份，不公开分发。`.install/` 与撤销备份不作为分发内容。

**许可**：`[license] no_redistribute = true` 默认阻止 pack；`wowsmod mod license <项目> --note "<来源与限制>"` 可手动记录扫描不到的许可。`--allow-restricted` 只作获准的本人备份，不能创造分发许可；移除素材并核对剩余许可后才考虑 clear。
**验证**：包哈希、安装文件、SDK 注册和实机效果是不同阶段。包成功生成不代表当前版本兼容或已验收；记录目标船、制作构建、许可与未验证事项。
