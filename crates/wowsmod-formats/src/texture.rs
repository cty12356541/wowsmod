//! 贴图像素层: BC1/BC2/BC3/BC4/BC5/BC7(+RGBA8/BGRA8)解码为 RGBA8、RGBA8 编码为块压缩格式、mip 生成、PNG 读写。
//!
//! 编解码使用 `block_compression`(MIT, Nils Hasenbanck; BC1–5/BC7 编码器移植自 Intel ISPC Texture Compressor,
//! 解码移植自 bcdec)。只启用其纯 CPU 部分(`default-features = false`), 不依赖 GPU。
use block_compression::decode as bdec;
use block_compression::{encode::compress_rgba8, BC7Settings, CompressionVariant};
use rayon::prelude::*;

use crate::dds::{Dds, Format};
use crate::error::{Error, Result};

/// RGBA8 图像(行优先, 无填充)。
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub px: Vec<u8>,
}

impl Image {
    pub fn new(width: u32, height: u32) -> Image {
        Image { width, height, px: vec![0; width as usize * height as usize * 4] }
    }
    #[inline]
    pub fn idx(&self, x: u32, y: u32) -> usize {
        (y as usize * self.width as usize + x as usize) * 4
    }
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let i = self.idx(x, y);
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }
}

/// 编码质量档。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Quality {
    Fast,
    Normal,
    High,
}

impl Quality {
    pub fn parse(s: &str) -> Option<Quality> {
        Some(match s {
            "fast" => Quality::Fast,
            "normal" => Quality::Normal,
            "high" | "slow" => Quality::High,
            _ => return None,
        })
    }
}

fn block_decoder(f: Format) -> Option<fn(&[u8], &mut [u8], usize)> {
    Some(match f {
        Format::BC1 => bdec::decode_block_bc1,
        Format::BC2 => bdec::decode_block_bc2,
        Format::BC3 => bdec::decode_block_bc3,
        Format::BC4 => bdec::decode_block_bc4,
        Format::BC5 => bdec::decode_block_bc5,
        Format::BC7 => bdec::decode_block_bc7,
        _ => return None,
    })
}

/// 解码一级 mip 的原始字节为 RGBA8。
/// BC4: R 复制到 RGB, A=255; BC5: (R, G, 0, 255)(法线 Z 不重建, 保持数据原样, 便于往返)。
pub fn decode_surface(format: Format, w: u32, h: u32, data: &[u8]) -> Result<Image> {
    let mut img = Image::new(w, h);
    match format {
        Format::Rgba8 | Format::Bgra8 => {
            if data.len() != img.px.len() {
                return Err(Error::format("texture", "RGBA 数据长度不符"));
            }
            img.px.copy_from_slice(data);
            if format == Format::Bgra8 {
                for p in img.px.chunks_exact_mut(4) {
                    p.swap(0, 2);
                }
            }
            return Ok(img);
        }
        _ => {}
    }
    let dec = block_decoder(format).ok_or_else(|| Error::Unsupported(format!("不能解码 {:?}", format)))?;
    let bb = format.block_bytes().unwrap();
    let (bw, bh) = (w.div_ceil(4) as usize, h.div_ceil(4) as usize);
    if data.len() != bw * bh * bb {
        return Err(Error::format("texture", format!("块数据 {} 字节, 应为 {}", data.len(), bw * bh * bb)));
    }
    let row_bytes = w as usize * 4;
    img.px.par_chunks_mut(row_bytes * 4).enumerate().for_each(|(by, rows)| {
        let mut tmp = [0u8; 64];
        let nrows = rows.len() / row_bytes;
        for bx in 0..bw {
            tmp.fill(0);
            if matches!(format, Format::BC4 | Format::BC5) {
                // BC4/5 的解码器写 1/2 通道紧密排列, 用单独缓冲
                let ch = if format == Format::BC4 { 1 } else { 2 };
                let mut t = [0u8; 32];
                dec(&data[(by * bw + bx) * bb..][..bb], &mut t, 4 * ch);
                for i in 0..16 {
                    let r = t[i * ch];
                    let g = if ch == 2 { t[i * ch + 1] } else { r };
                    let b = if ch == 2 { 0 } else { r };
                    tmp[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, 255]);
                }
            } else {
                dec(&data[(by * bw + bx) * bb..][..bb], &mut tmp, 16);
            }
            for y in 0..nrows.min(4) {
                for x in 0..4 {
                    let px = bx * 4 + x;
                    if px >= w as usize {
                        break;
                    }
                    rows[y * row_bytes + px * 4..][..4].copy_from_slice(&tmp[(y * 4 + x) * 4..][..4]);
                }
            }
        }
    });
    Ok(img)
}

/// 解码 DDS 的第 level 级。
pub fn decode_level(d: &Dds, level: u32) -> Result<Image> {
    let (w, h) = d.header.level_dims(level);
    decode_surface(d.header.format, w, h, d.level(level)?)
}

/// 图像是否有非 255 的 alpha。
pub fn has_alpha(img: &Image) -> bool {
    img.px.chunks_exact(4).any(|p| p[3] != 255)
}

fn bc7_settings(q: Quality, alpha: bool) -> BC7Settings {
    match (q, alpha) {
        (Quality::Fast, false) => BC7Settings::opaque_fast(),
        (Quality::Normal, false) => BC7Settings::opaque_basic(),
        (Quality::High, false) => BC7Settings::opaque_slow(),
        (Quality::Fast, true) => BC7Settings::alpha_fast(),
        (Quality::Normal, true) => BC7Settings::alpha_basic(),
        (Quality::High, true) => BC7Settings::alpha_slow(),
    }
}

/// 把图像补到 4 的倍数(边缘像素复制), 块压缩编码要求。
fn pad4(img: &Image) -> Image {
    let (w, h) = (img.width.div_ceil(4) * 4, img.height.div_ceil(4) * 4);
    if (w, h) == (img.width, img.height) {
        return img.clone();
    }
    let mut o = Image::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let s = img.idx(x.min(img.width - 1), y.min(img.height - 1));
            let d = o.idx(x, y);
            o.px[d..d + 4].copy_from_slice(&img.px[s..s + 4]);
        }
    }
    o
}

/// 编码一个面。BC7 是否按带 alpha 的设置编码由 `alpha` 决定(None = 自动检测)。
pub fn encode_surface(img: &Image, format: Format, q: Quality, alpha: Option<bool>) -> Result<Vec<u8>> {
    match format {
        Format::Rgba8 => return Ok(img.px.clone()),
        Format::Bgra8 => {
            let mut v = img.px.clone();
            for p in v.chunks_exact_mut(4) {
                p.swap(0, 2);
            }
            return Ok(v);
        }
        _ => {}
    }
    let var = match format {
        Format::BC1 => CompressionVariant::BC1,
        Format::BC2 => CompressionVariant::BC2,
        Format::BC3 => CompressionVariant::BC3,
        Format::BC4 => CompressionVariant::BC4,
        Format::BC5 => CompressionVariant::BC5,
        Format::BC7 => CompressionVariant::BC7(bc7_settings(q, alpha.unwrap_or_else(|| has_alpha(img)))),
        _ => return Err(Error::Unsupported(format!("不能编码为 {:?}", format))),
    };
    let p = pad4(img);
    let bb = format.block_bytes().unwrap();
    let bw = (p.width / 4) as usize;
    let row_px = p.width as usize * 4;
    let mut out = vec![0u8; bw * (p.height as usize / 4) * bb];
    // 按块行并行(各块独立编码)
    out.par_chunks_mut(bw * bb).zip(p.px.par_chunks(row_px * 4)).for_each(|(dst, src)| {
        compress_rgba8(var, src, dst, p.width, 4, row_px as u32);
        if format == Format::BC1 {
            // 含透明像素(alpha < 128)的块改用 BC1 三色 + 透明模式, 保留游戏原图里的"透明黑"像素
            for bx in 0..bw {
                let mut blk = [[0u8; 4]; 16];
                let mut any = false;
                for y in 0..4 {
                    for x in 0..4 {
                        let o = y * row_px + (bx * 4 + x) * 4;
                        blk[y * 4 + x].copy_from_slice(&src[o..o + 4]);
                        any |= src[o + 3] < 128;
                    }
                }
                if any {
                    dst[bx * 8..bx * 8 + 8].copy_from_slice(&bc1_punchthrough_block(&blk));
                }
            }
        }
    });
    Ok(out)
}

fn to565(c: [f32; 3]) -> u16 {
    let r = (c[0] * 31.0 / 255.0 + 0.5).clamp(0.0, 31.0) as u16;
    let g = (c[1] * 63.0 / 255.0 + 0.5).clamp(0.0, 63.0) as u16;
    let b = (c[2] * 31.0 / 255.0 + 0.5).clamp(0.0, 31.0) as u16;
    (r << 11) | (g << 5) | b
}
fn from565(v: u16) -> [i32; 3] {
    let (r, g, b) = ((v >> 11) & 31, (v >> 5) & 63, v & 31);
    [((r << 3) | (r >> 2)) as i32, ((g << 2) | (g >> 4)) as i32, ((b << 3) | (b >> 2)) as i32]
}

/// BC1 三色+透明模式编码一个块(color0 <= color1): 索引 0/1 = 端点, 2 = 中点, 3 = 透明黑。
/// 端点取不透明像素中距离最远的一对, 再在 565 量化后的邻域里做小范围细化。
pub fn bc1_punchthrough_block(px: &[[u8; 4]; 16]) -> [u8; 8] {
    let op: Vec<[f32; 3]> = px.iter().filter(|p| p[3] >= 128).map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
    let mut best = (u64::MAX, 0u16, 0u16);
    if !op.is_empty() {
        let mut cand = vec![];
        let (mut fa, mut fb, mut fd) = (0, 0, -1f32);
        for i in 0..op.len() {
            for j in i..op.len() {
                let d: f32 = (0..3).map(|c| (op[i][c] - op[j][c]).powi(2)).sum();
                if d > fd {
                    (fa, fb, fd) = (i, j, d);
                }
            }
        }
        let (a, b) = (to565(op[fa]), to565(op[fb]));
        cand.push((a, b));
        // 小范围细化: 端点各分量 ±1 量化级
        for &(da, db) in &[(1i32, 0i32), (0, 1), (-1, 0), (0, -1)] {
            for sh in [11u32, 5, 0] {
                let m = if sh == 5 { 63 } else { 31 };
                let adj = |v: u16, d: i32| -> u16 {
                    let f = ((v >> sh) & m) as i32 + d;
                    if !(0..=m as i32).contains(&f) { v } else { (v & !(m << sh)) | ((f as u16) << sh) }
                };
                cand.push((adj(a, da), adj(b, db)));
            }
        }
        for (a, b) in cand {
            let (c0, c1) = if a <= b { (a, b) } else { (b, a) };
            let (e0, e1) = (from565(c0), from565(c1));
            let mid = [(e0[0] + e1[0]) / 2, (e0[1] + e1[1]) / 2, (e0[2] + e1[2]) / 2];
            let err: u64 = op
                .iter()
                .map(|p| {
                    [e0, e1, mid].iter().map(|e| (0..3).map(|c| ((p[c] as i32 - e[c]).pow(2)) as u64).sum::<u64>()).min().unwrap()
                })
                .sum();
            if err < best.0 {
                best = (err, c0, c1);
            }
        }
    }
    let (_, c0, c1) = best;
    let (e0, e1) = (from565(c0), from565(c1));
    let mid = [(e0[0] + e1[0]) / 2, (e0[1] + e1[1]) / 2, (e0[2] + e1[2]) / 2];
    let mut idx = 0u32;
    for (i, p) in px.iter().enumerate() {
        let k = if p[3] < 128 {
            3
        } else {
            let pal = [e0, e1, mid];
            (0..3).min_by_key(|&k| (0..3).map(|c| (p[c] as i32 - pal[k][c]).pow(2)).sum::<i32>()).unwrap() as u32
        };
        idx |= k << (2 * i);
    }
    let mut o = [0u8; 8];
    o[0..2].copy_from_slice(&c0.to_le_bytes());
    o[2..4].copy_from_slice(&c1.to_le_bytes());
    o[4..8].copy_from_slice(&idx.to_le_bytes());
    o
}

#[inline]
fn srgb_to_lin(v: u8) -> f32 {
    let c = v as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
#[inline]
fn lin_to_srgb(l: f32) -> u8 {
    let c = if l <= 0.0031308 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
    (c * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

/// mip 下采样方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum MipFilter {
    /// 2x2 盒式平均, RGB 在线性空间平均(颜色贴图)
    BoxSrgb,
    /// 2x2 盒式平均, 原值平均(法线/遮罩/数据贴图)
    BoxLinear,
}

/// 生成下一级 mip(尺寸减半, 最小 1)。
pub fn downsample(img: &Image, f: MipFilter) -> Image {
    let (w, h) = ((img.width / 2).max(1), (img.height / 2).max(1));
    let mut o = Image::new(w, h);
    let lut: Vec<f32> = (0..256).map(|v| srgb_to_lin(v as u8)).collect();
    o.px.par_chunks_mut(w as usize * 4).enumerate().for_each(|(y, row)| {
        let y = y as u32;
        for x in 0..w {
            let xs = [(2 * x).min(img.width - 1), (2 * x + 1).min(img.width - 1)];
            let ys = [(2 * y).min(img.height - 1), (2 * y + 1).min(img.height - 1)];
            let mut acc = [0f32; 4];
            for &yy in &ys {
                for &xx in &xs {
                    let p = img.get(xx, yy);
                    for c in 0..4 {
                        acc[c] += if f == MipFilter::BoxSrgb && c < 3 { lut[p[c] as usize] } else { p[c] as f32 };
                    }
                }
            }
            for c in 0..4 {
                let v = acc[c] / 4.0;
                row[x as usize * 4 + c] = if f == MipFilter::BoxSrgb && c < 3 { lin_to_srgb(v) } else { (v + 0.5).clamp(0.0, 255.0) as u8 };
            }
        }
    });
    o
}

/// 从顶级生成完整 mip 链(含顶级), 共 `levels` 级(0 = 一直到 1x1)。
pub fn mip_chain(top: Image, levels: u32, f: MipFilter) -> Vec<Image> {
    let mut v = vec![top];
    loop {
        let l = v.last().unwrap();
        if (levels > 0 && v.len() as u32 >= levels) || (levels == 0 && l.width == 1 && l.height == 1) {
            break;
        }
        let n = downsample(l, f);
        v.push(n);
    }
    v
}

/// 由各级图像编码出一份 DDS。`template` 给出时沿用其头部(尺寸与级数必须一致), 否则新建头部。
pub fn encode_dds(levels: &[Image], format: Format, q: Quality, template: Option<&Dds>) -> Result<Dds> {
    if levels.is_empty() {
        return Err(Error::format("texture", "没有图像"));
    }
    let alpha = levels.iter().any(has_alpha);
    let enc: Vec<Vec<u8>> = levels.iter().map(|l| encode_surface(l, format, q, Some(alpha))).collect::<Result<_>>()?;
    if let Some(t) = template {
        if t.header.width != levels[0].width || t.header.height != levels[0].height || t.header.levels() != levels.len() as u32 || t.header.format != format || t.header.layers() != 1 {
            return Err(Error::format("texture", "模板头与新图像的尺寸/级数/格式不一致"));
        }
        let mut raw = t.header.raw.clone();
        // 新数据一定是标准块, 清掉 BC7Prep 标志
        raw[40..44].copy_from_slice(&0u32.to_le_bytes());
        let header = crate::dds::Header::parse(&raw)?;
        return Ok(Dds { header, data: enc.concat() });
    }
    Dds::new_2d(format, levels[0].width, levels[0].height, enc, false)
}

/// PSNR(dB), 只比较 RGB 或 RGBA。
pub fn psnr(a: &Image, b: &Image, with_alpha: bool) -> f64 {
    let ch = if with_alpha { 4 } else { 3 };
    let mut se = 0f64;
    let mut n = 0f64;
    for (p, q) in a.px.chunks_exact(4).zip(b.px.chunks_exact(4)) {
        for c in 0..ch {
            let d = p[c] as f64 - q[c] as f64;
            se += d * d;
            n += 1.0;
        }
    }
    if se == 0.0 {
        return f64::INFINITY;
    }
    10.0 * (255f64 * 255.0 / (se / n)).log10()
}

/// 读 PNG 为 RGBA8(支持灰度/灰度+alpha/RGB/RGBA, 8 或 16 位)。
pub fn read_png(bytes: &[u8]) -> Result<Image> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().map_err(|e| Error::format("png", e.to_string()))?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).map_err(|e| Error::format("png", e.to_string()))?;
    let (w, h) = (info.width, info.height);
    let mut img = Image::new(w, h);
    let src = &buf[..info.buffer_size()];
    match info.color_type {
        png::ColorType::Rgba => img.px.copy_from_slice(src),
        png::ColorType::Rgb => {
            for (d, s) in img.px.chunks_exact_mut(4).zip(src.chunks_exact(3)) {
                d.copy_from_slice(&[s[0], s[1], s[2], 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for (d, s) in img.px.chunks_exact_mut(4).zip(src.chunks_exact(2)) {
                d.copy_from_slice(&[s[0], s[0], s[0], s[1]]);
            }
        }
        png::ColorType::Grayscale => {
            for (d, s) in img.px.iter_mut().zip(src.iter().flat_map(|&g| [g, g, g, 255])) {
                *d = s;
            }
        }
        png::ColorType::Indexed => return Err(Error::Unsupported("索引色 PNG(EXPAND 后不应出现)".into())),
    }
    Ok(img)
}

/// 写 RGBA8 PNG(`keep_alpha`=false 时写 RGB)。
pub fn write_png(img: &Image, keep_alpha: bool) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.width, img.height);
        enc.set_color(if keep_alpha { png::ColorType::Rgba } else { png::ColorType::Rgb });
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut w = enc.write_header().map_err(|e| Error::format("png", e.to_string()))?;
        if keep_alpha {
            w.write_image_data(&img.px).map_err(|e| Error::format("png", e.to_string()))?;
        } else {
            let rgb: Vec<u8> = img.px.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
            w.write_image_data(&rgb).map_err(|e| Error::format("png", e.to_string()))?;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> Image {
        let mut img = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let i = img.idx(x, y);
                img.px[i..i + 4].copy_from_slice(&[(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, ((x + y) * 3 % 256) as u8, 255]);
            }
        }
        img
    }

    #[test]
    fn encode_decode_quality() {
        let img = gradient(64, 64);
        for (f, min) in [(Format::BC1, 30.0), (Format::BC3, 30.0), (Format::BC7, 38.0)] {
            let enc = encode_surface(&img, f, Quality::Normal, None).unwrap();
            let dec = decode_surface(f, 64, 64, &enc).unwrap();
            let p = psnr(&img, &dec, false);
            assert!(p > min, "{f:?} psnr {p}");
        }
        // BC5: 只比较 RG
        let enc = encode_surface(&img, Format::BC5, Quality::Normal, None).unwrap();
        let dec = decode_surface(Format::BC5, 64, 64, &enc).unwrap();
        let mut maxd = 0;
        for (p, q) in img.px.chunks_exact(4).zip(dec.px.chunks_exact(4)) {
            maxd = maxd.max((p[0] as i32 - q[0] as i32).abs()).max((p[1] as i32 - q[1] as i32).abs());
        }
        assert!(maxd <= 8, "bc5 maxd {maxd}");
    }

    #[test]
    fn bc1_punchthrough() {
        let mut img = gradient(8, 8);
        for i in [0usize, 5, 17, 40] {
            img.px[i * 4..i * 4 + 4].copy_from_slice(&[0, 0, 0, 0]);
        }
        let enc = encode_surface(&img, Format::BC1, Quality::Normal, None).unwrap();
        let dec = decode_surface(Format::BC1, 8, 8, &enc).unwrap();
        for i in 0..64 {
            assert_eq!(dec.px[i * 4 + 3] == 0, img.px[i * 4 + 3] == 0, "pixel {i}");
        }
        let p = psnr(&img, &dec, false);
        assert!(p > 20.0, "psnr {p}");
    }

    #[test]
    fn small_mips_and_chain() {
        let img = gradient(16, 8);
        let chain = mip_chain(img, 0, MipFilter::BoxSrgb);
        assert_eq!(chain.len(), 5);
        assert_eq!((chain[4].width, chain[4].height), (1, 1));
        let d = encode_dds(&chain, Format::BC7, Quality::Fast, None).unwrap();
        let bytes = d.to_bytes();
        let back = Dds::parse(&bytes).unwrap();
        assert_eq!(back.header.levels(), 5);
        let l4 = decode_level(&back, 4).unwrap();
        assert_eq!((l4.width, l4.height), (1, 1));
        let l3 = decode_level(&back, 3).unwrap();
        assert_eq!((l3.width, l3.height), (2, 1));
    }

    #[test]
    fn png_roundtrip() {
        let img = gradient(9, 7);
        let p = write_png(&img, true).unwrap();
        assert_eq!(read_png(&p).unwrap(), img);
    }
}
