use wowsmod_formats::geometry::{Geometry, meshopt};
fn main() {
    let p = std::env::args().nth(1).unwrap();
    let g = Geometry::parse(&std::fs::read(&p).unwrap()).unwrap();
    let x = &g.index_buffers[0];
    let idx = x.decode().unwrap();
    let enc = meshopt::encode_indices(&idx).unwrap();
    let dec = meshopt::decode_indices(idx.len(), 2, &enc).unwrap();
    let diffs: Vec<usize> = (0..idx.len()).filter(|&i| idx[i]!=dec[i]).collect();
    println!("n={} ndiff={} first={:?}", idx.len(), diffs.len(), diffs.first());
    if let Some(&f) = diffs.first() { let t=f/3*3; println!("orig tri {:?} dec tri {:?} prev {:?}", &idx[t..t+3], &dec[t..t+3], &idx[t.saturating_sub(6)..t]); }
    // degenerate triangles?
    let deg = idx.chunks(3).filter(|t| t[0]==t[1]||t[1]==t[2]||t[0]==t[2]).count();
    println!("degenerate tris={deg}");
    // python-like: is the original blob's decode equal to decode via meshopt-rs? check re-decode of original
    let orig_dec = meshopt::decode_indices(idx.len(), 2, &x.blob[8..]).unwrap();
    println!("orig redecode same={}", orig_dec==idx);
}
