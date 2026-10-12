//! `content/assets.bin` (PrototypeDatabase) 的最小读取器: 路径表、selfId→记录 查找、VisualPrototype 渲染集。
//!
//! 布局由 <WORK_DIR>\tools\assets_bin.py 与 m0\skins\visual_dump.py 移植(那两份 Python
//! 又部分参考了 landaire/wows-toolkit `models/assets_bin.rs`, MIT)。字段含义标"?"者为推测。
//!
//! ```text
//! 0x00 u32 magic=0x42574442 ("BDWB")  u32 version  u32 checksum  u16 arch  u16 endian
//! 0x10 body:
//!   +0x00 strings (0x28): u32 om_cap, ., i32 om_buckets_rel(+8), ., i32 om_values_rel(+16), ., u32 str_size(+24), ., i32 str_rel(+32)
//!   +0x28 r2p     (0x18): u32 cap, ., i32 buckets_rel(+8), ., i32 values_rel(+16)   (相对 r2p 段起点)
//!   +0x40 paths   (0x10): u32 count, ., i32 rel(+8)                                (相对 paths 段起点)
//!   +0x50 dbs     (0x10): u32 count, ., i32 rel(+8)   ← 实测 rel 不可靠, 改为按 magic 0x480DC57B 扫描定位
//! PathEntry (32B): u64 selfId, u64 parentId, u32 name_size, u32 ?, i32 name_rel(相对 entry+16)
//! r2p: 开放寻址哈希, 桶 16B {u64 key, u64 sentinel}, 值 u32: blob=(v&0xFF)/4, record=v>>8
//! db 条目 (0x18): u32 magic, u32 checksum, u32 size, u32 pad, i32 rel(相对条目) → blob: u64 count, u64 ?, 记录从 +16 开始
//! VisualPrototype (blob 2, 记录 0x40): +0x00 bboxMin f32x3+pad, +0x10 bboxMax, +0x20 geometry selfId u64,
//!   +0x28 primitives selfId u64?, +0x30 renderSetCount u64, +0x38 renderSets rel(相对记录)
//! RenderSet (0x50): +0 vertices mapping id u32, +4 indices mapping id u32, +8 u32 ?, +0xC u32 flags?,
//!   +0x20 material(.mfm) selfId u64, +0x28 u64 ?, +0x30 bboxMin, +0x40 bboxMax
//! ```
use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::util::*;

pub const ASSETS_MAGIC: u32 = 0x4257_4442;
pub const DB_MAGIC: u32 = 0x480D_C57B;
pub const VISUAL_BLOB: usize = 2;
pub const VISUAL_RECORD_SIZE: usize = 0x40;
pub const RENDER_SET_SIZE: usize = 0x50;

#[derive(Debug, Clone)]
pub struct PathEntry {
    pub self_id: u64,
    pub parent_id: u64,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct DbBlob {
    pub magic: u32,
    pub offset: usize,
    pub size: usize,
}

pub struct AssetsBin {
    data: Vec<u8>,
    pub paths: Vec<PathEntry>,
    by_id: HashMap<u64, usize>,
    r2p_cap: usize,
    r2p_buckets: usize,
    r2p_values: usize,
    pub dbs: Vec<Option<DbBlob>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RenderSet {
    pub vertices_mapping_id: u32,
    pub indices_mapping_id: u32,
    pub material_id: u64,
    pub material: Option<String>,
    /// 渲染集包围盒(引擎坐标), 用于独立校验配对: 正确配对时被索引引用的顶点包围盒应与之一致
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VisualInfo {
    pub path: String,
    pub geometry_id: u64,
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    pub render_sets: Vec<RenderSet>,
}

impl AssetsBin {
    pub fn parse(data: Vec<u8>) -> Result<AssetsBin> {
        let d = &data;
        let magic = u32le(d, 0, "assets.bin header")?;
        if magic != ASSETS_MAGIC {
            return Err(Error::format("assets.bin", format!("magic {magic:#x} != 0x42574442")));
        }
        let b = 16usize;
        let b2 = b + 0x28;
        let r2p_cap = u32le(d, b2, "assets r2p")? as usize;
        let r2p_buckets = rel(b2, i32le(d, b2 + 8, "assets r2p")? as i64, "r2p buckets")?;
        let r2p_values = rel(b2, i32le(d, b2 + 16, "assets r2p")? as i64, "r2p values")?;
        slice(d, r2p_buckets, r2p_cap * 16, "r2p buckets")?;
        slice(d, r2p_values, r2p_cap * 4, "r2p values")?;
        let b3 = b2 + 0x18;
        let paths_count = u32le(d, b3, "assets paths")? as usize;
        let pbase = rel(b3, i32le(d, b3 + 8, "assets paths")? as i64, "paths")?;
        let b4 = b3 + 0x10;
        let db_count = u32le(d, b4, "assets dbs")? as usize;

        let mut paths = Vec::with_capacity(paths_count);
        for i in 0..paths_count {
            let e = pbase + i * 32;
            let self_id = u64le(d, e, "path entry")?;
            let parent_id = u64le(d, e + 8, "path entry")?;
            let nsize = u32le(d, e + 16, "path entry")? as usize;
            let nrel = i32le(d, e + 24, "path entry")? as i64;
            let mut name = String::new();
            if nsize > 0 && nsize < 4096 {
                let off = rel(e + 16, nrel, "path name")?;
                let raw = slice(d, off, nsize, "path name")?;
                let raw = raw.strip_suffix(&[0]).unwrap_or(raw);
                name = String::from_utf8_lossy(raw).into_owned();
            }
            paths.push(PathEntry { self_id, parent_id, name });
        }
        let by_id = paths.iter().enumerate().map(|(i, p)| (p.self_id, i)).collect();

        // 数据库数组: 扫描 magic, 再向前回退找到连续 12 个合法条目的起点(与 Python 原型一致)
        let mpos = find_u32(d, DB_MAGIC).ok_or_else(|| Error::format("assets.bin", "找不到数据库 magic 0x480DC57B"))?;
        let mut arr = None;
        let lo = mpos.saturating_sub(0x400);
        let mut back = mpos.saturating_sub(2 * 0x18);
        while back > lo {
            let ok = (0..12).all(|k| {
                let e = back + k * 0x18;
                if e + 0x18 > d.len() {
                    return false;
                }
                let size = u32::from_le_bytes(d[e + 8..e + 12].try_into().unwrap()) as usize;
                let pad = u32::from_le_bytes(d[e + 12..e + 16].try_into().unwrap());
                let r = i32::from_le_bytes(d[e + 16..e + 20].try_into().unwrap());
                !(pad != 0 || size > d.len() || r < 0 || (size != 0 && r == 0))
            });
            if ok {
                arr = Some(back);
                break;
            }
            back -= 4;
        }
        let arr = arr.ok_or_else(|| Error::format("assets.bin", "数据库数组定位失败"))?;
        let mut dbs = Vec::with_capacity(db_count);
        for i in 0..db_count {
            let e = arr + i * 0x18;
            let magic = u32le(d, e, "db entry")?;
            let size = u32le(d, e + 8, "db entry")? as usize;
            let r = i32le(d, e + 16, "db entry")? as i64;
            if size == 0 || r == 0 {
                dbs.push(None);
                continue;
            }
            let offset = rel(e, r, "db blob")?;
            slice(d, offset, size, "db blob")?;
            dbs.push(Some(DbBlob { magic, offset, size }));
        }
        Ok(AssetsBin { data, paths, by_id, r2p_cap, r2p_buckets, r2p_values, dbs })
    }

    pub fn full_path(&self, idx: usize) -> String {
        let mut parts = Vec::new();
        let mut cur = idx;
        for _ in 0..128 {
            let p = &self.paths[cur];
            if !p.name.is_empty() {
                parts.push(p.name.as_str());
            }
            if p.parent_id == 0 {
                break;
            }
            match self.by_id.get(&p.parent_id) {
                Some(&j) => cur = j,
                None => break,
            }
        }
        parts.reverse();
        parts.join("/")
    }

    pub fn name_of(&self, self_id: u64) -> Option<String> {
        self.by_id.get(&self_id).map(|&i| self.full_path(i))
    }

    /// 按完整虚拟路径查 selfId(大小写不敏感)。
    pub fn find_path(&self, path: &str) -> Option<u64> {
        let path = path.replace('\\', "/");
        let path = path.trim_start_matches('/');
        let fname = path.rsplit('/').next()?;
        self.paths
            .iter()
            .enumerate()
            .filter(|(_, p)| p.name.eq_ignore_ascii_case(fname))
            .find(|(i, _)| self.full_path(*i).eq_ignore_ascii_case(path))
            .map(|(_, p)| p.self_id)
    }

    /// 按文件名查找完整路径(大小写不敏感), 用于本地文件 → 游戏内路径的推断。
    pub fn find_by_file_name(&self, name: &str) -> Vec<String> {
        self.paths
            .iter()
            .enumerate()
            .filter(|(_, p)| p.name.eq_ignore_ascii_case(name))
            .map(|(i, _)| self.full_path(i))
            .collect()
    }

    /// selfId → (blob 序号, 记录序号)
    pub fn r2p(&self, self_id: u64) -> Option<(usize, usize)> {
        let cap = self.r2p_cap;
        if cap == 0 {
            return None;
        }
        let d = &self.data;
        let start = (self_id % cap as u64) as usize;
        for probe in 0..cap {
            let s = (start + probe) % cap;
            let o = self.r2p_buckets + s * 16;
            let key = u64::from_le_bytes(d[o..o + 8].try_into().unwrap());
            let sentinel = u64::from_le_bytes(d[o + 8..o + 16].try_into().unwrap());
            if key == 0 && sentinel == 0 {
                return None;
            }
            if key == self_id {
                let v = u32::from_le_bytes(d[self.r2p_values + s * 4..self.r2p_values + s * 4 + 4].try_into().unwrap());
                return Some((((v & 0xFF) / 4) as usize, (v >> 8) as usize));
            }
        }
        None
    }

    fn record(&self, blob: usize, rec: usize, size: usize) -> Result<(usize, &[u8])> {
        let db = self.dbs.get(blob).and_then(|x| x.as_ref()).ok_or_else(|| Error::NotFound(format!("assets.bin blob {blob}")))?;
        let off = db.offset + 16 + rec * size;
        if off + size > db.offset + db.size {
            return Err(Error::Bounds { what: "assets.bin record", offset: off, len: size, total: db.offset + db.size });
        }
        Ok((off, &self.data[off..off + size]))
    }

    /// 读取一个 .visual 的 VisualPrototype。
    pub fn visual(&self, visual_index: usize) -> Result<Option<VisualInfo>> {
        let p = &self.paths[visual_index];
        let Some((blob, rec)) = self.r2p(p.self_id) else { return Ok(None) };
        if blob != VISUAL_BLOB {
            return Ok(None);
        }
        let (off, r) = self.record(blob, rec, VISUAL_RECORD_SIZE)?;
        let f = |o: usize| f32::from_le_bytes(r[o..o + 4].try_into().unwrap());
        let geometry_id = u64::from_le_bytes(r[0x20..0x28].try_into().unwrap());
        let n = u64::from_le_bytes(r[0x30..0x38].try_into().unwrap()) as usize;
        let rs_rel = i64::from_le_bytes(r[0x38..0x40].try_into().unwrap());
        if n > 4096 {
            return Err(Error::format("VisualPrototype", format!("renderSetCount={n} 不合理")));
        }
        let mut render_sets = Vec::with_capacity(n);
        let base = rel(off, rs_rel, "render sets")?;
        for k in 0..n {
            let o = base + k * RENDER_SET_SIZE;
            let rs = slice(&self.data, o, RENDER_SET_SIZE, "render set")?;
            let material_id = u64::from_le_bytes(rs[0x20..0x28].try_into().unwrap());
            let fv = |o: usize| f32::from_le_bytes(rs[o..o + 4].try_into().unwrap());
            render_sets.push(RenderSet {
                vertices_mapping_id: u32::from_le_bytes(rs[0..4].try_into().unwrap()),
                indices_mapping_id: u32::from_le_bytes(rs[4..8].try_into().unwrap()),
                material_id,
                material: self.name_of(material_id),
                bbox_min: [fv(0x30), fv(0x34), fv(0x38)],
                bbox_max: [fv(0x40), fv(0x44), fv(0x48)],
            });
        }
        Ok(Some(VisualInfo {
            path: self.full_path(visual_index),
            geometry_id,
            bbox_min: [f(0), f(4), f(8)],
            bbox_max: [f(0x10), f(0x14), f(0x18)],
            render_sets,
        }))
    }

    /// 找到所有引用该 geometry 的 .visual(包括 LOD visual)。
    pub fn visuals_for_geometry(&self, geometry_path: &str) -> Result<Vec<VisualInfo>> {
        let gid = self
            .find_path(geometry_path)
            .ok_or_else(|| Error::NotFound(format!("assets.bin 中没有路径 {geometry_path}")))?;
        let mut out = Vec::new();
        for (i, p) in self.paths.iter().enumerate() {
            if !p.name.ends_with(".visual") {
                continue;
            }
            if let Some((blob, rec)) = self.r2p(p.self_id) {
                if blob != VISUAL_BLOB {
                    continue;
                }
                let (_, r) = self.record(blob, rec, VISUAL_RECORD_SIZE)?;
                if u64::from_le_bytes(r[0x20..0x28].try_into().unwrap()) == gid {
                    if let Some(v) = self.visual(i)? {
                        out.push(v);
                    }
                }
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }
}

fn find_u32(d: &[u8], v: u32) -> Option<usize> {
    let pat = v.to_le_bytes();
    d.windows(4).position(|w| w == pat)
}
