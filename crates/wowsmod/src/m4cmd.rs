//! M4 子命令: install / uninstall / installed / conflicts / migrate / pack
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use clap::Args;
use serde_json::{json, Value};

use wowsmod_formats::zipfile::{self, ZipEntry};

use crate::modproj::{self, sha256_hex, InstallItem, Manifest, ManifestFile, Project};
use crate::registry::{self, ModInfo, Opts};
use crate::{emit, game, game_ctx, Cli, GameCtx};

pub const PKG_EXT: &str = "wsmod";
const PKG_MANIFEST: &str = "wowsmod-package.json";

#[derive(Args, Clone)]
pub struct InstallArgs {
    /// mod 项目目录, 或 pack 生成的 .wsmod 包
    pub target: PathBuf,
    /// 只列出将要做的事, 不写文件
    #[arg(long)]
    pub dry_run: bool,
    /// 已装同名 mod 时先卸载再装
    #[arg(long)]
    pub reinstall: bool,
    /// 允许两个 SDK mod 改同一条船(游戏会用哪个不确定)
    #[arg(long)]
    pub allow_same_ship: bool,
    /// 装到指定 res_mods(默认: 当前游戏版本的 bin\<版本>\res_mods)
    #[arg(long, value_name = "DIR")]
    pub res_mods: Option<PathBuf>,
    /// 游戏在运行时也继续(不推荐: 文件可能被占用)
    #[arg(long)]
    pub ignore_running: bool,
    /// 兼容旧用法(M2/M3 的 install --dev), 现在可省略
    #[arg(long, hide = true)]
    pub dev: bool,
}

#[derive(Args, Clone)]
pub struct UninstallArgs {
    /// mod 项目目录或 mod 名(见 wowsmod installed)
    pub target: String,
    #[arg(long)]
    pub dry_run: bool,
    /// 安装后被改动过的文件也删除(默认保留并报告)
    #[arg(long)]
    pub force: bool,
    #[arg(long, value_name = "DIR")]
    pub res_mods: Option<PathBuf>,
    #[arg(long)]
    pub ignore_running: bool,
}

#[derive(Args, Clone)]
pub struct InstalledArgs {
    /// 逐文件核对哈希与备份
    #[arg(long)]
    pub verify: bool,
    #[arg(long, value_name = "DIR")]
    pub res_mods: Option<PathBuf>,
}

#[derive(Args, Clone)]
pub struct ConflictsArgs {
    /// 要检查的项目目录或 .wsmod 包(可多个); 同时与已装 mod 比较
    #[arg(required = true)]
    pub targets: Vec<PathBuf>,
    #[arg(long, value_name = "DIR")]
    pub res_mods: Option<PathBuf>,
}

#[derive(Args, Clone)]
pub struct MigrateArgs {
    /// 旧 res_mods 目录或旧版本号(bin 下的数字目录); 省略则自动找带 wowsmod 登记表的旧版本目录
    #[arg(long)]
    pub from: Option<String>,
    /// 新 res_mods 目录或版本号; 省略 = 当前游戏版本
    #[arg(long)]
    pub to: Option<String>,
    /// 迁移成功后保留旧目录里的文件(默认从旧目录卸载)
    #[arg(long)]
    pub keep_old: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub ignore_running: bool,
}

#[derive(Args, Clone)]
pub struct PackArgs {
    /// mod 项目目录
    pub project: PathBuf,
    /// 输出 .wsmod 文件或目录(默认 <项目>/dist/<名>-<版本>.wsmod)
    #[arg(short, long)]
    pub out: Option<PathBuf>,
    /// 另附可手动复制的完整 res_mods 树(含从你的游戏补齐的原版资源; 只适合自用/转给同版本玩家)
    #[arg(long)]
    pub full: bool,
    /// 显式覆盖受限素材拦截(项目 [license] no_redistribute = true 时 pack 默认拒绝)。只用于自用备份, 不得公开分发
    #[arg(long)]
    pub allow_restricted: bool,
}

fn home() -> PathBuf {
    if let Ok(h) = std::env::var("WOWSMOD_HOME") {
        return PathBuf::from(h);
    }
    std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("wowsmod_data"))).unwrap_or_else(|| PathBuf::from("wowsmod_data"))
}

/// 本机 SDK 素材记录: <home>/sdk_sources.json { 船: {src, sdk_dir} }(import-sdk 成功时自动写)
fn sdk_sources_path() -> PathBuf {
    home().join("sdk_sources.json")
}
pub fn sdk_sources() -> Value {
    std::fs::read(sdk_sources_path()).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_else(|| json!({}))
}
pub fn sdk_source(ship: &str) -> Option<Value> {
    let v = sdk_sources();
    let k = &v[ship];
    let src = k["src"].as_str()?;
    Path::new(src).is_dir().then(|| k.clone())
}
pub fn remember_sdk_source(ship: &str, src: &Path, sdk_dir: Option<&Path>) {
    let abs = |p: &Path| std::fs::canonicalize(p).map(|x| x.display().to_string().trim_start_matches(r"\\?\").to_string()).unwrap_or_else(|_| p.display().to_string());
    let mut v = sdk_sources();
    v[ship] = json!({"src": abs(src), "sdk_dir": sdk_dir.map(abs)});
    let _ = std::fs::create_dir_all(home());
    let _ = std::fs::write(sdk_sources_path(), serde_json::to_string_pretty(&v).unwrap_or_default());
}

pub fn home_dir() -> PathBuf {
    home()
}

fn check_running(ignore: bool) -> Result<()> {
    let r = game::running();
    if !r.is_empty() && !ignore {
        bail!("游戏正在运行({}), 文件可能被占用。下一步: 退出游戏后重试(或加 --ignore-running)", r.join(", "));
    }
    Ok(())
}

fn res_mods_of(cli: &Cli, over: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = over {
        return Ok(p.to_path_buf());
    }
    let (gi, _) = game::locate(cli.game.as_deref());
    Ok(gi.ok_or_else(|| anyhow!("没有找到游戏; 用 --game <游戏根目录> 或 --res-mods <目录> 指定"))?.res_mods)
}

/// 包 → 解到 <home>/packages/<名>-<版本>-<sha8>/ 当作项目
pub fn open_package(pkg: &Path) -> Result<(Project, Value)> {
    let d = std::fs::read(pkg).with_context(|| format!("读取 {}", pkg.display()))?;
    let es = zipfile::read(&d).map_err(|e| anyhow!("{}: {e}", pkg.display()))?;
    let man = es.iter().find(|e| e.name == PKG_MANIFEST).ok_or_else(|| anyhow!("{} 不是 wowsmod 包(缺少 {PKG_MANIFEST})", pkg.display()))?;
    let m: Value = serde_json::from_slice(&man.data)?;
    if m["format"].as_u64() != Some(1) {
        bail!("包格式版本 {} 不支持(本程序支持 1); 下一步: 更新 wowsmod", m["format"]);
    }
    let byname: BTreeMap<&str, &ZipEntry> = es.iter().map(|e| (e.name.as_str(), e)).collect();
    for f in m["files"].as_array().ok_or_else(|| anyhow!("包清单缺少 files"))? {
        let p = f["path"].as_str().unwrap_or("");
        let e = byname.get(p).ok_or_else(|| anyhow!("包里缺少 {p}(包不完整)"))?;
        if sha256_hex(&e.data) != f["sha256"].as_str().unwrap_or("") {
            bail!("包内文件哈希不符: {p}(包已损坏或被改动)");
        }
    }
    let name = m["name"].as_str().unwrap_or("mod");
    let ver = m["version"].as_str().unwrap_or("0");
    let dir = home().join("packages").join(format!("{name}-{ver}-{}", &sha256_hex(&d)[..8]));
    if !dir.join("mod.toml").exists() {
        let tmp = dir.with_extension("tmp");
        let _ = std::fs::remove_dir_all(&tmp);
        for e in &es {
            if e.name == "mod.toml" || e.name.starts_with("assets/") {
                let o = tmp.join(&e.name);
                std::fs::create_dir_all(o.parent().unwrap())?;
                std::fs::write(&o, &e.data)?;
            }
        }
        std::fs::create_dir_all(tmp.join("edits"))?;
        std::fs::write(tmp.join(".gitignore"), ".install/\n")?;
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        std::fs::rename(&tmp, &dir)?;
    }
    Ok((Project::open(&dir)?, m))
}

fn is_pkg(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    if p.extension().map(|e| e.eq_ignore_ascii_case(PKG_EXT) || e.eq_ignore_ascii_case("zip")).unwrap_or(false) {
        return true;
    }
    // 没有扩展名也认 zip 魔数
    std::fs::File::open(p).and_then(|mut f| { use std::io::Read; let mut b = [0u8; 4]; f.read_exact(&mut b).map(|_| b == *b"PK\x03\x04") }).unwrap_or(false)
}

fn open_target(p: &Path) -> Result<(Project, String)> {
    if is_pkg(p) {
        let (pr, _) = open_package(p)?;
        Ok((pr, format!("package:{}", std::fs::canonicalize(p).unwrap_or(p.to_path_buf()).display())))
    } else {
        Ok((Project::open(p)?, "project".into()))
    }
}

fn info_of(p: &Project, source: &str, build: &str) -> ModInfo {
    ModInfo {
        name: p.cfg.meta.name.clone(),
        version: p.cfg.meta.version.clone(),
        ship: p.cfg.meta.ship.clone(),
        mode: p.cfg.install.mode.clone(),
        project_dir: std::fs::canonicalize(&p.root).unwrap_or(p.root.clone()),
        source: source.into(),
        game_build: build.into(),
    }
}

fn project_manifest(p: &Project, res_mods: &Path, build: &str, items: &[InstallItem]) -> Result<()> {
    let files: Vec<ManifestFile> = items.iter().map(|it| ManifestFile { rel: it.rel.clone(), sha256: sha256_hex(&it.data), size: it.data.len() as u64, origin: it.origin.clone(), backed_up: false }).collect();
    modproj::write_manifest(p, res_mods, build, &files, &[])?;
    // 标记: 这份清单由登记表管理(卸载走登记表)
    let mp = p.root.join(".install/manifest.json");
    let mut v: Value = serde_json::from_slice(&std::fs::read(&mp)?)?;
    v["managed_by_registry"] = json!(true);
    std::fs::write(&mp, serde_json::to_vec_pretty(&v)?)?;
    Ok(())
}

fn archive_project_manifest(root: &Path) {
    let mp = root.join(".install/manifest.json");
    if mp.exists() {
        let hist = root.join(".install/history");
        let _ = std::fs::create_dir_all(&hist);
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = std::fs::rename(&mp, hist.join(format!("manifest_{t}.json")));
    }
}

fn do_install(ctx: &GameCtx, p: &Project, source: &str, res_mods: &Path, a: &InstallArgs) -> Result<Value> {
    let (items, notes) = modproj::plan_install(p, &ctx.index, &ctx.reader, res_mods)?;
    let info = info_of(p, source, &ctx.install.build);
    let mut r = registry::install(res_mods, &info, &items, &Opts { dry_run: a.dry_run, allow_same_ship: a.allow_same_ship })?;
    if !a.dry_run {
        project_manifest(p, res_mods, &ctx.install.build, &items)?;
    }
    r["notes"] = json!(notes);
    r["by_origin"] = json!(items.iter().fold(BTreeMap::<String, usize>::new(), |mut m, f| {
        *m.entry(f.origin.split(':').next().unwrap().to_string()).or_default() += 1;
        m
    }));
    Ok(r)
}

pub fn cmd_install(cli: &Cli, a: &InstallArgs) -> Result<i32> {
    check_running(a.ignore_running || a.dry_run)?;
    let (p, source) = open_target(&a.target)?;
    let ctx = game_ctx(cli, true)?.unwrap();
    if !p.cfg.meta.game_build.is_empty() && p.cfg.meta.game_build != ctx.install.build {
        eprintln!("提示: mod 制作时的客户端构建 {} 与当前 {} 不同; 装完请进港检查(原版资源可能已更新)", p.cfg.meta.game_build, ctx.install.build);
    }
    let res_mods = a.res_mods.clone().unwrap_or(ctx.install.res_mods.clone());
    let mut pre = Value::Null;
    if a.reinstall && registry::load(&res_mods)?.mods.contains_key(&p.cfg.meta.name) && !a.dry_run {
        pre = registry::uninstall(&res_mods, &p.cfg.meta.name, false, false)?;
    }
    let mut r = do_install(&ctx, &p, &source, &res_mods, a)?;
    r["ok"] = json!(true);
    if !pre.is_null() {
        r["reinstall_uninstalled"] = pre;
    }
    crate::guide::stash(&r);
    if cli.json {
        emit(cli, &r);
    } else {
        for n in r["notes"].as_array().into_iter().flatten() {
            println!("{}", n.as_str().unwrap_or(""));
        }
        let fo = r["overwrote_foreign"].as_array().map(|v| v.len()).unwrap_or(0);
        println!(
            "{}装好 {} 到 {}: {} 个文件(新写 {}, 与其他 mod 共用 {}, 覆盖并备份了 {} 个原有文件)",
            if a.dry_run { "(演练)" } else { "已" },
            p.cfg.meta.name,
            res_mods.display(),
            r["files"],
            r["written"],
            r["shared_with_other_mods"],
            fo
        );
    }
    Ok(0)
}

/// 卸载目标解析: 项目目录 → (名, res_mods 来自项目清单); 否则当作 mod 名
fn resolve_uninstall(cli: &Cli, a: &UninstallArgs) -> Result<(String, PathBuf, Option<Project>)> {
    let p = Path::new(&a.target);
    if p.join("mod.toml").exists() {
        let pr = Project::open(p)?;
        let rm = match &a.res_mods {
            Some(r) => r.clone(),
            None => {
                let mp = p.join(".install/manifest.json");
                if let Ok(d) = std::fs::read(&mp) {
                    let m: Manifest = serde_json::from_slice(&d)?;
                    PathBuf::from(m.res_mods)
                } else {
                    res_mods_of(cli, None)?
                }
            }
        };
        return Ok((pr.cfg.meta.name.clone(), rm, Some(pr)));
    }
    if is_pkg(p) {
        let (pr, _) = open_package(p)?;
        return Ok((pr.cfg.meta.name.clone(), res_mods_of(cli, a.res_mods.as_deref())?, Some(pr)));
    }
    Ok((a.target.clone(), res_mods_of(cli, a.res_mods.as_deref())?, None))
}

pub fn cmd_uninstall(cli: &Cli, a: &UninstallArgs) -> Result<i32> {
    check_running(a.ignore_running || a.dry_run)?;
    let (name, res_mods, proj) = resolve_uninstall(cli, a)?;
    let reg = registry::load(&res_mods)?;
    let r = if reg.mods.contains_key(&name) {
        let r = registry::uninstall(&res_mods, &name, a.force, a.dry_run)?;
        if !a.dry_run {
            if let Some(pr) = &proj {
                archive_project_manifest(&pr.root);
            }
            if let Some(m) = reg.mods.get(&name) {
                archive_project_manifest(Path::new(&m.project_dir));
            }
        }
        r
    } else if let Some(pr) = proj.as_ref().filter(|pr| pr.root.join(".install/manifest.json").exists()) {
        // M2/M3 旧版安装(没有登记表)
        let mut r = modproj::uninstall(pr, a.dry_run)?;
        r["legacy"] = json!(true);
        r
    } else {
        bail!("{} 里没有已安装的 mod {name}。下一步: wowsmod installed 查看已装 mod", res_mods.display());
    };
    let complete = r["complete"].as_bool().unwrap_or(false);
    let mut r = r;
    r["ok"] = json!(a.dry_run || complete);
    r["res_mods"] = json!(res_mods.display().to_string());
    r["res_mods_pristine"] = json!(game::res_mods_state(&res_mods).pristine);
    crate::guide::stash(&r);
    if cli.json {
        emit(cli, &r);
    } else {
        println!(
            "{}卸载 {name}: 删除 {} 个, 还原备份 {} 个, 与其他 mod 共用而保留 {} 个, 被改动而保留 {} 个{}",
            if a.dry_run { "(演练)" } else { "已" },
            r["deleted"],
            r["restored_backups"],
            r["kept_shared"].as_u64().unwrap_or(0),
            r["kept_modified"].as_array().or(r["kept_modified_after_install"].as_array()).map(|v| v.len()).unwrap_or(0),
            if r["res_mods_pristine"] == true { "; res_mods 已回到只有 readme 的状态" } else { "" }
        );
    }
    Ok(if a.dry_run || complete { 0 } else { 2 })
}

pub fn cmd_installed(cli: &Cli, a: &InstalledArgs) -> Result<i32> {
    let rm = res_mods_of(cli, a.res_mods.as_deref())?;
    let v = registry::verify(&rm)?;
    let st = game::res_mods_state(&rm);
    let all_ok = v["mods"].as_array().map(|m| m.iter().all(|x| x["intact"] == true)).unwrap_or(true) && v["backups_bad"].as_array().map(|b| b.is_empty()).unwrap_or(true);
    let mut v = v;
    v["ok"] = json!(true);
    v["intact"] = json!(all_ok);
    v["res_mods_top_level"] = json!(st.top_level);
    v["pnf_mods_dirs"] = json!(st.pnf_mods);
    crate::guide::stash(&v);
    if cli.json {
        emit(cli, &v);
    } else {
        let ms = v["mods"].as_array().cloned().unwrap_or_default();
        println!("{}: {} 个 wowsmod 安装的 mod", rm.display(), ms.len());
        for m in &ms {
            println!(
                "  {} v{}  船 {}  {} 个文件  {}{}",
                m["name"].as_str().unwrap_or(""),
                m["version"].as_str().unwrap_or(""),
                m["ship"].as_str().unwrap_or(""),
                m["files"],
                if m["intact"] == true { "完好" } else { "有改动/缺失" },
                if a.verify { format!("  (改动 {} 缺失 {})", m["modified"].as_array().map(|x| x.len()).unwrap_or(0), m["missing"].as_array().map(|x| x.len()).unwrap_or(0)) } else { String::new() }
            );
        }
        if ms.is_empty() {
            println!("  (无){}", if st.pristine { "; res_mods 是原始状态" } else { "" });
        }
    }
    Ok(if all_ok { 0 } else { 2 })
}

pub fn cmd_conflicts(cli: &Cli, a: &ConflictsArgs) -> Result<i32> {
    let ctx = game_ctx(cli, true)?.unwrap();
    let rm = a.res_mods.clone().unwrap_or(ctx.install.res_mods.clone());
    let mut reg = registry::load(&rm)?;
    let mut out = Vec::new();
    let mut total = 0;
    // 依次“虚拟安装”到登记表副本里, 后面的与前面的及已装的比较
    for t in &a.targets {
        let (p, source) = open_target(t)?;
        let (items, _) = modproj::plan_install(&p, &ctx.index, &ctx.reader, &rm)?;
        let info = info_of(&p, &source, &ctx.install.build);
        let (conf, foreign, shared) = registry::check(&reg, &rm, &info, &items, false);
        total += conf.len();
        let dup = reg.mods.contains_key(&info.name);
        out.push(json!({"target": t.display().to_string(), "mod": info.name, "ship": info.ship, "files": items.len(), "conflicts": conf, "already_installed": dup, "would_overwrite_foreign": foreign, "shared_identical": shared}));
        // 记入副本
        if !dup {
            let mut files = Vec::new();
            for it in &items {
                let k = it.rel.to_lowercase();
                let e = reg.paths.entry(k.clone()).or_insert(registry::PathEntry { rel: it.rel.clone(), sha256: sha256_hex(&it.data), size: it.data.len() as u64, owners: vec![], backup_sha256: None, origin: it.origin.clone() });
                e.owners.push(info.name.clone());
                files.push(k);
            }
            reg.mods.insert(info.name.clone(), registry::ModEntry { name: info.name.clone(), version: info.version.clone(), ship: info.ship.clone(), mode: info.mode.clone(), project_dir: info.project_dir.display().to_string(), source, game_build: info.game_build.clone(), installed_unix: 0, files });
        }
    }
    let v = json!({"ok": true, "res_mods": rm.display().to_string(), "installed": registry::names(&registry::load(&rm)?), "targets": out, "conflicts_total": total, "compatible": total == 0});
    crate::guide::stash(&v);
    if cli.json {
        emit(cli, &v);
    } else {
        for t in &out {
            let n = t["conflicts"].as_array().map(|x| x.len()).unwrap_or(0);
            println!("{} ({}): {} 个文件, 冲突 {n}, 与他人相同可共用 {}{}", t["mod"].as_str().unwrap_or(""), t["target"].as_str().unwrap_or(""), t["files"], t["shared_identical"], if t["already_installed"] == true { ", 已安装" } else { "" });
            for c in t["conflicts"].as_array().into_iter().flatten().take(20) {
                println!("  {} {} ↔ {}: {}", if c["kind"] == "ship" { "[船]" } else { "[文件]" }, c["rel"].as_str().unwrap_or(""), c["other"].as_str().unwrap_or(""), c["detail"].as_str().unwrap_or(""));
            }
        }
        println!("{}", if total == 0 { "可以共存。" } else { "有冲突: 不能同时安装。" });
    }
    Ok(if total == 0 { 0 } else { 2 })
}

fn build_dir(root: &Path, s: &str) -> PathBuf {
    if s.chars().all(|c| c.is_ascii_digit()) {
        root.join("bin").join(s).join("res_mods")
    } else {
        PathBuf::from(s)
    }
}

pub fn cmd_migrate(cli: &Cli, a: &MigrateArgs) -> Result<i32> {
    check_running(a.ignore_running || a.dry_run)?;
    let ctx = game_ctx(cli, true)?.unwrap();
    let root = ctx.install.root.clone();
    let to = a.to.as_deref().map(|s| build_dir(&root, s)).unwrap_or(ctx.install.res_mods.clone());
    let from = match &a.from {
        Some(s) => build_dir(&root, s),
        None => {
            let cands: Vec<PathBuf> = game::bin_builds(&root).into_iter().map(|b| b.1.join("res_mods")).filter(|r| *r != to && r.join(registry::DIR).join("registry.json").exists()).collect();
            cands.into_iter().next().ok_or_else(|| anyhow!("没有找到带 wowsmod 登记表的旧版本 res_mods(bin\\<旧版本>\\res_mods\\.wowsmod)。下一步: 用 --from <旧 res_mods 目录> 指定; 或直接 wowsmod install <项目> 重新安装"))?
        }
    };
    if from == to {
        bail!("--from 与 --to 是同一个目录 {}", to.display());
    }
    if !to.is_dir() {
        if a.dry_run {
            bail!("目标 res_mods 不存在: {}", to.display());
        }
        std::fs::create_dir_all(&to)?;
    }
    let old = registry::load(&from)?;
    if old.mods.is_empty() {
        bail!("{} 里没有 wowsmod 安装的 mod", from.display());
    }
    let mut results = Vec::new();
    let mut failed = 0;
    for (name, me) in &old.mods {
        let pd = PathBuf::from(&me.project_dir);
        let one = (|| -> Result<Value> {
            let p = Project::open(&pd).with_context(|| format!("项目目录 {} 不可用(被移动/删除?)。若是从包安装的, 下一步: wowsmod install <包.wsmod>", pd.display()))?;
            let (items, notes) = modproj::plan_install(&p, &ctx.index, &ctx.reader, &to)?;
            // 原版资源变化: 旧登记表里的 game-fill 哈希 vs 新版本补齐结果
            let oldf = registry::game_fill_hashes(&old, name);
            let mut changed = Vec::new();
            for it in &items {
                if let Some(h) = oldf.get(&it.rel.to_lowercase()) {
                    if *h != sha256_hex(&it.data) {
                        changed.push(it.rel.clone());
                    }
                }
            }
            let info = ModInfo { source: me.source.clone(), ..info_of(&p, &me.source, &ctx.install.build) };
            let mut r = registry::install(&to, &info, &items, &Opts { dry_run: a.dry_run, allow_same_ship: false })?;
            if !a.dry_run {
                project_manifest(&p, &to, &ctx.install.build, &items)?;
            }
            r["notes"] = json!(notes);
            r["game_files_updated_since_install"] = json!(changed);
            Ok(r)
        })();
        match one {
            Ok(r) => results.push(json!({"mod": name, "ok": true, "result": r})),
            Err(e) => {
                failed += 1;
                results.push(json!({"mod": name, "ok": false, "error": format!("{e:#}")}));
            }
        }
    }
    let mut old_removed = Vec::new();
    if !a.keep_old && !a.dry_run {
        for r in &results {
            if r["ok"] == true {
                let n = r["mod"].as_str().unwrap();
                old_removed.push(registry::uninstall(&from, n, false, false).unwrap_or_else(|e| json!({"mod": n, "error": format!("{e:#}")})));
            }
        }
    }
    let v = json!({"ok": failed == 0, "from": from.display().to_string(), "to": to.display().to_string(), "build": ctx.install.build, "mods": results, "failed": failed, "old_uninstalled": old_removed, "dry_run": a.dry_run});
    crate::guide::stash(&v);
    if cli.json {
        emit(cli, &v);
    } else {
        println!("迁移 {} → {}:", from.display(), to.display());
        for r in &results {
            if r["ok"] == true {
                let ch = r["result"]["game_files_updated_since_install"].as_array().map(|x| x.len()).unwrap_or(0);
                println!("  {} 完成{}", r["mod"].as_str().unwrap_or(""), if ch > 0 { format!(", 其中 {ch} 个原版资源已随游戏更新(已用新版补齐)") } else { String::new() });
            } else {
                println!("  {} 失败: {}", r["mod"].as_str().unwrap_or(""), r["error"].as_str().unwrap_or(""));
            }
        }
    }
    Ok(if failed == 0 { 0 } else { 2 })
}

fn forbidden(rel: &str) -> Option<&'static str> {
    let l = rel.to_lowercase();
    let name = l.rsplit('/').next().unwrap_or(&l);
    if name.starts_with("oo2core") || name.contains("oodle") {
        return Some("Oodle 运行库(专有, 不可分发)");
    }
    if name.starts_with("pfsunpack") || name.starts_with("mkut") {
        return Some("第三方专有解包工具");
    }
    for e in [".exe", ".dll", ".pyd", ".so", ".dylib", ".bat", ".cmd", ".ps1", ".msi"] {
        if name.ends_with(e) {
            return Some("可执行文件/脚本(包里不允许)");
        }
    }
    None
}

pub fn cmd_pack(cli: &Cli, a: &PackArgs) -> Result<i32> {
    let p = Project::open(&a.project)?;
    let meta = &p.cfg.meta;
    let lic = &p.cfg.license;
    if lic.no_redistribute && !a.allow_restricted {
        bail!(
            "项目含禁止二次配布/再分发的素材, 默认不打包(本地 wowsmod install 不受影响):\n  {}\n下一步: 只在本机用 install; 确需打包自用备份时加 --allow-restricted(包内会写明受限, 不得公开分发); 去掉受限素材后可在 mod.toml 删除 [license] 段",
            if lic.reasons.is_empty() { "mod.toml [license] no_redistribute = true".to_string() } else { lic.reasons.join("\n  ") }
        );
    }
    let mut entries = Vec::new();
    let mut files = Vec::new();
    let mut bad = Vec::new();
    let (mut n_stub, mut n_real) = (0, 0);
    for f in p.files()? {
        let rel = format!("{}/{}", modproj::ASSETS, modproj::rel_slash_pub(&p.assets(), &f));
        if let Some(why) = forbidden(&rel) {
            bad.push(format!("{rel}: {why}"));
            continue;
        }
        let d = std::fs::read(&f)?;
        let stub = modproj::is_binary_stub(&rel, &d);
        if stub {
            n_stub += 1;
        } else {
            n_real += 1;
        }
        files.push(json!({"path": rel, "sha256": sha256_hex(&d), "size": d.len(), "stub": stub}));
        entries.push(ZipEntry { name: rel, data: d });
    }
    if !bad.is_empty() {
        bail!("项目里有不能分发的文件, 未打包:\n  {}\n下一步: 从 assets/ 里删掉它们后重新 pack", bad.join("\n  "));
    }
    let toml = std::fs::read(p.root.join("mod.toml"))?;
    files.push(json!({"path": "mod.toml", "sha256": sha256_hex(&toml), "size": toml.len(), "stub": false}));
    entries.push(ZipEntry { name: "mod.toml".into(), data: toml });
    // 改动摘要(不含本机绝对路径)
    let mut edits = Vec::new();
    if let Ok(rd) = std::fs::read_dir(p.root.join("edits")) {
        let mut v: Vec<_> = rd.flatten().map(|e| e.path()).filter(|x| x.extension().map(|e| e == "json").unwrap_or(false)).collect();
        v.sort();
        for e in v {
            if let Ok(j) = serde_json::from_slice::<Value>(&std::fs::read(&e)?) {
                edits.push(json!({"file": e.file_name().unwrap().to_string_lossy(), "cmd": j["cmd"], "time_unix": j["time_unix"], "note": j["note"]}));
            }
        }
    }
    let mut full_n = 0;
    let mut full_note = Value::Null;
    if a.full {
        let ctx = game_ctx(cli, true)?.unwrap();
        let (items, _) = modproj::plan_install(&p, &ctx.index, &ctx.reader, Path::new("__pack_no_res_mods__"))?;
        for it in items {
            let name = format!("res_mods/{}", it.rel);
            if forbidden(&name).is_some() {
                continue;
            }
            files.push(json!({"path": name, "sha256": sha256_hex(&it.data), "size": it.data.len(), "stub": false, "origin": it.origin}));
            entries.push(ZipEntry { name, data: it.data });
            full_n += 1;
        }
        full_note = json!(format!("res_mods/ 下是完整可复制的文件({full_n} 个), 含从游戏客户端 {} 补齐的原版资源", ctx.install.build));
    }
    let readme = format!(
        "{name} {ver}\r\n{desc}\r\n\r\n目标船: {ship}    制作时客户端构建: {build}\r\n\r\n安装(推荐): 用 wowsmod\r\n  wowsmod install {name}-{ver}.{PKG_EXT}\r\n卸载:\r\n  wowsmod uninstall {name}\r\n\r\n说明: 包里只有 mod 自己的文件; SDK 占位文件({n_stub} 个)会在安装时从你自己的游戏客户端补齐,\r\n所以包里不含游戏原版资源, 也不含任何专有组件(Oodle 运行库、pfsunpack 等)。\r\n{full}{restricted}",
        restricted = if lic.no_redistribute { format!("\r\n【受限】本包含禁止二次配布的素材, 仅限打包者本人自用备份, 不得公开分发/上传/转赠:\r\n  {}\r\n", lic.reasons.join("\r\n  ")) } else { String::new() },
        name = meta.name,
        ver = meta.version,
        desc = meta.description,
        ship = meta.ship,
        build = meta.game_build,
        full = if a.full { "\r\n本包另附 res_mods/ 目录: 没有 wowsmod 时可把其中内容复制到 游戏目录\\bin\\<版本号>\\res_mods\\ 下(只适用于同一客户端版本; 含原版资源, 仅限自用)。\r\n" } else { "" }
    );
    entries.push(ZipEntry { name: "README.txt".into(), data: readme.into_bytes() });
    let man = json!({"format": 1, "tool": "wowsmod", "wowsmod_version": env!("CARGO_PKG_VERSION"), "name": meta.name, "version": meta.version, "description": meta.description,
        "ship": meta.ship, "mode": p.cfg.install.mode, "game_build": meta.game_build, "files": files, "stubs": n_stub, "real_files": n_real, "full": a.full, "full_note": full_note, "edits": edits});
    entries.insert(0, ZipEntry { name: PKG_MANIFEST.into(), data: serde_json::to_vec_pretty(&man)? });
    let z = zipfile::write(&entries).map_err(|e| anyhow!("{e}"))?;
    let fname = format!("{}-{}{}.{PKG_EXT}", meta.name, meta.version, if a.full { "-full" } else { "" });
    // -o 可以是文件(以 .wsmod 结尾)或目录(已存在的目录 / 没有扩展名的路径)
    let out = match &a.out {
        None => p.root.join("dist").join(&fname),
        Some(o) if o.is_dir() || o.extension().is_none() => o.join(&fname),
        Some(o) => o.clone(),
    };
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(&out, &z)?;
    // 自检: 读回并校验
    let back = zipfile::read(&std::fs::read(&out)?).map_err(|e| anyhow!("自检失败: {e}"))?;
    let v = json!({"ok": true, "package": out.display().to_string(), "bytes": z.len(), "sha256": sha256_hex(&z), "entries": back.len(), "mod_files": n_real, "stubs_filled_at_install": n_stub, "full_files": full_n, "excluded": ["oo2core*/oodle*", "pfsunpack*/mkut*", "*.exe/*.dll/脚本", ".install/", "edits/.undo"], "restricted": lic.no_redistribute});
    crate::guide::stash(&v);
    if cli.json {
        emit(cli, &v);
    } else {
        println!("已打包 {} ({} 字节, {} 个条目; mod 文件 {n_real}, 占位 {n_stub} 个在安装时从玩家游戏补齐{})", out.display(), z.len(), back.len(), if a.full { format!(", 附完整 res_mods {full_n} 个") } else { String::new() });
    }
    Ok(0)
}
