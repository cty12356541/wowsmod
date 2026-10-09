use wowsmod_formats::geometry::Geometry;
fn main() {
    for p in std::env::args().skip(1) {
        let d = std::fs::read(&p).unwrap();
        let g = Geometry::parse(&d).unwrap();
        let a = g.serialize();
        let mut g2 = g.clone();
        let mut rep = vec![];
        for v in g2.vertex_buffers.iter_mut() { let raw = v.decode().unwrap(); let old = v.blob.clone(); v.encode_from(&raw).unwrap(); rep.push(format!("v {} {}", v.format(), old==v.blob)); }
        for x in g2.index_buffers.iter_mut() { let idx = x.decode().unwrap(); let old = x.blob.clone(); x.encode_from(&idx).unwrap();
            let same = old==x.blob; let fd = old.iter().zip(x.blob.iter()).position(|(a,b)| a!=b);
            rep.push(format!("i n={} isz={} same={same} len {} vs {} firstdiff={fd:?} sem={}", idx.len(), x.index_size, old.len(), x.blob.len(), x.decode().unwrap()==idx)); }
        println!("{} A={} B={} {:?}", p.rsplit('/').next().unwrap(), a==d, g2.serialize()==d, rep);
    }
}
