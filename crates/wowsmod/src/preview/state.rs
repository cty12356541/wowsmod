//! 从 mod 项目构建预览状态: 步骤、材质表(base/head/中间)、贴图 hash→来源、几何缓存、模拟数据。
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use wowsmod_formats::{mfm, texture::{self, Image}};

use crate::game::{self, GameInstall};
use crate::modproj::{self, Project};
use crate::texcmd;
use crate::{game_ctx, Cli, GameCtx};

const SLOT: &[(&str, &str)] = &[("diffuseMap", "a"), ("normalMap", "n"), ("metallicGlossMap", "mg"), ("ambientOcclusionMap", "ao"), ("animMap", "anim")];

pub struct PreviewState {
    pub project: Project,
    pub ship: String,
    pub hull_model: String,
    pub game: Option<GameInstall>,
    pub ctx: Option<GameCtx>,
    pub etag: AtomicU64,
    pub shutting_down: AtomicBool,
    steps: RwLock<Vec<Step>>,
    /// rev id → materials
    mats: RwLock<HashMap<String, BTreeMap<String, Mat>>>,
    /// hash16 → 如何取贴图
    blobs: RwLock<HashMap<String, BlobSrc>>,
    geom: RwLock<Option<Geom>>,
    sim: RwLock<Option<Value>>,
    parts: RwLock<Option<Value>>,
    install: RwLock<Value>,
    activity: RwLock<Value>,
    pub jobs: Mutex<HashMap<String, Value>>,
    sse: Mutex<Vec<std::sync::mpsc::Sender<Value>>>,
    png_cache: Mutex<HashMap<(String, u32, String), Vec<u8>>>,
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
}

impl PreviewState {
    pub fn open(cli: &Cli, root: &Path) -> Result<Self> {
        let project = Project::open(root)?;
        let ship = project.cfg.meta.ship.clone();
        if ship.is_empty() {
            bail!("mod.toml 没有 ship");
        }
        let _ = crate::preview::ensure_steps_json(root, &ship);
        let ctx = game_ctx(cli, false)?;
        let game = ctx.as_ref().map(|c| c.install.clone());
        let hull_model = detect_hull_model(&project, &ship);
        let st = PreviewState {
            project,
            ship: ship.clone(),
            hull_model,
            game,
            ctx,
            etag: AtomicU64::new(1),
            shutting_down: AtomicBool::new(false),
            steps: RwLock::new(vec![]),
            mats: RwLock::new(HashMap::new()),
            blobs: RwLock::new(HashMap::new()),
            geom: RwLock::new(None),
            sim: RwLock::new(None),
            parts: RwLock::new(None),
            install: RwLock::new(json!({"state": "not_installed", "rev": null, "files": 0, "time": null})),
            activity: RwLock::new(json!({"state": "idle", "title": null, "since": null, "step": null})),
            jobs: Mutex::new(HashMap::new()),
            sse: Mutex::new(Vec::new()),
            png_cache: Mutex::new(HashMap::new()),
        };
        st.rebuild()?;
        st.refresh_install();
        Ok(st)
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
        steps.iter().rev().find(|s| s.state == "active").map(|s| s.id.clone())
    }

    pub fn etag(&self) -> u64 {
        self.etag.load(Ordering::SeqCst)
    }

    fn rebuild(&self) -> Result<()> {
        let edits = load_edits(&self.project.root)?;
        let steps = group_steps(&self.project.root, &edits)?;
        // base materials from SDK copy
        let mut blobs = HashMap::new();
        let base_mats = self.scan_materials(false, &mut blobs)?;
        let head_mats = self.scan_materials(true, &mut blobs)?;
        // intermediate: start from base, apply texture outputs from edits
        let mut mats_map = HashMap::new();
        mats_map.insert("base".into(), base_mats.clone());
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
        self.ensure_geometry()?;
        self.ensure_sim_parts()?;
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

    fn scan_materials(&self, head: bool, blobs: &mut HashMap<String, BlobSrc>) -> Result<BTreeMap<String, Mat>> {
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
                    let id = regex::Regex::new(r"<identifier>\s*(\S+)\s*</identifier>").unwrap().captures(mat).map(|c| c[1].to_string());
                    let mf = regex::Regex::new(r"<mfm>\s*(\S+)\s*</mfm>").unwrap().captures(mat).map(|c| Path::new(&c[1]).file_name().unwrap().to_string_lossy().to_string());
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
                    "Int" | "Bool" => json!(v.parse::<i64>().unwrap_or(0)),
                    _ => json!(v),
                };
                params.insert(n, val);
            }
            let mut textures = BTreeMap::new();
            for (pn, _, v) in mfm::properties(&text) {
                if let Some((_, slot)) = SLOT.iter().find(|(s, _)| *s == pn) {
                    let tname = Path::new(&v).file_stem().unwrap_or_default().to_string_lossy().to_string();
                    let (hash, missing, w, h) = self.resolve_tex(&tname, head, Path::new(&v), blobs);
                    textures.insert(slot.to_string(), TexRef { name: tname, hash, missing, width: w, height: h });
                }
            }
            out.insert(name, Mat { family, fx: fx_base, params, textures });
        });
        Ok(out)
    }

    fn resolve_tex(&self, name: &str, head: bool, rel_in_mfm: &Path, blobs: &mut HashMap<String, BlobSrc>) -> (Option<String>, bool, Option<u32>, Option<u32>) {
        // 1) project exact path under assets
        if head {
            let under = self.project.assets().join(rel_in_mfm);
            if under.is_file() {
                if let Ok(d) = std::fs::read(&under) {
                    if !mfm::is_sdk_stub(&d) {
                        let h = hash16(&d);
                        blobs.insert(h.clone(), BlobSrc::Project(under));
                        return (Some(h), false, None, None);
                    }
                }
            }
            // by name unique non-stub
            if let Ok(hits) = self.project.find_by_name(&format!("{name}.dds")) {
                let real: Vec<_> = hits.into_iter().filter(|p| std::fs::read(p).map(|d| !mfm::is_sdk_stub(&d)).unwrap_or(false)).collect();
                if real.len() == 1 {
                    if let Ok(d) = std::fs::read(&real[0]) {
                        let h = hash16(&d);
                        blobs.insert(h.clone(), BlobSrc::Project(real[0].clone()));
                        return (Some(h), false, None, None);
                    }
                }
            }
        }
        // 2) game by file name
        if let Some(ctx) = &self.ctx {
            let hits: Vec<_> = ctx.index.entries.iter().filter(|e| e.path.rsplit('/').next().map(|x| x.eq_ignore_ascii_case(&format!("{name}.dds"))).unwrap_or(false)).collect();
            if hits.len() == 1 {
                let vp = hits[0].path.clone();
                // hash of game dds header file (not volumes) — use path-stable: read and hash
                if let Ok(d) = ctx.reader.read(hits[0]) {
                    let h = hash16(&d);
                    blobs.insert(h.clone(), BlobSrc::Game(vp));
                    return (Some(h), false, None, None);
                }
            }
            if hits.len() != 1 {
                return (None, true, None, None);
            }
        }
        (None, true, None, None)
    }

    fn ensure_geometry(&self) -> Result<()> {
        if self.geom.read().unwrap().is_some() {
            return Ok(());
        }
        let cache = self.project.root.join(".wowsmod/cache");
        std::fs::create_dir_all(&cache)?;
        let cached = cache.join("geometry.glb");
        let bytes = if cached.is_file() {
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
        *self.geom.write().unwrap() = Some(Geom { hash, bytes });
        Ok(())
    }

    fn build_geometry_fallback(&self, dest: &Path) -> Result<Vec<u8>> {
        // 调用自身 assemble --full, 再后处理材质名
        let exe = std::env::current_exe()?;
        let out = dest.with_extension("raw.glb");
        let mut c = std::process::Command::new(&exe);
        c.args(["--json", "assemble", &self.ship, "--project"]).arg(&self.project.root).args(["--full", "-o"]).arg(&out);
        if let Some(g) = &self.game {
            c.arg("--game").arg(&g.root);
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
        let Some(g) = &self.game else { return };
        if let Ok(reg) = crate::registry::load(&g.res_mods) {
            if let Some(m) = reg.mods.get(&self.project.cfg.meta.name) {
                *self.install.write().unwrap() = json!({"state": "installed", "rev": self.head_id(), "files": m.files.len(), "time": null, "project_dir": m.project_dir});
                return;
            }
        }
        *self.install.write().unwrap() = json!({"state": "not_installed", "rev": null, "files": 0, "time": null});
    }

    // ---------- JSON views ----------
    pub fn health(&self) -> Value {
        json!({"ok": true, "api_version": 1, "server": "wowsmod", "version": env!("CARGO_PKG_VERSION"), "time": iso_now()})
    }

    pub fn project_view(&self) -> Value {
        let running = game::running();
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
                "code": self.ship, "param": format!("P{}", self.ship.trim_start_matches('P')),
                "index": format!("P{}", self.ship.trim_start_matches('P').split('_').next().unwrap_or("")),
                "hull_model": self.hull_model, "name": {"zh": self.ship.clone(), "en": self.ship.clone(), "ru": self.ship.clone()},
                "level": null, "nation": null, "species": null
            },
            "game": {"client": "Мир кораблей", "version": ver, "build": build, "path": path, "running": !running.is_empty()},
            "install": self.install.read().unwrap().clone(),
            "activity": self.activity.read().unwrap().clone(),
            "head": self.head_id(), "etag": self.etag()
        })
    }

    pub fn state_view(&self) -> Value {
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
            None | Some("") | Some("head") | Some("current") => self.head_id().ok_or_else(|| anyhow!("no_model")),
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
}

// ---------- helpers ----------
pub fn find_seed(ship: &str) -> Option<PathBuf> {
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
    cands.push(PathBuf::from("D:/WoWSModding/preview_app/mock/data/FSB510")); // Windows 常见
    cands.push(PathBuf::from("/workspace/preview_app/mock/data/FSB510"));
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

fn hash16(d: &[u8]) -> String {
    let h = Sha256::digest(d);
    h.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn iso_now() -> String {
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
    ship.replace("FSB510_", "FSB025_") // 勃艮第常见
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
    let tex = argv.iter().find(|a| a.to_lowercase().ends_with(".dds") && !a.starts_with("--")).map(|s| short(s));
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
            if let Some(m) = opt("--mg") { parts.push(short(&m)); }
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
                    let mfm_path = p["extras"]["mfm"].as_str().unwrap_or("");
                    let name = Path::new(mfm_path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "unknown.mfm".into());
                    let idx = *mat_index.entry(name.clone()).or_insert_with(|| {
                        materials.push(json!({"name": name}));
                        materials.len() - 1
                    });
                    p["material"] = json!(idx);
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
