//! DDS 读写 + 莱服分卷(.dd0/.dd1/.dd2 + .dds)拆分/合并。
//!
//! 分卷规则(由游戏文件实测得出, 详见 docs/DESIGN.md §贴图分卷; 全量验证见 examples/dd_volume_scan.rs):
//! - 一张有 K 个分卷的贴图 = `name.dd0 … name.dd{K-1}` + `name.dds`。
//! - `name.ddN` 是只含第 N 级 mip 的独立 DDS: flags = 0x1007(无 MIPMAPCOUNT), mip 数 = 0,
//!   caps = 0x1000(TEXTURE), reserved 全 0, 其余字段(像素格式、DX10 扩展头)与 .dds 相同。
//! - `name.dds` 是从第 K 级开始的完整 mip 链(尺寸 = 原图 >> K), 头部 `dwReserved1[0]`(偏移 32)= K,
//!   `dwReserved1[1]`(偏移 36)= `'QHDD'`(0x44444851)。没有分卷的贴图这两个字段为 0。
//! - `dwReserved1[2]`(偏移 40)= 1 表示载荷是 BC7Prep(见 [`crate::bc7prep`]); 分卷各自独立 Prep。
use crate::error::{Error, Result};

pub const MAGIC: &[u8; 4] = b"DDS ";
pub const QHDD: u32 = 0x4444_4851;
pub const DDSD_CAPS: u32 = 0x1;
pub const DDSD_HEIGHT: u32 = 0x2;
pub const DDSD_WIDTH: u32 = 0x4;
pub const DDSD_PIXELFORMAT: u32 = 0x1000;
pub const DDSD_MIPMAPCOUNT: u32 = 0x20000;
pub const DDSD_LINEARSIZE: u32 = 0x80000;
pub const DDSCAPS_COMPLEX: u32 = 0x8;
pub const DDSCAPS_TEXTURE: u32 = 0x1000;
pub const DDSCAPS_MIPMAP: u32 = 0x40_0000;
/// 莱服默认: 尺寸不超过此值的 mip 留在 .dds 里, 更大的 mip 各自成为一个分卷。
pub const VOLUME_TAIL_MAX: u32 = 512;

/// 像素格式(只列本工具需要处理的; 其余按 [`Format::Other`] 原样保留, 不能解码)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Format {
    BC1,
    BC2,
    BC3,
    BC4,
    BC5,
    BC6H,
    BC7,
    /// R8G8B8A8(DXGI 28/29 或等价的老式掩码)
    Rgba8,
    /// B8G8R8A8(DXGI 87/91 或老式 A8R8G8B8)
    Bgra8,
    /// 其它: 只知道每像素/每块字节数时仍可拆分合并
    Other,
}

impl Format {
    /// 块压缩格式: 每 4x4 块字节数; 非块格式返回 None。
    pub fn block_bytes(self) -> Option<usize> {
        match self {
            Format::BC1 | Format::BC4 => Some(8),
            Format::BC2 | Format::BC3 | Format::BC5 | Format::BC6H | Format::BC7 => Some(16),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Format::BC1 => "BC1",
            Format::BC2 => "BC2",
            Format::BC3 => "BC3",
            Format::BC4 => "BC4",
            Format::BC5 => "BC5",
            Format::BC6H => "BC6H",
            Format::BC7 => "BC7",
            Format::Rgba8 => "RGBA8",
            Format::Bgra8 => "BGRA8",
            Format::Other => "other",
        }
    }
    pub fn parse(s: &str) -> Option<Format> {
        Some(match s.to_ascii_uppercase().as_str() {
            "BC1" | "DXT1" => Format::BC1,
            "BC2" | "DXT3" => Format::BC2,
            "BC3" | "DXT5" => Format::BC3,
            "BC4" | "ATI1" => Format::BC4,
            "BC5" | "ATI2" => Format::BC5,
            "BC7" => Format::BC7,
            "RGBA8" => Format::Rgba8,
            "BGRA8" => Format::Bgra8,
            _ => return None,
        })
    }
}

/// 解析后的 DDS 头。`raw` 保留原始头字节(128 或 148), 写回时以它为模板, 保证往返逐字节一致。
#[derive(Debug, Clone)]
pub struct Header {
    pub raw: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    /// 头里记录的 mip 数(0 也按 1 级处理, 见 [`Header::levels`])
    pub mip_count: u32,
    pub format: Format,
    pub fourcc: [u8; 4],
    pub dxgi: Option<u32>,
    /// 非块格式时每像素字节数
    pub bytes_per_pixel: Option<usize>,
    pub array_size: u32,
    pub cubemap: bool,
    pub volume_texture: bool,
}

fn u32le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
fn put32(d: &mut [u8], o: usize, v: u32) {
    d[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

impl Header {
    pub fn parse(d: &[u8]) -> Result<Header> {
        if d.len() < 128 || &d[0..4] != MAGIC || u32le(d, 4) != 124 {
            return Err(Error::format("dds", "不是 DDS 文件(魔数/头长度不符)"));
        }
        let flags_pf = u32le(d, 80);
        let fourcc: [u8; 4] = d[84..88].try_into().unwrap();
        let caps2 = u32le(d, 112);
        let mut hdr_len = 128;
        let mut dxgi = None;
        let mut array_size = 1;
        let mut cubemap = caps2 & 0x200 != 0;
        let mut bytes_per_pixel = None;
        let format = if flags_pf & 0x4 != 0 {
            match &fourcc {
                b"DXT1" => Format::BC1,
                b"DXT2" | b"DXT3" => Format::BC2,
                b"DXT4" | b"DXT5" => Format::BC3,
                b"ATI1" | b"BC4U" => Format::BC4,
                b"ATI2" | b"BC5U" => Format::BC5,
                b"DX10" => {
                    if d.len() < 148 {
                        return Err(Error::format("dds", "DX10 扩展头不完整"));
                    }
                    hdr_len = 148;
                    let f = u32le(d, 128);
                    dxgi = Some(f);
                    cubemap = cubemap || u32le(d, 136) & 0x4 != 0;
                    array_size = u32le(d, 140).max(1);
                    match f {
                        70..=72 => Format::BC1,
                        73..=75 => Format::BC2,
                        76..=78 => Format::BC3,
                        79..=81 => Format::BC4,
                        82..=84 => Format::BC5,
                        94..=96 => Format::BC6H,
                        97..=99 => Format::BC7,
                        27..=29 => {
                            bytes_per_pixel = Some(4);
                            Format::Rgba8
                        }
                        87 | 90 | 91 => {
                            bytes_per_pixel = Some(4);
                            Format::Bgra8
                        }
                        _ => Format::Other,
                    }
                }
                _ => Format::Other,
            }
        } else if flags_pf & 0x40 != 0 {
            let bits = u32le(d, 88);
            bytes_per_pixel = Some((bits as usize).div_ceil(8));
            let (r, g, b, a) = (u32le(d, 92), u32le(d, 96), u32le(d, 100), u32le(d, 104));
            match (bits, r, g, b, a) {
                (32, 0xff, 0xff00, 0xff_0000, _) => Format::Rgba8,
                (32, 0xff_0000, 0xff00, 0xff, _) => Format::Bgra8,
                _ => Format::Other,
            }
        } else {
            let bits = u32le(d, 88);
            if bits > 0 {
                bytes_per_pixel = Some((bits as usize).div_ceil(8));
            }
            Format::Other
        };
        let caps = u32le(d, 108);
        let flags = u32le(d, 8);
        let volume_texture = caps2 & 0x20_0000 != 0;
        let depth = if volume_texture { u32le(d, 24).max(1) } else { 1 };
        let _ = (caps, flags);
        Ok(Header {
            raw: d[..hdr_len].to_vec(),
            width: u32le(d, 16),
            height: u32le(d, 12),
            depth,
            mip_count: u32le(d, 28),
            format,
            fourcc,
            dxgi,
            bytes_per_pixel,
            array_size,
            cubemap,
            volume_texture,
        })
    }
    pub fn len(&self) -> usize {
        self.raw.len()
    }
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }
    /// 实际的 mip 级数(头里写 0 视为 1)。
    pub fn levels(&self) -> u32 {
        self.mip_count.max(1)
    }
    pub fn flags(&self) -> u32 {
        u32le(&self.raw, 8)
    }
    pub fn caps(&self) -> u32 {
        u32le(&self.raw, 108)
    }
    /// `dwReserved1[0]`: 莱服分卷数 K(仅当 `dwReserved1[1]` = 'QHDD' 时有意义)
    pub fn volume_count(&self) -> Option<u32> {
        (u32le(&self.raw, 36) == QHDD).then(|| u32le(&self.raw, 32))
    }
    /// `dwReserved1[2]` = 1: BC7Prep 载荷
    pub fn bc7prep_flag(&self) -> bool {
        u32le(&self.raw, 40) == 1
    }
    /// 面/数组层数(立方体贴图 ×6)
    pub fn layers(&self) -> u32 {
        self.array_size * if self.cubemap { 6 } else { 1 }
    }
    /// 某一级 mip 的尺寸
    pub fn level_dims(&self, level: u32) -> (u32, u32) {
        ((self.width >> level).max(1), (self.height >> level).max(1))
    }
    /// 一级 mip(一个面/层)的字节数
    pub fn level_size(&self, level: u32) -> Result<usize> {
        let (w, h) = self.level_dims(level);
        let d = (self.depth >> level).max(1) as usize;
        surface_size(self.format, self.bytes_per_pixel, w, h).map(|s| s * d)
    }
    /// 头之后应有的数据总字节数
    pub fn data_size(&self) -> Result<usize> {
        let mut s = 0;
        for l in 0..self.levels() {
            s += self.level_size(l)?;
        }
        Ok(s * self.layers() as usize)
    }
}

/// 一个 2D 面的字节数
pub fn surface_size(format: Format, bpp: Option<usize>, w: u32, h: u32) -> Result<usize> {
    if let Some(b) = format.block_bytes() {
        Ok(w.div_ceil(4) as usize * h.div_ceil(4) as usize * b)
    } else if let Some(p) = bpp {
        Ok(w as usize * h as usize * p)
    } else {
        Err(Error::Unsupported(format!("无法确定像素格式 {:?} 的数据大小", format)))
    }
}

/// 一份 DDS: 头 + 各 mip(按 层 → mip 顺序, 与 D3D 一致)。
#[derive(Debug, Clone)]
pub struct Dds {
    pub header: Header,
    pub data: Vec<u8>,
}

impl Dds {
    /// 解析; 数据长度必须与头一致(BC7Prep 载荷请先用 [`crate::bc7prep::decode_dds`] 还原)。
    pub fn parse(d: &[u8]) -> Result<Dds> {
        let header = Header::parse(d)?;
        if header.bc7prep_flag() && header.dxgi.is_some_and(|f| (97..=99).contains(&f)) && crate::bc7prep::is_bc7prep_dds(d) {
            return Err(Error::Unsupported("BC7Prep 载荷, 请先用 bc7prep::decode_dds 还原".into()));
        }
        let need = header.data_size()?;
        let have = d.len() - header.len();
        if have != need {
            return Err(Error::format("dds", format!("数据 {have} 字节, 按头部({}x{} {} mips={} layers={})应为 {need}", header.width, header.height, header.format.name(), header.levels(), header.layers())));
        }
        Ok(Dds { data: d[header.len()..].to_vec(), header })
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = self.header.raw.clone();
        v.extend_from_slice(&self.data);
        v
    }
    /// 第 level 级 mip(第 0 层)的字节切片
    pub fn level(&self, level: u32) -> Result<&[u8]> {
        if self.header.layers() != 1 {
            return Err(Error::Unsupported("暂不支持立方体/数组贴图的按级访问".into()));
        }
        let mut off = 0;
        for l in 0..level {
            off += self.header.level_size(l)?;
        }
        let n = self.header.level_size(level)?;
        Ok(&self.data[off..off + n])
    }

    /// 由格式、尺寸、各级 mip 数据构造一份新的 DDS(单层 2D)。BC 格式写 DX10 头仅当 `dx10`=true 或格式是 BC7/BC6H;
    /// BC1/BC2/BC3 用 DXT1/3/5 FourCC, BC4/BC5 用 ATI1/ATI2(与游戏文件一致)。
    pub fn new_2d(format: Format, width: u32, height: u32, levels: Vec<Vec<u8>>, srgb: bool) -> Result<Dds> {
        let n = levels.len() as u32;
        let mut h = vec![0u8; 128];
        h[0..4].copy_from_slice(MAGIC);
        put32(&mut h, 4, 124);
        let mut flags = DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH | DDSD_PIXELFORMAT;
        let mut caps = DDSCAPS_TEXTURE;
        if n > 1 {
            flags |= DDSD_MIPMAPCOUNT;
            caps |= DDSCAPS_COMPLEX | DDSCAPS_MIPMAP;
        }
        put32(&mut h, 8, flags);
        put32(&mut h, 12, height);
        put32(&mut h, 16, width);
        put32(&mut h, 28, if n > 1 { n } else { 0 });
        put32(&mut h, 76, 32);
        put32(&mut h, 108, caps);
        let mut dx10 = None;
        match format {
            Format::BC1 | Format::BC2 | Format::BC3 | Format::BC4 | Format::BC5 if !srgb => {
                put32(&mut h, 80, 0x4);
                let cc: &[u8; 4] = match format {
                    Format::BC1 => b"DXT1",
                    Format::BC2 => b"DXT3",
                    Format::BC3 => b"DXT5",
                    Format::BC4 => b"ATI1",
                    _ => b"ATI2",
                };
                h[84..88].copy_from_slice(cc);
            }
            Format::Rgba8 if !srgb => {
                put32(&mut h, 80, 0x41);
                put32(&mut h, 88, 32);
                put32(&mut h, 92, 0xff);
                put32(&mut h, 96, 0xff00);
                put32(&mut h, 100, 0xff_0000);
                put32(&mut h, 104, 0xff00_0000);
            }
            _ => {
                put32(&mut h, 80, 0x4);
                h[84..88].copy_from_slice(b"DX10");
                let f = match (format, srgb) {
                    (Format::BC1, false) => 71,
                    (Format::BC1, true) => 72,
                    (Format::BC2, false) => 74,
                    (Format::BC2, true) => 75,
                    (Format::BC3, false) => 77,
                    (Format::BC3, true) => 78,
                    (Format::BC4, _) => 80,
                    (Format::BC5, _) => 83,
                    (Format::BC7, false) => 98,
                    (Format::BC7, true) => 99,
                    (Format::Rgba8, false) => 28,
                    (Format::Rgba8, true) => 29,
                    (Format::Bgra8, false) => 87,
                    (Format::Bgra8, true) => 91,
                    _ => return Err(Error::Unsupported(format!("不能写 {:?} 格式", format))),
                };
                dx10 = Some(f);
            }
        }
        if let Some(f) = dx10 {
            h.extend_from_slice(&[0u8; 20]);
            put32(&mut h, 128, f);
            put32(&mut h, 132, 3); // TEXTURE2D
            put32(&mut h, 140, 1);
        }
        let header = Header::parse(&h)?;
        let mut data = Vec::new();
        for (l, lv) in levels.iter().enumerate() {
            let need = header.level_size(l as u32)?;
            if lv.len() != need {
                return Err(Error::format("dds", format!("第 {l} 级数据 {} 字节, 应为 {need}", lv.len())));
            }
            data.extend_from_slice(lv);
        }
        Ok(Dds { header, data })
    }
}

/// 某个尺寸的贴图按莱服规则应拆成几个分卷: 最大边 > `tail_max` 的 mip 级数。
pub fn volume_count_for(width: u32, height: u32, tail_max: u32) -> u32 {
    let mut k = 0;
    while (width >> k).max(height >> k) > tail_max {
        k += 1;
    }
    k
}

/// 分卷头: 以完整贴图头为模板, 尺寸 = 第 level 级, 单级(无 MIPMAPCOUNT), caps = TEXTURE, reserved 清零。
fn volume_header(full: &Header, level: u32) -> Vec<u8> {
    let (w, h) = full.level_dims(level);
    let mut r = full.raw.clone();
    put32(&mut r, 8, full.flags() & !DDSD_MIPMAPCOUNT);
    put32(&mut r, 12, h);
    put32(&mut r, 16, w);
    put32(&mut r, 20, 0);
    put32(&mut r, 28, 0);
    for o in (32..76).step_by(4) {
        put32(&mut r, o, 0);
    }
    put32(&mut r, 108, DDSCAPS_TEXTURE);
    r
}

/// 把完整 DDS 拆成 (`.dds` 尾部, [`.dd0`, `.dd1`, …])。`k` 为分卷数(通常用 [`volume_count_for`])。
/// k = 0 时原样返回。仅支持单层 2D 贴图。
pub fn split_volumes(full: &Dds, k: u32) -> Result<(Vec<u8>, Vec<Vec<u8>>)> {
    let h = &full.header;
    if k == 0 {
        return Ok((full.to_bytes(), vec![]));
    }
    if h.layers() != 1 || h.depth != 1 {
        return Err(Error::Unsupported("分卷只支持单层 2D 贴图".into()));
    }
    if k >= h.levels() {
        return Err(Error::format("dds 分卷", format!("分卷数 {k} 必须小于 mip 级数 {}(.dds 至少保留一级)", h.levels())));
    }
    let mut vols = Vec::new();
    let mut off = 0;
    for l in 0..k {
        let n = h.level_size(l)?;
        let mut v = volume_header(h, l);
        v.extend_from_slice(&full.data[off..off + n]);
        off += n;
        vols.push(v);
    }
    let (w, hh) = h.level_dims(k);
    let mut t = h.raw.clone();
    put32(&mut t, 12, hh);
    put32(&mut t, 16, w);
    put32(&mut t, 28, h.levels() - k);
    put32(&mut t, 32, k);
    put32(&mut t, 36, QHDD);
    let mut flags = h.flags() | DDSD_MIPMAPCOUNT;
    let mut caps = h.caps();
    if h.levels() - k > 1 {
        caps |= DDSCAPS_COMPLEX | DDSCAPS_MIPMAP;
    } else {
        flags &= !DDSD_MIPMAPCOUNT;
    }
    put32(&mut t, 8, flags);
    put32(&mut t, 108, caps);
    t.extend_from_slice(&full.data[off..]);
    Ok((t, vols))
}

/// 合并: `.dds` 尾部 + 分卷(按 dd0, dd1, … 顺序)→ 完整 DDS。合并结果的 reserved 分卷字段清零。
/// 会校验分卷数(头部 K)、各分卷尺寸与格式。
pub fn merge_volumes(tail: &[u8], vols: &[&[u8]]) -> Result<Dds> {
    let th = Header::parse(tail)?;
    let k = th.volume_count().unwrap_or(0);
    if k as usize != vols.len() {
        return Err(Error::format("dds 分卷", format!(".dds 头部记录分卷数 {k}, 实际提供 {} 个", vols.len())));
    }
    let tail_dds = Dds::parse(tail)?;
    if k == 0 {
        return Ok(tail_dds);
    }
    let v0 = Header::parse(vols[0])?;
    let mut full = th.raw.clone();
    put32(&mut full, 12, v0.height);
    put32(&mut full, 16, v0.width);
    put32(&mut full, 28, th.levels() + k);
    put32(&mut full, 32, 0);
    put32(&mut full, 36, 0);
    put32(&mut full, 8, th.flags() | DDSD_MIPMAPCOUNT);
    put32(&mut full, 108, th.caps() | DDSCAPS_COMPLEX | DDSCAPS_MIPMAP | DDSCAPS_TEXTURE);
    let fh = Header::parse(&full)?;
    let mut data = Vec::new();
    for (l, v) in vols.iter().enumerate() {
        let vh = Header::parse(v)?;
        let (w, h) = fh.level_dims(l as u32);
        if (vh.width, vh.height) != (w, h) || vh.format != fh.format || vh.dxgi != fh.dxgi {
            return Err(Error::format("dds 分卷", format!("dd{l}: {}x{} {:?}, 期望 {w}x{h} {:?}", vh.width, vh.height, vh.format, fh.format)));
        }
        let n = fh.level_size(l as u32)?;
        if v.len() != vh.len() + n {
            return Err(Error::format("dds 分卷", format!("dd{l}: {} 字节, 期望 {}", v.len(), vh.len() + n)));
        }
        data.extend_from_slice(&v[vh.len()..]);
    }
    let (tw, tht) = fh.level_dims(k);
    if (th.width, th.height) != (tw, tht) {
        return Err(Error::format("dds 分卷", format!(".dds 顶级 {}x{}, 期望 {tw}x{tht}", th.width, th.height)));
    }
    data.extend_from_slice(&tail_dds.data);
    let out = Dds { header: fh, data };
    if out.data.len() != out.header.data_size()? {
        return Err(Error::format("dds 分卷", "合并后数据长度与头部不符"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(format: Format, w: u32, h: u32) -> Dds {
        let mut levels = vec![];
        let (mut x, mut y, mut seed) = (w, h, 1u32);
        loop {
            let n = surface_size(format, Some(4), x, y).unwrap();
            levels.push((0..n).map(|_| { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); (seed >> 24) as u8 }).collect());
            if x == 1 && y == 1 { break; }
            x = (x / 2).max(1);
            y = (y / 2).max(1);
        }
        Dds::new_2d(format, w, h, levels, false).unwrap()
    }

    #[test]
    fn split_merge_roundtrip() {
        for (f, w, h) in [(Format::BC1, 4096, 4096), (Format::BC7, 2048, 1024), (Format::BC3, 1024, 2048), (Format::BC5, 512, 512)] {
            let d = fake(f, w, h);
            let k = volume_count_for(w, h, VOLUME_TAIL_MAX);
            let (tail, vols) = split_volumes(&d, k).unwrap();
            assert_eq!(vols.len() as u32, k);
            let th = Header::parse(&tail).unwrap();
            assert!(th.width.max(th.height) <= VOLUME_TAIL_MAX);
            let refs: Vec<&[u8]> = vols.iter().map(|v| v.as_slice()).collect();
            let m = merge_volumes(&tail, &refs).unwrap();
            assert_eq!(m.data, d.data);
            let (tail2, vols2) = split_volumes(&m, k).unwrap();
            assert_eq!(tail2, tail);
            assert_eq!(vols2, vols);
        }
    }

    #[test]
    fn volume_counts() {
        assert_eq!(volume_count_for(4096, 4096, 512), 3);
        assert_eq!(volume_count_for(1024, 1024, 512), 1);
        assert_eq!(volume_count_for(512, 512, 512), 0);
        assert_eq!(volume_count_for(4096, 1024, 512), 3);
    }
}
