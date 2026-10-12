//! geometry → GLB 导出(配对 + 解码 + 坐标转换 + 健康检查)。
use std::collections::HashMap;

use serde::Serialize;
use serde_json::json;

use crate::error::{Error, Result};
use crate::geometry::Geometry;
use crate::glb::MeshPrim;
use crate::pairing::Pair;
use crate::vertex;

#[derive(Debug, Clone, Serialize, Default)]
pub struct ExportStats {
    /// 有 assets.bin 包围盒可校验的渲染集数
    pub bbox_checked: usize,
    /// 包围盒不一致的渲染集数(正确配对应为 0)
    pub bbox_mismatch: usize,
    /// 有法线且非退化的三角形中, 几何法线(由绕序得出)与顶点法线均值同向的比例
    pub winding_agrees: f64,
    /// 未做逐 primitive 绕序规范化前的一致率
    pub winding_agrees_raw: f64,
    /// 被整体翻转绕序的 primitive 数
    pub flipped_prims: usize,
    pub checked_triangles: usize,
    /// 解包后法线长度的平均值(应接近 1)
    pub mean_normal_length: f64,
}

/// 由配对结果构建 GLB primitive。坐标从引擎左手系转为 glTF 右手系: Z 取反,
/// 并把每个三角形的绕序反转为 (a, c, b)。
///
/// 绕序依据(M1 实测): Z 取反后若保持原索引顺序, 由绕序算出的面法线与(正确解包的)顶点法线
/// 同向比例 ≈ 0.01%(Richelieu 四段 17 万三角形), 即正好全反; 反转后 ≈ 99.99%。
/// (wows-toolkit 的注释认为 Z 取反"隐式反转了绕序", 但它用错误的有符号法线解包掩盖了这个问题。)
pub fn build_prims(g: &Geometry, pairs: &[Pair]) -> Result<(Vec<MeshPrim>, Vec<String>, ExportStats)> {
    build_prims_opts(g, pairs, true)
}

/// `normalize_winding`: 若某 primitive 的多数三角形绕序与顶点法线相反(M1 实测: 副炮 FGS024/FGS052
/// 整体如此), 则把该 primitive 的绕序整体翻转, 使 glTF 正面与法线一致, 并在 extras.winding_flipped
/// 记下, 以便将来导回时还原。false = 一律保持统一的 (a,c,b) 变换。
pub fn build_prims_opts(g: &Geometry, pairs: &[Pair], normalize_winding: bool) -> Result<(Vec<MeshPrim>, Vec<String>, ExportStats)> {
    let mut vcache: HashMap<u16, Vec<u8>> = HashMap::new();
    let mut icache: HashMap<u16, Vec<u32>> = HashMap::new();
    let mut warnings = Vec::new();
    let mut prims = Vec::new();
    let (mut agree, mut checked, mut nlen_sum, mut nlen_cnt) = (0usize, 0usize, 0f64, 0usize);
    let (mut bbox_checked, mut bbox_mismatch) = (0usize, 0usize);
    let (mut agree_raw, mut flipped_prims) = (0usize, 0usize);
    for p in pairs {
        let vb = g
            .vertex_buffers
            .get(p.vertices.buffer_index as usize)
            .ok_or_else(|| Error::format("geometry", format!("顶点缓冲 {} 不存在", p.vertices.buffer_index)))?;
        let ib = g
            .index_buffers
            .get(p.indices.buffer_index as usize)
            .ok_or_else(|| Error::format("geometry", format!("索引缓冲 {} 不存在", p.indices.buffer_index)))?;
        if !vcache.contains_key(&p.vertices.buffer_index) {
            vcache.insert(p.vertices.buffer_index, vb.decode()?);
        }
        if !icache.contains_key(&p.indices.buffer_index) {
            icache.insert(p.indices.buffer_index, ib.decode()?);
        }
        let vraw = &vcache[&p.vertices.buffer_index];
        let iall = &icache[&p.indices.buffer_index];
        let stride = vb.stride as usize;
        let v0 = p.vertices.items_offset as usize;
        let vn = p.vertices.items_count as usize;
        let i0 = p.indices.items_offset as usize;
        let inn = p.indices.items_count as usize;
        if (v0 + vn) * stride > vraw.len() || i0 + inn > iall.len() {
            return Err(Error::format("geometry", format!("{}: mapping 区间超出缓冲范围", p.name)));
        }
        let fmt = vertex::parse_format(&vb.format());
        let (un, warn) = vertex::unpack(&vraw[v0 * stride..(v0 + vn) * stride], stride, &fmt);
        if let Some(w) = warn {
            warnings.push(format!("{}: {w}", p.name));
        }
        let src = &iall[i0..i0 + inn];
        let mut indices = Vec::with_capacity(src.len());
        for t in src.chunks(3) {
            if t.len() == 3 {
                indices.extend_from_slice(&[t[0], t[2], t[1]]);
            } else {
                indices.extend_from_slice(t);
            }
        }
        // 独立校验: 被索引引用的顶点包围盒 vs assets.bin 渲染集包围盒(引擎坐标)
        let mut bbox_check = serde_json::Value::Null;
        if let Some((emn, emx)) = p.expected_bbox {
            let mut mn = [f32::INFINITY; 3];
            let mut mx = [f32::NEG_INFINITY; 3];
            for &i in src {
                if let Some(v) = un.positions.get(i as usize) {
                    for k in 0..3 {
                        mn[k] = mn[k].min(v[k]);
                        mx[k] = mx[k].max(v[k]);
                    }
                }
            }
            let ext = (0..3).map(|k| (emx[k] - emn[k]).abs()).fold(0f32, f32::max).max(1e-3);
            let dev = (0..3).map(|k| (mn[k] - emn[k]).abs().max((mx[k] - emx[k]).abs())).fold(0f32, f32::max);
            let ok = dev <= ext * 1e-3 + 1e-4;
            if !ok {
                bbox_mismatch += 1;
                warnings.push(format!("{}: 引用顶点包围盒与 assets.bin 渲染集包围盒偏差 {dev:.4} (尺度 {ext:.3})", p.name));
            }
            bbox_checked += 1;
            bbox_check = json!({"ok": ok, "max_deviation": dev});
        }
        let positions: Vec<[f32; 3]> = un.positions.iter().map(|v| [v[0], v[1], -v[2]]).collect();
        let normals = un.normals.map(|ns| {
            ns.iter()
                .map(|n| {
                    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    nlen_sum += l as f64;
                    nlen_cnt += 1;
                    if l > 1e-6 {
                        [n[0] / l, n[1] / l, -n[2] / l]
                    } else {
                        [0.0, 1.0, 0.0]
                    }
                })
                .collect::<Vec<_>>()
        });
        let mut winding_flipped = false;
        if let Some(ns) = &normals {
            let (mut pa, mut pc) = (0usize, 0usize);
            for t in indices.chunks_exact(3) {
                if t.iter().any(|&i| i as usize >= positions.len()) {
                    continue;
                }
                let (a, b, c) = (positions[t[0] as usize], positions[t[1] as usize], positions[t[2] as usize]);
                let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let f = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                let fl = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
                if fl < 1e-12 {
                    continue;
                }
                let m: Vec<f32> = (0..3).map(|k| ns[t[0] as usize][k] + ns[t[1] as usize][k] + ns[t[2] as usize][k]).collect();
                pc += 1;
                if f[0] * m[0] + f[1] * m[1] + f[2] * m[2] > 0.0 {
                    pa += 1;
                }
            }
            checked += pc;
            agree_raw += pa;
            if normalize_winding && pa * 2 < pc {
                for t in indices.chunks_exact_mut(3) {
                    t.swap(1, 2);
                }
                winding_flipped = true;
                flipped_prims += 1;
                agree += pc - pa;
                warnings.push(format!(
                    "{}: 原始绕序与法线多数相反(一致率 {:.2}%), 已整体翻转该 primitive 绕序 (extras.winding_flipped)",
                    p.name,
                    pa as f64 * 100.0 / pc as f64
                ));
            } else {
                agree += pa;
            }
        }
        prims.push(MeshPrim {
            name: p.name.clone(),
            material: p.material.as_ref().map(|m| m.rsplit('/').next().unwrap_or(m).to_string()),
            positions,
            normals,
            uvs: un.uvs,
            indices,
            extras: json!({
                "vertices_mapping": format!("{:#010x}", p.vertices.id),
                "indices_mapping": format!("{:#010x}", p.indices.id),
                "vertex_format": vb.format(),
                "pairing": format!("{:?}", p.source),
                "ambiguous": p.ambiguous,
                "material_path": p.material,
                "bbox_check": bbox_check,
                "winding_flipped": winding_flipped,
            }),
        });
    }
    let stats = ExportStats {
        bbox_checked,
        bbox_mismatch,
        winding_agrees: if checked > 0 { agree as f64 / checked as f64 } else { 0.0 },
        winding_agrees_raw: if checked > 0 { agree_raw as f64 / checked as f64 } else { 0.0 },
        flipped_prims,
        checked_triangles: checked,
        mean_normal_length: if nlen_cnt > 0 { nlen_sum / nlen_cnt as f64 } else { 0.0 },
    };
    Ok((prims, warnings, stats))
}

/// 演示/对照用: 按 vmap/imap 数组下标两两配对(M0 时 glb 破面的错误做法)。
pub fn naive_pairs(g: &Geometry) -> Vec<Pair> {
    g.vertex_mappings
        .iter()
        .zip(g.index_mappings.iter())
        .enumerate()
        .map(|(k, (v, i))| Pair {
            name: format!("naive_{k}"),
            material: None,
            vertices: *v,
            indices: *i,
            source: crate::pairing::PairSource::Heuristic,
            ambiguous: true,
            expected_bbox: None,
        })
        .collect()
}

/// 每个索引 mapping 的最大局部索引(启发式配对用)。
pub fn max_local_index_fn(g: &Geometry) -> Result<impl Fn(&crate::geometry::MappingEntry) -> u32 + '_> {
    let mut decoded: HashMap<u16, Vec<u32>> = HashMap::new();
    for m in &g.index_mappings {
        if !decoded.contains_key(&m.buffer_index) {
            if let Some(ib) = g.index_buffers.get(m.buffer_index as usize) {
                decoded.insert(m.buffer_index, ib.decode()?);
            }
        }
    }
    Ok(move |m: &crate::geometry::MappingEntry| {
        decoded
            .get(&m.buffer_index)
            .and_then(|v| v.get(m.items_offset as usize..(m.items_offset + m.items_count) as usize))
            .and_then(|s| s.iter().copied().max())
            .unwrap_or(0)
    })
}
