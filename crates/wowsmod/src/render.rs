//! M6 出图: 不依赖游戏/GPU 的软件光栅预览(近似), 供人和 AI 快速看效果。
//!
//! - 3D: 用 `assemble --full` 得到整船 GLB(带 UV 与每个 primitive 的 .mfm), 按 .mfm 的 diffuseMap 贴图,
//!   SHIPMAT_EMISSIVE_* 材质按 _mg 的 B 通道叠加发光; 正交投影 + 面法线 Lambert + 2x 超采样。
//!   --compare 时上半是原版(项目里的 SDK 副本 + 游戏原版贴图), 下半是项目当前状态。
//! - 贴图: --texture <名> 原版 | 项目 左右并排。
//! - --engine browser(默认): 系统 Edge/Chrome 无头 + CDP 调前端 WebGL(API.md §11)。
//! - --engine soft: 内置软件光栅兜底。
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
    /// 系统 Edge/Chrome 无头 + CDP 调前端 WebGL(默认, API.md §11)
    Browser,
    /// 内置软件光栅(无浏览器时的兜底, 近似)
    Soft,
}

#[derive(Args, Clone, Debug)]
pub struct RenderArgs {
    /// mod 项目目录。也可写动作: render warm|status|stop <项目>(常驻出图实例)
    pub project: PathBuf,
    /// 动作写法时的项目目录(render status <项目>)
    #[arg(hide = true)]
    pub project2: Option<PathBuf>,
    /// 预热: 后台启动常驻出图实例(本机服务 + 无头浏览器 + 已加载的模型/贴图), 之后 render 自动连它, 每张约 1 秒。
    /// 冷启动(不预热)每张要几十秒。空闲 --idle 分钟后自动退出
    #[arg(long)]
    pub warm: bool,
    /// 常驻实例空闲多少分钟后自动退出(默认 15; 0 = 不退出; 也可设环境变量 WOWSMOD_RENDER_IDLE_MIN)
    #[arg(long, value_name = "MIN")]
    pub idle: Option<u64>,
    /// 单张出图超时(秒); 超时会自动重启浏览器重试一次
    #[arg(long, default_value_t = 60)]
    pub timeout: u64,
    /// 不用常驻实例, 强制冷启动(一次性起服务 + 浏览器, 用完即关)
    #[arg(long)]
    pub cold: bool,
    /// render status / stop 时: 列出 / 停止本机全部常驻实例
    #[arg(long)]
    pub all: bool,
    /// 视角(软件引擎) / 或与 --query 二选一
    #[arg(long, value_enum, default_value_t = View::Quarter)]
    pub view: View,
    /// 上下对比(软件) / CDP 时等价 compare=split&base=base
    #[arg(long)]
    pub compare: bool,
    /// 只看一张贴图(软件引擎)
    #[arg(long, value_name = "NAME")]
    pub texture: Option<String>,
    /// 输出 PNG(默认 <项目>/renders/…)
    #[arg(short, long)]
    pub out: Option<PathBuf>,
    /// 宽度(软件) / 与 --size 二选一
    #[arg(long, default_value_t = 1600)]
    pub width: u32,
    /// 尺寸 WxH(CDP, 默认 1600x900)
    #[arg(long, default_value = "1600x900")]
    pub size: String,
    /// 渲染引擎
    #[arg(long, value_enum, default_value_t = Engine::Browser)]
    pub engine: Engine,
    /// 浏览器可执行文件(默认自动查找 Edge/Chrome)
    #[arg(long)]
    pub browser: Option<PathBuf>,
    /// 原样查询串(CDP): view=fq&compare=split&base=base …
    #[arg(long)]
    pub query: Option<String>,
    /// 联系表
    #[arg(long)]
    pub sheet: bool,
    /// 版本(CDP) head/base/sN
    #[arg(long, default_value = "head")]
    pub rev: String,
    /// 对比基线
    #[arg(long)]
    pub base: Option<String>,
    /// 软件引擎自定义机位: 水平角(度, 0=右舷正侧, 正值转向船艏方向; quarter≈39)。与 --pitch 一起覆盖 --view
    #[arg(long)]
    pub yaw: Option<f32>,
    /// 软件引擎自定义机位: 俯角(度, 0=水平, quarter≈22)
    #[arg(long)]
    pub pitch: Option<f32>,
    /// 外观(浏览器引擎): game-day = 港口白天对齐(默认) / game-night = 港口夜景对齐 / studio = 影棚展台。
    /// 与 --query 同用时追加 look=…, 覆盖查询串里的 light。校准与残差见 wowsmod help workflow.layers.verify.wysiwyg
    #[arg(long, value_parser = ["game-day", "game-night", "studio", "game"])]
    pub look: Option<String>,
}

// ---------- 小向量 ----------
pub(crate) type V3 = [f32; 3];
pub(crate) fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(crate) fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(crate) fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub(crate) fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 1e-12 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [0.0, 1.0, 0.0]
    }
}
pub(crate) type M4 = [f32; 16]; // 列主序(glTF)
pub(crate) const IDENT: M4 = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];
fn mul(a: &M4, b: &M4) -> M4 {
    let mut r = [0f32; 16];
    for c in 0..4 {
        for row in 0..4 {
            r[c * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[c * 4 + k]).sum();
        }
    }
    r
}
pub(crate) fn xform(m: &M4, p: V3) -> V3 {
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
pub(crate) fn mesh_instances(j: &Value) -> HashMap<usize, Vec<M4>> {
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
    /// name: 贴图文件名或本地路径(项目里同名文件有多个时用 .mfm 给出的确切路径)
    fn get(&mut self, name: &str) -> Option<std::sync::Arc<Image>> {
        if let Some(v) = self.imgs.get(name) {
            return v.clone();
        }
        let r = (|| -> Result<Image> {
            let local = Path::new(name).is_file();
            let t = if local { texcmd::load_tex(name, Some(self.ctx), None, false)? } else { texcmd::load_tex(name, Some(self.ctx), self.proj, self.proj.is_none())? };
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

/// 自定义机位(yaw/pitch, 度) → 视线方向; 与内置视角同一约定(side = yaw 0, pitch 0)
pub(crate) fn yaw_pitch_dir(yaw: f32, pitch: f32) -> V3 {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    norm([-y.cos() * p.cos(), -p.sin(), y.sin() * p.cos()])
}

thread_local! {
    static CUSTOM_DIR: std::cell::Cell<Option<V3>> = const { std::cell::Cell::new(None) };
}
/// 设置本线程软件光栅的自定义视线(None = 用 --view 的内置视角)
pub(crate) fn set_custom_dir(d: Option<V3>) {
    CUSTOM_DIR.with(|c| c.set(d));
}

pub(crate) fn view_basis(v: View) -> (V3, V3) {
    if let Some(d) = CUSTOM_DIR.with(|c| c.get()) {
        if v != View::Top {
            return (d, [0., 1., 0.]);
        }
    }
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
            // 项目内确切文件(非占位)优先, 否则按文件名(项目同名/游戏原版)
            let proj_mode = tc.proj.is_some();
            let tex_of = |n: &str| {
                props.iter().find(|(k, _, _)| k == n).map(|(_, _, v)| {
                    let f = assets_root.join(v);
                    let real = proj_mode && std::fs::read(&f).map(|d| !mfm::is_sdk_stub(&d)).unwrap_or(false);
                    if real { f.display().to_string() } else { v.rsplit('/').next().unwrap_or(v).to_string() }
                })
            };
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

/// 与 [`rasterize`] 完全一致的投影(输出图像素坐标, 非超采样): 供 bake-ref 把世界坐标对到 render 图上
pub(crate) struct Proj {
    pub dir: V3,
    right: V3,
    up: V3,
    bounds: [f32; 4],
    scale: f32,
    ox: f32,
    oy: f32,
    pub w: u32,
    pub h: u32,
}
impl Proj {
    pub fn new(v: View, bounds: [f32; 4], width: u32) -> Proj {
        let ss = 2u32;
        let (dir, uph) = view_basis(v);
        let right = norm(cross(dir, uph));
        let up = cross(right, dir);
        let margin = 0.03;
        let (bw, bh) = ((bounds[2] - bounds[0]).max(1e-3), (bounds[3] - bounds[1]).max(1e-3));
        let w = width * ss;
        let scale = (w as f32 * (1.0 - 2.0 * margin)) / bw;
        let h = ((bh * scale) / (1.0 - 2.0 * margin)).ceil().clamp(64.0, (width * ss) as f32) as u32;
        let ox = w as f32 * margin;
        let oy = (h as f32 - bh * scale) / 2.0;
        Proj { dir, right, up, bounds, scale, ox, oy, w: w / ss, h: h / ss }
    }
    /// 世界坐标 → (x, y, 深度; 越大越近), 输出图像素
    pub fn px(&self, q: V3) -> V3 {
        [(self.ox + (dot(q, self.right) - self.bounds[0]) * self.scale) / 2.0, (self.oy + (self.bounds[3] - dot(q, self.up)) * self.scale) / 2.0, -dot(q, self.dir)]
    }
}

/// 世界坐标三角形列表的取景范围(与 frame_bounds 相同算法)
pub(crate) fn bounds_of(tris: &[[V3; 3]], v: View) -> [f32; 4] {
    let (dir, uph) = view_basis(v);
    let right = norm(cross(dir, uph));
    let up = cross(right, dir);
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in tris {
        for q in p {
            let x = dot(*q, right);
            let y = dot(*q, up);
            b[0] = b[0].min(x);
            b[1] = b[1].min(y);
            b[2] = b[2].max(x);
            b[3] = b[3].max(y);
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

pub(crate) fn self_exe_json(cli: &Cli, args: &[String]) -> Result<Value> {
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
    if a.engine == Engine::Browser {
        return render_browser(cli, a);
    }
    let p = Project::open(&a.project)?;
    let ctx = game_ctx(cli, true)?.unwrap();
    let rdir = p.root.join(".render");
    std::fs::create_dir_all(&rdir)?;
    let width = a.width.clamp(200, 8192);
    if let Some(t) = &a.texture {
        let mut tb = TexCache { ctx: &ctx, proj: None, imgs: HashMap::new(), missing: vec![] };
        let base = tb.get(t).ok_or_else(|| anyhow!("读不到原版贴图 {t}: {:?}", tb.missing))?;
        let mut th = TexCache { ctx: &ctx, proj: Some(&p), imgs: HashMap::new(), missing: vec![] };
        let real: Vec<PathBuf> = p.find_by_name(t)?.into_iter().filter(|f| std::fs::read(f).map(|d| !mfm::is_sdk_stub(&d)).unwrap_or(false)).collect();
        let spec = match real.len() {
            0 => t.clone(),
            1 => real[0].display().to_string(),
            n => bail!("项目里有 {n} 个已修改的 {t}: {:?}; 请给出其中一个的路径", real),
        };
        let head = th.get(&spec).ok_or_else(|| anyhow!("读不到项目贴图 {t}: {:?}", th.missing))?;
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
    set_custom_dir(if a.yaw.is_some() || a.pitch.is_some() { Some(yaw_pitch_dir(a.yaw.unwrap_or(0.0), a.pitch.unwrap_or(0.0))) } else { None });
    let views: Vec<View> = if a.yaw.is_some() || a.pitch.is_some() { vec![View::Side] } else if a.view == View::All { vec![View::Side, View::Quarter, View::Top] } else { vec![a.view] };
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
    // 世界→像素映射(第一视角、不含 --compare 的上下堆叠偏移), 供设计图按世界坐标作画后 bake-ref
    let frame = {
        let mut sc: Vec<&Scene> = vec![&head];
        if let Some(b) = &base {
            sc.push(b);
        }
        let v0 = views[0];
        let bounds = frame_bounds(&sc, v0);
        let pj = Proj::new(v0, bounds, width);
        let o = pj.px([0.0, 0.0, 0.0]);
        let ex = pj.px([1.0, 0.0, 0.0]);
        let ey = pj.px([0.0, 1.0, 0.0]);
        let ez = pj.px([0.0, 0.0, 1.0]);
        json!({"size": [pj.w, pj.h], "origin_px": [o[0], o[1]], "x_axis_px": [ex[0] - o[0], ex[1] - o[1]], "y_axis_px": [ey[0] - o[0], ey[1] - o[1]], "z_axis_px": [ez[0] - o[0], ez[1] - o[1]],
            "note": "像素 = origin_px + X*x_axis_px + Y*y_axis_px + Z*z_axis_px (GLB 世界坐标; y 向上, 船艏在 -z)。--compare 时每行原版在上, 此映射对应单张(不加 --compare)"})
    };
    let layout = if a.compare { "每个视角两行: 上=原版, 下=项目当前" } else { "项目当前" };
    Ok((png, json!({"kind": "model", "views": views.iter().map(|v| format!("{v:?}").to_lowercase()).collect::<Vec<_>>(), "layout": layout, "size": [img.width, img.height],
        "triangles": head.tris_world.len(), "materials": head.mats.len(), "emissive_materials": head.mats.iter().filter(|m| m.emissive).count(), "seconds": t0.elapsed().as_secs_f32(), "notes": notes, "frame": frame})))
}

fn parse_size(s: &str) -> (u32, u32) {
    let mut it = s.split(|c| c == 'x' || c == 'X' || c == ',');
    let w = it.next().and_then(|x| x.parse().ok()).unwrap_or(1600);
    let h = it.next().and_then(|x| x.parse().ok()).unwrap_or(900);
    (w.clamp(64, 4096), h.clamp(64, 4096))
}

pub(crate) fn build_query(a: &RenderArgs) -> String {
    let look = a.look.as_deref().map(|l| format!("&look={l}")).unwrap_or_default();
    if let Some(q) = &a.query {
        // 查询串里已有 look= 时以 --look 为准(前端取第一个 look, 故放前面)
        return match &a.look {
            Some(l) => format!("look={l}&{}", q.trim_start_matches('&')),
            None => q.clone(),
        };
    }
    let view = match a.view {
        View::Side => "side",
        View::Top => "top",
        View::Quarter | View::All => "fq",
    };
    let mut q = format!("view={view}&rev={}", a.rev);
    if a.sheet {
        q.push_str("&sheet=1&views=side,fq,rq,top&revs=base,head");
    } else if a.compare || a.base.is_some() {
        q.push_str(&format!("&compare=split&base={}", a.base.as_deref().unwrap_or("base")));
    }
    let (w, h) = parse_size(&a.size);
    q.push_str(&format!("&w={w}&h={h}"));
    q.push_str(&look);
    q
}

fn render_browser(cli: &Cli, a: &RenderArgs) -> Result<(Vec<u8>, Value)> {
    let q = build_query(a);
    let (w, h) = parse_size(&a.size);
    match crate::preview::render_pngs(cli, &a.project, &[("main".into(), q.clone())], a.browser.as_deref(), (w, h)) {
        Ok(mut v) => {
            let (png, meta) = v.remove(0);
            Ok((png, json!({"kind": "browser", "engine": "browser", "query": q, "meta": meta, "size": [meta["width"], meta["height"]]})))
        }
        Err(e) => {
            let msg = format!("{e:#}");
            if msg.contains("browser_not_found") {
                Err(anyhow!(crate::preview::BrowserMissing))
            } else {
                Err(e)
            }
        }
    }
}

fn emit_err(cli: &Cli, code: &str, msg: &str, project: &Path) -> i32 {
    let next = crate::preview::warm::next_for(code, project);
    let v = json!({"ok": false, "error": {"code": code, "message": msg}, "next": next});
    crate::guide::stash(&v);
    if cli.json {
        emit(cli, &v);
    } else {
        eprintln!("错误 [{code}]: {msg}");
        eprintln!("建议下一步:");
        for n in &next {
            eprintln!("  - {n}");
        }
    }
    match code {
        "bad_param" => 2,
        "browser_not_found" => 3,
        _ => 4,
    }
}

fn err_code(msg: &str) -> &'static str {
    for c in ["browser_not_found", "warm_start_failed", "render_timeout", "cdp_timeout", "warm_timeout", "page_crashed", "browser_crashed", "cdp_closed", "browser_exited", "bad_param", "render_failed", "warm_unreachable"] {
        if msg.contains(c) {
            return match c {
                "cdp_timeout" | "warm_timeout" => "render_timeout",
                "page_crashed" | "cdp_closed" | "browser_exited" => "browser_crashed",
                "warm_unreachable" => "warm_start_failed",
                x => x,
            };
        }
    }
    ""
}

fn warm_status_text(v: &Value) -> String {
    if v["ready"] == true {
        format!("常驻出图实例: 就绪(热)  pid {}  浏览器 pid {}  已出图 {} 张  空闲 {}s / 上限 {}s  预热耗时 {:.1}s",
            v["pid"], v["browser_pid"], v["renders"], v["idle_s"], v["idle_limit_s"], v["warmup"]["total_s"].as_f64().unwrap_or(0.0))
    } else if v["running"] == true {
        format!("常驻出图实例: 预热中(pid {}), 稍后再看 render status", v["pid"])
    } else {
        format!("常驻出图实例: 未运行({})。要出图先预热: wowsmod render --warm <项目>", v["reason"].as_str().unwrap_or(""))
    }
}

fn cmd_warm(cli: &Cli, project: &Path, a: &RenderArgs) -> Result<i32> {
    let idle = crate::preview::warm::idle_minutes(a.idle);
    let t0 = std::time::Instant::now();
    match crate::preview::warm::ensure_started(cli, project, idle, a.browser.as_deref(), true, std::time::Duration::from_secs(240)) {
        Ok(mut v) => {
            v["ok"] = json!(true);
            v["seconds"] = json!(t0.elapsed().as_secs_f32());
            v["next"] = json!([format!("wowsmod render {} --compare (每张约 1 秒)", project.display()), format!("wowsmod render status {}", project.display()), format!("wowsmod render stop {}", project.display())]);
            crate::guide::stash(&v);
            if cli.json {
                emit(cli, &v);
            } else {
                let already = v["spawned"] != true;
                println!("{}", if already { "已经是热的(常驻实例在运行)。" } else { "预热完成。" });
                println!("{}", warm_status_text(&v));
                println!("之后 wowsmod render {} … 会自动连这个实例; 空闲 {idle} 分钟自动退出; 停止: wowsmod render stop {}", project.display(), project.display());
            }
            Ok(0)
        }
        Err(e) => {
            let m = format!("{e:#}");
            let c = err_code(&m);
            Ok(emit_err(cli, if c.is_empty() { "warm_start_failed" } else { c }, &m, project))
        }
    }
}

pub fn cmd_render(cli: &Cli, a: &RenderArgs) -> Result<i32> {
    use crate::preview::warm;
    // 动作写法: render warm|status|stop|serve [项目]
    let word = a.project.to_str().unwrap_or("").to_string();
    let is_action = matches!(word.as_str(), "warm" | "status" | "stop" | "serve") && !a.project.join("mod.toml").is_file();
    if is_action {
        let proj = a.project2.clone();
        let need = || proj.clone().ok_or_else(|| anyhow!("用法: wowsmod render {word} <项目>"));
        match word.as_str() {
            "serve" => return warm::serve(cli, &need()?, warm::idle_minutes(a.idle), a.browser.as_deref()),
            "warm" => return cmd_warm(cli, &need()?, a),
            "status" => {
                let v = match (&proj, a.all) {
                    (Some(p), false) => warm::status(p),
                    _ => json!({"instances": warm::status_all()}),
                };
                crate::guide::stash(&v);
                if cli.json {
                    emit(cli, &v);
                } else if let Some(list) = v["instances"].as_array() {
                    if list.is_empty() {
                        println!("本机没有常驻出图实例。预热: wowsmod render --warm <项目>");
                    }
                    for x in list {
                        println!("{}  {}", x["project"].as_str().unwrap_or(""), warm_status_text(x));
                    }
                } else {
                    println!("{}", warm_status_text(&v));
                }
                return Ok(0);
            }
            _ => {
                let v = match (&proj, a.all) {
                    (Some(p), false) => warm::stop(p),
                    _ => {
                        let list: Vec<Value> = warm::status_all().iter().filter_map(|x| x["project"].as_str().map(|p| warm::stop(Path::new(p)))).collect();
                        json!({"ok": true, "stopped": list})
                    }
                };
                crate::guide::stash(&v);
                if cli.json { emit(cli, &v); } else { println!("{}", if v["stopped"] == false { "没有常驻实例".to_string() } else { format!("已停止 {}", v) }); }
                return Ok(0);
            }
        }
    }
    if a.warm {
        return cmd_warm(cli, &a.project, a);
    }
    let use_warm = a.engine == Engine::Browser && a.texture.is_none() && !a.cold && warm::status(&a.project)["ready"] == true;
    let t0 = std::time::Instant::now();
    let r = if use_warm {
        let q = build_query(a);
        warm::render_with_heal(cli, &a.project, &q, a.timeout, warm::idle_minutes(a.idle), a.browser.as_deref())
            .map(|(png, m)| (png, json!({"kind": "browser", "engine": "browser", "warm": true, "query": q, "meta": m["meta"], "timing": m["timing"], "warm_restarted": m["warm_restarted"], "size": [m["meta"]["width"], m["meta"]["height"]]})))
    } else {
        render(cli, a)
    };
    if let Err(e) = &r {
        let m = format!("{e:#}");
        let c = err_code(&m);
        if a.engine == Engine::Browser && !c.is_empty() && c != "browser_not_found" {
            return Ok(emit_err(cli, c, &m, &a.project));
        }
    }
    if let Err(e) = &r {
        if format!("{e:#}").contains("browser_not_found") {
            let url_hint = format!("(可先 wowsmod preview {} 手动查看)", a.project.display());
            let v = crate::preview::browser_not_found_value(Some(&url_hint));
            crate::guide::stash(&v);
            if cli.json { emit(cli, &v); } else {
                eprintln!("错误: 找不到 Edge/Chrome。用 --browser 指定, 或 --engine soft 用软件光栅。{}", url_hint);
            }
            return Ok(3);
        }
    }
    let (png, mut v) = r?;
    v["seconds"] = json!(t0.elapsed().as_secs_f32());
    if a.engine == Engine::Browser && a.texture.is_none() && !use_warm {
        v["warm"] = json!(false);
        v["hint"] = json!(format!("冷启动 {:.0} 秒(起服务+浏览器+加载模型)。下次先 wowsmod render --warm {} 预热, 之后每张约 1 秒", t0.elapsed().as_secs_f32(), a.project.display()));
    }
    let out = a.out.clone().unwrap_or_else(|| {
        let n = match &a.texture {
            Some(t) => format!("tex_{}.png", t.trim_end_matches(".dds")),
            None if a.engine == Engine::Browser => {
                let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                format!("{ts}_{}.png", format!("{:?}", a.view).to_lowercase())
            }
            None => format!("{}{}.png", format!("{:?}", a.view).to_lowercase(), if a.compare { "_compare" } else { "" }),
        };
        let dir = if a.engine == Engine::Browser { a.project.join("renders") } else { a.project.join(".render") };
        dir.join(n)
    });
    if let Some(d) = out.parent() { std::fs::create_dir_all(d)?; }
    std::fs::write(&out, &png)?;
    let abs = std::fs::canonicalize(&out).unwrap_or(out.clone());
    v["ok"] = json!(true);
    v["out"] = json!(abs.display().to_string().trim_start_matches(r"\\?\"));
    crate::guide::stash(&v);
    if cli.json {
        emit(cli, &v);
    } else {
        println!("{}", abs.display());
        if v["warm"] == true {
            println!("  (常驻实例, {:.2}s)", v["seconds"].as_f64().unwrap_or(0.0));
        }
        if let Some(h) = v["hint"].as_str() {
            println!("  提示: {h}");
        }
        if let Some(notes) = v["notes"].as_array() {
            for n in notes { println!("  注: {}", n.as_str().unwrap_or("")); }
        }
    }
    Ok(0)
}