//! M5/M6: `wowsmod preview` 本地服务 + CDP 无头出图。
//! 静态前端来自 preview_web/(M5 构建产物); API 按 preview_app/API.md v1。
pub mod cdp;
pub mod http;
pub mod state;
pub mod warm;

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
    let dbg = std::env::var("WOWSMOD_CDP_DEBUG").is_ok();
    let t0 = std::time::Instant::now();
    macro_rules! step { ($($a:tt)*) => { if dbg { eprintln!("[cdp {:.1}s] {}", t0.elapsed().as_secs_f32(), format!($($a)*)); } } }
    step!("open state");
    let st = Arc::new(state::PreviewState::open(cli, project)?);
    step!("state ok");
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
    step!("serve {url}");
    let r = match cdp::render_session(&url, queries, browser, size) {
        Ok(v) => Ok(v),
        Err(e) => {
            let msg = format!("{e:#}");
            // 超时/参数错/页面报错时回退 cdp.py 只会再等一轮, 直接报错; 只有协议层面的问题才回退
            let no_fallback = ["browser_not_found", "render_timeout", "cdp_timeout", "bad_param", "render_failed", "page_crashed"];
            if no_fallback.iter().any(|k| msg.contains(k)) {
                Err(e)
            } else {
                // 原生 CDP 失败时回退到验收用的 cdp.py
                eprintln!("提示: 原生 CDP 失败({msg}), 尝试 cdp.py…");
                cdp::render_via_python(&url, queries, browser)
            }
        }
    };
    step!("session done ok={}", r.is_ok());
    st.shutdown();
    // 唤醒 accept 循环: tiny_http 只在收到完整 HTTP 请求后才从 incoming_requests 返回,
    // 光 connect 不发请求会让 join 永久卡住(E2E 实测: CDP 出图 12s 完成后卡死)。所以发真实请求。
    for _ in 0..3 {
        if let Ok(mut s) = std::net::TcpStream::connect(format!("127.0.0.1:{port}")) {
            use std::io::Write;
            let _ = s.set_write_timeout(Some(std::time::Duration::from_millis(500)));
            let _ = s.write_all(b"GET /__wake HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n");
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    step!("joining server");
    // 兜底: 最多等 3s, 仍未退出就放弃 join(线程随进程结束)
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = handle.join();
        let _ = tx.send(());
    });
    let joined = rx.recv_timeout(std::time::Duration::from_secs(3)).is_ok();
    step!("joined={joined}");
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

/// 以前会把 mock 种子(勃艮第"深海涂装"故事)的 steps.json 拷进真实项目, 导致预览时间线显示的是 mock 步骤
/// 而不是项目自己的改动(克利希娜项目即如此)。现在不再写项目: 没有 steps.json 时按 edits 每条一步显示;
/// 已被拷进去的种子文件由 state::group_steps 识别并忽略。
pub fn ensure_steps_json(_project: &std::path::Path, _ship: &str) -> Result<()> {
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
