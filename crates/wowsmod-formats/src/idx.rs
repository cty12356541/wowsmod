//! `.idx` 索引文件解析 (莱服 26.x 使用 V40 = 0x02000000 版本)。
//!
//! 结构参考 landaire/wows-toolkit `crates/wowsunpack/src/data/idx.rs` (MIT, Copyright 2026 Lander Brandt),
//! 此处为不依赖 winnow 的重写, 并在莱服 26.10 的 225 个 idx 上验证。
//!
//! 布局(全部小端):
//! ```text
//! 0x00 u32 magic = 0x50465349 ("ISFP")   u32 version   u32 murmur   u32 arch/endian
//! 0x10 u32 res_count  u32 fi_count  u32 vol_count  u32 unused
//!      u64 res_ptr    u64 fi_ptr    u64 vol_ptr      (三个指针都相对 0x10)
//! Resource(32B): u64 name_len, u64 name_ptr(相对本条目), u64 id, u64 parent_id
//! FileInfo(48B): u64 resource_id, u64 volume_id, u64 offset, u64 compression_info,
//!                u32 size, u32 crc32, u32 unpacked_size, u32 pad
//! Volume(24B):   u64 name_len, u64 name_ptr(相对本条目), u64 volume_id
//! ```
use crate::error::{Error, Result};
use crate::util::*;

pub const IDX_MAGIC: u32 = 0x5046_5349;
pub const VERSION_V40: u32 = 0x0200_0000;
pub const ROOT_PARENT_ID: u64 = 0xdbb1_a1d1_b108_b927;

#[derive(Debug, Clone)]
pub struct Resource {
    pub id: u64,
    pub parent_id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct FileInfo {
    pub resource_id: u64,
    pub volume_id: u64,
    pub offset: u64,
    pub compression_info: u64,
    pub size: u32,
    pub crc32: u32,
    pub unpacked_size: u32,
    pub padding: u32,
}

#[derive(Debug, Clone)]
pub struct Volume {
    pub volume_id: u64,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct IdxFile {
    pub version: u32,
    pub resources: Vec<Resource>,
    pub file_infos: Vec<FileInfo>,
    pub volumes: Vec<Volume>,
}

fn name_at(d: &[u8], entry: usize, len: u64, ptr: u64, what: &'static str) -> Result<String> {
    let off = entry
        .checked_add(ptr as usize)
        .ok_or_else(|| Error::format(what, "名字指针溢出"))?;
    let raw = slice(d, off, len as usize, what)?;
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    Ok(String::from_utf8_lossy(&raw[..end]).into_owned())
}

impl IdxFile {
    pub fn parse(d: &[u8]) -> Result<IdxFile> {
        let magic = u32le(d, 0, "idx header")?;
        if magic != IDX_MAGIC {
            return Err(Error::format("idx", format!("magic 不对: {magic:#x} (应为 0x50465349)")));
        }
        let version = u32le(d, 4, "idx header")?;
        if version != VERSION_V40 {
            return Err(Error::Unsupported(format!(
                "idx 版本 {version:#010x}; 目前只支持莱服 26.x 的 V40 (0x02000000)。如果游戏大版本更新了格式, 请反馈。"
            )));
        }
        let base = 0x10usize;
        let res_count = u32le(d, base, "idx meta")? as usize;
        let fi_count = u32le(d, base + 4, "idx meta")? as usize;
        let vol_count = u32le(d, base + 8, "idx meta")? as usize;
        let res_ptr = base + u64le(d, base + 16, "idx meta")? as usize;
        let fi_ptr = base + u64le(d, base + 24, "idx meta")? as usize;
        let vol_ptr = base + u64le(d, base + 32, "idx meta")? as usize;

        let mut resources = Vec::with_capacity(res_count);
        for i in 0..res_count {
            let e = res_ptr + i * 32;
            let name_len = u64le(d, e, "idx resource")?;
            let name_ptr = u64le(d, e + 8, "idx resource")?;
            let id = u64le(d, e + 16, "idx resource")?;
            let parent_id = u64le(d, e + 24, "idx resource")?;
            let name = name_at(d, e, name_len, name_ptr, "idx resource name")?;
            resources.push(Resource { id, parent_id, name });
        }
        let mut file_infos = Vec::with_capacity(fi_count);
        for i in 0..fi_count {
            let e = fi_ptr + i * 48;
            file_infos.push(FileInfo {
                resource_id: u64le(d, e, "idx fileinfo")?,
                volume_id: u64le(d, e + 8, "idx fileinfo")?,
                offset: u64le(d, e + 16, "idx fileinfo")?,
                compression_info: u64le(d, e + 24, "idx fileinfo")?,
                size: u32le(d, e + 32, "idx fileinfo")?,
                crc32: u32le(d, e + 36, "idx fileinfo")?,
                unpacked_size: u32le(d, e + 40, "idx fileinfo")?,
                padding: u32le(d, e + 44, "idx fileinfo")?,
            });
        }
        let mut volumes = Vec::with_capacity(vol_count);
        for i in 0..vol_count {
            let e = vol_ptr + i * 24;
            let len = u64le(d, e, "idx volume")?;
            let ptr = u64le(d, e + 8, "idx volume")?;
            let volume_id = u64le(d, e + 16, "idx volume")?;
            let mut name = name_at(d, e, len, ptr, "idx volume name")?;
            // 卷名形如 "//.//vehicles_level4_panasia_0001.pkg"
            while let Some(s) = name.strip_prefix("//.//").or_else(|| name.strip_prefix("/")) {
                name = s.to_string();
            }
            volumes.push(Volume { volume_id, name });
        }
        Ok(IdxFile { version, resources, file_infos, volumes })
    }
}
