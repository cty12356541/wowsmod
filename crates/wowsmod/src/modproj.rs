//! mod 项目文件夹(M2 初版格式, 为 M4 铺路)与开发安装/卸载。
//!
//! ```text
//! <项目>/
//!   mod.toml                 # 元数据
//!   edits/NNN_<命令>.json    # 每次 tex/glow 等操作的记录(参数、输入哈希、输出文件、统计)
//!   assets/res_mods/...      # 产物, 路径即相对 res_mods 的路径
//!   .install/manifest.json   # install --dev 写入的清单(卸载依据)
//!   .install/backup/...      # 安装时被覆盖的原有文件(卸载时还原)
//! ```
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use wowsmod_formats::{dds, mfm};

pub const ASSETS: &str = "assets/res_mods";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModToml {
    #[serde(rename = "mod")]
    pub meta: ModMeta,
    #[serde(default)]
    pub install: InstallCfg,
    /// 素材许可: 含"禁止二次配布"类素材时置位, pack 默认拒绝(本地 install 不受影响)
    #[serde(default, skip_serializing_if = "LicenseCfg::is_empty")]
    pub license: LicenseCfg,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LicenseCfg {
    /// true = 项目含禁止再分发的素材; `wowsmod pack` 默认拒绝, 需 `--allow-restricted` 显式覆盖
    #[serde(default)]
    pub no_redistribute: bool,
    /// 来源说明(哪个素材、哪个规则文件、命中的字句)
    #[serde(default)]
    pub reasons: Vec<String>,
}
impl LicenseCfg {
    pub fn is_empty(&self) -> bool {
        !self.no_redistribute && self.reasons.is_empty()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModMeta {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// 制作时的客户端构建号(如 8867689)
    #[serde(default)]
    pub game_build: String,
    /// 目标船(SDK 名, 如 FSB510_Bourgogne)
    #[serde(default)]
    pub ship: String,
    #[serde(default = "default_fmt")]
    pub format_version: u32,
    /// 真实舰长(米, 可选): 给了就在导入人物/道具时把 visual 单位换算成米
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub ship_length_m: f64,
}
fn is_zero_f64(x: &f64) -> bool {
    *x == 0.0
}
fn default_version() -> String {
    "0.1.0".into()
}
fn default_fmt() -> u32 {
    1
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InstallCfg {
    /// loose: assets 直接覆盖 res_mods 下的游戏路径; sdk: 官方 ModsSDK 路线(PnFMods/<名>/Main.py)
    #[serde(default = "default_mode")]
    pub mode: String,
}
impl Default for InstallCfg {
    fn default() -> Self {
        InstallCfg { mode: default_mode() }
    }
}
fn default_mode() -> String {
    "loose".into()
}

pub struct Project {
    pub root: PathBuf,
    pub cfg: ModToml,
}

/// 明文类扩展名(这些文件本身就是小 XML/脚本, 不是占位)
pub fn is_text_ext(rel: &str) -> bool {
    let ext = rel.rsplit('.').next().unwrap_or("").to_lowercase();
    matches!(ext.as_str(), "mfm" | "visual" | "model" | "info" | "py" | "txt" | "xml" | "toml" | "json")
}

/// 二进制资源的 SDK 占位文件(.dds/.geometry/.splash 等写成了一行 XML)
pub fn is_binary_stub(rel: &str, d: &[u8]) -> bool {
    !is_text_ext(rel) && mfm::is_sdk_stub(d)
}

pub fn sha256_hex(d: &[u8]) -> String {
    let h = Sha256::digest(d);
    h.iter().map(|b| format!("{b:02x}")).collect()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let mut st = vec![dir.to_path_buf()];
    while let Some(d) = st.pop() {
        for e in std::fs::read_dir(&d).with_context(|| format!("读取目录 {}", d.display()))?.flatten() {
            let p = e.path();
            if p.is_dir() {
                st.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(())
}

fn rel_slash(base: &Path, p: &Path) -> String {
    p.strip_prefix(base).unwrap_or(p).components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")
}

/// main 对执行类命令置位; 打开项目时检查设计书是否定稿(每进程只提示一次)
pub static EXEC_CMD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static BOOK_HINTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl Project {
    pub fn open(root: &Path) -> Result<Project> {
        let t = std::fs::read_to_string(root.join("mod.toml")).with_context(|| format!("{} 不是 mod 项目(缺少 mod.toml; 先运行 wowsmod mod init)", root.display()))?;
        let cfg: ModToml = toml::from_str(&t).map_err(|e| anyhow!("解析 mod.toml: {e}"))?;
        use std::sync::atomic::Ordering;
        if EXEC_CMD.load(Ordering::Relaxed) && !BOOK_HINTED.swap(true, Ordering::Relaxed) {
            crate::design::draft_hint(root);
        }
        Ok(Project { root: root.to_path_buf(), cfg })
    }
    pub fn init(root: &Path, name: &str, ship: &str, mode: &str, build: &str) -> Result<Project> {
        if root.join("mod.toml").exists() {
            bail!("{} 已经是 mod 项目", root.display());
        }
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || name.is_empty() {
            bail!("mod 名只能用 ASCII 字母/数字/下划线(会成为 PnFMods/<名>/ 目录名): {name:?}");
        }
        if mode != "loose" && mode != "sdk" {
            bail!("--mode 只能是 loose 或 sdk");
        }
        std::fs::create_dir_all(root.join("edits"))?;
        std::fs::create_dir_all(root.join(ASSETS))?;
        let cfg = ModToml {
            meta: ModMeta { name: name.into(), version: default_version(), description: String::new(), game_build: build.into(), ship: ship.into(), format_version: 1, ship_length_m: 0.0 },
            install: InstallCfg { mode: mode.into() },
            license: LicenseCfg::default(),
        };
        let mut s = String::from("# wowsmod mod 项目(格式版本 1)。assets/res_mods 下的文件按相对路径安装到游戏 res_mods。\n");
        s += &toml::to_string_pretty(&cfg).map_err(|e| anyhow!("{e}"))?;
        std::fs::write(root.join("mod.toml"), s)?;
        std::fs::write(root.join(".gitignore"), ".install/\n")?;
        Ok(Project { root: root.to_path_buf(), cfg })
    }
    /// 重写 mod.toml(保留首行注释)
    pub fn save_cfg(&self) -> Result<()> {
        let mut s = String::from("# wowsmod mod 项目(格式版本 1)。assets/res_mods 下的文件按相对路径安装到游戏 res_mods。\n");
        s += &toml::to_string_pretty(&self.cfg).map_err(|e| anyhow!("{e}"))?;
        std::fs::write(self.root.join("mod.toml"), s)?;
        Ok(())
    }
    /// 标记项目含禁止再分发素材(幂等); 返回是否新加了理由
    pub fn mark_restricted(&mut self, reason: &str) -> Result<bool> {
        let new = !self.cfg.license.reasons.iter().any(|r| r == reason);
        if new {
            self.cfg.license.reasons.push(reason.to_string());
        }
        if new || !self.cfg.license.no_redistribute {
            self.cfg.license.no_redistribute = true;
            self.save_cfg()?;
        }
        Ok(new)
    }
    pub fn assets(&self) -> PathBuf {
        self.root.join(ASSETS)
    }
    pub fn files(&self) -> Result<Vec<PathBuf>> {
        let mut v = Vec::new();
        walk(&self.assets(), &mut v)?;
        Ok(v)
    }
    /// 项目 assets 里按文件名(不区分大小写)查找
    pub fn find_by_name(&self, name: &str) -> Result<Vec<PathBuf>> {
        let n = name.to_lowercase();
        Ok(self.files()?.into_iter().filter(|p| p.file_name().map(|f| f.to_string_lossy().to_lowercase() == n).unwrap_or(false)).collect())
    }
    pub fn rel(&self, p: &Path) -> String {
        rel_slash(&self.root, p)
    }
    /// 记录一次编辑, 返回记录文件路径
    pub fn record_edit(&self, cmd: &str, mut rec: Value) -> Result<PathBuf> {
        let dir = self.root.join("edits");
        std::fs::create_dir_all(&dir)?;
        let n = std::fs::read_dir(&dir)?.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".json")).count();
        let p = dir.join(format!("{:03}_{cmd}.json", n + 1));
        rec["cmd"] = json!(cmd);
        rec["argv"] = json!(std::env::args().collect::<Vec<_>>());
        rec["time_unix"] = json!(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0));
        rec["wowsmod_version"] = json!(env!("CARGO_PKG_VERSION"));
        std::fs::write(&p, serde_json::to_string_pretty(&rec)?)?;
        Ok(p)
    }
}

/// 改 mod 名: 目录 PnFMods/<旧>/ → PnFMods/<新>/, 明文文件(.mfm/.visual/.model/.xml/.txt/.py)与 compile.info 里的
/// `PnFMods/<旧>/` 引用一并替换, 最后写 mod.toml。compile.info 是 base64+zlib JSON(游戏按它做路径替换),
/// 只改明文不改它 → 游戏加载旧路径失败、港口卡在载入。
pub fn rename_mod(p: &mut Project, new: &str) -> Result<Value> {
    let old = p.cfg.meta.name.clone();
    if new.is_empty() || new.contains(['/', '\\', ' ']) {
        bail!("新名字不能为空或含 / \\ 空格");
    }
    if old == new {
        bail!("新旧名字相同: {new}");
    }
    let base = p.assets().join("PnFMods");
    let (src, dst) = (base.join(&old), base.join(new));
    if dst.exists() {
        bail!("{} 已存在", dst.display());
    }
    let (from, to) = (format!("PnFMods/{old}/"), format!("PnFMods/{new}/"));
    let (mut n_text, mut n_ci) = (0, 0);
    if src.is_dir() {
        let mut files = Vec::new();
        walk(&src, &mut files)?;
        for f in &files {
            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if name == "compile.info" {
                let ci = std::fs::read_to_string(f)?;
                let js = mfm::decode_compile_info(&ci)?;
                if js.contains(&from) {
                    std::fs::write(f, mfm::encode_compile_info(&ci, &js.replace(&from, &to))?)?;
                    n_ci += 1;
                }
            } else if matches!(ext.as_str(), "mfm" | "visual" | "model" | "xml" | "txt" | "py") {
                let d = std::fs::read(f)?;
                if let Ok(t) = String::from_utf8(d) {
                    if t.contains(&from) {
                        std::fs::write(f, t.replace(&from, &to))?;
                        n_text += 1;
                    }
                }
            }
        }
        std::fs::rename(&src, &dst)?;
    }
    p.cfg.meta.name = new.to_string();
    p.save_cfg()?;
    Ok(json!({"ok": true, "old_mod_name": old, "new_mod_name": new, "text_files_changed": n_text, "compile_info_changed": n_ci,
        "dir_renamed": dst.is_dir(), "note": "已安装的旧名请先 wowsmod uninstall <旧名>; edits/ 历史里的旧路径不改"}))
}

/// 导入 extractSources 产物(占位文件 + 明文 .mfm/.visual/.model + compile.info)并改名为本项目 mod 名。
/// 文本文件里的 `PnFMods/<旧名>/` 统一替换为 `PnFMods/<新名>/`, compile.info 解码-替换-重编码。
pub fn import_sdk(p: &Project, src: &Path, sdk_dir: Option<&Path>) -> Result<Value> {
    let ship = &p.cfg.meta.ship;
    if ship.is_empty() {
        bail!("mod.toml 里没有 ship(如 FSB510_Bourgogne)");
    }
    // src 可以是 <Mod> 目录(含 <ship>/)或 <ship> 目录本身
    let ship_dir = if src.join(ship).is_dir() { src.join(ship) } else { src.to_path_buf() };
    let ci = std::fs::read_to_string(ship_dir.join("compile.info")).with_context(|| format!("{} 里没有 compile.info(不是 extractSources 产物?)", ship_dir.display()))?;
    let js = mfm::decode_compile_info(&ci)?;
    // 旧 mod 名: 从 compile.info 的值 "PnFMods/<旧名>/<ship>/..." 推出
    let v: Value = serde_json::from_str(&js)?;
    let old = v
        .as_object()
        .and_then(|o| o.values().filter_map(|x| x.as_str()).find_map(|s| s.strip_prefix("PnFMods/").and_then(|r| r.split('/').next()).map(|s| s.to_string())))
        .ok_or_else(|| anyhow!("compile.info 里找不到 PnFMods/<名>/ 前缀"))?;
    let new = &p.cfg.meta.name;
    let (from, to) = (format!("PnFMods/{old}/"), format!("PnFMods/{new}/"));
    let dst_root = p.assets().join("PnFMods").join(new).join(ship);
    let mut files = Vec::new();
    walk(&ship_dir, &mut files)?;
    let (mut n_text, mut n_stub, mut n_bin, mut n_renamed) = (0, 0, 0, 0);
    for f in &files {
        let rel = rel_slash(&ship_dir, f);
        let dst = dst_root.join(&rel);
        std::fs::create_dir_all(dst.parent().unwrap())?;
        let d = std::fs::read(f)?;
        let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if rel == "compile.info" {
            let nj = js.replace(&from, &to);
            std::fs::write(&dst, mfm::encode_compile_info(&ci, &nj)?)?;
            n_text += 1;
            n_renamed += 1;
        } else if matches!(ext.as_str(), "mfm" | "visual" | "model" | "xml" | "txt") {
            let t = String::from_utf8_lossy(&d);
            if t.contains(&from) {
                n_renamed += 1;
            }
            std::fs::write(&dst, t.replace(&from, &to))?;
            n_text += 1;
        } else {
            if mfm::is_sdk_stub(&d) {
                n_stub += 1;
            } else {
                n_bin += 1;
            }
            std::fs::write(&dst, &d)?;
        }
    }
    // Main.py: registerShipMod; 根目录的 PnFModsLoader.py 由 install 负责(若 res_mods 里没有)
    let main_py = p.assets().join("PnFMods").join(new).join("Main.py");
    std::fs::write(&main_py, format!("API_VERSION = 'API_v1.0'\nSHIP_NAME = '{ship}'\ncontentSdk.registerShipMod(SHIP_NAME)\n"))?;
    let mut sdk_n = 0;
    if let Some(sd) = sdk_dir {
        let sd = if sd.join(ship).is_dir() { sd.join(ship) } else { sd.to_path_buf() };
        let dst = p.assets().join("PnFMods").join("ModsSDK.zip").join(ship);
        let mut v = Vec::new();
        walk(&sd, &mut v)?;
        for f in v {
            let o = dst.join(rel_slash(&sd, &f));
            std::fs::create_dir_all(o.parent().unwrap())?;
            std::fs::copy(&f, &o)?;
            sdk_n += 1;
        }
    }
    Ok(json!({"old_mod_name": old, "new_mod_name": new, "files": files.len(), "text_files": n_text, "renamed_refs_in_files": n_renamed, "stubs": n_stub, "binary": n_bin, "sdk_files": sdk_n, "dest": dst_root.display().to_string()}))
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ManifestFile {
    pub rel: String,
    pub sha256: String,
    pub size: u64,
    /// project / game-fill / generated
    pub origin: String,
    /// 安装前该位置已有文件 → 备份在 .install/backup/<rel>
    #[serde(default)]
    pub backed_up: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Manifest {
    pub mod_name: String,
    pub res_mods: String,
    pub game_build: String,
    pub time_unix: u64,
    pub files: Vec<ManifestFile>,
    /// install 新建的目录(卸载时若为空则删除), 深的在后
    pub created_dirs: Vec<String>,
    pub wowsmod_version: String,
}

pub struct GameLookup<'a> {
    pub by_name: BTreeMap<String, Vec<&'a wowsmod_formats::index::Entry>>,
}

impl<'a> GameLookup<'a> {
    pub fn new(ix: &'a wowsmod_formats::index::GameIndex) -> Self {
        let mut by_name: BTreeMap<String, Vec<&wowsmod_formats::index::Entry>> = BTreeMap::new();
        for e in &ix.entries {
            let n = e.path.rsplit('/').next().unwrap_or(&e.path).to_lowercase();
            by_name.entry(n).or_default().push(e);
        }
        GameLookup { by_name }
    }
    /// 按文件名找唯一的游戏文件。重名时: 若所有候选 crc+大小相同则取第一个, 否则报错。
    pub fn unique(&self, name: &str, hint_dir: Option<&str>) -> Result<&'a wowsmod_formats::index::Entry> {
        let v = self.by_name.get(&name.to_lowercase()).ok_or_else(|| anyhow!("游戏包里没有名为 {name} 的文件"))?;
        if v.len() == 1 {
            return Ok(v[0]);
        }
        let same = v.iter().all(|e| e.info.unpacked_size == v[0].info.unpacked_size && e.info.crc32 == v[0].info.crc32);
        if same {
            return Ok(v[0]);
        }
        if let Some(h) = hint_dir {
            let hv: Vec<_> = v.iter().filter(|e| e.path.to_lowercase().starts_with(&h.to_lowercase())).collect();
            if hv.len() == 1 {
                return Ok(hv[0]);
            }
        }
        // 船模用到的资源都在 content/gameplay/ 下(M0 补齐时 default_ao.dds 等重名文件也取这里)
        let cg: Vec<_> = v.iter().filter(|e| e.path.to_lowercase().starts_with("content/gameplay/")).collect();
        if cg.len() == 1 {
            return Ok(cg[0]);
        }
        bail!("游戏包里有 {} 个名为 {name} 的不同文件, 无法确定: {}", v.len(), v.iter().map(|e| e.path.as_str()).collect::<Vec<_>>().join(", "))
    }
}

#[derive(Clone)]
pub struct InstallItem {
    pub rel: String,
    pub data: Vec<u8>,
    pub origin: String,
}

/// 收集要安装的全部文件: 项目 assets + 占位补齐(含分卷) + 分卷完整性检查 + SDK 根加载器。
/// 内置 M0 的两个坑: (1) 贴图必须连同 .dd0–.ddN 一起放; (2) SDK 占位文件要从游戏包补齐。
pub fn plan_install(p: &Project, ix: &wowsmod_formats::index::GameIndex, reader: &wowsmod_formats::pkg::PkgReader, res_mods: &Path) -> Result<(Vec<InstallItem>, Vec<String>)> {
    let look = GameLookup::new(ix);
    let mut items: BTreeMap<String, InstallItem> = BTreeMap::new();
    let mut notes = Vec::new();
    let base = p.assets();
    for f in p.files()? {
        let rel = rel_slash(&base, &f);
        let d = std::fs::read(&f)?;
        items.insert(rel.to_lowercase(), InstallItem { rel, data: d, origin: "project".into() });
    }
    // (2) 占位补齐
    let keys: Vec<String> = items.keys().cloned().collect();
    let mut filled = 0;
    for k in &keys {
        let it = &items[k];
        if !is_binary_stub(&it.rel, &it.data) {
            continue;
        }
        let ext = it.rel.rsplit('.').next().unwrap_or("").to_lowercase();
        let name = it.rel.rsplit('/').next().unwrap().to_string();
        let e = look.unique(&name, None).with_context(|| format!("补齐 SDK 占位文件 {}", it.rel))?;
        let mut data = reader.read(e).map_err(|x| anyhow!("读取 {}: {x}", e.path))?;
        let rel = it.rel.clone();
        // 贴图: 一并补齐分卷(游戏里存在的 .ddN)
        if ext == "dds" {
            if wowsmod_formats::bc7prep::is_bc7prep_dds(&data) {
                data = wowsmod_formats::bc7prep::decode_dds(&data).map_err(|x| anyhow!("{}: {x}", e.path))?;
            }
            let stem = &e.path[..e.path.len() - 4];
            for n in 0..8 {
                let vp = format!("{stem}.dd{n}");
                let Some(ve) = ix.get(&vp) else { break };
                let mut vd = reader.read(ve).map_err(|x| anyhow!("读取 {vp}: {x}"))?;
                if wowsmod_formats::bc7prep::is_bc7prep_dds(&vd) {
                    vd = wowsmod_formats::bc7prep::decode_dds(&vd).map_err(|x| anyhow!("{vp}: {x}"))?;
                }
                let vrel = format!("{}.dd{n}", &rel[..rel.len() - 4]);
                let vk = vrel.to_lowercase();
                if items.get(&vk).map(|x| !mfm::is_sdk_stub(&x.data)).unwrap_or(false) {
                    continue; // 项目里已有真实分卷
                }
                items.insert(vk, InstallItem { rel: vrel, data: vd, origin: format!("game-fill:{vp}") });
            }
        }
        items.insert(k.clone(), InstallItem { rel, data, origin: format!("game-fill:{}", e.path) });
        filled += 1;
    }
    if filled > 0 {
        notes.push(format!("从游戏包补齐了 {filled} 个 SDK 占位文件(贴图连同分卷)"));
    }
    // (1) 分卷完整性
    let mut problems = Vec::new();
    for it in items.values() {
        if !it.rel.to_lowercase().ends_with(".dds") || it.data.len() < 128 || &it.data[0..4] != b"DDS " {
            continue;
        }
        let h = dds::Header::parse(&it.data).map_err(|e| anyhow!("{}: {e}", it.rel))?;
        let k = h.volume_count().unwrap_or(0);
        let stem = &it.rel[..it.rel.len() - 4];
        for n in 0..k {
            if !items.contains_key(&format!("{stem}.dd{n}").to_lowercase()) {
                problems.push(format!("{} 头部声明 {k} 个分卷, 缺少 .dd{n}", it.rel));
            }
        }
        // 与游戏原版对照: 原版有分卷而这里没有 → 游戏会把包里的旧分卷和新 .dds 混用(M0 实验踩过)
        if k == 0 {
            let name = it.rel.rsplit('/').next().unwrap();
            if let Ok(e) = look.unique(name, None) {
                let gstem = &e.path[..e.path.len() - 4];
                if ix.get(&format!("{gstem}.dd0")).is_some() {
                    problems.push(format!("{}: 游戏原版带 .dd0 分卷, 但这份 .dds 没有分卷信息; 请用 wowsmod tex import --like 生成(会按原版拆分卷)", it.rel));
                }
            }
        }
    }
    if !problems.is_empty() {
        bail!("贴图分卷检查未通过:\n  {}", problems.join("\n  "));
    }
    // SDK 根加载器
    // SDK 根加载器: 总是列入(origin=generated)。登记表里别的 mod 已装 → 共用; 用户自己放的 → 不接管
    if p.cfg.install.mode == "sdk" && !items.contains_key("pnfmodsloader.py") {
        items.insert("pnfmodsloader.py".into(), InstallItem { rel: "PnFModsLoader.py".into(), data: Vec::new(), origin: "generated".into() });
        if !res_mods.join("PnFModsLoader.py").exists() {
            notes.push("res_mods 里没有 PnFModsLoader.py, 将创建空文件(SDK 路线需要)".into());
        }
    }
    Ok((items.into_values().collect(), notes))
}

fn manifest_path(p: &Project) -> PathBuf {
    p.root.join(".install").join("manifest.json")
}

#[allow(dead_code)] // M2 旧路径, 只留给测试; M4 起用 registry::install
pub fn install(p: &Project, items: &[InstallItem], res_mods: &Path, build: &str, dry_run: bool) -> Result<Value> {
    let mp = manifest_path(p);
    if mp.exists() {
        bail!("项目已安装(存在 {}); 先 wowsmod uninstall", mp.display());
    }
    if !res_mods.is_dir() {
        bail!("res_mods 目录不存在: {}", res_mods.display());
    }
    let mut created_dirs: Vec<String> = Vec::new();
    let mut files = Vec::new();
    let mut replaced = 0;
    for it in items {
        if it.rel.split('/').any(|c| c == ".." || c.contains(':')) {
            bail!("拒绝不安全的路径 {}", it.rel);
        }
        let dst = res_mods.join(&it.rel);
        // 记录将新建的目录
        let mut dirs = Vec::new();
        let mut d = dst.parent().unwrap().to_path_buf();
        while !d.exists() && d.starts_with(res_mods) && d != res_mods {
            dirs.push(rel_slash(res_mods, &d));
            d = d.parent().unwrap().to_path_buf();
        }
        let backed_up = dst.exists();
        if backed_up {
            replaced += 1;
        }
        files.push(ManifestFile { rel: it.rel.clone(), sha256: sha256_hex(&it.data), size: it.data.len() as u64, origin: it.origin.clone(), backed_up });
        if dry_run {
            for x in dirs.into_iter().rev() {
                if !created_dirs.contains(&x) {
                    created_dirs.push(x);
                }
            }
            continue;
        }
        for x in dirs.into_iter().rev() {
            std::fs::create_dir_all(res_mods.join(&x))?;
            if !created_dirs.contains(&x) {
                created_dirs.push(x);
            }
        }
        if backed_up {
            let b = p.root.join(".install/backup").join(&it.rel);
            std::fs::create_dir_all(b.parent().unwrap())?;
            std::fs::copy(&dst, &b).with_context(|| format!("备份 {}", dst.display()))?;
        }
        std::fs::write(&dst, &it.data).with_context(|| format!("写入 {}", dst.display()))?;
        // 先写一次清单(断电/掉线时也能按已写部分卸载)
        if files.len() % 50 == 1 {
            write_manifest(p, res_mods, build, &files, &created_dirs)?;
        }
    }
    if !dry_run {
        write_manifest(p, res_mods, build, &files, &created_dirs)?;
        // 写后校验
        for f in &files {
            let d = std::fs::read(res_mods.join(&f.rel))?;
            if sha256_hex(&d) != f.sha256 {
                bail!("写后校验失败: {}", f.rel);
            }
        }
    }
    let total: u64 = files.iter().map(|f| f.size).sum();
    Ok(json!({"dry_run": dry_run, "files": files.len(), "bytes": total, "replaced_existing": replaced, "created_dirs": created_dirs.len(), "manifest": mp.display().to_string(),
        "by_origin": files.iter().fold(BTreeMap::<String, usize>::new(), |mut m, f| { *m.entry(f.origin.split(':').next().unwrap().to_string()).or_default() += 1; m })}))
}

pub fn write_manifest(p: &Project, res_mods: &Path, build: &str, files: &[ManifestFile], dirs: &[String]) -> Result<()> {
    let m = Manifest {
        mod_name: p.cfg.meta.name.clone(),
        res_mods: res_mods.display().to_string(),
        game_build: build.into(),
        time_unix: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        files: files.to_vec(),
        created_dirs: dirs.to_vec(),
        wowsmod_version: env!("CARGO_PKG_VERSION").into(),
    };
    let mp = manifest_path(p);
    std::fs::create_dir_all(mp.parent().unwrap())?;
    let tmp = mp.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&m)?)?;
    std::fs::rename(&tmp, &mp)?;
    Ok(())
}

pub fn uninstall(p: &Project, dry_run: bool) -> Result<Value> {
    let mp = manifest_path(p);
    let m: Manifest = serde_json::from_slice(&std::fs::read(&mp).with_context(|| format!("没有安装清单 {}(项目未安装?)", mp.display()))?)?;
    let res_mods = PathBuf::from(&m.res_mods);
    let (mut deleted, mut restored, mut missing) = (0, 0, 0);
    let mut kept_modified = Vec::new();
    for f in &m.files {
        let dst = res_mods.join(&f.rel);
        if !dst.exists() {
            missing += 1;
        } else {
            let d = std::fs::read(&dst)?;
            if sha256_hex(&d) != f.sha256 {
                kept_modified.push(f.rel.clone());
                continue;
            }
            if !dry_run {
                std::fs::remove_file(&dst).with_context(|| format!("删除 {}", dst.display()))?;
            }
            deleted += 1;
        }
        if f.backed_up {
            let b = p.root.join(".install/backup").join(&f.rel);
            if !dry_run {
                std::fs::copy(&b, &dst).with_context(|| format!("还原备份 {}", b.display()))?;
            }
            restored += 1;
        }
    }
    let mut removed_dirs = 0;
    let mut nonempty = Vec::new();
    for d in m.created_dirs.iter().rev() {
        let pd = res_mods.join(d);
        if !pd.is_dir() {
            continue;
        }
        let empty = std::fs::read_dir(&pd)?.next().is_none();
        if empty {
            if !dry_run {
                std::fs::remove_dir(&pd)?;
            }
            removed_dirs += 1;
        } else if !dry_run {
            nonempty.push(d.clone());
        }
    }
    if !dry_run {
        if kept_modified.is_empty() {
            let hist = p.root.join(".install/history");
            std::fs::create_dir_all(&hist)?;
            std::fs::rename(&mp, hist.join(format!("manifest_{}.json", m.time_unix)))?;
            let bk = p.root.join(".install/backup");
            if bk.exists() {
                std::fs::rename(&bk, hist.join(format!("backup_{}", m.time_unix)))?;
            }
        }
    }
    Ok(json!({"dry_run": dry_run, "res_mods": m.res_mods, "files_in_manifest": m.files.len(), "deleted": deleted, "restored_backups": restored, "already_missing": missing,
        "kept_modified_after_install": kept_modified, "removed_dirs": removed_dirs, "dirs_left_nonempty": nonempty.clone(), "complete": !dry_run && kept_modified.is_empty() && nonempty.is_empty()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(d: &Path) -> Vec<(String, String)> {
        let mut v = Vec::new();
        let mut all = Vec::new();
        walk(d, &mut all).unwrap();
        for f in all {
            v.push((rel_slash(d, &f), sha256_hex(&std::fs::read(&f).unwrap())));
        }
        // 目录也算
        let mut st = vec![d.to_path_buf()];
        while let Some(x) = st.pop() {
            for e in std::fs::read_dir(&x).unwrap().flatten() {
                if e.path().is_dir() {
                    v.push((rel_slash(d, &e.path()) + "/", String::new()));
                    st.push(e.path());
                }
            }
        }
        v.sort();
        v
    }
    #[test]
    fn install_uninstall_restores_exactly() {
        let tmp = std::env::temp_dir().join(format!("wowsmod_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let rm = tmp.join("res_mods");
        std::fs::create_dir_all(rm.join("content/x")).unwrap();
        std::fs::write(rm.join("content/x/keep.txt"), b"user file").unwrap();
        std::fs::write(rm.join("content/x/over.dds"), b"old").unwrap();
        let before = snapshot(&rm);
        let p = Project::init(&tmp.join("proj"), "T1", "", "loose", "1").unwrap();
        let items = vec![
            InstallItem { rel: "content/x/over.dds".into(), data: b"new".to_vec(), origin: "project".into() },
            InstallItem { rel: "content/y/z/new.dd0".into(), data: vec![1, 2, 3], origin: "project".into() },
        ];
        let r = install(&p, &items, &rm, "1", false).unwrap();
        assert_eq!(r["replaced_existing"], 1);
        assert_eq!(std::fs::read(rm.join("content/x/over.dds")).unwrap(), b"new");
        assert!(rm.join("content/y/z/new.dd0").exists());
        assert!(install(&p, &items, &rm, "1", false).is_err(), "重复安装应拒绝");
        let u = uninstall(&p, false).unwrap();
        assert_eq!(u["complete"], true, "{u}");
        assert_eq!(snapshot(&rm), before);
        // 被用户改过的文件不删
        install(&p, &items, &rm, "1", false).unwrap();
        std::fs::write(rm.join("content/y/z/new.dd0"), b"user edit").unwrap();
        let u = uninstall(&p, false).unwrap();
        assert_eq!(u["complete"], false);
        assert_eq!(u["kept_modified_after_install"][0], "content/y/z/new.dd0");
        let _ = std::fs::remove_dir_all(&tmp);
    }
    #[test]
    fn stubs() {
        assert!(is_binary_stub("a/x.dds", b"<x.dd0 />\n"));
        assert!(!is_binary_stub("a/x.model", b"<x.model>...</x.model>"));
    }
}

pub fn rel_slash_pub(base: &Path, p: &Path) -> String {
    rel_slash(base, p)
}
