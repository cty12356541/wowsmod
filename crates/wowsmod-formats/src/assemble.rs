//! 官方数据驱动的整船装配器(移植自 tools/assembler/wows_assemble.py, 算法见该目录 README)。
//!
//! 1. 主 visual(ship/<hull>.visual)里的 `HP_<Section>` 节点 → 船体分段及其变换 S;
//!    每段: `<hull>_<Sec>.visual`(渲染集)与 `<hull>_<Sec>_ports.visual`(挂点)。
//! 2. 节点世界矩阵 W = L·W(parent)(行向量约定), 挂点 = W_hp · S。
//! 3. GameParams 船条目里名字匹配 `_<Kind>$` 的模块(同类取排序第一个)的 `HP_xxx.model` 决定挂点装什么。
//! 4. 渲染集名 murmur3 → geometry 映射; 只取 LOD0(主 visual 的渲染集); 船体默认 intact(主体 + *_patch_*, 去 *_crack_*)。
//! 5. glTF: v' = C·v(并翻转绕序), N = C·Wᵀ·C。
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::geomedit::GeoEdit;
use crate::scene::{game_to_gltf, pos_game_to_gltf, SceneBuilder, ScenePrim};
use crate::visual::{mat_mul, Visual, M4};

pub const SECTIONS_RE: &str = r"_(Bow|MidFront|MidBack|Stern)(_ports.*)?\.visual$";

#[derive(Debug, Clone)]
pub struct AssembleOpts {
    /// intact | broken | main | all
    pub hullsets: String,
    /// GameParams 模块种类, 如 Artillery, ATBA, AirDefense, Directors, Radars; "all" = 全部带 HP 模型的模块
    pub kinds: Vec<String>,
    /// 输出 NORMAL / TEXCOORD_0 与材质分组(Python 版只有 POSITION)
    pub full_attributes: bool,
}

impl Default for AssembleOpts {
    fn default() -> Self {
        AssembleOpts { hullsets: "intact".into(), kinds: vec!["Artillery".into(), "ATBA".into()], full_attributes: false }
    }
}

pub fn keep_renderset(name: &str, mode: &str) -> bool {
    if mode == "all" {
        return true;
    }
    if name.contains("_crack_") {
        return mode == "broken";
    }
    if name.contains("_patch_") {
        return mode == "intact";
    }
    true
}

/// SDK 船目录中按模型名找 visual(gun/director/radar/finder/... 任意子目录, 不含 lods)
pub fn find_part_visual(sdk: &Path, model: &str) -> Option<PathBuf> {
    for sub in ["gun", "director", "radar", "finder"] {
        let p = sdk.join(sub).join(format!("{model}.visual"));
        if p.exists() {
            return Some(p);
        }
    }
    let rd = std::fs::read_dir(sdk).ok()?;
    for e in rd.flatten() {
        let p = e.path().join(format!("{model}.visual"));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

pub fn master_visual(sdk: &Path) -> Result<(String, Visual)> {
    let shipdir = sdk.join("ship");
    let re = Regex::new(SECTIONS_RE).unwrap();
    let mut files: Vec<String> = std::fs::read_dir(&shipdir)
        .map_err(|e| Error::Io { path: shipdir.display().to_string(), source: e })?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|f| f.ends_with(".visual") && !re.is_match(f))
        .collect();
    files.sort();
    for f in files {
        let v = Visual::parse(&read_text(&shipdir.join(&f))?)?;
        if v.nodes.iter().any(|n| n.name.starts_with("HP_")) {
            return Ok((f.trim_end_matches(".visual").to_string(), v));
        }
    }
    Err(Error::NotFound(format!("{} 中没有带 HP_<Section> 节点的主 visual", shipdir.display())))
}

pub fn read_text(p: &Path) -> Result<String> {
    let b = std::fs::read(p).map_err(|e| Error::Io { path: p.display().to_string(), source: e })?;
    Ok(String::from_utf8_lossy(&b).into_owned())
}

/// 挂点: (分段, 世界矩阵 W_hp·S)
pub fn hardpoints(sdk: &Path, master: &str, mv: &Visual) -> Result<(Vec<(String, M4)>, BTreeMap<String, (String, M4)>)> {
    let mut sections = Vec::new();
    for n in &mv.nodes {
        if n.name.starts_with("HP_") && n.name != "HP_Full" {
            sections.push((n.name[3..].to_string(), n.world));
        }
    }
    let mut hps = BTreeMap::new();
    for (s, sm) in &sections {
        let pv = sdk.join("ship").join(format!("{master}_{s}_ports.visual"));
        if !pv.exists() {
            continue;
        }
        let v = Visual::parse(&read_text(&pv)?)?;
        for n in &v.nodes {
            if n.name.starts_with("HP_") {
                hps.insert(n.name.clone(), (s.clone(), mat_mul(&n.world, sm)));
            }
        }
    }
    Ok((sections, hps))
}

/// GameParams 模块里的挂点 → 模型
#[derive(Debug, Clone, serde::Serialize)]
pub struct Mount {
    pub hp: String,
    pub module: String,
    pub kind: String,
    pub model: String,
    pub num_barrels: Value,
}

pub fn mounts_from_gp(ship: &Value, kinds: &[String]) -> Vec<Mount> {
    let obj = ship.as_object().cloned().unwrap_or_default();
    let mut all_kinds: Vec<String> = kinds.to_vec();
    if kinds.iter().any(|k| k == "all") {
        let re = Regex::new(r"^[A-Z0-9]+_([A-Za-z]+)$").unwrap();
        let mut ks: Vec<String> = obj
            .iter()
            .filter(|(_, v)| v.as_object().map(|o| o.iter().any(|(h, s)| h.starts_with("HP_") && s["model"].as_str().map(|m| !m.is_empty()).unwrap_or(false))).unwrap_or(false))
            .filter_map(|(k, _)| re.captures(k).map(|c| c[1].to_string()))
            .collect();
        ks.sort();
        ks.dedup();
        all_kinds = ks;
    }
    let mut out = Vec::new();
    for kind in &all_kinds {
        let re = Regex::new(&format!("_{}$", regex::escape(kind))).unwrap();
        let mut mods: Vec<&String> = obj.keys().filter(|k| re.is_match(k) && obj[*k].is_object()).collect();
        mods.sort();
        let Some(m) = mods.first() else { continue };
        let mut hps: Vec<(&String, &Value)> = obj[*m].as_object().unwrap().iter().filter(|(h, s)| h.starts_with("HP_") && s.is_object()).collect();
        hps.sort_by(|a, b| (a.0.len(), a.0).cmp(&(b.0.len(), b.0)));
        for (hp, spec) in hps {
            let model = spec["model"].as_str().unwrap_or("");
            if model.is_empty() {
                continue;
            }
            out.push(Mount { hp: hp.clone(), module: (*m).clone(), kind: kind.clone(), model: model.to_string(), num_barrels: spec["numBarrels"].clone() });
        }
    }
    out
}

fn r5(x: f64) -> f64 {
    (x * 1e5).round() / 1e5
}
fn r4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

/// 装配整船。`geometry(stem)` 返回 `<stem>.geometry` 的字节。返回 (GLB, 装配表, 警告)
pub fn assemble(sdk: &Path, ship_name: &str, ship: &Value, geometry: &dyn Fn(&str) -> Result<Vec<u8>>, opts: &AssembleOpts) -> Result<(Vec<u8>, Value, Vec<String>)> {
    let (master, mv) = master_visual(sdk)?;
    let (sections, hps) = hardpoints(sdk, &master, &mv)?;
    let mut warn = Vec::new();
    let mut sb = SceneBuilder::new();
    let mut table_sections = serde_json::Map::new();
    let mesh_prims = |ge: &mut GeoEdit, vis: &Visual, filter: &dyn Fn(&str) -> bool, warn: &mut Vec<String>| -> Result<Vec<ScenePrim>> {
        let mut prims = Vec::new();
        for rs in &vis.render_sets {
            let node = rs.nodes.first().cloned().unwrap_or_default();
            if !filter(&node) {
                continue;
            }
            let name = rs.base_name();
            let m = match ge.mesh(&name) {
                Ok(m) => m,
                Err(e) => {
                    warn.push(format!("{name}: {e}"));
                    continue;
                }
            };
            let positions: Vec<[f32; 3]> = m.positions.iter().map(|p| pos_game_to_gltf(*p)).collect();
            let indices: Vec<u32> = m.indices.chunks_exact(3).flat_map(|t| [t[0], t[2], t[1]]).collect();
            let (normals, uvs, material) = if opts.full_attributes {
                (m.normals.as_ref().map(|n| n.iter().map(|v| pos_game_to_gltf(*v)).collect()), m.uvs.clone(), Some(rs.material_identifier.clone()))
            } else {
                (None, None, None)
            };
            prims.push(ScenePrim {
                positions,
                normals,
                uvs,
                indices,
                material,
                extras: json!({"renderSet": node, "vertices": rs.vertices, "mfm": rs.mfm, "materialIdentifier": rs.material_identifier}),
            });
        }
        Ok(prims)
    };
    for (s, sm) in &sections {
        let stem = format!("{master}_{s}");
        let vp = sdk.join("ship").join(format!("{stem}.visual"));
        let vis = Visual::parse(&read_text(&vp)?)?;
        let mut ge = GeoEdit::parse(&geometry(&stem)?)?;
        let mode = opts.hullsets.clone();
        let prims = mesh_prims(&mut ge, &vis, &|n: &str| keep_renderset(n, &mode), &mut warn)?;
        let nv: usize = prims.iter().map(|p| p.positions.len()).sum();
        let np = prims.len();
        let mi = sb.add_mesh(&format!("Hull_{s}"), &prims);
        sb.add_node(&format!("Hull_{s}"), Some(mi), Some(&game_to_gltf(sm)), json!({"section": s}), None);
        table_sections.insert(s.clone(), json!({"renderSets": np, "vertices": nv}));
    }
    let mut gunmesh: BTreeMap<String, usize> = BTreeMap::new();
    let mut mounts = Vec::new();
    for mt in mounts_from_gp(ship, &opts.kinds) {
        let Some((sec, w)) = hps.get(&mt.hp).cloned() else {
            warn.push(format!("挂点不在 visual 中: {}", mt.hp));
            continue;
        };
        let model = mt.model.rsplit('/').next().unwrap_or(&mt.model).trim_end_matches(".model").to_string();
        if !gunmesh.contains_key(&model) {
            let Some(gv) = find_part_visual(sdk, &model) else {
                warn.push(format!("{model}: SDK 中没有 visual, 跳过 {}", mt.hp));
                continue;
            };
            let vis = Visual::parse(&read_text(&gv)?)?;
            let mut ge = match geometry(&model) {
                Ok(d) => GeoEdit::parse(&d)?,
                Err(e) => {
                    warn.push(format!("{model}: 读不到 geometry({e}), 跳过 {}", mt.hp));
                    continue;
                }
            };
            let prims = mesh_prims(&mut ge, &vis, &|_| true, &mut warn)?;
            gunmesh.insert(model.clone(), sb.add_mesh(&model, &prims));
        }
        let n = game_to_gltf(&w);
        let yaw = n[0][2].atan2(n[2][2]).to_degrees();
        sb.add_node(&format!("{}|{}", mt.hp, model), Some(gunmesh[&model]), Some(&n), json!({"hp": mt.hp, "model": mt.model, "module": mt.module, "section": sec}), None);
        mounts.push(json!({
            "hp": mt.hp, "module": mt.module, "kind": mt.kind, "model": mt.model, "section": sec, "numBarrels": mt.num_barrels,
            "game_world_matrix": w.iter().map(|r| r.iter().map(|x| r5(*x)).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "gltf_pos": [r4(n[0][3]), r4(n[1][3]), r4(n[2][3])],
            "gltf_yaw_deg": (yaw * 100.0).round() / 100.0,
            "game_barrel_dir": [r4(w[2][0]), r4(w[2][1]), r4(w[2][2])],
        }));
    }
    sb.asset_extras = json!({"ship": ship_name, "hullModel": master});
    let table = json!({
        "ship": ship_name, "hullModel": master, "sections": table_sections, "mounts": mounts,
        "hardpoints_total": hps.len(),
        "conventions": {
            "game": "BigWorld 左手系: x 右舷, y 上, z 船艏; .visual 矩阵为行向量约定, row3=平移",
            "gltf": "glTF 右手系: x 右舷, y 上, -z 船艏; C=diag(1,1,-1), v_gltf=C·v_game, N_gltf=C·Wᵀ·C",
            "meshes": "渲染集名 MurmurHash3_x86_32(seed0) → .geometry vmap/imap; 只取 LOD0; 船体取完好状态: 主体+*_patch_*, 去掉 *_crack_*"
        },
        "warnings": warn,
    });
    Ok((sb.write("wowsmod assemble"), table, warn))
}
