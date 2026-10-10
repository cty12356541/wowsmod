//! 外部网格来源(model import / model pmx2glb 用):
//! - glTF 材质读取: baseColor 贴图×系数、alphaMode/alphaCutoff、doubleSided、emissive 贴图×系数;
//!   贴图按"图片内容"去重(多个材质共用一张图集时只存一份)。
//! - 图片解码: PNG / JPEG(zune-jpeg, 纯 Rust) / BMP / TGA / DDS。
//! - PMX(MMD 模型, 2.0/2.1)→ GLB: 纯 Rust 读取顶点/面/材质/贴图, 按材质拆 primitive,
//!   左手系 → glTF 右手系(z 取反 + 绕序翻转), MMD 单位 × unit_m(默认 0.08 m)。
//!   骨骼/表情/物理/描边/toon/sphere 不转换(静态网格够 model import 用)。
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use wowsmod_formats::{
    dds::Dds,
    texture::{self, Image},
};

// ───────────── 图片解码 ─────────────

pub fn decode_image(b: &[u8]) -> Result<Image> {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        return texture::read_png(b).map_err(|e| anyhow!("PNG: {e}"));
    }
    if b.starts_with(&[0xFF, 0xD8]) {
        return decode_jpeg(b);
    }
    if b.starts_with(b"BM") {
        return decode_bmp(b);
    }
    if b.starts_with(b"DDS ") {
        let d = Dds::parse(b).map_err(|e| anyhow!("DDS: {e}"))?;
        return texture::decode_level(&d, 0).map_err(|e| anyhow!("DDS: {e}"));
    }
    // TGA 没有魔数: 按头部字段判断
    if b.len() > 18 && matches!(b[2], 2 | 3 | 10 | 11) {
        return decode_tga(b);
    }
    bail!("不支持的图片格式(支持 PNG/JPEG/BMP/TGA/DDS)")
}

fn decode_jpeg(b: &[u8]) -> Result<Image> {
    use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};
    let opt = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(b, opt);
    let px = dec.decode().map_err(|e| anyhow!("JPEG: {e:?}"))?;
    let info = dec.info().ok_or_else(|| anyhow!("JPEG: 没有尺寸"))?;
    let (w, h) = (info.width as u32, info.height as u32);
    if px.len() != (w * h * 4) as usize {
        bail!("JPEG: 输出大小不符 {}x{} / {}", w, h, px.len());
    }
    Ok(Image { width: w, height: h, px })
}

fn u16le(b: &[u8], o: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(b.get(o..o + 2).ok_or_else(|| anyhow!("文件截断"))?.try_into().unwrap()))
}
fn u32le(b: &[u8], o: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(b.get(o..o + 4).ok_or_else(|| anyhow!("文件截断"))?.try_into().unwrap()))
}

/// BMP: 无压缩 8(调色板)/24/32 位, 自下而上或自上而下
fn decode_bmp(b: &[u8]) -> Result<Image> {
    let off = u32le(b, 10)? as usize;
    let hs = u32le(b, 14)? as usize;
    let w = u32le(b, 18)? as i32;
    let h = u32le(b, 22)? as i32;
    let bpp = u16le(b, 28)?;
    let comp = u32le(b, 30)?;
    if comp != 0 && !(comp == 3 && bpp == 32) {
        bail!("BMP: 只支持无压缩(compression={comp})");
    }
    let (w, top_down, h) = (w.unsigned_abs(), h < 0, h.unsigned_abs());
    let pal_off = 14 + hs;
    let row = ((bpp as usize * w as usize + 31) / 32) * 4;
    let mut img = Image::new(w, h);
    for y in 0..h {
        let sy = if top_down { y } else { h - 1 - y } as usize;
        let r = b.get(off + sy * row..off + sy * row + row).ok_or_else(|| anyhow!("BMP: 截断"))?;
        for x in 0..w as usize {
            let c = match bpp {
                8 => {
                    let i = r[x] as usize;
                    let p = b.get(pal_off + i * 4..pal_off + i * 4 + 4).ok_or_else(|| anyhow!("BMP: 调色板截断"))?;
                    [p[2], p[1], p[0], 255]
                }
                24 => [r[x * 3 + 2], r[x * 3 + 1], r[x * 3], 255],
                32 => [r[x * 4 + 2], r[x * 4 + 1], r[x * 4], r[x * 4 + 3]],
                _ => bail!("BMP: 不支持 {bpp} 位"),
            };
            let o = (y as usize * w as usize + x) * 4;
            img.px[o..o + 4].copy_from_slice(&c);
        }
    }
    // 32 位但 alpha 全 0 的 BMP 实际不透明
    if bpp == 32 && img.px.chunks_exact(4).all(|p| p[3] == 0) {
        img.px.chunks_exact_mut(4).for_each(|p| p[3] = 255);
    }
    Ok(img)
}

/// TGA: 类型 2/10(真彩, RLE)与 3/11(灰度), 24/32/8 位
fn decode_tga(b: &[u8]) -> Result<Image> {
    let idlen = b[0] as usize;
    let ty = b[2];
    if b[1] != 0 {
        bail!("TGA: 不支持调色板");
    }
    let w = u16le(b, 12)? as u32;
    let h = u16le(b, 14)? as u32;
    let bpp = b[16] as usize;
    let top = b[17] & 0x20 != 0;
    let bpx = bpp / 8;
    if !matches!(bpx, 1 | 3 | 4) {
        bail!("TGA: 不支持 {bpp} 位");
    }
    let mut raw = Vec::with_capacity((w * h) as usize * bpx);
    let mut p = 18 + idlen;
    let n = (w * h) as usize * bpx;
    if ty == 10 || ty == 11 {
        while raw.len() < n {
            let hdr = *b.get(p).ok_or_else(|| anyhow!("TGA: 截断"))?;
            p += 1;
            let cnt = (hdr & 0x7f) as usize + 1;
            if hdr & 0x80 != 0 {
                let px = b.get(p..p + bpx).ok_or_else(|| anyhow!("TGA: 截断"))?;
                for _ in 0..cnt {
                    raw.extend_from_slice(px);
                }
                p += bpx;
            } else {
                raw.extend_from_slice(b.get(p..p + cnt * bpx).ok_or_else(|| anyhow!("TGA: 截断"))?);
                p += cnt * bpx;
            }
        }
        raw.truncate(n);
    } else {
        raw.extend_from_slice(b.get(p..p + n).ok_or_else(|| anyhow!("TGA: 截断"))?);
    }
    let mut img = Image::new(w, h);
    for y in 0..h as usize {
        let sy = if top { y } else { h as usize - 1 - y };
        for x in 0..w as usize {
            let s = &raw[(sy * w as usize + x) * bpx..][..bpx];
            let c = match bpx {
                1 => [s[0], s[0], s[0], 255],
                3 => [s[2], s[1], s[0], 255],
                _ => [s[2], s[1], s[0], s[3]],
            };
            let o = (y * w as usize + x) * 4;
            img.px[o..o + 4].copy_from_slice(&c);
        }
    }
    Ok(img)
}

/// 图片内容哈希(去重用)
pub fn image_hash(img: &Image) -> String {
    let mut h = Sha256::new();
    h.update(img.width.to_le_bytes());
    h.update(img.height.to_le_bytes());
    h.update(&img.px);
    h.finalize().iter().take(8).map(|b| format!("{b:02x}")).collect()
}

// ───────────── glTF 材质 ─────────────

#[derive(Clone, Debug)]
pub struct GMat {
    pub name: String,
    /// images[] 下标
    pub base_image: Option<usize>,
    pub base_factor: [f32; 4],
    /// OPAQUE / MASK / BLEND
    pub alpha_mode: String,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    pub emissive_image: Option<usize>,
    pub emissive_factor: [f32; 3],
    pub notes: Vec<String>,
}

pub struct GltfMats {
    pub mats: Vec<GMat>,
    /// 原始图片字节(解码失败/外链缺失为 None)
    pub images: Vec<Option<Vec<u8>>>,
    pub image_names: Vec<String>,
    pub notes: Vec<String>,
}

fn b64(s: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => bail!("base64 非法字符"),
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

/// 读 GLB 的材质与图片。`base_dir`: 外链 uri 图片的相对目录
pub fn read_glb_materials(d: &[u8], base_dir: Option<&Path>) -> Result<GltfMats> {
    if d.len() < 20 || &d[0..4] != b"glTF" {
        bail!("不是 GLB");
    }
    let jl = u32le(d, 12)? as usize;
    let j: Value = serde_json::from_slice(d.get(20..20 + jl).ok_or_else(|| anyhow!("GLB JSON 截断"))?)?;
    let bo = 20 + jl;
    let mut bin: &[u8] = &[];
    if bo + 8 <= d.len() && &d[bo + 4..bo + 8] == b"BIN\0" {
        let bl = u32le(d, bo)? as usize;
        bin = d.get(bo + 8..bo + 8 + bl).unwrap_or(&[]);
    }
    let mut notes = Vec::new();
    let mut images = Vec::new();
    let mut image_names = Vec::new();
    for (i, im) in j["images"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        image_names.push(im["name"].as_str().or(im["uri"].as_str().filter(|u| !u.starts_with("data:"))).map(|s| s.to_string()).unwrap_or(format!("image{i}")));
        let bytes = if let Some(bv) = im["bufferView"].as_u64() {
            let v = &j["bufferViews"][bv as usize];
            let (o, l) = (v["byteOffset"].as_u64().unwrap_or(0) as usize, v["byteLength"].as_u64().unwrap_or(0) as usize);
            bin.get(o..o + l).map(|s| s.to_vec())
        } else if let Some(u) = im["uri"].as_str() {
            if let Some(p) = u.strip_prefix("data:") {
                p.split_once(',').and_then(|(_, data)| b64(data).ok())
            } else {
                base_dir.and_then(|bd| std::fs::read(bd.join(u.replace("%20", " "))).ok())
            }
        } else {
            None
        };
        if bytes.is_none() {
            notes.push(format!("图片 {i} 读不到(外链缺失?)"));
        }
        images.push(bytes);
    }
    let tex_img = |ti: &Value, what: &str, n: &mut Vec<String>| -> Option<usize> {
        let idx = ti["index"].as_u64()? as usize;
        if ti["texCoord"].as_u64().unwrap_or(0) != 0 {
            n.push(format!("{what} 用 TEXCOORD_{}, 只支持 TEXCOORD_0", ti["texCoord"]));
        }
        if ti["extensions"].get("KHR_texture_transform").is_some() {
            n.push(format!("{what} 有 KHR_texture_transform, 未应用"));
        }
        let t = &j["textures"][idx];
        let s = t["source"].as_u64();
        if s.is_none() {
            n.push(format!("{what} 的贴图用扩展格式({}), 不支持", t["extensions"].as_object().map(|o| o.keys().cloned().collect::<Vec<_>>().join(",")).unwrap_or_default()));
        }
        s.map(|x| x as usize)
    };
    let f4 = |v: &Value, d: [f32; 4]| -> [f32; 4] { v.as_array().map(|a| std::array::from_fn(|i| a.get(i).and_then(|x| x.as_f64()).unwrap_or(d[i] as f64) as f32)).unwrap_or(d) };
    let mut mats = Vec::new();
    for (i, m) in j["materials"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        let mut n = Vec::new();
        let pbr = &m["pbrMetallicRoughness"];
        let base_image = tex_img(&pbr["baseColorTexture"], "baseColorTexture", &mut n);
        let emissive_image = tex_img(&m["emissiveTexture"], "emissiveTexture", &mut n);
        let ef = f4(&m["emissiveFactor"], [0.0; 4]);
        let mut strength = 1.0;
        if let Some(s) = m["extensions"]["KHR_materials_emissive_strength"]["emissiveStrength"].as_f64() {
            strength = s as f32;
        }
        mats.push(GMat {
            name: m["name"].as_str().map(|s| s.to_string()).unwrap_or(format!("mat{i}")),
            base_image,
            base_factor: f4(&pbr["baseColorFactor"], [1.0; 4]),
            alpha_mode: m["alphaMode"].as_str().unwrap_or("OPAQUE").to_string(),
            alpha_cutoff: m["alphaCutoff"].as_f64().unwrap_or(0.5) as f32,
            double_sided: m["doubleSided"].as_bool().unwrap_or(false),
            emissive_image,
            emissive_factor: [ef[0] * strength, ef[1] * strength, ef[2] * strength],
            notes: n,
        });
    }
    Ok(GltfMats { mats, images, image_names, notes })
}

// ───────────── PMX ─────────────

#[derive(Clone, Debug)]
pub struct PmxMat {
    pub name: String,
    pub diffuse: [f32; 4],
    /// 绘制标志: bit0 = 双面
    pub flags: u8,
    pub texture: Option<usize>,
    pub index_count: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Pmx {
    pub version: f32,
    pub name: String,
    pub comment: String,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub textures: Vec<String>,
    pub materials: Vec<PmxMat>,
    /// 每顶点最多 4 个 (骨骼, 权重); SDEF 按 BDEF2 处理
    pub weights: Vec<[(i32, f32); 4]>,
    pub bones: Vec<PmxBone>,
}

#[derive(Clone, Debug, Default)]
pub struct PmxBone {
    pub name: String,
    pub pos: [f32; 3],
    pub parent: i32,
}

struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self.b.get(self.p..self.p + n).ok_or_else(|| anyhow!("PMX 截断(偏移 {})", self.p))?;
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn fv<const N: usize>(&mut self) -> Result<[f32; N]> {
        let mut o = [0.0; N];
        for x in o.iter_mut() {
            *x = self.f32()?;
        }
        Ok(o)
    }
    fn text(&mut self, utf8: bool) -> Result<String> {
        let n = self.i32()?;
        if n < 0 {
            bail!("PMX 字符串长度为负");
        }
        let s = self.take(n as usize)?;
        Ok(if utf8 {
            String::from_utf8_lossy(s).into_owned()
        } else {
            String::from_utf16_lossy(&s.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
        })
    }
    /// 有符号索引(贴图/骨骼/材质): -1 = 无
    fn sidx(&mut self, size: u8) -> Result<i64> {
        Ok(match size {
            1 => self.u8()? as i8 as i64,
            2 => i16::from_le_bytes(self.take(2)?.try_into().unwrap()) as i64,
            4 => self.i32()? as i64,
            _ => bail!("PMX 索引大小 {size}"),
        })
    }
    /// 顶点索引: 1/2 字节无符号, 4 字节有符号
    fn vidx(&mut self, size: u8) -> Result<u32> {
        Ok(match size {
            1 => self.u8()? as u32,
            2 => u16::from_le_bytes(self.take(2)?.try_into().unwrap()) as u32,
            4 => self.i32()? as u32,
            _ => bail!("PMX 顶点索引大小 {size}"),
        })
    }
}

pub fn read_pmx(b: &[u8]) -> Result<Pmx> {
    let mut r = Rd { b, p: 0 };
    if r.take(4)? != b"PMX " {
        bail!("不是 PMX 文件(缺少 'PMX ' 魔数; PMD 旧格式请先用 PMXEditor 另存为 PMX)");
    }
    let version = r.f32()?;
    let gn = r.u8()? as usize;
    let g = r.take(gn)?.to_vec();
    if g.len() < 8 {
        bail!("PMX 全局信息不足 8 字节");
    }
    let (utf8, add_uv, vsz, tsz, msz, bsz) = (g[0] == 1, g[1] as usize, g[2], g[3], g[4], g[5]);
    let _ = msz;
    let name = r.text(utf8)?;
    let _name_en = r.text(utf8)?;
    let comment = r.text(utf8)?;
    let _comment_en = r.text(utf8)?;
    let nv = r.i32()? as usize;
    let mut p = Pmx { version, name, comment, ..Default::default() };
    p.positions.reserve(nv);
    for _ in 0..nv {
        p.positions.push(r.fv::<3>()?);
        p.normals.push(r.fv::<3>()?);
        p.uvs.push(r.fv::<2>()?);
        r.take(16 * add_uv)?;
        let wt = r.u8()?;
        let mut w = [(-1i32, 0.0f32); 4];
        match wt {
            0 => w[0] = (r.sidx(bsz)? as i32, 1.0),
            1 | 3 => {
                let (a, b) = (r.sidx(bsz)? as i32, r.sidx(bsz)? as i32);
                let x = r.f32()?;
                w[0] = (a, x);
                w[1] = (b, 1.0 - x);
                if wt == 3 {
                    r.take(36)?; // SDEF C/R0/R1: 近似为 BDEF2
                }
            }
            2 | 4 => {
                let ids = [r.sidx(bsz)?, r.sidx(bsz)?, r.sidx(bsz)?, r.sidx(bsz)?];
                for (k, id) in ids.iter().enumerate() {
                    w[k] = (*id as i32, r.f32()?);
                }
            }
            _ => bail!("PMX 顶点权重类型 {wt}"),
        };
        p.weights.push(w);
        r.f32()?; // 描边倍率
    }
    let ni = r.i32()? as usize;
    p.indices.reserve(ni);
    for _ in 0..ni {
        let i = r.vidx(vsz)?;
        if i as usize >= nv {
            bail!("PMX 面索引越界 {i}");
        }
        p.indices.push(i);
    }
    let nt = r.i32()? as usize;
    for _ in 0..nt {
        p.textures.push(r.text(utf8)?);
    }
    let nm = r.i32()? as usize;
    let mut used = 0usize;
    for _ in 0..nm {
        let name = r.text(utf8)?;
        let _en = r.text(utf8)?;
        let diffuse = r.fv::<4>()?;
        r.take(12 + 4 + 12)?; // specular, power, ambient
        let flags = r.u8()?;
        r.take(16 + 4)?; // edge color, size
        let tex = r.sidx(tsz)?;
        let _sphere = r.sidx(tsz)?;
        r.u8()?; // sphere mode
        let toon_shared = r.u8()?;
        if toon_shared == 0 {
            r.sidx(tsz)?;
        } else {
            r.u8()?;
        }
        let _memo = r.text(utf8)?;
        let cnt = r.i32()? as usize;
        if used + cnt > p.indices.len() || cnt % 3 != 0 {
            bail!("PMX 材质 {name} 的面数 {cnt} 越界");
        }
        used += cnt;
        p.materials.push(PmxMat { name, diffuse, flags, texture: (tex >= 0 && (tex as usize) < nt).then_some(tex as usize), index_count: cnt });
    }
    // 骨骼(摆姿势用); 读失败不影响静态网格
    if let Ok(nb) = r.i32() {
        let mut bones = Vec::new();
        let ok = (|| -> Result<()> {
            for _ in 0..nb.max(0) {
                let name = r.text(utf8)?;
                let _en = r.text(utf8)?;
                let pos = r.fv::<3>()?;
                let parent = r.sidx(bsz)? as i32;
                r.i32()?; // 变形阶层
                let flags = u16::from_le_bytes(r.take(2)?.try_into().unwrap());
                if flags & 0x1 != 0 {
                    r.sidx(bsz)?;
                } else {
                    r.fv::<3>()?;
                }
                if flags & 0x300 != 0 {
                    r.sidx(bsz)?;
                    r.f32()?;
                }
                if flags & 0x400 != 0 {
                    r.fv::<3>()?;
                }
                if flags & 0x800 != 0 {
                    r.fv::<6>()?;
                }
                if flags & 0x2000 != 0 {
                    r.i32()?;
                }
                if flags & 0x20 != 0 {
                    r.sidx(bsz)?;
                    r.i32()?;
                    r.f32()?;
                    let nl = r.i32()?;
                    for _ in 0..nl.max(0) {
                        r.sidx(bsz)?;
                        if r.u8()? != 0 {
                            r.fv::<6>()?;
                        }
                    }
                }
                bones.push(PmxBone { name, pos, parent });
            }
            Ok(())
        })();
        if ok.is_ok() {
            p.bones = bones;
        }
    }
    Ok(p)
}

fn pad4(v: &mut Vec<u8>) {
    while v.len() % 4 != 0 {
        v.push(0);
    }
}

/// PMX → GLB(每个材质一个 primitive, 贴图转 PNG 内嵌; 同一张贴图只嵌一次)
/// 返回 (GLB 字节, 报告)
pub fn pmx_to_glb(p: &Pmx, base_dir: &Path, unit_m: f32, skip_hidden: bool) -> Result<(Vec<u8>, Value)> {
    let mut bin: Vec<u8> = Vec::new();
    let mut views: Vec<Value> = Vec::new();
    let mut accs: Vec<Value> = Vec::new();
    let mut prims: Vec<Value> = Vec::new();
    let mut mats: Vec<Value> = Vec::new();
    let mut images: Vec<Value> = Vec::new();
    let mut textures: Vec<Value> = Vec::new();
    let mut tex_map: HashMap<usize, Option<(usize, bool)>> = HashMap::new(); // pmx 贴图 → (glTF texture, 有透明)
    let mut rep_mats = Vec::new();
    let mut missing = Vec::new();
    fn push_view(bin: &mut Vec<u8>, views: &mut Vec<Value>, data: &[u8], target: Option<u32>) -> usize {
        pad4(bin);
        let o = bin.len();
        bin.extend_from_slice(data);
        let mut v = json!({"buffer": 0, "byteOffset": o, "byteLength": data.len()});
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        views.push(v);
        views.len() - 1
    }
    let (mut gmn, mut gmx) = ([f32::MAX; 3], [f32::MIN; 3]);
    let mut start = 0usize;
    for (mi, m) in p.materials.iter().enumerate() {
        let idx = &p.indices[start..start + m.index_count];
        start += m.index_count;
        if m.index_count == 0 {
            continue;
        }
        if skip_hidden && m.diffuse[3] <= 0.001 {
            rep_mats.push(json!({"name": m.name, "skipped": "diffuse alpha = 0(MMD 里通常是隐藏的切换部件)"}));
            continue;
        }
        // 贴图
        let mut tex_i: Option<(usize, bool)> = None;
        if let Some(t) = m.texture {
            tex_i = *tex_map.entry(t).or_insert_with(|| {
                let rel = p.textures[t].replace('\\', "/");
                let path = base_dir.join(&rel);
                let img = std::fs::read(&path).map_err(|e| anyhow!("{e}")).and_then(|b| decode_image(&b));
                match img {
                    Ok(img) => {
                        let alpha = texture::has_alpha(&img);
                        let png = texture::write_png(&img, true).ok()?;
                        let bv = push_view(&mut bin, &mut views, &png, None);
                        images.push(json!({"name": rel, "mimeType": "image/png", "bufferView": bv}));
                        textures.push(json!({"source": images.len() - 1}));
                        Some((textures.len() - 1, alpha))
                    }
                    Err(e) => {
                        missing.push(json!({"texture": rel, "error": e.to_string()}));
                        None
                    }
                }
            });
        }
        // 只取用到的顶点(紧凑), z 取反, 绕序翻转
        let mut remap: HashMap<u32, u32> = HashMap::new();
        let (mut pos, mut nrm, mut uv) = (Vec::<u8>::new(), Vec::<u8>::new(), Vec::<u8>::new());
        let (mut mn, mut mx) = ([f32::MAX; 3], [f32::MIN; 3]);
        let mut out_idx: Vec<u32> = Vec::with_capacity(idx.len());
        for tri in idx.chunks_exact(3) {
            for &vi in [tri[0], tri[2], tri[1]].iter() {
                let n = remap.len() as u32;
                let k = *remap.entry(vi).or_insert_with(|| {
                    let v = p.positions[vi as usize];
                    let q = [v[0] * unit_m, v[1] * unit_m, -v[2] * unit_m];
                    for a in 0..3 {
                        mn[a] = mn[a].min(q[a]);
                        mx[a] = mx[a].max(q[a]);
                    }
                    q.iter().for_each(|x| pos.extend_from_slice(&x.to_le_bytes()));
                    let nn = p.normals[vi as usize];
                    [nn[0], nn[1], -nn[2]].iter().for_each(|x| nrm.extend_from_slice(&x.to_le_bytes()));
                    p.uvs[vi as usize].iter().for_each(|x| uv.extend_from_slice(&x.to_le_bytes()));
                    n
                });
                out_idx.push(k);
            }
        }
        let nvtx = remap.len();
        for a in 0..3 {
            gmn[a] = gmn[a].min(mn[a]);
            gmx[a] = gmx[a].max(mx[a]);
        }
        let ib: Vec<u8> = out_idx.iter().flat_map(|i| i.to_le_bytes()).collect();
        let vp = push_view(&mut bin, &mut views, &pos, Some(34962));
        let vn = push_view(&mut bin, &mut views, &nrm, Some(34962));
        let vu = push_view(&mut bin, &mut views, &uv, Some(34962));
        let vi = push_view(&mut bin, &mut views, &ib, Some(34963));
        let a0 = accs.len();
        accs.push(json!({"bufferView": vp, "componentType": 5126, "count": nvtx, "type": "VEC3", "min": mn, "max": mx}));
        accs.push(json!({"bufferView": vn, "componentType": 5126, "count": nvtx, "type": "VEC3"}));
        accs.push(json!({"bufferView": vu, "componentType": 5126, "count": nvtx, "type": "VEC2"}));
        accs.push(json!({"bufferView": vi, "componentType": 5125, "count": out_idx.len(), "type": "SCALAR"}));
        let double = m.flags & 1 != 0;
        let has_alpha = tex_i.map(|t| t.1).unwrap_or(false) || m.diffuse[3] < 0.999;
        let mut gm = json!({"name": m.name, "doubleSided": double,
            "pbrMetallicRoughness": {"baseColorFactor": m.diffuse, "metallicFactor": 0.0, "roughnessFactor": 0.8}});
        if let Some((ti, _)) = tex_i {
            gm["pbrMetallicRoughness"]["baseColorTexture"] = json!({"index": ti});
        }
        if has_alpha {
            gm["alphaMode"] = json!("MASK");
            gm["alphaCutoff"] = json!(0.5);
        }
        mats.push(gm);
        prims.push(json!({"attributes": {"POSITION": a0, "NORMAL": a0 + 1, "TEXCOORD_0": a0 + 2}, "indices": a0 + 3, "material": mats.len() - 1, "mode": 4}));
        rep_mats.push(json!({"name": m.name, "pmx_index": mi, "triangles": m.index_count / 3, "vertices": nvtx, "texture": m.texture.map(|t| p.textures[t].clone()), "doubleSided": double, "alpha": has_alpha}));
    }
    if prims.is_empty() {
        bail!("PMX 里没有可见材质");
    }
    pad4(&mut bin);
    let mut j = json!({
        "asset": {"version": "2.0", "generator": "wowsmod model pmx2glb"},
        "scene": 0, "scenes": [{"nodes": [0]}],
        "nodes": [{"name": if p.name.is_empty() { "pmx".to_string() } else { p.name.clone() }, "mesh": 0}],
        "meshes": [{"name": "pmx", "primitives": prims}],
        "materials": mats, "accessors": accs, "bufferViews": views,
        "buffers": [{"byteLength": bin.len()}]
    });
    if !images.is_empty() {
        j["images"] = json!(images);
        j["textures"] = json!(textures);
    }
    let mut js = serde_json::to_vec(&j)?;
    while js.len() % 4 != 0 {
        js.push(b' ');
    }
    let total = 12 + 8 + js.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&js);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    let (mn, mx) = (gmn, gmx); // 包围盒(输出单位, 只含导出的材质)
    let rep = json!({"pmx_version": p.version, "name": p.name, "comment_head": p.comment.chars().take(300).collect::<String>(),
        "vertices": p.positions.len(), "triangles": p.indices.len() / 3, "materials": rep_mats, "images_embedded": images.len(),
        "missing_textures": missing, "unit_m": unit_m, "bbox_min": mn, "bbox_max": mx, "height": mx[1] - mn[1],
        "not_converted": ["骨骼/蒙皮", "表情(morph)", "物理(刚体/关节)", "描边", "toon/sphere 贴图"],
        "license_note": "PMX 模型多有作者使用规约(禁止商用/禁止改造/需署名等), 转换前请确认; 本工具不改变模型的授权"});
    Ok((out, rep))
}

pub fn pmx_file_to_glb(input: &Path, unit_m: f32, skip_hidden: bool, pose: &[PoseOp]) -> Result<(Vec<u8>, Value)> {
    let b = std::fs::read(input).map_err(|e| anyhow!("读取 {}: {e}", input.display()))?;
    let mut p = read_pmx(&b)?;
    let pose_rep = if pose.is_empty() { Value::Null } else { apply_pose(&mut p, pose)? };
    let dir: PathBuf = input.parent().map(|x| x.to_path_buf()).unwrap_or_default();
    let (g, mut rep) = pmx_to_glb(&p, &dir, unit_m, skip_hidden)?;
    rep["pose"] = pose_rep;
    rep["bones"] = json!(p.bones.len());
    Ok((g, rep))
}

// ---------- 摆姿势(静态): 骨骼正向运动学 + 线性蒙皮 ----------
/// 姿势操作: 某骨骼(按名字)绕轴旋转; Lower = 把骨骼指向下方转动 deg 度(手臂放下等)
#[derive(Clone, Debug)]
pub enum PoseOp {
    /// 骨骼名, 绕世界轴(静止姿势坐标)的欧拉角 x,y,z(度, 依次 X→Y→Z)
    Euler(String, [f32; 3]),
    /// 骨骼名, 朝 -Y(向下)转动的角度(度); 转轴 = 骨骼方向 × 向下
    Lower(String, f32),
}

/// 解析: "arms-down[:度]" 预设, 或 "骨骼名=rx,ry,rz", 或 "骨骼名^度"(向下转)
pub fn parse_pose(spec: &str) -> Result<Vec<PoseOp>> {
    let spec = spec.trim();
    if let Some(rest) = spec.strip_prefix("arms-down") {
        let d: f32 = rest.trim_start_matches(':').parse().unwrap_or(60.0);
        return Ok(vec![PoseOp::Lower("左腕".into(), d), PoseOp::Lower("右腕".into(), d)]);
    }
    if let Some((n, d)) = spec.split_once('^') {
        return Ok(vec![PoseOp::Lower(n.trim().into(), d.trim().parse().map_err(|_| anyhow!("--pose {spec}: 角度"))?)]);
    }
    let (n, v) = spec.split_once('=').ok_or_else(|| anyhow!("--pose 格式: arms-down[:度] | 骨骼名=rx,ry,rz | 骨骼名^度"))?;
    let a: Vec<f32> = v.split(',').map(|x| x.trim().parse::<f32>()).collect::<std::result::Result<_, _>>().map_err(|_| anyhow!("--pose {spec}: 角度"))?;
    if a.len() != 3 {
        bail!("--pose {spec}: 需要 3 个角度");
    }
    Ok(vec![PoseOp::Euler(n.trim().into(), [a[0], a[1], a[2]])])
}

type Q = [f32; 4]; // x,y,z,w
fn q_axis(ax: [f32; 3], deg: f32) -> Q {
    let l = (ax[0] * ax[0] + ax[1] * ax[1] + ax[2] * ax[2]).sqrt().max(1e-9);
    let h = deg.to_radians() * 0.5;
    let s = h.sin() / l;
    [ax[0] * s, ax[1] * s, ax[2] * s, h.cos()]
}
fn q_mul(a: Q, b: Q) -> Q {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}
fn q_rot(q: Q, v: [f32; 3]) -> [f32; 3] {
    let p = q_mul(q_mul(q, [v[0], v[1], v[2], 0.0]), [-q[0], -q[1], -q[2], q[3]]);
    [p[0], p[1], p[2]]
}

/// 把姿势烘进顶点(位置与法线)。骨骼静止姿势无朝向(PMX 惯例), 局部旋转即在父骨骼已旋转的坐标系里。
pub fn apply_pose(p: &mut Pmx, ops: &[PoseOp]) -> Result<Value> {
    let nb = p.bones.len();
    if nb == 0 {
        bail!("PMX 没有可用的骨骼数据, 不能摆姿势");
    }
    let find = |n: &str| p.bones.iter().position(|b| b.name == n);
    let mut local: Vec<Q> = vec![[0.0, 0.0, 0.0, 1.0]; nb];
    let mut applied = Vec::new();
    let mut missing = Vec::new();
    for op in ops {
        match op {
            PoseOp::Euler(n, e) => match find(n) {
                Some(i) => {
                    let q = q_mul(q_axis([0.0, 0.0, 1.0], e[2]), q_mul(q_axis([0.0, 1.0, 0.0], e[1]), q_axis([1.0, 0.0, 0.0], e[0])));
                    local[i] = q_mul(q, local[i]);
                    applied.push(json!({"bone": n, "euler": e}));
                }
                None => missing.push(n.clone()),
            },
            PoseOp::Lower(n, d) => match find(n) {
                Some(i) => {
                    // 骨骼方向: 指向第一个子骨骼
                    let c = p.bones.iter().position(|b| b.parent == i as i32).ok_or_else(|| anyhow!("骨骼 {n} 没有子骨骼, 无法确定方向"))?;
                    let (a, b) = (p.bones[i].pos, p.bones[c].pos);
                    let dir = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                    let down = [0.0, -1.0, 0.0];
                    let ax = [dir[1] * down[2] - dir[2] * down[1], dir[2] * down[0] - dir[0] * down[2], dir[0] * down[1] - dir[1] * down[0]];
                    local[i] = q_mul(q_axis(ax, *d), local[i]);
                    applied.push(json!({"bone": n, "lower_deg": d}));
                }
                None => missing.push(n.clone()),
            },
        }
    }
    if applied.is_empty() {
        bail!("姿势里的骨骼都不存在: {missing:?}(骨骼名见 PMX; 常见: 左腕/右腕/左ひじ/右ひじ/首/頭)");
    }
    // 正向运动学: 世界旋转 R_b = R_parent * L_b; 世界位置 P_b = P_parent' + R_parent*(rest_b - rest_parent)
    let mut order: Vec<usize> = Vec::with_capacity(nb);
    let mut done = vec![false; nb];
    while order.len() < nb {
        let before = order.len();
        for i in 0..nb {
            let pa = p.bones[i].parent;
            if !done[i] && (pa < 0 || pa as usize >= nb || done[pa as usize]) {
                done[i] = true;
                order.push(i);
            }
        }
        if order.len() == before {
            bail!("PMX 骨骼父子关系有环");
        }
    }
    let mut wr: Vec<Q> = vec![[0.0, 0.0, 0.0, 1.0]; nb];
    let mut wp: Vec<[f32; 3]> = p.bones.iter().map(|b| b.pos).collect();
    for &i in &order {
        let pa = p.bones[i].parent;
        if pa >= 0 && (pa as usize) < nb {
            let pa = pa as usize;
            let off = [p.bones[i].pos[0] - p.bones[pa].pos[0], p.bones[i].pos[1] - p.bones[pa].pos[1], p.bones[i].pos[2] - p.bones[pa].pos[2]];
            let o = q_rot(wr[pa], off);
            wp[i] = [wp[pa][0] + o[0], wp[pa][1] + o[1], wp[pa][2] + o[2]];
            wr[i] = q_mul(wr[pa], local[i]);
        } else {
            wr[i] = local[i];
        }
    }
    let mut moved = 0usize;
    for (vi, w) in p.weights.iter().enumerate() {
        let v = p.positions[vi];
        let nrm = p.normals[vi];
        let (mut acc, mut accn, mut tw) = ([0.0f32; 3], [0.0f32; 3], 0.0f32);
        for &(b, wt) in w {
            if b < 0 || b as usize >= nb || wt <= 0.0 {
                continue;
            }
            let b = b as usize;
            let rest = p.bones[b].pos;
            let r = q_rot(wr[b], [v[0] - rest[0], v[1] - rest[1], v[2] - rest[2]]);
            let rn = q_rot(wr[b], nrm);
            for k in 0..3 {
                acc[k] += wt * (wp[b][k] + r[k]);
                accn[k] += wt * rn[k];
            }
            tw += wt;
        }
        if tw > 0.0 {
            let np = [acc[0] / tw, acc[1] / tw, acc[2] / tw];
            if (np[0] - v[0]).abs() + (np[1] - v[1]).abs() + (np[2] - v[2]).abs() > 1e-5 {
                moved += 1;
            }
            p.positions[vi] = np;
            let l = (accn[0] * accn[0] + accn[1] * accn[1] + accn[2] * accn[2]).sqrt().max(1e-9);
            p.normals[vi] = [accn[0] / l, accn[1] / l, accn[2] / l];
        }
    }
    Ok(json!({"applied": applied, "missing_bones": missing, "vertices_moved": moved, "note": "静态摆姿势: 线性蒙皮(SDEF 近似为 BDEF2), 无 IK/物理; 手指等细节不动"}))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn pose_spec_parses() {
        let a = parse_pose("arms-down:45").unwrap();
        assert_eq!(a.len(), 2);
        assert!(matches!(&a[0], PoseOp::Lower(_, d) if (*d - 45.0).abs() < 1e-3));
        let b = parse_pose("首=10,0,0").unwrap();
        assert!(matches!(&b[0], PoseOp::Euler(n, r) if n == "首" && (r[0] - 10.0).abs() < 1e-3));
        assert!(parse_pose("左腕^30").is_ok());
    }

    /// 测试用最小 PMX 2.0 写出器(UTF-8, 索引 4 字节)
    pub fn tiny_pmx(tex: &str) -> Vec<u8> {
        let mut b = Vec::new();
        let s = |b: &mut Vec<u8>, t: &str| {
            b.extend_from_slice(&(t.len() as i32).to_le_bytes());
            b.extend_from_slice(t.as_bytes());
        };
        let f = |b: &mut Vec<u8>, v: &[f32]| v.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
        b.extend_from_slice(b"PMX ");
        f(&mut b, &[2.0]);
        b.push(8);
        b.extend_from_slice(&[1, 0, 4, 4, 4, 4, 4, 4]);
        for t in ["t", "", "c", ""] {
            s(&mut b, t);
        }
        // 4 顶点(z = 1 → glTF z = -unit)
        b.extend_from_slice(&4i32.to_le_bytes());
        for (x, y) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
            f(&mut b, &[x, y, 1.0, 0.0, 0.0, -1.0, x, 1.0 - y]);
            b.push(0); // BDEF1
            b.extend_from_slice(&0i32.to_le_bytes());
            f(&mut b, &[1.0]);
        }
        b.extend_from_slice(&9i32.to_le_bytes());
        for i in [0u32, 1, 2, 0, 2, 3, 0, 1, 2] {
            b.extend_from_slice(&i.to_le_bytes());
        }
        b.extend_from_slice(&1i32.to_le_bytes());
        s(&mut b, tex);
        b.extend_from_slice(&3i32.to_le_bytes());
        for (name, flags, texi, cnt, alpha) in [("body", 0u8, 0i32, 3i32, 1.0f32), ("cloth", 1u8, -1, 3, 1.0), ("hidden", 0u8, -1, 3, 0.0)] {
            s(&mut b, name);
            s(&mut b, "");
            f(&mut b, &[1.0, 0.5, 0.25, alpha]);
            f(&mut b, &[0.0; 7]);
            b.push(flags);
            f(&mut b, &[0.0; 5]);
            b.extend_from_slice(&texi.to_le_bytes());
            b.extend_from_slice(&(-1i32).to_le_bytes());
            b.push(0);
            b.push(1);
            b.push(0);
            s(&mut b, "");
            b.extend_from_slice(&cnt.to_le_bytes());
        }
        b
    }

    #[test]
    fn pmx_roundtrip_to_glb() {
        let dir = std::env::temp_dir().join(format!("wm_pmx_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("tex")).unwrap();
        let mut im = Image::new(2, 2);
        im.px = vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 0, 255, 255, 255, 255];
        std::fs::write(dir.join("tex/a.png"), texture::write_png(&im, true).unwrap()).unwrap();
        let p = read_pmx(&tiny_pmx("tex\\a.png")).unwrap();
        assert_eq!((p.positions.len(), p.indices.len(), p.materials.len()), (4, 9, 3));
        assert_eq!(p.materials[1].flags & 1, 1);
        let (glb, rep) = pmx_to_glb(&p, &dir, 0.5, true).unwrap();
        assert_eq!(rep["images_embedded"], 1);
        let sc = wowsmod_formats::scene::read_scene(&glb).unwrap();
        let prims = &sc.meshes[0].prims;
        assert_eq!(prims.len(), 2, "hidden 材质应跳过");
        assert_eq!(prims[0].positions[0], [0.0, 0.0, -0.5]);
        assert_eq!(prims[0].indices, vec![0, 1, 2]);
        // 绕序翻转: 原 (0,1,2) → (0,2,1)
        assert_eq!(prims[0].positions[1], [0.5, 0.5, -0.5]);
        let gm = read_glb_materials(&glb, None).unwrap();
        assert_eq!(gm.mats.len(), 2);
        assert_eq!(gm.mats[0].alpha_mode, "MASK"); // 贴图有透明像素
        assert!(gm.mats[1].double_sided);
        assert_eq!(gm.mats[0].base_image, Some(0));
        let img = decode_image(gm.images[0].as_ref().unwrap()).unwrap();
        assert_eq!(img.px, im.px);
        assert_eq!(prims[1].material_index, Some(1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bmp_tga_decode() {
        // 2x1 24 位 BMP(自下而上)
        let mut b = b"BM".to_vec();
        let pix: Vec<u8> = vec![0, 0, 255, 0, 255, 0, 0, 0];
        b.extend_from_slice(&((54 + pix.len()) as u32).to_le_bytes());
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(&54u32.to_le_bytes());
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&2i32.to_le_bytes());
        b.extend_from_slice(&1i32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&24u16.to_le_bytes());
        b.extend_from_slice(&[0; 24]);
        b.extend_from_slice(&pix);
        let im = decode_image(&b).unwrap();
        assert_eq!(im.px, vec![255, 0, 0, 255, 0, 255, 0, 255]);
        // TGA RLE 32 位 2x1 自上而下
        let mut t = vec![0u8, 0, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 32, 0x28];
        t.extend_from_slice(&[0x81, 10, 20, 30, 40]);
        let im = decode_image(&t).unwrap();
        assert_eq!(im.px, vec![30, 20, 10, 40, 30, 20, 10, 40]);
    }

    #[test]
    fn jpeg_decode() {
        // testdata/red8.jpg: 8x8 纯色 (200,40,40), 本项目用 PIL 生成(无第三方版权)
        let im = decode_image(include_bytes!("../testdata/red8.jpg")).unwrap();
        assert_eq!((im.width, im.height), (8, 8));
        let p = &im.px[0..4];
        assert!((p[0] as i32 - 200).abs() <= 3 && (p[1] as i32 - 40).abs() <= 3 && (p[2] as i32 - 40).abs() <= 3 && p[3] == 255, "{p:?}");
        assert!(decode_image(&[0xFF, 0xD8, 0xFF, 0xD9]).is_err());
    }
}
