//! 贴图编解码自检: tex_probe <dds> <out.rgba> [reencode: fast|normal|high]
//! 解码第 0 级写成原始 RGBA8; 若给出质量档, 用同格式重新编码再解码, 打印 PSNR 与耗时。
use std::time::Instant;
use wowsmod_formats::{dds::Dds, texture};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let d = Dds::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let t = Instant::now();
    let img = texture::decode_level(&d, 0).unwrap();
    println!("{} {}x{} levels={} decode {:?}", d.header.format.name(), img.width, img.height, d.header.levels(), t.elapsed());
    std::fs::write(&a[2], &img.px).unwrap();
    if let Some(q) = a.get(3) {
        let q = texture::Quality::parse(q).unwrap();
        let t = Instant::now();
        let enc = texture::encode_surface(&img, d.header.format, q, None).unwrap();
        let el = t.elapsed();
        let back = texture::decode_surface(d.header.format, img.width, img.height, &enc).unwrap();
        println!("reencode {:?} {:?}: psnr rgb {:.2} dB, rgba {:.2} dB", q, el, texture::psnr(&img, &back, false), texture::psnr(&img, &back, true));
    }
}
