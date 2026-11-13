//! Crash-recoverable replacement of a managed ship mod.
use super::*;
use std::io::Write;

const PENDING: &str = "transaction";
const PREPARING: &str = "transaction.preparing";
const FINISHED: &str = "transaction.finished";

// Ship directories use FSB510_Bourgogne while metadata often uses PFSB510.
fn ship_key(ship: &str) -> String {
    let upper = ship.trim().to_ascii_uppercase();
    let bare = upper.strip_prefix('P').unwrap_or(&upper);
    if bare.len() >= 6 && bare.as_bytes()[..3].iter().all(u8::is_ascii_alphabetic)
        && bare.as_bytes()[3..6].iter().all(u8::is_ascii_digit)
        && (bare.len() == 6 || bare.as_bytes()[6] == b'_') {
        bare[..6].into()
    } else { upper }
}

fn project_key(path: &Path) -> String {
    let p = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.trim_start_matches("//?/").trim_end_matches('/').to_string();
    if cfg!(windows) { s.to_lowercase() } else { s }
}

/// Reject links/junctions and directory-as-file blockers, including in metadata.
pub(super) fn guard_path(root: &Path, rel: &str) -> Result<()> {
    let mut path = root.to_path_buf();
    for component in std::iter::once("").chain(rel.split('/')) {
        if !component.is_empty() { path.push(component); }
        match std::fs::symlink_metadata(&path) {
            Ok(meta) => {
                #[cfg(windows)]
                let linked = {
                    use std::os::windows::fs::MetadataExt;
                    meta.file_attributes() & 0x400 != 0
                };
                #[cfg(not(windows))]
                let linked = meta.file_type().is_symlink();
                if linked { bail!("拒绝经过符号链接/联接点写入: {}", path.display()); }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("检查路径 {}", path.display())),
        }
    }
    Ok(())
}

fn read_optional(root: &Path, rel: &str) -> Result<Option<Vec<u8>>> {
    guard_path(root, rel)?;
    match std::fs::read(root.join(rel)) {
        Ok(data) => Ok(Some(data)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("读取 {}", root.join(rel).display())),
    }
}

fn backup_rel(rel: &str) -> String { format!("{DIR}/backup/{rel}") }

struct Plan {
    writes: BTreeMap<String, Option<Vec<u8>>>,
    expected: BTreeMap<String, Option<String>>,
    result: Value,
}

fn plan(res_mods: &Path, info: &ModInfo, items: &[InstallItem], dry_run: bool) -> Result<Plan> {
    let registry_rel = format!("{DIR}/registry.json");
    let registry_bytes = read_optional(res_mods, &registry_rel)?;
    let old: Registry = match &registry_bytes {
        Some(bytes) => serde_json::from_slice(bytes).context("登记表损坏, 未应用")?,
        None => Registry { format: 1, ..Default::default() },
    };
    let mut expected = BTreeMap::from([(registry_rel, hash_option(&registry_bytes))]);
    let key = ship_key(&info.ship);
    let mut replace = BTreeSet::new();
    if info.mode == "sdk" && !key.is_empty() {
        for (name, entry) in &old.mods {
            if entry.mode == "sdk" && ship_key(&entry.ship) == key { replace.insert(name.clone()); }
        }
    }
    if let Some(prior) = old.mods.get(&info.name) {
        let same_project = prior.mode == info.mode && ship_key(&prior.ship) == key
            && project_key(Path::new(&prior.project_dir)) == project_key(&info.project_dir);
        if !replace.contains(&info.name) && !same_project {
            bail!("同名 mod {} 属于另一条船/项目, 不会替换: {}", info.name, prior.project_dir);
        }
        replace.insert(info.name.clone());
    }
    let mut candidates = BTreeMap::new();
    for item in items {
        safe_rel(&item.rel)?;
        guard_path(res_mods, &item.rel)?;
        if candidates.insert(item.rel.to_lowercase(), item).is_some() {
            bail!("候选含大小写重复的路径: {}", item.rel);
        }
    }
    let touched: BTreeSet<String> = old.paths.iter().filter(|(_, pe)| pe.owners.iter().any(|o| replace.contains(o)))
        .map(|(key, _)| key.clone()).chain(candidates.keys().cloned()).collect();
    // Verify every disappearing file as well as all shared candidate paths.
    // Missing files and changed backups are blockers, not permission to overwrite.
    for key in &touched {
        if let Some(pe) = old.paths.get(key) {
            safe_rel(&pe.rel)?;
            if pe.rel.to_lowercase() != *key { bail!("登记表路径键不一致: {key}"); }
            let current = read_optional(res_mods, &pe.rel)?;
            if current.as_ref().map(|bytes| sha256_hex(bytes)) != Some(pe.sha256.clone()) {
                bail!("已安装文件被改动或缺失, 未切换: {}。请先保留/还原用户改动", pe.rel);
            }
            expected.insert(pe.rel.clone(), hash_option(&current));
            if let Some(hash) = &pe.backup_sha256 {
                let backup = backup_rel(&pe.rel);
                let current = read_optional(res_mods, &backup)?;
                if hash_option(&current).as_ref() != Some(hash) {
                    bail!("原文件备份损坏或缺失, 未切换: {}", pe.rel);
                }
                expected.insert(backup, hash_option(&current));
            }
        }
    }
    // Reject corrupt ownership before making a snapshot or writing anything.
    for name in &replace {
        for key in &old.mods[name].files {
            if !old.paths.get(key).map(|pe| pe.owners.contains(name)).unwrap_or(false) {
                bail!("登记表归属缺失: {name}: {key}");
            }
        }
    }
    let previous: Vec<_> = replace.iter().map(|name| old.mods[name].clone()).collect();
    let mut reg = old.clone();
    let mut writes = BTreeMap::new();
    for name in &replace { reg.mods.remove(name); }
    for key in &touched {
        let Some(pe) = old.paths.get(key) else { continue };
        let remaining: Vec<_> = pe.owners.iter().filter(|name| !replace.contains(*name)).cloned().collect();
        if remaining.is_empty() {
            reg.paths.remove(key);
            if !candidates.contains_key(key) {
                let restore = if pe.backup_sha256.is_some() {
                    let rel = backup_rel(&pe.rel);
                    let bytes = read_optional(res_mods, &rel)?;
                    writes.insert(rel, None);
                    bytes
                } else { None };
                writes.insert(pe.rel.clone(), restore);
            }
        } else {
            reg.paths.get_mut(key).unwrap().owners = remaining;
        }
    }
    let mut files = Vec::new();
    let mut shared = 0;
    let mut foreign = Vec::new();
    let mut skipped = 0;
    let mut bytes = 0;
    let mut written = 0;
    for (key, it) in candidates {
        let hash = sha256_hex(&it.data);
        if let Some(pe) = reg.paths.get_mut(&key) {
            if pe.sha256 != hash { bail!("与其他 mod 文件冲突: {} ↔ {}", it.rel, pe.owners.join(", ")); }
            pe.owners.push(info.name.clone());
            files.push(key);
            shared += 1;
            continue;
        }
        let prior = old.paths.get(&key);
        // Keep the original spelling so case-insensitive ownership also works
        // when tests or a game directory live on a case-sensitive filesystem.
        let rel = prior.map(|pe| pe.rel.as_str()).unwrap_or(&it.rel);
        let current = read_optional(res_mods, rel)?;
        // Do not replace an installed hash expectation with a later disk read.
        expected.entry(rel.to_string()).or_insert_with(|| hash_option(&current));
        if prior.is_none() && it.origin == "generated" && current.is_some() { skipped += 1; continue; }
        let backup_hash = if let Some(pe) = prior { pe.backup_sha256.clone() }
        else if let Some(original) = current {
            let backup = backup_rel(rel);
            if read_optional(res_mods, &backup)?.is_some() { bail!("未登记的原件备份已存在, 请先检查: {backup}"); }
            expected.insert(backup.clone(), None);
            let hash = sha256_hex(&original);
            writes.insert(backup, Some(original));
            foreign.push(rel.to_string());
            Some(hash)
        } else { None };
        writes.insert(rel.to_string(), Some(it.data.clone()));
        written += 1;
        bytes += it.data.len();
        let mut parent = Path::new(rel).parent();
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            let rel = path.to_string_lossy().replace('\\', "/");
            if !res_mods.join(path).exists() { reg.dirs.insert(rel.to_lowercase(), rel); }
            parent = path.parent();
        }
        reg.paths.insert(key.clone(), PathEntry { rel: rel.into(), sha256: hash, size: it.data.len() as u64,
            owners: vec![info.name.clone()], backup_sha256: backup_hash, origin: it.origin.clone() });
        files.push(key);
    }
    let entry = ModEntry { name: info.name.clone(), version: info.version.clone(), ship: info.ship.clone(), mode: info.mode.clone(),
        project_dir: info.project_dir.display().to_string(), source: info.source.clone(), game_build: info.game_build.clone(), installed_unix: now(), files };
    let result = json!({"ok": true, "dry_run": dry_run, "mod": info.name, "previous": previous, "selected": entry,
        "res_mods": res_mods.display().to_string(), "files": entry.files.len(), "written": written, "bytes_written": bytes,
        "shared_with_other_mods": shared, "overwrote_foreign": foreign, "skipped_existing_loader": skipped,
        "warnings": [], "registry": reg_dir(res_mods).join("registry.json").display().to_string()});
    reg.mods.insert(info.name.clone(), entry);
    writes.insert(format!("{DIR}/registry.json"), Some(serde_json::to_vec_pretty(&reg)?));
    Ok(Plan { writes, expected, result })
}

#[derive(Serialize, Deserialize)]
struct Blob { file: String, sha256: String }
#[derive(Serialize, Deserialize)]
struct Change { rel: String, before: Option<Blob>, after: Option<Blob> }
#[derive(Serialize, Deserialize)]
struct Journal { format: u32, changes: Vec<Change>, created_dirs: Vec<String> }

fn sync_write(path: &Path, data: &[u8]) -> Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}

fn store_blob(dir: &Path, file: String, bytes: Option<&[u8]>) -> Result<Option<Blob>> {
    match bytes {
        Some(bytes) => { sync_write(&dir.join(&file), bytes)?; Ok(Some(Blob { file, sha256: sha256_hex(bytes) })) }
        None => Ok(None),
    }
}

fn read_blob(dir: &Path, blob: &Option<Blob>) -> Result<Option<Vec<u8>>> {
    let Some(blob) = blob else { return Ok(None) };
    if blob.file.contains(['/', '\\', ':']) || !blob.file.ends_with(".bin") { bail!("恢复快照路径无效"); }
    guard_path(dir, &blob.file)?;
    let data = std::fs::read(dir.join(&blob.file))?;
    if sha256_hex(&data) != blob.sha256 { bail!("恢复快照哈希不符: {}", blob.file); }
    Ok(Some(data))
}

fn internal_rel(rel: &str) -> Result<()> {
    if rel == format!("{DIR}/registry.json") { return Ok(()) }
    safe_rel(rel.strip_prefix(&format!("{DIR}/backup/")).unwrap_or(rel))
}

fn prepare(root: &Path, plan: &Plan) -> Result<Journal> {
    let staging = reg_dir(root).join(PREPARING);
    std::fs::create_dir(&staging).context("创建事务快照暂存目录")?;
    let result = (|| {
        let mut journal = Journal { format: 1, changes: vec![], created_dirs: vec![] };
        let mut dirs = BTreeSet::new();
        for (index, (rel, after)) in plan.writes.iter().enumerate() {
            internal_rel(rel)?;
            let before = read_optional(root, rel)?;
            if plan.expected.get(rel) != Some(&hash_option(&before)) {
                bail!("检查后文件被改动, 快照未提交: {rel}");
            }
            let mut parent = Path::new(rel).parent();
            while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
                let rel = path.to_string_lossy().replace('\\', "/");
                guard_path(root, &rel)?;
                if !root.join(path).exists() { dirs.insert(rel); }
                else if !root.join(path).is_dir() { bail!("文件挡住目标目录: {}", root.join(path).display()); }
                parent = path.parent();
            }
            journal.changes.push(Change { rel: rel.clone(),
                before: store_blob(&staging, format!("{index}-before.bin"), before.as_deref())?,
                after: store_blob(&staging, format!("{index}-after.bin"), after.as_deref())? });
        }
        journal.created_dirs = dirs.into_iter().collect();
        for (rel, expected) in &plan.expected {
            if &hash_option(&read_optional(root, rel)?) != expected { bail!("快照期间文件被改动, 未应用: {rel}"); }
        }
        sync_write(&staging.join("journal.json"), &serde_json::to_vec(&journal)?)?;
        std::fs::rename(&staging, reg_dir(root).join(PENDING))?;
        Ok(journal)
    })();
    if result.is_err() { let _ = std::fs::remove_dir_all(&staging); }
    result
}

fn hash_option(data: &Option<Vec<u8>>) -> Option<String> { data.as_ref().map(|bytes| sha256_hex(bytes)) }
fn blob_hash(blob: &Option<Blob>) -> Option<String> { blob.as_ref().map(|blob| blob.sha256.clone()) }

fn put(root: &Path, dir: &Path, rel: &str, bytes: Option<&[u8]>) -> Result<()> {
    guard_path(root, rel)?;
    let dst = root.join(rel);
    if let Some(bytes) = bytes {
        std::fs::create_dir_all(dst.parent().unwrap())?;
        let staging = dir.join("write.tmp");
        sync_write(&staging, bytes)?;
        // Same-volume rename replaces the destination atomically on Windows and
        // Unix. No remove-then-rename window and no temp file in user directories.
        std::fs::rename(staging, &dst).with_context(|| format!("替换 {}(先退出游戏)", dst.display()))?;
    } else {
        match std::fs::remove_file(&dst) {
            Ok(()) => {}, Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}, Err(e) => return Err(e.into()),
        }
    }
    if hash_option(&read_optional(root, rel)?) != bytes.map(sha256_hex) { bail!("事务写后校验失败: {rel}"); }
    Ok(())
}

pub(super) fn reject_pending(root: &Path) -> Result<()> {
    if reg_dir(root).join(PENDING).exists() || reg_dir(root).join(PREPARING).exists() {
        bail!("存在待恢复事务: {}。演练不会写入; 请先恢复后重试", reg_dir(root).join(PENDING).display());
    }
    Ok(())
}

pub(super) fn recover_locked(root: &Path) -> Result<Value> {
    let dir = reg_dir(root).join(PENDING);
    let retired = reg_dir(root).join(FINISHED);
    guard_path(root, &format!("{DIR}/{FINISHED}"))?;
    if retired.exists() { std::fs::remove_dir_all(&retired).context("清理已完成事务快照")?; }
    guard_path(root, &format!("{DIR}/{PENDING}"))?;
    if !dir.exists() {
        let staging = reg_dir(root).join(PREPARING);
        guard_path(root, &format!("{DIR}/{PREPARING}"))?;
        if staging.exists() {
            // No target writes happen until the complete snapshot is published.
            std::fs::remove_dir_all(staging)?;
            return Ok(json!({"recovered": true, "action": "discarded_unpublished_snapshot"}));
        }
        return Ok(json!({"recovered": false}));
    }
    let restore = (|| -> Result<Value> {
        guard_path(&dir, "journal.json")?;
        let raw = std::fs::read(dir.join("journal.json"))?;
        let journal: Journal = serde_json::from_slice(&raw)?;
        if journal.format != 1 { bail!("未知恢复日志格式 {}", journal.format); }
        let marker = read_optional(&dir, "committed")?;
        if let Some(marker) = &marker {
            if marker != sha256_hex(&raw).as_bytes() { bail!("事务提交标记损坏"); }
        }
        // Validate the *entire* recovery set before restoring the first file.
        let mut snapshots = Vec::new();
        let mut seen = BTreeSet::new();
        for change in &journal.changes {
            internal_rel(&change.rel)?;
            if !seen.insert(change.rel.to_lowercase()) { bail!("恢复日志含重复目标"); }
            let before = read_blob(&dir, &change.before)?;
            let after = read_blob(&dir, &change.after)?;
            let current = hash_option(&read_optional(root, &change.rel)?);
            if current != blob_hash(&change.before) && current != blob_hash(&change.after) {
                bail!("恢复被外部改动阻止: {}。快照已保留, 不会覆盖用户内容", change.rel);
            }
            if marker.is_some() && current != blob_hash(&change.after) {
                bail!("已提交事务的文件又被改动: {}", change.rel);
            }
            snapshots.push((change.rel.clone(), before, after));
        }
        for rel in &journal.created_dirs { internal_rel_dir(rel)?; guard_path(root, rel)?; }
        if marker.is_none() {
            for (rel, before, _) in &snapshots {
                if hash_option(&read_optional(root, rel)?) != hash_option(before) {
                    put(root, &dir, rel, before.as_deref())?;
                }
            }
            let mut dirs = journal.created_dirs;
            dirs.sort_by_key(|rel| std::cmp::Reverse(rel.len()));
            for rel in dirs {
                let path = root.join(&rel);
                if path.is_dir() && std::fs::read_dir(&path)?.next().is_none() { std::fs::remove_dir(path)?; }
            }
        }
        // Publish completion before deleting snapshots. A process exit during
        // recursive cleanup must not leave a half-deleted *pending* journal.
        std::fs::rename(&dir, &retired)?;
        #[cfg(test)] tests::fault("after_retire")?;
        std::fs::remove_dir_all(&retired)?;
        Ok(json!({"recovered": true, "action": if marker.is_some() { "finalized_committed" } else { "restored_prestate" }}))
    })();
    restore.with_context(|| format!("待恢复事务仍保留在 {}; 后续写入已停止", dir.display()))
}

fn internal_rel_dir(rel: &str) -> Result<()> {
    if rel == format!("{DIR}/backup") { return Ok(()) }
    internal_rel(rel)
}

pub fn switch(res_mods: &Path, info: &ModInfo, items: &[InstallItem], dry_run: bool) -> Result<Value> {
    if !res_mods.is_dir() { bail!("res_mods 目录不存在: {}", res_mods.display()); }
    if dry_run {
        reject_pending(res_mods)?;
        if reg_dir(res_mods).join("lock").exists() { bail!("另一个操作持有安装锁; 演练已停止"); }
        return Ok(plan(res_mods, info, items, true)?.result);
    }
    let _lock = Lock::take(res_mods)?;
    let recovered = recover_locked(res_mods)?;
    let mut plan = plan(res_mods, info, items, false)?;
    let journal = prepare(res_mods, &plan)?;
    let dir = reg_dir(res_mods).join(PENDING);
    let apply = (|| -> Result<()> {
        // Registry is always last. Until committed is durable, all changes are
        // rollback candidates, even if registry.json already contains B.
        for change in journal.changes.iter().filter(|c| c.rel != format!("{DIR}/registry.json"))
            .chain(journal.changes.iter().filter(|c| c.rel == format!("{DIR}/registry.json"))) {
            if hash_option(&read_optional(res_mods, &change.rel)?) != blob_hash(&change.before) {
                bail!("快照后文件被改动: {}", change.rel);
            }
            let after = read_blob(&dir, &change.after)?;
            put(res_mods, &dir, &change.rel, after.as_deref())?;
            #[cfg(test)] tests::fault(if change.rel == format!("{DIR}/registry.json") { "after_registry" } else { "after_file" })?;
        }
        let raw = std::fs::read(dir.join("journal.json"))?;
        sync_write(&dir.join("commit.tmp"), sha256_hex(&raw).as_bytes())?;
        std::fs::rename(dir.join("commit.tmp"), dir.join("committed"))?;
        Ok(())
    })();
    if let Err(error) = apply {
        return match recover_locked(res_mods) {
            Ok(recovery) => Err(error.context(format!("应用失败; 已回滚: {recovery}"))),
            Err(recovery) => Err(error.context(format!("应用失败且恢复被阻止: {recovery:#}"))),
        };
    }
    #[cfg(test)] tests::fault("after_commit")?;
    // The committed state is authoritative. Cleanup failure is not an apply
    // failure; the next mutation finalizes it under the same lock protocol.
    if let Err(error) = recover_locked(res_mods) {
        plan.result["warnings"] = json!([format!("应用已提交, 事务清理待重试: {error:#}")]);
    }
    plan.result["recovery"] = recovered;
    Ok(plan.result)
}

pub fn recover_pending(res_mods: &Path) -> Result<Value> {
    if !res_mods.is_dir() { bail!("res_mods 目录不存在: {}", res_mods.display()); }
    let _lock = Lock::take(res_mods)?;
    recover_locked(res_mods)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!("wowsmod_switch_{}_{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn put(&self, rel: &str, bytes: &[u8]) {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
    fn item(rel: &str, data: &[u8]) -> InstallItem {
        InstallItem { rel: rel.into(), data: data.to_vec(), origin: "project".into() }
    }
    fn info(name: &str, ship: &str) -> ModInfo {
        ModInfo { name: name.into(), version: "1".into(), ship: ship.into(), mode: "sdk".into(), project_dir: PathBuf::from(format!("/deleted-project/{name}")), source: "project".into(), game_build: "123".into() }
    }
    fn snap(root: &Path) -> BTreeMap<String, Vec<u8>> {
        fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                if path.is_dir() { out.insert(format!("{rel}/"), vec![]); walk(root, &path, out); }
                else { out.insert(rel, std::fs::read(path).unwrap()); }
            }
        }
        let mut result = BTreeMap::new(); walk(root, root, &mut result); result
    }
    const O: Opts = Opts { dry_run: false, allow_same_ship: false };

    thread_local! { static FAULT: std::cell::RefCell<Option<(String, bool)>> = const { std::cell::RefCell::new(None) }; }
    pub(super) fn fault(point: &str) -> Result<()> {
        FAULT.with(|cell| {
            if cell.borrow().as_ref().map(|(name, _)| name.as_str()) == Some(point) {
                let (_, crash) = cell.borrow_mut().take().unwrap();
                if crash { std::process::exit(77); }
                bail!("injected failure: {point}");
            }
            Ok(())
        })
    }
    fn armed(point: &str, crash: bool) { FAULT.with(|cell| *cell.borrow_mut() = Some((point.into(), crash))); }
    fn setup_crash(f: &Fixture) {
        f.put("content/skin", b"foreign original");
        install(&f.0, &info("A", "PFSB510"), &[item("content/skin", b"A"), item("old/a", b"a")], &O).unwrap();
    }
    fn next_items() -> Vec<InstallItem> { vec![item("content/skin", b"B"), item("new/b", b"b")] }
    fn crash_child(root: &Path, point: &str) {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child.args(["--exact", "registry::switch::tests::crash_worker", "--nocapture"])
            .env("WOWSMOD_SWITCH_TEST_ROOT", root).env("WOWSMOD_SWITCH_TEST_POINT", point);
        #[cfg(windows)] { use std::os::windows::process::CommandExt; child.creation_flags(0x0800_0000); }
        let output = child.output().unwrap();
        assert_eq!(output.status.code(), Some(77), "{}", String::from_utf8_lossy(&output.stdout));
    }

    #[test]
    fn crash_worker() {
        let Some(root) = std::env::var_os("WOWSMOD_SWITCH_TEST_ROOT") else { return };
        let point = std::env::var("WOWSMOD_SWITCH_TEST_POINT").unwrap();
        armed(&point, true);
        switch(Path::new(&root), &info("B", "PFSB510"), &next_items(), false).unwrap();
    }

    #[test]
    fn ordinary_errors_restore_every_file_registry_and_backup() {
        for point in ["after_file", "after_registry"] {
            let f = Fixture::new(); setup_crash(&f);
            let before = snap(&f.0);
            armed(point, false);
            let err = switch(&f.0, &info("B", "PFSB510"), &next_items(), false).unwrap_err();
            assert!(format!("{err:#}").contains("已回滚"));
            assert_eq!(snap(&f.0), before, "{point}");
            assert_eq!(verify(&f.0).unwrap()["mods"][0]["intact"], true);
        }
    }

    #[test]
    fn process_exit_releases_lock_and_recovery_restores_or_finalizes() {
        for point in ["after_file", "after_registry", "after_commit"] {
            let f = Fixture::new(); setup_crash(&f);
            let before = snap(&f.0);
            crash_child(&f.0, point);
            assert!(reg_dir(&f.0).join(PENDING).is_dir());
            let pending = snap(&f.0);
            assert!(switch(&f.0, &info("B", "PFSB510"), &next_items(), true).is_err());
            assert_eq!(snap(&f.0), pending, "dry-run must not recover");
            let recovered = recover_pending(&f.0).unwrap();
            assert_eq!(recovered["recovered"], true);
            if point == "after_commit" {
                assert_eq!(recovered["action"], "finalized_committed");
                assert_eq!(std::fs::read(f.0.join("content/skin")).unwrap(), b"B");
                assert_eq!(names(&load(&f.0).unwrap()), BTreeSet::from(["B".into()]));
                uninstall(&f.0, "B", false, false).unwrap();
                assert_eq!(std::fs::read(f.0.join("content/skin")).unwrap(), b"foreign original");
            } else { assert_eq!(snap(&f.0), before, "{point}"); }
        }
    }

    #[test]
    fn recovery_refuses_external_changes_and_keeps_evidence() {
        let f = Fixture::new(); setup_crash(&f);
        crash_child(&f.0, "after_file");
        f.put("content/skin", b"user edit after crash");
        let before = snap(&f.0);
        assert!(recover_pending(&f.0).is_err());
        assert_eq!(snap(&f.0), before);
        assert!(install(&f.0, &info("C", "PJSB001"), &[item("c", b"c")], &O).is_err());
        assert_eq!(snap(&f.0), before);
        assert!(uninstall(&f.0, "A", false, false).is_err());
        assert_eq!(snap(&f.0), before);
    }

    #[test]
    fn legacy_mutations_recover_before_reading_the_registry() {
        let f = Fixture::new(); setup_crash(&f);
        crash_child(&f.0, "after_registry");
        install(&f.0, &info("C", "PJSB001"), &[item("c", b"c")], &O).unwrap();
        assert_eq!(names(&load(&f.0).unwrap()), BTreeSet::from(["A".into(), "C".into()]));
        assert_eq!(std::fs::read(f.0.join("content/skin")).unwrap(), b"A");
        crash_child(&f.0, "after_registry");
        uninstall(&f.0, "A", false, false).unwrap();
        assert_eq!(names(&load(&f.0).unwrap()), BTreeSet::from(["C".into()]));
        assert_eq!(std::fs::read(f.0.join("content/skin")).unwrap(), b"foreign original");
    }

    #[test]
    fn lock_reclaims_abandoned_file_but_blocks_competing_threads() {
        let f = Fixture::new();
        f.put(".wowsmod/lock", b"old abandoned lock");
        let lock = Lock::take(&f.0).unwrap();
        // A still-live operation remains exclusive even if its lock mtime is ancient.
        lock.file.as_ref().unwrap().set_times(std::fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH)).unwrap();
        let root = f.0.clone();
        assert!(std::thread::spawn(move || switch(&root, &info("B", "PFSB510"), &next_items(), false).is_err()).join().unwrap());
        drop(lock);
        assert!(Lock::take(&f.0).is_ok());
    }

    #[test]
    fn rollback_restores_replaced_and_new_foreign_backups() {
        let f = Fixture::new(); setup_crash(&f);
        f.put("foreign/new", b"second foreign original");
        let before = snap(&f.0);
        armed("after_registry", false);
        assert!(switch(&f.0, &info("B", "PFSB510"), &[item("foreign/new", b"B")], false).is_err());
        assert_eq!(snap(&f.0), before);
        assert_eq!(std::fs::read(f.0.join(".wowsmod/backup/content/skin")).unwrap(), b"foreign original");
        assert!(!f.0.join(".wowsmod/backup/foreign/new").exists());
    }

    #[test]
    fn cleanup_interruption_leaves_committed_state_recoverable() {
        let f = Fixture::new(); setup_crash(&f);
        crash_child(&f.0, "after_retire");
        assert!(reg_dir(&f.0).join(FINISHED).exists());
        recover_pending(&f.0).unwrap();
        assert!(!reg_dir(&f.0).join(FINISHED).exists());
        assert_eq!(std::fs::read(f.0.join("content/skin")).unwrap(), b"B");
        assert_eq!(verify(&f.0).unwrap()["mods"][0]["intact"], true);
    }

    #[test]
    fn loose_and_global_candidates_never_evict_other_projects() {
        let f = Fixture::new();
        let mut a = info("A", ""); a.mode = "loose".into();
        install(&f.0, &a, &[item("global", b"A")], &O).unwrap();
        let mut b = info("B", ""); b.mode = "loose".into();
        let before = snap(&f.0);
        assert!(switch(&f.0, &b, &[item("global", b"B")], false).is_err());
        assert_eq!(snap(&f.0), before);
        switch(&f.0, &b, &[item("separate", b"B")], false).unwrap();
        assert_eq!(names(&load(&f.0).unwrap()), BTreeSet::from(["A".into(), "B".into()]));
        a.project_dir = PathBuf::from("/different/project");
        let before = snap(&f.0);
        assert!(switch(&f.0, &a, &[item("global", b"changed")], false).is_err());
        assert_eq!(snap(&f.0), before);
    }

    #[test]
    fn fresh_dry_run_and_unsafe_paths_leave_no_metadata() {
        let f = Fixture::new();
        switch(&f.0, &info("A", "PFSB510"), &[item("deep/path/file", b"A")], true).unwrap();
        assert!(snap(&f.0).is_empty());
        for path in ["../x", ".wowsmod", ".wowsmod/registry.json", "x/CON.txt", "x/alias.", "x/stream:data", "x\\y"] {
            assert!(switch(&f.0, &info("A", "PFSB510"), &[item(path, b"A")], false).is_err(), "{path}");
            assert!(snap(&f.0).is_empty(), "{path}");
        }
    }

    #[test]
    fn user_edit_between_plan_and_snapshot_is_not_accepted_as_prestate() {
        let f = Fixture::new(); setup_crash(&f);
        let planned = plan(&f.0, &info("B", "PFSB510"), &next_items(), false).unwrap();
        f.put("content/skin", b"edit after planning");
        let before = snap(&f.0);
        assert!(prepare(&f.0, &planned).is_err());
        assert_eq!(snap(&f.0), before);
    }

    #[cfg(windows)]
    #[test]
    fn locked_unchanged_file_does_not_block_rollback_of_written_files() {
        use std::os::windows::fs::OpenOptionsExt;
        let f = Fixture::new(); setup_crash(&f);
        let before = snap(&f.0);
        let _open = std::fs::OpenOptions::new().read(true).share_mode(1).open(f.0.join("old/a")).unwrap();
        let error = switch(&f.0, &info("B", "PFSB510"), &next_items(), false).unwrap_err();
        assert!(format!("{error:#}").contains("已回滚"), "{error:#}");
        assert_eq!(snap(&f.0), before);
    }

    #[test]
    fn replaces_same_ship_and_restores_original_foreign_file() {
        let f = Fixture::new();
        f.put("content/skin", b"foreign original");
        install(&f.0, &info("A", "PFSB510"), &[item("content/skin", b"A"), item("PnFMods/A/Main.py", b"a")], &O).unwrap();
        let result = switch(&f.0, &info("B", "PFSB510"), &[item("content/skin", b"B"), item("PnFMods/B/Main.py", b"b")], false).unwrap();
        assert_eq!(result["previous"][0]["name"], "A");
        assert_eq!(std::fs::read(f.0.join("content/skin")).unwrap(), b"B");
        assert!(!f.0.join("PnFMods/A/Main.py").exists());
        assert_eq!(names(&load(&f.0).unwrap()), BTreeSet::from(["B".into()]));
        uninstall(&f.0, "B", false, false).unwrap();
        assert_eq!(std::fs::read(f.0.join("content/skin")).unwrap(), b"foreign original");
    }

    #[test]
    fn ship_alias_and_same_name_different_project_are_replaced() {
        let f = Fixture::new();
        let mut a = info("SharedName", "PFSB510");
        a.project_dir = PathBuf::from("/deleted-project/A");
        install(&f.0, &a, &[item("PnFMods/SharedName/Main.py", b"A")], &O).unwrap();
        let mut b = info("SharedName", "FSB510_Bourgogne");
        b.project_dir = PathBuf::from("/deleted-project/B");
        switch(&f.0, &b, &[item("PnFMods/SharedName/Main.py", b"B")], false).unwrap();
        assert_eq!(std::fs::read(f.0.join("PnFMods/SharedName/Main.py")).unwrap(), b"B");
        assert!(load(&f.0).unwrap().mods["SharedName"].project_dir.ends_with("/B"));
        let before = snap(&f.0);
        assert!(switch(&f.0, &info("SharedName", "PJSB001"), &[item("other", b"x")], false).is_err());
        assert_eq!(snap(&f.0), before);
    }

    #[test]
    fn dry_run_conflicts_and_user_edits_leave_full_prestate_untouched() {
        let f = Fixture::new();
        install(&f.0, &info("A", "PFSB510"), &[item("old", b"A")], &O).unwrap();
        install(&f.0, &info("Other", "PJSB001"), &[item("shared", b"other")], &O).unwrap();
        let before = snap(&f.0);
        let b = info("B", "PFSB510");
        switch(&f.0, &b, &[item("new", b"B")], true).unwrap();
        assert_eq!(snap(&f.0), before);
        assert!(switch(&f.0, &b, &[item("shared", b"conflict")], false).is_err());
        assert_eq!(snap(&f.0), before);
        assert!(switch(&f.0, &b, &[item("../escape", b"bad")], false).is_err());
        assert_eq!(snap(&f.0), before);
        f.put("old", b"user edit");
        let edited = snap(&f.0);
        assert!(switch(&f.0, &b, &[item("new", b"B")], false).is_err());
        assert_eq!(snap(&f.0), edited);
    }

    #[test]
    fn switch_preserves_other_ship_shared_ownership() {
        let f = Fixture::new();
        install(&f.0, &info("A", "PFSB510"), &[item("shared", b"same"), item("a", b"a")], &O).unwrap();
        install(&f.0, &info("Other", "PJSB001"), &[item("shared", b"same")], &O).unwrap();
        switch(&f.0, &info("B", "PFSB510"), &[item("shared", b"same"), item("b", b"b")], false).unwrap();
        let reg = load(&f.0).unwrap();
        assert_eq!(reg.paths["shared"].owners, vec!["Other", "B"]);
        uninstall(&f.0, "B", false, false).unwrap();
        assert_eq!(std::fs::read(f.0.join("shared")).unwrap(), b"same");
        assert_eq!(load(&f.0).unwrap().paths["shared"].owners, vec!["Other"]);
    }
}
