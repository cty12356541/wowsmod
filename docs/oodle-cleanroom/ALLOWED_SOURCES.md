# Oodle 洁净室实现：允许 / 禁止来源

适用于 `crates/wowsmod-oodle` 及所有 Oodle 解码相关提交。目标：MIT 许可的纯 Rust 解码器，不含任何 GPL 或专有代码的衍生成分。

## 允许
- 本仓库自有的莱服容器格式文档（`docs/DESIGN.md` §2.1，`compression/mod.rs` 头注释）。
- 厂商公开的产品层资料：RAD/Epic 产品页、UE Oodle Data 用户文档、ryg / cbloom 博客上的**概念性**描述。
- encode.su 论坛帖子中的**散文式高层描述**（不含代码片段）。
- 通用压缩教材/论文：LZ77、Huffman、ANS/tANS、canonical code 等。
- **黑盒 oracle**：开发构建（`--features oozextract`）**只作为可执行程序 / API 使用**，用来产生 golden 解压结果。允许运行，**不允许打开、阅读、diff、搜索它的源码**（包括 `vendor/oozextract`、docs.rs 源码页、编译报错时显示的源码行）。
- 自己对 `.blk`/`.gold` 语料做的实验（统计、假设检验、修改输入看输出差异）。

## 禁止
- 阅读或参考 powzix/ooz 及其所有 fork（rarten、ex-nihil、Crauzer/kraken 等）、oozextract、oozle、sehnryr/oodle-rs，或任何从它们移植过来的代码 / 注释 / 文档。
- 阅读或参考 UE Oodle SDK 源码（`oodle2_src_for_unreal_*`、WorkingRobot/OodleUE）。
- 对 `Korabli64.exe`、pfsunpack 或任何 oo2core 二进制做反汇编 / 调试 / 算法逆向（只读查看 PE 导出表除外）。
- 把游戏数据（语料、golden）提交进 git。

## 贡献者声明（每位往 `wowsmod-oodle` 提交解码逻辑的人都要签署）
> 本人声明：我编写的 wowsmod-oodle 代码仅来源于上方“允许”列表；我没有看过“禁止”列表中任何来源的代码、注释或衍生文档；如曾意外接触，已在 DEVLOG.md 记录，并且不负责编写与所接触内容相关的模块。

| 贡献者 | 日期 | 范围 | 接触记录 |
|---|---|---|---|
| Grok Bot（代理） | 2026-10-13 | S0 语料工具、crate 骨架、测试框架（**不含解码逻辑**） | **已污染：Mermaid / Kraken**，见 DEVLOG 2026-10-13 |
| Grok Bot（代理，规格作者） | 2026-10-14 | 8c0a 黑盒实验与 SPEC_mermaid.md / EXPERIMENTS.md / HANDOFF.md、测试框架（**不写解码代码**） | **spec author; possible training-data exposure; writes no decoder code; spec requires third-party review**。本次未阅读任何禁止来源、未联网；仅分析 .blk/.gold |
