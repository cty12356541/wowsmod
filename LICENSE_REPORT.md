# 许可证扫描报告（发布前）

日期: 2026-10-10（Asia/Shanghai）  
范围: `wowsmod` Rust 依赖图 + 嵌入前端 `preview_web` 的构建源 `preview_app/web` + vendored `oozextract`  
工具: `cargo metadata` / `cargo tree` / `cargo deny check licenses` + npm `node_modules` 许可证汇总  
**未删除** `vendor/oozextract`（按要求仅报告与建议）。

---

## 1. 结论（可发布性）

| 项 | 结论 |
|---|---|
| 常规 Rust 依赖（除 oozextract 外） | **可按 MIT 发布**：主要为 MIT / Apache-2.0 / BSD / Zlib / 0BSD / Unlicense；另有 `option-ext` **MPL-2.0**（弱 copyleft，保留声明即可）、`unicode-ident` 附带 **Unicode-3.0** 数据许可 |
| `preview_app/web` npm 树 | **可发布**：MIT/ISC 为主；`caniuse-lite` 为 **CC-BY-4.0**（仅构建期 browserslist 数据，不进最终嵌入产物逻辑）。嵌入产物为自有 `preview_web/` 构建结果（Three.js 等运行时需保留其 MIT 声明，见 THIRD_PARTY_LICENSES） |
| **`oozextract`（关键阻塞）** | **高风险**：crate/`Cargo.toml` 自称 **MIT**，但 GitHub 仓库描述为 *「rust port of ooz oodle decompressor」*，且与 **GPL-3.0** 的 [powzix/ooz](https://github.com/powzix/ooz) 在结构、算法模块划分、**≥125 条英文注释逐字相同**、函数命名高度对应。这是典型的 **源码移植，而非洁净室重写**。在未获律师确认或作者双许可之前，**不宜把含默认 `oozextract` feature 的二进制当作「纯 MIT 开源」对外发布** |
| `cargo deny` | 对声明 SPDX 跑通为 `licenses ok`（它只看 crate 元数据，**不会发现 GPL 移植风险**） |

**总体建议**: 仓库其余部分可维持 MIT；**发布/对外分发前必须处理 Oodle 后端**（见 §3）。在问题解决前，对外材料应披露风险，或默认关闭 `oozextract` feature 并改用替代后端。

---

## 2. oozextract ↔ powzix/ooz 证据

| 证据 | 内容 |
|---|---|
| 上游自称 | https://github.com/lvlvllvlvllvlvl/oozextract — description: **「rust port of ooz oodle decompressor」**；GitHub license API **404**（未挂 LICENSE 文件），crates.io 填 MIT |
| GPL 源 | https://github.com/powzix/ooz — `kraken.cpp` 文件头 **Copyright (C) 2016, Powzix / GNU GPL v3+**；README 首句与 oozextract README 实质相同（Kraken/Mermaid/Selkie/Leviathan/LZNA/Bitknit） |
| 结构对应 | ooz: `lzna.cpp` / `bitknit.cpp` / `kraken.cpp`（含 Mermaid/Selkie/Leviathan）；oozextract: `src/algorithm/{lzna,bitknit,kraken,mermaid,leviathan}.rs` |
| 注释重合 | 对 ooz `*.cpp` 与 oozextract `src/**/*.rs` 英文 `//` 注释做集合交集：**约 125 条完全相同**（例：*「and the transformation offs * scaling + low_bits」*、*「Decode packed litlen stream. It's bounded by 1/5 of dst_size.」*、*「copy final literals」* 等） |
| 函数名 | 多处 C 名与 Rust `snake_case` 一一对应（如 `CombineScaledOffsetArrays`、`CopyFinalLiterals`、`Huff_ReadCodeLengthsNew/Old`、`Tans_*` 等） |
| LZNA 状态机 | C `struct LznaNibbleModel` / `LznaFarDistModel` / … 与 Rust 同名字段布局注释几乎逐段平移 |
| 本仓库补丁 | `vendor/oozextract/WOWSMOD_PATCH.md`：基于 crates.io 0.5.5，仅改短块尾部预读越界；**不改变来源问题** |

**风险解读（非法律意见）**: 若认定 oozextract 为 GPL 作品的衍生，则 **链接进 `wowsmod` 的默认构建可能传染 GPL-3 义务**（源码提供、同许可等）。自称 MIT **不能**抵消上游 GPL 文本与移植证据。需法务确认或更换实现。

**未做**: 未擅自删除 `vendor/oozextract`。

---

## 3. 替代方案（建议优先级）

已有抽象：`wowsmod-formats` 的 `OodleBackend` + feature `oozextract`（可关）+ `set_oodle_backend`（见 `compression/mod.rs`）。

1. **运行时加载本机游戏内嵌的 Oodle（推荐作默认发布路径）**  
   - 莱服 `Korabli64.exe` 字符串可见 `OodleLZ_Decompress`（Oodle **静态链进主程序**，目录下 **无** 独立 `oo2core_*.dll`）。  
   - 可行技术路线：  
     a) 从用户已安装的 `Korabli64.exe` / 相关模块解析导出或签名扫描调用（复杂、易碎）；  
     b) 文档要求用户自备合法 `oo2core_*`（如部分游戏/引擎分发的共享库），用 `libloading` 调 `OodleLZ_Decompress`（类似 crates.io `oodle` / `oodle-rs` 包装）；  
     c) 仅在「用户本机已安装莱服」时启用解压。  
   - 许可: **不再分发** Oodle/ooz 源码；依赖用户游戏授权。注意 EULA：勿再分发从游戏抽出的 DLL。

2. **可选 Cargo feature（现状已接近）**  
   - `default = []` 或 `default = ["oodle-runtime"]`，`oozextract` 仅 `--features oozextract` 给开发者自担风险。  
   - 发布 CI 产物默认 **不含** oozextract。

3. **洁净室自研解码器（长期）**  
   - 仅依据公开格式描述/黑盒测试向量实现 Kraken（莱服条目 compressor 实测以 Kraken 等为主），**禁止**对照 ooz/oozextract 源码。成本高，但许可最干净。

4. **不推荐**  
   - 继续默认静态链 oozextract 并以「MIT」宣传；  
   - 打包分发从游戏剥出的 `oo2core` DLL。

---

## 4. 其它依赖一览

详见同目录 [`THIRD_PARTY_LICENSES.md`](./THIRD_PARTY_LICENSES.md)。

要点：
- `cargo deny --config … check licenses`：**通过**（仅验证声明 SPDX）。
- Windows 目标解析约 **129** 个唯一 `(name, version)`。
- npm 构建树约 **126** 包；嵌入运行时主要是 React / Three / Radix / Zustand（均为 MIT 系）。

---

## 5. 建议的发布清单

- [ ] 法务确认 oozextract 策略，或默认关闭该 feature  
- [ ] 实现并测试 `OodleBackend` 的「本机 oo2core / 游戏侧」路径  
- [ ] 根目录保留 `LICENSE`（MIT）+ `THIRD_PARTY_LICENSES.md` + 本报告链接  
- [ ] README「许可证」节明确：**默认发布构建不含 GPL 移植解码器**（若已切换）  
- [ ] 不要删除现有 `vendor/oozextract` 直至替代就绪（开发/对照仍可用 feature 打开）

---

## 6. 参考链接

- powzix/ooz: https://github.com/powzix/ooz  
- lvlvllvlvllvlvl/oozextract: https://github.com/lvlvllvlvllvlvl/oozextract  
- 本仓补丁说明: `vendor/oozextract/WOWSMOD_PATCH.md`  
- 后端替换点: `crates/wowsmod-formats/src/compression/mod.rs`
