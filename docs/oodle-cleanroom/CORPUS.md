# 语料（不入 git）

生成：`cargo run --release -p wowsmod-formats --features oozextract --example oodle_corpus -- <idx> <res_packages> <out> [每个 codec 的条目数=300]`
I/O 控制：环境变量 `OODLE_CORPUS_K`（默认 60，只读取 path-crc 能被 K 整除的条目）。
位置：Windows 上为 `D:\WoWSModding\oodle_corpus`（约 972 MB），box 上为 `/workspace/oodle_corpus`（需要时手动拷贝子集）。
测试：`OODLE_CORPUS=<dir> cargo test -p wowsmod-oodle -- --nocapture`

## 26.10.0 (8867689) 统计，K=60，每个 codec 上限 300 个条目
| 容器 compressor | 读取的条目 | 抽样条目 | 块数 | 每块前 2 字节 | oracle 失败 | 条目 crc 不一致 |
|---|---|---|---|---|---|---|
| Hydra(12) | 849 | 300 | 2029 | 8c0a 777 / 8c0c 684 / 8c06 453 / cc06 115 | 0 | 0 |
| Mermaid(9) | 553 | 300 | 333 | 8c0a 333 | 0 | 0 |
| Kraken(8) | 468 | 300 | 419 | 8c06 406 / cc06 13 | 0 | 0 |
| Leviathan(13) | 242 | 242 | 321 | 8c0c 321 | 0 | 0 |

结论：以块为单位看，0x8c0a（Mermaid 族）共 1110 / 3102 = 35.8%；Hydra 只是逐块选择的封装。
因此 Mermaid 的实际覆盖率 ≈ Mermaid 条目 + 约 38% 的 Hydra 块。