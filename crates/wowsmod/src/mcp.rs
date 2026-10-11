//! M6: `wowsmod mcp` —— stdio MCP 服务器(JSON-RPC 2.0, 每行一条消息)。
//! 渐进式披露: 初始只有 5 个入口工具; wowsmod_expand 按组加入带参数说明的专用工具并发 tools/list_changed。
use std::collections::BTreeSet;
use std::io::{BufRead, Write};

use anyhow::Result;
use serde_json::{json, Value};

use crate::preview::warm;
use crate::{ai, guide, render, Cli};

const PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const MAX_TEXT: usize = 24_000;

/// 专用工具参数: (名字, 类型, 必填, 映射, 说明)
/// 映射: "" = 位置参数; "--x" = 选项; "--x!" = 布尔开关; "*" = 位置参数列表
type P = (&'static str, &'static str, bool, &'static str, &'static str);
struct Tool {
    name: &'static str,
    argv: &'static [&'static str],
    desc: &'static str,
    params: &'static [P],
}

const GROUPS: &[(&str, &str)] = &[
    ("texture", "换色/去迷彩/发光/港口色换算/贴图信息"),
    ("model", "几何: 列表/缩放/移动/旋转/删除/撤销"),
    ("project", "项目: 新建/导入 SDK/状态"),
    ("install", "安装/卸载/已装/冲突/迁移/打包"),
    ("assets", "游戏资源: 检索/解包/导出 GLB"),
];

const TOOLS: &[(&str, Tool)] = &[
    ("texture", Tool { name: "tex_recolor", argv: &["tex", "recolor"], desc: "贴图换色(保留细节)。船体先用 port_albedo 换算颜色。见 help recolor", params: &[
        ("tex", "string", true, "", "贴图文件名, 如 FSB025_Bourgogne_1945_Hull_a.dds"), ("project", "string", true, "--project", "项目目录"),
        ("to_color", "string", false, "--to-color", "目标色 #RRGGBB"), ("exclude_hue", "string", false, "--exclude-hue", "排除色相 lo:hi, 如 0:65"),
        ("soft", "number", false, "--soft", "软边 0.04"), ("hue_shift", "number", false, "--hue-shift", "色相旋转(度)"),
        ("glb", "string", false, "--glb", "按部件限定: export 的 GLB"), ("material", "string", false, "--material", "配合 glb 的材质正则"), ("mask", "string", false, "--mask", "遮罩 PNG")] }),
    ("texture", Tool { name: "tex_channel", argv: &["tex", "channel"], desc: "改单通道, 如 _mg 的 b 设 0 = 去迷彩遮罩", params: &[
        ("tex", "string", true, "", "贴图文件名"), ("project", "string", true, "--project", "项目目录"), ("channel", "string", true, "--channel", "r/g/b/a"),
        ("set", "number", false, "--set", "设为 0..255"), ("mul", "number", false, "--mul", "乘系数")] }),
    ("texture", Tool { name: "glow", argv: &["glow"], desc: "部件发光(_mg B + emissive 着色器 + 材质标识)。见 help glow", params: &[
        ("project", "string", true, "--project", "项目目录"), ("mg", "string", true, "--mg", "_mg 贴图名"), ("diffuse", "string", false, "--diffuse", "_a 贴图名"),
        ("color", "string", false, "--color", "#RRGGBB"), ("glb", "string", false, "--glb", "export 的 GLB"), ("material", "string", false, "--material", "材质正则"),
        ("mfm", "string", false, "--mfm", ".mfm 名"), ("power", "number", false, "--power", "emissivePower, 0.25 已很亮")] }),
    ("texture", Tool { name: "port_albedo", argv: &["port-albedo"], desc: "想在港口看到的颜色 → 贴图该用的 to_color", params: &[("screen", "string", true, "", "#RRGGBB")] }),
    ("texture", Tool { name: "tex_info", argv: &["tex", "info"], desc: "贴图格式/尺寸/分卷", params: &[("tex", "string", true, "", "贴图名或路径"), ("project", "string", false, "--project", "项目目录")] }),
    ("model", Tool { name: "model_list", argv: &["model", "list"], desc: "列出分段/渲染集/部件/挂点", params: &[("project", "string", true, "--project", "项目目录")] }),
    ("model", Tool { name: "model_scale", argv: &["model", "scale"], desc: "缩放(部件/挂点/渲染集)", params: &[
        ("factor", "string", true, "", "1.5 或 x,y,z"), ("project", "string", true, "--project", "项目目录"), ("part", "string", false, "--part", "部件名前缀"), ("hp", "string", false, "--hp", "挂点"), ("renderset", "string", false, "--renderset", "渲染集正则")] }),
    ("model", Tool { name: "model_move", argv: &["model", "move"], desc: "平移(游戏坐标 x 右舷 y 上 z 船艏)", params: &[
        ("by", "string", true, "--by", "x,y,z"), ("project", "string", true, "--project", "项目目录"), ("part", "string", false, "--part", "部件"), ("hp", "string", false, "--hp", "挂点"), ("renderset", "string", false, "--renderset", "渲染集正则")] }),
    ("model", Tool { name: "model_rotate", argv: &["model", "rotate"], desc: "旋转(度, 依次绕 x,y,z)", params: &[
        ("deg", "string", true, "--deg", "x,y,z"), ("project", "string", true, "--project", "项目目录"), ("part", "string", false, "--part", "部件"), ("hp", "string", false, "--hp", "挂点"), ("renderset", "string", false, "--renderset", "渲染集正则")] }),
    ("model", Tool { name: "model_remove", argv: &["model", "remove"], desc: "删除/隐藏", params: &[
        ("project", "string", true, "--project", "项目目录"), ("part", "string", false, "--part", "部件"), ("hp", "string", false, "--hp", "挂点"), ("renderset", "string", false, "--renderset", "渲染集正则")] }),
    ("model", Tool { name: "model_undo", argv: &["model", "undo"], desc: "撤销最近一次 model 编辑", params: &[("project", "string", true, "--project", "项目目录")] }),
    ("project", Tool { name: "mod_init", argv: &["mod", "init"], desc: "新建项目", params: &[
        ("dir", "string", true, "", "项目目录"), ("name", "string", true, "--name", "ASCII 名"), ("ship", "string", false, "--ship", "SDK 船名, 如 FSB510_Bourgogne"), ("mode", "string", false, "--mode", "sdk 或 loose")] }),
    ("project", Tool { name: "mod_import_sdk", argv: &["mod", "import-sdk"], desc: "导入 SDK 素材(本机记录过的船可省略路径)", params: &[
        ("dir", "string", true, "", "项目目录"), ("src", "string", false, "", "extractSources 产物目录"), ("sdk_dir", "string", false, "--sdk-dir", "ModsSDK.zip/<船> 目录")] }),
    ("project", Tool { name: "mod_status", argv: &["mod", "status"], desc: "项目文件/占位/编辑记录", params: &[("dir", "string", true, "", "项目目录")] }),
    ("install", Tool { name: "install", argv: &["install"], desc: "安装项目或 .wsmod(先查冲突, 失败回滚)", params: &[("target", "string", true, "", "项目目录或包"), ("dry_run", "boolean", false, "--dry-run!", "只演练")] }),
    ("install", Tool { name: "uninstall", argv: &["uninstall"], desc: "卸载并还原", params: &[("target", "string", true, "", "项目目录/mod 名/包"), ("dry_run", "boolean", false, "--dry-run!", "只演练"), ("force", "boolean", false, "--force!", "连被改动过的文件也删")] }),
    ("install", Tool { name: "installed", argv: &["installed"], desc: "已装 mod", params: &[("verify", "boolean", false, "--verify!", "逐文件核对")] }),
    ("install", Tool { name: "conflicts", argv: &["conflicts"], desc: "冲突检查(不写文件)", params: &[("targets", "array", true, "*", "项目或包列表")] }),
    ("install", Tool { name: "migrate", argv: &["migrate"], desc: "游戏更新后迁移", params: &[("from", "string", false, "--from", "旧版本号或目录"), ("dry_run", "boolean", false, "--dry-run!", "只演练")] }),
    ("install", Tool { name: "pack", argv: &["pack"], desc: "打 .wsmod 包", params: &[("project", "string", true, "", "项目目录"), ("out", "string", false, "-o", "输出文件或目录")] }),
    ("assets", Tool { name: "find", argv: &["find"], desc: "检索游戏资源或船", params: &[("pattern", "string", true, "", "glob 或关键字"), ("ship", "boolean", false, "--ship!", "按船名检索")] }),
    ("assets", Tool { name: "export", argv: &["export"], desc: "geometry → GLB", params: &[("input", "string", true, "", "游戏内路径或本地文件"), ("out", "string", true, "-o", "输出 .glb")] }),
    ("assets", Tool { name: "extract", argv: &["extract"], desc: "解包游戏文件", params: &[("patterns", "array", true, "*", "路径/glob"), ("out", "string", true, "-o", "输出目录")] }),
];

fn entry_tools() -> Vec<Value> {
    let groups: Vec<String> = GROUPS.iter().map(|(g, d)| format!("{g}({d})")).collect();
    vec![
        json!({"name": "wowsmod_status", "description": "第一步: 游戏位置/版本、已装 mod、本机 SDK 素材、项目状态。", "inputSchema": {"type": "object", "properties": {"project": {"type": "string", "description": "mod 项目目录(可选)"}}}}),
        json!({"name": "wowsmod_help", "description": "按需读取技能树节点(点分 id, 如 workflow.layers.paint; 不传 topic 显示树的前两层; root 是入口)。每个节点开头列出上级/下级, 只在需要时往下读。", "inputSchema": {"type": "object", "properties": {"topic": {"type": "string", "description": "节点 id 或旧主题名"}, "children": {"type": "boolean", "description": "只返回上级/下级索引"}, "tree": {"type": "boolean", "description": "返回以 topic 为根的子树"}, "search": {"type": "string", "description": "全文搜索"}}}}),
        json!({"name": "wowsmod_run", "description": "执行任意 wowsmod 命令(自动 --json), 如 [\"tex\",\"recolor\",\"X_a.dds\",\"--project\",\"p\",\"--to-color\",\"#123456\"]。结果含 next(建议下一步)。参数见 <命令> --help。", "inputSchema": {"type": "object", "properties": {"args": {"type": "array", "items": {"type": "string"}}}, "required": ["args"]}}),
        json!({"name": "wowsmod_render", "description": "出图并直接返回 PNG: 整船 WebGL(compare=分屏 原版|当前; query 可给完整参数, 见 help render)或单张贴图(texture, 左原版右当前)。走常驻出图实例: 首次调用会先预热(约 20~60 秒), 之后每张约 1 秒; wowsmod_status 带 project 时会在后台提前预热。", "inputSchema": {"type": "object", "properties": {
            "project": {"type": "string"}, "view": {"type": "string", "enum": ["side", "top", "quarter", "all"]}, "compare": {"type": "boolean"}, "texture": {"type": "string"},
            "query": {"type": "string", "description": "原样查询串, 如 view=fq&compare=split&base=base&w=1600&h=900&env=dark"}, "look": {"type": "string", "enum": ["game-day", "game-night", "studio"], "description": "外观: game-day(默认, 港口白天对齐) / game-night / studio(影棚展台)"}, "rev": {"type": "string"}, "base": {"type": "string"},
            "width": {"type": "integer", "description": "默认 1600"}, "height": {"type": "integer", "description": "默认 900"}, "timeout": {"type": "integer", "description": "单张超时秒, 默认 60"}}, "required": ["project"]}}),
        json!({"name": "wowsmod_expand", "description": format!("按组展开带参数说明的专用工具(展开后工具列表会更新; 不展开也可用 wowsmod_run)。组: {}", groups.join("; ")), "inputSchema": {"type": "object", "properties": {"group": {"type": "string", "enum": GROUPS.iter().map(|g| g.0).collect::<Vec<_>>()}}, "required": ["group"]}}),
    ]
}

fn tool_schema(t: &Tool) -> Value {
    let mut props = serde_json::Map::new();
    let mut req = Vec::new();
    for (n, ty, r, _, d) in t.params {
        let s = if *ty == "array" { json!({"type": "array", "items": {"type": "string"}, "description": d}) } else { json!({"type": ty, "description": d}) };
        props.insert(n.to_string(), s);
        if *r {
            req.push(json!(n));
        }
    }
    json!({"name": t.name, "description": t.desc, "inputSchema": {"type": "object", "properties": props, "required": req}})
}

fn tools_list(expanded: &BTreeSet<String>) -> Vec<Value> {
    let mut v = entry_tools();
    for (g, t) in TOOLS {
        if expanded.contains(*g) {
            v.push(tool_schema(t));
        }
    }
    v
}

fn val_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn argv_for(t: &Tool, a: &Value) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = t.argv.iter().map(|s| s.to_string()).collect();
    let mut opts = Vec::new();
    for (n, _, r, map, _) in t.params {
        let v = &a[*n];
        if v.is_null() {
            if *r {
                return Err(format!("缺少参数 {n}"));
            }
            continue;
        }
        match *map {
            "" => out.push(val_str(v)),
            "*" => out.extend(v.as_array().cloned().unwrap_or_default().iter().map(val_str)),
            m if m.ends_with('!') => {
                if v.as_bool().unwrap_or(false) {
                    opts.push(m.trim_end_matches('!').to_string());
                }
            }
            m => {
                opts.push(m.to_string());
                opts.push(val_str(v));
            }
        }
    }
    out.extend(opts);
    Ok(out)
}

fn run_self(cli: &Cli, args: &[String]) -> (bool, String) {
    if args.first().map(|a| a == "mcp").unwrap_or(false) {
        return (false, "不能在 MCP 里再启动 mcp".into());
    }
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => return (false, format!("{e}")),
    };
    let mut c = std::process::Command::new(exe);
    if let Some(g) = &cli.game {
        c.arg("--game").arg(g);
    }
    if !args.iter().any(|a| a == "--json") {
        c.arg("--json");
    }
    c.args(args).stdin(std::process::Stdio::null());
    match c.output() {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            let err = String::from_utf8_lossy(&o.stderr);
            if !err.trim().is_empty() {
                s += &format!("\n[stderr]\n{}", err.trim());
            }
            if s.chars().count() > MAX_TEXT {
                s = s.chars().take(MAX_TEXT).collect::<String>() + "\n…(输出过长已截断; 用更具体的参数或 --dry-run)";
            }
            (o.status.success(), s)
        }
        Err(e) => (false, format!("启动失败: {e}")),
    }
}

fn text(s: impl Into<String>, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": s.into()}], "isError": is_error})
}

fn call(cli: &Cli, name: &str, a: &Value, expanded: &mut BTreeSet<String>, changed: &mut bool) -> Value {
    match name {
        "wowsmod_status" => {
            let v = ai::status_value(cli, a["project"].as_str().map(std::path::Path::new));
            let n = guide::next("status", &v);
            let mut v = v;
            v["next"] = json!(n);
            if let Some(p) = a["project"].as_str() {
                // 后台预热常驻出图实例(不等待), 之后 wowsmod_render 每张约 1 秒
                let pp = std::path::PathBuf::from(p);
                if pp.join("mod.toml").is_file() {
                    v["render_warm"] = warm::status(&pp);
                    if v["render_warm"]["running"] != true {
                        let _ = warm::ensure_started(cli, &pp, warm::idle_minutes(None), None, false, std::time::Duration::from_secs(1));
                        v["render_warm"] = json!({"running": true, "ready": false, "state": "starting", "note": "已在后台预热, 约 20~60 秒后 wowsmod_render 每张约 1 秒"});
                    }
                }
            }
            text(serde_json::to_string_pretty(&v).unwrap_or_default(), false)
        }
        "wowsmod_help" => {
            if let Some(q) = a["search"].as_str() {
                return text(serde_json::to_string_pretty(&guide::search(q)).unwrap_or_default(), false);
            }
            if a["tree"] == true {
                return text(guide::tree_text(a["topic"].as_str().unwrap_or("root"), 99), false);
            }
            match a["topic"].as_str() {
                None => text(guide::list_text(), false),
                Some(t) => match guide::node(t) {
                    Some(n) if a["children"] == true => text(guide::children_text(n), false),
                    Some(n) => text(n.body, false),
                    None => text(format!("没有主题 {t:?}。\n{}", guide::list_text()), true),
                },
            }
        }
        "wowsmod_run" => {
            let args: Vec<String> = a["args"].as_array().cloned().unwrap_or_default().iter().map(val_str).collect();
            if args.is_empty() {
                return text("args 为空。例: [\"status\"]; 命令列表: [\"--help\"]", true);
            }
            let (ok, s) = run_self(cli, &args);
            text(s, !ok)
        }
        "wowsmod_render" => {
            let Some(p) = a["project"].as_str() else { return text("缺少 project", true) };
            let view = match a["view"].as_str().unwrap_or("quarter") {
                "side" => render::View::Side,
                "top" => render::View::Top,
                "all" => render::View::All,
                _ => render::View::Quarter,
            };
            let dir = std::path::Path::new(p).join(".render");
            let _ = std::fs::create_dir_all(&dir);
            let f = dir.join("mcp_last.png");
            // 不在 MCP 进程内渲染(任何挂起都会拖死整个 MCP): 贴图走子进程, 整船走常驻出图实例
            if let Some(t) = a["texture"].as_str() {
                let (ok, s) = run_self(cli, &["render".into(), p.into(), "--engine".into(), "soft".into(), "--texture".into(), t.into(), "-o".into(), f.display().to_string()]);
                return match (ok, std::fs::read(&f)) {
                    (true, Ok(png)) => json!({"content": [{"type": "image", "data": wowsmod_formats::mfm::b64_encode(&png), "mimeType": "image/png"}, {"type": "text", "text": s}], "isError": false}),
                    _ => text(s, true),
                };
            }
            let ra = render::RenderArgs { project: p.into(), project2: None, warm: false, idle: None, timeout: a["timeout"].as_u64().unwrap_or(60), cold: false, all: false,
                view, compare: a["compare"].as_bool().unwrap_or(false), texture: None, out: None,
                width: a["width"].as_u64().unwrap_or(1200) as u32, size: format!("{}x{}", a["width"].as_u64().unwrap_or(1600), a["height"].as_u64().unwrap_or(900)),
                engine: render::Engine::Browser, browser: None, query: a["query"].as_str().map(|s| s.to_string()), sheet: false, rev: a["rev"].as_str().unwrap_or("head").into(), base: a["base"].as_str().map(|s| s.to_string()), yaw: a["yaw"].as_f64().map(|x| x as f32), pitch: a["pitch"].as_f64().map(|x| x as f32),
                look: a["look"].as_str().filter(|l| matches!(*l, "game-day" | "game-night" | "studio" | "game")).map(|s| s.to_string()) };
            let q = render::build_query(&ra);
            let pp = std::path::Path::new(p);
            let idle = warm::idle_minutes(None);
            let t0 = std::time::Instant::now();
            let r = warm::ensure_started(cli, pp, idle, None, true, std::time::Duration::from_secs(240))
                .and_then(|w| warm::render_with_heal(cli, pp, &q, ra.timeout, idle, None).map(|(png, m)| (png, json!({"engine": "browser", "warm": true, "query": q, "meta": m["meta"], "timing": m["timing"], "warmed_now": w["spawned"]}))));
            match r {
                Ok((png, mut info)) => {
                    info["seconds"] = json!(t0.elapsed().as_secs_f32());
                    let _ = std::fs::write(&f, &png);
                    info["out"] = json!(f.display().to_string());
                    info["next"] = json!(["满意: wowsmod_run [\"install\", 项目]; 不满意: 调整后再 render"]);
                    json!({"content": [{"type": "image", "data": wowsmod_formats::mfm::b64_encode(&png), "mimeType": "image/png"}, {"type": "text", "text": serde_json::to_string_pretty(&info).unwrap_or_default()}], "isError": false})
                }
                Err(e) => {
                    let m = format!("{e:#}");
                    if m.contains("browser_not_found") {
                        return text(format!("{}\n(没有图片; 可用 wowsmod_render 的 texture 参数看单张贴图, 或 wowsmod_run [\"render\", 项目, \"--engine\", \"soft\"])", crate::preview::browser_not_found_value(None)), false);
                    }
                    let code = m.split(':').next().unwrap_or("render_failed").to_string();
                    text(format!("[{code}] {m}\n下一步: {}\n或 wowsmod_help {{\"topic\":\"render\"}}", warm::next_for(&code, pp).join(" / ")), true)
                }
            }
        }
        "wowsmod_expand" => {
            let g = a["group"].as_str().unwrap_or("");
            if !GROUPS.iter().any(|x| x.0 == g) {
                return text(format!("没有组 {g:?}; 可选: {}", GROUPS.iter().map(|x| x.0).collect::<Vec<_>>().join(", ")), true);
            }
            if expanded.insert(g.to_string()) {
                *changed = true;
            }
            let names: Vec<String> = TOOLS.iter().filter(|(gg, _)| *gg == g).map(|(_, t)| format!("{}: {}", t.name, t.desc)).collect();
            text(format!("已展开 {g}(工具列表已更新):\n{}\n若客户端没刷新工具列表, 用 wowsmod_run 执行同名命令即可。", names.join("\n")), false)
        }
        other => {
            if let Some((g, t)) = TOOLS.iter().find(|(_, t)| t.name == other) {
                if !expanded.contains(*g) {
                    // 仍然允许调用(有的客户端会缓存工具列表)
                }
                match argv_for(t, a) {
                    Ok(args) => {
                        let (ok, s) = run_self(cli, &args);
                        text(s, !ok)
                    }
                    Err(e) => text(e, true),
                }
            } else {
                text(format!("未知工具 {other}"), true)
            }
        }
    }
}

pub fn serve(cli: &Cli) -> Result<i32> {
    // 启动即后台预热: WOWSMOD_PROJECT 或当前目录是 mod 项目时
    let auto = std::env::var("WOWSMOD_PROJECT").ok().map(std::path::PathBuf::from).or_else(|| std::env::current_dir().ok().filter(|d| d.join("mod.toml").is_file()));
    if let Some(p) = auto {
        if p.join("mod.toml").is_file() && warm::status(&p)["running"] != true {
            let _ = warm::ensure_started(cli, &p, warm::idle_minutes(None), None, false, std::time::Duration::from_secs(1));
        }
    }
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    let mut expanded: BTreeSet<String> = BTreeSet::new();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                writeln!(out, "{}", json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}}))?;
                out.flush()?;
                continue;
            }
        };
        let id = msg.get("id").cloned();
        let method = msg["method"].as_str().unwrap_or("");
        let params = &msg["params"];
        let mut changed = false;
        let result: Option<Result<Value, (i64, String)>> = match method {
            "initialize" => {
                let want = params["protocolVersion"].as_str().unwrap_or(PROTOCOLS[0]);
                let pv = if PROTOCOLS.contains(&want) { want } else { PROTOCOLS[0] };
                Some(Ok(json!({"protocolVersion": pv, "capabilities": {"tools": {"listChanged": true}}, "serverInfo": {"name": "wowsmod", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": "wowsmod: 莱服《战舰世界》改模。先 wowsmod_status, 再 wowsmod_help(topic=root) 按技能树逐层读节点; 命令结果的 next 是建议下一步; 改完 wowsmod_render 看图再 install。"})))
            }
            "ping" => Some(Ok(json!({}))),
            "tools/list" => Some(Ok(json!({"tools": tools_list(&expanded)}))),
            "tools/call" => {
                let name = params["name"].as_str().unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                Some(Ok(call(cli, name, &args, &mut expanded, &mut changed)))
            }
            "resources/list" => Some(Ok(json!({"resources": []}))),
            "prompts/list" => Some(Ok(json!({"prompts": []}))),
            m if m.starts_with("notifications/") => None,
            _ => Some(Err((-32601, format!("method not found: {method}")))),
        };
        if let (Some(id), Some(r)) = (id, result) {
            let resp = match r {
                Ok(v) => json!({"jsonrpc": "2.0", "id": id, "result": v}),
                Err((c, m)) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": c, "message": m}}),
            };
            writeln!(out, "{resp}")?;
        }
        if changed {
            writeln!(out, "{}", json!({"jsonrpc": "2.0", "method": "notifications/tools/list_changed"}))?;
        }
        out.flush()?;
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn argv_mapping() {
        let t = &TOOLS.iter().find(|(_, t)| t.name == "tex_channel").unwrap().1;
        let v = argv_for(t, &json!({"tex": "X_mg.dds", "project": "p", "channel": "b", "set": 0})).unwrap();
        assert_eq!(v, ["tex", "channel", "X_mg.dds", "--project", "p", "--channel", "b", "--set", "0"]);
        let t = &TOOLS.iter().find(|(_, t)| t.name == "uninstall").unwrap().1;
        assert_eq!(argv_for(t, &json!({"target": "m", "force": true, "dry_run": false})).unwrap(), ["uninstall", "m", "--force"]);
        assert!(argv_for(t, &json!({})).is_err());
        let t = &TOOLS.iter().find(|(_, t)| t.name == "conflicts").unwrap().1;
        assert_eq!(argv_for(t, &json!({"targets": ["a", "b"]})).unwrap(), ["conflicts", "a", "b"]);
        assert_eq!(entry_tools().len(), 5);
        let names: BTreeSet<_> = TOOLS.iter().map(|t| t.1.name).collect();
        assert_eq!(names.len(), TOOLS.len(), "工具名唯一");
    }
}
