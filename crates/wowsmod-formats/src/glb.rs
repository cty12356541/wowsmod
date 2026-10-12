//! 最小 glTF 2.0 二进制 (GLB) 写入与读取(读取只为健康检查: POSITION + indices)。
use serde_json::{json, Value};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct MeshPrim {
    pub name: String,
    pub material: Option<String>,
    /// glTF 坐标(右手系)
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub uvs: Option<Vec<[f32; 2]>>,
    pub indices: Vec<u32>,
    pub extras: Value,
}

fn pad4(buf: &mut Vec<u8>, fill: u8) {
    while buf.len() % 4 != 0 {
        buf.push(fill);
    }
}

/// 写出 GLB: 每个 MeshPrim 一个 mesh + 一个 node; 同名材质合并。
pub fn write_glb(prims: &[MeshPrim], asset_extras: Value) -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();
    let mut materials: Vec<Value> = Vec::new();
    let mut mat_index = std::collections::HashMap::<String, usize>::new();

    let mut add_view = |bin: &mut Vec<u8>, data: &[u8], target: u32| -> usize {
        pad4(bin, 0);
        let off = bin.len();
        bin.extend_from_slice(data);
        views.push(json!({"buffer": 0, "byteOffset": off, "byteLength": data.len(), "target": target}));
        views.len() - 1
    };

    for p in prims {
        let mut attrs = serde_json::Map::new();
        // POSITION
        let mut mn = [f32::INFINITY; 3];
        let mut mx = [f32::NEG_INFINITY; 3];
        let mut raw = Vec::with_capacity(p.positions.len() * 12);
        for v in &p.positions {
            for k in 0..3 {
                mn[k] = mn[k].min(v[k]);
                mx[k] = mx[k].max(v[k]);
                raw.extend_from_slice(&v[k].to_le_bytes());
            }
        }
        if p.positions.is_empty() {
            mn = [0.0; 3];
            mx = [0.0; 3];
        }
        let bv = add_view(&mut bin, &raw, 34962);
        accessors.push(json!({"bufferView": bv, "componentType": 5126, "count": p.positions.len(), "type": "VEC3", "min": mn, "max": mx}));
        attrs.insert("POSITION".into(), json!(accessors.len() - 1));
        if let Some(ns) = &p.normals {
            let mut raw = Vec::with_capacity(ns.len() * 12);
            for v in ns {
                for c in v {
                    raw.extend_from_slice(&c.to_le_bytes());
                }
            }
            let bv = add_view(&mut bin, &raw, 34962);
            accessors.push(json!({"bufferView": bv, "componentType": 5126, "count": ns.len(), "type": "VEC3"}));
            attrs.insert("NORMAL".into(), json!(accessors.len() - 1));
        }
        if let Some(uv) = &p.uvs {
            let mut raw = Vec::with_capacity(uv.len() * 8);
            for v in uv {
                for c in v {
                    raw.extend_from_slice(&c.to_le_bytes());
                }
            }
            let bv = add_view(&mut bin, &raw, 34962);
            accessors.push(json!({"bufferView": bv, "componentType": 5126, "count": uv.len(), "type": "VEC2"}));
            attrs.insert("TEXCOORD_0".into(), json!(accessors.len() - 1));
        }
        // indices
        let max_i = p.indices.iter().copied().max().unwrap_or(0);
        let (ct, raw): (u32, Vec<u8>) = if max_i < 65535 {
            (5123, p.indices.iter().flat_map(|&i| (i as u16).to_le_bytes()).collect())
        } else {
            (5125, p.indices.iter().flat_map(|&i| i.to_le_bytes()).collect())
        };
        let bv = add_view(&mut bin, &raw, 34963);
        accessors.push(json!({"bufferView": bv, "componentType": ct, "count": p.indices.len(), "type": "SCALAR"}));
        let ia = accessors.len() - 1;
        let mut prim = json!({"attributes": attrs, "indices": ia, "mode": 4});
        if let Some(m) = &p.material {
            let n = materials.len();
            let mi = *mat_index.entry(m.clone()).or_insert_with(|| {
                materials.push(json!({"name": m, "pbrMetallicRoughness": {"baseColorFactor": [0.8, 0.8, 0.8, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.8}, "doubleSided": false}));
                n
            });
            prim["material"] = json!(mi);
        }
        meshes.push(json!({"name": p.name, "primitives": [prim], "extras": p.extras}));
        nodes.push(json!({"name": p.name, "mesh": meshes.len() - 1}));
    }
    pad4(&mut bin, 0);
    let node_ids: Vec<usize> = (0..nodes.len()).collect();
    let mut doc = json!({
        "asset": {"version": "2.0", "generator": format!("wowsmod {}", env!("CARGO_PKG_VERSION")), "extras": asset_extras},
        "scene": 0,
        "scenes": [{"nodes": node_ids}],
        "nodes": nodes,
        "meshes": meshes,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": bin.len()}],
    });
    if !materials.is_empty() {
        doc["materials"] = json!(materials);
    }
    let mut js = serde_json::to_vec(&doc).unwrap();
    pad4(&mut js, b' ');
    let total = 12 + 8 + js.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&js);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}

/// 从 GLB 读出的一个三角形 primitive(仅用于检查)。
#[derive(Debug, Clone)]
pub struct ReadPrim {
    pub mesh: usize,
    pub prim: usize,
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub uvs: Option<Vec<[f32; 2]>>,
    pub material: Option<String>,
    pub indices: Option<Vec<u32>>,
}

pub fn read_glb(d: &[u8]) -> Result<(Value, Vec<ReadPrim>)> {
    if d.len() < 20 || &d[0..4] != b"glTF" {
        return Err(Error::format("glb", "不是 GLB 文件(缺少 glTF 魔数)"));
    }
    let jl = u32::from_le_bytes(d[12..16].try_into().unwrap()) as usize;
    if &d[16..20] != b"JSON" || 20 + jl > d.len() {
        return Err(Error::format("glb", "JSON chunk 损坏"));
    }
    let j: Value = serde_json::from_slice(&d[20..20 + jl]).map_err(|e| Error::format("glb", e.to_string()))?;
    let mut bin: &[u8] = &[];
    let bo = 20 + jl;
    if bo + 8 <= d.len() {
        let bl = u32::from_le_bytes(d[bo..bo + 4].try_into().unwrap()) as usize;
        if &d[bo + 4..bo + 8] == b"BIN\0" && bo + 8 + bl <= d.len() {
            bin = &d[bo + 8..bo + 8 + bl];
        }
    }
    let acc = |i: &Value| -> Result<(usize, u64, usize, usize, usize)> {
        // (count, componentType, byteOffset_abs, stride, comps)
        let a = &j["accessors"][i.as_u64().ok_or_else(|| Error::format("glb", "accessor 索引无效"))? as usize];
        let count = a["count"].as_u64().unwrap_or(0) as usize;
        let ct = a["componentType"].as_u64().unwrap_or(0);
        let comps = match a["type"].as_str().unwrap_or("") {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            t => return Err(Error::Unsupported(format!("accessor type {t}"))),
        };
        let csize = match ct {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return Err(Error::Unsupported(format!("componentType {ct}"))),
        };
        if a.get("sparse").is_some() {
            return Err(Error::Unsupported("sparse accessor".into()));
        }
        let bvi = a["bufferView"].as_u64().ok_or_else(|| Error::Unsupported("accessor 没有 bufferView".into()))? as usize;
        let bv = &j["bufferViews"][bvi];
        if bv["buffer"].as_u64().unwrap_or(0) != 0 {
            return Err(Error::Unsupported("只支持 GLB 内嵌 buffer 0".into()));
        }
        let off = bv["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let stride = bv["byteStride"].as_u64().map(|s| s as usize).unwrap_or(csize * comps);
        Ok((count, ct, off, stride, comps))
    };
    let mut prims = Vec::new();
    for (mi, m) in j["meshes"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        for (pi, p) in m["primitives"].as_array().cloned().unwrap_or_default().iter().enumerate() {
            if p["mode"].as_u64().unwrap_or(4) != 4 {
                continue;
            }
            let (n, ct, off, stride, comps) = acc(&p["attributes"]["POSITION"])?;
            if ct != 5126 || comps != 3 {
                return Err(Error::Unsupported("POSITION 不是 float VEC3".into()));
            }
            let mut positions = Vec::with_capacity(n);
            for i in 0..n {
                let o = off + i * stride;
                let s = crate::util::slice(bin, o, 12, "glb POSITION")?;
                positions.push([
                    f32::from_le_bytes(s[0..4].try_into().unwrap()),
                    f32::from_le_bytes(s[4..8].try_into().unwrap()),
                    f32::from_le_bytes(s[8..12].try_into().unwrap()),
                ]);
            }
            let normals = if p["attributes"].get("NORMAL").is_some() {
                let (nn, ct, off, stride, comps) = acc(&p["attributes"]["NORMAL"])?;
                if ct != 5126 || comps != 3 || nn != n {
                    None
                } else {
                    let mut v = Vec::with_capacity(nn);
                    for i in 0..nn {
                        let s = crate::util::slice(bin, off + i * stride, 12, "glb NORMAL")?;
                        v.push([
                            f32::from_le_bytes(s[0..4].try_into().unwrap()),
                            f32::from_le_bytes(s[4..8].try_into().unwrap()),
                            f32::from_le_bytes(s[8..12].try_into().unwrap()),
                        ]);
                    }
                    Some(v)
                }
            } else {
                None
            };
            let uvs = if p["attributes"].get("TEXCOORD_0").is_some() {
                let (nn, ct, off, stride, comps) = acc(&p["attributes"]["TEXCOORD_0"])?;
                if ct != 5126 || comps != 2 || nn != n {
                    None
                } else {
                    let mut v = Vec::with_capacity(nn);
                    for i in 0..nn {
                        let s = crate::util::slice(bin, off + i * stride, 8, "glb TEXCOORD_0")?;
                        v.push([f32::from_le_bytes(s[0..4].try_into().unwrap()), f32::from_le_bytes(s[4..8].try_into().unwrap())]);
                    }
                    Some(v)
                }
            } else {
                None
            };
            let material = p["material"].as_u64().and_then(|mi| j["materials"][mi as usize]["name"].as_str().map(|s| s.to_string()));
            let indices = if p.get("indices").is_some() {
                let (n, ct, off, stride, _) = acc(&p["indices"])?;
                let mut v = Vec::with_capacity(n);
                for i in 0..n {
                    let o = off + i * stride;
                    v.push(match ct {
                        5121 => crate::util::slice(bin, o, 1, "glb indices")?[0] as u32,
                        5123 => u16::from_le_bytes(crate::util::slice(bin, o, 2, "glb indices")?.try_into().unwrap()) as u32,
                        5125 => u32::from_le_bytes(crate::util::slice(bin, o, 4, "glb indices")?.try_into().unwrap()),
                        _ => return Err(Error::Unsupported(format!("索引 componentType {ct}"))),
                    });
                }
                Some(v)
            } else {
                None
            };
            prims.push(ReadPrim { mesh: mi, prim: pi, name: m["name"].as_str().unwrap_or("").to_string(), positions, normals, uvs, material, indices });
        }
    }
    Ok((j, prims))
}
