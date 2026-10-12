//! M6: status(AI 第一步的总览)、help(分主题说明)、ai init(AGENTS.md 与各工具规则副本、MCP 配置)。
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::{json, Value};

use crate::modproj::{self, Project};
use crate::{emit, game, guide, m4cmd, registry, Cli};

#[derive(Args, Clone, Debug)]
pub struct StatusArgs {
    /// mod 项目目录(可选)
    pub project: Option<PathBuf>,
}

pub fn status_value(cli: &Cli, project: Option<&Path>) -> Value {
    let (gi, _) = game::locate(cli.game.as_deref());
    let game_v = match &gi {
        Some(g) => json!({"found": true, "root": g.root.display().to_string(), "build": g.build, "client_version": g.client_version, "res_mods": g.res_mods.display().to_string(), "bin_builds": g.bin_dirs}),
        None => json!({"found": false, "hint": "wowsmod doctor; 或 --game <游戏根目录>"}),
    };
    let installed = gi
        .as_ref()
        .map(|g| match registry::load(&g.res_mods) {
            Ok(r) => json!(r.mods.values().map(|m| json!({"name": m.name, "ship": m.ship, "version": m.version, "project_dir": m.project_dir})).collect::<Vec<_>>()),
            Err(e) => json!({"error": format!("{e:#}")}),
        })
        .unwrap_or(Value::Null);
    let pristine = gi.as_ref().map(|g| game::res_mods_state(&g.res_mods).pristine);
    let proj = project.map(|d| match Project::open(d) {
        Ok(p) => {
            let files = p.files().unwrap_or_default();
            let stubs = files.iter().filter(|f| std::fs::read(f).map(|x| modproj::is_binary_stub(&p.rel(f), &x)).unwrap_or(false)).count();
            let edits = std::fs::read_dir(p.root.join("edits")).map(|r| r.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".json")).count()).unwrap_or(0);
            let ship_dir = p.assets().join("PnFMods").join(&p.cfg.meta.name).join(&p.cfg.meta.ship);
            json!({"dir": p.root.display().to_string(), "name": p.cfg.meta.name, "ship": p.cfg.meta.ship, "mode": p.cfg.install.mode, "files": files.len(), "stubs": stubs, "edits": edits,
                "sdk_imported": ship_dir.join("ship").is_dir(), "sdk_copy": p.assets().join("PnFMods").join("ModsSDK.zip").join(&p.cfg.meta.ship).is_dir(),
                "installed": installed.as_array().map(|a| a.iter().any(|m| m["name"] == p.cfg.meta.name.as_str())).unwrap_or(false)})
        }
        Err(e) => json!({"dir": d.display().to_string(), "error": format!("{e:#}")}),
    });
    let running = game::running();
    json!({"ok": true, "wowsmod_version": env!("CARGO_PKG_VERSION"), "game": game_v, "game_running": running, "installed_mods": installed, "res_mods_pristine": pristine,
        "sdk_sources": m4cmd::sdk_sources(), "wowsmod_home": m4cmd::home_dir().display().to_string(), "project": proj.unwrap_or(Value::Null),
        "skill_tree": guide::tree_json("root")["children"].as_array().map(|a| a.iter().map(|c| c["id"].clone()).collect::<Vec<_>>()).unwrap_or_default(), "next_help": "wowsmod help root"})
}

pub fn cmd_status(cli: &Cli, a: &StatusArgs) -> Result<i32> {
    let v = status_value(cli, a.project.as_deref());
    guide::stash(&v);
    if cli.json {
        emit(cli, &v);
    } else {
        let g = &v["game"];
        if g["found"] == true {
            println!("游戏: {} (版本 {}, build {})", g["root"].as_str().unwrap_or(""), g["client_version"].as_str().unwrap_or("?"), g["build"].as_str().unwrap_or(""));
            println!("res_mods: {}{}", g["res_mods"].as_str().unwrap_or(""), if v["res_mods_pristine"] == true { "(原样, 未装 mod)" } else { "" });
        } else {
            println!("没有找到游戏(wowsmod doctor / --game)");
        }
        if let Some(r) = v["game_running"].as_array().filter(|a| !a.is_empty()) {
            println!("游戏正在运行: {r:?}(此时不能 install/uninstall)");
        }
        match v["installed_mods"].as_array() {
            Some(a) if !a.is_empty() => {
                println!("已装 mod:");
                for m in a {
                    println!("  {} {} [{}]", m["name"].as_str().unwrap_or(""), m["version"].as_str().unwrap_or(""), m["ship"].as_str().unwrap_or(""));
                }
            }
            _ => println!("已装 mod: 无"),
        }
        let s = v["sdk_sources"].as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
        println!("本机已记录 SDK 素材的船: {}", if s.is_empty() { "无(首次见 wowsmod help project)".into() } else { s.join(", ") });
        if let Some(p) = v["project"].as_object() {
            if let Some(e) = p.get("error") {
                println!("项目: {}", e.as_str().unwrap_or(""));
            } else {
                println!("项目 {}: mod={} ship={} mode={} 文件 {} (占位 {}) 编辑 {} 次{}{}", p["dir"].as_str().unwrap_or(""), p["name"].as_str().unwrap_or(""), p["ship"].as_str().unwrap_or(""), p["mode"].as_str().unwrap_or(""), p["files"], p["stubs"], p["edits"],
                    if p["sdk_imported"] == true { ", 已导入 SDK" } else { "" }, if p["installed"] == true { ", 已安装" } else { "" });
            }
        }
    }
    Ok(0)
}

pub fn cmd_help(cli: &Cli, topic: Option<&str>, search: Option<&str>, tree: bool, children: bool, check: bool) -> Result<i32> {
    if check {
        let errs = guide::check_tree();
        let sizes: Vec<Value> = guide::nodes().iter().map(|n| json!({"id": n.id, "chars": n.body.chars().count(), "lines": n.body.lines().count()})).collect();
        if cli.json {
            emit(cli, &json!({"ok": errs.is_empty(), "errors": errs, "nodes": sizes, "max_chars": guide::NODE_MAX_CHARS}));
        } else {
            for n in guide::nodes() {
                println!("{:<40} {:>5} 字 {:>3} 行", n.id, n.body.chars().count(), n.body.lines().count());
            }
            if errs.is_empty() {
                println!("技能树校验通过: {} 个节点", guide::nodes().len());
            } else {
                for e in &errs {
                    println!("错误: {e}");
                }
            }
        }
        return Ok(if errs.is_empty() { 0 } else { 1 });
    }
    if let Some(q) = search {
        let r = guide::search(q);
        if cli.json {
            emit(cli, &json!({"ok": true, "search": q, "hits": r}));
        } else if r.is_empty() {
            println!("没有找到 {q:?}。技能树: wowsmod help --tree");
        } else {
            for h in &r {
                println!("[{}]", h["topic"].as_str().unwrap_or(""));
                for l in h["lines"].as_array().into_iter().flatten() {
                    println!("  {}", l.as_str().unwrap_or(""));
                }
            }
            println!("\n读节点: wowsmod help <id>");
        }
        return Ok(0);
    }
    if tree {
        let from = topic.unwrap_or("root");
        if cli.json {
            emit(cli, &json!({"ok": true, "tree": guide::tree_json(from)}));
        } else {
            print!("{}", guide::tree_text(from, 99));
        }
        return Ok(0);
    }
    match topic {
        None => {
            if cli.json {
                emit(cli, &json!({"ok": true, "tree": guide::tree_json("root"), "next": ["wowsmod help root"]}));
            } else {
                println!("{}", guide::list_text());
            }
            Ok(0)
        }
        Some(t) => match guide::node(t) {
            Some(n) => {
                if cli.json {
                    emit(cli, &json!({"ok": true, "id": n.id, "topic": n.id, "title": n.title, "parent": n.parent, "children": n.children.iter().map(|(c, w)| json!({"id": c, "when": w})).collect::<Vec<_>>(), "text": if children { "" } else { n.body }, "chars": n.body.chars().count()}));
                } else if children {
                    print!("{}", guide::children_text(n));
                } else {
                    println!("{}", n.body);
                }
                Ok(0)
            }
            None => {
                let v = json!({"ok": false, "error": format!("没有节点 {t:?}"), "next": [format!("命令参数请用 wowsmod {t} --help"), "wowsmod help --tree".to_string()]});
                if cli.json {
                    emit(cli, &v);
                } else {
                    println!("没有节点 {t:?}。\n{}\n(命令参数: wowsmod {t} --help)", guide::list_text());
                }
                Ok(2)
            }
        },
    }
}

#[derive(Subcommand, Clone, Debug)]
pub enum AiCmd {
    /// 在工作目录写 AGENTS.md(正本, 极短)与 CLAUDE.md / .cursor/rules / GEMINI.md / copilot 指针, 以及 .cursor/mcp.json、.mcp.json
    Init {
        /// 工作目录(AI 工具在这里打开)
        dir: PathBuf,
        /// 覆盖已有文件
        #[arg(long)]
        force: bool,
    },
    /// 打印 AGENTS.md 内容(不写文件)
    Agents,
}

pub fn agents_md(exe: &str) -> String {
    format!(
        r#"# wowsmod 改模工作区

本目录用 `wowsmod` 改 Lesta《战舰世界》的船模与贴图。可执行文件:`{exe}`(下文简称 wowsmod)。

## 规则
- 只用 wowsmod 改项目目录; 只有 `install` 写游戏, `uninstall` 完全还原。游戏运行时不能 install; 验证只在港口, 不进战斗。
- 命令加 `--json`, 结果里的 `next` 是建议下一步。不确定就说不确定, 用 `--dry-run` / `render` 先验证。
- 出图看效果: `render --look game-day`(游戏内预览, 按港口实拍拟合)定稿, 港口只确认; `--look studio` 是浅色展台图。见 `help workflow.layers.verify.wysiwyg`。
- Oodle: 本仓库不含 Oodle 解码器(见 docs/OODLE.md), 勿分发游戏 DLL。
- 素材许可: 第三方模型/立绘多禁止二次分发; 导入时 wowsmod 会扫描使用规则并给项目打 `[license]` 标记, `pack` 默认拒绝, 只本机 `install`。训练房/购买需用户明确授权。
- Windows PowerShell 会弄坏管道里的 UTF-8 JSON: 用 `--json-out <文件>` 取结果。

## 技能树(渐进阅读, 不要一次读完)
1. `wowsmod status [项目]`。
2. `wowsmod help root`: 判断任务 → 整套外观走 `workflow`(先判断模式: 复刻/主题/原创), 单个操作查 `ref`。
3. 每个节点开头列出上级/下级与"何时加载"; 只读当前需要的节点(`wowsmod help <id>`, `--children` 只看索引, `--tree` 看全树)。

## MCP
如果已连接 `wowsmod mcp`,用它的 5 个入口工具(status / help / run / render / expand),流程同上。
"#
    )
}

fn write_if(path: &Path, content: &str, force: bool, done: &mut Vec<Value>) -> Result<()> {
    if path.exists() && !force {
        done.push(json!({"file": path.display().to_string(), "action": "skipped(已存在, --force 覆盖)"}));
        return Ok(());
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, content)?;
    done.push(json!({"file": path.display().to_string(), "action": "written", "chars": content.chars().count()}));
    Ok(())
}

pub fn cmd_ai(cli: &Cli, c: &AiCmd) -> Result<i32> {
    let exe = std::env::current_exe()?.display().to_string();
    let exe = exe.trim_start_matches(r"\\?\").to_string();
    match c {
        AiCmd::Agents => {
            print!("{}", agents_md(&exe));
            Ok(0)
        }
        AiCmd::Init { dir, force } => {
            std::fs::create_dir_all(dir)?;
            let mut done = Vec::new();
            let agents = agents_md(&exe);
            write_if(&dir.join("AGENTS.md"), &agents, *force, &mut done)?;
            // 各工具的入口只指向正本, 避免多份内容不同步
            write_if(&dir.join("CLAUDE.md"), "@AGENTS.md\n", *force, &mut done)?;
            write_if(&dir.join("GEMINI.md"), "@AGENTS.md\n", *force, &mut done)?;
            write_if(&dir.join(".cursor/rules/wowsmod.mdc"), "---\ndescription: wowsmod 改模工作区规则\nalwaysApply: true\n---\n本工作区规则见根目录 AGENTS.md(必读, 很短)。细节用 `wowsmod help <节点id>` 沿技能树按需读取(从 root 开始)。\n", *force, &mut done)?;
            write_if(&dir.join(".github/copilot-instructions.md"), "本工作区规则见根目录 AGENTS.md(必读, 很短)。细节用 `wowsmod help <节点id>` 沿技能树按需读取(从 root 开始)。\n", *force, &mut done)?;
            let mut args = vec![json!("mcp")];
            if let Some(g) = &cli.game {
                args = vec![json!("--game"), json!(g.display().to_string()), json!("mcp")];
            }
            let mcp = serde_json::to_string_pretty(&json!({"mcpServers": {"wowsmod": {"command": exe, "args": args}}}))? + "\n";
            write_if(&dir.join(".cursor/mcp.json"), &mcp, *force, &mut done)?;
            write_if(&dir.join(".mcp.json"), &mcp, *force, &mut done)?;
            let codex = format!("[mcp_servers.wowsmod]\ncommand = {:?}\nargs = [\"mcp\"]\n", exe);
            let v = json!({"ok": true, "dir": dir.display().to_string(), "files": done, "agents_md_chars": agents.chars().count(),
                "other_tools": {"codex": format!("Codex 读 AGENTS.md; MCP 加到 ~/.codex/config.toml:\n{codex}"), "generic": format!("任何支持 MCP 的工具: 命令 {exe} 参数 mcp(stdio)")}});
            guide::stash(&v);
            if cli.json {
                emit(cli, &v);
            } else {
                for f in &done {
                    println!("{} {}", f["action"].as_str().unwrap_or(""), f["file"].as_str().unwrap_or(""));
                }
                println!("AGENTS.md {} 字。Codex 的 MCP 配置(全局, 未自动写):\n{codex}", agents.chars().count());
            }
            Ok(0)
        }
    }
}
