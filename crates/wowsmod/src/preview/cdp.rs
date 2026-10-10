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

struct Browser {
    child: Child,
    user_dir: PathBuf,
    ws_url: String,
}

fn launch(exe: &Path, swiftshader: bool) -> Result<Browser> {
    let user_dir = std::env::temp_dir().join(format!("wm_cdp_{}", std::process::id()));
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
        "--headless=new".into(),
    ];
    if swiftshader {
        args.push("--use-angle=swiftshader".into());
        args.push("--enable-unsafe-swiftshader".into());
    }
    args.push("about:blank".into());
    let child = Command::new(exe).args(&args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().context("启动浏览器")?;
    let port_file = user_dir.join("DevToolsActivePort");
    let t0 = Instant::now();
    loop {
        if t0.elapsed() > Duration::from_secs(20) {
            bail!("浏览器未在 20s 内写出 DevToolsActivePort");
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
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn close(mut b: Browser) {
    let _ = b.child.kill();
    let _ = b.child.wait();
    for _ in 0..20 {
        if std::fs::remove_dir_all(&b.user_dir).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

// ---------- 最小 WebSocket 客户端 ----------
struct Ws {
    s: TcpStream,
}
impl Ws {
    fn connect(url: &str) -> Result<Self> {
        let u = url.strip_prefix("ws://").ok_or_else(|| anyhow!("只支持 ws://"))?;
        let (hostport, path) = u.split_once('/').map(|(h, p)| (h, format!("/{p}"))).unwrap_or((u, "/".into()));
        let (host, port) = hostport.split_once(':').unwrap_or((hostport, "80"));
        let mut s = TcpStream::connect(format!("{host}:{port}"))?;
        s.set_read_timeout(Some(Duration::from_secs(180)))?;
        s.set_write_timeout(Some(Duration::from_secs(60)))?;
        let key = mfm::b64_encode(&rand16());
        let req = format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n");
        s.write_all(req.as_bytes())?;
        let mut hdr = Vec::new();
        let mut buf = [0u8; 1];
        while !hdr.windows(4).any(|w| w == b"\r\n\r\n") {
            s.read_exact(&mut buf)?;
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
    fn recv_text(&mut self) -> Result<String> {
        let mut out = Vec::new();
        loop {
            let mut h = [0u8; 2];
            self.s.read_exact(&mut h)?;
            let op = h[0] & 0x0f;
            let mut n = (h[1] & 0x7f) as usize;
            if n == 126 {
                let mut b = [0u8; 2];
                self.s.read_exact(&mut b)?;
                n = u16::from_be_bytes(b) as usize;
            } else if n == 127 {
                let mut b = [0u8; 8];
                self.s.read_exact(&mut b)?;
                n = u64::from_be_bytes(b) as usize;
            }
            let mut payload = vec![0u8; n];
            self.s.read_exact(&mut payload)?;
            if op == 8 {
                bail!("websocket closed");
            }
            if op == 9 {
                continue;
            } // ping
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
    events: Vec<Value>,
}
impl Cdp {
    fn new(ws_url: &str) -> Result<Self> {
        Ok(Cdp { ws: Ws::connect(ws_url)?, id: 0, session: None, events: vec![] })
    }
    fn call(&mut self, method: &str, params: Value, use_session: bool, timeout_s: u64) -> Result<Value> {
        self.id += 1;
        let mid = self.id;
        let mut msg = json!({"id": mid, "method": method, "params": params});
        if use_session {
            if let Some(s) = &self.session {
                msg["sessionId"] = json!(s);
            }
        }
        self.ws.send_text(&msg.to_string())?;
        let t0 = Instant::now();
        loop {
            let raw = self.ws.recv_text()?;
            let m: Value = serde_json::from_str(&raw)?;
            if m["id"].as_u64() == Some(mid) {
                if m.get("error").is_some() {
                    bail!("{method}: {}", m["error"]);
                }
                return Ok(m.get("result").cloned().unwrap_or(json!({})));
            }
            self.events.push(m);
            if t0.elapsed() > Duration::from_secs(timeout_s) {
                bail!("CDP 超时: {method}");
            }
        }
    }
    fn wait_console(&mut self, marker: &str, timeout_s: u64) -> Result<String> {
        let t0 = Instant::now();
        loop {
            for i in 0..self.events.len() {
                if self.events[i]["method"] == "Runtime.consoleAPICalled" {
                    let txt: String = self.events[i]["params"]["args"].as_array().into_iter().flatten().map(|a| {
                        if let Some(s) = a["value"].as_str() { s.to_string() }
                        else if let Some(s) = a["description"].as_str() { s.to_string() }
                        else { a["value"].to_string() }
                    }).collect::<Vec<_>>().join(" ");
                    if txt.starts_with(marker) {
                        self.events.drain(..=i);
                        return Ok(txt);
                    }
                }
            }
            self.events.clear();
            let raw = self.ws.recv_text()?;
            self.events.push(serde_json::from_str(&raw)?);
            if t0.elapsed() > Duration::from_secs(timeout_s) {
                bail!("等控制台 {marker} 超时");
            }
        }
    }
}

/// queries: (out_label, query_string) — 返回每张 PNG 字节与 meta
pub fn render_session(base_url: &str, queries: &[(String, String)], browser: Option<&Path>, size: (u32, u32)) -> Result<Vec<(Vec<u8>, Value)>> {
    let exe = find_browser(browser).ok_or_else(|| anyhow!("browser_not_found"))?;
    let mut attempt_sw = false;
    loop {
        let b = launch(&exe, attempt_sw)?;
        let result = (|| -> Result<Vec<(Vec<u8>, Value)>> {
            let mut c = Cdp::new(&b.ws_url)?;
            let tid = c.call("Target.createTarget", json!({"url": "about:blank"}), false, 30)?["targetId"].as_str().unwrap().to_string();
            c.session = Some(c.call("Target.attachToTarget", json!({"targetId": tid, "flatten": true}), false, 30)?["sessionId"].as_str().unwrap().to_string());
            c.call("Runtime.enable", json!({}), true, 30)?;
            c.call("Page.enable", json!({}), true, 30)?;
            let (w, h) = size;
            c.call("Emulation.setDeviceMetricsOverride", json!({"width": w, "height": h, "deviceScaleFactor": 1, "mobile": false}), true, 30)?;
            let url = format!("{}{}", base_url.trim_end_matches('/'), "/?render=wait");
            c.call("Page.navigate", json!({"url": url}), true, 60)?;
            c.wait_console("WM_RENDER_READY", 60)?;
            let mut out = Vec::new();
            for (label, q) in queries {
                let q2 = if q.contains("data=") { q.clone() } else if q.is_empty() { "data=true".into() } else { format!("{q}&data=true") };
                let expr = format!("window.__render(window.__renderParse(new URLSearchParams({})))", serde_json::to_string(&q2)?);
                let res = c.call("Runtime.evaluate", json!({"expression": expr, "awaitPromise": true, "returnByValue": true}), true, 120)?;
                if res.get("exceptionDetails").is_some() {
                    bail!("JS: {}", res["exceptionDetails"]);
                }
                let val = res["result"]["value"].clone();
                if val["ok"] != true {
                    let code = val["error"]["code"].as_str().unwrap_or("");
                    if code == "webgl" && !attempt_sw {
                        return Err(anyhow!("webgl_retry"));
                    }
                    bail!("render 失败: {}", val["error"]);
                }
                let rw = val["width"].as_u64().unwrap_or(w as u64) as u32;
                let rh = val["height"].as_u64().unwrap_or(h as u64) as u32;
                c.call("Emulation.setDeviceMetricsOverride", json!({"width": rw, "height": rh, "deviceScaleFactor": 1, "mobile": false}), true, 30)?;
                let png = if let Some(data_url) = val["dataURL"].as_str() {
                    let b64 = data_url.split(',').nth(1).unwrap_or("");
                    mfm::b64_decode(b64).ok_or_else(|| anyhow!("dataURL 解码失败"))?
                } else {
                    let shot = c.call("Page.captureScreenshot", json!({"format": "png", "clip": {"x": 0, "y": 0, "width": rw, "height": rh, "scale": 1}, "captureBeyondViewport": true}), true, 60)?;
                    mfm::b64_decode(shot["data"].as_str().unwrap_or("")).ok_or_else(|| anyhow!("截图解码失败"))?
                };
                let mut meta = val.clone();
                meta["label"] = json!(label);
                meta["query"] = json!(q);
                out.push((png, meta));
            }
            Ok(out)
        })();
        close(b);
        match result {
            Err(e) if e.to_string().contains("webgl_retry") && !attempt_sw => {
                attempt_sw = true;
                continue;
            }
            other => return other,
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
    let o = cmd.output().context("运行 cdp.py")?;
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
    c.push(PathBuf::from("D:/WoWSModding/preview_app/tools/cdp.py"));
    c.push(PathBuf::from("/workspace/preview_app/tools/cdp.py"));
    if let Ok(e) = std::env::current_exe() {
        if let Some(d) = e.parent() {
            c.push(d.join("../../../preview_app/tools/cdp.py"));
        }
    }
    c.into_iter().find(|p| p.is_file())
}
