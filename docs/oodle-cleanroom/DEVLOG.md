# wowsmod-oodle 开发日志

## 2026-10-13 — S0 完成；S1 因接触污染暂停（Grok Bot）
**接触披露（重要）**：在之前的调研中，网页搜索结果里内嵌出现了以下内容的片段：
(1) Crauzer/kraken `kraken.cpp`（ooz 衍生，GPL）中描述 Mermaid/Selkie flag/length/off16/off32 流语义的注释块；
(2) docs.rs 上 oozextract `kraken.rs` 的命令字节循环代码；
(3) encode.su 上 powzix 发布 ooz 的帖子（Leviathan 命令字节、字面量模式）以及 Shelwien 给出的块头字节表。
(3) 属于允许范围（论坛散文）。(1)(2) 属于**禁止**来源。
**后果**：本代理对 Mermaid 和 Kraken 已不算洁净，**不编写**这两个 codec 的解码逻辑。S1 改由另一个未接触过的实现者（新代理会话 / 人工）在只提供“允许”材料的前提下完成。

完成内容：
- `examples/oodle_corpus.rs`：按 codec 抽样，导出 `.blk` 与 oracle 生成的 `.gold`，同时校验条目 crc32。
- `crates/wowsmod-oodle`：骨架 + 基于语料的测试框架（未设置 `OODLE_CORPUS` 时跳过）；`decompress_block` 目前返回 `Unsupported`。
- 黑盒观察（来自语料统计）：每个块的前两个字节分布见 CORPUS.md；Hydra 条目会逐块在 0x8c0a / 0x8c0c / 0x8c06 中选择。