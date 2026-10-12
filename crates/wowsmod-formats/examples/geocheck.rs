//! geocheck <geometry> <visual...>: 法线约定/texel density/等价重建 自检
use wowsmod_formats::geomedit::*;
use wowsmod_formats::visual::Visual;
static mut TD: f64 = 0.0; static mut TC: usize = 0; static mut TS: usize = 0;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let d = std::fs::read(&a[0]).unwrap();
    let mut names = vec![];
    for v in &a[1..] {
        let vis = Visual::parse(&std::fs::read_to_string(v).unwrap()).unwrap();
        for rs in vis.render_sets { let n = rs.base_name(); if !names.contains(&n) { names.push(n); } }
    }
    let mut g = GeoEdit::parse(&d).unwrap();
    let (mut dots, mut cnt, mut neg) = (0f64, 0usize, 0usize);
    let mut td_ratio = vec![];
    for n in &names {
        if !g.has(n) { println!("missing {n}"); continue; }
        let m = g.mesh(n).unwrap();
        let mut m2 = m.clone(); m2.compute_normals();
        let (a, b) = (m.normals.as_ref().unwrap(), m2.normals.as_ref().unwrap());
        for i in 0..a.len() { let dd = (a[i][0]*b[i][0]+a[i][1]*b[i][1]+a[i][2]*b[i][2]) as f64; dots += dd; cnt += 1; if dd < 0.0 { neg += 1; } }
        let info = g.info(n).unwrap();
        if let Some(ta) = g.format_of(n).unwrap().find(wowsmod_formats::vertex::Semantic::Tangent).map(|a| a.offset) {
            let tg = m.compute_tangents();
            for i in 0..m.positions.len() { let r = g.raw_vertex(n, i).unwrap(); let t = wowsmod_formats::vertex::unpack_normal(r[ta..ta+4].try_into().unwrap());
                let dd = t[0]*tg[i].0[0]+t[1]*tg[i].0[1]+t[2]*tg[i].0[2]; unsafe { TD += dd as f64; TC += 1; if (r[ta+3] == 254) == (tg[i].1 < 0.0) { TS += 1; } } }
        }
        td_ratio.push(info.texel_density / m.texel_density());
    }
    td_ratio.sort_by(|x, y| x.partial_cmp(y).unwrap());
    println!("{} names={} mean_dot={:.3} neg_frac={:.3} td(file/formula) min={:.3} med={:.3} max={:.3}", a[0].rsplit('/').next().unwrap(), names.len(), dots / cnt as f64, neg as f64 / cnt as f64,
        td_ratio[0], td_ratio[td_ratio.len()/2], td_ratio[td_ratio.len()-1]);
    unsafe { println!("tangent: mean_dot={:.3} sign(w=254 <-> +1) agree={:.3}", TD / TC as f64, TS as f64 / TC as f64); }
    // 等价重建
    let (geo, warn) = rebuild_from_meshes(&g.geo, &names).unwrap();
    let bytes = geo.serialize();
    let mut g2 = GeoEdit::parse(&bytes).unwrap();
    let mut ok = true;
    for n in &names { if !g.has(n) { continue; }
        let (x, y) = (g.mesh(n).unwrap(), g2.mesh(n).unwrap());
        let bits = |m: &MeshData| (m.positions.iter().flat_map(|p| p.map(f32::to_bits)).collect::<Vec<_>>(), m.uvs.as_ref().map(|u| u.iter().flat_map(|p| p.map(f32::to_bits)).collect::<Vec<_>>()));
        let roteq = x.indices.len()==y.indices.len() && x.indices.chunks(3).zip(y.indices.chunks(3)).all(|(a,b)| a==b || (a[0]==b[1]&&a[1]==b[2]&&a[2]==b[0]) || (a[0]==b[2]&&a[1]==b[0]&&a[2]==b[1]));
        if bits(&x) != bits(&y) || !roteq { ok = false; println!("DIFF {n} pos={} idx={} uv={}", x.positions==y.positions, x.indices==y.indices, x.uvs==y.uvs); }
        let (rx, ry) = (g.raw_vertex(n, 0).unwrap(), g2.raw_vertex(n, 0).unwrap());
        if rx != ry { println!("raw0 differs {n}: {:?} vs {:?}", rx, ry); }
    }
    println!("rebuild: {} bytes (orig {}), meshes equal={ok}, warn={warn:?}", bytes.len(), d.len());
    std::fs::write(format!("/tmp/rebuild_{}", a[0].rsplit('/').next().unwrap()), &bytes).unwrap();
}
