//! 诊断: 打印条目的莱服 Oodle 容器块表, 并逐块尝试解码; 失败块把原始字节写到 <输出目录>。
//! 用法: oodle_blocks <idx 目录> <res_packages> <输出目录> <虚拟路径>...
use std::path::PathBuf;
use wowsmod_formats::{compression, index::GameIndex, pkg::PkgReader};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ix = GameIndex::load_dir(&PathBuf::from(&a[1])).unwrap();
    let rd = PkgReader::new(&PathBuf::from(&a[2]));
    let out = PathBuf::from(&a[3]);
    std::fs::create_dir_all(&out).unwrap();
    let be = compression::oodle_backend().unwrap();
    for p in &a[4..] {
        let e = ix.get(p).expect("not in index");
        let raw = rd.read_raw(e).unwrap();
        std::fs::write(out.join(p.replace('/', "__") + ".raw"), &raw).unwrap();
        let h = compression::parse_lesta_header(&raw).unwrap();
        println!("{p}: compressor {} ({}) unk {:#x} unpacked {} packed {} blocks {} bs {}", h.compressor, compression::compressor_name(h.compressor), h.unknown_0c, h.unpacked_size, h.packed_size, h.block_count, h.block_size);
        let mut src = h.stream_offset;
        for (i, &cs) in h.block_sizes.iter().enumerate() {
            let ds = (h.unpacked_size as usize - i * h.block_size as usize).min(h.block_size as usize);
            let inp = &raw[src..src + cs as usize];
            let mut o = vec![0u8; ds];
            let r = be.decompress(inp, &mut o);
            println!("  block {i}: csize {cs} dsize {ds} first bytes {:02x?} -> {:?}", &inp[..8.min(inp.len())], r.as_ref().map(|n| *n));
            src += cs as usize;
        }
    }
}
