//! 顶点池 ↔ 索引 配对。
//!
//! geometry 的 vmap / imap 数组顺序**不**两两对应(M0 的 glb 破面根源), 且一个 geometry 往往同时
//! 含主模型和 LOD 的 mapping。正确的配对来源, 按优先级:
//! 1. **assets.bin VisualPrototype 渲染集**: 每个 RenderSet 直接给出 (vertices mapping id, indices mapping id, 材质)。
//! 2. **.visual XML 名字**: `<vertices>XShape.vertices</vertices>` / `<primitive>XShape.indices</primitive>`,
//!    mapping id = murmur3_32(该字符串, seed 0)(M1 发现, 16/16 验证)。
//! 3. **启发式兜底**(仅当以上都没有时, 会给出警告): 对每个索引 mapping, 在 texel density 相同、
//!    顶点数 > 最大局部索引的顶点 mapping 中选择; 有歧义时标记为"不确定"。
use crate::geometry::{Geometry, MappingEntry};
use crate::murmur3::murmur3_32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PairSource {
    AssetsBin,
    VisualXml,
    Heuristic,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Pair {
    pub name: String,
    pub material: Option<String>,
    pub vertices: MappingEntry,
    pub indices: MappingEntry,
    pub source: PairSource,
    /// 启发式有多个候选时为 true
    pub ambiguous: bool,
    /// 来自 assets.bin 的渲染集包围盒(引擎坐标), 用于校验
    pub expected_bbox: Option<([f32; 3], [f32; 3])>,
}

/// 一个渲染集引用: (顶点 mapping id, 索引 mapping id, 名字, 材质, 期望包围盒)
#[derive(Debug, Clone)]
pub struct RenderSetRef {
    pub vertices: u32,
    pub indices: u32,
    pub name: String,
    pub material: Option<String>,
    pub bbox: Option<([f32; 3], [f32; 3])>,
}

impl RenderSetRef {
    pub fn from_assets(rs: &crate::assets_bin::RenderSet, name: String) -> Self {
        RenderSetRef {
            vertices: rs.vertices_mapping_id,
            indices: rs.indices_mapping_id,
            name,
            material: rs.material.clone(),
            bbox: Some((rs.bbox_min, rs.bbox_max)),
        }
    }
}

#[derive(Debug, Default, serde::Serialize)]
pub struct PairingResult {
    pub pairs: Vec<Pair>,
    pub warnings: Vec<String>,
}

/// 由 (vmap id, imap id, 名字, 材质) 列表配对。
pub fn pair_by_ids(g: &Geometry, sets: &[RenderSetRef], source: PairSource) -> PairingResult {
    let mut r = PairingResult::default();
    for RenderSetRef { vertices: v, indices: i, name, material: mat, bbox } in sets {
        match (g.vertex_mapping(*v), g.index_mapping(*i)) {
            (Some(vm), Some(im)) => r.pairs.push(Pair {
                name: name.clone(),
                material: mat.clone(),
                vertices: *vm,
                indices: *im,
                source,
                ambiguous: false,
                expected_bbox: *bbox,
            }),
            (vm, im) => r.warnings.push(format!(
                "{name}: mapping 在 geometry 中找不到 (vertices {v:#010x} {}, indices {i:#010x} {})",
                if vm.is_some() { "有" } else { "缺" },
                if im.is_some() { "有" } else { "缺" }
            )),
        }
    }
    r
}

/// 从 .visual XML 文本中提取 renderSet 的 (vertices, primitive, 材质 identifier)。
pub fn render_sets_from_visual_xml(xml: &str) -> Vec<RenderSetRef> {
    fn tag<'a>(s: &'a str, t: &str) -> Option<&'a str> {
        let open = format!("<{t}>");
        let close = format!("</{t}>");
        let a = s.find(&open)? + open.len();
        let b = s[a..].find(&close)? + a;
        Some(s[a..b].trim())
    }
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(p) = rest.find("<renderSet>") {
        let after = &rest[p..];
        let end = after.find("</renderSet>").map(|e| e + "</renderSet>".len()).unwrap_or(after.len());
        let block = &after[..end];
        if let (Some(v), Some(i)) = (tag(block, "vertices"), tag(block, "primitive")) {
            let mat = tag(block, "identifier").map(|s| s.to_string());
            let name = v.trim_end_matches(".vertices").trim_end_matches("Shape").to_string();
            out.push(RenderSetRef { vertices: murmur3_32(v.as_bytes(), 0), indices: murmur3_32(i.as_bytes(), 0), name, material: mat, bbox: None });
        }
        rest = &after[end..];
    }
    out
}

/// 启发式兜底配对(仅在没有 assets.bin / .visual 时使用)。
pub fn pair_heuristic(g: &Geometry, max_local_index: &dyn Fn(&MappingEntry) -> u32) -> PairingResult {
    let mut r = PairingResult::default();
    r.warnings.push("没有 assets.bin / .visual 配对信息, 使用启发式配对(texel density + 顶点数), 结果可能不准".into());
    let mut used = vec![false; g.vertex_mappings.len()];
    for (k, im) in g.index_mappings.iter().enumerate() {
        let need = max_local_index(im);
        let cands: Vec<usize> = g
            .vertex_mappings
            .iter()
            .enumerate()
            .filter(|(_, vm)| vm.packed_texel_density == im.packed_texel_density && vm.items_count > need)
            .map(|(j, _)| j)
            .collect();
        let pick = cands.iter().copied().find(|&j| !used[j]).or(cands.first().copied());
        match pick {
            Some(j) => {
                used[j] = true;
                r.pairs.push(Pair {
                    name: format!("heuristic_{k}"),
                    material: None,
                    vertices: g.vertex_mappings[j],
                    indices: *im,
                    source: PairSource::Heuristic,
                    ambiguous: cands.len() > 1,
                    expected_bbox: None,
                });
            }
            None => r.warnings.push(format!("索引 mapping {:#010x} 找不到可用的顶点 mapping", im.id)),
        }
    }
    r
}
