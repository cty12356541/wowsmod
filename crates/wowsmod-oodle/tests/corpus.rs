//! Golden tests against the local corpus (game data, never in git). Skips if OODLE_CORPUS unset.
use std::path::PathBuf;

fn corpus() -> Option<PathBuf> { std::env::var_os("OODLE_CORPUS").map(PathBuf::from).filter(|p| p.join("meta.jsonl").exists()) }

#[test]
fn corpus_match_rate() {
    let Some(root) = corpus() else { eprintln!("OODLE_CORPUS not set; skipping"); return; };
    for codec in ["mermaid", "hydra", "kraken", "leviathan"] {
        let dir = root.join(codec);
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let (mut n, mut ok) = (0usize, 0usize);
        let mut fam = std::collections::BTreeMap::<String, (usize, usize)>::new();
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("blk") { continue; }
            let blk = std::fs::read(&p).unwrap();
            let gold = std::fs::read(p.with_extension("gold")).unwrap();
            let mut out = vec![0u8; gold.len()];
            let good = matches!(wowsmod_oodle::decompress_block(&blk, &mut out), Ok(k) if k == gold.len() && out == gold);
            n += 1; ok += good as usize;
            let f = fam.entry(format!("{:?}", wowsmod_oodle::family(&blk))).or_default();
            f.0 += 1; f.1 += good as usize;
        }
        eprintln!("{codec}: {ok}/{n} blocks match golden; by family {fam:?}");
    }
}