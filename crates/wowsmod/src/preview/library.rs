//! Lightweight library controller and lazily-created, explicitly identified preview sessions.
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use crate::{game, library::LibraryStore, modproj::Project, ship_catalog::ShipCatalog, Cli, Cmd};
use super::state::PreviewState;

const MAX_SESSIONS: usize = 8;
struct Session { state: Arc<PreviewState>, used: Instant }

pub struct PreviewService {
    pub game: Option<game::GameInstall>,
    pub selected_game: Option<PathBuf>,
    pub default: Option<Arc<PreviewState>>,
    home: PathBuf,
    sessions: Mutex<HashMap<String, Session>>,
}

impl PreviewService {
    pub fn new(cli: &Cli, home: &Path, default: Option<Arc<PreviewState>>) -> Result<Self> {
        let (game, selected_game) = if let Some(st) = &default { (st.game.clone(), st.selected_game.clone()) }
            else { let (game, _) = game::locate(cli.game.as_deref()); let selected = cli.game.clone().or_else(|| game.as_ref().map(|g| g.root.clone())); (game, selected) };
        Ok(Self { selected_game,
            game, default, home: home.to_path_buf(), sessions: Mutex::new(HashMap::new()) })
    }

    pub fn legacy(st: Arc<PreviewState>) -> Result<Self> {
        Self::new(&selected_cli(st.selected_game.clone()), &crate::m4cmd::home_dir(), Some(st))
    }

    pub fn store(&self) -> Result<LibraryStore> { LibraryStore::open(&self.home) }

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
            projects.push(value);
        }
        let mut warnings = listing.warnings;
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
        let source = self.source(id)?;
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(id) {
            if same_path(&session.state.project.root, &source) {
                session.used = Instant::now();
                return Ok(Arc::clone(&session.state));
            }
            if busy(&session.state) || Arc::strong_count(&session.state) > 1 { bail!("session_busy: 项目任务尚未完成，请稍后重新定位"); }
            sessions.remove(id);
        }
        if sessions.len() >= MAX_SESSIONS {
            let oldest = sessions.iter().filter(|(_, s)| !busy(&s.state) && Arc::strong_count(&s.state) == 1)
                .min_by_key(|(_, s)| s.used).map(|(id, _)| id.clone());
            if let Some(id) = oldest { sessions.remove(&id); }
            else { bail!("session_busy: 预览会话正在使用，请等待任务结束后重试"); }
        }
        // No available client means no child command may rediscover a real client.
        let pinned = self.selected_game.clone().or_else(|| Some(self.home.join("unavailable-client")));
        let state = Arc::new(PreviewState::open(&selected_cli(pinned), &source)?);
        sessions.insert(id.to_string(), Session { state: Arc::clone(&state), used: Instant::now() });
        Ok(state)
    }

    pub fn invalidate(&self, id: &str) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.get(id).map(|s| busy(&s.state) || Arc::strong_count(&s.state) > 1).unwrap_or(false) { bail!("session_busy: 项目任务尚未完成，请稍后重试"); }
        sessions.remove(id);
        Ok(())
    }
}

fn busy(state: &PreviewState) -> bool {
    state.jobs.lock().unwrap().values().any(|j| j["state"] == "running")
}

pub fn selected_cli(game: Option<PathBuf>) -> Cli {
    Cli { game, json: true, json_out: None, cmd: Cmd::Doctor }
}

fn path_identity(path: &Path) -> String {
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
        let project = Project::open(root)?;
        let mut source = HashMap::new();
        for path in project.files()? {
            let rel = crate::modproj::rel_slash_pub(&project.assets(), &path).to_lowercase();
            source.insert(rel, path);
        }
        let mut changed = project.cfg.install.mode != m.mode || project.cfg.meta.name != m.name
            || project.cfg.meta.version != m.version || crate::ship_catalog::canonical_ship_key(&project.cfg.meta.ship) != crate::ship_catalog::canonical_ship_key(&m.ship);
        for key in &m.files {
            let p = reg.paths.get(key).ok_or_else(|| anyhow!("登记文件缺失: {key}"))?;
            // Refuse malformed registry paths before reading outside the selected client's root.
            if p.rel.contains(':') || p.rel.replace('\\', "/").split('/').any(|c| c == ".." || c.is_empty()) { bail!("登记文件路径无效: {}", p.rel); }
            let installed = std::fs::read(game.res_mods.join(&p.rel))?;
            if crate::modproj::sha256_hex(&installed) != p.sha256 { return Ok(("unknown", Some(format!("已应用文件缺失或已被修改: {}", p.rel)))); }
            if let Some(path) = source.remove(key) {
                let raw = std::fs::read(path)?;
                if !crate::modproj::is_binary_stub(&p.rel, &raw) && crate::modproj::sha256_hex(&raw) != p.sha256 { changed = true; }
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
        assert!(service.invalidate(&id).is_err(), "cannot detach a session already serving a request");
        state.jobs.lock().unwrap().insert("active".into(), json!({"state":"running"}));
        drop(state);
        for n in 0..MAX_SESSIONS + 3 {
            let p = f.project(&format!("p{n}")); let id = service.store().unwrap().add(&p.root).unwrap().id;
            drop(service.session(&id).unwrap());
        }
        assert!(service.sessions.lock().unwrap().len() <= MAX_SESSIONS);
        assert_eq!(service.session(&id).unwrap().jobs.lock().unwrap()["active"]["state"], "running");
        assert!(service.invalidate(&id).is_err());
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
}
