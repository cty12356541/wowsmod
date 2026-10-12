//! 游戏客户端定位与状态(doctor 使用)。
//!
//! 定位顺序(全部候选都会列出, 第一个通过校验的胜出):
//! 1. `--game <目录>` 或环境变量 `WOWSMOD_GAME`
//! 2. 注册表 Uninstall 项(Lesta 安装器写入的 `LGC-*` 项 InstallLocation / UninstallString)
//! 3. Lesta Game Center 启动器配置 `preferences.xml` 中的 `<working_dir>` / `<current_game>` / `<MK>`
//!    (启动器位置取自注册表 "Lesta Game Center" 卸载项或 `lgc://` 协议处理程序)
//! 4. 常见目录 + 所有盘符的浅层扫描(X:\Korabli, X:\Games\*, X:\Lesta\* ...)
//!
//! 校验: 目录下有 `res_packages\`, `bin\<数字>\idx\*.idx`; `game_info.xml` 的 `<id>` 以 `MK.` 开头为莱服。
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub path: String,
    pub source: String,
    pub valid: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameInstall {
    pub root: PathBuf,
    pub game_id: Option<String>,
    pub is_lesta: bool,
    pub client_version: Option<String>,
    pub build: String,
    pub bin_dirs: Vec<String>,
    pub idx_dir: PathBuf,
    pub res_packages: PathBuf,
    pub res_mods: PathBuf,
    pub found_by: String,
}

fn read_text(p: &Path) -> Option<String> {
    let b = std::fs::read(p).ok()?;
    Some(String::from_utf8_lossy(&b).into_owned())
}

fn xml_values(text: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(a) = rest.find(&open) {
        let s = &rest[a + open.len()..];
        if let Some(b) = s.find(&close) {
            out.push(s[..b].trim().to_string());
            rest = &s[b..];
        } else {
            break;
        }
    }
    out
}

/// 列出 bin 下的数字子目录(降序), 只保留含 idx/*.idx 的。
pub fn bin_builds(root: &Path) -> Vec<(u64, PathBuf, bool)> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root.join("bin")) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if let Ok(n) = name.parse::<u64>() {
                let idx = e.path().join("idx");
                let has_idx = std::fs::read_dir(&idx)
                    .map(|r| r.flatten().any(|f| f.path().extension().map(|x| x.eq_ignore_ascii_case("idx")).unwrap_or(false)))
                    .unwrap_or(false);
                v.push((n, e.path(), has_idx));
            }
        }
    }
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v
}

pub fn validate(root: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Err("目录不存在".into());
    }
    if !root.join("res_packages").is_dir() {
        return Err("缺少 res_packages\\".into());
    }
    if !bin_builds(root).iter().any(|b| b.2) {
        return Err("bin\\<版本号>\\idx\\ 下没有 .idx".into());
    }
    Ok(())
}

pub fn describe(root: &Path, found_by: &str) -> Result<GameInstall, String> {
    validate(root)?;
    let info = read_text(&root.join("game_info.xml"));
    let game_id = info.as_deref().and_then(|t| xml_values(t, "id").into_iter().next());
    let client_version = info.as_deref().and_then(|t| {
        let i = t.find("name=\"client\"")?;
        let s = &t[i..];
        let a = s.find("installed=\"")? + "installed=\"".len();
        let b = s[a..].find('"')? + a;
        Some(s[a..b].to_string())
    });
    let builds = bin_builds(root);
    let (bn, bdir, _) = builds.iter().find(|b| b.2).cloned().ok_or("没有可用的 bin 版本目录")?;
    let is_lesta = game_id.as_deref().map(|s| s.starts_with("MK.")).unwrap_or(false)
        || root.join("Korabli.exe").exists()
        || root.join("lgc_api.exe").exists();
    Ok(GameInstall {
        root: root.to_path_buf(),
        game_id,
        is_lesta,
        client_version,
        build: bn.to_string(),
        bin_dirs: builds.iter().map(|b| format!("{}{}", b.0, if b.2 { "" } else { " (无 idx)" })).collect(),
        idx_dir: bdir.join("idx"),
        res_packages: root.join("res_packages"),
        res_mods: bdir.join("res_mods"),
        found_by: found_by.to_string(),
    })
}

#[cfg(windows)]
mod win {
    use winreg::enums::*;
    use winreg::{RegKey, HKEY};

    pub struct UninstallEntry {
        pub display_name: String,
        pub install_location: String,
        pub uninstall_string: String,
        pub display_version: String,
        pub key: String,
    }

    pub fn uninstall_entries() -> Vec<UninstallEntry> {
        let mut out = Vec::new();
        let roots = [
            (HKEY_CURRENT_USER, "HKCU", r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
            (HKEY_LOCAL_MACHINE, "HKLM", r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
            (HKEY_LOCAL_MACHINE, "HKLM", r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
        ];
        for (hive, hname, path) in roots {
            let Ok(k) = RegKey::predef(hive).open_subkey_with_flags(path, KEY_READ) else { continue };
            for name in k.enum_keys().flatten() {
                let Ok(sk) = k.open_subkey_with_flags(&name, KEY_READ) else { continue };
                let g = |v: &str| sk.get_value::<String, _>(v).unwrap_or_default();
                out.push(UninstallEntry {
                    display_name: g("DisplayName"),
                    install_location: g("InstallLocation"),
                    uninstall_string: g("UninstallString"),
                    display_version: g("DisplayVersion"),
                    key: format!(r"{hname}\{path}\{name}"),
                });
            }
        }
        out
    }

    pub fn reg_default(hive: HKEY, path: &str) -> Option<String> {
        RegKey::predef(hive).open_subkey_with_flags(path, KEY_READ).ok()?.get_value::<String, _>("").ok()
    }
    pub use winreg::enums::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER};
}

/// 从命令行字符串中取出 exe 所在目录, 如 `"D:\Korabli\lgc_api.exe" --uninstall` → D:\Korabli
#[allow(dead_code)]
fn exe_dir_of(cmd: &str) -> Option<PathBuf> {
    let c = cmd.trim();
    let exe = if let Some(rest) = c.strip_prefix('"') { rest.split('"').next()? } else { c.split(".exe").next().map(|s| s.trim())? };
    let exe = if exe.to_lowercase().ends_with(".exe") { exe.to_string() } else { format!("{exe}.exe") };
    Path::new(&exe).parent().map(|p| p.to_path_buf())
}

pub fn drive_roots() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        (b'A'..=b'Z').map(|c| PathBuf::from(format!("{}:\\", c as char))).filter(|p| p.is_dir()).collect()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// 收集所有候选目录(按优先级), 并逐一校验。
pub fn candidates(explicit: Option<&Path>) -> Vec<Candidate> {
    let mut list: Vec<(PathBuf, String)> = Vec::new();
    if let Some(p) = explicit {
        list.push((p.to_path_buf(), "--game 参数".into()));
    }
    if let Ok(p) = std::env::var("WOWSMOD_GAME") {
        list.push((PathBuf::from(p), "环境变量 WOWSMOD_GAME".into()));
    }
    #[cfg(windows)]
    {
        let entries = win::uninstall_entries();
        for e in &entries {
            let n = e.display_name.to_lowercase();
            let looks = e.key.contains("LGC-")
                || n.contains("мир кораблей")
                || n.contains("korabli")
                || n.contains("mir korabley")
                || e.uninstall_string.to_lowercase().contains("lgc_api.exe");
            if looks {
                if !e.install_location.is_empty() {
                    list.push((PathBuf::from(&e.install_location), format!("注册表 {} InstallLocation", e.key)));
                }
                if let Some(d) = exe_dir_of(&e.uninstall_string) {
                    list.push((d, format!("注册表 {} UninstallString", e.key)));
                }
            }
        }
        // Lesta Game Center 启动器配置
        let mut lgc_dirs: Vec<(PathBuf, String)> = Vec::new();
        for e in &entries {
            if e.display_name.to_lowercase().contains("lesta game center") {
                if let Some(d) = exe_dir_of(&e.uninstall_string) {
                    lgc_dirs.push((d, format!("注册表 {}", e.key)));
                }
                if !e.install_location.is_empty() {
                    lgc_dirs.push((PathBuf::from(&e.install_location), format!("注册表 {}", e.key)));
                }
            }
        }
        for key in [r"Software\Classes\lgc\shell\open\command"] {
            if let Some(cmd) = win::reg_default(win::HKEY_CURRENT_USER, key) {
                if let Some(d) = exe_dir_of(&cmd) {
                    lgc_dirs.push((d, format!(r"注册表 HKCU\{key} (lgc:// 协议)")));
                }
            }
        }
        if let Some(cmd) = win::reg_default(win::HKEY_CLASSES_ROOT, r"lgc\shell\open\command") {
            if let Some(d) = exe_dir_of(&cmd) {
                lgc_dirs.push((d, r"注册表 HKCR\lgc (lgc:// 协议)".into()));
            }
        }
        for v in ["ProgramData", "LOCALAPPDATA", "APPDATA"] {
            if let Ok(b) = std::env::var(v) {
                lgc_dirs.push((PathBuf::from(b).join("Lesta").join("GameCenter"), format!("默认位置 %{v}%")));
            }
        }
        for d in drive_roots() {
            lgc_dirs.push((d.join("Lesta").join("GameCenter"), "默认位置".into()));
        }
        for (d, why) in lgc_dirs {
            if let Some(t) = read_text(&d.join("preferences.xml")) {
                for tag in ["working_dir", "current_game", "MK"] {
                    for v in xml_values(&t, tag) {
                        if !v.is_empty() {
                            list.push((PathBuf::from(v), format!("启动器配置 {}\\preferences.xml <{tag}> (启动器来自{why})", d.display())));
                        }
                    }
                }
            }
        }
    }
    // 常见目录 + 盘符浅层扫描
    let names = ["Korabli", "Mir Korabley", "Мир кораблей", "Lesta\\Korabli", "Games\\Korabli", "Games\\Мир кораблей", "Program Files\\Lesta\\Korabli"];
    for d in drive_roots() {
        for n in names {
            list.push((d.join(n), "常见目录".into()));
        }
        for sub in ["", "Games", "Lesta", "Program Files", "Program Files (x86)"] {
            let base = if sub.is_empty() { d.clone() } else { d.join(sub) };
            if let Ok(rd) = std::fs::read_dir(&base) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.join("game_info.xml").is_file() && p.join("res_packages").is_dir() {
                        list.push((p, format!("盘符扫描 {}", base.display())));
                    }
                }
            }
        }
    }
    // 去重(大小写不敏感) + 校验
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (p, src) in list {
        let key = p.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase();
        let exists = p.is_dir();
        if !seen.insert(key) {
            continue;
        }
        // 不存在的"常见目录"不列出, 避免噪音
        if !exists && src == "常见目录" {
            continue;
        }
        let (valid, note) = match validate(&p) {
            Ok(()) => (true, "通过校验".to_string()),
            Err(e) => (false, e),
        };
        out.push(Candidate { path: p.display().to_string(), source: src, valid, note });
    }
    out
}

pub fn locate(explicit: Option<&Path>) -> (Option<GameInstall>, Vec<Candidate>) {
    let c = candidates(explicit);
    // 显式指定时只认显式路径
    if let Some(p) = explicit {
        let gi = describe(p, "--game 参数").ok();
        return (gi, c);
    }
    for cand in &c {
        if cand.valid {
            if let Ok(gi) = describe(Path::new(&cand.path), &cand.source) {
                return (Some(gi), c);
            }
        }
    }
    (None, c)
}

#[derive(Debug, Clone, Serialize)]
pub struct ResModsState {
    pub path: String,
    pub exists: bool,
    pub files: usize,
    pub bytes: u64,
    pub top_level: Vec<String>,
    pub pnf_mods: Vec<String>,
    /// 只有 readme.txt(或为空)
    pub pristine: bool,
}

pub fn res_mods_state(dir: &Path) -> ResModsState {
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    files += 1;
                    bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
                }
            }
        }
    }
    let mut top: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| format!("{}{}", e.file_name().to_string_lossy(), if e.path().is_dir() { "\\" } else { "" })).collect())
        .unwrap_or_default();
    top.sort();
    let mut pnf: Vec<String> = std::fs::read_dir(dir.join("PnFMods"))
        .map(|rd| rd.flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().to_string()).collect())
        .unwrap_or_default();
    pnf.sort();
    let pristine = top.iter().all(|t| t.eq_ignore_ascii_case("readme.txt"));
    ResModsState { path: dir.display().to_string(), exists: dir.is_dir(), files, bytes, top_level: top, pnf_mods: pnf, pristine }
}

#[derive(Debug, Clone, Serialize)]
pub struct BlenderInfo {
    pub path: String,
    pub version: String,
    pub source: String,
}

pub fn find_blender() -> Vec<BlenderInfo> {
    let mut out: Vec<BlenderInfo> = Vec::new();
    let mut push = |exe: PathBuf, version: String, source: String| {
        if exe.is_file() && !out.iter().any(|b| b.path.eq_ignore_ascii_case(&exe.display().to_string())) {
            out.push(BlenderInfo { path: exe.display().to_string(), version, source });
        }
    };
    #[cfg(windows)]
    {
        for e in win::uninstall_entries() {
            if e.display_name.to_lowercase().starts_with("blender") && !e.install_location.is_empty() {
                push(PathBuf::from(&e.install_location).join("blender.exe"), e.display_version.clone(), format!("注册表 {}", e.key));
            }
        }
        if let Some(cmd) = win::reg_default(win::HKEY_CLASSES_ROOT, r"blendfile\shell\open\command") {
            if let Some(d) = exe_dir_of(&cmd) {
                push(d.join("blender.exe"), String::new(), r"注册表 HKCR\blendfile (.blend 关联)".into());
            }
        }
        for v in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Ok(pf) = std::env::var(v) {
                let base = PathBuf::from(pf).join("Blender Foundation");
                if let Ok(rd) = std::fs::read_dir(&base) {
                    for e in rd.flatten() {
                        let ver = e.file_name().to_string_lossy().trim_start_matches("Blender").trim().to_string();
                        push(e.path().join("blender.exe"), ver, format!("默认目录 {}", base.display()));
                    }
                }
            }
        }
        // Steam 版
        for d in drive_roots() {
            for s in [r"Program Files (x86)\Steam\steamapps\common\Blender", r"SteamLibrary\steamapps\common\Blender"] {
                push(d.join(s).join("blender.exe"), String::new(), "Steam".into());
            }
        }
    }
    let exe_name = if cfg!(windows) { "blender.exe" } else { "blender" };
    // 显式指定
    if let Some(p) = std::env::var_os("WOWSMOD_BLENDER") {
        let p = PathBuf::from(p);
        let exe = if p.is_dir() { p.join(exe_name) } else { p };
        push(exe, String::new(), "环境变量 WOWSMOD_BLENDER".into());
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            push(d.join(exe_name), String::new(), "PATH".into());
        }
    }
    // 便携版: wowsmod.exe 所在目录及其上 4 级祖先目录下的 blender*/ 与 tools/blender*/
    // (例如 <WORK_DIR>\tools\blender-5.2.2-windows-x64\blender.exe)
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Ok(me) = std::env::current_exe() {
        bases.extend(me.ancestors().skip(1).take(5).map(Path::to_path_buf));
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.extend(cwd.ancestors().take(5).map(Path::to_path_buf));
    }
    for base in bases {
        for sub in [base.clone(), base.join("tools")] {
            if let Ok(rd) = std::fs::read_dir(&sub) {
                for e in rd.flatten() {
                    if e.file_name().to_string_lossy().to_ascii_lowercase().starts_with("blender") && e.path().is_dir() {
                        push(e.path().join(exe_name), String::new(), format!("便携版 (在 {} 下找到)", sub.display()));
                    }
                }
            }
        }
    }
    // 版本号补全: 从同目录下形如 "4.2" 的子目录推断
    for b in out.iter_mut() {
        if b.version.is_empty() {
            if let Some(dir) = Path::new(&b.path).parent() {
                if let Ok(rd) = std::fs::read_dir(dir) {
                    let mut vs: Vec<String> = rd
                        .flatten()
                        .filter(|e| e.path().is_dir())
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .filter(|n| n.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) && n.contains('.'))
                        .collect();
                    vs.sort();
                    if let Some(v) = vs.pop() {
                        b.version = v;
                    } else if let Some(n) = dir.file_name().map(|n| n.to_string_lossy().to_string()) {
                        // blender-5.2.2-windows-x64 → 5.2.2
                        if let Some(v) = n.split('-').find(|x| x.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)) {
                            b.version = format!("{v} (按目录名推断)");
                        }
                    }
                }
            }
        }
    }
    out
}

/// 游戏客户端是否在运行(Windows: tasklist; 其他平台恒为 false)
pub fn running() -> Vec<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("tasklist").args(["/FO", "CSV", "/NH"]).creation_flags(0x0800_0000).output();
        if let Ok(o) = out {
            let s = String::from_utf8_lossy(&o.stdout).to_lowercase();
            return ["korabli64.exe", "korabli.exe", "worldofwarships64.exe", "worldofwarships.exe"].iter().filter(|n| s.contains(&format!("\"{n}\""))).map(|s| s.to_string()).collect();
        }
        Vec::new()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}
