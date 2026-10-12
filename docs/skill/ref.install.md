# ref.install: 安装 / 卸载 / 冲突
上级: ref
下级: 无
---
```
wowsmod conflicts <项目或包>...          # 只检查, 不写文件
wowsmod install <项目或 .wsmod> [--dry-run]
wowsmod installed --verify               # 列出已装 mod, 逐文件核对哈希
wowsmod uninstall <项目 / mod 名 / 包>   # 完全还原
```
- 安装目标是当前游戏版本的 `bin/<版本>/res_mods`(可以用 `--res-mods` 指定)。
- 每个文件都记录哈希,被覆盖的原文件会备份(`res_mods/.wowsmod/`)。安装中途失败会自动回滚。
- 卸载只删除哈希仍与安装时一致的文件。被手动改过的文件默认保留,加 `--force` 才删除。最后一个 mod 卸载后,res_mods 回到安装前的原样。
- 冲突规则:
  - 同一路径、内容不同 → 拒绝;
  - 两个 SDK mod 改同一条船 → 拒绝(`--allow-same-ship` 可强制,但游戏用哪个不确定);
  - 内容相同的文件会共用。
- 游戏运行时会拒绝执行。装好后重启游戏才能看到效果。
- 进港检查时只看港口,不要进战斗。
