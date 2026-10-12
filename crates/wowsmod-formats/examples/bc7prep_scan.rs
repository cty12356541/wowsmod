//! 全量扫描: 读取游戏内全部 .dds/.dd0/.dd1/.dd2, 统计 BC7Prep 贴图的 flags / 模式计数,
//! 并对每一个尝试还原(只检查能否还原、长度是否正确; 逐字节正确性由 accept_textures.ps1 与 pfsunpack 对比)。
//! 遇到"不支持"的情形时把原始文件存到 <样本目录>(每类最多 3 个), 便于继续逆向。
//! 第 4 个参数可指定 glob(默认 "*.dd[s012]"); 用 "*" 即对游戏内全部文件做一次解压 + crc32 校验(全量解包自检)。
//! 用法: bc7prep_scan <idx 目录> <res_packages 目录> [样本目录|-] [glob]   → stdout 输出 JSON
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use wowsmod_formats::{bc7prep, index::GameIndex, pkg::PkgReader};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (idx, res) = (PathBuf::from(&a[1]), PathBuf::from(&a[2]));
    let sample_dir = a.get(3).filter(|s| s.as_str() != "-").map(PathBuf::from);
    let pattern = a.get(4).cloned().unwrap_or_else(|| "*.dd[s012]".to_string());
    let ix = GameIndex::load_dir(&idx).expect("idx");
    let reader = PkgReader::new(&res);
    let tex: Vec<_> = ix
        .search(&pattern)
        .expect("glob")
        .into_iter()
        .collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    #[derive(Default)]
    struct St {
        textures: u64,
        prep: u64,
        decoded: u64,
        read_errors: Vec<String>,
        bytes: u64,
        flags: BTreeMap<String, u64>,
        mode_blocks: [u64; 10],
        decor_mode_blocks: [u64; 10],
        unsupported: BTreeMap<String, Vec<String>>,
        bad_len: Vec<String>,
    }
    let st = Mutex::new(St::default());
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if i >= tex.len() {
                    break;
                }
                let e = tex[i];
                let d = match reader.read(e) {
                    Ok(d) => d,
                    Err(err) => {
                        st.lock().unwrap().read_errors.push(format!("{}: {err}", e.path));
                        continue;
                    }
                };
                let is = bc7prep::is_bc7prep_dds(&d);
                let mut g = st.lock().unwrap();
                g.textures += 1;
                g.bytes += d.len() as u64;
                if !is {
                    continue;
                }
                g.prep += 1;
                let q = &d[148..];
                let flags = u32::from_le_bytes(q[4..8].try_into().unwrap());
                *g.flags.entry(format!("{flags:#x}")).or_default() += 1;
                let mut counts = [0u64; 10];
                for (m, c) in counts.iter_mut().enumerate() {
                    *c = u32::from_le_bytes(q[8 + 4 * m..12 + 4 * m].try_into().unwrap()) as u64;
                }
                for m in 0..10 {
                    if flags & bc7prep::FLAG_DECORRELATE != 0 {
                        g.decor_mode_blocks[m] += counts[m];
                    } else {
                        g.mode_blocks[m] += counts[m];
                    }
                }
                drop(g);
                let r = bc7prep::decode_dds(&d);
                let mut g = st.lock().unwrap();
                match r {
                    Ok(out) => {
                        let n: u64 = counts.iter().sum();
                        if out.len() as u64 != 148 + 16 * n {
                            g.bad_len.push(e.path.clone());
                        } else {
                            g.decoded += 1;
                        }
                    }
                    Err(err) => {
                        let key = err.to_string();
                        let v = g.unsupported.entry(key).or_default();
                        v.push(e.path.clone());
                        if v.len() <= 3 {
                            if let Some(sd) = &sample_dir {
                                let _ = std::fs::create_dir_all(sd);
                                let _ = std::fs::write(sd.join(e.path.replace('/', "__")), &d);
                            }
                        }
                    }
                }
            });
        }
    });
    let g = st.into_inner().unwrap();
    let unsup: BTreeMap<_, _> = g.unsupported.iter().map(|(k, v)| (k.clone(), serde_json::json!({"count": v.len(), "examples": v.iter().take(5).collect::<Vec<_>>()}))).collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "pattern": pattern, "files_matched": tex.len(), "files_read_ok": g.textures, "bytes": g.bytes, "bc7prep": g.prep, "decoded_ok": g.decoded,
            "read_errors": g.read_errors.len(), "read_error_list": g.read_errors,
            "bad_length": g.bad_len, "flags": g.flags,
            "mode_blocks_plain": g.mode_blocks, "mode_blocks_decorrelated": g.decor_mode_blocks,
            "unsupported": unsup,
        }))
        .unwrap()
    );
}
