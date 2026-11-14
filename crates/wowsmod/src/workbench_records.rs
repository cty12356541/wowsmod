//! User-reported observations, keyed by stable project identity (never a path).
use crate::project_revision::{
    failure, is_revision, read_plain_file, read_source_manifest, safe_project_key, sha256_hex,
    SafeDirectory,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: u32 = 1;
const MAX_DOCUMENT_BYTES: u64 = 32 * 1024 * 1024;
const MAX_RECORDS: usize = 10_000;
pub const MAX_BUILD_BYTES: usize = 128;
pub const MAX_SCENE_BYTES: usize = 256;
pub const MAX_NOTE_BYTES: usize = 16_384;
pub const MAX_EVIDENCE: usize = 64;
pub const MAX_EVIDENCE_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationOutcome {
    Passed,
    Issues,
    Failed,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VerificationRecord {
    pub id: String,
    pub project_id: String,
    pub revision: String,
    pub build: String,
    pub outcome: VerificationOutcome,
    pub scene: String,
    pub note: String,
    pub evidence: Vec<String>,
    pub recorded_unix: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationRequest {
    pub expected_revision: String,
    pub build: String,
    pub outcome: VerificationOutcome,
    pub scene: String,
    pub note: String,
    pub evidence: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: u32,
    project_id: String,
    records: Vec<VerificationRecord>,
}
pub struct VerificationStore {
    home: PathBuf,
}
impl VerificationStore {
    /// Opening/listing never hashes or even accesses project source content.
    pub fn open(home: &Path) -> Result<Self> {
        let home = std::path::absolute(home)?;
        check_existing_ancestors(&home)?;
        Ok(Self { home })
    }
    fn directory(&self, project_id: &str) -> PathBuf {
        self.home
            .join("workbench-records")
            .join(safe_project_key(project_id))
    }
    fn read(&self, project_id: &str) -> Result<Document> {
        validate_project_id(project_id)?;
        let directory = self.directory(project_id);
        check_existing_ancestors(&directory)?;
        let path = directory.join("verifications.json");
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Document {
                    schema_version: SCHEMA_VERSION,
                    project_id: project_id.into(),
                    records: vec![],
                })
            }
            Err(e) => {
                return Err(failure(
                    "corrupt_records",
                    format!("Cannot read records; original retained: {e}"),
                ))
            }
            Ok(_) => (),
        }
        let bytes = read_plain_file(&path, MAX_DOCUMENT_BYTES).map_err(|e| {
            failure(
                "corrupt_records",
                format!("Cannot read records; original retained: {e:#}"),
            )
        })?;
        let doc: Document = serde_json::from_slice(&bytes).map_err(|e| {
            failure(
                "corrupt_records",
                format!(
                    "Invalid verification JSON; original retained at {}: {e}",
                    path.display()
                ),
            )
        })?;
        let valid = || -> Result<()> {
            if doc.schema_version != SCHEMA_VERSION
                || doc.project_id != project_id
                || doc.records.len() > MAX_RECORDS
            {
                return Err(failure(
                    "corrupt_records",
                    "Record schema, identity or count invalid",
                ));
            }
            let mut ids = HashSet::new();
            for record in &doc.records {
                if record.project_id != project_id
                    || record.id.len() > 128
                    || !record.id.starts_with("verification-")
                    || !ids.insert(&record.id)
                {
                    return Err(failure(
                        "corrupt_records",
                        "Invalid or duplicated record identity",
                    ));
                }
                validate_request(&VerificationRequest {
                    expected_revision: record.revision.clone(),
                    build: record.build.clone(),
                    outcome: record.outcome,
                    scene: record.scene.clone(),
                    note: record.note.clone(),
                    evidence: record.evidence.clone(),
                })?;
            }
            Ok(())
        };
        valid().map_err(|e| {
            failure(
                "corrupt_records",
                format!(
                    "Invalid verification metadata; original retained at {}: {e:#}",
                    path.display()
                ),
            )
        })?;
        Ok(doc)
    }
    pub fn list(&self, project_id: &str) -> Result<Vec<VerificationRecord>> {
        Ok(self.read(project_id)?.records)
    }
    pub fn append(
        &self,
        project_id: &str,
        root: &Path,
        mut request: VerificationRequest,
    ) -> Result<VerificationRecord> {
        validate_project_id(project_id)?;
        request.build = request.build.trim().into();
        request.scene = request.scene.trim().into();
        validate_request(&request)?;
        let directory = self.directory(project_id);
        let _guard = SafeDirectory::create(&directory)?;
        let _lock = RecordLock::take(&directory)?;
        // Read before capturing source so a damaged document is never replaced.
        let mut document = self.read(project_id)?;
        if document.records.len() >= MAX_RECORDS {
            return Err(failure(
                "snapshot_limit",
                "Verification history reaches 10000 records; archive it before adding records",
            ));
        }
        if read_source_manifest(root)?.revision != request.expected_revision {
            return Err(failure(
                "source_changed",
                "Source revision changed; refresh before recording an observation",
            ));
        }
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| failure("invalid_request", "System time precedes Unix epoch"))?;
        let record = VerificationRecord {
            id: format!(
                "verification-{}",
                sha256_hex(
                    format!(
                        "{project_id}\0{}\0{}\0{}",
                        std::process::id(),
                        time.as_nanos(),
                        document.records.len()
                    )
                    .as_bytes()
                )
            ),
            project_id: project_id.into(),
            revision: request.expected_revision,
            build: request.build,
            outcome: request.outcome,
            scene: request.scene,
            note: request.note,
            evidence: request.evidence,
            recorded_unix: time.as_secs(),
        };
        document.records.push(record.clone());
        let bytes = serde_json::to_vec_pretty(&document)?;
        if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
            return Err(failure(
                "snapshot_limit",
                "Verification history exceeds 32 MiB; archive it before adding records",
            ));
        }
        // Recheck after serialization/record preparation; stale expected content
        // is never relabelled as the user's new observation.
        if read_source_manifest(root)?.revision != record.revision {
            return Err(failure(
                "source_changed",
                "Source changed while preparing record; refresh and retry",
            ));
        }
        crate::library::atomic_write(&directory.join("verifications.json"), &bytes)?;
        Ok(record)
    }
    pub fn latest(&self, project_id: &str) -> Result<Option<VerificationRecord>> {
        Ok(self.list(project_id)?.pop())
    }
}
/// Append order breaks same-second timestamps and remains correct on clock rollback.
/// Outcome is deliberately not filtered: the latest failure replaces older passes.
pub fn latest_matching<'a>(
    records: &'a [VerificationRecord],
    project_id: &str,
    revision: &str,
    build: &str,
) -> Option<&'a VerificationRecord> {
    records.iter().rev().find(|record| {
        record.project_id == project_id && record.revision == revision && record.build == build
    })
}
fn validate_project_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 1024 {
        return Err(failure(
            "invalid_request",
            "Project ID must contain 1–1024 UTF-8 bytes",
        ));
    }
    Ok(())
}
fn validate_request(request: &VerificationRequest) -> Result<()> {
    if !is_revision(&request.expected_revision) {
        return Err(failure(
            "invalid_request",
            "expected_revision must be sha256:<64 lowercase hex>",
        ));
    }
    for (name, value, max) in [
        ("build", request.build.as_str(), MAX_BUILD_BYTES),
        ("scene", request.scene.as_str(), MAX_SCENE_BYTES),
    ] {
        if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
            return Err(failure(
                "invalid_request",
                format!("{name} must contain 1–{max} UTF-8 bytes without control characters"),
            ));
        }
    }
    if request.note.len() > MAX_NOTE_BYTES
        || request.evidence.len() > MAX_EVIDENCE
        || request
            .evidence
            .iter()
            .any(|e| e.trim().is_empty() || e.len() > MAX_EVIDENCE_BYTES || e.contains('\0'))
    {
        return Err(failure(
            "invalid_request",
            "Note/evidence length limit exceeded or empty evidence item",
        ));
    }
    Ok(())
}
fn check_existing_ancestors(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => {
                let _guard = SafeDirectory::open(ancestor)?;
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    Err(failure(
        "unsafe_source",
        "No existing safe storage ancestor",
    ))
}
struct RecordLock {
    path: PathBuf,
    file: Option<File>,
}
impl RecordLock {
    fn take(directory: &Path) -> Result<Self> {
        let path = directory.join("verifications.lock");
        let file = crate::project_revision::retry_io(
            std::time::Duration::from_secs(3),
            crate::project_revision::lock_contention,
            || OpenOptions::new().write(true).create_new(true).open(&path),
        ).map_err(|error| {
            let message = format!("Verification history locked or unwritable: {error}; if its process exited, inspect and remove {}", path.display());
            anyhow::Error::new(error).context(crate::project_revision::SourceError { code: "records_locked", message })
        })?;
        let mut lock = Self {
            path,
            file: Some(file),
        };
        writeln!(lock.file.as_mut().unwrap(), "pid={}", std::process::id())?;
        Ok(lock)
    }
}
impl Drop for RecordLock {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_revision::{
        error_code, read_source_manifest, safe_project_key, tests::Fixture,
    };
    fn request(root: &Path, outcome: VerificationOutcome) -> VerificationRequest {
        VerificationRequest {
            expected_revision: read_source_manifest(root).unwrap().revision,
            build: "123".into(),
            outcome,
            scene: "harbor + battle".into(),
            note: "manual observation".into(),
            evidence: vec!["https://example.invalid/user-evidence".into()],
        }
    }
    #[test]
    fn latest_matching_requires_identity_content_and_build_and_keeps_failure() {
        let p = Fixture::project();
        let h = Fixture::new();
        let store = VerificationStore::open(&h.0).unwrap();
        assert!(store.list("stable-id").unwrap().is_empty());
        let pass = store
            .append(
                "stable-id",
                &p.0,
                request(&p.0, VerificationOutcome::Passed),
            )
            .unwrap();
        let failed = store
            .append(
                "stable-id",
                &p.0,
                request(&p.0, VerificationOutcome::Failed),
            )
            .unwrap();
        let records = store.list("stable-id").unwrap();
        assert_eq!(
            latest_matching(&records, "stable-id", &pass.revision, "123")
                .unwrap()
                .id,
            failed.id
        );
        assert!(latest_matching(&records, "other-id", &pass.revision, "123").is_none());
        assert!(latest_matching(&records, "stable-id", &pass.revision, "124").is_none());
        assert!(latest_matching(&records, "stable-id", "sha256:other", "123").is_none());
        assert_eq!(
            store.latest("stable-id").unwrap().unwrap().outcome,
            VerificationOutcome::Failed
        );
    }
    #[test]
    fn expected_revision_change_returns_conflict_without_history_mutation() {
        let p = Fixture::project();
        let h = Fixture::new();
        let store = VerificationStore::open(&h.0).unwrap();
        let req = request(&p.0, VerificationOutcome::Passed);
        store.append("id", &p.0, req.clone()).unwrap();
        let original = store.list("id").unwrap();
        p.put("custom.txt", b"changed");
        assert_eq!(
            error_code(&store.append("id", &p.0, req).unwrap_err()),
            Some("source_changed")
        );
        assert_eq!(store.list("id").unwrap(), original);
    }
    #[test]
    fn stable_id_preserves_history_after_move_copy_identity_has_none() {
        let p = Fixture::project();
        let h = Fixture::new();
        let moved = h.0.join("relocated");
        let store = VerificationStore::open(&h.0).unwrap();
        let record = store
            .append(
                "stable-id",
                &p.0,
                request(&p.0, VerificationOutcome::Passed),
            )
            .unwrap();
        std::fs::rename(&p.0, &moved).unwrap();
        assert_eq!(store.list("stable-id").unwrap(), vec![record.clone()]);
        assert_eq!(
            read_source_manifest(&moved).unwrap().revision,
            record.revision
        );
        assert!(store.list("copied-new-id").unwrap().is_empty());
    }
    #[test]
    fn corrupted_json_is_preserved_and_project_id_is_not_a_disk_path() {
        let p = Fixture::project();
        let h = Fixture::new();
        let store = VerificationStore::open(&h.0).unwrap();
        let id = "../../outside:stream";
        let path =
            h.0.join("workbench-records")
                .join(safe_project_key(id))
                .join("verifications.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{corrupt").unwrap();
        assert!(store.list(id).is_err());
        assert!(store
            .append(id, &p.0, request(&p.0, VerificationOutcome::Passed))
            .is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"{corrupt");
    }
    #[test]
    fn invalid_requests_do_not_publish_or_change_history() {
        let p = Fixture::project();
        let h = Fixture::new();
        let store = VerificationStore::open(&h.0).unwrap();
        let req = request(&p.0, VerificationOutcome::Passed);
        let mut bad = req.clone();
        bad.build = "  ".into();
        assert_eq!(
            error_code(&store.append("id", &p.0, bad).unwrap_err()),
            Some("invalid_request")
        );
        let mut bad = req.clone();
        bad.scene = "s".repeat(MAX_SCENE_BYTES + 1);
        assert!(store.append("id", &p.0, bad).is_err());
        let mut bad = req.clone();
        bad.evidence.push(String::new());
        assert!(store.append("id", &p.0, bad).is_err());
        let mut bad = req;
        bad.expected_revision = "not-a-revision".into();
        assert!(store.append("id", &p.0, bad).is_err());
        assert!(store.list("id").unwrap().is_empty());
        assert!(!store.directory("id").exists());
    }
    #[test]
    fn concurrent_appends_do_not_lose_records_or_duplicate_ids() {
        let p = Fixture::project();
        let h = Fixture::new();
        let req = request(&p.0, VerificationOutcome::Issues);
        std::thread::scope(|scope| {
            for _ in 0..6 {
                let req = req.clone();
                let p = &p.0;
                let h = &h.0;
                scope.spawn(move || {
                    VerificationStore::open(h)
                        .unwrap()
                        .append("id", p, req)
                        .unwrap()
                });
            }
        });
        let records = VerificationStore::open(&h.0).unwrap().list("id").unwrap();
        assert_eq!(records.len(), 6);
        assert_eq!(
            records.iter().map(|r| &r.id).collect::<HashSet<_>>().len(),
            6
        );
    }
    #[test]
    fn record_lock_real_create_delete_contention_keeps_every_operation() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let directory = Fixture::new();
        let active = AtomicUsize::new(0);
        let completed = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..6 {
                let directory = &directory.0;
                let active = &active;
                let completed = &completed;
                scope.spawn(move || {
                    for _ in 0..100 {
                        let lock = RecordLock::take(directory).unwrap();
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
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    }
    #[cfg(windows)]
    #[test]
    fn append_waits_for_strict_history_reader_then_publishes_complete_json() {
        use std::io::Read;
        use std::time::{Duration, Instant};
        let p = Fixture::project();
        let h = Fixture::new();
        let store = VerificationStore::open(&h.0).unwrap();
        let request = request(&p.0, VerificationOutcome::Passed);
        let first = store.append("id", &p.0, request.clone()).unwrap();
        let directory = store.directory("id");
        let path = directory.join("verifications.json");
        let mut reader = crate::project_revision::open_plain(&path, false).unwrap();
        std::thread::scope(|scope| {
            let (send, receive) = std::sync::mpsc::channel();
            let store = &store;
            let root = &p.0;
            let writer = scope.spawn(move || send.send(store.append("id", root, request)).unwrap());
            let deadline = Instant::now() + Duration::from_secs(2);
            let early = loop {
                if let Ok(result) = receive.try_recv() {
                    break Some(result);
                }
                if fs::read_dir(&directory)
                    .unwrap()
                    .any(|e| e.unwrap().file_name().to_string_lossy().ends_with(".tmp"))
                {
                    break receive.recv_timeout(Duration::from_millis(100)).ok();
                }
                assert!(
                    Instant::now() < deadline,
                    "writer did not prepare its atomic publication"
                );
                std::thread::yield_now();
            };
            let mut old_json = vec![];
            reader.read_to_end(&mut old_json).unwrap();
            let old: Document = serde_json::from_slice(&old_json).unwrap();
            assert_eq!(old.records, [first.clone()]);
            drop(reader);
            let result =
                early.unwrap_or_else(|| receive.recv_timeout(Duration::from_secs(4)).unwrap());
            let second = result.expect("writer must survive the concurrent strict reader");
            writer.join().unwrap();
            assert_eq!(store.list("id").unwrap(), [first, second]);
        });
        let names: Vec<_> = fs::read_dir(directory)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [std::ffi::OsString::from("verifications.json")]);
    }
    #[test]
    fn records_list_survives_unavailable_source_and_preserves_evidence_text() {
        let p = Fixture::project();
        let h = Fixture::new();
        let store = VerificationStore::open(&h.0).unwrap();
        let mut req = request(&p.0, VerificationOutcome::Passed);
        req.evidence = vec![
            " C:\\evidence\\battle.png ".into(),
            "https://example.invalid/no-network-request".into(),
        ];
        let record = store.append("id", &p.0, req.clone()).unwrap();
        drop(p);
        assert_eq!(store.latest("id").unwrap().unwrap(), record);
        assert_eq!(record.evidence, req.evidence);
    }
    #[test]
    fn wrong_schema_and_cross_project_record_are_not_overwritten() {
        let p = Fixture::project();
        let h = Fixture::new();
        let store = VerificationStore::open(&h.0).unwrap();
        let record = store
            .append("id", &p.0, request(&p.0, VerificationOutcome::Passed))
            .unwrap();
        let path = store.directory("id").join("verifications.json");
        for malformed in [
            serde_json::json!({"schema_version": 99, "project_id":"id", "records":[]}),
            serde_json::json!({"schema_version":1,"project_id":"other-id","records":[record]}),
        ] {
            let bytes = serde_json::to_vec(&malformed).unwrap();
            fs::write(&path, &bytes).unwrap();
            assert_eq!(
                error_code(&store.list("id").unwrap_err()),
                Some("corrupt_records")
            );
            assert!(store
                .append("id", &p.0, request(&p.0, VerificationOutcome::Passed))
                .is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }
}
