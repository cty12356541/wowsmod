//! tiny_http 本机服务: 静态前端 + /api/v1/*
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use rust_embed::Embed;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use super::state::PreviewState;
use crate::game;

#[derive(Embed)]
#[folder = "preview_web/"]
struct WebAssets;

pub fn bind(port: u16) -> Result<(u16, Server)> {
    if port == 0 {
        let s = Server::http("127.0.0.1:0").map_err(|e| anyhow!("bind: {e}"))?;
        let p = s.server_addr().to_ip().map(|a| a.port()).unwrap_or(0);
        return Ok((p, s));
    }
    for i in 0..20u16 {
        let p = port + i;
        match Server::http(format!("127.0.0.1:{p}")) {
            Ok(s) => return Ok((p, s)),
            Err(_) => continue,
        }
    }
    Err(anyhow!("端口 {port}..{} 都被占用", port + 19))
}

/// 常驻出图实例的控制端点钩子(/__wm/*): 返回 None 表示不处理
pub type Ctl = Arc<dyn Fn(&Method, &str, &mut Request) -> Option<Response<std::io::Cursor<Vec<u8>>>> + Send + Sync>;

pub fn serve(server: Server, st: Arc<PreviewState>) -> Result<()> {
    serve_with(server, st, None)
}

pub fn serve_with(server: Server, st: Arc<PreviewState>, ctl: Option<Ctl>) -> Result<()> {
    // 多线程处理: 前端会并行拉 geometry + 多张贴图
    for mut req in server.incoming_requests() {
        if st.is_shutdown() {
            break;
        }
        let st = Arc::clone(&st);
        let ctl = ctl.clone();
        std::thread::spawn(move || {
            let method = req.method().clone();
            let url = req.url().to_string();
            if let Some(c) = &ctl {
                if url.starts_with("/__wm/") {
                    if let Some(r) = c(&method, &url, &mut req) {
                        let _ = req.respond(r);
                        return;
                    }
                }
            }
            if method == Method::Options {
                let mut r = Response::from_data(Vec::<u8>::new()).with_status_code(204);
                r.add_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap());
                r.add_header(Header::from_bytes(&b"Access-Control-Allow-Methods"[..], &b"GET,POST,OPTIONS"[..]).unwrap());
                r.add_header(Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Content-Type"[..]).unwrap());
                let _ = req.respond(r);
                return;
            }
            let r = match handle(&st, &method, &url, &mut req) {
                Ok(r) => r,
                Err(e) => {
                    let (code, body) = api_err(&e);
                    json_resp(code, &body)
                }
            };
            let _ = req.respond(r);
        });
    }
    Ok(())
}

fn handle(st: &Arc<PreviewState>, method: &Method, url: &str, req: &mut Request) -> Result<Response<std::io::Cursor<Vec<u8>>>, Error> {
    let (path, query) = split_url(url);
    if path.starts_with("/api/v1/") {
        return api(st, method, &path, &query, req);
    }
    if method != &Method::Get && method != &Method::Head {
        return Err(Error::status(405, "method_not_allowed", "只用 GET/POST"));
    }
    static_file(&path)
}

fn api(st: &Arc<PreviewState>, method: &Method, path: &str, q: &[(String, String)], req: &mut Request) -> Result<Response<std::io::Cursor<Vec<u8>>>, Error> {
    let get = |k: &str| q.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str());
    match (method, path) {
        (Method::Get, "/api/v1/health") => Ok(json_ok(&st.health())),
        (Method::Get, "/api/v1/project") => Ok(json_ok(&st.project_view())),
        (Method::Get, "/api/v1/state") => Ok(json_ok(&st.state_view())),
        (Method::Get, "/api/v1/steps") => Ok(json_ok(&st.steps_view())),
        (Method::Get, "/api/v1/parts") => Ok(json_ok(&st.parts_view(get("rev"), get("base")).map_err(Error::from)?)),
        (Method::Get, "/api/v1/model") => Ok(json_ok(&st.model_view(get("rev")).map_err(Error::from)?)),
        (Method::Get, "/api/v1/textures") => Ok(json_ok(&st.textures_view(get("rev"), get("base")).map_err(Error::from)?)),
        (Method::Get, "/api/v1/sim") => Ok(json_ok(&st.sim_view().map_err(Error::from)?)),
        (Method::Get, p) if p.starts_with("/api/v1/geometry/") && p.ends_with(".glb") => {
            let hash = &p["/api/v1/geometry/".len()..p.len() - 4];
            let b = st.geometry_bytes(hash).map_err(Error::from)?;
            Ok(bytes_resp(200, b, "model/gltf-binary", "public, max-age=31536000, immutable"))
        }
        (Method::Get, p) if p.starts_with("/api/v1/blob/") && p.ends_with(".png") => {
            let hash = &p["/api/v1/blob/".len()..p.len() - 4];
            let max: u32 = get("max").and_then(|s| s.parse().ok()).unwrap_or(0);
            let ch = get("ch").unwrap_or("rgba");
            let b = st.blob_png(hash, max, ch).map_err(Error::from)?;
            Ok(bytes_resp(200, b, "image/png", "public, max-age=31536000, immutable"))
        }
        (Method::Get, "/api/v1/events") => sse(st),
        (Method::Get, p) if p.starts_with("/api/v1/jobs/") => {
            let id = &p["/api/v1/jobs/".len()..];
            let jobs = st.jobs.lock().unwrap();
            match jobs.get(id) {
                Some(j) => Ok(json_ok(j)),
                None => Err(Error::status(404, "not_found", "没有这个任务")),
            }
        }
        (Method::Post, "/api/v1/install") | (Method::Post, "/api/v1/uninstall") => {
            if !req_json_ct(req) {
                return Err(Error::status(415, "unsupported_media", "Content-Type 必须是 application/json"));
            }
            if !game::running().is_empty() {
                return Err(Error::status(409, "game_running", "游戏正在运行, 请先退出"));
            }
            let install = path.ends_with("install");
            let id = format!("job{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis());
            {
                let mut jobs = st.jobs.lock().unwrap();
                jobs.insert(id.clone(), json!({"id": id, "state": "running", "progress": 0.0, "message": if install { "安装中" } else { "卸载中" }}));
            }
            st.bump("job", json!({"id": id}));
            let root = st.project.root.clone();
            let st2 = Arc::clone(st);
            let id2 = id.clone();
            std::thread::spawn(move || {
                let r = if install {
                    // 通过子进程调用, 避免 Cli 生命周期问题
                    run_self(&["install", &root.display().to_string()])
                } else {
                    run_self(&["uninstall", &root.display().to_string()])
                };
                let mut jobs = st2.jobs.lock().unwrap();
                if let Some(j) = jobs.get_mut(&id2) {
                    if r["ok"] == true {
                        *j = json!({"id": id2, "state": "done", "progress": 1.0, "message": "完成", "result": r});
                    } else {
                        *j = json!({"id": id2, "state": "failed", "progress": 1.0, "message": "失败", "error": {"code": "install_failed", "message": r["error"].clone()}});
                    }
                }
                st2.refresh_install();
                st2.bump("job", json!({"id": id2}));
                st2.bump("project", json!({}));
            });
            Ok(json_ok(&json!({"ok": true, "job_id": id})))
        }
        (Method::Post, p) if p.contains("/revert") || p.contains("/restore") => Err(Error::status(501, "not_implemented", "撤回/恢复需要对象仓库, 尚未接通; 可用 wowsmod model undo / 重新编辑")),
        _ => Err(Error::status(404, "not_found", &format!("未知端点 {path}"))),
    }
}

impl PreviewState {
}

fn run_self(args: &[&str]) -> Value {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => return json!({"ok": false, "error": format!("{e}")}),
    };
    match std::process::Command::new(exe).arg("--json").args(args).output() {
        Ok(o) => serde_json::from_slice(&o.stdout).unwrap_or_else(|_| json!({"ok": o.status.success(), "stderr": String::from_utf8_lossy(&o.stderr)})),
        Err(e) => json!({"ok": false, "error": format!("{e}")}),
    }
}

fn sse(st: &Arc<PreviewState>) -> Result<Response<std::io::Cursor<Vec<u8>>>, Error> {
    // tiny_http 对长连接支持有限: 用同步写一段 hello+短流后关闭, 前端会重连; 同时提供 /state 轮询。
    // 更好的做法是接管 raw writer — tiny_http Request::into_writer
    // 这里用短暂 SSE: hello + 当前状态, 然后结束(前端 2s 重连 / 用 /state)
    let head = st.head_id();
    let body = format!(
        "event: hello\ndata: {}\n\nevent: ping\ndata: {}\n\n",
        json!({"etag": st.etag(), "head": head, "retry_ms": 2000}),
        json!({"etag": st.etag(), "time": chrono_now()})
    );
    let mut r = Response::from_data(body.into_bytes()).with_status_code(200);
    r.add_header(Header::from_bytes(&b"Content-Type"[..], &b"text/event-stream; charset=utf-8"[..]).unwrap());
    r.add_header(Header::from_bytes(&b"Cache-Control"[..], &b"no-store"[..]).unwrap());
    Ok(cors(r))
}

fn chrono_now() -> String {
    // 与 state::iso 一致的简易时间戳
    format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
}

fn static_file(path: &str) -> Result<Response<std::io::Cursor<Vec<u8>>>, Error> {
    // WOWSMOD_PREVIEW_WEB 覆盖
    if let Ok(dir) = std::env::var("WOWSMOD_PREVIEW_WEB") {
        let rel = if path == "/" || path.is_empty() { "index.html" } else { path.trim_start_matches('/') };
        let fp = Path::new(&dir).join(rel);
        if fp.is_file() {
            let b = std::fs::read(&fp).map_err(|e| Error::status(500, "io", &e.to_string()))?;
            return Ok(bytes_resp(200, b, mime_of(rel), "no-cache"));
        }
    }
    let rel = if path == "/" || path.is_empty() { "index.html".to_string() } else { path.trim_start_matches('/').to_string() };
    match WebAssets::get(&rel) {
        Some(f) => Ok(bytes_resp(200, f.data.as_ref().to_vec(), mime_of(&rel), "no-cache")),
        None => Err(Error::status(404, "not_found", "静态文件不存在")),
    }
}

fn mime_of(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "json" => "application/json",
        "woff2" => "font/woff2",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn split_url(url: &str) -> (String, Vec<(String, String)>) {
    let mut parts = url.splitn(2, '?');
    let path = parts.next().unwrap_or("/").to_string();
    let mut q = Vec::new();
    if let Some(qs) = parts.next() {
        for pair in qs.split('&') {
            let mut kv = pair.splitn(2, '=');
            let k = kv.next().unwrap_or("").to_string();
            let v = kv.next().unwrap_or("").replace('+', " ");
            let v = urlencoding_decode(&v);
            if !k.is_empty() {
                q.push((k, v));
            }
        }
    }
    (path, q)
}
fn urlencoding_decode(s: &str) -> String {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (from_hex(b[i + 1]), from_hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
fn from_hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn req_json_ct(req: &Request) -> bool {
    req.headers().iter().any(|h| h.field.equiv("Content-Type") && h.value.as_str().contains("application/json"))
}

fn json_ok(v: &Value) -> Response<std::io::Cursor<Vec<u8>>> {
    json_resp(200, v)
}
pub(crate) fn json_resp(code: u16, v: &Value) -> Response<std::io::Cursor<Vec<u8>>> {
    let b = serde_json::to_vec(v).unwrap_or_default();
    let mut r = Response::from_data(b).with_status_code(StatusCode(code));
    r.add_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json; charset=utf-8"[..]).unwrap());
    r.add_header(Header::from_bytes(&b"Cache-Control"[..], &b"no-store"[..]).unwrap());
    cors(r)
}
fn bytes_resp(code: u16, b: Vec<u8>, ctype: &str, cache: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut r = Response::from_data(b).with_status_code(StatusCode(code));
    r.add_header(Header::from_bytes(&b"Content-Type"[..], ctype.as_bytes()).unwrap());
    r.add_header(Header::from_bytes(&b"Cache-Control"[..], cache.as_bytes()).unwrap());
    cors(r)
}
fn cors(mut r: Response<std::io::Cursor<Vec<u8>>>) -> Response<std::io::Cursor<Vec<u8>>> {
    r.add_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap());
    r
}

struct Error {
    status: u16,
    body: Value,
}
impl Error {
    fn status(status: u16, code: &str, message: &str) -> Self {
        Self { status, body: json!({"ok": false, "error": {"code": code, "message": message}}) }
    }
    fn from(e: anyhow::Error) -> Self {
        let msg = format!("{e:#}");
        let code = if msg.contains("not_found") {
            "not_found"
        } else if msg.contains("no_model") {
            "no_model"
        } else {
            "internal"
        };
        let status = if code == "not_found" || code == "no_model" { 404 } else { 500 };
        Self::status(status, code, &msg)
    }
}
fn api_err(e: &Error) -> (u16, Value) {
    (e.status, e.body.clone())
}
