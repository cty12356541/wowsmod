# learn.makers.sdk-pipeline: ModsSDK / ContentSDK 管线
上级: learn.makers
下级: 无
---
**官方步骤(WG DevTracker / 莱服 BattleFrame / 巴哈雨思 一致)**:
1. 取版本匹配 `ModsSDK_*.zip`, 内容放入 `res_mods/.../PnFMods/ModsSDK.zip/`(**文件夹名**含 .zip)。
2. 空 `PnFModsLoader.py`; 建 mod 目录与 `Main.py`: `extractSources(MOD_NAME, SHIP_NAME)`。
3. 进游戏等导出 → 船目录出现 → 把 `Main.py` 改成 `registerShipMod(SHIP_NAME)`; 去掉 Export 用目录。
4. 改贴图/模型后重进港验证。
**包内结构**: `ship/gun/radar/...` + `compile.info`(勿手改) + `.geometry/.visual/.model/.mfm/.dds`。
**莱服补充**: 论坛有独立「皮肤」「模型」「UV」帖; 工具链常提 gmConverter3D、MK Model Export、NVIDIA Texture Tools。解包莱服 26.x 二进制资产需 pfsunpack/MKUT(见本机 TOOLKIT), 勿用旧 wowsunpack 硬解 dds/geometry。
**对本工具**: 日常不手搓 Export——`mod init --mode sdk` + `import-sdk`(需本机已有 extractSources 产物与 ModsSDK 船目录, 记在 sdk_sources.json)。细节 `ref.project`。官方 Maya 全流程是背景知识, 非常规路径。
**完成标准**: 能说清 extract → register 与 `import-sdk` 的对应关系; 不知船 ID 时用 `find --ship`。
**两种导出写法等价**: 临时改目标 mod 的 Main.py 调 extractSources, 或用 `ModsSDKExport/Main.py`(WG 2019); 导出后删 Export、**勿改 mod 文件夹名**。
**莱服 API**: 船外观只用 `API_v1.0`+`registerShipMod`; **勿**塞 `API_v2.0`/SIGNATURE(那是受限脚本原型)。ModsSDK 从莱服当前版本盘取, 勿盲用 WG 包。
