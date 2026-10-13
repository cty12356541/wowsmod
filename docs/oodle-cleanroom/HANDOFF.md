# 交接包：wowsmod-oodle 8c0a（Mermaid 族）洁净室实现者

角色分工：**规格作者**（Grok Bot，可能存在训练数据接触，不写代码）→ **第三方审核人**（审 SPEC 与实验）→ **实现者（你）**（写全部解码代码）。

## 1. 开工前
1. 读完 `ALLOWED_SOURCES.md`，在声明表里**以本人名义**加一行：日期、“clean implementer, no prior exposure”、负责范围、接触记录。只有你能如实声明从未接触过下面“禁止阅读”的内容时才能签。
2. 确认 SPEC_mermaid.md 顶部写有审核人签字/日期。**没有经过审核的 SPEC 不要使用。**

## 2. 可以读
- `docs/oodle-cleanroom/SPEC_mermaid.md`（散文规格）、`EXPERIMENTS.md`（表格和观察到的原始字节）、`CORPUS.md`、`ALLOWED_SOURCES.md` 的“允许”部分。
- crate 骨架 `crates/wowsmod-oodle`、测试 `tests/*.rs`。
- 通用教材：LZ77、Huffman / canonical code、ANS/tANS 论文。
- 自己语料上的任何实验（统计、改输入看输出）。

## 3. 禁止阅读
- `docs/oodle-cleanroom/experiments/*.py`（规格作者写的分析脚本，可能受训练数据影响）、`DEVLOG.md`（含污染披露）。
- `vendor/oozextract`、cargo registry 中的 oozextract、powzix/ooz 及其所有 fork、oozle、oodle-rs、UE Oodle 源码、任何 oo2core 反汇编。
- box 上的 `/workspace/tmp_ooz`（来源未知，按禁止处理）。
- 不要对 Korabli64.exe / oo2core 做调试或反汇编。

## 4. 获取语料（用你自己的游戏安装）
需要开发构建的 oracle（`--features oozextract`）。它**只能作为可执行程序运行**，不许打开它的源码：

    cargo run --release -p wowsmod-formats --features oozextract --example oodle_corpus -- <idx 目录> <res_packages 目录> <输出目录> 300

可选环境变量 `OODLE_CORPUS_K`（默认 60）用于抽样。输出包括 `meta.jsonl` 和各 codec 目录下的 `NNNNNN.blk` / `.gold`。**不要把语料提交进 git。**
（注意：如果编译报错时显示了 oozextract 的源码行，不要看，直接关掉，并在声明表的“接触记录”里写一笔。）

## 5. 运行测试

    set OODLE_CORPUS=D:\path\to\oodle_corpus
    cargo test -p wowsmod-oodle -- --nocapture

- `tests/corpus.rs`：分别输出各容器和各块类别（cc06 / 8c0a / 8c06 / 8c0c）的 golden 匹配率。
- `tests/robustness.rs`：随机输入、截断输入、按位翻转的 golden 块，要求解码器**不 panic**（结果可以是 Err）。不需要语料也能跑；有语料时会额外测截断/翻转的真实块。

## 6. 验收门槛（按顺序）
1. cc06 与 8c0a 在 `corpus.rs` 中达到 **100%** 逐字节匹配（三个容器中所有 8c0a 块）。
2. `robustness.rs` 全部通过，且以 `--release` 再跑一遍没有 overflow 或 panic；crate 保持 `forbid(unsafe_code)`。
3. 第三方审核人复核提交历史（小步提交，每次提交说明对应 SPEC 的哪一节或你自己的哪次实验）。
4. 以上全部通过后，才可以在 `wowsmod-formats` 里接入 **feature `oodle-native`**（默认关闭），并排在 DLL loader 之前作为首选路径；8c06/8c0c 仍然回退到原有路径。

## 7. SPEC 的使用方式
SPEC 只描述观察到的结果，并且标了置信度。§7 的待解决问题（尤其是 Q4 熵编码子流）需要你自己用语料做实验来解决。请把你的实验写进**你自己的** `DEVLOG_impl.md`，不要直接引用规格作者的脚本。
