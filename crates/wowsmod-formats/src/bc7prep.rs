//! BC7Prep 逆变换: 把莱服贴图中经 Oodle Texture "BC7Prep" 重排过的 BC7 数据还原成标准 BC7 块。
//!
//! 背景: 部分 `.dds/.dd0/.dd1/.dd2` 贴图(DX10 头, DXGI 格式 BC7, 文件偏移 40 即 `dwReserved1[2]` = 1)
//! 在 pkg 中存放的不是标准 BC7, 而是 BC7Prep 形式(更利于 Oodle 压缩)。游戏/引擎加载时还原;
//! pfsunpack 解包时也会还原, 并把偏移 40 清零。本模块是**黑盒逆向**所得的纯 Rust 实现:
//! 通过 12 个(原始, pfsunpack 输出)成对样本推出逐位映射, 在这些样本上逐字节一致。
//! 格式细节见 docs/DESIGN.md §贴图。未覆盖/未见过的情形(版本号 ≠ 0x7bc、模式 8 计数 > 0)
//! 会明确报"不支持"而不是猜测。
//!
//! 载荷布局(紧跟 148 字节 DDS+DX10 头):
//! - 48 字节头: u32 版本(0x7bc), u32 flags, u32 counts[10](各"模式"块数, Σ = 块总数 N)。
//!   flags 第 m 位(m = 0..7)= 1 表示模式 m 的记录按字节拆成两个平面(SoA), 拆分点见 [`SPLIT`];
//!   flags 第 16 位 = 1 表示颜色去相关: 端点按 (G, R−G, B−G[, A]) 存放(模 2^位宽), 见 [`unprep_block_decorrelated`]。
//! - 按模式 0..9 依次的记录组: 模式 0..7 每块 16 字节(规范化后的字段排列), 模式 9 每块 4 字节(纯色 RGBA8)。
//! - 尾部 ceil(N/2) 字节模式图, 每块一个 nibble(低 nibble 在前), 按块序。
//!   模式 m 的第 k 个块(块序)对应模式 m 组里第 k 条记录。
use crate::error::{Error, Result};

/// 唯一见过的载荷版本号。
pub const VERSION: u32 = 0x7bc;
/// 拆平面时第一平面的字节宽度(按模式)。
pub const SPLIT: [usize; 8] = [8, 8, 12, 12, 6, 8, 8, 12];
/// flags 中表示"颜色去相关"的位。
pub const FLAG_DECORRELATE: u32 = 1 << 16;
const DDS_HDR: usize = 148;

/// 文件是否为 BC7Prep 形式的 DDS(DX10 头 + BC7 + 偏移 40 标志 = 1 + 载荷版本号匹配)。
pub fn is_bc7prep_dds(d: &[u8]) -> bool {
    if d.len() < DDS_HDR + 48 || &d[0..4] != b"DDS " || &d[84..88] != b"DX10" {
        return false;
    }
    let fmt = u32le(d, 128);
    if !(97..=99).contains(&fmt) {
        return false;
    }
    u32le(d, 40) == 1 && u32le(d, DDS_HDR) == VERSION
}

/// 还原整份 DDS: 头部原样保留, 仅把偏移 40 的标志清零(与 pfsunpack 输出一致), 载荷还原成标准 BC7。
pub fn decode_dds(d: &[u8]) -> Result<Vec<u8>> {
    if !is_bc7prep_dds(d) {
        return Err(Error::format("bc7prep", "不是 BC7Prep 形式的 DDS / not a BC7Prep DDS"));
    }
    let blocks = decode_payload(&d[DDS_HDR..])?;
    let mut out = Vec::with_capacity(DDS_HDR + blocks.len());
    out.extend_from_slice(&d[..DDS_HDR]);
    out[40..44].copy_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&blocks);
    Ok(out)
}

fn u32le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

/// 还原 BC7Prep 载荷(不含 DDS 头), 返回 N×16 字节的标准 BC7 块流。
pub fn decode_payload(q: &[u8]) -> Result<Vec<u8>> {
    if q.len() < 48 {
        return Err(Error::Bounds { what: "bc7prep header", offset: 0, len: 48, total: q.len() });
    }
    let ver = u32le(q, 0);
    let flags = u32le(q, 4);
    if ver != VERSION {
        return Err(Error::Unsupported(format!("BC7Prep 版本 {ver:#x}(只见过 {VERSION:#x}) / unknown BC7Prep version")));
    }
    let mut counts = [0usize; 10];
    for (m, c) in counts.iter_mut().enumerate() {
        *c = u32le(q, 8 + 4 * m) as usize;
    }
    if counts[8] != 0 {
        return Err(Error::Unsupported(format!("BC7Prep 模式 8 计数 {}(未见过的情形) / mode-8 records not supported", counts[8])));
    }
    if flags & !(0xff | FLAG_DECORRELATE) != 0 {
        return Err(Error::Unsupported(format!("BC7Prep flags {flags:#x} 含未知位 / unknown flag bits")));
    }
    let decor = flags & FLAG_DECORRELATE != 0;
    let n: usize = counts.iter().sum();
    let map_len = n.div_ceil(2);
    let mut offs = [0usize; 10];
    let mut o = 48usize;
    for m in 0..10 {
        offs[m] = o;
        o = o
            .checked_add(counts[m].checked_mul(if m == 9 { 4 } else { 16 }).ok_or_else(|| Error::format("bc7prep", "计数溢出"))?)
            .ok_or_else(|| Error::format("bc7prep", "计数溢出"))?;
    }
    if o + map_len != q.len() {
        return Err(Error::format("bc7prep", format!("载荷长度 {} 与计数推算的 {} 不符 / size mismatch", q.len(), o + map_len)));
    }
    let map = &q[o..];
    let mut next = [0usize; 10];
    let mut out = vec![0u8; n * 16];
    for i in 0..n {
        let m = ((map[i >> 1] >> (4 * (i & 1))) & 15) as usize;
        if m > 9 {
            return Err(Error::format("bc7prep", format!("块 {i} 的模式号 {m} 非法")));
        }
        let j = next[m];
        next[m] += 1;
        if j >= counts[m] {
            return Err(Error::format("bc7prep", format!("模式 {m} 的块数多于头部计数 {}", counts[m])));
        }
        let blk: u128 = if m == 9 {
            let b = &q[offs[9] + 4 * j..offs[9] + 4 * j + 4];
            if decor {
                // (G, R−G, B−G, A)
                solid_block([b[0].wrapping_add(b[1]), b[0], b[0].wrapping_add(b[2]), b[3]])
            } else {
                solid_block([b[0], b[1], b[2], b[3]])
            }
        } else {
            let mut rec = [0u8; 16];
            if (flags >> m) & 1 == 1 {
                let s = SPLIT[m];
                let a = offs[m] + s * j;
                let b = offs[m] + s * counts[m] + (16 - s) * j;
                rec[..s].copy_from_slice(&q[a..a + s]);
                rec[s..].copy_from_slice(&q[b..b + 16 - s]);
            } else {
                rec.copy_from_slice(&q[offs[m] + 16 * j..offs[m] + 16 * j + 16]);
            }
            let r = u128::from_le_bytes(rec);
            if decor {
                unprep_block_decorrelated(m, r)?
            } else {
                unprep_block(m, r)
            }
        };
        out[16 * i..16 * i + 16].copy_from_slice(&blk.to_le_bytes());
    }
    Ok(out)
}

#[inline]
fn g(r: u128, s: u32, n: u32) -> u32 {
    ((r >> s) & ((1u128 << n) - 1)) as u32
}

struct W {
    v: u128,
    o: u32,
}
impl W {
    fn new() -> Self {
        W { v: 0, o: 0 }
    }
    #[inline]
    fn put(&mut self, x: u128, n: u32) {
        self.v |= (x & ((1u128 << n) - 1)) << self.o;
        self.o += n;
    }
}

/// 规范化记录 → 标准 BC7 块(模式 0..7)。
pub fn unprep_block(m: usize, r: u128) -> u128 {
    let mut w = W::new();
    match m {
        0 => {
            w.put(1, 1);
            w.put(g(r, 72, 4) as u128, 4);
            for k in 0..18 {
                w.put(g(r, 4 * k, 4) as u128, 4);
            }
            for k in 0..6 {
                w.put(g(r, 77 + k, 1) as u128, 1);
            }
            w.put(r >> 83, 45);
        }
        1 => {
            w.put(2, 2);
            w.put(g(r, 72, 6) as u128, 6);
            for e in 0..4 {
                w.put(g(r, 16 * e, 6) as u128, 6);
            }
            for e in 0..4 {
                w.put((g(r, 64 + 2 * e, 1) | g(r, 16 * e + 6, 5) << 1) as u128, 6);
            }
            for e in 0..4 {
                w.put((g(r, 65 + 2 * e, 1) | g(r, 16 * e + 11, 5) << 1) as u128, 6);
            }
            w.put(g(r, 80, 2) as u128, 2);
            w.put(r >> 82, 46);
        }
        2 => {
            w.put(4, 3);
            let part: u32 = (0..6).map(|e| g(r, 16 * e, 1) << e).sum();
            w.put(part as u128, 6);
            for c in 0..3 {
                for e in 0..6 {
                    w.put(g(r, 16 * e + 1 + 5 * c, 5) as u128, 5);
                }
            }
            w.put(r >> 99, 29);
        }
        3 => {
            w.put(8, 4);
            let part: u32 = (0..6).map(|k| g(r, 7 + 8 * k, 1) << k).sum();
            w.put(part as u128, 6);
            for c in 0..3 {
                for e in 0..4 {
                    w.put(g(r, 24 * e + 8 * c, 7) as u128, 7);
                }
            }
            w.put(g(r, 55, 1) as u128, 1);
            w.put(g(r, 63, 1) as u128, 1);
            w.put(g(r, 96, 2) as u128, 2);
            w.put(r >> 98, 30);
        }
        4 => {
            let rot = g(r, 48, 2);
            let im = g(r, 42, 1);
            // 槽 R,G,B,A 各两个 5 位端点(未旋转的规范顺序); 6 位分量的最低位在 bit0/bit1
            let slot = |c: u32, e: u32| g(r, 2 + 10 * c + 5 * e, 5);
            let sc = if rot == 0 { 3 } else { rot - 1 };
            w.put(16, 5);
            w.put(rot as u128, 2);
            w.put(im as u128, 1);
            for c in 0..3 {
                let src = if c == sc { 3 } else { c };
                for e in 0..2 {
                    w.put(slot(src, e) as u128, 5);
                }
            }
            for e in 0..2 {
                w.put((slot(sc, e) << 1 | g(r, e, 1)) as u128, 6);
            }
            w.put(r >> 50, 78);
        }
        5 => {
            let rot = g(r, 64, 2);
            let slot = |c: u32, e: u32| g(r, 16 * c + 8 * e, 8);
            let sc = if rot == 0 { 3 } else { rot - 1 };
            w.put(32, 6);
            w.put(rot as u128, 2);
            for c in 0..3 {
                let src = if c == sc { 3 } else { c };
                for e in 0..2 {
                    w.put((slot(src, e) >> 1) as u128, 7);
                }
            }
            for e in 0..2 {
                w.put(slot(sc, e) as u128, 8);
            }
            w.put(r >> 66, 62);
        }
        6 => {
            w.put(64, 7);
            for k in 0..8 {
                w.put(g(r, 8 * k, 7) as u128, 7);
            }
            w.put(g(r, 63, 2) as u128, 2);
            w.put(r >> 65, 63);
        }
        7 => {
            w.put(128, 8);
            w.put(g(r, 92, 6) as u128, 6);
            for c in 0..4 {
                for e in 0..4 {
                    w.put((g(r, 16 * c + 4 * e, 4) << 1 | g(r, 76 + 4 * c + e, 1)) as u128, 5);
                }
            }
            w.put(g(r, 72, 4) as u128, 4);
            w.put(r >> 98, 30);
        }
        _ => unreachable!("mode {m}"),
    }
    debug_assert_eq!(w.o, 128);
    w.v
}

/// 颜色去相关形式(flags 第 16 位)的规范化记录 → 标准 BC7 块(模式 0..7)。
/// 颜色端点存为 G、R−G、B−G(模该字段位宽), alpha 原样。各模式的位置:
/// - 模式 0/1/2/3: 与非去相关形式相同的位置, 只是原来放 R/G/B 的槽依次放 G/R−G/B−G;
/// - 模式 4: 5 位槽依次为 G0 G1 (R−G)0 (R−G)1 (B−G)0 (B−G)1 A0 A1(规范/未旋转顺序), 6 位分量最低位仍在 bit0/1;
/// - 模式 5: 每个端点 4 字节 (G, R−G, B−G, A)(8 位域; 7 位颜色是 v<<1)。旋转 2(G 为 8 位)时 G 的最低位
///   放在 R−G 字节的最低位(R−G 本身是偶数);
/// - 模式 6: 每个端点 4 字节 (G, R−G, B−G, A)(7 位值);
/// - 模式 7: 每个端点 16 位字 [p | G<<1 | (R−G)<<6 | (B−G)<<11], A 为 72 起的 4 个 5 位值。
pub fn unprep_block_decorrelated(m: usize, r: u128) -> Result<u128> {
    // 返回 Result 以便将来遇到未见过的情形时报错; 目前模式 0..7 均已由真实样本验证
    let mut w = W::new();
    match m {
        0 => {
            w.put(1, 1);
            w.put(g(r, 72, 4) as u128, 4);
            let gg: Vec<u32> = (0..6).map(|e| g(r, 4 * e, 4)).collect();
            for e in 0..6 {
                w.put(((gg[e as usize] + g(r, 24 + 4 * e, 4)) & 15) as u128, 4);
            }
            for e in 0..6 {
                w.put(gg[e] as u128, 4);
            }
            for e in 0..6 {
                w.put(((gg[e as usize] + g(r, 48 + 4 * e, 4)) & 15) as u128, 4);
            }
            for k in 0..6 {
                w.put(g(r, 77 + k, 1) as u128, 1);
            }
            w.put(r >> 83, 45);
        }
        1 => {
            w.put(2, 2);
            w.put(g(r, 72, 6) as u128, 6);
            let gg: Vec<u32> = (0..4).map(|e| g(r, 16 * e, 6)).collect();
            for e in 0..4u32 {
                let dr = g(r, 64 + 2 * e, 1) | g(r, 16 * e + 6, 5) << 1;
                w.put(((gg[e as usize] + dr) & 63) as u128, 6);
            }
            for e in 0..4 {
                w.put(gg[e] as u128, 6);
            }
            for e in 0..4u32 {
                let db = g(r, 65 + 2 * e, 1) | g(r, 16 * e + 11, 5) << 1;
                w.put(((gg[e as usize] + db) & 63) as u128, 6);
            }
            w.put(g(r, 80, 2) as u128, 2);
            w.put(r >> 82, 46);
        }
        3 => {
            w.put(8, 4);
            let part: u32 = (0..6).map(|k| g(r, 7 + 8 * k, 1) << k).sum();
            w.put(part as u128, 6);
            let gg: Vec<u32> = (0..4).map(|e| g(r, 24 * e, 7)).collect();
            for e in 0..4u32 {
                w.put(((gg[e as usize] + g(r, 24 * e + 8, 7)) & 127) as u128, 7);
            }
            for e in 0..4 {
                w.put(gg[e] as u128, 7);
            }
            for e in 0..4u32 {
                w.put(((gg[e as usize] + g(r, 24 * e + 16, 7)) & 127) as u128, 7);
            }
            w.put(g(r, 55, 1) as u128, 1);
            w.put(g(r, 63, 1) as u128, 1);
            w.put(g(r, 96, 2) as u128, 2);
            w.put(r >> 98, 30);
        }
        4 => {
            let rot = g(r, 48, 2);
            let im = g(r, 42, 1);
            let f = |k: u32| g(r, 2 + 5 * k, 5);
            // 规范槽(R,G,B,A) × 端点, 5 位高位
            let slot = |c: u32, e: u32| -> u32 {
                match c {
                    0 => (f(e) + f(2 + e)) & 31,
                    1 => f(e),
                    2 => (f(e) + f(4 + e)) & 31,
                    _ => f(6 + e),
                }
            };
            let sc = if rot == 0 { 3 } else { rot - 1 };
            w.put(16, 5);
            w.put(rot as u128, 2);
            w.put(im as u128, 1);
            for c in 0..3 {
                let src = if c == sc { 3 } else { c };
                for e in 0..2 {
                    w.put(slot(src, e) as u128, 5);
                }
            }
            for e in 0..2 {
                w.put((slot(sc, e) << 1 | g(r, e, 1)) as u128, 6);
            }
            w.put(r >> 50, 78);
        }
        5 => {
            let rot = g(r, 64, 2);
            let mut slot = [[0u32; 2]; 4];
            for e in 0..2u32 {
                let b = |k: u32| g(r, 32 * e + 8 * k, 8);
                let (gv, rv) = if rot == 2 { (b(0) | (b(1) & 1), (b(0) + (b(1) & !1)) & 255) } else { (b(0), (b(0) + b(1)) & 255) };
                slot[0][e as usize] = rv;
                slot[1][e as usize] = gv;
                slot[2][e as usize] = (b(0) + b(2)) & 255;
                slot[3][e as usize] = b(3);
            }
            let sc = if rot == 0 { 3 } else { rot as usize - 1 };
            w.put(32, 6);
            w.put(rot as u128, 2);
            for c in 0..3 {
                let src = if c == sc { 3 } else { c };
                for e in 0..2 {
                    w.put((slot[src][e] >> 1) as u128, 7);
                }
            }
            for e in 0..2 {
                w.put(slot[sc][e] as u128, 8);
            }
            w.put(r >> 66, 62);
        }
        6 => {
            let s = |k: u32| g(r, 8 * k, 7);
            w.put(64, 7);
            for e in 0..2 {
                w.put(((s(4 * e) + s(4 * e + 1)) & 127) as u128, 7);
            }
            for e in 0..2 {
                w.put(s(4 * e) as u128, 7);
            }
            for e in 0..2 {
                w.put(((s(4 * e) + s(4 * e + 2)) & 127) as u128, 7);
            }
            for e in 0..2 {
                w.put(s(4 * e + 3) as u128, 7);
            }
            w.put(g(r, 63, 2) as u128, 2);
            w.put(r >> 65, 63);
        }
        7 => {
            w.put(128, 8);
            w.put(g(r, 92, 6) as u128, 6);
            let wd = |e: u32| g(r, 16 * e, 16);
            for e in 0..4 {
                w.put((((wd(e) >> 1) + (wd(e) >> 6)) & 31) as u128, 5);
            }
            for e in 0..4 {
                w.put(((wd(e) >> 1) & 31) as u128, 5);
            }
            for e in 0..4 {
                w.put((((wd(e) >> 1) + (wd(e) >> 11)) & 31) as u128, 5);
            }
            for e in 0..4 {
                w.put(g(r, 72 + 5 * e, 5) as u128, 5);
            }
            for e in 0..4 {
                w.put((wd(e) & 1) as u128, 1);
            }
            w.put(r >> 98, 30);
        }
        2 => {
            w.put(4, 3);
            let part: u32 = (0..6).map(|e| g(r, 16 * e, 1) << e).sum();
            w.put(part as u128, 6);
            let gg = |e: u32| g(r, 16 * e + 1, 5);
            for e in 0..6 {
                w.put(((gg(e) + g(r, 16 * e + 6, 5)) & 31) as u128, 5);
            }
            for e in 0..6 {
                w.put(gg(e) as u128, 5);
            }
            for e in 0..6 {
                w.put(((gg(e) + g(r, 16 * e + 11, 5)) & 31) as u128, 5);
            }
            w.put(r >> 99, 29);
        }
        _ => unreachable!("mode {m}"),
    }
    debug_assert_eq!(w.o, 128);
    Ok(w.v)
}

/// 纯色(模式 9 记录) → 标准 BC7 模式 5 块: 旋转 0, 颜色索引全为 1, alpha 索引全为 0,
/// A0 = alpha, A1 = 0; 每个颜色分量 v 用 7 位端点 (v>>1, v<128 ? (v+1)>>1 : (v-1)>>1)。
pub fn solid_block(rgba: [u8; 4]) -> u128 {
    let mut w = W::new();
    w.put(32, 6);
    w.put(0, 2);
    for &v in &rgba[..3] {
        let v = v as u128;
        let e1 = if v < 128 { (v + 1) >> 1 } else { (v - 1) >> 1 };
        w.put(v >> 1, 7);
        w.put(e1, 7);
    }
    w.put(rgba[3] as u128, 8);
    w.put(0, 8);
    w.put(0x2AAA_AAAB, 31);
    w.put(0, 31);
    w.v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(b: u128, s: u32, n: u32) -> u32 {
        g(b, s, n)
    }

    /// 测试用正向变换(标准块 → 规范化记录), 与 unprep_block 互逆。
    fn prep_block(m: usize, b: u128) -> u128 {
        let mut r: u128 = 0;
        let mut set = |s: u32, n: u32, v: u32| r |= ((v as u128) & ((1u128 << n) - 1)) << s;
        match m {
            0 => {
                set(72, 4, bits(b, 1, 4));
                for k in 0..18 {
                    set(4 * k, 4, bits(b, 5 + 4 * k, 4));
                }
                for k in 0..6 {
                    set(77 + k, 1, bits(b, 77 + k, 1));
                }
                set(83, 45, 0);
                r |= (b >> 83) << 83;
            }
            1 => {
                set(72, 6, bits(b, 2, 6));
                for e in 0..4 {
                    set(16 * e, 6, bits(b, 8 + 6 * e, 6));
                    let gv = bits(b, 32 + 6 * e, 6);
                    let bv = bits(b, 56 + 6 * e, 6);
                    set(16 * e + 6, 5, gv >> 1);
                    set(64 + 2 * e, 1, gv & 1);
                    set(16 * e + 11, 5, bv >> 1);
                    set(65 + 2 * e, 1, bv & 1);
                }
                set(80, 2, bits(b, 80, 2));
                r |= (b >> 82) << 82;
            }
            2 => {
                let part = bits(b, 3, 6);
                for e in 0..6 {
                    set(16 * e, 1, part >> e);
                    for c in 0..3 {
                        set(16 * e + 1 + 5 * c, 5, bits(b, 9 + 30 * c + 5 * e, 5));
                    }
                }
                r |= (b >> 99) << 99;
            }
            3 => {
                let part = bits(b, 4, 6);
                for k in 0..6 {
                    set(7 + 8 * k, 1, part >> k);
                }
                for c in 0..3 {
                    for e in 0..4 {
                        set(24 * e + 8 * c, 7, bits(b, 10 + 28 * c + 7 * e, 7));
                    }
                }
                set(55, 1, bits(b, 94, 1));
                set(63, 1, bits(b, 95, 1));
                set(96, 2, bits(b, 96, 2));
                r |= (b >> 98) << 98;
            }
            4 => {
                let rot = bits(b, 5, 2);
                let sc = if rot == 0 { 3 } else { rot - 1 };
                set(48, 2, rot);
                set(42, 1, bits(b, 7, 1));
                for c in 0..3u32 {
                    let dst = if c == sc { 3 } else { c };
                    for e in 0..2 {
                        set(2 + 10 * dst + 5 * e, 5, bits(b, 8 + 10 * c + 5 * e, 5));
                    }
                }
                for e in 0..2 {
                    let a = bits(b, 38 + 6 * e, 6);
                    set(2 + 10 * sc + 5 * e, 5, a >> 1);
                    set(e, 1, a & 1);
                }
                r |= (b >> 50) << 50;
            }
            5 => {
                let rot = bits(b, 6, 2);
                let sc = if rot == 0 { 3 } else { rot - 1 };
                set(64, 2, rot);
                for c in 0..3u32 {
                    let dst = if c == sc { 3 } else { c };
                    for e in 0..2 {
                        set(16 * dst + 8 * e, 8, bits(b, 8 + 14 * c + 7 * e, 7) << 1);
                    }
                }
                for e in 0..2 {
                    set(16 * sc + 8 * e, 8, bits(b, 50 + 8 * e, 8));
                }
                r |= (b >> 66) << 66;
            }
            6 => {
                for k in 0..8 {
                    set(8 * k, 7, bits(b, 7 + 7 * k, 7));
                }
                set(63, 2, bits(b, 63, 2));
                r |= (b >> 65) << 65;
            }
            7 => {
                set(92, 6, bits(b, 8, 6));
                for c in 0..4 {
                    for e in 0..4 {
                        let v = bits(b, 14 + 20 * c + 5 * e, 5);
                        set(16 * c + 4 * e, 4, v >> 1);
                        set(76 + 4 * c + e, 1, v & 1);
                    }
                }
                set(72, 4, bits(b, 94, 4));
                r |= (b >> 98) << 98;
            }
            _ => unreachable!(),
        }
        r
    }

    fn rng(seed: &mut u64) -> u64 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    }

    #[test]
    fn unprep_inverts_prep_all_modes_random() {
        let mut s = 0x2026_1010_u64;
        for m in 0..8usize {
            for _ in 0..20000 {
                let lo = rng(&mut s) as u128 | (rng(&mut s) as u128) << 64;
                // 构造合法的模式 m 块: 低 m 位为 0, 第 m 位为 1
                let b = (lo << (m + 1)) | (1u128 << m);
                let back = unprep_block(m, prep_block(m, b));
                assert_eq!(back, b, "mode {m}");
            }
        }
    }

    /// 测试用: 去相关形式的正向变换(标准块 → 记录), 由非去相关记录改写而来。
    fn prep_block_decorrelated(m: usize, b: u128) -> u128 {
        let base = prep_block(m, b);
        let mut r = base;
        let mut set = |r: &mut u128, s: u32, n: u32, v: u32| {
            let mask = ((1u128 << n) - 1) << s;
            *r = (*r & !mask) | (((v as u128) << s) & mask);
        };
        match m {
            0 => {
                for e in 0..6 {
                    let (rv, gv, bv) = (g(base, 4 * e, 4), g(base, 24 + 4 * e, 4), g(base, 48 + 4 * e, 4));
                    set(&mut r, 4 * e, 4, gv);
                    set(&mut r, 24 + 4 * e, 4, rv.wrapping_sub(gv) & 15);
                    set(&mut r, 48 + 4 * e, 4, bv.wrapping_sub(gv) & 15);
                }
            }
            1 => {
                for e in 0..4 {
                    let rv = g(base, 16 * e, 6);
                    let gv = g(base, 64 + 2 * e, 1) | g(base, 16 * e + 6, 5) << 1;
                    let bv = g(base, 65 + 2 * e, 1) | g(base, 16 * e + 11, 5) << 1;
                    let (dr, db) = (rv.wrapping_sub(gv) & 63, bv.wrapping_sub(gv) & 63);
                    set(&mut r, 16 * e, 6, gv);
                    set(&mut r, 16 * e + 6, 5, dr >> 1);
                    set(&mut r, 64 + 2 * e, 1, dr & 1);
                    set(&mut r, 16 * e + 11, 5, db >> 1);
                    set(&mut r, 65 + 2 * e, 1, db & 1);
                }
            }
            2 => {
                for e in 0..6 {
                    let (rv, gv, bv) = (g(base, 16 * e + 1, 5), g(base, 16 * e + 6, 5), g(base, 16 * e + 11, 5));
                    set(&mut r, 16 * e + 1, 5, gv);
                    set(&mut r, 16 * e + 6, 5, rv.wrapping_sub(gv) & 31);
                    set(&mut r, 16 * e + 11, 5, bv.wrapping_sub(gv) & 31);
                }
            }
            3 => {
                for e in 0..4 {
                    let (rv, gv, bv) = (g(base, 24 * e, 7), g(base, 24 * e + 8, 7), g(base, 24 * e + 16, 7));
                    set(&mut r, 24 * e, 7, gv);
                    set(&mut r, 24 * e + 8, 7, rv.wrapping_sub(gv) & 127);
                    set(&mut r, 24 * e + 16, 7, bv.wrapping_sub(gv) & 127);
                }
            }
            4 => {
                let sl = |c: u32, e: u32| g(base, 2 + 10 * c + 5 * e, 5);
                for e in 0..2 {
                    set(&mut r, 2 + 5 * e, 5, sl(1, e));
                    set(&mut r, 2 + 5 * (2 + e), 5, sl(0, e).wrapping_sub(sl(1, e)) & 31);
                    set(&mut r, 2 + 5 * (4 + e), 5, sl(2, e).wrapping_sub(sl(1, e)) & 31);
                    set(&mut r, 2 + 5 * (6 + e), 5, sl(3, e));
                }
            }
            5 => {
                let rot = g(base, 64, 2);
                let sl = |c: u32, e: u32| g(base, 16 * c + 8 * e, 8);
                for e in 0..2 {
                    let (rv, gv, bv, av) = (sl(0, e), sl(1, e), sl(2, e), sl(3, e));
                    let g0 = gv & !1;
                    let b1 = if rot == 2 { (rv.wrapping_sub(g0) & 255) | (gv & 1) } else { rv.wrapping_sub(gv) & 255 };
                    set(&mut r, 32 * e, 8, if rot == 2 { g0 } else { gv });
                    set(&mut r, 32 * e + 8, 8, b1);
                    set(&mut r, 32 * e + 16, 8, bv.wrapping_sub(if rot == 2 { g0 } else { gv }) & 255);
                    set(&mut r, 32 * e + 24, 8, av);
                }
            }
            6 => {
                let ch = |c: u32, e: u32| g(base, 16 * c + 8 * e, 7);
                for e in 0..2 {
                    let (rv, gv, bv, av) = (ch(0, e), ch(1, e), ch(2, e), ch(3, e));
                    set(&mut r, 32 * e, 7, gv);
                    set(&mut r, 32 * e + 8, 7, rv.wrapping_sub(gv) & 127);
                    set(&mut r, 32 * e + 16, 7, bv.wrapping_sub(gv) & 127);
                    set(&mut r, 32 * e + 24, 7, av);
                }
            }
            7 => {
                let v = |c: u32, e: u32| g(base, 16 * c + 4 * e, 4) << 1 | g(base, 76 + 4 * c + e, 1);
                let mut out = r & !((1u128 << 98) - 1); // 保留索引
                let mut put = |s: u32, n: u32, x: u32| out |= ((x as u128) & ((1u128 << n) - 1)) << s;
                for e in 0..4 {
                    let (rv, gv, bv) = (v(0, e), v(1, e), v(2, e));
                    put(16 * e, 1, g(base, 72 + e, 1));
                    put(16 * e + 1, 5, gv);
                    put(16 * e + 6, 5, rv.wrapping_sub(gv) & 31);
                    put(16 * e + 11, 5, bv.wrapping_sub(gv) & 31);
                    put(72 + 5 * e, 5, v(3, e));
                }
                put(92, 6, g(base, 92, 6));
                return out;
            }
            _ => unreachable!(),
        }
        r
    }

    #[test]
    fn decorrelated_unprep_inverts_prep_random() {
        let mut s = 0x1600_0010_u64;
        for m in 0..8usize {
            for _ in 0..20000 {
                let lo = rng(&mut s) as u128 | (rng(&mut s) as u128) << 64;
                let b = (lo << (m + 1)) | (1u128 << m);
                let back = unprep_block_decorrelated(m, prep_block_decorrelated(m, b)).unwrap();
                assert_eq!(back, b, "mode {m}");
            }
        }
    }

    fn expand7(x: u32) -> u32 {
        (x << 1) | (x >> 6)
    }

    #[test]
    fn solid_block_interpolates_exactly() {
        for v in 0..=255u8 {
            let b = solid_block([v, v, v, v]);
            assert_eq!(b & 0xff, 32); // 模式 5, 旋转 0
            for c in 0..3 {
                let e0 = expand7(g(b, 8 + 14 * c, 7));
                let e1 = expand7(g(b, 15 + 14 * c, 7));
                // 颜色索引 1(2 位索引权重 21/64)
                assert_eq!(((64 - 21) * e0 + 21 * e1 + 32) >> 6, v as u32, "v={v}");
            }
            assert_eq!(g(b, 50, 8), v as u32);
            assert_eq!(g(b, 58, 8), 0);
        }
    }

    #[test]
    fn rejects_bad_payloads() {
        let mut q = vec![0u8; 48];
        q[0..4].copy_from_slice(&0x7bdu32.to_le_bytes());
        assert!(decode_payload(&q).is_err());
        q[0..4].copy_from_slice(&VERSION.to_le_bytes());
        q[8..12].copy_from_slice(&1u32.to_le_bytes()); // 1 个模式 0 块, 但没有数据
        assert!(decode_payload(&q).is_err());
    }

    #[test]
    fn tiny_payload_roundtrip() {
        // 两个块: 模式 9 纯色 + 模式 6(拆平面)
        let solid = [10u8, 200, 128, 255];
        let mut seed = 7u64;
        let m6: u128 = ((rng(&mut seed) as u128 | (rng(&mut seed) as u128) << 64) << 7) | 64;
        let rec = prep_block(6, m6).to_le_bytes();
        let mut q = Vec::new();
        q.extend_from_slice(&VERSION.to_le_bytes());
        q.extend_from_slice(&(1u32 << 6).to_le_bytes());
        let mut counts = [0u32; 10];
        counts[6] = 1;
        counts[9] = 1;
        for c in counts {
            q.extend_from_slice(&c.to_le_bytes());
        }
        q.extend_from_slice(&rec[..8]);
        q.extend_from_slice(&rec[8..]);
        q.extend_from_slice(&solid);
        q.push(9 | 6 << 4);
        let out = decode_payload(&q).unwrap();
        assert_eq!(u128::from_le_bytes(out[..16].try_into().unwrap()), solid_block(solid));
        assert_eq!(u128::from_le_bytes(out[16..].try_into().unwrap()), m6);
    }
}
