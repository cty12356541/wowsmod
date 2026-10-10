//! M6 渐进式披露: 分主题说明(`wowsmod help <主题>`)、每条命令的“下一步”提示(--help 末尾 / JSON 的 next / 人类输出的 建议下一步)、出错提示。
use std::sync::Mutex;

use serde_json::{json, Value};

pub const TOPICS: &[(&str, &str, &str)] = &[
    ("start", "从零开始 / 任务路由", include_str!("../../../docs/topics/start.md")),
    ("project", "mod 项目与 SDK 导入", include_str!("../../../docs/topics/project.md")),
    ("recolor", "换色 / 涂装 / 去迷彩", include_str!("../../../docs/topics/recolor.md")),
    ("glow", "发光", include_str!("../../../docs/topics/glow.md")),
    ("model", "几何编辑: 缩放/移动/删除/替换/新增", include_str!("../../../docs/topics/model.md")),
    ("blender", "Blender 往返", include_str!("../../../docs/topics/blender.md")),
    ("render", "出图看效果", include_str!("../../../docs/topics/render.md")),
    ("install", "安装 / 卸载 / 冲突", include_str!("../../../docs/topics/install.md")),
    ("pack", "打包分发", include_str!("../../../docs/topics/pack.md")),
    ("migrate", "游戏更新后迁移", include_str!("../../../docs/topics/migrate.md")),
    ("troubleshoot", "常见问题", include_str!("../../../docs/topics/troubleshoot.md")),
    ("formats", "格式与原理(深入)", include_str!("../../../docs/topics/formats.md")),
    ("mcp", "AI 工具接入(MCP / 规则文件)", include_str!("../../../docs/topics/mcp.md")),
];

/// 主题别名(命令名 → 主题)
fn alias(t: &str) -> &str {
    match t {
        "tex" | "texture" | "color" | "colour" | "camo" | "port-albedo" | "换色" => "recolor",
        "mod" | "init" | "import-sdk" | "sdk" | "new" => "project",
        "emissive" | "发光" => "glow",
        "uninstall" | "installed" | "conflicts" | "conflict" => "install",
        "assemble" | "scale" | "move" | "rotate" | "remove" | "replace" | "add" => "model",
        "error" | "errors" | "问题" => "troubleshoot",
        "ai" | "agents" => "mcp",
        "export" | "extract" | "find" | "geom" | "check" => "formats",
        x => x,
    }
}

pub fn topic(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    let n = alias(name.trim().to_lowercase().as_str()).to_string();
    TOPICS.iter().find(|t| t.0 == n).copied()
}

pub fn list_text() -> String {
    let mut s = String::from("wowsmod 主题说明(按需阅读: wowsmod help <主题>; 搜索: wowsmod help --search <词>)\n");
    for (n, t, _) in TOPICS {
        s += &format!("  {n:<13}{t}\n");
    }
    s += "命令参数: wowsmod <命令> --help。新手从 `wowsmod help start` 开始。";
    s
}

pub fn search(q: &str) -> Vec<Value> {
    let q = q.to_lowercase();
    let mut out = Vec::new();
    for (n, _, body) in TOPICS {
        let lines: Vec<&str> = body.lines().filter(|l| l.to_lowercase().contains(&q)).take(4).collect();
        if !lines.is_empty() {
            out.push(json!({"topic": n, "lines": lines}));
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
        "render" => vec![s("满意: wowsmod install {p} 后重启游戏进港查看(只看港口)"), "不满意: 调整后再 render".into()],
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
        "glow" => "下一步: wowsmod render <项目> --compare。详见 wowsmod help glow(含勃艮第烟囱实测参数)",
        "model" | "assemble" => "下一步: 编辑后 wowsmod render <项目> --compare --view all; 撤销 model undo。详见 wowsmod help model / blender",
        "render" => "下一步: 满意则 wowsmod install <项目>。详见 wowsmod help render",
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
        for (n, _, body) in TOPICS {
            assert!(body.starts_with(&format!("# {n}")), "主题 {n} 标题");
            assert!(body.chars().count() < 2500, "主题 {n} 太长");
        }
        assert_eq!(topic("tex").unwrap().0, "recolor");
        assert!(topic("nope").is_none());
        assert!(!next("port-albedo", &json!({"to_color": "#123456"})).is_empty());
    }
}
