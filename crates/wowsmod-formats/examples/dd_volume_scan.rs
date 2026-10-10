//! 验证莱服贴图分卷规则: 对每组 `name.dds` + `name.dd0..` :
//! 1. 头部 K(dwReserved1[0], 'QHDD')== 实际分卷文件数;
//! 2. K == volume_count_for(原图尺寸, 512)(规则: 最大边 > 512 的 mip 各成一卷);
//! 3. 合并 → 拆分 逐字节还原全部文件(BC7Prep 文件先还原成标准 BC7 再比较)。
//! 用法:
//!   dd_volume_scan dir <目录>                  (本地已解包目录, 递归)
//!   dd_volume_scan game <idx 目录> <res_packages 目录> [glob, 默认 "*"]   → stdout JSON
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use wowsmod_formats::{bc7prep, dds, index::GameIndex, pkg::PkgReader};

#[derive(Default)]
struct St {
    groups: u64,
    with_volumes: u64,
    roundtrip_ok: u64,
    k_hist: BTreeMap<String, u64>,
    tail_top_hist: BTreeMap<String, u64>,
    no_vol_top_hist: BTreeMap<String, u64>,
    formats: BTreeMap<String, u64>,
    prep_groups: u64,
    problems: Vec<String>,
}

fn decode(d: Vec<u8>) -> Result<Vec<u8>, String> {
    if bc7prep::is_bc7prep_dds(&d) {
        bc7prep::decode_dds(&d).map_err(|e| e.to_string())
    } else {
        Ok(d)
    }
}

fn check_group(name: &str, tail: Vec<u8>, vols: Vec<Vec<u8>>, st: &Mutex<St>) {
    let prep = bc7prep::is_bc7prep_dds(&tail) || vols.iter().any(|v| bc7prep::is_bc7prep_dds(v));
    let r = (|| -> Result<(String, String, u32), String> {
        let tail = decode(tail)?;
        let vols: Vec<Vec<u8>> = vols.into_iter().map(decode).collect::<Result<_, _>>()?;
        let th = dds::Header::parse(&tail).map_err(|e| e.to_string())?;
        let k = th.volume_count().unwrap_or(0);
        let fmt = format!("{}{}", th.format.name(), th.dxgi.map(|f| format!("/dxgi{f}")).unwrap_or_default());
        let top = format!("{}x{}", th.width, th.height);
        if k as usize != vols.len() {
            return Err(format!("头部 K={k} 但分卷文件 {} 个", vols.len()));
        }
        if k == 0 {
            if th.layers() == 1 && th.width.max(th.height) > dds::VOLUME_TAIL_MAX {
                return Err(format!("无分卷但顶级 {top} > 512 (规则例外)"));
            }
            // 数据长度自检
            let need = th.data_size().map_err(|e| e.to_string())?;
            if tail.len() != th.len() + need {
                return Err(format!("无分卷 .dds 长度 {} != 头 {} + 数据 {need}", tail.len(), th.len()));
            }
            return Ok((fmt, top, 0));
        }
        let refs: Vec<&[u8]> = vols.iter().map(|v| v.as_slice()).collect();
        let full = dds::merge_volumes(&tail, &refs).map_err(|e| e.to_string())?;
        let rule_k = dds::volume_count_for(full.header.width, full.header.height, dds::VOLUME_TAIL_MAX);
        if rule_k != k {
            return Err(format!("K={k} 但规则给出 {rule_k} (原图 {}x{})", full.header.width, full.header.height));
        }
        let (t2, v2) = dds::split_volumes(&full, k).map_err(|e| e.to_string())?;
        if t2 != tail {
            let i = t2.iter().zip(&tail).position(|(a, b)| a != b).unwrap_or(t2.len().min(tail.len()));
            return Err(format!(".dds 往返不一致, 首个差异偏移 {i}"));
        }
        for (i, (a, b)) in v2.iter().zip(&vols).enumerate() {
            if a != b {
                let j = a.iter().zip(b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
                return Err(format!("dd{i} 往返不一致, 首个差异偏移 {j}"));
            }
        }
        Ok((fmt, top, k))
    })();
    let mut g = st.lock().unwrap();
    g.groups += 1;
    if prep {
        g.prep_groups += 1;
    }
    match r {
        Ok((fmt, top, k)) => {
            *g.formats.entry(fmt).or_default() += 1;
            if k > 0 {
                g.with_volumes += 1;
                g.roundtrip_ok += 1;
                *g.k_hist.entry(k.to_string()).or_default() += 1;
                *g.tail_top_hist.entry(top).or_default() += 1;
            } else {
                *g.no_vol_top_hist.entry(top).or_default() += 1;
            }
        }
        Err(e) => g.problems.push(format!("{name}: {e}")),
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let st = Mutex::new(St::default());
    // 组: base -> (tail 路径, [卷路径])
    let mut groups: BTreeMap<String, (Option<String>, BTreeMap<u32, String>)> = BTreeMap::new();
    let mut add = |p: &str| {
        let (base, ext) = match p.rsplit_once('.') {
            Some(x) => x,
            None => return,
        };
        let e = ext.to_ascii_lowercase();
        let g = groups.entry(base.to_string()).or_default();
        if e == "dds" {
            g.0 = Some(p.to_string());
        } else if let Some(n) = e.strip_prefix("dd").and_then(|n| n.parse::<u32>().ok()) {
            g.1.insert(n, p.to_string());
        }
    };
    let mode = a.get(1).map(String::as_str).unwrap_or("");
    let ix;
    let reader: Box<dyn Fn(&str) -> Result<Vec<u8>, String> + Sync + '_>;
    if mode == "dir" {
        fn walk(p: &Path, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(p).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out)
                } else {
                    out.push(p)
                }
            }
        }
        let mut files = vec![];
        walk(Path::new(&a[2]), &mut files);
        for f in &files {
            add(&f.to_string_lossy());
        }
        reader = Box::new(|p: &str| std::fs::read(p).map_err(|e| e.to_string()));
    } else if mode == "game" {
        ix = GameIndex::load_dir(Path::new(&a[2])).expect("idx");
        let pat = a.get(4).cloned().unwrap_or_else(|| "*".into());
        for e in ix.search(&pat).expect("glob") {
            add(&e.path);
        }
        let pr = PkgReader::new(Path::new(&a[3]));
        let map: HashMap<String, _> = ix.search(&pat).unwrap().into_iter().map(|e| (e.path.clone(), e)).collect();
        reader = Box::new(move |p: &str| pr.read(map[p]).map_err(|e| e.to_string()));
    } else {
        eprintln!("用法: dd_volume_scan dir <目录> | game <idx> <res_packages> [glob]");
        std::process::exit(2);
    }
    let list: Vec<_> = groups.into_iter().filter(|(_, (t, v))| t.is_some() || !v.is_empty()).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if i >= list.len() {
                    break;
                }
                let (base, (tail, vols)) = &list[i];
                let Some(tail) = tail else {
                    st.lock().unwrap().problems.push(format!("{base}: 有分卷但没有 .dds"));
                    continue;
                };
                // 分卷编号必须连续 0..K
                if vols.keys().enumerate().any(|(i, &n)| i as u32 != n) {
                    st.lock().unwrap().problems.push(format!("{base}: 分卷编号不连续 {:?}", vols.keys().collect::<Vec<_>>()));
                    continue;
                }
                let t = match reader(tail) {
                    Ok(d) => d,
                    Err(e) => {
                        st.lock().unwrap().problems.push(format!("{tail}: 读取失败 {e}"));
                        continue;
                    }
                };
                let mut vs = vec![];
                let mut bad = false;
                for p in vols.values() {
                    match reader(p) {
                        Ok(d) => vs.push(d),
                        Err(e) => {
                            st.lock().unwrap().problems.push(format!("{p}: 读取失败 {e}"));
                            bad = true;
                            break;
                        }
                    }
                }
                if !bad {
                    check_group(base, t, vs, &st);
                }
            });
        }
    });
    let g = st.into_inner().unwrap();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "groups": g.groups, "with_volumes": g.with_volumes, "roundtrip_ok": g.roundtrip_ok, "bc7prep_groups": g.prep_groups,
            "volume_count_hist": g.k_hist, "dds_top_dims_with_volumes": g.tail_top_hist, "dds_top_dims_without_volumes": g.no_vol_top_hist,
            "formats": g.formats, "problems_count": g.problems.len(), "problems": g.problems,
        }))
        .unwrap()
    );
}
