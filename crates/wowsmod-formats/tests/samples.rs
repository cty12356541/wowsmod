//! 依赖真实游戏样本的集成测试。样本不随仓库分发(版权), 用 scripts/fetch_samples.ps1 准备。
//!
//! 环境变量:
//! - WOWSMOD_GEOMETRY_SAMPLES: 18 个 .geometry 样本目录(默认 <workspace>/testdata/geometry18)
//! - WOWSMOD_ASSETS_BIN: assets.bin 路径(默认 <workspace>/testdata/assets.bin)
//! - WOWSMOD_PKG_SAMPLES: 含 *.idx 与对应 *.pkg 的目录(默认 <workspace>/testdata/pkg)
//! - WOWSMOD_REQUIRE_SAMPLES=1: 样本缺失时测试失败(验收时使用), 否则跳过并打印提示。
use std::path::{Path, PathBuf};
use wowsmod_formats::*;

fn ws() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sample_path(var: &str, default: &str) -> Option<PathBuf> {
    let p = std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| ws().join(default));
    if p.exists() {
        Some(p)
    } else if std::env::var("WOWSMOD_REQUIRE_SAMPLES").as_deref() == Ok("1") {
        panic!("缺少样本 {} ({var}); 先运行 scripts/fetch_samples.ps1", p.display());
    } else {
        eprintln!("跳过: 样本不存在 {} (设置 {var} 或运行 scripts/fetch_samples.ps1)", p.display());
        None
    }
}

fn geometry_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("geometry")).unwrap_or(false))
        .collect();
    v.sort();
    v
}

/// 验收 M1-5: 18 个样本 geometry 逐字节往返(解析→序列化, 以及 解码→重新编码→序列化)
#[test]
fn geometry_roundtrip_18_samples() {
    let Some(dir) = sample_path("WOWSMOD_GEOMETRY_SAMPLES", "testdata/geometry18") else { return };
    let files = geometry_files(&dir);
    assert_eq!(files.len(), 18, "样本目录应有 18 个 .geometry: {}", dir.display());
    let mut lines = Vec::new();
    for f in &files {
        let d = std::fs::read(f).unwrap();
        let g = geometry::Geometry::parse(&d).unwrap();
        assert!(g.serialize() == d, "重排序列化不一致: {}", f.display());
        let mut g2 = g.clone();
        for v in g2.vertex_buffers.iter_mut() {
            let raw = v.decode().unwrap();
            v.encode_from(&raw).unwrap();
        }
        for x in g2.index_buffers.iter_mut() {
            let idx = x.decode().unwrap();
            x.encode_from(&idx).unwrap();
        }
        assert!(g2.serialize() == d, "重新编码后不一致: {}", f.display());
        lines.push(format!("OK {}", f.file_name().unwrap().to_string_lossy()));
    }
    eprintln!("{}\n18/18 逐字节一致", lines.join("\n"));
}

/// 验收 M1-4 的库级部分: 18 个样本用 assets.bin 配对导出, 0 越界 / 0 尖刺 / 包围盒全部一致
#[test]
fn export_health_with_assets_bin() {
    let Some(dir) = sample_path("WOWSMOD_GEOMETRY_SAMPLES", "testdata/geometry18") else { return };
    let Some(ab) = sample_path("WOWSMOD_ASSETS_BIN", "testdata/assets.bin") else { return };
    let ab = assets_bin::AssetsBin::parse(std::fs::read(ab).unwrap()).unwrap();
    for f in geometry_files(&dir) {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let vp = ab.find_by_file_name(&name);
        assert_eq!(vp.len(), 1, "{name} 在 assets.bin 中应唯一");
        let g = geometry::Geometry::parse(&std::fs::read(&f).unwrap()).unwrap();
        let vis = ab.visuals_for_geometry(&vp[0]).unwrap();
        let mut sets = Vec::new();
        for v in vis.iter().filter(|v| !v.render_sets.is_empty()) {
            for (k, r) in v.render_sets.iter().enumerate() {
                sets.push(pairing::RenderSetRef::from_assets(r, format!("{}/rs{k}", v.path)));
            }
        }
        let pr = pairing::pair_by_ids(&g, &sets, pairing::PairSource::AssetsBin);
        assert!(!pr.pairs.is_empty(), "{name}: 无配对");
        let (prims, _w, st) = export::build_prims(&g, &pr.pairs).unwrap();
        assert_eq!(st.bbox_mismatch, 0, "{name}: 包围盒不一致");
        assert_eq!(st.bbox_checked, pr.pairs.len());
        let items: Vec<_> = prims
            .iter()
            .map(|p| health::PrimInput { name: p.name.clone(), positions: &p.positions, indices: &p.indices, normals: p.normals.as_deref() })
            .collect();
        let h = health::check(&items);
        assert_eq!(h.total_oob_triangles, 0, "{name}");
        assert_eq!(h.total_spike_triangles, 0, "{name}");
        assert!(st.winding_agrees > 0.98, "{name}: 绕序一致率 {}", st.winding_agrees);
        // GLB 写出再读回仍健康
        let glb = glb::write_glb(&prims, serde_json::json!({}));
        let h2 = health::check_glb(&glb).unwrap();
        assert!(h2.ok, "{name}: GLB 复查失败");
        eprintln!("OK {name}: {} 渲染集, {} 三角形", pr.pairs.len(), h.total_triangles);
    }
}

/// 对照: naive(数组下标)配对必然被健康检查抓出(检查器灵敏度)
#[test]
fn naive_pairing_is_detected() {
    let Some(dir) = sample_path("WOWSMOD_GEOMETRY_SAMPLES", "testdata/geometry18") else { return };
    let f = dir.join("FSB003_Richelieu_1944_Bow.geometry");
    let g = geometry::Geometry::parse(&std::fs::read(f).unwrap()).unwrap();
    let pairs = export::naive_pairs(&g);
    let (prims, _, _) = export::build_prims(&g, &pairs).unwrap();
    let items: Vec<_> = prims
        .iter()
        .map(|p| health::PrimInput { name: p.name.clone(), positions: &p.positions, indices: &p.indices, normals: p.normals.as_deref() })
        .collect();
    let h = health::check(&items);
    assert!(h.total_oob_triangles > 1000 && h.total_spike_triangles > 10, "{} {}", h.total_oob_triangles, h.total_spike_triangles);
}

/// idx/pkg: 每个条目解压后 crc32 与 idx 记录一致
#[test]
fn pkg_all_entries_crc() {
    let Some(dir) = sample_path("WOWSMOD_PKG_SAMPLES", "testdata/pkg") else { return };
    let idx = index::GameIndex::load_dir(&dir).unwrap();
    let rd = pkg::PkgReader::new(&dir);
    let (mut n, mut multi) = (0, 0);
    for e in &idx.entries {
        let d = rd.read(e).unwrap_or_else(|x| panic!("{}: {x}", e.path));
        assert_eq!(d.len() as u64, e.info.unpacked_size as u64, "{}", e.path);
        assert_eq!(crc32fast::hash(&d), e.info.crc32, "{}", e.path);
        if compression::method_for(e.info.compression_info, e.info.size, e.info.unpacked_size) == compression::Method::LestaOodle {
            let raw = rd.read_raw(e).unwrap();
            if compression::parse_lesta_header(&raw).unwrap().block_count > 1 {
                multi += 1;
            }
        }
        n += 1;
    }
    eprintln!("{n} 个条目 crc 全部一致, 其中多块 Oodle {multi} 个");
    assert!(n > 0);
}

/// GameParams: 解析全部舰船, 勃艮第/黎塞留的参数与船体模型
#[test]
fn gameparams_ships() {
    let Some(p) = sample_path("WOWSMOD_GAMEPARAMS", "testdata/GameParams_py3.data") else { return };
    let ships = gameparams::ships_from_raw(&std::fs::read(p).unwrap()).unwrap();
    assert!(ships.len() > 1000, "舰船数 {}", ships.len());
    let b = ships.iter().find(|s| s.index == "PFSB510").expect("PFSB510");
    assert_eq!(b.name, "PFSB510_Bourgogne");
    assert_eq!(b.level, 10);
    assert_eq!(b.hull_models, vec!["content/gameplay/france/ship/battleship/FSB025_Bourgogne_1945/FSB025_Bourgogne_1945.model".to_string()]);
    assert!(b.other_models.iter().any(|m| m.contains("FGM049_380mm_45_Mle_1935")));
    let r = ships.iter().find(|s| s.index == "PFSB108").expect("PFSB108");
    assert!(r.hull_models[0].contains("FSB003_Richelieu_1944"));
    eprintln!("{} 条舰船", ships.len());
}

/// BC7Prep 逆变换: 真实贴图(pkg 原始字节)还原后与 pfsunpack 输出逐字节一致。
/// 目录结构: <dir>/raw/<名> 与 <dir>/ref/<名>(fetch_samples.ps1 用 `wowsmod extract --raw` 与 pfsunpack 生成)。
#[test]
fn bc7prep_matches_pfsunpack() {
    let Some(dir) = sample_path("WOWSMOD_BC7PREP_SAMPLES", "testdata/bc7prep") else { return };
    let mut names: Vec<_> = std::fs::read_dir(dir.join("raw")).unwrap().filter_map(|e| e.ok().map(|e| e.file_name())).collect();
    names.sort();
    assert!(!names.is_empty(), "{} 下没有样本", dir.display());
    let mut modes = [0usize; 10];
    for n in &names {
        let raw = std::fs::read(dir.join("raw").join(n)).unwrap();
        let reff = std::fs::read(dir.join("ref").join(n)).unwrap();
        assert!(bc7prep::is_bc7prep_dds(&raw), "不是 BC7Prep: {n:?}");
        for (m, c) in modes.iter_mut().enumerate() {
            *c += u32::from_le_bytes(raw[148 + 8 + 4 * m..148 + 12 + 4 * m].try_into().unwrap()) as usize;
        }
        let out = bc7prep::decode_dds(&raw).unwrap();
        assert!(out == reff, "还原结果与 pfsunpack 不一致: {n:?}");
    }
    eprintln!("BC7Prep {} 个样本逐字节一致; 各模式块数 {:?}", names.len(), modes);
}
