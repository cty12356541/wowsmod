//! Local catalog; catalog operations never write project content.
use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const SCHEMA_VERSION: u32 = 1;
const MAX_SCAN_DIRS: usize = 10_000;
const MAX_SCAN_DEPTH: usize = 32;
const MAX_SCAN_PROJECTS: usize = 1_000;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

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
                let (project, entry_warnings) = match project_metadata(&entry.path) {
                    Ok(project) => (Some(project), vec![]),
                    Err(e) => (None, vec![format!("{}: {e:#}", entry.path.display())]),
                };
                warnings.extend(entry_warnings.clone());
                LibraryEntryView {
                    available: project.is_some(),
                    project,
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
        let root = std::fs::canonicalize(root)
            .with_context(|| format!("扫描目录不可访问: {}", root.display()))?;
        if !root.is_dir() || root.parent().is_none() {
            bail!(
                "扫描需要指定项目父目录，不能是文件或整盘根目录: {}",
                root.display()
            );
        }
        let mut result = ScanResult {
            entries: vec![],
            warnings: vec![],
            visited_dirs: 0,
            truncated: false,
        };
        let mut stack = vec![(root.clone(), 0)];
        let mut visited = HashSet::new();
        let mut projects = vec![];
        while let Some((dir, depth)) = stack.pop() {
            if result.visited_dirs >= MAX_SCAN_DIRS || projects.len() >= MAX_SCAN_PROJECTS {
                result.truncated = true;
                break;
            }
            let canonical = match std::fs::canonicalize(&dir) {
                Ok(path) => path,
                Err(e) => {
                    result.warnings.push(format!("{}: {e}", dir.display()));
                    continue;
                }
            };
            if !canonical.starts_with(&root) || !visited.insert(path_identity(&canonical)) {
                continue;
            }
            result.visited_dirs += 1;
            match std::fs::symlink_metadata(canonical.join("mod.toml")) {
                Ok(_) => {
                    match project_metadata(&canonical) {
                        Ok(project) => projects.push((canonical, project)),
                        Err(e) => result
                            .warnings
                            .push(format!("{}: {e:#}", canonical.display())),
                    }
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
            let children = match std::fs::read_dir(&canonical) {
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
                if is_link(&meta) || !meta.is_dir() {
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
        projects.sort_by(|a, b| a.0.cmp(&b.0));
        if !projects.is_empty() {
            result.entries = self.mutate(|doc| {
                Ok(projects
                    .into_iter()
                    .map(|(path, project)| add_entry(doc, path, &project))
                    .collect())
            })?;
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
    pub fn resolve(&self, id: &str) -> Result<PathBuf> {
        let doc = self.read()?;
        let entry = doc
            .entries
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow!("目录库中没有项目 {id}"))?;
        canonical_project(&entry.path)
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
    let text = std::fs::read_to_string(path.join("mod.toml"))
        .with_context(|| format!("缺少或无法读取 mod.toml: {}", path.display()))?;
    let cfg: crate::modproj::ModToml = toml::from_str(&text).context("解析 mod.toml 失败")?;
    if cfg.meta.name.trim().is_empty() {
        bail!("mod.toml 的 name 为空");
    }
    Ok(ProjectMetadata {
        ship_key: crate::ship_catalog::canonical_ship_key(&cfg.meta.ship),
        name: cfg.meta.name,
        version: cfg.meta.version,
        ship: cfg.meta.ship,
        game_build: cfg.meta.game_build,
        mode: cfg.install.mode,
    })
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
fn is_link(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_type().is_symlink() || meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}
struct CatalogLock {
    path: PathBuf,
    file: Option<std::fs::File>,
}
impl CatalogLock {
    fn take(home: &Path) -> Result<Self> {
        std::fs::create_dir_all(home)?;
        let path = home.join("library.lock");
        let start = std::time::Instant::now();
        loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    writeln!(file, "pid={}", std::process::id())?;
                    return Ok(Self {
                        path,
                        file: Some(file),
                    });
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::AlreadyExists
                        && start.elapsed().as_secs() < 3 =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(20))
                }
                Err(e) => {
                    return Err(e).with_context(|| {
                        format!(
                            "目录库被锁定或不可写: {}；若进程已退出，请检查并手动移除残留锁",
                            path.display()
                        )
                    })
                }
            }
        }
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
            if unsafe { MoveFileExW(src.as_ptr(), dst.as_ptr(), 0x1 | 0x8) } == 0 {
                return Err(std::io::Error::last_os_error())
                    .context("原子替换目录库失败，旧文件保留");
            }
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
