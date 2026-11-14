//! Faithful preview-only skin assembly from SDK visual node palettes.
use std::collections::BTreeMap;
use serde_json::{json, Value};
use crate::{geomedit::GeoEdit, scene::{game_to_gltf, SceneBuilder, ScenePrim}, vertex::SkinData, visual::{Visual, VNode, IDENTITY}, Error, Result};

pub const VERSION: u32 = 1;

pub struct PreviewRig {
    nodes: Vec<VNode>,
    palette: Vec<usize>,
    pub skins: Vec<SkinData>,
}

fn bone_role(name: &str) -> Option<&'static str> {
    match name { "Rotate_Y" => Some("yaw"), "Rotate_X" => Some("pitch"), "Roll_Back1" => Some("recoil"), _ => None }
}
fn muzzle_index(name: &str) -> Option<usize> {
    let suffix = name.strip_prefix("HP_gunFire")?;
    if suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_digit()) { return None; }
    suffix.parse::<usize>().ok()?.checked_sub(1)
}

impl PreviewRig {
    pub fn prepare(vis: &Visual, ge: &mut GeoEdit, prims: &[ScenePrim]) -> Result<Self> {
        let mut skins = Vec::new();
        for p in prims {
            let vertices = p.extras["vertices"].as_str().ok_or_else(|| Error::format("skin", "missing renderSet vertices"))?;
            let rs = vis.render_sets.iter().position(|r| r.vertices == vertices).ok_or_else(|| Error::format("skin", "renderSet missing from visual"))?;
            let skin = ge.mesh_skin(&vis.render_sets[rs].base_name())?.ok_or_else(|| Error::Unsupported(format!("{vertices}: no supported skin attributes")))?;
            skins.push((rs, skin));
        }
        Self::from_visual_skins(vis, skins)
    }

    fn from_visual_skins(vis: &Visual, source: Vec<(usize, SkinData)>) -> Result<Self> {
        if source.is_empty() { return Err(Error::format("skin", "no mesh primitives")); }
        let mut names = BTreeMap::new();
        for (i, n) in vis.nodes.iter().enumerate() {
            if names.insert(n.name.as_str(), i).is_some() { return Err(Error::format("skin", format!("ambiguous node {}", n.name))); }
        }
        let mut palette = Vec::new();
        let mut skins = Vec::new();
        for (rs_index, mut skin) in source {
            let rs = vis.render_sets.get(rs_index).ok_or_else(|| Error::format("skin", "renderSet index out of bounds"))?;
            let mut remap = Vec::new();
            for name in &rs.nodes {
                let node = *names.get(name.as_str()).ok_or_else(|| Error::format("skin", format!("missing palette node {name}")))?;
                let i = match palette.iter().position(|n| *n == node) { Some(i) => i, None => { palette.push(node); palette.len() - 1 } };
                remap.push(u16::try_from(i).map_err(|_| Error::format("skin", "too many joints"))?);
            }
            if remap.is_empty() || skin.joints.len() != skin.weights.len() { return Err(Error::format("skin", "missing palette or mismatched weights")); }
            for (joints, weights) in skin.joints.iter_mut().zip(&skin.weights) {
                for i in 0..4 {
                    joints[i] = if weights[i] == 0.0 { 0 } else { *remap.get(joints[i] as usize).ok_or_else(|| Error::format("skin", "joint outside renderSet palette"))? };
                }
            }
            skins.push(skin);
        }
        Ok(Self { nodes: vis.nodes.clone(), palette, skins })
    }

    pub fn metadata(&self) -> Value {
        let bones: Vec<&str> = ["yaw", "pitch", "recoil"].into_iter().filter(|role| self.nodes.iter().any(|n| bone_role(&n.name) == Some(role))).collect();
        json!({"version": VERSION, "status": "skinned", "bones": bones, "muzzles": self.nodes.iter().filter(|n| muzzle_index(&n.name).is_some()).count()})
    }

    pub fn add_instance(&self, sb: &mut SceneBuilder, mount: usize) {
        let mut ids = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let mut extras = json!({"sourceNode": node.name});
            if let Some(role) = bone_role(&node.name) { extras["bone"] = json!(role); }
            if let Some(i) = muzzle_index(&node.name) { extras["muzzle"] = json!(i); }
            let parent = node.parent.map(|p| ids[p]).unwrap_or(mount);
            ids.push(sb.add_node(&node.name, None, Some(&game_to_gltf(&node.local)), extras, Some(parent)));
        }
        let joints: Vec<usize> = self.palette.iter().map(|i| ids[*i]).collect();
        // BlendBone descendants already contain the source inverse-bind compensation.
        // BigWorld evaluates sum(weight * BlendBone.world * vertex) directly.
        let skin = sb.add_skin(&joints, &vec![IDENTITY; joints.len()]);
        sb.set_node_skin(mount, skin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{scene::ScenePrim, vertex::SkinData};
    use serde_json::{json, Value};

    fn visual() -> Visual {
        Visual::parse("<x><node><identifier>Root</identifier><node><identifier>Rotate_Y</identifier><node><identifier>Rotate_Y_BlendBone</identifier></node><node><identifier>Rotate_X</identifier><node><identifier>Roll_Back1</identifier><node><identifier>Roll_Back1_BlendBone</identifier></node><node><identifier>HP_gunFire1</identifier></node><node><identifier>HP_gunFireEffect</identifier></node></node></node></node></node><renderSet><node>Rotate_Y_BlendBone</node><node>Roll_Back1_BlendBone</node><geometry><vertices>BodyShape.vertices</vertices></geometry></renderSet><renderSet><node>Roll_Back1_BlendBone</node><geometry><vertices>BarrelShape.vertices</vertices></geometry></renderSet></x>").unwrap()
    }
    fn skin(joints: [u16; 4], weights: [f32; 4]) -> SkinData { SkinData { joints: vec![joints], weights: vec![weights] } }

    #[test]
    fn remaps_render_set_local_palettes_without_discarding_blends() {
        let rig = PreviewRig::from_visual_skins(&visual(), vec![
            (0, skin([0, 1, 0, 0], [0.25, 0.75, 0.0, 0.0])),
            (1, skin([0; 4], [1.0, 0.0, 0.0, 0.0])),
        ]).unwrap();
        assert_eq!(rig.skins[0].joints[0], [0, 1, 0, 0]);
        assert_eq!(rig.skins[0].weights[0], [0.25, 0.75, 0.0, 0.0]);
        assert_eq!(rig.skins[1].joints[0], [1, 0, 0, 0]);
        assert_eq!(rig.metadata()["muzzles"], 1);
        assert_eq!(rig.metadata()["bones"], json!(["yaw", "pitch", "recoil"]));
        assert!(PreviewRig::from_visual_skins(&visual(), vec![(1, skin([1, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]))]).is_err());
    }

    #[test]
    fn each_mount_gets_its_own_source_hierarchy_and_joint_nodes() {
        let rig = PreviewRig::from_visual_skins(&visual(), vec![(0, skin([0; 4], [1.0, 0.0, 0.0, 0.0]))]).unwrap();
        let mut b = SceneBuilder::new();
        let mesh = b.add_mesh_skinned("body", &[ScenePrim { positions: vec![[0.0; 3]], ..Default::default() }], &rig.skins).unwrap();
        for i in 0..2 {
            let parent = b.add_node(&format!("mount{i}"), Some(mesh), None, Value::Null, None);
            rig.add_instance(&mut b, parent);
        }
        let (j, _) = crate::glb::read_glb(&b.write("test")).unwrap();
        assert_eq!(j["skins"].as_array().unwrap().len(), 2);
        assert_ne!(j["skins"][0]["joints"], j["skins"][1]["joints"]);
        assert_eq!(j["nodes"].as_array().unwrap().iter().filter(|n| n["extras"].get("muzzle").is_some()).count(), 2);
    }
}
