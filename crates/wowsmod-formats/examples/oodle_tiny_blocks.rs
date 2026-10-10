//! 列出莱服 Oodle 容器中"某块压缩后很小"(默认 < 64 字节)的条目 —— 即 oozextract 原版会因尾部预读越界而失败的候选。
//! 用法: oodle_tiny_blocks <idx 目录> <res_packages 目录> [阈值=64]  → stdout: 每行 "路径\t块号/块数\t压缩大小\t解压大小"
use std::path::Path;
use wowsmod_formats::{compression, index::GameIndex, pkg::PkgReader};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ix = GameIndex::load_dir(Path::new(&a[1])).expect("idx");
    let pr = PkgReader::new(Path::new(&a[2]));
    let th: u32 = a.get(3).map(|s| s.parse().unwrap()).unwrap_or(64);
    for e in ix.search("*").unwrap() {
        if compression::method_for(e.info.compression_info, e.info.size, e.info.unpacked_size) != compression::Method::LestaOodle {
            continue;
        }
        let raw = match pr.read_raw(e) {
            Ok(r) => r,
            Err(err) => {
                eprintln!("{}: {err}", e.path);
                continue;
            }
        };
        let h = match compression::parse_lesta_header(&raw) {
            Ok(h) => h,
            Err(err) => {
                eprintln!("{}: {err}", e.path);
                continue;
            }
        };
        let bs = h.block_size as u64;
        for (i, &c) in h.block_sizes.iter().enumerate() {
            let un = (h.unpacked_size - i as u64 * bs).min(bs);
            if c < th && (c as u64) < un {
                println!("{}\t{}/{}\t{}\t{}", e.path, i, h.block_count, c, un);
            }
        }
    }
}
