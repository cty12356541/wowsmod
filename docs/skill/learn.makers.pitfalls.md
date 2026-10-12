# learn.makers.pitfalls: 常见坑
上级: learn.makers
下级: 无
---
1. **版本目录**: mod 装在 `bin/<最大数字>/res_mods/`; 更新清空 → 备份并 `migrate`。浩舰 0.9.6+ 文已强调。
2. **mip 缺失**: 4096 无 mip → 远景闪烁/吃显存; 我们导入默认生成完整 mip。
3. **迷彩盖图案**: 忘清 `_mg` B → 穿永久迷彩后自定义区消失; 安装说明常要求卸永久迷彩, 或改材质标识规避(流萤 `shipmatX` 为推断, 港口先实证)。
4. **emissive 与迷彩互斥**: 开发光后 B 不再当迷彩遮罩(莱服官方说明)。
5. **整船重导丢 LOD/断裂**: 社区 Maya 包常删 crack/patch、单 LOD; 我们 import 默认写全 LOD。
6. **莱服解包**: 新压缩 → wowsunpack 报 corrupt; 用 pfsunpack。`export-ship` 可能不可用, 改 `export-model --no-vfs` + 修复脚本。
7. **服差异**: 夙岚等简介常写「WG/国服可用, 莱服不能用」——涂装包按客户端做; 结构类似但 geometry/atlas 路径可能不同, **勿盲装直营包进莱服**。
8. **主题误用人物 mesh**: 高质量主题≠立人像; 立人像仅复刻定制例外。见 `workflow.theme`。
9. **垃圾源文件**: 原包常夹 PNG/副本/新建文件夹——打包前清; `pack` 扫许可。
10. **mkmod(莱服 25.10+)**: 零压缩 zip 放 `bin/<build>/mods/`; `res_mods` 优先; **含 PnF/Python 的船外观不能进 mkmod**, 仍放 res_mods。
11. **手改 paths.xml**: 中文旧帖常见, **0.9.6 后过时**; 只装 `bin/<最大>/res_mods`。
排错命令: `ref.troubleshoot` / `doctor` / 港口验证 `workflow.layers.verify.port`。
