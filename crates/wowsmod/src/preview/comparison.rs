//! Independent project comparisons, published only as complete same-camera pairs.
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{
    cdp,
    library::{path_identity, same_path, PreviewService},
    state,
};
use crate::{
    modproj::sha256_hex,
    project_revision::{read_plain_file, read_source_manifest, SafeDirectory},
};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const MAX_QUEUE: usize = 8;
const MAX_STATES: usize = 32;
const MAX_PNG_BYTES: u64 = 4 * 1024 * 1024;
const RECIPE_QUERY: &str =
    "rev=head&compare=single&w=640&h=360&quality=high&wm=0&look=game-day&env=studio&lod=0&t=0";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct CommonCamera {
    pos: [f64; 3],
    tgt: [f64; 3],
    up: [f64; 3],
    fov: f64,
    near: f64,
    far: f64,
    aspect: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Bounds {
    min: [f64; 3],
    max: [f64; 3],
}

pub(super) fn fit_comparison_camera(
    left: &Bounds,
    right: &Bounds,
    reference: &CommonCamera,
) -> Result<CommonCamera> {
    for bounds in [left, right] {
        if (0..3).any(|i| {
            !bounds.min[i].is_finite()
                || !bounds.max[i].is_finite()
                || bounds.min[i] > bounds.max[i]
        }) {
            bail!("invalid_bounds: 模型包围盒无效");
        }
    }
    let min = std::array::from_fn::<_, 3, _>(|i| left.min[i].min(right.min[i]));
    let max = std::array::from_fn::<_, 3, _>(|i| left.max[i].max(right.max[i]));
    let tgt = std::array::from_fn(|i| min[i] * 0.5 + max[i] * 0.5);
    let backward = unit(sub(reference.pos, reference.tgt))?;
    let up = [0., 1., 0.];
    let horizontal = unit(cross(up, backward))?;
    let vertical = cross(backward, horizontal);
    let tangent = (34_f64.to_radians() / 2.).tan();
    let aspect = WIDTH as f64 / HEIGHT as f64;
    let mut distance = 0_f64;
    let mut depths = Vec::with_capacity(8);
    for x in [min[0], max[0]] {
        for y in [min[1], max[1]] {
            for z in [min[2], max[2]] {
                let offset = sub([x, y, z], tgt);
                let depth = dot(offset, backward);
                distance =
                    distance.max(depth + dot(offset, horizontal).abs() / (tangent * aspect * 0.87));
                distance = distance.max(depth + dot(offset, vertical).abs() / (tangent * 0.87));
                depths.push(depth);
            }
        }
    }
    // Strictly inside both the frustum and its clipping planes, even for flat boxes.
    distance += distance.abs().max(1.) * 1e-9;
    let nearest = depths
        .iter()
        .map(|z| distance - z)
        .fold(f64::INFINITY, f64::min);
    let furthest = depths
        .iter()
        .map(|z| distance - z)
        .fold(f64::NEG_INFINITY, f64::max);
    let camera = CommonCamera {
        pos: std::array::from_fn(|i| tgt[i] + backward[i] * distance),
        tgt,
        up,
        fov: 34.,
        near: nearest * 0.25,
        far: furthest + (furthest - nearest).max(1.),
        aspect,
    };
    validate_camera(&camera)?;
    Ok(camera)
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: [f64; 3]) -> Result<[f64; 3]> {
    let n = dot(a, a).sqrt();
    if !n.is_finite() || n < 1e-12 {
        bail!("invalid_camera: 无效相机方向");
    }
    Ok([a[0] / n, a[1] / n, a[2] / n])
}
fn validate_camera(c: &CommonCamera) -> Result<()> {
    if c.pos
        .iter()
        .chain(&c.tgt)
        .chain(&c.up)
        .chain([&c.fov, &c.near, &c.far, &c.aspect])
        .any(|x| !x.is_finite())
        || c.fov != 34.
        || c.aspect != WIDTH as f64 / HEIGHT as f64
        || c.up != [0., 1., 0.]
        || c.near <= 0.
        || c.far <= c.near
    {
        bail!("invalid_camera: 无效共同相机");
    }
    unit(cross(c.up, unit(sub(c.pos, c.tgt))?))?;
    Ok(())
}

#[derive(Clone)]
struct Work {
    id: String,
    ids: [String; 2],
    sources: [PathBuf; 2],
    ship: String,
    port: u16,
    refresh: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Input {
    ids: [String; 2],
    paths: [String; 2],
    revisions: [String; 2],
    geometry: [Value; 2],
    context: Value,
    recipe: Value,
}
#[derive(Clone, Serialize, Deserialize)]
struct Descriptor {
    schema: u32,
    id: String,
    key: String,
    input: Input,
    camera: CommonCamera,
    hashes: [String; 2],
    meta: [Value; 2],
    #[serde(default)]
    previous: Option<[String; 2]>,
}
struct Entry {
    work: Work,
    descriptor: Option<Descriptor>,
    state: &'static str,
    reason: Option<String>,
    used: Instant,
}
impl Entry {
    fn pending(&self) -> bool {
        matches!(self.state, "queued" | "rendering")
    }
    fn value(&self) -> Value {
        let side = |i: usize, name: &str| json!({"project_id":self.work.ids[i],"revision":self.descriptor.as_ref().map(|d|&d.input.revisions[i]),"url":self.descriptor.as_ref().map(|d|format!("/api/v1/library/comparisons/{}/{}/{}.png",self.work.id,name,d.hashes[i]))});
        json!({"ok":true,"id":self.work.id,"state":self.state,"left":side(0,"left"),"right":side(1,"right"),"camera":self.descriptor.as_ref().map(|d|&d.camera),"recipe":recipe(),"stale":self.descriptor.is_some()&&self.state!="ready","reason":self.reason})
    }
}
#[derive(Default)]
struct Queue {
    entries: HashMap<String, Entry>,
    jobs: VecDeque<Work>,
    running: bool,
}
struct Rendered {
    pngs: [Vec<u8>; 2],
    camera: CommonCamera,
    meta: [Value; 2],
    _leases: Vec<Arc<state::PreviewState>>,
}
pub(super) struct ComparisonCache {
    root: PathBuf,
    queue: Mutex<Queue>,
}
impl ComparisonCache {
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            root,
            queue: Mutex::new(Queue::default()),
        }
    }
    pub(super) fn request(
        self: &Arc<Self>,
        service: &PreviewService,
        left: &str,
        right: &str,
        port: u16,
        refresh: bool,
    ) -> Result<Value> {
        let work = resolve(service, left, right, port, refresh)?;
        let (value, start) = self.enqueue(work);
        if start {
            let service = service.clone();
            let cache = Arc::clone(self);
            std::thread::spawn(move || cache.drain(&service, &render));
        }
        Ok(value)
    }
    pub(super) fn status(&self, service: &PreviewService, id: &str) -> Result<Value> {
        if !valid_hash(id) {
            bail!("not_found: 对照任务不存在");
        }
        let work = {
            let mut queue = self.queue.lock().unwrap();
            let entry = queue
                .entries
                .get_mut(id)
                .ok_or_else(|| anyhow!("not_found: 对照任务已过期，请重新对照"))?;
            entry.used = Instant::now();
            entry.work.clone()
        };
        // Polling checks only registered identity and tiny mod.toml, never content hashes/models.
        let identity = verify_source(service, &work);
        let queue = self.queue.lock().unwrap();
        let entry = queue
            .entries
            .get(id)
            .ok_or_else(|| anyhow!("not_found: 对照任务已过期"))?;
        let mut value = entry.value();
        // Do not alter the running marker: even a removed identity cannot enqueue
        // a second worker while the first still owns its browsers and leases.
        if let Err(error) = identity {
            value["state"] = json!("missing");
            value["reason"] = json!(format!("{error:#}"));
            value["stale"] = json!(entry.descriptor.is_some());
        }
        Ok(value)
    }
    pub(super) fn image(
        &self,
        service: &PreviewService,
        id: &str,
        side: &str,
        hash: &str,
    ) -> Result<Vec<u8>> {
        if !valid_hash(id) || !valid_hash(hash) {
            bail!("not_found: 无效对照图片");
        }
        let index = match side {
            "left" => 0,
            "right" => 1,
            _ => bail!("not_found: 无效对照侧"),
        };
        let descriptor = self.load(id)?;
        if descriptor.hashes[index] != hash
            && descriptor.previous.as_ref().map(|p| p[index].as_str()) != Some(hash)
        {
            bail!("not_found: 对照图片版本不匹配");
        }
        let work = resolve(
            service,
            &descriptor.input.ids[0],
            &descriptor.input.ids[1],
            0,
            false,
        )?;
        if work.id != id {
            bail!("not_found: 项目已重新定位");
        }
        let bytes = self.read_png(id, hash)?;
        verify_source(service, &work)?;
        Ok(bytes)
    }
    fn enqueue(&self, work: Work) -> (Value, bool) {
        let mut queue = self.queue.lock().unwrap();
        let now = Instant::now();
        if let Some(entry) = queue.entries.get_mut(&work.id) {
            entry.used = now;
            if entry.pending() {
                return (entry.value(), false);
            }
        }
        if !queue.entries.contains_key(&work.id) {
            if queue.entries.len() >= MAX_STATES {
                if let Some(oldest) = queue
                    .entries
                    .iter()
                    .filter(|(_, e)| !e.pending())
                    .min_by_key(|(_, e)| e.used)
                    .map(|(id, _)| id.clone())
                {
                    queue.entries.remove(&oldest);
                }
            }
            queue.entries.insert(
                work.id.clone(),
                Entry {
                    work: work.clone(),
                    descriptor: None,
                    state: "missing",
                    reason: None,
                    used: now,
                },
            );
        }
        let full = queue.jobs.len() >= MAX_QUEUE;
        let entry = queue.entries.get_mut(&work.id).unwrap();
        entry.work = work.clone();
        entry.used = now;
        entry.state = if full { "failed" } else { "queued" };
        entry.reason = full.then(|| "queue_full: 对照队列繁忙，请稍后重试".into());
        let value = entry.value();
        if full {
            return (value, false);
        }
        queue.jobs.push_back(work);
        let start = !queue.running;
        queue.running = true;
        (value, start)
    }
    fn drain(
        &self,
        service: &PreviewService,
        renderer: &dyn Fn(&PreviewService, &Work) -> Result<Rendered>,
    ) {
        loop {
            let work = {
                let mut queue = self.queue.lock().unwrap();
                let Some(work) = queue.jobs.pop_front() else {
                    queue.running = false;
                    return;
                };
                queue.entries.get_mut(&work.id).unwrap().state = "rendering";
                work
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.process(service, &work, renderer)
            }))
            .unwrap_or_else(|_| Err(anyhow!("render_failed: 对照渲染异常，请重试")));
            {
                let mut queue = self.queue.lock().unwrap();
                let entry = queue.entries.get_mut(&work.id).unwrap();
                match result {
                    Ok(d) => {
                        entry.descriptor = Some(d);
                        entry.state = "ready";
                        entry.reason = None;
                    }
                    Err(error) => {
                        entry.state = "failed";
                        entry.reason = Some(format!("{error:#}"));
                    }
                }
                entry.used = Instant::now();
            }
            self.prune(&work.id);
        }
    }
    fn process(
        &self,
        service: &PreviewService,
        work: &Work,
        renderer: &dyn Fn(&PreviewService, &Work) -> Result<Rendered>,
    ) -> Result<Descriptor> {
        verify_source(service, work)?;
        let _left_guard = SafeDirectory::open(&work.sources[0])?;
        let _right_guard = SafeDirectory::open(&work.sources[1])?;
        let revisions = revisions(work)?;
        let context = context(service)?;
        let mut input = Input {
            ids: work.ids.clone(),
            paths: work.sources.each_ref().map(|p| path_identity(p)),
            revisions: revisions.clone(),
            geometry: geometry(work)?,
            context: context.clone(),
            recipe: recipe(),
        };
        let old = self.load(&work.id).ok();
        if let Some(old) = &old {
            {
                let mut queue = self.queue.lock().unwrap();
                if let Some(entry) = queue.entries.get_mut(&work.id) {
                    entry.descriptor = Some(old.clone());
                }
            }
            if !work.refresh && old.input == input {
                verify_stable(service, work, &revisions, &context)?;
                return Ok(old.clone());
            }
        }
        let rendered = renderer(service, work)?;
        validate_rendered(&rendered)?;
        verify_stable(service, work, &revisions, &context)?;
        input.geometry = geometry(work)?;
        let descriptor = Descriptor {
            schema: 1,
            id: work.id.clone(),
            key: descriptor_key(&input, &rendered.camera),
            input,
            camera: rendered.camera.clone(),
            hashes: rendered.pngs.each_ref().map(|png| sha256_hex(png)),
            meta: rendered.meta.clone(),
            previous: old.map(|d| d.hashes),
        };
        let dir = self.root.join(&work.id);
        // Both payloads precede the single publication point. Failed pairs are never addressable.
        for i in 0..2 {
            write_cache_file(
                &dir.join(format!("{}.png", descriptor.hashes[i])),
                &rendered.pngs[i],
            )?;
        }
        verify_stable(service, work, &revisions, &context)?;
        if descriptor.input.geometry != geometry(work)? {
            bail!("source_changed: 发布前模型缓存发生变化，请重试");
        }
        write_cache_file(&dir.join("pair.json"), &serde_json::to_vec(&descriptor)?)?;
        // Keep both registered session leases through descriptor publication.
        drop(rendered);
        Ok(descriptor)
    }
    fn load(&self, id: &str) -> Result<Descriptor> {
        if !valid_hash(id) {
            bail!("not_found: 无效对照身份");
        }
        let raw = read_plain_file(&self.root.join(id).join("pair.json"), 512 * 1024)?;
        let d: Descriptor = serde_json::from_slice(&raw)?;
        if d.schema != 1
            || d.id != id
            || d.key != descriptor_key(&d.input, &d.camera)
            || d.input.recipe != recipe()
            || d.input.ids[0] == d.input.ids[1]
            || d.input
                .revisions
                .iter()
                .any(|r| !crate::project_revision::is_revision(r))
            || d.previous
                .as_ref()
                .map(|p| p.iter().any(|h| !valid_hash(h)))
                .unwrap_or(false)
        {
            bail!("invalid_cache: 对照缓存身份不匹配");
        }
        let pair = sha256_hex(&serde_json::to_vec(&(&d.input.ids, &d.input.paths))?);
        if pair != id {
            bail!("invalid_cache: 对照项目路径不匹配");
        }
        validate_camera(&d.camera)?;
        let pngs = [
            self.read_png(id, &d.hashes[0])?,
            self.read_png(id, &d.hashes[1])?,
        ];
        validate_rendered(&Rendered {
            pngs,
            camera: d.camera.clone(),
            meta: d.meta.clone(),
            _leases: vec![],
        })?;
        Ok(d)
    }
    fn read_png(&self, id: &str, hash: &str) -> Result<Vec<u8>> {
        if !valid_hash(id) || !valid_hash(hash) {
            bail!("not_found: 无效对照图片版本");
        }
        let bytes = read_plain_file(
            &self.root.join(id).join(format!("{hash}.png")),
            MAX_PNG_BYTES,
        )?;
        if sha256_hex(&bytes) != hash {
            bail!("invalid_cache: 对照图片校验失败");
        }
        validate_png(&bytes)?;
        Ok(bytes)
    }
    fn prune(&self, current: &str) {
        let protected: Vec<_> = self
            .queue
            .lock()
            .unwrap()
            .entries
            .iter()
            .filter(|(_, e)| e.pending())
            .map(|(id, _)| id.clone())
            .collect();
        let Ok(_guard) = SafeDirectory::open(&self.root) else {
            return;
        };
        let Ok(files) = std::fs::read_dir(&self.root) else {
            return;
        };
        let mut owned: Vec<_> = files
            .flatten()
            .filter_map(|entry| {
                let id = entry.file_name().to_string_lossy().into_owned();
                let md = std::fs::symlink_metadata(entry.path()).ok()?;
                if !valid_hash(&id) || crate::project_revision::is_link(&md) || !md.is_dir() {
                    return None;
                }
                Some((md.modified().ok()?, id, entry.path()))
            })
            .collect();
        owned.sort_by_key(|(time, _, _)| *time);
        let mut excess = owned.len().saturating_sub(MAX_STATES);
        for (_, id, path) in owned {
            if protected.contains(&id) {
                continue;
            }
            let remove = excess > 0 && id != current;
            let keep = if remove {
                None
            } else {
                self.load(&id).ok().map(|d| {
                    let mut hashes = d.hashes.to_vec();
                    if let Some(previous) = d.previous {
                        hashes.extend(previous);
                    }
                    hashes
                })
            };
            let Ok(_guard) = SafeDirectory::open(&path) else {
                continue;
            };
            if let Ok(files) = std::fs::read_dir(&path) {
                for file in files.flatten() {
                    let name = file.file_name().to_string_lossy().into_owned();
                    let png = name.strip_suffix(".png").filter(|h| valid_hash(h));
                    let discard = png
                        .map(|h| {
                            !keep
                                .as_ref()
                                .map(|k| k.iter().any(|v| v == h))
                                .unwrap_or(false)
                        })
                        .unwrap_or(remove && name == "pair.json");
                    if discard && file.file_type().map(|t| t.is_file()).unwrap_or(false) {
                        let _ = std::fs::remove_file(file.path());
                    }
                }
            }
            drop(_guard);
            if remove && std::fs::remove_dir(&path).is_ok() {
                excess -= 1;
            }
        }
    }
}
fn recipe() -> Value {
    json!({"width":WIDTH,"height":HEIGHT,"dpr":1,"quality":"high","direction":"left-deck","fov":34,"fill":0.87,"look":"game-day","env":"studio","lod":0,"t":0,"compare":"single","rev":"head","wm":false})
}
fn resolve(
    service: &PreviewService,
    left: &str,
    right: &str,
    port: u16,
    refresh: bool,
) -> Result<Work> {
    if left == right || left.is_empty() || right.is_empty() {
        return Err(crate::project_revision::failure(
            "invalid_request",
            "请选择两个不同的独立方案",
        ));
    }
    let sources = [service.source(left)?, service.source(right)?];
    let ships = sources.each_ref().map(|p| {
        crate::library::read_project_config(p)
            .map(|c| crate::ship_catalog::canonical_ship_key(&c.meta.ship))
    });
    let [left_ship, right_ship] = ships;
    let ship = left_ship?;
    if ship.is_empty() || ship != right_ship? {
        return Err(crate::project_revision::failure(
            "invalid_request",
            "只支持同一艘船的独立方案对照",
        ));
    }
    let ids = [left.to_string(), right.to_string()];
    let id = sha256_hex(&serde_json::to_vec(&(
        &ids,
        sources.each_ref().map(|p| path_identity(p)),
    ))?);
    Ok(Work {
        id,
        ids,
        sources,
        ship,
        port,
        refresh,
    })
}
fn verify_source(service: &PreviewService, work: &Work) -> Result<()> {
    let current = resolve(service, &work.ids[0], &work.ids[1], work.port, work.refresh)?;
    if current.id != work.id
        || current.ship != work.ship
        || !same_path(&current.sources[0], &work.sources[0])
        || !same_path(&current.sources[1], &work.sources[1])
    {
        bail!("source_changed: 项目已移除、重新定位或更换舰船，请重新对照");
    }
    Ok(())
}
fn revisions(work: &Work) -> Result<[String; 2]> {
    Ok([
        read_source_manifest(&work.sources[0])?.revision,
        read_source_manifest(&work.sources[1])?.revision,
    ])
}
fn geometry(work: &Work) -> Result<[Value; 2]> {
    Ok([
        file_content(&work.sources[0].join(".wowsmod/cache/geometry.glb"))?,
        file_content(&work.sources[1].join(".wowsmod/cache/geometry.glb"))?,
    ])
}
fn verify_stable(
    service: &PreviewService,
    work: &Work,
    revision: &[String; 2],
    expected_context: &Value,
) -> Result<()> {
    verify_source(service, work)?;
    if &revisions(work)? != revision || &context(service)? != expected_context {
        bail!("source_changed: 项目或客户端在对照期间发生变化，请重试");
    }
    Ok(())
}
fn valid_hash(hash: &str) -> bool {
    crate::project_revision::is_sha256(hash)
}
fn file_content(path: &Path) -> Result<Value> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Null),
        Err(e) => Err(e.into()),
        Ok(_) => {
            let bytes = read_plain_file(path, 512 * 1024 * 1024)?;
            Ok(json!({"sha256":sha256_hex(&bytes),"bytes":bytes.len()}))
        }
    }
}
fn stamp(path: &Path) -> Value {
    let md = std::fs::metadata(path).ok();
    json!({"path":path_identity(path),"bytes":md.as_ref().map(|m|m.len()),"mtime":md.and_then(|m|m.modified().ok()).and_then(|t|t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d|d.as_nanos().to_string())})
}
fn external_tree(root: &Path) -> Result<Value> {
    if !root.exists() {
        return Ok(Value::Null);
    }
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    let mut entries = std::collections::BTreeMap::new();
    let mut total = 0u64;
    while let Some((dir, depth)) = pending.pop() {
        if depth > 64 {
            bail!("snapshot_limit: 外部几何目录层级过深");
        }
        let _guard = SafeDirectory::open(&dir)?;
        for file in std::fs::read_dir(&dir)? {
            let file = file?;
            let path = file.path();
            let md = std::fs::symlink_metadata(&path)?;
            if crate::project_revision::is_link(&md) {
                bail!("unsafe_source: 外部几何包含链接");
            }
            if md.is_dir() {
                pending.push((path, depth + 1));
            } else if md.is_file() {
                total = total.saturating_add(md.len());
                if entries.len() >= 100_000 || total > 16 * 1024 * 1024 * 1024 {
                    bail!("snapshot_limit: 外部几何过大");
                }
                entries.insert(
                    path.strip_prefix(root)?
                        .to_string_lossy()
                        .replace('\\', "/"),
                    file_content(&path)?,
                );
            } else {
                bail!("unsafe_source: 外部几何包含非常规文件");
            }
        }
    }
    Ok(json!({"path":path_identity(root),"sha256":sha256_hex(&serde_json::to_vec(&entries)?)}))
}
fn context(service: &PreviewService) -> Result<Value> {
    let selected = service.selected_game.as_ref();
    let game = selected.and_then(|p| crate::game::describe(p, "comparison").ok());
    let gp = std::env::var_os("WOWSMOD_GP").map(PathBuf::from);
    let mut directories = vec![];
    if let Some(paths) = std::env::var_os("WOWSMOD_GEOM_DIR") {
        for path in std::env::split_paths(&paths) {
            directories.push(json!({"path":path_identity(&path),"contents":external_tree(&path)?}));
        }
    }
    Ok(
        json!({"schema":1,"version":env!("CARGO_PKG_VERSION"),"web":super::http::web_fingerprint(),"selected":selected.map(|p|path_identity(p)),
        "available":game.is_some(),"build":game.as_ref().map(|g|&g.build),"client_version":game.as_ref().and_then(|g|g.client_version.as_ref()),
        "client_info":selected.map(|p|file_content(&p.join("game_info.xml"))).transpose()?,"packages":game.as_ref().map(|g|stamp(&g.res_packages)),"index":game.as_ref().map(|g|stamp(&g.idx_dir)),
        "external":{"gp":gp.as_ref().map(|p|file_content(p)).transpose()?,"gp_path":gp.as_ref().map(|p|path_identity(p)),"geometry_dirs":directories}}),
    )
}
fn write_cache_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("invalid_cache: 缓存目录无效"))?;
    let _guard = SafeDirectory::create(parent)?;
    let native = std::fs::canonicalize(parent)?
        .join(path.file_name().ok_or_else(|| anyhow!("invalid_cache"))?);
    crate::library::atomic_write(&native, bytes)
}
fn validate_rendered(rendered: &Rendered) -> Result<()> {
    validate_camera(&rendered.camera)?;
    for i in 0..2 {
        validate_png(&rendered.pngs[i])?;
        let meta = &rendered.meta[i];
        // Browser JSON numbers have no integer/float distinction. Decode the
        // contract before exact f64 comparison, retaining one-ULP sensitivity.
        let camera: CommonCamera = serde_json::from_value(meta["meta"]["camera"].clone())
            .map_err(|_| anyhow!("camera_mismatch: 出图缺少有效相机"))?;
        if meta["ok"] != true
            || meta["width"] != WIDTH
            || meta["height"] != HEIGHT
            || camera != rendered.camera
        {
            bail!("camera_mismatch: 两侧最终相机不一致");
        }
        let fields = &meta["meta"];
        if fields["env"] != "studio"
            || fields["look"] != "game-day"
            || fields["light"] != "day"
            || fields["compare"] != "single"
            || fields["view"] != "custom"
            || fields["channel"] != "final"
            || !fields["lod"]
                .as_object()
                .map(|lod| {
                    lod.values().any(|v| v.as_i64() == Some(0))
                        && lod.values().all(|v| matches!(v.as_i64(), Some(-1 | 0)))
                })
                .unwrap_or(false)
        {
            bail!("recipe_mismatch: 两侧渲染条件不一致");
        }
        serde_json::from_value::<Bounds>(fields["bounds"].clone())
            .map_err(|_| anyhow!("invalid_bounds: 出图缺少包围盒"))?;
    }
    Ok(())
}
fn render(service: &PreviewService, work: &Work) -> Result<Rendered> {
    let browser = cdp::find_browser(None)
        .ok_or_else(|| anyhow!("browser_not_found: 需要 Microsoft Edge 或 Google Chrome"))?;
    let context_key = sha256_hex(&serde_json::to_vec(&context(service)?)?);
    let left = service.comparison_session(&work.ids[0], &context_key)?;
    let right = service.comparison_session(&work.ids[1], &context_key)?;
    for (i, lease) in [&left, &right].into_iter().enumerate() {
        if !same_path(&lease.project.root, &work.sources[i]) {
            bail!("source_changed: 项目已重新定位");
        }
        let current = service
            .selected_game
            .as_deref()
            .and_then(|p| crate::game::describe(p, "comparison").ok());
        if current.as_ref().map(|g| &g.build) != lease.game.as_ref().map(|g| &g.build) {
            bail!("source_changed: 客户端已更新，请重启预览服务");
        }
        lease.model_view(Some("head"))?;
    }
    let geometry_before = geometry(work)?;
    let base = |id: &str| {
        format!(
            "http://127.0.0.1:{}/?project={}",
            work.port,
            super::thumbnails::encode_id(id)
        )
    };
    let mut page_left = cdp::Page::open(
        &browser,
        &base(&work.ids[0]),
        (WIDTH, HEIGHT),
        false,
        "comparison-left",
    )?;
    let mut page_right = cdp::Page::open(
        &browser,
        &base(&work.ids[1]),
        (WIDTH, HEIGHT),
        false,
        "comparison-right",
    )?;
    let (_, left_probe) = page_left.render(
        "comparison-left-bounds",
        &format!("{RECIPE_QUERY}&view=deck"),
        120,
    )?;
    let (_, right_probe) = page_right.render(
        "comparison-right-bounds",
        &format!("{RECIPE_QUERY}&view=deck"),
        120,
    )?;
    let bounds_left: Bounds = serde_json::from_value(left_probe["meta"]["bounds"].clone())?;
    let bounds_right: Bounds = serde_json::from_value(right_probe["meta"]["bounds"].clone())?;
    let reference: CommonCamera = serde_json::from_value(left_probe["meta"]["camera"].clone())?;
    let camera = fit_comparison_camera(&bounds_left, &bounds_right, &reference)?;
    let csv = |v: [f64; 3]| {
        v.iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    let query = format!(
        "{RECIPE_QUERY}&view=custom&pos={}&tgt={}&fov={}&near={}&far={}",
        csv(camera.pos),
        csv(camera.tgt),
        camera.fov,
        camera.near,
        camera.far
    );
    let (left_png, left_meta) = page_left.render("comparison-left", &query, 120)?;
    let (right_png, right_meta) = page_right.render("comparison-right", &query, 120)?;
    if geometry_before != geometry(work)? {
        bail!("source_changed: 模型缓存出图期间变化");
    }
    Ok(Rendered {
        pngs: [left_png, right_png],
        camera,
        meta: [left_meta, right_meta],
        _leases: vec![left, right],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reference() -> CommonCamera {
        CommonCamera {
            pos: [300., 120., 300.],
            tgt: [0.; 3],
            up: [0., 1., 0.],
            fov: 34.,
            near: 0.05,
            far: 1600.,
            aspect: 640. / 360.,
        }
    }
    #[test]
    fn union_camera_contains_all_corners_with_identical_projection() {
        let a = Bounds {
            min: [-120., -5., -10.],
            max: [120., 20., 10.],
        };
        let b = Bounds {
            min: [-90., -20., -35.],
            max: [145., 40., 40.],
        };
        let camera = fit_comparison_camera(&a, &b, &reference()).unwrap();
        assert_eq!(camera.tgt, [12.5, 10., 2.5]);
        assert_eq!(camera.up, [0., 1., 0.]);
        assert_eq!(camera.fov, 34.);
        for bounds in [&a, &b] {
            for x in [bounds.min[0], bounds.max[0]] {
                for y in [bounds.min[1], bounds.max[1]] {
                    for z in [bounds.min[2], bounds.max[2]] {
                        let backward = unit(sub(camera.pos, camera.tgt)).unwrap();
                        let horizontal = unit(cross(camera.up, backward)).unwrap();
                        let vertical = cross(backward, horizontal);
                        let delta = sub([x, y, z], camera.pos);
                        let depth = -dot(delta, backward);
                        let tangent = (camera.fov.to_radians() / 2.).tan();
                        assert!(depth > camera.near && depth < camera.far);
                        assert!(
                            (dot(delta, horizontal) / (depth * tangent * camera.aspect)).abs()
                                <= 0.8700000001
                        );
                        assert!((dot(delta, vertical) / (depth * tangent)).abs() <= 0.8700000001);
                        let copy: CommonCamera =
                            serde_json::from_slice(&serde_json::to_vec(&camera).unwrap()).unwrap();
                        assert_eq!(camera, copy);
                    }
                }
            }
        }
    }
    #[test]
    fn invalid_bounds_and_parallel_camera_fail() {
        let b = Bounds {
            min: [0.; 3],
            max: [1.; 3],
        };
        let mut reference = reference();
        reference.pos = [0.; 3];
        assert!(fit_comparison_camera(&b, &b, &reference).is_err());
        reference.pos = [0., 1., 0.];
        assert!(fit_comparison_camera(&b, &b, &reference).is_err());
        reference = tests::reference();
        assert!(fit_comparison_camera(
            &Bounds {
                min: [f64::NAN, 0., 0.],
                max: [1.; 3]
            },
            &b,
            &reference
        )
        .is_err());
    }
    fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }
    fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }
    fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }
    fn unit(a: [f64; 3]) -> Result<[f64; 3]> {
        let n = dot(a, a).sqrt();
        if !n.is_finite() || n < 1e-12 {
            bail!("invalid direction");
        }
        Ok([a[0] / n, a[1] / n, a[2] / n])
    }
    struct Fixture {
        root: PathBuf,
        service: PreviewService,
        cache: Arc<ComparisonCache>,
        work: Work,
    }
    impl Fixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "wowsmod_comparison_{}_{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let service = PreviewService::new(
                &super::super::library::selected_cli(Some(root.join("missing-client"))),
                &root.join("home"),
                None,
            )
            .unwrap();
            let sources = ["a", "b"].map(|name| {
                crate::modproj::Project::init(&root.join(name), "same", "PASC001", "loose", "123")
                    .unwrap()
                    .root
            });
            let ids = sources
                .each_ref()
                .map(|source| service.store().unwrap().add(source).unwrap().id);
            // Construct identity here so tests of queue/cache do not depend on resolve's validation.
            let id = sha256_hex(
                &serde_json::to_vec(&(&ids, sources.each_ref().map(|p| path_identity(p)))).unwrap(),
            );
            let work = Work {
                id,
                ids,
                sources,
                ship: crate::ship_catalog::canonical_ship_key("PASC001"),
                port: 0,
                refresh: false,
            };
            Self {
                cache: Arc::new(ComparisonCache::new(root.join("home/comparisons"))),
                root,
                service,
                work,
            }
        }
        fn run(&self, renderer: &dyn Fn(&PreviewService, &Work) -> Result<Rendered>) -> Value {
            self.cache.enqueue(self.work.clone());
            self.cache.drain(&self.service, renderer);
            self.cache.status(&self.service, &self.work.id).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    fn output() -> Result<Rendered> {
        let camera = reference();
        let meta = json!({"ok":true,"width":640,"height":360,"meta":{"camera":camera,"bounds":{"min":[-1,-1,-1],"max":[1,1,1]},"look":"game-day","env":"studio","light":"day","compare":"single","view":"custom","channel":"final","lod":{"hull":0}}});
        Ok(Rendered {
            pngs: [[1, 2, 3, 255], [40, 50, 60, 255]].map(|color| {
                wowsmod_formats::texture::write_png(
                    &wowsmod_formats::texture::Image {
                        width: WIDTH,
                        height: HEIGHT,
                        px: color.repeat((WIDTH * HEIGHT) as usize),
                    },
                    false,
                )
                .unwrap()
            }),
            camera,
            meta: [meta.clone(), meta],
            _leases: vec![],
        })
    }

    #[test]
    fn browser_integer_camera_numbers_are_exactly_equivalent_but_one_ulp_is_not() {
        let mut rendered = output().unwrap();
        // JSON.stringify emits whole-valued JS doubles as integers. Serde Value
        // retains that representation, but it is the same numerical projection.
        rendered.meta[0]["meta"]["camera"] = json!({
            "pos": [300, 120, 300], "tgt": [0, 0, 0], "up": [0, 1, 0],
            "fov": 34, "near": 0.05, "far": 1600, "aspect": 640.0 / 360.0
        });
        assert!(validate_rendered(&rendered).is_ok());
        rendered.meta[0]["meta"]["camera"]["pos"][0] = json!(f64::from_bits(300.0_f64.to_bits() + 1));
        assert!(validate_rendered(&rendered).is_err(), "different numerical cameras remain strictly rejected");
    }
    #[test]
    fn pair_validation_requires_two_distinct_registered_same_ship_ids() {
        let f = Fixture::new();
        assert!(resolve(&f.service, &f.work.ids[0], &f.work.ids[1], 0, false).is_ok());
        let invalid = resolve(&f.service, &f.work.ids[0], &f.work.ids[0], 0, false)
            .err()
            .unwrap();
        assert_eq!(
            crate::project_revision::error_code(&invalid),
            Some("invalid_request")
        );
        let other =
            crate::modproj::Project::init(&f.root.join("other"), "same", "PASC002", "loose", "123")
                .unwrap();
        let id = f.service.store().unwrap().add(&other.root).unwrap().id;
        assert!(resolve(&f.service, &f.work.ids[0], &id, 0, false).is_err());
    }
    #[test]
    fn queue_deduplicates_and_bounds_jobs_and_states() {
        let f = Fixture::new();
        assert_eq!(f.cache.enqueue(f.work.clone()).0["state"], "queued");
        for _ in 0..20 {
            f.cache.enqueue(f.work.clone());
        }
        assert_eq!(f.cache.queue.lock().unwrap().jobs.len(), 1);
        for n in 0..50 {
            let mut work = f.work.clone();
            work.id = sha256_hex(n.to_string().as_bytes());
            f.cache.enqueue(work);
        }
        let q = f.cache.queue.lock().unwrap();
        assert_eq!(q.jobs.len(), 8);
        assert!(q.entries.len() <= 32);
    }
    #[test]
    fn pair_publication_and_disk_reuse_require_both_valid_pngs_and_meta() {
        let f = Fixture::new();
        let ready = f.run(&|_, _| output());
        assert_eq!(ready["state"], "ready");
        assert_eq!(ready["stale"], false);
        let d = f.cache.load(&f.work.id).unwrap();
        assert!(f
            .cache
            .image(&f.service, &f.work.id, "left", &d.hashes[0])
            .is_ok());
        assert!(f
            .cache
            .image(&f.service, &f.work.id, "right", &d.hashes[0])
            .is_err());
        let cache = ComparisonCache::new(f.cache.root.clone());
        cache.enqueue(f.work.clone());
        cache.drain(&f.service, &|_, _| panic!("cache hit must not render"));
        assert_eq!(
            cache.status(&f.service, &f.work.id).unwrap()["state"],
            "ready"
        );
        std::fs::write(
            f.cache
                .root
                .join(&f.work.id)
                .join(format!("{}.png", d.hashes[1])),
            b"corrupt",
        )
        .unwrap();
        assert!(f.cache.load(&f.work.id).is_err());
        assert!(f
            .cache
            .image(&f.service, &f.work.id, "right", &d.hashes[1])
            .is_err());
    }
    #[test]
    fn failures_do_not_publish_half_pairs_and_old_pairs_are_stale() {
        let f = Fixture::new();
        let failed = f.run(&|_, _| {
            let mut r = output()?;
            r.pngs[1] = b"invalid".to_vec();
            Ok(r)
        });
        assert_eq!(failed["state"], "failed");
        assert!(failed["left"]["url"].is_null());
        assert!(failed["right"]["url"].is_null());
        assert!(f.cache.load(&f.work.id).is_err());
        let ready = f.run(&|_, _| output());
        assert_eq!(ready["state"], "ready");
        std::fs::write(f.work.sources[0].join("paint.txt"), b"updated source").unwrap();
        let failed = f.run(&|_, _| bail!("right render failed"));
        assert_eq!(failed["state"], "failed");
        assert_eq!(failed["stale"], true);
        assert_eq!(failed["left"]["url"], ready["left"]["url"]);
    }
    #[test]
    fn source_changed_during_render_and_relocation_never_publish() {
        let f = Fixture::new();
        let value = f.run(&|_, w| {
            std::fs::write(w.sources[1].join("during.txt"), b"new")?;
            output()
        });
        assert_eq!(value["state"], "failed");
        assert!(f.cache.load(&f.work.id).is_err());
        let fresh =
            crate::modproj::Project::init(&f.root.join("new"), "same", "PASC001", "loose", "123")
                .unwrap();
        f.service.relocate(&f.work.ids[0], &fresh.root).unwrap();
        let value = f.run(&|_, _| panic!("old identity must not render"));
        assert_ne!(value["state"], "ready");
    }
    #[test]
    fn different_camera_or_recipe_is_not_ready() {
        for mismatch in ["camera", "env"] {
            let f = Fixture::new();
            let v = f.run(&|_, _| {
                let mut r = output()?;
                if mismatch == "camera" {
                    r.meta[1]["meta"]["camera"]["near"] = json!(2.);
                } else {
                    r.meta[1]["meta"]["env"] = json!("night");
                }
                Ok(r)
            });
            assert_eq!(v["state"], "failed");
            assert!(f.cache.load(&f.work.id).is_err());
        }
    }
    #[test]
    fn same_length_and_mtime_source_changes_invalidate_content_cache() {
        let f = Fixture::new();
        let path = f.work.sources[0].join("same-time.txt");
        std::fs::write(&path, b"old").unwrap();
        let time = std::fs::metadata(&path).unwrap().modified().unwrap();
        let ready = f.run(&|_, _| output());
        assert_eq!(ready["state"], "ready");
        let before = f.cache.load(&f.work.id).unwrap();
        std::fs::write(&path, b"new").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(time)
            .unwrap();
        let renders = std::sync::atomic::AtomicUsize::new(0);
        let after = f.run(&|_, _| {
            renders.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            output()
        });
        assert_eq!(after["state"], "ready");
        assert_eq!(renders.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_ne!(
            before.input.revisions,
            f.cache.load(&f.work.id).unwrap().input.revisions
        );
    }
    #[test]
    fn cache_key_binds_every_render_input_and_camera() {
        let f = Fixture::new();
        assert_eq!(f.run(&|_, _| output())["state"], "ready");
        let d = f.cache.load(&f.work.id).unwrap();
        let original = descriptor_key(&d.input, &d.camera);
        for field in ["ids", "paths", "revisions", "geometry", "context", "recipe"] {
            for side in 0..2 {
                let mut value = serde_json::to_value(&d.input).unwrap();
                if matches!(field, "ids" | "paths" | "revisions") {
                    value[field][side] = json!(format!("changed-{side}"));
                } else if field == "geometry" {
                    value[field][side] = json!("different-geometry");
                } else {
                    value[field] = json!({"changed":true});
                }
                let changed: Input = serde_json::from_value(value).unwrap();
                assert_ne!(
                    original,
                    descriptor_key(&changed, &d.camera),
                    "{field} {side}"
                );
            }
        }
        let mut camera = d.camera.clone();
        camera.far += 1.;
        assert_ne!(original, descriptor_key(&d.input, &camera));
    }
    #[test]
    fn unsafe_image_paths_and_wrong_dimensions_are_rejected() {
        let f = Fixture::new();
        assert_eq!(f.run(&|_, _| output())["state"], "ready");
        for (id, side, hash) in [
            ("../outside", "left", "a"),
            (f.work.id.as_str(), "../left", "a"),
            (f.work.id.as_str(), "left", "../../mod.toml"),
        ] {
            assert!(f.cache.image(&f.service, id, side, hash).is_err());
        }
        assert!(validate_png(b"broken").is_err());
        let mut data = output().unwrap().pngs[0].clone();
        data[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(validate_png(&data).is_err());
    }
    #[test]
    fn hidden_parts_are_not_mistaken_for_lower_quality_lod() {
        let mut r = output().unwrap();
        for m in &mut r.meta {
            m["meta"]["lod"]["hidden-optional-boat"] = json!(-1);
        }
        assert!(validate_rendered(&r).is_ok());
        r.meta[1]["meta"]["lod"]["hull"] = json!(1);
        assert!(validate_rendered(&r).is_err());
    }
    #[test]
    fn generated_geometry_is_new_baseline_but_changed_geometry_invalidates() {
        let f = Fixture::new();
        let make = |_: &PreviewService, w: &Work| {
            for source in &w.sources {
                std::fs::create_dir_all(source.join(".wowsmod/cache"))?;
                std::fs::write(source.join(".wowsmod/cache/geometry.glb"), b"new geometry")?;
            }
            output()
        };
        assert_eq!(f.run(&make)["state"], "ready");
        assert_eq!(
            f.run(&|_, _| panic!("first generated geometry must be cache baseline"))["state"],
            "ready"
        );
        std::fs::write(
            f.work.sources[1].join(".wowsmod/cache/geometry.glb"),
            b"changed geometry",
        )
        .unwrap();
        let renders = std::sync::atomic::AtomicUsize::new(0);
        assert_eq!(
            f.run(&|_, _| {
                renders.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                output()
            })["state"],
            "ready"
        );
        assert_eq!(renders.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
    #[test]
    fn disk_capacity_and_polling_do_not_touch_source_hashes() {
        let f = Fixture::new();
        assert_eq!(f.run(&|_, _| output())["state"], "ready");
        // Status observes the completed job without enqueuing another capture.
        for _ in 0..20 {
            assert_eq!(
                f.cache.status(&f.service, &f.work.id).unwrap()["state"],
                "ready"
            );
        }
        assert!(f.cache.queue.lock().unwrap().jobs.is_empty());
        for n in 0..40 {
            let path = f.cache.root.join(sha256_hex(format!("old-{n}").as_bytes()));
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("pair.json"), b"broken descriptor").unwrap();
        }
        let unrelated = f.cache.root.join("user-folder");
        std::fs::create_dir_all(&unrelated).unwrap();
        std::fs::write(unrelated.join("keep.txt"), b"keep").unwrap();
        f.cache.prune(&f.work.id);
        assert!(
            std::fs::read_dir(&f.cache.root)
                .unwrap()
                .flatten()
                .filter(|e| valid_hash(&e.file_name().to_string_lossy()))
                .count()
                <= 32
        );
        assert!(f.cache.load(&f.work.id).is_ok());
        assert_eq!(std::fs::read(unrelated.join("keep.txt")).unwrap(), b"keep");
    }
    #[test]
    fn refresh_preserves_previous_immutable_urls_for_inflight_readers() {
        let f = Fixture::new();
        assert_eq!(f.run(&|_, _| output())["state"], "ready");
        let old = f.cache.load(&f.work.id).unwrap();
        let mut work = f.work.clone();
        work.refresh = true;
        f.cache.enqueue(work);
        f.cache.drain(&f.service, &|_, _| {
            let mut rendered = output()?;
            rendered.pngs.swap(0, 1);
            Ok(rendered)
        });
        let current = f.cache.load(&f.work.id).unwrap();
        assert_ne!(old.hashes, current.hashes);
        assert!(f
            .cache
            .image(&f.service, &f.work.id, "left", &old.hashes[0])
            .is_ok());
        assert!(f
            .cache
            .image(&f.service, &f.work.id, "right", &current.hashes[1])
            .is_ok());
    }
    #[test]
    fn external_content_and_selected_client_build_change_context() {
        let f = Fixture::new();
        let directory = f.root.join("external");
        std::fs::create_dir_all(&directory).unwrap();
        let file = directory.join("hull.geometry");
        std::fs::write(&file, b"old").unwrap();
        let modified = std::fs::metadata(&file).unwrap().modified().unwrap();
        let before = external_tree(&directory).unwrap();
        std::fs::write(&file, b"new").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert_ne!(before, external_tree(&directory).unwrap());
        let unavailable = context(&f.service).unwrap();
        let game = f.service.selected_game.as_ref().unwrap();
        std::fs::create_dir_all(game.join("res_packages")).unwrap();
        std::fs::create_dir_all(game.join("bin/100/idx")).unwrap();
        std::fs::write(game.join("bin/100/idx/fixture.idx"), b"index").unwrap();
        let available = context(&f.service).unwrap();
        assert_ne!(unavailable, available);
        assert_eq!(available["build"], "100");
        std::fs::create_dir_all(game.join("bin/101/idx")).unwrap();
        std::fs::write(game.join("bin/101/idx/fixture.idx"), b"next").unwrap();
        assert_ne!(available, context(&f.service).unwrap());
    }
}
fn descriptor_key(input: &Input, camera: &CommonCamera) -> String {
    sha256_hex(&serde_json::to_vec(&(input, camera)).expect("finite comparison input"))
}
fn validate_png(bytes: &[u8]) -> Result<()> {
    if bytes.len() < 24
        || bytes.len() as u64 > MAX_PNG_BYTES
        || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
        || &bytes[12..16] != b"IHDR"
        || u32::from_be_bytes(bytes[16..20].try_into()?) != WIDTH
        || u32::from_be_bytes(bytes[20..24].try_into()?) != HEIGHT
    {
        bail!("invalid_png: 对照图片必须是640×360 PNG");
    }
    let image = wowsmod_formats::texture::read_png(bytes)?;
    if image.width != WIDTH || image.height != HEIGHT {
        bail!("invalid_png: 对照图片尺寸无效");
    }
    Ok(())
}
