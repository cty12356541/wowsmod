//! M2 子命令: tex / mask / glow / mod / install / uninstall
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use clap::Subcommand;
use serde_json::{json, Value};

use wowsmod_formats::{
    dds::Format,
    recolor::{self, GlowEdit, Selector},
    texture::{self, MipFilter},
};

use crate::modproj::{self, Project};
use crate::texcmd::{self, MaskArgs, SelectArgs};
use crate::{emit, game_ctx, Cli};

#[derive(Subcommand)]
pub enum TexCmd {
    /// 贴图信息(格式/尺寸/mip/分卷/BC7Prep)
    Info {
        /// 本地 .dds、游戏内路径/glob/文件名, 或(配合 --project)项目里的同名贴图
        tex: String,
        #[arg(long)]
        project: Option<PathBuf>,
        /// 忽略项目里已编辑的版本, 直接用游戏原版
        #[arg(long)]
        from_game: bool,
    },
    /// DDS(含分卷自动合并、BC7Prep 自动还原) → PNG, 并写 <png>.json 旁注
    Export {
        tex: String,
        #[arg(short, long)]
        out: PathBuf,
        /// 导出第几级 mip(0 = 最大)
        #[arg(long, default_value_t = 0)]
        mip: u32,
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        from_game: bool,
    },
    /// PNG → DDS: 按 --like 贴图的格式/级数/分卷规则编码(自动生成 mip、拆 .ddN)
    Import {
        png: PathBuf,
        /// 参照贴图(决定格式、mip 级数、分卷数、默认输出位置)
        #[arg(long)]
        like: String,
        /// 输出 .dds(分卷写在旁边); 省略且给了 --project 时写进项目
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        from_game: bool,
        /// 编码质量 fast/normal/high
        #[arg(long, default_value = "high")]
        quality: String,
        /// 改用其它格式(bc1/bc3/bc5/bc7/rgba8), 默认与参照相同
        #[arg(long)]
        format: Option<String>,
        /// mip 过滤: srgb / linear(默认: _a 用 srgb, 其余 linear)
        #[arg(long)]
        mip_filter: Option<String>,
    },
    /// 不重新编码地重写容器: 还原 BC7Prep、合并再按原规则拆分卷(用于验证游戏接受普通 DDS)
    Rewrite {
        tex: String,
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        from_game: bool,
    },
    /// 换色(保留细节): 整体换成目标色 / 色相旋转 / 饱和度与亮度调整, 可用遮罩与颜色范围限定区域
    Recolor {
        tex: String,
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        from_game: bool,
        /// 目标色 #RRGGBB: 选区平均色移到该色, 像素间亮度起伏保留
        #[arg(long)]
        to_color: Option<String>,
        /// 色相旋转(度)
        #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
        hue_shift: f32,
        /// 饱和度乘数
        #[arg(long, default_value_t = 1.0)]
        sat_mul: f32,
        /// 亮度乘数
        #[arg(long, default_value_t = 1.0)]
        light_mul: f32,
        /// 亮度加量(OKLab L, -1..1)
        #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
        light_add: f32,
        /// 总强度 0..1
        #[arg(long, default_value_t = 1.0)]
        strength: f32,
        /// --to-color 时亮度细节保留系数
        #[arg(long, default_value_t = 1.0)]
        detail: f32,
        /// --to-color 时原有色斑保留系数(0 = 色调完全统一)
        #[arg(long, default_value_t = 0.35)]
        chroma_detail: f32,
        /// --to-color 时保持原平均亮度(只换色相/饱和度)
        #[arg(long)]
        keep_lightness: bool,
        #[command(flatten)]
        mask: MaskArgs,
        #[command(flatten)]
        select: SelectArgs,
        /// 另存一张缩小的预览 PNG
        #[arg(long)]
        preview: Option<PathBuf>,
        /// 另存实际使用的权重图(选区 × 颜色挑选)PNG
        #[arg(long)]
        weights_out: Option<PathBuf>,
        #[arg(long, default_value = "high")]
        quality: String,
    },
    /// 改单个通道: 在遮罩内把 R/G/B/A 设为常数或乘系数(如清零 _mg 的 B 通道 = 去掉迷彩/涂装遮罩, 让 _a 颜色直接显示)
    Channel {
        tex: String,
        /// r / g / b / a
        #[arg(long)]
        channel: String,
        /// 设为该值(0..255)
        #[arg(long)]
        set: Option<u8>,
        /// 乘以该系数
        #[arg(long)]
        mul: Option<f32>,
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        from_game: bool,
        #[command(flatten)]
        mask: MaskArgs,
        #[arg(long)]
        preview: Option<PathBuf>,
        #[arg(long, default_value = "high")]
        quality: String,
    },
    /// 把全彩图案(UV 空间 RGBA, 如 bake-ref 输出或外部绘制/AI 生成的 UV 图)按 alpha 合成进贴图, 保留底图的明暗细节(AO/板缝)
    Composite {
        tex: String,
        /// UV 空间 RGBA PNG(与贴图同尺寸; 不同尺寸按最近邻缩放), 可重复, 按顺序叠加
        #[arg(long = "over", required = true)]
        over: Vec<PathBuf>,
        /// 混合: normal(图案颜色 × 底图明暗) / multiply / screen
        #[arg(long, default_value = "normal")]
        blend: String,
        /// 保留底图明暗细节的强度 0..1(normal 时生效)
        #[arg(long, default_value_t = 0.35)]
        keep_detail: f32,
        /// 整体不透明度 0..1
        #[arg(long, default_value_t = 1.0)]
        opacity: f32,
        /// 图案 alpha 低于该值视为透明(去掉 bake 边缘噪点)
        #[arg(long, default_value_t = 0.02)]
        alpha_floor: f32,
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        from_game: bool,
        /// 额外限定区域的遮罩(与图案 alpha 相乘)
        #[command(flatten)]
        mask: MaskArgs,
        #[arg(long)]
        preview: Option<PathBuf>,
        #[arg(long, default_value = "high")]
        quality: String,
    },
}

#[derive(Subcommand)]
pub enum ModCmd {
    /// 新建 mod 项目(mod.toml + edits/ + assets/res_mods/)
    Init {
        dir: PathBuf,
        #[arg(long)]
        name: String,
        /// 目标船 SDK 名, 如 FSB510_Bourgogne
        #[arg(long, default_value = "")]
        ship: String,
        /// loose(直接覆盖游戏路径) 或 sdk(官方 ModsSDK 路线)
        #[arg(long, default_value = "loose")]
        mode: String,
    },
    /// 导入 contentSdk.extractSources 的产物并改名为本项目(可同时拷入 ModsSDK.zip/<船> 子集)
    ImportSdk {
        dir: PathBuf,
        /// extractSources 产物目录(PnFMods/<旧名>/ 或其中的 <船>/); 省略则用本机上次为同一条船导入时记下的位置
        src: Option<PathBuf>,
        /// res_mods/PnFMods/ModsSDK.zip/<船> 目录(官方 SDK 明文源)
        #[arg(long)]
        sdk_dir: Option<PathBuf>,
    },
    /// 项目状态: 文件、占位、编辑记录、是否已安装
    Status { dir: PathBuf },
    /// 改项目元数据: 真实舰长(用于米/单位换算)等
    Set {
        dir: PathBuf,
        /// 真实舰长(米), 例如从百科查到的全长
        #[arg(long)]
        ship_length: Option<f64>,
        #[arg(long)]
        description: Option<String>,
    },
    /// 素材许可标记: 网页/口头声明的「禁止二次分发」等手动写进 mod.toml [license](pack 默认拒绝)
    License {
        dir: PathBuf,
        /// 许可说明(来源 + 原文要点), 可重复
        #[arg(long = "note")]
        notes: Vec<String>,
        /// 清除标记(只在受限素材已全部移除后)
        #[arg(long)]
        clear: bool,
    },
}

fn open_proj(p: &Option<PathBuf>) -> Result<Option<Project>> {
    p.as_ref().map(|d| Project::open(d)).transpose()
}

fn need_game_for(spec: &str) -> bool {
    !Path::new(spec).is_file()
}

pub fn cmd_tex(cli: &Cli, c: &TexCmd) -> Result<i32> {
    match c {
        TexCmd::Info { tex, project, from_game } => {
            let pj = open_proj(project)?;
            let ctx = game_ctx(cli, false)?;
            let t = texcmd::load_tex(tex, ctx.as_ref(), pj.as_ref(), *from_game)?;
            let j = texcmd::tex_info_json(&t);
            if cli.json {
                emit(cli, &json!({"ok": true, "texture": j}));
            } else {
                let h = &t.dds.header;
                println!("{}\n  格式 {}  {}x{}  mip {} 级  分卷 {}  BC7Prep {}", t.label, h.format.name(), h.width, h.height, h.levels(), t.k, if t.bc7prep { "是" } else { "否" });
            }
            Ok(0)
        }
        TexCmd::Export { tex, out, mip, project, from_game } => {
            let pj = open_proj(project)?;
            let ctx = game_ctx(cli, need_game_for(tex) && pj.is_none())?;
            let t = texcmd::load_tex(tex, ctx.as_ref(), pj.as_ref(), *from_game)?;
            if *mip >= t.dds.header.levels() {
                bail!("--mip {mip} 超出范围(共 {} 级)", t.dds.header.levels());
            }
            let img = texture::decode_level(&t.dds, *mip).map_err(|e| anyhow!("解码: {e}"))?;
            let keep_alpha = texture::has_alpha(&img);
            if let Some(d) = out.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(out, texture::write_png(&img, keep_alpha).map_err(|e| anyhow!("{e}"))?)?;
            let side = json!({"wowsmod": "tex export", "texture": texcmd::tex_info_json(&t), "mip": mip, "png_has_alpha": keep_alpha,
                "note": "改图后用 wowsmod tex import <png> --like <同一贴图> 写回; 尺寸不变时沿用原 mip 级数与分卷"});
            let sp = PathBuf::from(format!("{}.json", out.display()));
            std::fs::write(&sp, serde_json::to_string_pretty(&side)?)?;
            if cli.json {
                emit(cli, &json!({"ok": true, "png": out.display().to_string(), "sidecar": sp.display().to_string(), "width": img.width, "height": img.height, "alpha": keep_alpha}));
            } else {
                println!("已导出 {} ({}x{}{})", out.display(), img.width, img.height, if keep_alpha { ", 含透明" } else { "" });
            }
            Ok(0)
        }
        TexCmd::Import { png, like, out, project, from_game, quality, format, mip_filter } => {
            let pj = open_proj(project)?;
            let ctx = game_ctx(cli, need_game_for(like))?;
            let t = texcmd::load_tex(like, ctx.as_ref(), pj.as_ref(), *from_game)?;
            let img = texture::read_png(&std::fs::read(png)?).map_err(|e| anyhow!("{}: {e}", png.display()))?;
            let q = texcmd::quality(quality)?;
            let fmt = format.as_deref().map(|f| Format::parse(f).ok_or_else(|| anyhow!("未知格式 {f}"))).transpose()?;
            let filt = mip_filter.as_deref().map(parse_filter).transpose()?;
            let dst = texcmd::resolve_dst(out.as_deref(), pj.as_ref(), &t)?;
            let tm = Instant::now();
            let (outs, full) = texcmd::write_tex(&img, &t, &dst, q, filt, fmt)?;
            // 质量: 顶级解码后与输入比较
            let back = texture::decode_level(&full, 0).map_err(|e| anyhow!("{e}"))?;
            let psnr = if back.width == img.width && back.height == img.height { Some(texture::psnr(&img, &back, false)) } else { None };
            let rec = json!({"input_png": png.display().to_string(), "input_png_sha256": modproj::sha256_hex(&std::fs::read(png)?), "like": t.label, "like_sha256": t.sha256,
                "format": full.header.format.name(), "levels": full.header.levels(), "volumes": outs.len() - 1, "psnr_rgb_top": psnr, "outputs": texcmd::outputs_json(pj.as_ref(), &outs), "encode_seconds": tm.elapsed().as_secs_f64()});
            let ep = if let Some(pj) = &pj { Some(pj.record_edit("tex_import", rec.clone())?) } else { None };
            report(cli, "已写入", &rec, ep.as_deref());
            Ok(0)
        }
        TexCmd::Rewrite { tex, out, project, from_game } => {
            let pj = open_proj(project)?;
            let ctx = game_ctx(cli, need_game_for(tex))?;
            let t = texcmd::load_tex(tex, ctx.as_ref(), pj.as_ref(), *from_game)?;
            let dst = texcmd::resolve_dst(out.as_deref(), pj.as_ref(), &t)?;
            let (tail, vols) = wowsmod_formats::dds::split_volumes(&t.dds, t.k).map_err(|e| anyhow!("{e}"))?;
            if let Some(d) = dst.parent() {
                std::fs::create_dir_all(d)?;
            }
            let mut outs = vec![(dst.clone(), modproj::sha256_hex(&tail))];
            std::fs::write(&dst, &tail)?;
            for (n, v) in vols.iter().enumerate() {
                let p = dst.with_extension(format!("dd{n}"));
                std::fs::write(&p, v)?;
                outs.push((p, modproj::sha256_hex(v)));
            }
            let rec = json!({"source": t.label, "source_sha256": t.sha256, "bc7prep_removed": t.bc7prep, "volumes": t.k, "outputs": texcmd::outputs_json(pj.as_ref(), &outs)});
            let ep = if let Some(pj) = &pj { Some(pj.record_edit("tex_rewrite", rec.clone())?) } else { None };
            report(cli, "已重写", &rec, ep.as_deref());
            Ok(0)
        }
        TexCmd::Recolor { tex, out, project, from_game, to_color, hue_shift, sat_mul, light_mul, light_add, strength, detail, chroma_detail, keep_lightness, mask, select, preview, weights_out, quality } => {
            let pj = open_proj(project)?;
            let ctx = game_ctx(cli, need_game_for(tex))?;
            let t = texcmd::load_tex(tex, ctx.as_ref(), pj.as_ref(), *from_game)?;
            let q = texcmd::quality(quality)?;
            let rc = texcmd::recolor_cfg(to_color.as_deref(), *hue_shift, *sat_mul, *light_mul, *light_add, *strength, *detail, *chroma_detail, *keep_lightness)?;
            if rc.to_color.is_none() && rc.hue_shift == 0.0 && rc.sat_mul == 1.0 && rc.light_mul == 1.0 && rc.light_add == 0.0 {
                bail!("没有指定任何换色操作(--to-color / --hue-shift / --sat-mul / --light-mul / --light-add)");
            }
            let mut img = texture::decode_level(&t.dds, 0).map_err(|e| anyhow!("解码: {e}"))?;
            let (m, minfo) = mask.build(img.width, img.height)?;
            let sel = select.build()?;
            let w = recolor::selection(&img, m.as_ref(), &sel, rc.strength);
            if let Some(wp) = weights_out {
                let wm = recolor::Mask { width: img.width, height: img.height, v: w.clone() };
                texcmd::preview(&wm.to_image(), wp, 4096)?;
            }
            let st = recolor::recolor(&mut img, &w, &rc);
            let dst = texcmd::resolve_dst(out.as_deref(), pj.as_ref(), &t)?;
            if let Some(pv) = preview {
                texcmd::preview(&img, pv, 1024)?;
            }
            let (outs, _) = texcmd::write_tex(&img, &t, &dst, q, None, None)?;
            let rec = json!({"source": t.label, "source_sha256": t.sha256, "mask": minfo, "selector": format!("{sel:?}"), "recolor": format!("{rc:?}"), "stats": st, "outputs": texcmd::outputs_json(pj.as_ref(), &outs)});
            let ep = if let Some(pj) = &pj { Some(pj.record_edit("tex_recolor", rec.clone())?) } else { None };
            report(cli, "已换色", &rec, ep.as_deref());
            Ok(0)
        }
        TexCmd::Composite { tex, over, blend, keep_detail, opacity, alpha_floor, out, project, from_game, mask, preview, quality } => {
            let pj = open_proj(project)?;
            let ctx = game_ctx(cli, need_game_for(tex))?;
            let t = texcmd::load_tex(tex, ctx.as_ref(), pj.as_ref(), *from_game)?;
            let q = texcmd::quality(quality)?;
            if !["normal", "multiply", "screen"].contains(&blend.as_str()) {
                bail!("--blend 只能是 normal / multiply / screen");
            }
            let mut img = texture::decode_level(&t.dds, 0).map_err(|e| anyhow!("解码: {e}"))?;
            let (w, h) = (img.width as usize, img.height as usize);
            let (m, minfo) = mask.build(img.width, img.height)?;
            // 底图亮度的中位数作为"平均明暗", 细节 = L / 中位数
            let lum = |p: &[u8]| 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32;
            let mut ls: Vec<f32> = img.px.chunks_exact(4).step_by(7).map(lum).collect();
            ls.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let med = ls.get(ls.len() / 2).copied().unwrap_or(128.0).max(8.0);
            let mut covered = 0u64;
            let mut infos = vec![];
            for op in over {
                let o = texture::read_png(&std::fs::read(op).with_context(|| format!("读取 {}", op.display()))?).map_err(|e| anyhow!("{}: {e}", op.display()))?;
                infos.push(json!({"over": op.display().to_string(), "size": [o.width, o.height]}));
                let (ow, oh) = (o.width as usize, o.height as usize);
                for y in 0..h {
                    let oy = (y * oh / h).min(oh - 1);
                    for x in 0..w {
                        let ox = (x * ow / w).min(ow - 1);
                        let op_ = &o.px[(oy * ow + ox) * 4..(oy * ow + ox) * 4 + 4];
                        let mut a = op_[3] as f32 / 255.0;
                        if a < *alpha_floor {
                            continue;
                        }
                        a *= opacity.clamp(0.0, 1.0) * m.as_ref().map(|m| m.v[y * w + x]).unwrap_or(1.0).clamp(0.0, 1.0);
                        if a <= 0.0 {
                            continue;
                        }
                        covered += 1;
                        let i = (y * w + x) * 4;
                        let base = [img.px[i] as f32, img.px[i + 1] as f32, img.px[i + 2] as f32];
                        let f = (1.0 + keep_detail * (lum(&img.px[i..i + 4]) / med - 1.0)).clamp(0.3, 1.7);
                        for c in 0..3 {
                            let oc = op_[c] as f32;
                            let v = match blend.as_str() {
                                "multiply" => base[c] * oc / 255.0,
                                "screen" => 255.0 - (255.0 - base[c]) * (255.0 - oc) / 255.0,
                                _ => oc * f,
                            };
                            img.px[i + c] = (base[c] + (v - base[c]) * a).round().clamp(0.0, 255.0) as u8;
                        }
                    }
                }
            }
            let dst = texcmd::resolve_dst(out.as_deref(), pj.as_ref(), &t)?;
            if let Some(pv) = preview {
                texcmd::preview(&img, pv, 2048)?;
            }
            let (outs, _) = texcmd::write_tex(&img, &t, &dst, q, None, None)?;
            let rec = json!({"source": t.label, "source_sha256": t.sha256, "overs": infos, "blend": blend, "keep_detail": keep_detail, "opacity": opacity, "mask": minfo, "covered_px": covered, "coverage": covered as f64 / (w * h) as f64, "outputs": texcmd::outputs_json(pj.as_ref(), &outs)});
            let ep = if let Some(pj) = &pj { Some(pj.record_edit("tex_composite", rec.clone())?) } else { None };
            report(cli, "已合成图案", &rec, ep.as_deref());
            Ok(0)
        }
        TexCmd::Channel { tex, channel, set, mul, out, project, from_game, mask, preview, quality } => {
            let pj = open_proj(project)?;
            let ctx = game_ctx(cli, need_game_for(tex))?;
            let t = texcmd::load_tex(tex, ctx.as_ref(), pj.as_ref(), *from_game)?;
            let q = texcmd::quality(quality)?;
            let ci = match channel.to_ascii_lowercase().as_str() {
                "r" => 0,
                "g" => 1,
                "b" => 2,
                "a" => 3,
                _ => bail!("--channel 只能是 r/g/b/a"),
            };
            if set.is_none() == mul.is_none() {
                bail!("--set 与 --mul 二选一");
            }
            let mut img = texture::decode_level(&t.dds, 0).map_err(|e| anyhow!("解码: {e}"))?;
            let (m, minfo) = mask.build(img.width, img.height)?;
            let (mut before, mut after, mut n) = (0f64, 0f64, 0f64);
            for (i, p) in img.px.chunks_exact_mut(4).enumerate() {
                let w = m.as_ref().map(|m| m.v[i]).unwrap_or(1.0).clamp(0.0, 1.0);
                let old = p[ci] as f32;
                let target = if let Some(v) = set { *v as f32 } else { (old * mul.unwrap()).clamp(0.0, 255.0) };
                let v = old + (target - old) * w;
                p[ci] = v.round().clamp(0.0, 255.0) as u8;
                before += old as f64;
                after += p[ci] as f64;
                n += 1.0;
            }
            let dst = texcmd::resolve_dst(out.as_deref(), pj.as_ref(), &t)?;
            if let Some(pv) = preview {
                let mut b = img.clone();
                for p in b.px.chunks_exact_mut(4) {
                    let v = p[ci];
                    p[0] = v;
                    p[1] = v;
                    p[2] = v;
                    p[3] = 255;
                }
                texcmd::preview(&b, pv, 1024)?;
            }
            let (outs, _) = texcmd::write_tex(&img, &t, &dst, q, None, None)?;
            let st = json!({"channel": channel, "mean_before": before / n.max(1.0), "mean_after": after / n.max(1.0)});
            let rec = json!({"source": t.label, "source_sha256": t.sha256, "mask": minfo, "set": set, "mul": mul, "stats": st, "outputs": texcmd::outputs_json(pj.as_ref(), &outs)});
            let ep = if let Some(pj) = &pj { Some(pj.record_edit("tex_channel", rec.clone())?) } else { None };
            report(cli, "已改通道", &rec, ep.as_deref());
            Ok(0)
        }
    }
}

fn parse_filter(s: &str) -> Result<MipFilter> {
    match s {
        "srgb" => Ok(MipFilter::BoxSrgb),
        "linear" => Ok(MipFilter::BoxLinear),
        _ => bail!("--mip-filter 只能是 srgb 或 linear"),
    }
}

fn report(cli: &Cli, what: &str, rec: &Value, edit: Option<&Path>) {
    if cli.json {
        let mut r = rec.clone();
        r["ok"] = json!(true);
        r["edit_record"] = json!(edit.map(|p| p.display().to_string()));
        emit(cli, &r);
    } else {
        for o in rec["outputs"].as_array().into_iter().flatten() {
            println!("{what} {}", o["path"].as_str().unwrap_or(""));
        }
        if let Some(s) = rec.get("stats") {
            println!("  统计: {s}");
        }
        if let Some(p) = edit {
            println!("  已记录到 {}", p.display());
        }
    }
}

/// 独立的遮罩命令: 生成遮罩 PNG, 可叠加在贴图上看位置
pub fn cmd_mask(cli: &Cli, mask: &MaskArgs, like: Option<&str>, size: Option<&str>, out: &Path, overlay: Option<&Path>, project: &Option<PathBuf>) -> Result<i32> {
    let pj = open_proj(project)?;
    let (w, h, tex) = if let Some(l) = like {
        let ctx = game_ctx(cli, need_game_for(l))?;
        let t = texcmd::load_tex(l, ctx.as_ref(), pj.as_ref(), false)?;
        (t.dds.header.width, t.dds.header.height, Some(t))
    } else if let Some(s) = size {
        let (a, b) = s.split_once('x').unwrap_or((s, s));
        (a.parse()?, b.parse()?, None)
    } else {
        bail!("需要 --like <贴图> 或 --size WxH");
    };
    let (m, info) = mask.build(w, h)?;
    let m = m.ok_or_else(|| anyhow!("没有给任何遮罩条件(--glb/--uv-rect/--mask)"))?;
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(out, texture::write_png(&m.to_image(), false).map_err(|e| anyhow!("{e}"))?)?;
    if let (Some(op), Some(t)) = (overlay, &tex) {
        let mut img = texture::decode_level(&t.dds, 0).map_err(|e| anyhow!("{e}"))?;
        for (p, &v) in img.px.chunks_exact_mut(4).zip(&m.v) {
            let a = (v * 0.6).clamp(0.0, 1.0);
            p[0] = (p[0] as f32 * (1.0 - a) + 255.0 * a) as u8;
            p[1] = (p[1] as f32 * (1.0 - a)) as u8;
            p[2] = (p[2] as f32 * (1.0 - a) + 255.0 * a) as u8;
            p[3] = 255;
        }
        texcmd::preview(&img, op, 2048)?;
    }
    if cli.json {
        emit(cli, &json!({"ok": true, "mask": out.display().to_string(), "width": w, "height": h, "info": info}));
    } else {
        println!("已写遮罩 {} ({w}x{h}), {info}", out.display());
    }
    Ok(0)
}

#[derive(clap::Args, Debug)]
pub struct GlowArgs {
    /// _mg 贴图(R=光泽 G=金属 B=发光遮罩/贴花混合)
    #[arg(long)]
    pub mg: String,
    /// _a 漫反射贴图: 发光颜色 = 该处漫反射颜色(着色器: B² × 线性漫反射 × emissivePower), 给出时把发光区染成 --color
    #[arg(long)]
    pub diffuse: Option<String>,
    /// 发光颜色 #RRGGBB(需要 --diffuse)
    #[arg(long)]
    pub color: Option<String>,
    /// 染色时亮度细节保留(0 = 纯色发光, 1 = 原样保留明暗)
    #[arg(long, default_value_t = 0.5)]
    pub tint_detail: f32,
    /// 发光强度 0..1(写入 B = sqrt(强度))
    #[arg(long, default_value_t = 1.0)]
    pub intensity: f32,
    /// 遮罩外保留原 B 通道(默认清零; 原 B 不是发光遮罩, 保留会导致大面积误发光)
    #[arg(long)]
    pub keep_b_outside: bool,
    /// B 上限 0..255(遮罩满值处的 B; 低值如 25–50 = 大面积微光, 官方/流萤式整船微光约 ≤49)
    #[arg(long, default_value_t = 255)]
    pub b_max: u8,
    #[command(flatten)]
    pub mask: MaskArgs,
    /// 要切到发光着色器的 .mfm(项目内文件名或路径), 可重复; 只有用这些材质的部件会发光
    #[arg(long)]
    pub mfm: Vec<String>,
    /// emissivePower(着色器默认 2.0; 官方太空皮肤用 0.5)
    #[arg(long)]
    pub power: Option<f32>,
    /// 额外设置 .mfm 属性 name=Type:value(如 emissionAnimationMode=Int:1), 可重复
    #[arg(long = "mfm-set")]
    pub mfm_set: Vec<String>,
    /// 不改 .visual 的材质标识(默认会把引用该 .mfm 的 SHIPMAT_PBS_* 改成 SHIPMAT_EMISSIVE_PBS_*, 否则引擎不用发光着色器)
    #[arg(long)]
    pub keep_identifier: bool,
    /// 输出目录(不用项目时)
    #[arg(short, long)]
    pub out: Option<PathBuf>,
    #[arg(long)]
    pub project: Option<PathBuf>,
    #[arg(long)]
    pub from_game: bool,
    #[arg(long, default_value = "high")]
    pub quality: String,
    /// 预览目录: 写 mg_B.png / diffuse.png 缩略图
    #[arg(long)]
    pub preview: Option<PathBuf>,
}

pub fn cmd_glow(cli: &Cli, a: &GlowArgs) -> Result<i32> {
    let pj = open_proj(&a.project)?;
    if a.out.is_none() && pj.is_none() {
        bail!("需要 -o <目录> 或 --project");
    }
    if a.color.is_some() && a.diffuse.is_none() {
        bail!("--color 需要同时给 --diffuse(发光颜色取自漫反射贴图)");
    }
    let ctx = game_ctx(cli, need_game_for(&a.mg))?;
    let q = texcmd::quality(&a.quality)?;
    let mg = texcmd::load_tex(&a.mg, ctx.as_ref(), pj.as_ref(), a.from_game)?;
    let mut mgi = texture::decode_level(&mg.dds, 0).map_err(|e| anyhow!("{e}"))?;
    let (m, minfo) = a.mask.build(mgi.width, mgi.height)?;
    let m = m.unwrap_or_else(|| recolor::Mask::filled(mgi.width, mgi.height, 1.0));
    let gst = recolor::write_glow_channel(&mut mgi, &m, &GlowEdit { intensity: a.intensity, keep_outside: a.keep_b_outside, b_max: a.b_max });
    let out_dir = a.out.clone();
    let dst_of = |t: &texcmd::Tex| -> Result<PathBuf> { texcmd::resolve_dst(out_dir.as_deref().map(|o| o as &Path), pj.as_ref(), t) };
    if let Some(o) = &a.out {
        std::fs::create_dir_all(o)?;
    }
    let (mut outs, _) = texcmd::write_tex(&mgi, &mg, &dst_of(&mg)?, q, None, None)?;
    let mut tint = Value::Null;
    if let Some(dspec) = &a.diffuse {
        let d = texcmd::load_tex(dspec, ctx.as_ref(), pj.as_ref(), a.from_game)?;
        let mut di = texture::decode_level(&d.dds, 0).map_err(|e| anyhow!("{e}"))?;
        if let Some(c) = &a.color {
            let rc = texcmd::recolor_cfg(Some(c), 0.0, 1.0, 1.0, 0.0, 1.0, a.tint_detail, 0.15, false)?;
            let mm = m.resized(di.width, di.height);
            let st = texcmd::apply_recolor(&mut di, Some(&mm), &Selector::default(), &rc);
            tint = json!({"diffuse": d.label, "color": c, "stats": st});
            let (o2, _) = texcmd::write_tex(&di, &d, &dst_of(&d)?, q, None, None)?;
            outs.extend(o2);
        }
        if let Some(pv) = &a.preview {
            std::fs::create_dir_all(pv)?;
            texcmd::preview(&di, &pv.join("diffuse.png"), 1024)?;
        }
    }
    if let Some(pv) = &a.preview {
        std::fs::create_dir_all(pv)?;
        let mut b = mgi.clone();
        for p in b.px.chunks_exact_mut(4) {
            p[0] = p[2];
            p[1] = p[2];
            p[3] = 255;
        }
        texcmd::preview(&b, &pv.join("mg_B.png"), 1024)?;
    }
    let mfms = texcmd::edit_mfms(pj.as_ref(), &a.mfm, a.power.or(if a.mfm.is_empty() { None } else { Some(4.0) }), &a.mfm_set, !a.keep_identifier)?;
    let mut warnings = Vec::new();
    if a.mfm.is_empty() {
        warnings.push("没有 --mfm: 只改了贴图。发光需要材质用 PBS_ship_emissive 着色器(SDK 路线可改明文 .mfm); 否则 B 通道只影响贴花混合, 不会发光".to_string());
    }
    let rec = json!({"mg": mg.label, "mg_sha256": mg.sha256, "mask": minfo, "glow": gst, "intensity": a.intensity, "b_max": a.b_max, "tint": tint, "mfm": mfms, "outputs": texcmd::outputs_json(pj.as_ref(), &outs), "warnings": warnings});
    let ep = if let Some(pj) = &pj { Some(pj.record_edit("glow", rec.clone())?) } else { None };
    report(cli, "已写入", &rec, ep.as_deref());
    if !cli.json {
        for w in &warnings {
            eprintln!("警告: {w}");
        }
        for m in &mfms {
            println!("  材质 {}: {} → {}", m["mfm"].as_str().unwrap_or(""), m["fx_before"].as_str().unwrap_or(""), m["fx_after"].as_str().unwrap_or(""));
        }
    }
    Ok(0)
}

fn emit_or_print(cli: &Cli, j: &Value) {
    if cli.json {
        emit(cli, j);
    } else {
        println!("{}", serde_json::to_string_pretty(j).unwrap_or_default());
    }
}

pub fn cmd_mod(cli: &Cli, c: &ModCmd) -> Result<i32> {
    match c {
        ModCmd::Init { dir, name, ship, mode } => {
            let build = game_ctx(cli, false)?.map(|g| g.install.build.clone()).unwrap_or_default();
            let p = Project::init(dir, name, ship, mode, &build)?;
            if cli.json {
                emit(cli, &json!({"ok": true, "project": p.root.display().to_string(), "mod": p.cfg.meta.name, "mode": p.cfg.install.mode, "game_build": build}));
            } else {
                println!("已创建 mod 项目 {} (mode={})", p.root.display(), p.cfg.install.mode);
            }
            Ok(0)
        }
        ModCmd::ImportSdk { dir, src, sdk_dir } => {
            let p = Project::open(dir)?;
            if p.cfg.install.mode != "sdk" {
                bail!("项目 mode 不是 sdk");
            }
            let ship = p.cfg.meta.ship.clone();
            let known = crate::m4cmd::sdk_source(&ship);
            let src = match (src, &known) {
                (Some(s), _) => s.clone(),
                (None, Some(k)) => PathBuf::from(k["src"].as_str().unwrap_or("")),
                (None, None) => bail!("本机还没有 {ship} 的 extractSources 产物记录。下一步: 看 `wowsmod help project` 的“首次准备”, 然后 wowsmod mod import-sdk {} <产物目录> --sdk-dir <ModsSDK.zip/{ship} 目录>", dir.display()),
            };
            let sdk_dir: Option<PathBuf> = sdk_dir.clone().or_else(|| known.as_ref().and_then(|k| k["sdk_dir"].as_str()).map(PathBuf::from));
            let mut r = modproj::import_sdk(&p, &src, sdk_dir.as_deref())?;
            crate::m4cmd::remember_sdk_source(&ship, &src, sdk_dir.as_deref());
            r["src"] = json!(src.display().to_string());
            r["sdk_dir"] = json!(sdk_dir.as_ref().map(|d| d.display().to_string()));
            let ep = p.record_edit("import_sdk", r.clone())?;
            if cli.json {
                let mut r = r;
                r["ok"] = json!(true);
                r["edit_record"] = json!(ep.display().to_string());
                emit(cli, &r);
            } else {
                println!("已导入 SDK 源: {r}");
            }
            Ok(0)
        }
        ModCmd::Set { dir, ship_length, description } => {
            let mut p = Project::open(dir)?;
            if let Some(l) = ship_length {
                if *l <= 0.0 {
                    bail!("--ship-length 要 > 0(米)");
                }
                p.cfg.meta.ship_length_m = *l;
            }
            if let Some(d) = description {
                p.cfg.meta.description = d.clone();
            }
            p.save_cfg()?;
            let j = json!({"ok": true, "ship_length_m": p.cfg.meta.ship_length_m, "description": p.cfg.meta.description, "next": [format!("wowsmod model list --project {} --json  # units.m_per_unit", dir.display())]});
            emit_or_print(cli, &j);
            Ok(0)
        }
        ModCmd::License { dir, notes, clear } => {
            let mut p = Project::open(dir)?;
            if *clear {
                p.cfg.license = Default::default();
                p.save_cfg()?;
            }
            let mut added = 0;
            for n in notes.iter().filter(|n| !n.trim().is_empty()) {
                if p.mark_restricted(&format!("手动标记: {n}"))? {
                    added += 1;
                }
            }
            let j = json!({"ok": true, "no_redistribute": p.cfg.license.no_redistribute, "reasons": p.cfg.license.reasons, "added": added,
                "note": if p.cfg.license.no_redistribute { "pack 默认拒绝; 本地 install 不受影响" } else { "未标记受限" }});
            emit_or_print(cli, &j);
            Ok(0)
        }
        ModCmd::Status { dir } => {
            let p = Project::open(dir)?;
            let files = p.files()?;
            let mut stubs = 0;
            let mut bytes = 0u64;
            for f in &files {
                let d = std::fs::read(f)?;
                bytes += d.len() as u64;
                if modproj::is_binary_stub(&f.to_string_lossy(), &d) {
                    stubs += 1;
                }
            }
            let mut edits: Vec<String> = std::fs::read_dir(p.root.join("edits")).map(|r| r.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
            edits.sort();
            let installed = p.root.join(".install/manifest.json").exists();
            let j = json!({"ok": true, "mod": p.cfg.meta.name, "mode": p.cfg.install.mode, "ship": p.cfg.meta.ship, "files": files.len(), "bytes": bytes, "sdk_stubs": stubs, "edits": edits, "installed": installed});
            crate::guide::stash(&j);
            if cli.json {
                emit(cli, &j);
            } else {
                println!("{j}");
            }
            Ok(0)
        }
    }
}

/// 港口观感补偿(M2 实测标定): 渲染后线性值 ≈ a·(贴图线性值 − t)(按通道, 低端被压成 0)。
/// 标定数据: 勃艮第船体侧板, 港口默认机位白天, 两组贴图 (40.9,74.3,112.9)→截图(3.1,61.4,117.7) 与 (59.6,73.9,88.6)→(27.0,58.4,86.0)。
/// G 通道两组几乎同值, 斜率按 R/B 的平均起点估计。换船/换光照/换机位后只能当起点, 不确定。
pub const PORT_SLOPE: [f32; 3] = [0.7091, 1.1475, 1.1523];
pub const PORT_TOE: [f32; 3] = [0.0249, 0.0294, 0.0137];

pub fn port_albedo(screen: [u8; 3]) -> [f32; 3] {
    let mut o = [0f32; 3];
    for c in 0..3 {
        let l = wowsmod_formats::color::srgb_to_linear(screen[c] as f32 / 255.0);
        let a = PORT_TOE[c] + l / PORT_SLOPE[c];
        o[c] = wowsmod_formats::color::linear_to_srgb(a.clamp(0.0, 1.0)) * 255.0;
    }
    o
}

pub fn cmd_port_albedo(cli: &Cli, screen: &str, mean_offset: f32) -> Result<i32> {
    let c = wowsmod_formats::color::parse_color(screen).ok_or_else(|| anyhow!("颜色格式: #RRGGBB"))?;
    let rgb = c;
    let alb = port_albedo(rgb);
    let to: Vec<u8> = alb.iter().map(|v| (v - mean_offset).round().clamp(0.0, 255.0) as u8).collect();
    let hex = format!("#{:02X}{:02X}{:02X}", to[0], to[1], to[2]);
    let j = json!({"ok": true, "screen": screen, "surface_albedo": alb, "to_color": hex, "mean_offset": mean_offset,
        "model": {"slope": PORT_SLOPE, "toe": PORT_TOE, "note": "勃艮第船体侧板/港口默认机位白天 三点重拟合(r21: 两点版把 periwinkle 转成粉, 金实测发黄绿), 其他船/光照下是近似值"}});
    crate::guide::stash(&j);
    if cli.json { emit(cli, &j); } else { println!("港口里想看到 {screen}: 贴图表面色约 ({:.0},{:.0},{:.0}), recolor --to-color {hex}", alb[0], alb[1], alb[2]); }
    Ok(0)
}
