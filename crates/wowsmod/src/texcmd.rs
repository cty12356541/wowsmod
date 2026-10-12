//! tex / mask / glow 子命令的实现。
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use wowsmod_formats::{
    bc7prep,
    dds::{self, Dds, Format},
    mfm,
    recolor::{self, Mask, Recolor, Selector},
    texture::{self, Image, MipFilter, Quality},
    uvmask,
};

use crate::modproj::{self, Project};
use crate::GameCtx;

/// 一份完整(分卷已合并、BC7Prep 已还原)的贴图及其来源
pub struct Tex {
    pub label: String,
    pub file_name: String,
    pub vpath: Option<String>,
    /// 项目内文件(若来源于项目)
    pub project_path: Option<PathBuf>,
    pub dds: Dds,
    /// 原 .dds 记录的分卷数(写回时沿用)
    pub k: u32,
    pub bc7prep: bool,
    pub sha256: String,
}

fn unprep(d: Vec<u8>) -> Result<(Vec<u8>, bool)> {
    if bc7prep::is_bc7prep_dds(&d) {
        Ok((bc7prep::decode_dds(&d).map_err(|e| anyhow!("{e}"))?, true))
    } else {
        Ok((d, false))
    }
}

fn assemble(label: &str, tail: Vec<u8>, mut vol: impl FnMut(u32) -> Result<Option<Vec<u8>>>) -> Result<(Dds, u32, bool)> {
    if modproj_stub(&tail) {
        bail!("{label} 是 SDK 占位文件, 不是真实贴图");
    }
    let (tail, p0) = unprep(tail)?;
    let h = dds::Header::parse(&tail).map_err(|e| anyhow!("{label}: {e}"))?;
    let k = h.volume_count().unwrap_or(0);
    let mut prep = p0;
    let mut vols = Vec::new();
    for n in 0..k {
        let v = vol(n)?.ok_or_else(|| anyhow!("{label}: 头部声明 {k} 个分卷, 找不到 .dd{n}"))?;
        let (v, p) = unprep(v)?;
        prep |= p;
        vols.push(v);
    }
    let refs: Vec<&[u8]> = vols.iter().map(|v| v.as_slice()).collect();
    let full = if k == 0 { Dds::parse(&tail).map_err(|e| anyhow!("{label}: {e}"))? } else { dds::merge_volumes(&tail, &refs).map_err(|e| anyhow!("{label}: {e}"))? };
    Ok((full, k, prep))
}

fn modproj_stub(d: &[u8]) -> bool {
    mfm::is_sdk_stub(d)
}

fn vol_path(p: &Path, n: u32) -> PathBuf {
    p.with_extension(format!("dd{n}"))
}

/// 解析贴图参数: 本地 .dds(同目录 .ddN 自动合并) / 项目里同名的已编辑贴图 / 游戏内路径或 glob(必须唯一)。
pub fn load_tex(spec: &str, ctx: Option<&GameCtx>, proj: Option<&Project>, from_game: bool) -> Result<Tex> {
    let name = spec.replace('\\', "/").rsplit('/').next().unwrap_or(spec).to_string();
    // 1) 项目里已有真实(非占位)同名文件 → 在其基础上继续编辑
    if let (Some(pj), false) = (proj, from_game) {
        if name.to_lowercase().ends_with(".dds") {
            let hits = pj.find_by_name(&name)?;
            if hits.len() > 1 {
                bail!("项目里有多个 {name}: {:?}", hits);
            }
            if let Some(h) = hits.first() {
                let d = std::fs::read(h)?;
                if !modproj_stub(&d) {
                    let sha = modproj::sha256_hex(&d);
                    let (dds, k, prep) = assemble(&h.display().to_string(), d, |n| Ok(std::fs::read(vol_path(h, n)).ok()))?;
                    return Ok(Tex { label: pj.rel(h), file_name: name, vpath: None, project_path: Some(h.clone()), dds, k, bc7prep: prep, sha256: sha });
                }
                // 占位: 用给定的本地文件或游戏原版, 但记住项目内位置
                let src = if Path::new(spec).is_file() { spec.to_string() } else { name.clone() };
                let mut t = load_tex(&src, ctx, None, true)?;
                t.project_path = Some(h.clone());
                return Ok(t);
            }
        }
    }
    // 2) 本地文件
    let p = Path::new(spec);
    if p.is_file() {
        let d = std::fs::read(p)?;
        if modproj_stub(&d) {
            let mut t = load_tex(&name, ctx, None, true)?;
            t.label = format!("{} (占位 → {})", p.display(), t.label);
            return Ok(t);
        }
        let sha = modproj::sha256_hex(&d);
        let (dds, k, prep) = assemble(&p.display().to_string(), d, |n| Ok(std::fs::read(vol_path(p, n)).ok()))?;
        let vpath = ctx.and_then(|c| {
            let v: Vec<_> = c.index.entries.iter().filter(|e| e.path.rsplit('/').next().map(|x| x.eq_ignore_ascii_case(&name)).unwrap_or(false)).collect();
            (v.len() == 1).then(|| v[0].path.clone())
        });
        return Ok(Tex { label: p.display().to_string(), file_name: name, vpath, project_path: None, dds, k, bc7prep: prep, sha256: sha });
    }
    // 3) 游戏
    let ctx = ctx.ok_or_else(|| anyhow!("{spec} 不是本地文件, 也没有找到游戏客户端(用 --game 指定)"))?;
    let e = if let Some(e) = ctx.index.get(spec) {
        e
    } else {
        let mut hits: Vec<_> = ctx.index.search(spec).map_err(|e| anyhow!("{e}"))?.into_iter().filter(|e| e.path.to_lowercase().ends_with(".dds")).collect();
        if hits.is_empty() && !spec.contains('/') {
            hits = ctx.index.entries.iter().filter(|e| e.path.rsplit('/').next().map(|x| x.eq_ignore_ascii_case(spec)).unwrap_or(false)).collect();
        }
        match hits.len() {
            0 => bail!("游戏内没有匹配 {spec:?} 的 .dds"),
            1 => hits[0],
            n => bail!("{spec:?} 匹配到 {n} 个 .dds, 请写得更具体: {}", hits.iter().take(8).map(|e| e.path.as_str()).collect::<Vec<_>>().join(", ")),
        }
    };
    let d = ctx.reader.read(e).map_err(|x| anyhow!("读取 {}: {x}", e.path))?;
    let sha = modproj::sha256_hex(&d);
    let stem = e.path[..e.path.len() - 4].to_string();
    let (dds, k, prep) = assemble(&e.path, d, |n| match ctx.index.get(&format!("{stem}.dd{n}")) {
        Some(ve) => Ok(Some(ctx.reader.read(ve).map_err(|x| anyhow!("读取 {}: {x}", ve.path))?)),
        None => Ok(None),
    })?;
    Ok(Tex { label: e.path.clone(), file_name: e.path.rsplit('/').next().unwrap().to_string(), vpath: Some(e.path.clone()), project_path: None, dds, k, bc7prep: prep, sha256: sha })
}

pub fn tex_info_json(t: &Tex) -> Value {
    let h = &t.dds.header;
    let vols: Vec<Value> = (0..t.k)
        .map(|n| {
            let (w, hh) = h.level_dims(n);
            json!({"file": format!(".dd{n}"), "mip": n, "size": [w, hh]})
        })
        .collect();
    let (tw, th) = h.level_dims(t.k);
    let tail = json!({"first_mip": t.k, "size": [tw, th], "levels": h.levels() - t.k});
    json!({"source": t.label, "vpath": t.vpath, "format": h.format.name(), "dxgi": h.dxgi, "width": h.width, "height": h.height, "levels": h.levels(),
        "volumes": t.k, "bc7prep_in_source": t.bc7prep, "sha256_of_dds": t.sha256, "volume_files": vols, "tail": tail})
}

/// 默认 mip 过滤: 颜色贴图(_a / ClanSkin_a)按 sRGB, 其余(_mg/_n/_ao…)线性
pub fn default_filter(name: &str) -> MipFilter {
    let n = name.to_lowercase();
    let stem = n.trim_end_matches(".dds");
    if stem.ends_with("_a") || stem.ends_with("_albedo") || stem.ends_with("_d") {
        MipFilter::BoxSrgb
    } else {
        MipFilter::BoxLinear
    }
}

/// 由新的顶级图像编码出与原贴图同格式/同级数的 DDS, 按原分卷数拆分, 写到 dst(.dds) 及其 .ddN。返回写出的文件。
pub fn write_tex(top: &Image, like: &Tex, dst: &Path, q: Quality, filter: Option<MipFilter>, format: Option<Format>) -> Result<(Vec<(PathBuf, String)>, Dds)> {
    let h = &like.dds.header;
    let fmt = format.unwrap_or(h.format);
    if fmt.block_bytes().is_none() && !matches!(fmt, Format::Rgba8 | Format::Bgra8) {
        bail!("不支持写出 {} 格式", fmt.name());
    }
    let same_dims = top.width == h.width && top.height == h.height;
    let levels = if same_dims { h.levels() } else { 0 };
    let f = filter.unwrap_or_else(|| default_filter(&like.file_name));
    let chain = texture::mip_chain(top.clone(), levels, f);
    let template = (same_dims && fmt == h.format && h.layers() == 1).then_some(&like.dds);
    let full = texture::encode_dds(&chain, fmt, q, template).map_err(|e| anyhow!("编码: {e}"))?;
    let k = if same_dims { like.k } else if like.k > 0 { dds::volume_count_for(top.width, top.height, 512) } else { 0 };
    let (tail, vols) = dds::split_volumes(&full, k).map_err(|e| anyhow!("拆分卷: {e}"))?;
    if let Some(d) = dst.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut out = Vec::new();
    std::fs::write(dst, &tail)?;
    out.push((dst.to_path_buf(), modproj::sha256_hex(&tail)));
    for (n, v) in vols.iter().enumerate() {
        let p = vol_path(dst, n as u32);
        std::fs::write(&p, v)?;
        out.push((p, modproj::sha256_hex(v)));
    }
    // 清掉目标位置残留的更多分卷(例如旧文件分卷更多)
    for n in vols.len() as u32..8 {
        let p = vol_path(dst, n);
        if p.exists() {
            std::fs::remove_file(&p)?;
        }
    }
    Ok((out, full))
}

/// 输出位置: -o 显式; 否则 --project: 项目里同名文件的位置, 或 assets/res_mods/<游戏路径>
pub fn resolve_dst(out: Option<&Path>, proj: Option<&Project>, t: &Tex) -> Result<PathBuf> {
    if let Some(o) = out {
        // 已存在的目录, 或不以 .dds 结尾的路径 → 视为目录
        if o.is_dir() || !o.to_string_lossy().to_lowercase().ends_with(".dds") {
            std::fs::create_dir_all(o)?;
            return Ok(o.join(&t.file_name));
        }
        return Ok(o.to_path_buf());
    }
    let pj = proj.ok_or_else(|| anyhow!("需要 -o <输出.dds> 或 --project <项目目录>"))?;
    if let Some(p) = &t.project_path {
        return Ok(p.clone());
    }
    let hits = pj.find_by_name(&t.file_name)?;
    if let Some(h) = hits.first() {
        return Ok(h.clone());
    }
    let vp = t.vpath.as_ref().ok_or_else(|| anyhow!("{} 不知道游戏内路径, 无法决定在项目里的位置; 请用 -o", t.label))?;
    let mut p = pj.assets();
    for c in vp.split('/') {
        if c == ".." || c.is_empty() {
            bail!("不安全的路径 {vp}");
        }
        p.push(c);
    }
    Ok(p)
}

pub fn preview(img: &Image, path: &Path, max: u32) -> Result<()> {
    let mut im = img.clone();
    while im.width > max || im.height > max {
        im = texture::downsample(&im, MipFilter::BoxSrgb);
    }
    std::fs::write(path, texture::write_png(&im, false).map_err(|e| anyhow!("{e}"))?)?;
    Ok(())
}

/// 遮罩参数(recolor / glow 共用)
#[derive(clap::Args, Debug, Clone, Default)]
pub struct MaskArgs {
    /// 遮罩图(PNG, 取 R×A; 白=选中), 尺寸不同时双线性缩放
    #[arg(long)]
    pub mask: Vec<PathBuf>,
    /// UV 矩形 u0,v0,u1,v1(0..1, 左上为原点), 可重复(并集)
    #[arg(long = "uv-rect", value_name = "U0,V0,U1,V1")]
    pub uv_rect: Vec<String>,
    /// 从 GLB(wowsmod export 的产物)按部件生成遮罩, 可给多个
    #[arg(long)]
    pub glb: Vec<PathBuf>,
    /// 配合 --glb: 材质名正则(如 "DeckHouse_skinned")
    #[arg(long)]
    pub material: Option<String>,
    /// 配合 --glb: 网格名正则(如 "^rs3$")
    #[arg(long)]
    pub mesh: Option<String>,
    /// 配合 --glb: 盒子 x0,y0,z0,x1,y1,z1(GLB 坐标), 可重复; 默认按像素裁剪(条带边缘平直)
    #[arg(long = "box", value_name = "X0,Y0,Z0,X1,Y1,Z1")]
    pub boxes: Vec<String>,
    /// --box 改回按三角形重心整块选择(旧行为; 大三角形会出锯齿块)
    #[arg(long)]
    pub box_tri: bool,
    /// 由 GLB 生成时 UV 岛外扩像素(防 mip 漏边)
    #[arg(long, default_value_t = 4)]
    pub dilate: u32,
    /// 反选
    #[arg(long)]
    pub invert_mask: bool,
    /// 羽化半径(像素)
    #[arg(long, default_value_t = 0)]
    pub feather: u32,
}

impl MaskArgs {
    pub fn is_empty(&self) -> bool {
        self.mask.is_empty() && self.uv_rect.is_empty() && self.glb.is_empty()
    }
    /// 生成遮罩; 返回(遮罩, 统计)。全空时返回 None(= 全图)。
    pub fn build(&self, w: u32, h: u32) -> Result<(Option<Mask>, Value)> {
        if self.is_empty() {
            if self.invert_mask {
                bail!("--invert-mask 需要先给遮罩");
            }
            return Ok((None, json!({"kind": "all"})));
        }
        let mut m = Mask::filled(w, h, 0.0);
        let mut info = serde_json::Map::new();
        for p in &self.mask {
            let img = texture::read_png(&std::fs::read(p).with_context(|| format!("读取遮罩 {}", p.display()))?).map_err(|e| anyhow!("{}: {e}", p.display()))?;
            m.union(&Mask::from_image(&img, w, h));
        }
        for r in &self.uv_rect {
            let v: Vec<f32> = r.split(',').map(|x| x.trim().parse::<f32>()).collect::<Result<_, _>>().map_err(|_| anyhow!("--uv-rect 格式: u0,v0,u1,v1"))?;
            if v.len() != 4 {
                bail!("--uv-rect 需要 4 个数");
            }
            m.add_uv_rect([v[0], v[1], v[2], v[3]], 1.0);
        }
        if !self.glb.is_empty() {
            let mut prims = Vec::new();
            for g in &self.glb {
                let d = std::fs::read(g).with_context(|| format!("读取 {}", g.display()))?;
                let (_, ps) = wowsmod_formats::glb::read_glb(&d).map_err(|e| anyhow!("{}: {e}", g.display()))?;
                prims.extend(ps);
            }
            let f = uvmask::TriFilter {
                material: self.material.as_deref().map(|r| regex::Regex::new(&format!("(?i){r}"))).transpose().map_err(|e| anyhow!("--material 正则: {e}"))?,
                mesh: self.mesh.as_deref().map(|r| regex::Regex::new(&format!("(?i){r}"))).transpose().map_err(|e| anyhow!("--mesh 正则: {e}"))?,
                boxes: self.boxes.iter().map(|b| uvmask::parse_box(b).ok_or_else(|| anyhow!("--box 格式: x0,y0,z0,x1,y1,z1"))).collect::<Result<_>>()?,
                pixel_clip: !self.box_tri,
            };
            let (gm, st) = uvmask::mask_from_prims(&prims, &f, w, h, self.dilate);
            if st.triangles == 0 {
                bail!("--glb 条件没有选中任何三角形(检查 --material/--mesh/--box)");
            }
            info.insert("glb".into(), serde_json::to_value(&st)?);
            m.union(&gm);
        }
        if self.invert_mask {
            m.invert();
        }
        m.blur(self.feather);
        info.insert("coverage".into(), json!(m.coverage()));
        Ok((Some(m), Value::Object(info)))
    }
}

/// 颜色挑选参数
#[derive(clap::Args, Debug, Clone, Default)]
pub struct SelectArgs {
    /// 只改 HSL 饱和度在此范围的像素, 如 0:0.25
    #[arg(long = "sat-range", value_name = "LO:HI")]
    pub sat_range: Option<String>,
    /// 只改 HSL 亮度在此范围的像素, 如 0.1:0.9
    #[arg(long = "light-range", value_name = "LO:HI")]
    pub light_range: Option<String>,
    /// 只改色相(度)在此范围的像素, 可跨 360, 如 330:30
    #[arg(long = "hue-range", value_name = "LO:HI")]
    pub hue_range: Option<String>,
    /// 排除色相范围(度), 可重复, 如 0:60 排除木甲板/锈色/船底红(只影响有饱和度的像素)
    #[arg(long = "exclude-hue", value_name = "LO:HI")]
    pub exclude_hue: Vec<String>,
    /// 挑选的软边宽度
    #[arg(long, default_value_t = 0.05)]
    pub soft: f32,
}
fn range(s: &Option<String>) -> Result<Option<(f32, f32)>> {
    s.as_ref()
        .map(|x| {
            let (a, b) = x.split_once(':').ok_or_else(|| anyhow!("范围格式 LO:HI: {x}"))?;
            Ok((a.trim().parse::<f32>()?, b.trim().parse::<f32>()?))
        })
        .transpose()
}
impl SelectArgs {
    pub fn build(&self) -> Result<Selector> {
        let ex = self.exclude_hue.iter().map(|x| range(&Some(x.clone())).map(|r| r.unwrap())).collect::<Result<Vec<_>>>()?;
        Ok(Selector { sat: range(&self.sat_range)?, light: range(&self.light_range)?, hue: range(&self.hue_range)?, exclude_hue: ex, soft: self.soft })
    }
}

pub fn quality(s: &str) -> Result<Quality> {
    Quality::parse(s).ok_or_else(|| anyhow!("--quality 只能是 fast/normal/high"))
}

pub fn outputs_json(pj: Option<&Project>, outs: &[(PathBuf, String)]) -> Value {
    json!(outs.iter().map(|(p, s)| json!({"path": pj.map(|j| j.rel(p)).unwrap_or_else(|| p.display().to_string()), "sha256": s})).collect::<Vec<_>>())
}

/// 改 .mfm(仅 SDK 明文 .mfm): 切换到 emissive 着色器并设置属性
pub fn edit_mfms(pj: Option<&Project>, mfms: &[String], power: Option<f32>, sets: &[String], identifiers: bool) -> Result<Vec<Value>> {
    let mut res = Vec::new();
    for m in mfms {
        let path = if Path::new(m).is_file() {
            PathBuf::from(m)
        } else {
            let pj = pj.ok_or_else(|| anyhow!("--mfm {m}: 不是文件, 也没有 --project"))?;
            let n = if m.to_lowercase().ends_with(".mfm") { m.clone() } else { format!("{m}.mfm") };
            let mut hits = pj.find_by_name(&n)?;
            // ModsSDK.zip 下的同名文件是官方 SDK 原件, 不改
            hits.retain(|p| !p.to_string_lossy().replace('\\', "/").contains("/ModsSDK.zip/"));
            match hits.len() {
                0 => bail!("项目里没有 {n}(SDK 路线需要先 wowsmod mod import-sdk)"),
                1 => hits.remove(0),
                k => bail!("项目里有 {k} 个 {n}"),
            }
        };
        let t = std::fs::read_to_string(&path).with_context(|| format!("读取 {}", path.display()))?;
        let old_fx = mfm::fx(&t).ok_or_else(|| anyhow!("{} 没有 <fx>", path.display()))?;
        let new_fx = mfm::emissive_variant(&old_fx).ok_or_else(|| anyhow!("{}: 着色器 {old_fx} 没有已知的 emissive 变体(只支持 PBS_ship / PBS_ship_skinned)", path.display()))?;
        let mut n = mfm::set_fx(&t, &new_fx)?;
        if let Some(pw) = power {
            n = mfm::set_property(&n, "emissivePower", "Float", &format!("{pw:.4}"))?;
        }
        for s in sets {
            let (name, rest) = s.split_once('=').ok_or_else(|| anyhow!("--mfm-set 格式 name=Type:value, 如 emissionAnimationMode=Int:1"))?;
            let (ty, val) = rest.split_once(':').ok_or_else(|| anyhow!("--mfm-set 格式 name=Type:value"))?;
            n = mfm::set_property(&n, name.trim(), ty.trim(), val.trim())?;
        }
        std::fs::write(&path, &n)?;
        // .visual 的材质标识决定实际着色器(只改 fx 不生效), 同步改成 SHIPMAT_EMISSIVE_PBS_*
        let mut idents = Vec::new();
        if identifiers {
            if let Some(pj) = pj {
                let fname = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                for f in pj.files()? {
                    let fs = f.to_string_lossy().replace('\\', "/");
                    if !fs.to_lowercase().ends_with(".visual") || fs.contains("/ModsSDK.zip/") {
                        continue;
                    }
                    let Ok(vt) = std::fs::read_to_string(&f) else { continue };
                    let (nv, ch) = mfm::set_emissive_identifiers(&vt, &fname);
                    if !ch.is_empty() {
                        std::fs::write(&f, nv)?;
                        for (a, b) in ch {
                            idents.push(json!({"visual": pj.rel(&f), "from": a, "to": b}));
                        }
                    }
                }
            }
        }
        res.push(json!({"mfm": pj.map(|j| j.rel(&path)).unwrap_or_else(|| path.display().to_string()), "fx_before": old_fx, "fx_after": new_fx, "visual_identifiers": idents, "properties": mfm::properties(&n).into_iter().filter(|(_, ty, _)| ty != "Texture").map(|(a, b, c)| json!([a, b, c])).collect::<Vec<_>>()}));
    }
    Ok(res)
}

pub fn recolor_cfg(to_color: Option<&str>, hue: f32, sat: f32, lmul: f32, ladd: f32, strength: f32, detail: f32, chroma_detail: f32, keep_lightness: bool) -> Result<Recolor> {
    Ok(Recolor {
        to_color: to_color.map(|c| wowsmod_formats::color::parse_color(c).ok_or_else(|| anyhow!("颜色格式: #RRGGBB 或 r,g,b: {c}"))).transpose()?,
        detail,
        chroma_detail,
        match_lightness: !keep_lightness,
        hue_shift: hue,
        sat_mul: sat,
        light_mul: lmul,
        light_add: ladd,
        strength,
    })
}

pub fn apply_recolor(img: &mut Image, mask: Option<&Mask>, sel: &Selector, rc: &Recolor) -> recolor::RecolorStats {
    let w = recolor::selection(img, mask, sel, rc.strength);
    recolor::recolor(img, &w, rc)
}
