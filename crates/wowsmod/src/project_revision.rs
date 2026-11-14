//! Complete, portable source manifests for explicit detail/capture operations.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const SOURCE_SCOPE_VERSION: u32 = 1;
pub const MAX_SOURCE_DEPTH: usize = 64;
pub const MAX_SOURCE_FILES: usize = 100_000;
pub const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024 * 1024;
pub const EXCLUDED_ROOTS: &[&str] = &[
    ".git",
    ".wowsmod",
    ".install",
    ".render",
    "renders",
    "out",
    "target",
    "node_modules",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceManifest {
    pub scope_version: u32,
    pub revision: String,
    pub files: Vec<SourceFile>,
    pub total_bytes: u64,
}
#[derive(Debug)]
pub struct SourceError {
    pub code: &'static str,
    pub message: String,
}
impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for SourceError {}
pub(crate) fn failure(code: &'static str, message: impl Into<String>) -> anyhow::Error {
    SourceError {
        code,
        message: message.into(),
    }
    .into()
}
pub fn error_code(error: &anyhow::Error) -> Option<&'static str> {
    error.downcast_ref::<SourceError>().map(|e| e.code)
}

pub(crate) fn windows_sharing_contention(error: &std::io::Error) -> bool {
    #[cfg(windows)]
    {
        matches!(error.raw_os_error(), Some(5 | 32))
    }
    #[cfg(not(windows))]
    {
        let _ = error;
        false
    }
}
pub(crate) fn lock_contention(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::AlreadyExists || windows_sharing_contention(error)
}
pub(crate) fn retry_io<T>(
    timeout: std::time::Duration,
    retryable: impl Fn(&std::io::Error) -> bool,
    operation: impl FnMut() -> std::io::Result<T>,
) -> std::io::Result<T> {
    let started = std::time::Instant::now();
    retry_io_with_clock(
        timeout,
        retryable,
        operation,
        || started.elapsed(),
        std::thread::sleep,
    )
}
fn retry_io_with_clock<T>(
    timeout: std::time::Duration,
    retryable: impl Fn(&std::io::Error) -> bool,
    mut operation: impl FnMut() -> std::io::Result<T>,
    mut elapsed: impl FnMut() -> std::time::Duration,
    mut wait: impl FnMut(std::time::Duration),
) -> std::io::Result<T> {
    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) => {
                if !retryable(&error) {
                    return Err(error);
                }
                let remaining = timeout.saturating_sub(elapsed());
                if remaining.is_zero() {
                    return Err(error);
                }
                wait(remaining.min(std::time::Duration::from_millis(20)));
                // A scheduler delay must not permit another filesystem call
                // after the original deadline. Preserve the last OS error.
                if elapsed() >= timeout {
                    return Err(error);
                }
            }
        }
    }
}
/// Explicit captures only: never invoke this from the catalog listing path.
pub fn read_source_manifest(root: &Path) -> Result<SourceManifest> {
    let guard = SafeDirectory::open(root)?;
    let captured = capture(&guard.path, true)?;
    finish_capture(&guard.path, captured)
}

/// Validate an untrusted persisted manifest before copying/restoring any object.
pub fn validate_source_manifest(manifest: &SourceManifest) -> Result<()> {
    if manifest.scope_version != SOURCE_SCOPE_VERSION {
        return Err(failure("unsafe_source", "Unsupported source scope version"));
    }
    let mut paths = PathRegistry::default();
    let mut total = 0u64;
    let mut previous: Option<&str> = None;
    for f in &manifest.files {
        validate_source_path(&f.path)?;
        if previous.is_some_and(|p| p >= f.path.as_str()) {
            return Err(failure(
                "unsafe_source",
                "Manifest paths must be unique and sorted",
            ));
        }
        previous = Some(&f.path);
        paths.file(&f.path)?;
        if !is_sha256(&f.sha256) {
            return Err(failure(
                "unsafe_source",
                format!("Invalid SHA-256 for {}", f.path),
            ));
        }
        total = total
            .checked_add(f.bytes)
            .ok_or_else(|| failure("snapshot_limit", "Source byte count overflow"))?;
    }
    check_limits(manifest.files.len(), total)?;
    if !manifest.files.iter().any(|f| f.path == "mod.toml") {
        return Err(failure(
            "unsafe_source",
            "Source must contain ordinary file mod.toml",
        ));
    }
    if total != manifest.total_bytes || revision(&manifest.files)? != manifest.revision {
        return Err(failure(
            "unsafe_source",
            "Source manifest digest or byte count does not match",
        ));
    }
    Ok(())
}

/// Slash-separated portable paths. Denies traversal, ADS, devices, and Windows aliases.
pub fn validate_source_path(path: &str) -> Result<()> {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') {
        return Err(failure(
            "unsafe_source",
            format!("Invalid source path: {path:?}"),
        ));
    }
    let components: Vec<_> = path.split('/').collect();
    if components.len() > MAX_SOURCE_DEPTH {
        return Err(failure(
            "snapshot_limit",
            format!("Source depth exceeds {MAX_SOURCE_DEPTH}: {path}"),
        ));
    }
    for component in &components {
        let basename = component
            .split('.')
            .next()
            .unwrap_or_default()
            .to_uppercase();
        let device = matches!(
            basename.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
        ) || ["COM", "LPT"].iter().any(|prefix| {
            basename.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        });
        if component.is_empty()
            || matches!(*component, "." | "..")
            || component.ends_with(['.', ' '])
            || device
            || component
                .chars()
                .any(|c| c.is_control() || "<>:\"|?*".contains(c))
        {
            return Err(failure(
                "unsafe_source",
                format!("Nonportable or unsafe source path: {path:?}"),
            ));
        }
    }
    if excluded(components[0]) {
        return Err(failure(
            "unsafe_source",
            format!("Excluded root in manifest: {path}"),
        ));
    }
    Ok(())
}
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn safe_project_key(project_id: &str) -> String {
    sha256_hex(project_id.as_bytes())
}

pub fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn is_revision(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(is_sha256)
}
fn excluded(name: &str) -> bool {
    EXCLUDED_ROOTS.iter().any(|n| n.eq_ignore_ascii_case(name))
}
fn revision(files: &[SourceFile]) -> Result<String> {
    #[derive(Serialize)]
    struct Canonical<'a> {
        scope_version: u32,
        files: &'a [SourceFile],
    }
    let mut hash = Sha256::new();
    hash.update(b"wowsmod-source-manifest\0schema:1\0");
    hash.update(serde_json::to_vec(&Canonical {
        scope_version: SOURCE_SCOPE_VERSION,
        files,
    })?);
    Ok(format!("sha256:{:x}", hash.finalize()))
}
fn check_limits(count: usize, bytes: u64) -> Result<()> {
    // Reaching a limit is an error, rather than silently publishing a prefix.
    if count >= MAX_SOURCE_FILES || bytes >= MAX_SOURCE_BYTES {
        return Err(failure("snapshot_limit", format!("Source reaches snapshot limit ({count} files, {bytes} bytes); reduce source below {MAX_SOURCE_FILES} files and {MAX_SOURCE_BYTES} bytes")));
    }
    Ok(())
}

#[derive(Default)]
struct PathRegistry(BTreeMap<String, (String, bool)>);
impl PathRegistry {
    fn insert(&mut self, path: &str, directory: bool) -> Result<()> {
        let key = path.to_uppercase();
        if let Some((old, was_dir)) = self.0.get(&key) {
            if old != path || *was_dir != directory || !directory {
                return Err(failure(
                    "unsafe_source",
                    format!("Conflicting source paths: {old} / {path}"),
                ));
            }
        } else {
            self.0.insert(key, (path.into(), directory));
        }
        Ok(())
    }
    fn file(&mut self, path: &str) -> Result<()> {
        for (i, _) in path.match_indices('/') {
            self.insert(&path[..i], true)?;
        }
        self.insert(path, false)
    }
}

pub fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
fn io_error(path: &Path, error: std::io::Error) -> anyhow::Error {
    failure(
        "unsafe_source",
        format!("Cannot read complete source {}: {error}", path.display()),
    )
}

/// Keep this guard alive while using `path`. On Windows ancestor handles deny
/// delete sharing, preventing junction/rename substitutions during a capture.
pub struct SafeDirectory {
    pub path: PathBuf,
    _ancestors: Vec<File>,
}
impl SafeDirectory {
    /// Identity of the held directory, not a fresh lookup of its pathname.
    pub fn identity(&self) -> Result<(u64, u64)> {
        let directory = self
            ._ancestors
            .last()
            .ok_or_else(|| failure("unsafe_source", "Missing held directory"))?;
        Ok(stamp(directory, &self.path)?.identity)
    }
    /// Create missing directories one level at a time while retaining every
    /// ancestor handle. Never follow a concurrently inserted reparse point.
    pub fn create(path: &Path) -> Result<Self> {
        Self::open_impl(path, true)
    }
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_impl(path, false)
    }
    fn open_impl(path: &Path, create: bool) -> Result<Self> {
        let path = std::path::absolute(path).map_err(|e| io_error(path, e))?;
        let mut ancestors: Vec<_> = path
            .ancestors()
            .filter(|p| !p.as_os_str().is_empty())
            .collect();
        ancestors.reverse();
        let mut handles = Vec::new();
        for ancestor in ancestors {
            if create {
                match fs::symlink_metadata(ancestor) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        match fs::create_dir(ancestor) {
                            Ok(()) => (),
                            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                            Err(e) => return Err(io_error(ancestor, e)),
                        }
                    }
                    Err(e) => return Err(io_error(ancestor, e)),
                    Ok(_) => (),
                }
            }
            handles.push(open_plain(ancestor, true)?);
        }
        Ok(Self {
            path,
            _ancestors: handles,
        })
    }
}

pub(crate) fn open_plain(path: &Path, directory: bool) -> Result<File> {
    let before = fs::symlink_metadata(path).map_err(|e| io_error(path, e))?;
    if is_link(&before) || (directory && !before.is_dir()) || (!directory && !before.is_file()) {
        return Err(failure(
            "unsafe_source",
            format!(
                "Links, reparse points, and special files are not source: {}",
                path.display()
            ),
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000 | if directory { 0x02000000 } else { 0 }); // OPEN_REPARSE_POINT | BACKUP_SEMANTICS
        options.share_mode(if directory { 1 | 2 } else { 1 }); // no delete sharing; ordinary files also deny writes
        if directory {
            // Desired access 0 creates a metadata-only handle that does not
            // enforce delete sharing. LIST_DIRECTORY is necessary to pin it.
            options.access_mode(0x1 | 0x80); // FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000 | if directory { 0x10000 } else { 0 });
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x100 | if directory { 0x100000 } else { 0 });
    }
    let file = options.open(path).map_err(|e| io_error(path, e))?;
    let after = file.metadata().map_err(|e| io_error(path, e))?;
    if is_link(&after) || (directory && !after.is_dir()) || (!directory && !after.is_file()) {
        return Err(failure(
            "unsafe_source",
            format!(
                "Source was replaced by a link or special file: {}",
                path.display()
            ),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != after.dev() || before.ino() != after.ino() {
            return Err(failure(
                "source_changed",
                format!("Source changed while opening: {}", path.display()),
            ));
        }
    }
    Ok(file)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    identity: (u64, u64),
    len: u64,
    modified: i128,
    changed: i128,
    directory: bool,
}
fn stamp(file: &File, path: &Path) -> Result<Stamp> {
    let m = file.metadata().map_err(|e| io_error(path, e))?;
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        #[repr(C)]
        struct Time {
            low: u32,
            high: u32,
        }
        #[repr(C)]
        struct Info {
            attributes: u32,
            created: Time,
            accessed: Time,
            modified: Time,
            volume: u32,
            size_high: u32,
            size_low: u32,
            links: u32,
            index_high: u32,
            index_low: u32,
        }
        #[repr(C)]
        struct Basic {
            created: i64,
            accessed: i64,
            modified: i64,
            changed: i64,
            attributes: u32,
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut Info) -> i32;
            fn GetFileInformationByHandleEx(
                handle: *mut std::ffi::c_void,
                class: i32,
                info: *mut std::ffi::c_void,
                size: u32,
            ) -> i32;
        }
        let mut info = std::mem::MaybeUninit::<Info>::zeroed();
        let mut basic = std::mem::MaybeUninit::<Basic>::zeroed();
        let ok = unsafe {
            GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) != 0
                && GetFileInformationByHandleEx(
                    file.as_raw_handle(),
                    0,
                    basic.as_mut_ptr().cast(),
                    std::mem::size_of::<Basic>() as u32,
                ) != 0
        };
        if !ok {
            return Err(io_error(path, std::io::Error::last_os_error()));
        }
        let info = unsafe { info.assume_init() };
        let basic = unsafe { basic.assume_init() };
        if info.attributes & 0x400 != 0 {
            return Err(failure(
                "unsafe_source",
                "Reparse point encountered while reading source",
            ));
        }
        Ok(Stamp {
            identity: (
                info.volume as u64,
                (info.index_high as u64) << 32 | info.index_low as u64,
            ),
            len: m.len(),
            modified: basic.modified as i128,
            changed: basic.changed as i128,
            directory: m.is_dir(),
        })
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(Stamp {
            identity: (m.dev(), m.ino()),
            len: m.len(),
            modified: m.mtime() as i128 * 1_000_000_000 + m.mtime_nsec() as i128,
            changed: m.ctime() as i128 * 1_000_000_000 + m.ctime_nsec() as i128,
            directory: m.is_dir(),
        })
    }
}

#[derive(Default)]
struct Capture {
    stamps: BTreeMap<String, Stamp>,
    files: Vec<SourceFile>,
    total_bytes: u64,
    paths: PathRegistry,
}
fn capture(root: &Path, hash_contents: bool) -> Result<Capture> {
    let mut capture = Capture::default();
    walk(root, "", hash_contents, &mut capture)?;
    if !capture.files.iter().any(|f| f.path == "mod.toml") {
        return Err(failure(
            "unsafe_source",
            "Source must contain ordinary file mod.toml",
        ));
    }
    capture.files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(capture)
}
fn walk(path: &Path, relative: &str, hash_contents: bool, capture: &mut Capture) -> Result<()> {
    let dir = open_plain(path, true)?;
    let mut before = stamp(&dir, path)?;
    // Root times also change when excluded caches are added/removed; identity
    // and the managed children inventory, not these times, guard the root.
    if relative.is_empty() {
        before.len = 0;
        before.modified = 0;
        before.changed = 0;
    }
    capture.stamps.insert(relative.into(), before.clone());
    if capture.stamps.len() >= 200_000 {
        return Err(failure(
            "snapshot_limit",
            "Source reaches 200000 filesystem entries; remove empty directory trees",
        ));
    }
    let entries = fs::read_dir(path).map_err(|e| io_error(path, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| io_error(path, e))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| failure("unsafe_source", "Source contains non-UTF-8 path"))?;
        if relative.is_empty() && excluded(&name) {
            continue;
        }
        let rel = if relative.is_empty() {
            name
        } else {
            format!("{relative}/{name}")
        };
        validate_source_path(&rel)?;
        let p = entry.path();
        let metadata = fs::symlink_metadata(&p).map_err(|e| io_error(&p, e))?;
        if is_link(&metadata) {
            return Err(failure(
                "unsafe_source",
                format!("Source contains a link/reparse point: {rel}"),
            ));
        }
        if metadata.is_dir() {
            capture.paths.insert(&rel, true)?;
            walk(&p, &rel, hash_contents, capture)?;
        } else if metadata.is_file() {
            capture.paths.file(&rel)?;
            let mut file = open_plain(&p, false)?;
            let initial = stamp(&file, &p)?;
            capture.total_bytes = capture
                .total_bytes
                .checked_add(initial.len)
                .ok_or_else(|| failure("snapshot_limit", "Source byte count overflow"))?;
            check_limits(capture.files.len() + 1, capture.total_bytes)?;
            let hash = if hash_contents {
                digest_reader(&mut file, initial.len, &mut std::io::sink())?
            } else {
                String::new()
            };
            if stamp(&file, &p)? != initial {
                return Err(failure(
                    "source_changed",
                    format!("Source file changed while reading: {rel}"),
                ));
            }
            capture.stamps.insert(rel.clone(), initial.clone());
            capture.files.push(SourceFile {
                path: rel,
                bytes: initial.len,
                sha256: hash,
            });
        } else {
            return Err(failure(
                "unsafe_source",
                format!("Source contains a special file: {rel}"),
            ));
        }
    }
    let mut after = stamp(&dir, path)?;
    if relative.is_empty() {
        after.len = 0;
        after.modified = 0;
        after.changed = 0;
    }
    if before != after {
        return Err(failure(
            "source_changed",
            format!("Source directory changed while reading: {}", path.display()),
        ));
    }
    Ok(())
}
fn finish_capture(root: &Path, captured: Capture) -> Result<SourceManifest> {
    let end = capture(root, false).map_err(|e| {
        failure(
            "source_changed",
            format!("Source changed during capture: {e:#}"),
        )
    })?;
    if captured.stamps != end.stamps {
        return Err(failure(
            "source_changed",
            "Source changed during capture; refresh and retry",
        ));
    }
    let manifest = SourceManifest {
        scope_version: SOURCE_SCOPE_VERSION,
        revision: revision(&captured.files)?,
        files: captured.files,
        total_bytes: captured.total_bytes,
    };
    validate_source_manifest(&manifest)?;
    Ok(manifest)
}
fn digest_reader(
    reader: &mut impl Read,
    expected_bytes: u64,
    writer: &mut impl Write,
) -> Result<String> {
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|e| failure("unsafe_source", format!("Cannot read complete source: {e}")))?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        if bytes > expected_bytes {
            return Err(failure("source_changed", "Source file grew during capture"));
        }
        writer.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
    }
    if bytes != expected_bytes {
        return Err(failure(
            "source_changed",
            "Source file shrank during capture",
        ));
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Stream a manifest member into an object/staging file, checking identity,
/// length and SHA-256. The caller must publish only after this returns Ok.
pub fn copy_verified_source_file(
    root: &Path,
    source: &SourceFile,
    writer: &mut impl Write,
) -> Result<()> {
    validate_source_path(&source.path)?;
    if !is_sha256(&source.sha256) {
        return Err(failure("unsafe_source", "Invalid source digest"));
    }
    check_limits(1, source.bytes)?;
    let target = root.join(&source.path);
    let _guard = SafeDirectory::open(
        target
            .parent()
            .ok_or_else(|| failure("unsafe_source", "Missing source parent"))?,
    )?;
    let mut file = open_plain(&target, false)?;
    let before = stamp(&file, &target)?;
    if before.len != source.bytes {
        return Err(failure(
            "source_changed",
            "Source length no longer matches captured manifest",
        ));
    }
    let digest = digest_reader(&mut file, source.bytes, writer)?;
    if digest != source.sha256 || stamp(&file, &target)? != before {
        return Err(failure(
            "source_changed",
            "Source bytes no longer match captured manifest",
        ));
    }
    Ok(())
}

/// Bounded, no-reparse read for small persisted metadata (not source listings).
pub fn read_plain_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>> {
    let _guard = SafeDirectory::open(
        path.parent()
            .ok_or_else(|| failure("unsafe_source", "Missing file parent"))?,
    )?;
    let mut file = open_plain(path, false)?;
    let before = stamp(&file, path)?;
    if before.len > max_bytes {
        return Err(failure(
            "snapshot_limit",
            format!(
                "Metadata file exceeds {max_bytes} bytes: {}",
                path.display()
            ),
        ));
    }
    let mut bytes = Vec::with_capacity(before.len as usize);
    digest_reader(&mut file, before.len, &mut bytes)?;
    if stamp(&file, path)? != before {
        return Err(failure("source_changed", "Metadata changed while reading"));
    }
    Ok(bytes)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);

    #[cfg(windows)]
    #[test]
    fn io_retry_accepts_lock_exists_then_windows_5_and_32_then_success() {
        use std::time::Duration;
        let time = std::cell::Cell::new(Duration::ZERO);
        let mut errors = std::collections::VecDeque::from([
            std::io::Error::from(std::io::ErrorKind::AlreadyExists),
            std::io::Error::from_raw_os_error(5),
            std::io::Error::from_raw_os_error(32),
        ]);
        let mut attempts = 0;
        let value = retry_io_with_clock(
            Duration::from_millis(100),
            lock_contention,
            || {
                attempts += 1;
                match errors.pop_front() {
                    Some(e) => Err(e),
                    None => Ok("acquired"),
                }
            },
            || time.get(),
            |delay| time.set(time.get() + delay),
        )
        .unwrap();
        assert_eq!(value, "acquired");
        assert_eq!(attempts, 4);
        assert_eq!(time.get(), Duration::from_millis(60));
    }
    #[cfg(windows)]
    #[test]
    fn io_retry_permanent_windows_denial_stops_at_deadline_with_original_error() {
        use std::time::Duration;
        let time = std::cell::Cell::new(Duration::ZERO);
        let mut attempts = 0;
        let mut waits = vec![];
        let error = retry_io_with_clock(
            Duration::from_millis(50),
            lock_contention,
            || {
                attempts += 1;
                Err::<(), _>(std::io::Error::from_raw_os_error(5))
            },
            || time.get(),
            |delay| {
                waits.push(delay);
                time.set(time.get() + delay);
            },
        )
        .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(5));
        assert_eq!(attempts, 3);
        assert_eq!(
            waits,
            [
                Duration::from_millis(20),
                Duration::from_millis(20),
                Duration::from_millis(10)
            ]
        );
        assert_eq!(time.get(), Duration::from_millis(50));
    }
    #[test]
    fn io_retry_non_contention_errors_fail_without_waiting() {
        let mut attempts = 0;
        let error = retry_io_with_clock(
            std::time::Duration::from_secs(3),
            lock_contention,
            || {
                attempts += 1;
                Err::<(), _>(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "original-error",
                ))
            },
            || std::time::Duration::ZERO,
            |_| panic!("must not retry permanent errors"),
        )
        .unwrap_err();
        assert_eq!(attempts, 1);
        assert_eq!(error.to_string(), "original-error");
    }
    #[test]
    fn io_retry_never_attempts_after_a_wait_overshoots_its_deadline() {
        use std::time::Duration;
        let time = std::cell::Cell::new(Duration::ZERO);
        let mut attempts = 0;
        let error = retry_io_with_clock(
            Duration::from_millis(50),
            lock_contention,
            || {
                attempts += 1;
                Err::<(), _>(std::io::Error::from(std::io::ErrorKind::AlreadyExists))
            },
            || time.get(),
            |_| time.set(Duration::from_millis(500)),
        )
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(attempts, 1);
    }
    #[test]
    fn atomic_retry_does_not_retry_non_windows_contention() {
        let error = retry_io_with_clock(
            std::time::Duration::from_secs(3),
            windows_sharing_contention,
            || Err::<(), _>(std::io::Error::from(std::io::ErrorKind::AlreadyExists)),
            || std::time::Duration::ZERO,
            |_| panic!("file exists is not atomic-replace sharing contention"),
        )
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    }
    pub(crate) struct Fixture(pub PathBuf);
    impl Fixture {
        pub(crate) fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "wowsmod-source-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
        pub(crate) fn put(&self, path: &str, bytes: &[u8]) {
            let p = self.0.join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, bytes).unwrap();
        }
        pub(crate) fn project() -> Self {
            let f = Self::new();
            f.put("mod.toml", b"[mod]\nname = 'source-test'\n");
            f
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // Each test owns exactly one fresh directory directly beneath temp.
            assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
            assert!(self
                .0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("wowsmod-source-"));
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn revision_is_content_based_and_relocation_independent() {
        let a = Fixture::project();
        let b = Fixture::project();
        for f in [
            "assets/res_mods/ship.dds",
            "edits/001.json",
            "fx.json",
            "design/palette.json",
            "notes/custom.txt",
        ] {
            a.put(f, b"same");
            b.put(f, b"same");
        }
        let first = read_source_manifest(&a.0).unwrap();
        assert_eq!(first, read_source_manifest(&b.0).unwrap());
        assert_eq!(first.files.len(), 6);
        let p = a.0.join("notes/custom.txt");
        let before = fs::metadata(&p).unwrap().modified().unwrap();
        fs::write(&p, b"else").unwrap();
        fs::File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(before))
            .unwrap();
        assert_ne!(first.revision, read_source_manifest(&a.0).unwrap().revision);
    }
    #[test]
    fn excluded_cache_and_install_files_do_not_change_revision() {
        let a = Fixture::project();
        let before = read_source_manifest(&a.0).unwrap();
        for d in EXCLUDED_ROOTS {
            a.put(&format!("{d}/noise"), b"cached");
        }
        assert_eq!(before, read_source_manifest(&a.0).unwrap());
        a.put("design/out/source.txt", b"nested out is source");
        assert_ne!(
            before.revision,
            read_source_manifest(&a.0).unwrap().revision
        );
    }
    #[test]
    fn portable_paths_reject_traversal_ads_and_device_names() {
        for p in [
            "../secret",
            "/absolute",
            "C:/drive",
            "a\\b",
            "a//b",
            "a/./b",
            "file:stream",
            "NUL.txt",
            "a/COM1",
            "a/trailing.",
            "a/space ",
            "a/<bad>",
        ] {
            assert!(validate_source_path(p).is_err(), "{p}");
        }
        validate_source_path("assets/res_mods/船.dds").unwrap();
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[test]
    fn manifest_rejects_tampering_and_case_or_prefix_conflicts() {
        let a = Fixture::project();
        a.put("design/source.txt", b"a");
        let original = read_source_manifest(&a.0).unwrap();
        validate_source_manifest(&original).unwrap();
        let mut bad = original.clone();
        bad.files[0].bytes += 1;
        assert!(validate_source_manifest(&bad).is_err());
        let mut bad = original.clone();
        bad.files.push(SourceFile {
            path: "DESIGN/other.txt".into(),
            bytes: 1,
            sha256: sha256_hex(b"b"),
        });
        assert!(validate_source_manifest(&bad).is_err());
        let mut bad = original.clone();
        bad.files.push(SourceFile {
            path: "design".into(),
            bytes: 0,
            sha256: sha256_hex(b""),
        });
        assert!(validate_source_manifest(&bad).is_err());
    }
    #[test]
    fn missing_manifest_and_unreadable_root_fail_closed() {
        let a = Fixture::new();
        assert_eq!(
            error_code(&read_source_manifest(&a.0).unwrap_err()),
            Some("unsafe_source")
        );
        a.put("a", b"file");
        assert!(read_source_manifest(&a.0.join("a")).is_err());
    }
    #[test]
    fn rejects_case_collisions_and_file_directory_aliases() {
        let mut paths = PathRegistry::default();
        paths.file("design/a.txt").unwrap();
        assert_eq!(
            error_code(&paths.file("Design/b.txt").unwrap_err()),
            Some("unsafe_source")
        );
        assert!(paths.file("design").is_err());
        assert!(paths.file("design/a.txt").is_err());
    }
    #[test]
    fn changed_source_between_read_and_final_inventory_fails_closed() {
        let a = Fixture::project();
        a.put("design/a", b"same");
        let captured = capture(&a.0, true).unwrap();
        let p = a.0.join("design/a");
        let time = fs::metadata(&p).unwrap().modified().unwrap();
        fs::write(&p, b"else").unwrap();
        File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(time))
            .unwrap();
        assert_eq!(
            error_code(&finish_capture(&a.0, captured).unwrap_err()),
            Some("source_changed")
        );
        let captured = capture(&a.0, true).unwrap();
        a.put("new-source", b"new");
        assert_eq!(
            error_code(&finish_capture(&a.0, captured).unwrap_err()),
            Some("source_changed")
        );
        let captured = capture(&a.0, true).unwrap();
        fs::remove_file(a.0.join("new-source")).unwrap();
        assert_eq!(
            error_code(&finish_capture(&a.0, captured).unwrap_err()),
            Some("source_changed")
        );
    }
    #[test]
    fn limit_boundaries_fail_instead_of_truncating() {
        assert!(check_limits(99_999, 16 * 1024 * 1024 * 1024 - 1).is_ok());
        assert_eq!(
            error_code(&check_limits(100_000, 0).unwrap_err()),
            Some("snapshot_limit")
        );
        assert_eq!(
            error_code(&check_limits(1, 16 * 1024 * 1024 * 1024).unwrap_err()),
            Some("snapshot_limit")
        );
        let a = Fixture::project();
        let deep = std::iter::repeat_n("d", 64).collect::<Vec<_>>().join("/");
        a.put(&format!("{deep}/source"), b"deep");
        assert_eq!(
            error_code(&read_source_manifest(&a.0).unwrap_err()),
            Some("snapshot_limit")
        );
    }
    #[test]
    fn verified_copy_checks_hash_and_metadata_read_is_bounded() {
        let a = Fixture::project();
        a.put("design/a", b"frozen bytes");
        let manifest = read_source_manifest(&a.0).unwrap();
        let entry = manifest
            .files
            .iter()
            .find(|f| f.path == "design/a")
            .unwrap();
        let mut out = vec![];
        copy_verified_source_file(&a.0, entry, &mut out).unwrap();
        assert_eq!(out, b"frozen bytes");
        a.put("design/a", b"mutant bytes");
        assert_eq!(
            error_code(&copy_verified_source_file(&a.0, entry, &mut vec![]).unwrap_err()),
            Some("source_changed")
        );
        assert!(read_plain_file(&a.0.join("design/a"), 1).is_err());
        assert_eq!(
            read_plain_file(&a.0.join("design/a"), 1024).unwrap(),
            b"mutant bytes"
        );
    }
    #[cfg(windows)]
    #[test]
    fn unreadable_source_is_not_silently_omitted() {
        use std::os::windows::fs::OpenOptionsExt;
        let a = Fixture::project();
        a.put("locked.bin", b"locked");
        let _exclusive = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(a.0.join("locked.bin"))
            .unwrap();
        assert_eq!(
            error_code(&read_source_manifest(&a.0).unwrap_err()),
            Some("unsafe_source")
        );
    }
    #[test]
    fn links_and_linked_ancestors_are_rejected() {
        let a = Fixture::project();
        let external = Fixture::project();
        let link = a.0.join("linked");
        #[cfg(windows)]
        {
            let output = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(&external.0)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "Cannot create junction fixture: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&external.0, &link).unwrap();
        assert_eq!(
            error_code(&read_source_manifest(&a.0).unwrap_err()),
            Some("unsafe_source")
        );
        assert_eq!(
            error_code(&read_source_manifest(&link).unwrap_err()),
            Some("unsafe_source")
        );
        external.put("nested/mod.toml", b"source");
        assert_eq!(
            error_code(&read_source_manifest(&link.join("nested")).unwrap_err()),
            Some("unsafe_source")
        );
    }

    #[test]
    fn safe_directory_creation_guards_each_new_ancestor() {
        let fixture = Fixture::new();
        let path = fixture.0.join("new/metadata/project");
        let guard = SafeDirectory::create(&path).unwrap();
        assert!(guard.path.is_dir());
        #[cfg(windows)]
        assert!(fs::rename(&guard.path, fixture.0.join("replaced")).is_err());
        drop(guard);
        fs::rename(&path, fixture.0.join("moved")).unwrap();
    }
    #[test]
    fn safe_directory_identity_distinguishes_replacement_at_the_same_path() {
        let fixture = Fixture::new();
        let path = fixture.0.join("project");
        let guard = SafeDirectory::create(&path).unwrap();
        let identity = guard.identity().unwrap();
        assert_eq!(guard.identity().unwrap(), identity);
        drop(guard);
        fs::rename(&path, fixture.0.join("original")).unwrap();
        let replacement = SafeDirectory::create(&path).unwrap();
        assert_ne!(replacement.identity().unwrap(), identity);
    }
}
