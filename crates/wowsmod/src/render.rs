//! M6 出图: 不依赖游戏/GPU 的软件光栅预览(近似), 供人和 AI 快速看效果。
//!
//! - 3D: 用 `assemble --full` 得到整船 GLB(带 UV 与每个 primitive 的 .mfm), 按 .mfm 的 diffuseMap 贴图,
//!   SHIPMAT_EMISSIVE_* 材质按 _mg 的 B 通道叠加发光; 正交投影 + 面法线 Lambert + 2x 超采样。
//!   --compare 时上半是原版(项目里的 SDK 副本 + 游戏原版贴图), 下半是项目当前状态。
//! - 贴图: --texture <名> 原版 | 项目 左右并排。
//! - --engine preview(M5 React 预览的 WebGL 无头渲染, API.md §11)只留了接口, 尚未接通。
//! 近似: 没有法线贴图/AO/PBR/迷彩/海面光照, 颜色只适合比较"改没改、改成什么色", 港口实际观感以进港截图为准。
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, ValueEnum};
use serde_json::{json, Value};

use wowsmod_formats::{glb, mfm, texture::{self, Image}};

use crate::modproj::Project;
use crate::texcmd;
use crate::{emit, game_ctx, Cli, GameCtx};

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum, Debug)]
pub enum View {
    /// 右舷正侧面(船艏朝右)
    Side,
    /// 俯视(船艏朝右)
    Top,
    /// 右前上方 3/4 视角
    Quarter,
    /// side + quarter + top 三张竖排
    All,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum, Debug)]
pub enum Engine {
    /// 内置软件光栅(默认, 不需要游戏运行/GPU)
    Soft,
    /// M5 React 预览的 WebGL 无头渲染(接口保留, 尚未接通)
    Preview,
}

#[derive(Args, Clone, Debug)]
pub struct RenderArgs {
    /// mod 项目目录
    pub project: PathBuf,
    /// 视角
    #[arg(long, value_enum, default_value_t = View::Quarter)]
    pub view: View,
    /// 上下对比: 上=原版, 下=项目当前
    #[arg(long)]
    pub compare: bool,
    /// 只看一张贴图(文件名, 如 FSB025_Bourgogne_1945_Hull_a.dds): 左=原版, 右=项目
    #[arg(long, value_name = "NAME")]
    pub texture: Option<String>,
    /// 输出 PNG(默认 <项目>/.render/<视角>.png)
    #[arg(short, long)]
    pub out: Option<PathBuf>,
    /// 输出宽度(像素)
    #[arg(long, default_value_t = 1600)]
    pub width: u32,
    /// 渲染引擎
    #[arg(long, value_enum, default_value_t = Engine::Soft)]
    pub engine: Engine,
}

// ---------- 小向量 ----------
type V3 = [f32; 3];
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 1e-12 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [0.0, 1.0, 0.0]
    }
}
type M4 = [f32; 16]; // 列主序(glTF)
const IDENT: M4 = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];
fn mul(a: &M4, b: &M4) -> M4 {
    let mut r = [0f32; 16];
    for c in 0..4 {
        for row in 0..4 {
            r[c * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[c * 4 + k]).sum();
        }
    }
    r
}
fn xform(m: &M4, p: V3) -> V3 {
    [m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12], m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13], m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14]]
}
fn node_local(n: &Value) -> M4 {
    if let Some(a) = n["matrix"].as_array() {
        let mut m = IDENT;
        for (i, x) in a.iter().take(16).enumerate() {
            m[i] = x.as_f64().unwrap_or(0.0) as f32;
        }
        return m;
    }
    let f = |k: &str, d: &[f32]| -> Vec<f32> { n[k].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_else(|| d.to_vec()) };
    let t = f("translation", &[0., 0., 0.]);
    let q = f("rotation", &[0., 0., 0., 1.]);
    let s = f("scale", &[1., 1., 1.]);
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let r = [
        1. - 2. * (y * y + z * z), 2. * (x * y + z * w), 2. * (x * z - y * w), 0.,
        2. * (x * y - z * w), 1. - 2. * (x * x + z * z), 2. * (y * z + x * w), 0.,
        2. * (x * z + y * w), 2. * (y * z - x * w), 1. - 2. * (x * x + y * y), 0.,
        0., 0., 0., 1.,
    ];
    let mut m = r;
    for c in 0..3 {
        for row in 0..3 {
            m[c * 4 + row] *= s[c];
        }
    }
    m[12] = t[0];
    m[13] = t[1];
    m[14] = t[2];
    m
}

/// mesh 索引 → 世界矩阵列表(同一 mesh 可被多个节点引用)
fn mesh_instances(j: &Value) -> HashMap<usize, Vec<M4>> {
    let nodes = j["nodes"].as_array().cloned().unwrap_or_default();
    let mut has_parent = vec![false; nodes.len()];
    for n in &nodes {
        for c in n["children"].as_array().into_iter().flatten() {
            if let Some(i) = c.as_u64() {
                if (i as usize) < has_parent.len() {
                    has_parent[i as usize] = true;
                }
            }
        }
    }
    let mut out: HashMap<usize, Vec<M4>> = HashMap::new();
    let mut st: Vec<(usize, M4)> = (0..nodes.len()).filter(|i| !has_parent[*i]).map(|i| (i, IDENT)).collect();
    let mut guard = 0;
    while let Some((i, pm)) = st.pop() {
        guard += 1;
        if guard > 100_000 {
            break;
        }
        let m = mul(&pm, &node_local(&nodes[i]));
        if let Some(mi) = nodes[i]["mesh"].as_u64() {
            out.entry(mi as usize).or_default().push(m);
        }
        for c in nodes[i]["children"].as_array().into_iter().flatten() {
            if let Some(ci) = c.as_u64() {
                if (ci as usize) < nodes.len() {
                    st.push((ci as usize, m));
                }
            }
        }
    }
    out
}

// ---------- 材质 ----------
#[derive(Clone)]
struct Mat {
    albedo: Option<std::sync::Arc<Image>>,
    glow: Option<std::sync::Arc<Image>>,
    emissive: bool,
    /// grid_alpha / wire 等带透明的网格材质做 alpha 测试
    alpha_test: bool,
}

struct TexCache<'a> {
    ctx: &'a GameCtx,
    proj: Option<&'a Project>,
    imgs: HashMap<String, Option<std::sync::Arc<Image>>>,
    missing: Vec<String>,
}

impl TexCache<'_> {
    fn get(&mut self, name: &str) -> Option<std::sync::Arc<Image>> {
        if let Some(v) = self.imgs.get(name) {
            return v.clone();
        }
        let r = (|| -> Result<Image> {
            let t = texcmd::load_tex(name, Some(self.ctx), self.proj, self.proj.is_none())?;
            let lv = (0..t.dds.header.levels()).find(|l| t.dds.header.level_dims(*l).0 <= 1024).unwrap_or(t.dds.header.levels() - 1);
            texture::decode_level(&t.dds, lv).map_err(|e| anyhow!("{e}"))
        })();
        let v = match r {
            Ok(i) => Some(std::sync::Arc::new(i)),
            Err(e) => {
                self.missing.push(format!("{name}: {e:#}"));
                None
            }
        };
        self.imgs.insert(name.to_string(), v.clone());
        v
    }
}

fn sample(img: &Image, u: f32, v: f32) -> [u8; 4] {
    let fu = u - u.floor();
    let fv = v - v.floor();
    let x = ((fu * img.width as f32) as u32).min(img.width - 1);
    let y = ((fv * img.height as f32) as u32).min(img.height - 1);
    img.get(x, y)
}

// ---------- 光栅 ----------
struct Tri {
    p: [V3; 3], // 屏幕 x, y, 深度(越大越近)
    uv: [[f32; 2]; 3],
    shade: f32,
    mat: usize,
}

pub struct Frame {
    pub img: Image,
}

fn view_basis(v: View) -> (V3, V3) {
    match v {
        View::Side | View::All => ([-1., 0., 0.], [0., 1., 0.]),
        View::Top => ([0., -1., 0.], [-1., 0., 0.]),
        View::Quarter => (norm([-0.72, -0.38, 0.58]), [0., 1., 0.]),
    }
}

struct Scene {
    tris_world: Vec<([V3; 3], [[f32; 2]; 3], usize)>,
    mats: Vec<Mat>,
}

fn load_scene(glb_path: &Path, assets_root: &Path, tc: &mut TexCache) -> Result<Scene> {
    let d = std::fs::read(glb_path).with_context(|| format!("读取 {}", glb_path.display()))?;
    let (j, prims) = glb::read_glb(&d).map_err(|e| anyhow!("{e}"))?;
    let inst = mesh_instances(&j);
    let mut mats: Vec<Mat> = Vec::new();
    let mut mat_idx: HashMap<String, usize> = HashMap::new();
    let mut tris = Vec::new();
    for p in &prims {
        let ex = &j["meshes"][p.mesh]["primitives"][p.prim]["extras"];
        let mfm_rel = ex["mfm"].as_str().unwrap_or("").to_string();
        let ident = ex["materialIdentifier"].as_str().unwrap_or("");
        let key = format!("{mfm_rel}|{}", ident.contains("EMISSIVE"));
        let mi = if let Some(i) = mat_idx.get(&key) {
            *i
        } else {
            let text = std::fs::read_to_string(assets_root.join(&mfm_rel)).unwrap_or_default();
            let props = mfm::properties(&text);
            let tex_of = |n: &str| props.iter().find(|(k, _, _)| k == n).map(|(_, _, v)| v.rsplit('/').next().unwrap_or(v).to_string());
            let fxs = mfm::fx(&text).unwrap_or_default();
            let emissive = ident.contains("EMISSIVE") || fxs.contains("emissive");
            let albedo = tex_of("diffuseMap").and_then(|n| tc.get(&n));
            let glow = if emissive { tex_of("metallicGlossMap").and_then(|n| tc.get(&n)) } else { None };
            let alpha_test = fxs.contains("alpha") || fxs.contains("wire") || fxs.contains("glass");
            mats.push(Mat { albedo, glow, emissive, alpha_test });
            mat_idx.insert(key, mats.len() - 1);
            mats.len() - 1
        };
        let idx: Vec<u32> = p.indices.clone().unwrap_or_else(|| (0..p.positions.len() as u32).collect());
        let uvs = p.uvs.clone();
        for m in inst.get(&p.mesh).cloned().unwrap_or_else(|| vec![IDENT]) {
            let wp: Vec<V3> = p.positions.iter().map(|x| xform(&m, *x)).collect();
            for t in idx.chunks_exact(3) {
                let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
                if a >= wp.len() || b >= wp.len() || c >= wp.len() {
                    continue;
                }
                let uv = match &uvs {
                    Some(u) => [u[a], u[b], u[c]],
                    None => [[0.0; 2]; 3],
                };
                tris.push(([wp[a], wp[b], wp[c]], uv, mi));
            }
        }
    }
    Ok(Scene { tris_world: tris, mats })
}

/// 两个场景共用同一取景(对比时比例一致)
fn frame_bounds(scenes: &[&Scene], v: View) -> [f32; 4] {
    let (dir, uph) = view_basis(v);
    let right = norm(cross(dir, uph));
    let up = cross(right, dir);
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for s in scenes {
        for (p, _, _) in &s.tris_world {
            for q in p {
                let x = dot(*q, right);
                let y = dot(*q, up);
                b[0] = b[0].min(x);
                b[1] = b[1].min(y);
                b[2] = b[2].max(x);
                b[3] = b[3].max(y);
            }
        }
    }
    b
}

fn rasterize(s: &Scene, v: View, bounds: [f32; 4], width: u32) -> Frame {
    let ss = 2u32;
    let (dir, uph) = view_basis(v);
    let right = norm(cross(dir, uph));
    let up = cross(right, dir);
    let light = norm([-dir[0] * 0.6 + 0.25, -dir[1] * 0.6 + 0.75, -dir[2] * 0.6 - 0.2]);
    let margin = 0.03;
    let (bw, bh) = ((bounds[2] - bounds[0]).max(1e-3), (bounds[3] - bounds[1]).max(1e-3));
    let w = width * ss;
    let scale = (w as f32 * (1.0 - 2.0 * margin)) / bw;
    let h = ((bh * scale) / (1.0 - 2.0 * margin)).ceil().clamp(64.0, (width * ss) as f32) as u32;
    let ox = w as f32 * margin;
    let oy = (h as f32 - bh * scale) / 2.0;
    let tris: Vec<Tri> = s
        .tris_world
        .iter()
        .map(|(p, uv, mi)| {
            let n = norm(cross(sub(p[1], p[0]), sub(p[2], p[0])));
            let shade = 0.38 + 0.62 * dot(n, light).abs();
            let sp = p.map(|q| [ox + (dot(q, right) - bounds[0]) * scale, oy + (bounds[3] - dot(q, up)) * scale, -dot(q, dir)]);
            Tri { p: sp, uv: *uv, shade, mat: *mi }
        })
        .collect();
    let bg = [38u8, 44, 52, 255];
    let mut col = vec![0u8; (w * h * 4) as usize];
    for px in col.chunks_exact_mut(4) {
        px.copy_from_slice(&bg);
    }
    let mut zb = vec![f32::MIN; (w * h) as usize];
    for t in &tris {
        let [a, b, c] = t.p;
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if area.abs() < 1e-9 {
            continue;
        }
        let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.0) as i64;
        let x1 = a[0].max(b[0]).max(c[0]).ceil().min(w as f32 - 1.0) as i64;
        let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.0) as i64;
        let y1 = a[1].max(b[1]).max(c[1]).ceil().min(h as f32 - 1.0) as i64;
        if x0 > x1 || y0 > y1 {
            continue;
        }
        let m = &s.mats[t.mat];
        for y in y0..=y1 {
            let py = y as f32 + 0.5;
            for x in x0..=x1 {
                let pxf = x as f32 + 0.5;
                let w0 = ((b[0] - pxf) * (c[1] - py) - (b[1] - py) * (c[0] - pxf)) / area;
                let w1 = ((c[0] - pxf) * (a[1] - py) - (c[1] - py) * (a[0] - pxf)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-5 || w1 < -1e-5 || w2 < -1e-5 {
                    continue;
                }
                let z = w0 * a[2] + w1 * b[2] + w2 * c[2];
                let zi = (y as u32 * w + x as u32) as usize;
                if z <= zb[zi] {
                    continue;
                }
                let u = w0 * t.uv[0][0] + w1 * t.uv[1][0] + w2 * t.uv[2][0];
                let vv = w0 * t.uv[0][1] + w1 * t.uv[1][1] + w2 * t.uv[2][1];
                let al = m.albedo.as_ref().map(|i| sample(i, u, vv)).unwrap_or([150, 150, 150, 255]);
                if m.alpha_test && al[3] < 100 {
                    continue;
                }
                zb[zi] = z;
                let g = if m.emissive { m.glow.as_ref().map(|i| sample(i, u, vv)[2] as f32 / 255.0).unwrap_or(0.0) } else { 0.0 };
                let o = zi * 4;
                for k in 0..3 {
                    let base = al[k] as f32 * t.shade;
                    col[o + k] = (base + al[k] as f32 * g * 1.4 + g * 40.0).min(255.0) as u8;
                }
                col[o + 3] = 255;
            }
        }
    }
    // 2x2 下采样
    let (ow, oh) = (w / ss, h / ss);
    let mut img = Image::new(ow, oh);
    for y in 0..oh {
        for x in 0..ow {
            let mut acc = [0u32; 4];
            for dy in 0..ss {
                for dx in 0..ss {
                    let i = (((y * ss + dy) * w + x * ss + dx) * 4) as usize;
                    for k in 0..4 {
                        acc[k] += col[i + k] as u32;
                    }
                }
            }
            let o = img.idx(x, y);
            for k in 0..4 {
                img.px[o + k] = (acc[k] / (ss * ss)) as u8;
            }
        }
    }
    Frame { img }
}

fn stack(imgs: &[Image], gap: u32) -> Image {
    let w = imgs.iter().map(|i| i.width).max().unwrap_or(1);
    let h: u32 = imgs.iter().map(|i| i.height).sum::<u32>() + gap * (imgs.len().saturating_sub(1) as u32);
    let mut out = Image::new(w, h.max(1));
    for p in out.px.chunks_exact_mut(4) {
        p.copy_from_slice(&[230, 230, 230, 255]);
    }
    let mut y0 = 0;
    for im in imgs {
        for y in 0..im.height {
            for x in 0..im.width {
                let s = im.idx(x, y);
                let d = out.idx(x, y0 + y);
                out.px[d..d + 4].copy_from_slice(&im.px[s..s + 4]);
            }
        }
        y0 += im.height + gap;
    }
    out
}

fn side_by_side(a: &Image, b: &Image, gap: u32) -> Image {
    let h = a.height.max(b.height);
    let mut out = Image::new(a.width + b.width + gap, h);
    for p in out.px.chunks_exact_mut(4) {
        p.copy_from_slice(&[230, 230, 230, 255]);
    }
    for (im, x0) in [(a, 0), (b, a.width + gap)] {
        for y in 0..im.height {
            for x in 0..im.width {
                let s = im.idx(x, y);
                let d = out.idx(x0 + x, y);
                out.px[d..d + 4].copy_from_slice(&im.px[s..s + 4]);
            }
        }
    }
    out
}

fn self_exe_json(cli: &Cli, args: &[String]) -> Result<Value> {
    let exe = std::env::current_exe()?;
    let mut c = std::process::Command::new(exe);
    if let Some(g) = &cli.game {
        c.arg("--game").arg(g);
    }
    c.arg("--json").args(args);
    let o = c.output().context("启动 wowsmod 子进程")?;
    let v: Value = serde_json::from_slice(&o.stdout).map_err(|_| anyhow!("子命令输出不是 JSON: {}", String::from_utf8_lossy(&o.stdout).chars().take(400).collect::<String>()))?;
    if v["ok"] == false {
        bail!("{} 失败: {}", args.first().map(|s| s.as_str()).unwrap_or(""), v["error"]);
    }
    Ok(v)
}

/// 渲染; 返回 (PNG 字节, 说明 JSON)
pub fn render(cli: &Cli, a: &RenderArgs) -> Result<(Vec<u8>, Value)> {
    if a.engine == Engine::Preview {
        bail!("--engine preview 尚未接通: 需要 M5 预览后端(见 /workspace/preview_app/API.md §11 ?render=wait 无头渲染约定)。下一步: 先用默认 --engine soft");
    }
    let p = Project::open(&a.project)?;
    let ctx = game_ctx(cli, true)?.unwrap();
    let rdir = p.root.join(".render");
    std::fs::create_dir_all(&rdir)?;
    let width = a.width.clamp(200, 4096);
    if let Some(t) = &a.texture {
        let mut tb = TexCache { ctx: &ctx, proj: None, imgs: HashMap::new(), missing: vec![] };
        let base = tb.get(t).ok_or_else(|| anyhow!("读不到原版贴图 {t}: {:?}", tb.missing))?;
        let mut th = TexCache { ctx: &ctx, proj: Some(&p), imgs: HashMap::new(), missing: vec![] };
        let head = th.get(t).ok_or_else(|| anyhow!("读不到项目贴图 {t}: {:?}", th.missing))?;
        let half = (width / 2).max(64);
        let fit = |i: &Image| -> Image {
            let mut x = i.clone();
            while x.width > half * 2 - 1 && x.width > 1 {
                x = texture::downsample(&x, texture::MipFilter::BoxSrgb);
            }
            x
        };
        let img = side_by_side(&fit(&base), &fit(&head), 8);
        let png = texture::write_png(&img, false).map_err(|e| anyhow!("{e}"))?;
        return Ok((png, json!({"kind": "texture", "texture": t, "layout": "左=原版(游戏), 右=项目当前", "size": [img.width, img.height]})));
    }
    let ship = p.cfg.meta.ship.clone();
    if ship.is_empty() {
        bail!("mod.toml 没有 ship, 无法装配整船; 下一步: 用 --texture <贴图名> 只看贴图");
    }
    let head_glb = rdir.join("head.glb");
    let t0 = std::time::Instant::now();
    self_exe_json(cli, &["assemble".into(), ship.clone(), "--project".into(), p.root.display().to_string(), "--full".into(), "-o".into(), head_glb.display().to_string()])?;
    let assets = p.assets();
    let mut th = TexCache { ctx: &ctx, proj: Some(&p), imgs: HashMap::new(), missing: vec![] };
    let head = load_scene(&head_glb, &assets, &mut th)?;
    let mut notes = vec!["软件光栅近似: 只有漫反射贴图+发光, 没有法线/AO/PBR/迷彩/环境光; 用于比较改动, 实际观感以进港截图为准".to_string()];
    let base = if a.compare {
        let sdk = assets.join("PnFMods").join("ModsSDK.zip").join(&ship);
        if !sdk.join("ship").is_dir() {
            bail!("项目里没有 SDK 原版副本 {} (import-sdk 时加 --sdk-dir); 不加 --compare 可只看当前效果", sdk.display());
        }
        let base_glb = rdir.join("base.glb");
        if !base_glb.is_file() {
            self_exe_json(cli, &["assemble".into(), ship.clone(), "--sdk".into(), sdk.display().to_string(), "--full".into(), "-o".into(), base_glb.display().to_string()])?;
        }
        let mut tb = TexCache { ctx: &ctx, proj: None, imgs: HashMap::new(), missing: vec![] };
        let s = load_scene(&base_glb, &assets, &mut tb)?;
        if !tb.missing.is_empty() {
            notes.push(format!("原版有 {} 张贴图读不到(用灰色)", tb.missing.len()));
        }
        Some(s)
    } else {
        None
    };
    if !th.missing.is_empty() {
        notes.push(format!("项目有 {} 张贴图读不到(用灰色): {}", th.missing.len(), th.missing.iter().take(3).cloned().collect::<Vec<_>>().join("; ")));
    }
    let views: Vec<View> = if a.view == View::All { vec![View::Side, View::Quarter, View::Top] } else { vec![a.view] };
    let mut rows = Vec::new();
    for v in &views {
        let mut sc: Vec<&Scene> = vec![&head];
        if let Some(b) = &base {
            sc.push(b);
        }
        let bounds = frame_bounds(&sc, *v);
        if let Some(b) = &base {
            rows.push(rasterize(b, *v, bounds, width).img);
        }
        rows.push(rasterize(&head, *v, bounds, width).img);
    }
    let img = stack(&rows, 6);
    let png = texture::write_png(&img, false).map_err(|e| anyhow!("{e}"))?;
    let layout = if a.compare { "每个视角两行: 上=原版, 下=项目当前" } else { "项目当前" };
    Ok((png, json!({"kind": "model", "views": views.iter().map(|v| format!("{v:?}").to_lowercase()).collect::<Vec<_>>(), "layout": layout, "size": [img.width, img.height],
        "triangles": head.tris_world.len(), "materials": head.mats.len(), "emissive_materials": head.mats.iter().filter(|m| m.emissive).count(), "seconds": t0.elapsed().as_secs_f32(), "notes": notes})))
}

pub fn cmd_render(cli: &Cli, a: &RenderArgs) -> Result<i32> {
    let (png, mut v) = render(cli, a)?;
    let out = a.out.clone().unwrap_or_else(|| {
        let n = match &a.texture {
            Some(t) => format!("tex_{}.png", t.trim_end_matches(".dds")),
            None => format!("{}{}.png", format!("{:?}", a.view).to_lowercase(), if a.compare { "_compare" } else { "" }),
        };
        a.project.join(".render").join(n)
    });
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(&out, &png)?;
    v["ok"] = json!(true);
    v["out"] = json!(out.display().to_string());
    crate::guide::stash(&v);
    if cli.json {
        emit(cli, &v);
    } else {
        println!("已出图: {} ({}x{}) {}", out.display(), v["size"][0], v["size"][1], v["layout"].as_str().unwrap_or(""));
        for n in v["notes"].as_array().into_iter().flatten() {
            println!("  注: {}", n.as_str().unwrap_or(""));
        }
        crate::guide::stash(&v);
    }
    Ok(0)
}
