//! 设计辅助命令(定制复刻 / 主题 / 原创三种工作流共用):
//! - `lineart`: 在 UV 空间按船体结构生成"贴合"的线稿遮罩 —— 几何折边(crease)、部件外轮廓(boundary)、
//!   世界坐标等值线(contour: 沿船体走向的水平/竖直线)、世界空间圆环(rings: 舷侧"光核")、法线贴图板缝(normal-edges)。
//!   与程序网格条带不同, 这些线都来自船本身的几何/贴图, 所以在 3D 上贴着结构走。
//! - `bake-ref`: 把与 `render --engine soft` 同机位、同尺寸的设计图/参考图投影进某材质的 UV(带遮挡判断),
//!   并可按颜色抠出遮罩(如青色线稿 → 发光遮罩, 浅灰块 → 换色遮罩)。
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use clap::Args;
use serde_json::{json, Value};

use wowsmod_formats::recolor::Mask;
use wowsmod_formats::{glb, texture::{self, Image}};

use crate::modproj::Project;
use crate::render::{self, cross, dot, norm, sub, Proj, View, V3};
use crate::{emit, game_ctx, texcmd, Cli};

/// 世界坐标三角形(已乘节点矩阵) + UV + 材质(.mfm 相对路径)
pub struct WTri {
    pub p: [V3; 3],
    pub uv: [[f32; 2]; 3],
    pub mfm: String,
}

/// 装配项目整船(复用 render 的 .render/head.glb; 不存在或 --refresh 时重新 assemble)
pub fn load_world(cli: &Cli, p: &Project, refresh: bool) -> Result<Vec<WTri>> {
    let rdir = p.root.join(".render");
    std::fs::create_dir_all(&rdir)?;
    let head = rdir.join("head.glb");
    if refresh || !head.is_file() {
        let ship = p.cfg.meta.ship.clone();
        if ship.is_empty() {
            bail!("mod.toml 没有 ship, 无法装配整船");
        }
        render::self_exe_json(cli, &["assemble".into(), ship, "--project".into(), p.root.display().to_string(), "--full".into(), "-o".into(), head.display().to_string()])?;
    }
    let d = std::fs::read(&head).with_context(|| format!("读取 {}", head.display()))?;
    let (j, prims) = glb::read_glb(&d).map_err(|e| anyhow!("{e}"))?;
    let inst = render::mesh_instances(&j);
    let mut out = Vec::new();
    for pr in &prims {
        let Some(uv) = &pr.uvs else { continue };
        let mfm = j["meshes"][pr.mesh]["primitives"][pr.prim]["extras"]["mfm"].as_str().unwrap_or("").to_string();
        let idx: Vec<u32> = pr.indices.clone().unwrap_or_else(|| (0..pr.positions.len() as u32).collect());
        for m in inst.get(&pr.mesh).cloned().unwrap_or_else(|| vec![render::IDENT]) {
            let wp: Vec<V3> = pr.positions.iter().map(|x| render::xform(&m, *x)).collect();
            for t in idx.chunks_exact(3) {
                let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
                if a >= wp.len() || b >= wp.len() || c >= wp.len() || a >= uv.len() || b >= uv.len() || c >= uv.len() {
                    continue;
                }
                out.push(WTri { p: [wp[a], wp[b], wp[c]], uv: [uv[a], uv[b], uv[c]], mfm: mfm.clone() });
            }
        }
    }
    Ok(out)
}

fn mfm_name(m: &str) -> String {
    m.rsplit(['/', '\\']).next().unwrap_or(m).trim_end_matches(".mfm").to_string()
}

/// 选中三角形: 材质名(.mfm 文件名, 不含扩展名)正则 + 可选盒子
fn select<'a>(all: &'a [WTri], material: &str, boxes: &[([f32; 3], [f32; 3])]) -> Result<Vec<&'a WTri>> {
    let re = regex::Regex::new(&format!("(?i){material}")).map_err(|e| anyhow!("--material 正则: {e}"))?;
    let v: Vec<&WTri> = all
        .iter()
        .filter(|t| re.is_match(&mfm_name(&t.mfm)))
        .filter(|t| {
            let c = [0, 1, 2].map(|k| (t.p[0][k] + t.p[1][k] + t.p[2][k]) / 3.0);
            boxes.is_empty() || boxes.iter().any(|(mn, mx)| (0..3).all(|k| c[k] >= mn[k] && c[k] <= mx[k]))
        })
        .collect();
    if v.is_empty() {
        let mut names: Vec<String> = all.iter().map(|t| mfm_name(&t.mfm)).collect();
        names.sort();
        names.dedup();
        bail!("--material {material:?} 没有选中三角形。可选材质: {}", names.join(", "));
    }
    Ok(v)
}

fn bbox(ts: &[&WTri]) -> ([f32; 3], [f32; 3]) {
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for t in ts {
        for q in t.p {
            for k in 0..3 {
                mn[k] = mn[k].min(q[k]);
                mx[k] = mx[k].max(q[k]);
            }
        }
    }
    (mn, mx)
}

fn tex_dims(cli: &Cli, p: &Project, like: &str) -> Result<(u32, u32)> {
    let ctx = game_ctx(cli, true)?;
    let t = texcmd::load_tex(like, ctx.as_ref(), Some(p), false)?;
    Ok((t.dds.header.width, t.dds.header.height))
}

/// UV 像素空间里画一条粗线段(距离场, 1px 抗锯齿), 按 wrap 取模
fn draw_seg(m: &mut [f32], w: u32, h: u32, a: [f32; 2], b: [f32; 2], width: f32) {
    let (ax, ay, bx, by) = (a[0] * w as f32, a[1] * h as f32, b[0] * w as f32, b[1] * h as f32);
    let ox = (ax.min(bx) / w as f32).floor() * w as f32;
    let oy = (ay.min(by) / h as f32).floor() * h as f32;
    let (ax, ay, bx, by) = (ax - ox, ay - oy, bx - ox, by - oy);
    let r = width * 0.5 + 1.0;
    let (x0, x1) = ((ax.min(bx) - r).floor() as i64, (ax.max(bx) + r).ceil() as i64);
    let (y0, y1) = ((ay.min(by) - r).floor() as i64, (ay.max(by) + r).ceil() as i64);
    if (x1 - x0) > 2 * w as i64 || (y1 - y0) > 2 * h as i64 {
        return;
    }
    let (dx, dy) = (bx - ax, by - ay);
    let l2 = (dx * dx + dy * dy).max(1e-9);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let t = (((px - ax) * dx + (py - ay) * dy) / l2).clamp(0.0, 1.0);
            let (qx, qy) = (ax + t * dx - px, ay + t * dy - py);
            let d = (qx * qx + qy * qy).sqrt();
            let v = (width * 0.5 - d + 0.5).clamp(0.0, 1.0);
            if v > 0.0 {
                let i = (y.rem_euclid(h as i64) as usize) * w as usize + x.rem_euclid(w as i64) as usize;
                if m[i] < v {
                    m[i] = v;
                }
            }
        }
    }
}

/// 在 UV 里光栅化三角形, 对每个像素回调 (像素索引, 重心坐标, 每像素的世界坐标梯度长度 helper)
fn raster_uv<F: FnMut(usize, [f32; 3])>(w: u32, h: u32, uv: [[f32; 2]; 3], mut f: F) {
    let mut p = uv.map(|v| [v[0] * w as f32, v[1] * h as f32]);
    let ox = (p[0][0].min(p[1][0]).min(p[2][0]) / w as f32).floor() * w as f32;
    let oy = (p[0][1].min(p[1][1]).min(p[2][1]) / h as f32).floor() * h as f32;
    for v in &mut p {
        v[0] -= ox;
        v[1] -= oy;
    }
    let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]);
    if area.abs() < 1e-9 {
        return;
    }
    let x0 = p[0][0].min(p[1][0]).min(p[2][0]).floor() as i64;
    let x1 = p[0][0].max(p[1][0]).max(p[2][0]).ceil() as i64;
    let y0 = p[0][1].min(p[1][1]).min(p[2][1]).floor() as i64;
    let y1 = p[0][1].max(p[1][1]).max(p[2][1]).ceil() as i64;
    if (x1 - x0) > 2 * w as i64 || (y1 - y0) > 2 * h as i64 {
        return;
    }
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = ((p[1][0] - cx) * (p[2][1] - cy) - (p[1][1] - cy) * (p[2][0] - cx)) / area;
            let w1 = ((p[2][0] - cx) * (p[0][1] - cy) - (p[2][1] - cy) * (p[0][0] - cx)) / area;
            let w2 = 1.0 - w0 - w1;
            let e = -0.02;
            if w0 < e || w1 < e || w2 < e {
                continue;
            }
            let i = (y.rem_euclid(h as i64) as usize) * w as usize + x.rem_euclid(w as i64) as usize;
            f(i, [w0, w1, w2]);
        }
    }
}

/// 标量场 f(世界坐标) 在三角形上线性近似时, 每 UV 像素的变化量(用于把"线宽像素"换成场的单位)
fn field_grad_px(t: &WTri, w: u32, h: u32, f: &dyn Fn(V3) -> f32) -> f32 {
    let fv = t.p.map(f);
    let uv = t.uv.map(|v| [v[0] * w as f32, v[1] * h as f32]);
    let (e1, e2) = ([uv[1][0] - uv[0][0], uv[1][1] - uv[0][1]], [uv[2][0] - uv[0][0], uv[2][1] - uv[0][1]]);
    let (d1, d2) = (fv[1] - fv[0], fv[2] - fv[0]);
    let det = e1[0] * e2[1] - e1[1] * e2[0];
    if det.abs() < 1e-9 {
        return 0.0;
    }
    let gx = (d1 * e2[1] - d2 * e1[1]) / det;
    let gy = (d2 * e1[0] - d1 * e2[0]) / det;
    (gx * gx + gy * gy).sqrt()
}

#[derive(Args, Debug)]
pub struct LineartArgs {
    /// mod 项目目录
    pub project: PathBuf,
    /// 材质(.mfm 文件名)正则, 如 "Hull$|Hull_skinned" / "DeckHouse" / "FGM049"
    #[arg(long)]
    pub material: String,
    /// 遮罩尺寸取自该贴图(通常是同部件的 _a 或 _mg)
    #[arg(long)]
    pub like: String,
    /// 输出遮罩 PNG
    #[arg(short, long)]
    pub out: PathBuf,
    /// 几何折边: 相邻面法线夹角大于该角度(度)的边画线, 如 35(板/甲板/上层建筑棱线)
    #[arg(long)]
    pub crease: Option<f32>,
    /// 部件外轮廓: 只属于一个三角形的几何边(开口边)画线
    #[arg(long)]
    pub boundary: bool,
    /// 世界坐标等值线 AXIS:STEP[:OFFSET[:FROM:TO]], AXIS=x|y|z(GLB: y 向上, z 沿船长, x 横向), 可重复
    #[arg(long, value_name = "AXIS:STEP[:OFFSET[:FROM:TO]]")]
    pub contour: Vec<String>,
    /// 世界空间同心圆(与船体表面的交线) X,Y,Z:R1/R2/..., 可重复(舷侧"光核/眼")
    #[arg(long, value_name = "X,Y,Z:R1/R2")]
    pub rings: Vec<String>,
    /// 从法线贴图提取板缝(Sobel), 给贴图名, 如 FSB025_Bourgogne_1945_Hull_n.dds
    #[arg(long = "normal-edges")]
    pub normal_edges: Option<String>,
    /// 板缝阈值(0..1, 越大越少)
    #[arg(long, default_value_t = 0.35)]
    pub normal_threshold: f32,
    /// 线宽(贴图像素)
    #[arg(long, default_value_t = 6.0)]
    pub width: f32,
    /// 只保留这些盒子里的三角形 x0,y0,z0,x1,y1,z1(GLB 坐标), 可重复
    #[arg(long = "box", value_name = "X0,Y0,Z0,X1,Y1,Z1")]
    pub boxes: Vec<String>,
    /// 只保留法线朝向满足条件的面: AXIS>v 或 AXIS<v, 如 "|x|>0.5"(舷侧) 或 "y>0.7"(甲板/顶面)
    #[arg(long)]
    pub facing: Option<String>,
    /// 整面填充: 把选中的三角形(材质 + --box + --facing, 世界坐标, 已含炮塔等实例的摆放)整块画进遮罩,
    /// 用于"按世界位置选部件"(如主炮炮管、某层上层建筑)。可与线稿叠加
    #[arg(long)]
    pub fill: bool,
    /// 把遮罩以青色叠到 --like 贴图上, 写预览 PNG
    #[arg(long)]
    pub overlay: Option<PathBuf>,
    /// 重新装配整船(项目几何改过时)
    #[arg(long)]
    pub refresh: bool,
}

fn facing_ok(spec: &Option<String>, n: V3) -> Result<bool> {
    let Some(s) = spec else { return Ok(true) };
    let s = s.replace(' ', "");
    let (lhs, gt, rhs) = if let Some((a, b)) = s.split_once('>') { (a.to_string(), true, b.to_string()) } else if let Some((a, b)) = s.split_once('<') { (a.to_string(), false, b.to_string()) } else { bail!("--facing 格式: \"|x|>0.5\" / \"y>0.7\"") };
    let abs = lhs.starts_with('|');
    let ax = match lhs.trim_matches('|') {
        "x" => 0,
        "y" => 1,
        "z" => 2,
        o => bail!("--facing 轴 {o:?}"),
    };
    let v: f32 = rhs.parse().map_err(|_| anyhow!("--facing 数值"))?;
    let c = if abs { n[ax].abs() } else { n[ax] };
    Ok(if gt { c > v } else { c < v })
}

fn parse_contour(s: &str) -> Result<(usize, f32, f32, Option<(f32, f32)>)> {
    let v: Vec<&str> = s.split(':').collect();
    let ax = match v.first().copied().unwrap_or("") {
        "x" => 0,
        "y" => 1,
        "z" => 2,
        o => bail!("--contour 轴 {o:?}(x|y|z)"),
    };
    let step: f32 = v.get(1).ok_or_else(|| anyhow!("--contour 需要 STEP"))?.parse()?;
    let off: f32 = v.get(2).map(|x| x.parse()).transpose()?.unwrap_or(0.0);
    let range = match (v.get(3), v.get(4)) {
        (Some(a), Some(b)) => Some((a.parse()?, b.parse()?)),
        _ => None,
    };
    if step <= 0.0 {
        bail!("--contour STEP 必须 > 0");
    }
    Ok((ax, step, off, range))
}

fn save_mask(path: &Path, m: &Mask) -> Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, texture::write_png(&m.to_image(), false).map_err(|e| anyhow!("{e}"))?)?;
    Ok(())
}

fn overlay_png(cli: &Cli, p: &Project, like: &str, m: &Mask, out: &Path, rgb: [f32; 3]) -> Result<()> {
    let ctx = game_ctx(cli, true)?;
    let t = texcmd::load_tex(like, ctx.as_ref(), Some(p), false)?;
    let mut img = texture::decode_level(&t.dds, 0).map_err(|e| anyhow!("{e}"))?;
    let mm = if m.width != img.width { m.resized(img.width, img.height) } else { m.clone() };
    for (px, &v) in img.px.chunks_exact_mut(4).zip(&mm.v) {
        let a = (v * 0.85).clamp(0.0, 1.0);
        for k in 0..3 {
            px[k] = (px[k] as f32 * (1.0 - a) + rgb[k] * a) as u8;
        }
        px[3] = 255;
    }
    texcmd::preview(&img, out, 2048)
}

pub fn cmd_lineart(cli: &Cli, a: &LineartArgs) -> Result<i32> {
    let p = Project::open(&a.project)?;
    let (w, h) = tex_dims(cli, &p, &a.like)?;
    let all = load_world(cli, &p, a.refresh)?;
    let boxes: Vec<_> = a.boxes.iter().map(|b| wowsmod_formats::uvmask::parse_box(b).ok_or_else(|| anyhow!("--box 格式: x0,y0,z0,x1,y1,z1"))).collect::<Result<_>>()?;
    let mut sel = select(&all, &a.material, &boxes)?;
    if a.facing.is_some() {
        let mut keep = Vec::new();
        for t in sel {
            if facing_ok(&a.facing, norm(cross(sub(t.p[1], t.p[0]), sub(t.p[2], t.p[0]))))? {
                keep.push(t);
            }
        }
        sel = keep;
        if sel.is_empty() {
            bail!("--facing 过滤后没有三角形");
        }
    }
    let (mn, mx) = bbox(&sel);
    let mut m = vec![0f32; (w * h) as usize];
    let mut stats = serde_json::Map::new();
    // 1) 几何边: 按量化位置建边表
    if a.crease.is_some() || a.boundary {
        let q = |v: V3| -> [i64; 3] { v.map(|x| (x * 2000.0).round() as i64) };
        let mut edges: HashMap<([i64; 3], [i64; 3]), Vec<(usize, usize)>> = HashMap::new();
        for (ti, t) in sel.iter().enumerate() {
            for e in 0..3 {
                let (i, j) = (e, (e + 1) % 3);
                let (ka, kb) = (q(t.p[i]), q(t.p[j]));
                if ka == kb {
                    continue;
                }
                let key = if ka < kb { (ka, kb) } else { (kb, ka) };
                edges.entry(key).or_default().push((ti, e));
            }
        }
        let normals: Vec<V3> = sel.iter().map(|t| norm(cross(sub(t.p[1], t.p[0]), sub(t.p[2], t.p[0])))).collect();
        let cos_lim = a.crease.map(|d| d.to_radians().cos());
        let (mut n_crease, mut n_bound) = (0usize, 0usize);
        for occ in edges.values() {
            let draw = if occ.len() == 1 {
                if a.boundary {
                    n_bound += 1;
                    true
                } else {
                    false
                }
            } else if let Some(cl) = cos_lim {
                let mut sharp = false;
                for x in 0..occ.len() {
                    for y in x + 1..occ.len() {
                        if dot(normals[occ[x].0], normals[occ[y].0]).abs() < cl {
                            sharp = true;
                        }
                    }
                }
                if sharp {
                    n_crease += 1;
                }
                sharp
            } else {
                false
            };
            if draw {
                for &(ti, e) in occ {
                    let t = sel[ti];
                    draw_seg(&mut m, w, h, t.uv[e], t.uv[(e + 1) % 3], a.width);
                }
            }
        }
        stats.insert("crease_edges".into(), json!(n_crease));
        stats.insert("boundary_edges".into(), json!(n_bound));
    }
    // 2) 标量场等值线(contour / rings)
    let mut fields: Vec<(Box<dyn Fn(V3) -> f32>, Vec<f32>, Option<f32>, Option<(f32, f32)>)> = Vec::new();
    for c in &a.contour {
        let (ax, step, off, range) = parse_contour(c)?;
        // levels[0] 存 offset
        fields.push((Box::new(move |q: V3| q[ax]), vec![off], Some(step), range));
    }
    for r in &a.rings {
        let (c, rs) = r.split_once(':').ok_or_else(|| anyhow!("--rings 格式 X,Y,Z:R1/R2"))?;
        let cv: Vec<f32> = c.split(',').map(|x| x.trim().parse::<f32>()).collect::<Result<_, _>>()?;
        if cv.len() != 3 {
            bail!("--rings 中心需 3 个数");
        }
        let radii: Vec<f32> = rs.split('/').map(|x| x.trim().parse::<f32>()).collect::<Result<_, _>>()?;
        let cc = [cv[0], cv[1], cv[2]];
        fields.push((Box::new(move |q: V3| dot(sub(q, cc), sub(q, cc)).sqrt()), radii, None, None));
    }
    let mut field_px = vec![0usize; fields.len()];
    for (fi, (f, levels, step, range)) in fields.iter().enumerate() {
        for t in &sel {
            let g = field_grad_px(t, w, h, f.as_ref());
            if g <= 1e-9 {
                continue;
            }
            let fv = t.p.map(|q| f(q));
            let (tmin, tmax) = (fv.iter().cloned().fold(f32::MAX, f32::min), fv.iter().cloned().fold(f32::MIN, f32::max));
            let half = a.width * 0.5 * g;
            // 快速跳过: 三角形范围内没有等值线
            if let Some(s) = step {
                let off = levels[0];
                let k0 = ((tmin - half - off) / s).ceil();
                let k1 = ((tmax + half - off) / s).floor();
                if k0 > k1 {
                    continue;
                }
            } else if !levels.iter().any(|&l| l >= tmin - half && l <= tmax + half) {
                continue;
            }
            let mut cnt = 0;
            raster_uv(w, h, t.uv, |i, bc| {
                let v = bc[0] * fv[0] + bc[1] * fv[1] + bc[2] * fv[2];
                let d = if let Some(s) = step {
                    if let Some((lo, hi)) = range {
                        if v < *lo || v > *hi {
                            return;
                        }
                    }
                    let off = levels[0];
                    let r = (v - off) / s;
                    (r - r.round()).abs() * s
                } else {
                    levels.iter().map(|l| (v - l).abs()).fold(f32::MAX, f32::min)
                };
                let px = d / g;
                let val = (a.width * 0.5 - px + 0.5).clamp(0.0, 1.0);
                if val > 0.0 && m[i] < val {
                    m[i] = val;
                    cnt += 1;
                }
            });
            field_px[fi] += cnt;
        }
    }
    if !fields.is_empty() {
        stats.insert("field_pixels".into(), json!(field_px));
    }
    // 0) 整面填充
    if a.fill {
        let mut cnt = 0usize;
        for t in &sel {
            raster_uv(w, h, t.uv, |i, _| {
                if m[i] < 1.0 {
                    m[i] = 1.0;
                    cnt += 1;
                }
            });
            // 细三角形可能落不到像素中心: 补画三条边
            for e in 0..3 {
                draw_seg(&mut m, w, h, t.uv[e], t.uv[(e + 1) % 3], 2.0);
            }
        }
        stats.insert("fill_pixels".into(), json!(cnt));
    }
    // 3) 法线贴图板缝(只保留选中部件 UV 覆盖范围内)
    if let Some(nt) = &a.normal_edges {
        let ctx = game_ctx(cli, true)?;
        let t = texcmd::load_tex(nt, ctx.as_ref(), Some(&p), false)?;
        let lv = (0..t.dds.header.levels()).find(|l| t.dds.header.level_dims(*l).0 <= w).unwrap_or(0);
        let img = texture::decode_level(&t.dds, lv).map_err(|e| anyhow!("{e}"))?;
        let nm = normal_edges(&img, a.normal_threshold);
        let nm = if nm.width != w { nm.resized(w, h) } else { nm };
        let mut cover = vec![false; (w * h) as usize];
        for t in &sel {
            raster_uv(w, h, t.uv, |i, _| cover[i] = true);
        }
        let mut cnt = 0usize;
        for i in 0..m.len() {
            if cover[i] && nm.v[i] > m[i] {
                m[i] = nm.v[i];
                cnt += 1;
            }
        }
        stats.insert("normal_edge_pixels".into(), json!(cnt));
    }
    let mask = Mask { width: w, height: h, v: m };
    save_mask(&a.out, &mask)?;
    if let Some(o) = &a.overlay {
        overlay_png(cli, &p, &a.like, &mask, o, [0.0, 229.0, 255.0])?;
    }
    let v = json!({"ok": true, "mask": a.out.display().to_string(), "size": [w, h], "triangles": sel.len(), "coverage": mask.coverage(),
        "bbox_min": mn, "bbox_max": mx, "stats": stats,
        "next": ["先看 --overlay 叠加图, 再 wowsmod glow --mask <此遮罩> …; 用 wowsmod render <项目> --engine soft --view side 对照", "参数说明: wowsmod help replicate"]});
    if cli.json {
        emit(cli, &v);
    } else {
        println!("已写线稿遮罩 {} ({w}x{h}) 三角形 {} 覆盖 {:.2}%  bbox {:?}..{:?}  {}", a.out.display(), sel.len(), mask.coverage() * 100.0, mn, mx, Value::Object(stats));
    }
    Ok(0)
}

/// 法线贴图 Sobel 边缘(切线空间 RG), 阈值后细化为 ~2px 线
pub fn normal_edges(img: &Image, thr: f32) -> Mask {
    let (w, h) = (img.width, img.height);
    let nx = |x: i64, y: i64| -> [f32; 2] {
        let xx = x.rem_euclid(w as i64) as u32;
        let yy = y.rem_euclid(h as i64) as u32;
        let p = img.get(xx, yy);
        [p[0] as f32 / 127.5 - 1.0, p[1] as f32 / 127.5 - 1.0]
    };
    let mut v = vec![0f32; (w * h) as usize];
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let mut g = 0f32;
            for c in 0..2 {
                let s = |dx: i64, dy: i64| nx(x + dx, y + dy)[c];
                let gx = s(1, -1) + 2.0 * s(1, 0) + s(1, 1) - s(-1, -1) - 2.0 * s(-1, 0) - s(-1, 1);
                let gy = s(-1, 1) + 2.0 * s(0, 1) + s(1, 1) - s(-1, -1) - 2.0 * s(0, -1) - s(1, -1);
                g += gx * gx + gy * gy;
            }
            let g = g.sqrt() / 4.0;
            v[(y as u32 * w + x as u32) as usize] = ((g - thr) / thr.max(0.05) * 2.0).clamp(0.0, 1.0);
        }
    }
    Mask { width: w, height: h, v }
}

#[derive(Args, Debug)]
pub struct BakeRefArgs {
    /// mod 项目目录
    pub project: PathBuf,
    /// 设计图/参考图 PNG: 必须与 `wowsmod render <项目> --engine soft --view <同视角> [--yaw --pitch] --width <同宽>`(不加 --compare)同尺寸同机位
    #[arg(long = "ref")]
    pub reference: PathBuf,
    /// 材质(.mfm 文件名)正则
    #[arg(long)]
    pub material: String,
    /// 输出尺寸取自该贴图
    #[arg(long)]
    pub like: String,
    /// 输出: UV 空间的颜色图(alpha = 该像素在此机位可见)
    #[arg(short, long)]
    pub out: PathBuf,
    /// 视角(side / quarter; 或用 --yaw/--pitch)
    #[arg(long, value_enum, default_value_t = View::Side)]
    pub view: View,
    #[arg(long)]
    pub yaw: Option<f32>,
    #[arg(long)]
    pub pitch: Option<f32>,
    /// 与 render 相同的 --width
    #[arg(long, default_value_t = 1600)]
    pub width: u32,
    /// 按颜色抠遮罩: "#RRGGBB:容差=输出.png"(容差 0..1, OKLab 距离), 可重复
    #[arg(long)]
    pub key: Vec<String>,
    /// 另一舷: 把世界 x 取反再投影(左右舷 UV 不共用时让两舷都拿到图案)
    #[arg(long)]
    pub mirror_x: bool,
    /// 遮挡深度容差(世界单位)
    #[arg(long, default_value_t = 0.25)]
    pub depth_tol: f32,
    /// 只投影朝向相机的面(法线与视线夹角 cos 大于此值)
    #[arg(long, default_value_t = 0.15)]
    pub min_facing: f32,
    /// 重新装配整船
    #[arg(long)]
    pub refresh: bool,
}

fn srgb_to_oklab(c: [u8; 3]) -> [f32; 3] {
    use wowsmod_formats::color::{linear_to_oklab, srgb_to_linear};
    linear_to_oklab(c.map(|x| srgb_to_linear(x as f32 / 255.0)))
}

fn parse_hex(s: &str) -> Result<[u8; 3]> {
    wowsmod_formats::color::parse_color(s).ok_or_else(|| anyhow!("颜色格式 #RRGGBB: {s:?}"))
}

pub fn cmd_bake_ref(cli: &Cli, a: &BakeRefArgs) -> Result<i32> {
    let p = Project::open(&a.project)?;
    let (w, h) = tex_dims(cli, &p, &a.like)?;
    let all = load_world(cli, &p, a.refresh)?;
    let sel = select(&all, &a.material, &[])?;
    render::set_custom_dir(if a.yaw.is_some() || a.pitch.is_some() { Some(render::yaw_pitch_dir(a.yaw.unwrap_or(0.0), a.pitch.unwrap_or(0.0))) } else { None });
    let view = if a.yaw.is_some() || a.pitch.is_some() { View::Side } else { a.view };
    let tris: Vec<[V3; 3]> = all.iter().map(|t| t.p).collect();
    let bounds = render::bounds_of(&tris, view);
    let width = a.width.clamp(200, 8192);
    let pj = Proj::new(view, bounds, width);
    let mut refimg = texture::read_png(&std::fs::read(&a.reference).with_context(|| format!("读取 {}", a.reference.display()))?).map_err(|e| anyhow!("{e}"))?;
    let mut notes = Vec::new();
    if refimg.width != pj.w || refimg.height != pj.h {
        notes.push(format!("参考图 {}x{} 与机位帧 {}x{} 不同, 已拉伸(请用同参数的 render 图做底稿)", refimg.width, refimg.height, pj.w, pj.h));
        refimg = resize(&refimg, pj.w, pj.h);
    }
    // 深度缓冲(全船做遮挡)
    let zbuf = |mirror: bool| -> Vec<f32> {
        let mut zb = vec![f32::MIN; (pj.w * pj.h) as usize];
        for t in &all {
            let s = t.p.map(|q| pj.px(if mirror { [-q[0], q[1], q[2]] } else { q }));
            raster_screen(&s, pj.w, pj.h, |i, z| {
                if z > zb[i] {
                    zb[i] = z;
                }
            });
        }
        zb
    };
    let passes: Vec<bool> = if a.mirror_x { vec![false, true] } else { vec![false] };
    let mut out = Image::new(w, h);
    let mut seen = 0usize;
    for &mir in &passes {
        let zb = zbuf(mir);
        for t in &sel {
            let wp = t.p.map(|q| if mir { [-q[0], q[1], q[2]] } else { q });
            let n = norm(cross(sub(wp[1], wp[0]), sub(wp[2], wp[0])));
            // 法线朝向相机(视线 dir 指向场景; 双面: 取绝对值, 镜像会翻转绕序)
            if dot(n, pj.dir).abs() < a.min_facing {
                continue;
            }
            let sp = wp.map(|q| pj.px(q));
            raster_uv(w, h, t.uv, |i, bc| {
                let x = bc[0] * sp[0][0] + bc[1] * sp[1][0] + bc[2] * sp[2][0];
                let y = bc[0] * sp[0][1] + bc[1] * sp[1][1] + bc[2] * sp[2][1];
                let z = bc[0] * sp[0][2] + bc[1] * sp[1][2] + bc[2] * sp[2][2];
                if x < 0.0 || y < 0.0 || x >= pj.w as f32 || y >= pj.h as f32 {
                    return;
                }
                let zi = (y as u32 * pj.w + x as u32) as usize;
                if z < zb[zi] - a.depth_tol {
                    return;
                }
                let c = bilinear(&refimg, x, y);
                let o = i * 4;
                // 参考图的 alpha 一并带进 UV(透明处不写); 多个面共用同一 UV 时保留 alpha 更高的那次,
                // 这样只在一座炮塔上画的图案不会被另一座(透明处)覆盖
                let ca = c[3];
                if ca == 0 || ca < out.px[o + 3] {
                    return;
                }
                if out.px[o + 3] == 0 {
                    seen += 1;
                }
                out.px[o..o + 3].copy_from_slice(&c[..3]);
                out.px[o + 3] = ca;
            });
        }
    }
    if let Some(d) = a.out.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(&a.out, texture::write_png(&out, true).map_err(|e| anyhow!("{e}"))?)?;
    let mut keys = Vec::new();
    for k in &a.key {
        let (spec, path) = k.split_once('=').ok_or_else(|| anyhow!("--key 格式 \"#RRGGBB:容差=输出.png\""))?;
        let (col, tol) = spec.split_once(':').unwrap_or((spec, "0.12"));
        let lab = srgb_to_oklab(parse_hex(col)?);
        let tol: f32 = tol.parse()?;
        let mut m = Mask::filled(w, h, 0.0);
        for (i, px) in out.px.chunks_exact(4).enumerate() {
            if px[3] == 0 {
                continue;
            }
            let l = srgb_to_oklab([px[0], px[1], px[2]]);
            let d = ((l[0] - lab[0]).powi(2) + (l[1] - lab[1]).powi(2) + (l[2] - lab[2]).powi(2)).sqrt();
            m.v[i] = ((tol - d) / (tol * 0.35).max(1e-3)).clamp(0.0, 1.0);
        }
        save_mask(Path::new(path), &m)?;
        keys.push(json!({"key": col, "tol": tol, "out": path, "coverage": m.coverage()}));
    }
    render::set_custom_dir(None);
    let v = json!({"ok": true, "out": a.out.display().to_string(), "size": [w, h], "frame": [pj.w, pj.h], "triangles": sel.len(), "texels_seen": seen,
        "seen_fraction": seen as f64 / (w as f64 * h as f64), "keys": keys, "notes": notes,
        "next": ["抠出的遮罩可直接用于 tex recolor --mask / glow --mask", "看不到的面(背面/被遮挡)没有像素: 换 --yaw 再烘一次并 --mask 合并, 或加 --mirror-x"]});
    if cli.json {
        emit(cli, &v);
    } else {
        println!("{v:#}");
    }
    Ok(0)
}

fn raster_screen<F: FnMut(usize, f32)>(s: &[V3; 3], w: u32, h: u32, mut f: F) {
    let [a, b, c] = *s;
    let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    if area.abs() < 1e-9 {
        return;
    }
    let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.0) as i64;
    let x1 = a[0].max(b[0]).max(c[0]).ceil().min(w as f32 - 1.0) as i64;
    let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.0) as i64;
    let y1 = a[1].max(b[1]).max(c[1]).ceil().min(h as f32 - 1.0) as i64;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = ((b[0] - px) * (c[1] - py) - (b[1] - py) * (c[0] - px)) / area;
            let w1 = ((c[0] - px) * (a[1] - py) - (c[1] - py) * (a[0] - px)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                continue;
            }
            f((y as u32 * w + x as u32) as usize, w0 * a[2] + w1 * b[2] + w2 * c[2]);
        }
    }
}

fn bilinear(img: &Image, x: f32, y: f32) -> [u8; 4] {
    let (x, y) = ((x - 0.5).max(0.0), (y - 0.5).max(0.0));
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(img.width - 1), (y0 + 1).min(img.height - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let (a, b, c, d) = (img.get(x0.min(img.width - 1), y0.min(img.height - 1)), img.get(x1, y0.min(img.height - 1)), img.get(x0.min(img.width - 1), y1), img.get(x1, y1));
    let mut o = [0u8; 4];
    for k in 0..4 {
        o[k] = ((a[k] as f32 * (1.0 - fx) + b[k] as f32 * fx) * (1.0 - fy) + (c[k] as f32 * (1.0 - fx) + d[k] as f32 * fx) * fy).round() as u8;
    }
    o
}

fn resize(img: &Image, w: u32, h: u32) -> Image {
    let mut o = Image::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let c = bilinear(img, (x as f32 + 0.5) * img.width as f32 / w as f32, (y as f32 + 0.5) * img.height as f32 / h as f32);
            let i = o.idx(x, y);
            o.px[i..i + 4].copy_from_slice(&c);
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seg_draws() {
        let mut m = vec![0f32; 64 * 64];
        draw_seg(&mut m, 64, 64, [0.1, 0.5], [0.9, 0.5], 4.0);
        assert!(m[32 * 64 + 32] > 0.9);
        assert!(m[10 * 64 + 32] == 0.0);
    }
    #[test]
    fn contour_parse() {
        let (ax, s, o, r) = parse_contour("y:1.5:0.2:-3:9").unwrap();
        assert_eq!((ax, s, o), (1, 1.5, 0.2));
        assert_eq!(r, Some((-3.0, 9.0)));
    }
    #[test]
    fn facing() {
        assert!(facing_ok(&Some("|x|>0.5".into()), [-0.9, 0.0, 0.1]).unwrap());
        assert!(!facing_ok(&Some("y>0.7".into()), [0.0, 0.2, 0.9]).unwrap());
    }
}

/// 设计工作表模板(与船/参考无关, 用占位符)
pub const TEMPLATES: &[(&str, &str)] = &[
    ("analysis", include_str!("../../../docs/templates/analysis.md")),
    ("breakdown", include_str!("../../../docs/templates/breakdown.md")),
    ("acceptance", include_str!("../../../docs/templates/acceptance.md")),
    ("book", include_str!("../../../docs/templates/book.md")),
    ("assets", include_str!("../../../docs/templates/assets.md")),
];

#[derive(Args, Debug)]
pub struct TemplateArgs {
    /// analysis(参考分析) | breakdown(拆解清单) | acceptance(分层验收清单) | book(设计书/拆解书) | assets(素材清单)
    pub name: String,
    /// 写到文件(不给则打印)
    #[arg(short, long)]
    pub out: Option<PathBuf>,
}

pub fn cmd_template(cli: &Cli, a: &TemplateArgs) -> Result<i32> {
    let Some((n, body)) = TEMPLATES.iter().find(|t| t.0 == a.name.as_str()) else {
        bail!("没有模板 {:?}; 可选: {}", a.name, TEMPLATES.iter().map(|t| t.0).collect::<Vec<_>>().join(", "));
    };
    if let Some(o) = &a.out {
        if let Some(d) = o.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(o, body)?;
    }
    if cli.json {
        emit(cli, &json!({"ok": true, "template": n, "out": a.out.as_ref().map(|p| p.display().to_string()), "content": if a.out.is_none() { Some(*body) } else { None }, "next": ["填写后按 wowsmod help design 的分层推进"]}));
    } else if a.out.is_none() {
        print!("{body}");
    } else {
        println!("已写模板 {n} → {}", a.out.as_ref().unwrap().display());
    }
    Ok(0)
}

// ---------------- 设计书 ----------------

/// 项目里设计书的约定位置
pub fn book_path(root: &Path) -> PathBuf {
    root.join("design").join("BOOK.md")
}

#[derive(Debug, Default)]
pub struct BookState {
    pub version: String,
    pub status: String,
    pub mode: String,
    pub ship: String,
    pub finalized: String,
    pub problems: Vec<String>,
}

fn field(text: &str, key: &str) -> String {
    for l in text.lines() {
        let l = l.trim().trim_start_matches(['-', '*', ' ']);
        if let Some(r) = l.strip_prefix(key) {
            let r = r.trim_start();
            if let Some(v) = r.strip_prefix(':').or_else(|| r.strip_prefix('：')) {
                return v.trim().to_string();
            }
        }
    }
    String::new()
}

/// 解析并检查设计书: 机读字段 + 必填节是否还留着 <占位符>
pub fn book_state(text: &str) -> BookState {
    let mut b = BookState {
        version: field(text, "版本"),
        status: field(text, "状态"),
        mode: field(text, "模式"),
        ship: field(text, "目标船"),
        finalized: field(text, "定稿"),
        problems: vec![],
    };
    if !regex::Regex::new(r"^v\d+(\.\d+)*$").unwrap().is_match(&b.version) {
        b.problems.push(format!("版本号格式应为 vN.N, 现在是 {:?}", b.version));
    }
    if !["草稿", "评审中", "已定稿"].contains(&b.status.as_str()) {
        b.problems.push(format!("状态应为 草稿/评审中/已定稿, 现在是 {:?}", b.status));
    }
    if !["theme", "original", "replicate"].contains(&b.mode.as_str()) {
        b.problems.push(format!("模式应为 theme/original/replicate, 现在是 {:?}", b.mode));
    }
    if b.ship.is_empty() || b.ship.contains('<') {
        b.problems.push("目标船未填".into());
    }
    // 必填节: 1 2 3 4; 节内不能有 <…> 占位符(注释除外)
    let no_comment = regex::Regex::new(r"(?s)<!--.*?-->").unwrap().replace_all(text, "");
    let ph = regex::Regex::new(r"<[^<>\n]{1,80}>").unwrap();
    let mut sec = String::new();
    let mut sub = String::new();
    for l in no_comment.lines() {
        if let Some(h) = l.strip_prefix("## ") {
            sec = h.trim().to_string();
            sub.clear();
            continue;
        }
        if let Some(h) = l.strip_prefix("### ") {
            sub = h.trim().to_string();
            continue;
        }
        // §1 只检查与 模式 对应的小节(### theme / ### original / ### replicate)
        let other_mode = sec.starts_with("1 ")
            && ["theme", "original", "replicate"].iter().any(|m| sub.starts_with(m) && *m != b.mode.as_str());
        let need = !other_mode && ["1 ", "2 ", "3 ", "4 "].iter().any(|p| sec.starts_with(p));
        if need {
            for m in ph.find_iter(l) {
                b.problems.push(format!("§{} 还有占位符 {}", sec, m.as_str()));
            }
        }
    }
    for s in ["## 1 ", "## 2 ", "## 3 ", "## 4 "] {
        if !no_comment.contains(s) {
            b.problems.push(format!("缺少章节 {s}…"));
        }
    }
    b
}

/// 执行类命令调用: 项目里有设计书但未定稿时给提示(不阻止)
pub fn draft_hint(root: &Path) {
    // 明确知情后继续执行(如复刻模式先按草稿做): WOWSMOD_BOOK_IGNORE=1 关掉提示
    if std::env::var("WOWSMOD_BOOK_IGNORE").map(|v| v == "1").unwrap_or(false) {
        return;
    }
    let p = book_path(root);
    let Ok(t) = std::fs::read_to_string(&p) else { return };
    let b = book_state(&t);
    if b.status != "已定稿" {
        eprintln!(
            "提示: 设计书 {} 状态为 {:?}(版本 {}), 尚未定稿。按工作流应先与用户迭代设计书、用户确认后 `wowsmod design-book finalize`, 再执行修改; 探索性预览可忽略本提示(知情继续: 设 WOWSMOD_BOOK_IGNORE=1)。",
            p.display(),
            b.status,
            b.version
        );
    }
}

#[derive(clap::Subcommand, Debug)]
pub enum BookCmd {
    /// 在项目里生成设计书 design/BOOK.md(theme/original 为设计书, replicate 为拆解书)
    Init {
        project: PathBuf,
        #[arg(long, value_parser = ["theme", "original", "replicate"])]
        mode: String,
        #[arg(long, default_value = "")]
        title: String,
        #[arg(long)]
        force: bool,
    },
    /// 检查设计书: 字段、版本号、必填节占位符、是否已定稿(未定稿退出码 2)
    Check {
        /// 项目目录或 .md 文件
        target: PathBuf,
    },
    /// 用户确认后定稿: 检查通过才写入 状态=已定稿 + 定稿日期/确认人
    Finalize {
        target: PathBuf,
        /// 确认人(写进 定稿 字段), 如 用户
        #[arg(long)]
        by: String,
    },
    /// 改稿: 版本号 +0.1, 状态回到 评审中, 记一行修订记录
    Revise {
        target: PathBuf,
        /// 本次改了什么 / 用户意见
        #[arg(long)]
        note: String,
    },
}

fn book_file(t: &Path) -> PathBuf {
    if t.is_dir() { book_path(t) } else { t.to_path_buf() }
}

fn set_field(text: &str, key: &str, val: &str) -> String {
    let re = regex::Regex::new(&format!(r"(?m)^{key}\s*[:：].*$")).unwrap();
    re.replace(text, format!("{key}: {val}").as_str()).into_owned()
}

fn bump(v: &str) -> String {
    let n: Vec<u32> = v.trim_start_matches('v').split('.').filter_map(|x| x.parse().ok()).collect();
    match n.as_slice() {
        [a, b, ..] => format!("v{a}.{}", b + 1),
        [a] => format!("v{a}.1"),
        _ => "v0.2".into(),
    }
}

fn today() -> String {
    std::process::Command::new(if cfg!(windows) { "powershell" } else { "date" })
        .args(if cfg!(windows) { vec!["-NoProfile", "-Command", "Get-Date -Format yyyy-MM-dd"] } else { vec!["+%Y-%m-%d"] })
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "<日期>".into())
}

pub fn cmd_book(cli: &Cli, c: &BookCmd) -> Result<i32> {
    match c {
        BookCmd::Init { project, mode, title, force } => {
            let p = book_path(project);
            if p.exists() && !force {
                bail!("{} 已存在(--force 覆盖)", p.display());
            }
            std::fs::create_dir_all(p.parent().unwrap())?;
            let ship = Project::open(project).map(|x| x.cfg.meta.ship).unwrap_or_default();
            let mut t = TEMPLATES.iter().find(|t| t.0 == "book").unwrap().1.to_string();
            t = set_field(&t, "模式", mode);
            if !ship.is_empty() {
                t = set_field(&t, "目标船", &ship);
            }
            if !title.is_empty() {
                t = t.replacen("<标题>", title, 1);
            }
            t = t.replacen("| v0.1 | <日期> |", &format!("| v0.1 | {} |", today()), 1);
            std::fs::write(&p, t)?;
            emit_or_print(cli, json!({"ok": true, "book": p.display().to_string(), "mode": mode,
                "next": ["填 §1(按模式)→§2 设计语言卡→§3 逐部件→§4 2~3 个方向预览, 交用户评审", "改稿用 wowsmod design-book revise --note …", "用户确认后 wowsmod design-book finalize --by 用户"]}),
                &format!("已生成设计书 → {}", p.display()));
            Ok(0)
        }
        BookCmd::Check { target } => {
            let p = book_file(target);
            let t = std::fs::read_to_string(&p).with_context(|| format!("读取 {}(先 wowsmod design-book init)", p.display()))?;
            let b = book_state(&t);
            let fin = b.status == "已定稿" && b.problems.is_empty();
            let r = json!({"ok": true, "book": p.display().to_string(), "version": b.version, "status": b.status, "mode": b.mode, "finalized": fin, "finalized_by": b.finalized, "problems": b.problems});
            if cli.json {
                emit(cli, &r);
            } else {
                println!("{}  版本 {}  状态 {}  模式 {}", p.display(), b.version, b.status, b.mode);
                for x in &b.problems {
                    println!("  ✗ {x}");
                }
                println!("{}", if fin { "✓ 已定稿, 可按分层执行" } else { "未定稿: 先完成并与用户评审" });
            }
            Ok(if fin { 0 } else { 2 })
        }
        BookCmd::Finalize { target, by } => {
            let p = book_file(target);
            let t = std::fs::read_to_string(&p)?;
            let b = book_state(&t);
            if !b.problems.is_empty() {
                bail!("设计书未通过检查, 不能定稿: {}", b.problems.join("; "));
            }
            let mut t = set_field(&t, "状态", "已定稿");
            t = set_field(&t, "定稿", &format!("{} {by} 确认 ({})", today(), b.version));
            std::fs::write(&p, t)?;
            emit_or_print(cli, json!({"ok": true, "book": p.display().to_string(), "version": b.version, "next": ["按 wowsmod help design 分层执行, 每层对照设计书 §2–§3 自查, 记到 §5"]}), &format!("已定稿 {} {}", p.display(), b.version));
            Ok(0)
        }
        BookCmd::Revise { target, note } => {
            let p = book_file(target);
            let t = std::fs::read_to_string(&p)?;
            let b = book_state(&t);
            let nv = bump(&b.version);
            let mut t = set_field(&t, "版本", &nv);
            t = set_field(&t, "状态", "评审中");
            t = set_field(&t, "定稿", "未定稿");
            let row = format!("| {nv} | {} | {} | |", today(), note.replace('|', "/"));
            // 插到修订记录表最后一行之后
            if let Some(i) = t.find("## 修订记录") {
                let after = &t[i..];
                let end = after.find("\n\n").map(|e| i + e).unwrap_or(t.len());
                t.insert_str(end, &format!("\n{row}"));
            }
            std::fs::write(&p, t)?;
            emit_or_print(cli, json!({"ok": true, "book": p.display().to_string(), "version": nv, "status": "评审中"}), &format!("改稿 → {nv}(评审中)"));
            Ok(0)
        }
    }
}

fn emit_or_print(cli: &Cli, v: Value, human: &str) {
    if cli.json {
        emit(cli, &v);
    } else {
        println!("{human}");
    }
}
