use wowsmod_formats::*;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ab = assets_bin::AssetsBin::parse(std::fs::read(&a[1]).unwrap()).unwrap();
    let gp = &a[3];
    let vpath = format!("content/gameplay/france/ship/battleship/FSB003_Richelieu_1944/{gp}");
    let g = geometry::Geometry::parse(&std::fs::read(format!("{}/{gp}", a[2])).unwrap()).unwrap();
    let vis = ab.visuals_for_geometry(&vpath).unwrap();
    let sets: Vec<_> = vis[0].render_sets.iter().enumerate().map(|(k,r)| pairing::RenderSetRef::from_assets(r, format!("rs{k}"))).collect();
    let pr = pairing::pair_by_ids(&g, &sets, pairing::PairSource::AssetsBin);
    let (prims, _, _) = export::build_prims(&g, &pr.pairs).unwrap();
    let all: Vec<[f32;3]> = prims.iter().flat_map(|p| p.positions.iter().copied()).collect();
    let mut mn=[f32::MAX;3]; let mut mx=[f32::MIN;3]; for p in &all { for k in 0..3 { mn[k]=mn[k].min(p[k]); mx[k]=mx[k].max(p[k]); } }
    println!("model bbox {mn:?} {mx:?}");
    for p in &prims {
        let mut pmn=[f32::MAX;3]; let mut pmx=[f32::MIN;3]; for q in &p.positions { for k in 0..3 { pmn[k]=pmn[k].min(q[k]); pmx[k]=pmx[k].max(q[k]); } }
        let diag=((0..3).map(|k| (pmx[k]-pmn[k]).powi(2)).sum::<f32>()).sqrt();
        let mut edges: Vec<f32> = vec![]; let mut sp = vec![];
        for t in p.indices.chunks(3) { let v: Vec<[f32;3]> = t.iter().map(|&i| p.positions[i as usize]).collect();
            let d = |x:&[f32;3],y:&[f32;3]| ((0..3).map(|k| (x[k]-y[k]).powi(2)).sum::<f32>()).sqrt();
            let e = [d(&v[0],&v[1]), d(&v[1],&v[2]), d(&v[2],&v[0])];
            let emax = e.iter().cloned().fold(0./0., f32::max); edges.extend(e);
            // area & min height
            let u=[v[1][0]-v[0][0],v[1][1]-v[0][1],v[1][2]-v[0][2]]; let w=[v[2][0]-v[0][0],v[2][1]-v[0][1],v[2][2]-v[0][2]];
            let c=[u[1]*w[2]-u[2]*w[1],u[2]*w[0]-u[0]*w[2],u[0]*w[1]-u[1]*w[0]]; let area=0.5*(c[0]*c[0]+c[1]*c[1]+c[2]*c[2]).sqrt();
            if emax > 0.5*diag { sp.push((emax, 2.0*area/emax, v[0], v[1], v[2])); } }
        edges.sort_by(|a,b| a.partial_cmp(b).unwrap());
        if !sp.is_empty() { println!("{} {:?} tris={} diag={diag:.3} median_edge={:.4} p99={:.3} spikes={}", p.name, p.material, p.indices.len()/3, edges[edges.len()/2], edges[edges.len()*99/100], sp.len());
            for s in sp.iter().take(4) { println!("    len={:.3} height={:.5} {:?} {:?} {:?}", s.0, s.1, s.2, s.3, s.4); } }
    }
}
