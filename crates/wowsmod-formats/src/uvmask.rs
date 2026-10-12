//! 从网格生成 UV 遮罩: 把选中三角形的 UV 光栅化到贴图像素空间。
//!
//! UV 约定与 D3D/glTF 一致: (0,0) = 贴图左上角, u 向右, v 向下。超出 0..1 的 UV 按平铺(wrap)取模。
use crate::glb::ReadPrim;
use crate::recolor::Mask;
use regex::Regex;

/// 三角形选择条件
#[derive(Clone, Debug, Default)]
pub struct TriFilter {
    /// 材质名正则(不区分大小写由调用者在正则里写 (?i))
    pub material: Option<Regex>,
    /// 网格名(GLB mesh name, 如 rs3)正则
    pub mesh: Option<Regex>,
    /// 三角形重心须落在的轴对齐盒(GLB/glTF 坐标: min, max)
    pub boxes: Vec<([f32; 3], [f32; 3])>,
    /// true = 按像素裁剪(每个像素插值出 3D 位置再判盒, 边缘平直); false = 旧行为(三角形重心判盒, 大三角形会出锯齿块)
    pub pixel_clip: bool,
}

type Boxes = [([f32; 3], [f32; 3])];
fn in_boxes(b: &Boxes, c: [f32; 3]) -> bool {
    b.is_empty() || b.iter().any(|(mn, mx)| (0..3).all(|k| c[k] >= mn[k] && c[k] <= mx[k]))
}
fn tri_overlaps(b: &Boxes, pos: &[[f32; 3]; 3]) -> bool {
    b.is_empty()
        || b.iter().any(|(mn, mx)| {
            (0..3).all(|k| {
                let lo = pos[0][k].min(pos[1][k]).min(pos[2][k]);
                let hi = pos[0][k].max(pos[1][k]).max(pos[2][k]);
                hi >= mn[k] && lo <= mx[k]
            })
        })
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct UvMaskStats {
    pub prims_matched: usize,
    pub triangles: usize,
    pub pixels: usize,
    /// 被选中区域里, 同时也被"未选中的同材质三角形"占用的像素比例(UV 共用, 改动会同时出现在别处)
    pub shared_fraction: f64,
    pub coverage: f64,
}

fn edge(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

/// 把一个 UV 三角形(像素坐标)画进 hit 缓冲(按 wrap 取模)。保守补偿: 极小三角形至少点亮重心所在像素。
fn raster(hit: &mut [u8], w: u32, h: u32, t: [[f32; 2]; 3], val: u8) -> usize {
    raster_clip(hit, w, h, t, val, None)
}

/// clip = Some((三角形 3D 顶点, 盒子)): 只点亮插值位置落在盒内的像素
fn raster_clip(hit: &mut [u8], w: u32, h: u32, t: [[f32; 2]; 3], val: u8, clip: Option<(&[[f32; 3]; 3], &Boxes)>) -> usize {
    let mut p = t.map(|v| [v[0] * w as f32, v[1] * h as f32]);
    // 平移到第一个周期附近, 避免巨大负数
    let ox = (p[0][0].min(p[1][0]).min(p[2][0]) / w as f32).floor() * w as f32;
    let oy = (p[0][1].min(p[1][1]).min(p[2][1]) / h as f32).floor() * h as f32;
    for v in &mut p {
        v[0] -= ox;
        v[1] -= oy;
    }
    let area = edge(p[0], p[1], p[2]);
    let x0 = p[0][0].min(p[1][0]).min(p[2][0]).floor() as i64;
    let x1 = p[0][0].max(p[1][0]).max(p[2][0]).ceil() as i64;
    let y0 = p[0][1].min(p[1][1]).min(p[2][1]).floor() as i64;
    let y1 = p[0][1].max(p[1][1]).max(p[2][1]).ceil() as i64;
    // 防御: 异常巨大的三角形(UV 跨度超过 4 个周期)跳过
    if (x1 - x0) > 4 * w as i64 || (y1 - y0) > 4 * h as i64 {
        return 0;
    }
    let mut n = 0;
    let ok_at = |b: [f32; 3]| -> bool {
        match clip {
            None => true,
            Some((pos, bx)) => in_boxes(bx, [0, 1, 2].map(|k| pos[0][k] * b[0] + pos[1][k] * b[1] + pos[2][k] * b[2])),
        }
    };
    let mut put = |x: i64, y: i64, hit: &mut [u8]| {
        let xx = x.rem_euclid(w as i64) as usize;
        let yy = y.rem_euclid(h as i64) as usize;
        let i = yy * w as usize + xx;
        if hit[i] < val {
            hit[i] = val;
        }
        n += 1;
    };
    if area.abs() > 1e-12 {
        for y in y0..y1.max(y0 + 1) {
            for x in x0..x1.max(x0 + 1) {
                let c = [x as f32 + 0.5, y as f32 + 0.5];
                let e0 = edge(p[1], p[2], c);
                let e1 = edge(p[2], p[0], c);
                let e2 = edge(p[0], p[1], c);
                let inside = if area > 0.0 { e0 >= 0.0 && e1 >= 0.0 && e2 >= 0.0 } else { e0 <= 0.0 && e1 <= 0.0 && e2 <= 0.0 };
                if inside && ok_at([e0 / area, e1 / area, e2 / area]) {
                    put(x, y, hit);
                }
            }
        }
    }
    // 重心 + 三个顶点所在像素(细长/极小三角形)
    let cx = (p[0][0] + p[1][0] + p[2][0]) / 3.0;
    let cy = (p[0][1] + p[1][1] + p[2][1]) / 3.0;
    if ok_at([1.0 / 3.0; 3]) {
        put(cx.floor() as i64, cy.floor() as i64, hit);
    }
    for (i, v) in p.iter().enumerate() {
        let mut b = [0.0f32; 3];
        b[i] = 1.0;
        if ok_at(b) {
            put(v[0].floor() as i64, v[1].floor() as i64, hit);
        }
    }
    n
}

/// 材质"贴图族"名: 去掉 .mfm 与 _skinned 后缀(同一族通常共用一套贴图, 如 DeckHouse 与 DeckHouse_skinned)
pub fn material_family(m: &str) -> String {
    let m = m.to_lowercase();
    let m = m.trim_end_matches(".mfm");
    m.trim_end_matches("_skinned").to_string()
}

/// 由若干 GLB primitive 生成遮罩。只有带 UV 的 primitive 参与。
/// 统计里的 shared_fraction: 选中像素中, 同时被"同一贴图族(见 [`material_family`])但未被选中"的三角形占用的比例
/// —— 改这些像素的漫反射/通道时, 这些未选中部件也会跟着变。
pub fn mask_from_prims(prims: &[ReadPrim], f: &TriFilter, width: u32, height: u32, dilate: u32) -> (Mask, UvMaskStats) {
    let mut hit = vec![0u8; (width * height) as usize];
    let mut other = vec![0u8; (width * height) as usize];
    let mut st = UvMaskStats::default();
    let mut families = std::collections::BTreeSet::new();
    let mut rest: Vec<(&ReadPrim, Vec<usize>)> = Vec::new();
    for p in prims {
        let (Some(uv), Some(idx)) = (&p.uvs, &p.indices) else { continue };
        let mat = p.material.clone().unwrap_or_default();
        let mat_ok = f.material.as_ref().is_none_or(|r| r.is_match(&mat));
        let mesh_ok = f.mesh.as_ref().is_none_or(|r| r.is_match(&p.name));
        let mut any = false;
        let mut not_sel = Vec::new();
        for (ti, t) in idx.chunks_exact(3).enumerate() {
            if t.iter().any(|&i| i as usize >= uv.len() || i as usize >= p.positions.len()) {
                continue;
            }
            let pos = [p.positions[t[0] as usize], p.positions[t[1] as usize], p.positions[t[2] as usize]];
            let c = [0, 1, 2].map(|k| (pos[0][k] + pos[1][k] + pos[2][k]) / 3.0);
            let clip = f.pixel_clip && !f.boxes.is_empty();
            let in_box = if clip { tri_overlaps(&f.boxes, &pos) } else { in_boxes(&f.boxes, c) };
            if mat_ok && mesh_ok && in_box {
                let tri_uv = [uv[t[0] as usize], uv[t[1] as usize], uv[t[2] as usize]];
                let n = raster_clip(&mut hit, width, height, tri_uv, 255, if clip { Some((&pos, &f.boxes[..])) } else { None });
                if n == 0 && clip {
                    not_sel.push(ti);
                    continue;
                }
                st.triangles += 1;
                any = true;
            } else {
                not_sel.push(ti);
            }
        }
        if any {
            st.prims_matched += 1;
            families.insert(material_family(&mat));
        }
        rest.push((p, not_sel));
    }
    for (p, tris) in rest {
        if !families.contains(&material_family(p.material.as_deref().unwrap_or(""))) {
            continue;
        }
        let (uv, idx) = (p.uvs.as_ref().unwrap(), p.indices.as_ref().unwrap());
        for ti in tris {
            let t = &idx[ti * 3..ti * 3 + 3];
            raster(&mut other, width, height, [uv[t[0] as usize], uv[t[1] as usize], uv[t[2] as usize]], 255);
        }
    }
    st.pixels = hit.iter().filter(|&&x| x > 0).count();
    let shared = hit.iter().zip(&other).filter(|(a, b)| **a > 0 && **b > 0).count();
    st.shared_fraction = if st.pixels > 0 { shared as f64 / st.pixels as f64 } else { 0.0 };
    let mut m = Mask { width, height, v: hit.iter().map(|&x| x as f32 / 255.0).collect() };
    m.dilate(dilate);
    st.coverage = m.coverage();
    (m, st)
}

/// 解析 "x0,y0,z0,x1,y1,z1"
pub fn parse_box(s: &str) -> Option<([f32; 3], [f32; 3])> {
    let v: Vec<f32> = s.split(',').map(|x| x.trim().parse::<f32>()).collect::<Result<_, _>>().ok()?;
    if v.len() != 6 {
        return None;
    }
    Some(([v[0].min(v[3]), v[1].min(v[4]), v[2].min(v[5])], [v[0].max(v[3]), v[1].max(v[4]), v[2].max(v[5])]))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn quad(mat: &str, uv0: [f32; 2], uv1: [f32; 2], z: f32) -> ReadPrim {
        ReadPrim {
            mesh: 0,
            prim: 0,
            name: "rs0".into(),
            positions: vec![[0.0, 0.0, z], [1.0, 0.0, z], [1.0, 1.0, z], [0.0, 1.0, z]],
            normals: None,
            uvs: Some(vec![[uv0[0], uv0[1]], [uv1[0], uv0[1]], [uv1[0], uv1[1]], [uv0[0], uv1[1]]]),
            material: Some(mat.into()),
            indices: Some(vec![0, 1, 2, 0, 2, 3]),
        }
    }
    #[test]
    fn rasterize_quad() {
        let ps = vec![quad("A.mfm", [0.0, 0.0], [0.5, 0.5], 0.0), quad("A.mfm", [0.25, 0.25], [0.75, 0.75], 5.0), quad("B.mfm", [0.5, 0.5], [1.0, 1.0], 0.0)];
        let f = TriFilter { material: Some(Regex::new("^A").unwrap()), boxes: vec![([-1.0, -1.0, -1.0], [2.0, 2.0, 1.0])], ..Default::default() };
        let (m, st) = mask_from_prims(&ps, &f, 16, 16, 0);
        assert_eq!(st.prims_matched, 1);
        assert_eq!(st.triangles, 2);
        // 左上 8x8 被点亮(加上顶点像素可能多 1 行/列)
        assert!(st.pixels >= 64 && st.pixels <= 81, "{}", st.pixels);
        assert_eq!(m.v[0], 1.0);
        assert_eq!(m.v[15 * 16 + 15], 0.0);
        // 第二个 quad (z=5 不在盒内)与第一个在 4..8 重叠: 共享 16 像素左右
        assert!(st.shared_fraction > 0.2 && st.shared_fraction < 0.45, "{}", st.shared_fraction);
        // wrap
        let ps2 = vec![quad("A", [1.0, 1.0], [1.25, 1.25], 0.0)];
        let (m2, _) = mask_from_prims(&ps2, &TriFilter::default(), 16, 16, 0);
        assert_eq!(m2.v[1 * 16 + 1], 1.0);
        // 按像素裁剪: 盒子只含 x<0.5 的一半 → 大约左半边
        let ps3 = vec![quad("A", [0.0, 0.0], [1.0, 1.0], 0.0)];
        let f3 = TriFilter { boxes: vec![([-1.0, -1.0, -1.0], [0.5, 2.0, 1.0])], pixel_clip: true, ..Default::default() };
        let (m3, st3) = mask_from_prims(&ps3, &f3, 16, 16, 0);
        assert!(st3.pixels >= 120 && st3.pixels <= 144, "{}", st3.pixels);
        assert_eq!(m3.v[2 * 16 + 1], 1.0);
        assert_eq!(m3.v[2 * 16 + 14], 0.0);
    }
}
