//! 带节点层级的 glTF 2.0 (GLB) 读写: 整船装配、Blender 往返用。
//!
//! 坐标约定: 游戏(BigWorld)左手系 x 右舷 / y 上 / z 船艏, 矩阵行向量约定;
//! glTF 右手系。C = diag(1,1,−1): 顶点 v' = C·v(并翻转绕序), 节点矩阵 N = C·Mᵀ·C(列向量约定)。
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::visual::{mat_mul, transpose, M4, IDENTITY};

const C: M4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, -1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

/// 游戏矩阵(行向量) → glTF 矩阵(列向量, 以 M4 行主序表示)
pub fn game_to_gltf(m: &M4) -> M4 {
    mat_mul(&mat_mul(&C, &transpose(m)), &C)
}
/// glTF 矩阵 → 游戏矩阵
pub fn gltf_to_game(n: &M4) -> M4 {
    transpose(&mat_mul(&mat_mul(&C, n), &C))
}
pub fn pos_game_to_gltf(p: [f32; 3]) -> [f32; 3] {
    [p[0], p[1], -p[2]]
}

#[derive(Debug, Clone, Default)]
pub struct ScenePrim {
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub uvs: Option<Vec<[f32; 2]>>,
    pub indices: Vec<u32>,
    pub material: Option<String>,
    /// glTF materials[] 下标(读取时填; 写出时不用)
    pub material_index: Option<usize>,
    pub extras: Value,
}

#[derive(Default)]
pub struct SceneBuilder {
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
    meshes: Vec<Value>,
    nodes: Vec<Value>,
    roots: Vec<usize>,
    materials: Vec<Value>,
    mat_index: std::collections::HashMap<String, usize>,
    pub asset_extras: Value,
}

impl SceneBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    fn view(&mut self, data: &[u8], target: u32) -> usize {
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        let off = self.bin.len();
        self.bin.extend_from_slice(data);
        self.views.push(json!({"buffer": 0, "byteOffset": off, "byteLength": data.len(), "target": target}));
        self.views.len() - 1
    }
    fn accessor_f32(&mut self, data: &[f32], comps: usize, minmax: bool) -> usize {
        let raw: Vec<u8> = data.iter().flat_map(|x| x.to_le_bytes()).collect();
        let v = self.view(&raw, 34962);
        let ty = ["", "SCALAR", "VEC2", "VEC3", "VEC4"][comps];
        let mut a = json!({"bufferView": v, "componentType": 5126, "count": data.len() / comps, "type": ty});
        if minmax {
            let mut mn = vec![f32::INFINITY; comps];
            let mut mx = vec![f32::NEG_INFINITY; comps];
            for ch in data.chunks_exact(comps) {
                for k in 0..comps {
                    mn[k] = mn[k].min(ch[k]);
                    mx[k] = mx[k].max(ch[k]);
                }
            }
            if data.is_empty() {
                mn = vec![0.0; comps];
                mx = vec![0.0; comps];
            }
            a["min"] = json!(mn);
            a["max"] = json!(mx);
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }
    pub fn add_mesh(&mut self, name: &str, prims: &[ScenePrim]) -> usize {
        let mut ps = Vec::new();
        for p in prims {
            let pos: Vec<f32> = p.positions.iter().flatten().copied().collect();
            let mut attrs = serde_json::Map::new();
            attrs.insert("POSITION".into(), json!(self.accessor_f32(&pos, 3, true)));
            if let Some(n) = &p.normals {
                let d: Vec<f32> = n.iter().flatten().copied().collect();
                attrs.insert("NORMAL".into(), json!(self.accessor_f32(&d, 3, false)));
            }
            if let Some(u) = &p.uvs {
                let d: Vec<f32> = u.iter().flatten().copied().collect();
                attrs.insert("TEXCOORD_0".into(), json!(self.accessor_f32(&d, 2, false)));
            }
            let raw: Vec<u8> = p.indices.iter().flat_map(|x| x.to_le_bytes()).collect();
            let iv = self.view(&raw, 34963);
            self.accessors.push(json!({"bufferView": iv, "componentType": 5125, "count": p.indices.len(), "type": "SCALAR"}));
            let mut pr = json!({"attributes": attrs, "indices": self.accessors.len() - 1, "mode": 4});
            if let Some(m) = &p.material {
                let k = match self.mat_index.get(m) {
                    Some(k) => *k,
                    None => {
                        self.materials.push(json!({"name": m, "doubleSided": false}));
                        self.mat_index.insert(m.clone(), self.materials.len() - 1);
                        self.materials.len() - 1
                    }
                };
                pr["material"] = json!(k);
            }
            if !p.extras.is_null() {
                pr["extras"] = p.extras.clone();
            }
            ps.push(pr);
        }
        self.meshes.push(json!({"name": name, "primitives": ps}));
        self.meshes.len() - 1
    }
    /// `matrix_gltf`: 列向量约定的 4×4(行主序给出); None = 单位阵
    pub fn add_node(&mut self, name: &str, mesh: Option<usize>, matrix_gltf: Option<&M4>, extras: Value, parent: Option<usize>) -> usize {
        let mut n = json!({"name": name});
        if let Some(m) = mesh {
            n["mesh"] = json!(m);
        }
        if let Some(m) = matrix_gltf {
            let ident = (0..4).all(|i| (0..4).all(|j| (m[i][j] - IDENTITY[i][j]).abs() < 1e-12));
            if !ident {
                // glTF matrix 为列主序
                let cm: Vec<f64> = (0..4).flat_map(|c| (0..4).map(move |r| m[r][c])).collect();
                n["matrix"] = json!(cm);
            }
        }
        if !extras.is_null() {
            n["extras"] = extras;
        }
        self.nodes.push(n);
        let id = self.nodes.len() - 1;
        match parent {
            Some(p) => {
                let ch = self.nodes[p].as_object_mut().unwrap().entry("children").or_insert(json!([]));
                ch.as_array_mut().unwrap().push(json!(id));
            }
            None => self.roots.push(id),
        }
        id
    }
    pub fn write(mut self, generator: &str) -> Vec<u8> {
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        let mut j = json!({
            "asset": {"version": "2.0", "generator": generator},
            "scene": 0, "scenes": [{"nodes": self.roots}],
            "nodes": self.nodes, "meshes": self.meshes, "accessors": self.accessors, "bufferViews": self.views,
            "buffers": [{"byteLength": self.bin.len()}],
        });
        if !self.materials.is_empty() {
            j["materials"] = json!(self.materials);
        }
        if !self.asset_extras.is_null() {
            j["asset"]["extras"] = self.asset_extras.clone();
        }
        let mut js = serde_json::to_vec(&j).unwrap();
        while js.len() % 4 != 0 {
            js.push(b' ');
        }
        let total = 12 + 8 + js.len() + 8 + self.bin.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(js.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&js);
        out.extend_from_slice(&(self.bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&self.bin);
        out
    }
}

// ---------------------------------------------------------------- 读取

#[derive(Debug, Clone)]
pub struct SceneNode {
    pub name: String,
    pub parent: Option<usize>,
    pub mesh: Option<usize>,
    pub local: M4,
    /// 世界矩阵(glTF 列向量约定)
    pub world: M4,
    pub extras: Value,
}

#[derive(Debug, Clone)]
pub struct SceneMesh {
    pub name: String,
    pub prims: Vec<ScenePrim>,
}

#[derive(Debug, Clone)]
pub struct Scene {
    pub json: Value,
    pub nodes: Vec<SceneNode>,
    pub meshes: Vec<SceneMesh>,
}

fn quat_mat(q: [f64; 4]) -> M4 {
    let [x, y, z, w] = q;
    [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w), 0.0],
        [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w), 0.0],
        [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y), 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn node_local(n: &Value) -> M4 {
    if let Some(m) = n["matrix"].as_array() {
        let v: Vec<f64> = m.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect();
        if v.len() == 16 {
            let mut r = IDENTITY;
            for c in 0..4 {
                for rr in 0..4 {
                    r[rr][c] = v[c * 4 + rr];
                }
            }
            return r;
        }
    }
    let f = |k: &str, d: Vec<f64>| -> Vec<f64> { n[k].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or(d) };
    let t = f("translation", vec![0.0; 3]);
    let r = f("rotation", vec![0.0, 0.0, 0.0, 1.0]);
    let s = f("scale", vec![1.0; 3]);
    let mut tm = IDENTITY;
    tm[0][3] = t[0];
    tm[1][3] = t[1];
    tm[2][3] = t[2];
    let rm = quat_mat([r[0], r[1], r[2], r[3]]);
    let mut sm = IDENTITY;
    sm[0][0] = s[0];
    sm[1][1] = s[1];
    sm[2][2] = s[2];
    mat_mul(&mat_mul(&tm, &rm), &sm)
}

pub fn xform_gltf_point(m: &M4, p: [f32; 3]) -> [f32; 3] {
    let p = [p[0] as f64, p[1] as f64, p[2] as f64];
    std::array::from_fn(|i| (m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3]) as f32)
}

pub fn read_scene(d: &[u8]) -> Result<Scene> {
    let (j, prims) = crate::glb::read_glb(d)?;
    let mut meshes: Vec<SceneMesh> =
        j["meshes"].as_array().cloned().unwrap_or_default().iter().map(|m| SceneMesh { name: m["name"].as_str().unwrap_or("").to_string(), prims: vec![] }).collect();
    for p in prims {
        let pj = &j["meshes"][p.mesh]["primitives"][p.prim];
        let idx = p.indices.clone().unwrap_or_else(|| (0..p.positions.len() as u32).collect());
        meshes[p.mesh].prims.push(ScenePrim {
            positions: p.positions,
            normals: p.normals,
            uvs: p.uvs,
            indices: idx,
            material: p.material,
            material_index: pj["material"].as_u64().map(|x| x as usize),
            extras: pj["extras"].clone(),
        });
    }
    let jn = j["nodes"].as_array().cloned().unwrap_or_default();
    let mut parent = vec![None; jn.len()];
    for (i, n) in jn.iter().enumerate() {
        for c in n["children"].as_array().cloned().unwrap_or_default() {
            if let Some(c) = c.as_u64() {
                if (c as usize) < jn.len() {
                    parent[c as usize] = Some(i);
                }
            }
        }
    }
    let locals: Vec<M4> = jn.iter().map(node_local).collect();
    let mut nodes = Vec::with_capacity(jn.len());
    for (i, n) in jn.iter().enumerate() {
        let mut w = locals[i];
        let mut p = parent[i];
        let mut guard = 0;
        while let Some(pi) = p {
            w = mat_mul(&locals[pi], &w);
            p = parent[pi];
            guard += 1;
            if guard > 1000 {
                return Err(Error::format("glb", "节点层级有环"));
            }
        }
        nodes.push(SceneNode {
            name: n["name"].as_str().unwrap_or("").to_string(),
            parent: parent[i],
            mesh: n["mesh"].as_u64().map(|x| x as usize),
            local: locals[i],
            world: w,
            extras: n["extras"].clone(),
        });
    }
    Ok(Scene { json: j, nodes, meshes })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn convention_roundtrip() {
        let m = crate::visual::compose([1.0, 2.0, 3.0], [0.0, 30.0, 0.0], [1.0; 3]);
        let n = game_to_gltf(&m);
        let back = gltf_to_game(&n);
        for i in 0..4 {
            for k in 0..4 {
                assert!((back[i][k] - m[i][k]).abs() < 1e-12);
            }
        }
        // 游戏点经 M 变换再转 glTF == 先转 glTF 再经 N
        let p = [0.5f32, -1.0, 2.0];
        let g1 = pos_game_to_gltf(crate::visual::xform_point(&m, p.map(|x| x as f64)).map(|x| x as f32));
        let g2 = xform_gltf_point(&n, pos_game_to_gltf(p));
        for k in 0..3 {
            assert!((g1[k] - g2[k]).abs() < 1e-5);
        }
    }
    #[test]
    fn write_read() {
        let mut b = SceneBuilder::new();
        let me = b.add_mesh("m", &[ScenePrim { positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], indices: vec![0, 1, 2], material: Some("A".into()), extras: json!({"renderSet": "X"}), ..Default::default() }]);
        let mut t = IDENTITY;
        t[0][3] = 5.0;
        let root = b.add_node("root", None, Some(&t), Value::Null, None);
        b.add_node("child", Some(me), Some(&t), json!({"hp": "HP_1"}), Some(root));
        let s = read_scene(&b.write("test")).unwrap();
        assert_eq!(s.nodes[1].world[0][3], 10.0);
        assert_eq!(s.meshes[0].prims[0].extras["renderSet"], "X");
        assert_eq!(s.meshes[0].prims[0].material.as_deref(), Some("A"));
    }
}
