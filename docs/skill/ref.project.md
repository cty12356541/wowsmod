# ref.project: 项目与 SDK 来源
上级: ref
下级: 无
---
**模式**：`sdk` 用 ModsSDK 注册船外观，可改 visual/mfm/网格/贴图；`loose` 按游戏路径覆盖，适合局部或全局资源。先确认目标船与使用范围。
```
wowsmod mod init <目录> --name <英文名> --ship <SDK船名> --mode sdk
wowsmod mod import-sdk <项目>
wowsmod mod status <项目>
```
`--name` 仅 ASCII 字母、数字、下划线，作为 PnFMods 目录名；它不是制作库身份。SDK 船名如 `FSB510_Bourgogne`，用 `wowsmod find <关键词> --ship` 查。多方案管理见 `ref.variants`。

**首次导入需要两份实际素材**：游戏里 `contentSdk.extractSources(<mod名>, <船>)` 生成的船目录(含 compile.info、明文配置与占位资源)，以及对应 ModsSDK 压缩包解出的船目录。
```
wowsmod mod import-sdk <项目> <extractSources产物> --sdk-dir <ModsSDK船目录>
```
没有素材时不能自动生成或编造路径；需要启动游戏导出时先取得相应授权。工具会在 `<WOWSMOD_HOME>/sdk_sources.json` 记住来源位置；缓存沿用不证明版本匹配。项目记录应注明两份来源的船、客户端构建、SDK 版本与日期，缺失写未知；不同构建先核对资源结构，见 `ref.migrate`。

**结构**：`mod.toml` 元数据与制作构建；`edits/` 编辑/撤销记录；`assets/res_mods/` 安装相对路径；`.render/` 预览产物。
**占位区别**：项目/标准包中的二进制 XML 占位合法，用来在安装时从目标客户端补齐未修改资源；导入或编辑后的真实资源仍保留。安装后应为可用游戏资源，不能把遗留 XML 占位当成成功。文件补齐、注册与画面可见分开验证。
