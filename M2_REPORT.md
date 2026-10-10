# M2 报告(进行中 — 断点续作记录)

## 进度(最新在上)
- [~] 0. Hydra 极小末块解压失败修复
  - 根因(已确认): oozextract 0.5.5 的 tANS 位读取器在流末尾一次预读 4 字节(`tans.rs:61` → `pointer.rs` `get_le_bytes`),
    原版 C ooz 依赖缓冲区尾部留白, Rust 版带边界检查越界即 `OozError`(verbose_errors 报 `oob Input[17]..21`)。
    受影响的是压缩后只有 25–30 字节的末块。
  - 修复: vendor/oozextract(MIT, 0.5.5)+ `[patch.crates-io]`; `get_le_bytes/get_be_bytes` 越界部分补 0。见 vendor/oozextract/WOWSMOD_PATCH.md。
  - box 验证: LMY404 末块(25B→128B)、lightning4x1 末块(26B→148B)输出与 pfsunpack 结果逐字节一致; cargo test 全过。
  - 待做: Windows 全量 `bc7prep_scan ... "*"` 0 失败 + pfsunpack 对比。
