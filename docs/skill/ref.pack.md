# ref.pack: 打包分发
上级: ref
下级: 无
---
```
wowsmod pack <项目> [-o <文件或目录>]     # → <名>-<版本>.wsmod
wowsmod install <包.wsmod>                 # 校验每个文件的 sha256 后安装
```
- 包是 zip,内含:`wowsmod-package.json`(文件清单与哈希)、`mod.toml`、`assets/res_mods/**`、`README.txt`。同一项目重复打包,结果完全相同。
- 不打包的内容:
  - 游戏原版资源:占位文件保持占位,安装时从使用者的游戏里补齐;
  - Oodle(oo2core*)、pfsunpack/mkut 等专有工具;
  - 任何可执行文件或脚本;
  - `.install/` 与撤销数据。
- `--full` 会把补齐后的原版资源也打进包,仅供个人备份,不要分发。
- **受限素材硬拦截**: mod.toml 有 `[license] no_redistribute = true`(导入受限模型时自动写入)时 `pack` 报错退出并列出原因; 只在本机 `install`。确需自用备份: `pack --allow-restricted`(README 写明【受限】, 不得公开分发)。网页/口头声明的许可(扫描不到的)手动标: `wowsmod mod license <项目> --note "<来源+原文要点>"` 或导入时 `--license-note`; 去掉受限素材后 `mod license <项目> --clear`。
