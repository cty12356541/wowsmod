//! model import: 外部网格(GLB/OBJ)作为新渲染集导入, 带独立材质(.mfm)与独立贴图(_a/_mg/_n[/_ao]), 可发光。
//!
//! 挂接:
//! - `--part <部件>`: 写进部件几何(该部件所有实例都有, 随炮塔/炮管转); 放置相对 `--node`(默认模板渲染集第一个节点);
//!   蒙皮(世界空间)模板时, 新顶点的骨骼绑定取"离新网格中心最近的模板顶点", 所以放在炮塔顶就跟炮塔回转, 放在炮管上就跟俯仰。
//! - `--hp HP_xxx`: 写进该挂点所在的船体分段, 放置相对挂点(位置+朝向); 不随炮塔转(要随转请用 --part)。
//! - `--section S`: 写进分段, 放置相对分段原点。
//! 放置: `--at x,y,z`(游戏坐标, 挂接节点局部) `--rot` 度 `--scale`; 网格本身按 glTF/OBJ 约定(y 上, 右手)。
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use wowsmod_formats::{
    dds::{Dds, Format},
    geomedit::GeoEdit,
    mfm,
    scene::{self, ScenePrim},
    texture::{self, Image, MipFilter, Quality},
    visual::{compose, mat_mul, Visual, IDENTITY, M4},
};

use super::{aabb_of_points, add_prims, convert_prim, find_template, inv, load_visuals, parse_vec3, sanitize, section_matrix, units_for, GeomSrc, GlbPrim, Target, Tx};
use crate::{meshsrc, texcmd};

pub struct ImportOpts<'a> {
    pub mesh: &'a Path,
    pub like: &'a str,
    pub node: Option<&'a str>,
    pub at: &'a str,
    pub rot: &'a str,
    pub scale: &'a str,
    pub name: Option<&'a str>,
    pub texture: Option<&'a Path>,
    pub color: Option<&'a str>,
    pub glow: Option<&'a Path>,
    pub glow_all: bool,
    pub power: f32,
    pub tex_size: u32,
    pub gloss: u8,
    pub lod0_only: bool,
    /// 发光 B 上限(遮罩满值处的 B, 0..255)
    pub b_max: u8,
    /// 材质模式: auto(GLB 有材质且没给 --texture/--color 时按材质拆) / single
    pub materials: &'a str,
    /// 这些材质(正则, 匹配 glTF 材质名)整块发光
    pub glow_mat: Option<&'a str>,
    /// 全部材质双面
    pub double_sided: bool,
    /// 全部材质 alpha 测试阈值(0..255)
    pub alpha_test: Option<u8>,
    /// 等比缩放到这个高度(glTF y 轴, visual 单位); 覆盖 --scale
    pub height: Option<f64>,
}

// ───────────── 网格读取 ─────────────

/// OBJ(v/vt/vn/f, 多边形扇形三角化, 负索引, 按 usemtl 分 primitive)
pub fn read_obj(text: &str) -> Result<Vec<ScenePrim>> {
    let (mut v, mut vt, mut vn) = (Vec::<[f32; 3]>::new(), Vec::<[f32; 2]>::new(), Vec::<[f32; 3]>::new());
    struct P {
        mat: Option<String>,
        pos: Vec<[f32; 3]>,
        uv: Vec<[f32; 2]>,
        nrm: Vec<[f32; 3]>,
        idx: Vec<u32>,
        map: std::collections::HashMap<(i64, i64, i64), u32>,
        has_uv: bool,
        has_n: bool,
    }
    let newp = |m: Option<String>| P { mat: m, pos: vec![], uv: vec![], nrm: vec![], idx: vec![], map: Default::default(), has_uv: true, has_n: true };
    let mut prims: Vec<P> = vec![newp(None)];
    let fl = |s: Option<&str>| -> Result<f32> { s.ok_or_else(|| anyhow!("OBJ 数值缺失"))?.parse::<f32>().map_err(|e| anyhow!("OBJ 数值: {e}")) };
    for (ln, line) in text.lines().enumerate() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => v.push([fl(it.next())?, fl(it.next())?, fl(it.next())?]),
            Some("vt") => vt.push([fl(it.next())?, fl(it.next())?]),
            Some("vn") => vn.push([fl(it.next())?, fl(it.next())?, fl(it.next())?]),
            Some("usemtl") => {
                let m = it.next().map(|s| s.to_string());
                if prims.last().map(|p| p.idx.is_empty()).unwrap_or(false) {
                    prims.last_mut().unwrap().mat = m;
                } else {
                    prims.push(newp(m));
                }
            }
            Some("f") => {
                let p = prims.last_mut().unwrap();
                let mut corners = Vec::new();
                for c in it {
                    let mut f = c.split('/');
                    let res = |s: Option<&str>, n: usize| -> Result<i64> {
                        match s {
                            None | Some("") => Ok(-1),
                            Some(x) => {
                                let i: i64 = x.parse().map_err(|_| anyhow!("OBJ 第 {} 行索引 {x}", ln + 1))?;
                                let k = if i < 0 { n as i64 + i } else { i - 1 };
                                if k < 0 || k as usize >= n {
                                    bail!("OBJ 第 {} 行索引越界 {x}", ln + 1);
                                }
                                Ok(k)
                            }
                        }
                    };
                    let key = (res(f.next(), v.len())?, res(f.next(), vt.len())?, res(f.next(), vn.len())?);
                    if key.0 < 0 {
                        bail!("OBJ 第 {} 行缺顶点索引", ln + 1);
                    }
                    let id = if let Some(&id) = p.map.get(&key) {
                        id
                    } else {
                        let id = p.pos.len() as u32;
                        p.pos.push(v[key.0 as usize]);
                        if key.1 >= 0 { p.uv.push(vt[key.1 as usize]) } else { p.has_uv = false; p.uv.push([0.0, 0.0]) }
                        if key.2 >= 0 { p.nrm.push(vn[key.2 as usize]) } else { p.has_n = false; p.nrm.push([0.0, 1.0, 0.0]) }
                        p.map.insert(key, id);
                        id
                    };
                    corners.push(id);
                }
                for k in 1..corners.len().saturating_sub(1) {
                    p.idx.extend([corners[0], corners[k], corners[k + 1]]);
                }
            }
            _ => {}
        }
    }
    let out: Vec<ScenePrim> = prims
        .into_iter()
        .filter(|p| !p.idx.is_empty())
        // OBJ 的 vt 原点在左下, glTF 在左上: v 翻转
        .map(|p| ScenePrim {
            uvs: p.has_uv.then(|| p.uv.iter().map(|u| [u[0], 1.0 - u[1]]).collect()),
            normals: p.has_n.then_some(p.nrm),
            positions: p.pos,
            indices: p.idx,
            material: p.mat,
            material_index: None,
            extras: Value::Null,
        })
        .collect();
    if out.is_empty() {
        bail!("OBJ 里没有面");
    }
    Ok(out)
}

/// 读网格(GLB/OBJ) → 各 primitive 用 `g`(glTF 世界 → 目标 visual 空间)转换
pub(super) fn mesh_prims(path: &Path, g: &M4) -> Result<Vec<(GlbPrim, Option<usize>)>> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let d = std::fs::read(path).map_err(|e| anyhow!("读取 {}: {e}", path.display()))?;
    let mut out = Vec::new();
    if ext == "obj" {
        for (i, p) in read_obj(&String::from_utf8_lossy(&d))?.iter().enumerate() {
            out.push((GlbPrim { node: format!("obj{i}"), material: p.material.clone(), mesh: convert_prim(&IDENTITY, g, p) }, None));
        }
    } else {
        let sc = scene::read_scene(&d).map_err(|e| anyhow!("{}: {e}", path.display()))?;
        for n in &sc.nodes {
            let Some(mi) = n.mesh else { continue };
            for p in &sc.meshes[mi].prims {
                out.push((GlbPrim { node: n.name.clone(), material: p.material.clone(), mesh: convert_prim(&n.world, g, p) }, p.material_index));
            }
        }
    }
    if out.is_empty() {
        bail!("{} 里没有网格", path.display());
    }
    // 没有 UV 时按包围盒做平面投影(最大的两个轴), 否则整块贴图只取一个像素
    for (gp, _) in out.iter_mut() {
        if gp.mesh.uvs.is_none() {
            if let Some((mn, mx)) = aabb_of_points(gp.mesh.positions.iter().map(|p| p.map(|x| x as f64))) {
                let ext: Vec<f64> = (0..3).map(|i| (mx[i] - mn[i]).max(1e-9)).collect();
                let mut ax = [0usize, 1, 2];
                ax.sort_by(|a, b| ext[*b].partial_cmp(&ext[*a]).unwrap());
                let (a, b) = (ax[0], ax[1]);
                gp.mesh.uvs = Some(gp.mesh.positions.iter().map(|p| [((p[a] as f64 - mn[a]) / ext[a]) as f32, ((p[b] as f64 - mn[b]) / ext[b]) as f32]).collect());
            }
        }
    }
    Ok(out)
}

/// 网格在 glTF 空间(节点世界矩阵后)的 y 向高度
pub(super) fn mesh_height(path: &Path) -> Result<f64> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let d = std::fs::read(path)?;
    let pts: Vec<[f64; 3]> = if ext == "obj" {
        read_obj(&String::from_utf8_lossy(&d))?.iter().flat_map(|p| p.positions.iter().map(|q| q.map(|x| x as f64))).collect()
    } else {
        let sc = scene::read_scene(&d).map_err(|e| anyhow!("{e}"))?;
        sc.nodes
            .iter()
            .filter_map(|n| n.mesh.map(|m| (n, m)))
            .flat_map(|(n, m)| sc.meshes[m].prims.iter().flat_map(move |p| p.positions.iter().map(move |q| scene::xform_gltf_point(&n.world, *q).map(|x| x as f64))))
            .collect()
    };
    let (mn, mx) = aabb_of_points(pts.into_iter()).ok_or_else(|| anyhow!("网格没有顶点"))?;
    Ok(mx[1] - mn[1])
}

// ───────────── 贴图 ─────────────

fn resize(img: &Image, w: u32, h: u32) -> Image {
    let mut o = Image::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * img.width as f32 / w as f32 - 0.5).clamp(0.0, img.width as f32 - 1.0);
            let fy = ((y as f32 + 0.5) * img.height as f32 / h as f32 - 0.5).clamp(0.0, img.height as f32 - 1.0);
            let (x0, y0) = (fx.floor() as u32, fy.floor() as u32);
            let (x1, y1) = ((x0 + 1).min(img.width - 1), (y0 + 1).min(img.height - 1));
            let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
            for c in 0..4 {
                let p = |xx: u32, yy: u32| img.px[((yy * img.width + xx) * 4 + c) as usize] as f32;
                let v = p(x0, y0) * (1.0 - tx) * (1.0 - ty) + p(x1, y0) * tx * (1.0 - ty) + p(x0, y1) * (1.0 - tx) * ty + p(x1, y1) * tx * ty;
                o.px[((y * w + x) * 4 + c) as usize] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    o
}

fn solid(w: u32, h: u32, c: [u8; 4]) -> Image {
    let mut o = Image::new(w, h);
    for p in o.px.chunks_exact_mut(4) {
        p.copy_from_slice(&c);
    }
    o
}

/// 按模板贴图的格式/sRGB 编码一张新贴图(单文件, 完整 mip 链)
fn encode_like(img: &Image, like: Option<&texcmd::Tex>, fallback: Format, filter: MipFilter, need_alpha: bool) -> Result<(Vec<u8>, String)> {
    let (mut fmt, srgb) = match like {
        Some(t) => (t.dds.header.format, matches!(t.dds.header.dxgi, Some(29 | 72 | 75 | 78 | 91 | 93 | 99))),
        None => (fallback, false),
    };
    if fmt.block_bytes().is_none() && !matches!(fmt, Format::Rgba8 | Format::Bgra8) {
        fmt = fallback;
    }
    // alpha 测试要 alpha 通道: BC1 只有 1 位 alpha, 换 BC3
    if need_alpha && matches!(fmt, Format::BC1) {
        fmt = Format::BC3;
    }
    let chain = texture::mip_chain(img.clone(), 0, filter);
    let alpha = chain.iter().any(texture::has_alpha);
    let enc = chain.iter().map(|l| texture::encode_surface(l, fmt, Quality::Normal, Some(alpha))).collect::<std::result::Result<Vec<_>, _>>().map_err(|e| anyhow!("编码: {e}"))?;
    let d = Dds::new_2d(fmt, img.width, img.height, enc, srgb).map_err(|e| anyhow!("{e}"))?;
    Ok((d.to_bytes(), format!("{}{}", fmt.name(), if srgb { " sRGB" } else { "" })))
}

/// PnFMods/<mod>/<ship>/rel → 项目里的文件
fn proj_file(root: &Path, res_path: &str) -> Result<PathBuf> {
    let parts: Vec<&str> = res_path.trim().split('/').collect();
    if parts.len() < 4 || parts[0] != "PnFMods" {
        bail!("模板材质 {res_path} 不在本项目(PnFMods/<mod>/<ship>/...)里; 换一个本项目的渲染集做 --like");
    }
    let mut p = root.to_path_buf();
    for c in &parts[3..] {
        if *c == ".." {
            bail!("不安全的路径 {res_path}");
        }
        p.push(c);
    }
    Ok(p)
}

pub fn import(tx: &mut Tx, root: &Path, src: &GeomSrc, t: &Target, o: &ImportOpts) -> Result<Value> {
    // 1) 目标单元与挂接矩阵(挂接节点局部 → 单元 visual 空间)
    let mut t2 = t.clone();
    let mut attach = json!(null);
    let mut base_m = IDENTITY;
    if let Some(hp) = &t.hp {
        if t.part.is_some() {
            bail!("--hp 与 --part 只能二选一(随炮塔转用 --part, 固定在船体挂点用 --hp)");
        }
        let (master, mv) = wowsmod_formats::assemble::master_visual(root).map_err(|e| anyhow!("{e}"))?;
        let (_, hps) = wowsmod_formats::assemble::hardpoints(root, &master, &mv).map_err(|e| anyhow!("{e}"))?;
        let (sec, w) = hps.get(hp).cloned().ok_or_else(|| anyhow!("没有挂点 {hp}(wowsmod model list 看 hardpoints)"))?;
        base_m = mat_mul(&w, &inv(&section_matrix(root, &sec)?)?);
        t2.section = Some(sec.clone());
        attach = json!({"hp": hp, "section": sec, "hp_world_pos": [w[3][0], w[3][1], w[3][2]]});
    } else if t.part.is_none() && t.section.is_none() {
        bail!("import 需要 --part <部件> / --hp <挂点> / --section <分段> 之一");
    }
    let u = units_for(root, &t2)?.into_iter().next().ok_or_else(|| anyhow!("没有目标"))?;
    let mut ge = GeoEdit::parse(&tx.geometry(&u.geometry, src)?).map_err(|e| anyhow!("{e}"))?;
    let mut vis = load_visuals(tx, &u.visuals)?;
    let tk = find_template(&vis[0], o.like)?;
    let tpl = vis[0].render_sets[tk].clone();
    let tname = tpl.base_name();
    if t.hp.is_none() {
        let nn = o.node.map(|s| s.to_string()).or_else(|| tpl.nodes.first().cloned()).unwrap_or_default();
        if !nn.is_empty() {
            let n = vis[0].node(&nn).ok_or_else(|| anyhow!("{} 里没有节点 {nn}", u.name))?;
            base_m = n.world;
        }
        attach = json!({"unit": u.kind, "name": u.name, "node": nn});
    }
    // 放置: 局部 TRS, 再乘挂接矩阵
    let mut s = parse_vec3(o.scale)?;
    let mut fit = json!(null);
    if let Some(h) = o.height {
        let mh = mesh_height(o.mesh)?;
        if mh <= 1e-9 {
            bail!("网格高度为 0, 不能 --height");
        }
        let k = h / mh;
        s = [k; 3];
        fit = json!({"mesh_height": mh, "target_height": h, "scale": k});
    }
    let place = compose(parse_vec3(o.at)?, parse_vec3(o.rot)?, s);
    let g = mat_mul(&place, &base_m);
    let prims_m = mesh_prims(o.mesh, &g)?;

    let base = o.name.map(sanitize).unwrap_or_else(|| format!("wm_{}", sanitize(&o.mesh.file_stem().unwrap().to_string_lossy())));
    let tpl_mfm_res = tpl.mfm.clone();
    let MatBuild { groups, mats_out, warn, n_written, n_shared, per_mat } = build_materials(tx, root, src, &tpl_mfm_res, None, o, &prims_m, &base)?;

    // 3) 几何: 用模板渲染集的格式/节点; 蒙皮模板时骨骼绑定取最近的模板顶点
    let mut per_tpl: Option<Vec<u8>> = None;
    let mut bind = json!(null);
    if tpl.world_space {
        let all: Vec<[f32; 3]> = prims_m.iter().flat_map(|(p, _)| p.mesh.positions.clone()).collect();
        let n = all.len().max(1) as f64;
        let c: [f64; 3] = std::array::from_fn(|i| all.iter().map(|p| p[i] as f64).sum::<f64>() / n);
        let md = ge.mesh(&tname).map_err(|e| anyhow!("{e}"))?;
        let (bi, bd) = md.positions.iter().enumerate().map(|(i, p)| (i, (0..3).map(|k| (p[k] as f64 - c[k]).powi(2)).sum::<f64>())).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).ok_or_else(|| anyhow!("模板没有顶点"))?;
        let rv = ge.raw_vertex(&tname, bi).map_err(|e| anyhow!("{e}"))?;
        let hex: String = rv.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
        per_tpl = Some(rv);
        bind = json!({"nearest_template_vertex": bi, "template_vertex_pos": md.positions[bi], "template_vertex_bytes": hex, "bones": tpl.nodes, "distance": bd.sqrt(), "center": c, "note": "蒙皮模板: 新网格的骨骼绑定 = 该模板顶点的骨骼(放在哪块就跟哪块动)"});
    }
    let mut added = Vec::new();
    for (gp, stem, mres, _) in &groups {
        added.extend(add_prims_with(&mut ge, &mut vis, gp, stem, tk, o.lod0_only, per_tpl.as_deref(), mres)?);
    }
    // 发光: 新材质标识改成 EMISSIVE 族
    let mut idents = Vec::new();
    for (_, _, mres, glowing) in &groups {
        if !glowing {
            continue;
        }
        let mname = mres.rsplit('/').next().unwrap_or("").to_string();
        for v in vis.iter_mut() {
            let (nt, ch) = mfm::set_emissive_identifiers(&v.text, &mname);
            if !ch.is_empty() {
                *v = Visual::parse(&nt).map_err(|e| anyhow!("{e}"))?;
                idents.extend(ch.into_iter().map(|(a, b)| json!([a, b])));
            }
        }
    }
    let geo = ge.finish().map_err(|e| anyhow!("{e}"))?;
    tx.write(&u.geometry, geo.serialize());
    for (vp, v) in u.visuals.iter().zip(&vis) {
        tx.write_visual(vp, v);
    }
    Ok(json!({"unit": u.kind, "name": u.name, "attach": attach, "template": tname, "mesh": o.mesh.display().to_string(),
        "placement": {"at": o.at, "rot_deg": o.rot, "scale": s, "fit_height": fit}, "added": added, "skin_binding": bind,
        "material_mode": if per_mat { "per-material" } else { "single" },
        "materials": mats_out, "identifiers_changed": idents,
        "textures_written": n_written, "texture_reuses": n_shared, "warnings": warn,
        "notes": [
            if u.kind == "part" { "写进部件几何: 该部件的所有实例都会出现(如 3 座主炮塔各一份)" } else { "写进船体分段: 固定在船体上" },
            if o.lod0_only { "只加到 LOD0, 远处会消失" } else { "加到全部 LOD visual" },
            "只改渲染网格, 不改装甲/碰撞"
        ]}))
}

/// 材质构建结果: 每组 (primitive, 渲染集基名, mfm 资源, 是否发光)
pub(super) struct MatBuild {
    pub groups: Vec<(Vec<GlbPrim>, String, String, bool)>,
    pub mats_out: Vec<Value>,
    pub warn: Vec<String>,
    pub n_written: usize,
    pub n_shared: usize,
    pub per_mat: bool,
}

/// 2) 材质: 每组 primitive 一个独立 .mfm(复制模板 `tpl_mfm_res`), 贴图另建; 相同内容的贴图只写一份。
/// `out_dir`: (资源目录, 项目路径) 覆盖输出位置(默认模板 .mfm 所在目录)
#[allow(clippy::too_many_arguments)]
pub(super) fn build_materials(tx: &mut Tx, root: &Path, src: &GeomSrc, tpl_mfm_res: &str, out_dir: Option<(&str, &Path)>, o: &ImportOpts, prims_m: &[(GlbPrim, Option<usize>)], base: &str) -> Result<MatBuild> {
    let tpl_mfm_res = tpl_mfm_res.trim();
    let base = base.to_string();
    let tpl_mfm_path = proj_file(root, tpl_mfm_res)?;
    let mtext = tx.read_text(&tpl_mfm_path)?;
    let (dir_res, dir_path) = match out_dir {
        Some((r, p)) => (r.to_string(), p.to_path_buf()),
        None => (tpl_mfm_res.trim().rsplit_once('/').map(|x| x.0.to_string()).unwrap_or_default(), tpl_mfm_path.parent().unwrap().to_path_buf()),
    };
    let tpl_mfm_name = tpl_mfm_path.file_name().unwrap().to_string_lossy().to_string();
    let props = mfm::properties(&mtext);
    let ctx = src.game();
    let sz = o.tex_size.clamp(16, 4096).next_power_of_two();
    let is_glb = !o.mesh.extension().map(|e| e.eq_ignore_ascii_case("obj")).unwrap_or(false);
    let gm = if is_glb { Some(meshsrc::read_glb_materials(&std::fs::read(o.mesh)?, o.mesh.parent())?) } else { None };
    let per_mat = match o.materials {
        "single" => false,
        "auto" | "split" => gm.as_ref().map(|g| !g.mats.is_empty()).unwrap_or(false) && o.texture.is_none() && o.color.is_none(),
        m => bail!("--materials 只能是 auto(按材质, 相同贴图/设置的合并) / split(不合并) / single: {m}"),
    };
    let glow_re = o.glow_mat.map(regex::Regex::new).transpose().map_err(|e| anyhow!("--glow-mat 正则: {e}"))?;
    let user_glow: Option<Image> = match o.glow {
        Some(p) => Some(texture::read_png(&std::fs::read(p)?).map_err(|e| anyhow!("{}: {e}", p.display()))?),
        None => None,
    };
    // 每个材质的规格
    struct Spec {
        key: String,
        albedo: Image,
        glow: Option<Image>,
        alpha_ref: Option<u8>,
        double_sided: bool,
        src: Value,
        prims: Vec<GlbPrim>,
    }
    let mut specs: Vec<Spec> = Vec::new();
    let mut warn: Vec<String> = Vec::new();
    let pow2 = |x: u32| x.clamp(1, sz).next_power_of_two().min(sz);
    if per_mat {
        let gm = gm.as_ref().unwrap();
        warn.extend(gm.notes.iter().cloned());
        let mut dec: std::collections::HashMap<usize, Option<Image>> = Default::default();
        let mut decode = |i: usize, warn: &mut Vec<String>| -> Option<Image> {
            dec.entry(i)
                .or_insert_with(|| {
                    let b = gm.images.get(i).cloned().flatten()?;
                    match meshsrc::decode_image(&b) {
                        Ok(im) => Some(im),
                        Err(e) => {
                            warn.push(format!("图片 {} 解码失败: {e}", gm.image_names.get(i).cloned().unwrap_or_default()));
                            None
                        }
                    }
                })
                .clone()
        };
        // 按材质分组(保持出现顺序)
        let mut order: Vec<Option<usize>> = Vec::new();
        for (_, mi) in prims_m {
            if !order.contains(mi) {
                order.push(*mi);
            }
        }
        let mut used_keys = std::collections::HashSet::new();
        for mi in order {
            let gmat = mi.and_then(|i| gm.mats.get(i));
            let raw = gmat.map(|m| m.name.clone()).unwrap_or_else(|| "default".into());
            let mut key: String = sanitize(&raw).chars().take(24).collect();
            if key.trim_matches('_').is_empty() {
                key = format!("m{}", mi.map(|x| x as i64).unwrap_or(-1).max(0));
            }
            let mut k2 = key.clone();
            let mut n = 1;
            while !used_keys.insert(k2.clone()) {
                n += 1;
                k2 = format!("{key}{n}");
            }
            let key = k2;
            let prims: Vec<GlbPrim> = prims_m.iter().filter(|(_, m)| *m == mi).map(|(p, _)| p.clone()).collect();
            let (mut albedo, mut alpha_ref, mut ds, mut src) = (solid(64, 64, [128, 128, 128, 255]), None, false, json!({"gltf_material": raw}));
            let mut glow: Option<Image> = None;
            if let Some(m) = gmat {
                warn.extend(m.notes.iter().map(|x| format!("{raw}: {x}")));
                let f = m.base_factor;
                albedo = match m.base_image.and_then(|i| decode(i, &mut warn)) {
                    Some(im) => resize(&im, pow2(im.width), pow2(im.height)),
                    None => solid(64, 64, [255, 255, 255, 255]),
                };
                for p in albedo.px.chunks_exact_mut(4) {
                    for c in 0..3 {
                        // 系数作用在线性空间: 近似为 sRGB 值 × 系数^(1/2.2)
                        p[c] = (p[c] as f32 * f[c].clamp(0.0, 1.0).powf(1.0 / 2.2)).round() as u8;
                    }
                    p[3] = (p[3] as f32 * f[3].clamp(0.0, 1.0)).round() as u8;
                }
                match m.alpha_mode.as_str() {
                    "MASK" => alpha_ref = Some((m.alpha_cutoff.clamp(0.0, 1.0) * 255.0).round() as u8),
                    "BLEND" => {
                        alpha_ref = Some(128);
                        warn.push(format!("{raw}: alphaMode BLEND 游戏船体材质没有半透明, 按 alpha 测试(阈值 128)处理"));
                    }
                    _ => {}
                }
                ds = m.double_sided;
                // glTF 自发光 → 遮罩(亮度); 游戏里发光颜色 = 漫反射颜色
                let ef = m.emissive_factor;
                let emax = ef.iter().cloned().fold(0.0f32, f32::max);
                if emax > 0.0 {
                    let mut mask = match m.emissive_image.and_then(|i| decode(i, &mut warn)) {
                        Some(im) => resize(&im, albedo.width, albedo.height),
                        None => solid(albedo.width, albedo.height, [255, 255, 255, 255]),
                    };
                    for p in mask.px.chunks_exact_mut(4) {
                        for c in 0..3 {
                            p[c] = (p[c] as f32 * ef[c].clamp(0.0, 1.0)).round() as u8;
                        }
                        p[3] = 255;
                    }
                    glow = Some(mask);
                    warn.push(format!("{raw}: glTF 自发光已转成发光遮罩; 游戏里发光颜色取漫反射颜色, 不是 emissive 颜色"));
                }
                src = json!({"gltf_material": raw, "baseColorFactor": f, "base_image": m.base_image.map(|i| gm.image_names.get(i).cloned()), "alphaMode": m.alpha_mode, "alphaCutoff": m.alpha_cutoff, "doubleSided": m.double_sided, "emissiveFactor": ef});
            }
            if o.glow_all || glow_re.as_ref().map(|r| r.is_match(&raw)).unwrap_or(false) {
                glow = Some(solid(albedo.width, albedo.height, [255, 255, 255, 255]));
            }
            if let Some(ug) = &user_glow {
                glow = Some(resize(ug, albedo.width, albedo.height));
            }
            specs.push(Spec { key, albedo, glow, alpha_ref, double_sided: ds, src, prims });
        }
    } else {
        let mut albedo = match (o.texture, o.color) {
            (Some(p), _) => {
                let im = meshsrc::decode_image(&std::fs::read(p).map_err(|e| anyhow!("读取 {}: {e}", p.display()))?).map_err(|e| anyhow!("{}: {e}", p.display()))?;
                resize(&im, sz, sz)
            }
            (None, Some(c)) => {
                let c = wowsmod_formats::color::parse_color(c).ok_or_else(|| anyhow!("颜色格式: #RRGGBB: {c}"))?;
                solid(sz, sz, [c[0], c[1], c[2], 255])
            }
            (None, None) => solid(sz, sz, [128, 128, 128, 255]),
        };
        if o.alpha_test.is_none() {
            for p in albedo.px.chunks_exact_mut(4) {
                p[3] = 255;
            }
        }
        let glow = match (&user_glow, o.glow_all) {
            (Some(g), _) => Some(resize(g, sz, sz)),
            (None, true) => Some(solid(sz, sz, [255, 255, 255, 255])),
            _ => None,
        };
        let prims = prims_m.iter().map(|(p, _)| p.clone()).collect();
        specs.push(Spec { key: String::new(), albedo, glow, alpha_ref: None, double_sided: false, src: json!({"mode": "single"}), prims });
    }
    for sp in specs.iter_mut() {
        if o.double_sided {
            sp.double_sided = true;
        }
        if let Some(a) = o.alpha_test {
            sp.alpha_ref = Some(a);
        }
        if sp.alpha_ref.is_none() {
            sp.albedo.px.chunks_exact_mut(4).for_each(|p| p[3] = 255);
        }
    }
    // 合并: 贴图/发光/双面/镂空都相同的材质渲染结果一样, 合成一个 .mfm(少 draw call; PMX 常有几十个同图材质)
    if per_mat && o.materials != "split" && specs.len() > 1 {
        let sig = |sp: &Spec| format!("{}|{}|{:?}|{}", meshsrc::image_hash(&sp.albedo), sp.glow.as_ref().map(meshsrc::image_hash).unwrap_or_default(), sp.alpha_ref, sp.double_sided);
        let mut merged: Vec<Spec> = Vec::new();
        let mut sigs: Vec<String> = Vec::new();
        for sp in specs.drain(..) {
            let g = sig(&sp);
            if let Some(i) = sigs.iter().position(|x| *x == g) {
                let m = &mut merged[i];
                m.prims.extend(sp.prims);
                let mut list = m.src.get("merged").and_then(|v| v.as_array().cloned()).unwrap_or_else(|| vec![m.src.clone()]);
                list.push(sp.src);
                m.src = json!({"merged": list});
            } else {
                sigs.push(g);
                merged.push(sp);
            }
        }
        specs = merged;
    }
    // 贴图: 先算全部(属性, 图)并按内容去重, 再命名写出
    let flat_n = solid(64, 64, [128, 128, 255, 128]);
    let flat_ao = solid(64, 64, [255, 255, 255, 255]);
    let mut per_spec: Vec<Vec<(&'static str, &'static str, Image, Format, MipFilter)>> = Vec::new();
    for sp in &specs {
        let mg = match &sp.glow {
            Some(gm) => {
                let mut mg = solid(gm.width, gm.height, [0, o.gloss, 0, 255]);
                for (d, s) in mg.px.chunks_exact_mut(4).zip(gm.px.chunks_exact(4)) {
                    let l = (s[0] as u32 * 2126 + s[1] as u32 * 7152 + s[2] as u32 * 722) / 10000;
                    d[2] = ((l * s[3] as u32 / 255) * o.b_max as u32 / 255) as u8;
                }
                mg
            }
            None => solid(64, 64, [0, o.gloss, 0, 255]),
        };
        per_spec.push(vec![
            ("diffuseMap", "_a", sp.albedo.clone(), Format::BC7, MipFilter::BoxSrgb),
            ("metallicGlossMap", "_mg", mg, Format::BC7, MipFilter::BoxLinear),
            ("normalMap", "_n", flat_n.clone(), Format::BC3, MipFilter::BoxLinear),
            ("ambientOcclusionMap", "_ao", flat_ao.clone(), Format::BC1, MipFilter::BoxLinear),
        ]);
    }
    let mut users: std::collections::HashMap<(String, String), Vec<usize>> = Default::default();
    for (i, v) in per_spec.iter().enumerate() {
        for (prop, _, img, _, _) in v {
            users.entry((prop.to_string(), meshsrc::image_hash(img))).or_default().push(i);
        }
    }
    let mut written: std::collections::HashMap<(String, String), (String, String)> = Default::default();
    let mut mats_out = Vec::new();
    let mut groups: Vec<(Vec<GlbPrim>, String, String, bool)> = Vec::new(); // (prims, 渲染集基名, mfm 资源, 发光)
    let (mut n_written, mut n_shared) = (0usize, 0usize);
    for (i, sp) in specs.iter().enumerate() {
        let stem = if sp.key.is_empty() { base.clone() } else { format!("{base}_{}", sp.key) };
        let new_mfm_name = format!("{stem}.mfm");
        let mut ntext = mtext.replace(&format!("<{tpl_mfm_name}>"), &format!("<{new_mfm_name}>")).replace(&format!("</{tpl_mfm_name}>"), &format!("</{new_mfm_name}>"));
        let mut texs = Vec::new();
        for (prop, suffix, img, fb, filt) in &per_spec[i] {
            let Some((_, _, old)) = props.iter().find(|(k, ty, _)| k == prop && ty == "Texture") else { continue };
            let key = (prop.to_string(), meshsrc::image_hash(img));
            let shared_by = users.get(&key).map(|v| v.len()).unwrap_or(1);
            let (res, fmt) = if let Some(w) = written.get(&key) {
                n_shared += 1;
                w.clone()
            } else {
                let fname = old.trim().rsplit('/').next().unwrap_or("").to_string();
                let like = texcmd::load_tex(old.trim(), ctx, Some(tx.p), false).ok().or_else(|| texcmd::load_tex(&fname, ctx, None, true).ok());
                let (bytes, fmt) = encode_like(img, like.as_ref(), *fb, *filt, *prop == "diffuseMap" && sp.alpha_ref.is_some())?;
                // 多个材质共用的贴图用基名命名, 否则用材质名
                let nf = if shared_by > 1 && !sp.key.is_empty() { format!("{base}_shared{}{suffix}.dds", written.values().filter(|(r, _)| r.contains("_shared") && r.ends_with(&format!("{suffix}.dds"))).count()) } else { format!("{stem}{suffix}.dds") };
                tx.write(&dir_path.join(&nf), bytes);
                n_written += 1;
                let r = (format!("{dir_res}/{nf}"), fmt);
                written.insert(key, r.clone());
                r
            };
            ntext = mfm::set_property(&ntext, prop, "Texture", &res).map_err(|e| anyhow!("{e}"))?;
            texs.push(json!({"property": prop, "file": res, "format": fmt, "size": [img.width, img.height], "shared_by_materials": shared_by, "replaced": old.trim()}));
        }
        if sp.double_sided {
            ntext = mfm::set_property(&ntext, "doubleSided", "Bool", "true").map_err(|e| anyhow!("{e}"))?;
        }
        if let Some(a) = sp.alpha_ref {
            ntext = mfm::set_property(&ntext, "alphaTestEnable", "Bool", "true").map_err(|e| anyhow!("{e}"))?;
            ntext = mfm::set_property(&ntext, "alphaReference", "Int", &a.to_string()).map_err(|e| anyhow!("{e}"))?;
        }
        let glowing = sp.glow.is_some();
        let mut fx_after = mfm::fx(&ntext).unwrap_or_default();
        if glowing {
            let nfx = mfm::emissive_variant(&fx_after).ok_or_else(|| anyhow!("模板着色器 {fx_after} 没有已知的 emissive 变体, 换一个模板(--like)"))?;
            ntext = mfm::set_fx(&ntext, &nfx).map_err(|e| anyhow!("{e}"))?;
            ntext = mfm::set_property(&ntext, "emissivePower", "Float", &format!("{:.4}", o.power)).map_err(|e| anyhow!("{e}"))?;
            fx_after = nfx;
        }
        let new_mfm_res = format!("{dir_res}/{new_mfm_name}");
        tx.write(&dir_path.join(&new_mfm_name), ntext.into_bytes());
        mats_out.push(json!({"mfm": new_mfm_res, "from": tpl_mfm_res, "fx": fx_after, "emissive": glowing, "emissivePower": glowing.then_some(o.power), "b_max": glowing.then_some(o.b_max),
            "doubleSided": sp.double_sided, "alphaReference": sp.alpha_ref, "source": sp.src, "textures": texs}));
        groups.push((sp.prims.clone(), stem, new_mfm_res, glowing));
    }
    Ok(MatBuild { groups, mats_out, warn, n_written, n_shared, per_mat })
}

/// add_prims 的变体: 可给骨骼绑定模板顶点与新材质
#[allow(clippy::too_many_arguments)]
fn add_prims_with(ge: &mut GeoEdit, vis: &mut [Visual], prims: &[GlbPrim], base: &str, tpl_k: usize, lod0_only: bool, tpl_vertex: Option<&[u8]>, mfm_res: &str) -> Result<Vec<Value>> {
    // 先按原逻辑加(用模板材质), 再把新渲染集的材质换成新 .mfm
    let only_tpl: Vec<GlbPrim> = prims.iter().map(|p| GlbPrim { node: p.node.clone(), material: None, mesh: p.mesh.clone() }).collect();
    let mut out = add_prims(ge, vis, &only_tpl, base, tpl_k, lod0_only, tpl_vertex)?;
    for r in out.iter_mut() {
        let name = r["renderSet"].as_str().unwrap_or_default().to_string();
        for v in vis.iter_mut() {
            if let Some(k) = v.render_sets.iter().position(|x| x.base_name() == name) {
                let ident = v.render_sets[k].material_identifier.clone();
                v.set_render_set_material(k, &ident, mfm_res).map_err(|e| anyhow!("{e}"))?;
            }
        }
        // 记录里写实际使用的新材质(模板材质另记)
        r["template_mfm"] = r["mfm"].clone();
        r["mfm"] = json!(mfm_res);
    }
    Ok(out)
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn obj_quad_fan_and_negative_index() {
        let t = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nusemtl A\nf 1/1 2/2 3/3 4/4\nusemtl B\nf -4/-4 -2/-2 -1/-1\n";
        let p = read_obj(t).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].indices, vec![0, 1, 2, 0, 2, 3]);
        assert_eq!(p[0].material.as_deref(), Some("A"));
        assert_eq!(p[1].positions, vec![[0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]]);
        assert_eq!(p[0].uvs.as_ref().unwrap()[2], [1.0, 0.0]); // v 翻转
        assert!(p[0].normals.is_none());
    }
}
