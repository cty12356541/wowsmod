# ref.formats: 格式与原理
上级: ref
下级: 无
---
- 资源包:`res_packages/*.pkg` 加 `bin/<版本>/idx/*.idx`。莱服用 Oodle 压缩,wowsmod 用纯 Rust 读取,只读不写。
- `res_mods/` 下的同路径文件会覆盖包内资源(loose 路线)。SDK 路线走 `res_mods/PnFMods/<名>/`,由空文件 `PnFModsLoader.py` 和 mod 的 `Main.py` 注册。
- 贴图:DDS(BC1/3/5/7)。大贴图拆成 `.dd0`/`.dd1`… 分卷,部分是 BC7Prep 变体;wowsmod 会自动合并、还原。
  - `_a`:漫反射;
  - `_mg`:R 光泽、G 金属、B 迷彩遮罩或发光;
  - `_n`:法线;`_ao`:环境光遮蔽。
- 几何:`.geometry` 由顶点/索引块组成,用 meshopt 编码;`.visual` 的渲染集通过名字的 MurmurHash3 对应到具体块。
- 坐标:游戏用左手系(x 右舷、y 上、z 船艏);GLB 输出为右手系(z 取反)。
- SDK:`ModsSDK.zip/<船>/` 是明文 .visual/.mfm/.model;`content.info` 和 `compile.info` 是 base64(zlib(JSON)) 编码的路径映射。
- 详细设计见仓库 `docs/DESIGN.md`。
