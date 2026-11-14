//! 从 mod 项目构建预览状态: 步骤、材质表(base/head/中间)、贴图 hash→来源、几何缓存、模拟数据。
use std::cell::OnceCell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, RwLock};

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use wowsmod_formats::{mfm, texture::{self, Image}};

use crate::game::{self, GameInstall};
use crate::modproj::Project;
use crate::texcmd;
use crate::{game_ctx, Cli, GameCtx};

const SLOT: &[(&str, &str)] = &[("diffuseMap", "a"), ("normalMap", "n"), ("metallicGlossMap", "mg"), ("ambientOcclusionMap", "ao"), ("animMap", "anim")];

pub struct PreviewState {
    pub project: Project,
    pub ship: String,
    pub hull_model: String,
    pub game: Option<GameInstall>,
    /// Pin the explicit client even when it is unavailable. Child commands must never auto-detect another client.
    pub selected_game: Option<PathBuf>,
    pub ctx: Option<GameCtx>,
    /// Shared by immutable snapshots of the same library project.
    pub etag: Arc<AtomicU64>,
    pub shutting_down: AtomicBool,
    steps: RwLock<Vec<Step>>,
    /// rev id → materials
    mats: RwLock<HashMap<String, BTreeMap<String, Mat>>>,
    /// hash16 → 如何取贴图
    blobs: RwLock<HashMap<String, BlobSrc>>,
    geom: RwLock<Option<Geom>>,
    preview_error: RwLock<Option<String>>,
    sim: RwLock<Option<Value>>,
    parts: RwLock<Option<Value>>,
    install: RwLock<Value>,
    activity: RwLock<Value>,
    pub jobs: Arc<Mutex<HashMap<String, Value>>>,
    sse: Mutex<Vec<std::sync::mpsc::Sender<Value>>>,
    png_cache: Mutex<HashMap<(String, u32, String), Vec<u8>>>,
    /// 常驻出图增量重载: 项目文件 路径 → (大小, mtime, 是否占位, hash16); 没变的文件不再读
    file_memo: Mutex<HashMap<PathBuf, (u64, u128, bool, String)>>,
    /// 游戏内贴图 vpath → hash16(会话内游戏文件不变)
    game_memo: Mutex<HashMap<String, String>>,
    /// 本次 rebuild 的项目文件名索引(小写文件名 → 路径), 替代逐贴图全目录遍历
    name_index: Mutex<Option<HashMap<String, Vec<PathBuf>>>>,
    /// GameParams 里的等级/国家/类型 + 本地化船名(后台线程加载, 按游戏版本缓存在临时目录)
    ship_meta: std::sync::Arc<RwLock<Value>>,
}

#[derive(Clone)]
pub struct Step {
    pub id: String,
    pub index: usize,
    pub title: String,
    pub user_prompt: Option<String>,
    pub ai_note: Option<String>,
    pub state: String,
    pub time_unix: u64,
    pub time: String,
    pub edits: Vec<Value>,
    pub files: Vec<Value>,
    pub textures: Vec<String>,
    pub materials: Vec<String>,
    pub parts: Vec<String>,
}

#[derive(Clone)]
struct Mat {
    family: String,
    fx: String,
    params: BTreeMap<String, Value>,
    textures: BTreeMap<String, TexRef>,
}
#[derive(Clone)]
struct TexRef {
    name: String,
    hash: Option<String>,
    missing: bool,
    width: Option<u32>,
    height: Option<u32>,
}
#[derive(Clone)]
enum BlobSrc {
    Project(PathBuf),
    Game(String), // vpath
    Bytes(Vec<u8>),
}
struct Geom {
    hash: String,
    bytes: Vec<u8>,
    /// 缓存文件 (大小, mtime): 变了才重读
    stamp: Option<(u64, u128)>,
    /// 模型编辑指纹(有 model 编辑时 = 项目内真实 .geometry 与全部 .visual 的路径+大小+mtime); 没有编辑为 None
    model_key: Option<String>,
}

struct GameDdsNames {
    // None marks a basename that occurs more than once in the game index.
    by_stem: HashMap<String, Option<usize>>,
}

impl GameDdsNames {
    fn new<'a>(paths: impl IntoIterator<Item = &'a str>) -> Self {
        let mut by_stem = HashMap::new();
        for (index, path) in paths.into_iter().enumerate() {
            let basename = path.rsplit('/').next().unwrap_or_default();
            let Some((stem, ext)) = basename.rsplit_once('.') else {
                continue;
            };
            if ext.eq_ignore_ascii_case("dds") {
                by_stem
                    .entry(stem.to_ascii_lowercase())
                    .and_modify(|match_index| *match_index = None)
                    .or_insert(Some(index));
            }
        }
        Self { by_stem }
    }

    fn unique_index(&self, stem: &str) -> Option<usize> {
        self.by_stem.get(&stem.to_ascii_lowercase()).copied().flatten()
    }
}

impl PreviewState {
    pub fn open(cli: &Cli, root: &Path) -> Result<Self> {
        let project = Project::open(root)?;
        let ship = project.cfg.meta.ship.clone();
        let _ = crate::preview::ensure_steps_json(root, &ship);
        let (game, _) = game::locate(cli.game.as_deref());
        let selected_game = cli.game.clone().or_else(|| game.as_ref().map(|g| g.root.clone()));
        let ctx = game_ctx(cli, false).ok().flatten();
        let hull_model = detect_hull_model(&project, &ship);
        let st = PreviewState {
            project,
            ship: ship.clone(),
            hull_model,
            game,
            selected_game,
            ctx,
            etag: Arc::new(AtomicU64::new(1)),
            shutting_down: AtomicBool::new(false),
            steps: RwLock::new(vec![]),
            mats: RwLock::new(HashMap::new()),
            blobs: RwLock::new(HashMap::new()),
            geom: RwLock::new(None),
            preview_error: RwLock::new(None),
            sim: RwLock::new(None),
            parts: RwLock::new(None),
            install: RwLock::new(json!({"state": "not_installed", "rev": null, "files": 0, "time": null})),
            activity: RwLock::new(json!({"state": "idle", "title": null, "since": null, "step": null})),
            jobs: Arc::new(Mutex::new(HashMap::new())),
            sse: Mutex::new(Vec::new()),
            png_cache: Mutex::new(HashMap::new()),
            file_memo: Mutex::new(HashMap::new()),
            game_memo: Mutex::new(HashMap::new()),
            name_index: Mutex::new(None),
            ship_meta: std::sync::Arc::new(RwLock::new(Value::Null)),
        };
        st.load_ship_meta();
        st.rebuild()?;
        st.refresh_install();
        Ok(st)
    }

    /// Share metadata by canonical client root/build instead of a ship/build-only temp cache.
    fn load_ship_meta(&self) {
        let metadata = crate::ship_catalog::ShipCatalog::load(&crate::m4cmd::home_dir(), self.game.as_ref());
        *self.ship_meta.write().unwrap() = json!(metadata.lookup(&self.ship));
    }

    pub fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        // 唤醒 SSE
        if let Ok(mut s) = self.sse.lock() {
            s.clear();
        }
    }
    pub fn is_shutdown(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub fn bump(&self, kind: &str, data: Value) {
        let etag = self.etag.fetch_add(1, Ordering::SeqCst) + 1;
        let ev = json!({"type": kind, "etag": etag, "data": data});
        if let Ok(mut subs) = self.sse.lock() {
            subs.retain(|tx| tx.send(ev.clone()).is_ok());
        }
    }

    pub fn subscribe(&self) -> std::sync::mpsc::Receiver<Value> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.sse.lock().unwrap().push(tx);
        rx
    }

    pub fn head_id(&self) -> Option<String> {
        let steps = self.steps.read().unwrap();
        Some(steps.iter().rev().find(|s| s.state == "active").map(|s| s.id.clone()).unwrap_or_else(|| "current".into()))
    }

    pub fn etag(&self) -> u64 {
        self.etag.load(Ordering::SeqCst)
    }

    /// 常驻出图: 项目改动后增量重载(只重读大小/mtime 变了的文件; 贴图按内容哈希, 前端只拉新哈希)
    pub fn reload(&self) -> Result<Value> {
        let t0 = std::time::Instant::now();
        let before: std::collections::HashSet<String> = self.blobs.read().unwrap().keys().cloned().collect();
        let geom_before = self.geom.read().unwrap().as_ref().map(|g| g.hash.clone());
        self.rebuild()?;
        self.refresh_install();
        let after: Vec<String> = self.blobs.read().unwrap().keys().cloned().collect();
        let new_blobs = after.iter().filter(|h| !before.contains(*h)).count();
        let geom_after = self.geom.read().unwrap().as_ref().map(|g| g.hash.clone());
        self.bump("steps", json!({}));
        Ok(json!({"new_textures": new_blobs, "textures": after.len(), "geometry_changed": geom_before != geom_after, "head": self.head_id(), "seconds": t0.elapsed().as_secs_f32()}))
    }

    /// 项目内容指纹(文件路径+大小+mtime), 用来判断要不要增量重载。跳过 renders/.render/.wowsmod(缓存的 geometry.glb 除外)
    pub fn fingerprint(&self) -> u64 {
        project_fingerprint(&self.project.root)
    }

    fn file_info(&self, p: &Path) -> Option<(bool, String)> {
        let md = std::fs::metadata(p).ok()?;
        let key = (md.len(), mtime_ns(&md));
        if let Some((sz, mt, stub, h)) = self.file_memo.lock().unwrap().get(p) {
            if (*sz, *mt) == key {
                return Some((*stub, h.clone()));
            }
        }
        let d = std::fs::read(p).ok()?;
        let stub = mfm::is_sdk_stub(&d);
        let h = hash16(&d);
        self.file_memo.lock().unwrap().insert(p.to_path_buf(), (key.0, key.1, stub, h.clone()));
        Some((stub, h))
    }

    fn rebuild(&self) -> Result<()> {
        // 文件名索引(一次遍历)
        let mut idx: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for f in self.project.files().unwrap_or_default() {
            if let Some(n) = f.file_name() {
                idx.entry(n.to_string_lossy().to_lowercase()).or_default().push(f);
            }
        }
        *self.name_index.lock().unwrap() = Some(idx);
        let edits = load_edits(&self.project.root)?;
        let steps = group_steps(&self.project.root, &edits)?;
        // base materials from SDK copy
        let mut blobs = HashMap::new();
        let (base_mats, head_mats) = {
            // Base and head share one index, built only if a game fallback is needed.
            let game_dds_names = OnceCell::new();
            (
                self.scan_materials(false, &mut blobs, &game_dds_names)?,
                self.scan_materials(true, &mut blobs, &game_dds_names)?,
            )
        };
        // intermediate: start from base, apply texture outputs from edits
        let mut mats_map = HashMap::new();
        mats_map.insert("base".into(), base_mats.clone());
        // Imported projects need a real current snapshot even before their first recorded edit.
        mats_map.insert("current".into(), head_mats.clone());
        let mut cur = base_mats;
        for st in &steps {
            for ed in &st.edits {
                apply_edit_to_mats(&mut cur, ed, &mut blobs, &self.project);
            }
            // 若是 head 且与当前项目扫描差异大, 用扫描结果(更准, 含 mfm 改动)
            if Some(&st.id) == steps.last().map(|s| &s.id) {
                mats_map.insert(st.id.clone(), head_mats.clone());
                cur = head_mats.clone();
            } else {
                mats_map.insert(st.id.clone(), cur.clone());
            }
        }
        // enrich steps with mat diffs
        let mut steps = steps;
        let mut prev = "base".to_string();
        for st in &mut steps {
            let (tx, mt) = diff_mats(mats_map.get(&prev).unwrap_or(&BTreeMap::new()), mats_map.get(&st.id).unwrap_or(&BTreeMap::new()));
            st.textures = tx;
            st.materials = mt;
            st.parts = vec![]; // 填完 parts 后再算
            prev = st.id.clone();
        }
        *self.mats.write().unwrap() = mats_map;
        *self.blobs.write().unwrap() = blobs;
        *self.steps.write().unwrap() = steps;
        *self.preview_error.write().unwrap() = self.ensure_geometry().err().map(|e| format!("{e:#}"));
        // Missing/corrupt preview data must not hide project metadata or application actions.
        if self.preview_error.read().unwrap().is_some() {
            *self.geom.write().unwrap() = None;
        }
        let _ = self.ensure_sim_parts();
        // 步骤标题里的贴图名 → 部件中文名
        {
            let parts_doc = self.parts.read().unwrap().clone();
            let mut steps = self.steps.write().unwrap();
            for st in steps.iter_mut() {
                st.title = resolve_part_tokens(&st.title, parts_doc.as_ref());
            }
        }
        // parts changed
        if let Some(parts) = self.parts.read().unwrap().clone() {
            let plist = parts["parts"].as_array().cloned().unwrap_or_default();
            let mut steps = self.steps.write().unwrap();
            for st in steps.iter_mut() {
                let set: std::collections::HashSet<_> = st.materials.iter().cloned().collect();
                st.parts = plist
                    .iter()
                    .filter(|p| p["materials"].as_array().into_iter().flatten().any(|m| set.contains(m.as_str().unwrap_or(""))))
                    .filter_map(|p| p["id"].as_str().map(|s| s.to_string()))
                    .collect();
            }
        }
        Ok(())
    }

    fn scan_materials(
        &self,
        head: bool,
        blobs: &mut HashMap<String, BlobSrc>,
        game_dds_names: &OnceCell<GameDdsNames>,
    ) -> Result<BTreeMap<String, Mat>> {
        static IDENTIFIER: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"<identifier>\s*(\S+)\s*</identifier>").unwrap());
        static MFM_PATH: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"<mfm>\s*(\S+)\s*</mfm>").unwrap());

        let ship_dir = if head {
            self.project.assets().join("PnFMods").join(&self.project.cfg.meta.name).join(&self.ship)
        } else {
            self.project.assets().join("PnFMods").join("ModsSDK.zip").join(&self.ship)
        };
        let mut out = BTreeMap::new();
        if !ship_dir.is_dir() {
            return Ok(out);
        }
        // visual identifiers: mfm basename → has EMISSIVE
        let mut emissive_id = std::collections::HashSet::new();
        walk(&ship_dir, &mut |p| {
            if p.extension().and_then(|e| e.to_str()) != Some("visual") {
                return;
            }
            if let Ok(t) = std::fs::read_to_string(p) {
                // per material block
                for mat in t.split("<material>").skip(1) {
                    let id = IDENTIFIER.captures(mat).map(|c| c[1].to_string());
                    let mf = MFM_PATH.captures(mat).map(|c| {
                        Path::new(&c[1]).file_name().unwrap().to_string_lossy().to_string()
                    });
                    if let (Some(id), Some(mf)) = (id, mf) {
                        if id.contains("EMISSIVE") {
                            emissive_id.insert(mf);
                        }
                    }
                }
            }
        });
        walk(&ship_dir, &mut |p| {
            if p.extension().and_then(|e| e.to_str()) != Some("mfm") {
                return;
            }
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let text = std::fs::read_to_string(p).unwrap_or_default();
            let fx = mfm::fx(&text).unwrap_or_default();
            let fx_base = Path::new(&fx).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or(fx.clone());
            let family = family_of(&fx_base, emissive_id.contains(&name));
            let mut params = BTreeMap::new();
            for (n, ty, v) in mfm::properties(&text) {
                if SLOT.iter().any(|(s, _)| *s == n) {
                    continue;
                }
                let val = match ty.as_str() {
                    "Float" => json!(v.parse::<f64>().unwrap_or(0.0)),
                    "Int" => json!(v.trim().parse::<i64>().unwrap_or(0)),
                    // mfm 里 Bool 写作 true/false
                    "Bool" => json!(matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1")),
                    _ => json!(v),
                };
                params.insert(n, val);
            }
            let mut textures = BTreeMap::new();
            for (pn, _, v) in mfm::properties(&text) {
                if let Some((_, slot)) = SLOT.iter().find(|(s, _)| *s == pn) {
                    let tname = Path::new(&v).file_stem().unwrap_or_default().to_string_lossy().to_string();
                    let (hash, missing, w, h) = self.resolve_tex(&tname, head, Path::new(&v), blobs, game_dds_names);
                    textures.insert(slot.to_string(), TexRef { name: tname, hash, missing, width: w, height: h });
                }
            }
            out.insert(name, Mat { family, fx: fx_base, params, textures });
        });
        Ok(out)
    }

    fn resolve_tex(
        &self,
        name: &str,
        head: bool,
        rel_in_mfm: &Path,
        blobs: &mut HashMap<String, BlobSrc>,
        game_dds_names: &OnceCell<GameDdsNames>,
    ) -> (Option<String>, bool, Option<u32>, Option<u32>) {
        // 1) project exact path under assets
        if head {
            let under = self.project.assets().join(rel_in_mfm);
            if under.is_file() {
                if let Some((stub, h)) = self.file_info(&under) {
                    if !stub {
                        blobs.insert(h.clone(), BlobSrc::Project(under));
                        return (Some(h), false, None, None);
                    }
                }
            }
            // by name unique non-stub
            let fname = format!("{name}.dds").to_lowercase();
            let hits: Vec<PathBuf> = match self.name_index.lock().unwrap().as_ref() {
                Some(ix) => ix.get(&fname).cloned().unwrap_or_default(),
                None => self.project.find_by_name(&fname).unwrap_or_default(),
            };
            let real: Vec<(PathBuf, String)> = hits.into_iter().filter_map(|p| self.file_info(&p).filter(|(stub, _)| !stub).map(|(_, h)| (p, h))).collect();
            if real.len() == 1 {
                let (p, h) = real.into_iter().next().unwrap();
                blobs.insert(h.clone(), BlobSrc::Project(p));
                return (Some(h), false, None, None);
            }
        }
        // 2) game by file name
        if let Some(ctx) = &self.ctx {
            let names = game_dds_names.get_or_init(|| {
                GameDdsNames::new(ctx.index.entries.iter().map(|entry| entry.path.as_str()))
            });
            if let Some(index) = names.unique_index(name) {
                let entry = &ctx.index.entries[index];
                let vp = entry.path.clone();
                if let Some(h) = self.game_memo.lock().unwrap().get(&vp).cloned() {
                    blobs.insert(h.clone(), BlobSrc::Game(vp));
                    return (Some(h), false, None, None);
                }
                // hash of game dds header file (not volumes) — use path-stable: read and hash
                if let Ok(d) = ctx.reader.read(entry) {
                    let h = hash16(&d);
                    self.game_memo.lock().unwrap().insert(vp.clone(), h.clone());
                    blobs.insert(h.clone(), BlobSrc::Game(vp));
                    return (Some(h), false, None, None);
                }
            }
        }
        (None, true, None, None)
    }

    /// 项目有 model 编辑(真实 .geometry 或 edits/ 里有 model 记录)时, 返回几何指纹; 否则 None(用原船种子几何)
    fn model_key(&self) -> Option<String> {
        let assets = self.project.assets();
        let edited_record = std::fs::read_dir(self.project.root.join("edits")).map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().contains("_model_"))).unwrap_or(false);
        let mut items: Vec<String> = vec!["fallback-v3".into()]; // 后处理格式版本, 变了就重建
        let mut real_geom = false;
        let mut st = vec![assets];
        while let Some(d) = st.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    st.push(p);
                    continue;
                }
                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
                // .model/.seq/.ssm: 动画对象(model animate)也要触发重新装配
                if !matches!(ext.as_str(), "geometry" | "visual" | "model" | "seq" | "ssm") {
                    continue;
                }
                let Ok(m) = e.metadata() else { continue };
                if ext == "geometry" {
                    if m.len() < 4096 && std::fs::read(&p).map(|d| mfm::is_sdk_stub(&d)).unwrap_or(true) {
                        continue;
                    }
                    real_geom = true;
                }
                items.push(format!("{}|{}|{}", p.display(), m.len(), mtime_ns(&m)));
            }
        }
        if !real_geom && !edited_record {
            return None;
        }
        items.sort();
        Some(hash16(items.join("\n").as_bytes()))
    }

    fn ensure_geometry(&self) -> Result<()> {
        let cache = self.project.root.join(".wowsmod/cache");
        let cached = cache.join("geometry.glb");
        let key_file = cache.join("geometry.model_key");
        let key = self.model_key();
        let stamp_now = std::fs::metadata(&cached).ok().map(|m| (m.len(), mtime_ns(&m)));
        if let Some(g) = self.geom.read().unwrap().as_ref() {
            // 缓存文件没变(或本来就不是从缓存文件来的)且模型编辑没变就复用
            if (g.stamp.is_none() || g.stamp == stamp_now) && g.model_key == key {
                return Ok(());
            }
        }
        if !cached.is_file() && find_seed(&self.ship).and_then(|s| s.join("geometry.glb").is_file().then_some(s)).is_none()
            && (self.ship.is_empty() || (self.game.is_none() && std::env::var_os("WOWSMOD_GP").is_none())) {
            bail!("no_model: 项目没有可用模型缓存；请关联目标船并提供客户端/装配资源后重试");
        }
        std::fs::create_dir_all(&cache)?;
        let key_on_disk = std::fs::read_to_string(&key_file).ok();
        let bytes = if let Some(k) = &key {
            // 有模型编辑: 由项目重新装配(新增/替换/缩放的网格才能在工作台看到)
            if cached.is_file() && key_on_disk.as_deref() == Some(k.as_str()) {
                std::fs::read(&cached)?
            } else {
                let b = self.build_geometry_fallback(&cached)?;
                std::fs::write(&key_file, k)?;
                b
            }
        } else if key_on_disk.is_some() {
            // 编辑已撤销: 丢掉按编辑装配的缓存, 回到原船几何
            let _ = std::fs::remove_file(&key_file);
            let _ = std::fs::remove_file(&cached);
            if let Some(seed) = find_seed(&self.ship).filter(|s| s.join("geometry.glb").is_file()) {
                std::fs::copy(seed.join("geometry.glb"), &cached)?;
                std::fs::read(&cached)?
            } else {
                self.build_geometry_fallback(&cached)?
            }
        } else if cached.is_file() {
            std::fs::read(&cached)?
        } else if let Some(seed) = find_seed(&self.ship) {
            let src = seed.join("geometry.glb");
            if src.is_file() {
                std::fs::copy(&src, &cached)?;
                std::fs::read(&cached)?
            } else {
                self.build_geometry_fallback(&cached)?
            }
        } else {
            self.build_geometry_fallback(&cached)?
        };
        let hash = hash16(&bytes);
        let stamp = std::fs::metadata(&cached).ok().map(|m| (m.len(), mtime_ns(&m)));
        *self.geom.write().unwrap() = Some(Geom { hash, bytes, stamp, model_key: key });
        Ok(())
    }

    fn build_geometry_fallback(&self, dest: &Path) -> Result<Vec<u8>> {
        if self.ship.is_empty() {
            bail!("no_model: 全局项目未关联舰船，无法装配船体");
        }
        if self.game.is_none() && std::env::var_os("WOWSMOD_GP").is_none() {
            bail!("no_model: 指定客户端不可用，请修正 --game 或提供项目模型缓存");
        }
        // 调用自身 assemble --full, 再后处理材质名
        let exe = std::env::current_exe()?;
        let out = dest.with_extension("raw.glb");
        let mut c = std::process::Command::new(&exe);
        c.args(["--json", "assemble", &self.ship, "--project"]).arg(&self.project.root).args(["--full", "-o"]).arg(&out);
        // A deliberately nonexistent path is safer than omitting --game in offline GP mode.
        c.arg("--game").arg(self.selected_game.clone().unwrap_or_else(|| self.project.root.join(".wowsmod/no-client")));
        // 无完整客户端的环境(如 CI/盒子): WOWSMOD_GP = GameParams 船条目 JSON, WOWSMOD_GEOM_DIR = 原版 .geometry 目录(可多个, 用系统路径分隔符)
        if let Ok(gp) = std::env::var("WOWSMOD_GP") {
            c.arg("--gp").arg(gp);
        }
        if let Some(dirs) = std::env::var_os("WOWSMOD_GEOM_DIR") {
            for d in std::env::split_paths(&dirs) {
                c.arg("--geom-dir").arg(d);
            }
        }
        let o = c.output().context("assemble")?;
        if !o.status.success() {
            bail!("assemble 失败: {}", String::from_utf8_lossy(&o.stderr));
        }
        let raw = std::fs::read(&out)?;
        let fixed = postprocess_glb_materials(&raw)?;
        std::fs::write(dest, &fixed)?;
        let _ = std::fs::remove_file(&out);
        Ok(fixed)
    }

    fn ensure_sim_parts(&self) -> Result<()> {
        let cache = self.project.root.join(".wowsmod/cache");
        let mut cands = vec![cache];
        if let Some(seed) = find_seed(&self.ship) {
            cands.push(seed);
        }
        for dir in cands {
            let sim = dir.join("sim.json");
            let parts = dir.join("parts.json");
            if self.sim.read().unwrap().is_none() && sim.is_file() {
                *self.sim.write().unwrap() = Some(serde_json::from_slice(&std::fs::read(sim)?)?);
            }
            if self.parts.read().unwrap().is_none() && parts.is_file() {
                *self.parts.write().unwrap() = Some(serde_json::from_slice(&std::fs::read(parts)?)?);
            }
        }
        // TODO: 从 GameParams 实时生成(无种子时)
        Ok(())
    }

    pub fn refresh_install(&self) {
        *self.install.write().unwrap() = super::library::install_status(&self.project.root, self.game.as_ref());
    }

    // ---------- JSON views ----------
    pub fn health(&self) -> Value {
        json!({"ok": true, "api_version": 1, "server": "wowsmod", "version": env!("CARGO_PKG_VERSION"), "time": iso_now()})
    }

    pub fn project_view(&self) -> Value {
        self.refresh_install();
        let running = game::running();
        let meta = self.ship_meta.read().unwrap().clone();
        let (ver, build, path) = match &self.game {
            Some(g) => (g.client_version.clone().unwrap_or_default(), g.build.clone(), g.root.display().to_string()),
            None => (String::new(), String::new(), String::new()),
        };
        json!({
            "api_version": 1,
            "server": {"name": "wowsmod", "version": env!("CARGO_PKG_VERSION")},
            "project": {
                "name": self.project.cfg.meta.name, "version": self.project.cfg.meta.version,
                "description": self.project.cfg.meta.description, "dir": self.project.root.display().to_string(),
                "format_version": self.project.cfg.meta.format_version, "install_mode": self.project.cfg.install.mode
            },
            "ship": {
                "code": self.ship, "param": meta["param"],
                "index": crate::ship_catalog::canonical_ship_key(&self.ship),
                "hull_model": self.hull_model, "name": {"zh": meta["name_local"].as_str().unwrap_or(&self.ship), "en": self.ship.clone(), "ru": meta["name_local"].as_str().unwrap_or(&self.ship)},
                "level": meta["level"], "nation": meta["nation"], "species": meta["species"]
            },
            "game": {"client": "Мир кораблей", "available": self.game.is_some(), "version": ver, "build": build, "path": path, "running": !running.is_empty()},
            "capabilities": self.capabilities(),
            "install": self.install.read().unwrap().clone(),
            "activity": self.activity.read().unwrap().clone(),
            "head": self.head_id(), "etag": self.etag(),
            "fx": crate::fxcmd::project_fx_view(&self.project.root)
        })
    }

    pub fn state_view(&self) -> Value {
        self.refresh_install();
        let running = !game::running().is_empty();
        json!({"etag": self.etag(), "head": self.head_id(), "activity": self.activity.read().unwrap().clone(),
            "install": self.install.read().unwrap().clone(), "game": {"running": running}, "steps": self.steps.read().unwrap().len()})
    }

    pub fn steps_view(&self) -> Value {
        let steps: Vec<Value> = self.steps.read().unwrap().iter().map(|s| json!({
            "id": s.id, "index": s.index, "title": s.title, "user_prompt": s.user_prompt, "ai_note": s.ai_note,
            "state": s.state, "time": s.time, "time_unix": s.time_unix, "edits": s.edits, "files": s.files,
            "textures": s.textures, "materials": s.materials, "parts": s.parts
        })).collect();
        json!({"head": self.head_id(), "etag": self.etag(), "steps": steps})
    }

    pub fn resolve_rev(&self, r: Option<&str>) -> Result<String> {
        match r {
            None | Some("") | Some("head") | Some("current") => Ok(self.head_id().unwrap_or_else(|| "base".into())),
            Some("base") => Ok("base".into()),
            Some(x) => {
                let steps = self.steps.read().unwrap();
                if steps.iter().any(|s| s.id == x) {
                    Ok(x.into())
                } else if let Ok(n) = x.parse::<usize>() {
                    steps.get(n).map(|s| s.id.clone()).ok_or_else(|| anyhow!("not_found"))
                } else {
                    bail!("not_found")
                }
            }
        }
    }

    pub fn model_view(&self, rev: Option<&str>) -> Result<Value> {
        let r = self.resolve_rev(rev)?;
        let mats = self.mats.read().unwrap();
        let m = mats.get(&r).ok_or_else(|| anyhow!("not_found"))?;
        let geom = self.geom.read().unwrap();
        let g = geom.as_ref().ok_or_else(|| anyhow!("no_model"))?;
        let mut materials = serde_json::Map::new();
        for (name, mat) in m {
            let mut tex = serde_json::Map::new();
            for (slot, t) in &mat.textures {
                if t.missing {
                    tex.insert(slot.clone(), json!({"name": t.name, "missing": true}));
                } else if let Some(h) = &t.hash {
                    tex.insert(slot.clone(), json!({"name": t.name, "hash": h, "url": format!("/api/v1/blob/{h}.png"), "width": t.width, "height": t.height}));
                }
            }
            materials.insert(name.clone(), json!({"family": mat.family, "fx": mat.fx, "params": mat.params, "textures": tex}));
        }
        Ok(json!({
            "rev": r,
            "geometry": {"hash": g.hash, "url": format!("/api/v1/geometry/{}.glb", g.hash), "bytes": g.bytes.len()},
            "axes": {"up": "+y", "forward": "-z", "right": "+x"},
            "waterline_y": 0.0,
            "materials": materials
        }))
    }

    pub fn geometry_bytes(&self, hash: &str) -> Result<Vec<u8>> {
        let g = self.geom.read().unwrap();
        let g = g.as_ref().ok_or_else(|| anyhow!("no_model"))?;
        if g.hash != hash {
            bail!("not_found");
        }
        Ok(g.bytes.clone())
    }

    pub fn blob_png(&self, hash: &str, max: u32, ch: &str) -> Result<Vec<u8>> {
        let key = (hash.to_string(), max, ch.to_string());
        if let Some(v) = self.png_cache.lock().unwrap().get(&key) {
            return Ok(v.clone());
        }
        let src = self.blobs.read().unwrap().get(hash).cloned().ok_or_else(|| anyhow!("not_found"))?;
        let img = self.load_image(&src)?;
        let mut img = img;
        if max > 0 {
            while img.width.max(img.height) > max && img.width > 1 {
                img = texture::downsample(&img, texture::MipFilter::BoxSrgb);
            }
        }
        // ch filter: rgba/rgb/r/g/b/a — for single channel, replicate to gray RGB
        if matches!(ch, "r" | "g" | "b" | "a") {
            let i = match ch { "r" => 0, "g" => 1, "b" => 2, _ => 3 };
            for px in img.px.chunks_exact_mut(4) {
                let v = px[i];
                px[0] = v; px[1] = v; px[2] = v; px[3] = 255;
            }
        } else if ch == "rgb" {
            for px in img.px.chunks_exact_mut(4) {
                px[3] = 255;
            }
        }
        let png = texture::write_png(&img, ch == "rgba").map_err(|e| anyhow!("{e}"))?;
        let mut cache = self.png_cache.lock().unwrap();
        if cache.len() > 400 {
            cache.clear();
        }
        cache.insert(key, png.clone());
        Ok(png)
    }

    fn load_image(&self, src: &BlobSrc) -> Result<Image> {
        match src {
            BlobSrc::Bytes(b) => texture::read_png(b).map_err(|e| anyhow!("{e}")),
            BlobSrc::Project(p) => {
                let t = texcmd::load_tex(&p.display().to_string(), self.ctx.as_ref(), None, false)?;
                let lv = (0..t.dds.header.levels()).find(|l| t.dds.header.level_dims(*l).0 <= 2048).unwrap_or(0);
                texture::decode_level(&t.dds, lv).map_err(|e| anyhow!("{e}"))
            }
            BlobSrc::Game(vp) => {
                let t = texcmd::load_tex(vp, self.ctx.as_ref(), None, true)?;
                let lv = (0..t.dds.header.levels()).find(|l| t.dds.header.level_dims(*l).0 <= 2048).unwrap_or(0);
                texture::decode_level(&t.dds, lv).map_err(|e| anyhow!("{e}"))
            }
        }
    }

    pub fn textures_view(&self, rev: Option<&str>, base: Option<&str>) -> Result<Value> {
        let r = self.resolve_rev(rev)?;
        let b = self.resolve_rev(base.or(Some("base")))?;
        let mats = self.mats.read().unwrap();
        let M = mats.get(&r).ok_or_else(|| anyhow!("not_found"))?;
        let B = mats.get(&b).cloned().unwrap_or_default();
        let mut out: BTreeMap<String, Value> = BTreeMap::new();
        for (m, e) in M {
            for (slot, t) in &e.textures {
                if t.missing { continue; }
                let o = out.entry(t.name.clone()).or_insert_with(|| json!({
                    "name": t.name, "slot": slot, "hash": t.hash, "base_hash": null, "materials": [], "parts": 0,
                    "width": t.width, "height": t.height
                }));
                o["materials"].as_array_mut().unwrap().push(json!(m));
                if let Some(tb) = B.get(m).and_then(|x| x.textures.get(slot)) {
                    o["base_hash"] = json!(tb.hash);
                }
            }
        }
        let mut lst: Vec<Value> = out.into_values().map(|mut o| {
            let changed = o["hash"] != o["base_hash"];
            o["changed"] = json!(changed);
            if let Some(h) = o["hash"].as_str() { o["url"] = json!(format!("/api/v1/blob/{h}.png")); }
            if let Some(h) = o["base_hash"].as_str() { o["base_url"] = json!(format!("/api/v1/blob/{h}.png")); }
            o
        }).collect();
        lst.sort_by_key(|o| (!o["changed"].as_bool().unwrap_or(false), o["name"].as_str().unwrap_or("").to_string()));
        Ok(json!({"rev": r, "base": b, "textures": lst}))
    }

    pub fn parts_view(&self, rev: Option<&str>, base: Option<&str>) -> Result<Value> {
        let r = self.resolve_rev(rev)?;
        let b = self.resolve_rev(base.or(Some("base")))?;
        let mats = self.mats.read().unwrap();
        let (_, mt) = diff_mats(mats.get(&b).unwrap_or(&BTreeMap::new()), mats.get(&r).unwrap_or(&BTreeMap::new()));
        let changed: std::collections::HashSet<_> = mt.into_iter().collect();
        let parts_doc = self.parts.read().unwrap().clone().unwrap_or_else(|| json!({"parts": [], "textures": {}}));
        let parts: Vec<Value> = parts_doc["parts"].as_array().cloned().unwrap_or_default().into_iter().map(|mut p| {
            let mats: Vec<String> = p["materials"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
            p["changed"] = json!(mats.iter().any(|m| changed.contains(m)));
            p
        }).collect();
        Ok(json!({
            "rev": r, "base": b,
            "groups": [{"id":"hull","kind":"hull"},{"id":"main","kind":"main"},{"id":"secondary","kind":"secondary"},{"id":"aa","kind":"aa"},{"id":"director","kind":"director"},{"id":"radar","kind":"radar"}],
            "parts": parts
        }))
    }

    pub fn sim_view(&self) -> Result<Value> {
        let sim = self.sim.read().unwrap().clone().ok_or_else(|| anyhow!("not_found"))?;
        Ok(json!({
            "api_version": 1,
            "source": {"gameparams": format!("P{}", self.ship.trim_start_matches('P')), "game_version": self.game.as_ref().and_then(|g| g.client_version.clone())},
            "ship": sim["ship"], "mounts": sim["mounts"],
            "effects": {"emission": {"note": "emissionAnimationMode 等参数见 /model 的 materials[].params"}}
        }))
    }

    pub fn capabilities(&self) -> Value {
        let preview = self.geom.read().unwrap().is_some();
        let simulation = self.sim.read().unwrap().is_some();
        json!({"library": true, "preview": preview, "apply": self.game.is_some(), "undo": false, "restore": false,
            "simulation": simulation, "reasons": {
                "preview": if preview { None } else { self.preview_error.read().unwrap().clone().or_else(|| Some("项目没有可用的三维模型".into())) },
                "apply": if self.game.is_some() { None } else { Some("指定客户端不可用；请用 --game 指定有效客户端") },
                "undo": "撤回需要对象仓库，目前尚未接通", "restore": "恢复需要对象仓库，目前尚未接通",
                "simulation": if simulation { None } else { Some("此项目没有对应舰船的模拟数据") }
            }})
    }
}

// ---------- helpers ----------
pub fn find_seed(ship: &str) -> Option<PathBuf> {
    if ship.is_empty() || !ship.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') { return None; }
    let mut cands = vec![];
    if let Ok(h) = std::env::var("WOWSMOD_HOME") {
        cands.push(PathBuf::from(h).join("preview_seed").join(ship));
    }
    if let Ok(e) = std::env::current_exe() {
        if let Some(d) = e.parent() {
            cands.push(d.join("preview_seed").join(ship));
            cands.push(d.join("../preview_seed").join(ship));
        }
    }
    // 开发树 / 同级 preview_app
    cands.push(PathBuf::from("crates/wowsmod/preview_seed").join(ship));
    cands.push(PathBuf::from("/workspace/m1/wowsmod/crates/wowsmod/preview_seed").join(ship));
    if crate::ship_catalog::canonical_ship_key(ship) == "PFSB510" {
        cands.push(PathBuf::from("D:/WoWSModding/preview_app/mock/data/FSB510"));
        cands.push(PathBuf::from("/workspace/preview_app/mock/data/FSB510"));
    }
    for c in cands {
        if c.join("geometry.glb").is_file() || c.join("sim.json").is_file() {
            // map FSB510 folder name
            return Some(c);
        }
    }
    // ship-specific under preview_app
    let alt = PathBuf::from("D:/WoWSModding/preview_app/mock/data").join(ship);
    if alt.join("geometry.glb").is_file() {
        return Some(alt);
    }
    None
}

fn mtime_ns(m: &std::fs::Metadata) -> u128 {
    m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0)
}

/// 项目内容指纹: 路径+大小+mtime。跳过出图输出与 .wowsmod(其中 cache/geometry.glb 单独计入)
pub fn project_fingerprint(root: &Path) -> u64 {
    fingerprint_files(root, true)
}

/// Source-only guard for rebuilding: preview-generated cache files cannot make
/// an otherwise stable source appear to be edited while it is being read.
pub(super) fn project_source_fingerprint(root: &Path) -> u64 {
    fingerprint_files(root, false)
}

fn fingerprint_files(root: &Path, include_cache: bool) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let skip = ["renders", ".render", ".wowsmod", ".install", ".git", "target"];
    let mut st = vec![root.to_path_buf()];
    let mut items: Vec<(String, u64, u128)> = Vec::new();
    while let Some(d) = st.pop() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() {
                    if d == root && skip.iter().any(|s| e.file_name().to_string_lossy() == *s) {
                        continue;
                    }
                    st.push(p);
                } else if let Ok(md) = e.metadata() {
                    items.push((p.to_string_lossy().into_owned(), md.len(), mtime_ns(&md)));
                }
            }
        }
    }
    for name in ["geometry.glb", "sim.json", "parts.json"].into_iter().filter(|_| include_cache) {
        let cached = root.join(".wowsmod/cache").join(name);
        if let Ok(md) = std::fs::metadata(&cached) {
            items.push((cached.to_string_lossy().into_owned(), md.len(), mtime_ns(&md)));
        }
    }
    items.sort();
    items.hash(&mut h);
    h.finish()
}

fn hash16(d: &[u8]) -> String {
    let h = Sha256::digest(d);
    h.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

pub(super) fn iso_now() -> String {
    // 本地 +08:00 近似(box/用户在上海)
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    let offset = 8 * 3600;
    let t = secs + offset;
    let days = t / 86400;
    let tod = t % 86400;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}+08:00", tod / 3600, (tod % 3600) / 60, tod % 60)
}
fn iso_at(unix: u64) -> String {
    let offset = 8 * 3600i64;
    let t = unix as i64 + offset;
    let days = t / 86400;
    let tod = t % 86400;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}+08:00", tod / 3600, (tod % 3600) / 60, tod % 60)
}
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    // days since 1970-01-01
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

fn family_of(fx: &str, emissive_id: bool) -> String {
    let f = fx.to_lowercase();
    if emissive_id || f.contains("emissive") {
        "emissive".into()
    } else if f.contains("wire") {
        "wire".into()
    } else if f.contains("alpha") || f.contains("grid") {
        "alpha".into()
    } else if f.contains("glass") {
        "glass".into()
    } else {
        "pbs".into()
    }
}

fn detect_hull_model(p: &Project, ship: &str) -> String {
    let d = p.assets().join("PnFMods").join(&p.cfg.meta.name).join(ship).join("ship");
    if let Ok(rd) = std::fs::read_dir(&d) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.ends_with("_Bow.model") {
                return n.trim_end_matches("_Bow.model").to_string();
            }
        }
    }
    String::new()
}

fn walk(dir: &Path, f: &mut dyn FnMut(&Path)) {
    let mut st = vec![dir.to_path_buf()];
    while let Some(d) = st.pop() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    st.push(p);
                } else {
                    f(&p);
                }
            }
        }
    }
}

fn load_edits(root: &Path) -> Result<BTreeMap<String, (String, Value)>> {
    let dir = root.join("edits");
    let mut m = BTreeMap::new();
    if !dir.is_dir() {
        return Ok(m);
    }
    for e in std::fs::read_dir(&dir)?.flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if let Some(cap) = regex::Regex::new(r"^(\d{3})_(.+)\.json$").unwrap().captures(&n) {
            let j: Value = serde_json::from_slice(&std::fs::read(e.path())?)?;
            m.insert(cap[1].to_string(), (n, j));
        }
    }
    Ok(m)
}

fn group_steps(root: &Path, edits: &BTreeMap<String, (String, Value)>) -> Result<Vec<Step>> {
    let meta_path = root.join("edits/steps.json");
    let meta: Option<Value> = if meta_path.is_file() && !is_seed_steps(root) { serde_json::from_slice(&std::fs::read(&meta_path)?).ok() } else { None };
    let mut groups: Vec<(String, String, Option<String>, Option<String>, Vec<String>)> = if let Some(j) = meta {
        j["steps"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|s| {
                (
                    s["id"].as_str().unwrap_or("").to_string(),
                    s["title"].as_str().unwrap_or("").to_string(),
                    s["user_prompt"].as_str().map(|x| x.to_string()),
                    s["ai_note"].as_str().map(|x| x.to_string()),
                    s["edits"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(|s| s.to_string())).collect(),
                )
            })
            .collect()
    } else {
        edits
            .keys()
            .map(|k| {
                let (f, j) = &edits[k];
                (format!("e{k}"), edit_title(f, j), None, None, vec![k.clone()])
            })
            .collect()
    };
    // steps.json 与实际 edits 对不上时(引用了不存在的 edit / 有 edit 没被引用): 丢掉空步骤, 没被引用的 edit 各成一步
    let covered: std::collections::HashSet<String> = groups.iter().flat_map(|g| g.4.iter().cloned()).collect();
    groups.retain(|g| g.4.is_empty() && g.0 == "s0" || g.4.iter().any(|e| edits.contains_key(e)));
    for k in edits.keys().filter(|k| !covered.contains(*k)) {
        let (f, j) = &edits[k];
        groups.push((format!("e{k}"), edit_title(f, j), None, None, vec![k.clone()]));
    }
    let mut out = Vec::new();
    for (i, (id, title, up, note, eds)) in groups.into_iter().enumerate() {
        let ed: Vec<_> = eds.iter().filter_map(|e| edits.get(e).cloned()).collect();
        let tu = ed.iter().filter_map(|(_, j)| j["time_unix"].as_u64()).max().unwrap_or(0);
        let edits_v: Vec<Value> = ed
            .iter()
            .map(|(f, j)| {
                let argv = j["argv"].as_array().cloned().unwrap_or_default();
                let cl = command_line(&argv);
                json!({"file": format!("edits/{f}"), "cmd": j["cmd"], "time": j["time_unix"].as_u64().map(iso_at), "command_line": cl,
                    "outputs": j["outputs"], "warnings": j.get("warnings").cloned().unwrap_or(json!([])), "stats": j.get("stats").or(j.get("glow")).cloned()})
            })
            .collect();
        let files = files_of(&ed);
        out.push(Step { id, index: i, title, user_prompt: up, ai_note: note, state: "active".into(), time_unix: tu, time: iso_at(tu), edits: edits_v, files, textures: vec![], materials: vec![], parts: vec![] });
    }
    Ok(out)
}

/// edits/steps.json 是不是早期 preview 从 mock 种子(勃艮第"深海涂装")拷进来的; 是的话忽略, 按真实 edits 显示
fn is_seed_steps(root: &Path) -> bool {
    let p = root.join("edits/steps.json");
    let Ok(d) = std::fs::read(&p) else { return false };
    let mut seeds = vec![];
    if let Ok(t) = std::fs::read_to_string(root.join("mod.toml")) {
        if let Some(c) = regex::Regex::new(r#"(?m)^ship\s*=\s*"([^"]+)""#).unwrap().captures(&t) {
            if let Some(s) = find_seed(&c[1]) {
                seeds.push(s.join("steps.json"));
            }
        }
    }
    if seeds.iter().any(|s| std::fs::read(s).map(|x| x == d).unwrap_or(false)) {
        return true;
    }
    // 种子特征: 引用了 028 等不存在的 edit, 且标题是 mock 故事
    let t = String::from_utf8_lossy(&d);
    t.contains("\"wowsmod-steps/1\"") && t.contains("整船换深海蓝") && !root.join("edits/028_glow.json").is_file()
}

/// 每个 edit 一步时的标题: 命令 + 主要参数(贴图/颜色/通道/强度)
fn edit_title(file: &str, j: &Value) -> String {
    let cmd = j["cmd"].as_str().unwrap_or(file);
    let argv: Vec<String> = j["argv"].as_array().into_iter().flatten().filter_map(|a| a.as_str().map(|s| s.to_string())).collect();
    let opt = |k: &str| argv.iter().position(|a| a == k).and_then(|i| argv.get(i + 1)).cloned();
    let short = |n: &str| {
        let stem = n.rsplit(['/', '\\']).next().unwrap_or(n).trim_end_matches(".dds").to_string();
        let segs: Vec<&str> = stem.split('_').collect();
        if segs.len() > 3 { segs[segs.len() - 2..].join("_") } else { stem }
    };
    let _ = short;
    // 贴图名先写成 ⟦全名⟧, rebuild 拿到部件表后换成部件中文名(resolve_part_tokens)
    let token = |n: &str| format!("⟦{}⟧", n.rsplit(['/', '\\']).next().unwrap_or(n).trim_end_matches(".dds"));
    let tex = argv.iter().find(|a| a.to_lowercase().ends_with(".dds") && !a.starts_with("--")).map(|s| token(s));
    let verb = match cmd {
        "import_sdk" => return "基线: 导入官方 SDK 模型".into(),
        "tex_recolor" => "换色",
        "tex_channel" => "改通道",
        "tex_import" => "导入贴图",
        "tex_rewrite" => "重写贴图",
        "glow" => "发光",
        "port_albedo" => "港口色换算",
        c if c.starts_with("model") => "改模型",
        c => c,
    };
    let mut parts = vec![verb.to_string()];
    match cmd {
        "glow" => {
            if let Some(m) = opt("--mg") { parts.push(token(&m)); }
            if let Some(c) = opt("--color") { parts.push(c); }
            if let Some(p) = opt("--power") { parts.push(format!("强度 {p}")); }
        }
        "tex_channel" => {
            if let Some(t) = tex { parts.push(t); }
            if let Some(c) = opt("--channel") {
                parts.push(match (opt("--set"), opt("--mul")) { (Some(v), _) => format!("{c}={v}"), (_, Some(m)) => format!("{c}×{m}"), _ => c });
            }
        }
        _ => {
            if let Some(t) = tex { parts.push(t); }
            if let Some(c) = opt("--to-color") { parts.push(format!("→ {c}")); }
            if let Some(h) = opt("--hue-shift") { parts.push(format!("色相 {h}°")); }
        }
    }
    parts.join(" ")
}

const GROUP_ZH: &[(&str, &str)] = &[("hull", "船体"), ("main", "主炮"), ("secondary", "副炮"), ("aa", "防空炮"), ("director", "火控指挥仪"), ("radar", "雷达")];

/// 贴图名(去掉 .dds) → 部件中文名: 先按部件表(哪些部件的材质用这个贴图), 再按名字关键字, 都不行就用缩写
fn part_label(stem: &str, parts_doc: Option<&Value>) -> String {
    let segs: Vec<String> = stem.split('_').map(|x| x.to_lowercase()).collect();
    let kw = [("deckhouse", "上层建筑"), ("hull", "船体"), ("barrel", "炮管"), ("funnel", "烟囱"), ("mast", "桅杆"), ("bridge", "舰桥"), ("misc", "杂项"), ("bow", "舰艏"), ("stern", "舰艉")];
    let kwl = kw.iter().find(|(k, _)| segs.iter().any(|s| s == k)).map(|x| x.1);
    // 去掉通道后缀(_a/_mg/_n/_ao/_anim)得到材质名前缀
    let mut base = stem.to_lowercase();
    for suf in ["_anim", "_mg", "_ao", "_a", "_n"] {
        if let Some(b) = base.strip_suffix(suf) {
            base = b.to_string();
            break;
        }
    }
    let mut groups: Vec<&str> = vec![];
    if let Some(doc) = parts_doc {
        for p in doc["parts"].as_array().into_iter().flatten() {
            let hit = p["materials"].as_array().into_iter().flatten().filter_map(|m| m.as_str()).any(|m| {
                let m = m.to_lowercase();
                let m = m.trim_end_matches(".mfm");
                m == base || m.strip_prefix(&base).map(|r| r.starts_with('_')).unwrap_or(false)
            });
            if hit {
                let g = p["group"].as_str().unwrap_or("");
                if let Some((_, zh)) = GROUP_ZH.iter().find(|(k, _)| *k == g) {
                    if !groups.contains(zh) {
                        groups.push(zh);
                    }
                }
            }
        }
    }
    if groups.is_empty() || groups == ["船体"] {
        if let Some(k) = kwl {
            return k.to_string();
        }
        if !groups.is_empty() {
            return "船体".into();
        }
        let s: Vec<&str> = stem.split('_').collect();
        return if s.len() > 3 { s[s.len() - 3..].join("_") } else { stem.to_string() };
    }
    groups.retain(|g| *g != "船体");
    let mut l = groups.iter().take(2).cloned().collect::<Vec<_>>().join("/");
    if kwl == Some("炮管") {
        l.push_str("炮管");
    }
    l
}

fn resolve_part_tokens(title: &str, parts_doc: Option<&Value>) -> String {
    let mut out = String::new();
    let mut rest = title;
    while let Some(a) = rest.find('⟦') {
        out.push_str(&rest[..a]);
        let after = &rest[a + '⟦'.len_utf8()..];
        match after.find('⟧') {
            Some(b) => {
                out.push_str(&part_label(&after[..b], parts_doc));
                rest = &after[b + '⟧'.len_utf8()..];
            }
            None => {
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn command_line(argv: &[Value]) -> String {
    let parts: Vec<String> = argv.iter().skip(1).filter_map(|a| a.as_str().map(|s| s.to_string())).collect();
    let mut s = String::from("wowsmod");
    for p in parts {
        let p2 = if p.contains("m2_projects") || p.contains("m3_projects") { ".".into() } else { p };
        if p2.contains(' ') {
            s.push_str(&format!(" \"{p2}\""));
        } else {
            s.push(' ');
            s.push_str(&p2);
        }
    }
    s
}

fn files_of(ed: &[(String, Value)]) -> Vec<Value> {
    let mut seen = BTreeMap::new();
    for (_, j) in ed {
        if j["cmd"] == "import_sdk" {
            seen.insert(format!("assets/res_mods/… ({} 个文件)", j["files"].as_u64().unwrap_or(0)), "added");
            continue;
        }
        for o in j["outputs"].as_array().into_iter().flatten() {
            let p = o["path"].as_str().unwrap_or("").replace('\\', "/");
            if regex::Regex::new(r"\.dd\d$").unwrap().is_match(&p) {
                continue;
            }
            seen.insert(p, "modified");
        }
        for m in j["mfm"].as_array().into_iter().flatten() {
            if let Some(p) = m["mfm"].as_str() {
                seen.insert(p.replace('\\', "/"), "modified");
            }
        }
    }
    seen.into_iter().map(|(path, change)| json!({"path": path, "change": change})).collect()
}

fn apply_edit_to_mats(mats: &mut BTreeMap<String, Mat>, ed: &Value, blobs: &mut HashMap<String, BlobSrc>, project: &Project) {
    for o in ed["outputs"].as_array().into_iter().flatten() {
        let path = o["path"].as_str().unwrap_or("");
        let sha = o["sha256"].as_str().unwrap_or("");
        if sha.is_empty() {
            continue;
        }
        let h = sha.chars().take(16).collect::<String>();
        let abs = project.root.join(path);
        if abs.is_file() {
            blobs.insert(h.clone(), BlobSrc::Project(abs));
        }
        let name = Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        // update any mat texture with this name
        for mat in mats.values_mut() {
            for t in mat.textures.values_mut() {
                if t.name == name || t.name == name.trim_end_matches("_a").to_string() + /* keep */ "" {
                    if t.name == name {
                        t.hash = Some(h.clone());
                        t.missing = false;
                    }
                }
            }
        }
        let stem = name.clone();
        for mat in mats.values_mut() {
            for t in mat.textures.values_mut() {
                if t.name == stem {
                    t.hash = Some(h.clone());
                    t.missing = false;
                }
            }
        }
    }
}

fn diff_mats(a: &BTreeMap<String, Mat>, b: &BTreeMap<String, Mat>) -> (Vec<String>, Vec<String>) {
    let mut tx = std::collections::BTreeSet::new();
    let mut mt = std::collections::BTreeSet::new();
    for (m, e) in b {
        let ea = a.get(m);
        if ea.map(|x| x.fx.as_str()) != Some(e.fx.as_str()) || ea.map(|x| &x.params) != Some(&e.params) || ea.map(|x| x.family.as_str()) != Some(e.family.as_str()) {
            mt.insert(m.clone());
        }
        for (slot, t) in &e.textures {
            let th = ea.and_then(|x| x.textures.get(slot)).and_then(|x| x.hash.clone());
            if th != t.hash {
                tx.insert(t.name.clone());
                mt.insert(m.clone());
            }
        }
    }
    (tx.into_iter().collect(), mt.into_iter().collect())
}

fn postprocess_glb_materials(raw: &[u8]) -> Result<Vec<u8>> {
    // 最小后处理: 为每个 primitive 按 extras.mfm 建 material 名; 给节点加 part
    use wowsmod_formats::glb;
    let (mut j, _prims) = glb::read_glb(raw).map_err(|e| anyhow!("{e}"))?;
    // re-read to get bin
    if raw.len() < 20 {
        bail!("bad glb");
    }
    let jl = u32::from_le_bytes(raw[12..16].try_into().unwrap()) as usize;
    let bin_off = 20 + jl;
    let bin = if bin_off + 8 <= raw.len() && &raw[bin_off + 4..bin_off + 8] == b"BIN\0" {
        let bl = u32::from_le_bytes(raw[bin_off..bin_off + 4].try_into().unwrap()) as usize;
        raw[bin_off + 8..bin_off + 8 + bl].to_vec()
    } else {
        vec![]
    };
    let mut mat_index: HashMap<String, usize> = HashMap::new();
    let mut materials = Vec::new();
    if let Some(meshes) = j["meshes"].as_array_mut() {
        for m in meshes {
            if let Some(prims) = m["primitives"].as_array_mut() {
                for p in prims {
                    let mfm_path = p["extras"]["mfm"].as_str().unwrap_or("").to_string();
                    let name = Path::new(&mfm_path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "unknown.mfm".into());
                    let idx = *mat_index.entry(name.clone()).or_insert_with(|| {
                        materials.push(json!({"name": name}));
                        materials.len() - 1
                    });
                    p["material"] = json!(idx);
                    // 前端用 extras.mfm 覆盖材质名查表(键为文件名), 全路径会让整船查不到材质而全白
                    if !mfm_path.is_empty() {
                        p["extras"]["mfm_path"] = json!(mfm_path);
                        p["extras"]["mfm"] = json!(name.clone());
                    }
                    // keep identifier in extras
                    if p["extras"].get("materialIdentifier").is_some() {
                        let id = p["extras"]["materialIdentifier"].clone();
                        p["extras"]["identifier"] = id;
                    }
                }
            }
        }
    }
    j["materials"] = json!(materials);
    if let Some(nodes) = j["nodes"].as_array_mut() {
        for n in nodes {
            if let Some(sec) = n["extras"]["section"].as_str() {
                n["extras"]["part"] = json!(format!("hull/{sec}"));
                n["extras"]["lods"] = json!([{"lod": 0, "extent": 1e9}]);
            }
            let name = n["name"].as_str().unwrap_or("").to_string();
            if n["extras"].get("anim").is_some() {
                // seq 驱动的独立模型: 单独成部件, 前端按 extras.anim.frames 播放
                n["extras"]["part"] = json!(format!("anim/{name}"));
            }
            if name.starts_with("HP_") {
                n["extras"]["part"] = json!(name);
                n["extras"]["hp"] = json!(name);
                n["extras"]["bones"] = json!([]);
                n["extras"]["lods"] = json!([{"lod": 0, "extent": 1e9}]);
            }
        }
    }
    // rewrite GLB
    let js = serde_json::to_vec(&j)?;
    let mut js_pad = js.clone();
    while js_pad.len() % 4 != 0 {
        js_pad.push(b' ');
    }
    let mut bin_pad = bin;
    while bin_pad.len() % 4 != 0 {
        bin_pad.push(0);
    }
    let mut out = Vec::with_capacity(12 + 8 + js_pad.len() + 8 + bin_pad.len());
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    let total = (12 + 8 + js_pad.len() + 8 + bin_pad.len()) as u32;
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&(js_pad.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&js_pad);
    out.extend_from_slice(&(bin_pad.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin_pad);
    Ok(out)
}

#[cfg(test)]
mod library_tests {
    use super::*;

    #[test]
    fn game_dds_names_match_unique_basenames_with_ascii_case_folding() {
        let names = GameDdsNames::new([
            "shared/ordinary.png",
            "ship/PaInT.DdS",
            "other/paint.dd0",
            "ship/Çamo.dds",
        ]);
        assert_eq!(names.unique_index("pAiNt"), Some(1));
        assert_eq!(names.unique_index("ÇAMO"), Some(3));
        assert_eq!(names.unique_index("çamo"), None);
        assert_eq!(names.unique_index("ship/paint"), None);
        assert_eq!(names.unique_index("ordinary"), None);
        assert_eq!(names.unique_index("missing"), None);
    }

    #[test]
    fn game_dds_names_keep_duplicate_basenames_ambiguous() {
        let names = GameDdsNames::new([
            "ship/two.dds",
            "shared/TWO.DDS",
            "ship/three.dds",
            "shared/Three.dds",
            "other/THREE.DDS",
        ]);
        assert_eq!(names.unique_index("two"), None);
        assert_eq!(names.unique_index("three"), None);
    }

    #[test]
    fn imported_assets_without_history_expose_a_current_snapshot() {
        let dir = std::env::temp_dir().join(format!("preview_no_history_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Project::init(&dir, "same", "UNKNOWN_Example", "loose", "").unwrap();
        let material_dir = dir.join("assets/res_mods/PnFMods/same/UNKNOWN_Example");
        std::fs::create_dir_all(&material_dir).unwrap();
        std::fs::write(material_dir.join("current.mfm"), "<mfm><fx>PBS_ship.fx</fx><property>g_detailScaleU<Float>3</Float></property></mfm>").unwrap();
        std::fs::create_dir_all(dir.join(".wowsmod/cache")).unwrap();
        std::fs::write(dir.join(".wowsmod/cache/geometry.glb"), b"project model").unwrap();
        std::fs::remove_dir(dir.join("edits")).unwrap();
        let cli = Cli { game: Some(dir.join("missing-client")), json: true, json_out: None, cmd: crate::Cmd::Doctor };
        let st = PreviewState::open(&cli, &dir).unwrap();
        assert_eq!(st.head_id().as_deref(), Some("current"));
        assert_eq!(st.steps_view()["steps"], json!([]));
        assert_eq!(st.model_view(None).unwrap()["rev"], "current");
        assert_eq!(st.model_view(None).unwrap()["materials"]["current.mfm"]["params"]["g_detailScaleU"], 3.0);
        assert_eq!(st.capabilities()["preview"], true);
        assert!(!dir.join("edits").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn global_and_unknown_projects_open_without_invented_model_or_client() {
        for ship in ["", "UNKNOWN_Example"] {
            let dir = std::env::temp_dir().join(format!("preview_missing_{}_{}", std::process::id(), ship));
            let _ = std::fs::remove_dir_all(&dir);
            let p = Project::init(&dir, "same", ship, "loose", "").unwrap();
            let cli = Cli { game: Some(dir.join("missing-client")), json: true, json_out: None, cmd: crate::Cmd::Doctor };
            let st = PreviewState::open(&cli, &dir).expect("metadata must remain available without 3D resources");
            assert!(st.game.is_none());
            assert_eq!(st.hull_model, "", "never invent a hull from the ship name");
            let v = st.project_view();
            assert_eq!(v["capabilities"]["preview"], false);
            assert_eq!(v["capabilities"]["simulation"], false);
            assert_eq!(v["capabilities"]["apply"], false);
            assert!(v["capabilities"]["reasons"]["preview"].is_string());
            assert_eq!(st.parts_view(None, None).unwrap()["parts"], json!([]));
            assert!(!p.root.join(".wowsmod").exists(), "unavailable preview must not manufacture source caches");
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }
}
