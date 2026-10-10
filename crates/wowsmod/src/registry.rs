//! M4: res_mods 里的安装登记表(多 mod 共存、逐文件哈希复原、迁移的依据)。
//!
//! ```text
//! <res_mods>/.wowsmod/registry.json   # 已装 mod 与每个文件的归属/哈希/备份
//! <res_mods>/.wowsmod/backup/<rel>    # 安装时被覆盖的“外来”文件(不是 wowsmod 装的)原件
//! <res_mods>/.wowsmod/lock            # 写登记表时的锁(防两个进程同时安装)
//! ```
//! 规则:
//! - 同一路径两个 mod 内容相同 → 共用(引用计数), 内容不同 → 冲突, 拒绝安装。
//! - 同一条船两个 SDK 路线 mod → 冲突(游戏会用哪个不确定), 需 `--allow-same-ship`。
//! - 卸载只删“哈希仍等于安装时”的文件; 被别人改过的保留并报告(`--force` 才删);
//!   有备份的还原并核对备份哈希; 最后一个 mod 卸载后 `.wowsmod/` 整个删除, res_mods 回到安装前。
//! - 安装中途出错自动回滚已写部分。
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::modproj::{sha256_hex, InstallItem};

pub const DIR: &str = ".wowsmod";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Registry {
    pub format: u32,
    pub mods: BTreeMap<String, ModEntry>,
    /// 小写相对路径 → 归属
    pub paths: BTreeMap<String, PathEntry>,
    /// wowsmod 新建的目录(小写 → 原样相对路径); 空了就删
    #[serde(default)]
    pub dirs: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModEntry {
    pub name: String,
    pub version: String,
    pub ship: String,
    pub mode: String,
    /// 项目目录(迁移/重装时从这里重新生成)
    pub project_dir: String,
    /// project / package:<包路径>
    pub source: String,
    pub game_build: String,
    pub installed_unix: u64,
    pub files: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PathEntry {
    pub rel: String,
    pub sha256: String,
    pub size: u64,
    pub owners: Vec<String>,
    /// 安装前已有的外来文件, 备份在 .wowsmod/backup/<rel>
    #[serde(default)]
    pub backup_sha256: Option<String>,
    /// project / game-fill:<游戏内路径> / generated
    #[serde(default)]
    pub origin: String,
}

pub struct ModInfo {
    pub name: String,
    pub version: String,
    pub ship: String,
    pub mode: String,
    pub project_dir: PathBuf,
    pub source: String,
    pub game_build: String,
}

pub struct Opts {
    pub dry_run: bool,
    pub allow_same_ship: bool,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn reg_dir(res_mods: &Path) -> PathBuf {
    res_mods.join(DIR)
}

pub fn load(res_mods: &Path) -> Result<Registry> {
    let p = reg_dir(res_mods).join("registry.json");
    if !p.exists() {
        return Ok(Registry { format: 1, ..Default::default() });
    }
    let r: Registry = serde_json::from_slice(&std::fs::read(&p)?).with_context(|| format!("登记表损坏: {}(可用 wowsmod installed --verify 检查)", p.display()))?;
    Ok(r)
}

fn save(res_mods: &Path, r: &Registry) -> Result<()> {
    let d = reg_dir(res_mods);
    if r.mods.is_empty() && r.paths.is_empty() {
        // 最后一个 mod 卸载: 清掉 .wowsmod(备份目录此时应为空)
        let _ = std::fs::remove_file(d.join("registry.json"));
        remove_empty_tree(&d.join("backup"));
        return Ok(());
    }
    std::fs::create_dir_all(&d)?;
    let tmp = d.join("registry.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(r)?)?;
    std::fs::rename(&tmp, d.join("registry.json"))?;
    Ok(())
}

fn remove_empty_tree(d: &Path) {
    if !d.is_dir() {
        return;
    }
    if let Ok(rd) = std::fs::read_dir(d) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                remove_empty_tree(&e.path());
            }
        }
    }
    let _ = std::fs::remove_dir(d);
}

/// 进程级互斥: .wowsmod/lock(超过 10 分钟的旧锁视为残留)
pub struct Lock(PathBuf);
impl Lock {
    pub fn take(res_mods: &Path) -> Result<Lock> {
        let d = reg_dir(res_mods);
        std::fs::create_dir_all(&d)?;
        let p = d.join("lock");
        if let Ok(m) = std::fs::metadata(&p) {
            let old = m.modified().ok().and_then(|t| t.elapsed().ok()).map(|e| e.as_secs() > 600).unwrap_or(true);
            if old {
                let _ = std::fs::remove_file(&p);
            }
        }
        std::fs::OpenOptions::new().write(true).create_new(true).open(&p).map_err(|_| anyhow!("另一个 wowsmod 正在安装/卸载({} 存在)。等它结束; 若确定没有在运行, 删除该文件后重试", p.display()))?;
        Ok(Lock(p))
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        if let Some(d) = self.0.parent() {
            // 只有锁的空目录也删掉(安装失败回滚后)
            let _ = std::fs::remove_dir(d);
        }
    }
}

fn safe_rel(rel: &str) -> Result<()> {
    if rel.is_empty() || rel.split('/').any(|c| c.is_empty() || c == ".." || c == "." || c.contains(':') || c.contains('\\')) {
        bail!("拒绝不安全的路径 {rel:?}");
    }
    if rel.to_lowercase().starts_with(&format!("{DIR}/")) {
        bail!("路径不能写进 {DIR}/: {rel}");
    }
    Ok(())
}

fn write_atomic(dst: &Path, data: &[u8]) -> Result<()> {
    let tmp = dst.with_file_name(format!("{}.wowsmod-tmp", dst.file_name().unwrap().to_string_lossy()));
    std::fs::write(&tmp, data).with_context(|| format!("写入 {}", tmp.display()))?;
    if dst.exists() {
        std::fs::remove_file(dst).with_context(|| format!("替换 {}(文件被占用? 先退出游戏)", dst.display()))?;
    }
    std::fs::rename(&tmp, dst).with_context(|| format!("改名 {}", tmp.display()))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct Conflict {
    pub kind: String,
    pub rel: String,
    pub other: String,
    pub detail: String,
}

/// 只检查不写: 与已装 mod 的文件/船冲突, 以及会覆盖的外来文件
pub fn check(reg: &Registry, res_mods: &Path, info: &ModInfo, items: &[InstallItem], allow_same_ship: bool) -> (Vec<Conflict>, Vec<String>, usize) {
    let mut conf = Vec::new();
    let mut foreign = Vec::new();
    let mut shared = 0;
    for it in items {
        let k = it.rel.to_lowercase();
        if let Some(pe) = reg.paths.get(&k) {
            let others: Vec<&String> = pe.owners.iter().filter(|o| **o != info.name).collect();
            if others.is_empty() {
                continue;
            }
            if pe.sha256 == sha256_hex(&it.data) {
                shared += 1;
            } else {
                conf.push(Conflict { kind: "file".into(), rel: it.rel.clone(), other: others.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(","), detail: "同一路径内容不同".into() });
            }
        } else if res_mods.join(&it.rel).is_file() && !(it.origin == "generated" && it.data.is_empty()) {
            foreign.push(it.rel.clone());
        }
    }
    if info.mode == "sdk" && !info.ship.is_empty() && !allow_same_ship {
        for m in reg.mods.values() {
            if m.name != info.name && m.mode == "sdk" && m.ship.eq_ignore_ascii_case(&info.ship) {
                conf.push(Conflict { kind: "ship".into(), rel: format!("PnFMods/*/{}", info.ship), other: m.name.clone(), detail: "两个 SDK mod 改同一条船, 游戏会用哪一个不确定".into() });
            }
        }
    }
    (conf, foreign, shared)
}

pub fn conflicts_text(c: &[Conflict]) -> String {
    let mut s: Vec<String> = c.iter().take(20).map(|x| format!("{} {} ↔ {}: {}", if x.kind == "ship" { "[船]" } else { "[文件]" }, x.rel, x.other, x.detail)).collect();
    if c.len() > 20 {
        s.push(format!("…共 {} 处", c.len()));
    }
    s.join("\n  ")
}

/// 安装(写 res_mods + 登记表)。出错时回滚本次已写的部分。
pub fn install(res_mods: &Path, info: &ModInfo, items: &[InstallItem], o: &Opts) -> Result<Value> {
    if !res_mods.is_dir() {
        bail!("res_mods 目录不存在: {}", res_mods.display());
    }
    for it in items {
        safe_rel(&it.rel)?;
    }
    let _lock = if o.dry_run { None } else { Some(Lock::take(res_mods)?) };
    let mut reg = load(res_mods)?;
    if reg.mods.contains_key(&info.name) {
        bail!("mod {} 已经装在 {}; 先 wowsmod uninstall {}(或 install --reinstall)", info.name, res_mods.display(), info.name);
    }
    let (conf, foreign, shared) = check(&reg, res_mods, info, items, o.allow_same_ship);
    if !conf.is_empty() {
        bail!("与已装 mod 冲突, 未安装:\n  {}\n下一步: wowsmod installed 查看已装 mod; 卸载对方(wowsmod uninstall <名>)或改名后再装", conflicts_text(&conf));
    }
    let mut written: Vec<String> = Vec::new();
    let mut entry = ModEntry {
        name: info.name.clone(),
        version: info.version.clone(),
        ship: info.ship.clone(),
        mode: info.mode.clone(),
        project_dir: info.project_dir.display().to_string(),
        source: info.source.clone(),
        game_build: info.game_build.clone(),
        installed_unix: now(),
        files: Vec::new(),
    };
    let mut skipped_foreign_loader = 0;
    let mut bytes = 0u64;
    let res: Result<()> = (|| {
        for (i, it) in items.iter().enumerate() {
            let k = it.rel.to_lowercase();
            let sha = sha256_hex(&it.data);
            let dst = res_mods.join(&it.rel);
            if let Some(pe) = reg.paths.get_mut(&k) {
                // check 已保证内容相同 → 共用
                if !pe.owners.contains(&info.name) {
                    pe.owners.push(info.name.clone());
                }
                entry.files.push(k.clone());
                continue;
            }
            // 用户自己放的空 PnFModsLoader.py 等: 不接管
            if it.origin == "generated" && dst.is_file() {
                skipped_foreign_loader += 1;
                continue;
            }
            let mut backup_sha = None;
            if !o.dry_run {
                // 目录
                let mut d = dst.parent().unwrap().to_path_buf();
                let mut new_dirs = Vec::new();
                while !d.exists() && d.starts_with(res_mods) && d != res_mods {
                    new_dirs.push(d.clone());
                    d = d.parent().unwrap().to_path_buf();
                }
                for nd in new_dirs.iter().rev() {
                    std::fs::create_dir(nd).with_context(|| format!("建目录 {}", nd.display()))?;
                    let r = crate::modproj::rel_slash_pub(res_mods, nd);
                    reg.dirs.insert(r.to_lowercase(), r);
                }
                if dst.is_file() {
                    let old = std::fs::read(&dst)?;
                    let b = reg_dir(res_mods).join("backup").join(&it.rel);
                    std::fs::create_dir_all(b.parent().unwrap())?;
                    std::fs::write(&b, &old)?;
                    let bs = sha256_hex(&old);
                    if sha256_hex(&std::fs::read(&b)?) != bs {
                        bail!("备份校验失败 {}", b.display());
                    }
                    backup_sha = Some(bs);
                }
                write_atomic(&dst, &it.data)?;
                if sha256_hex(&std::fs::read(&dst)?) != sha {
                    bail!("写后校验失败 {}", it.rel);
                }
            } else if dst.is_file() {
                backup_sha = Some(String::from("(演练)"));
            }
            reg.paths.insert(k.clone(), PathEntry { rel: it.rel.clone(), sha256: sha, size: it.data.len() as u64, owners: vec![info.name.clone()], backup_sha256: backup_sha, origin: it.origin.clone() });
            entry.files.push(k.clone());
            written.push(k);
            bytes += it.data.len() as u64;
            if !o.dry_run && i % 100 == 99 {
                let mut tmp = reg.clone();
                tmp.mods.insert(info.name.clone(), entry.clone());
                save(res_mods, &tmp)?;
            }
        }
        Ok(())
    })();
    if let Err(e) = res {
        if !o.dry_run {
            // 回滚: 按“已登记但未完成”的状态卸载本 mod
            reg.mods.insert(info.name.clone(), entry.clone());
            let rb = remove_mod(res_mods, &mut reg, &info.name, true, false);
            let _ = save(res_mods, &reg);
            return Err(e.context(format!("安装中途失败, 已回滚({})", rb.map(|v| v.to_string()).unwrap_or_else(|x| format!("回滚也失败: {x:#}")))));
        }
        return Err(e);
    }
    reg.mods.insert(info.name.clone(), entry.clone());
    if !o.dry_run {
        save(res_mods, &reg)?;
    }
    Ok(json!({"dry_run": o.dry_run, "mod": info.name, "res_mods": res_mods.display().to_string(), "files": entry.files.len(), "written": written.len(), "shared_with_other_mods": shared,
        "overwrote_foreign": foreign, "skipped_existing_loader": skipped_foreign_loader, "bytes_written": bytes, "registry": reg_dir(res_mods).join("registry.json").display().to_string()}))
}

/// 从登记表里移除一个 mod 并复原文件(调用方负责加锁和保存)
pub fn remove_mod(res_mods: &Path, reg: &mut Registry, name: &str, force: bool, dry_run: bool) -> Result<Value> {
    let m = reg.mods.get(name).cloned().ok_or_else(|| anyhow!("登记表里没有 mod {name}; 下一步: wowsmod installed 查看已装 mod"))?;
    let (mut deleted, mut restored, mut shared_kept, mut missing) = (0, 0, 0, 0);
    let mut kept_modified = Vec::new();
    let mut errors = Vec::new();
    for k in &m.files {
        let Some(pe) = reg.paths.get(k).cloned() else { continue };
        let owners: Vec<String> = pe.owners.iter().filter(|o| *o != name).cloned().collect();
        if !owners.is_empty() {
            if !dry_run {
                reg.paths.get_mut(k).unwrap().owners = owners;
            }
            shared_kept += 1;
            continue;
        }
        let dst = res_mods.join(&pe.rel);
        let bpath = reg_dir(res_mods).join("backup").join(&pe.rel);
        if dst.is_file() {
            let cur = sha256_hex(&std::fs::read(&dst)?);
            if cur != pe.sha256 && !force {
                kept_modified.push(pe.rel.clone());
                if !dry_run {
                    // 文件留给用户; 备份(若有)改名保留, 不丢原件
                    if bpath.is_file() {
                        let keep = bpath.with_file_name(format!("{}.orig-{}", bpath.file_name().unwrap().to_string_lossy(), now()));
                        let _ = std::fs::rename(&bpath, keep);
                    }
                    reg.paths.remove(k);
                }
                continue;
            }
            if !dry_run {
                std::fs::remove_file(&dst).with_context(|| format!("删除 {}(被占用? 先退出游戏)", dst.display()))?;
            }
            deleted += 1;
        } else {
            missing += 1;
        }
        if let Some(bs) = &pe.backup_sha256 {
            if !dry_run {
                match std::fs::read(&bpath) {
                    Ok(b) if &sha256_hex(&b) == bs => {
                        std::fs::write(&dst, &b)?;
                        std::fs::remove_file(&bpath)?;
                        restored += 1;
                    }
                    Ok(_) => errors.push(format!("备份哈希不符, 未还原: {}", bpath.display())),
                    Err(e) => errors.push(format!("备份丢失, 未还原 {}: {e}", pe.rel)),
                }
            } else {
                restored += 1;
            }
        }
        if !dry_run {
            reg.paths.remove(k);
        }
    }
    // 目录: 所有 wowsmod 建的、现在空了的(深的先删)
    let mut removed_dirs = 0;
    if !dry_run {
        reg.mods.remove(name);
        let mut ds: Vec<(String, String)> = reg.dirs.iter().map(|(a, b)| (a.clone(), b.clone())).collect();
        ds.sort_by_key(|(k, _)| std::cmp::Reverse(k.matches('/').count()));
        for (k, r) in ds {
            let pd = res_mods.join(&r);
            if !pd.exists() {
                reg.dirs.remove(&k);
                continue;
            }
            if pd.is_dir() && std::fs::read_dir(&pd)?.next().is_none() {
                std::fs::remove_dir(&pd)?;
                reg.dirs.remove(&k);
                removed_dirs += 1;
            }
        }
        if reg.mods.is_empty() {
            // 没有 mod 了: 剩下的目录登记(非空的, 里面是用户文件)也不再归我们管
            reg.dirs.clear();
        }
        // 备份目录里空了的子目录
        remove_empty_tree_children(&reg_dir(res_mods).join("backup"));
    }
    Ok(json!({"mod": name, "dry_run": dry_run, "files": m.files.len(), "deleted": deleted, "restored_backups": restored, "kept_shared": shared_kept, "already_missing": missing,
        "kept_modified": kept_modified, "removed_dirs": removed_dirs, "errors": errors, "complete": kept_modified.is_empty() && errors.is_empty()}))
}

fn remove_empty_tree_children(d: &Path) {
    if let Ok(rd) = std::fs::read_dir(d) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                remove_empty_tree(&e.path());
            }
        }
    }
}

pub fn uninstall(res_mods: &Path, name: &str, force: bool, dry_run: bool) -> Result<Value> {
    let _lock = if dry_run { None } else { Some(Lock::take(res_mods)?) };
    let mut reg = load(res_mods)?;
    let r = remove_mod(res_mods, &mut reg, name, force, dry_run)?;
    if !dry_run {
        save(res_mods, &reg)?;
    }
    Ok(r)
}

/// 核对: 已装文件是否还在且哈希一致、备份是否完好
pub fn verify(res_mods: &Path) -> Result<Value> {
    let reg = load(res_mods)?;
    let mut mods = Vec::new();
    for m in reg.mods.values() {
        let (mut ok, mut modified, mut missing) = (0, Vec::new(), Vec::new());
        for k in &m.files {
            let Some(pe) = reg.paths.get(k) else {
                missing.push(k.clone());
                continue;
            };
            match std::fs::read(res_mods.join(&pe.rel)) {
                Ok(d) if sha256_hex(&d) == pe.sha256 => ok += 1,
                Ok(_) => modified.push(pe.rel.clone()),
                Err(_) => missing.push(pe.rel.clone()),
            }
        }
        mods.push(json!({"name": m.name, "version": m.version, "ship": m.ship, "mode": m.mode, "game_build": m.game_build, "project_dir": m.project_dir, "source": m.source,
            "files": m.files.len(), "ok": ok, "modified": modified, "missing": missing, "intact": modified.is_empty() && missing.is_empty()}));
    }
    let mut bad_backups = Vec::new();
    for pe in reg.paths.values() {
        if let Some(bs) = &pe.backup_sha256 {
            match std::fs::read(reg_dir(res_mods).join("backup").join(&pe.rel)) {
                Ok(b) if &sha256_hex(&b) == bs => {}
                _ => bad_backups.push(pe.rel.clone()),
            }
        }
    }
    let shared = reg.paths.values().filter(|p| p.owners.len() > 1).count();
    Ok(json!({"res_mods": res_mods.display().to_string(), "mods": mods, "paths": reg.paths.len(), "shared_paths": shared, "backups_bad": bad_backups}))
}

/// 登记表里各 mod 的游戏补齐文件哈希(迁移时比较原版资源是否更新)
pub fn game_fill_hashes(reg: &Registry, name: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    if let Some(me) = reg.mods.get(name) {
        for k in &me.files {
            if let Some(pe) = reg.paths.get(k) {
                if pe.origin.starts_with("game-fill") {
                    m.insert(k.clone(), pe.sha256.clone());
                }
            }
        }
    }
    m
}

pub fn names(reg: &Registry) -> BTreeSet<String> {
    reg.mods.keys().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snap(d: &Path) -> Vec<(String, String)> {
        let mut v = Vec::new();
        let mut st = vec![d.to_path_buf()];
        while let Some(x) = st.pop() {
            for e in std::fs::read_dir(&x).unwrap().flatten() {
                let r = e.path().strip_prefix(d).unwrap().to_string_lossy().replace('\\', "/");
                if e.path().is_dir() {
                    v.push((r + "/", String::new()));
                    st.push(e.path());
                } else {
                    v.push((r, sha256_hex(&std::fs::read(e.path()).unwrap())));
                }
            }
        }
        v.sort();
        v
    }
    fn item(rel: &str, d: &[u8]) -> InstallItem {
        InstallItem { rel: rel.into(), data: d.to_vec(), origin: "project".into() }
    }
    fn info(n: &str, ship: &str) -> ModInfo {
        ModInfo { name: n.into(), version: "1".into(), ship: ship.into(), mode: "sdk".into(), project_dir: PathBuf::from("/x"), source: "project".into(), game_build: "1".into() }
    }
    const O: Opts = Opts { dry_run: false, allow_same_ship: false };

    #[test]
    fn coexist_share_conflict_restore() {
        let tmp = std::env::temp_dir().join(format!("wowsmod_reg_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let rm = tmp.join("res_mods");
        std::fs::create_dir_all(rm.join("content/x")).unwrap();
        std::fs::write(rm.join("readme.txt"), b"readme").unwrap();
        std::fs::write(rm.join("content/x/over.dds"), b"user original").unwrap();
        let before = snap(&rm);
        let loader = InstallItem { rel: "PnFModsLoader.py".into(), data: vec![], origin: "generated".into() };
        let a = vec![item("PnFMods/A/Main.py", b"a"), item("content/x/over.dds", b"A tex"), item("PnFMods/ModsSDK.zip/S1/x.visual", b"same"), loader.clone()];
        let b = vec![item("PnFMods/B/Main.py", b"b"), item("PnFMods/ModsSDK.zip/S1/x.visual", b"same"), loader.clone()];
        install(&rm, &info("A", "S1"), &a, &O).unwrap();
        // 同船 SDK → 冲突
        let e = install(&rm, &info("B", "S1"), &b, &O).unwrap_err();
        assert!(format!("{e:#}").contains("[船]"), "{e:#}");
        let r = install(&rm, &info("B", "S2"), &b, &O).unwrap();
        assert_eq!(r["shared_with_other_mods"], 2, "{r}");
        // 文件冲突
        let c = vec![item("content/x/over.dds", b"C tex")];
        assert!(install(&rm, &info("C", "S3"), &c, &O).is_err());
        assert!(!rm.join(".wowsmod/lock").exists());
        // 重复安装
        assert!(install(&rm, &info("A", "S1"), &a, &O).is_err());
        let v = verify(&rm).unwrap();
        assert_eq!(v["shared_paths"], 2, "{v}");
        // 卸 A: 共用文件留给 B, 外来文件还原
        let u = uninstall(&rm, "A", false, false).unwrap();
        assert_eq!(u["complete"], true, "{u}");
        assert_eq!(std::fs::read(rm.join("content/x/over.dds")).unwrap(), b"user original");
        assert!(rm.join("PnFModsLoader.py").exists());
        assert!(rm.join("PnFMods/ModsSDK.zip/S1/x.visual").exists());
        let u = uninstall(&rm, "B", false, false).unwrap();
        assert_eq!(u["complete"], true, "{u}");
        assert_eq!(snap(&rm), before, "res_mods 应完全复原");
        // 改过的文件保留; --force 删除
        install(&rm, &info("A", "S1"), &a, &O).unwrap();
        std::fs::write(rm.join("PnFMods/A/Main.py"), b"user edit").unwrap();
        let u = uninstall(&rm, "A", false, false).unwrap();
        assert_eq!(u["complete"], false);
        assert_eq!(u["kept_modified"][0], "PnFMods/A/Main.py");
        std::fs::remove_file(rm.join("PnFMods/A/Main.py")).unwrap();
        std::fs::remove_dir(rm.join("PnFMods/A")).unwrap();
        // 回滚: 第二个文件路径不安全 → 整个安装失败且不留痕迹
        let before2 = snap(&rm);
        let bad = vec![item("PnFMods/D/ok.txt", b"1"), item("../evil", b"2")];
        assert!(install(&rm, &info("D", "S4"), &bad, &O).is_err());
        assert_eq!(snap(&rm), before2);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rollback_midway() {
        let tmp = std::env::temp_dir().join(format!("wowsmod_rb_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let rm = tmp.join("res_mods");
        std::fs::create_dir_all(&rm).unwrap();
        std::fs::write(rm.join("readme.txt"), b"r").unwrap();
        // 一个文件挡住目录 → 第二个条目建目录失败
        std::fs::write(rm.join("blocker"), b"x").unwrap();
        let before = snap(&rm);
        let items = vec![item("PnFMods/E/a.txt", b"1"), item("blocker/b.txt", b"2")];
        let e = install(&rm, &info("E", "S"), &items, &O).unwrap_err();
        assert!(format!("{e:#}").contains("回滚"), "{e:#}");
        assert_eq!(snap(&rm), before);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
