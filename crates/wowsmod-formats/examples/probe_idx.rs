use wowsmod_formats::{index::GameIndex, pkg::PkgReader, compression};
fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let gi = GameIndex::load_dir(std::path::Path::new(&dir)).unwrap();
    println!("entries={} dups={}", gi.entries.len(), gi.duplicates);
    let mut r = PkgReader::new(std::path::Path::new(&dir));
    r.verify_crc = false;
    let (mut ok, mut bad, mut err) = (0, 0, 0);
    for e in &gi.entries {
        match r.read(e) {
            Ok(d) => { let c = crc32fast::hash(&d); let rc = crc32fast::hash(&r.read_raw(e).unwrap());
                if c == e.info.crc32 { ok += 1 } else { bad += 1; if bad < 4 { println!("crc mismatch {} m={:?} rawcrc_match={}", e.path, compression::method_for(e.info.compression_info,e.info.size,e.info.unpacked_size), rc==e.info.crc32) } } }
            Err(x) => { err += 1; if err < 4 { println!("ERR {}: {x}", e.path) } }
        }
    }
    println!("crc ok={ok} bad={bad} err={err}");
    for e in gi.entries.iter().take(3) { println!("{} {} {:x}", e.path, e.pkg, e.info.compression_info); }
}
