//! 特效层(炮口焰 / 开火烟 / 烟囱烟 / 航迹): 查找 → 看船的特效绑定 → 按项目 fx.json 主题换色 → 工作台同主题预览。
//! 不写死船名: 船的特效链从 GameParams 读; 贴图按「类」用通用路径规则匹配。
//! 真相: 粒子定义(发射器/寿命/颜色曲线)编译在 assets.bin, 不可改; 能改的是共享粒子贴图(全局生效)。
//! 见 wowsmod help workflow.layers.fx
use anyhow::{anyhow, bail, Result};
use clap::{Args, Subcommand};
use regex::Regex;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use wowsmod_formats::texture::{self, Image, Quality};

use crate::modproj::Project;
use crate::{game_ctx, texcmd, Cli};

pub const FX_FILE: &str = "fx.json";

/// 特效类: (类名, 说明, 贴图路径正则, 状态, 作用范围说明)
pub const KINDS: &[(&str, &str, &str, &str, &str)] = &[
    ("funnel", "烟囱烟(航行 RunSmoke / 港口 idlePort_RunSmoke)", r"^particles/animated/smoke_[^/]*\.dds$", "可改(港口已实证)", "全局: 烟贴图被烟囱与多种汽团共用"),
    ("muzzle", "炮口焰(主炮 GK_Shot / 副炮 PMK_Shot)", r"^particles/animated/muzzleflash[^/]*\.dds$", "文件层可改, 待训练房实测", "全局: 所有船所有炮"),
    ("shot-smoke", "开火烟/炮弹出膛烟迹", r"^trails/shot_(smoke|hat)\.dds$", "文件层可改(RGB 纯白 × 粒子色, 换色=乘色), 待训练房实测", "全局: 所有船主副炮"),
    ("wake", "航迹(WakeTrace* 粒子)", r"^(trails/foam[^/]*|particles/animated/[^/]*(foam|wake)[^/]*)\.dds$", "颜色大概率不可改(trails/foam* 是数据贴图 R=0/G=遮罩/B≈0.5), 待训练房实测", "全局; 按船只能改 GP 绑定(未验证, 有风险)"),
    ("flame", "火焰序列帧(炮口焰外焰 + 着火共用)", r"^particles/animated/flame_[^/]*\.dds$", "可改但不建议: 与着火共用, 影响战斗判读", "全局"),
];

/// 数据贴图(不是颜色): 法线/扰动/运动矢量/溶解/温度/形变/旋转/遮罩, 换色会坏
const DATA_RE: &str = r"(?i)(normal|distort|_mv|mvea|dissolve|temperature|deform|rotation|mask|_n\.dds$)";

#[derive(Subcommand, Debug)]
pub enum FxCmd {
    /// 按类列出游戏里的特效贴图(及可改性/作用范围) / list FX textures by kind
    Find {
        /// funnel | muzzle | shot-smoke | wake | flame; 省略 = 全部
        #[arg(long)]
        kind: Option<String>,
    },
    /// 读 GameParams 列出一条船的特效绑定(挂点 → 粒子 XML), 按类归组 / ship effect bindings
    ShipEffects(ShipFxArgs),
    /// 在项目里写 fx.json 主题模板(或从文件复制) / write project fx.json
    Init {
        project: PathBuf,
        /// 从已有主题 JSON 复制
        #[arg(long)]
        from: Option<PathBuf>,
        #[arg(long)]
        force: bool,
    },
    /// 按项目 fx.json 生成换色后的特效贴图(全局生效, 写到 assets/res_mods/<游戏路径>) / apply theme to FX textures
    Apply {
        project: PathBuf,
        /// 只做这些类(可多次); 省略 = fx.json 里 strength>0 的全部类
        #[arg(long)]
        kind: Vec<String>,
        /// 只列计划
        #[arg(long)]
        dry_run: bool,
        /// 连数据贴图(法线/扰动/运动矢量…)也改(不建议)
        #[arg(long)]
        include_data: bool,
    },
}

#[derive(Args, Debug)]
pub struct ShipFxArgs {
    /// 船: GameParams 名或代号(PFSB510_Bourgogne / FSB510 …)
    pub ship: String,
    /// GameParams 船条目 JSON(无客户端时)
    #[arg(long)]
    pub gp: Option<PathBuf>,
}

pub fn cmd(cli: &Cli, c: &FxCmd) -> Result<i32> {
    match c {
        FxCmd::Find { kind } => cmd_find(cli, kind.as_deref()),
        FxCmd::ShipEffects(a) => cmd_ship(cli, a),
        FxCmd::Init { project, from, force } => cmd_init(cli, project, from.as_deref(), *force),
        FxCmd::Apply { project, kind, dry_run, include_data } => cmd_apply(cli, project, kind, *dry_run, *include_data),
    }
}

fn kind_row(k: &str) -> Result<&'static (&'static str, &'static str, &'static str, &'static str, &'static str)> {
    KINDS.iter().find(|r| r.0 == k).ok_or_else(|| anyhow!("未知特效类 {k:?}: {}", KINDS.iter().map(|r| r.0).collect::<Vec<_>>().join(" | ")))
}

fn cmd_find(cli: &Cli, kind: Option<&str>) -> Result<i32> {
    let ctx = game_ctx(cli, true)?.unwrap();
    let rows: Vec<_> = match kind {
        Some(k) => vec![kind_row(k)?],
        None => KINDS.iter().collect(),
    };
    let data = Regex::new(DATA_RE)?;
    let mut out = Vec::new();
    for r in rows {
        let re = Regex::new(&format!("(?i){}", r.2))?;
        let files: Vec<Value> = ctx
            .index
            .entries
            .iter()
            .filter(|e| re.is_match(&e.path))
            .map(|e| json!({"path": e.path, "pkg": e.pkg, "size": e.info.unpacked_size, "data_texture": data.is_match(&e.path)}))
            .collect();
        if !cli.json {
            println!("[{}] {} — {} · {} · {} 个", r.0, r.1, r.3, r.4, files.len());
            for f in &files {
                println!("    {:60} {:>9}  {}{}", f["path"].as_str().unwrap_or(""), f["size"], f["pkg"].as_str().unwrap_or(""), if f["data_texture"].as_bool() == Some(true) { "  (数据贴图, apply 默认跳过)" } else { "" });
            }
            if files.is_empty() {
                println!("    (本客户端索引里没有; 粒子贴图在 particles_*.pkg)");
            }
        }
        out.push(json!({"kind": r.0, "what": r.1, "status": r.3, "scope": r.4, "files": files}));
    }
    if cli.json {
        crate::emit(cli, &json!({"ok": true, "kinds": out}));
    }
    Ok(0)
}

/// 粒子路径 → 特效类
pub fn classify(path_key: &str, particle: &str) -> &'static str {
    let k = path_key.to_lowercase();
    let p = particle.to_lowercase();
    if k.contains("waketrace") || p.contains("waketrace") {
        "wake"
    } else if p.contains("runsmoke") || k.ends_with("/smoke") || k.contains("effects/smoke/") {
        "funnel"
    } else if k.contains("shoteffect") || p.ends_with("_shot.xml") {
        "muzzle"
    } else if k.contains("purging") || p.contains("purging") {
        "shot-smoke"
    } else if k.contains("fire") || p.contains("fire_") {
        "fire"
    } else {
        "other"
    }
}

fn load_ship(cli: &Cli, a: &ShipFxArgs) -> Result<(String, Value)> {
    if let Some(gp) = &a.gp {
        let v: Value = serde_json::from_slice(&std::fs::read(gp)?)?;
        return crate::m3cmd::resolve_ship_json(&v, &a.ship);
    }
    let ctx = game_ctx(cli, true)?.unwrap();
    let e = ctx.index.get("content/GameParams_py3.data").ok_or_else(|| anyhow!("索引中没有 content/GameParams_py3.data(无客户端时用 --gp)"))?;
    let raw = ctx.reader.read(e).map_err(|x| anyhow!("{x}"))?;
    let q = if a.ship.starts_with('P') { a.ship.clone() } else { format!("P{}", a.ship) };
    for k in [a.ship.clone(), q.clone(), q.split('_').next().unwrap_or(&q).to_string()] {
        if let Ok(r) = wowsmod_formats::gameparams::ship_json(&raw, &k) {
            return Ok(r);
        }
    }
    bail!("GameParams 中没有船 {}", a.ship)
}

/// 遍历船条目, 收集 (JSON 路径, 挂点, 粒子)
pub fn ship_bindings(ship: &Value) -> Vec<(String, Option<String>, String)> {
    fn walk(v: &Value, path: &mut Vec<String>, out: &mut Vec<(String, Option<String>, String)>) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    path.push(k.clone());
                    walk(x, path, out);
                    path.pop();
                }
            }
            Value::Array(a) => {
                // effects/<kind>/<i> = [挂点, 粒子]
                if a.len() == 2 {
                    if let (Some(hp), Some(p)) = (a[0].as_str(), a[1].as_str()) {
                        if hp.starts_with("HP_") && p.starts_with("particles/") {
                            out.push((path.join("/"), Some(hp.to_string()), p.to_string()));
                            return;
                        }
                    }
                }
                for (i, x) in a.iter().enumerate() {
                    path.push(i.to_string());
                    walk(x, path, out);
                    path.pop();
                }
            }
            Value::String(s) if s.starts_with("particles/") || s.starts_with("trails/") => {
                // 炮塔模块: A_Artillery/HP_FGM_1/shotEffect = particles/guns/GK_Shot.xml
                let hp = path.iter().rev().find(|x| x.starts_with("HP_")).cloned();
                out.push((path.join("/"), hp, s.clone()));
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(ship, &mut vec![], &mut out);
    out
}

fn cmd_ship(cli: &Cli, a: &ShipFxArgs) -> Result<i32> {
    let (name, ship) = load_ship(cli, a)?;
    let b = ship_bindings(&ship);
    let mut groups: std::collections::BTreeMap<&str, Vec<Value>> = Default::default();
    for (p, hp, part) in &b {
        groups.entry(classify(p, part)).or_default().push(json!({"key": p, "hp": hp, "particle": part}));
    }
    let v = json!({"ok": true, "ship": name, "bindings": b.len(), "groups": groups,
        "note": "粒子 XML 编译在 content/assets.bin(不可改); 改的是它们共用的贴图(全局)。按船区分只能改挂点位置(.visual)或 GP 绑定(未验证)。"});
    if cli.json {
        crate::emit(cli, &v);
    } else {
        println!("{name}: {} 条特效绑定", b.len());
        for k in ["funnel", "muzzle", "shot-smoke", "wake"] {
            let Some(g) = groups.get(k) else { continue };
            let mut by: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> = Default::default();
            for x in g {
                by.entry(x["particle"].as_str().unwrap_or("").to_string()).or_default().insert(x["hp"].as_str().unwrap_or("-").to_string());
            }
            println!("[{k}] {} 条", g.len());
            for (part, hps) in &by {
                let h: Vec<&str> = hps.iter().map(|s| s.as_str()).collect();
                let hs = if h.len() > 4 { format!("{} … 共 {} 个挂点", h[..3].join(", "), h.len()) } else { h.join(", ") };
                println!("    {part}  ← {hs}");
            }
        }
        println!("其余: {}", groups.iter().filter(|(k, _)| !["funnel", "muzzle", "shot-smoke", "wake"].contains(k)).map(|(k, v)| format!("{k} {}", v.len())).collect::<Vec<_>>().join(", "));
    }
    Ok(0)
}

/// 主题模板: 原版(strength 0)
pub fn template() -> Value {
    json!({
        "name": "vanilla",
        "note": "preview 段 = 工作台渲染 + fx apply 换色的唯一来源。tint=#RRGGBB(sRGB), strength 0..1 = 保亮度换色强度, keep 0..1 = 高光保留(炮口焰白热核), alpha ≤1 = 贴图不透明度倍数(只减不加, 战斗可读性)。改动全局生效。",
        "preview": {
            "muzzle": {"tint": "#FFFFFF", "strength": 0, "keep": 0.6, "alpha": 1},
            "shotSmoke": {"tint": "#FFFFFF", "strength": 0, "alpha": 1},
            "funnel": {"tint": "#FFFFFF", "strength": 0, "alpha": 1},
            "wake": {"tint": "#FFFFFF", "strength": 0, "alpha": 1}
        }
    })
}

/// 预览服务 /api/v1/project 的 fx 字段
pub fn project_fx_view(root: &Path) -> Value {
    let p = root.join(FX_FILE);
    match std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
        Some(v) => json!({"file": FX_FILE, "name": v["name"], "preview": v["preview"]}),
        None => Value::Null,
    }
}

fn cmd_init(cli: &Cli, project: &Path, from: Option<&Path>, force: bool) -> Result<i32> {
    let p = Project::open(project)?;
    let dst = p.root.join(FX_FILE);
    if dst.exists() && !force {
        bail!("{} 已存在(--force 覆盖)", dst.display());
    }
    let v: Value = match from {
        Some(f) => serde_json::from_slice(&std::fs::read(f)?)?,
        None => template(),
    };
    if v["preview"].is_null() {
        bail!("主题 JSON 缺 preview 段");
    }
    std::fs::write(&dst, serde_json::to_string_pretty(&v)? + "\n")?;
    crate::emit(cli, &json!({"ok": true, "file": dst.display().to_string(), "next": [format!("wowsmod render {} --look game-day --query \"env=water&speed=full&fire=main\"", project.display()), format!("wowsmod fx apply {}", project.display())]}));
    Ok(0)
}

#[derive(Debug, Clone, Copy)]
pub struct Tint {
    pub c: [f32; 3], // 线性
    pub strength: f32,
    pub keep: f32,
    pub alpha: f32,
}

fn srgb2lin(x: f32) -> f32 {
    if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
}
fn lin2srgb(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.0031308 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 }
}

pub fn parse_tint(v: &Value) -> Result<Option<Tint>> {
    let s = v["strength"].as_f64().unwrap_or(0.0) as f32;
    let a = v["alpha"].as_f64().unwrap_or(1.0) as f32;
    if s <= 0.0 && a >= 1.0 {
        return Ok(None);
    }
    let h = v["tint"].as_str().unwrap_or("#FFFFFF").trim_start_matches('#');
    if h.len() != 6 {
        bail!("tint 需要 #RRGGBB: {v}");
    }
    let n = u32::from_str_radix(h, 16)?;
    let c = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map(|x| srgb2lin(x as f32 / 255.0));
    Ok(Some(Tint { c, strength: s.min(1.0), keep: v["keep"].as_f64().unwrap_or(0.0) as f32, alpha: a.clamp(0.0, 1.0) }))
}

/// 保亮度换色(与工作台 fxTint 同一公式): c ⊙ mix(1, tint/L(tint), k·(1-keep·hi)), hi = 像素高光度; alpha × alpha
pub fn apply_tint(img: &mut Image, t: &Tint) {
    let lt = (0.2126 * t.c[0] + 0.7152 * t.c[1] + 0.0722 * t.c[2]).max(1e-4);
    let ch = t.c.map(|x| x / lt);
    for px in img.px.chunks_exact_mut(4) {
        let c = [px[0], px[1], px[2]].map(|x| srgb2lin(x as f32 / 255.0));
        let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let hi = ((l - 0.5) / 0.5).clamp(0.0, 1.0);
        let k = t.strength * (1.0 - t.keep * hi * hi * (3.0 - 2.0 * hi));
        let mut o = [0f32; 3];
        for i in 0..3 {
            o[i] = c[i] * (1.0 + (ch[i] - 1.0) * k);
        }
        let m = o.iter().cloned().fold(0.0f32, f32::max);
        if m > 1.0 {
            o = o.map(|x| x / m); // 白色贴图乘色后超 1: 等比压回(保色相)
        }
        for i in 0..3 {
            px[i] = (lin2srgb(o[i]) * 255.0 + 0.5) as u8;
        }
        px[3] = (px[3] as f32 * t.alpha + 0.5) as u8;
    }
}

/// 像素判断数据贴图: R≈0 且 B≈0.5(打包的遮罩/偏移), 或明显非颜色
fn looks_like_data(img: &Image) -> bool {
    let n = (img.px.len() / 4).max(1) as f64;
    let (mut r, mut b) = (0f64, 0f64);
    for p in img.px.chunks_exact(4) {
        r += p[0] as f64;
        b += p[2] as f64;
    }
    r / n < 3.0 && (b / n - 127.5).abs() < 12.0
}

const THEME_KEY: &[(&str, &str)] = &[("funnel", "funnel"), ("muzzle", "muzzle"), ("shot-smoke", "shotSmoke"), ("wake", "wake")];

fn cmd_apply(cli: &Cli, project: &Path, kinds: &[String], dry: bool, include_data: bool) -> Result<i32> {
    let p = Project::open(project)?;
    let fx: Value = serde_json::from_slice(&std::fs::read(p.root.join(FX_FILE)).map_err(|_| anyhow!("项目里没有 {FX_FILE}(先 wowsmod fx init)"))?)?;
    let ctx = game_ctx(cli, true)?.unwrap();
    let data = Regex::new(DATA_RE)?;
    let mut plan = Vec::new();
    for (k, tk) in THEME_KEY {
        if !kinds.is_empty() && !kinds.iter().any(|x| x == k) {
            continue;
        }
        let Some(t) = parse_tint(&fx["preview"][*tk])? else { continue };
        let r = kind_row(k)?;
        let re = Regex::new(&format!("(?i){}", r.2))?;
        let files: Vec<String> = ctx.index.entries.iter().filter(|e| re.is_match(&e.path)).map(|e| e.path.clone()).collect();
        plan.push((k.to_string(), t, files, r.4));
    }
    let mut rows = Vec::new();
    let mut ok = true;
    for (k, t, files, scope) in &plan {
        if files.is_empty() {
            rows.push(json!({"kind": k, "status": "missing", "note": "本客户端索引里没有这类贴图(粒子贴图在 particles_*.pkg; 换完整客户端再跑)"}));
            continue;
        }
        for f in files {
            if data.is_match(f) && !include_data {
                rows.push(json!({"kind": k, "path": f, "status": "skipped", "reason": "数据贴图(名称)"}));
                continue;
            }
            if dry {
                rows.push(json!({"kind": k, "path": f, "status": "planned", "scope": scope}));
                continue;
            }
            let res = (|| -> Result<Value> {
                let tex = texcmd::load_tex(f, Some(&ctx), None, true)?;
                let mut img = texture::decode_level(&tex.dds, 0).map_err(|e| anyhow!("解码: {e}"))?;
                if looks_like_data(&img) && !include_data {
                    return Ok(json!({"status": "skipped", "reason": "数据贴图(像素: R≈0, B≈0.5)"}));
                }
                apply_tint(&mut img, t);
                let dst = texcmd::resolve_dst(None, Some(&p), &tex)?;
                let (outs, _) = texcmd::write_tex(&img, &tex, &dst, Quality::Normal, None, None)?;
                Ok(json!({"status": "written", "outputs": texcmd::outputs_json(Some(&p), &outs)}))
            })();
            match res {
                Ok(mut v) => {
                    v["kind"] = json!(k);
                    v["path"] = json!(f);
                    rows.push(v)
                }
                Err(e) => {
                    ok = false;
                    rows.push(json!({"kind": k, "path": f, "status": "error", "error": e.to_string()}))
                }
            }
        }
    }
    let written: Vec<&Value> = rows.iter().filter(|r| r["status"] == "written").collect();
    if !dry && !written.is_empty() {
        p.record_edit("fx_apply", json!({"theme": fx["name"], "preview": fx["preview"], "files": written}))?;
    }
    let v = json!({"ok": ok, "dry_run": dry, "theme": fx["name"], "rows": rows,
        "warning": "特效贴图全局生效(所有船), 装上后用训练房实测; 卸载 = wowsmod uninstall 或删除 res_mods/particles 与 res_mods/trails 下对应文件"});
    if cli.json {
        crate::emit(cli, &v);
    } else {
        for r in &rows {
            println!("{:11} {:8} {} {}", r["kind"].as_str().unwrap_or(""), r["status"].as_str().unwrap_or(""), r["path"].as_str().unwrap_or(""), r["reason"].as_str().or(r["note"].as_str()).or(r["error"].as_str()).unwrap_or(""));
        }
        println!("注意: {}", v["warning"].as_str().unwrap());
    }
    Ok(if ok { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classify_kinds() {
        assert_eq!(classify("A_Hull/effects/waketracefront/0", "particles/vehicles/WakeTraceForward_front.xml"), "wake");
        assert_eq!(classify("A_Hull/effects/smoke/0", "particles/vehicles/RunSmoke_big.xml"), "funnel");
        assert_eq!(classify("A_Hull/effects/idlePort/8", "particles/vehicles/idlePort_RunSmoke_big.xml"), "funnel");
        assert_eq!(classify("A_Artillery/HP_FGM_1/shotEffect", "particles/guns/GK_Shot.xml"), "muzzle");
        assert_eq!(classify("A_Artillery/HP_FGM_1/purgingEffect", "particles/guns/GK_Purging.xml"), "shot-smoke");
    }
    #[test]
    fn tint_white_and_keep() {
        let t = parse_tint(&json!({"tint": "#8F8BC8", "strength": 1.0, "alpha": 0.5})).unwrap().unwrap();
        let mut im = Image::new(1, 1);
        im.px = vec![255, 255, 255, 200];
        apply_tint(&mut im, &t);
        assert!(im.px[2] > im.px[0] && im.px[2] == 255, "{:?}", im.px);
        assert_eq!(im.px[3], 100);
        // strength 0 + alpha 1 = 不改
        assert!(parse_tint(&json!({"tint": "#8F8BC8", "strength": 0})).unwrap().is_none());
    }
    #[test]
    fn bindings_pairs() {
        let s = json!({"A_Hull": {"effects": {"smoke": [["HP_FX_Smoke_1", "particles/vehicles/RunSmoke_big.xml"]]}}, "A_Artillery": {"HP_FGM_1": {"shotEffect": "particles/guns/GK_Shot.xml"}}});
        let b = ship_bindings(&s);
        assert!(b.iter().any(|x| x.1.as_deref() == Some("HP_FX_Smoke_1")));
        assert!(b.iter().any(|x| x.1.as_deref() == Some("HP_FGM_1") && x.2.ends_with("GK_Shot.xml")));
    }
}
