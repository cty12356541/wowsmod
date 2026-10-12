use wowsmod_formats::{index::GameIndex, pkg::PkgReader, compression};
fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let gi = GameIndex::load_dir(std::path::Path::new(&dir)).unwrap();
    let r = PkgReader::new(std::path::Path::new(&dir));
    let mut idxs = std::collections::BTreeMap::new();
    for e in &gi.entries { *idxs.entry(e.idx.clone()).or_insert(0)+=1; }
    println!("{idxs:?}");
    for e in &gi.entries {
        if compression::method_for(e.info.compression_info,e.info.size,e.info.unpacked_size)!=compression::Method::LestaOodle {continue}
        let raw = r.read_raw(e).unwrap();
        let h = compression::parse_lesta_header(&raw).unwrap();
        let ok = r.read(e).is_ok();
        let mut bt: Vec<u32> = vec![];
        let nb=u32::from_le_bytes(raw[0x20..0x24].try_into().unwrap()) as usize; for b in 0..nb+1 { let o=h.header_len as usize+4*b; bt.push(u32::from_le_bytes(raw[o..o+4].try_into().unwrap())); }
        let start = h.header_len as usize + 4*(nb+1); let extra = raw.len() as i64 - (h.header_len as i64 + h.packed_size as i64);
        println!("extra={extra} nb={nb} {} ok={ok} size={} us={} hdr={:?} head={:02x?} blocks={:?} hdrbytes={:02x?}", e.path, e.info.size, e.info.unpacked_size, (h.compressor,h.block_count,h.unpacked_size,h.packed_size,h.block_size), &raw[start..start+4], &bt, &raw[0x20..0x34]);
    }
}
