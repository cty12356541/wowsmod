//! On-demand library covers. Request threads only resolve identity and enqueue work;
//! fingerprints, preview sessions and browsers belong exclusively to the serial worker.
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{cdp, library::{path_identity, same_path, PreviewService}, state};
use crate::{library::atomic_write, modproj::sha256_hex};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const MAX_QUEUE: usize = 16;
const MAX_STATES: usize = 128;
const CHECK_INTERVAL: Duration = Duration::from_secs(30);
const MAX_PNG_BYTES: u64 = 4 * 1024 * 1024;
const RECIPE: &str = "rev=head&compare=single&view=deck&w=640&h=360&wm=0&look=game-day&env=studio&lod=0";

#[derive(Clone, Default, Deserialize, Serialize)]
struct Descriptor {
    schema: u32,
    key: String,
    input: String,
    png: Option<String>,
    failed_input: Option<String>,
    reason: Option<String>,
    #[serde(default)]
    external: Value,
    #[serde(default)]
    geometry: Value,
}

struct Entry {
    descriptor: Descriptor,
    state: &'static str,
    reason: Option<String>,
    checked: Option<Instant>,
    used: Instant,
}
impl Entry {
    fn pending(&self) -> bool { matches!(self.state, "queued" | "rendering") }
    fn value(&self, id: &str) -> Value {
        let url = self.descriptor.png.as_ref().map(|hash| format!("/api/v1/library/projects/{}/thumbnail/{hash}.png", encode_id(id)));
        json!({"ok": true, "state": self.state, "url": url,
            "stale": self.descriptor.png.is_some() && self.state != "ready", "reason": self.reason,
            "width": WIDTH, "height": HEIGHT})
    }
}
#[derive(Clone)]
struct Work { id: String, source: PathBuf, key: String, port: u16, force: bool }
#[derive(Default)]
struct Queue { entries: HashMap<String, Entry>, jobs: VecDeque<Work>, running: bool }

pub(super) struct ThumbnailCache { root: PathBuf, queue: Mutex<Queue> }
impl ThumbnailCache {
    pub(super) fn new(root: PathBuf) -> Self { Self { root, queue: Mutex::new(Queue::default()) } }

    pub(super) fn request(&self, service: &PreviewService, id: &str, port: u16, force: bool) -> Result<Value> {
        let source = match service.source(id) {
            Ok(source) => source,
            Err(e) if format!("{e:#}").starts_with("project_unavailable:") => return Ok(json!({
                "ok": true, "state": "missing", "url": null, "stale": false,
                "reason": "项目目录不可用，请重新定位或修复 mod.toml", "width": WIDTH, "height": HEIGHT})),
            Err(e) => return Err(e),
        };
        let work = Work { id: id.into(), key: cache_key(id, &source), source, port, force };
        let (value, start) = self.enqueue(work);
        if start {
            let service = service.clone();
            // A single, short-lived worker owns all queued jobs. No lock is held
            // while CDP calls back into this very same HTTP service.
            std::thread::spawn(move || service.thumbnails.drain(&service, &render));
        }
        Ok(value)
    }

    fn enqueue(&self, work: Work) -> (Value, bool) {
        let mut queue = self.queue.lock().unwrap();
        let now = Instant::now();
        if !queue.entries.contains_key(&work.key) {
            if queue.entries.len() >= MAX_STATES {
                let oldest = queue.entries.iter().filter(|(_, e)| !e.pending()).min_by_key(|(_, e)| e.used).map(|(k, _)| k.clone());
                if let Some(key) = oldest { queue.entries.remove(&key); }
            }
            let descriptor = self.load(&work.key);
            let state = if descriptor.failed_input.is_some() { "failed" } else if descriptor.png.is_some() { "ready" } else { "missing" };
            queue.entries.insert(work.key.clone(), Entry { reason: descriptor.reason.clone(), descriptor, state, checked: None, used: now });
        }
        let entry = queue.entries.get_mut(&work.key).unwrap();
        entry.used = now;
        if entry.pending() || (!work.force && entry.checked.map(|t| t.elapsed() < CHECK_INTERVAL).unwrap_or(false)) {
            return (entry.value(&work.id), false);
        }
        if queue.jobs.len() >= MAX_QUEUE {
            let entry = queue.entries.get_mut(&work.key).unwrap();
            entry.state = "failed";
            entry.reason = Some("封面队列繁忙，请稍后重试".into());
            entry.checked = Some(now);
            return (entry.value(&work.id), false);
        }
        let entry = queue.entries.get_mut(&work.key).unwrap();
        entry.state = "queued";
        entry.reason = None;
        let value = entry.value(&work.id);
        queue.jobs.push_back(work);
        let start = !queue.running;
        queue.running = true;
        (value, start)
    }

    fn drain(&self, service: &PreviewService, renderer: &dyn Fn(&PreviewService, &Work) -> Result<Rendered>) {
        loop {
            let work = {
                let mut queue = self.queue.lock().unwrap();
                let Some(work) = queue.jobs.pop_front() else { queue.running = false; return; };
                queue.entries.get_mut(&work.key).unwrap().state = "rendering";
                work
            };
            // An unexpected renderer panic must not permanently strand the queue.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.process(service, &work, renderer)))
                .unwrap_or_else(|_| Err(anyhow!("封面渲染异常，请重试")));
            let mut queue = self.queue.lock().unwrap();
            let entry = queue.entries.get_mut(&work.key).unwrap();
            match result {
                Ok(descriptor) => {
                    entry.state = if descriptor.failed_input.is_some() { "failed" } else { "ready" };
                    entry.reason = descriptor.reason.clone(); entry.descriptor = descriptor;
                }
                Err(e) => { entry.state = "failed"; entry.reason = Some(format!("{e:#}")); }
            }
            entry.checked = Some(Instant::now());
        }
    }

    fn load(&self, key: &str) -> Descriptor {
        let path = self.root.join(key).join("cover.json");
        let read = || -> Result<Descriptor> {
            if std::fs::metadata(&path)?.len() > 65536 { bail!("descriptor too large"); }
            let mut d: Descriptor = serde_json::from_slice(&std::fs::read(&path)?)?;
            if d.schema != 1 || d.key != key { bail!("descriptor identity mismatch"); }
            if d.png.as_ref().map(|h| !valid_hash(h) || !self.root.join(key).join(format!("{h}.png")).is_file()).unwrap_or(false) {
                d.png = None; d.input.clear(); d.failed_input = None;
            }
            Ok(d)
        };
        read().unwrap_or_else(|_| Descriptor { schema: 1, key: key.into(), ..Descriptor::default() })
    }

    fn process(&self, service: &PreviewService, work: &Work, renderer: &dyn Fn(&PreviewService, &Work) -> Result<Rendered>) -> Result<Descriptor> {
        verify_source(service, work)?;
        let source_before = state::project_source_fingerprint(&work.source);
        let context_before = context(service);
        let input = fingerprint(&work.source, &context_before);
        let mut descriptor = self.load(&work.key);
        if let Some(hash) = &descriptor.png {
            if self.read_png(&work.key, hash).is_err() { descriptor.png = None; descriptor.input.clear(); descriptor.failed_input = None; }
        }
        if !work.force {
            if descriptor.png.is_some() && descriptor.input == input && descriptor.failed_input.is_none() { return Ok(descriptor); }
            if descriptor.failed_input.as_deref() == Some(&input) { return Ok(descriptor); }
        }
        let result = (|| -> Result<Rendered> {
            // External resources do not participate in PreviewState's existing
            // geometry cache contract. Refuse to bless its previous model as a
            // new external-input snapshot, including on an explicit retry.
            if !descriptor.external.is_null() && descriptor.external != context_before["external"]
                && descriptor.geometry["bytes"].is_number()
                && descriptor.geometry == stamp(&work.source.join(".wowsmod/cache/geometry.glb")) {
                bail!("外部模型资源已变化，请先重建项目模型，再重试封面");
            }
            let rendered = renderer(service, work)?;
            validate_png(&rendered.png)?;
            // Geometry assembly may legitimately write .wowsmod/cache on the
            // first visit. Compare only source inputs across that operation.
            if source_before != state::project_source_fingerprint(&work.source) || context_before != context(service) {
                bail!("项目或客户端在出图期间发生变化，请重试");
            }
            verify_source(service, work)?;
            Ok(rendered)
        })();
        match result {
            Ok(rendered) => {
                let hash = sha256_hex(&rendered.png);
                let dir = self.root.join(&work.key);
                atomic_write(&dir.join(format!("{hash}.png")), &rendered.png)?;
                // Keep the session lease until the descriptor has been atomically
                // published; remove/relocate therefore retain their busy guard.
                verify_source(service, work)?;
                let old_hash = descriptor.png.clone();
                descriptor.png = Some(hash.clone());
                descriptor.input = fingerprint(&work.source, &context_before);
                if source_before != state::project_source_fingerprint(&work.source) || context_before != context(service) {
                    bail!("项目或客户端在发布封面前发生变化，请重试");
                }
                descriptor.failed_input = None; descriptor.reason = None;
                descriptor.external = context_before["external"].clone();
                descriptor.geometry = stamp(&work.source.join(".wowsmod/cache/geometry.glb"));
                self.save(&work.key, &descriptor)?;
                drop(rendered);
                self.prune(&dir, &hash, old_hash.as_deref());
            }
            Err(error) => {
                verify_source(service, work)?;
                if descriptor.external.is_null() {
                    descriptor.external = context_before["external"].clone();
                    descriptor.geometry = stamp(&work.source.join(".wowsmod/cache/geometry.glb"));
                }
                descriptor.failed_input = Some(input);
                descriptor.reason = Some(format!("{error:#}"));
                self.save(&work.key, &descriptor)?;
            }
        }
        self.prune_identities(&work.key);
        Ok(descriptor)
    }

    fn save(&self, key: &str, descriptor: &Descriptor) -> Result<()> {
        atomic_write(&self.root.join(key).join("cover.json"), &serde_json::to_vec(descriptor)?)
    }

    fn read_png(&self, key: &str, hash: &str) -> Result<Vec<u8>> {
        if !valid_hash(hash) { bail!("not_found: 无效封面版本"); }
        let path = self.root.join(key).join(format!("{hash}.png"));
        let metadata = std::fs::metadata(&path).map_err(|_| anyhow!("not_found: 封面不存在"))?;
        if metadata.len() > MAX_PNG_BYTES { bail!("not_found: 封面文件无效"); }
        let bytes = std::fs::read(path)?;
        if sha256_hex(&bytes) != hash { bail!("not_found: 封面校验失败"); }
        validate_png(&bytes).map_err(|_| anyhow!("not_found: 封面文件损坏"))?;
        Ok(bytes)
    }

    pub(super) fn image(&self, service: &PreviewService, id: &str, hash: &str) -> Result<Vec<u8>> {
        let source = service.source(id)?;
        let bytes = self.read_png(&cache_key(id, &source), hash)?;
        if !same_path(&source, &service.source(id)?) { bail!("not_found: 项目已重新定位"); }
        Ok(bytes)
    }

    fn prune(&self, dir: &Path, current: &str, previous: Option<&str>) {
        // Only our strict content-addressed filenames, in this identity's own
        // directory. Preserve the preceding image for outstanding browser GETs.
        if let Ok(files) = std::fs::read_dir(dir) {
            for file in files.flatten() {
                let name = file.file_name(); let name = name.to_string_lossy();
                if let Some(hash) = name.strip_suffix(".png") {
                    if valid_hash(hash) && hash != current && Some(hash) != previous { let _ = std::fs::remove_file(file.path()); }
                }
            }
        }
    }

    fn prune_identities(&self, current: &str) {
        let protected: Vec<_> = self.queue.lock().unwrap().jobs.iter().map(|job| job.key.clone()).collect();
        let Ok(dirs) = std::fs::read_dir(&self.root) else { return; };
        let mut owned: Vec<_> = dirs.flatten().filter_map(|dir| {
            let key = dir.file_name().to_string_lossy().into_owned();
            // Never descend through a symlink/junction or an unknown directory.
            if !valid_hash(&key) || !dir.file_type().ok()?.is_dir() { return None; }
            let path = dir.path();
            let descriptor: Descriptor = serde_json::from_slice(&std::fs::read(path.join("cover.json")).ok()?).ok()?;
            if descriptor.schema != 1 || descriptor.key != key { return None; }
            Some((std::fs::metadata(path.join("cover.json")).ok()?.modified().ok()?, key, path))
        }).collect();
        let mut excess = owned.len().saturating_sub(MAX_STATES);
        owned.sort_by_key(|(time, _, _)| *time);
        for (_, key, path) in owned {
            if excess == 0 { break; }
            if key == current || protected.contains(&key) { continue; }
            let Ok(files) = std::fs::read_dir(&path) else { continue; };
            for file in files.flatten() {
                let name = file.file_name().to_string_lossy().into_owned();
                if file.file_type().map(|t| t.is_file()).unwrap_or(false)
                    && (name == "cover.json" || name.strip_suffix(".png").map(valid_hash).unwrap_or(false)) {
                    let _ = std::fs::remove_file(file.path());
                }
            }
            // Non-recursive removal deliberately leaves any unrelated files.
            if std::fs::remove_dir(&path).is_ok() { excess -= 1; }
        }
    }
}

struct Rendered { png: Vec<u8>, _lease: Option<Arc<state::PreviewState>> }
fn render(service: &PreviewService, work: &Work) -> Result<Rendered> {
    if cdp::find_browser(None).is_none() { bail!("browser_not_found: 需要 Microsoft Edge 或 Google Chrome"); }
    let lease = service.session(&work.id)?;
    if !same_path(&lease.project.root, &work.source) { bail!("项目已重新定位"); }
    let current = service.selected_game.as_deref().and_then(|p| crate::game::describe(p, "thumbnail").ok());
    if current.as_ref().map(|g| &g.build) != lease.game.as_ref().map(|g| &g.build) {
        bail!("客户端版本已变化，请重启预览服务后重试");
    }
    lease.model_view(Some("head"))?;
    let rendered_input = lease.fingerprint();
    let base = format!("http://127.0.0.1:{}/?project={}", work.port, encode_id(&work.id));
    let mut images = cdp::render_session(&base, &[("library-cover".into(), RECIPE.into())], None, (WIDTH, HEIGHT))?;
    if rendered_input != lease.fingerprint() { bail!("模型缓存或源文件在出图期间发生变化，请重试"); }
    let (png, _) = images.pop().ok_or_else(|| anyhow!("封面渲染未返回图片"))?;
    Ok(Rendered { png, _lease: Some(lease) })
}

fn verify_source(service: &PreviewService, work: &Work) -> Result<()> {
    if !same_path(&service.source(&work.id)?, &work.source) { bail!("not_found: 项目已重新定位"); }
    Ok(())
}
fn cache_key(id: &str, source: &Path) -> String { sha256_hex(&serde_json::to_vec(&(id, path_identity(source))).unwrap()) }
fn valid_hash(hash: &str) -> bool { hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) }
pub(super) fn encode_id(id: &str) -> String {
    id.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}
fn fingerprint(source: &Path, context: &Value) -> String {
    sha256_hex(&serde_json::to_vec(&(state::project_fingerprint(source), context)).unwrap())
}
fn stamp(path: &Path) -> Value {
    let metadata = std::fs::metadata(path).ok();
    json!({"path": path_identity(path), "bytes": metadata.as_ref().map(|m| m.len()),
        "mtime": metadata.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos().to_string())})
}
fn context(service: &PreviewService) -> Value {
    let selected = service.selected_game.as_ref();
    let game = selected.and_then(|p| crate::game::describe(p, "thumbnail").ok());
    let external = external_context(std::env::var_os("WOWSMOD_GP").as_deref(), std::env::var_os("WOWSMOD_GEOM_DIR").as_deref());
    json!({"schema": 1, "recipe": RECIPE, "web": super::http::web_fingerprint(), "version": env!("CARGO_PKG_VERSION"),
        "selected": selected.map(|p| path_identity(p)), "available": game.is_some(), "build": game.as_ref().map(|g| &g.build),
        "client_version": game.as_ref().and_then(|g| g.client_version.as_ref()),
        "client_info": selected.map(|p| stamp(&p.join("game_info.xml"))),
        "packages": game.as_ref().map(|g| stamp(&g.res_packages)), "index": game.as_ref().map(|g| stamp(&g.idx_dir)), "external": external})
}
fn external_context(gp: Option<&std::ffi::OsStr>, geometry_dirs: Option<&std::ffi::OsStr>) -> Value {
    let directories: Vec<_> = geometry_dirs.into_iter().flat_map(std::env::split_paths).map(|path| {
        json!({"stamp": stamp(&path), "contents": if path.is_dir() { Some(state::project_fingerprint(&path)) } else { None }})
    }).collect();
    json!({"gp": gp.map(|path| stamp(Path::new(path))), "geometry_dirs": directories})
}
fn validate_png(bytes: &[u8]) -> Result<()> {
    // Bound the declared dimensions before invoking the decoder/allocation.
    if bytes.len() < 24 || bytes.len() as u64 > MAX_PNG_BYTES || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
        || &bytes[12..16] != b"IHDR" || u32::from_be_bytes(bytes[16..20].try_into()?) != WIDTH
        || u32::from_be_bytes(bytes[20..24].try_into()?) != HEIGHT { bail!("封面必须是 640 × 360 PNG"); }
    let image = wowsmod_formats::texture::read_png(bytes)?;
    if image.width != WIDTH || image.height != HEIGHT { bail!("封面尺寸不符"); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    struct Fixture { root: PathBuf, service: PreviewService }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!("wowsmod_thumbnails_{}_{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            let service = PreviewService::new(&super::super::library::selected_cli(Some(root.join("missing-client"))), &root.join("home"), None).unwrap();
            Self { root, service }
        }
        fn project(&self, name: &str) -> Work {
            let project = crate::modproj::Project::init(&self.root.join(name), "same", "UNKNOWN", "loose", "123").unwrap();
            let id = self.service.store().unwrap().add(&project.root).unwrap().id;
            let source = self.service.source(&id).unwrap();
            Work { key: cache_key(&id, &source), id, source, port: 0, force: false }
        }
        fn cache(&self) -> &ThumbnailCache { &self.service.thumbnails }
        fn expire(&self, work: &Work) { self.cache().queue.lock().unwrap().entries.get_mut(&work.key).unwrap().checked = None; }
        fn value(&self, work: &Work) -> Value { self.cache().queue.lock().unwrap().entries[&work.key].value(&work.id) }
        fn run(&self, work: &Work, renderer: &dyn Fn(&PreviewService, &Work) -> Result<Rendered>) {
            self.expire_if_present(work);
            self.cache().enqueue(work.clone());
            self.cache().drain(&self.service, renderer);
        }
        fn expire_if_present(&self, work: &Work) {
            if let Some(entry) = self.cache().queue.lock().unwrap().entries.get_mut(&work.key) { entry.checked = None; }
        }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.root); } }
    fn png(color: [u8; 4]) -> Vec<u8> {
        wowsmod_formats::texture::write_png(&wowsmod_formats::texture::Image { width: WIDTH, height: HEIGHT, px: color.repeat((WIDTH * HEIGHT) as usize) }, false).unwrap()
    }
    fn output() -> Result<Rendered> { Ok(Rendered { png: png([12, 34, 56, 255]), _lease: None }) }

    #[test]
    fn thumbnail_queue_deduplicates_polling_and_reuses_disk_without_preview_sessions() {
        let f = Fixture::new(); let work = f.project("a");
        assert_eq!(f.cache().enqueue(work.clone()).0["state"], "queued");
        for _ in 0..50 { assert_eq!(f.cache().enqueue(work.clone()).0["state"], "queued"); }
        assert_eq!(f.cache().queue.lock().unwrap().jobs.len(), 1);
        f.cache().drain(&f.service, &|service, task| {
            for _ in 0..50 { assert_eq!(service.thumbnails.request(service, &task.id, 0, false)?["state"], "rendering"); }
            output()
        });
        let value = f.value(&work);
        assert_eq!(value["state"], "ready"); assert_eq!(value["stale"], false);
        assert!(value["url"].as_str().unwrap().starts_with(&format!("/api/v1/library/projects/{}/thumbnail/", work.id)));
        assert!(!work.source.join(".wowsmod").exists(), "even a disk-cache check must not open PreviewState");
        let cache = ThumbnailCache::new(f.root.join("home/library-thumbnails"));
        assert_eq!(cache.enqueue(work.clone()).0["url"], value["url"]);
        cache.drain(&f.service, &|_, _| panic!("cache hit must not render"));
        assert_eq!(cache.queue.lock().unwrap().entries[&work.key].value(&work.id)["state"], "ready");
    }

    #[test]
    fn thumbnail_identity_assets_and_corruption_control_refresh() {
        let f = Fixture::new(); let a = f.project("a"); let b = f.project("b");
        f.run(&a, &|_, _| output());
        f.run(&b, &|_, _| Ok(Rendered { png: png([255, 0, 0, 255]), _lease: None }));
        let ah = f.cache().load(&a.key).png.unwrap(); let bh = f.cache().load(&b.key).png.unwrap();
        assert_ne!(ah, bh); assert_ne!(a.key, b.key);
        assert!(f.cache().image(&f.service, &b.id, &ah).is_err());
        std::fs::write(a.source.join("assets/paint.txt"), b"new asset without a new head").unwrap();
        let renders = AtomicUsize::new(0);
        f.run(&a, &|_, _| { renders.fetch_add(1, Ordering::Relaxed); output() });
        assert_eq!(renders.load(Ordering::Relaxed), 1);
        std::fs::write(f.cache().root.join(&a.key).join(format!("{ah}.png")), b"broken").unwrap();
        assert!(f.cache().image(&f.service, &a.id, &ah).is_err());
        f.run(&a, &|_, _| { renders.fetch_add(1, Ordering::Relaxed); output() });
        assert_eq!(renders.load(Ordering::Relaxed), 2);
        assert!(f.cache().image(&f.service, &a.id, &ah).is_ok());
    }

    #[test]
    fn thumbnail_failure_keeps_previous_png_and_requires_change_or_explicit_retry() {
        let f = Fixture::new(); let mut work = f.project("a");
        f.run(&work, &|_, _| output()); let url = f.value(&work)["url"].clone();
        std::fs::write(work.source.join("assets/paint.txt"), b"new").unwrap();
        f.run(&work, &|_, _| bail!("browser unavailable"));
        assert_eq!(f.value(&work)["state"], "failed"); assert_eq!(f.value(&work)["url"], url); assert_eq!(f.value(&work)["stale"], true);
        f.run(&work, &|_, _| panic!("same failed input must not start another browser"));
        assert_eq!(f.value(&work)["reason"], "browser unavailable");
        // Failure backoff also survives process restart / state eviction.
        let cache = ThumbnailCache::new(f.cache().root.clone());
        cache.enqueue(work.clone()); cache.drain(&f.service, &|_, _| panic!("persisted failure must not render"));
        assert_eq!(cache.queue.lock().unwrap().entries[&work.key].value(&work.id)["state"], "failed");
        work.force = true; f.run(&work, &|_, _| output());
        assert_eq!(f.value(&work)["state"], "ready"); assert_eq!(f.value(&work)["stale"], false);
    }

    #[test]
    fn thumbnail_cache_generation_is_allowed_but_concurrent_source_edits_are_rejected() {
        let f = Fixture::new(); let mut work = f.project("a");
        f.run(&work, &|_, task| {
            std::fs::create_dir_all(task.source.join(".wowsmod/cache"))?;
            std::fs::write(task.source.join(".wowsmod/cache/geometry.glb"), b"first assembled geometry")?;
            output()
        });
        assert_eq!(f.value(&work)["state"], "ready"); let previous = f.value(&work)["url"].clone();
        f.run(&work, &|_, _| panic!("first geometry generation must be the new cache baseline"));
        assert_eq!(f.value(&work)["state"], "ready");
        work.force = true;
        f.run(&work, &|_, task| { std::fs::write(task.source.join("assets/during.txt"), b"changed while rendering")?; output() });
        assert_eq!(f.value(&work)["state"], "failed"); assert_eq!(f.value(&work)["url"], previous);
        assert!(f.value(&work)["reason"].as_str().unwrap().contains("发生变化"));
        work.force = false; f.run(&work, &|_, _| output());
        assert_eq!(f.value(&work)["state"], "ready", "new source fingerprint recovers automatically");
    }

    #[test]
    fn thumbnail_removal_and_relocation_invalidate_old_urls_and_pending_jobs() {
        let f = Fixture::new(); let work = f.project("a"); let replacement = f.project("b");
        f.run(&work, &|_, _| output()); let hash = f.cache().load(&work.key).png.unwrap();
        f.service.remove(&replacement.id).unwrap();
        f.service.relocate(&work.id, &replacement.source).unwrap();
        assert!(f.cache().image(&f.service, &work.id, &hash).is_err());
        f.run(&work, &|_, _| panic!("relocated queued identity must never render another project"));
        assert_eq!(f.value(&work)["state"], "failed");
        f.service.remove(&work.id).unwrap();
        assert!(f.cache().image(&f.service, &work.id, &hash).is_err());
        assert!(work.source.join("mod.toml").is_file());
    }

    #[test]
    fn thumbnail_queue_and_memory_are_bounded_and_full_queue_can_retry() {
        let f = Fixture::new(); let template = f.project("a");
        for n in 0..MAX_STATES + 12 {
            let mut work = template.clone(); work.id = format!("id{n}"); work.key = cache_key(&work.id, &work.source);
            f.cache().enqueue(work);
        }
        let queue = f.cache().queue.lock().unwrap();
        assert_eq!(queue.jobs.len(), MAX_QUEUE); assert!(queue.entries.len() <= MAX_STATES);
        assert_eq!(queue.entries.values().filter(|entry| entry.pending()).count(), MAX_QUEUE);
        drop(queue);
        // An overflow response is failed, never a queued job that cannot run.
        let overflow = f.cache().enqueue(template.clone()).0;
        assert_eq!(overflow["state"], "failed"); assert!(overflow["reason"].as_str().unwrap().contains("队列"));
        f.cache().drain(&f.service, &|_, _| panic!("unknown IDs are rejected before rendering"));
        f.expire(&template);
        assert_eq!(f.cache().enqueue(template).0["state"], "queued");
    }

    #[test]
    fn thumbnail_id_and_png_validation_never_create_user_controlled_paths() {
        let f = Fixture::new(); let work = f.project("a");
        let id = "../other/船 ?#%";
        assert!(valid_hash(&cache_key(id, &work.source)));
        assert_eq!(encode_id(id), "..%2Fother%2F%E8%88%B9%20%3F%23%25");
        assert!(!valid_hash(&"A".repeat(64))); assert!(!valid_hash("../../elsewhere"));
        assert!(f.cache().image(&f.service, &work.id, "../../mod.toml").is_err());
        assert!(validate_png(b"broken").is_err());
        let mut image = png([1, 2, 3, 255]); image[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(validate_png(&image).is_err());
    }

    #[test]
    fn thumbnail_context_detects_client_availability_and_in_place_build_change() {
        let f = Fixture::new(); let root = f.service.selected_game.as_ref().unwrap();
        let unavailable = context(&f.service);
        std::fs::create_dir_all(root.join("res_packages")).unwrap();
        std::fs::create_dir_all(root.join("bin/100/idx")).unwrap();
        std::fs::write(root.join("bin/100/idx/test.idx"), b"fixture").unwrap();
        let first = context(&f.service); assert_ne!(unavailable, first); assert_eq!(first["build"], "100");
        std::fs::create_dir_all(root.join("bin/101/idx")).unwrap();
        std::fs::write(root.join("bin/101/idx/test.idx"), b"fixture").unwrap();
        let second = context(&f.service); assert_ne!(first, second); assert_eq!(second["build"], "101");
    }

    #[test]
    fn thumbnail_owned_disk_cache_is_bounded_without_removing_unrelated_files() {
        let f = Fixture::new(); let work = f.project("a");
        for n in 0..MAX_STATES + 4 {
            let key = sha256_hex(format!("old{n}").as_bytes());
            f.cache().save(&key, &Descriptor { schema: 1, key: key.clone(), ..Descriptor::default() }).unwrap();
        }
        std::fs::create_dir_all(f.cache().root.join("user-folder")).unwrap();
        std::fs::write(f.cache().root.join("user-folder/keep.txt"), b"unrelated").unwrap();
        f.run(&work, &|_, _| output());
        let owned = std::fs::read_dir(&f.cache().root).unwrap().flatten().filter(|e| valid_hash(&e.file_name().to_string_lossy())).count();
        assert!(owned <= MAX_STATES, "old identities must not grow the disk cache forever: {owned}");
        assert_eq!(std::fs::read(f.cache().root.join("user-folder/keep.txt")).unwrap(), b"unrelated");
        assert!(f.cache().load(&work.key).png.is_some());
    }

    #[test]
    fn thumbnail_external_changes_require_rebuilt_geometry_even_for_explicit_retry() {
        let f = Fixture::new(); let mut work = f.project("a");
        std::fs::create_dir_all(work.source.join(".wowsmod/cache")).unwrap();
        std::fs::write(work.source.join(".wowsmod/cache/geometry.glb"), b"existing assembled geometry").unwrap();
        f.run(&work, &|_, _| output());
        let url = f.value(&work)["url"].clone();
        let file = f.cache().root.join(&work.key).join("cover.json");
        let mut descriptor: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        descriptor["external"] = json!("previous external resources");
        descriptor["input"] = json!("previous input");
        atomic_write(&file, &serde_json::to_vec(&descriptor).unwrap()).unwrap();
        work.force = true;
        for _ in 0..2 {
            f.run(&work, &|_, _| output());
            assert_eq!(f.value(&work)["state"], "failed");
            assert_eq!(f.value(&work)["url"], url);
            assert!(f.value(&work)["reason"].as_str().unwrap().contains("外部"));
        }
        std::fs::create_dir_all(work.source.join(".wowsmod/cache")).unwrap();
        std::fs::write(work.source.join(".wowsmod/cache/geometry.glb"), b"rebuilt geometry").unwrap();
        f.run(&work, &|_, _| output());
        assert_eq!(f.value(&work)["state"], "ready");
    }

    #[test]
    fn thumbnail_external_directory_lists_track_every_directory_without_global_env_changes() {
        let f = Fixture::new(); let a = f.root.join("external-a"); let b = f.root.join("external-b");
        std::fs::create_dir_all(&a).unwrap(); std::fs::create_dir_all(&b).unwrap();
        let paths = std::env::join_paths([&a, &b]).unwrap();
        let before = external_context(None, Some(&paths));
        assert_eq!(before["geometry_dirs"].as_array().unwrap().len(), 2);
        std::fs::write(b.join("hull.geometry"), b"updated second directory").unwrap();
        let after = external_context(None, Some(&paths));
        assert_eq!(before["geometry_dirs"][0], after["geometry_dirs"][0]);
        assert_ne!(before["geometry_dirs"][1], after["geometry_dirs"][1]);
    }

    #[test]
    fn thumbnail_missing_model_can_recover_after_external_resources_become_available() {
        let f = Fixture::new(); let mut work = f.project("a");
        f.run(&work, &|_, _| bail!("no_model"));
        assert_eq!(f.value(&work)["state"], "failed");
        let file = f.cache().root.join(&work.key).join("cover.json");
        let mut descriptor: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        descriptor["external"] = json!("previous unavailable external resources");
        atomic_write(&file, &serde_json::to_vec(&descriptor).unwrap()).unwrap();
        work.force = true;
        f.run(&work, &|_, task| {
            std::fs::create_dir_all(task.source.join(".wowsmod/cache"))?;
            std::fs::write(task.source.join(".wowsmod/cache/geometry.glb"), b"first available geometry")?;
            output()
        });
        assert_eq!(f.value(&work)["state"], "ready");
    }
}
