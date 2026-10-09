//! 网格健康检查: 索引越界 + 尖刺三角形(取代 tools/glb_check.py)。
//!
//! 规则:
//! - **越界(OOB)**: 任一索引 >= 该 primitive 的顶点数。配对错误时最常见的症状。
//! - **长边三角形(long)**: 最长边 > 0.5 × 该 primitive 包围盒 3D 对角线(glb_check.py 的几何规则,
//!   但修正为真正的 3D 对角线并检查三条边)。M1 实测: 游戏原始数据里本来就有这类三角形
//!   (甲板长条、舭龙骨、钢索、整块甲板四边形等, 都是平面/对称的规整结构), 所以它**只作参考**。
//! - **尖刺(spike)**: 长边三角形 **且** 其面法线(由绕序得出)与三个顶点法线之和的同向关系
//!   与该 primitive 的多数三角形相反(面积为 0 的三角形不可见, 不计)。
//!   即"表面法线不支持的长三角形"——配对错误产生的乱连三角形几乎必然如此, 而真实几何的长三角形
//!   法线与绕序一致。以"多数方向"为基准而不是固定要求点积 > 0, 是因为 M1 实测部分游戏模型
//!   (如副炮 FGS024/FGS052)整体绕序与法线相反(有向体积为负), 固定基准会把它们的长三角形全部误报。
//!   例外: 面法线与顶点法线**严格共线但反向**(|cos| >= 0.95)的三角形只计入 `backface_long_triangles`
//!   (参考), 不算尖刺——它位于顶点法线所描述的那个平面内, 只是绕序反了; M1 实测 FSB017 MidBack 的
//!   C002_Razlom 贴花里有 1 个这样的原始三角形。配对错误产生的乱连三角形, 面法线与顶点法线几乎不会恰好共线。没有法线时退化为 glb_check 规则: 长边三角形占比 > 0.5% 视为该 primitive 全部长边三角形为尖刺。
//! - **全局长边**: 最长边 > 0.5 × 整个模型包围盒对角线(仅参考)。
//! 判定 ok: 无越界、无尖刺、无非有限坐标、索引数是 3 的倍数。
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct PrimHealth {
    pub name: String,
    pub vertices: usize,
    pub triangles: usize,
    pub max_index: Option<u32>,
    pub oob_indices: usize,
    pub oob_triangles: usize,
    pub spike_triangles: usize,
    pub long_triangles: usize,
    pub global_long_triangles: usize,
    pub degenerate_triangles: usize,
    /// 长边、反向、但与顶点法线严格共线的三角形(参考, 不算尖刺)
    pub backface_long_triangles: usize,
    pub nonfinite_positions: usize,
    /// 有法线时: 面法线与顶点法线同向的三角形比例(非零面积三角形中)
    pub winding_normal_agreement: Option<f64>,
    /// 是否用法线判定尖刺(false = 无法线, 用 glb_check 占比规则)
    pub normal_based: bool,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Health {
    pub primitives: Vec<PrimHealth>,
    pub total_triangles: usize,
    pub total_oob_triangles: usize,
    pub total_spike_triangles: usize,
    pub total_long_triangles: usize,
    pub total_global_long_triangles: usize,
    pub ok: bool,
}

/// 检查输入
pub struct PrimInput<'a> {
    pub name: String,
    pub positions: &'a [[f32; 3]],
    pub indices: &'a [u32],
    pub normals: Option<&'a [[f32; 3]]>,
}

pub const SPIKE_EDGE_RATIO: f32 = 0.5;
pub const SPIKE_FLAG_FRACTION: f64 = 0.005;
pub const BACKFACE_COS: f32 = 0.95;

fn bbox(ps: &[[f32; 3]]) -> f32 {
    let mut mn = [f32::INFINITY; 3];
    let mut mx = [f32::NEG_INFINITY; 3];
    for p in ps.iter().filter(|p| p.iter().all(|c| c.is_finite())) {
        for k in 0..3 {
            mn[k] = mn[k].min(p[k]);
            mx[k] = mx[k].max(p[k]);
        }
    }
    if mn[0] > mx[0] {
        return 0.0;
    }
    ((mx[0] - mn[0]).powi(2) + (mx[1] - mn[1]).powi(2) + (mx[2] - mn[2]).powi(2)).sqrt()
}

fn dist(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// 检查一组 primitive(glTF 坐标与绕序: 逆时针为正面)。
pub fn check(prims: &[PrimInput]) -> Health {
    let all: Vec<[f32; 3]> = prims.iter().flat_map(|p| p.positions.iter().copied()).collect();
    let gdiag = bbox(&all);
    let mut h = Health::default();
    for p in prims {
        let (pos, idx) = (p.positions, p.indices);
        let n = pos.len();
        let diag = bbox(pos);
        let normals = p.normals.filter(|ns| ns.len() == n);
        let mut ph = PrimHealth {
            name: p.name.clone(),
            vertices: n,
            triangles: idx.len() / 3,
            max_index: idx.iter().copied().max(),
            nonfinite_positions: pos.iter().filter(|p| !p.iter().all(|c| c.is_finite())).count(),
            normal_based: normals.is_some(),
            ..Default::default()
        };
        ph.oob_indices = idx.iter().filter(|&&i| i as usize >= n).count();
        // 每个三角形的 面法线·顶点法线和 的符号(+1/-1; 0 = 零面积或无法线)
        // 返回 (符号, |cos|)
        let sign_cos = |t: &[u32]| -> (i32, f32) {
            let Some(ns) = normals else { return (0, 0.0) };
            let (a, b, c) = (&pos[t[0] as usize], &pos[t[1] as usize], &pos[t[2] as usize]);
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let f = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            if f[0] == 0.0 && f[1] == 0.0 && f[2] == 0.0 {
                return (0, 0.0);
            }
            let m: Vec<f32> = (0..3).map(|k| ns[t[0] as usize][k] + ns[t[1] as usize][k] + ns[t[2] as usize][k]).collect();
            let d = f[0] * m[0] + f[1] * m[1] + f[2] * m[2];
            let fl = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
            let ml = (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt();
            let cos = if fl > 0.0 && ml > 0.0 { (d / (fl * ml)).abs() } else { 0.0 };
            (if d > 0.0 { 1 } else { -1 }, cos)
        };
        let sign = |t: &[u32]| sign_cos(t).0;
        let (mut pos_n, mut neg_n) = (0usize, 0usize);
        if normals.is_some() {
            for t in idx.chunks_exact(3).filter(|t| t.iter().all(|&i| (i as usize) < n)) {
                match sign(t) {
                    1 => pos_n += 1,
                    -1 => neg_n += 1,
                    _ => {}
                }
            }
            if pos_n + neg_n > 0 {
                ph.winding_normal_agreement = Some(pos_n as f64 / (pos_n + neg_n) as f64);
            }
        }
        let majority = if neg_n > pos_n { -1 } else { 1 };
        for t in idx.chunks_exact(3) {
            if t.iter().any(|&i| i as usize >= n) {
                ph.oob_triangles += 1;
                continue;
            }
            if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                ph.degenerate_triangles += 1;
            }
            let (a, b, c) = (&pos[t[0] as usize], &pos[t[1] as usize], &pos[t[2] as usize]);
            let e = dist(a, b).max(dist(b, c)).max(dist(c, a));
            let long = e > diag * SPIKE_EDGE_RATIO;
            if long {
                ph.long_triangles += 1;
                if normals.is_some() {
                    let (sg, cos) = sign_cos(t);
                    if sg != 0 && sg != majority {
                        if cos >= BACKFACE_COS {
                            ph.backface_long_triangles += 1;
                        } else {
                            ph.spike_triangles += 1;
                        }
                    }
                }
            }
            if e > gdiag * SPIKE_EDGE_RATIO {
                ph.global_long_triangles += 1;
            }
        }
        if normals.is_none() && ph.triangles > 0 && (ph.long_triangles as f64) > ph.triangles as f64 * SPIKE_FLAG_FRACTION {
            ph.spike_triangles = ph.long_triangles;
        }
        ph.ok = ph.oob_indices == 0 && ph.spike_triangles == 0 && ph.nonfinite_positions == 0 && idx.len() % 3 == 0;
        h.total_triangles += ph.triangles;
        h.total_oob_triangles += ph.oob_triangles;
        h.total_spike_triangles += ph.spike_triangles;
        h.total_long_triangles += ph.long_triangles;
        h.total_global_long_triangles += ph.global_long_triangles;
        h.primitives.push(ph);
    }
    h.ok = h.primitives.iter().all(|p| p.ok);
    h
}

/// 检查一个 GLB 文件(字节)。
pub fn check_glb(d: &[u8]) -> crate::Result<Health> {
    let (_, prims) = crate::glb::read_glb(d)?;
    let items: Vec<PrimInput> = prims
        .iter()
        .filter_map(|p| {
            p.indices.as_ref().map(|i| PrimInput {
                name: format!("mesh{}.{} {}", p.mesh, p.prim, p.name),
                positions: p.positions.as_slice(),
                indices: i.as_slice(),
                normals: p.normals.as_deref(),
            })
        })
        .collect();
    Ok(check(&items))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个朝 +Y 的平面网格(glTF 逆时针为正面), 加上可选的额外三角形
    fn plane(n: usize) -> (Vec<[f32; 3]>, Vec<u32>, Vec<[f32; 3]>) {
        let mut p = Vec::new();
        for z in 0..=n {
            for x in 0..=n {
                p.push([x as f32, 0.0, z as f32]);
            }
        }
        let w = (n + 1) as u32;
        let mut idx = Vec::new();
        for z in 0..n as u32 {
            for x in 0..n as u32 {
                let a = z * w + x;
                // 从 +Y 往下看逆时针: a, a+w, a+1
                idx.extend_from_slice(&[a, a + w, a + 1, a + 1, a + w, a + w + 1]);
            }
        }
        let ns = vec![[0.0, 1.0, 0.0]; p.len()];
        (p, idx, ns)
    }

    fn run(p: &[[f32; 3]], i: &[u32], n: Option<&[[f32; 3]]>) -> Health {
        check(&[PrimInput { name: "t".into(), positions: p, indices: i, normals: n }])
    }

    #[test]
    fn clean_plane_ok() {
        let (p, i, n) = plane(10);
        let h = run(&p, &i, Some(&n));
        assert!(h.ok);
        assert_eq!(h.primitives[0].winding_normal_agreement, Some(1.0));
    }

    #[test]
    fn oob_detected() {
        let (p, mut i, n) = plane(4);
        i.extend_from_slice(&[0, 1, 9999]);
        let h = run(&p, &i, Some(&n));
        assert!(!h.ok);
        assert_eq!(h.total_oob_triangles, 1);
    }

    #[test]
    fn spike_detected() {
        // 跨越整个平面且竖起来的三角形: 面法线与顶点法线(+Y)垂直 → 尖刺
        let (mut p, mut i, mut n) = plane(10);
        let base = p.len() as u32;
        p.extend_from_slice(&[[0.0, 0.0, 0.0], [10.0, 0.0, 10.0], [5.0, 8.0, 5.0]]);
        n.extend_from_slice(&[[0.0, 1.0, 0.0]; 3]);
        // 让它与多数方向相反: 选择使 face·(+Y) <= 0 的顺序
        i.extend_from_slice(&[base, base + 2, base + 1]);
        let h = run(&p, &i, Some(&n));
        let ph = &h.primitives[0];
        assert_eq!(ph.long_triangles, 1);
        assert_eq!(ph.spike_triangles + ph.backface_long_triangles, 1);
        assert_eq!(ph.spike_triangles, 1, "竖直三角形与 +Y 法线不共线, 应判为尖刺");
    }

    #[test]
    fn long_but_consistent_is_not_spike() {
        // 整块平面的大四边形(真实游戏数据里的甲板长条一类): 长边但与法线一致
        let p = vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0], [10.0, 0.0, 10.0]];
        let i = vec![0, 2, 1, 1, 2, 3];
        let n = vec![[0.0, 1.0, 0.0]; 4];
        let h = run(&p, &i, Some(&n));
        assert!(h.ok);
        assert_eq!(h.total_long_triangles, 2);
    }

    #[test]
    fn coplanar_backface_is_info_only() {
        let (mut p, mut i, mut n) = plane(10);
        let base = p.len() as u32;
        p.extend_from_slice(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0]]);
        n.extend_from_slice(&[[0.0, 1.0, 0.0]; 3]);
        i.extend_from_slice(&[base, base + 1, base + 2]); // 顺时针 = 背面, 但与法线共线
        let h = run(&p, &i, Some(&n));
        assert!(h.ok);
        assert_eq!(h.primitives[0].backface_long_triangles, 1);
    }

    #[test]
    fn globally_inverted_primitive_not_flagged() {
        // 整体绕序反了(如副炮 FGS024/FGS052): 多数基准下不应全部误报
        let (p, mut i, n) = plane(10);
        for t in i.chunks_exact_mut(3) {
            t.swap(1, 2);
        }
        let mut p2 = p.clone();
        p2.extend_from_slice(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0]]);
        let mut n2 = n.clone();
        n2.extend_from_slice(&[[0.0, 1.0, 0.0]; 3]);
        let b = p.len() as u32;
        i.extend_from_slice(&[b, b + 1, b + 2]); // 与多数(反向)一致的长三角形
        let h = run(&p2, &i, Some(&n2));
        assert!(h.ok, "{:?}", h.primitives[0]);
        assert_eq!(h.primitives[0].winding_normal_agreement, Some(0.0));
    }

    #[test]
    fn no_normals_fraction_rule() {
        let p = vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0], [10.0, 0.0, 10.0]];
        let i = vec![0, 2, 1, 1, 2, 3];
        let h = run(&p, &i, None);
        assert!(!h.ok, "无法线时退化为占比规则: 长边占比 100% > 0.5%");
    }
}
