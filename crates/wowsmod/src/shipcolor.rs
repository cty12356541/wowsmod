//! 整船同色: 一条命令把船体 / 上层建筑 / 主副炮与防空 / 指挥仪 / 雷达等部件组按组换色(内部逐张调用 `tex recolor` + 清 `_mg` B)。
//! 不写死船名: 按项目里 `*_a.dds` 所在目录与文件名分组。
use anyhow::{bail, Result};
use clap::Args;
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::modproj::Project;
use crate::Cli;

#[derive(Args, Debug, Clone)]
pub struct ShipColorArgs {
    /// mod 项目目录(已 import-sdk)
    pub project: PathBuf,
    /// 船体 *_Hull_a 的颜色 #RRGGBB(先用 port-albedo 换算)
    #[arg(long)]
    pub hull: Option<String>,
    /// 上层建筑 *_DeckHouse_a 的颜色
    #[arg(long)]
    pub deckhouse: Option<String>,
    /// 炮塔与火炮 gun/*(主炮/副炮/防空)
    #[arg(long)]
    pub guns: Option<String>,
    /// 指挥仪/测距仪 director/*
    #[arg(long)]
    pub directors: Option<String>,
    /// 雷达 radar/*
    #[arg(long)]
    pub radars: Option<String>,
    /// 以上部件组(guns/directors/radars)统一用一个颜色, 单独给的优先
    #[arg(long)]
    pub parts: Option<String>,
    /// 只改文件名匹配这个正则的部件贴图(如 "FGM|FGS" 只改主副炮)
    #[arg(long)]
    pub parts_filter: Option<String>,
    /// 传给 recolor: 保留的色相范围(木甲板/锈/船底红)
    #[arg(long, default_value = "0:65")]
    pub exclude_hue: String,
    /// 传给 recolor: 原色斑保留系数
    #[arg(long, default_value_t = 0.2)]
    pub chroma_detail: f32,
    /// 上层建筑的 _mg B 不清(留给发光)
    #[arg(long)]
    pub keep_deckhouse_mg: bool,
    /// 只列出会改哪些贴图
    #[arg(long)]
    pub dry_run: bool,
}

fn group_of(rel: &str) -> Option<&'static str> {
    let l = rel.to_lowercase().replace('\\', "/");
    let file = l.rsplit('/').next().unwrap_or("");
    if !file.ends_with("_a.dds") {
        return None;
    }
    // 通用网格/玻璃/裂纹等共享贴图不动
    if file.starts_with('c') && file.chars().nth(1).map(|c| c.is_ascii_digit()).unwrap_or(false) || file.contains("alpha") || file.contains("glass") {
        return None;
    }
    if l.contains("/ship/") {
        if file.ends_with("_hull_a.dds") {
            return Some("hull");
        }
        if file.ends_with("_deckhouse_a.dds") {
            return Some("deckhouse");
        }
        return None;
    }
    for g in ["gun", "director", "radar"] {
        if l.contains(&format!("/{g}/")) {
            return Some(match g {
                "gun" => "guns",
                "director" => "directors",
                _ => "radars",
            });
        }
    }
    None
}

pub fn cmd(cli: &Cli, a: &ShipColorArgs) -> Result<i32> {
    let p = Project::open(&a.project)?;
    let filt = a.parts_filter.as_deref().map(|r| regex::Regex::new(&format!("(?i){r}"))).transpose()?;
    let color_for = |g: &str| -> Option<String> {
        match g {
            "hull" => a.hull.clone(),
            "deckhouse" => a.deckhouse.clone(),
            "guns" => a.guns.clone().or(a.parts.clone()),
            "directors" => a.directors.clone().or(a.parts.clone()),
            "radars" => a.radars.clone().or(a.parts.clone()),
            _ => None,
        }
    };
    if ["hull", "deckhouse", "guns", "directors", "radars"].iter().all(|g| color_for(g).is_none()) {
        bail!("至少给一个颜色: --hull / --deckhouse / --guns / --directors / --radars / --parts");
    }
    let mut plan: Vec<(String, String, String)> = Vec::new(); // (group, file name, color)
    let mut seen = std::collections::BTreeSet::new();
    for f in p.files()? {
        let rel = p.rel(&f);
        let Some(g) = group_of(&rel) else { continue };
        let Some(c) = color_for(g) else { continue };
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        if matches!(g, "guns" | "directors" | "radars") {
            if let Some(r) = &filt {
                if !r.is_match(&name) {
                    continue;
                }
            }
        }
        if seen.insert(name.to_lowercase()) {
            plan.push((g.to_string(), name, c));
        }
    }
    if plan.is_empty() {
        bail!("项目里没找到可分组的 *_a.dds(需要已 import-sdk 的 SDK 项目)");
    }
    let mut steps: Vec<Value> = Vec::new();
    let mut ok = true;
    for (g, name, c) in &plan {
        let mg = name.replace("_a.dds", "_mg.dds");
        let has_mg = !p.find_by_name(&mg)?.is_empty();
        let clear_mg = has_mg && !(g == "deckhouse" && a.keep_deckhouse_mg);
        if a.dry_run {
            steps.push(json!({"group": g, "texture": name, "color": c, "clear_mg_b": clear_mg}));
            continue;
        }
        let proj = a.project.display().to_string();
        let r = crate::design_preview::run_self(cli, &["tex".into(), "recolor".into(), name.clone(), "--project".into(), proj.clone(), "--to-color".into(), c.clone(), "--exclude-hue".into(), a.exclude_hue.clone(), "--soft".into(), "0.04".into(), "--chroma-detail".into(), a.chroma_detail.to_string()]);
        let rok = r["ok"].as_bool().unwrap_or(false);
        let mut s = json!({"group": g, "texture": name, "color": c, "ok": rok});
        if !rok {
            s["error"] = r.get("error").cloned().unwrap_or(r.get("stderr").cloned().unwrap_or(Value::Null));
            ok = false;
        }
        if rok && clear_mg {
            let r2 = crate::design_preview::run_self(cli, &["tex".into(), "channel".into(), mg.clone(), "--project".into(), proj, "--channel".into(), "b".into(), "--set".into(), "0".into()]);
            s["clear_mg_b"] = json!(r2["ok"].as_bool().unwrap_or(false));
        }
        steps.push(s);
    }
    let mut counts = std::collections::BTreeMap::new();
    for (g, _, _) in &plan {
        *counts.entry(g.clone()).or_insert(0) += 1;
    }
    let v = json!({"ok": ok, "dry_run": a.dry_run, "groups": counts, "textures": steps,
        "next": [format!("wowsmod render {} --compare", a.project.display()), "颜色先用 wowsmod port-albedo <想在港口看到的色> 换算".to_string()]});
    crate::emit(cli, &v);
    if !cli.json {
        for s in &steps {
            println!("{:10} {:45} {} {}", s["group"].as_str().unwrap_or(""), s["texture"].as_str().unwrap_or(""), s["color"].as_str().unwrap_or(""), if s["ok"].as_bool() == Some(false) { "失败" } else { "" });
        }
    }
    Ok(if ok { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::group_of;
    #[test]
    fn groups() {
        assert_eq!(group_of("assets/res_mods/PnFMods/M/S/ship/X_1945_Hull_a.dds"), Some("hull"));
        assert_eq!(group_of("assets/res_mods/PnFMods/M/S/ship/X_DeckHouse_a.dds"), Some("deckhouse"));
        assert_eq!(group_of("assets/res_mods/PnFMods/M/S/gun/FGM049_380mm_a.dds"), Some("guns"));
        assert_eq!(group_of("assets/res_mods/PnFMods/M/S/gun/C035_Gun_Barrel_a.dds"), None);
        assert_eq!(group_of("assets/res_mods/PnFMods/M/S/radar/C001_Net_alpha_a.dds"), None);
        assert_eq!(group_of("assets/res_mods/PnFMods/M/S/director/FD026_Tele_57_a.dds"), Some("directors"));
        assert_eq!(group_of("assets/res_mods/PnFMods/M/S/ship/X_Hull_mg.dds"), None);
    }
}
