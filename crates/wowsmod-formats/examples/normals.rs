use wowsmod_formats::*;
fn main() {
    for p in std::env::args().skip(1) {
        let g = geometry::Geometry::parse(&std::fs::read(&p).unwrap()).unwrap();
        for vb in &g.vertex_buffers {
            let raw = vb.decode().unwrap(); let st = vb.stride as usize; let n = raw.len()/st;
            let mut s = [0f64; 4]; let mut s2=[0f64;4];
            let mut samples = vec![];
            for i in 0..n { let b: [u8;4] = raw[i*st+12..i*st+16].try_into().unwrap(); let u = u32::from_le_bytes(b);
                let c: [[f32;3];4] = [
                    [(b[0] as i8) as f32/127.0,(b[1] as i8) as f32/127.0,(b[2] as i8) as f32/127.0],
                    [b[0] as f32/127.5-1.0,b[1] as f32/127.5-1.0,b[2] as f32/127.5-1.0],
                    { let x=((u<<21) as i32 >>21) as f32/1023.0; let y=((u<<10) as i32>>21) as f32/1023.0; let z=((u as i32)>>22) as f32/511.0; [x,y,z] },
                    [(b[2] as i8) as f32/127.0,(b[1] as i8) as f32/127.0,(b[0] as i8) as f32/127.0],
                ];
                for k in 0..4 { let l=(c[k][0]*c[k][0]+c[k][1]*c[k][1]+c[k][2]*c[k][2]).sqrt() as f64; s[k]+=l; s2[k]+=(l-1.0).abs(); }
                if i<4 { samples.push(format!("{:02x?}", b)); }
            }
            println!("{} {} n={n} meanlen int8={:.3} u8={:.3} 11-11-10={:.3} | mean|l-1| {:.4} {:.4} {:.4} samples {:?}", p.rsplit('/').next().unwrap(), vb.format(), s[0]/n as f64, s[1]/n as f64, s[2]/n as f64, s2[0]/n as f64, s2[1]/n as f64, s2[2]/n as f64, samples);
        }
    }
}
