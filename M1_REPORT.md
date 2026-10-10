# M1 报告

完成时间: 2026-10-10 (Asia/Shanghai)。证据目录: `D:\WoWSModding\wowsmod\out\`。

## 交付物

| 项 | 位置 |
|----|------|
| 源码 / 本地 git | `D:\WoWSModding\wowsmod` (未 push) |
| 单文件 exe | `target\release\wowsmod.exe` ≈ 3.0 MiB, MSVC + crt-static |
| 设计说明 | `docs/DESIGN.md` |
| 中文 README | `README.md` |
| 验收脚本 | `scripts\accept_*.ps1`, `scripts\fetch_samples.ps1` |

## 验收结果

### doctor — 通过
- 自动定位 `D:\Korabli`, idx `bin\8867689\idx`(客户端 26.10.0.0.8867689)。
- Blender: 便携版 `D:\WoWSModding\tools\blender-5.2.2-windows-x64\blender.exe` (5.2)。
- 证据: `out\logs\doctor.json`。

### find — 通过
- glob / 关键字; `--ship` 基于 GameParams(1165 舰, 与 Python pickle 一致) + 本地化名。

### extract vs pfsunpack — 通过
- `accept_extract.ps1`(种子 20261010): **429/429** 逐字节一致(随机 250 + 大 Oodle 10 + 整船 PFSB510/PFSB108)。
- 其中 23 个 BC7Prep 贴图: extract 默认还原为标准 BC7(与 pfsunpack 一致); `--raw` 保留 pkg 原始字节。
- `accept_textures.ps1`: **2000/2000** 一致, 其中 BC7Prep **238/238**。
- 用时约 1.5s(wowsmod) / 86s(pfsunpack)(extract 样本集)。

### BC7Prep — 通过
- 纯 Rust `bc7prep.rs`(黑盒逆向)。覆盖模式 0–7 + 纯色 9; flags 拆平面 + 第 16 位颜色去相关(含模式 2)。
- 集成测试 52 真实样本逐字节一致; 全量扫描 11,461 个 BC7Prep 在 Oodle 可解前提下全部可还原。
- 详见 `docs/DESIGN.md` §7。

### Oodle 已知限制
- **19** 个文件: Hydra 压缩、末块仅 25–30 字节 → oozextract 0.5.5 `OozError`; pfsunpack 可解。约占贴图 0.02%。替换点 `compression::OodleBackend` 已预留。

### export — 通过
- 主验收(勃艮第/黎塞留/FGM049 LOD0) 11 文件: **0 越界 / 0 尖刺 / 包围盒 100 处全一致**。
- 全 LOD: 同 0/0; 两船全部模型目录 45 文件 0/0。
- naive 对照: 239,274 越界 / 1,479 尖刺(检查器有效)。
- visual 交叉验证: **78/78** mapping id 一致(材质名为 XML 模板 vs .mfm, 属预期差异)。

### geometry 往返 — 通过
- 18 样本: 重排 18/18, 解码再编码 18/18; cargo test 全绿。

### 设计笔记(M4 TODO)
1. SDK 占位文件崩溃 → 打包须填 pkg 真文件。
2. 贴图须 `.dds`+`.dd0–.dd2`; 回写标准 BC7 vs 正向 BC7Prep 待实测。
3. 导回还原 `extras.winding_flipped`。

## 本地 git 提交(节选, 未 push)

```
a60f55e fix: visual 交叉验证只比 mapping id; roundtrip ...
dcd5214 BC7Prep: 去相关形式的模式 2 ...
178e463 BC7Prep: flags 第 16 位颜色去相关形式 ...
1b49ca7 extract: BC7Prep 贴图逆变换 ...
57b829c docs: DESIGN/README 草稿, M1_REPORT 进度记录
40ed251 GameParams 舰船检索、extract --ship/--list ...
b706edd M1: wowsmod workspace 初始导入
```

