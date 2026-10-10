//! M5/M6: `wowsmod preview` 本地服务 + CDP 无头出图。
//! 静态前端来自 preview_web/(M5 构建产物); API 按 preview_app/API.md v1。
pub mod cdp;
pub mod http;
pub mod state;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Result};
use clap::Args;
use serde_json::{json, Value};

use crate::{emit, guide, Cli};

#[derive(Args, Clone, Debug)]
pub struct PreviewArgs {
    /// mod 项目目录
    pub project: PathBuf,
    /// 端口(被占用则 +1 重试, 最多 20 次)
    #[arg(long, default_value_t = 8960)]
    pub port: u16,
    /// 不自动打开系统浏览器
    #[arg(long)]
    pub no_open: bool,
}

pub fn cmd_preview(cli: &Cli, a: &PreviewArgs) -> Result<i32> {
    let st = Arc::new(state::PreviewState::open(cli, &a.project)?);
    let (port, server) = http::bind(a.port)?;
    let url = format!("http://127.0.0.1:{port}/");
    let v = json!({"ok": true, "url": url, "port": port, "project": a.project.display().to_string(), "ship": st.ship, "head": st.head_id()});
    guide::stash(&v);
    if cli.json {
        emit(cli, &v);
    } else {
        println!("预览服务: {url}");
        println!("项目: {}  船: {}  head: {}", a.project.display(), st.ship, st.head_id().unwrap_or_else(|| "null".into()));
        println!("Ctrl+C 退出。");
    }
    if !a.no_open {
        let _ = open_browser(&url);
    }
    http::serve(server, st)?;
    Ok(0)
}

fn open_browser(url: &str) -> Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    Ok(())
}

/// 供 render / 改动命令调用: 起内嵌服务(随机端口), 用 CDP 出图, 返回 PNG 字节与 meta。
pub fn render_pngs(cli: &Cli, project: &std::path::Path, queries: &[(String, String)], browser: Option<&std::path::Path>, size: (u32, u32)) -> Result<Vec<(Vec<u8>, Value)>> {
    // 先确认浏览器, 避免为 browser_not_found 白开重型 PreviewState
    if cdp::find_browser(browser).is_none() {
        bail!("browser_not_found");
    }
    let st = Arc::new(state::PreviewState::open(cli, project)?);
    let (port, server) = http::bind(0)?;
    let url = format!("http://127.0.0.1:{port}/");
    let st2 = Arc::clone(&st);
    let handle = std::thread::spawn(move || {
        let _ = http::serve(server, st2);
    });
    // 等端口可连
    for _ in 0..50 {
        if std::net::TcpStream::connect(format!("127.0.0.1:{port}")).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let r = match cdp::render_session(&url, queries, browser, size) {
        Ok(v) => Ok(v),
        Err(e) => {
            let msg = format!("{e:#}");
            if msg.contains("browser_not_found") {
                Err(e)
            } else {
                // 原生 CDP 失败时回退到验收用的 cdp.py
                eprintln!("提示: 原生 CDP 失败({msg}), 尝试 cdp.py…");
                cdp::render_via_python(&url, queries, browser)
            }
        }
    };
    st.shutdown();
    // 多次唤醒 accept 循环, 避免 join 卡死
    for _ in 0..5 {
        let _ = std::net::TcpStream::connect(format!("127.0.0.1:{port}"));
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let _ = handle.join();
    r
}

pub fn browser_not_found_value(preview_url: Option<&str>) -> Value {
    json!({
        "ok": false,
        "error": {"code": "browser_not_found", "message": "需要 Microsoft Edge 或 Google Chrome；可用 --browser 指定路径"},
        "preview_url": preview_url,
        "hint": "安装 Edge/Chrome 后重试, 或 wowsmod preview <项目> 在浏览器里手动查看"
    })
}

pub fn ensure_steps_json(project: &std::path::Path, ship: &str) -> Result<()> {
    let p = project.join("edits/steps.json");
    if p.is_file() {
        return Ok(());
    }
    // 若有种子(勃艮第 deepsea 类), 拷一份; 否则不强制
    if let Some(seed) = state::find_seed(ship) {
        let src = seed.join("steps.json");
        if src.is_file() {
            std::fs::create_dir_all(project.join("edits"))?;
            std::fs::copy(&src, &p)?;
        }
    }
    Ok(())
}


#[derive(Debug)]
pub struct BrowserMissing;
impl std::fmt::Display for BrowserMissing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "browser_not_found: 需要 Microsoft Edge 或 Google Chrome；可用 --browser 指定路径")
    }
}
impl std::error::Error for BrowserMissing {}
