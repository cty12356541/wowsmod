//! Golden tests against the local corpus (game data, never in git). Skips if OODLE_CORPUS unset.
//! Reports match rate per container codec and per block class (first two bytes).
use std::collections::BTreeMap;
use std::path::PathBuf;

fn corpus() -> Option<PathBuf> {
    std::env::var_os("OODLE_CORPUS").map(PathBuf::from).filter(|p| p.join("meta.jsonl").exists())
}

fn class(blk: &[u8]) -> String {
    match blk { [a, b, ..] => format!("{a:02x}{b:02x}"), _ => "short".into() }
}

#[test]
fn corpus_match_rate() {
    let Some(root) = corpus() else { eprintln!("OODLE_CORPUS not set; skipping"); return; };
    // class -> (blocks, matched, errors)
    let mut by_class = BTreeMap::<String, (usize, usize, usize)>::new();
    for codec in ["mermaid", "hydra", "kraken", "leviathan"] {
        let Ok(rd) = std::fs::read_dir(root.join(codec)) else { continue };
        let (mut n, mut ok) = (0usize, 0usize);
        let mut paths: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().and_then(|s| s.to_str()) == Some("blk")).collect();
        paths.sort();
        for p in paths {
            let blk = std::fs::read(&p).unwrap();
            let gold = std::fs::read(p.with_extension("gold")).unwrap();
            let mut out = vec![0u8; gold.len()];
            let r = wowsmod_oodle::decompress_block(&blk, &mut out);
            let good = matches!(r, Ok(k) if k == gold.len() && out == gold);
            n += 1; ok += good as usize;
            let c = by_class.entry(class(&blk)).or_default();
            c.0 += 1; c.1 += good as usize; c.2 += r.is_err() as usize;
        }
        eprintln!("container {codec:>9}: {ok}/{n} blocks match golden");
    }
    eprintln!("per block class (first two bytes): class blocks matched errors rate");
    for (c, (n, ok, err)) in &by_class {
        eprintln!("  {c:>5} {n:>6} {ok:>6} {err:>6} {:>6.1}%", 100.0 * *ok as f64 / (*n).max(1) as f64);
    }
}
