// 方向探针: 每个渲染集输出 顶点格式 / 绕序-法线一致率 / 有向体积(导出后 glTF 坐标)
use wowsmod_formats::*;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ab = assets_bin::AssetsBin::parse(std::fs::read(&a[1]).unwrap()).unwrap();
    for f in &a[2..] {
        let name = std::path::Path::new(f).file_name().unwrap().to_string_lossy().to_string();
        let vp = ab.find_by_file_name(&name);
        let g = geometry::Geometry::parse(&std::fs::read(f).unwrap()).unwrap();
        let vis = ab.visuals_for_geometry(&vp[0]).unwrap();
        let sets: Vec<_> = vis[0].render_sets.iter().enumerate().map(|(k, r)| pairing::RenderSetRef::from_assets(r, format!("rs{k}"))).collect();
        let pr = pairing::pair_by_ids(&g, &sets, pairing::PairSource::AssetsBin);
        let (prims, _, _) = export::build_prims(&g, &pr.pairs).unwrap();
        println!("{name}");
        for (p, pp) in prims.iter().zip(pr.pairs.iter()) {
            let fmt = g.vertex_buffers[pp.vertices.buffer_index as usize].format();
            let ns = p.normals.as_ref().unwrap();
            let (mut agree, mut n, mut vol) = (0, 0, 0f64);
            for t in p.indices.chunks_exact(3) {
                let (x, y, z) = (p.positions[t[0] as usize], p.positions[t[1] as usize], p.positions[t[2] as usize]);
                let u = [y[0] - x[0], y[1] - x[1], y[2] - x[2]];
                let v = [z[0] - x[0], z[1] - x[1], z[2] - x[2]];
                let c = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                vol += (x[0] as f64 * c[0] as f64 + x[1] as f64 * c[1] as f64 + x[2] as f64 * c[2] as f64) / 6.0;
                let m: Vec<f32> = (0..3).map(|k| ns[t[0] as usize][k] + ns[t[1] as usize][k] + ns[t[2] as usize][k]).collect();
                n += 1;
                if c[0] * m[0] + c[1] * m[1] + c[2] * m[2] > 0.0 { agree += 1; }
            }
            // 法线朝外? 质心到顶点方向与法线点积
            let cen: Vec<f32> = (0..3).map(|k| p.positions.iter().map(|q| q[k]).sum::<f32>() / p.positions.len() as f32).collect();
            let out = p.positions.iter().zip(ns).filter(|(q, nn)| (0..3).map(|k| (q[k] - cen[k]) * nn[k]).sum::<f32>() > 0.0).count();
            println!("  {} {:<22} agree={:.4} signed_vol={:+.4} normals_outward={:.3} mat={:?}", p.name, fmt, agree as f64 / n.max(1) as f64, vol, out as f64 / ns.len() as f64, p.material);
        }
    }
}
