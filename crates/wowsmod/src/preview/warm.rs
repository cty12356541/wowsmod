//! 常驻出图实例(`wowsmod render --warm`): 预览服务 + 无头浏览器 + 已加载的 `?render=wait` 页面常驻,
//! 后续 `wowsmod render` / MCP `wowsmod_render` 通过本机 HTTP 控制端点(/__wm/*)出图, 每张只付渲染本身的时间。
//!
//! - 状态文件: `<项目>/.wowsmod/render_warm.json`(pid/端口/令牌/就绪), 另在 `%TEMP%/wowsmod-render-warm/` 登记一份供 `render status` 列出全部。
//! - 项目改动: 每次出图前比对项目指纹(路径+大小+mtime), 变了就 PreviewState::reload(只重读变化的文件)
//!   + 前端 `__renderReload()`(几何按哈希、贴图按内容哈希复用, 只拉新哈希)。
//! - 超时与自愈: 守护进程内每步有截止时间; 出图超时/浏览器崩溃 → 重启浏览器页面重试一次;
//!   客户端连不上/等不到守护进程 → 杀掉并重启常驻实例重试一次。错误带 code 与下一步建议。
//! - 空闲超时自动退出(默认 15 分钟, `--idle <分钟>` 或 WOWSMOD_RENDER_IDLE_MIN; 0 = 不退出)。
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tiny_http::Method;

use super::cdp::{self, Page};
use super::http;
use super::state::PreviewState;
use crate::Cli;

pub const DEFAULT_IDLE_MIN: u64 = 15;
/// 守护进程预热时的首张图: 同时加载 base 与 head 的贴图(compare 常用)
const WARMUP_QUERY: &str = "view=fq&rev=head&compare=split&base=base&w=640&h=360&wm=0";

pub fn idle_minutes(arg: Option<u64>) -> u64 {
    arg.or_else(|| std::env::var("WOWSMOD_RENDER_IDLE_MIN").ok().and_then(|s| s.parse().ok())).unwrap_or(DEFAULT_IDLE_MIN)
}

fn abs(p: &Path) -> PathBuf {
    let a = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    PathBuf::from(a.display().to_string().trim_start_matches(r"\\?\"))
}

pub fn state_path(project: &Path) -> PathBuf {
    abs(project).join(".wowsmod").join("render_warm.json")
}
fn log_path(project: &Path) -> PathBuf {
    abs(project).join(".wowsmod").join("render_warm.log")
}
fn registry_dir() -> PathBuf {
    std::env::temp_dir().join("wowsmod-render-warm")
}

fn write_json_atomic(p: &Path, v: &Value) {
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let tmp = p.with_extension("json.tmp");
    if std::fs::write(&tmp, serde_json::to_vec_pretty(v).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&tmp, p);
    }
}

pub fn read_state(project: &Path) -> Option<Value> {
    std::fs::read(state_path(project)).ok().and_then(|d| serde_json::from_slice(&d).ok())
}

fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ---------------------------------------------------------------- 客户端

/// 最小 HTTP/1.1 客户端(Connection: close, 读到 EOF), 全程带超时
fn http_call(port: u16, method: &str, path: &str, token: &str, body: Option<&[u8]>, timeout: Duration) -> Result<(u16, Vec<u8>)> {
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|e| anyhow!("warm_unreachable: 连不上常驻实例 127.0.0.1:{port} ({e})"))?;
    s.set_write_timeout(Some(Duration::from_secs(10)))?;
    let b = body.unwrap_or(&[]);
    let head = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-WM-Token: {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", b.len());
    s.write_all(head.as_bytes())?;
    s.write_all(b)?;
    let deadline = Instant::now() + timeout;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 65536];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            bail!("warm_timeout: 常驻实例 {}s 内没有响应 {path}", timeout.as_secs());
        }
        s.set_read_timeout(Some(left))?;
        match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => bail!("warm_timeout: 常驻实例 {}s 内没有响应 {path}", timeout.as_secs()),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                if buf.is_empty() {
                    bail!("warm_unreachable: 读取失败 {e}");
                }
                break;
            }
        }
    }
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| anyhow!("warm_unreachable: 响应不完整"))?;
    let head = String::from_utf8_lossy(&buf[..split]).to_string();
    let code: u16 = head.split_whitespace().nth(1).and_then(|x| x.parse().ok()).unwrap_or(0);
    let mut body = buf[split + 4..].to_vec();
    if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        body = dechunk(&body);
    }
    Ok((code, body))
}

fn dechunk(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let Some(e) = b[i..].windows(2).position(|w| w == b"\r\n") else { break };
        let n = usize::from_str_radix(String::from_utf8_lossy(&b[i..i + e]).split(';').next().unwrap_or("0").trim(), 16).unwrap_or(0);
        i += e + 2;
        if n == 0 || i + n > b.len() {
            break;
        }
        out.extend_from_slice(&b[i..i + n]);
        i += n + 2;
    }
    out
}

fn conn(st: &Value) -> Option<(u16, String)> {
    Some((st["port"].as_u64()? as u16, st["token"].as_str()?.to_string()))
}

/// 常驻实例状态。running=false 时附 reason; 状态文件残留(进程已死)会被清理。
pub fn status(project: &Path) -> Value {
    let Some(st) = read_state(project) else {
        return json!({"running": false, "ready": false, "project": abs(project).display().to_string(), "reason": "没有常驻实例"});
    };
    if st["state"] == "failed" {
        return json!({"running": false, "ready": false, "project": st["project"], "reason": "上次预热失败", "error": st["error"], "log": log_path(project).display().to_string()});
    }
    if st["state"] == "starting" {
        let alive = st["pid"].as_u64().map(|p| pid_alive(p as u32)).unwrap_or(false);
        if alive {
            return json!({"running": true, "ready": false, "state": "starting", "pid": st["pid"], "project": st["project"], "since_unix": st["started_unix"]});
        }
        let _ = std::fs::remove_file(state_path(project));
        return json!({"running": false, "ready": false, "reason": "预热进程已退出(见日志)", "log": log_path(project).display().to_string()});
    }
    let Some((port, token)) = conn(&st) else { return json!({"running": false, "ready": false, "reason": "状态文件损坏"}) };
    match http_call(port, "GET", "/__wm/status", &token, None, Duration::from_secs(5)) {
        Ok((200, b)) => serde_json::from_slice::<Value>(&b).unwrap_or(json!({"running": true})),
        Ok((code, _)) => json!({"running": false, "ready": false, "reason": format!("端口 {port} 上不是本项目的常驻实例(HTTP {code})")}),
        Err(e) => {
            let msg = e.to_string();
            if msg.starts_with("warm_unreachable") && !st["pid"].as_u64().map(|p| pid_alive(p as u32)).unwrap_or(false) {
                let _ = std::fs::remove_file(state_path(project));
                json!({"running": false, "ready": false, "reason": "常驻实例已退出(清理了残留状态文件)"})
            } else {
                json!({"running": true, "ready": false, "hung": true, "pid": st["pid"], "reason": msg})
            }
        }
    }
}

/// 列出本机全部常驻实例(登记目录)
pub fn status_all() -> Vec<Value> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(registry_dir()) {
        for e in rd.flatten() {
            if let Some(v) = std::fs::read(e.path()).ok().and_then(|d| serde_json::from_slice::<Value>(&d).ok()) {
                if let Some(p) = v["project"].as_str() {
                    let s = status(Path::new(p));
                    if s["running"] == true {
                        out.push(s);
                    } else {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
        }
    }
    out
}

pub fn pid_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let o = std::process::Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"]).output();
        o.map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\""))).unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
}

fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill").args(["/F", "/T", "/PID", &pid.to_string()]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
}

/// 让本进程的标准句柄不被子进程继承: 否则后台常驻进程会一直占着调用方(终端/AI 工具/MCP 客户端)的输出管道,
/// 调用方等不到 EOF 就会挂住。
#[cfg(windows)]
fn no_inherit_std_handles() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(n: u32) -> isize;
        fn SetHandleInformation(h: isize, mask: u32, flags: u32) -> i32;
    }
    for n in [-10i32 as u32, -11i32 as u32, -12i32 as u32] {
        // SAFETY: 只修改本进程标准句柄的继承标志; 无效句柄时调用失败无副作用
        unsafe {
            let h = GetStdHandle(n);
            if h != 0 && h != -1 {
                SetHandleInformation(h, 1, 0);
            }
        }
    }
}

fn spawn_daemon(cli: &Cli, project: &Path, idle_min: u64, browser: Option<&Path>) -> Result<u32> {
    let exe = std::env::current_exe()?;
    let log = log_path(project);
    if let Some(d) = log.parent() {
        std::fs::create_dir_all(d)?;
    }
    let lf = std::fs::File::create(&log)?;
    let lf2 = lf.try_clone()?;
    let mut c = std::process::Command::new(exe);
    if let Some(g) = &cli.game {
        c.arg("--game").arg(g);
    }
    c.args(["render", "serve"]).arg(abs(project)).arg("--idle").arg(idle_min.to_string());
    if let Some(b) = browser {
        c.arg("--browser").arg(b);
    }
    c.stdin(std::process::Stdio::null()).stdout(lf).stderr(lf2);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        no_inherit_std_handles();
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        // 先尝试脱离调用方的作业对象(否则调用方结束时常驻进程会被一起杀掉); 不允许脱离时退回普通方式
        c.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB);
        match c.spawn() {
            Ok(ch) => return Ok(ch.id()),
            Err(_) => {
                c.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
            }
        }
    }
    let ch = c.spawn().context("启动常驻出图进程")?;
    Ok(ch.id())
}

fn log_tail(project: &Path) -> String {
    let t = std::fs::read_to_string(log_path(project)).unwrap_or_default();
    let lines: Vec<&str> = t.lines().collect();
    lines[lines.len().saturating_sub(8)..].join("\n")
}

/// 确保常驻实例在运行并就绪(必要时启动)。wait=false 时只负责启动, 立即返回。
pub fn ensure_started(cli: &Cli, project: &Path, idle_min: u64, browser: Option<&Path>, wait: bool, timeout: Duration) -> Result<Value> {
    let t0 = Instant::now();
    let mut s = status(project);
    let mut spawned = false;
    if s["running"] == true && s["hung"] == true {
        // 挂死的实例: 杀掉重来
        if let Some(p) = s["pid"].as_u64() {
            kill_tree(p as u32);
        }
        let _ = std::fs::remove_file(state_path(project));
        s = json!({"running": false});
    }
    if s["running"] != true {
        crate::modproj::Project::open(project).context("打开项目")?;
        let pid = spawn_daemon(cli, project, idle_min, browser)?;
        write_json_atomic(&state_path(project), &json!({"state": "starting", "pid": pid, "project": abs(project).display().to_string(), "started_unix": now_unix()}));
        spawned = true;
    }
    if !wait {
        return Ok(json!({"running": true, "ready": s["ready"] == true, "spawned": spawned}));
    }
    loop {
        let s = status(project);
        if s["ready"] == true {
            let mut v = s;
            v["spawned"] = json!(spawned);
            v["waited_s"] = json!(t0.elapsed().as_secs_f32());
            return Ok(v);
        }
        if s["running"] != true {
            bail!("warm_start_failed: 常驻实例启动失败: {}\n日志 {}:\n{}", s["error"].as_str().or(s["reason"].as_str()).unwrap_or(""), log_path(project).display(), log_tail(project));
        }
        if t0.elapsed() > timeout {
            if let Some(p) = read_state(project).and_then(|v| v["pid"].as_u64()) {
                kill_tree(p as u32);
            }
            let _ = std::fs::remove_file(state_path(project));
            bail!("warm_start_failed: 常驻实例 {}s 内没有就绪, 已终止。日志 {}:\n{}", timeout.as_secs(), log_path(project).display(), log_tail(project));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub fn stop(project: &Path) -> Value {
    let Some(st) = read_state(project) else { return json!({"ok": true, "stopped": false, "reason": "没有常驻实例"}) };
    let pid = st["pid"].as_u64().map(|p| p as u32);
    let mut graceful = false;
    if let Some((port, token)) = conn(&st) {
        graceful = matches!(http_call(port, "POST", "/__wm/stop", &token, None, Duration::from_secs(5)), Ok((200, _)));
    }
    let t0 = Instant::now();
    while graceful && state_path(project).exists() && t0.elapsed() < Duration::from_secs(8) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let forced = state_path(project).exists();
    if forced {
        if let Some(p) = pid {
            kill_tree(p);
        }
        if let Some(bp) = st["browser_pid"].as_u64() {
            kill_tree(bp as u32);
        }
        let _ = std::fs::remove_file(state_path(project));
    }
    if let Some(p) = pid {
        let _ = std::fs::remove_file(registry_dir().join(format!("{p}.json")));
    }
    json!({"ok": true, "stopped": true, "pid": pid, "forced": forced})
}

/// 通过常驻实例出一张图。返回 (PNG, meta)。
pub fn render_via(project: &Path, query: &str, timeout_s: u64) -> Result<(Vec<u8>, Value)> {
    let st = read_state(project).ok_or_else(|| anyhow!("warm_unreachable: 没有常驻实例"))?;
    let (port, token) = conn(&st).ok_or_else(|| anyhow!("warm_unreachable: 常驻实例未就绪"))?;
    let body = json!({"query": query, "timeout_s": timeout_s}).to_string();
    // 守护进程内部最多: 增量重载 + 出图 + 重启浏览器重试一次
    let (code, b) = http_call(port, "POST", "/__wm/render", &token, Some(body.as_bytes()), Duration::from_secs(timeout_s * 2 + 90))?;
    let v: Value = serde_json::from_slice(&b).map_err(|_| anyhow!("warm_unreachable: 常驻实例返回了非 JSON (HTTP {code})"))?;
    if v["ok"] != true {
        let c = v["error"]["code"].as_str().unwrap_or("render_failed");
        bail!("{c}: {}", v["error"]["message"].as_str().unwrap_or(""));
    }
    let png = wowsmod_formats::mfm::b64_decode(v["png_b64"].as_str().unwrap_or("")).ok_or_else(|| anyhow!("render_failed: PNG 解码失败"))?;
    let mut meta = v;
    if let Some(o) = meta.as_object_mut() {
        o.remove("png_b64");
    }
    Ok((png, meta))
}

/// 出图(常驻): 实例挂死/退出时重启常驻实例重试一次
pub fn render_with_heal(cli: &Cli, project: &Path, query: &str, timeout_s: u64, idle_min: u64, browser: Option<&Path>) -> Result<(Vec<u8>, Value)> {
    match render_via(project, query, timeout_s) {
        Ok(x) => Ok(x),
        Err(e) => {
            let m = e.to_string();
            let retry = m.starts_with("warm_unreachable") || m.starts_with("warm_timeout");
            if !retry {
                return Err(e);
            }
            eprintln!("提示: 常驻实例无响应({m}), 正在重启常驻实例并重试一次…");
            let _ = stop(project);
            ensure_started(cli, project, idle_min, browser, true, Duration::from_secs(240))?;
            let (png, mut meta) = render_via(project, query, timeout_s).map_err(|e2| anyhow!("{e2:#}(已重启常驻实例重试一次仍失败)"))?;
            meta["warm_restarted"] = json!(true);
            Ok((png, meta))
        }
    }
}

/// 错误码 → 下一步建议
pub fn next_for(code: &str, project: &Path) -> Vec<String> {
    let p = project.display();
    match code {
        "render_timeout" => vec![format!("wowsmod render status {p} 看实例状态"), format!("wowsmod render stop {p} 后再 wowsmod render --warm {p}"), "加大 --timeout, 或先用小尺寸 --size 800x450".into(), "设 WOWSMOD_CDP_DEBUG=1 看每步耗时".into()],
        "browser_crashed" | "page_crashed" | "cdp_closed" => vec![format!("wowsmod render stop {p} 后再 wowsmod render --warm {p}"), "显卡驱动问题时可设 WOWSMOD_BROWSER 指向 Chrome, 或 --engine soft".into()],
        "warm_start_failed" => vec![format!("查看日志 {}", log_path(project).display()), format!("wowsmod render {p} --cold 先确认冷启动能出图"), "wowsmod doctor".into()],
        "bad_param" => vec!["检查 --query 参数(见 wowsmod help render)".into()],
        _ => vec![format!("wowsmod render status {p}"), "wowsmod help render".into()],
    }
}

// ---------------------------------------------------------------- 守护进程

struct Warm {
    st: Arc<PreviewState>,
    base_url: String,
    exe: PathBuf,
    page: Mutex<Option<Page>>,
    fp: Mutex<u64>,
    last_used: Mutex<Instant>,
    started: Instant,
    started_unix: u64,
    renders: AtomicU64,
    reloads: AtomicU64,
    restarts: AtomicU64,
    stop: AtomicBool,
    idle: Duration,
    token: String,
    port: u16,
    project: PathBuf,
    timings: Mutex<Value>,
    last: Mutex<Value>,
    ready: AtomicBool,
    sw: AtomicBool,
}

fn code_of(msg: &str) -> &'static str {
    let head = msg.split(':').next().unwrap_or("");
    match head {
        "render_timeout" | "cdp_timeout" => "render_timeout",
        "page_crashed" | "cdp_closed" | "browser_exited" | "browser_launch_timeout" | "cdp_connect" => "browser_crashed",
        "bad_param" => "bad_param",
        "browser_not_found" => "browser_not_found",
        _ if msg.contains("browser_not_found") => "browser_not_found",
        _ => "render_failed",
    }
}

impl Warm {
    fn open_page(&self) -> Result<Page> {
        let sw = self.sw.load(Ordering::SeqCst);
        let mut p = Page::open(&self.exe, &self.base_url, (1600, 900), sw, "warm")?;
        // 首张图把几何与 base/head 贴图都拉进页面
        match p.render("warmup", WARMUP_QUERY, 180) {
            Ok(_) => Ok(p),
            Err(e) if e.to_string().contains("webgl_retry") && !sw => {
                p.close();
                self.sw.store(true, Ordering::SeqCst);
                let mut p = Page::open(&self.exe, &self.base_url, (1600, 900), true, "warm")?;
                p.render("warmup", WARMUP_QUERY, 180)?;
                Ok(p)
            }
            Err(e) => Err(e),
        }
    }

    fn status_json(&self) -> Value {
        let idle_s = self.last_used.lock().unwrap().elapsed().as_secs();
        let bpid = self.page.try_lock().ok().and_then(|g| g.as_ref().and_then(|p| p.browser_pid()));
        json!({"running": true, "ready": self.ready.load(Ordering::SeqCst), "pid": std::process::id(), "port": self.port, "project": self.project.display().to_string(),
            "browser_pid": bpid, "busy": self.page.try_lock().is_err(), "uptime_s": self.started.elapsed().as_secs(), "started_unix": self.started_unix,
            "idle_s": idle_s, "idle_limit_s": self.idle.as_secs(), "idle_left_s": if self.idle.is_zero() { Value::Null } else { json!(self.idle.as_secs().saturating_sub(idle_s)) },
            "renders": self.renders.load(Ordering::SeqCst), "reloads": self.reloads.load(Ordering::SeqCst), "browser_restarts": self.restarts.load(Ordering::SeqCst),
            "swiftshader": self.sw.load(Ordering::SeqCst), "warmup": self.timings.lock().unwrap().clone(), "last": self.last.lock().unwrap().clone()})
    }

    fn render(&self, q: &str, timeout_s: u64) -> Result<(Vec<u8>, Value)> {
        let t0 = Instant::now();
        let mut g = self.page.lock().unwrap();
        *self.last_used.lock().unwrap() = Instant::now();
        let mut info = json!({});
        // 1) 浏览器活着吗
        if !g.as_mut().map(|p| p.healthy()).unwrap_or(false) {
            if let Some(p) = g.take() {
                p.close();
            }
            let tb = Instant::now();
            *g = Some(self.open_page()?);
            self.restarts.fetch_add(1, Ordering::SeqCst);
            info["browser_restarted_s"] = json!(tb.elapsed().as_secs_f32());
        }
        // 2) 项目改了吗 → 增量重载
        let tf = Instant::now();
        let fp = self.st.fingerprint();
        info["fingerprint_ms"] = json!(tf.elapsed().as_millis() as u64);
        if fp != *self.fp.lock().unwrap() {
            let tr = Instant::now();
            let back = self.st.reload()?;
            let front = g.as_mut().unwrap().reload_project(120);
            let front = match front {
                Ok(v) => v,
                Err(e) => {
                    // 页面重载失败: 整个页面重开
                    eprintln!("[warm] 前端重载失败({e:#}), 重开页面");
                    if let Some(p) = g.take() {
                        p.close();
                    }
                    *g = Some(self.open_page()?);
                    self.restarts.fetch_add(1, Ordering::SeqCst);
                    json!({"mode": "reopen"})
                }
            };
            *self.fp.lock().unwrap() = fp;
            self.reloads.fetch_add(1, Ordering::SeqCst);
            info["reload"] = json!({"backend": back, "frontend": front, "seconds": tr.elapsed().as_secs_f32()});
        }
        // 3) 出图; 超时/崩溃 → 重开页面重试一次
        let tr = Instant::now();
        let r = g.as_mut().unwrap().render("main", q, timeout_s);
        let (png, meta) = match r {
            Ok(x) => x,
            Err(e) => {
                let m = format!("{e:#}");
                let code = code_of(&m);
                if code == "bad_param" {
                    return Err(e);
                }
                eprintln!("[warm] 出图失败({m}), 重启浏览器页面重试一次");
                if let Some(p) = g.take() {
                    p.close();
                }
                let tb = Instant::now();
                *g = Some(self.open_page().map_err(|e2| anyhow!("{}: 重启浏览器失败: {e2:#}(首次错误: {m})", code_of(&format!("{e2:#}"))))?);
                self.restarts.fetch_add(1, Ordering::SeqCst);
                info["browser_restarted_s"] = json!(tb.elapsed().as_secs_f32());
                info["first_error"] = json!(m);
                g.as_mut().unwrap().render("main", q, timeout_s).map_err(|e2| anyhow!("{e2:#}(已重启浏览器重试一次仍失败; 首次错误: {m})"))?
            }
        };
        info["render_s"] = json!(tr.elapsed().as_secs_f32());
        info["seconds"] = json!(t0.elapsed().as_secs_f32());
        self.renders.fetch_add(1, Ordering::SeqCst);
        *self.last_used.lock().unwrap() = Instant::now();
        *self.last.lock().unwrap() = json!({"query": q, "seconds": info["seconds"], "unix": now_unix()});
        Ok((png, json!({"meta": meta, "timing": info})))
    }
}

fn read_body(req: &mut tiny_http::Request) -> String {
    let mut s = String::new();
    let _ = req.as_reader().take(1 << 20).read_to_string(&mut s);
    s
}

/// `wowsmod render serve <项目>`: 前台运行常驻实例(通常由 `render --warm` 在后台拉起)
pub fn serve(cli: &Cli, project: &Path, idle_min: u64, browser: Option<&Path>) -> Result<i32> {
    let project = abs(project);
    let sp = state_path(&project);
    let t0 = Instant::now();
    let fail = |e: &anyhow::Error| {
        write_json_atomic(&sp, &json!({"state": "failed", "pid": std::process::id(), "project": project.display().to_string(), "error": format!("{e:#}"), "unix": now_unix()}));
    };
    eprintln!("[warm] 启动 pid={} 项目 {}", std::process::id(), project.display());
    write_json_atomic(&sp, &json!({"state": "starting", "pid": std::process::id(), "project": project.display().to_string(), "started_unix": now_unix()}));
    let exe = match cdp::find_browser(browser) {
        Some(e) => e,
        None => {
            let e = anyhow!("browser_not_found: 需要 Microsoft Edge 或 Google Chrome");
            fail(&e);
            return Err(e);
        }
    };
    let st = match PreviewState::open(cli, &project) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            fail(&e);
            return Err(e);
        }
    };
    let state_s = t0.elapsed().as_secs_f32();
    eprintln!("[warm] 项目状态 {state_s:.1}s");
    let (port, server) = http::bind(0)?;
    let token = {
        let h = Sha256::digest(format!("{}|{:?}|{}", std::process::id(), Instant::now(), project.display()).as_bytes());
        h.iter().take(12).map(|b| format!("{b:02x}")).collect::<String>()
    };
    let w = Arc::new(Warm {
        st: Arc::clone(&st),
        base_url: format!("http://127.0.0.1:{port}/"),
        exe,
        page: Mutex::new(None),
        fp: Mutex::new(st.fingerprint()),
        last_used: Mutex::new(Instant::now()),
        started: t0,
        started_unix: now_unix(),
        renders: AtomicU64::new(0),
        reloads: AtomicU64::new(0),
        restarts: AtomicU64::new(0),
        stop: AtomicBool::new(false),
        idle: Duration::from_secs(idle_min * 60),
        token: token.clone(),
        port,
        project: project.clone(),
        timings: Mutex::new(json!({"state_s": state_s})),
        last: Mutex::new(Value::Null),
        ready: AtomicBool::new(false),
        sw: AtomicBool::new(false),
    });
    let w2 = Arc::clone(&w);
    let ctl: http::Ctl = Arc::new(move |method: &Method, url: &str, req: &mut tiny_http::Request| {
        let tok_ok = req.headers().iter().any(|h| h.field.equiv("X-WM-Token") && h.value.as_str() == w2.token);
        if !tok_ok {
            return Some(http::json_resp(403, &json!({"ok": false, "error": {"code": "forbidden", "message": "令牌不对"}})));
        }
        let path = url.split('?').next().unwrap_or("");
        Some(match (method, path) {
            (Method::Get, "/__wm/status") => http::json_resp(200, &w2.status_json()),
            (Method::Post, "/__wm/stop") => {
                w2.stop.store(true, Ordering::SeqCst);
                http::json_resp(200, &json!({"ok": true}))
            }
            (Method::Post, "/__wm/render") => {
                let body: Value = serde_json::from_str(&read_body(req)).unwrap_or(json!({}));
                let q = body["query"].as_str().unwrap_or("").to_string();
                let to = body["timeout_s"].as_u64().unwrap_or(60).clamp(5, 600);
                if !w2.ready.load(Ordering::SeqCst) {
                    return Some(http::json_resp(503, &json!({"ok": false, "error": {"code": "warm_not_ready", "message": "常驻实例还在预热"}})));
                }
                match w2.render(&q, to) {
                    Ok((png, mut v)) => {
                        v["ok"] = json!(true);
                        v["warm"] = json!(true);
                        v["png_b64"] = json!(wowsmod_formats::mfm::b64_encode(&png));
                        http::json_resp(200, &v)
                    }
                    Err(e) => {
                        let m = format!("{e:#}");
                        http::json_resp(500, &json!({"ok": false, "error": {"code": code_of(&m), "message": m}}))
                    }
                }
            }
            _ => return None,
        })
    });
    let st2 = Arc::clone(&st);
    std::thread::spawn(move || {
        let _ = http::serve_with(server, st2, Some(ctl));
    });
    // 浏览器 + 页面 + 首张图
    let tb = Instant::now();
    match w.open_page() {
        Ok(p) => {
            let open_s = p.open_s;
            *w.page.lock().unwrap() = Some(p);
            let mut t = w.timings.lock().unwrap();
            t["browser_page_s"] = json!(open_s);
            t["first_render_s"] = json!(tb.elapsed().as_secs_f32() - open_s);
            t["total_s"] = json!(t0.elapsed().as_secs_f32());
        }
        Err(e) => {
            fail(&e);
            return Err(e);
        }
    }
    w.ready.store(true, Ordering::SeqCst);
    *w.last_used.lock().unwrap() = Instant::now();
    let bpid = w.page.lock().unwrap().as_ref().and_then(|p| p.browser_pid());
    let reg = json!({"state": "ready", "pid": std::process::id(), "port": port, "token": token, "project": project.display().to_string(), "browser_pid": bpid,
        "started_unix": w.started_unix, "idle_min": idle_min, "version": env!("CARGO_PKG_VERSION"), "warmup": w.timings.lock().unwrap().clone()});
    write_json_atomic(&sp, &reg);
    let _ = std::fs::create_dir_all(registry_dir());
    write_json_atomic(&registry_dir().join(format!("{}.json", std::process::id())), &json!({"project": project.display().to_string(), "pid": std::process::id()}));
    eprintln!("[warm] 就绪 {:.1}s port={port}", t0.elapsed().as_secs_f32());
    // 空闲/停止
    let reason = loop {
        std::thread::sleep(Duration::from_millis(250));
        if w.stop.load(Ordering::SeqCst) {
            break "stop";
        }
        if !w.idle.is_zero() && w.page.try_lock().is_ok() && w.last_used.lock().unwrap().elapsed() > w.idle {
            break "idle";
        }
        // 状态文件被删/被别的实例覆盖 → 退出(避免孤儿)
        match read_state(&project) {
            Some(v) if v["pid"].as_u64() == Some(std::process::id() as u64) => {}
            _ => break "state_file_gone",
        }
    };
    eprintln!("[warm] 退出: {reason}");
    if let Ok(mut g) = w.page.lock() {
        if let Some(p) = g.take() {
            p.close();
        }
    }
    if read_state(&project).and_then(|v| v["pid"].as_u64()) == Some(std::process::id() as u64) {
        let _ = std::fs::remove_file(&sp);
    }
    let _ = std::fs::remove_file(registry_dir().join(format!("{}.json", std::process::id())));
    std::process::exit(0);
}
