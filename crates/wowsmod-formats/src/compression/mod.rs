//! pkg 条目解压。
//!
//! 莱服 26.x 的 idx `compression_info` 实测只有两种取值(342,823 个条目全量统计):
//! - `0x0000000_6`, 且 size == unpacked_size  → 原样存储(218,404 个)
//! - `0x7_00000006`                           → 莱服 Oodle 容器(124,419 个)
//!
//! 另外兼容 WG 风格的 raw deflate(高 32 位为 0 且 size != unpacked_size), 以防未来版本出现。
//!
//! 莱服 Oodle 容器布局(M0/M1 逆向, 与 pfsunpack 输出逐字节一致; 字段含义标"?"者为推测):
//! ```text
//! +0x00 u64 header_len (= 0x30 + 4*block_count; 单块时 0x34)
//! +0x08 u32 compressor  (Oodle 枚举: 8=Kraken 9=Mermaid 11=Selkie 12=Hydra 13=Leviathan)
//! +0x0C u32 (=1, 版本?)
//! +0x10 u64 unpacked_size
//! +0x18 u64 packed_size  (= 所有块压缩大小之和)
//! +0x20 u32 block_count  (每块解压后 block_size 字节, 末块可短)
//! +0x24 u32 block_size   (=0x40000 = 256KiB)
//! +0x28..0x38  0
//! +0x38 block_count 个 u32: 每块压缩大小(之和 = packed_size)
//! +0x38+4*block_count: 连续的 Oodle 块(每块以 0x8C/0x0C 块头开始), 总长 packed_size
//! 条目总大小 = 0x38 + 4*block_count + packed_size (在 342k 条目上验证, 见 M1_REPORT)
//! ```
//! M0 原型只处理过单块条目(块表恰好在 0x38、流在 0x3C), 误以为 +0x0C 是块数;
//! M1 在多块条目(>256KiB)上发现并修正: 块数在 +0x20, 块表从 0x38 开始。
//!
//! **Oodle 后端替换点**: trait [`OodleBackend`]。本仓库不内置任何实现;
//! 调用 [`set_oodle_backend`] 注册其它实现即可, 其它代码无需改动。

use std::sync::OnceLock;

use crate::error::{Error, Result};
use crate::util::*;

/// Oodle 原始 LZ 流解码器接口。`input` 是去掉莱服容器头后的 Oodle 流,
/// `output` 长度即期望的解压大小; 返回实际写入字节数。
pub trait OodleBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn decompress(&self, input: &[u8], output: &mut [u8]) -> std::result::Result<usize, String>;
}

static BACKEND: OnceLock<Box<dyn OodleBackend>> = OnceLock::new();

/// 注册自定义 Oodle 后端(只能在第一次解压前调用一次)。返回是否注册成功。
pub fn set_oodle_backend(b: Box<dyn OodleBackend>) -> bool {
    BACKEND.set(b).is_ok()
}

/// 延迟加载器: 第一次需要 Oodle 时调用一次(例如运行时加载用户游戏自带的 Oodle 库)。
static LOADER: OnceLock<fn() -> Option<Box<dyn OodleBackend>>> = OnceLock::new();

/// 注册延迟加载器(不会立即执行)。返回是否注册成功。
pub fn set_oodle_loader(f: fn() -> Option<Box<dyn OodleBackend>>) -> bool {
    LOADER.set(f).is_ok()
}

pub fn oodle_backend() -> Option<&'static dyn OodleBackend> {
    if BACKEND.get().is_none() {
        if let Some(f) = LOADER.get() {
            if let Some(b) = f() {
                let _ = BACKEND.set(b);
            }
        }
    }
    BACKEND.get().map(|b| b.as_ref())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Method {
    Stored,
    LestaOodle,
    Deflate,
}

pub const LESTA_OODLE_CINFO: u64 = 0x7_0000_0006;

pub fn method_for(cinfo: u64, size: u32, unpacked: u32) -> Method {
    if cinfo >> 32 == 7 {
        Method::LestaOodle
    } else if size == unpacked {
        Method::Stored
    } else {
        Method::Deflate
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LestaOodleHeader {
    pub header_len: u64,
    pub compressor: u32,
    pub unknown_0c: u32,
    pub unpacked_size: u64,
    pub packed_size: u64,
    pub block_count: u32,
    pub block_size: u32,
    /// 每块压缩大小
    pub block_sizes: Vec<u32>,
    /// Oodle 流在条目内的起始偏移
    pub stream_offset: usize,
}

pub fn compressor_name(id: u32) -> &'static str {
    match id {
        8 => "Kraken",
        9 => "Mermaid",
        11 => "Selkie",
        12 => "Hydra",
        13 => "Leviathan",
        _ => "unknown",
    }
}

pub fn parse_lesta_header(d: &[u8]) -> Result<LestaOodleHeader> {
    let header_len = u64le(d, 0, "lesta oodle header")?;
    let block_count = u32le(d, 0x20, "lesta oodle header")?;
    let block_size = u32le(d, 0x24, "lesta oodle header")?;
    let unpacked_size = u64le(d, 16, "lesta oodle header")?;
    let packed_size = u64le(d, 24, "lesta oodle header")?;
    if block_size == 0 || block_count as u64 != unpacked_size.div_ceil(block_size as u64).max(1) {
        return Err(Error::format(
            "lesta oodle header",
            format!("块数 {block_count} 与 解压大小 {unpacked_size}/块大小 {block_size} 不符"),
        ));
    }
    if header_len != 0x30 + 4 * block_count as u64 {
        return Err(Error::format(
            "lesta oodle header",
            format!("header_len={header_len:#x}, 按块数 {block_count} 应为 {:#x}; 可能是新的容器版本", 0x30 + 4 * block_count),
        ));
    }
    let table = 0x38usize;
    let mut block_sizes = Vec::with_capacity(block_count as usize);
    for i in 0..block_count as usize {
        block_sizes.push(u32le(d, table + 4 * i, "lesta oodle block table")?);
    }
    let sum: u64 = block_sizes.iter().map(|&x| x as u64).sum();
    if sum != packed_size {
        return Err(Error::format("lesta oodle block table", format!("块大小之和 {sum} != packed_size {packed_size}")));
    }
    let stream_offset = table + 4 * block_count as usize;
    if stream_offset as u64 + packed_size != d.len() as u64 {
        return Err(Error::format(
            "lesta oodle container",
            format!("头 {stream_offset} + 压缩流 {packed_size} != 条目大小 {}", d.len()),
        ));
    }
    Ok(LestaOodleHeader {
        header_len,
        compressor: u32le(d, 8, "lesta oodle header")?,
        unknown_0c: u32le(d, 12, "lesta oodle header")?,
        unpacked_size,
        packed_size,
        block_count,
        block_size,
        block_sizes,
        stream_offset,
    })
}

/// 解压一个莱服 Oodle 容器。
pub fn decompress_lesta_oodle(d: &[u8], expected: usize, what: &str) -> Result<Vec<u8>> {
    let h = parse_lesta_header(d)?;
    if h.unpacked_size as usize != expected {
        return Err(Error::Decompress {
            what: what.into(),
            msg: format!("容器头解压大小 {} 与 idx 记录 {} 不一致", h.unpacked_size, expected),
        });
    }
    let backend = oodle_backend().ok_or(Error::NoOodleBackend)?;
    let mut out = vec![0u8; expected];
    // 每块(256KiB)独立解码: 块表给出每块压缩大小; 实测莱服各块是独立压缩的,
    // 连续流方式解码 3 块以上的条目会失败(参考解码器报错), 逐块解码与 pfsunpack 一致。
    let mut src = h.stream_offset;
    let bs = h.block_size as usize;
    for (i, &csize) in h.block_sizes.iter().enumerate() {
        let dst_start = i * bs;
        let dst_end = (dst_start + bs).min(expected);
        let input = slice(d, src, csize as usize, "lesta oodle block")?;
        let n = backend.decompress(input, &mut out[dst_start..dst_end]).map_err(|e| Error::Decompress {
            what: what.into(),
            msg: format!("块 {i}/{}: {} ({}): {e}", h.block_count, backend.name(), compressor_name(h.compressor)),
        })?;
        if n != dst_end - dst_start {
            return Err(Error::Decompress {
                what: what.into(),
                msg: format!("块 {i} 只解出 {n} 字节, 期望 {}", dst_end - dst_start),
            });
        }
        src += csize as usize;
    }
    Ok(out)
}

/// 按 idx 记录解压一个 pkg 条目。
pub fn decompress_entry(raw: &[u8], cinfo: u64, size: u32, unpacked: u32, what: &str) -> Result<Vec<u8>> {
    match method_for(cinfo, size, unpacked) {
        Method::Stored => Ok(raw.to_vec()),
        Method::LestaOodle => decompress_lesta_oodle(raw, unpacked as usize, what),
        Method::Deflate => {
            let out = miniz_oxide::inflate::decompress_to_vec_with_limit(raw, unpacked as usize)
                .map_err(|e| Error::Decompress { what: what.into(), msg: format!("deflate: {e:?} (cinfo={cinfo:#x})") })?;
            if out.len() != unpacked as usize {
                return Err(Error::Decompress { what: what.into(), msg: format!("deflate 解出 {} 字节, 期望 {unpacked}", out.len()) });
            }
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container(unpacked: u64, blocks: &[u32]) -> Vec<u8> {
        let nb = blocks.len() as u32;
        let packed: u64 = blocks.iter().map(|&b| b as u64).sum();
        let mut d = Vec::new();
        d.extend_from_slice(&(0x30u64 + 4 * nb as u64).to_le_bytes());
        d.extend_from_slice(&8u32.to_le_bytes());
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&unpacked.to_le_bytes());
        d.extend_from_slice(&packed.to_le_bytes());
        d.extend_from_slice(&nb.to_le_bytes());
        d.extend_from_slice(&0x40000u32.to_le_bytes());
        d.resize(0x38, 0);
        for b in blocks {
            d.extend_from_slice(&b.to_le_bytes());
        }
        d.resize(d.len() + packed as usize, 0xAB);
        d
    }

    #[test]
    fn header_multi_block() {
        let d = container(0x40000 * 2 + 5, &[100, 200, 3]);
        let h = parse_lesta_header(&d).unwrap();
        assert_eq!(h.block_count, 3);
        assert_eq!(h.block_sizes, vec![100, 200, 3]);
        assert_eq!(h.stream_offset, 0x38 + 12);
        assert_eq!(h.packed_size, 303);
        assert_eq!(compressor_name(h.compressor), "Kraken");
    }

    #[test]
    fn header_rejects_inconsistent() {
        // 块数与解压大小不符
        assert!(parse_lesta_header(&container(10, &[1, 2])).is_err());
        // 条目被截断
        let mut d = container(0x40000 + 1, &[10, 10]);
        d.pop();
        assert!(parse_lesta_header(&d).is_err());
        // 块大小表之和与 packed 不符
        let mut d = container(0x40000 + 1, &[10, 10]);
        d[0x38] = 11;
        assert!(parse_lesta_header(&d).is_err());
    }

    #[test]
    fn method_selection() {
        assert_eq!(method_for(6, 10, 10), Method::Stored);
        assert_eq!(method_for(LESTA_OODLE_CINFO, 5, 10), Method::LestaOodle);
        assert_eq!(method_for(6, 5, 10), Method::Deflate);
    }
}

#[cfg(test)]
mod backend_tests {
    use super::*;
    struct Mock;
    impl OodleBackend for Mock {
        fn name(&self) -> &'static str { "mock" }
        fn decompress(&self, input: &[u8], output: &mut [u8]) -> std::result::Result<usize, String> {
            let n = input.len().min(output.len());
            output[..n].copy_from_slice(&input[..n]);
            Ok(n)
        }
    }
    fn load_mock() -> Option<Box<dyn OodleBackend>> { Some(Box::new(Mock)) }

    #[test]
    fn lazy_loader_registers_backend_once() {
        assert!(set_oodle_loader(load_mock));
        let b = oodle_backend().expect("loader should provide backend");
        assert_eq!(b.name(), "mock");
        let mut out = [0u8; 3];
        assert_eq!(b.decompress(b"abc", &mut out).unwrap(), 3);
        assert_eq!(&out, b"abc");
        assert!(!set_oodle_backend(Box::new(Mock)), "second registration must fail");
    }
}
