//! S0 clean-room corpus tool (dev only). See docs/oodle-cleanroom/ALLOWED_SOURCES.md.
//!
//! Extracts raw Oodle blocks from the user's own Lesta packs, grouped by compressor id,
//! and writes golden decompressed outputs produced by the *black-box oracle*
//! (whatever OodleBackend is registered; in dev that is the `oozextract` feature,
//! used only as a binary/API oracle -- its source is not read).
//!
//! Usage: oodle_corpus <idx dir> <res_packages> <out dir> [entries_per_codec=300]
//! Output (never commit -- game data):
//!   <out>/<codec>/<n>.blk   raw compressed block
//!   <out>/<codec>/<n>.gold  oracle output for that block
//!   <out>/meta.jsonl        {codec,id,path,block_i,block_count,src_len,dest_len,gold_crc32,entry_crc_ok}
//!   <out>/stats.json        per-codec counts + first-byte histograms
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use wowsmod_formats::{compression, index::GameIndex, pkg::PkgReader};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 4 {
        eprintln!("usage: oodle_corpus <idx dir> <res_packages> <out dir> [entries_per_codec]");
        std::process::exit(2);
    }
    let per: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(300);
    let ix = GameIndex::load_dir(&PathBuf::from(&a[1])).expect("idx");
    let rd = PkgReader::new(&PathBuf::from(&a[2]));
    let out = PathBuf::from(&a[3]);
    std::fs::create_dir_all(&out).unwrap();
    let be = compression::oodle_backend().expect("no oracle backend (build with --features oozextract)");
    let mut meta = std::fs::File::create(out.join("meta.jsonl")).unwrap();
    let mut taken: BTreeMap<u32, usize> = BTreeMap::new();
    let mut blocks: BTreeMap<u32, usize> = BTreeMap::new();
    let mut b0: BTreeMap<u32, BTreeMap<String, usize>> = BTreeMap::new();
    let mut fails: BTreeMap<u32, usize> = BTreeMap::new();
    let mut total: BTreeMap<u32, usize> = BTreeMap::new();
    // deterministic spread: stride over entries
    let oodle: Vec<_> = ix.entries.iter().filter(|e| compression::method_for(e.info.compression_info, e.info.size, e.info.unpacked_size) == compression::Method::LestaOodle).collect();
    // light I/O: only read ~1/K of entries (deterministic by path crc); no full pre-pass
    let k: u32 = std::env::var("OODLE_CORPUS_K").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
    for e in &oodle {
        if crc32fast::hash(e.path.as_bytes()) % k != 0 { continue; }
        let raw = match rd.read_raw(e) { Ok(r) => r, Err(_) => continue };
        let h = match compression::parse_lesta_header(&raw) { Ok(h) => h, Err(_) => continue };
        let c = h.compressor;
        *total.entry(c).or_default() += 1;
        if *taken.get(&c).unwrap_or(&0) >= per { continue; }
        *taken.entry(c).or_default() += 1;
        let name = compression::compressor_name(c).to_lowercase();
        let dir = out.join(&name);
        std::fs::create_dir_all(&dir).unwrap();
        // entry-level oracle check vs idx crc32
        let full = compression::decompress_entry(&raw, e.info.compression_info, e.info.size, e.info.unpacked_size, &e.path);
        let entry_ok = full.as_ref().map(|v| crc32fast::hash(v) == e.info.crc32).unwrap_or(false);
        let mut src = h.stream_offset;
        for (i, &cs) in h.block_sizes.iter().enumerate() {
            let ds = (h.unpacked_size as usize - i * h.block_size as usize).min(h.block_size as usize);
            let inp = &raw[src..src + cs as usize];
            src += cs as usize;
            let mut o = vec![0u8; ds];
            let ok = be.decompress(inp, &mut o).map(|n| n == ds).unwrap_or(false);
            if !ok { *fails.entry(c).or_default() += 1; continue; }
            let id = *blocks.entry(c).or_default();
            *blocks.get_mut(&c).unwrap() += 1;
            std::fs::write(dir.join(format!("{id:06}.blk")), inp).unwrap();
            std::fs::write(dir.join(format!("{id:06}.gold")), &o).unwrap();
            let k = format!("{:02x}{:02x}", inp.first().copied().unwrap_or(0), inp.get(1).copied().unwrap_or(0));
            *b0.entry(c).or_default().entry(k).or_default() += 1;
            let line = serde_json::json!({"codec": name, "id": id, "path": e.path, "block_i": i, "block_count": h.block_count,
                "src_len": cs, "dest_len": ds, "gold_crc32": crc32fast::hash(&o), "entry_crc_ok": entry_ok});
            writeln!(meta, "{line}").unwrap();
        }
    }
    let stats: BTreeMap<String, serde_json::Value> = total.iter().map(|(&c, &n)| {
        let mut top: Vec<_> = b0.get(&c).map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect()).unwrap_or_default();
        top.sort_by(|x: &(String, usize), y| y.1.cmp(&x.1));
        top.truncate(16);
        (compression::compressor_name(c).to_string(), serde_json::json!({"id": c, "entries_read": n,
            "entries_sampled": taken.get(&c).copied().unwrap_or(0), "blocks": blocks.get(&c).copied().unwrap_or(0),
            "oracle_block_fail": fails.get(&c).copied().unwrap_or(0), "first2_hist": top}))
    }).collect();
    std::fs::write(out.join("stats.json"), serde_json::to_string_pretty(&stats).unwrap()).unwrap();
    println!("{}", serde_json::to_string_pretty(&stats).unwrap());
}