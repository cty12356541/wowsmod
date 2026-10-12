//! `.geometry` 编辑: 按渲染集名(= .visual 的 `XxxShape.vertices` / `XxxShape.indices`)
//! 变换顶点、追加新网格、删除映射, 再用与游戏一致的 meshopt 编码写回。
//!
//! 约定(26.10 实测, 见 M3_REPORT):
//! - 映射 id = MurmurHash3_x86_32("<名字>Shape.vertices" / ".indices", seed 0); 索引相对该顶点映射的起点。
//! - 法线/切线: 4 个无符号字节, 分量 = b/127.5 − 1; 法线 w=127; 切线 w ∈ {0, 254}: 254 表示副切线 = −(n×t)
//!   (按 UV 推导的副切线方向与 w 对照, 勃艮第全船 97–99% 一致)。
//! - UV: 2 × f16, 存储值 = 实际值 − 0.5。
//! - 蒙皮 `iiiww`: 8 字节(3 个骨骼索引 + 填充 + 权重…), 编码细节不完全清楚 → 新顶点一律**复制模板顶点的这 8 字节**(刚性绑定到模板所在骨骼)。
//! - `r` 字段(4 字节, 含义不确定)同样复制模板。
//! - 映射里的 f16 "texel density": 经验上 ≈ sqrt(UV 面积 / 世界面积)(约 1/3 的映射精确吻合, 其余偏差 < 2 倍),
//!   估计用于贴图流式加载的 mip 选择; 变换时按缩放比例更新, 新网格按此公式计算。
//! - 碰撞(collision)与装甲(armor)段原样保留。
use crate::error::{Error, Result};
use crate::geometry::{Geometry, IndexBuffer, MappingEntry, VertexBuffer};
use crate::murmur3::murmur3_32;
use crate::vertex::{parse_format, unpack_normal, Semantic, VertexFormat};
use crate::visual::{inverse, M4};

/// 渲染集名 → 映射名前缀。通常 `X` → `XShape`; 少数 LOD 用 `X_lodShape2.vertices` 这种写法,
/// 此时 base_name 为 `X_lodShape2`(以 Shape+数字结尾), 原样使用。
pub fn mapping_stem(name: &str) -> String {
    let t = name.trim_end_matches(|c: char| c.is_ascii_digit());
    if t.len() < name.len() && t.ends_with("Shape") {
        name.to_string()
    } else {
        format!("{name}Shape")
    }
}
pub fn vertices_id(name: &str) -> u32 {
    murmur3_32(format!("{}.vertices", mapping_stem(name)).as_bytes(), 0)
}
pub fn indices_id(name: &str) -> u32 {
    murmur3_32(format!("{}.indices", mapping_stem(name)).as_bytes(), 0)
}

pub fn pack_unit(v: [f32; 3], w: u8) -> [u8; 4] {
    let q = |x: f32| ((x.clamp(-1.0, 1.0) + 1.0) * 127.5).round().clamp(0.0, 255.0) as u8;
    [q(v[0]), q(v[1]), q(v[2]), w]
}

pub fn pack_uv(uv: [f32; 2]) -> [u8; 4] {
    let a = half::f16::from_f32(uv[0] - 0.5).to_bits().to_le_bytes();
    let b = half::f16::from_f32(uv[1] - 0.5).to_bits().to_le_bytes();
    [a[0], a[1], b[0], b[1]]
}

fn norm3(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l < 1e-20 {
        v
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}

/// 游戏坐标下的网格
#[derive(Debug, Clone, Default)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub uvs: Option<Vec<[f32; 2]>>,
    /// 局部索引(0..positions.len())
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn bbox(&self) -> Option<([f32; 3], [f32; 3])> {
        if self.positions.is_empty() {
            return None;
        }
        let mut mn = [f32::INFINITY; 3];
        let mut mx = [f32::NEG_INFINITY; 3];
        for p in &self.positions {
            for k in 0..3 {
                mn[k] = mn[k].min(p[k]);
                mx[k] = mx[k].max(p[k]);
            }
        }
        Some((mn, mx))
    }

    /// texel density 估计: sqrt(UV 面积 / 世界面积)
    pub fn texel_density(&self) -> f32 {
        let Some(uv) = &self.uvs else { return 1.0 };
        let (mut wa, mut ua) = (0f64, 0f64);
        for t in self.indices.chunks_exact(3) {
            let p = |i: u32| self.positions[i as usize].map(|x| x as f64);
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cr = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
            wa += 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
            let q = |i: u32| uv[i as usize].map(|x| x as f64);
            let (ua0, ub, uc) = (q(t[0]), q(t[1]), q(t[2]));
            ua += 0.5 * ((ub[0] - ua0[0]) * (uc[1] - ua0[1]) - (ub[1] - ua0[1]) * (uc[0] - ua0[0])).abs();
        }
        if wa <= 0.0 || ua <= 0.0 {
            1.0
        } else {
            (ua / wa).sqrt() as f32
        }
    }

    /// 平滑法线(面积加权), 没有法线时用
    pub fn compute_normals(&mut self) {
        let mut n = vec![[0f64; 3]; self.positions.len()];
        for t in self.indices.chunks_exact(3) {
            let p = |i: u32| self.positions[i as usize].map(|x| x as f64);
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            // e1×e2 与文件中存的法线同向(Bourgogne 全船 + 炮塔实测 98–99% 顶点点积 > 0, 见 M3_REPORT)
            let cr = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
            for &i in t {
                for k in 0..3 {
                    n[i as usize][k] += cr[k];
                }
            }
        }
        self.normals = Some(n.into_iter().map(|v| norm3(v).map(|x| x as f32)).collect());
    }

    /// 由 UV 求每顶点切线(MikkTSpace 的简化版)。返回 (切线, 符号 +1/-1)
    pub fn compute_tangents(&self) -> Vec<([f32; 3], f32)> {
        let n = self.positions.len();
        let mut tan = vec![[0f64; 3]; n];
        let mut bit = vec![[0f64; 3]; n];
        if let Some(uv) = &self.uvs {
            for t in self.indices.chunks_exact(3) {
                let p = |i: u32| self.positions[i as usize].map(|x| x as f64);
                let q = |i: u32| uv[i as usize].map(|x| x as f64);
                let (p0, p1, p2) = (p(t[0]), p(t[1]), p(t[2]));
                let (w0, w1, w2) = (q(t[0]), q(t[1]), q(t[2]));
                let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
                let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
                let (s1, t1, s2, t2) = (w1[0] - w0[0], w1[1] - w0[1], w2[0] - w0[0], w2[1] - w0[1]);
                let d = s1 * t2 - s2 * t1;
                if d.abs() < 1e-20 {
                    continue;
                }
                let r = 1.0 / d;
                let sd = [(t2 * e1[0] - t1 * e2[0]) * r, (t2 * e1[1] - t1 * e2[1]) * r, (t2 * e1[2] - t1 * e2[2]) * r];
                let td = [(s1 * e2[0] - s2 * e1[0]) * r, (s1 * e2[1] - s2 * e1[1]) * r, (s1 * e2[2] - s2 * e1[2]) * r];
                for &i in t {
                    for k in 0..3 {
                        tan[i as usize][k] += sd[k];
                        bit[i as usize][k] += td[k];
                    }
                }
            }
        }
        let normals = self.normals.clone().unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; n]);
        (0..n)
            .map(|i| {
                let nn = normals[i].map(|x| x as f64);
                let t = tan[i];
                let d = nn[0] * t[0] + nn[1] * t[1] + nn[2] * t[2];
                let mut o = norm3([t[0] - nn[0] * d, t[1] - nn[1] * d, t[2] - nn[2] * d]);
                if (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]) < 0.5 {
                    // 没有可用 UV: 取任一垂直方向
                    let a = if nn[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
                    o = norm3([a[1] * nn[2] - a[2] * nn[1], a[2] * nn[0] - a[0] * nn[2], a[0] * nn[1] - a[1] * nn[0]]);
                }
                let c = [nn[1] * o[2] - nn[2] * o[1], nn[2] * o[0] - nn[0] * o[2], nn[0] * o[1] - nn[1] * o[0]];
                let b = bit[i];
                let s = if c[0] * b[0] + c[1] * b[1] + c[2] * b[2] < 0.0 { -1.0 } else { 1.0 };
                (o.map(|x| x as f32), s)
            })
            .collect()
    }
}

/// 可编辑的 geometry: 缓冲区按需解码, 改过的才重新编码
pub struct GeoEdit {
    pub geo: Geometry,
    vb: Vec<Option<Vec<u8>>>,
    ib: Vec<Option<Vec<u32>>>,
    vdirty: Vec<bool>,
    idirty: Vec<bool>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MeshInfo {
    pub name: String,
    pub format: String,
    pub buffer: u16,
    pub vertex_offset: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub texel_density: f32,
}

impl GeoEdit {
    pub fn new(geo: Geometry) -> Self {
        let (nv, ni) = (geo.vertex_buffers.len(), geo.index_buffers.len());
        GeoEdit { geo, vb: vec![None; nv], ib: vec![None; ni], vdirty: vec![false; nv], idirty: vec![false; ni] }
    }
    pub fn parse(d: &[u8]) -> Result<Self> {
        Ok(Self::new(Geometry::parse(d)?))
    }

    fn vbuf(&mut self, k: usize) -> Result<&mut Vec<u8>> {
        if self.vb[k].is_none() {
            self.vb[k] = Some(self.geo.vertex_buffers[k].decode()?);
        }
        Ok(self.vb[k].as_mut().unwrap())
    }
    fn ibuf(&mut self, k: usize) -> Result<&mut Vec<u32>> {
        if self.ib[k].is_none() {
            self.ib[k] = Some(self.geo.index_buffers[k].decode()?);
        }
        Ok(self.ib[k].as_mut().unwrap())
    }

    pub fn has(&self, name: &str) -> bool {
        self.geo.vertex_mapping(vertices_id(name)).is_some()
    }

    fn maps(&self, name: &str) -> Result<(MappingEntry, MappingEntry)> {
        let v = *self.geo.vertex_mapping(vertices_id(name)).ok_or_else(|| Error::NotFound(format!("geometry 中没有 {name}Shape.vertices")))?;
        let i = *self.geo.index_mapping(indices_id(name)).ok_or_else(|| Error::NotFound(format!("geometry 中没有 {name}Shape.indices")))?;
        Ok((v, i))
    }

    pub fn info(&self, name: &str) -> Result<MeshInfo> {
        let (v, i) = self.maps(name)?;
        Ok(MeshInfo {
            name: name.to_string(),
            format: self.geo.vertex_buffers[v.buffer_index as usize].format(),
            buffer: v.buffer_index,
            vertex_offset: v.items_offset,
            vertex_count: v.items_count,
            index_count: i.items_count,
            texel_density: v.texel_density(),
        })
    }

    pub fn format_of(&self, name: &str) -> Result<VertexFormat> {
        let (v, _) = self.maps(name)?;
        Ok(parse_format(&self.geo.vertex_buffers[v.buffer_index as usize].format()))
    }

    /// 读出一个渲染集(游戏坐标)
    pub fn mesh(&mut self, name: &str) -> Result<MeshData> {
        let (v, i) = self.maps(name)?;
        let k = v.buffer_index as usize;
        let stride = self.geo.vertex_buffers[k].stride as usize;
        let fmt = parse_format(&self.geo.vertex_buffers[k].format());
        let raw = self.vbuf(k)?[v.items_offset as usize * stride..(v.items_offset + v.items_count) as usize * stride].to_vec();
        let idx = self.ibuf(i.buffer_index as usize)?[i.items_offset as usize..(i.items_offset + i.items_count) as usize].to_vec();
        if idx.iter().any(|&x| x >= v.items_count) {
            return Err(Error::format("geometry", format!("{name}: 索引越界")));
        }
        let (u, _) = crate::vertex::unpack(&raw, stride, &fmt);
        Ok(MeshData { positions: u.positions, normals: u.normals, uvs: u.uvs, indices: idx })
    }

    /// 就地把指定三角形改成退化三角形(三个索引都取该三角形第一个顶点): 顶点缓冲/计数/映射全不变, 只改索引缓冲。
    /// tris = 渲染集内三角形序号。返回实际改动数。
    pub fn degenerate_tris(&mut self, name: &str, tris: &[usize]) -> Result<usize> {
        let (_, i) = self.maps(name)?;
        let k = i.buffer_index as usize;
        let off = i.items_offset as usize;
        let n = i.items_count as usize / 3;
        let buf = self.ibuf(k)?;
        let mut c = 0;
        for &t in tris {
            if t >= n { continue; }
            let a = buf[off + 3 * t];
            if buf[off + 3 * t + 1] != a || buf[off + 3 * t + 2] != a {
                buf[off + 3 * t + 1] = a;
                buf[off + 3 * t + 2] = a;
                c += 1;
            }
        }
        if c > 0 { self.idirty[k] = true; }
        Ok(c)
    }

    /// 原始顶点字节(用作新顶点的模板)
    pub fn raw_vertex(&mut self, name: &str, i: usize) -> Result<Vec<u8>> {
        let (v, _) = self.maps(name)?;
        let k = v.buffer_index as usize;
        let stride = self.geo.vertex_buffers[k].stride as usize;
        if i >= v.items_count as usize {
            return Err(Error::NotFound(format!("{name} 顶点 {i}")));
        }
        let o = (v.items_offset as usize + i) * stride;
        Ok(self.vbuf(k)?[o..o + stride].to_vec())
    }

    /// 对一个渲染集做仿射变换(行向量约定 p' = [p,1]·M)。法线用逆转置, 切线用 M 的 3×3。
    /// 返回变换的顶点数。texel density 按 |det|^(1/3) 缩放。
    pub fn transform(&mut self, name: &str, m: &M4) -> Result<usize> {
        let (v, i) = self.maps(name)?;
        let k = v.buffer_index as usize;
        let stride = self.geo.vertex_buffers[k].stride as usize;
        let fmt = parse_format(&self.geo.vertex_buffers[k].format());
        if fmt.stride != stride {
            return Err(Error::Unsupported(format!("顶点格式 {} stride 不一致", fmt.code)));
        }
        let inv = inverse(m).ok_or_else(|| Error::Unsupported("变换矩阵不可逆".into()))?;
        let nrm_m = crate::visual::transpose(&inv); // 行向量约定下 n' = n·(M⁻¹)ᵀ
        let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
        let noff = fmt.find(Semantic::Normal).map(|a| a.offset);
        let toff = fmt.find(Semantic::Tangent).map(|a| a.offset);
        let boff = fmt.find(Semantic::Binormal).map(|a| a.offset);
        let buf = self.vbuf(k)?;
        for vi in v.items_offset as usize..(v.items_offset + v.items_count) as usize {
            let o = vi * stride;
            let p: [f64; 3] = std::array::from_fn(|c| f32::from_le_bytes(buf[o + 4 * c..o + 4 * c + 4].try_into().unwrap()) as f64);
            let q = crate::visual::xform_point(m, p);
            for c in 0..3 {
                buf[o + 4 * c..o + 4 * c + 4].copy_from_slice(&(q[c] as f32).to_le_bytes());
            }
            if let Some(no) = noff {
                let b: [u8; 4] = buf[o + no..o + no + 4].try_into().unwrap();
                let n = unpack_normal(b).map(|x| x as f64);
                let n2 = norm3(crate::visual::xform_dir(&nrm_m, n));
                buf[o + no..o + no + 4].copy_from_slice(&pack_unit(n2.map(|x| x as f32), b[3]));
            }
            for off in [toff, boff].into_iter().flatten() {
                let b: [u8; 4] = buf[o + off..o + off + 4].try_into().unwrap();
                let t = unpack_normal(b).map(|x| x as f64);
                let t2 = norm3(crate::visual::xform_dir(m, t));
                let w = if det < 0.0 { if b[3] == 0 { 254 } else if b[3] == 254 { 0 } else { b[3] } } else { b[3] };
                buf[o + off..o + off + 4].copy_from_slice(&pack_unit(t2.map(|x| x as f32), w));
            }
        }
        self.vdirty[k] = true;
        // 行列式为负(镜像)时需翻转绕序
        if det < 0.0 {
            let ib = self.ibuf(i.buffer_index as usize)?;
            for t in ib[i.items_offset as usize..(i.items_offset + i.items_count) as usize].chunks_exact_mut(3) {
                t.swap(1, 2);
            }
            self.idirty[i.buffer_index as usize] = true;
        }
        let s = det.abs().cbrt();
        if s > 0.0 {
            for list in [&mut self.geo.vertex_mappings, &mut self.geo.index_mappings] {
                for e in list.iter_mut().filter(|e| e.id == v.id || e.id == i.id) {
                    let td = half::f16::from_bits(e.packed_texel_density).to_f32() / s as f32;
                    e.packed_texel_density = half::f16::from_f32(td).to_bits();
                }
            }
        }
        Ok(v.items_count as usize)
    }

    /// 追加一个新渲染集。`format` 为顶点格式名(如 "set3/xyznuvtpc"); 没有该格式的缓冲时新建一个。
    /// `template`: 一个原始顶点(stride 字节), 蒙皮/`r` 等无法从网格得到的字段从它复制。
    pub fn add_mesh(&mut self, name: &str, format: &str, mesh: &MeshData, template: Option<&[u8]>) -> Result<MeshInfo> {
        self.add_mesh_tpl(name, format, mesh, template, None)
    }

    /// 同 add_mesh, 但可给出逐顶点模板(长度须等于顶点数; 用于保持蒙皮权重等逐顶点字段)
    pub fn add_mesh_tpl(&mut self, name: &str, format: &str, mesh: &MeshData, template: Option<&[u8]>, per_vertex: Option<&[Vec<u8>]>) -> Result<MeshInfo> {
        if let Some(pv) = per_vertex {
            if pv.len() != mesh.positions.len() {
                return Err(Error::format("template", format!("{name}: 逐顶点模板 {} 个, 顶点 {} 个", pv.len(), mesh.positions.len())));
            }
        }
        let template: Option<&[u8]> = template.or_else(|| per_vertex.and_then(|p| p.first().map(|v| v.as_slice())));
        if self.has(name) || self.geo.index_mapping(indices_id(name)).is_some() {
            return Err(Error::format("geometry", format!("{name} 已存在(渲染集名必须唯一)")));
        }
        if mesh.positions.len() > 65535 {
            return Err(Error::Unsupported(format!("{name}: {} 个顶点超过 16 位索引上限 65535, 请拆分", mesh.positions.len())));
        }
        if mesh.indices.len() % 3 != 0 || mesh.indices.iter().any(|&x| x as usize >= mesh.positions.len()) {
            return Err(Error::format("mesh", format!("{name}: 索引数不是 3 的倍数或越界")));
        }
        let fmt = parse_format(format);
        let mut m = mesh.clone();
        if m.normals.is_none() {
            m.compute_normals();
        }
        let tangents = m.compute_tangents();
        let k = match self.geo.vertex_buffers.iter().position(|b| b.format() == fmt_name(format)) {
            Some(k) => k,
            None => {
                let mut raw = fmt_name(format).into_bytes();
                raw.push(0);
                let skinned = 0; // 26.10 实测所有缓冲的 skinned/bumped 标志都是 0
                self.geo.vertex_buffers.push(VertexBuffer { blob: vec![], format_raw: raw, name_pad: 0, stride: fmt.stride as u16, skinned, bumped: 0 });
                self.vb.push(Some(vec![]));
                self.vdirty.push(true);
                self.geo.vertex_buffers.len() - 1
            }
        };
        let stride = self.geo.vertex_buffers[k].stride as usize;
        if fmt.stride != stride {
            return Err(Error::Unsupported(format!("格式 {format} 解析 stride {} 与缓冲 {stride} 不一致", fmt.stride)));
        }
        if let Some(t) = template {
            if t.len() != stride {
                return Err(Error::format("template", format!("模板顶点 {} 字节, 需要 {stride}", t.len())));
            }
        }
        let normals = m.normals.clone().unwrap();
        let mut bytes = Vec::with_capacity(stride * m.positions.len());
        for (vi, p) in m.positions.iter().enumerate() {
            let mut v = match per_vertex {
                Some(pv) if pv[vi].len() == stride => pv[vi].clone(),
                _ => template.map(|t| t.to_vec()).unwrap_or_else(|| vec![0u8; stride]),
            };
            for c in 0..3 {
                v[4 * c..4 * c + 4].copy_from_slice(&p[c].to_le_bytes());
            }
            for a in &fmt.attributes {
                let o = a.offset;
                match a.semantic {
                    Semantic::Position => {}
                    Semantic::Normal => v[o..o + 4].copy_from_slice(&pack_unit(normals[vi], 127)),
                    Semantic::TexCoord0 | Semantic::TexCoord1 => {
                        let uv = m.uvs.as_ref().map(|u| u[vi]).unwrap_or([0.5, 0.5]);
                        v[o..o + 4].copy_from_slice(&pack_uv(uv));
                    }
                    Semantic::Tangent => {
                        let (t, s) = tangents[vi];
                        v[o..o + 4].copy_from_slice(&pack_unit(t, if s > 0.0 { 0 } else { 254 }));
                    }
                    Semantic::Binormal => {
                        let (t, s) = tangents[vi];
                        let n = normals[vi];
                        let b = [
                            (n[1] * t[2] - n[2] * t[1]) * s,
                            (n[2] * t[0] - n[0] * t[2]) * s,
                            (n[0] * t[1] - n[1] * t[0]) * s,
                        ];
                        v[o..o + 4].copy_from_slice(&pack_unit(b, 127));
                    }
                    Semantic::Skin | Semantic::Extra => {
                        if template.is_none() {
                            return Err(Error::Unsupported(format!("格式 {format} 含蒙皮/额外字段, 新网格需要模板顶点")));
                        }
                    }
                }
            }
            bytes.extend_from_slice(&v);
        }
        let buf = self.vbuf(k)?;
        let voff = (buf.len() / stride) as u32;
        buf.extend_from_slice(&bytes);
        self.vdirty[k] = true;
        let ik = 0usize;
        if self.geo.index_buffers.is_empty() {
            self.geo.index_buffers.push(IndexBuffer { blob: vec![], reserved: 0, index_size: 2 });
            self.ib.push(Some(vec![]));
            self.idirty.push(true);
        }
        let isz = self.geo.index_buffers[ik].index_size;
        if isz == 2 && m.positions.len() > 65536 {
            return Err(Error::Unsupported("16 位索引".into()));
        }
        let ib = self.ibuf(ik)?;
        let ioff = ib.len() as u32;
        ib.extend_from_slice(&m.indices);
        self.idirty[ik] = true;
        let td = half::f16::from_f32(m.texel_density()).to_bits();
        self.geo.vertex_mappings.push(MappingEntry { id: vertices_id(name), buffer_index: k as u16, packed_texel_density: td, items_offset: voff, items_count: m.positions.len() as u32 });
        self.geo.index_mappings.push(MappingEntry { id: indices_id(name), buffer_index: ik as u16, packed_texel_density: td, items_offset: ioff, items_count: m.indices.len() as u32 });
        self.info(name)
    }

    /// 删除一个渲染集的映射(顶点/索引数据留在缓冲里, 不再被引用)。
    pub fn remove(&mut self, name: &str) -> Result<()> {
        let (v, i) = self.maps(name)?;
        self.geo.vertex_mappings.retain(|e| e.id != v.id);
        self.geo.index_mappings.retain(|e| e.id != i.id);
        Ok(())
    }

    /// 用新数据整体替换一个已有渲染集的网格(保持名字; 旧数据留在缓冲里)
    pub fn replace_mesh(&mut self, name: &str, mesh: &MeshData, template: Option<&[u8]>) -> Result<MeshInfo> {
        let format = self.info(name)?.format;
        let tpl = match template {
            Some(t) => Some(t.to_vec()),
            None => Some(self.raw_vertex(name, 0)?),
        };
        self.remove(name)?;
        self.add_mesh(name, &format, mesh, tpl.as_deref())
    }

/// 替换网格, 逐顶点从原网格找模板顶点(保留蒙皮/额外字节): 先按位置精确匹配(量化 1e-5), 匹配不到的取最近的原顶点。
    /// 返回 (信息, 精确匹配数, 最近邻匹配数)。用于 Blender 往返(顶点顺序/数量可能变化)。
    pub fn replace_mesh_matched(&mut self, name: &str, mesh: &MeshData) -> Result<(MeshInfo, usize, usize)> {
        let info = self.info(name)?;
        let old = self.mesh(name)?;
        let q = |p: &[f32; 3]| -> (i64, i64, i64) { ((p[0] as f64 * 1e5).round() as i64, (p[1] as f64 * 1e5).round() as i64, (p[2] as f64 * 1e5).round() as i64) };
        let mut map: std::collections::HashMap<(i64, i64, i64), usize> = std::collections::HashMap::new();
        for (i, p) in old.positions.iter().enumerate() {
            map.entry(q(p)).or_insert(i);
        }
        let n_old = old.positions.len();
        let mut src_idx = Vec::with_capacity(mesh.positions.len());
        let (mut exact, mut near) = (0, 0);
        let unmatched = mesh.positions.iter().filter(|p| !map.contains_key(&q(p))).count();
        let brute = unmatched.saturating_mul(n_old) <= 80_000_000;
        for p in &mesh.positions {
            if let Some(&i) = map.get(&q(p)) {
                src_idx.push(i);
                exact += 1;
            } else if brute {
                let mut best = (f32::MAX, 0usize);
                for (i, o) in old.positions.iter().enumerate() {
                    let d = (o[0] - p[0]).powi(2) + (o[1] - p[1]).powi(2) + (o[2] - p[2]).powi(2);
                    if d < best.0 {
                        best = (d, i);
                    }
                }
                src_idx.push(best.1);
                near += 1;
            } else {
                src_idx.push(0);
                near += 1;
            }
        }
        let mut raws = Vec::with_capacity(n_old);
        for i in 0..n_old {
            raws.push(self.raw_vertex(name, i)?);
        }
        let pv: Vec<Vec<u8>> = src_idx.iter().map(|&i| raws[i].clone()).collect();
        self.remove(name)?;
        let mi = self.add_mesh_tpl(name, &info.format, mesh, Some(&raws[0]), Some(&pv))?;
        Ok((mi, exact, near))
    }

    /// 替换网格; 顶点数与原来相同时逐顶点保留原始的蒙皮/额外字节(按下标), 否则用第 0 个顶点做模板
    pub fn replace_mesh_keep(&mut self, name: &str, mesh: &MeshData) -> Result<MeshInfo> {
        let info = self.info(name)?;
        let pv: Option<Vec<Vec<u8>>> = if info.vertex_count as usize == mesh.positions.len() {
            Some((0..mesh.positions.len()).map(|i| self.raw_vertex(name, i)).collect::<Result<Vec<_>>>()?)
        } else {
            None
        };
        let t0 = self.raw_vertex(name, 0)?;
        self.remove(name)?;
        self.add_mesh_tpl(name, &info.format, mesh, Some(&t0), pv.as_deref())
    }

    /// 重新编码改动过的缓冲, 返回 Geometry
    pub fn finish(mut self) -> Result<Geometry> {
        for k in 0..self.vb.len() {
            if self.vdirty[k] {
                let raw = self.vb[k].take().unwrap();
                self.geo.vertex_buffers[k].encode_from(&raw)?;
            }
        }
        for k in 0..self.ib.len() {
            if self.idirty[k] {
                let raw = self.ib[k].take().unwrap();
                self.geo.index_buffers[k].encode_from(&raw)?;
            }
        }
        Ok(self.geo)
    }

    pub fn names_known(&self) -> usize {
        self.geo.vertex_mappings.len()
    }
}

fn fmt_name(f: &str) -> String {
    if f.contains('/') {
        f.to_string()
    } else {
        format!("set3/{f}")
    }
}

/// 从零重建: 把一个 geometry 的全部渲染集(按名字列表)解码后用编码器重新写出(新缓冲、新映射),
/// 碰撞/装甲原样保留。用于验证编码器("等价重建")。
pub fn rebuild_from_meshes(src: &Geometry, names: &[String]) -> Result<(Geometry, Vec<String>)> {
    rebuild_from_meshes_opts(src, names, true)
}

/// `keep_tangents = false` 时切线/副法线也由编码器重新计算(更严格地检验编码器)。
pub fn rebuild_from_meshes_opts(src: &Geometry, names: &[String], keep_tangents: bool) -> Result<(Geometry, Vec<String>)> {
    let mut g = GeoEdit::new(src.clone());
    let mut fresh = GeoEdit::new(Geometry { collision: src.collision.clone(), armor: src.armor.clone(), ..Default::default() });
    let mut warn = Vec::new();
    for n in names {
        if !g.has(n) {
            warn.push(format!("{n}: 不在 geometry 中, 跳过"));
            continue;
        }
        let info = g.info(n)?;
        let mesh = g.mesh(n)?;
        let tpl = g.raw_vertex(n, 0)?;
        // 逐顶点保留原始的蒙皮/r/切线字节: 先按编码器生成, 再把编码器无法推导的字段拷回
        let fmt = parse_format(&info.format);
        fresh.add_mesh(n, &info.format, &mesh, Some(&tpl))?;
        let (v, _) = fresh.maps(n)?;
        let stride = fmt.stride;
        let k = v.buffer_index as usize;
        let mut orig = Vec::with_capacity(stride * info.vertex_count as usize);
        for i in 0..info.vertex_count as usize {
            orig.extend_from_slice(&g.raw_vertex(n, i)?);
        }
        let buf = fresh.vbuf(k)?;
        for i in 0..info.vertex_count as usize {
            let o = (v.items_offset as usize + i) * stride;
            for a in &fmt.attributes {
                if matches!(a.semantic, Semantic::Skin | Semantic::Extra) || (keep_tangents && matches!(a.semantic, Semantic::Tangent | Semantic::Binormal)) {
                    buf[o + a.offset..o + a.offset + a.size].copy_from_slice(&orig[i * stride + a.offset..i * stride + a.offset + a.size]);
                }
            }
            // 法线的 w 字节也保留
            if let Some(a) = fmt.find(Semantic::Normal) {
                buf[o + a.offset + 3] = orig[i * stride + a.offset + 3];
            }
        }
        // texel density 用原值(公式只是近似)
        let td = src.vertex_mapping(vertices_id(n)).map(|e| e.packed_texel_density).unwrap_or(0);
        for list in [&mut fresh.geo.vertex_mappings, &mut fresh.geo.index_mappings] {
            for e in list.iter_mut().filter(|e| e.id == vertices_id(n) || e.id == indices_id(n)) {
                e.packed_texel_density = td;
            }
        }
    }
    // 没有被任何名字引用的映射: 原始字节原样搬进新缓冲(不经编码器重算), 保证不丢数据
    let vnamed: std::collections::HashSet<u32> = fresh.geo.vertex_mappings.iter().map(|e| e.id).collect();
    let inamed: std::collections::HashSet<u32> = fresh.geo.index_mappings.iter().map(|e| e.id).collect();
    let mut raw_v = 0;
    for e in src.vertex_mappings.clone() {
        if vnamed.contains(&e.id) {
            continue;
        }
        let k = e.buffer_index as usize;
        let stride = src.vertex_buffers[k].stride as usize;
        let fmt = src.vertex_buffers[k].format();
        let bytes = g.vbuf(k)?[e.items_offset as usize * stride..(e.items_offset + e.items_count) as usize * stride].to_vec();
        let fk = match fresh.geo.vertex_buffers.iter().position(|b| b.format() == fmt) {
            Some(x) => x,
            None => {
                let vb = &src.vertex_buffers[k];
                fresh.geo.vertex_buffers.push(VertexBuffer { blob: vec![], format_raw: vb.format_raw.clone(), name_pad: vb.name_pad, stride: vb.stride, skinned: vb.skinned, bumped: vb.bumped });
                fresh.vb.push(Some(vec![]));
                fresh.vdirty.push(true);
                fresh.geo.vertex_buffers.len() - 1
            }
        };
        let buf = fresh.vbuf(fk)?;
        let off = (buf.len() / stride) as u32;
        buf.extend_from_slice(&bytes);
        fresh.vdirty[fk] = true;
        fresh.geo.vertex_mappings.push(MappingEntry { id: e.id, buffer_index: fk as u16, packed_texel_density: e.packed_texel_density, items_offset: off, items_count: e.items_count });
        raw_v += 1;
    }
    let mut raw_i = 0;
    for e in src.index_mappings.clone() {
        if inamed.contains(&e.id) {
            continue;
        }
        let idx = g.ibuf(e.buffer_index as usize)?[e.items_offset as usize..(e.items_offset + e.items_count) as usize].to_vec();
        if fresh.geo.index_buffers.is_empty() {
            fresh.geo.index_buffers.push(IndexBuffer { blob: vec![], reserved: 0, index_size: 2 });
            fresh.ib.push(Some(vec![]));
            fresh.idirty.push(true);
        }
        let ib = fresh.ibuf(0)?;
        let off = ib.len() as u32;
        ib.extend_from_slice(&idx);
        fresh.idirty[0] = true;
        fresh.geo.index_mappings.push(MappingEntry { id: e.id, buffer_index: 0, packed_texel_density: e.packed_texel_density, items_offset: off, items_count: e.items_count });
        raw_i += 1;
    }
    if raw_v + raw_i > 0 {
        warn.push(format!("{raw_v} 个顶点映射 / {raw_i} 个索引映射没有对应的渲染集名, 原始字节原样搬入(未经编码器重算)"));
    }
    Ok((fresh.finish()?, warn))
}

/// 两个 geometry 按映射 id 逐项比较(用于验证重建/写回): 位置逐位、UV(f16)逐位、法线/切线打包字节差、其余字节、
/// 索引(允许三角形顶点轮转, meshopt 编码会轮转)、碰撞/装甲字节。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct GeoCompare {
    pub vertex_mappings: usize,
    pub missing_in_b: usize,
    pub vertices: usize,
    pub position_bit_diff: usize,
    pub uv_bit_diff: usize,
    pub normal_max_byte_diff: u8,
    pub tangent_max_byte_diff: u8,
    pub other_byte_diff: usize,
    pub index_mappings: usize,
    pub triangles: usize,
    pub triangles_rotated: usize,
    pub triangles_diff: usize,
    pub collision_equal: bool,
    pub armor_equal: bool,
}

impl GeoCompare {
    pub fn equivalent(&self) -> bool {
        self.missing_in_b == 0 && self.position_bit_diff == 0 && self.uv_bit_diff == 0 && self.other_byte_diff == 0 && self.triangles_diff == 0 && self.collision_equal && self.armor_equal
    }
}

pub fn compare_geometry(a: &Geometry, b: &Geometry) -> Result<GeoCompare> {
    let mut ga = GeoEdit::new(a.clone());
    let mut gb = GeoEdit::new(b.clone());
    let mut r = GeoCompare { collision_equal: a.collision == b.collision, armor_equal: a.armor == b.armor, ..Default::default() };
    for e in &a.vertex_mappings {
        r.vertex_mappings += 1;
        let Some(f) = b.vertex_mapping(e.id).copied() else {
            r.missing_in_b += 1;
            continue;
        };
        let ka = e.buffer_index as usize;
        let kb = f.buffer_index as usize;
        let fmt = parse_format(&a.vertex_buffers[ka].format());
        let stride = fmt.stride;
        if b.vertex_buffers[kb].format() != a.vertex_buffers[ka].format() || e.items_count != f.items_count {
            r.missing_in_b += 1;
            continue;
        }
        let va = ga.vbuf(ka)?[e.items_offset as usize * stride..(e.items_offset + e.items_count) as usize * stride].to_vec();
        let vb = gb.vbuf(kb)?[f.items_offset as usize * stride..(f.items_offset + f.items_count) as usize * stride].to_vec();
        for i in 0..e.items_count as usize {
            r.vertices += 1;
            let (x, y) = (&va[i * stride..(i + 1) * stride], &vb[i * stride..(i + 1) * stride]);
            for at in &fmt.attributes {
                let (o, n) = (at.offset, at.size);
                let (p, q) = (&x[o..o + n], &y[o..o + n]);
                match at.semantic {
                    Semantic::Position => r.position_bit_diff += (p != q) as usize,
                    Semantic::TexCoord0 | Semantic::TexCoord1 => r.uv_bit_diff += (p != q) as usize,
                    Semantic::Normal => {
                        let d = p.iter().zip(q).map(|(u, v)| u.abs_diff(*v)).max().unwrap_or(0);
                        r.normal_max_byte_diff = r.normal_max_byte_diff.max(d);
                    }
                    Semantic::Tangent | Semantic::Binormal => {
                        let d = p.iter().zip(q).map(|(u, v)| u.abs_diff(*v)).max().unwrap_or(0);
                        r.tangent_max_byte_diff = r.tangent_max_byte_diff.max(d);
                    }
                    _ => r.other_byte_diff += (p != q) as usize,
                }
            }
        }
    }
    for e in &a.index_mappings {
        r.index_mappings += 1;
        let Some(f) = b.index_mapping(e.id).copied() else {
            r.missing_in_b += 1;
            continue;
        };
        let ia = ga.ibuf(e.buffer_index as usize)?[e.items_offset as usize..(e.items_offset + e.items_count) as usize].to_vec();
        let ib = gb.ibuf(f.buffer_index as usize)?[f.items_offset as usize..(f.items_offset + f.items_count) as usize].to_vec();
        if ia.len() != ib.len() {
            r.triangles_diff += ia.len().max(ib.len()) / 3;
            continue;
        }
        for (t, u) in ia.chunks_exact(3).zip(ib.chunks_exact(3)) {
            r.triangles += 1;
            if t == u {
                continue;
            }
            if (t[0] == u[1] && t[1] == u[2] && t[2] == u[0]) || (t[0] == u[2] && t[1] == u[0] && t[2] == u[1]) {
                r.triangles_rotated += 1;
            } else {
                r.triangles_diff += 1;
            }
        }
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertex::unpack_uv;

    #[test]
    fn mapping_names() {
        assert_eq!(mapping_stem("Turret"), "TurretShape");
        assert_eq!(mapping_stem("BD018_Ster_Sell_Sight_lodShape2"), "BD018_Ster_Sell_Sight_lodShape2");
        assert_eq!(vertices_id("X_lodShape2"), crate::murmur3::murmur3_32(b"X_lodShape2.vertices", 0));
        assert_eq!(indices_id("Turret"), crate::murmur3::murmur3_32(b"TurretShape.indices", 0));
    }
    #[test]
    fn pack_roundtrip() {
        let n = [0.3f32, -0.5, 0.81];
        let b = pack_unit(n, 127);
        let u = unpack_normal(b);
        for k in 0..3 {
            assert!((u[k] - n[k]).abs() < 0.005);
        }
        let uv = [0.25f32, 0.75];
        let r = unpack_uv(pack_uv(uv));
        assert!((r[0] - 0.25).abs() < 1e-3 && (r[1] - 0.75).abs() < 1e-3);
    }

    #[test]
    fn add_transform_remove() {
        let mut g = GeoEdit::new(Geometry::default());
        let mesh = MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            normals: None,
            uvs: Some(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]),
            indices: vec![0, 1, 2],
        };
        g.add_mesh("Tri", "set3/xyznuvtpc", &mesh, None).unwrap();
        let geo = g.finish().unwrap();
        let bytes = geo.serialize();
        let mut g = GeoEdit::parse(&bytes).unwrap();
        let m = g.mesh("Tri").unwrap();
        assert_eq!(m.positions, mesh.positions);
        assert_eq!(m.indices, vec![0, 1, 2]);
        let n = m.normals.unwrap()[0];
        assert!(n[1].abs() > 0.99, "法线 {n:?}");
        let s = crate::visual::compose([0.0; 3], [0.0; 3], [2.0; 3]);
        g.transform("Tri", &s).unwrap();
        assert_eq!(g.mesh("Tri").unwrap().positions[1], [2.0, 0.0, 0.0]);
        g.remove("Tri").unwrap();
        assert!(!g.has("Tri"));
    }
}
