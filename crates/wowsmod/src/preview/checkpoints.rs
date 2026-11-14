//! Immutable source checkpoints and no-overwrite recovery, independent of live previews.
use crate::project_revision::{
    copy_verified_source_file, failure, is_revision, is_sha256, read_plain_file,
    read_source_manifest, safe_project_key, sha256_hex, validate_source_manifest,
    validate_source_path, SafeDirectory, SourceFile, SourceManifest,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_RECORD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CHECKPOINTS: usize = 10_000;
const MAX_PREVIEW_QUEUE: usize = 8;
const MAX_PNG_BYTES: u64 = 4 * 1024 * 1024;
const RECIPE: &str = "rev=head&compare=single&view=deck&w=640&h=360&quality=high&wm=0&look=game-day&env=studio&lod=0&t=0";
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCheckpoint {
    pub label: String,
    pub expected_revision: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreviewStatus {
    Pending,
    Ready,
    Failed,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointPreview {
    pub state: PreviewStatus,
    pub url: Option<String>,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Checkpoint {
    pub id: String,
    pub label: String,
    pub created_unix: u64,
    pub revision: String,
    pub scope_version: u32,
    pub file_count: usize,
    pub total_bytes: u64,
    pub project_build: String,
    pub target_build: Option<String>,
    pub preview: CheckpointPreview,
}
#[derive(Clone, Debug, Serialize)]
pub struct CheckpointDiff {
    pub ok: bool,
    pub checkpoint_revision: String,
    pub current_revision: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
    pub unchanged_count: usize,
}
#[derive(Clone, Debug)]
pub struct RestoredProject {
    pub path: PathBuf,
    pub checkpoint_id: String,
    pub revision: String,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPreview {
    state: PreviewStatus,
    hash: Option<String>,
    reason: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredCheckpoint {
    schema_version: u32,
    project_id: String,
    id: String,
    label: String,
    created_unix: u64,
    source_root: PathBuf,
    source: SourceManifest,
    project_build: String,
    target_build: Option<String>,
    renderer_version: String,
    recipe: String,
    preview: StoredPreview,
}
impl StoredCheckpoint {
    fn public(&self) -> Checkpoint {
        Checkpoint {
            id: self.id.clone(),
            label: self.label.clone(),
            created_unix: self.created_unix,
            revision: self.source.revision.clone(),
            scope_version: self.source.scope_version,
            file_count: self.source.files.len(),
            total_bytes: self.source.total_bytes,
            project_build: self.project_build.clone(),
            target_build: self.target_build.clone(),
            preview: CheckpointPreview {
                state: self.preview.state.clone(),
                reason: self.preview.reason.clone(),
                url: self.preview.hash.as_ref().map(|hash| {
                    format!(
                        "/api/v1/library/projects/{}/checkpoints/{}/preview/{hash}.png",
                        super::thumbnails::encode_id(&self.project_id),
                        self.id
                    )
                }),
            },
        }
    }
}
#[derive(Clone)]
struct PreviewJob {
    project_id: String,
    checkpoint_id: String,
    selected_game: Option<PathBuf>,
}
#[derive(Default)]
struct PreviewQueue {
    jobs: VecDeque<PreviewJob>,
    active: HashSet<(String, String)>,
    running: bool,
}
pub struct CheckpointStore {
    home: PathBuf,
    queue: Mutex<PreviewQueue>,
}
impl CheckpointStore {
    pub fn open(home: &Path) -> Result<Self> {
        let home = normalized_path(home)?;
        Ok(Self {
            home,
            queue: Mutex::new(PreviewQueue::default()),
        })
    }
    fn root(&self) -> PathBuf {
        self.home.join("checkpoints")
    }
    fn directory(&self, project_id: &str) -> PathBuf {
        self.root().join(safe_project_key(project_id))
    }
    fn record_path(&self, project_id: &str, checkpoint_id: &str) -> Result<PathBuf> {
        validate_identity(project_id, checkpoint_id)?;
        Ok(self
            .directory(project_id)
            .join(format!("{checkpoint_id}.json")))
    }
    fn read(&self, project_id: &str, checkpoint_id: &str) -> Result<StoredCheckpoint> {
        let path = self.record_path(project_id, checkpoint_id)?;
        let _guard = SafeDirectory::open(path.parent().unwrap())?;
        let bytes = read_plain_file(&path, MAX_RECORD_BYTES)?;
        let record: StoredCheckpoint = serde_json::from_slice(&bytes).map_err(|e| {
            failure(
                "corrupt_checkpoint",
                format!("Invalid checkpoint JSON; original retained: {e}"),
            )
        })?;
        validate_record(&record, project_id, checkpoint_id)?;
        Ok(record)
    }
    pub fn list(&self, project_id: &str) -> Result<Vec<Checkpoint>> {
        validate_project_id(project_id)?;
        let directory = self.directory(project_id);
        let _ = normalized_path(&directory)?;
        match fs::symlink_metadata(&directory) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e.into()),
            Ok(_) => (),
        }
        let _guard = SafeDirectory::open(&directory)?;
        let mut result = vec![];
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| failure("corrupt_checkpoint", "Non-UTF8 checkpoint entry"))?;
            if !name.ends_with(".json") {
                continue;
            }
            if result.len() >= MAX_CHECKPOINTS {
                return Err(failure(
                    "snapshot_limit",
                    "Too many checkpoints; archive history before continuing",
                ));
            }
            let id = name.strip_suffix(".json").unwrap();
            result.push(self.read(project_id, id)?.public());
        }
        result.sort_by(|a, b| {
            a.created_unix
                .cmp(&b.created_unix)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(result)
    }
    pub fn create(
        &self,
        project_id: &str,
        root: &Path,
        request: CreateCheckpoint,
    ) -> Result<Checkpoint> {
        self.create_with_context(project_id, root, request, None)
    }
    pub fn create_with_context(
        &self,
        project_id: &str,
        root: &Path,
        request: CreateCheckpoint,
        target_build: Option<&str>,
    ) -> Result<Checkpoint> {
        validate_project_id(project_id)?;
        validate_label(&request.label)?;
        if !is_revision(&request.expected_revision) {
            return Err(failure(
                "invalid_request",
                "Expected a complete source revision",
            ));
        }
        if target_build
            .is_some_and(|s| s.len() > 128 || s.is_empty() || s.chars().any(char::is_control))
        {
            return Err(failure("invalid_request", "Invalid target build"));
        }
        let source_guard = SafeDirectory::open(root)?;
        let root = normalized_path(&source_guard.path)?;
        // Storage inside the source would itself mutate the captured project.
        if inside(&self.root(), &root) || inside(&root, &self.root()) {
            return Err(failure(
                "unsafe_source",
                "Checkpoint storage and source must not overlap",
            ));
        }
        let _storage = SafeDirectory::create(&self.root())?;
        let _operation = CaptureLock::take(&self.root())?;
        if self.list(project_id)?.len() >= MAX_CHECKPOINTS {
            return Err(failure(
                "snapshot_limit",
                "Checkpoint history is full; archive it first",
            ));
        }
        let source = read_source_manifest(&root)?;
        if source.revision != request.expected_revision {
            return Err(failure(
                "source_changed",
                "Source changed; refresh before creating a checkpoint",
            ));
        }
        let objects = SafeDirectory::create(&self.root().join("objects"))?;
        for file in &source.files {
            store_source_object(&objects.path, &root, file)?;
        }
        // Metadata is parsed from the exact stored TOML, not a second live read.
        let manifest_file = source.files.iter().find(|f| f.path == "mod.toml").unwrap();
        let manifest_bytes =
            read_plain_file(&objects.path.join(&manifest_file.sha256), 16 * 1024 * 1024)?;
        let config: crate::modproj::ModToml = toml::from_str(std::str::from_utf8(&manifest_bytes)?)
            .context("Checkpoint mod.toml is not a valid project")?;
        let directory = SafeDirectory::create(&self.directory(project_id))?;
        let record = StoredCheckpoint {
            schema_version: 1,
            project_id: project_id.into(),
            id: format!("checkpoint-{}", nonce()),
            label: request.label.trim().into(),
            created_unix: unix_now()?,
            source_root: root.clone(),
            source,
            project_build: config.meta.game_build,
            target_build: target_build.map(str::to_owned),
            renderer_version: format!(
                "{}:{}",
                env!("CARGO_PKG_VERSION"),
                super::http::web_fingerprint()
            ),
            recipe: RECIPE.into(),
            preview: StoredPreview {
                state: PreviewStatus::Pending,
                hash: None,
                reason: None,
            },
        };
        let bytes = record_bytes(&record)?;
        // All objects and metadata are complete before the last full rehash.
        if read_source_manifest(&root)? != record.source {
            return Err(failure(
                "source_changed",
                "Source changed during capture; no checkpoint was published",
            ));
        }
        publish_new(&directory.path.join(format!("{}.json", record.id)), &bytes)?;
        Ok(record.public())
    }
    pub fn diff(
        &self,
        project_id: &str,
        checkpoint_id: &str,
        root: &Path,
    ) -> Result<CheckpointDiff> {
        let saved = self.read(project_id, checkpoint_id)?;
        let current = read_source_manifest(root)?;
        let before: BTreeMap<_, _> = saved
            .source
            .files
            .iter()
            .map(|f| (f.path.as_str(), f))
            .collect();
        let after: BTreeMap<_, _> = current.files.iter().map(|f| (f.path.as_str(), f)).collect();
        let mut added = vec![];
        let mut removed = vec![];
        let mut changed = vec![];
        let mut unchanged_count = 0;
        for (path, file) in &after {
            match before.get(path) {
                None => added.push((*path).into()),
                Some(old) if old.sha256 != file.sha256 || old.bytes != file.bytes => {
                    changed.push((*path).into())
                }
                Some(_) => unchanged_count += 1,
            }
        }
        for path in before.keys() {
            if !after.contains_key(path) {
                removed.push((*path).into());
            }
        }
        Ok(CheckpointDiff {
            ok: true,
            checkpoint_revision: saved.source.revision,
            current_revision: current.revision,
            added,
            removed,
            changed,
            unchanged_count,
        })
    }
    pub fn restore(
        &self,
        project_id: &str,
        checkpoint_id: &str,
        destination: &Path,
        expected_revision: &str,
    ) -> Result<RestoredProject> {
        self.restore_with_context(
            project_id,
            checkpoint_id,
            destination,
            expected_revision,
            &[],
        )
    }
    pub fn restore_with_context(
        &self,
        project_id: &str,
        checkpoint_id: &str,
        destination: &Path,
        expected_revision: &str,
        forbidden: &[PathBuf],
    ) -> Result<RestoredProject> {
        let record = self.read(project_id, checkpoint_id)?;
        if expected_revision != record.source.revision {
            return Err(failure(
                "source_changed",
                "Selected checkpoint revision does not match; refresh history",
            ));
        }
        let (parent, destination) = self.restore_destination(&record, destination, forbidden)?;
        let _storage = SafeDirectory::open(&self.root())?;
        let _operation = CaptureLock::take(&self.root())?;
        let staging = parent.path.join(format!(".wowsmod-restore-{}", nonce()));
        let result = (|| -> Result<()> {
            fs::create_dir(&staging)?;
            let staging_guard = SafeDirectory::open(&staging)?;
            self.materialize(&record, &staging_guard.path)?;
            // Release the staging directory handle before rename on Windows;
            // keep its separately held parent pinned until publication ends.
            drop(staging_guard);
            rename_no_replace(&staging, &destination).map_err(|e| {
                failure(
                    "restore_conflict",
                    format!("Destination could not be published without overwrite: {e}"),
                )
            })?;
            Ok(())
        })();
        if result.is_err() {
            cleanup_owned_directory(&staging, &parent.path);
        }
        result?;
        Ok(RestoredProject {
            path: destination,
            checkpoint_id: record.id,
            revision: record.source.revision,
            warnings: vec![
                "External SDK and game resources are not included in this source checkpoint."
                    .into(),
            ],
        })
    }
    fn restore_destination(
        &self,
        record: &StoredCheckpoint,
        destination: &Path,
        forbidden: &[PathBuf],
    ) -> Result<(SafeDirectory, PathBuf)> {
        let absolute = std::path::absolute(destination)?;
        // Reject syntactic traversal before normalization can erase it.
        if destination
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(failure(
                "unsafe_destination",
                "Destination must not contain parent traversal",
            ));
        }
        let name = absolute
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| failure("unsafe_destination", "Choose a new named project directory"))?;
        validate_source_path(name).map_err(|e| {
            failure(
                "unsafe_destination",
                format!("Invalid destination name: {e}"),
            )
        })?;
        let parent = SafeDirectory::open(absolute.parent().ok_or_else(|| {
            failure("unsafe_destination", "Destination needs an existing parent")
        })?)?;
        let canonical_parent = fs::canonicalize(&parent.path)?;
        let target = canonical_parent.join(name);
        match fs::symlink_metadata(&target) {
            Ok(_) => {
                return Err(failure(
                    "destination_exists",
                    "Destination already exists; choose a new directory",
                ))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        for protected in std::iter::once(record.source_root.clone())
            .chain(std::iter::once(self.root()))
            .chain(forbidden.iter().cloned())
        {
            let protected = normalized_path(&protected)?;
            if inside(&target, &protected) {
                return Err(failure("unsafe_destination", format!("Destination is inside a protected source, checkpoint or game directory: {}", protected.display())));
            }
        }
        Ok((parent, target))
    }
    fn materialize(&self, record: &StoredCheckpoint, destination: &Path) -> Result<()> {
        validate_source_manifest(&record.source)?;
        let objects = SafeDirectory::open(&self.root().join("objects"))?;
        for source in &record.source.files {
            let path = destination.join(&source.path);
            let _parent = SafeDirectory::create(path.parent().unwrap())?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            copy_object(&objects.path, source, &mut file)?;
            file.sync_all()?;
        }
        let actual = read_source_manifest(destination)?;
        if actual != record.source {
            return Err(failure(
                "corrupt_checkpoint",
                "Restored source content did not match its checkpoint",
            ));
        }
        crate::modproj::Project::open(destination)
            .context("Restored checkpoint is not a valid project")?;
        Ok(())
    }
    pub fn queue_preview(
        self: &Arc<Self>,
        project_id: &str,
        checkpoint_id: &str,
        selected_game: Option<PathBuf>,
    ) -> Result<()> {
        let record = self.read(project_id, checkpoint_id)?;
        if record.preview.state != PreviewStatus::Pending {
            return Ok(());
        }
        let key = (project_id.to_owned(), checkpoint_id.to_owned());
        let mut queue = self.queue.lock().unwrap();
        if queue.active.contains(&key) {
            return Ok(());
        }
        if queue.active.len() >= MAX_PREVIEW_QUEUE {
            drop(queue);
            self.set_preview(
                project_id,
                checkpoint_id,
                Err(failure(
                    "preview_busy",
                    "Snapshot saved; preview queue is full",
                )),
            )?;
            return Err(failure(
                "preview_busy",
                "Snapshot saved; preview queue is full",
            ));
        }
        queue.active.insert(key);
        queue.jobs.push_back(PreviewJob {
            project_id: project_id.into(),
            checkpoint_id: checkpoint_id.into(),
            selected_game,
        });
        let start = !queue.running;
        queue.running = true;
        drop(queue);
        if start {
            let store = Arc::clone(self);
            std::thread::spawn(move || store.drain_previews());
        }
        Ok(())
    }
    fn drain_previews(&self) {
        loop {
            let job = {
                let mut queue = self.queue.lock().unwrap();
                let Some(job) = queue.jobs.pop_front() else {
                    queue.running = false;
                    return;
                };
                job
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let record = self.read(&job.project_id, &job.checkpoint_id)?;
                self.generate_preview(&job.project_id, &job.checkpoint_id, &|frozen| {
                    // Target build is a capture-time fact. Never label a new
                    // client's render as the old build's preview.
                    if let Some(expected) = record.target_build.as_deref() {
                        let (game, _) = crate::game::locate(job.selected_game.as_deref());
                        if game.as_ref().map(|g| g.build.as_str()) != Some(expected) {
                            return Err(failure(
                                "preview_context_changed",
                                "Selected game build differs from capture-time build",
                            ));
                        }
                    }
                    if record.renderer_version
                        != format!(
                            "{}:{}",
                            env!("CARGO_PKG_VERSION"),
                            super::http::web_fingerprint()
                        )
                    {
                        return Err(failure(
                            "preview_context_changed",
                            "Renderer changed after snapshot capture",
                        ));
                    }
                    let cli = super::library::selected_cli(job.selected_game.clone());
                    let mut rendered = super::render_pngs(
                        &cli,
                        frozen,
                        &[("checkpoint".into(), RECIPE.into())],
                        None,
                        (640, 360),
                    )?;
                    if rendered.len() != 1 {
                        return Err(failure(
                            "preview_failed",
                            "Renderer did not return exactly one checkpoint PNG",
                        ));
                    }
                    if let Some(expected) = record.target_build.as_deref() {
                        let (game, _) = crate::game::locate(job.selected_game.as_deref());
                        if game.as_ref().map(|g| g.build.as_str()) != Some(expected) {
                            return Err(failure(
                                "preview_context_changed",
                                "Selected game build changed while rendering",
                            ));
                        }
                    }
                    Ok(rendered.remove(0).0)
                })
            }))
            .unwrap_or_else(|_| {
                Err(failure(
                    "preview_failed",
                    "Checkpoint renderer panicked; snapshot remains recoverable",
                ))
            });
            if let Err(error) = result {
                if let Err(save_error) =
                    self.set_preview(&job.project_id, &job.checkpoint_id, Err(error))
                {
                    eprintln!("Checkpoint preview status could not be persisted: {save_error:#}");
                }
            }
            self.queue
                .lock()
                .unwrap()
                .active
                .remove(&(job.project_id, job.checkpoint_id));
        }
    }
    pub fn read_preview(
        &self,
        project_id: &str,
        checkpoint_id: &str,
        hash: &str,
    ) -> Result<Vec<u8>> {
        if !is_sha256(hash) {
            return Err(failure("invalid_request", "Invalid PNG hash"));
        }
        let record = self.read(project_id, checkpoint_id)?;
        if record.preview.state != PreviewStatus::Ready
            || record.preview.hash.as_deref() != Some(hash)
        {
            return Err(failure(
                "not_found",
                "This image is not bound to the requested checkpoint",
            ));
        }
        let _guard = SafeDirectory::open(&self.root().join("previews"))?;
        let bytes = read_plain_file(
            &self.root().join("previews").join(format!("{hash}.png")),
            MAX_PNG_BYTES,
        )?;
        if sha256_hex(&bytes) != hash {
            return Err(failure(
                "corrupt_checkpoint",
                "Checkpoint PNG hash does not match",
            ));
        }
        validate_png(&bytes)?;
        Ok(bytes)
    }
    fn generate_preview(
        &self,
        project_id: &str,
        checkpoint_id: &str,
        renderer: &dyn Fn(&Path) -> Result<Vec<u8>>,
    ) -> Result<()> {
        let record = self.read(project_id, checkpoint_id)?;
        let workspace = self.home.join("checkpoint-preview-work");
        let mut owned = None;
        let result = (|| -> Result<String> {
            let parent = SafeDirectory::create(&workspace)?;
            let frozen = parent.path.join(format!(".wowsmod-preview-{}", nonce()));
            owned = Some(OwnedWorkspace {
                path: frozen.clone(),
                parent: parent.path.clone(),
            });
            let frozen_guard = {
                let _storage = SafeDirectory::open(&self.root())?;
                let _operation = CaptureLock::take(&self.root())?;
                fs::create_dir(&frozen)?;
                let guard = SafeDirectory::open(&frozen)?;
                self.materialize(&record, &frozen)?;
                guard
            };
            // Materialization releases the shared capture lock before CDP.
            // The renderer owns a completely separate temporary service/state.
            let png = renderer(&frozen_guard.path)?;
            validate_png(&png)?;
            if read_source_manifest(&frozen_guard.path)? != record.source {
                return Err(failure(
                    "preview_source_changed",
                    "Renderer changed source inputs in frozen workspace",
                ));
            }
            let hash = sha256_hex(&png);
            let images = SafeDirectory::create(&self.root().join("previews"))?;
            let image_path = images.path.join(format!("{hash}.png"));
            match fs::symlink_metadata(&image_path) {
                Ok(_) => {
                    let existing = read_plain_file(&image_path, MAX_PNG_BYTES)?;
                    if sha256_hex(&existing) != hash {
                        return Err(failure(
                            "corrupt_checkpoint",
                            "Existing immutable preview object is damaged",
                        ));
                    }
                    validate_png(&existing)?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    publish_new(&image_path, &png)?
                }
                Err(e) => return Err(e.into()),
            }
            Ok(hash)
        })();
        drop(owned);
        self.set_preview(project_id, checkpoint_id, result)
    }
    fn set_preview(
        &self,
        project_id: &str,
        checkpoint_id: &str,
        result: Result<String>,
    ) -> Result<()> {
        let path = self.record_path(project_id, checkpoint_id)?;
        let _parent = SafeDirectory::open(path.parent().unwrap())?;
        let mut record = self.read(project_id, checkpoint_id)?;
        record.preview = match result {
            Ok(hash) => StoredPreview {
                state: PreviewStatus::Ready,
                hash: Some(hash),
                reason: None,
            },
            Err(error) => {
                let message = format!("{error:#}");
                StoredPreview {
                    state: PreviewStatus::Failed,
                    hash: None,
                    reason: Some(message.chars().take(4096).collect()),
                }
            }
        };
        crate::library::atomic_write(&path, &record_bytes(&record)?)
    }
}

fn validate_project_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
        return Err(failure("invalid_request", "Invalid project identity"));
    }
    Ok(())
}
fn validate_png(bytes: &[u8]) -> Result<()> {
    if bytes.len() < 24
        || bytes.len() as u64 > MAX_PNG_BYTES
        || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
        || &bytes[12..16] != b"IHDR"
        || u32::from_be_bytes(bytes[16..20].try_into()?) != 640
        || u32::from_be_bytes(bytes[20..24].try_into()?) != 360
    {
        return Err(failure(
            "preview_failed",
            "Checkpoint preview must be a 640 × 360 PNG",
        ));
    }
    let image = wowsmod_formats::texture::read_png(bytes)?;
    if image.width != 640 || image.height != 360 {
        return Err(failure(
            "preview_failed",
            "Decoded checkpoint preview dimensions differ",
        ));
    }
    Ok(())
}
fn validate_identity(project_id: &str, checkpoint_id: &str) -> Result<()> {
    validate_project_id(project_id)?;
    if !checkpoint_id
        .strip_prefix("checkpoint-")
        .is_some_and(is_sha256)
    {
        return Err(failure("invalid_request", "Invalid checkpoint identity"));
    }
    Ok(())
}
fn validate_label(label: &str) -> Result<()> {
    if label.trim().is_empty() || label.len() > 256 || label.chars().any(char::is_control) {
        return Err(failure(
            "invalid_request",
            "Checkpoint label must be 1–256 bytes without control characters",
        ));
    }
    Ok(())
}
fn validate_record(record: &StoredCheckpoint, project_id: &str, checkpoint_id: &str) -> Result<()> {
    let valid = || -> Result<()> {
        if record.schema_version != 1
            || record.project_id != project_id
            || record.id != checkpoint_id
            || !record.source_root.is_absolute()
            || record.recipe != RECIPE
            || record.renderer_version.is_empty()
            || record.renderer_version.len() > 256
        {
            return Err(failure(
                "corrupt_checkpoint",
                "Checkpoint schema, identity or recipe does not match",
            ));
        }
        validate_identity(&record.project_id, &record.id)?;
        validate_label(&record.label)?;
        validate_source_manifest(&record.source)?;
        match record.preview.state {
            PreviewStatus::Ready
                if record.preview.hash.as_deref().is_some_and(is_sha256)
                    && record.preview.reason.is_none() =>
            {
                ()
            }
            PreviewStatus::Pending
                if record.preview.hash.is_none() && record.preview.reason.is_none() =>
            {
                ()
            }
            PreviewStatus::Failed
                if record.preview.hash.is_none()
                    && record
                        .preview
                        .reason
                        .as_ref()
                        .is_some_and(|s| !s.is_empty() && s.len() <= 16384) =>
            {
                ()
            }
            _ => {
                return Err(failure(
                    "corrupt_checkpoint",
                    "Checkpoint preview metadata is inconsistent",
                ))
            }
        }
        Ok(())
    };
    valid().map_err(|e| {
        failure(
            "corrupt_checkpoint",
            format!("Invalid checkpoint; original retained: {e:#}"),
        )
    })
}
fn record_bytes(record: &StoredCheckpoint) -> Result<Vec<u8>> {
    validate_record(record, &record.project_id, &record.id)?;
    let bytes = serde_json::to_vec(record)?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(failure(
            "snapshot_limit",
            "Checkpoint manifest exceeds 64 MiB; reduce the source",
        ));
    }
    Ok(bytes)
}
fn unix_now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
fn nonce() -> String {
    sha256_hex(
        format!(
            "{}:{}:{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
        .as_bytes(),
    )
}
/// Canonicalize the nearest existing ancestor, checking every level for links.
/// Missing source paths still remain usable as restore exclusion boundaries.
fn normalized_path(path: &Path) -> Result<PathBuf> {
    if path
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(failure(
            "unsafe_source",
            "Directory paths must not contain parent traversal",
        ));
    }
    let absolute = std::path::absolute(path)?;
    let mut current = absolute.as_path();
    let mut suffix = vec![];
    loop {
        match fs::symlink_metadata(current) {
            Ok(_) => {
                let _guard = SafeDirectory::open(current)?;
                let mut normalized = fs::canonicalize(current)?;
                for component in suffix.into_iter().rev() {
                    normalized.push(component);
                }
                return Ok(normalized);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                suffix.push(
                    current
                        .file_name()
                        .ok_or_else(|| failure("unsafe_source", "No existing directory ancestor"))?
                        .to_os_string(),
                );
                current = current
                    .parent()
                    .ok_or_else(|| failure("unsafe_source", "No existing directory ancestor"))?;
            }
            Err(e) => return Err(e.into()),
        }
    }
}
fn inside(path: &Path, parent: &Path) -> bool {
    #[cfg(windows)]
    {
        let path: Vec<_> = path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect();
        let parent: Vec<_> = parent
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect();
        path.starts_with(&parent)
    }
    #[cfg(not(windows))]
    {
        path.starts_with(parent)
    }
}
fn copy_object(objects: &Path, source: &SourceFile, writer: &mut impl Write) -> Result<()> {
    let object = SourceFile {
        path: source.sha256.clone(),
        bytes: source.bytes,
        sha256: source.sha256.clone(),
    };
    copy_verified_source_file(objects, &object, writer).map_err(|e| {
        failure(
            "corrupt_checkpoint",
            format!(
                "Missing, unreadable or corrupt content object {} for {}: {e:#}",
                source.sha256, source.path
            ),
        )
    })
}
fn store_source_object(objects: &Path, root: &Path, source: &SourceFile) -> Result<()> {
    let final_path = objects.join(&source.sha256);
    match fs::symlink_metadata(&final_path) {
        Ok(_) => return copy_object(objects, source, &mut std::io::sink()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    let temporary = objects.join(format!(".{}.tmp", nonce()));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        copy_verified_source_file(root, source, &mut file)?;
        file.sync_all()?;
        drop(file);
        // A hard-link publication is atomic and never replaces an existing file.
        fs::hard_link(&temporary, &final_path)?;
        copy_object(objects, source, &mut std::io::sink())?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}
fn publish_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| failure("unsafe_source", "Missing publication parent"))?;
    let _guard = SafeDirectory::open(parent)?;
    let temporary = parent.join(format!(".{}.tmp", nonce()));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        rename_no_replace(&temporary, path)?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
fn rename_no_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
        }
        let from: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0x8) } == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        extern "C" {
            fn renameat2(
                olddirfd: i32,
                oldpath: *const i8,
                newdirfd: i32,
                newpath: *const i8,
                flags: u32,
            ) -> i32;
        }
        let from = std::ffi::CString::new(source.as_os_str().as_bytes())?;
        let to = std::ffi::CString::new(destination.as_os_str().as_bytes())?;
        if unsafe { renameat2(-100, from.as_ptr(), -100, to.as_ptr(), 1) } == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        extern "C" {
            fn renamex_np(from: *const i8, to: *const i8, flags: u32) -> i32;
        }
        let from = std::ffi::CString::new(source.as_os_str().as_bytes())?;
        let to = std::ffi::CString::new(destination.as_os_str().as_bytes())?;
        if unsafe { renamex_np(from.as_ptr(), to.as_ptr(), 4) } == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = (source, destination);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Atomic no-replace rename unavailable",
        ))
    }
}
fn cleanup_owned_directory(path: &Path, parent: &Path) {
    if path.parent() == Some(parent)
        && path
            .file_name()
            .is_some_and(|s| s.to_string_lossy().starts_with(".wowsmod-"))
    {
        let _ = fs::remove_dir_all(path);
    }
}
struct OwnedWorkspace {
    path: PathBuf,
    parent: PathBuf,
}
impl Drop for OwnedWorkspace {
    fn drop(&mut self) {
        cleanup_owned_directory(&self.path, &self.parent);
    }
}
struct CaptureLock {
    path: PathBuf,
    file: Option<File>,
}
impl CaptureLock {
    fn take(directory: &Path) -> Result<Self> {
        let path = directory.join("capture.lock");
        let file = crate::project_revision::retry_io(Duration::from_secs(3), crate::project_revision::lock_contention,
            || OpenOptions::new().write(true).create_new(true).open(&path))
            .map_err(|e| failure("checkpoint_busy", format!("Another capture/materialization is busy or storage is unwritable: {e}; if its process exited, inspect {}", path.display())))?;
        let mut result = Self {
            path,
            file: Some(file),
        };
        writeln!(result.file.as_mut().unwrap(), "pid={}", std::process::id())?;
        Ok(result)
    }
}
impl Drop for CaptureLock {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_revision::{read_source_manifest, tests::Fixture};
    use std::fs;
    fn request(root: &Path) -> CreateCheckpoint {
        CreateCheckpoint {
            label: "snapshot A".into(),
            expected_revision: read_source_manifest(root).unwrap().revision,
        }
    }
    #[test]
    fn frozen_capture_survives_change_reopen_and_missing_source() {
        let p = Fixture::project();
        let h = Fixture::new();
        let out = Fixture::new();
        fs::write(p.0.join("ordinary.txt"), b"A").unwrap();
        fs::create_dir_all(p.0.join(".install")).unwrap();
        fs::write(p.0.join(".install/manifest.json"), "old install").unwrap();
        let store = CheckpointStore::open(&h.0).unwrap();
        let saved = store.create("project-a", &p.0, request(&p.0)).unwrap();
        fs::write(p.0.join("ordinary.txt"), b"B").unwrap();
        drop(store);
        let store = CheckpointStore::open(&h.0).unwrap();
        assert_eq!(store.list("project-a").unwrap()[0], saved);
        assert!(store.list("different-project").unwrap().is_empty());
        let restored = store
            .restore(
                "project-a",
                &saved.id,
                &out.0.join("restored"),
                &saved.revision,
            )
            .unwrap();
        assert_eq!(fs::read(restored.path.join("ordinary.txt")).unwrap(), b"A");
        assert_eq!(fs::read(p.0.join("ordinary.txt")).unwrap(), b"B");
        assert!(!restored.path.join(".install").exists());
        assert_eq!(
            read_source_manifest(&restored.path).unwrap().revision,
            saved.revision
        );
        fs::remove_dir_all(&p.0).unwrap();
        assert_eq!(store.list("project-a").unwrap().len(), 1);
        store
            .restore(
                "project-a",
                &saved.id,
                &out.0.join("without-source"),
                &saved.revision,
            )
            .unwrap();
    }
    #[test]
    fn diff_uses_checkpoint_as_baseline_and_expected_revision_prevents_stale_capture() {
        let p = Fixture::project();
        let h = Fixture::new();
        fs::write(p.0.join("removed.txt"), "old").unwrap();
        fs::write(p.0.join("changed.txt"), "A").unwrap();
        let store = CheckpointStore::open(&h.0).unwrap();
        let before = request(&p.0);
        let saved = store.create("id", &p.0, before.clone()).unwrap();
        fs::remove_file(p.0.join("removed.txt")).unwrap();
        fs::write(p.0.join("added.txt"), "new").unwrap();
        fs::write(p.0.join("changed.txt"), "B").unwrap();
        let diff = store.diff("id", &saved.id, &p.0).unwrap();
        assert_eq!(diff.added, ["added.txt"]);
        assert_eq!(diff.removed, ["removed.txt"]);
        assert_eq!(diff.changed, ["changed.txt"]);
        assert_eq!(diff.checkpoint_revision, saved.revision);
        assert_ne!(diff.current_revision, saved.revision);
        assert!(store.create("id", &p.0, before).is_err());
        assert_eq!(store.list("id").unwrap().len(), 1);
    }
    #[test]
    fn recovery_never_overwrites_and_rejects_protected_directories() {
        let p = Fixture::project();
        let h = Fixture::new();
        let out = Fixture::new();
        let game = Fixture::new();
        let store = CheckpointStore::open(&h.0).unwrap();
        let saved = store.create("id", &p.0, request(&p.0)).unwrap();
        fs::write(out.0.join("sentinel"), "KEEP").unwrap();
        assert!(store
            .restore("id", &saved.id, &out.0, &saved.revision)
            .is_err());
        assert_eq!(fs::read(out.0.join("sentinel")).unwrap(), b"KEEP");
        assert!(store
            .restore("id", &saved.id, &p.0.join("inside"), &saved.revision)
            .is_err());
        assert!(store
            .restore(
                "id",
                &saved.id,
                &h.0.join("checkpoints/inside"),
                &saved.revision
            )
            .is_err());
        assert!(store
            .restore_with_context(
                "id",
                &saved.id,
                &game.0.join("inside"),
                &saved.revision,
                &[game.0.clone()]
            )
            .is_err());
        assert!(store
            .restore("id", &saved.id, &out.0.join("stale"), "sha256:bad")
            .is_err());
        assert!(!p.0.join("inside").exists());
        assert!(!game.0.join("inside").exists());
    }
    #[test]
    fn corrupt_or_missing_object_never_publishes_a_restored_directory() {
        let p = Fixture::project();
        let h = Fixture::new();
        let out = Fixture::new();
        fs::write(p.0.join("payload.txt"), "unique payload").unwrap();
        let store = CheckpointStore::open(&h.0).unwrap();
        let saved = store.create("id", &p.0, request(&p.0)).unwrap();
        let hash = crate::project_revision::sha256_hex(b"unique payload");
        let object = h.0.join("checkpoints/objects").join(hash);
        fs::write(&object, "BROKEN").unwrap();
        assert!(store
            .restore("id", &saved.id, &out.0.join("corrupt"), &saved.revision)
            .is_err());
        assert!(!out.0.join("corrupt").exists());
        assert!(store.create("id", &p.0, request(&p.0)).is_err());
        assert_eq!(store.list("id").unwrap().len(), 1);
        fs::remove_file(&object).unwrap();
        assert!(store
            .restore("id", &saved.id, &out.0.join("missing"), &saved.revision)
            .is_err());
        assert!(fs::read_dir(&out.0).unwrap().next().is_none());
    }
    #[test]
    fn duplicate_content_is_shared_and_scope_preserves_author_files() {
        let p = Fixture::project();
        let h = Fixture::new();
        for name in ["one.txt", "design/two.txt", "edits/three.txt", "fx.json"] {
            let path = p.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "same").unwrap();
        }
        for name in [
            ".render/cache.bin",
            ".wowsmod/cache.bin",
            "out/last.png",
            "renders/image.png",
        ] {
            let path = p.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "cache").unwrap();
        }
        let store = CheckpointStore::open(&h.0).unwrap();
        let first = store.create("id", &p.0, request(&p.0)).unwrap();
        let second = store.create("id", &p.0, request(&p.0)).unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(first.revision, second.revision);
        let manifest = read_source_manifest(&p.0).unwrap();
        let unique: std::collections::HashSet<_> =
            manifest.files.iter().map(|f| &f.sha256).collect();
        assert_eq!(
            fs::read_dir(h.0.join("checkpoints/objects"))
                .unwrap()
                .count(),
            unique.len()
        );
        assert!(manifest.files.iter().any(|f| f.path == "design/two.txt"));
        assert_eq!(first.file_count, manifest.files.len());
    }
    #[cfg(windows)]
    #[test]
    fn recovery_rejects_real_junction_parent() {
        let p = Fixture::project();
        let h = Fixture::new();
        let out = Fixture::new();
        let other = Fixture::new();
        let store = CheckpointStore::open(&h.0).unwrap();
        let saved = store.create("id", &p.0, request(&p.0)).unwrap();
        let link = out.0.join("junction");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&other.0)
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "{}",
            String::from_utf8_lossy(&status.stderr)
        );
        assert!(store
            .restore("id", &saved.id, &link.join("restored"), &saved.revision)
            .is_err());
        assert!(fs::read_dir(&other.0).unwrap().next().is_none());
        fs::remove_dir(link).unwrap();
    }
    #[test]
    fn untrusted_manifest_paths_and_case_conflicts_are_rejected_without_writes() {
        let p = Fixture::project();
        let h = Fixture::new();
        let out = Fixture::new();
        fs::write(p.0.join("payload.txt"), "data").unwrap();
        let store = CheckpointStore::open(&h.0).unwrap();
        let saved = store.create("id", &p.0, request(&p.0)).unwrap();
        let record =
            h.0.join("checkpoints")
                .join(crate::project_revision::safe_project_key("id"))
                .join(format!("{}.json", saved.id));
        let original = fs::read(&record).unwrap();
        for path in [
            "../escaped",
            "/absolute",
            "C:/absolute",
            "payload.txt:stream",
            "MOD.toml",
        ] {
            let mut doc: serde_json::Value = serde_json::from_slice(&original).unwrap();
            doc["source"]["files"][1]["path"] = serde_json::json!(path);
            let corrupt = serde_json::to_vec(&doc).unwrap();
            fs::write(&record, &corrupt).unwrap();
            assert!(
                store
                    .restore("id", &saved.id, &out.0.join("restored"), &saved.revision)
                    .is_err(),
                "{path}"
            );
            assert!(fs::read_dir(&out.0).unwrap().next().is_none());
            assert_eq!(fs::read(&record).unwrap(), corrupt);
        }
    }
    #[test]
    fn preview_reads_frozen_revision_and_failure_retains_recoverable_snapshot() {
        let p = Fixture::project();
        let h = Fixture::new();
        let out = Fixture::new();
        fs::write(p.0.join("payload.txt"), "A").unwrap();
        let store = CheckpointStore::open(&h.0).unwrap();
        let saved = store.create("id", &p.0, request(&p.0)).unwrap();
        fs::write(p.0.join("payload.txt"), "B").unwrap();
        store
            .generate_preview("id", &saved.id, &|frozen| {
                assert_eq!(fs::read(frozen.join("payload.txt")).unwrap(), b"A");
                assert_ne!(frozen, p.0);
                anyhow::bail!("model_missing: no renderable model in fixture")
            })
            .unwrap();
        let saved = &store.list("id").unwrap()[0];
        assert_eq!(saved.preview.state, PreviewStatus::Failed);
        assert!(saved
            .preview
            .reason
            .as_deref()
            .unwrap()
            .contains("model_missing"));
        assert!(saved.preview.url.is_none());
        let restored = store
            .restore("id", &saved.id, &out.0.join("restored"), &saved.revision)
            .unwrap();
        assert_eq!(fs::read(restored.path.join("payload.txt")).unwrap(), b"A");
        assert!(!p.0.join(".wowsmod").exists());
    }
    #[test]
    fn ready_preview_is_immutable_hash_checked_and_ignores_generated_cache() {
        let p = Fixture::project();
        let h = Fixture::new();
        fs::write(p.0.join("payload.txt"), "A").unwrap();
        let store = CheckpointStore::open(&h.0).unwrap();
        let saved = store.create("id", &p.0, request(&p.0)).unwrap();
        fs::write(p.0.join("payload.txt"), "B").unwrap();
        let png = wowsmod_formats::texture::write_png(
            &wowsmod_formats::texture::Image {
                width: 640,
                height: 360,
                px: vec![255; 640 * 360 * 4],
            },
            true,
        )
        .unwrap();
        store
            .generate_preview("id", &saved.id, &|frozen| {
                assert_eq!(fs::read(frozen.join("payload.txt")).unwrap(), b"A");
                fs::create_dir_all(frozen.join(".wowsmod/cache"))?;
                fs::write(frozen.join(".wowsmod/cache/generated"), "cache")?;
                Ok(png.clone())
            })
            .unwrap();
        let latest = &store.list("id").unwrap()[0];
        assert_eq!(latest.preview.state, PreviewStatus::Ready);
        let hash = crate::project_revision::sha256_hex(&png);
        assert!(latest
            .preview
            .url
            .as_deref()
            .unwrap()
            .ends_with(&format!("/{hash}.png")));
        assert_eq!(store.read_preview("id", &saved.id, &hash).unwrap(), png);
        assert!(store
            .read_preview("id", &saved.id, &"0".repeat(64))
            .is_err());
        fs::write(
            h.0.join("checkpoints/previews").join(format!("{hash}.png")),
            "CORRUPT",
        )
        .unwrap();
        assert!(store.read_preview("id", &saved.id, &hash).is_err());
        assert!(!p.0.join(".wowsmod").exists());
        assert_eq!(fs::read(p.0.join("payload.txt")).unwrap(), b"B");
    }
    #[test]
    fn mtime_disguise_does_not_allow_stale_capture() {
        let p = Fixture::project();
        let h = Fixture::new();
        let path = p.0.join("payload.txt");
        fs::write(&path, "A").unwrap();
        let old_time = fs::metadata(&path).unwrap().modified().unwrap();
        let expected = request(&p.0);
        fs::write(&path, "B").unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old_time))
            .unwrap();
        let store = CheckpointStore::open(&h.0).unwrap();
        let error = store.create("id", &p.0, expected).unwrap_err();
        assert_eq!(
            crate::project_revision::error_code(&error),
            Some("source_changed")
        );
        assert!(store.list("id").unwrap().is_empty());
    }
    #[test]
    fn change_during_object_copy_is_not_published() {
        let p = Fixture::project();
        let h = Fixture::new();
        fs::write(p.0.join("zz-big.bin"), vec![7u8; 64 * 1024 * 1024]).unwrap();
        let expected = request(&p.0);
        let source = p.0.clone();
        let home = h.0.clone();
        let (send, receive) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            send.send(
                CheckpointStore::open(&home)
                    .unwrap()
                    .create("id", &source, expected),
            )
            .unwrap();
        });
        let started = std::time::Instant::now();
        loop {
            let copying = fs::read_dir(h.0.join("checkpoints/objects"))
                .ok()
                .is_some_and(|entries| {
                    entries
                        .flatten()
                        .any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
                });
            if copying {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "capture never entered object copy"
            );
            assert!(
                matches!(
                    receive.try_recv(),
                    Err(std::sync::mpsc::TryRecvError::Empty)
                ),
                "capture finished before synchronization"
            );
            std::thread::yield_now();
        }
        fs::write(p.0.join("added-while-copying.txt"), "late").unwrap();
        let error = receive
            .recv_timeout(Duration::from_secs(20))
            .unwrap()
            .unwrap_err();
        worker.join().unwrap();
        assert_eq!(
            crate::project_revision::error_code(&error),
            Some("source_changed")
        );
        assert!(CheckpointStore::open(&h.0)
            .unwrap()
            .list("id")
            .unwrap()
            .is_empty());
    }
    #[test]
    fn final_no_replace_rename_rejects_even_an_empty_destination() {
        let out = Fixture::new();
        let staging = out.0.join("staging");
        let target = out.0.join("new-target");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("payload"), "A").unwrap();
        fs::create_dir(&target).unwrap();
        assert!(rename_no_replace(&staging, &target).is_err());
        assert!(fs::read_dir(&target).unwrap().next().is_none());
        assert_eq!(fs::read(staging.join("payload")).unwrap(), b"A");
    }
    #[test]
    fn renderer_panic_cleans_owned_workspace() {
        let p = Fixture::project();
        let h = Fixture::new();
        let store = CheckpointStore::open(&h.0).unwrap();
        let saved = store.create("id", &p.0, request(&p.0)).unwrap();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.generate_preview("id", &saved.id, &|_| panic!("renderer crash"))
        }));
        assert!(fs::read_dir(h.0.join("checkpoint-preview-work"))
            .unwrap()
            .next()
            .is_none());
    }
}
