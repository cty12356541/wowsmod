//! tiny_http 本机服务: 静态前端 + /api/v1/*
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use rust_embed::Embed;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use super::state::PreviewState;
use super::library::PreviewService;
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
    serve_service_with(server, Arc::new(PreviewService::legacy(st)?), ctl)
}

pub fn serve_library(server: Server, service: Arc<PreviewService>) -> Result<()> {
    serve_service_with(server, service, None)
}

fn serve_service_with(server: Server, service: Arc<PreviewService>, ctl: Option<Ctl>) -> Result<()> {
    let port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(0);
    // 多线程处理: 前端会并行拉 geometry + 多张贴图
    for mut req in server.incoming_requests() {
        if service.default.as_ref().map(|s| s.is_shutdown()).unwrap_or(false) {
            break;
        }
        let service = Arc::clone(&service);
        let ctl = ctl.clone();
        std::thread::spawn(move || {
            let method = req.method().clone();
            let url = req.url().to_string();
            if let Err(e) = validate_origin(&req, port) {
                let _ = req.respond(json_resp(e.status, &e.body));
                return;
            }
            if let Some(c) = &ctl {
                if url.starts_with("/__wm/") {
                    if let Some(r) = c(&method, &url, &mut req) {
                        let _ = req.respond(r);
                        return;
                    }
                }
            }
            if method == Method::Options {
                let _ = req.respond(Response::from_data(Vec::<u8>::new()).with_status_code(204));
                return;
            }
            let r = match handle_service(&service, &method, &url, &mut req, port) {
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

fn handle_service(service: &PreviewService, method: &Method, url: &str, req: &mut Request, port: u16) -> Result<Response<std::io::Cursor<Vec<u8>>>, Error> {
    validate_origin(req, port)?;
    let (path, query) = split_url(url);
    if path == "/api/v1/health" && method == &Method::Get { return Ok(json_ok(&service.health())); }
    if path == "/api/v1/library" || path.starts_with("/api/v1/library/") {
        return library_api(service, method, &path, req);
    }
    if let Some(rest) = path.strip_prefix("/api/v1/projects/") {
        let (id, route) = rest.split_once('/').ok_or_else(|| Error::status(404, "not_found", "缺少项目端点"))?;
        if id.is_empty() || route.is_empty() { return Err(Error::status(404, "not_found", "缺少项目 ID 或端点")); }
        let st = service.session(id).map_err(Error::from)?;
        return api(&st, method, &format!("/api/v1/{route}"), &query, req, Some(id));
    }
    if path.starts_with("/api/v1/") {
        let st = service.default.as_ref().ok_or_else(|| Error::status(404, "not_found", "制作库模式需要明确的项目 ID"))?;
        return api(st, method, &path, &query, req, None);
    }
    if method != &Method::Get && method != &Method::Head {
        return Err(Error::status(405, "method_not_allowed", "只用 GET/POST"));
    }
    static_file(&path)
}

fn library_api(service: &PreviewService, method: &Method, path: &str, req: &mut Request) -> Result<Response<std::io::Cursor<Vec<u8>>>, Error> {
    if method == &Method::Get && path == "/api/v1/library" { return Ok(json_ok(&service.listing().map_err(Error::from)?)); }
    if method != &Method::Post { return Err(Error::status(405, "method_not_allowed", "目录库修改需要 POST")); }
    let body = read_json(req)?;
    let store = service.store().map_err(Error::from)?;
    let string = |key: &str| body[key].as_str().filter(|s| !s.trim().is_empty()).ok_or_else(|| Error::status(400, "invalid_request", &format!("{key} 必须是非空字符串")));
    let value = match path {
        "/api/v1/library/add" => json!({"ok": true, "entry": store.add(Path::new(string("path")?)).map_err(Error::catalog)?}),
        "/api/v1/library/scan" => {
            let scan = store.scan(Path::new(string("root")?)).map_err(Error::catalog)?;
            json!({"ok": true, "entries": scan.entries, "warnings": scan.warnings, "visited_dirs": scan.visited_dirs, "truncated": scan.truncated})
        }
        _ => {
            let rest = path.strip_prefix("/api/v1/library/projects/").ok_or_else(|| Error::status(404, "not_found", "未知目录库端点"))?;
            let (id, action) = rest.split_once('/').ok_or_else(|| Error::status(404, "not_found", "缺少项目操作"))?;
            if !store.list().map_err(Error::from)?.entries.iter().any(|e| e.entry.id == id) { return Err(Error::status(404, "not_found", "目录库中没有这个项目")); }
            match action {
                "update" => {
                    let patch: crate::library::LibraryPatch = serde_json::from_value(body).map_err(|e| Error::status(400, "invalid_request", &e.to_string()))?;
                    json!({"ok": true, "entry": store.update(id, patch).map_err(Error::catalog)?})
                }
                "relocate" => {
                    json!({"ok": true, "entry": service.relocate(id, Path::new(string("path")?)).map_err(Error::catalog_change)?})
                }
                "remove" => {
                    json!({"ok": true, "entry": service.remove(id).map_err(Error::catalog_change)?})
                }
                "open" => {
                    service.source(id).map_err(Error::from)?;
                    store.touch_opened(id).map_err(Error::catalog)?;
                    json!({"ok": true, "url": format!("/?project={id}")})
                }
                _ => return Err(Error::status(404, "not_found", "未知目录库操作")),
            }
        }
    };
    Ok(json_ok(&value))
}

fn rewrite_urls(value: &mut Value, id: &str) {
    match value {
        Value::String(s) if s.starts_with("/api/v1/") && !s.starts_with("/api/v1/projects/") => {
            *s = format!("/api/v1/projects/{id}/{}", &s["/api/v1/".len()..]);
        }
        Value::Array(values) => for value in values { rewrite_urls(value, id); },
        Value::Object(values) => for value in values.values_mut() { rewrite_urls(value, id); },
        _ => {},
    }
}

fn api(st: &Arc<PreviewState>, method: &Method, path: &str, q: &[(String, String)], req: &mut Request, scope: Option<&str>) -> Result<Response<std::io::Cursor<Vec<u8>>>, Error> {
    let get = |k: &str| q.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str());
    let json_ok = |value: &Value| { let mut value = value.clone(); if let Some(id) = scope { rewrite_urls(&mut value, id); } json_resp(200, &value) };
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
        (Method::Post, "/api/v1/install" | "/api/v1/uninstall" | "/api/v1/apply") => {
            let body = read_json(req)?;
            let action = Action::from_path(path).expect("matched action");
            let dry_run = if action == Action::Apply {
                body["dry_run"].as_bool().ok_or_else(|| Error::status(400, "invalid_request", "dry_run 必须是布尔值"))?
            } else { false };
            if st.game.is_none() { return Err(Error::status(409, "game_unavailable", "指定客户端不可用，请用 --game 指定有效客户端")); }
            if !game::running().is_empty() {
                return Err(Error::status(409, "game_running", "游戏正在运行, 请先退出"));
            }
            if action == Action::Uninstall {
                let installed = super::library::install_status(&st.project.root, st.game.as_ref());
                if installed["project_dir"].is_null() {
                    return Err(Error::status(409, "not_installed", "当前客户端没有应用此项目，不能按同名或旧清单卸载其他项目"));
                }
            }
            if action == Action::Apply && dry_run {
                let result = crate::m4cmd::apply_project(&super::library::selected_cli(st.selected_game.clone()), &st.project.root, true).map_err(Error::action)?;
                return Ok(json_ok(&result));
            }
            static NEXT_JOB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            let id = format!("job{}", NEXT_JOB.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
            {
                let mut jobs = st.jobs.lock().unwrap();
                // Bound completed job history while preserving every ongoing job.
                if jobs.len() >= 128 { jobs.retain(|_, j| j["state"] == "running"); }
                jobs.insert(id.clone(), json!({"id": id, "state": "running", "progress": 0.0, "message": if action == Action::Uninstall { "卸载中" } else { "应用中" }}));
            }
            st.bump("job", json!({"id": id}));
            let root = st.project.root.clone();
            let st2 = Arc::clone(st);
            let id2 = id.clone();
            std::thread::spawn(move || {
                let r = if action == Action::Apply {
                    crate::m4cmd::apply_project(&super::library::selected_cli(st2.selected_game.clone()), &root, false)
                        .unwrap_or_else(|e| json!({"ok": false, "error": {"code": "apply_failed", "message": format!("{e:#}")}}))
                } else {
                    run_self(&action_args(action, &root, st2.selected_game.as_deref().unwrap_or(&st2.game.as_ref().unwrap().root)))
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
            Ok(json_ok(&json!({"ok": true, "job_id": id, "url": format!("/api/v1/jobs/{id}")})))
        }
        (Method::Post, p) if p.contains("/revert") || p.contains("/restore") => Err(Error::status(501, "not_implemented", "撤回/恢复需要对象仓库, 尚未接通; 可用 wowsmod model undo / 重新编辑")),
        _ => Err(Error::status(404, "not_found", &format!("未知端点 {path}"))),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action { Install, Uninstall, Apply }
impl Action {
    fn from_path(path: &str) -> Option<Self> {
        match path { "/api/v1/install" => Some(Self::Install), "/api/v1/uninstall" => Some(Self::Uninstall), "/api/v1/apply" => Some(Self::Apply), _ => None }
    }
    fn command(self) -> &'static str { match self { Self::Install => "install", Self::Uninstall => "uninstall", Self::Apply => "apply" } }
}
fn action_args(action: Action, project: &Path, selected_game: &Path) -> Vec<std::ffi::OsString> {
    vec!["--json".into(), "--game".into(), selected_game.as_os_str().to_os_string(), action.command().into(), project.as_os_str().to_os_string()]
}

fn run_self(args: &[std::ffi::OsString]) -> Value {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => return json!({"ok": false, "error": format!("{e}")}),
    };
    match std::process::Command::new(exe).args(args).output() {
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
    if path.contains('\\') || path.contains(':') || path.split('/').any(|p| p == "..") {
        return Err(Error::status(404, "not_found", "静态文件不存在"));
    }
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
    req.headers().iter().any(|h| h.field.equiv("Content-Type") && h.value.as_str().split(';').next().unwrap_or("").trim().eq_ignore_ascii_case("application/json"))
}

fn read_json(req: &mut Request) -> Result<Value, Error> {
    if !req_json_ct(req) { return Err(Error::status(415, "unsupported_media", "Content-Type 必须是 application/json")); }
    use std::io::Read;
    const LIMIT: u64 = 65536;
    let mut bytes = vec![];
    req.as_reader().take(LIMIT + 1).read_to_end(&mut bytes).map_err(|e| Error::status(400, "invalid_request", &e.to_string()))?;
    if bytes.len() as u64 > LIMIT { return Err(Error::status(413, "request_too_large", "JSON 请求超过 64 KiB")); }
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| Error::status(400, "invalid_json", &e.to_string()))?;
    if !value.is_object() { return Err(Error::status(400, "invalid_request", "请求正文必须是 JSON 对象")); }
    Ok(value)
}

fn validate_origin(req: &Request, port: u16) -> Result<(), Error> {
    let host = req.headers().iter().find(|h| h.field.equiv("Host")).map(|h| h.value.as_str()).unwrap_or("");
    let allowed = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if !allowed.iter().any(|s| s.eq_ignore_ascii_case(host)) {
        return Err(Error::status(403, "invalid_host", "Host 必须匹配本机预览服务地址"));
    }
    if let Some(origin) = req.headers().iter().find(|h| h.field.equiv("Origin")) {
        if !origin.value.as_str().eq_ignore_ascii_case(&format!("http://{host}")) {
            return Err(Error::status(403, "invalid_origin", "Origin 必须匹配本机预览服务地址"));
        }
    }
    Ok(())
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
fn cors(r: Response<std::io::Cursor<Vec<u8>>>) -> Response<std::io::Cursor<Vec<u8>>> {
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
        let code = if msg.contains("project_unavailable") {
            "project_unavailable"
        } else if msg.contains("session_busy") {
            "session_busy"
        } else if msg.contains("not_found") {
            "not_found"
        } else if msg.contains("no_model") {
            "no_model"
        } else {
            "internal"
        };
        let status = if code == "not_found" || code == "no_model" { 404 } else if code == "project_unavailable" || code == "session_busy" { 409 } else { 500 };
        Self::status(status, code, &msg)
    }
    fn catalog(e: anyhow::Error) -> Self { Self::status(400, "catalog_error", &format!("{e:#}")) }
    fn catalog_change(e: anyhow::Error) -> Self {
        let message = format!("{e:#}");
        if message.contains("session_busy:") { Self::from(e) }
        else if message.starts_with("目录库中没有项目 ") { Self::status(404, "not_found", &message) }
        else { Self::catalog(e) }
    }
    fn action(e: anyhow::Error) -> Self { Self::status(409, "apply_failed", &format!("{e:#}")) }
}
fn api_err(e: &Error) -> (u16, Value) {
    (e.status, e.body.clone())
}

#[cfg(test)]
mod library_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    pub struct Fixture { pub root: PathBuf, pub service: PreviewService }
    impl Fixture {
        pub fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!("wowsmod_preview_api_{}_{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            let cli = super::super::library::selected_cli(Some(root.join("missing-client")));
            let service = PreviewService::new(&cli, &root.join("home"), None).unwrap();
            Self { root, service }
        }
        pub fn project(&self, dir: &str, geometry: &[u8]) -> String {
            let p = crate::modproj::Project::init(&self.root.join(dir), "same", "UNKNOWN_Example", "loose", "").unwrap();
            if !geometry.is_empty() {
                std::fs::create_dir_all(p.root.join(".wowsmod/cache")).unwrap();
                std::fs::write(p.root.join(".wowsmod/cache/geometry.glb"), geometry).unwrap();
            }
            std::fs::write(p.root.join("edits/001_test.json"), br#"{"cmd":"tex_recolor","outputs":[],"time_unix":1}"#).unwrap();
            self.service.store().unwrap().add(&p.root).unwrap().id
        }
        pub fn request(&self, method: &str, path: &str, body: &str, content_type: &str, origin: Option<&str>) -> (u16, Vec<u8>) {
            let (port, server) = bind(0).unwrap();
            let mut socket = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            socket.set_read_timeout(Some(std::time::Duration::from_secs(15))).unwrap();
            let origin = origin.map(|s| format!("Origin: {s}\r\n")).unwrap_or_default();
            write!(socket, "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: {content_type}\r\n{origin}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            let mut request = server.recv().unwrap();
            let method = request.method().clone();
            let response = handle_service(&self.service, &method, path, &mut request, port).unwrap_or_else(|e| json_resp(e.status, &e.body));
            request.respond(response).unwrap();
            let mut raw = Vec::new(); socket.read_to_end(&mut raw).unwrap();
            let sep = raw.windows(4).position(|s| s == b"\r\n\r\n").unwrap();
            let status = String::from_utf8_lossy(&raw[..sep]).split_whitespace().nth(1).unwrap().parse().unwrap();
            (status, raw[sep+4..].to_vec())
        }
        fn json(&self, method: &str, path: &str, body: &str) -> (u16, Value) {
            let (status, bytes) = self.request(method, path, body, "application/json", None);
            (status, serde_json::from_slice(&bytes).unwrap())
        }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.root); } }

    #[test]
    fn catalog_is_read_only_and_missing_ids_do_not_use_another_project() {
        let f = Fixture::new(); let id = f.project("a", &[]);
        let before = std::fs::read(f.root.join("a/mod.toml")).unwrap();
        let (status, list) = f.json("GET", "/api/v1/library", "");
        assert_eq!(status, 200); assert_eq!(list["projects"][0]["id"], id);
        assert!(!f.root.join("a/.wowsmod").exists());
        assert_eq!(std::fs::read(f.root.join("a/mod.toml")).unwrap(), before);
        assert_eq!(f.json("GET", "/api/v1/projects/no-such/project", "").0, 404);
        std::fs::remove_file(f.root.join("a/mod.toml")).unwrap();
        let (status, error) = f.json("GET", &format!("/api/v1/projects/{id}/project"), "");
        assert_eq!(status, 409); assert_eq!(error["error"]["code"], "project_unavailable");
    }

    #[test]
    fn two_same_name_same_revision_sessions_have_scoped_geometry_and_jobs() {
        let f = Fixture::new(); let a = f.project("a", b"geometry-a"); let b = f.project("b", b"geometry-b");
        let sa = f.service.session(&a).unwrap(); let sb = f.service.session(&b).unwrap();
        assert_eq!(sa.head_id(), sb.head_id()); assert!(!Arc::ptr_eq(&sa, &sb));
        sa.jobs.lock().unwrap().insert("same-job".into(), json!({"state":"done", "result": {"url":"/api/v1/blob/a.png"}}));
        sb.jobs.lock().unwrap().insert("same-job".into(), json!({"state":"failed"}));
        let (status, model) = f.json("GET", &format!("/api/v1/projects/{a}/model"), "");
        assert_eq!(status, 200);
        let url = model["geometry"]["url"].as_str().unwrap();
        assert!(url.starts_with(&format!("/api/v1/projects/{a}/geometry/")));
        assert_eq!(f.request("GET", url, "", "application/json", None).1, b"geometry-a");
        assert_eq!(f.json("GET", &url.replace(&a, &b), "").0, 404);
        let (_, job) = f.json("GET", &format!("/api/v1/projects/{a}/jobs/same-job"), "");
        assert_eq!(job["result"]["url"], format!("/api/v1/projects/{a}/blob/a.png"));
        assert_eq!(f.json("GET", &format!("/api/v1/projects/{b}/jobs/same-job"), "").1["state"], "failed");
    }

    #[test]
    fn scoped_texture_pngs_cannot_be_read_through_another_project() {
        use wowsmod_formats::{dds::Format, texture::{encode_dds, Image, Quality, read_png}};
        let f = Fixture::new(); let a = f.project("a", b"geometry"); let b = f.project("b", b"geometry");
        for (dir, color) in [("a", [255,0,0,255]), ("b", [0,0,255,255])] {
            let root = f.root.join(dir).join("assets/res_mods/PnFMods/same/UNKNOWN_Example");
            std::fs::create_dir_all(&root).unwrap();
            let pixels = Image { width:4, height:4, px: color.repeat(16) };
            let dds = encode_dds(&[pixels], Format::Rgba8, Quality::Fast, None).unwrap();
            std::fs::write(root.join("paint.dds"), dds.to_bytes()).unwrap();
            std::fs::write(root.join("paint.mfm"), "<mfm><fx>PBS_ship.fx</fx><property>diffuseMap<Texture>PnFMods/same/UNKNOWN_Example/paint.dds</Texture></property></mfm>").unwrap();
        }
        let (status, model) = f.json("GET", &format!("/api/v1/projects/{a}/model"), "");
        assert_eq!(status, 200);
        let url = model["materials"]["paint.mfm"]["textures"]["a"]["url"].as_str().unwrap();
        assert!(url.starts_with(&format!("/api/v1/projects/{a}/blob/")));
        let (status, png) = f.request("GET", url, "", "application/json", None);
        assert_eq!(status, 200); assert_eq!(read_png(&png).unwrap().get(0,0), [255,0,0,255]);
        assert_eq!(f.json("GET", &url.replace(&a, &b), "").0, 404);
        let (_, textures) = f.json("GET", &format!("/api/v1/projects/{b}/textures"), "");
        let url = textures["textures"][0]["url"].as_str().unwrap();
        assert!(url.starts_with(&format!("/api/v1/projects/{b}/blob/")));
        let (_, png) = f.request("GET", url, "", "application/json", None);
        assert_eq!(read_png(&png).unwrap().get(0,0), [0,0,255,255]);
    }

    #[test]
    fn catalog_http_mutations_preserve_id_and_only_unregister() {
        let f = Fixture::new(); let id = f.project("a", &[]);
        let route = format!("/api/v1/library/projects/{id}");
        let (_, updated) = f.json("POST", &format!("{route}/update"), r#"{"display_name":"Edited","favorite":true,"tags":["paint"],"status":"review"}"#);
        assert_eq!(updated["entry"]["favorite"], true);
        assert_eq!(f.json("POST", &format!("{route}/update"), r#"{"status":"invalid"}"#).0, 400);
        assert_eq!(f.json("POST", &format!("{route}/relocate"), &json!({"path": f.root.join("absent")}).to_string()).0, 400);
        assert_eq!(f.json("POST", &format!("{route}/open"), "{}").1["url"], format!("/?project={id}"));
        assert!(!f.root.join("a/.wowsmod").exists());
        std::fs::rename(f.root.join("a"), f.root.join("moved")).unwrap();
        let body = json!({"path": f.root.join("moved")}).to_string();
        assert_eq!(f.json("POST", &format!("{route}/relocate"), &body).1["entry"]["id"], id);
        assert_eq!(f.json("POST", &format!("{route}/remove"), "{}").0, 200);
        assert!(f.root.join("moved/mod.toml").is_file());
        assert_eq!(f.json("POST", &format!("{route}/open"), "{}").0, 404);
    }

    #[test]
    fn no_game_actions_validate_json_and_never_spawn_or_autodetect() {
        let f = Fixture::new(); let id = f.project("a", &[]); let prefix = format!("/api/v1/projects/{id}");
        for action in ["apply", "install", "uninstall"] {
            assert_eq!(f.json("POST", &format!("{prefix}/{action}"), "{\"dry_run\":false}").0, 409);
        }
        assert_eq!(f.json("POST", &format!("{prefix}/apply"), "not json").0, 400);
        assert_eq!(f.json("POST", &format!("{prefix}/apply"), "{}").0, 400);
        assert_eq!(f.request("POST", &format!("{prefix}/apply"), "{}", "text/plain", None).0, 415);
        assert_eq!(f.request("POST", "/api/v1/library/add", "{}", "application/json", Some("https://evil.example")).0, 403);
        assert!(f.service.session(&id).unwrap().jobs.lock().unwrap().is_empty());
    }

    #[test]
    fn action_dispatch_and_selected_client_are_exact() {
        let project = Path::new("fixture project"); let client = Path::new("unavailable selected client");
        for (route, command) in [("install", "install"), ("uninstall", "uninstall"), ("apply", "apply")] {
            let action = Action::from_path(&format!("/api/v1/{route}")).unwrap();
            let args = action_args(action, project, client);
            assert_eq!(args, ["--json", "--game", "unavailable selected client", command, "fixture project"].map(std::ffi::OsString::from));
        }
        assert!(Action::from_path("/api/v1/something/uninstall").is_none());
        assert!(Action::from_path("/api/v1/install-more").is_none());
    }

    #[test]
    fn legacy_render_routes_keep_unscoped_model_and_events() {
        let mut f = Fixture::new(); let id = f.project("a", b"legacy-geometry");
        f.service.default = Some(f.service.session(&id).unwrap());
        assert_eq!(f.json("GET", "/api/v1/health", "").1["library_mode"], false);
        let (status, model) = f.json("GET", "/api/v1/model", "");
        assert_eq!(status, 200);
        assert!(model["geometry"]["url"].as_str().unwrap().starts_with("/api/v1/geometry/"));
        assert_eq!(f.request("GET", model["geometry"]["url"].as_str().unwrap(), "", "application/json", None).1, b"legacy-geometry");
        let (status, events) = f.request("GET", "/api/v1/events", "", "application/json", None);
        assert_eq!(status, 200); assert!(String::from_utf8(events).unwrap().contains("event: hello"));
        assert_eq!(f.request("GET", "/?library=1", "", "application/json", None).0, 200);
        assert_eq!(f.json("GET", "/api/v1/library", "").0, 200);
    }

    #[test]
    fn warm_control_hook_and_shutdown_remain_compatible() {
        let f = Fixture::new(); let id = f.project("a", b"warm-geometry");
        let state = f.service.session(&id).unwrap();
        let (port, server) = bind(0).unwrap();
        let control: Ctl = Arc::new(|method, url, _| {
            (method == &Method::Get && url == "/__wm/status").then(|| json_resp(200, &json!({"ok":true,"warm":true})))
        });
        let child = Arc::clone(&state);
        let server_thread = std::thread::spawn(move || serve_with(server, child, Some(control)).unwrap());
        let get = |path: &str| {
            let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            write!(stream, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").unwrap();
            let mut data = String::new(); stream.read_to_string(&mut data).unwrap(); data
        };
        let result = get("/__wm/status");
        assert!(result.starts_with("HTTP/1.1 200")); assert!(result.contains("\"warm\":true"));
        assert!(get("/api/v1/model").contains("/api/v1/geometry/"));
        state.shutdown(); let _ = get("/__wake"); server_thread.join().unwrap();
    }
}
