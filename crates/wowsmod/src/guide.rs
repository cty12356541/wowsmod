//! M6 渐进式披露: 分主题说明(`wowsmod help <主题>`)、每条命令的“下一步”提示(--help 末尾 / JSON 的 next / 人类输出的 建议下一步)、出错提示。
use std::sync::Mutex;

use serde_json::{json, Value};

include!(concat!(env!("OUT_DIR"), "/skill_nodes.rs"));

/// 技能树节点: 文件开头 `# <id>: <标题>` / `上级: <id|->` / `下级: 无` 或 `下级:` + `- <id>: <何时加载>` / `---`
#[derive(Debug, Clone)]
pub struct Node {
    pub id: &'static str,
    pub title: &'static str,
    pub parent: Option<&'static str>,
    pub children: Vec<(&'static str, &'static str)>,
    pub body: &'static str,
    pub file: &'static str,
}

fn parse_node(file: &'static str, text: &'static str) -> Result<Node, String> {
    let mut lines = text.lines();
    let h = lines.next().unwrap_or("");
    let h = h.strip_prefix("# ").ok_or(format!("{file}: 第一行应为 `# <id>: <标题>`"))?;
    let (id, title) = h.split_once(": ").ok_or(format!("{file}: 标题行缺 `: `"))?;
    let p = lines.next().unwrap_or("").strip_prefix("上级: ").ok_or(format!("{file}: 第二行应为 `上级: <id>`"))?.trim();
    let parent = if p == "-" { None } else { Some(p) };
    let mut children = Vec::new();
    let mut seen_sep = false;
    for l in lines.by_ref() {
        if l.trim() == "---" {
            seen_sep = true;
            break;
        }
        if let Some(c) = l.strip_prefix("- ") {
            let (cid, when) = c.split_once(": ").ok_or(format!("{file}: 下级行应为 `- <id>: <何时加载>`"))?;
            children.push((cid.trim(), when.trim()));
        }
    }
    if !seen_sep {
        return Err(format!("{file}: 头部缺 `---`"));
    }
    Ok(Node { id: id.trim(), title: title.trim(), parent, children, body: text, file })
}

pub fn nodes() -> &'static [Node] {
    static N: std::sync::OnceLock<Vec<Node>> = std::sync::OnceLock::new();
    N.get_or_init(|| NODE_FILES.iter().filter_map(|(f, t)| parse_node(f, t).ok()).collect())
}

/// 校验: 头部格式、无孤儿、无死链(下级与正文里的 `help <id>` / 反引号节点 id)、长度上限
pub const NODE_MAX_CHARS: usize = 2000;
pub const ROOT_MAX_CHARS: usize = 1200;
pub fn check_tree() -> Vec<String> {
    let mut errs = Vec::new();
    let mut ns = Vec::new();
    for (f, t) in NODE_FILES {
        match parse_node(f, t) {
            Ok(n) => ns.push(n),
            Err(e) => errs.push(e),
        }
    }
    let ids: std::collections::HashSet<&str> = ns.iter().map(|n| n.id).collect();
    if ids.len() != ns.len() {
        errs.push("有重复的节点 id".into());
    }
    let id_re = regex::Regex::new(r"(?:help |`)((?:root|workflow|ref|styles|learn)(?:\.[a-z0-9-]+)*)\b").unwrap();
    let help_re = regex::Regex::new(r"wowsmod help ([a-z][a-z0-9.-]*)").unwrap();
    for n in &ns {
        let lim = if n.id == "root" { ROOT_MAX_CHARS } else { NODE_MAX_CHARS };
        let c = n.body.chars().count();
        if c > lim {
            errs.push(format!("{}: {c} 字, 超过上限 {lim}(拆成子节点)", n.id));
        }
        match n.parent {
            None if n.id != "root" => errs.push(format!("{}: 只有 root 可以没有上级", n.id)),
            Some(p) => match ns.iter().find(|x| x.id == p) {
                None => errs.push(format!("{}: 上级 {p} 不存在", n.id)),
                Some(pn) => {
                    if !pn.children.iter().any(|c| c.0 == n.id) {
                        errs.push(format!("{}: 上级 {p} 的下级列表里没有它(孤儿)", n.id));
                    }
                    if !n.id.starts_with(&format!("{p}.")) && p != "root" {
                        errs.push(format!("{}: id 应以上级路径 {p}. 开头", n.id));
                    }
                }
            },
            _ => {}
        }
        for (c, w) in &n.children {
            if !ids.contains(c) {
                errs.push(format!("{}: 下级 {c} 不存在(死链)", n.id));
            }
            if w.is_empty() {
                errs.push(format!("{}: 下级 {c} 缺'何时加载'", n.id));
            }
        }
        for cap in id_re.captures_iter(n.body).chain(help_re.captures_iter(n.body)) {
            let t = cap[1].trim_end_matches('.');
            if !ids.contains(t) && topic(t).is_none() {
                errs.push(format!("{}: 引用了不存在的节点 {t}", n.id));
            }
        }
    }
    errs
}

/// 旧主题名 / 命令名 → 节点 id
fn alias(t: &str) -> &str {
    match t {
        "start" | "help" | "入口" => "root",
        "project" | "mod" | "init" | "import-sdk" | "sdk" | "new" => "ref.project",
        "recolor" | "tex" | "texture" | "color" | "colour" | "camo" | "port-albedo" | "换色" => "ref.recolor",
        "glow" | "emissive" | "发光" => "ref.glow",
        "install" | "uninstall" | "installed" | "conflicts" | "conflict" => "ref.install",
        "model" | "assemble" | "scale" | "move" | "rotate" | "remove" | "replace" | "add" => "ref.model",
        "import" | "pmx" | "pmx2glb" => "ref.model.import",
        "animate" | "anim" | "动画" => "ref.model.animate",
        "blender" => "ref.blender",
        "render" | "preview" | "出图" => "ref.render",
        "warm" => "ref.render.warm",
        "pack" => "ref.pack",
        "migrate" => "ref.migrate",
        "troubleshoot" | "error" | "errors" | "问题" => "ref.troubleshoot",
        "formats" | "export" | "extract" | "find" | "geom" | "check" => "ref.formats",
        "mcp" | "ai" | "agents" => "ref.mcp",
        "design" | "设计" | "skin" | "涂装设计" | "mode" | "模式" => "workflow",
        "design-book" | "设计书" | "拆解书" | "book" | "design-template" | "template" => "workflow.book",
        "layers" | "层" => "workflow.layers",
        "design-ref" | "参考" | "ref-layer" => "workflow.layers.ref",
        "拆解" | "breakdown" | "design-map" => "workflow.layers.map",
        "design-shape" | "shape" | "大形" => "workflow.layers.shape",
        "design-paint" | "lineart" | "bake-ref" | "图案" | "paint" => "workflow.layers.paint",
        "design-glow" => "workflow.layers.glow",
        "design-fx" | "fx" | "特效" | "smoke" | "烟" => "workflow.layers.fx",
        "design-verify" | "验收" | "verify" => "workflow.layers.verify",
        "design-changelog" | "修订记录" | "changelog" => "workflow.changelog",
        "review" | "复盘" => "workflow.review",
        "replicate" | "复刻" | "定制复刻" | "replica" | "copy" => "workflow.replicate",
        "theme" | "主题" | "主题设计" => "workflow.theme",
        "original" | "原创" => "workflow.original",
        "style" | "风格包" => "styles",
        x => x,
    }
}

/// (id, 标题, 全文)
pub fn topic(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    let low = name.trim().to_lowercase();
    let n = alias(low.as_str());
    nodes().iter().find(|x| x.id == n).map(|x| (x.id, x.title, x.body))
}

pub fn node(name: &str) -> Option<&'static Node> {
    let (id, _, _) = topic(name)?;
    nodes().iter().find(|x| x.id == id)
}

/// 树形文本(从 id 起, 缩进显示 id、标题与何时加载)
pub fn tree_text(from: &str, max_depth: usize) -> String {
    fn walk(id: &str, when: &str, depth: usize, max: usize, out: &mut String) {
        let Some(n) = nodes().iter().find(|x| x.id == id) else { return };
        let w = if when.is_empty() { String::new() } else { format!(" — {when}") };
        *out += &format!("{}{}  {}{}\n", "  ".repeat(depth), n.id, n.title, w);
        if depth < max {
            for (c, w) in &n.children {
                walk(c, w, depth + 1, max, out);
            }
        }
    }
    let mut s = String::new();
    let id = topic(from).map(|t| t.0).unwrap_or("root");
    walk(id, "", 0, max_depth, &mut s);
    s
}

pub fn tree_json(from: &str) -> Value {
    fn walk(id: &str, when: &str) -> Value {
        let Some(n) = nodes().iter().find(|x| x.id == id) else { return Value::Null };
        json!({"id": n.id, "title": n.title, "when": when, "chars": n.body.chars().count(), "children": n.children.iter().map(|(c, w)| walk(c, w)).collect::<Vec<_>>()})
    }
    walk(topic(from).map(|t| t.0).unwrap_or("root"), "")
}

pub fn children_text(n: &Node) -> String {
    let mut s = format!("{}  {}\n上级: {}\n", n.id, n.title, n.parent.unwrap_or("-"));
    if n.children.is_empty() {
        s += "下级: 无\n";
    }
    for (c, w) in &n.children {
        s += &format!("  {c} — {w}\n");
    }
    s
}

pub fn list_text() -> String {
    let mut s = String::from("wowsmod 技能树(渐进阅读: wowsmod help <id>; 只看下级: --children; 全树: --tree; 搜索: --search <词>)\n");
    s += &tree_text("root", 2);
    s += "命令参数: wowsmod <命令> --help。从 `wowsmod help root` 开始。";
    s
}

pub fn search(q: &str) -> Vec<Value> {
    let q = q.to_lowercase();
    let mut out = Vec::new();
    for n in nodes() {
        let lines: Vec<&str> = n.body.lines().filter(|l| l.to_lowercase().contains(&q)).take(4).collect();
        if !lines.is_empty() {
            out.push(json!({"topic": n.id, "lines": lines}));
        }
    }
    out
}

// ---------- 命令键与参数 ----------
const GROUPS: &[&str] = &["tex", "mod", "model"];

/// 从 argv 得到命令键, 如 "install" / "tex recolor"
pub fn cmd_key_from(args: &[String]) -> String {
    let mut it = args.iter().skip(1).peekable();
    let mut parts: Vec<String> = Vec::new();
    while let Some(a) = it.next() {
        if a == "--game" {
            it.next();
            continue;
        }
        if a.starts_with('-') {
            continue;
        }
        parts.push(a.clone());
        if parts.len() == 1 && GROUPS.contains(&a.as_str()) {
            continue;
        }
        break;
    }
    parts.join(" ")
}

pub fn cmd_key() -> String {
    cmd_key_from(&std::env::args().collect::<Vec<_>>())
}

/// 命令里指向的项目目录(--project X, 或 mod/render/install 等的第一个位置参数)
pub fn project_arg() -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    if let Some(i) = a.iter().position(|x| x == "--project") {
        return a.get(i + 1).cloned();
    }
    let key = cmd_key_from(&a);
    let n = key.split(' ').count();
    let positional_first = ["mod init", "mod import-sdk", "mod status", "render", "install", "pack", "status"];
    if !positional_first.contains(&key.as_str()) {
        return None;
    }
    let mut seen = 0;
    let mut skip = false;
    for x in a.iter().skip(1) {
        if skip {
            skip = false;
            continue;
        }
        if x == "--game" || x == "-o" || x == "--out" || x == "--name" || x == "--ship" || x == "--mode" || x == "--res-mods" {
            skip = true;
            continue;
        }
        if x.starts_with('-') {
            continue;
        }
        seen += 1;
        if seen == n + 1 {
            return Some(x.clone());
        }
    }
    None
}

fn q(s: &str) -> String {
    if s.contains(' ') {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

/// 结果 → 建议下一步
pub fn next(key: &str, v: &Value) -> Vec<String> {
    let p = project_arg().map(|s| q(&s)).unwrap_or_else(|| "<项目>".into());
    let s = |x: &str| x.replace("{p}", &p);
    let ok = v.get("ok").map(|x| x != false).unwrap_or(true);
    let mut n: Vec<String> = match key {
        "doctor" => vec!["wowsmod status".into(), "wowsmod help start".into()],
        "status" => {
            if v["project"].is_null() {
                vec!["wowsmod help start(按任务选主题)".into(), "新建项目: wowsmod help project".into()]
            } else if v["project"]["sdk_imported"] == false && v["project"]["mode"] == "sdk" {
                vec![s("wowsmod mod import-sdk {p}(见 wowsmod help project)")]
            } else {
                vec![s("wowsmod render {p} --compare"), s("wowsmod install {p}"), "按任务看主题: wowsmod help recolor / glow / model".into()]
            }
        }
        "find" => vec!["wowsmod extract <游戏内路径> -o <目录>".into(), "建 mod 项目: wowsmod help project".into()],
        "extract" | "export" | "geom" | "check" => vec!["改模请建项目: wowsmod help project".into()],
        "mod init" => {
            if v["mode"] == "sdk" {
                vec![s("wowsmod mod import-sdk {p}(首次见 wowsmod help project)")]
            } else {
                vec!["loose 项目: 用 tex import/recolor --project 往里放贴图(wowsmod help recolor)".into()]
            }
        }
        "mod import-sdk" => vec![s("wowsmod mod status {p}"), "按任务看主题: wowsmod help recolor / glow / model".into()],
        "mod status" => vec![s("wowsmod render {p} --compare"), s("wowsmod install {p}")],
        "port-albedo" => vec![format!("wowsmod tex recolor <贴图_a.dds> --project <项目> --to-color {} --exclude-hue 0:65 --soft 0.04", v["to_color"].as_str().unwrap_or("<to_color>"))],
        "tex recolor" => vec!["对应 _mg 去迷彩遮罩: wowsmod tex channel <同名_mg.dds> --project <项目> --channel b --set 0(DeckHouse 若要发光则跳过)".into(), s("wowsmod render {p} --compare")],
        "tex channel" | "tex import" | "tex rewrite" | "glow" | "mask" => vec![s("wowsmod render {p} --compare"), s("wowsmod install {p}")],
        "tex info" | "tex export" => vec!["换色: wowsmod help recolor".into()],
        k if k.starts_with("model ") => {
            if k == "model list" {
                vec!["wowsmod help model".into()]
            } else {
                vec![s("wowsmod render {p} --compare --view all"), s("撤销: wowsmod model undo --project {p}"), s("wowsmod install {p}")]
            }
        }
        "assemble" => vec!["看图: wowsmod render <项目> --compare".into()],
        "render" if ["warm", "status", "stop", "serve"].contains(&p.as_str()) => vec![],
        "render" if std::env::args().any(|x| x == "--warm") => vec![s("出图: wowsmod render {p} --compare(自动连常驻实例)"), s("状态 / 关闭: wowsmod render status {p} / wowsmod render stop {p}")],
        "render" => vec![s("满意: wowsmod install {p} 后重启游戏进港查看(只看港口)"), "不满意: 调整后再 render".into()].into_iter().chain((v["warm"] != true).then(|| s("要连续出图: 先 wowsmod render --warm {p} 预热, 之后每张约 1 秒"))).collect(),
        "install" => {
            if ok {
                let t = project_arg().map(|x| q(&x)).unwrap_or_else(|| v["mod"].as_str().unwrap_or("<mod>").to_string());
                vec!["重启游戏进港查看(只看港口, 不进战斗)".into(), "wowsmod installed --verify".into(), format!("撤销: wowsmod uninstall {t}")]
            } else {
                vec!["wowsmod conflicts <项目>".into(), "wowsmod help install".into()]
            }
        }
        "uninstall" => {
            if v["kept_modified"].as_array().map(|a| !a.is_empty()).unwrap_or(false) {
                vec!["有被改动过的文件被保留: 确认不要后加 --force 再卸载一次".into()]
            } else {
                vec!["wowsmod installed".into()]
            }
        }
        "installed" => {
            if v["intact"] == false {
                vec!["有文件被改动/缺失: wowsmod install <项目> --reinstall 修复, 或 wowsmod uninstall <mod> --force".into()]
            } else {
                vec!["wowsmod status".into()]
            }
        }
        "conflicts" => {
            if v["conflicts_total"].as_u64().unwrap_or(0) > 0 || !ok {
                vec!["先卸载冲突的 mod: wowsmod uninstall <mod 名>".into(), "wowsmod help install".into()]
            } else {
                vec!["wowsmod install <项目>".into()]
            }
        }
        "migrate" => vec!["wowsmod installed --verify".into(), "game_files_updated_since_install 里列出的改动请进港复查".into()],
        "pack" => vec![format!("分发 {}; 对方: wowsmod install <包.wsmod>", v["package"].as_str().unwrap_or("<包>")), "wowsmod help pack".into()],
        "ai init" => vec!["在该目录打开 AI 工具(Cursor / Claude Code / Codex 等); 它会读 AGENTS.md".into(), "wowsmod help mcp".into()],
        _ => vec![],
    };
    n.dedup();
    n
}

/// 出错 → 下一步
pub fn error_hint(key: &str, msg: &str) -> Vec<String> {
    let mut h = Vec::new();
    let m = msg;
    if m.contains("下一步") {
        return h; // 出错信息自身已给出
    }
    if m.contains("没有找到游戏") || m.contains("找不到游戏") || m.contains("游戏客户端") {
        h.push("wowsmod doctor; 或加 --game <游戏根目录(含 res_packages 与 bin)>".into());
    }
    if m.contains("mod.toml") || m.contains("不是 mod 项目") {
        h.push("先 wowsmod mod init <目录> --name <名> --ship <船> --mode sdk(wowsmod help project)".into());
    }
    if m.contains("import-sdk") || m.contains("不是 SDK 路线项目") {
        h.push("wowsmod help project".into());
    }
    if m.contains("冲突") {
        h.push("wowsmod conflicts <项目>; wowsmod help install".into());
    }
    if m.contains("游戏正在运行") {
        h.push("退出游戏后重试".into());
    }
    if m.contains("匹配到") || m.contains("没有匹配") {
        h.push("用 wowsmod find <关键字> 查准确路径/文件名".into());
    }
    if h.is_empty() {
        let t = topic(key.split(' ').next().unwrap_or("")).map(|t| t.0).unwrap_or("troubleshoot");
        h.push(format!("wowsmod {key} --help"));
        h.push(format!("wowsmod help {t}"));
    }
    h
}

/// clap --help 末尾的“下一步/相关主题”
pub fn after_help(key: &str) -> Option<String> {
    let t = match key {
        "doctor" | "status" => "下一步: wowsmod help start(按任务选主题)",
        "find" | "extract" | "export" | "geom" | "check" => "相关主题: wowsmod help formats。改模请建项目: wowsmod help project",
        "mod" | "mod init" | "mod import-sdk" | "mod status" => "下一步: init → import-sdk → 编辑 → render → install。详见 wowsmod help project",
        "tex" | "tex recolor" | "tex channel" | "tex export" | "tex import" | "tex info" | "tex rewrite" | "port-albedo" | "mask" => "下一步: 编辑后 wowsmod render <项目> --compare。详见 wowsmod help recolor",
        "glow" => "下一步: wowsmod render <项目> --compare。详见 wowsmod help ref.glow",
        "model" | "assemble" => "下一步: 编辑后 wowsmod render <项目> --compare --view all; 撤销 model undo。详见 wowsmod help model / blender",
        "render" => "下一步: 满意则 wowsmod install <项目>。要连续出图先 wowsmod render --warm <项目> 预热(之后每张约 1 秒)。详见 wowsmod help render",
        "install" | "uninstall" | "installed" | "conflicts" => "下一步: install 后重启游戏进港查看; 撤销 wowsmod uninstall。详见 wowsmod help install",
        "migrate" => "详见 wowsmod help migrate",
        "pack" => "详见 wowsmod help pack",
        "mcp" | "ai" | "ai init" => "详见 wowsmod help mcp",
        _ => return None,
    };
    Some(t.to_string())
}

pub fn decorate(mut cmd: clap::Command) -> clap::Command {
    cmd = cmd.after_help("从这里开始: wowsmod help start   主题列表: wowsmod help   AI 接入: wowsmod help mcp");
    let names: Vec<String> = cmd.get_subcommands().map(|c| c.get_name().to_string()).collect();
    for n in names {
        cmd = cmd.mut_subcommand(&n, |mut sc| {
            if let Some(a) = after_help(&n) {
                sc = sc.after_help(a);
            }
            let subs: Vec<String> = sc.get_subcommands().map(|c| c.get_name().to_string()).collect();
            for s in subs {
                let k = format!("{n} {s}");
                if let Some(a) = after_help(&k).or_else(|| after_help(&n)) {
                    sc = sc.mut_subcommand(&s, |x| x.after_help(a));
                }
            }
            sc
        });
    }
    cmd
}

// ---------- 人类模式: 结果暂存, 结束时打印建议下一步 ----------
static LAST: Mutex<Option<Value>> = Mutex::new(None);

pub fn stash(v: &Value) {
    if let Ok(mut g) = LAST.lock() {
        *g = Some(v.clone());
    }
}

pub fn peek() -> Option<Value> {
    LAST.lock().ok().and_then(|g| g.clone())
}

pub fn take() -> Option<Value> {
    LAST.lock().ok().and_then(|mut g| g.take())
}

/// JSON 结果加上 next(已有则不覆盖)
pub fn with_next(v: &Value) -> Value {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
        if !o.contains_key("next") {
            let n = next(&cmd_key(), &Value::Object(o.clone()));
            if !n.is_empty() {
                o.insert("next".into(), json!(n));
            }
        }
    }
    v
}

pub fn print_next_human(code: i32) {
    let key = cmd_key();
    if key.is_empty() || key == "help" || key == "mcp" {
        return;
    }
    let v = take().unwrap_or_else(|| json!({"ok": code == 0}));
    let n = next(&key, &v);
    if !n.is_empty() {
        println!("\n建议下一步:");
        for x in n {
            println!("  - {x}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn a(s: &str) -> Vec<String> {
        s.split(' ').map(|x| x.to_string()).collect()
    }
    #[test]
    fn keys_and_topics() {
        assert_eq!(cmd_key_from(&a("wowsmod --json tex recolor X --project p")), "tex recolor");
        assert_eq!(cmd_key_from(&a("wowsmod --game D:/K install p")), "install");
        assert_eq!(cmd_key_from(&a("wowsmod --json")), "");
        let errs = check_tree();
        assert!(errs.is_empty(), "技能树校验: {errs:#?}");
        assert_eq!(topic("tex").unwrap().0, "ref.recolor");
        assert_eq!(topic("start").unwrap().0, "root");
        assert!(topic("nope").is_none());
        assert!(tree_text("root", 9).contains("workflow.layers.paint"));
        assert!(!next("port-albedo", &json!({"to_color": "#123456"})).is_empty());
    }
}
