//! model animate: 外部网格做成独立的会动的模型(misc/<名>.model/.visual/.geometry + 材质贴图),
//! 生成 .seq(关键帧)与 .ssm(状态机), 并在分段 .model 里挂上 `<animations>`。
//! 预设: spin(自转) / orbit(绕圆心环绕) / float(上下浮动), 可叠加(如 orbit + --float-amp)。
//! 结构参考公开 mod 的只读拆解(见 wowsmod_formats::anim), 引擎语义需港口实证。
use std::path::Path;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use wowsmod_formats::{
    anim::{self, AnimSpec},
    geomedit::GeoEdit,
    geometry::Geometry,
    mfm,
    visual::{compose, format_transform, IDENTITY},
};

use super::import::{build_materials, mesh_height, mesh_prims, ImportOpts};
use super::{find_template, load_visuals, parse_vec3, sanitize, units_for, GeomSrc, Target, Tx};

pub struct AnimOpts<'a> {
    pub preset: &'a str,
    pub rpm: Option<f64>,
    pub axis: &'a str,
    pub radius: f64,
    pub orbit_rpm: Option<f64>,
    pub phase: f64,
    pub float_amp: Option<f64>,
    pub period: f64,
    pub format: &'a str,
    pub all_lods: bool,
}

fn deskin(ident: &str, skinned: bool) -> String {
    if skinned {
        ident.to_string()
    } else {
        ident.trim().trim_end_matches("_skinned").to_string()
    }
}

fn axis_index(a: &str) -> Result<usize> {
    Ok(match a {
        "vertical" | "yaw" | "z" => 2,
        "pitch" | "x" => 1,
        "roll" | "y" => 0,
        _ => bail!("--axis 只能是 vertical(yaw) / pitch / roll"),
    })
}

pub fn animate(tx: &mut Tx, root: &Path, src: &GeomSrc, t: &Target, o: &ImportOpts, a: &AnimOpts) -> Result<Value> {
    if t.section.is_none() || t.part.is_some() || t.hp.is_some() {
        bail!("animate 需要 --section <分段>(动画挂在分段 .model 上; 位置用 --at, 分段坐标)");
    }
    let u = units_for(root, t)?.into_iter().next().ok_or_else(|| anyhow!("没有分段"))?;
    let vis = load_visuals(tx, &u.visuals)?;
    let tk = find_template(&vis[0], o.like)?;
    let tpl = vis[0].render_sets[tk].clone();
    // 资源前缀 PnFMods/<mod>/<ship>
    let parts: Vec<&str> = tpl.mfm.trim().split('/').collect();
    if parts.len() < 4 || parts[0] != "PnFMods" {
        bail!("模板材质 {} 不在本项目里, 换一个 --like", tpl.mfm);
    }
    let prefix = parts[..3].join("/");
    let base = o.name.map(sanitize).unwrap_or_else(|| format!("wm_{}", sanitize(&o.mesh.file_stem().unwrap().to_string_lossy())));
    let misc_res = format!("{prefix}/misc");
    let misc_dir = root.join("misc");

    // 网格: 在自身局部空间(原点 = 动画位置), 只做旋转/缩放
    let mut s = parse_vec3(o.scale)?;
    let mut fit = json!(null);
    if let Some(h) = o.height {
        let mh = mesh_height(o.mesh)?;
        if mh <= 1e-9 {
            bail!("网格高度为 0");
        }
        s = [h / mh; 3];
        fit = json!({"mesh_height": mh, "target_height": h, "scale": h / mh});
    }
    let g = compose([0.0; 3], parse_vec3(o.rot)?, s);
    let prims_m = mesh_prims(o.mesh, &g)?;
    let mb = build_materials(tx, root, src, &tpl.mfm, Some((&misc_res, &misc_dir)), o, &prims_m, &base)?;

    // 独立模型不蒙皮: 顶点格式没有骨骼字段时, 材质着色器与标识去掉 _skinned
    let skinned = a.format.contains("iiiww");
    let mut deskinned = Vec::new();
    if !skinned {
        for (_, _, mres, _) in &mb.groups {
            let mp = misc_dir.join(mres.rsplit('/').next().unwrap_or(""));
            let t0 = tx.read_text(&mp)?;
            if let Some(fx) = mfm::fx(&t0) {
                if fx.contains("_skinned") {
                    let nfx = fx.replace("_skinned", "");
                    tx.write(&mp, mfm::set_fx(&t0, &nfx).map_err(|e| anyhow!("{e}"))?.into_bytes());
                    deskinned.push(json!([fx, nfx]));
                }
            }
        }
    }
    // 独立 geometry(全新, 不含碰撞/装甲)
    let mut ge = GeoEdit::new(Geometry::default());
    let (mut mn, mut mx) = ([f64::MAX; 3], [f64::MIN; 3]);
    let mut rsets = String::new();
    let mut names = Vec::new();
    for (gp, stem, mres, glowing) in &mb.groups {
        for (i, p) in gp.iter().enumerate() {
            let name = if gp.len() == 1 { stem.clone() } else { format!("{stem}_{i}") };
            for q in &p.mesh.positions {
                for k in 0..3 {
                    mn[k] = mn[k].min(q[k] as f64);
                    mx[k] = mx[k].max(q[k] as f64);
                }
            }
            ge.add_mesh(&name, a.format, &p.mesh, None).map_err(|e| anyhow!("{name}: {e}"))?;
            let _ = glowing;
            let ident = deskin(&tpl.material_identifier, skinned);
            rsets += &format!(
                "\t<renderSet>\n\t\t<treatAsWorldSpaceObject>\tfalse\t</treatAsWorldSpaceObject>\n\t\t<node>\t{base}\t</node>\n\t\t<geometry>\n\t\t\t<vertices>\t{name}Shape.vertices\t</vertices>\n\t\t\t<primitive>\t{name}Shape.indices\t</primitive>\n\t\t\t<primitiveGroup>\t0\n\t\t\t\t<material>\n\t\t\t\t\t<identifier>\t{ident}\t</identifier>\n\t\t\t\t\t<mfm>\t{mres}\t</mfm>\n\t\t\t\t</material>\n\t\t\t</primitiveGroup>\n\t\t</geometry>\n\t</renderSet>\n"
            );
            names.push(json!({"renderSet": name, "mfm": mres, "identifier": ident, "vertices": p.mesh.positions.len(), "triangles": p.mesh.indices.len() / 3}));
        }
    }
    let geo = ge.finish().map_err(|e| anyhow!("{e}"))?;
    let geo_path = misc_dir.join(format!("{base}.geometry"));
    tx.write(&geo_path, geo.serialize());
    let vis_text = format!(
        "<{base}.visual>\n\t<node>\n\t\t<identifier>\tScene Root\t</identifier>\n{}\t\t<node>\n\t\t\t<identifier>\t{base}\t</identifier>\n{}\t\t</node>\n\t</node>\n{rsets}\t<boundingBox>\n\t\t<min>\t{:.4} {:.4} {:.4}\t</min>\n\t\t<max>\t{:.4} {:.4} {:.4}\t</max>\n\t</boundingBox>\n</{base}.visual>\n",
        format!("\t\t{}\n", format_transform(&IDENTITY, "\t\t")),
        format!("\t\t\t{}\n", format_transform(&IDENTITY, "\t\t\t")),
        mn[0], mn[1], mn[2], mx[0], mx[1], mx[2]
    );
    // 发光材质: 标识改成 EMISSIVE 族(同 import)
    let mut vis_text = vis_text;
    let mut idents = Vec::new();
    for (_, _, mres, glowing) in &mb.groups {
        if *glowing {
            let (nt, ch) = mfm::set_emissive_identifiers(&vis_text, mres.rsplit('/').next().unwrap_or(""));
            vis_text = nt;
            idents.extend(ch.into_iter().map(|(a, b)| json!([a, b])));
        }
    }
    tx.write(&misc_dir.join(format!("{base}.visual")), vis_text.into_bytes());
    let model_res = format!("{misc_res}/{base}.model");
    let model_text = format!("<{base}.model>\n\t<castsShadow>\ttrue\t</castsShadow>\n\t<extent>\t2000.0000\t</extent>\n\t<nodefullVisual>\t{misc_res}/{base}\t</nodefullVisual>\n\t<metaData>\twowsmod model animate\t</metaData>\n</{base}.model>\n");
    tx.write(&misc_dir.join(format!("{base}.model")), model_text.into_bytes());

    // 动画规格
    let preset = a.preset;
    let at = parse_vec3(o.at)?;
    let mut sp = AnimSpec { at, spin_rpm: 0.0, spin_axis: axis_index(a.axis)?, orbit_radius: 0.0, orbit_rpm: 0.0, phase_deg: a.phase, float_amp: 0.0, float_period: a.period, base_rot: [0.0; 3], scale: 1.0 };
    match preset {
        "spin" => sp.spin_rpm = a.rpm.unwrap_or(10.0),
        "orbit" => {
            if a.radius <= 0.0 {
                bail!("orbit 需要 --radius(visual 单位, 1 ≈ 30 m)");
            }
            sp.orbit_radius = a.radius;
            sp.orbit_rpm = a.orbit_rpm.unwrap_or(3.0);
            sp.spin_rpm = a.rpm.unwrap_or(0.0);
        }
        "float" => {
            sp.float_amp = a.float_amp.unwrap_or(0.02);
            sp.spin_rpm = a.rpm.unwrap_or(0.0);
        }
        p => bail!("--preset 只能是 spin / orbit / float: {p}"),
    }
    if let Some(f) = a.float_amp {
        sp.float_amp = f;
    }
    if sp.float_amp > 0.0 && sp.float_period <= 0.0 {
        bail!("--period 必须 > 0");
    }
    let seq_res = format!("{misc_res}/{base}.seq");
    let ssm_res = format!("{misc_res}/{base}.ssm");
    let (seq, dur) = anim::seq_text(&base, &model_res, &model_res, &sp);
    tx.write(&misc_dir.join(format!("{base}.seq")), seq.into_bytes());
    tx.write(&misc_dir.join(format!("{base}.ssm")), anim::ssm_text(&base, &seq_res).into_bytes());
    // 挂到分段 .model(LOD0; --all-lods 时每级都挂)
    let mut hooked = Vec::new();
    let vlist: Vec<_> = if a.all_lods { u.visuals.clone() } else { u.visuals.iter().take(1).cloned().collect() };
    for vp in vlist {
        let mp = vp.with_extension("model");
        if !mp.is_file() && !tx.pending.contains_key(&mp) {
            bail!("找不到分段模型 {}", mp.display());
        }
        let txt = tx.read_text(&mp)?;
        let nt = anim::model_add_animation(&txt, &ssm_res).map_err(|e| anyhow!("{}: {e}", mp.display()))?;
        tx.write(&mp, nt.into_bytes());
        hooked.push(mp.display().to_string());
    }
    Ok(json!({"section": u.name, "name": base, "model": model_res, "seq": seq_res, "ssm": ssm_res, "duration_s": dur,
        "preset": preset, "spec": {"at": sp.at, "spin_rpm": sp.spin_rpm, "spin_axis": a.axis, "orbit_radius": sp.orbit_radius, "orbit_rpm": sp.orbit_rpm, "phase_deg": sp.phase_deg, "float_amp": sp.float_amp, "float_period_s": sp.float_period},
        "geometry": {"file": geo_path.display().to_string(), "format": a.format, "bbox": [mn, mx], "render_sets": names, "fit_height": fit},
        "material_mode": if mb.per_mat { "per-material" } else { "single" }, "materials": mb.mats_out, "textures_written": mb.n_written, "texture_reuses": mb.n_shared,
        "hooked_models": hooked, "warnings": mb.warn, "fx_deskinned": deskinned, "identifiers_changed": idents,
        "notes": ["动画在分段 .model 的 <animations> 里, 由 .ssm 循环播放 .seq; 港口实证前, 转速单位(度/秒)与 rotationEuler 轴向是按拆解推断的",
                  "工作台: 装配时按 .seq 采样放置并播放(见 help model)", "撤销: model undo"]}))
}
