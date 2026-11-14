//! Short-lived server-side selections for read-only library scans.
use crate::library::{ImportResult, LibraryStore, ScanCandidate, ScanPreview};
use crate::project_revision::failure;
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_SESSIONS: usize = 16;
const SESSION_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Serialize)]
pub(crate) struct ScanPreviewResponse {
    scan_id: String,
    expires_unix: u64,
    #[serde(flatten)]
    preview: ScanPreview,
}

struct Session {
    id: String,
    expires: Instant,
    candidates: HashMap<String, ScanCandidate>,
}

#[derive(Default)]
pub(crate) struct ScanSessions {
    sessions: Mutex<VecDeque<Session>>,
}

impl ScanSessions {
    pub(crate) fn preview(&self, store: &LibraryStore, root: &Path) -> Result<ScanPreviewResponse> {
        let preview = store.scan_preview(root)?;
        self.publish(preview, Instant::now())
    }

    fn publish(&self, preview: ScanPreview, now: Instant) -> Result<ScanPreviewResponse> {
        let scan_id = new_scan_id()?;
        let expires_unix = (SystemTime::now() + SESSION_TTL)
            .duration_since(UNIX_EPOCH)?
            .as_secs();
        let candidates = preview
            .candidates
            .iter()
            .filter_map(|candidate| {
                let reference = candidate.import_reference()?;
                Some((reference.candidate_id.clone(), reference))
            })
            .collect();

        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| scan_sessions_unavailable())?;
        sessions.retain(|session| now < session.expires);
        if sessions.len() == MAX_SESSIONS {
            sessions.pop_front();
        }
        sessions.push_back(Session {
            id: scan_id.clone(),
            expires: now + SESSION_TTL,
            candidates,
        });
        Ok(ScanPreviewResponse {
            scan_id,
            expires_unix,
            preview,
        })
    }

    pub(crate) fn import(
        &self,
        store: &LibraryStore,
        scan_id: &str,
        candidate_ids: &[String],
    ) -> Result<ImportResult> {
        self.import_at(store, scan_id, candidate_ids, Instant::now())
    }

    fn import_at(
        &self,
        store: &LibraryStore,
        scan_id: &str,
        candidate_ids: &[String],
        now: Instant,
    ) -> Result<ImportResult> {
        let selected = {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| scan_sessions_unavailable())?;
            sessions.retain(|session| now < session.expires);
            let session = sessions
                .iter()
                .find(|session| session.id == scan_id)
                .ok_or_else(|| failure("scan_changed", "扫描预览已失效，请重新预览"))?;
            let mut seen = HashSet::new();
            let mut selected = Vec::new();
            for id in candidate_ids {
                let candidate = session.candidates.get(id).ok_or_else(|| {
                    failure("scan_changed", "选中项目不属于本次有效预览，请重新预览")
                })?;
                if seen.insert(id) {
                    selected.push(candidate.clone());
                }
            }
            selected
        };
        store.import_candidates(&selected)
    }
}

fn scan_sessions_unavailable() -> anyhow::Error {
    anyhow!("扫描预览会话不可用")
}

fn new_scan_id() -> Result<String> {
    let mut bytes = [0u8; 16];
    #[cfg(windows)]
    {
        #[link(name = "bcrypt")]
        extern "system" {
            fn BCryptGenRandom(
                algorithm: *mut std::ffi::c_void,
                buf: *mut u8,
                len: u32,
                flags: u32,
            ) -> i32;
        }
        const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 2;
        // Safety: null algorithm is required for the system RNG; the buffer is writable for len bytes.
        let status = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
        if status < 0 {
            anyhow::bail!("生成扫描会话 ID 失败 ({status:#x})");
        }
    }
    #[cfg(unix)]
    {
        use std::io::Read;
        std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    }
    Ok(format!("{:032x}", u128::from_be_bytes(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let fixture = Self(std::env::temp_dir().join(format!(
                "wowsmod-scan-sessions-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            )));
            for project in ["a", "b"] {
                let path = fixture.root().join(project);
                std::fs::create_dir_all(&path).unwrap();
                std::fs::write(path.join("mod.toml"), format!("[mod]\nname={project:?}\n"))
                    .unwrap();
            }
            fixture
        }
        fn root(&self) -> PathBuf {
            self.0.join("source")
        }
        fn store(&self) -> LibraryStore {
            LibraryStore::open(&self.0.join("home")).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn changed(result: Result<ImportResult>) {
        assert_eq!(
            crate::project_revision::error_code(&result.unwrap_err()),
            Some("scan_changed")
        );
    }

    #[test]
    fn scan_sessions_reject_unknown_and_other_scan_candidate_ids_before_import() {
        let f = Fixture::new();
        let store = f.store();
        let sessions = ScanSessions::default();
        let preview = sessions.preview(&store, &f.root()).unwrap();
        let second = sessions.preview(&store, &f.root()).unwrap();
        let a = preview.preview.candidates[0].candidate_id.clone();
        let other = second.preview.candidates[1].candidate_id.clone();
        changed(sessions.import(&store, &preview.scan_id, &[a.clone(), "invented".into()]));
        changed(sessions.import(&store, &preview.scan_id, &[a.clone(), other]));
        changed(sessions.import(&store, "invented", std::slice::from_ref(&a)));
        assert!(!f.0.join("home/library.json").exists());
        let result = sessions.import(&store, &preview.scan_id, &[a]).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].display_name, "a");
    }

    #[test]
    fn scan_sessions_reject_expired_ids_and_evict_eldest_at_capacity() {
        let f = Fixture::new();
        let store = f.store();
        let sessions = ScanSessions::default();
        let now = Instant::now();
        let wall_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let preview_at = |time| {
            sessions
                .publish(store.scan_preview(&f.root()).unwrap(), time)
                .unwrap()
        };
        let first = preview_at(now);
        let selected = [first.preview.candidates[0].candidate_id.clone()];
        assert!(first.expires_unix >= wall_time + 600);
        changed(sessions.import_at(
            &store,
            &first.scan_id,
            &selected,
            now + Duration::from_secs(600),
        ));
        let eldest = preview_at(now);
        let mut latest = preview_at(now);
        for i in 2..=16 {
            latest = preview_at(now + Duration::from_millis(i));
        }
        changed(sessions.import_at(
            &store,
            &eldest.scan_id,
            &[eldest.preview.candidates[0].candidate_id.clone()],
            now + Duration::from_secs(1),
        ));
        assert!(!f.0.join("home/library.json").exists());
        let result = sessions
            .import_at(
                &store,
                &latest.scan_id,
                &[latest.preview.candidates[0].candidate_id.clone()],
                now + Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(result.entries.len(), 1);
    }
}
