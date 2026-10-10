# M1 报告

> 断点续作记录保留在文末"进度"节。下面是截至本地最后一次验收的结果。

## 交付物

| 项 | 位置 |
|----|------|
| 源码 / 本地 git | `D:\WoWSModding\wowsmod` |
| 单文件 exe | `target\release\wowsmod.exe` (MSVC + crt-static) |
| 设计说明 | `docs/DESIGN.md` |
| 中文 README | `README.md` |
| 验收脚本 | `scripts\accept_*.ps1`, `scripts\fetch_samples.ps1` |

## 验收结果摘要

### doctor
- 注册表定位到 `D:\Korabli`, 版本 `26.10.0.0.8867689`, idx 在 `bin\8867689\idx`, 约 225 个 idx / 342,823 条目。
- 检测 Blender(便携目录)、res_mods 状态。`--game` 可覆盖。

### find
- glob / 关键字检索。
- `--ship`: GameParams(1165 条舰船, 与 Python pickle 逐条一致) + 俄文/英文/代号; 整船文件集 = 模型目录 + 同级 textures。

### extract vs pfsunpack
- `scripts\accept_extract.ps1`(种子 20261010, 随机 250 + 最大 Oodle 10 + 整船 PFSB510/PFSB108): **429/429 逐字节一致**。
- 其中 23 个是 BC7Prep 贴图(默认还原为标准 BC7, 与 pfsunpack 一致); `--raw` 可保留 pkg 原始字节。
- 用时约 1.5s(wowsmod) / 86s(pfsunpack)。

### BC7Prep(贴图逆变换)
- 纯 Rust 实现 `crates/wowsmod-formats/src/bc7prep.rs`(黑盒逆向, 无公开规范)。
- 覆盖: 模式 0–7 + 纯色模式 9; flags 低 8 位(拆平面)与第 16 位(颜色去相关 G / R−G / B−G)。
- 集成测试: 52 个真实样本(含去相关+模式 2)与 pfsunpack 逐字节一致。
- 全量扫描(`examples/bc7prep_scan`): 95,120 个贴图中 11,461 个 BC7Prep; 在 Oodle 解压成功的前提下, **全部** BC7Prep 可还原(含去相关模式 2 的 54 个)。
- 详见 `docs/DESIGN.md` §7。

### Oodle 已知限制(oozextract)
- 全量扫描时 **19** 个文件 Oodle 解压失败: 均为 Hydra(compressor=12), 最后一块压缩后仅 25–30 字节、解压目标 < 256 字节的情形; oozextract 0.5.5 报 `OozError`, pfsunpack(内嵌 OodleLZ)可解。
- 示例: `LMY404_tree_alpha_a.dd0`, `particles/animated/lightning4x1.dds`。
- 占贴图总量约 0.02%; 随机+整船验收样本未命中。M2/M4 可换官方 Oodle 或修 oozextract; 替换点已预留(`compression::OodleBackend`)。

### export(geometry → glb)
- 见 `out\accept_export\report.json`(脚本跑完后填具体数字)。
- 配对: assets.bin VisualPrototype/RenderSet 的 murmur3 mapping id; 健康检查 0 越界 / 0 尖刺为目标。

### geometry 往返
- 18 个样本集成测试: 重排与重新编码均逐字节一致(需先 `fetch_samples.ps1`)。
- `accept_roundtrip.ps1 -All` 可选全量。

### 设计笔记(M4 TODO)
1. SDK `extractSources` 占位文件(35–44 字节 XML)会导致 `registerShipMod` 崩溃 → 打包须用 pkg 真文件填充。
2. 改贴图须同时提供 `.dds` + `.dd0–.dd2`; 回写时标准 BC7(偏移 40=0) vs 正向 BC7Prep 需实测。
3. 导回时还原 `extras.winding_flipped`。

## 进度(最新在上)

- [!] **2026-10-10 ~13:05 CST 用户电脑再次掉线**; 源码与修复脚本在 box `/workspace/m1/wowsmod` 与 `wowsmod_src_9.tgz`。

- [x] BC7Prep 去相关形式模式 2(全量扫描 54 个, 33 个与 pfsunpack 对比通过)
- [x] BC7Prep 去相关形式(flags bit16)模式 0/1/3–7 + 纯色
- [x] BC7Prep 普通形式; extract 默认还原 / `--raw`; accept_extract **429/429**
- [x] 工程 / doctor / find / GameParams / 18 样本往返单测 / 设计文档草稿
- [x] accept_textures: **2000/2000** 与 pfsunpack 一致(其中 BC7Prep 238/238)
- [x] accept_export: 主验收通过(勃艮第/黎塞留/FGM049 LOD0+全 LOD **0 越界 / 0 尖刺 / 包围盒一致**; naive 对照正确抓出错误)
- [~] accept_visual_xcheck: 78 个可比较 visual 的 **mapping id 实际全部一致**, 脚本误把 XML 模板材质名 vs assets.bin 的 .mfm 路径算作失败。
      修复脚本已写好(scripts/accept_visual_xcheck.ps1, 只比 vertices/indices mapping), 待机器重连后重跑。
- [~] accept_roundtrip: PowerShell 把 cargo stderr 的 Compiling 当成错误退出。修复脚本已写好(用 cmd 重定向), 待重跑。
- [ ] 机器重连后: 拷 scripts + M1_REPORT/DESIGN → 重跑 visual/roundtrip → PLAN.md 勾选 → 最终提交

