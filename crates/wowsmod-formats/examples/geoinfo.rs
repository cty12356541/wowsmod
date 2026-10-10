use wowsmod_formats::geometry::Geometry;
fn main() {
    for p in std::env::args().skip(1) {
        let g = Geometry::parse(&std::fs::read(&p).unwrap()).unwrap();
        let vb: Vec<String> = g.vertex_buffers.iter().map(|v| format!("{} s{} sk{} bu{} pad{} n{}", v.format(), v.stride, v.skinned, v.bumped, v.name_pad, v.vertex_count())).collect();
        let ib: Vec<String> = g.index_buffers.iter().map(|x| format!("isz{} res{} n{}", x.index_size, x.reserved, x.index_count())).collect();
        println!("{} {:?} {:?} col={:?} arm={:?}", p.rsplit('/').next().unwrap(), vb, ib, g.collision.iter().map(|s| (s.name(), s.body.len(), s.tail, s.name_pad, s.pad2)).collect::<Vec<_>>(), g.armor.iter().map(|s| (s.name(), s.body.len())).collect::<Vec<_>>());
    }
}
