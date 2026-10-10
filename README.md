# wowsmod — 莱服《战舰世界》(Мир кораблей) AI 模组工具箱

单文件 Windows 命令行工具 `wowsmod.exe` + Rust 格式库 `wowsmod-formats`。
目标客户端: 莱服 Lesta 26.10.0.0.8867689。M1 阶段只**读取**游戏文件, 不写游戏目录, 不启动游戏。

> 状态: M1(定位 / 检索 / 解包 / 导出 GLB / geometry 往返)。改模与打包在后续里程碑(见 `D:\WoWSModding\PLAN.md`)。

## 快速开始

```powershell
wowsmod doctor                                  # 自动找到客户端, 报告版本/idx/pkg/res_mods/Blender
wowsmod find --ship Bourgogne                   # 按船名/参数代号(PFSB510)/模型代号/俄文名检索
wowsmod find "*FSB025*.geometry"                # glob(不区分大小写; 不含 / 时只匹配文件名)
wowsmod find Richelieu --limit 20               # 关键字(完整路径子串)
wowsmod extract --ship PFSB510 -o out\bourgogne # 整船(船体 + 炮塔/防空/雷达等 + 对应贴图)
wowsmod extract "*FGM049*" -o out\fgm049        # 按 glob/关键字解包, crc32 全部校验
wowsmod extract --list paths.txt -o out\x       # 精确路径列表
wowsmod export "content/gameplay/france/ship/battleship/FSB025_Bourgogne_1945/*.geometry" -o out\glb
wowsmod export out\bourgogne\...\FSB025_Bourgogne_1945_Bow.geometry -o bow.glb --lod all
wowsmod check bow.glb                           # GLB 健康检查(越界/尖刺)
wowsmod geom "*.geometry" --roundtrip           # geometry 逐字节往返校验
```

所有命令都支持 `--json`(机器可读; 出错时输出 `{"ok": false, "error": [...]}`)与 `--game <客户端目录>`。

退出码: 0 成功; 1 错误; 2 `export --strict` 健康检查未通过; 3 没有匹配结果。

## 各命令说明

### doctor
查找顺序: `--game` → 环境变量 `WOWSMOD_GAME` → 注册表卸载项(`LGC-*` 的 InstallLocation, Lesta Game Center 写入)
→ Lesta Game Center `preferences.xml` 的 `working_dir` → 各盘符常见目录。报告客户端版本(game_info.xml)、
最新 bin 目录、idx/pkg 数量与大小、Oodle 条目数、res_mods/mods 是否干净、Blender 位置
(注册表 / .blend 关联 / Program Files / Steam / PATH / `WOWSMOD_BLENDER` / wowsmod.exe 附近的便携版)。

### find
- glob(含 `* ? [`): 大小写不敏感; 不含 `/` 的 glob 匹配文件名, 含 `/` 的匹配完整路径。
- 否则按关键字(完整路径子串)匹配。
- `--ship`: 解析 `content/GameParams_py3.data`(自写 pickle 读取器)得到全部 1165 条舰船, 按参数代号、参数名、
  本地化名(global.mo)、船体模型目录名匹配; 没有参数的模型目录(如 FSB026_Bourgogne)作为兜底列出。

### extract
纯 Rust 读取 idx/pkg, 支持莱服 Oodle 多块容器(oozextract 后端), 多线程, 默认校验 idx 中的 crc32。
输出保持游戏内相对路径; 拒绝 `..`/盘符等不安全路径。

### export
geometry → GLB。配对来源优先级: `--visual`/同目录 `.visual` → `assets.bin`(从游戏 pkg 读取 `content/assets.bin`, 或 `--assets-bin`)
→ 启发式(标记为"不确定")。导出后:
- 健康检查: 越界三角形、尖刺三角形(长边且法线不支持), 详见 docs/DESIGN.md §5;
- assets.bin 渲染集包围盒与实际引用顶点包围盒逐个比对(独立校验配对正确性);
- GLB 写盘后重新读回再检查一次。
`--lod 0|1|2|3|all`、`--pairing naive`(错误配对对照)、`--keep-winding`、`--strict`。

### geom
geometry 结构信息; `--roundtrip` 做两种往返: 重排版(解析→序列化)与 meshopt 解码→重新编码→序列化, 要求逐字节一致。

## 构建

```powershell
rustup default stable-x86_64-pc-windows-msvc   # 需要 VS 2022 Build Tools(MSVC 链接器)
cargo build --release                          # → target\release\wowsmod.exe(静态 CRT, 单文件)
cargo test --release                           # 单元测试; 集成测试需要样本, 见下
```

不带 Oodle 后端构建: `cargo build --release --no-default-features`(Oodle 条目会报"未编译 Oodle 后端")。

## 测试与验收(可重跑)

```powershell
powershell -ExecutionPolicy Bypass -File scripts\fetch_samples.ps1      # 准备 testdata\(不入库)
$env:WOWSMOD_REQUIRE_SAMPLES=1; cargo test --release                   # 含 18 样本往返、导出健康、pkg crc、GameParams
powershell -ExecutionPolicy Bypass -File scripts\accept_extract.ps1     # 与 pfsunpack 逐字节对比(随机 ≥200 + 整船)
powershell -ExecutionPolicy Bypass -File scripts\accept_export.ps1      # 勃艮第/黎塞留/FGM049 导出健康 + naive 对照
powershell -ExecutionPolicy Bypass -File scripts\accept_visual_xcheck.ps1  # SDK .visual 与 assets.bin 配对交叉验证
powershell -ExecutionPolicy Bypass -File scripts\accept_roundtrip.ps1 -All # 18 样本 + 全部游戏 geometry 往返
```
结果写到 `out\<脚本名>\report.json`。验收结论见 `M1_REPORT.md`。

## 署名与许可证

本项目代码: MIT。依赖与移植来源:

| 组件 | 用途 | 许可证 | 说明 |
|---|---|---|---|
| wows-toolkit (landaire) | idx 格式解析思路/结构 | MIT | 移植部分见 `idx.rs` 文件头; 其法线(有符号)与绕序结论经实测不适用于 26.10, 已更正 |
| meshoptimizer 1.3 (zeux) | v0 顶点编码器 | MIT | 移植到 `vertex_codec.rs`, 保留版权声明 |
| meshopt-rs 0.1.2 | 顶点/索引解码, 索引编码 | MIT | crates.io |
| oozextract 0.5.5 | Oodle Kraken/Mermaid/Leviathan 解压 | Cargo.toml 写 MIT, **许可证状态待定** | 可关闭的 feature; 替换点 `compression::OodleBackend` |
| clap / serde / rayon / sha2 / globset / miniz_oxide / half / crc32fast / thiserror / anyhow / winreg | 通用 | MIT 或 Apache-2.0 | crates.io |

游戏资源版权归 Lesta Games; 本仓库不包含任何游戏文件(testdata\ 与 out\ 不入库)。
