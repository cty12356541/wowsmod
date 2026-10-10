//! M3 子命令: assemble(整船装配) / model(模型写回: scale/move/rotate/replace/remove/add/rebuild/export-blender/import-blender/undo/replay/list)
//!
//! 模型编辑作用在 SDK 路线的 mod 项目(PnFMods/<mod>/<ship>/...)上:
//! - 目标 `--hp HP_xxx`: 改 `*_ports.visual` 里挂点节点的矩阵(只影响这一个挂点实例);
//! - 目标 `--part <模型名>`: 改该部件的 .geometry(全部 LOD 渲染集)与 visual 节点(枢轴/骨骼位置同步)、包围盒;
//! - 目标 `--renderset <正则> [--section Bow|MidFront|MidBack|Stern]`: 船体渲染集, 按“去掉 _lodN 后的家族名”同时作用于全部 LOD visual。
//! 每次编辑先在内存里完成, 全部成功才落盘; 改动前的文件备份到 edits/.undo/<id>/, 记录写入 edits/NNN_model_*.json(可 undo / replay)。
use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Subcommand};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use wowsmod_formats::{
    assemble::{self, AssembleOpts},
    geomedit::{GeoEdit, MeshData},
    geometry::Geometry,
    mfm,
    scene::{self, game_to_gltf, pos_game_to_gltf, SceneBuilder, ScenePrim},
    visual::{compose, inverse, mat_mul, xform_dir, xform_point, NewRenderSet, Visual, IDENTITY, M4},
};

use crate::modproj::{self, GameLookup, Project};
use crate::{emit, game_ctx, Cli, GameCtx};

/// 取真实 .geometry 字节: 先查 --geom-dir(按文件名, 递归), 再查游戏包(按需加载索引)。
pub struct GeomSrc<'a> {
    map: HashMap<String, PathBuf>,
    cli: &'a Cli,
    ctx: OnceCell<Option<GameCtx>>,
}

impl<'a> GeomSrc<'a> {
    pub fn new(cli: &'a Cli, dirs: &[PathBuf]) -> Result<Self> {
        let mut map = HashMap::new();
        for d in dirs {
            if !d.is_dir() {
                bail!("--geom-dir {} 不是目录", d.display());
            }
            let mut st = vec![d.clone()];
            while let Some(x) = st.pop() {
                for e in std::fs::read_dir(&x)?.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        st.push(p);
                    } else if let Some(n) = p.file_name() {
                        map.entry(n.to_string_lossy().to_lowercase()).or_insert(p);
                    }
                }
            }
        }
        Ok(GeomSrc { map, cli, ctx: OnceCell::new() })
    }
    fn game(&self) -> Option<&GameCtx> {
        self.ctx.get_or_init(|| game_ctx(self.cli, false).ok().flatten()).as_ref()
    }
    /// 按文件名取(如 FGS036_127mm_54_5_1948.geometry)
    pub fn get(&self, file: &str) -> Result<Vec<u8>> {
        if let Some(p) = self.map.get(&file.to_lowercase()) {
            return std::fs::read(p).with_context(|| format!("读取 {}", p.display()));
        }
        let g = self.game().ok_or_else(|| anyhow!("{file}: --geom-dir 里没有, 也找不到游戏客户端(用 --game 或 --geom-dir)"))?;
        let e = GameLookup::new(&g.index).unique(file, None).with_context(|| format!("在游戏包里查找 {file}"))?;
        g.reader.read(e).map_err(|x| anyhow!("读取 {}: {x}", e.path))
    }
    fn gameparams_raw(&self) -> Result<Vec<u8>> {
        let g = self.game().ok_or_else(|| anyhow!("需要游戏客户端读取 GameParams(或用 --gp 指定 JSON)"))?;
        let e = g.index.get("content/GameParams_py3.data").ok_or_else(|| anyhow!("索引中没有 content/GameParams_py3.data"))?;
        g.reader.read(e).map_err(|x| anyhow!("{x}"))
    }
}

fn parse_vec3(s: &str) -> Result<[f64; 3]> {
    let v: Vec<f64> = s
        .split(|c| c == ',' || c == ' ')
        .filter(|x| !x.is_empty())
        .map(|x| x.trim().parse::<f64>())
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| anyhow!("{s:?} 不是数字列表: {e}"))?;
    match v.len() {
        1 => Ok([v[0]; 3]),
        3 => Ok([v[0], v[1], v[2]]),
        _ => bail!("{s:?}: 需要 1 个或 3 个数(x,y,z)"),
    }
}

fn m4_json(m: &M4) -> Value {
    json!(m.iter().map(|r| r.iter().map(|x| (x * 1e6).round() / 1e6).collect::<Vec<_>>()).collect::<Vec<_>>())
}

#[derive(Args, Debug)]
pub struct AssembleArgs {
    /// 船: GameParams 名或代号(PFSB510_Bourgogne / FSB510_Bourgogne / PFSB510)
    pub ship: String,
    /// 输出 GLB
    #[arg(short, long)]
    pub out: PathBuf,
    /// 装配表 JSON(默认 <out 去掉扩展名>_assembly.json)
    #[arg(long)]
    pub table: Option<PathBuf>,
    /// SDK 船目录(含 ship/ gun/ ...; 或其上级目录); 与 --project 二选一
    #[arg(long)]
    pub sdk: Option<PathBuf>,
    /// 用 mod 项目里的(可能已编辑的)visual 与 geometry 装配, 便于预览改动
    #[arg(long)]
    pub project: Option<PathBuf>,
    /// 额外的 .geometry 目录(按文件名查找, 优先于游戏包)
    #[arg(long = "geom-dir")]
    pub geom_dir: Vec<PathBuf>,
    /// GameParams 船条目 JSON(字典, 键为 GameParams 名); 省略则从游戏读 GameParams_py3.data
    #[arg(long)]
    pub gp: Option<PathBuf>,
    /// 船体渲染集: intact(默认, 主体+patch) / broken / main / all
    #[arg(long, default_value = "intact")]
    pub hullsets: String,
    /// 装哪些模块种类(逗号分隔), 或 all
    #[arg(long, default_value = "Artillery,ATBA")]
    pub kinds: String,
    /// 额外输出法线/UV/材质分组(Python 版只有 POSITION)
    #[arg(long)]
    pub full: bool,
}

fn resolve_ship_json(gp: &Value, q: &str) -> Result<(String, Value)> {
    let o = gp.as_object().ok_or_else(|| anyhow!("--gp 不是 JSON 字典"))?;
    let qp = if q.starts_with('P') { q.to_string() } else { format!("P{q}") };
    for k in [q.to_string(), qp.clone()] {
        if let Some(v) = o.get(&k) {
            return Ok((k, v.clone()));
        }
    }
    let hits: Vec<&String> = o.keys().filter(|k| k.starts_with(&format!("{qp}_")) || o[*k].get("index").and_then(|x| x.as_str()) == Some(qp.as_str())).collect();
    match hits.as_slice() {
        [k] => Ok(((*k).clone(), o[*k].clone())),
        [] => bail!("--gp 中没有船 {q}"),
        _ => bail!("{q} 有多个匹配: {hits:?}"),
    }
}

fn ship_from_game(src: &GeomSrc, q: &str) -> Result<(String, Value)> {
    let raw = src.gameparams_raw()?;
    let qp = if q.starts_with('P') { q.to_string() } else { format!("P{q}") };
    for k in [q.to_string(), qp.clone(), qp.split('_').next().unwrap_or(&qp).to_string()] {
        if let Ok(r) = wowsmod_formats::gameparams::ship_json(&raw, &k) {
            return Ok(r);
        }
    }
    bail!("GameParams 中没有船 {q}")
}

/// SDK 船目录: dir 本身含 ship/, 或 dir/<sdk名>/ship
fn sdk_ship_dir(dir: &Path, sdk_name: &str) -> Result<PathBuf> {
    if dir.join("ship").is_dir() {
        return Ok(dir.to_path_buf());
    }
    for c in [dir.join(sdk_name), dir.join("PnFMods").join("ModsSDK.zip").join(sdk_name)] {
        if c.join("ship").is_dir() {
            return Ok(c);
        }
    }
    bail!("{} 里找不到 SDK 船目录(需要含 ship/ 子目录)", dir.display())
}

pub fn project_ship_dir(p: &Project) -> Result<PathBuf> {
    let d = p.assets().join("PnFMods").join(&p.cfg.meta.name).join(&p.cfg.meta.ship);
    if !d.join("ship").is_dir() {
        bail!("{} 不是 SDK 路线项目, 或还没 import-sdk(缺少 {})", p.root.display(), d.join("ship").display());
    }
    Ok(d)
}

/// 读 .geometry: 项目文件是 SDK 占位时从 GeomSrc 取真实字节
fn load_geometry(path: &Path, src: &GeomSrc) -> Result<Vec<u8>> {
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    match std::fs::read(path) {
        Ok(d) if !mfm::is_sdk_stub(&d) => Ok(d),
        _ => src.get(&name),
    }
}

pub fn cmd_assemble(cli: &Cli, a: &AssembleArgs) -> Result<i32> {
    let t0 = std::time::Instant::now();
    let src = GeomSrc::new(cli, &a.geom_dir)?;
    let (ship_name, ship) = match &a.gp {
        Some(p) => {
            let v: Value = serde_json::from_slice(&std::fs::read(p).with_context(|| format!("读取 {}", p.display()))?)?;
            resolve_ship_json(&v, &a.ship)?
        }
        None => ship_from_game(&src, &a.ship)?,
    };
    let sdk_name = ship_name.strip_prefix('P').unwrap_or(&ship_name).to_string();
    let (sdk, proj_dir) = match (&a.project, &a.sdk) {
        (Some(p), _) => {
            let pr = Project::open(p)?;
            let d = project_ship_dir(&pr)?;
            (d.clone(), Some(d))
        }
        (None, Some(s)) => (sdk_ship_dir(s, &sdk_name)?, None),
        (None, None) => bail!("需要 --sdk <SDK 船目录> 或 --project <mod 项目>"),
    };
    // geometry: 项目/SDK 目录里的非占位文件优先, 其次 --geom-dir / 游戏
    let mut local: HashMap<String, PathBuf> = HashMap::new();
    for root in proj_dir.iter().chain(std::iter::once(&sdk)) {
        let mut st = vec![root.clone()];
        while let Some(x) = st.pop() {
            for e in std::fs::read_dir(&x)?.flatten() {
                let p = e.path();
                if p.is_dir() {
                    st.push(p);
                } else if p.extension().map(|x| x == "geometry").unwrap_or(false) {
                    local.entry(p.file_stem().unwrap().to_string_lossy().to_lowercase()).or_insert(p);
                }
            }
        }
    }
    let geometry = |stem: &str| -> wowsmod_formats::error::Result<Vec<u8>> {
        if let Some(p) = local.get(&stem.to_lowercase()) {
            if let Ok(d) = std::fs::read(p) {
                if !mfm::is_sdk_stub(&d) {
                    return Ok(d);
                }
            }
        }
        src.get(&format!("{stem}.geometry")).map_err(|e| wowsmod_formats::error::Error::NotFound(format!("{e:#}")))
    };
    let kinds: Vec<String> = a.kinds.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let opts = AssembleOpts { hullsets: a.hullsets.clone(), kinds, full_attributes: a.full };
    let (glb, mut table, warn) = assemble::assemble(&sdk, &ship_name, &ship, &geometry, &opts).map_err(|e| anyhow!("装配失败: {e}"))?;
    if let Some(d) = a.out.parent() {
        if !d.as_os_str().is_empty() {
            std::fs::create_dir_all(d)?;
        }
    }
    std::fs::write(&a.out, &glb)?;
    let tp = a.table.clone().unwrap_or_else(|| {
        let s = a.out.with_extension("");
        PathBuf::from(format!("{}_assembly.json", s.display()))
    });
    table["sdk"] = json!(sdk.display().to_string());
    std::fs::write(&tp, serde_json::to_string_pretty(&table)?)?;
    let n_mounts = table["mounts"].as_array().map(|x| x.len()).unwrap_or(0);
    let r = json!({"ok": true, "ship": ship_name, "glb": a.out.display().to_string(), "glb_bytes": glb.len(), "table": tp.display().to_string(),
        "sections": table["sections"], "mounts": n_mounts, "hardpoints_total": table["hardpoints_total"], "warnings": warn, "seconds": t0.elapsed().as_secs_f64()});
    if cli.json {
        emit(cli, &r);
    } else {
        println!("装配 {ship_name}: {} 个分段, {n_mounts} 个挂点模型 → {} ({} 字节)", table["sections"].as_object().map(|o| o.len()).unwrap_or(0), a.out.display(), glb.len());
        println!("装配表 → {}", tp.display());
        for w in &warn {
            println!("  ⚠ {w}");
        }
    }
    Ok(0)
}

// ───────────── 事务(原子写 + undo 备份) ─────────────

struct Tx<'p> {
    p: &'p Project,
    orig: BTreeMap<PathBuf, Option<Vec<u8>>>,
    pending: BTreeMap<PathBuf, Vec<u8>>,
}

impl<'p> Tx<'p> {
    fn new(p: &'p Project) -> Self {
        Tx { p, orig: BTreeMap::new(), pending: BTreeMap::new() }
    }
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        if let Some(d) = self.pending.get(path) {
            return Ok(d.clone());
        }
        std::fs::read(path).with_context(|| format!("读取 {}", path.display()))
    }
    fn read_text(&self, path: &Path) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.read(path)?).into_owned())
    }
    fn geometry(&self, path: &Path, src: &GeomSrc) -> Result<Vec<u8>> {
        if let Some(d) = self.pending.get(path) {
            return Ok(d.clone());
        }
        load_geometry(path, src)
    }
    fn write(&mut self, path: &Path, data: Vec<u8>) {
        if !self.orig.contains_key(path) {
            self.orig.insert(path.to_path_buf(), std::fs::read(path).ok());
        }
        self.pending.insert(path.to_path_buf(), data);
    }
    fn write_visual(&mut self, path: &Path, v: &Visual) {
        self.write(path, v.text.clone().into_bytes());
    }
    /// 落盘 + 备份 + 记录。返回记录文件路径
    fn commit(self, cmd: &str, args: &ModelCmd, mut rec: Value) -> Result<PathBuf> {
        let id = format!("{}_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0), std::process::id());
        let bdir = self.p.root.join("edits").join(".undo").join(&id);
        let mut files = Vec::new();
        for (path, data) in &self.pending {
            let rel = self.p.rel(path);
            let before = self.orig.get(path).cloned().flatten();
            if let Some(b) = &before {
                let bp = bdir.join(&rel);
                std::fs::create_dir_all(bp.parent().unwrap())?;
                std::fs::write(&bp, b)?;
            }
            files.push(json!({"path": rel, "existed": before.is_some(), "before_sha256": before.as_ref().map(|b| modproj::sha256_hex(b)), "after_sha256": modproj::sha256_hex(data), "bytes": data.len()}));
        }
        for (path, data) in &self.pending {
            if let Some(d) = path.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(path, data).with_context(|| format!("写入 {}", path.display()))?;
        }
        rec["args"] = serde_json::to_value(args)?;
        rec["undo"] = json!({"id": id, "backup_dir": self.p.rel(&bdir), "files": files});
        self.p.record_edit(cmd, rec)
    }
}

// ───────────── 目标解析 ─────────────

const PART_DIRS: [&str; 5] = ["gun", "director", "radar", "finder", "misc"];

struct PartFiles {
    stem: String,
    visuals: Vec<PathBuf>, // [主, lod1, lod2, ...]
    geometry: PathBuf,
}

fn lod_visuals(dir: &Path, stem: &str) -> Vec<PathBuf> {
    let mut v: Vec<(u32, PathBuf)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir.join("lods")) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if let Some(r) = n.strip_prefix(&format!("{stem}_lod")).and_then(|r| r.strip_suffix(".visual")) {
                if let Ok(k) = r.parse::<u32>() {
                    v.push((k, e.path()));
                }
            }
        }
    }
    v.sort();
    v.into_iter().map(|x| x.1).collect()
}

fn find_part(root: &Path, stem: &str) -> Result<PartFiles> {
    for d in PART_DIRS {
        let dir = root.join(d);
        let vp = dir.join(format!("{stem}.visual"));
        if vp.is_file() {
            let mut visuals = vec![vp];
            visuals.extend(lod_visuals(&dir, stem));
            return Ok(PartFiles { stem: stem.to_string(), visuals, geometry: dir.join(format!("{stem}.geometry")) });
        }
    }
    // 允许只给前缀(如 FGS036)
    let mut hits = Vec::new();
    for d in PART_DIRS {
        if let Ok(rd) = std::fs::read_dir(root.join(d)) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n.ends_with(".visual") && n.to_lowercase().starts_with(&stem.to_lowercase()) {
                    hits.push(n.trim_end_matches(".visual").to_string());
                }
            }
        }
    }
    match hits.as_slice() {
        [h] => find_part(root, &h.clone()),
        [] => bail!("没有部件 {stem}(在 {} 下找 {:?}/<名>.visual)", root.display(), PART_DIRS),
        _ => bail!("部件 {stem} 有多个匹配: {hits:?}"),
    }
}

struct SectionFiles {
    sec: String,
    visuals: Vec<PathBuf>,
    geometry: PathBuf,
}

fn sections(root: &Path) -> Result<(String, Vec<SectionFiles>)> {
    let (master, mv) = assemble::master_visual(root).map_err(|e| anyhow!("找主 visual: {e}"))?;
    let (secs, _) = assemble::hardpoints(root, &master, &mv).map_err(|e| anyhow!("{e}"))?;
    let ship = root.join("ship");
    let mut out = Vec::new();
    for (s, _) in secs {
        let stem = format!("{master}_{s}");
        let mut visuals = vec![ship.join(format!("{stem}.visual"))];
        visuals.extend(lod_visuals(&ship.join(""), &stem));
        out.push(SectionFiles { sec: s, visuals, geometry: ship.join(format!("{stem}.geometry")) });
    }
    Ok((master, out))
}

fn section_matrix(root: &Path, sec: &str) -> Result<M4> {
    let (master, mv) = assemble::master_visual(root).map_err(|e| anyhow!("{e}"))?;
    let (secs, _) = assemble::hardpoints(root, &master, &mv).map_err(|e| anyhow!("{e}"))?;
    secs.into_iter().find(|(s, _)| s.eq_ignore_ascii_case(sec)).map(|x| x.1).ok_or_else(|| anyhow!("没有分段 {sec}"))
}

/// 船体分段或部件: 一组 visual(LOD0..N) + 一个 geometry
struct Unit {
    kind: &'static str, // "section" | "part"
    name: String,
    visuals: Vec<PathBuf>,
    geometry: PathBuf,
}

fn units_for(root: &Path, t: &Target) -> Result<Vec<Unit>> {
    if let Some(p) = &t.part {
        let pf = find_part(root, p)?;
        return Ok(vec![Unit { kind: "part", name: pf.stem, visuals: pf.visuals, geometry: pf.geometry }]);
    }
    let (_, secs) = sections(root)?;
    let v: Vec<Unit> = secs
        .into_iter()
        .filter(|s| t.section.as_ref().map(|x| x.eq_ignore_ascii_case(&s.sec)).unwrap_or(true))
        .map(|s| Unit { kind: "section", name: s.sec, visuals: s.visuals, geometry: s.geometry })
        .collect();
    if v.is_empty() {
        bail!("没有分段 {:?}", t.section);
    }
    Ok(v)
}

fn load_visuals(tx: &Tx, paths: &[PathBuf]) -> Result<Vec<Visual>> {
    paths.iter().map(|vp| Visual::parse(&tx.read_text(vp)?).map_err(|e| anyhow!("{}: {e}", vp.display()))).collect()
}

/// renderSet 顶点所在空间 → visual 空间
fn rs_to_visual(v: &Visual, k: usize) -> M4 {
    let rs = &v.render_sets[k];
    if rs.world_space {
        IDENTITY
    } else {
        rs.nodes.first().and_then(|n| v.node(n)).map(|n| n.world).unwrap_or(IDENTITY)
    }
}

type Aabb = ([f64; 3], [f64; 3]);

fn aabb_union(a: Option<Aabb>, b: Aabb) -> Aabb {
    match a {
        None => b,
        Some((mn, mx)) => (std::array::from_fn(|i| mn[i].min(b.0[i])), std::array::from_fn(|i| mx[i].max(b.1[i]))),
    }
}

fn aabb_of_points(pts: impl Iterator<Item = [f64; 3]>) -> Option<Aabb> {
    let mut r: Option<Aabb> = None;
    for p in pts {
        r = Some(aabb_union(r, (p, p)));
    }
    r
}

fn aabb_xform(b: Aabb, m: &M4) -> Aabb {
    let corners = (0..8).map(|i| xform_point(m, [if i & 1 == 0 { b.0[0] } else { b.1[0] }, if i & 2 == 0 { b.0[1] } else { b.1[1] }, if i & 4 == 0 { b.0[2] } else { b.1[2] }]));
    aabb_of_points(corners).unwrap()
}

fn mesh_aabb(ge: &mut GeoEdit, name: &str, m: &M4) -> Result<Option<Aabb>> {
    let md = ge.mesh(name).map_err(|e| anyhow!("{e}"))?;
    Ok(aabb_of_points(md.positions.iter().map(|p| xform_point(m, p.map(|x| x as f64)))))
}

fn translate(d: [f64; 3]) -> M4 {
    compose(d, [0.0; 3], [1.0; 3])
}

fn inv(m: &M4) -> Result<M4> {
    inverse(m).ok_or_else(|| anyhow!("矩阵不可逆"))
}

fn norm_dir(d: [f64; 3]) -> [f32; 3] {
    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-12);
    [(d[0] / l) as f32, (d[1] / l) as f32, (d[2] / l) as f32]
}

fn select_families(v: &Visual, re: &Regex) -> BTreeSet<String> {
    v.render_sets.iter().filter(|r| re.is_match(&r.base_name()) || r.nodes.first().map(|n| re.is_match(n)).unwrap_or(false)).map(|r| r.family()).collect()
}

// ───────────── CLI 定义 ─────────────

#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    /// mod 项目目录(SDK 路线)
    #[arg(long)]
    pub project: PathBuf,
    /// 挂点(如 HP_FGM_3): 只改这一个挂点实例的矩阵(*_ports.visual)
    #[arg(long)]
    pub hp: Option<String>,
    /// 部件模型名(如 FGM049_380mm_45_Mle_1935, 可只写前缀 FGM049): 改该模型的全部实例
    #[arg(long)]
    pub part: Option<String>,
    /// 渲染集(正则, 匹配 renderSet 名或其节点名, 如 '^MidBack_Net$'); 同时作用于全部 LOD
    #[arg(long)]
    pub renderset: Option<String>,
    /// 限定船体分段(Bow/MidFront/MidBack/Stern)
    #[arg(long)]
    pub section: Option<String>,
    /// 额外的 .geometry 目录(项目里是 SDK 占位时先从这里取, 再从游戏包取)
    #[arg(long = "geom-dir")]
    #[serde(default)]
    pub geom_dir: Vec<PathBuf>,
}

#[derive(Subcommand, Debug, Clone, Serialize, Deserialize)]
pub enum ModelCmd {
    /// 列出可编辑目标: 分段与渲染集、部件、挂点
    List {
        #[command(flatten)]
        t: Target,
    },
    /// 缩放(一个数=等比; 或 x,y,z)。部件绕自身原点(挂点处), 渲染集默认绕自身包围盒中心, 挂点绕自身原点
    Scale {
        #[command(flatten)]
        t: Target,
        factor: String,
        /// 枢轴(目标空间坐标 x,y,z)
        #[arg(long, allow_hyphen_values = true)]
        pivot: Option<String>,
    },
    /// 平移 x,y,z(游戏坐标: x 右舷, y 上, z 船艏; 单位同 visual; 挂点/船体为分段空间)
    Move {
        #[command(flatten)]
        t: Target,
        #[arg(long, allow_hyphen_values = true)]
        by: String,
    },
    /// 旋转(度, 依次绕 x,y,z; 如 --deg 0,90,0 为偏航 90°)
    Rotate {
        #[command(flatten)]
        t: Target,
        #[arg(long, allow_hyphen_values = true)]
        deg: String,
        #[arg(long, allow_hyphen_values = true)]
        pivot: Option<String>,
    },
    /// 替换: 部件换成另一部件(--with, 本项目或 --with-sdk 别船 SDK 船目录), 或部件/渲染集换成 GLB 网格(--glb)
    Replace {
        #[command(flatten)]
        t: Target,
        #[arg(long)]
        with: Option<String>,
        #[arg(long)]
        with_sdk: Option<PathBuf>,
        #[arg(long)]
        glb: Option<PathBuf>,
        /// GLB 坐标: local(部件/分段自身空间, glTF 约定) 或 world(wowsmod assemble 输出的整船坐标; 仅船体)
        #[arg(long, default_value = "local")]
        space: String,
    },
    /// 删除: 渲染集从全部 LOD visual 中移除; 部件=全部渲染集换成退化三角形(隐藏); 挂点=缩到 0.1%(隐藏)
    Remove {
        #[command(flatten)]
        t: Target,
    },
    /// 新增网格: GLB 加成新渲染集(材质/顶点格式/挂接节点取自 --like), 默认写进全部 LOD
    Add {
        #[command(flatten)]
        t: Target,
        #[arg(long)]
        glb: PathBuf,
        /// 模板渲染集(正则, LOD0 visual 中第一个匹配)
        #[arg(long)]
        like: String,
        /// 新渲染集基名(默认 wm_<glb 名>)
        #[arg(long)]
        name: Option<String>,
        /// world(assemble 输出的整船 glTF 坐标; 仅船体) 或 local(分段/部件自身空间, glTF 约定)
        #[arg(long, default_value = "world")]
        space: String,
        /// 只写进 LOD0(默认全部 LOD, 避免远处消失)
        #[arg(long)]
        lod0_only: bool,
    },
    /// 等价重建: 解码全部渲染集后由编码器从零写出 geometry(验证编码器); 默认整船分段+全部部件
    Rebuild {
        #[command(flatten)]
        t: Target,
        /// 切线/副法线也由编码器重算(默认保留原始字节)
        #[arg(long)]
        recompute_tangents: bool,
    },
    /// 导出给 Blender: 每个渲染集一个对象(名=渲染集, 材质=identifier), 挂点为空物体, 整船 glTF 坐标
    ExportBlender {
        #[command(flatten)]
        t: Target,
        #[arg(short, long)]
        out: PathBuf,
        /// 船体渲染集: intact / all
        #[arg(long, default_value = "intact")]
        hullsets: String,
    },
    /// 从 Blender 导回: 同名对象写回对应渲染集(有变化才写), 新对象按材质名找模板渲染集新增
    ImportBlender {
        #[command(flatten)]
        t: Target,
        glb: PathBuf,
    },
    /// 撤销最近一次 model 编辑(文件哈希与编辑后一致才还原)
    Undo {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        force: bool,
    },
    /// 在本项目上按顺序重放另一个项目(或其 edits 目录)里的 model 编辑记录
    Replay {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        from: PathBuf,
    },
}

impl ModelCmd {
    fn target_mut(&mut self) -> Option<&mut Target> {
        match self {
            ModelCmd::List { t }
            | ModelCmd::Scale { t, .. }
            | ModelCmd::Move { t, .. }
            | ModelCmd::Rotate { t, .. }
            | ModelCmd::Replace { t, .. }
            | ModelCmd::Remove { t }
            | ModelCmd::Add { t, .. }
            | ModelCmd::Rebuild { t, .. }
            | ModelCmd::ExportBlender { t, .. }
            | ModelCmd::ImportBlender { t, .. } => Some(t),
            _ => None,
        }
    }
    fn name(&self) -> &'static str {
        match self {
            ModelCmd::List { .. } => "model_list",
            ModelCmd::Scale { .. } => "model_scale",
            ModelCmd::Move { .. } => "model_move",
            ModelCmd::Rotate { .. } => "model_rotate",
            ModelCmd::Replace { .. } => "model_replace",
            ModelCmd::Remove { .. } => "model_remove",
            ModelCmd::Add { .. } => "model_add",
            ModelCmd::Rebuild { .. } => "model_rebuild",
            ModelCmd::ExportBlender { .. } => "model_export_blender",
            ModelCmd::ImportBlender { .. } => "model_import_blender",
            ModelCmd::Undo { .. } => "model_undo",
            ModelCmd::Replay { .. } => "model_replay",
        }
    }
    /// 记录里的路径一律转绝对路径, 便于重放
    fn absolutized(&self) -> ModelCmd {
        let abs = |p: &PathBuf| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
        let mut c = self.clone();
        if let Some(t) = c.target_mut() {
            t.geom_dir = t.geom_dir.iter().map(abs).collect();
            t.project = abs(&t.project);
        }
        match &mut c {
            ModelCmd::Replace { with_sdk, glb, .. } => {
                *with_sdk = with_sdk.as_ref().map(abs);
                *glb = glb.as_ref().map(abs);
            }
            ModelCmd::Add { glb, .. } | ModelCmd::ImportBlender { glb, .. } => *glb = abs(glb),
            ModelCmd::ExportBlender { out, .. } => {
                if let Some(parent) = out.parent() {
                    if let Ok(pp) = std::fs::canonicalize(if parent.as_os_str().is_empty() { Path::new(".") } else { parent }) {
                        *out = pp.join(out.file_name().unwrap_or_default());
                    }
                }
            }
            _ => {}
        }
        c
    }
}

pub fn cmd_model(cli: &Cli, c: &ModelCmd) -> Result<i32> {
    let r = run_model(cli, c)?;
    if cli.json {
        emit(cli, &r);
    } else {
        println!("{}", serde_json::to_string_pretty(&r)?);
    }
    Ok(0)
}

fn run_model(cli: &Cli, c: &ModelCmd) -> Result<Value> {
    let mut c = c.absolutized();
    match &c {
        ModelCmd::Undo { project, force } => return undo(&Project::open(project)?, *force),
        ModelCmd::Replay { project, from } => return replay(cli, project, from),
        _ => {}
    }
    let t = c.target_mut().unwrap().clone();
    let p = Project::open(&t.project)?;
    let root = project_ship_dir(&p)?;
    let src = GeomSrc::new(cli, &t.geom_dir)?;
    let mut tx = Tx::new(&p);
    let n_targets = [t.hp.is_some(), t.part.is_some(), t.renderset.is_some()].iter().filter(|x| **x).count();
    let needs_target = matches!(c, ModelCmd::Scale { .. } | ModelCmd::Move { .. } | ModelCmd::Rotate { .. } | ModelCmd::Replace { .. } | ModelCmd::Remove { .. });
    if needs_target && n_targets != 1 {
        bail!("需要且只能给一个目标: --hp / --part / --renderset");
    }
    let mut rec = match &c {
        ModelCmd::List { .. } => return list(&root),
        ModelCmd::ExportBlender { out, hullsets, .. } => return export_blender(&root, &src, &t, out, hullsets),
        ModelCmd::Scale { factor, pivot, .. } => {
            let s = parse_vec3(factor)?;
            if s.iter().any(|x| x.abs() < 1e-6) {
                bail!("缩放系数不能为 0(要隐藏请用 remove)");
            }
            transform(&mut tx, &root, &src, &t, &compose([0.0; 3], [0.0; 3], s), pivot.as_deref().map(parse_vec3).transpose()?, false)?
        }
        ModelCmd::Rotate { deg, pivot, .. } => transform(&mut tx, &root, &src, &t, &compose([0.0; 3], parse_vec3(deg)?, [1.0; 3]), pivot.as_deref().map(parse_vec3).transpose()?, false)?,
        ModelCmd::Move { by, .. } => transform(&mut tx, &root, &src, &t, &translate(parse_vec3(by)?), None, true)?,
        ModelCmd::Remove { .. } => remove(&mut tx, &root, &src, &t)?,
        ModelCmd::Replace { with, with_sdk, glb, space, .. } => replace(&mut tx, &p, &root, &src, &t, with.as_deref(), with_sdk.as_deref(), glb.as_deref(), space)?,
        ModelCmd::Add { glb, like, name, space, lod0_only, .. } => add(&mut tx, &root, &src, &t, glb, like, name.as_deref(), space, *lod0_only)?,
        ModelCmd::Rebuild { recompute_tangents, .. } => rebuild(&mut tx, &root, &src, &t, *recompute_tangents)?,
        ModelCmd::ImportBlender { glb, .. } => import_blender(&mut tx, &root, &src, glb)?,
        ModelCmd::Undo { .. } | ModelCmd::Replay { .. } => unreachable!(),
    };
    let changed: Vec<String> = tx.pending.keys().map(|k| p.rel(k)).collect();
    rec["changed_files"] = json!(changed);
    rec["ok"] = json!(true);
    if tx.pending.is_empty() {
        rec["note"] = json!("没有文件需要改动");
        return Ok(rec);
    }
    let name = c.name();
    let path = tx.commit(name, &c, rec.clone())?;
    rec["edit_record"] = json!(path.display().to_string());
    Ok(rec)
}

// ───────────── list ─────────────

fn list(root: &Path) -> Result<Value> {
    let (master, secs) = sections(root)?;
    let mut sj = Vec::new();
    for s in &secs {
        let v = Visual::parse(&std::fs::read_to_string(&s.visuals[0])?).map_err(|e| anyhow!("{e}"))?;
        sj.push(json!({"section": s.sec, "lod_visuals": s.visuals.len(),
            "renderSets": v.render_sets.iter().map(|r| json!({"name": r.base_name(), "node": r.nodes.first(), "material": r.material_identifier, "worldSpace": r.world_space})).collect::<Vec<_>>()}));
    }
    let mut parts = Vec::new();
    for d in PART_DIRS {
        if let Ok(rd) = std::fs::read_dir(root.join(d)) {
            let mut v: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".visual")).collect();
            v.sort();
            for n in v {
                let stem = n.trim_end_matches(".visual").to_string();
                parts.push(json!({"part": stem, "dir": d, "lods": lod_visuals(&root.join(d), &stem).len()}));
            }
        }
    }
    let (m2, mv) = assemble::master_visual(root).map_err(|e| anyhow!("{e}"))?;
    let (_, hps) = assemble::hardpoints(root, &m2, &mv).map_err(|e| anyhow!("{e}"))?;
    let hp: Vec<Value> = hps.iter().map(|(k, (s, w))| json!({"hp": k, "section": s, "pos": [w[3][0], w[3][1], w[3][2]]})).collect();
    Ok(json!({"ok": true, "hull": master, "sections": sj, "parts": parts, "hardpoints": hp}))
}

// ───────────── scale / move / rotate ─────────────

/// 部件整体变换后节点的新世界矩阵: 位置按 T 变换, 姿态只乘 T 的旋转部分(缩放不进骨骼矩阵)
fn node_world_after(w: &M4, t: &M4) -> M4 {
    let mut q = IDENTITY;
    for i in 0..3 {
        let n = (t[i][0] * t[i][0] + t[i][1] * t[i][1] + t[i][2] * t[i][2]).sqrt().max(1e-12);
        for j in 0..3 {
            q[i][j] = t[i][j] / n;
        }
    }
    let mut r = mat_mul(w, &q);
    let p = xform_point(t, [w[3][0], w[3][1], w[3][2]]);
    r[3] = [p[0], p[1], p[2], 1.0];
    for row in r.iter_mut().take(3) {
        row[3] = 0.0;
    }
    r
}

fn find_hp(tx: &Tx, root: &Path, hp: &str) -> Result<(PathBuf, Visual)> {
    let ship = root.join("ship");
    let mut cands: Vec<PathBuf> = std::fs::read_dir(&ship)?.flatten().map(|e| e.path()).filter(|p| p.extension().map(|x| x == "visual").unwrap_or(false)).collect();
    cands.sort();
    cands.sort_by_key(|p| !p.to_string_lossy().contains("_ports"));
    for p in cands {
        let v = Visual::parse(&tx.read_text(&p)?).map_err(|e| anyhow!("{}: {e}", p.display()))?;
        if v.node(hp).is_some() {
            return Ok((p, v));
        }
    }
    bail!("ship/*.visual 里没有挂点节点 {hp}")
}

fn target_json(t: &Target) -> Value {
    json!({"hp": t.hp, "part": t.part, "renderset": t.renderset, "section": t.section})
}

fn transform(tx: &mut Tx, root: &Path, src: &GeomSrc, t: &Target, op: &M4, pivot: Option<[f64; 3]>, is_move: bool) -> Result<Value> {
    if let Some(hp) = &t.hp {
        let (path, mut v) = find_hp(tx, root, hp)?;
        let n = v.node(hp).unwrap().clone();
        let pw = n.parent.map(|i| v.nodes[i].world).unwrap_or(IDENTITY);
        let new_local = if is_move {
            // 平移在分段空间: W' = W·T, L' = W'·inv(W_parent)
            mat_mul(&mat_mul(&n.world, op), &inv(&pw)?)
        } else {
            let pv = pivot.unwrap_or([0.0; 3]);
            mat_mul(&mat_mul(&mat_mul(&translate(pv.map(|x| -x)), op), &translate(pv)), &n.local)
        };
        v.set_node_local(hp, &new_local).map_err(|e| anyhow!("{e}"))?;
        tx.write_visual(&path, &v);
        return Ok(json!({"target": target_json(t), "visual": path.file_name().unwrap().to_string_lossy(), "old_local": m4_json(&n.local), "new_local": m4_json(&new_local),
            "note": "只改了这个挂点的矩阵; 引擎是否采用挂点矩阵里的缩放以港口实测为准"}));
    }
    let units = units_for(root, t)?;
    let re = t.renderset.as_ref().map(|r| Regex::new(r)).transpose().map_err(|e| anyhow!("--renderset 正则: {e}"))?;
    let mut out = Vec::new();
    for u in units {
        let mut vis = load_visuals(tx, &u.visuals)?;
        let fams: BTreeSet<String> = match &re {
            Some(re) => select_families(&vis[0], re),
            None => vis.iter().flat_map(|v| v.render_sets.iter().map(|r| r.family())).collect(),
        };
        if fams.is_empty() {
            continue;
        }
        let mut ge = GeoEdit::parse(&tx.geometry(&u.geometry, src)?).map_err(|e| anyhow!("{}: {e}", u.geometry.display()))?;
        let whole_part = u.kind == "part" && re.is_none();
        // 枢轴: 部件=原点(挂点处); 渲染集=LOD0 选中渲染集的包围盒中心
        let pv = match pivot {
            Some(p) => p,
            None if whole_part || is_move => [0.0; 3],
            None => {
                let mut bb = None;
                for k in 0..vis[0].render_sets.len() {
                    if fams.contains(&vis[0].render_sets[k].family()) {
                        let m = rs_to_visual(&vis[0], k);
                        if let Some(b) = mesh_aabb(&mut ge, &vis[0].render_sets[k].base_name(), &m)? {
                            bb = Some(aabb_union(bb, b));
                        }
                    }
                }
                let b = bb.ok_or_else(|| anyhow!("选中的渲染集没有顶点"))?;
                std::array::from_fn(|i| (b.0[i] + b.1[i]) / 2.0)
            }
        };
        let tm = mat_mul(&mat_mul(&translate(pv.map(|x| -x)), op), &translate(pv));
        let mut done: BTreeMap<String, M4> = BTreeMap::new();
        let mut warn = Vec::new();
        for v in vis.iter_mut() {
            let new_world: Vec<M4> = if whole_part { v.nodes.iter().map(|n| node_world_after(&n.world, &tm)).collect() } else { v.nodes.iter().map(|n| n.world).collect() };
            let mut touched: Vec<usize> = Vec::new();
            for k in 0..v.render_sets.len() {
                let rs = v.render_sets[k].clone();
                if !fams.contains(&rs.family()) {
                    continue;
                }
                touched.push(k);
                let name = rs.base_name();
                // 顶点所在空间里的等效变换 M = W(n)·T·inv(W'(n))
                let m = if rs.world_space {
                    tm
                } else {
                    match rs.nodes.first().and_then(|n| v.nodes.iter().position(|x| &x.name == n)) {
                        Some(i) => mat_mul(&mat_mul(&v.nodes[i].world, &tm), &inv(&new_world[i])?),
                        None => tm,
                    }
                };
                if let Some(prev) = done.get(&name) {
                    if (0..4).any(|i| (0..4).any(|j| (prev[i][j] - m[i][j]).abs() > 1e-6)) {
                        warn.push(format!("{name}: 不同 visual 中的节点矩阵不同, 按第一次的变换"));
                    }
                    continue;
                }
                ge.transform(&name, &m).map_err(|e| anyhow!("{name}: {e}"))?;
                done.insert(name, m);
            }
            if whole_part {
                for i in 0..v.nodes.len() {
                    let n = v.nodes[i].clone();
                    if n.transform_span.is_none() {
                        continue;
                    }
                    let pw = n.parent.map(|p| new_world[p]).unwrap_or(IDENTITY);
                    let l = mat_mul(&new_world[i], &inv(&pw)?);
                    v.set_node_local(&n.name, &l).map_err(|e| anyhow!("{e}"))?;
                }
            }
            if let Some(b) = v.bbox {
                let nb = if whole_part {
                    aabb_xform(b, &tm)
                } else {
                    let mut acc = Some(b);
                    for k in touched {
                        let m = rs_to_visual(v, k);
                        if let Some(x) = mesh_aabb(&mut ge, &v.render_sets[k].base_name(), &m)? {
                            acc = Some(aabb_union(acc, x));
                        }
                    }
                    acc.unwrap()
                };
                v.set_bbox(nb.0, nb.1).map_err(|e| anyhow!("{e}"))?;
            }
        }
        let geo = ge.finish().map_err(|e| anyhow!("{e}"))?;
        tx.write(&u.geometry, geo.serialize());
        for (vp, v) in u.visuals.iter().zip(&vis) {
            tx.write_visual(vp, v);
        }
        out.push(json!({"unit": u.kind, "name": u.name, "renderSets_transformed": done.keys().collect::<Vec<_>>(), "families": fams, "pivot": pv, "matrix": m4_json(&tm), "nodes_updated": whole_part, "visuals": u.visuals.len(), "warnings": warn}));
    }
    if out.is_empty() {
        bail!("没有匹配的渲染集");
    }
    Ok(json!({"target": target_json(t), "units": out, "lod_note": "同一 geometry 里的 LOD1-3 渲染集一并变换; 船体 lod4(整船合并 Full_lod4, 极远处)未改"}))
}

// ───────────── remove ─────────────

fn remove(tx: &mut Tx, root: &Path, src: &GeomSrc, t: &Target) -> Result<Value> {
    if t.hp.is_some() {
        let mut r = transform(tx, root, src, t, &compose([0.0; 3], [0.0; 3], [0.001; 3]), None, false)?;
        r["mode"] = json!("hide_hp_scale_0.001");
        return Ok(r);
    }
    let units = units_for(root, t)?;
    let re = t.renderset.as_ref().map(|r| Regex::new(r)).transpose().map_err(|e| anyhow!("--renderset 正则: {e}"))?;
    let mut out = Vec::new();
    for u in units {
        let mut vis = load_visuals(tx, &u.visuals)?;
        if let Some(re) = &re {
            let fams = select_families(&vis[0], re);
            if fams.is_empty() {
                continue;
            }
            let mut removed = Vec::new();
            for v in vis.iter_mut() {
                for k in (0..v.render_sets.len()).rev() {
                    if fams.contains(&v.render_sets[k].family()) {
                        removed.push(v.render_sets[k].base_name());
                        v.remove_render_set(k).map_err(|e| anyhow!("{e}"))?;
                    }
                }
                if v.render_sets.is_empty() {
                    bail!("删除后某个 visual 将没有任何渲染集(可能导致客户端异常); 整个部件请用 remove --part(隐藏)");
                }
            }
            for (vp, v) in u.visuals.iter().zip(&vis) {
                tx.write_visual(vp, v);
            }
            out.push(json!({"unit": u.kind, "name": u.name, "mode": "visual_remove", "families": fams, "renderSets_removed": removed, "geometry": "未改(数据保留, 不再被引用)"}));
        } else {
            // 部件: 每个渲染集换成退化三角形(visual 结构不变, 最稳妥)
            let mut ge = GeoEdit::parse(&tx.geometry(&u.geometry, src)?).map_err(|e| anyhow!("{e}"))?;
            let names: BTreeSet<String> = vis.iter().flat_map(|v| v.render_sets.iter().map(|r| r.base_name())).collect();
            for n in &names {
                let tri = MeshData { positions: vec![[0.0; 3]; 3], normals: Some(vec![[0.0, 1.0, 0.0]; 3]), uvs: Some(vec![[0.0, 0.0]; 3]), indices: vec![0, 1, 2] };
                ge.replace_mesh(n, &tri, None).map_err(|e| anyhow!("{n}: {e}"))?;
            }
            let geo = ge.finish().map_err(|e| anyhow!("{e}"))?;
            tx.write(&u.geometry, geo.serialize());
            out.push(json!({"unit": u.kind, "name": u.name, "mode": "degenerate_hide", "renderSets": names}));
        }
    }
    if out.is_empty() {
        bail!("没有匹配的渲染集");
    }
    Ok(json!({"target": target_json(t), "units": out}))
}

// ───────────── GLB → 网格 ─────────────

struct GlbPrim {
    node: String,
    material: Option<String>,
    /// 目标 visual 空间的游戏坐标(已从 glTF 转换, 绕序已翻转)
    mesh: MeshData,
}

/// glTF 点(某节点局部) → 游戏坐标: 先用 glTF 矩阵 `m`, 再 C 翻转 z, 再乘游戏行向量矩阵 `g`
fn gltf_to_game_pt(m: &M4, g: &M4, q: [f32; 3]) -> [f64; 3] {
    let w = scene::xform_gltf_point(m, q);
    xform_point(g, [w[0] as f64, w[1] as f64, -(w[2] as f64)])
}

fn convert_prim(m: &M4, g: &M4, p: &ScenePrim) -> MeshData {
    let positions: Vec<[f32; 3]> = p.positions.iter().map(|q| gltf_to_game_pt(m, g, *q).map(|x| x as f32)).collect();
    let normals = p.normals.as_ref().map(|ns| {
        ns.iter()
            .zip(&p.positions)
            .map(|(nv, q)| {
                let a = gltf_to_game_pt(m, g, *q);
                let b = gltf_to_game_pt(m, g, [q[0] + nv[0] * 0.01, q[1] + nv[1] * 0.01, q[2] + nv[2] * 0.01]);
                norm_dir([b[0] - a[0], b[1] - a[1], b[2] - a[2]])
            })
            .collect::<Vec<_>>()
    });
    // C 翻转 z 是镜像 → 绕序翻转
    let indices: Vec<u32> = p.indices.chunks_exact(3).flat_map(|t| [t[0], t[2], t[1]]).collect();
    MeshData { positions, normals, uvs: p.uvs.clone(), indices }
}

/// 读 GLB 的全部 primitive, 用各节点的世界矩阵, 再乘 `g`(游戏整船/分段空间 → 目标 visual 空间)
fn glb_prims(path: &Path, g: &M4) -> Result<Vec<GlbPrim>> {
    let d = std::fs::read(path).with_context(|| format!("读取 {}", path.display()))?;
    let sc = scene::read_scene(&d).map_err(|e| anyhow!("{}: {e}", path.display()))?;
    let mut out = Vec::new();
    for n in &sc.nodes {
        let Some(mi) = n.mesh else { continue };
        for p in &sc.meshes[mi].prims {
            out.push(GlbPrim { node: n.name.clone(), material: p.material.clone(), mesh: convert_prim(&n.world, g, p) });
        }
    }
    if out.is_empty() {
        bail!("{} 里没有网格", path.display());
    }
    Ok(out)
}

fn sanitize(s: &str) -> String {
    let r: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if r.is_empty() {
        "mesh".into()
    } else {
        r
    }
}

fn find_template(v: &Visual, pat: &str) -> Result<usize> {
    let re = Regex::new(pat).map_err(|e| anyhow!("--like 正则: {e}"))?;
    v.render_sets
        .iter()
        .position(|r| re.is_match(&r.base_name()) || r.nodes.first().map(|n| re.is_match(n)).unwrap_or(false))
        .ok_or_else(|| anyhow!("LOD0 visual 里没有匹配 {pat:?} 的渲染集"))
}

/// 把 GLB primitive 作为新渲染集写进 unit(全部 LOD 或仅 LOD0)。prim 坐标在 visual 空间。
/// 材质: GLB 材质名等于本 visual 某渲染集的 identifier 时用那个渲染集做模板, 否则用 `tpl_k`。
fn add_prims(ge: &mut GeoEdit, vis: &mut [Visual], prims: &[GlbPrim], base: &str, tpl_k: usize, lod0_only: bool) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    for (i, gp) in prims.iter().enumerate() {
        let name = if prims.len() == 1 { base.to_string() } else { format!("{base}_{i}") };
        let k = gp.material.as_ref().and_then(|m| vis[0].render_sets.iter().position(|r| &r.material_identifier == m)).unwrap_or(tpl_k);
        let r = vis[0].render_sets[k].clone();
        let rn = r.base_name();
        let fmt = ge.info(&rn).map_err(|e| anyhow!("{rn}: {e}"))?.format;
        let tv = ge.raw_vertex(&rn, 0).map_err(|e| anyhow!("{e}"))?;
        let to_vs = rs_to_visual(&vis[0], k);
        let space = inv(&to_vs)?;
        let mut m = gp.mesh.clone();
        for p in m.positions.iter_mut() {
            *p = xform_point(&space, p.map(|x| x as f64)).map(|x| x as f32);
        }
        if let Some(ns) = m.normals.as_mut() {
            for n in ns.iter_mut() {
                *n = norm_dir(xform_dir(&space, n.map(|x| x as f64)));
            }
        }
        let info = ge.add_mesh(&name, &fmt, &m, Some(&tv)).map_err(|e| anyhow!("{name}: {e}"))?;
        let nrs = NewRenderSet { nodes: r.nodes.clone(), name: name.clone(), material_identifier: r.material_identifier.clone(), mfm: r.mfm.clone(), world_space: r.world_space };
        let n_vis = if lod0_only { 1 } else { vis.len() };
        let bb = aabb_of_points(gp.mesh.positions.iter().map(|p| p.map(|x| x as f64)));
        for v in vis.iter_mut().take(n_vis) {
            v.add_render_set(&nrs).map_err(|e| anyhow!("{e}"))?;
            if let (Some(b), Some(x)) = (v.bbox, bb) {
                let u = aabb_union(Some(b), x);
                v.set_bbox(u.0, u.1).map_err(|e| anyhow!("{e}"))?;
            }
        }
        out.push(json!({"renderSet": name, "from_node": gp.node, "glb_material": gp.material, "template": rn, "material": r.material_identifier, "mfm": r.mfm, "format": fmt,
            "vertices": info.vertex_count, "indices": info.index_count, "texel_density": info.texel_density, "nodes": r.nodes, "worldSpace": r.world_space, "lod_visuals": n_vis}));
    }
    Ok(out)
}

/// GLB 坐标 → 目标 visual 空间(游戏行向量矩阵, 作用在 C 翻转后的点上)
fn glb_to_unit_matrix(root: &Path, u: &Unit, space: &str) -> Result<M4> {
    match space {
        "local" => Ok(IDENTITY),
        "world" if u.kind == "section" => inv(&section_matrix(root, &u.name)?),
        "world" => bail!("部件没有唯一的整船位置, 部件请用 --space local(部件自身坐标, glTF 约定)"),
        s => bail!("--space 只能是 local 或 world: {s}"),
    }
}

// ───────────── add ─────────────

#[allow(clippy::too_many_arguments)]
fn add(tx: &mut Tx, root: &Path, src: &GeomSrc, t: &Target, glb: &Path, like: &str, name: Option<&str>, space: &str, lod0_only: bool) -> Result<Value> {
    if t.part.is_none() && t.section.is_none() {
        bail!("add 需要 --section <分段> 或 --part <部件>");
    }
    let u = units_for(root, t)?.into_iter().next().ok_or_else(|| anyhow!("没有目标"))?;
    let g = glb_to_unit_matrix(root, &u, space)?;
    let prims = glb_prims(glb, &g)?;
    let mut ge = GeoEdit::parse(&tx.geometry(&u.geometry, src)?).map_err(|e| anyhow!("{e}"))?;
    let mut vis = load_visuals(tx, &u.visuals)?;
    let tk = find_template(&vis[0], like)?;
    let tname = vis[0].render_sets[tk].base_name();
    let base = name.map(sanitize).unwrap_or_else(|| format!("wm_{}", sanitize(&glb.file_stem().unwrap().to_string_lossy())));
    let added = add_prims(&mut ge, &mut vis, &prims, &base, tk, lod0_only)?;
    let geo = ge.finish().map_err(|e| anyhow!("{e}"))?;
    tx.write(&u.geometry, geo.serialize());
    for (vp, v) in u.visuals.iter().zip(&vis) {
        tx.write_visual(vp, v);
    }
    Ok(json!({"unit": u.kind, "name": u.name, "template": tname, "space": space, "glb": glb.display().to_string(), "added": added,
        "lod_note": if lod0_only { "只加到 LOD0, 远处会消失" } else { "加到 LOD0-3 全部 visual(远处不消失); 船体 lod4 整船合并模型未改" }}))
}

// ───────────── replace ─────────────

/// 把 text 里引用的 `PnFMods/<m>/<s>/<rel>` 文件从 src_root 复制到 dst_root(不存在时), 前缀改成 dst_prefix
fn copy_refs(tx: &mut Tx, text: &str, src_root: &Path, dst_root: &Path, dst_prefix: &str, depth: usize) -> Result<(String, Vec<String>)> {
    let re = Regex::new(r"PnFMods/[^/\s<]+/[^/\s<]+/([^\s<]+)").unwrap();
    let mut copied = Vec::new();
    let refs: BTreeSet<String> = re.captures_iter(text).map(|c| c[1].to_string()).collect();
    for rel in &refs {
        let s = src_root.join(rel);
        let d = dst_root.join(rel);
        if s.is_file() && !d.exists() && !tx.pending.contains_key(&d) {
            let data = std::fs::read(&s)?;
            let data = if depth < 3 && rel.ends_with(".mfm") {
                let (t2, c2) = copy_refs(tx, &String::from_utf8_lossy(&data), src_root, dst_root, dst_prefix, depth + 1)?;
                copied.extend(c2);
                t2.into_bytes()
            } else {
                data
            };
            tx.write(&d, data);
            copied.push(rel.clone());
        }
    }
    let out = re.replace_all(text, |c: &regex::Captures| format!("{dst_prefix}{}", &c[1])).into_owned();
    Ok((out, copied))
}

#[allow(clippy::too_many_arguments)]
fn replace(tx: &mut Tx, p: &Project, root: &Path, src: &GeomSrc, t: &Target, with: Option<&str>, with_sdk: Option<&Path>, glb: Option<&Path>, space: &str) -> Result<Value> {
    if t.hp.is_some() {
        bail!("挂点装什么型号由 GameParams(服务器数据)决定, 客户端不能按挂点替换; 请用 --part 替换该型号模型(影响该型号全部实例)");
    }
    if let Some(glb) = glb {
        let re = t.renderset.as_ref().map(|r| Regex::new(r)).transpose().map_err(|e| anyhow!("{e}"))?;
        let mut out = Vec::new();
        for u in units_for(root, t)? {
            let mut vis = load_visuals(tx, &u.visuals)?;
            let sel: Vec<usize> = (0..vis[0].render_sets.len())
                .filter(|&k| re.as_ref().map(|re| re.is_match(&vis[0].render_sets[k].base_name()) || vis[0].render_sets[k].nodes.first().map(|n| re.is_match(n)).unwrap_or(false)).unwrap_or(true))
                .collect();
            if sel.is_empty() {
                continue;
            }
            let fams: BTreeSet<String> = sel.iter().map(|&k| vis[0].render_sets[k].family()).collect();
            let g = glb_to_unit_matrix(root, &u, space)?;
            let prims = glb_prims(glb, &g)?;
            let mut ge = GeoEdit::parse(&tx.geometry(&u.geometry, src)?).map_err(|e| anyhow!("{e}"))?;
            let base = format!("wm_{}", sanitize(&glb.file_stem().unwrap().to_string_lossy()));
            // 先加新网格(模板仍在 visual 里), 再删旧渲染集家族
            let added = add_prims(&mut ge, &mut vis, &prims, &base, sel[0], false)?;
            let mut removed = Vec::new();
            for v in vis.iter_mut() {
                for k in (0..v.render_sets.len()).rev() {
                    if fams.contains(&v.render_sets[k].family()) {
                        removed.push(v.render_sets[k].base_name());
                        v.remove_render_set(k).map_err(|e| anyhow!("{e}"))?;
                    }
                }
            }
            let geo = ge.finish().map_err(|e| anyhow!("{e}"))?;
            tx.write(&u.geometry, geo.serialize());
            for (vp, v) in u.visuals.iter().zip(&vis) {
                tx.write_visual(vp, v);
            }
            out.push(json!({"unit": u.kind, "name": u.name, "removed": removed, "added": added}));
        }
        if out.is_empty() {
            bail!("没有匹配的渲染集");
        }
        return Ok(json!({"target": target_json(t), "glb": glb.display().to_string(), "space": space, "units": out,
            "note": "蒙皮字段取自模板渲染集第 0 个顶点(新网格整体跟随该顶点绑定的骨骼)"}));
    }
    let with = with.ok_or_else(|| anyhow!("replace 需要 --with <部件> 或 --glb <文件>"))?;
    let tp = t.part.as_ref().ok_or_else(|| anyhow!("--with 只能配合 --part"))?;
    let dst = find_part(root, tp)?;
    let (src_root, other) = match with_sdk {
        Some(d) => (d.to_path_buf(), true),
        None => (root.to_path_buf(), false),
    };
    let srcp = find_part(&src_root, with)?;
    if srcp.stem == dst.stem && !other {
        bail!("--with 与目标相同");
    }
    tx.write(&dst.geometry, load_geometry(&srcp.geometry, src)?);
    let dst_prefix = format!("PnFMods/{}/{}/", p.cfg.meta.name, p.cfg.meta.ship);
    let mut copied = Vec::new();
    let mut pairs = Vec::new();
    for (i, dv) in dst.visuals.iter().enumerate() {
        let sv = &srcp.visuals[i.min(srcp.visuals.len() - 1)];
        let mut text = std::fs::read_to_string(sv)?;
        let sroot = format!("{}.visual", sv.file_stem().unwrap().to_string_lossy());
        let droot = format!("{}.visual", dv.file_stem().unwrap().to_string_lossy());
        text = text.replace(&format!("<{sroot}>"), &format!("<{droot}>")).replace(&format!("</{sroot}>"), &format!("</{droot}>"));
        if other {
            let (t2, c) = copy_refs(tx, &text, &src_root, root, &dst_prefix, 0)?;
            text = t2;
            copied.extend(c);
        }
        Visual::parse(&text).map_err(|e| anyhow!("{}: {e}", sv.display()))?;
        tx.write(dv, text.into_bytes());
        pairs.push(json!([p.rel(dv), sv.display().to_string()]));
    }
    Ok(json!({"target": target_json(t), "with": srcp.stem, "with_sdk": with_sdk.map(|x| x.display().to_string()), "geometry": p.rel(&dst.geometry), "visuals": pairs, "copied_refs": copied,
        "note": "目标 .model 与 LOD 链不变, 内容换成来源部件(来源 LOD 少时用其最后一级); 炮管数/HP_gunFire 与 GameParams 不一致时的效果需实测"}))
}

// ───────────── rebuild ─────────────

fn all_units(root: &Path) -> Result<Vec<Unit>> {
    let (_, secs) = sections(root)?;
    let mut units: Vec<Unit> = secs.into_iter().map(|s| Unit { kind: "section", name: s.sec, visuals: s.visuals, geometry: s.geometry }).collect();
    for d in PART_DIRS {
        if let Ok(rd) = std::fs::read_dir(root.join(d)) {
            let mut v: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".visual")).collect();
            v.sort();
            for n in v {
                let pf = find_part(root, n.trim_end_matches(".visual"))?;
                units.push(Unit { kind: "part", name: pf.stem, visuals: pf.visuals, geometry: pf.geometry });
            }
        }
    }
    Ok(units)
}

fn rebuild(tx: &mut Tx, root: &Path, src: &GeomSrc, t: &Target, recompute_tangents: bool) -> Result<Value> {
    let units = if t.part.is_some() || t.section.is_some() { units_for(root, t)? } else { all_units(root)? };
    let mut out = Vec::new();
    for u in units {
        let d = match tx.geometry(&u.geometry, src) {
            Ok(d) => d,
            Err(e) => {
                out.push(json!({"unit": u.kind, "name": u.name, "skipped": format!("{e:#}")}));
                continue;
            }
        };
        let g = Geometry::parse(&d).map_err(|e| anyhow!("{e}"))?;
        let mut names: Vec<String> = Vec::new();
        for v in load_visuals(tx, &u.visuals)? {
            for r in &v.render_sets {
                let n = r.base_name();
                if !names.contains(&n) {
                    names.push(n);
                }
            }
        }
        let (ng, warn) = wowsmod_formats::geomedit::rebuild_from_meshes_opts(&g, &names, !recompute_tangents).map_err(|e| anyhow!("{}: {e}", u.name))?;
        let bytes = ng.serialize();
        // 自检: 重新解析写出的字节, 与原 geometry 逐映射比较
        let back = Geometry::parse(&bytes).map_err(|e| anyhow!("{}: 重建结果无法解析: {e}", u.name))?;
        let cmp = wowsmod_formats::geomedit::compare_geometry(&g, &back).map_err(|e| anyhow!("{e}"))?;
        out.push(json!({"unit": u.kind, "name": u.name, "renderSets": names.len(), "bytes_before": d.len(), "bytes_after": bytes.len(), "byte_identical": bytes == d,
            "equivalent": cmp.equivalent(), "compare": cmp, "warnings": warn}));
        tx.write(&u.geometry, bytes);
    }
    Ok(json!({"mode": if recompute_tangents { "rebuild_recompute_tangents" } else { "rebuild_keep_tangents" }, "units": out}))
}

// ───────────── Blender 往返 ─────────────

fn export_blender(root: &Path, src: &GeomSrc, t: &Target, out: &Path, hullsets: &str) -> Result<Value> {
    let mut sb = SceneBuilder::new();
    let (master, mv) = assemble::master_visual(root).map_err(|e| anyhow!("{e}"))?;
    let (secs, hps) = assemble::hardpoints(root, &master, &mv).map_err(|e| anyhow!("{e}"))?;
    let mut n_obj = 0usize;
    let mut add_unit = |sb: &mut SceneBuilder, kind: &str, name: &str, vp: &Path, gp: &Path, mat: &M4, filter: &dyn Fn(&str) -> bool| -> Result<()> {
        let v = Visual::parse(&std::fs::read_to_string(vp)?).map_err(|e| anyhow!("{e}"))?;
        let mut ge = GeoEdit::parse(&load_geometry(gp, src)?).map_err(|e| anyhow!("{e}"))?;
        let parent = sb.add_node(&format!("{kind}|{name}"), None, Some(&game_to_gltf(mat)), json!({"wowsmod_unit": kind, "wowsmod_name": name}), None);
        for k in 0..v.render_sets.len() {
            let rs = &v.render_sets[k];
            let node = rs.nodes.first().cloned().unwrap_or_default();
            if !filter(&node) {
                continue;
            }
            let rn = rs.base_name();
            let m = ge.mesh(&rn).map_err(|e| anyhow!("{rn}: {e}"))?;
            let sp = rs_to_visual(&v, k);
            let positions: Vec<[f32; 3]> = m.positions.iter().map(|p| pos_game_to_gltf(xform_point(&sp, p.map(|x| x as f64)).map(|x| x as f32))).collect();
            let normals = m.normals.as_ref().map(|ns| ns.iter().map(|n| pos_game_to_gltf(norm_dir(xform_dir(&sp, n.map(|x| x as f64))))).collect());
            let indices: Vec<u32> = m.indices.chunks_exact(3).flat_map(|t| [t[0], t[2], t[1]]).collect();
            let prim = ScenePrim { positions, normals, uvs: m.uvs.clone(), indices, material: Some(rs.material_identifier.clone()), extras: Value::Null };
            let mi = sb.add_mesh(&rn, &[prim]);
            sb.add_node(&rn, Some(mi), None, json!({"wowsmod_renderset": rn}), Some(parent));
            n_obj += 1;
        }
        Ok(())
    };
    if let Some(pn) = &t.part {
        let pf = find_part(root, pn)?;
        add_unit(&mut sb, "part", &pf.stem, &pf.visuals[0], &pf.geometry, &IDENTITY, &|_| true)?;
    } else {
        for (s, sm) in &secs {
            if t.section.as_ref().map(|x| !x.eq_ignore_ascii_case(s)).unwrap_or(false) {
                continue;
            }
            let stem = format!("{master}_{s}");
            let hs = hullsets.to_string();
            add_unit(&mut sb, "section", s, &root.join("ship").join(format!("{stem}.visual")), &root.join("ship").join(format!("{stem}.geometry")), sm, &|n: &str| assemble::keep_renderset(n, &hs))?;
        }
    }
    let mut n_hp = 0;
    if t.part.is_none() {
        for (hp, (s, w)) in &hps {
            sb.add_node(&format!("HP|{hp}"), None, Some(&game_to_gltf(w)), json!({"wowsmod_hp": hp, "section": s}), None);
            n_hp += 1;
        }
    }
    sb.asset_extras = json!({"wowsmod": "export-blender", "hull": master});
    let glb = sb.write("wowsmod model export-blender");
    if let Some(d) = out.parent() {
        if !d.as_os_str().is_empty() {
            std::fs::create_dir_all(d)?;
        }
    }
    std::fs::write(out, &glb)?;
    Ok(json!({"ok": true, "glb": out.display().to_string(), "bytes": glb.len(), "objects": n_obj, "hardpoint_empties": n_hp,
        "how": "Blender: 导入 glTF; 修改后导出 glTF(GLB, +Y Up, 勾选 Custom Properties), 再 wowsmod model import-blender"}))
}

fn same_mesh(a: &MeshData, b: &MeshData) -> bool {
    if a.positions.len() != b.positions.len() || a.indices.len() != b.indices.len() {
        return false;
    }
    a.positions.iter().zip(&b.positions).all(|(p, q)| (0..3).all(|i| (p[i] - q[i]).abs() <= 1e-4 * (1.0 + p[i].abs())))
}

fn import_blender(tx: &mut Tx, root: &Path, src: &GeomSrc, glb: &Path) -> Result<Value> {
    let d = std::fs::read(glb).with_context(|| format!("读取 {}", glb.display()))?;
    let sc = scene::read_scene(&d).map_err(|e| anyhow!("{e}"))?;
    // 网格节点所属 unit: 祖先节点名 "section|X" / "part|X"(Blender 可能加 .001 后缀)
    let unit_of = |mut i: usize| -> Option<(String, String, usize)> {
        loop {
            let n = &sc.nodes[i];
            if let Some((k, v)) = n.name.split_once('|') {
                if k == "section" || k == "part" {
                    let v = v.split('.').next().unwrap_or(v).to_string();
                    return Some((k.to_string(), v, i));
                }
            }
            i = n.parent?;
        }
    };
    let mut groups: BTreeMap<(String, String), Vec<(usize, usize)>> = BTreeMap::new();
    let mut skipped = Vec::new();
    for (i, n) in sc.nodes.iter().enumerate() {
        if n.mesh.is_none() {
            continue;
        }
        match unit_of(i) {
            Some((k, v, ui)) => groups.entry((k, v)).or_default().push((i, ui)),
            None => skipped.push(n.name.clone()),
        }
    }
    let mut out = Vec::new();
    for ((kind, name), items) in groups {
        let (vps, gp) = if kind == "section" {
            let (_, secs) = sections(root)?;
            let s = secs.into_iter().find(|s| s.sec == name).ok_or_else(|| anyhow!("没有分段 {name}"))?;
            (s.visuals, s.geometry)
        } else {
            let pf = find_part(root, &name)?;
            (pf.visuals, pf.geometry)
        };
        let mut ge = GeoEdit::parse(&tx.geometry(&gp, src)?).map_err(|e| anyhow!("{e}"))?;
        let mut vis = load_visuals(tx, &vps)?;
        let mut changed: Vec<Value> = Vec::new();
        let mut changed_names: BTreeSet<String> = BTreeSet::new();
        let mut unchanged = 0;
        let mut new_prims: Vec<GlbPrim> = Vec::new();
        for (ni, ui) in items {
            let n = &sc.nodes[ni];
            // 网格节点相对 unit 节点的矩阵(glTF), 转换后即 visual 空间
            let rel = mat_mul(&inv(&sc.nodes[ui].world)?, &n.world);
            let rs_name = n.extras.get("wowsmod_renderset").and_then(|x| x.as_str()).map(str::to_string).unwrap_or_else(|| n.name.split('.').next().unwrap_or(&n.name).to_string());
            let prims = &sc.meshes[n.mesh.unwrap()].prims;
            for p in prims {
                let mesh_vs = convert_prim(&rel, &IDENTITY, p);
                let k0 = vis[0].render_sets.iter().position(|r| r.base_name() == rs_name);
                match k0 {
                    Some(k) if prims.len() == 1 && ge.has(&rs_name) => {
                        let to_local = inv(&rs_to_visual(&vis[0], k))?;
                        let mut m = mesh_vs.clone();
                        for q in m.positions.iter_mut() {
                            *q = xform_point(&to_local, q.map(|x| x as f64)).map(|x| x as f32);
                        }
                        if let Some(ns) = m.normals.as_mut() {
                            for v in ns.iter_mut() {
                                *v = norm_dir(xform_dir(&to_local, v.map(|x| x as f64)));
                            }
                        }
                        let old = ge.mesh(&rs_name).map_err(|e| anyhow!("{e}"))?;
                        if same_mesh(&old, &m) {
                            unchanged += 1;
                            continue;
                        }
                        let info = ge.replace_mesh_keep(&rs_name, &m).map_err(|e| anyhow!("{rs_name}: {e}"))?;
                        changed.push(json!({"renderSet": rs_name, "vertices_before": old.positions.len(), "vertices_after": info.vertex_count, "per_vertex_skin_kept": old.positions.len() == m.positions.len()}));
                        changed_names.insert(rs_name.clone());
                    }
                    _ => new_prims.push(GlbPrim { node: n.name.clone(), material: p.material.clone(), mesh: mesh_vs }),
                }
            }
        }
        let mut added = Vec::new();
        for (i, gpm) in new_prims.into_iter().enumerate() {
            // 模板: 材质名相同的渲染集; 否则第一个非 crack 渲染集
            let tk = gpm
                .material
                .as_ref()
                .and_then(|m| vis[0].render_sets.iter().position(|r| &r.material_identifier == m))
                .or_else(|| vis[0].render_sets.iter().position(|r| !r.base_name().contains("_crack_")))
                .unwrap_or(0);
            let base = format!("wm_{}_{}", sanitize(&gpm.node), i);
            added.extend(add_prims(&mut ge, &mut vis, &[gpm], &base, tk, false)?);
        }
        if changed.is_empty() && added.is_empty() {
            out.push(json!({"unit": kind, "name": name, "unchanged": unchanged}));
            continue;
        }
        // LOD 策略: 改过的渲染集在 LOD1-N 中同家族的渲染集改指向 LOD0 的新网格(远处不跳回原样; 代价是远处面数不降)
        let mut lod_redirect = Vec::new();
        let fam_of: BTreeMap<String, (String, String)> = vis[0]
            .render_sets
            .iter()
            .filter(|r| changed_names.contains(&r.base_name()))
            .map(|r| (r.family(), (r.vertices.clone(), r.primitive.clone())))
            .collect();
        for v in vis.iter_mut().skip(1) {
            for k in 0..v.render_sets.len() {
                if let Some((vv, pp)) = fam_of.get(&v.render_sets[k].family()) {
                    lod_redirect.push(format!("{} → {vv}", v.render_sets[k].vertices));
                    v.set_render_set_geometry(k, vv, pp).map_err(|e| anyhow!("{e}"))?;
                }
            }
        }
        for v in vis.iter_mut() {
            if let Some(b) = v.bbox {
                let mut nb = Some(b);
                for k in 0..v.render_sets.len() {
                    let rn = v.render_sets[k].base_name();
                    if changed_names.contains(&rn) {
                        let m = rs_to_visual(v, k);
                        if let Some(x) = mesh_aabb(&mut ge, &rn, &m)? {
                            nb = Some(aabb_union(nb, x));
                        }
                    }
                }
                let nb = nb.unwrap();
                v.set_bbox(nb.0, nb.1).map_err(|e| anyhow!("{e}"))?;
            }
        }
        let geo = ge.finish().map_err(|e| anyhow!("{e}"))?;
        tx.write(&gp, geo.serialize());
        for (vp, v) in vps.iter().zip(&vis) {
            tx.write_visual(vp, v);
        }
        out.push(json!({"unit": kind, "name": name, "changed": changed, "added": added, "unchanged": unchanged,
            "lod_redirect": lod_redirect,
            "lod_note": "改动的渲染集: LOD1-3 中同家族渲染集改指向 LOD0 新网格(远处不跳变, 但远处面数不降); 新增的渲染集写进全部 LOD visual"}));
    }
    Ok(json!({"glb": glb.display().to_string(), "units": out, "ignored_nodes_without_unit": skipped}))
}

// ───────────── undo / replay ─────────────

fn edit_records(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).with_context(|| format!("读取 {}", dir.display()))?.flatten().map(|e| e.path()).filter(|p| p.extension().map(|x| x == "json").unwrap_or(false)).collect();
    v.sort();
    Ok(v)
}

fn undo(p: &Project, force: bool) -> Result<Value> {
    let recs = edit_records(&p.root.join("edits"))?;
    let (rp, v) = recs
        .iter()
        .rev()
        .find_map(|r| {
            let v: Value = serde_json::from_slice(&std::fs::read(r).ok()?).ok()?;
            v.get("undo").is_some().then(|| (r.clone(), v))
        })
        .ok_or_else(|| anyhow!("没有可撤销的 model 编辑记录"))?;
    let bdir = p.root.join(v["undo"]["backup_dir"].as_str().unwrap_or("-"));
    let files = v["undo"]["files"].as_array().cloned().unwrap_or_default();
    let mut mism = Vec::new();
    for f in &files {
        let path = p.root.join(f["path"].as_str().unwrap_or(""));
        let cur = std::fs::read(&path).ok().map(|d| modproj::sha256_hex(&d));
        if cur.as_deref() != f["after_sha256"].as_str() {
            mism.push(f["path"].as_str().unwrap_or("").to_string());
        }
    }
    if !mism.is_empty() && !force {
        bail!("这些文件在该次编辑之后又被改过, 不撤销(确认覆盖请加 --force): {mism:?}");
    }
    let mut restored = Vec::new();
    for f in &files {
        let rel = f["path"].as_str().unwrap_or("");
        let path = p.root.join(rel);
        if f["existed"].as_bool().unwrap_or(false) {
            let b = std::fs::read(bdir.join(rel)).with_context(|| format!("备份缺失: {rel}"))?;
            std::fs::write(&path, b)?;
        } else if path.exists() {
            std::fs::remove_file(&path)?;
        }
        restored.push(rel.to_string());
    }
    let undone = rp.with_extension("json.undone");
    std::fs::rename(&rp, &undone)?;
    let _ = std::fs::remove_dir_all(&bdir);
    Ok(json!({"ok": true, "undone": rp.file_name().unwrap().to_string_lossy(), "renamed_to": undone.file_name().unwrap().to_string_lossy(), "restored": restored, "forced_over": mism}))
}

fn replay(cli: &Cli, project: &Path, from: &Path) -> Result<Value> {
    let dir = if from.join("edits").is_dir() { from.join("edits") } else { from.to_path_buf() };
    let mut done = Vec::new();
    for r in edit_records(&dir)? {
        let v: Value = serde_json::from_slice(&std::fs::read(&r)?)?;
        if !v["cmd"].as_str().unwrap_or("").starts_with("model_") {
            continue;
        }
        let Some(args) = v.get("args") else { continue };
        let mut c: ModelCmd = serde_json::from_value(args.clone()).with_context(|| format!("{} 的 args 无法解析", r.display()))?;
        match c.target_mut() {
            Some(t) => t.project = project.to_path_buf(),
            None => continue,
        }
        let res = run_model(cli, &c).with_context(|| format!("重放 {}", r.display()))?;
        done.push(json!({"record": r.file_name().unwrap().to_string_lossy(), "cmd": v["cmd"], "changed_files": res["changed_files"], "edit_record": res["edit_record"]}));
    }
    Ok(json!({"ok": true, "replayed": done}))
}
