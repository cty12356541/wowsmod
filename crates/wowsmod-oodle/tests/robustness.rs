//! Fuzz-style robustness: the decoder must never panic, whatever the input. Contains no decoding logic.
//! Runs without a corpus; with OODLE_CORPUS set it also truncates / bit-flips real blocks.
use std::path::PathBuf;

struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 { let mut x = self.0; x ^= x << 13; x ^= x >> 7; x ^= x << 17; self.0 = x; x }
    fn bytes(&mut self, n: usize) -> Vec<u8> { (0..n).map(|_| self.next() as u8).collect() }
}

fn run(input: &[u8], out_len: usize) {
    let mut out = vec![0u8; out_len];
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| wowsmod_oodle::decompress_block(input, &mut out)));
    assert!(r.is_ok(), "decoder panicked on input of len {} (head {:02x?})", input.len(), &input[..input.len().min(16)]);
    if let Ok(Ok(k)) = r { assert!(k <= out_len, "reported {k} bytes > buffer {out_len}"); }
}

#[test]
fn random_inputs_do_not_panic() {
    let mut rng = XorShift(0x9e37_79b9_7f4a_7c15);
    let heads: [[u8; 2]; 5] = [[0x8c, 0x0a], [0xcc, 0x06], [0x8c, 0x06], [0x8c, 0x0c], [0xcc, 0x0a]];
    for i in 0..4000 {
        let len = (rng.next() % 600) as usize;
        let mut v = rng.bytes(len);
        if len >= 2 && i % 2 == 0 { v[..2].copy_from_slice(&heads[i % heads.len()]); }
        let out_len = [0usize, 1, 128, 0x20000, 0x40000][(rng.next() % 5) as usize];
        run(&v, out_len);
    }
    for n in 0..8 { run(&vec![0u8; n], 0x40000); run(&vec![0xffu8; n], 0x40000); }
}

#[test]
fn corrupted_corpus_blocks_do_not_panic() {
    let Some(root) = std::env::var_os("OODLE_CORPUS").map(PathBuf::from) else { eprintln!("OODLE_CORPUS not set; skipping"); return; };
    let mut rng = XorShift(0x1234_5678_9abc_def1);
    let limit: usize = std::env::var("OODLE_FUZZ_BLOCKS").ok().and_then(|s| s.parse().ok()).unwrap_or(200);
    let mut done = 0;
    for codec in ["mermaid", "hydra"] {
        let Ok(rd) = std::fs::read_dir(root.join(codec)) else { continue };
        for p in rd.flatten().map(|e| e.path()).filter(|p| p.extension().and_then(|s| s.to_str()) == Some("blk")) {
            if done >= limit { return; }
            let blk = std::fs::read(&p).unwrap();
            let gold_len = std::fs::metadata(p.with_extension("gold")).map(|m| m.len() as usize).unwrap_or(0x40000);
            for cut in [0, 1, 2, 5, 8, blk.len() / 2, blk.len().saturating_sub(1)] { run(&blk[..cut.min(blk.len())], gold_len); }
            for _ in 0..8 {
                let mut v = blk.clone();
                if !v.is_empty() { let i = (rng.next() as usize) % v.len(); v[i] ^= 1 << (rng.next() % 8); }
                run(&v, gold_len);
            }
            done += 1;
        }
    }
}
