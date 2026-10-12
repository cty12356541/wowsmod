//! `wowsmod design-preview <spec.toml>`: 方向粗样一键出图。
//! 每个方向 = 基础项目的一份副本 + 一串 wowsmod 子命令(换色/烘图/发光/导入模型…) + 同一组机位出图,
//! 最后拼一张总表(行=方向, 列=机位)与 index.html。船名/角色/贴图名都只在 spec 里, 命令本身不写死。

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Cli;

#[derive(clap::Args, Clone)]
pub struct PreviewArgs {
    /// 方向 spec(TOML)。不存在且加 --init 时写一份示例
    pub spec: PathBuf,
    /// 写示例 spec 后退出
    #[arg(long)]
    pub init: bool,
    /// 只做这些方向(逗号分隔 id)
    #[arg(long)]
    pub only: Option<String>,
    /// 不重建副本、不跑步骤, 只出图(调机位用)
    #[arg(long)]
    pub render_only: bool,
    /// 只建副本+跑步骤, 不出图
    #[arg(long)]
    pub no_render: bool,
    /// 总表每格宽度(像素)
    #[arg(long, default_value_t = 480)]
    pub tile: u32,
}

#[derive(Deserialize, Default)]
struct Spec {
    /// 基础项目目录(相对 spec 所在目录)
    base: String,
    /// 输出目录(相对 spec 所在目录): <out>/<方向id>/ 是项目副本, <out>/shots/ 是出图
    #[serde(default = "default_out")]
    out: String,
    /// 自由变量, 步骤里写 {名字} 引用; 内置 {proj}(该方向副本的绝对路径)、{spec_dir}、{out}
    #[serde(default)]
    vars: BTreeMap<String, String>,
    #[serde(default, rename = "direction")]
    directions: Vec<Direction>,
    #[serde(default, rename = "view")]
    views: Vec<View>,
}
fn default_out() -> String {
    "preview".into()
}

#[derive(Deserialize, Clone)]
struct Direction {
    id: String,
    #[serde(default)]
    label: String,
    /// 每步一条 wowsmod 命令行(不含 wowsmod 本身), 引号可包含空格
    #[serde(default)]
    steps: Vec<String>,
}

#[derive(Deserialize, Clone)]
struct View {
    id: String,
    /// render --query 的查询串(view/az/el/dist/focus/env/light/w/h)
    query: String,
}

fn default_views() -> Vec<View> {
    let v = |id: &str, q: &str| View { id: id.into(), query: q.into() };
    vec![
        v("day_fq", "view=custom&az=140&el=18&dist=22&w=1280&h=720&env=water&light=day"),
        v("day_side", "view=custom&az=90&el=8&dist=20&w=1280&h=720&env=water&light=day"),
        v("day_stern", "view=custom&az=200&el=12&dist=10&focus=hull%2FStern&w=1280&h=720&env=studio&light=day"),
        v("night_fq", "view=custom&az=140&el=18&dist=22&w=1280&h=720&env=water&light=night"),
        v("night_stern", "view=custom&az=200&el=12&dist=10&focus=hull%2FStern&w=1280&h=720&env=dark&light=night"),
    ]
}

const EXAMPLE: &str = r##"# wowsmod design-preview 方向 spec(示例; 全部名字按你的船/角色改)
# 用法: wowsmod design-preview <本文件>   → <out>/<方向>/ 项目副本 + <out>/shots/*.png + <out>/sheet.png + <out>/index.html
base = "base"          # 已 import-sdk 的基础项目(相对本文件)
out = "preview"

[vars]                 # 步骤里用 {名字} 引用; 内置 {proj} {spec_dir} {out}
hull_a = "<船>_Hull_a.dds"
hull_mg = "<船>_Hull_mg.dds"
dh_a = "<船>_DeckHouse_a.dds"
light = "#F4F4DA"      # 先用 wowsmod port-albedo <想看到的色> 换算
accent = "#71C1C9"

[[direction]]
id = "a"
label = "克制"
steps = [
  "tex recolor {hull_a} --project {proj} --to-color {light} --exclude-hue 0:65",
  "tex channel {hull_mg} --project {proj} --channel b --set 0",
]

[[direction]]
id = "b"
label = "主题强化"
steps = [
  "tex recolor {hull_a} --project {proj} --to-color {light} --exclude-hue 0:65",
  "tex channel {hull_mg} --project {proj} --channel b --set 0",
  # "bake-ref {proj} --ref design_b_side.png --material Hull --like {hull_a} --out {out}/uv_b.png --view side --mirror-x",
  # "tex composite {hull_a} --project {proj} --over {out}/uv_b.png",
  # "model import --project {proj} --mesh figure.glb --like <挂接渲染集> --section <分段> --at <x,y,z> --height <单位>",
]

[[direction]]
id = "c"
label = "夸张"
steps = []

# 不写 [[view]] 就用默认 5 机位(昼斜/昼侧/昼艉/夜斜/夜艉)
# [[view]]
# id = "day_fq"
# query = "view=custom&az=140&el=18&dist=22&w=1280&h=720&env=water&light=day"
"##;

/// 简单命令行切分: 空白分隔, 双引号包住的部分保留空格
fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut q = false;
    let mut has = false;
    for ch in s.chars() {
        match ch {
            '"' => {
                q = !q;
                has = true;
            }
            c if c.is_whitespace() && !q => {
                if has {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        out.push(cur);
    }
    out
}

fn subst(s: &str, vars: &BTreeMap<String, String>) -> Result<String> {
    let re = regex::Regex::new(r"\{([A-Za-z_][A-Za-z0-9_]*)\}").unwrap();
    let mut missing = Vec::new();
    let r = re.replace_all(s, |c: &regex::Captures| match vars.get(&c[1]) {
        Some(v) => v.clone(),
        None => {
            missing.push(c[1].to_string());
            c[0].to_string()
        }
    });
    if !missing.is_empty() {
        bail!("步骤里引用了未定义变量 {:?}: {s}", missing);
    }
    Ok(r.into_owned())
}

fn copy_dir(src: &Path, dst: &Path) -> Result<u64> {
    let mut n = 0;
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src).with_context(|| format!("读 {}", src.display()))? {
        let e = e?;
        let name = e.file_name();
        let ns = name.to_string_lossy();
        if matches!(ns.as_ref(), ".install" | "renders" | "dist" | ".wowsmod") {
            continue;
        }
        let p = e.path();
        let d = dst.join(&name);
        if e.file_type()?.is_dir() {
            n += copy_dir(&p, &d)?;
        } else {
            std::fs::copy(&p, &d)?;
            n += 1;
        }
    }
    Ok(n)
}

pub(crate) fn run_self(cli: &Cli, args: &[String]) -> Value {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("wowsmod"));
    let mut cmd = Command::new(exe);
    if let Some(g) = &cli.game {
        cmd.arg("--game").arg(g);
    }
    cmd.arg("--json").args(args).env("WOWSMOD_BOOK_IGNORE", "1");
    match cmd.output() {
        Ok(o) => {
            let txt = String::from_utf8_lossy(&o.stdout);
            let mut v = serde_json::from_str::<Value>(txt.trim()).unwrap_or_else(|_| json!({"ok": o.status.success(), "stdout": txt.chars().take(400).collect::<String>()}));
            if !o.status.success() {
                v["ok"] = json!(false);
                let err = String::from_utf8_lossy(&o.stderr);
                if !err.trim().is_empty() {
                    v["stderr"] = json!(err.chars().take(600).collect::<String>());
                }
            }
            v
        }
        Err(e) => json!({"ok": false, "error": [format!("启动 wowsmod 失败: {e}")]}),
    }
}

fn scale_into(dst: &mut wowsmod_formats::texture::Image, src: &wowsmod_formats::texture::Image, ox: u32, oy: u32, w: u32, h: u32) {
    for y in 0..h {
        let sy = (y as u64 * src.height as u64 / h as u64) as u32;
        for x in 0..w {
            let sx = (x as u64 * src.width as u64 / w as u64) as u32;
            let i = src.idx(sx.min(src.width - 1), sy.min(src.height - 1));
            let j = dst.idx(ox + x, oy + y);
            dst.px[j..j + 3].copy_from_slice(&src.px[i..i + 3]);
            dst.px[j + 3] = 255;
        }
    }
}

pub fn cmd(cli: &Cli, a: &PreviewArgs) -> Result<i32> {
    if a.init {
        if a.spec.exists() {
            bail!("{} 已存在, 不覆盖", a.spec.display());
        }
        if let Some(d) = a.spec.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&a.spec, EXAMPLE)?;
        let v = json!({"ok": true, "spec": a.spec.display().to_string(), "next": ["按船/角色改 [vars] 与各方向 steps", format!("wowsmod design-preview {}", a.spec.display())]});
        crate::emit(cli, &v);
        if !cli.json {
            println!("已写示例 spec: {}", a.spec.display());
        }
        return Ok(0);
    }
    let txt = std::fs::read_to_string(&a.spec).with_context(|| format!("读 spec {}(没有就先 --init 生成示例)", a.spec.display()))?;
    let spec: Spec = toml::from_str(&txt).map_err(|e| anyhow!("解析 spec: {e}"))?;
    let spec_dir = std::fs::canonicalize(a.spec.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new(".")))?;
    // Windows canonicalize 会带 \\?\ 前缀: 子命令和外部工具常不认, 去掉
    let spec_dir = {
        let t = spec_dir.to_string_lossy().to_string();
        PathBuf::from(t.strip_prefix(r"\\?\").map(|x| x.to_string()).unwrap_or(t))
    };
    let base = spec_dir.join(&spec.base);
    if !base.join("mod.toml").exists() {
        bail!("base {} 不是 mod 项目(缺 mod.toml)", base.display());
    }
    let out = spec_dir.join(&spec.out);
    let shots = out.join("shots");
    std::fs::create_dir_all(&shots)?;
    let only: Option<Vec<String>> = a.only.as_ref().map(|s| s.split(',').map(|x| x.trim().to_string()).collect());
    let dirs: Vec<Direction> = spec.directions.iter().filter(|d| only.as_ref().map(|o| o.contains(&d.id)).unwrap_or(true)).cloned().collect();
    if dirs.is_empty() {
        bail!("spec 里没有要做的方向([[direction]])");
    }
    for d in &dirs {
        if d.id.is_empty() || !d.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            bail!("方向 id 只能是 ASCII 字母/数字/_/-: {:?}", d.id);
        }
    }
    let views = if spec.views.is_empty() { default_views() } else { spec.views.clone() };
    let mut report = Vec::new();
    let mut failed = 0;
    for d in &dirs {
        let proj = out.join(&d.id);
        let mut vars = spec.vars.clone();
        vars.insert("proj".into(), proj.display().to_string());
        vars.insert("spec_dir".into(), spec_dir.display().to_string());
        vars.insert("out".into(), out.display().to_string());
        let mut steps = Vec::new();
        let mut ok = true;
        if !a.render_only {
            if proj.exists() {
                let _ = run_self(cli, &["render".into(), "stop".into(), proj.display().to_string()]);
                std::fs::remove_dir_all(&proj).with_context(|| format!("清理旧副本 {}(是否有程序占用?)", proj.display()))?;
            }
            let n = copy_dir(&base, &proj)?;
            steps.push(json!({"step": "copy_base", "files": n, "ok": true}));
            for s in &d.steps {
                let line = subst(s, &vars)?;
                let args = split_args(&line);
                let r = run_self(cli, &args);
                let sok = r["ok"].as_bool().unwrap_or(true);
                steps.push(json!({"step": line, "ok": sok, "error": r.get("error"), "warnings": r.get("warnings"), "units": r.get("units"), "license": r.get("license")}));
                if !sok {
                    ok = false;
                    break;
                }
            }
        }
        let mut imgs = Vec::new();
        if ok && !a.no_render {
            let _ = run_self(cli, &["render".into(), proj.display().to_string(), "--warm".into()]);
            for v in &views {
                let f = shots.join(format!("{}_{}.png", d.id, v.id));
                let r = run_self(cli, &["render".into(), proj.display().to_string(), "--query".into(), v.query.clone(), "-o".into(), f.display().to_string()]);
                imgs.push(json!({"view": v.id, "file": f.display().to_string(), "ok": r["ok"].as_bool().unwrap_or(f.exists()) && f.exists()}));
            }
            let _ = run_self(cli, &["render".into(), "stop".into(), proj.display().to_string()]);
        }
        if !ok {
            failed += 1;
        }
        report.push(json!({"id": d.id, "label": d.label, "project": proj.display().to_string(), "ok": ok, "steps": steps, "shots": imgs}));
    }
    // 总表 PNG(行=方向, 列=机位)与 index.html
    let mut sheet_path = Value::Null;
    if !a.no_render {
        let tw = a.tile.max(64);
        let th = tw * 9 / 16;
        let gap = 6;
        let (cols, rows) = (views.len() as u32, dirs.len() as u32);
        let mut sheet = wowsmod_formats::texture::Image::new(cols * (tw + gap) + gap, rows * (th + gap) + gap);
        for p in sheet.px.chunks_exact_mut(4) {
            p.copy_from_slice(&[24, 24, 28, 255]);
        }
        for (r, d) in dirs.iter().enumerate() {
            for (c, v) in views.iter().enumerate() {
                let f = shots.join(format!("{}_{}.png", d.id, v.id));
                if let Ok(b) = std::fs::read(&f) {
                    if let Ok(img) = wowsmod_formats::texture::read_png(&b) {
                        scale_into(&mut sheet, &img, gap + c as u32 * (tw + gap), gap + r as u32 * (th + gap), tw, th);
                    }
                }
            }
        }
        let sp = out.join("sheet.png");
        std::fs::write(&sp, wowsmod_formats::texture::write_png(&sheet, false).map_err(|e| anyhow!("{e}"))?)?;
        sheet_path = json!(sp.display().to_string());
        let mut html = String::from("<!doctype html><meta charset=utf-8><title>design-preview</title><style>body{background:#181820;color:#eee;font:14px sans-serif}td{padding:4px;vertical-align:top}img{width:360px}</style><table><tr><th></th>");
        for v in &views {
            html += &format!("<th>{}</th>", v.id);
        }
        html += "</tr>";
        for d in &dirs {
            html += &format!("<tr><th>{} {}</th>", d.id, d.label);
            for v in &views {
                html += &format!("<td><img src=\"shots/{}_{}.png\"></td>", d.id, v.id);
            }
            html += "</tr>";
        }
        html += "</table>";
        std::fs::write(out.join("index.html"), html)?;
    }
    let v = json!({"ok": failed == 0, "out": out.display().to_string(), "sheet": sheet_path, "rows": dirs.iter().map(|d| format!("{} {}", d.id, d.label)).collect::<Vec<_>>(),
        "cols": views.iter().map(|v| v.id.clone()).collect::<Vec<_>>(), "directions": report,
        "next": ["按 workflow.layers.map.preview 的审美自查逐张看 sheet.png; 远景(fq)要一眼分出各方向", "把 sheet/各图放进设计书 §4, 请用户选方向"]});
    std::fs::write(out.join("preview.json"), serde_json::to_string_pretty(&v)?)?;
    crate::emit(cli, &v);
    if !cli.json {
        println!("方向预览: {} 个方向 × {} 机位 → {}", dirs.len(), views.len(), out.display());
        if failed > 0 {
            println!("有 {failed} 个方向步骤失败, 见 preview.json");
        }
    }
    Ok(if failed == 0 { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_and_subst() {
        assert_eq!(split_args(r#"tex recolor "a b.dds" --x 1"#), vec!["tex", "recolor", "a b.dds", "--x", "1"]);
        let mut v = BTreeMap::new();
        v.insert("proj".to_string(), "P".to_string());
        assert_eq!(subst("--project {proj}", &v).unwrap(), "--project P");
        assert!(subst("{nope}", &v).is_err());
        let s: Spec = toml::from_str(EXAMPLE).unwrap();
        assert_eq!(s.directions.len(), 3);
    }
}
