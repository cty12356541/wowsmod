# M4 报告:打包与安装完善

状态:**已完成**(2026-10-10)。假 res_mods 实测 36/36 通过;进港验证 1 次,结束后 res_mods 已复原为只有 readme.txt(SHA-256 0157BF75…2F3F)。

## 1. 做了什么

| 命令 | 作用 |
|---|---|
| `wowsmod install <项目目录 或 .wsmod>` | 先检查冲突,再逐文件原子写入(写临时文件后改名,写完回读校验);中途失败自动回滚;登记每个文件的哈希和被覆盖原文件的备份 |
| `wowsmod uninstall <项目目录 / mod 名 / 包>` | 只删除哈希仍与安装时一致的文件;安装后被改过的文件默认保留(`--force` 才删);还原备份前先校验备份的哈希;删除 wowsmod 自己建的空目录;最后一个 mod 卸载后连 `.wowsmod/` 一起删除 |
| `wowsmod installed [--verify]` | 列出已装的 mod;加 `--verify` 时逐文件核对哈希和备份 |
| `wowsmod conflicts <项目/包…>` | 只检查,不安装:这些项目/包之间以及与已装 mod 之间的冲突。有冲突时退出码为 2 |
| `wowsmod migrate [--from 旧版本号或目录] [--to …] [--keep-old] [--dry-run]` | 游戏更新、bin 下出现新版本号目录后,把 mod 迁到新的 res_mods。按新版本重新补齐原版资源,并报告哪些原版资源已经随更新变化 |
| `wowsmod pack <项目> [-o 文件或目录] [--full]` | 生成可分发的 `.wsmod` 包,内容确定:同一项目重复打包,sha256 相同 |

- **登记表**位于 `res_mods/.wowsmod/registry.json`,内容有:
  - 每个 mod 的来源、版本、船、项目目录;
  - 每个路径的哈希、所属 mod(引用计数)、备份哈希、来源(project / game-fill / generated)。
  - 备份放在 `.wowsmod/backup/`;
  - 锁文件 `.wowsmod/lock`,超过 600 秒视为过期。
- **冲突规则**:
  - 同一路径、内容相同 → 共用;
  - 同一路径、内容不同 → 拒绝;
  - 两个 SDK 模式的 mod 改同一条船 → 拒绝(游戏会用哪个不确定),可用 `--allow-same-ship` 强制。
  - SDK 路线需要的空 `PnFModsLoader.py` 由 wowsmod 生成,多个 mod 共用;用户自己的 loader 不会被接管。
- **游戏运行检测**:install / uninstall / migrate 检测到 Korabli64 正在运行时拒绝执行(`--ignore-running` 可跳过)。
- **包格式**(`.wsmod`,即 zip):
  - 内容:`wowsmod-package.json`(每个文件的 sha256 与占位标记)、`mod.toml`、`assets/res_mods/**`、中文 `README.txt`;
  - 安装前逐文件校验 sha256,损坏的包会被拒绝;
  - 安装时解包到 `WOWSMOD_HOME/packages/<名>-<版本>-<sha8>/`,游戏原版资源从本机游戏补齐;
  - **不打包**:
    - Oodle 运行库(oo2core* / *oodle*)、pfsunpack* / mkut*;
    - 任何可执行文件或脚本(exe / dll / pyd / so / dylib / bat / cmd / ps1 / msi);
    - `.install/` 与撤销数据;
    - 游戏原版资源(SDK 占位文件保持占位,安装时补齐)。
  - `--full` 会把补齐后的原版资源也打进包,标为“仅限个人使用”,不建议分发。
- **zip 读写**(`wowsmod-formats/src/zipfile.rs`)由本项目自己实现,deflate 用 miniz_oxide(MIT/Apache)。读取时检查 CRC 与路径安全(禁止 `..` 与绝对路径)。

## 2. 验收

### 2.1 假 res_mods 实测(Windows,`scripts/m4/m4_fake_tests.ps1`)

所有测试只写 `out\m4\fake`。日志:`out\m4\fake\log.txt`。结果 **36/36 PASS**。

| 组 | 内容 | 结果 |
|---|---|---|
| T1 | 安装 deepsea6,`--verify` 通过;卸载后与安装前逐文件(含目录)完全一致 | 通过 |
| T2 | remove(同船 SDK)报冲突,退出码 2,安装被拒,被拒后 deepsea6 完好;loose 贴图 mod 与 deepsea6 共存;按任意顺序卸载后完全复原 | 通过 |
| T3 | 目标位置已有用户自己的文件:安装时备份,卸载时还原,其余文件一致 | 通过 |
| T4 | 安装后被改动的文件:`verify` 报出,卸载时默认保留,`--force` 后可回到干净状态 | 通过 |
| T5 | pack:527 个条目,20.1 MB,450 个 mod 文件(其中 74 个占位文件由安装时补齐);包内没有专有组件或可执行文件;从包安装和校验都通过;损坏的包被拒;按 mod 名卸载后完全复原 | 通过 |
| T6 | migrate:两个 mod 从旧目录迁到新目录,`--dry-run` 不改动任何文件;迁移后新目录校验通过,旧目录已复原;新目录卸载后复原 | 通过 |

此外,单元测试(`cargo test`)全部通过,其中包括登记表的共存 / 共用 / 冲突 / 复原,以及中途失败回滚。

### 2.2 进港验证(`scripts/ingame/m4_game.ps1`,1 次会话,只在港口,不进战斗)

完整流程:

1. `pack deepsea6` 生成 `out\m4\dist\DeepSeaBourgogne-0.1.0.wsmod`,sha256 `02c851c8…b963`,与 T5 那次打包完全相同,说明包内容是确定的。
2. 把包**装到模拟的旧版本目录** `out\m4\fake_bin\8863954\res_mods`(本机 `bin` 下确实有旧版本 8863954,但真实目录不允许写入,所以用这个模拟目录)。
3. `migrate --from <旧目录>`,`--to` 取默认值,即当前版本 `D:\Korabli\bin\8867689\res_mods`:
   - 写入 541 个文件;
   - 旧目录里 541 个文件全部删除,21 个空目录清理掉;
   - `installed --verify` 显示完好。
4. 进港后:
   - python.log 中 `registerShipMod` 成功,26 个模型被替换,没有意外错误,没有崩溃;
   - 截图中勃艮第船体为深海蓝、烟囱发光,与 M2 的效果一致;
   - 已安装的汉化 mod(MK_L10N_CHS)界面正常,两者可以共存。
5. 按“是”退出后执行 `uninstall DeepSeaBourgogne`:删除 541 个文件,`res_mods_pristine = true`。
6. 再次核对:真实 res_mods 只剩 readme.txt,哈希与原始值一致;旧目录只剩模拟用的 readme.txt。

截图位置:
- Windows:`out\m4_ingame\migrated\`;
- box:`/workspace/m4/ingame/m4_*_small.png`。

## 3. 已知限制 / 不确定

- 真实的游戏更新还没遇到过。这次是用另建 bin 目录的方式模拟。如果新版本改动了 SDK 结构或船的资源路径,migrate 会把失败的 mod 和原因单独列出,但能否自动修好**不确定**。
- 从包安装的 mod 依赖 `WOWSMOD_HOME/packages/` 下的解包目录才能 migrate。这个目录被删除后,需要重新 `install <包>`(出错信息会提示这一点)。
- 两个 SDK mod 同时改同一条船时,游戏实际用哪一个**不确定**,所以默认拒绝。
- 旧版 `.install/manifest.json` 方式安装的 mod 仍可卸载(兼容路径),但不进入登记表。
