# wowsmod 对 oozextract 0.5.5 的补丁

来源: crates.io `oozextract` 0.5.5 (Cargo.toml 声明 MIT, https://github.com/lvlvllvlvllvlvl/oozextract)。

修改: `src/decoder/pointer.rs` 的 `Core::get_le_bytes` / `Core::get_be_bytes`。
位读取器(tANS 等)在流末尾一次读取 4 字节做"预读", 原版 C 实现(ooz)依赖缓冲区尾部留白,
oozextract 用带边界检查的切片读取, 越界即报 `OozError`。莱服部分条目的最后一块压缩后只有 25–30 字节
(解压 128–256 字节), 预读必然越过块尾, 于是整条目解压失败(M1 全量扫描 19 个)。
补丁: 越界部分按 0 填充。合法流不会消费这些位; 数据正确性另由 idx 中的 crc32 校验保证。

其余文件未改(删除了 benches/perf/wasm-test 等与构建无关的目录)。
