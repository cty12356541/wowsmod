// 打印指定 geometry 中被判为尖刺的三角形细节(顶点、法线、边长、相邻三角形)
use wowsmod_formats::*;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ab = assets_bin::AssetsBin::parse(std::fs::read(&a[1]).unwrap()).unwrap();
    let f = &a[2];
    let name = std::path::Path::new(f).file_name().unwrap().to_string_lossy().to_string();
    let vp = ab.find_by_file_name(&name);
    let g = geometry::Geometry::parse(&std::fs::read(f).unwrap()).unwrap();
    let vis = ab.visuals_for_geometry(&vp[0]).unwrap();
    let sets: Vec<_> = vis[0].render_sets.iter().enumerate().map(|(k, r)| pairing::RenderSetRef::from_assets(r, format!("rs{k}"))).collect();
    let pr = pairing::pair_by_ids(&g, &sets, pairing::PairSource::AssetsBin);
    let (prims, _, _) = export::build_prims(&g, &pr.pairs).unwrap();
    for p in &prims {
        let h = health::check(&[health::PrimInput { name: p.name.clone(), positions: &p.positions, indices: &p.indices, normals: p.normals.as_deref() }]);
        if h.total_spike_triangles == 0 { continue; }
        let ns = p.normals.as_ref().unwrap();
        let mut mn=[f32::MAX;3]; let mut mx=[f32::MIN;3]; for q in &p.positions { for k in 0..3 { mn[k]=mn[k].min(q[k]); mx[k]=mx[k].max(q[k]); } }
        let diag=((0..3).map(|k| (mx[k]-mn[k]).powi(2)).sum::<f32>()).sqrt();
        println!("{} {:?} tris={} diag={diag:.3} bbox {mn:?}..{mx:?}", p.name, p.material, p.indices.len()/3);
        for (ti, t) in p.indices.chunks_exact(3).enumerate() {
            let v: Vec<[f32;3]> = t.iter().map(|&i| p.positions[i as usize]).collect();
            let d = |x:&[f32;3],y:&[f32;3]| ((0..3).map(|k| (x[k]-y[k]).powi(2)).sum::<f32>()).sqrt();
            let e = d(&v[0],&v[1]).max(d(&v[1],&v[2])).max(d(&v[2],&v[0]));
            if e <= 0.5*diag { continue; }
            let u=[v[1][0]-v[0][0],v[1][1]-v[0][1],v[1][2]-v[0][2]]; let w=[v[2][0]-v[0][0],v[2][1]-v[0][1],v[2][2]-v[0][2]];
            let c=[u[1]*w[2]-u[2]*w[1],u[2]*w[0]-u[0]*w[2],u[0]*w[1]-u[1]*w[0]];
            let m: Vec<f32> = (0..3).map(|k| ns[t[0] as usize][k]+ns[t[1] as usize][k]+ns[t[2] as usize][k]).collect();
            let dot = c[0]*m[0]+c[1]*m[1]+c[2]*m[2];
            if dot > 0.0 { continue; }
            let area = 0.5*(c[0]*c[0]+c[1]*c[1]+c[2]*c[2]).sqrt();
            println!("  tri#{ti} idx={t:?} maxedge={e:.3} area={area:.5} dot={dot:.5}");
            for (k,&i) in t.iter().enumerate() { println!("    v{k} {:?} n {:?}", v[k], ns[i as usize]); }
            // 共享顶点的其它三角形数
            let sh = p.indices.chunks_exact(3).filter(|s| s.iter().any(|x| t.contains(x))).count();
            println!("    共享顶点的三角形数 {sh}");
        }
    }
}
