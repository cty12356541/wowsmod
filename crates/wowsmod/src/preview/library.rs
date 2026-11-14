//! Lightweight library controller and lazily-created, explicitly identified preview sessions.
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::sync::atomic::AtomicU64;
use std::time::Instant;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use crate::{game, library::LibraryStore, modproj::Project, ship_catalog::ShipCatalog, Cli, Cmd};
use super::state::PreviewState;

const MAX_SESSIONS: usize = 8;
struct Session {
    state: Arc<PreviewState>,
    used: Instant,
    fingerprint: u64,
    // A streamed request or background job can still hold a previous snapshot.
    // Weak references account for those leases without retaining unused models.
    retired: Vec<Weak<PreviewState>>,
}

#[derive(Clone)]
pub struct PreviewService {
    pub game: Option<game::GameInstall>,
    pub selected_game: Option<PathBuf>,
    pub default: Option<Arc<PreviewState>>,
    home: PathBuf,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    epochs: Arc<Mutex<HashMap<String, Arc<AtomicU64>>>>,
    pub(super) thumbnails: Arc<super::thumbnails::ThumbnailCache>,
    pub(super) scans: Arc<super::scan::ScanSessions>,
    pub(super) comparisons: Arc<super::comparison::ComparisonCache>,
    pub(super) checkpoints: Arc<super::checkpoints::CheckpointStore>,
}

impl PreviewService {
    pub fn new(cli: &Cli, home: &Path, default: Option<Arc<PreviewState>>) -> Result<Self> {
        let (game, selected_game) = if let Some(st) = &default { (st.game.clone(), st.selected_game.clone()) }
            else { let (game, _) = game::locate(cli.game.as_deref()); let selected = cli.game.clone().or_else(|| game.as_ref().map(|g| g.root.clone())); (game, selected) };
        Ok(Self { selected_game,
            game, default, home: home.to_path_buf(), sessions: Arc::new(Mutex::new(HashMap::new())), epochs: Arc::new(Mutex::new(HashMap::new())),
            thumbnails: Arc::new(super::thumbnails::ThumbnailCache::new(home.join("library-thumbnails"))),
            scans: Arc::new(super::scan::ScanSessions::default()),
            comparisons: Arc::new(super::comparison::ComparisonCache::new(home.join("library-comparisons"))),
            checkpoints: Arc::new(super::checkpoints::CheckpointStore::open(home)?) })
    }

    pub fn legacy(st: Arc<PreviewState>) -> Result<Self> {
        Self::new(&selected_cli(st.selected_game.clone()), &crate::m4cmd::home_dir(), Some(st))
    }

    pub fn store(&self) -> Result<LibraryStore> { LibraryStore::open(&self.home) }

    pub fn source_revision(&self, id: &str) -> Result<Value> {
        let root = self.source(id)?;
        let manifest = crate::project_revision::read_source_manifest(&root)?;
        Ok(json!({"ok": true, "project_id": id, "revision": manifest.revision}))
    }

    pub fn list_checkpoints(&self, id: &str) -> Result<Value> {
        self.store()?.entry(id)?;
        let mut checkpoints = self.checkpoints.list(id)?;
        // Persisted pending snapshots survive server restarts. Opening their history
        // resumes the bounded frozen-source queue; live source files are not required.
        for checkpoint in checkpoints.iter_mut().filter(|c| c.preview.state == super::checkpoints::PreviewStatus::Pending).take(8) {
            if let Err(error) = self.checkpoints.queue_preview(id, &checkpoint.id, self.selected_game.clone()) {
                checkpoint.preview.state = super::checkpoints::PreviewStatus::Failed;
                checkpoint.preview.reason = Some(format!("{error:#}"));
            }
        }
        Ok(json!({"ok": true, "checkpoints": checkpoints,
            "scope_version": crate::project_revision::SOURCE_SCOPE_VERSION,
            "excluded_roots": crate::project_revision::EXCLUDED_ROOTS}))
    }

    pub fn create_checkpoint(&self, id: &str, request: super::checkpoints::CreateCheckpoint) -> Result<Value> {
        let root = self.source(id)?;
        let mut checkpoint = self.checkpoints.create_with_context(id, &root, request, self.game.as_ref().map(|g| g.build.as_str()))?;
        // A preview failure must not turn a successfully saved source snapshot into a failed save.
        if let Err(error) = self.checkpoints.queue_preview(id, &checkpoint.id, self.selected_game.clone()) {
            checkpoint.preview.state = super::checkpoints::PreviewStatus::Failed;
            checkpoint.preview.reason = Some(format!("{error:#}"));
        }
        Ok(json!({"ok": true, "checkpoint": checkpoint}))
    }

    pub fn restore_checkpoint(&self, id: &str, checkpoint: &str, destination: &Path, revision: &str) -> Result<Value> {
        let store = self.store()?;
        store.entry(id)?;
        let mut forbidden: Vec<_> = store.entries()?.into_iter().map(|e| e.path).collect();
        if let Some(game) = &self.selected_game { forbidden.push(game.clone()); }
        let restored = self.checkpoints.restore_with_context(id, checkpoint, destination, revision, &forbidden)?;
        let mut warnings = restored.warnings;
        let entry = match store.add(&restored.path) {
            Ok(entry) => Some(entry),
            Err(error) => { warnings.push(format!("源文件已恢复，登记失败；可通过添加项目登记 {}：{error:#}", restored.path.display())); None }
        };
        let url = entry.as_ref().map(|e| format!("/?project={}", super::thumbnails::encode_id(&e.id)));
        Ok(json!({"ok": true, "entry": entry, "checkpoint_id": restored.checkpoint_id,
            "revision": restored.revision, "url": url, "warnings": warnings, "restored_path": restored.path}))
    }

    pub fn compatibility(&self, id: &str) -> Result<Value> {
        let entry = self.store()?.entry(id)?;
        let records = crate::workbench_records::VerificationStore::open(&self.home)?.list(id)?;
        let mut warnings = vec![];
        let current_revision = match crate::project_revision::read_source_manifest(&entry.path) {
            Ok(manifest) => Some(manifest.revision),
            Err(e) => { warnings.push(format!("{e:#}")); None }
        };
        let project_build = crate::library::read_project_config(&entry.path).ok().map(|config| config.meta.game_build);
        let target_build = self.game.as_ref().map(|game| game.build.as_str());
        let matching_record_id = current_revision.as_deref().zip(target_build)
            .and_then(|(revision, build)| crate::workbench_records::latest_matching(&records, id, revision, build))
            .map(|record| record.id.clone());
        Ok(json!({"ok": true, "project_id": id, "project_build": project_build,
            "target_build": target_build, "current_revision": current_revision,
            "install": install_status(&entry.path, self.game.as_ref()), "records": records,
            "matching_record_id": matching_record_id, "warnings": warnings}))
    }

    pub fn record_verification(&self, id: &str, request: crate::workbench_records::VerificationRequest) -> Result<Value> {
        let root = self.source(id)?;
        let record = crate::workbench_records::VerificationStore::open(&self.home)?.append(id, &root, request)?;
        Ok(json!({"ok": true, "record": record}))
    }

    pub fn health(&self) -> Value {
        json!({"ok": true, "api_version": 1, "server": "wowsmod", "version": env!("CARGO_PKG_VERSION"), "library_mode": self.default.is_none(), "time": super::state::iso_now()})
    }

    pub fn listing(&self) -> Result<Value> {
        let listing = self.store()?.list()?;
        let metadata = ShipCatalog::load(&self.home, self.game.as_ref());
        let mut ships = BTreeMap::<String, Value>::new();
        let mut nations = std::collections::BTreeSet::new();
        let mut levels = std::collections::BTreeSet::new();
        let mut species = std::collections::BTreeSet::new();
        let mut projects = vec![];
        let records = crate::workbench_records::VerificationStore::open(&self.home)?;
        let mut record_warnings = vec![];
        for entry in listing.entries {
            let ship = metadata.lookup(entry.project.as_ref().map(|p| p.ship.as_str()).unwrap_or_default());
            let mut value = serde_json::to_value(&entry)?;
            let meta = serde_json::to_value(&ship)?;
            if let Some(v) = meta["nation"].as_str() { nations.insert(v.to_string()); }
            if let Some(v) = meta["level"].as_i64() { levels.insert(v); }
            if let Some(v) = meta["species"].as_str() { species.insert(v.to_string()); }
            let key = meta["key"].as_str().unwrap_or("").to_string();
            let group = ships.entry(key.clone()).or_insert_with(|| json!({"key": key, "ship_metadata": meta, "projects": 0}));
            group["projects"] = json!(group["projects"].as_u64().unwrap_or(0) + 1);
            value["ship_metadata"] = serde_json::to_value(ship)?;
            value["install"] = install_status(&entry.entry.path, self.game.as_ref());
            value["verification"] = match records.latest(&entry.entry.id) {
                Ok(record) => serde_json::to_value(record)?,
                Err(e) => { record_warnings.push(format!("{}: {e:#}", entry.entry.id)); Value::Null }
            };
            projects.push(value);
        }
        let mut warnings = listing.warnings;
        warnings.extend(record_warnings);
        warnings.extend(metadata.source.warnings.clone());
        Ok(json!({"ok": true, "projects": projects, "ships": ships.into_values().collect::<Vec<_>>(),
            "facets": {"nations": nations, "levels": levels, "species": species, "statuses": ["working", "review", "complete", "archived"]},
            "game": {"available": self.game.is_some(), "path": self.selected_game, "build": self.game.as_ref().map(|g| &g.build),
                "version": self.game.as_ref().and_then(|g| g.client_version.as_ref()), "running": !game::running().is_empty()},
            "warnings": warnings, "metadata_source": metadata.source}))
    }

    /// Recheck the catalog/source on every request, including already-cached sessions.
    pub fn source(&self, id: &str) -> Result<PathBuf> {
        self.store()?.resolve(id).map_err(|e| {
            let message = format!("{e:#}");
            if message.starts_with("目录库中没有项目 ") { anyhow!("not_found: {message}") }
            else { anyhow!("project_unavailable: 项目目录不可用，请重新定位或修复 mod.toml：{message}") }
        })
    }

    pub fn session(&self, id: &str) -> Result<Arc<PreviewState>> {
        self.acquire_session(id, None)
    }

    /// Comparisons verify source bytes, so they must never inherit a metadata-only session hit.
    pub(super) fn comparison_session(&self, id: &str, context: &str) -> Result<Arc<PreviewState>> {
        self.acquire_session(id, Some(context))
    }

    fn acquire_session(&self, id: &str, comparison_context: Option<&str>) -> Result<Arc<PreviewState>> {
        // The session gate is outermost. Job checks release their guard before
        // catalog mutation takes its file lock; job workers never take this gate.
        // Path resolution belongs inside this gate, so a queued request cannot
        // carry an old path past a successful relocation/removal.
        let mut sessions = self.sessions.lock().unwrap();
        let source = self.source(id)?;
        if let Some(session) = sessions.get_mut(id) {
            session.used = Instant::now();
            session.retired.retain(|state| state.strong_count() > 0);
            let same_source = same_path(&session.state.project.root, &source);
            if comparison_context.is_none() && same_source && session.fingerprint == super::state::project_fingerprint(&source) {
                return Ok(Arc::clone(&session.state));
            }
            if !same_source && session_busy(session) { bail!("session_busy: 项目任务尚未完成，请稍后重新定位"); }
            let (state, fingerprint) = self.open_snapshot(id, &source, Some(&session.state))?;
            if let Some(context) = comparison_context { state.prepare_comparison_geometry(context)?; }
            let fingerprint = if comparison_context.is_some() { state.fingerprint() } else { fingerprint };
            if Arc::strong_count(&session.state) > 1 { session.retired.push(Arc::downgrade(&session.state)); }
            // Capture after preview generation: its own cache output is the
            // baseline, not another input edit that perpetually rebuilds itself.
            session.fingerprint = fingerprint;
            session.state = Arc::clone(&state);
            return Ok(state);
        }
        if sessions.len() >= MAX_SESSIONS {
            let oldest = sessions.iter().filter(|(_, s)| !session_busy(s))
                .min_by_key(|(_, s)| s.used).map(|(id, _)| id.clone());
            if let Some(id) = oldest { sessions.remove(&id); }
            else { bail!("session_busy: 预览会话正在使用，请等待任务结束后重试"); }
        }
        let (state, fingerprint) = self.open_snapshot(id, &source, None)?;
        if let Some(context) = comparison_context { state.prepare_comparison_geometry(context)?; }
        let fingerprint = if comparison_context.is_some() { state.fingerprint() } else { fingerprint };
        sessions.insert(id.to_string(), Session { state: Arc::clone(&state), used: Instant::now(), fingerprint, retired: vec![] });
        Ok(state)
    }

    fn open_snapshot(&self, id: &str, source: &Path, previous: Option<&Arc<PreviewState>>) -> Result<(Arc<PreviewState>, u64)> {
        let pinned = self.selected_game.clone().or_else(|| Some(self.home.join("unavailable-client")));
        for _ in 0..3 {
            let before = super::state::project_source_fingerprint(source);
            let mut state = PreviewState::open(&selected_cli(pinned.clone()), source)?;
            let fingerprint = state.fingerprint();
            // An editor may save another asset while assembly is running. Do
            // not bless that later source stamp as if this snapshot had read it.
            if before != super::state::project_source_fingerprint(source) { continue; }
            if let Some(previous) = previous { state.jobs = Arc::clone(&previous.jobs); }
            // Keep a lightweight per-ID epoch after model eviction, too.
            state.etag = Arc::clone(self.epochs.lock().unwrap().entry(id.to_string()).or_insert_with(|| Arc::new(AtomicU64::new(0))));
            state.bump("project", json!({"refreshed": previous.is_some()}));
            return Ok((Arc::new(state), fingerprint));
        }
        bail!("session_busy: 项目文件持续变化，请等待当前编辑完成后重试")
    }

    pub fn relocate(&self, id: &str, path: &Path) -> Result<crate::library::LibraryEntry> {
        let sessions = self.sessions.lock().unwrap();
        if sessions.get(id).map(session_busy).unwrap_or(false) { bail!("session_busy: 项目任务尚未完成，请稍后重试"); }
        // Hold the session gate throughout the catalog's own atomic mutation.
        // The old idle snapshot remains until the next acquisition rebuilds it,
        // retaining its completed-job identities and monotonic epoch.
        self.store()?.relocate(id, path)
    }

    pub fn remove(&self, id: &str) -> Result<crate::library::LibraryEntry> {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.get(id).map(session_busy).unwrap_or(false) { bail!("session_busy: 项目任务尚未完成，请稍后重试"); }
        let entry = self.store()?.remove(id)?;
        sessions.remove(id);
        Ok(entry)
    }
}

fn session_busy(session: &Session) -> bool {
    busy(&session.state) || Arc::strong_count(&session.state) > 1 || session.retired.iter().any(|state| state.strong_count() > 0)
}

fn busy(state: &PreviewState) -> bool {
    state.jobs.lock().unwrap().values().any(|j| j["state"] == "running")
}

pub fn selected_cli(game: Option<PathBuf>) -> Cli {
    Cli { game, json: true, json_out: None, cmd: Cmd::Doctor }
}

pub(super) fn path_identity(path: &Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let s = canonical.to_string_lossy().replace('\\', "/").trim_start_matches("//?/").trim_end_matches('/').to_string();
    if cfg!(windows) { s.to_lowercase() } else { s }
}
pub fn same_path(a: &Path, b: &Path) -> bool { path_identity(a) == path_identity(b) }

/// Registry identity is authoritative; a project name or stale .install is insufficient.
pub fn install_status(root: &Path, game: Option<&game::GameInstall>) -> Value {
    let empty = |state: &str, reason: Option<String>| json!({"state": state, "rev": null, "files": 0, "time": null, "reason": reason});
    let Some(game) = game else { return empty("unknown", Some("指定客户端不可用".into())); };
    let reg = match crate::registry::load(&game.res_mods) {
        Ok(reg) => reg, Err(e) => return empty("unknown", Some(format!("{e:#}"))),
    };
    let matches: Vec<_> = reg.mods.values().filter(|m| !m.project_dir.is_empty() && same_path(Path::new(&m.project_dir), root)).collect();
    if matches.is_empty() { return empty("not_installed", None); }
    if matches.len() != 1 { return empty("unknown", Some("同一项目有多个登记记录，请检查安装登记表".into())); }
    let m = matches[0];
    let check = || -> Result<(&str, Option<String>)> {
        if m.game_build != game.build { return Ok(("unknown", Some("安装登记的构建号与当前客户端不同".into()))); }
        let root = std::path::absolute(root)?;
        let config = crate::library::read_project_config(&root)?;
        let mut source = HashMap::new();
        for path in crate::library::source_asset_files(&root)? {
            let rel = crate::modproj::rel_slash_pub(&root.join(crate::modproj::ASSETS), &path).to_lowercase();
            source.insert(rel, path);
        }
        let mut changed = config.install.mode != m.mode || config.meta.name != m.name
            || config.meta.version != m.version || crate::ship_catalog::canonical_ship_key(&config.meta.ship) != crate::ship_catalog::canonical_ship_key(&m.ship);
        for key in &m.files {
            let p = reg.paths.get(key).ok_or_else(|| anyhow!("登记文件缺失: {key}"))?;
            // Refuse malformed registry paths before reading outside the selected client's root.
            if p.rel.contains(':') || p.rel.replace('\\', "/").split('/').any(|c| c == ".." || c.is_empty()) { bail!("登记文件路径无效: {}", p.rel); }
            let installed = crate::project_revision::read_plain_file(&game.res_mods.join(&p.rel), 512 * 1024 * 1024)?;
            if crate::modproj::sha256_hex(&installed) != p.sha256 { return Ok(("unknown", Some(format!("已应用文件缺失或已被修改: {}", p.rel)))); }
            if let Some(path) = source.remove(key) {
                let raw = crate::project_revision::read_plain_file(&path, 512 * 1024 * 1024)?;
                if crate::modproj::is_binary_stub(&p.rel, &raw) {
                    // A previously custom resource changed back to a placeholder:
                    // its effective application now needs game-fill, not the
                    // custom bytes still installed. Only an existing game-fill
                    // record for this build can justify retaining installed.
                    if !p.origin.starts_with("game-fill:") { changed = true; }
                } else if crate::modproj::sha256_hex(&raw) != p.sha256 { changed = true; }
            } else if p.origin == "project" { changed = true; }
        }
        if !source.is_empty() { changed = true; }
        Ok((if changed { "outdated" } else { "installed" }, None))
    };
    let (state, reason) = check().unwrap_or_else(|e| ("unknown", Some(format!("{e:#}"))));
    json!({"state": state, "rev": null, "files": m.files.len(), "time": m.installed_unix, "project_dir": m.project_dir,
        "name": m.name, "build": game.build, "target_client": game.root, "reason": reason})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{modproj::InstallItem, registry::{ModInfo, Opts}};
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Fixture { root: PathBuf }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!("wowsmod_preview_sessions_{}_{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir_all(&root).unwrap(); Self { root }
        }
        fn project(&self, dir: &str) -> Project { Project::init(&self.root.join(dir), "same", "UNKNOWN", "loose", "123").unwrap() }
        fn game(&self, dir: &str) -> game::GameInstall {
            let root = self.root.join(dir);
            game::GameInstall { res_mods: root.join("bin/123/res_mods"), idx_dir: root.join("bin/123/idx"), res_packages: root.join("res_packages"),
                root, game_id: None, is_lesta: true, client_version: None, build: "123".into(), bin_dirs: vec!["123".into()], found_by: "fixture".into() }
        }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.root); } }
    #[cfg(windows)]
    #[test]
    fn listing_installed_project_rejects_asset_junction_without_following_it() {
        let f = Fixture::new(); let project = f.project("linked-installed"); let game = f.game("safe-client");
        std::fs::create_dir_all(&game.res_mods).unwrap();
        std::fs::write(project.assets().join("paint.txt"), b"paint").unwrap();
        let info = ModInfo { name: project.cfg.meta.name.clone(), version: project.cfg.meta.version.clone(), ship: project.cfg.meta.ship.clone(), mode: "loose".into(), project_dir: project.root.clone(), source: "project".into(), game_build: "123".into() };
        crate::registry::install(&game.res_mods, &info, &[InstallItem { rel: "paint.txt".into(), data: b"paint".to_vec(), origin: "project".into() }], &Opts { dry_run: false, allow_same_ship: false }).unwrap();
        let home = f.root.join("home");
        let mut service = PreviewService::new(&selected_cli(Some(game.root.clone())), &home, None).unwrap();
        service.store().unwrap().add(&project.root).unwrap(); service.game = Some(game);
        assert_eq!(service.listing().unwrap()["projects"][0]["install"]["state"], "installed");
        let assets = project.root.join("assets").join("res_mods");
        let link = assets.join("cycle");
        let output = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&link).arg(&assets).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let listing = service.listing().unwrap();
        assert!(listing["projects"][0]["modified_unix"].is_null());
        assert_eq!(listing["projects"][0]["install"]["state"], "unknown");
        assert!(!listing["projects"][0]["warnings"].as_array().unwrap().is_empty());
        std::fs::remove_dir(&link).unwrap();
    }
    #[test]
    fn opening_history_resumes_pending_checkpoint_after_service_restart() {
        let f = Fixture::new(); let project = f.project("checkpoint-resume");
        let home = f.root.join("home"); let cli = selected_cli(Some(f.root.join("no-client")));
        let first = PreviewService::new(&cli, &home, None).unwrap();
        let id = first.store().unwrap().add(&project.root).unwrap().id;
        let request = super::super::checkpoints::CreateCheckpoint { label: "pending".into(), expected_revision: crate::project_revision::read_source_manifest(&project.root).unwrap().revision };
        let saved = first.checkpoints.create_with_context(&id, &project.root, request, Some("unavailable-captured-client")).unwrap();
        drop(first);
        std::fs::remove_dir_all(&project.root).unwrap();
        let reopened = PreviewService::new(&cli, &home, None).unwrap();
        assert_eq!(reopened.checkpoints.list(&id).unwrap()[0].preview.state, super::super::checkpoints::PreviewStatus::Pending);
        reopened.list_checkpoints(&id).unwrap();
        let until = Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let current = reopened.checkpoints.list(&id).unwrap().remove(0);
            if current.preview.state != super::super::checkpoints::PreviewStatus::Pending {
                assert_eq!(current.id, saved.id); assert_eq!(current.revision, saved.revision);
                assert_eq!(current.preview.state, super::super::checkpoints::PreviewStatus::Failed);
                assert!(current.preview.reason.unwrap().contains("build"));
                break;
            }
            assert!(Instant::now() < until, "reopened pending preview never resumed");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn comparison_refresh_pins_fresh_materials_and_preserves_session_jobs() {
        let f = Fixture::new(); let project = f.project("comparison-fresh");
        let material_dir = project.assets().join("PnFMods/same/UNKNOWN");
        std::fs::create_dir_all(&material_dir).unwrap();
        let material = material_dir.join("paint.mfm");
        let value = |n| format!("<mfm><fx>PBS_ship.fx</fx><property>g_detailScaleU<Float>{n}</Float></property></mfm>");
        std::fs::write(&material, value(3)).unwrap();
        let stamp = std::fs::metadata(&material).unwrap().modified().unwrap();
        let cache = project.root.join(".wowsmod/cache"); std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("geometry.glb"), b"custom geometry").unwrap();
        let service = PreviewService::new(&selected_cli(Some(f.root.join("no-client"))), &f.root.join("home"), None).unwrap();
        let id = service.store().unwrap().add(&project.root).unwrap().id;
        let old = service.session(&id).unwrap();
        old.jobs.lock().unwrap().insert("kept".into(), json!({"state":"done"}));
        std::fs::write(&material, value(7)).unwrap();
        std::fs::File::options().write(true).open(&material).unwrap().set_times(std::fs::FileTimes::new().set_modified(stamp)).unwrap();
        let fresh = service.comparison_session(&id, "context").unwrap();
        assert!(!Arc::ptr_eq(&old, &fresh));
        assert_eq!(fresh.model_view(None).unwrap()["materials"]["paint.mfm"]["params"]["g_detailScaleU"], 7.0);
        assert_eq!(fresh.jobs.lock().unwrap()["kept"]["state"], "done");
        assert!(Arc::ptr_eq(&fresh, &service.session(&id).unwrap()), "prepared cache fingerprint must match the published session");
        assert!(service.remove(&id).is_err(), "retired and current render leases block removal");
        assert_eq!(std::fs::read(cache.join("geometry.glb")).unwrap(), b"custom geometry", "unmarked authored cache is preserved");
    }

    #[test]
    fn installed_identity_and_content_are_independent_from_name_and_revision() {
        let f = Fixture::new(); let mut a = f.project("a"); let b = f.project("b"); let game = f.game("client");
        std::fs::create_dir_all(&game.res_mods).unwrap();
        std::fs::write(a.assets().join("paint.txt"), b"a").unwrap();
        std::fs::write(a.assets().join("paint.mfm"), b"<mfm>original</mfm>").unwrap();
        std::fs::write(b.assets().join("paint.txt"), b"a").unwrap();
        let info = ModInfo { name: "same".into(), version: a.cfg.meta.version.clone(), ship: a.cfg.meta.ship.clone(), mode: "loose".into(),
            project_dir: a.root.clone(), source: "project".into(), game_build: "123".into() };
        crate::registry::install(&game.res_mods, &info, &[InstallItem { rel: "paint.txt".into(), data: b"a".to_vec(), origin: "project".into() },
            InstallItem { rel: "paint.mfm".into(), data: b"<mfm>original</mfm>".to_vec(), origin: "project".into() }], &Opts { dry_run: false, allow_same_ship: false }).unwrap();
        assert_eq!(install_status(&a.root, Some(&game))["state"], "installed");
        assert!(install_status(&a.root, Some(&game))["rev"].is_null());
        assert_eq!(install_status(&b.root, Some(&game))["state"], "not_installed");
        assert_eq!(install_status(&a.root, Some(&f.game("other-client")))["state"], "not_installed");
        std::fs::write(a.assets().join("paint.mfm"), b"<mfm>edited</mfm>").unwrap();
        assert_eq!(install_status(&a.root, Some(&game))["state"], "outdated", "small XML materials are real content, not binary placeholders");
        std::fs::write(a.assets().join("paint.mfm"), b"<mfm>original</mfm>").unwrap();
        a.cfg.install.mode = "sdk".into(); a.save_cfg().unwrap();
        assert_eq!(install_status(&a.root, Some(&game))["state"], "outdated", "changing application mode changes the effective candidate");
        a.cfg.install.mode = "loose".into(); a.save_cfg().unwrap();
        std::fs::write(a.assets().join("paint.txt"), b"new paint").unwrap();
        assert_eq!(install_status(&a.root, Some(&game))["state"], "outdated");
        std::fs::write(game.res_mods.join("paint.txt"), b"external edit").unwrap();
        assert_eq!(install_status(&a.root, Some(&game))["state"], "unknown");
    }

    #[test]
    fn session_disposal_is_bounded_and_preserves_in_flight_requests_and_jobs() {
        let f = Fixture::new();
        let service = PreviewService::new(&selected_cli(Some(f.root.join("no-client"))), &f.root.join("home"), None).unwrap();
        let a = f.project("a"); let id = service.store().unwrap().add(&a.root).unwrap().id;
        assert_eq!(service.listing().unwrap()["projects"].as_array().unwrap().len(), 1);
        assert!(service.sessions.lock().unwrap().is_empty());
        let state = service.session(&id).unwrap();
        assert!(service.remove(&id).is_err(), "cannot detach a session already serving a request");
        state.jobs.lock().unwrap().insert("active".into(), json!({"state":"running"}));
        drop(state);
        for n in 0..MAX_SESSIONS + 3 {
            let p = f.project(&format!("p{n}")); let id = service.store().unwrap().add(&p.root).unwrap().id;
            drop(service.session(&id).unwrap());
        }
        assert!(service.sessions.lock().unwrap().len() <= MAX_SESSIONS);
        assert_eq!(service.session(&id).unwrap().jobs.lock().unwrap()["active"]["state"], "running");
        assert!(service.remove(&id).is_err());
    }

    #[test]
    fn damaged_catalog_does_not_disable_legacy_preview_or_warm_render() {
        let f = Fixture::new(); let project = f.project("legacy");
        let cli = selected_cli(Some(f.root.join("no-client")));
        let state = Arc::new(PreviewState::open(&cli, &project.root).unwrap());
        let home = f.root.join("home"); std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("library.json"), b"broken").unwrap();
        let service = PreviewService::new(&cli, &home, Some(state)).expect("catalog is independent from legacy preview");
        assert_eq!(service.health()["library_mode"], false);
        assert!(service.listing().is_err());
        assert_eq!(std::fs::read(home.join("library.json")).unwrap(), b"broken");
    }

    #[test]
    fn refreshes_same_id_source_config_and_history_without_losing_jobs() {
        let f = Fixture::new(); let mut project = f.project("changing");
        let material_dir = project.assets().join("PnFMods/same/UNKNOWN");
        std::fs::create_dir_all(&material_dir).unwrap();
        let material = |value: u8| format!("<mfm><fx>PBS_ship.fx</fx><property>g_detailScaleU<Float>{value}</Float></property></mfm>");
        std::fs::write(material_dir.join("paint.mfm"), material(3)).unwrap();
        let cache = project.root.join(".wowsmod/cache"); std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("geometry.glb"), b"geometry before").unwrap();
        let service = PreviewService::new(&selected_cli(Some(f.root.join("no-client"))), &f.root.join("home"), None).unwrap();
        let id = service.store().unwrap().add(&project.root).unwrap().id;
        let original = service.session(&id).unwrap();
        assert_eq!(original.head_id().as_deref(), Some("current"));
        let etag = original.etag();
        original.jobs.lock().unwrap().insert("finished".into(), json!({"state":"done", "result":"kept"}));
        original.jobs.lock().unwrap().insert("active".into(), json!({"state":"running"}));
        std::fs::write(material_dir.join("paint.mfm"), material(7)).unwrap();
        let changed = service.session(&id).unwrap();
        assert_eq!(changed.model_view(None).unwrap()["materials"]["paint.mfm"]["params"]["g_detailScaleU"], 7.0);
        assert_eq!(changed.head_id(), original.head_id());
        assert!(changed.etag() > etag);
        assert_eq!(changed.jobs.lock().unwrap()["finished"]["result"], "kept");
        original.jobs.lock().unwrap().insert("active".into(), json!({"state":"done"}));
        original.bump("job", json!({"id":"active"}));
        assert_eq!(changed.jobs.lock().unwrap()["active"]["state"], "done");
        // Release the current snapshot; only the old request keeps a lease.
        // Checking the newest Arc's reference count alone must not pass this.
        drop(changed);
        assert!(service.remove(&id).unwrap_err().to_string().contains("session_busy"));
        assert!(service.relocate(&id, &f.root.join("absent")).unwrap_err().to_string().contains("session_busy"));
        drop(original);
        let changed = service.session(&id).unwrap();
        let etag = changed.etag();
        project.cfg.meta.name = "changed".into(); project.save_cfg().unwrap();
        std::fs::rename(project.assets().join("PnFMods/same"), project.assets().join("PnFMods/changed")).unwrap();
        let renamed = service.session(&id).unwrap();
        assert_eq!(renamed.project_view()["project"]["name"], "changed");
        assert_eq!(renamed.model_view(None).unwrap()["materials"]["paint.mfm"]["params"]["g_detailScaleU"], 7.0);
        assert!(renamed.etag() > etag); drop(changed);
        let etag = renamed.etag();
        std::fs::write(project.root.join("edits/001_test.json"), br#"{"cmd":"tex_recolor","outputs":[],"time_unix":1}"#).unwrap();
        let edited = service.session(&id).unwrap();
        assert_eq!(edited.head_id().as_deref(), Some("e001")); assert!(edited.etag() > etag); drop(renamed);
        let etag = edited.etag(); let old_geometry = edited.model_view(None).unwrap()["geometry"]["hash"].clone();
        std::fs::write(cache.join("geometry.glb"), b"geometry after").unwrap();
        let geometry = service.session(&id).unwrap();
        assert_ne!(geometry.model_view(None).unwrap()["geometry"]["hash"], old_geometry);
        assert!(geometry.etag() > etag); drop(edited);
        let unchanged = service.session(&id).unwrap();
        assert!(Arc::ptr_eq(&geometry, &unchanged));
        assert_eq!(geometry.etag(), unchanged.etag(), "cache reads/writes must not cause refresh loops");
        std::fs::create_dir_all(project.root.join(".install")).unwrap();
        std::fs::write(project.root.join(".install/manifest.json"), "{}").unwrap();
        std::fs::write(cache.join("geometry.model_key"), "preview generated key").unwrap();
        assert!(Arc::ptr_eq(&geometry, &service.session(&id).unwrap()), "runtime outputs do not change source generation");
    }

    #[test]
    fn monotonic_project_epoch_survives_idle_model_eviction() {
        let f = Fixture::new();
        let service = PreviewService::new(&selected_cli(Some(f.root.join("no-client"))), &f.root.join("home"), None).unwrap();
        let p = f.project("oldest"); let id = service.store().unwrap().add(&p.root).unwrap().id;
        let first = service.session(&id).unwrap(); let epoch = first.etag(); drop(first);
        for n in 0..MAX_SESSIONS {
            let p = f.project(&format!("p{n}")); let next = service.store().unwrap().add(&p.root).unwrap().id;
            drop(service.session(&next).unwrap());
        }
        assert!(!service.sessions.lock().unwrap().contains_key(&id));
        assert!(service.session(&id).unwrap().etag() > epoch);
    }

    #[test]
    fn catalog_identity_changes_serialize_with_queued_session_requests() {
        for relocate in [true, false] {
            let f = Fixture::new(); let a = f.project("a"); let b = f.project("b");
            let service = Arc::new(PreviewService::new(&selected_cli(Some(f.root.join("no-client"))), &f.root.join("home"), None).unwrap());
            let id = service.store().unwrap().add(&a.root).unwrap().id;
            drop(service.session(&id).unwrap());
            let lock = f.root.join("home/library.lock"); std::fs::write(&lock, "held by test").unwrap();
            let controller = Arc::clone(&service); let mutation_id = id.clone();
            let mutation = std::thread::spawn(move || if relocate { controller.relocate(&mutation_id, &b.root) } else { controller.remove(&mutation_id) });
            // Observe entry into the critical section without a production timing hook:
            // before the fix the entry disappears; after the fix the controller holds its gate.
            let deadline = Instant::now() + std::time::Duration::from_secs(2);
            loop {
                match service.sessions.try_lock() {
                    Ok(sessions) if !sessions.contains_key(&id) => break,
                    Err(std::sync::TryLockError::WouldBlock) => break,
                    _ => {},
                }
                assert!(Instant::now() < deadline, "mutation did not reach its session gate"); std::thread::yield_now();
            }
            let controller = Arc::clone(&service); let requested_id = id.clone();
            let (send, receive) = std::sync::mpsc::channel();
            let request = std::thread::spawn(move || { send.send(controller.session(&requested_id)).unwrap(); });
            let early = receive.recv_timeout(std::time::Duration::from_millis(250)).ok();
            std::fs::remove_file(&lock).unwrap();
            let mutation_result = mutation.join().unwrap();
            let arrived_early = early.is_some();
            let acquired = early.unwrap_or_else(|| receive.recv_timeout(std::time::Duration::from_secs(5)).unwrap());
            request.join().unwrap();
            mutation_result.unwrap();
            assert!(!arrived_early, "a queued request must not acquire the old source while relocate/remove waits for catalog lock");
            if relocate { assert!(same_path(&acquired.unwrap().project.root, &f.root.join("b"))); }
            else { assert!(acquired.err().unwrap().to_string().contains("not_found")); }
        }
    }

    #[test]
    fn real_source_replaced_with_stub_is_outdated_but_game_fill_stubs_remain_valid() {
        let f = Fixture::new(); let p = f.project("a"); let game = f.game("client"); std::fs::create_dir_all(&game.res_mods).unwrap();
        std::fs::write(p.assets().join("custom.geometry"), b"CUSTOM-GEOMETRY").unwrap();
        std::fs::write(p.assets().join("base.geometry"), b"<SDK placeholder/>").unwrap();
        let info = ModInfo { name: "same".into(), version: p.cfg.meta.version.clone(), ship: p.cfg.meta.ship.clone(), mode: "loose".into(),
            project_dir: p.root.clone(), source: "project".into(), game_build: "123".into() };
        crate::registry::install(&game.res_mods, &info, &[
            InstallItem { rel:"custom.geometry".into(), data:b"CUSTOM-GEOMETRY".to_vec(), origin:"project".into() },
            InstallItem { rel:"base.geometry".into(), data:b"GAME-GEOMETRY".to_vec(), origin:"game-fill:content/base.geometry".into() },
        ], &Opts { dry_run:false, allow_same_ship:false }).unwrap();
        assert_eq!(install_status(&p.root, Some(&game))["state"], "installed");
        std::fs::write(p.assets().join("custom.geometry"), b"<SDK placeholder/>").unwrap();
        assert_eq!(install_status(&p.root, Some(&game))["state"], "outdated");
        assert_eq!(std::fs::read(game.res_mods.join("custom.geometry")).unwrap(), b"CUSTOM-GEOMETRY");
        std::fs::write(p.assets().join("custom.geometry"), b"CUSTOM-GEOMETRY").unwrap();
        assert_eq!(install_status(&p.root, Some(&game))["state"], "installed");
    }
}
