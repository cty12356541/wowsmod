//! `.geometry` 文件读/写。
//!
//! 格式见 docs/DESIGN.md; 本实现由 M0 Python 原型
//! (m0/geometry_roundtrip/geo_roundtrip.py, 18 个样本逐字节往返) 移植而来。
//!
//! ```text
//! Header (0x48):
//!   u32 vertices_count, indices_count, vmap_count, imap_count, collision_count, armor_count
//!   i64 vmap_ptr, imap_ptr, vertices_ptr, indices_ptr, collision_ptr, armor_ptr   (绝对偏移; count=0 时为 0)
//! MappingEntry (16B): u32 id, u16 buffer_index, u16 packed_texel_density(f16), u32 items_offset, u32 items_count
//! VerticesPrototype (0x20): i64 data_rel(相对本结构), PackedString{u32 len, u32 pad, i64 text_rel(相对 PackedString)},
//!                           u32 size, u16 stride, u8 skinned, u8 bumped
//! IndicesPrototype (0x10):  i64 data_rel, u32 size, u16 reserved, u16 index_size
//! Collision/Armor (0x20):   i64 rel(指向 body 尾部 tail 字节之前), PackedString, u32 tail_size, u32 pad
//! 数据块: "ENCD" + u32 元素数 + meshoptimizer 编码 (顶点 v0 = 0xA0 头, 索引 v1 = 0xE1 头); 或原始数据
//! 紧凑排布顺序: header, vmap, imap, vprotos, [vblob,name]*, iprotos, iblob*, cmprotos, [cmbody,name]*, armorprotos, [body,name]*
//! ```
//! 注意: mapping 的索引是"局部"的(相对 items_offset 所属 mapping 的顶点区间), 且 vmap / imap
//! 数组顺序**不**两两对应——配对必须用 mapping id(见 [`crate::pairing`])。

use crate::error::{Error, Result};
use crate::util::*;

pub const ENCD: &[u8; 4] = b"ENCD";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct MappingEntry {
    pub id: u32,
    pub buffer_index: u16,
    pub packed_texel_density: u16,
    pub items_offset: u32,
    pub items_count: u32,
}

impl MappingEntry {
    pub fn texel_density(&self) -> f32 {
        half::f16::from_bits(self.packed_texel_density).to_f32()
    }
    fn parse(d: &[u8], off: usize) -> Result<Self> {
        Ok(MappingEntry {
            id: u32le(d, off, "mapping")?,
            buffer_index: u16le(d, off + 4, "mapping")?,
            packed_texel_density: u16le(d, off + 6, "mapping")?,
            items_offset: u32le(d, off + 8, "mapping")?,
            items_count: u32le(d, off + 12, "mapping")?,
        })
    }
    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.id.to_le_bytes());
        out.extend_from_slice(&self.buffer_index.to_le_bytes());
        out.extend_from_slice(&self.packed_texel_density.to_le_bytes());
        out.extend_from_slice(&self.items_offset.to_le_bytes());
        out.extend_from_slice(&self.items_count.to_le_bytes());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexBuffer {
    /// 原始数据块(通常是 ENCD + meshopt 编码)
    pub blob: Vec<u8>,
    /// 格式名原样字节(含结尾 NUL), 例如 b"set3/xyznuvtpc\0"
    pub format_raw: Vec<u8>,
    pub name_pad: u32,
    pub stride: u16,
    pub skinned: u8,
    pub bumped: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexBuffer {
    pub blob: Vec<u8>,
    pub reserved: u16,
    pub index_size: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub body: Vec<u8>,
    pub tail: u32,
    pub name_raw: Vec<u8>,
    pub name_pad: u32,
    pub pad2: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Geometry {
    pub vertex_mappings: Vec<MappingEntry>,
    pub index_mappings: Vec<MappingEntry>,
    pub vertex_buffers: Vec<VertexBuffer>,
    pub index_buffers: Vec<IndexBuffer>,
    pub collision: Vec<Section>,
    pub armor: Vec<Section>,
}

fn strip_nul(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

impl VertexBuffer {
    pub fn format(&self) -> String {
        strip_nul(&self.format_raw)
    }
    pub fn is_encoded(&self) -> bool {
        self.blob.len() >= 8 && &self.blob[..4] == ENCD
    }
    pub fn vertex_count(&self) -> usize {
        if self.is_encoded() {
            u32::from_le_bytes(self.blob[4..8].try_into().unwrap()) as usize
        } else if self.stride > 0 {
            self.blob.len() / self.stride as usize
        } else {
            0
        }
    }
    /// 解码为紧密排列的顶点字节(vertex_count * stride)。
    pub fn decode(&self) -> Result<Vec<u8>> {
        let stride = self.stride as usize;
        if !self.is_encoded() {
            return Ok(self.blob.clone());
        }
        let n = self.vertex_count();
        crate::geometry::meshopt::decode_vertices(n, stride, &self.blob[8..])
    }
    /// 由紧密顶点字节编码(meshopt 顶点编码 v0, 与游戏文件一致)。
    pub fn encode_from(&mut self, raw: &[u8]) -> Result<()> {
        let stride = self.stride as usize;
        if stride == 0 || raw.len() % stride != 0 {
            return Err(Error::format("vertex buffer", format!("数据长度 {} 不是 stride {stride} 的整数倍", raw.len())));
        }
        let n = raw.len() / stride;
        let mut blob = Vec::with_capacity(raw.len() / 2 + 64);
        blob.extend_from_slice(ENCD);
        blob.extend_from_slice(&(n as u32).to_le_bytes());
        blob.extend_from_slice(&crate::geometry::meshopt::encode_vertices(n, stride, raw)?);
        self.blob = blob;
        Ok(())
    }
}

impl IndexBuffer {
    pub fn is_encoded(&self) -> bool {
        self.blob.len() >= 8 && &self.blob[..4] == ENCD
    }
    pub fn index_count(&self) -> usize {
        if self.is_encoded() {
            u32::from_le_bytes(self.blob[4..8].try_into().unwrap()) as usize
        } else {
            self.blob.len() / (self.index_size.max(1) as usize)
        }
    }
    pub fn decode(&self) -> Result<Vec<u32>> {
        let n = self.index_count();
        if !self.is_encoded() {
            return Ok(match self.index_size {
                2 => self.blob.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]) as u32).collect(),
                4 => self.blob.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect(),
                s => return Err(Error::Unsupported(format!("index_size={s}"))),
            });
        }
        crate::geometry::meshopt::decode_indices(n, self.index_size, &self.blob[8..])
    }
    pub fn encode_from(&mut self, indices: &[u32]) -> Result<()> {
        let mut blob = Vec::new();
        blob.extend_from_slice(ENCD);
        blob.extend_from_slice(&(indices.len() as u32).to_le_bytes());
        blob.extend_from_slice(&crate::geometry::meshopt::encode_indices(indices)?);
        self.blob = blob;
        Ok(())
    }
}

impl Section {
    pub fn name(&self) -> String {
        strip_nul(&self.name_raw)
    }
}

impl Geometry {
    pub fn parse(d: &[u8]) -> Result<Geometry> {
        let mut c = [0u32; 6];
        for (k, v) in c.iter_mut().enumerate() {
            *v = u32le(d, 4 * k, "geometry header")?;
        }
        let mut p = [0i64; 6];
        for (k, v) in p.iter_mut().enumerate() {
            *v = i64le(d, 0x18 + 8 * k, "geometry header")?;
        }
        let [nv, ni, nvm, nim, ncm, nam] = c.map(|x| x as usize);
        if nv > 4096 || ni > 4096 || nvm > 1 << 20 || nim > 1 << 20 || ncm > 4096 || nam > 4096 {
            return Err(Error::format("geometry header", format!("计数不合理: {c:?} (这不是 .geometry 文件?)")));
        }
        let mut g = Geometry::default();
        for k in 0..nvm {
            g.vertex_mappings.push(MappingEntry::parse(d, p[0] as usize + 16 * k)?);
        }
        for k in 0..nim {
            g.index_mappings.push(MappingEntry::parse(d, p[1] as usize + 16 * k)?);
        }
        for k in 0..nv {
            let b = p[2] as usize + 0x20 * k;
            let data = rel(b, i64le(d, b, "vproto")?, "vproto data")?;
            let name_len = u32le(d, b + 8, "vproto")? as usize;
            let name_pad = u32le(d, b + 12, "vproto")?;
            let name_off = rel(b + 8, i64le(d, b + 16, "vproto")?, "vproto name")?;
            let size = u32le(d, b + 0x18, "vproto")? as usize;
            let stride = u16le(d, b + 0x1C, "vproto")?;
            let skinned = slice(d, b + 0x1E, 1, "vproto")?[0];
            let bumped = slice(d, b + 0x1F, 1, "vproto")?[0];
            g.vertex_buffers.push(VertexBuffer {
                blob: slice(d, data, size, "vertex blob")?.to_vec(),
                format_raw: slice(d, name_off, name_len, "vertex format name")?.to_vec(),
                name_pad,
                stride,
                skinned,
                bumped,
            });
        }
        for k in 0..ni {
            let b = p[3] as usize + 0x10 * k;
            let data = rel(b, i64le(d, b, "iproto")?, "iproto data")?;
            let size = u32le(d, b + 8, "iproto")? as usize;
            let reserved = u16le(d, b + 12, "iproto")?;
            let index_size = u16le(d, b + 14, "iproto")?;
            g.index_buffers.push(IndexBuffer { blob: slice(d, data, size, "index blob")?.to_vec(), reserved, index_size });
        }
        for (cnt, ptr, which) in [(ncm, p[4], 0), (nam, p[5], 1)] {
            let base = ptr as usize;
            let mut cur = base + 0x20 * cnt;
            let mut v = Vec::with_capacity(cnt);
            for k in 0..cnt {
                let b = base + 0x20 * k;
                let r = i64le(d, b, "section proto")?;
                let name_len = u32le(d, b + 8, "section proto")? as usize;
                let name_pad = u32le(d, b + 12, "section proto")?;
                let name_off = rel(b + 8, i64le(d, b + 16, "section proto")?, "section name")?;
                let tail = u32le(d, b + 0x18, "section proto")?;
                let pad2 = u32le(d, b + 0x1C, "section proto")?;
                let end = rel(b, r, "section body")? + tail as usize;
                if name_off != end || end < cur {
                    return Err(Error::format(
                        "geometry section",
                        format!("碰撞/装甲段布局不是紧凑的(名字不紧跟数据), 暂不支持: name_off={name_off} end={end} cur={cur}"),
                    ));
                }
                let body = slice(d, cur, end - cur, "section body")?.to_vec();
                let name_raw = slice(d, name_off, name_len, "section name")?.to_vec();
                cur = end + name_len;
                v.push(Section { body, tail, name_raw, name_pad, pad2 });
            }
            if which == 0 {
                g.collision = v;
            } else {
                g.armor = v;
            }
        }
        Ok(g)
    }

    /// 以游戏的紧凑排布写出。对未修改的 Geometry, 输出与原文件逐字节一致(18 个样本验证)。
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = vec![0u8; 0x48];
        let vm = out.len();
        for m in &self.vertex_mappings {
            m.write(&mut out);
        }
        let im = out.len();
        for m in &self.index_mappings {
            m.write(&mut out);
        }
        let vp = out.len();
        out.resize(out.len() + 0x20 * self.vertex_buffers.len(), 0);
        for (k, v) in self.vertex_buffers.iter().enumerate() {
            let b = vp + 0x20 * k;
            let bo = out.len();
            out.extend_from_slice(&v.blob);
            let no = out.len();
            out.extend_from_slice(&v.format_raw);
            put_i64(&mut out, b, bo as i64 - b as i64);
            put_u32(&mut out, b + 8, v.format_raw.len() as u32);
            put_u32(&mut out, b + 12, v.name_pad);
            put_i64(&mut out, b + 16, no as i64 - (b + 8) as i64);
            put_u32(&mut out, b + 0x18, v.blob.len() as u32);
            out[b + 0x1C..b + 0x1E].copy_from_slice(&v.stride.to_le_bytes());
            out[b + 0x1E] = v.skinned;
            out[b + 0x1F] = v.bumped;
        }
        let ip = out.len();
        out.resize(out.len() + 0x10 * self.index_buffers.len(), 0);
        for (k, x) in self.index_buffers.iter().enumerate() {
            let b = ip + 0x10 * k;
            let bo = out.len();
            out.extend_from_slice(&x.blob);
            put_i64(&mut out, b, bo as i64 - b as i64);
            put_u32(&mut out, b + 8, x.blob.len() as u32);
            out[b + 12..b + 14].copy_from_slice(&x.reserved.to_le_bytes());
            out[b + 14..b + 16].copy_from_slice(&x.index_size.to_le_bytes());
        }
        let mut sec_ptr = [0usize; 2];
        for (si, secs) in [&self.collision, &self.armor].into_iter().enumerate() {
            sec_ptr[si] = if secs.is_empty() { 0 } else { out.len() };
            let base = out.len();
            out.resize(out.len() + 0x20 * secs.len(), 0);
            for (k, s) in secs.iter().enumerate() {
                let b = base + 0x20 * k;
                out.extend_from_slice(&s.body);
                let end = out.len();
                out.extend_from_slice(&s.name_raw);
                put_i64(&mut out, b, end as i64 - s.tail as i64 - b as i64);
                put_u32(&mut out, b + 8, s.name_raw.len() as u32);
                put_u32(&mut out, b + 12, s.name_pad);
                put_i64(&mut out, b + 16, end as i64 - (b + 8) as i64);
                put_u32(&mut out, b + 0x18, s.tail);
                put_u32(&mut out, b + 0x1C, s.pad2);
            }
        }
        let counts = [
            self.vertex_buffers.len(),
            self.index_buffers.len(),
            self.vertex_mappings.len(),
            self.index_mappings.len(),
            self.collision.len(),
            self.armor.len(),
        ];
        for (k, c) in counts.iter().enumerate() {
            put_u32(&mut out, 4 * k, *c as u32);
        }
        let ptrs = [vm, im, vp, ip, sec_ptr[0], sec_ptr[1]];
        for (k, p) in ptrs.iter().enumerate() {
            put_i64(&mut out, 0x18 + 8 * k, *p as i64);
        }
        out
    }

    pub fn vertex_mapping(&self, id: u32) -> Option<&MappingEntry> {
        self.vertex_mappings.iter().find(|m| m.id == id)
    }
    pub fn index_mapping(&self, id: u32) -> Option<&MappingEntry> {
        self.index_mappings.iter().find(|m| m.id == id)
    }
}

fn put_u32(out: &mut [u8], off: usize, v: u32) {
    out[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
fn put_i64(out: &mut [u8], off: usize, v: i64) {
    out[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

/// meshoptimizer 编解码封装(纯 Rust 的 meshopt-rs 0.1.2, MIT)。
pub mod meshopt {
    use crate::error::{Error, Result};
    use meshopt_rs::index::buffer::{decode_index_buffer, encode_index_buffer, encode_index_buffer_bound};
    use meshopt_rs::index::IndexEncodingVersion;
    use meshopt_rs::vertex::buffer::decode_vertex_buffer;

    // meshopt-rs 要求 size_of::<Vertex>() == stride; stride 只有运行时才知道,
    // 因此对 4..=256 中 4 的倍数逐一单态化(做法同 wows-toolkit geometry.rs, MIT)。
    macro_rules! with_stride {
        ($stride:expr, $body:ident) => {
            seq_strides!($stride, $body, 4 8 12 16 20 24 28 32 36 40 44 48 52 56 60 64 68 72 76 80 84 88 92 96 100 104 108 112 116 120 124 128)
        };
    }
    macro_rules! seq_strides {
        ($stride:expr, $body:ident, $($n:literal)*) => {
            match $stride {
                $( $n => { #[repr(C, align(4))] #[derive(Copy, Clone)] struct V([u8; $n]); $body!(V) } )*
                s => return Err(Error::Meshopt(format!("不支持的顶点 stride {s} (需 4 的倍数且 <= 128)"))),
            }
        };
    }

    pub fn decode_vertices(count: usize, stride: usize, enc: &[u8]) -> Result<Vec<u8>> {
        // Vec<u8> 只保证 1 字节对齐; 用 u32 缓冲保证 align(4), 再拷出
        let mut aligned = vec![0u32; (count * stride).div_ceil(4)];
        let ptr = aligned.as_mut_ptr() as *mut u8;
        macro_rules! go {
            ($V:ty) => {{
                // SAFETY: aligned 至少 count*stride 字节且 4 字节对齐, $V 大小 = stride, 对齐 4
                let dst: &mut [$V] = unsafe { std::slice::from_raw_parts_mut(ptr as *mut $V, count) };
                decode_vertex_buffer(dst, enc).map_err(|e| Error::Meshopt(format!("顶点解码: {e:?}")))?;
            }};
        }
        with_stride!(stride, go);
        let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, count * stride) }.to_vec();
        drop(aligned);
        Ok(bytes)
    }

    /// 顶点编码 v0: 使用本库移植的 meshoptimizer 1.3 编码器(与游戏文件逐字节一致)。
    pub fn encode_vertices(count: usize, stride: usize, raw: &[u8]) -> Result<Vec<u8>> {
        if stride == 0 || stride > 256 || stride % 4 != 0 || raw.len() != count * stride {
            return Err(Error::Meshopt(format!("顶点编码参数不合法: count={count} stride={stride} len={}", raw.len())));
        }
        Ok(crate::vertex_codec::encode_vertex_buffer_v0(raw, stride))
    }

    pub fn decode_indices(count: usize, index_size: u16, enc: &[u8]) -> Result<Vec<u32>> {
        let _ = index_size; // 解码统一输出 u32; index_size 只影响游戏内 GPU 缓冲格式
        let mut out = vec![0u32; count];
        decode_index_buffer(&mut out, enc).map_err(|e| Error::Meshopt(format!("索引解码: {e:?}")))?;
        Ok(out)
    }

    fn encode_indices_raw(indices: &[u32]) -> Result<Vec<u8>> {
        let vcount = indices.iter().copied().max().map(|m| m as usize + 1).unwrap_or(0);
        let mut buf = vec![0u8; encode_index_buffer_bound(indices.len(), vcount)];
        let n = encode_index_buffer(&mut buf, indices, IndexEncodingVersion::V1)
            .ok_or_else(|| Error::Meshopt("索引编码失败(缓冲不足或索引数不是 3 的倍数)".into()))?;
        buf.truncate(n);
        Ok(buf)
    }

    /// 索引编码 v1, 并尽量保证 decode(encode(x)) == x。
    ///
    /// meshopt 编码器会按 FIFO 状态轮换三角形顶点顺序(绕序不变)。对普通三角形, 游戏文件解码出的
    /// 顺序本身就是编码器的"不动点"; 但退化三角形(有重复顶点, 如 219,219,220)再编码时会被再次轮换。
    /// 这里检测解码不一致的三角形, 依次尝试另外两种轮换作为输入, 直到解码结果与输入完全一致。
    /// 实测这样得到的字节流与游戏文件逐字节一致(18 个样本, 含 6 个带退化三角形的文件)。
    ///
    /// 对任意新输入(例如 M2 导回的网格), 某些退化三角形的特定轮换可能是编码器"到不了"的
    /// (三种轮换都会被解码成另一种轮换)。这种三角形接受"轮换等价"(同一绕序), 不报错;
    /// 统计见 [`encode_indices_report`]。游戏原始数据本身由该编码器产生, 不会出现这种情况。
    pub fn encode_indices(indices: &[u32]) -> Result<Vec<u8>> {
        encode_indices_report(indices).map(|(e, _)| e)
    }

    fn rot_eq(a: &[u32], b: &[u32]) -> bool {
        a == b || (a[0] == b[1] && a[1] == b[2] && a[2] == b[0]) || (a[0] == b[2] && a[1] == b[0] && a[2] == b[1])
    }

    /// 同 [`encode_indices`], 另返回"只能轮换等价复现"的三角形数(游戏原始数据应为 0)。
    pub fn encode_indices_report(indices: &[u32]) -> Result<(Vec<u8>, usize)> {
        if indices.len() % 3 != 0 {
            return Err(Error::Meshopt(format!("索引数 {} 不是 3 的倍数", indices.len())));
        }
        let ntri = indices.len() / 3;
        // 快速路径: 一次编码。若有"非退化"三角形被轮换, 说明输入不是编码器的不动点(不是游戏原始顺序,
        // 例如新导入的网格), 逐个尝试轮换没有意义且是 O(n²): 直接接受轮换等价。
        {
            let enc = encode_indices_raw(indices)?;
            let mut dec = vec![0u32; indices.len()];
            decode_index_buffer(&mut dec, &enc).map_err(|e| Error::Meshopt(format!("索引自检解码: {e:?}")))?;
            let mut diff_plain = 0usize;
            let mut diff_any = 0usize;
            for t in 0..ntri {
                let (d, o) = (&dec[3 * t..3 * t + 3], &indices[3 * t..3 * t + 3]);
                if d != o {
                    if !rot_eq(d, o) {
                        return Err(Error::Meshopt(format!("三角形 {t} 编码后绕序改变, 无法编码")));
                    }
                    diff_any += 1;
                    if !(o[0] == o[1] || o[1] == o[2] || o[0] == o[2]) {
                        diff_plain += 1;
                    }
                }
            }
            if diff_any == 0 {
                return Ok((enc, 0));
            }
            if diff_plain > 0 {
                return Ok((enc, diff_any));
            }
        }
        let mut input = indices.to_vec();
        let mut tried_rot = std::collections::HashMap::<usize, u8>::new();
        let mut accepted = std::collections::HashSet::<usize>::new();
        for _ in 0..(ntri * 3 + 4) {
            let enc = encode_indices_raw(&input)?;
            let mut dec = vec![0u32; indices.len()];
            decode_index_buffer(&mut dec, &enc).map_err(|e| Error::Meshopt(format!("索引自检解码: {e:?}")))?;
            let bad = (0..ntri).find(|&t| {
                let (d, o) = (&dec[3 * t..3 * t + 3], &indices[3 * t..3 * t + 3]);
                if accepted.contains(&t) {
                    !rot_eq(d, o)
                } else {
                    d != o
                }
            });
            let Some(t) = bad else { return Ok((enc, accepted.len())) };
            let o = &indices[3 * t..3 * t + 3];
            if accepted.contains(&t) {
                return Err(Error::Meshopt(format!("三角形 {t} 编码后绕序改变, 无法编码")));
            }
            let r = tried_rot.entry(t).or_insert(0);
            *r += 1;
            if *r > 2 {
                if !rot_eq(&dec[3 * t..3 * t + 3], o) {
                    return Err(Error::Meshopt(format!("三角形 {t} 编码后绕序改变, 无法编码")));
                }
                // 三种轮换都复现不了原顺序: 接受轮换等价, 输入恢复为原顺序
                accepted.insert(t);
                input[3 * t..3 * t + 3].copy_from_slice(o);
                continue;
            }
            let k = *r as usize;
            input[3 * t] = o[k % 3];
            input[3 * t + 1] = o[(k + 1) % 3];
            input[3 * t + 2] = o[(k + 2) % 3];
        }
        Err(Error::Meshopt("索引编码迭代次数超限".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::meshopt::*;

    fn rot_eq(a: &[u32], b: &[u32]) -> bool {
        a == b || (a[0] == b[1] && a[1] == b[2] && a[2] == b[0]) || (a[0] == b[2] && a[1] == b[0] && a[2] == b[1])
    }

    #[test]
    fn index_roundtrip_with_degenerates() {
        // 含退化三角形(重复顶点)的索引: 编码后解码必须与输入完全一致(旋转修正)
        let mut idx: Vec<u32> = Vec::new();
        for i in 0..300u32 {
            idx.extend_from_slice(&[i, i + 1, i + 2]);
            if i % 17 == 0 {
                idx.extend_from_slice(&[i, i, i + 1]);
                idx.extend_from_slice(&[i + 2, i + 1, i + 2]);
            }
        }
        // 1) 任意输入: 每个三角形至少轮换等价(绕序不变)
        let (enc, rotated) = encode_indices_report(&idx).unwrap();
        let dec = decode_indices(idx.len(), 4, &enc).unwrap();
        for (t, (d, o)) in dec.chunks(3).zip(idx.chunks(3)).enumerate() {
            assert!(rot_eq(d, o), "三角形 {t} 绕序必须保持: {d:?} vs {o:?}");
        }
        eprintln!("任意输入中仅轮换等价复现的三角形: {rotated}");
        // 2) 编码器产出的数据(即游戏文件的情形): 再编码必须逐字节一致且 0 个轮换
        let (enc2, rotated2) = encode_indices_report(&dec).unwrap();
        assert_eq!(rotated2, 0);
        assert_eq!(decode_indices(dec.len(), 2, &enc2).unwrap(), dec);
        assert_eq!(enc2, enc, "对编码器不动点再编码应得到相同字节");
    }
}
