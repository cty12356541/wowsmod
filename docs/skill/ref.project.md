# ref.project: mod 项目与 SDK 导入
上级: ref
下级: 无
---
## 两种模式
- `sdk`(推荐,用于改整条船):走官方 ModsSDK。mod 放在 `res_mods/PnFMods/<名>/<船>/`,游戏启动时由 `contentSdk.registerShipMod` 注册,可以改 .visual / .mfm / 几何 / 贴图。
- `loose`:直接按游戏内路径覆盖文件,适合只换贴图。

## 新建 SDK 项目
```
wowsmod mod init <目录> --name <英文名> --ship <SDK 船名> --mode sdk
wowsmod mod import-sdk <目录>        # 本机导入过这条船时, 素材位置会自动沿用
wowsmod mod status <目录>
```
- `--name` 只能用 ASCII 字母、数字、下划线,它会成为 `PnFMods/<名>` 目录名。
- 船名用 SDK 名(如 `FSB510_Bourgogne`)。不知道船名时用 `wowsmod find <关键字> --ship` 查。

## 首次准备(每条船只需一次)
`import-sdk` 需要两样素材:
1. **extractSources 产物**:在游戏里执行官方 `contentSdk.extractSources(<mod名>, <船>)` 生成,内容是 `PnFMods/<名>/<船>/`,含 compile.info、明文 .visual/.mfm/.model 和资源占位文件;
2. **ModsSDK 船目录**:官方 ModsSDK 压缩包(如 `ModsSDK_26.10.0.zip`)解压后的 `<船>/` 子目录。

```
wowsmod mod import-sdk <项目> <产物目录> --sdk-dir <ModsSDK.zip/<船> 目录>
```
导入成功后,本机会记住这两个位置(记录在 `<WOWSMOD_HOME>/sdk_sources.json`),之后同一条船的新项目可以省略这两个参数。
- `wowsmod status` 会列出已记录的船。
- 没有记录、也没有这两样素材时:**无法**自动生成,需要人在游戏里执行一次 extractSources。不要编造路径。

## 项目结构
`mod.toml`、`edits/`(每次编辑的记录与撤销数据)、`assets/res_mods/`(安装时按相对路径复制)、`.render/`(出图)。
二进制资源在项目里是 SDK 占位文件(一行 XML);install 时从游戏补齐真实文件,所以项目和包都很小。
