# ref.migrate: 游戏更新后迁移
上级: ref
下级: 无
---
游戏更新后,`bin` 下会出现新的版本号目录,res_mods 也随之换到新目录,旧目录里的 mod 不会生效。

```
wowsmod migrate --dry-run                 # 自动找带登记表的旧版本目录, 迁到当前版本
wowsmod migrate --from 8863954            # 指定旧版本号(或旧 res_mods 目录)
wowsmod migrate --keep-old                # 迁移后保留旧目录里的安装
```
- 每个 mod 按新版本重新补齐原版资源。结果里的 `game_files_updated_since_install` 列出随更新变化过的原版资源;这些文件对应的编辑最好进港复查。
- 迁移要用到原来的项目目录(或包的解包目录)。如果它已被删除,改用 `wowsmod install <包>` 重新安装。
- 如果更新改了船的资源结构,某个 mod 可能迁移失败,结果里会单独列出原因。能否自动修好**不确定**。
