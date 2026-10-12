use wowsmod_formats::*;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let t = std::time::Instant::now();
    let ab = assets_bin::AssetsBin::parse(std::fs::read(&a[1]).unwrap()).unwrap();
    println!("assets parsed {:?} paths={}", t.elapsed(), ab.paths.len());
    for gp in &a[3..] {
        let vpath = format!("content/gameplay/france/ship/battleship/FSB003_Richelieu_1944/{gp}");
        let g = geometry::Geometry::parse(&std::fs::read(format!("{}/{gp}", a[2])).unwrap()).unwrap();
        let t = std::time::Instant::now();
        let vis = ab.visuals_for_geometry(&vpath).unwrap();
        println!("{gp}: vmaps={} imaps={} visuals={:?} ({:?})", g.vertex_mappings.len(), g.index_mappings.len(), vis.iter().map(|v| (v.path.rsplit('/').next().unwrap().to_string(), v.render_sets.len())).collect::<Vec<_>>(), t.elapsed());
        for (label, pairs) in [("assets", { let main = &vis[0]; let sets: Vec<_> = main.render_sets.iter().enumerate().map(|(k,r)| pairing::RenderSetRef::from_assets(r, format!("rs{k}"))).collect(); let pr = pairing::pair_by_ids(&g, &sets, pairing::PairSource::AssetsBin); println!("  warn {:?}", pr.warnings); pr.pairs }), ("naive", export::naive_pairs(&g))] {
            let (prims, w, ns) = export::build_prims(&g, &pairs).unwrap();
            let items: Vec<_> = prims.iter().map(|p| health::PrimInput{name: p.name.clone(), positions: p.positions.as_slice(), indices: p.indices.as_slice(), normals: p.normals.as_deref()}).collect();
            let h = health::check(&items);
            println!("  {label}: prims={} tris={} oob={} spikes={} long={} glong={} ok={} stats={:?} warn={}", prims.len(), h.total_triangles, h.total_oob_triangles, h.total_spike_triangles, h.total_long_triangles, h.total_global_long_triangles, h.ok, ns, w.len());
            if label=="assets" { for (p, ph) in prims.iter().zip(h.primitives.iter()) { if !ph.ok { println!("     BAD {} mat={:?} tris={} spikes={}", p.name, p.material, ph.triangles, ph.spike_triangles); } } }
            if label=="assets" { let glb = glb::write_glb(&prims, serde_json::json!({})); std::fs::write(format!("/tmp/{gp}.glb"), &glb).unwrap(); let h2 = health::check_glb(&glb).unwrap(); println!("  reread glb ok={} tris={} oob={} spikes={}", h2.ok, h2.total_triangles, h2.total_oob_triangles, h2.total_spike_triangles); }
        }
    }
}
