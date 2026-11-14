//! Local catalog; catalog operations never write project content.
use crate::project_revision::{failure, is_link, open_plain, sha256_hex, SafeDirectory};
use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs::{File, Metadata};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: u32 = 1;
const MAX_SCAN_DIRS: usize = 10_000;
const MAX_SCAN_DEPTH: usize = 32;
const MAX_SCAN_PROJECTS: usize = 1_000;
const MAX_SOURCE_ENTRIES: usize = 20_000;
const MAX_SOURCE_DEPTH: usize = 32;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);
static NEXT_SCAN: AtomicU64 = AtomicU64::new(1);

#[derive(Args)]
pub struct LibraryArgs {
    #[command(subcommand)]
    pub command: LibraryCommand,
}
#[derive(Subcommand)]
pub enum LibraryCommand {
    /// 添加一个现有 mod 项目（不复制资源）
    Add { path: PathBuf },
    /// 仅在指定目录内有界扫描项目
    Scan { root: PathBuf },
    /// 列出本机目录库
    List,
    /// 移除目录记录（保留项目文件）
    Remove { id: String },
    /// 为已有记录更新项目位置并保留 ID
    Relocate { id: String, path: PathBuf },
    /// 修改目录库中的展示属性
    Update {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, value_enum)]
        status: Option<LibraryStatus>,
        #[arg(long, action = clap::ArgAction::Set)]
        favorite: Option<bool>,
        /// 替换标签集合；重复使用 --tag，--tag "" 清空
        #[arg(long = "tag")]
        tags: Option<Vec<String>>,
    },
}

pub fn cmd_library(cli: &crate::Cli, args: &LibraryArgs) -> Result<i32> {
    let home = crate::m4cmd::home_dir();
    let store = LibraryStore::open(&home)?;
    let report = match &args.command {
        LibraryCommand::Add { path } => json!({"ok": true, "entry": store.add(path)?}),
        LibraryCommand::Scan { root } => {
            let result = store.scan(root)?;
            json!({"ok": true, "entries": result.entries, "warnings": result.warnings, "visited_dirs": result.visited_dirs, "truncated": result.truncated})
        }
        LibraryCommand::Remove { id } => json!({"ok": true, "entry": store.remove(id)?}),
        LibraryCommand::Relocate { id, path } => {
            json!({"ok": true, "entry": store.relocate(id, path)?})
        }
        LibraryCommand::Update {
            id,
            name,
            status,
            favorite,
            tags,
        } => {
            json!({"ok": true, "entry": store.update(id, LibraryPatch { display_name: name.clone(), status: status.clone(), favorite: *favorite, tags: tags.clone() })?})
        }
        LibraryCommand::List => {
            let listing = store.list()?;
            let (game, _) = crate::game::locate(cli.game.as_deref());
            let metadata = crate::ship_catalog::ShipCatalog::load(&home, game.as_ref());
            let entries: Vec<_> = listing
                .entries
                .iter()
                .map(|entry| {
                    let mut v = serde_json::to_value(entry).expect("serializable library view");
                    v["ship_metadata"] = json!(metadata.lookup(
                        entry
                            .project
                            .as_ref()
                            .map(|p| p.ship.as_str())
                            .unwrap_or_default()
                    ));
                    v
                })
                .collect();
            json!({"ok": true, "entries": entries, "source": listing.source, "schema_version": SCHEMA_VERSION, "warnings": listing.warnings, "metadata_source": metadata.source, "metadata_warnings": metadata.source.warnings})
        }
    };
    crate::emit(cli, &report);
    if !cli.json {
        if let Some(entries) = report["entries"].as_array() {
            for entry in entries {
                println!(
                    "{}  {}  {}",
                    entry["id"].as_str().unwrap_or_default(),
                    entry["display_name"].as_str().unwrap_or_default(),
                    entry["path"].as_str().unwrap_or_default()
                );
            }
            println!("{} 个项目", entries.len());
        } else {
            println!(
                "{}  {}",
                report["entry"]["id"].as_str().unwrap_or_default(),
                report["entry"]["path"].as_str().unwrap_or_default()
            );
        }
        for key in ["warnings", "metadata_warnings"] {
            if let Some(warnings) = report[key].as_array() {
                for warning in warnings {
                    eprintln!("⚠ {}", warning.as_str().unwrap_or_default());
                }
            }
        }
    }
    Ok(0)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum LibraryStatus {
    Working,
    Review,
    Complete,
    Archived,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub id: String,
    pub path: PathBuf,
    pub display_name: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub status: LibraryStatus,
    pub added_unix: u64,
    pub opened_unix: Option<u64>,
}
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct LibraryPatch {
    pub display_name: Option<String>,
    pub tags: Option<Vec<String>>,
    pub favorite: Option<bool>,
    pub status: Option<LibraryStatus>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ProjectMetadata {
    pub name: String,
    pub version: String,
    pub ship: String,
    pub ship_key: String,
    pub game_build: String,
    pub mode: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct LibraryEntryView {
    #[serde(flatten)]
    pub entry: LibraryEntry,
    pub project: Option<ProjectMetadata>,
    pub modified_unix: Option<u64>,
    pub available: bool,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct LibraryListing {
    pub entries: Vec<LibraryEntryView>,
    pub source: PathBuf,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub entries: Vec<LibraryEntry>,
    pub warnings: Vec<String>,
    pub visited_dirs: usize,
    pub truncated: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanCandidateStatus {
    New,
    Registered,
    Invalid,
}
#[derive(Debug)]
struct ScanRoot {
    path: PathBuf,
    identity: (u64, u64),
}
#[derive(Clone, Debug)]
struct ScanSource {
    directory_identity: (u64, u64),
    manifest_sha256: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct ScanCandidate {
    pub candidate_id: String,
    pub path: PathBuf,
    pub project: Option<ProjectMetadata>,
    pub status: ScanCandidateStatus,
    pub existing_id: Option<String>,
    pub warning: Option<String>,
    #[serde(skip)]
    root: Arc<ScanRoot>,
    #[serde(skip)]
    source: Option<ScanSource>,
}
impl ScanCandidate {
    // Retain just the identity and proof in the session cache, not display fields.
    pub(crate) fn import_reference(&self) -> Option<Self> {
        Some(Self {
            candidate_id: self.candidate_id.clone(),
            path: self.path.clone(),
            project: None,
            status: self.status,
            existing_id: self.existing_id.clone(),
            warning: None,
            root: self.root.clone(),
            source: Some(self.source.as_ref()?.clone()),
        })
    }
}
#[derive(Debug, Serialize)]
pub struct ScanPreview {
    pub root: PathBuf,
    pub candidates: Vec<ScanCandidate>,
    pub warnings: Vec<String>,
    pub visited_dirs: usize,
    pub truncated: bool,
}
#[derive(Debug, Default, Serialize)]
pub struct ImportResult {
    pub entries: Vec<LibraryEntry>,
    pub already_registered: Vec<LibraryEntry>,
    pub warnings: Vec<String>,
}
#[derive(Serialize, Deserialize)]
struct CatalogDocument {
    schema_version: u32,
    entries: Vec<LibraryEntry>,
}
pub struct LibraryStore {
    home: PathBuf,
}
impl LibraryStore {
    pub fn open(home: &Path) -> Result<Self> {
        let store = Self {
            home: home.to_path_buf(),
        };
        store.read()?;
        Ok(store)
    }
    fn path(&self) -> PathBuf {
        self.home.join("library.json")
    }
    fn read(&self) -> Result<CatalogDocument> {
        let path = self.path();
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(CatalogDocument {
                    schema_version: SCHEMA_VERSION,
                    entries: vec![],
                })
            }
            Err(e) => return Err(e).with_context(|| format!("读取目录库 {}", path.display())),
        };
        let doc: CatalogDocument = serde_json::from_slice(&bytes)
            .with_context(|| format!("目录库损坏，原件保留: {}", path.display()))?;
        if doc.schema_version != SCHEMA_VERSION {
            bail!(
                "不支持目录库 schema_version {}（支持 1），原件保留: {}",
                doc.schema_version,
                path.display()
            );
        }
        let (mut ids, mut paths) = (HashSet::new(), HashSet::new());
        for e in &doc.entries {
            if e.id.is_empty()
                || !e.path.is_absolute()
                || !ids.insert(e.id.clone())
                || !paths.insert(path_identity(&e.path))
            {
                bail!("目录库含无效/重复 ID 或路径，原件保留: {}", path.display());
            }
        }
        Ok(doc)
    }
    fn mutate<T>(&self, update: impl FnOnce(&mut CatalogDocument) -> Result<T>) -> Result<T> {
        let _lock = CatalogLock::take(&self.home)?;
        let mut doc = self.read()?;
        let result = update(&mut doc)?;
        atomic_write(&self.path(), &serde_json::to_vec_pretty(&doc)?)?;
        Ok(result)
    }
    pub fn list(&self) -> Result<LibraryListing> {
        let doc = self.read()?;
        let mut warnings = vec![];
        let entries = doc
            .entries
            .into_iter()
            .map(|entry| {
                let mut entry_warnings = vec![];
                let (project, modified_unix) = match listing_project_metadata(&entry.path) {
                    Ok((project, modified_unix)) => {
                        let modified_unix = modified_unix
                            .map_err(|e| {
                                entry_warnings.push(format!(
                                    "{}: 无法确定最近修改时间: {e:#}",
                                    entry.path.display()
                                ));
                            })
                            .ok();
                        (Some(project), modified_unix)
                    }
                    Err(e) => {
                        entry_warnings.push(format!("{}: {e:#}", entry.path.display()));
                        (None, None)
                    }
                };
                warnings.extend(entry_warnings.clone());
                LibraryEntryView {
                    available: project.is_some(),
                    project,
                    modified_unix,
                    entry,
                    warnings: entry_warnings,
                }
            })
            .collect();
        Ok(LibraryListing {
            entries,
            source: self.path(),
            warnings,
        })
    }
    pub fn add(&self, path: &Path) -> Result<LibraryEntry> {
        let path = canonical_project(path)?;
        let project = project_metadata(&path)?;
        self.mutate(|doc| Ok(add_entry(doc, path, &project)))
    }
    pub fn scan(&self, root: &Path) -> Result<ScanResult> {
        let preview = self.scan_preview(root)?;
        let candidates: Vec<_> = preview
            .candidates
            .into_iter()
            .filter(|candidate| candidate.status != ScanCandidateStatus::Invalid)
            .collect();
        let imported = self.import_candidates(&candidates)?;
        let mut entries = imported.entries;
        entries.extend(imported.already_registered);
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let mut warnings = preview.warnings;
        warnings.extend(imported.warnings);
        Ok(ScanResult {
            entries,
            warnings,
            visited_dirs: preview.visited_dirs,
            truncated: preview.truncated,
        })
    }
    pub fn scan_preview(&self, root: &Path) -> Result<ScanPreview> {
        // Check the spelling supplied by the client before canonicalizing, so
        // a root or ancestor junction cannot silently become a regular path.
        let supplied = SafeDirectory::open(root)
            .with_context(|| format!("扫描目录不可访问: {}", root.display()))?;
        let root = std::fs::canonicalize(&supplied.path)?;
        if root.parent().is_none() {
            bail!(
                "扫描需要指定项目父目录，不能是文件或整盘根目录: {}",
                root.display()
            );
        }
        let root_guard = SafeDirectory::open(&root)?;
        let root_identity = root_guard.identity()?;
        if supplied.identity()? != root_identity {
            return Err(scan_changed(&root, "扫描根目录已变化"));
        }
        let catalog = self.read()?;
        let registered: HashMap<_, _> = catalog
            .entries
            .iter()
            .map(|entry| (path_identity(&entry.path), &entry.id))
            .collect();
        let scan_root = Arc::new(ScanRoot {
            path: root.clone(),
            identity: root_identity,
        });
        let generation = NEXT_SCAN.fetch_add(1, Ordering::Relaxed);
        let mut result = ScanPreview {
            root: root.clone(),
            candidates: vec![],
            warnings: vec![],
            visited_dirs: 0,
            truncated: false,
        };
        let mut stack = vec![(root.clone(), 0)];
        let mut visited = HashSet::new();
        while let Some((dir, depth)) = stack.pop() {
            if result.visited_dirs >= MAX_SCAN_DIRS || result.candidates.len() >= MAX_SCAN_PROJECTS
            {
                result.truncated = true;
                break;
            }
            let (guard, canonical) = match open_scan_directory(&root, &dir) {
                Ok(opened) => opened,
                Err(e) => {
                    result.warnings.push(format!("{}: {e:#}", dir.display()));
                    continue;
                }
            };
            let identity = path_identity(&canonical);
            if !visited.insert(identity.clone()) {
                continue;
            }
            result.visited_dirs += 1;
            match std::fs::symlink_metadata(guard.path.join("mod.toml")) {
                Ok(_) => {
                    let existing_id = registered.get(&identity).map(|id| (*id).clone());
                    let manifest = open_scan_manifest(&guard)
                        .and_then(|manifest| Ok((guard.identity()?, manifest)));
                    let (status, project, source, warning) = match manifest {
                        Ok((directory_identity, manifest)) => (
                            if existing_id.is_some() {
                                ScanCandidateStatus::Registered
                            } else {
                                ScanCandidateStatus::New
                            },
                            Some(manifest.project),
                            Some(ScanSource {
                                directory_identity,
                                manifest_sha256: manifest.sha256,
                            }),
                            None,
                        ),
                        Err(e) => {
                            let warning = format!("{}: {e:#}", canonical.display());
                            result.warnings.push(warning.clone());
                            (ScanCandidateStatus::Invalid, None, None, Some(warning))
                        }
                    };
                    result.candidates.push(ScanCandidate {
                        candidate_id: format!(
                            "candidate-{generation:x}-{:x}",
                            result.candidates.len()
                        ),
                        path: canonical,
                        project,
                        status,
                        existing_id,
                        warning,
                        root: scan_root.clone(),
                        source,
                    });
                    continue;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => {
                    result
                        .warnings
                        .push(format!("{}: {e}", canonical.display()));
                    continue;
                }
            }
            let children = match std::fs::read_dir(&guard.path) {
                Ok(rd) => rd,
                Err(e) => {
                    result
                        .warnings
                        .push(format!("{}: {e}", canonical.display()));
                    continue;
                }
            };
            for child in children {
                let child = match child {
                    Ok(child) => child,
                    Err(e) => {
                        result
                            .warnings
                            .push(format!("{}: {e}", canonical.display()));
                        continue;
                    }
                };
                let name = child.file_name().to_string_lossy().to_ascii_lowercase();
                if matches!(
                    name.as_str(),
                    "assets" | "out" | "target" | "node_modules" | ".git" | ".wowsmod" | ".install"
                ) {
                    continue;
                }
                let meta = match std::fs::symlink_metadata(child.path()) {
                    Ok(meta) => meta,
                    Err(e) => {
                        result
                            .warnings
                            .push(format!("{}: {e}", child.path().display()));
                        continue;
                    }
                };
                if is_link(&meta) {
                    result
                        .warnings
                        .push(format!("{}: 跳过链接或重解析点", child.path().display()));
                    continue;
                }
                if !meta.is_dir() {
                    continue;
                }
                if depth >= MAX_SCAN_DEPTH {
                    result.truncated = true;
                    continue;
                }
                stack.push((child.path(), depth + 1));
            }
        }
        if result.truncated {
            result.warnings.push(format!("扫描达到边界（最多 {MAX_SCAN_DIRS} 个目录、{MAX_SCAN_DEPTH} 层、{MAX_SCAN_PROJECTS} 个项目），请缩小 root 后重试"));
        }
        result.candidates.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(result)
    }
    pub fn import_candidates(&self, candidates: &[ScanCandidate]) -> Result<ImportResult> {
        let mut result = ImportResult::default();
        let Some(first) = candidates.first() else {
            return Ok(result);
        };
        let _lock = CatalogLock::take(&self.home)?;
        let mut doc = self.read()?;
        let root = &first.root;
        let (root_guard, _) =
            open_scan_directory(&root.path, &root.path).map_err(|e| scan_changed(&root.path, e))?;
        if root_guard
            .identity()
            .map_err(|e| scan_changed(&root.path, e))?
            != root.identity
        {
            return Err(scan_changed(&root.path, "扫描根目录已变化"));
        }
        let mut selected = Vec::with_capacity(candidates.len());
        let mut seen = HashSet::new();
        for candidate in candidates {
            let source = candidate
                .source
                .as_ref()
                .filter(|_| Arc::ptr_eq(root, &candidate.root))
                .ok_or_else(|| scan_changed(&candidate.path, "候选未在本次扫描中有效预览"))?;
            if !seen.insert(&candidate.candidate_id) {
                continue;
            }
            let (guard, path, manifest) = validate_scan_candidate(root, candidate, source)?;
            // The Windows handles stay open through the catalog commit and
            // deny directory replacement and manifest writes after validation.
            selected.push((candidate, guard, path, manifest));
        }

        for (candidate, _, path, manifest) in &selected {
            let existing = doc
                .entries
                .iter()
                .find(|entry| path_identity(&entry.path) == path_identity(path));
            if let Some(id) = &candidate.existing_id {
                if existing.map_or(true, |entry| &entry.id != id) {
                    return Err(scan_changed(path, "预览时的目录库记录已变化"));
                }
            }
            if let Some(entry) = existing {
                result.already_registered.push(entry.clone());
            } else {
                result
                    .entries
                    .push(add_entry(&mut doc, path.clone(), &manifest.project));
            }
        }
        if !result.entries.is_empty() {
            let bytes = serde_json::to_vec_pretty(&doc)?;
            // Unix permits renames despite open handles; check the batch again
            // immediately before publication rather than relying on handles alone.
            #[cfg(unix)]
            for (candidate, _, _, _) in &selected {
                let source = candidate.source.as_ref().expect("already validated");
                validate_scan_candidate(root, candidate, source)?;
            }
            atomic_write(&self.path(), &bytes)?;
        }
        Ok(result)
    }
    pub fn update(&self, id: &str, patch: LibraryPatch) -> Result<LibraryEntry> {
        if patch
            .display_name
            .as_ref()
            .is_some_and(|name| name.trim().is_empty())
        {
            bail!("展示名称不能为空");
        }
        self.mutate(|doc| {
            let entry = find_entry(doc, id)?;
            if let Some(name) = patch.display_name {
                entry.display_name = name.trim().to_string();
            }
            if let Some(tags) = patch.tags {
                let mut seen = HashSet::new();
                entry.tags = tags
                    .into_iter()
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty() && seen.insert(t.clone()))
                    .collect();
            }
            if let Some(favorite) = patch.favorite {
                entry.favorite = favorite;
            }
            if let Some(status) = patch.status {
                entry.status = status;
            }
            Ok(entry.clone())
        })
    }
    pub fn relocate(&self, id: &str, path: &Path) -> Result<LibraryEntry> {
        let path = canonical_project(path)?;
        self.mutate(|doc| {
            if doc
                .entries
                .iter()
                .any(|e| e.id != id && path_identity(&e.path) == path_identity(&path))
            {
                bail!("目标路径已属于其他项目记录: {}", path.display());
            }
            let entry = find_entry(doc, id)?;
            entry.path = path;
            Ok(entry.clone())
        })
    }
    pub fn remove(&self, id: &str) -> Result<LibraryEntry> {
        self.mutate(|doc| {
            let index = doc
                .entries
                .iter()
                .position(|e| e.id == id)
                .ok_or_else(|| anyhow!("目录库中没有项目 {id}"))?;
            Ok(doc.entries.remove(index))
        })
    }
    /// Look up the catalog record without touching its source directory.
    pub fn entries(&self) -> Result<Vec<LibraryEntry>> { Ok(self.read()?.entries) }

    /// Look up one record without scanning other projects.
    pub fn entry(&self, id: &str) -> Result<LibraryEntry> {
        self.read()?
            .entries
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow!("目录库中没有项目 {id}"))
    }
    pub fn resolve(&self, id: &str) -> Result<PathBuf> {
        canonical_project(&self.entry(id)?.path)
    }
    pub fn touch_opened(&self, id: &str) -> Result<LibraryEntry> {
        self.mutate(|doc| {
            let entry = find_entry(doc, id)?;
            entry.opened_unix = Some(now());
            Ok(entry.clone())
        })
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
fn path_identity(path: &Path) -> String {
    let text = path.to_string_lossy();
    #[cfg(windows)]
    {
        text.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        text.into_owned()
    }
}
fn canonical_project(path: &Path) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("项目目录不可访问: {}", path.display()))?;
    if !canonical.is_dir() {
        bail!("不是项目目录: {}", path.display());
    }
    project_metadata(&canonical)?;
    Ok(canonical)
}
fn project_metadata(path: &Path) -> Result<ProjectMetadata> {
    Ok(project_metadata_from_config(read_project_config(path)?))
}

fn open_scan_directory(root: &Path, path: &Path) -> Result<(SafeDirectory, PathBuf)> {
    let guard = SafeDirectory::open(path)?;
    let canonical = std::fs::canonicalize(&guard.path)?;
    if !canonical.starts_with(root) || path_identity(&canonical) != path_identity(path) {
        bail!("项目位置已变化或不在扫描目录内: {}", path.display());
    }
    Ok((guard, canonical))
}

fn validate_scan_candidate(
    root: &ScanRoot,
    candidate: &ScanCandidate,
    source: &ScanSource,
) -> Result<(SafeDirectory, PathBuf, ScanManifest)> {
    let path = &candidate.path;
    let validate = || -> Result<_> {
        let (guard, canonical) = open_scan_directory(&root.path, path)?;
        if guard.identity()? != source.directory_identity {
            bail!("项目目录已被替换");
        }
        let manifest = open_scan_manifest(&guard)?;
        if manifest.sha256 != source.manifest_sha256 {
            bail!("mod.toml 内容已变化");
        }
        Ok((guard, canonical, manifest))
    };
    validate().map_err(|error| scan_changed(path, format_args!("{error:#}")))
}

fn scan_changed(path: &Path, error: impl std::fmt::Display) -> anyhow::Error {
    failure(
        "scan_changed",
        format!("扫描结果已变化，请重新预览: {}: {error}", path.display()),
    )
}

struct ScanManifest {
    project: ProjectMetadata,
    sha256: String,
    _file: File,
}
fn open_scan_manifest(root: &SafeDirectory) -> Result<ScanManifest> {
    let (file, _, text) = read_manifest_text(root)?;
    Ok(ScanManifest {
        project: project_metadata_from_config(parse_project_manifest(&text)?),
        sha256: sha256_hex(text.as_bytes()),
        _file: file,
    })
}

fn project_metadata_from_config(cfg: crate::modproj::ModToml) -> ProjectMetadata {
    ProjectMetadata {
        ship_key: crate::ship_catalog::canonical_ship_key(&cfg.meta.ship),
        name: cfg.meta.name,
        version: cfg.meta.version,
        ship: cfg.meta.ship,
        game_build: cfg.meta.game_build,
        mode: cfg.install.mode,
    }
}

pub fn read_project_config(root: &Path) -> Result<crate::modproj::ModToml> {
    let root = SafeDirectory::open(root)?;
    Ok(read_project_manifest(&root)?.0)
}

fn read_project_manifest(root: &SafeDirectory) -> Result<(crate::modproj::ModToml, Metadata)> {
    let (_file, metadata, text) = read_manifest_text(root)?;
    Ok((parse_project_manifest(&text)?, metadata))
}

fn read_manifest_text(root: &SafeDirectory) -> Result<(File, Metadata, String)> {
    let path = root.path.join("mod.toml");
    let mut file = open_plain(&path, false)
        .with_context(|| format!("缺少或无法读取 mod.toml: {}", path.display()))?;
    let metadata = file.metadata()?;
    let mut text = String::new();
    (&mut file)
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_string(&mut text)
        .with_context(|| format!("读取 mod.toml {}", path.display()))?;
    if text.len() as u64 > MAX_MANIFEST_BYTES {
        bail!(
            "mod.toml 超过 {MAX_MANIFEST_BYTES} 字节: {}",
            path.display()
        );
    }
    if metadata.len() != text.len() as u64 || metadata.modified()? != file.metadata()?.modified()? {
        bail!("读取期间 mod.toml 已变化: {}", path.display());
    }
    Ok((file, metadata, text))
}

fn parse_project_manifest(text: &str) -> Result<crate::modproj::ModToml> {
    let cfg: crate::modproj::ModToml = toml::from_str(text).context("解析 mod.toml 失败")?;
    if cfg.meta.name.trim().is_empty() {
        bail!("mod.toml 的 name 为空");
    }
    Ok(cfg)
}

fn listing_project_metadata(path: &Path) -> Result<(ProjectMetadata, Result<u64>)> {
    // The registered directory may have been replaced after registration.
    let root = SafeDirectory::open(path)?;
    let (cfg, manifest) = read_project_manifest(&root)?;
    Ok((
        project_metadata_from_config(cfg),
        project_modified_unix(&root, &manifest),
    ))
}

fn project_modified_unix(root: &SafeDirectory, manifest: &Metadata) -> Result<u64> {
    let mut newest = source_mtime(&root.path.join("mod.toml"), manifest)?;
    let fx = root.path.join(crate::fxcmd::FX_FILE);
    match std::fs::symlink_metadata(&fx) {
        Ok(metadata) => {
            check_source_file(&fx, &metadata)?;
            newest = newest.max(source_mtime(&fx, &metadata)?);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("读取制作配置属性 {}", fx.display())),
    }

    visit_source_assets(&root.path, |path, metadata| {
        // Directory mtimes account for renamed and deleted assets.
        newest = newest.max(source_mtime(path, metadata)?);
        Ok(())
    })?;
    Ok(newest.duration_since(UNIX_EPOCH)?.as_secs())
}

/// Enumerate ordinary files in the same bounded resource tree used for source mtimes.
/// Callers reopening these paths must independently use a guarded, no-reparse open.
pub fn source_asset_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    visit_source_assets(root, |path, metadata| {
        if metadata.is_file() {
            files.push(path.to_owned());
        }
        Ok(())
    })?;
    files.sort();
    Ok(files)
}

fn visit_source_assets(
    root: &Path,
    mut visitor: impl FnMut(&Path, &Metadata) -> Result<()>,
) -> Result<()> {
    let assets = SafeDirectory::open(&root.join(crate::modproj::ASSETS))?;
    let metadata = std::fs::symlink_metadata(&assets.path)
        .with_context(|| format!("读取资源目录属性 {}", assets.path.display()))?;
    let mut remaining = MAX_SOURCE_ENTRIES;
    walk_assets(&assets.path, metadata, 0, &mut remaining, &mut visitor)
}

fn walk_assets(
    path: &Path,
    metadata: Metadata,
    depth: usize,
    remaining: &mut usize,
    visitor: &mut impl FnMut(&Path, &Metadata) -> Result<()>,
) -> Result<()> {
    if depth > MAX_SOURCE_DEPTH || *remaining == 0 {
        bail!(
            "资源扫描超过 {MAX_SOURCE_DEPTH} 层或 {MAX_SOURCE_ENTRIES} 项: {}",
            path.display()
        );
    }
    *remaining -= 1;
    if is_link(&metadata) || !(metadata.is_dir() || metadata.is_file()) {
        bail!("资源须为普通目录或文件: {}", path.display());
    }
    if metadata.is_file() {
        return visitor(path, &metadata);
    }

    // Hold each checked directory open until its subtree has been visited.
    let dir = open_plain(path, true)?;
    let before = dir.metadata()?;
    visitor(path, &before)?;
    let children =
        std::fs::read_dir(path).with_context(|| format!("读取资源目录 {}", path.display()))?;
    for child in children {
        let child = child.with_context(|| format!("读取资源目录项 {}", path.display()))?;
        let child_path = child.path();
        let child_metadata = child
            .metadata()
            .with_context(|| format!("读取资源属性 {}", child_path.display()))?;
        walk_assets(&child_path, child_metadata, depth + 1, remaining, visitor)?;
    }
    if before.modified()? != dir.metadata()?.modified()? {
        bail!("扫描期间资源目录已变化: {}", path.display());
    }
    Ok(())
}

fn check_source_file(path: &Path, metadata: &std::fs::Metadata) -> Result<()> {
    if is_link(metadata) || !metadata.is_file() {
        bail!("制作源须为普通文件: {}", path.display());
    }
    Ok(())
}

fn source_mtime(path: &Path, metadata: &std::fs::Metadata) -> Result<SystemTime> {
    metadata
        .modified()
        .with_context(|| format!("读取文件系统修改时间 {}", path.display()))
}

fn add_entry(doc: &mut CatalogDocument, path: PathBuf, project: &ProjectMetadata) -> LibraryEntry {
    if let Some(entry) = doc
        .entries
        .iter()
        .find(|e| path_identity(&e.path) == path_identity(&path))
    {
        return entry.clone();
    }
    let identity = path_identity(&path);
    let mut id = format!(
        "project-{}",
        &crate::modproj::sha256_hex(identity.as_bytes())[..24]
    );
    // A previous record may retain this ID after relocation; do not reuse it.
    let base = id.clone();
    let mut suffix = 1;
    while doc.entries.iter().any(|e| e.id == id) {
        id = format!("{base}-{suffix}");
        suffix += 1;
    }
    let entry = LibraryEntry {
        id,
        path,
        display_name: project.name.clone(),
        tags: vec![],
        favorite: false,
        status: LibraryStatus::Working,
        added_unix: now(),
        opened_unix: None,
    };
    doc.entries.push(entry.clone());
    entry
}
fn find_entry<'a>(doc: &'a mut CatalogDocument, id: &str) -> Result<&'a mut LibraryEntry> {
    doc.entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or_else(|| anyhow!("目录库中没有项目 {id}"))
}
struct CatalogLock {
    path: PathBuf,
    file: Option<std::fs::File>,
}
impl CatalogLock {
    fn take(home: &Path) -> Result<Self> {
        std::fs::create_dir_all(home)?;
        let path = home.join("library.lock");
        let file = crate::project_revision::retry_io(
            std::time::Duration::from_secs(3),
            crate::project_revision::lock_contention,
            || std::fs::OpenOptions::new().write(true).create_new(true).open(&path),
        ).with_context(|| format!(
            "目录库被锁定或不可写: {}；若进程已退出，请检查并手动移除残留锁",
            path.display()
        ))?;
        // Own cleanup before writing so a full disk cannot strand our lock.
        let mut lock = Self { path, file: Some(file) };
        writeln!(lock.file.as_mut().unwrap(), "pid={}", std::process::id())?;
        Ok(lock)
    }
}
impl Drop for CatalogLock {
    fn drop(&mut self) {
        self.file.take();
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Same-directory, flushed replacement. Windows never removes the old catalog first.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic_write_with_timeout(path, bytes, std::time::Duration::from_secs(3))
}
fn atomic_write_with_timeout(path: &Path, bytes: &[u8], _timeout: std::time::Duration) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("文件没有父目录: {}", path.display()))?;
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.{}-{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            #[link(name = "kernel32")]
            extern "system" {
                fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
            }
            let src: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
            let dst: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            crate::project_revision::retry_io(
                _timeout,
                crate::project_revision::windows_sharing_contention,
                || {
                    if unsafe { MoveFileExW(src.as_ptr(), dst.as_ptr(), 0x1 | 0x8) } == 0 {
                        Err(std::io::Error::last_os_error())
                    } else { Ok(()) }
                },
            ).context("原子替换文件失败，旧文件保留")?;
        }
        #[cfg(not(windows))]
        {
            std::fs::rename(&tmp, path)?;
            std::fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(tmp);
    }
    result.with_context(|| format!("写入 {}", path.display()))
}

#[cfg(test)]
mod persistence_retry_tests {
    use super::*;
    use crate::project_revision::tests::Fixture;

    #[cfg(windows)]
    #[test]
    fn atomic_publish_deadline_keeps_old_json_and_removes_own_temporary_file() {
        let directory = Fixture::new();
        let path = directory.0.join("verifications.json");
        let old = br#"{"records":[{"id":"old"}]}"#;
        let new = br#"{"records":[{"id":"old"},{"id":"new"}]}"#;
        std::fs::write(&path, old).unwrap();
        let _reader = crate::project_revision::open_plain(&path, false).unwrap();
        let error = atomic_write_with_timeout(&path, new, std::time::Duration::from_millis(60)).unwrap_err();
        let original = error.downcast_ref::<std::io::Error>().unwrap();
        assert!(matches!(original.raw_os_error(), Some(5 | 32)));
        assert!(format!("{error:#}").contains("verifications.json"));
        let retained = std::fs::read(&path).unwrap();
        assert_eq!(retained, old);
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&retained).unwrap()["records"].as_array().unwrap().len(), 1);
        let entries: Vec<_> = std::fs::read_dir(&directory.0).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(entries, [std::ffi::OsString::from("verifications.json")]);
    }

    #[test]
    fn catalog_lock_real_create_delete_contention_keeps_every_operation() {
        let directory = Fixture::new();
        let active = std::sync::atomic::AtomicUsize::new(0);
        let completed = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..6 {
                let directory = &directory.0; let active = &active; let completed = &completed;
                scope.spawn(move || {
                    for _ in 0..100 {
                        let lock = CatalogLock::take(directory).unwrap();
                        assert_eq!(active.fetch_add(1, Ordering::SeqCst), 0);
                        std::thread::yield_now();
                        assert_eq!(active.fetch_sub(1, Ordering::SeqCst), 1);
                        drop(lock);
                        completed.fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
        });
        assert_eq!(completed.load(Ordering::SeqCst), 600);
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "wowsmod-library-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn project(&self, rel: &str, name: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.join("assets/res_mods")).unwrap();
            std::fs::write(
                p.join("mod.toml"),
                format!("[mod]\nname = {name:?}\nship = \"FSB510_Bourgogne\"\n"),
            )
            .unwrap();
            std::fs::write(p.join("assets/res_mods/test.dds"), b"project asset").unwrap();
            p
        }
        fn store(&self) -> LibraryStore {
            LibraryStore::open(&self.0.join("home")).unwrap()
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn tree(root: &Path) -> BTreeMap<PathBuf, String> {
        let mut out = BTreeMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(p) = stack.pop() {
            for item in std::fs::read_dir(p).unwrap() {
                let p = item.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.insert(
                        p.strip_prefix(root).unwrap().to_path_buf(),
                        crate::modproj::sha256_hex(&std::fs::read(p).unwrap()),
                    );
                }
            }
        }
        out
    }
    fn set_test_mtime(path: &Path, seconds: u64) {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // FILE_WRITE_ATTRIBUTES and FILE_FLAG_BACKUP_SEMANTICS allow directory mtimes.
            options.access_mode(0x100).custom_flags(0x02000000);
        }
        options
            .open(path)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds))
            .unwrap();
    }
    #[test]
    fn library_modification_time_moves_when_only_an_asset_changes() {
        let t = Temp::new();
        let a = t.project("a", "a");
        let b = t.project("b", "b");
        let s = t.store();
        s.add(&a).unwrap();
        s.add(&b).unwrap();
        let time = now() + 60;
        set_test_mtime(&a.join("assets/res_mods/test.dds"), time);
        set_test_mtime(&b.join("assets/res_mods/test.dds"), time + 1);
        let first = json!(s.list().unwrap().entries);
        assert_eq!(first[0]["modified_unix"], time);
        assert_eq!(first[1]["modified_unix"], time + 1);

        set_test_mtime(&a.join("assets/res_mods/test.dds"), time + 2);
        let edited = json!(s.list().unwrap().entries);
        assert_eq!(edited[0]["modified_unix"], time + 2);
        assert_eq!(edited[1]["modified_unix"], time + 1);
    }
    #[test]
    fn library_modification_time_ignores_catalog_and_generated_outputs() {
        let t = Temp::new();
        let a = t.project("a", "a");
        let s = t.store();
        let entry = s.add(&a).unwrap();
        let time = now() - 100;
        for path in ["mod.toml", "assets/res_mods", "assets/res_mods/test.dds"] {
            set_test_mtime(&a.join(path), time);
        }
        assert_eq!(json!(s.list().unwrap().entries)[0]["modified_unix"], time);
        s.update(
            &entry.id,
            LibraryPatch {
                display_name: Some("New display label".into()),
                favorite: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        s.touch_opened(&entry.id).unwrap();
        for path in [".wowsmod/cache", ".render", "renders", ".install"] {
            std::fs::create_dir_all(a.join(path)).unwrap();
            std::fs::write(a.join(path).join("output"), b"generated").unwrap();
        }
        assert_eq!(json!(s.list().unwrap().entries)[0]["modified_unix"], time);
        let catalog: serde_json::Value =
            serde_json::from_slice(&std::fs::read(s.path()).unwrap()).unwrap();
        assert!(!catalog["entries"][0]
            .as_object()
            .unwrap()
            .contains_key("modified_unix"));
    }
    #[test]
    fn library_modification_time_counts_source_config_and_asset_deletion() {
        let t = Temp::new();
        let a = t.project("a", "a");
        let s = t.store();
        s.add(&a).unwrap();
        let time = now() - 100;
        let assets = a.join("assets/res_mods");
        let nested = assets.join("nested");
        std::fs::create_dir(&nested).unwrap();
        let source = nested.join("material.mfm");
        std::fs::rename(assets.join("test.dds"), &source).unwrap();
        let fx = a.join("fx.json");
        std::fs::write(&fx, "{}").unwrap();
        for path in [a.join("mod.toml"), assets, nested.clone(), source.clone()] {
            set_test_mtime(&path, time);
        }
        set_test_mtime(&fx, time + 1);
        assert_eq!(
            json!(s.list().unwrap().entries)[0]["modified_unix"],
            time + 1
        );

        std::fs::remove_file(source).unwrap();
        let deletion_time = std::fs::metadata(nested)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(deletion_time > time + 1);
        assert_eq!(
            json!(s.list().unwrap().entries)[0]["modified_unix"],
            deletion_time
        );
    }
    #[test]
    fn library_modification_time_is_unknown_for_incomplete_sources() {
        let t = Temp::new();
        let missing_assets = t.project("missing-assets", "missing_assets");
        let deep = t.project("deep", "deep");
        let s = t.store();
        s.add(&missing_assets).unwrap();
        s.add(&deep).unwrap();
        std::fs::remove_dir_all(missing_assets.join("assets/res_mods")).unwrap();
        let mut nested = deep.join("assets/res_mods");
        for _ in 0..40 {
            nested.push("d");
        }
        std::fs::create_dir_all(nested).unwrap();
        let list = s.list().unwrap();
        assert_eq!(list.entries.len(), 2);
        for entry in list.entries {
            assert!(entry.available);
            assert!(!entry.warnings.is_empty());
            assert_eq!(
                json!(entry).get("modified_unix"),
                Some(&serde_json::Value::Null)
            );
        }
    }
    #[test]
    fn library_modification_time_is_unknown_for_linked_sources() {
        let t = Temp::new();
        let linked_parent = t.project("linked-parent", "parent");
        let linked_child = t.project("linked-child", "child");
        let outside = t.project("outside", "outside");
        let s = t.store();
        s.add(&linked_parent).unwrap();
        s.add(&linked_child).unwrap();
        // Link an ancestor of the resource root so checking the leaf alone is insufficient.
        let parent = linked_parent.join("assets");
        std::fs::remove_dir_all(&parent).unwrap();
        let target = outside.join("assets");
        let child = linked_child.join("assets").join("res_mods").join("linked");
        for link in [parent, child] {
            #[cfg(windows)]
            {
                let result = std::process::Command::new("cmd")
                    .args(["/C", "mklink", "/J"])
                    .arg(&link)
                    .arg(&target)
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &link).unwrap();
        }
        for entry in s.list().unwrap().entries {
            assert!(entry.available);
            assert!(!entry.warnings.is_empty());
            assert_eq!(
                json!(entry).get("modified_unix"),
                Some(&serde_json::Value::Null)
            );
        }
        assert!(source_asset_files(&linked_parent).is_err());
        assert!(source_asset_files(&linked_child).is_err());
    }
    #[test]
    fn source_asset_files_includes_only_regular_files_in_the_resource_tree() {
        let t = Temp::new();
        let root = t.project("project", "sample");
        let assets = root.join(crate::modproj::ASSETS);
        std::fs::create_dir(assets.join("nested")).unwrap();
        std::fs::write(assets.join("nested/a.mfm"), b"asset").unwrap();
        assert_eq!(
            source_asset_files(&root).unwrap(),
            [assets.join("nested").join("a.mfm"), assets.join("test.dds")]
        );
    }
    #[cfg(windows)]
    #[test]
    fn visiting_assets_pins_directories_against_replacement() {
        let t = Temp::new();
        let root = t.project("project", "sample");
        let assets = root.join(crate::modproj::ASSETS);
        let nested = assets.join("nested");
        let moved = assets.join("moved");
        std::fs::create_dir(&nested).unwrap();
        let mut checked = false;
        visit_source_assets(&root, |path, _| {
            if path.file_name() == nested.file_name() {
                checked = true;
                assert!(std::fs::rename(&nested, &moved).is_err());
            }
            Ok(())
        })
        .unwrap();
        assert!(checked);
        std::fs::rename(nested, moved).unwrap();
    }
    #[test]
    fn canonical_paths_dedupe_but_same_name_projects_stay_distinct() {
        let t = Temp::new();
        let a = t.project("a", "same");
        let b = t.project("b", "same");
        let s = t.store();
        let first = s.add(&a).unwrap();
        assert_eq!(s.add(&a.join(".")).unwrap().id, first.id);
        assert_ne!(s.add(&b).unwrap().id, first.id);
        assert_eq!(s.list().unwrap().entries.len(), 2);
    }
    #[cfg(windows)]
    #[test]
    fn windows_junction_and_case_variants_are_same_identity() {
        let t = Temp::new();
        let a = t.project("Project", "sample");
        let link = t.0.join("Alias");
        assert!(std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&a)
            .output()
            .unwrap()
            .status
            .success());
        let s = t.store();
        let id = s.add(&a).unwrap().id;
        assert_eq!(s.add(&link).unwrap().id, id);
        assert_eq!(
            s.add(&PathBuf::from(a.to_string_lossy().to_lowercase()))
                .unwrap()
                .id,
            id
        );
    }
    #[test]
    fn relocation_preserves_id_and_rejects_another_registered_path() {
        let t = Temp::new();
        let a = t.project("a", "sample");
        let b = t.project("b", "sample");
        let s = t.store();
        let entry = s.add(&a).unwrap();
        let moved = t.0.join("moved");
        std::fs::rename(&a, &moved).unwrap();
        let relocated = s.relocate(&entry.id, &moved).unwrap();
        assert_eq!(relocated.id, entry.id);
        assert_eq!(
            s.resolve(&entry.id).unwrap(),
            std::fs::canonicalize(&moved).unwrap()
        );
        s.add(&b).unwrap();
        assert!(s.relocate(&entry.id, &b).is_err());
    }
    #[test]
    fn metadata_archiving_opening_and_removal_never_modify_projects() {
        let t = Temp::new();
        let a = t.project("a", "sample");
        let before = tree(&a);
        let s = t.store();
        let e = s.add(&a).unwrap();
        let e = s
            .update(
                &e.id,
                LibraryPatch {
                    display_name: Some("Display label".into()),
                    tags: Some(vec!["paint".into()]),
                    favorite: Some(true),
                    status: Some(LibraryStatus::Archived),
                },
            )
            .unwrap();
        assert_eq!(e.display_name, "Display label");
        assert_eq!(e.tags, ["paint"]);
        assert!(e.favorite);
        assert_eq!(e.status, LibraryStatus::Archived);
        assert!(s.touch_opened(&e.id).unwrap().opened_unix.is_some());
        assert_eq!(s.list().unwrap().entries.len(), 1);
        s.remove(&e.id).unwrap();
        assert!(s.list().unwrap().entries.is_empty());
        assert_eq!(tree(&a), before);
    }
    #[test]
    fn corrupt_and_unknown_schema_catalogs_are_preserved() {
        let t = Temp::new();
        let home = t.0.join("home");
        std::fs::create_dir_all(&home).unwrap();
        for bad in ["{broken", "{\"schema_version\":2,\"entries\":[]}"] {
            std::fs::write(home.join("library.json"), bad).unwrap();
            assert!(LibraryStore::open(&home).is_err());
            assert_eq!(
                std::fs::read_to_string(home.join("library.json")).unwrap(),
                bad
            );
        }
    }
    #[test]
    fn stale_store_handles_reload_before_writes() {
        let t = Temp::new();
        let a = t.project("a", "one");
        let b = t.project("b", "two");
        let s1 = t.store();
        let s2 = t.store();
        let e = s1.add(&a).unwrap();
        s2.add(&b).unwrap();
        s1.update(
            &e.id,
            LibraryPatch {
                favorite: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let list = s2.list().unwrap();
        assert_eq!(list.entries.len(), 2);
        assert!(
            list.entries
                .iter()
                .find(|v| v.entry.id == e.id)
                .unwrap()
                .entry
                .favorite
        );
    }
    #[test]
    fn scan_prunes_assets_and_projects_and_keeps_good_candidates() {
        let t = Temp::new();
        let root = t.0.join("scan");
        t.project("scan/a", "a");
        t.project("scan/a/nested", "nested");
        t.project("scan/assets/hidden", "hidden");
        t.project("scan/target/hidden", "hidden");
        t.project("scan/z", "z");
        std::fs::create_dir_all(root.join("bad")).unwrap();
        std::fs::write(root.join("bad/mod.toml"), "not toml").unwrap();
        let s = t.store();
        let scan = s.scan(&root).unwrap();
        assert_eq!(scan.entries.len(), 2);
        assert!(!scan.warnings.is_empty());
        assert!(!scan.truncated);
        assert_eq!(s.list().unwrap().entries.len(), 2);
        assert!(s.scan(&root.join("missing")).is_err());
    }
    #[test]
    fn scan_preview_never_creates_or_rewrites_the_catalog() {
        let t = Temp::new();
        let a = t.project("scan/a", "first");
        t.project("scan/b", "second");
        let bad = t.project("scan/bad", "bad");
        std::fs::write(bad.join("mod.toml"), "not a manifest").unwrap();
        let s = t.store();
        let root = t.0.join("scan");

        let preview = s.scan_preview(&root).unwrap();
        assert!(!s.path().exists());
        assert!(!s.home.exists());
        assert_eq!(preview.candidates.len(), 3);
        assert_eq!(preview.candidates[0].status, ScanCandidateStatus::New);
        assert_eq!(preview.candidates[2].status, ScanCandidateStatus::Invalid);
        assert!(preview.candidates[2].warning.is_some());
        assert!(preview
            .warnings
            .iter()
            .any(|warning| warning.contains("bad")));

        let entry = s.add(&a).unwrap();
        let bytes = std::fs::read(s.path()).unwrap();
        let preview = s.scan_preview(&root).unwrap();
        assert_eq!(
            preview.candidates[0].status,
            ScanCandidateStatus::Registered
        );
        assert_eq!(
            preview.candidates[0].existing_id.as_deref(),
            Some(entry.id.as_str())
        );
        assert_eq!(std::fs::read(s.path()).unwrap(), bytes);
    }

    #[test]
    fn scan_import_registers_only_the_selected_project_and_reuses_its_id() {
        let t = Temp::new();
        t.project("scan/a", "first");
        let b = t.project("scan/b", "second");
        let s = t.store();
        let preview = s.scan_preview(&t.0.join("scan")).unwrap();
        let selected = &preview.candidates[1..];
        let result = s.import_candidates(selected).unwrap();
        assert_eq!(result.entries.len(), 1);
        let entry = &result.entries[0];
        assert_eq!(entry.path, std::fs::canonicalize(b).unwrap());
        assert_eq!(entry.opened_unix, None);
        assert_eq!(s.list().unwrap().entries.len(), 1);

        let bytes = std::fs::read(s.path()).unwrap();
        let repeated = s.import_candidates(selected).unwrap();
        assert!(repeated.entries.is_empty());
        assert_eq!(repeated.already_registered.len(), 1);
        assert_eq!(repeated.already_registered[0].id, entry.id);
        assert_eq!(std::fs::read(s.path()).unwrap(), bytes);
    }

    #[test]
    fn scan_import_validates_every_manifest_before_writing_any_entry() {
        let t = Temp::new();
        t.project("scan/a", "first");
        let b = t.project("scan/b", "second");
        let s = t.store();
        s.add(&t.project("untouched", "existing")).unwrap();
        let bytes = std::fs::read(s.path()).unwrap();
        let preview = s.scan_preview(&t.0.join("scan")).unwrap();
        let manifest = b.join("mod.toml");
        let mut changed = std::fs::read(&manifest).unwrap();
        changed.extend_from_slice(b"# same metadata, different manifest bytes\n");
        std::fs::write(manifest, changed).unwrap();

        let error = s.import_candidates(&preview.candidates).unwrap_err();
        assert_eq!(
            crate::project_revision::error_code(&error),
            Some("scan_changed")
        );
        assert_eq!(std::fs::read(s.path()).unwrap(), bytes);
    }

    #[test]
    fn scan_import_keeps_a_missing_same_name_record_and_reuses_a_concurrent_add() {
        let t = Temp::new();
        let missing = t.project("missing", "same");
        let a = t.project("scan/a", "same");
        t.project("scan/b", "same");
        let s = t.store();
        let old = s.add(&missing).unwrap();
        std::fs::rename(missing, t.0.join("moved")).unwrap();
        let preview = s.scan_preview(&t.0.join("scan")).unwrap();
        let concurrent = t.store().add(&a).unwrap();
        let result = s.import_candidates(&preview.candidates).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert_ne!(result.entries[0].id, old.id);
        assert_eq!(result.already_registered.len(), 1);
        assert_eq!(result.already_registered[0].id, concurrent.id);
        assert_eq!(s.entry(&old.id).unwrap().path, old.path);
        assert!(s.resolve(&old.id).is_err());
    }

    #[test]
    fn scan_import_rejects_a_project_moved_out_of_the_root() {
        let t = Temp::new();
        t.project("scan/a", "first");
        let b = t.project("scan/b", "second");
        let s = t.store();
        let preview = s.scan_preview(&t.0.join("scan")).unwrap();
        std::fs::rename(b, t.0.join("outside")).unwrap();

        let error = s.import_candidates(&preview.candidates).unwrap_err();
        assert_eq!(
            crate::project_revision::error_code(&error),
            Some("scan_changed")
        );
        assert!(!s.path().exists());
    }

    #[test]
    fn scan_import_rejects_a_replacement_directory_with_the_same_manifest_bytes() {
        let t = Temp::new();
        t.project("scan/a", "first");
        let b = t.project("scan/b", "second");
        let s = t.store();
        let preview = s.scan_preview(&t.0.join("scan")).unwrap();
        let outside = t.0.join("outside");
        std::fs::rename(&b, &outside).unwrap();
        std::fs::create_dir(&b).unwrap();
        std::fs::copy(outside.join("mod.toml"), b.join("mod.toml")).unwrap();

        let error = s.import_candidates(&preview.candidates).unwrap_err();
        assert_eq!(
            crate::project_revision::error_code(&error),
            Some("scan_changed")
        );
        assert!(!s.path().exists());
    }

    #[cfg(any(windows, unix))]
    #[test]
    fn scan_import_rejects_a_project_replaced_by_a_link_even_inside_the_root() {
        let t = Temp::new();
        t.project("scan/a", "first");
        t.project("scan/b", "second");
        let root = t.0.join("scan");
        let b = root.join("b");
        let s = t.store();
        let preview = s.scan_preview(&root).unwrap();
        let target = root.join("renamed");
        std::fs::rename(&b, &target).unwrap();
        #[cfg(windows)]
        {
            let output = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&b)
                .arg(&target)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &b).unwrap();

        let error = s.import_candidates(&preview.candidates).unwrap_err();
        assert_eq!(
            crate::project_revision::error_code(&error),
            Some("scan_changed")
        );
        assert!(!s.path().exists());
        assert!(s.scan_preview(&b).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn scan_does_not_follow_junction_cycles_or_escape_root() {
        let t = Temp::new();
        let root = t.0.join("scan");
        std::fs::create_dir_all(root.join("nested")).unwrap();
        t.project("outside", "outside");
        for (link, target) in [
            (root.join("nested").join("cycle"), root.clone()),
            (root.join("escape"), t.0.join("outside")),
        ] {
            let output = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let result = t.store().scan(&root).unwrap();
        assert!(result.entries.is_empty());
        assert!(result.visited_dirs <= 2);
    }
    #[test]
    fn missing_or_invalid_project_is_reported_without_losing_entry() {
        let t = Temp::new();
        let a = t.project("a", "sample");
        let s = t.store();
        let e = s.add(&a).unwrap();
        std::fs::write(a.join("mod.toml"), "bad").unwrap();
        let list = s.list().unwrap();
        assert_eq!(list.entries.len(), 1);
        assert!(!list.entries[0].available);
        assert!(!list.warnings.is_empty());
        assert_eq!(
            json!(list.entries[0]).get("modified_unix"),
            Some(&serde_json::Value::Null)
        );
        assert_eq!(s.entry(&e.id).unwrap().id, e.id);
        assert!(s.entry("missing-id").is_err());
        assert!(s.resolve(&e.id).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn scan_reports_inaccessible_manifest_and_preserves_accessible_projects() {
        use std::os::windows::fs::OpenOptionsExt;
        let t = Temp::new();
        t.project("scan/good", "good");
        let blocked = t.project("scan/blocked", "blocked");
        let _exclusive = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(blocked.join("mod.toml"))
            .unwrap();
        let result = t.store().scan(&t.0.join("scan")).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].display_name, "good");
        assert!(result.warnings.iter().any(|w| w.contains("blocked")));
    }
    #[test]
    fn concurrent_writers_keep_all_records() {
        let t = Temp::new();
        let projects: Vec<_> = (0..8)
            .map(|i| t.project(&format!("project-{i}"), "same"))
            .collect();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(projects.len()));
        std::thread::scope(|scope| {
            let mut threads = vec![];
            for project in projects {
                let home = t.0.join("home");
                let barrier = std::sync::Arc::clone(&barrier);
                threads.push(scope.spawn(move || {
                    let s = LibraryStore::open(&home).unwrap();
                    barrier.wait();
                    s.add(&project).unwrap();
                }));
            }
            for thread in threads {
                thread.join().unwrap();
            }
        });
        assert_eq!(t.store().list().unwrap().entries.len(), 8);
    }
    #[test]
    fn scan_depth_limit_is_visible_and_does_not_scan_past_it() {
        let t = Temp::new();
        let root = t.0.join("scan");
        let mut deep = root.clone();
        for _ in 0..34 {
            deep.push("d");
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("mod.toml"), "[mod]\nname = \"deep\"\n").unwrap();
        let scan = t.store().scan(&root).unwrap();
        assert!(scan.entries.is_empty());
        assert!(scan.truncated);
        assert!(!scan.warnings.is_empty());
        assert!(scan.visited_dirs <= 33);
    }
}
