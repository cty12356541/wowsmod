//! 系统 Edge/Chrome 无头 + CDP 出图(对照 preview_app/tools/cdp.py 与 API.md §11.5)。
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use wowsmod_formats::mfm;

pub fn find_browser(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return p.is_file().then(|| p.to_path_buf());
    }
    if let Ok(e) = std::env::var("WOWSMOD_BROWSER") {
        let p = PathBuf::from(e);
        if p.is_file() {
            return Some(p);
        }
    }
    #[cfg(windows)]
    {
        for exe in ["msedge.exe", "chrome.exe"] {
            for hive in [winreg::enums::HKEY_LOCAL_MACHINE, winreg::enums::HKEY_CURRENT_USER] {
                if let Ok(k) = winreg::RegKey::predef(hive).open_subkey(format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{exe}")) {
                    if let Ok(p) = k.get_value::<String, _>("") {
                        let p = PathBuf::from(p.trim_matches('"'));
                        if p.is_file() {
                            return Some(p);
                        }
                    }
                }
            }
        }
        let cands = [
            std::env::var("ProgramFiles(x86)").unwrap_or_default() + r"\Microsoft\Edge\Application\msedge.exe",
            std::env::var("ProgramFiles").unwrap_or_default() + r"\Microsoft\Edge\Application\msedge.exe",
            std::env::var("ProgramFiles").unwrap_or_default() + r"\Google\Chrome\Application\chrome.exe",
            std::env::var("LOCALAPPDATA").unwrap_or_default() + r"\Google\Chrome\Application\chrome.exe",
        ];
        for c in cands {
            let p = PathBuf::from(c);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    #[cfg(not(windows))]
    {
        for c in ["microsoft-edge", "google-chrome", "chromium", "chromium-browser", "google-chrome-stable"] {
            if let Ok(p) = which(c) {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn which(name: &str) -> Result<PathBuf> {
    let o = Command::new("which").arg(name).output()?;
    if o.status.success() {
        let p = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    bail!("not found")
}

/// 无头浏览器进程(user-data-dir 在临时目录, 关闭时删除)
pub(crate) struct Browser {
    child: Child,
    user_dir: PathBuf,
    ws_url: String,
}
impl Browser {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

static LAUNCH_SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// tag 区分 user-data-dir: 冷启动 "", 常驻实例 "warm"(同一实例内磁盘缓存跨页面重载保留)
pub(crate) fn launch(exe: &Path, swiftshader: bool, tag: &str) -> Result<Browser> {
    let n = LAUNCH_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let user_dir = std::env::temp_dir().join(format!("wm_cdp{tag}_{}_{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&user_dir);
    std::fs::create_dir_all(&user_dir)?;
    let mut args = vec![
        "--remote-debugging-port=0".into(),
        format!("--user-data-dir={}", user_dir.display()),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-extensions".into(),
        "--hide-scrollbars".into(),
        "--mute-audio".into(),
        "--force-device-scale-factor=1".into(),
        "--disable-background-timer-throttling".into(),
        "--disable-renderer-backgrounding".into(),
        "--disable-backgrounding-occluded-windows".into(),
        "--disable-features=CalculateNativeWinOcclusion,msEdgeStartupBoost".into(),
        "--headless=new".into(),
    ];
    if swiftshader {
        args.push("--use-angle=swiftshader".into());
        args.push("--enable-unsafe-swiftshader".into());
    }
    args.push("about:blank".into());
    let mut child = Command::new(exe).args(&args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().context("启动浏览器")?;
    #[cfg(windows)]
    kill_on_exit::attach(&child);
    let port_file = user_dir.join("DevToolsActivePort");
    let t0 = Instant::now();
    loop {
        if t0.elapsed() > Duration::from_secs(20) {
            close(Browser { child, user_dir, ws_url: String::new() });
            bail!("browser_launch_timeout: 浏览器 20s 内没有写出 DevToolsActivePort");
        }
        if let Ok(Some(st)) = child.try_wait() {
            // Edge 有时把无头请求交给已在运行的实例后自己退出; 独立 user-data-dir 下不应发生
            let _ = std::fs::remove_dir_all(&user_dir);
            bail!("browser_exited: 浏览器启动后立即退出({st})");
        }
        if port_file.is_file() {
            let t = std::fs::read_to_string(&port_file)?;
            let mut lines = t.lines();
            if let (Some(port), Some(path)) = (lines.next(), lines.next()) {
                if !port.is_empty() && !path.is_empty() {
                    return Ok(Browser { child, user_dir, ws_url: format!("ws://127.0.0.1:{port}{path}") });
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub(crate) fn close(mut b: Browser) {
    let _ = b.child.kill();
    let _ = b.child.wait();
    #[cfg(windows)]
    {
        // 子进程(GPU/renderer)偶尔晚于主进程退出, 占着 user-data-dir
        let _ = Command::new("taskkill").args(["/F", "/T", "/PID", &b.child.id().to_string()]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    for _ in 0..20 {
        if std::fs::remove_dir_all(&b.user_dir).is_ok() || !b.user_dir.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

// ---------- 最小 WebSocket 客户端(每次读都带截止时间) ----------
struct Ws {
    s: TcpStream,
}
fn is_timeout(e: &std::io::Error) -> bool {
    matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)
}
impl Ws {
    fn connect(url: &str) -> Result<Self> {
        let u = url.strip_prefix("ws://").ok_or_else(|| anyhow!("只支持 ws://"))?;
        let (hostport, path) = u.split_once('/').map(|(h, p)| (h, format!("/{p}"))).unwrap_or((u, "/".into()));
        let (host, port) = hostport.split_once(':').unwrap_or((hostport, "80"));
        let addr: std::net::SocketAddr = format!("{host}:{port}").parse().context("ws 地址")?;
        let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(10)).context("cdp_connect: 连不上浏览器调试端口")?;
        s.set_nodelay(true).ok();
        s.set_read_timeout(Some(Duration::from_secs(10)))?;
        s.set_write_timeout(Some(Duration::from_secs(30)))?;
        let key = mfm::b64_encode(&rand16());
        let req = format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n");
        s.write_all(req.as_bytes())?;
        let mut hdr = Vec::new();
        let mut buf = [0u8; 1];
        while !hdr.windows(4).any(|w| w == b"\r\n\r\n") {
            s.read_exact(&mut buf).context("cdp_connect: websocket 握手无响应")?;
            hdr.push(buf[0]);
            if hdr.len() > 8192 {
                bail!("websocket handshake 过大");
            }
        }
        let status = String::from_utf8_lossy(&hdr);
        if !status.contains(" 101 ") {
            bail!("websocket handshake 失败: {}", status.lines().next().unwrap_or(""));
        }
        Ok(Ws { s })
    }
    fn send_text(&mut self, text: &str) -> Result<()> {
        let data = text.as_bytes();
        let n = data.len();
        let mask = rand16();
        let mut head = vec![0x81u8];
        if n < 126 {
            head.push(0x80 | n as u8);
        } else if n < 65536 {
            head.push(0x80 | 126);
            head.extend_from_slice(&(n as u16).to_be_bytes());
        } else {
            head.push(0x80 | 127);
            head.extend_from_slice(&(n as u64).to_be_bytes());
        }
        head.extend_from_slice(&mask[..4]);
        let masked: Vec<u8> = data.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]).collect();
        self.s.write_all(&head)?;
        self.s.write_all(&masked)?;
        Ok(())
    }
    fn read_exact_by(&mut self, buf: &mut [u8], deadline: Instant) -> Result<()> {
        let mut got = 0;
        while got < buf.len() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                bail!("cdp_timeout");
            }
            self.s.set_read_timeout(Some(left.max(Duration::from_millis(5))))?;
            match self.s.read(&mut buf[got..]) {
                Ok(0) => bail!("cdp_closed: 浏览器断开了调试连接"),
                Ok(k) => got += k,
                Err(e) if is_timeout(&e) => bail!("cdp_timeout"),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => bail!("cdp_closed: {e}"),
            }
        }
        Ok(())
    }
    /// 读一条完整文本消息; 超过 deadline 返回 cdp_timeout(此后连接视为损坏)
    fn recv_text(&mut self, deadline: Instant) -> Result<String> {
        let mut out = Vec::new();
        loop {
            let mut h = [0u8; 2];
            self.read_exact_by(&mut h, deadline)?;
            let op = h[0] & 0x0f;
            let mut n = (h[1] & 0x7f) as usize;
            if n == 126 {
                let mut b = [0u8; 2];
                self.read_exact_by(&mut b, deadline)?;
                n = u16::from_be_bytes(b) as usize;
            } else if n == 127 {
                let mut b = [0u8; 8];
                self.read_exact_by(&mut b, deadline)?;
                n = u64::from_be_bytes(b) as usize;
            }
            let mut payload = vec![0u8; n];
            self.read_exact_by(&mut payload, deadline)?;
            if op == 8 {
                bail!("cdp_closed: websocket closed");
            }
            if op == 9 || op == 10 {
                continue;
            } // ping/pong
            out.extend_from_slice(&payload);
            if h[0] & 0x80 != 0 {
                return Ok(String::from_utf8_lossy(&out).into_owned());
            }
        }
    }
}
fn rand16() -> [u8; 16] {
    let mut a = [0u8; 16];
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos().to_le_bytes();
    for i in 0..16 {
        a[i] = t[i % 8] ^ (i as u8).wrapping_mul(37) ^ 0xA5;
    }
    a
}

struct Cdp {
    ws: Ws,
    id: u64,
    session: Option<String>,
    /// 只保留控制台与崩溃类事件(其余丢弃, 常驻时不无限增长)
    events: std::collections::VecDeque<Value>,
    broken: bool,
}
impl Cdp {
    fn new(ws_url: &str) -> Result<Self> {
        Ok(Cdp { ws: Ws::connect(ws_url)?, id: 0, session: None, events: Default::default(), broken: false })
    }
    fn recv(&mut self, deadline: Instant, what: &str) -> Result<Value> {
        let raw = match self.ws.recv_text(deadline) {
            Ok(r) => r,
            Err(e) => {
                self.broken = true;
                let m = e.to_string();
                if m.starts_with("cdp_timeout") {
                    bail!("cdp_timeout: {what} 超时");
                }
                return Err(e);
            }
        };
        let m: Value = serde_json::from_str(&raw)?;
        let method = m["method"].as_str().unwrap_or("");
        if matches!(method, "Inspector.targetCrashed" | "Target.targetCrashed") {
            self.broken = true;
            bail!("page_crashed: 页面渲染进程崩溃({what})");
        }
        if method == "Target.detachedFromTarget" && m["params"]["sessionId"].as_str() == self.session.as_deref() {
            self.broken = true;
            bail!("page_crashed: 页面已断开({what})");
        }
        Ok(m)
    }
    fn keep(&mut self, m: Value) {
        if m["method"] == "Runtime.consoleAPICalled" {
            self.events.push_back(m);
            while self.events.len() > 200 {
                self.events.pop_front();
            }
        }
    }
    fn call(&mut self, method: &str, params: Value, use_session: bool, timeout_s: u64) -> Result<Value> {
        if self.broken {
            bail!("cdp_closed: 连接已损坏");
        }
        self.id += 1;
        let mid = self.id;
        let mut msg = json!({"id": mid, "method": method, "params": params});
        if use_session {
            if let Some(s) = &self.session {
                msg["sessionId"] = json!(s);
            }
        }
        if let Err(e) = self.ws.send_text(&msg.to_string()) {
            self.broken = true;
            bail!("cdp_closed: 发送 {method} 失败: {e}");
        }
        let deadline = Instant::now() + Duration::from_secs(timeout_s);
        loop {
            let m = self.recv(deadline, method)?;
            if m["id"].as_u64() == Some(mid) {
                if m.get("error").is_some() {
                    bail!("{method}: {}", m["error"]);
                }
                return Ok(m.get("result").cloned().unwrap_or(json!({})));
            }
            self.keep(m);
        }
    }
    fn console_text(ev: &Value) -> String {
        ev["params"]["args"].as_array().into_iter().flatten().map(|a| {
            if let Some(s) = a["value"].as_str() { s.to_string() }
            else if let Some(s) = a["description"].as_str() { s.to_string() }
            else { a["value"].to_string() }
        }).collect::<Vec<_>>().join(" ")
    }
    fn wait_console(&mut self, marker: &str, timeout_s: u64) -> Result<String> {
        let deadline = Instant::now() + Duration::from_secs(timeout_s);
        loop {
            while let Some(ev) = self.events.pop_front() {
                let txt = Self::console_text(&ev);
                if txt.starts_with(marker) {
                    return Ok(txt);
                }
            }
            let m = self.recv(deadline, &format!("等控制台 {marker}"))?;
            self.keep(m);
        }
    }
}

fn dbg_on() -> bool {
    std::env::var("WOWSMOD_CDP_DEBUG").is_ok()
}

/// 一个已加载 `?render=wait` 的无头页面。常驻实例长期持有它, 冷启动用完即关。
pub(crate) struct Page {
    browser: Option<Browser>,
    cdp: Cdp,
    base_url: String,
    pub swiftshader: bool,
    /// 前端 window.__renderApi(2 = 支持 __renderReload 增量重载)
    pub api: u64,
    pub open_s: f32,
}

impl Page {
    /// 启动浏览器 → 新标签 → 打开 ?render=wait → 等 WM_RENDER_READY。每步都有超时。
    pub fn open(exe: &Path, base_url: &str, size: (u32, u32), swiftshader: bool, tag: &str) -> Result<Page> {
        let t0 = Instant::now();
        if dbg_on() { eprintln!("[cdp] launch {} sw={swiftshader}", exe.display()); }
        let b = launch(exe, swiftshader, tag)?;
        if dbg_on() { eprintln!("[cdp] ws {}", b.ws_url); }
        let r = (|| -> Result<(Cdp, u64)> {
            let mut c = Cdp::new(&b.ws_url)?;
            let tid = c.call("Target.createTarget", json!({"url": "about:blank"}), false, 30)?["targetId"].as_str().unwrap_or("").to_string();
            c.session = Some(c.call("Target.attachToTarget", json!({"targetId": tid, "flatten": true}), false, 30)?["sessionId"].as_str().unwrap_or("").to_string());
            c.call("Runtime.enable", json!({}), true, 30)?;
            c.call("Page.enable", json!({}), true, 30)?;
            c.call("Inspector.enable", json!({}), true, 30).ok();
            let (w, h) = size;
            c.call("Emulation.setDeviceMetricsOverride", json!({"width": w, "height": h, "deviceScaleFactor": 1, "mobile": false}), true, 30)?;
            let url = format!("{}{}", base_url.trim_end_matches('/'), "/?render=wait");
            c.call("Page.navigate", json!({"url": url}), true, 60)?;
            if dbg_on() { eprintln!("[cdp] navigated, waiting WM_RENDER_READY"); }
            c.wait_console("WM_RENDER_READY", 60)?;
            if dbg_on() { eprintln!("[cdp] ready"); }
            let api = c.call("Runtime.evaluate", json!({"expression": "window.__renderApi || 1", "returnByValue": true}), true, 10)?["result"]["value"].as_u64().unwrap_or(1);
            Ok((c, api))
        })();
        match r {
            Ok((cdp, api)) => Ok(Page { browser: Some(b), cdp, base_url: base_url.to_string(), swiftshader, api, open_s: t0.elapsed().as_secs_f32() }),
            Err(e) => {
                close(b);
                Err(e)
            }
        }
    }

    pub fn browser_pid(&self) -> Option<u32> {
        self.browser.as_ref().map(|b| b.pid())
    }

    /// 浏览器进程活着且调试连接没坏
    pub fn healthy(&mut self) -> bool {
        !self.cdp.broken && self.browser.as_mut().map(|b| b.alive()).unwrap_or(false)
    }

    /// 出一张图: 返回 (PNG, meta)。webgl 失败且未用软件 GL 时返回 "webgl_retry" 错误。
    pub fn render(&mut self, label: &str, q: &str, timeout_s: u64) -> Result<(Vec<u8>, Value)> {
        let q2 = if q.contains("data=") { q.to_string() } else if q.is_empty() { "data=true".into() } else { format!("{q}&data=true") };
        let expr = format!("window.__render(window.__renderParse(new URLSearchParams({})))", serde_json::to_string(&q2)?);
        let res = self.cdp.call("Runtime.evaluate", json!({"expression": expr, "awaitPromise": true, "returnByValue": true}), true, timeout_s)
            .map_err(|e| if e.to_string().starts_with("cdp_timeout") { anyhow!("render_timeout: 页面 {timeout_s}s 内没有出图(window.__render 未返回)") } else { e })?;
        if res.get("exceptionDetails").is_some() {
            bail!("render_failed: JS 异常 {}", res["exceptionDetails"]);
        }
        let val = res["result"]["value"].clone();
        if val["ok"] != true {
            let code = val["error"]["code"].as_str().unwrap_or("");
            if code == "webgl" && !self.swiftshader {
                return Err(anyhow!("webgl_retry"));
            }
            if code == "bad_param" {
                bail!("bad_param: {}", val["error"]["message"].as_str().unwrap_or(""));
            }
            bail!("render_failed: {}", val["error"]);
        }
        let rw = val["width"].as_u64().unwrap_or(1600) as u32;
        let rh = val["height"].as_u64().unwrap_or(900) as u32;
        let png = if let Some(data_url) = val["dataURL"].as_str() {
            let b64 = data_url.split(',').nth(1).unwrap_or("");
            mfm::b64_decode(b64).ok_or_else(|| anyhow!("render_failed: dataURL 解码失败"))?
        } else {
            self.cdp.call("Emulation.setDeviceMetricsOverride", json!({"width": rw, "height": rh, "deviceScaleFactor": 1, "mobile": false}), true, 30)?;
            let shot = self.cdp.call("Page.captureScreenshot", json!({"format": "png", "clip": {"x": 0, "y": 0, "width": rw, "height": rh, "scale": 1}, "captureBeyondViewport": true}), true, 60)?;
            mfm::b64_decode(shot["data"].as_str().unwrap_or("")).ok_or_else(|| anyhow!("render_failed: 截图解码失败"))?
        };
        let mut meta = val;
        if let Some(o) = meta.as_object_mut() {
            o.remove("dataURL");
        }
        meta["label"] = json!(label);
        meta["query"] = json!(q);
        Ok((png, meta))
    }

    /// 项目改动后让页面增量重载(前端 __renderReload: 重新取 manifest, 几何按哈希、贴图按内容哈希复用)。
    /// 旧前端没有 __renderReload 时退回整页重新打开(浏览器磁盘缓存仍按哈希命中)。
    pub fn reload_project(&mut self, timeout_s: u64) -> Result<Value> {
        if self.api >= 2 {
            let res = self.cdp.call("Runtime.evaluate", json!({"expression": "window.__renderReload()", "awaitPromise": true, "returnByValue": true}), true, timeout_s)
                .map_err(|e| if e.to_string().starts_with("cdp_timeout") { anyhow!("render_timeout: 页面 {timeout_s}s 内没有完成重载") } else { e })?;
            let v = res["result"]["value"].clone();
            if v["ok"] == false {
                bail!("render_failed: 重载失败 {}", v["error"]);
            }
            return Ok(v);
        }
        let t0 = Instant::now();
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), "/?render=wait");
        self.cdp.events.clear();
        self.cdp.call("Page.navigate", json!({"url": url}), true, 60)?;
        self.cdp.wait_console("WM_RENDER_READY", 60)?;
        Ok(json!({"ok": true, "mode": "navigate", "ms": t0.elapsed().as_millis() as u64}))
    }

    pub fn close(mut self) {
        if let Some(b) = self.browser.take() {
            close(b);
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        if let Some(b) = self.browser.take() {
            close(b);
        }
    }
}

/// 冷启动: 起浏览器、出完 queries 里的每张图后关闭。queries: (out_label, query_string)
pub fn render_session(base_url: &str, queries: &[(String, String)], browser: Option<&Path>, size: (u32, u32)) -> Result<Vec<(Vec<u8>, Value)>> {
    let exe = find_browser(browser).ok_or_else(|| anyhow!("browser_not_found"))?;
    let mut sw = false;
    loop {
        let mut page = Page::open(&exe, base_url, size, sw, "")?;
        let mut out = Vec::new();
        let mut err = None;
        for (label, q) in queries {
            match page.render(label, q, 120) {
                Ok(x) => out.push(x),
                Err(e) => {
                    err = Some(e);
                    break;
                }
            }
        }
        page.close();
        match err {
            None => return Ok(out),
            Some(e) if e.to_string().contains("webgl_retry") && !sw => {
                sw = true;
                continue;
            }
            Some(e) => return Err(e),
        }
    }
}


/// 回退: 调 preview_app/tools/cdp.py(验收用的参考实现)
pub fn render_via_python(base_url: &str, queries: &[(String, String)], browser: Option<&Path>) -> Result<Vec<(Vec<u8>, Value)>> {
    let helper = find_cdp_py().ok_or_else(|| anyhow!("no cdp.py"))?;
    let mut cmd = Command::new("py");
    cmd.args(["-3", &helper.display().to_string(), "render", "--url", base_url]);
    let owned = find_browser(None);
    if let Some(b) = browser.or(owned.as_deref()) {
        cmd.arg("--browser").arg(b);
    }
    let mut outs = Vec::new();
    for (i, (label, q)) in queries.iter().enumerate() {
        let tmp = std::env::temp_dir().join(format!("wm_cdp_out_{i}.png"));
        let _ = std::fs::remove_file(&tmp);
        cmd.arg(&tmp).arg(q);
        outs.push((label.clone(), tmp, q.clone()));
    }
    let o = output_with_timeout(cmd, 180).context("运行 cdp.py")?;
    let stdout = String::from_utf8_lossy(&o.stdout);
    if !o.status.success() {
        let err = String::from_utf8_lossy(&o.stderr);
        if stdout.contains("browser_not_found") || err.contains("browser_not_found") {
            bail!("browser_not_found");
        }
        bail!("cdp.py 失败: {stdout}{err}");
    }
    let mut result = Vec::new();
    for line in stdout.lines() {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if v["ok"] == true {
                let path = v["out"].as_str().unwrap_or("");
                let png = std::fs::read(path)?;
                result.push((png, v));
            } else if v["error"]["code"] == "browser_not_found" {
                bail!("browser_not_found");
            }
        }
    }
    if result.is_empty() {
        bail!("cdp.py 没有输出成功结果: {stdout}");
    }
    Ok(result)
}

fn find_cdp_py() -> Option<PathBuf> {
    let mut c = vec![];
    if let Ok(e) = std::env::var("WOWSMOD_CDP_HELPER") { c.push(PathBuf::from(e)); }
    c.push(PathBuf::from("preview_app/tools/cdp.py"));
    if let Ok(e) = std::env::current_exe() {
        if let Some(d) = e.parent() {
            c.push(d.join("../../../preview_app/tools/cdp.py"));
        }
    }
    c.into_iter().find(|p| p.is_file())
}

/// 子进程带超时: 超时杀掉并报 render_timeout(之前 cdp.py 回退没有超时, 是挂起点之一)
pub(crate) fn output_with_timeout(mut cmd: Command, secs: u64) -> Result<std::process::Output> {
    let tmp = std::env::temp_dir().join(format!("wm_sub_{}_{}", std::process::id(), LAUNCH_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    let (fo, fe) = (std::fs::File::create(tmp.with_extension("out"))?, std::fs::File::create(tmp.with_extension("err"))?);
    let mut child = cmd.stdin(Stdio::null()).stdout(fo).stderr(fe).spawn()?;
    let t0 = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if t0.elapsed() > Duration::from_secs(secs) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(tmp.with_extension("out"));
            let _ = std::fs::remove_file(tmp.with_extension("err"));
            bail!("render_timeout: 子进程 {secs}s 未结束, 已终止");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stdout = std::fs::read(tmp.with_extension("out")).unwrap_or_default();
    let stderr = std::fs::read(tmp.with_extension("err")).unwrap_or_default();
    let _ = std::fs::remove_file(tmp.with_extension("out"));
    let _ = std::fs::remove_file(tmp.with_extension("err"));
    Ok(std::process::Output { status, stdout, stderr })
}


/// Windows: 把无头浏览器放进 "本进程退出即杀" 的 Job 里。
/// 常驻实例/冷启动进程被强杀(任务管理器、taskkill、崩溃)时浏览器跟着退出, 不留孤儿 msedge。
#[cfg(windows)]
mod kill_on_exit {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimit {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct IoCounters([u64; 6]);
    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimit {
        basic: BasicLimit,
        io: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }
    extern "system" {
        fn CreateJobObjectW(attrs: *mut core::ffi::c_void, name: *const u16) -> *mut core::ffi::c_void;
        fn SetInformationJobObject(job: *mut core::ffi::c_void, class: i32, info: *mut core::ffi::c_void, len: u32) -> i32;
        fn AssignProcessToJobObject(job: *mut core::ffi::c_void, process: *mut core::ffi::c_void) -> i32;
    }
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;

    struct Job(usize);
    static JOB: OnceLock<Option<Job>> = OnceLock::new();

    fn job() -> Option<usize> {
        JOB.get_or_init(|| unsafe {
            let h = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
            if h.is_null() {
                return None;
            }
            let mut info = ExtendedLimit::default();
            info.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(h, JOB_OBJECT_EXTENDED_LIMIT_INFORMATION, &mut info as *mut _ as *mut _, std::mem::size_of::<ExtendedLimit>() as u32) == 0 {
                return None;
            }
            // 句柄故意不关: 进程退出时系统关闭它 → Job 里的浏览器全部结束
            Some(Job(h as usize))
        })
        .as_ref()
        .map(|j| j.0)
    }

    pub fn attach(child: &std::process::Child) {
        if let Some(j) = job() {
            unsafe {
                AssignProcessToJobObject(j as *mut _, child.as_raw_handle() as *mut _);
            }
        }
    }
}
