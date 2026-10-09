//! meshoptimizer 顶点编码 v0 的纯 Rust 编码器移植。
//!
//! 移植自 meshoptimizer 1.3 `src/vertexcodec.cpp` 的 v0 路径
//! (MIT License, Copyright (c) 2016-2026 Arseny Kapoulkine)。
//!
//! 为什么不直接用 meshopt-rs 的编码器: meshopt-rs 0.1.2 移植自较老的 meshoptimizer,
//! 其 `encodeBytes` 缺少"同等大小时优先沿用上一组位宽"的规则, 输出虽可正确解码,
//! 但与游戏文件不逐字节一致; 本实现在 18 个样本的全部顶点缓冲上与游戏文件逐字节一致。
//! 解码仍使用 meshopt-rs(与格式兼容, 已在样本上验证)。

const VERTEX_HEADER: u8 = 0xa0;
const VERTEX_BLOCK_SIZE_BYTES: usize = 8192;
const VERTEX_BLOCK_MAX_SIZE: usize = 256;
const BYTE_GROUP_SIZE: usize = 16;
const TAIL_MIN_SIZE_V0: usize = 32;
const BITS_V0: [u32; 4] = [0, 2, 4, 8];

fn vertex_block_size(vertex_size: usize) -> usize {
    let r = (VERTEX_BLOCK_SIZE_BYTES / vertex_size) & !(BYTE_GROUP_SIZE - 1);
    r.min(VERTEX_BLOCK_MAX_SIZE)
}

fn group_measure(g: &[u8], bits: u32) -> usize {
    match bits {
        0 => {
            if g.iter().all(|&b| b == 0) {
                0
            } else {
                usize::MAX
            }
        }
        8 => BYTE_GROUP_SIZE,
        _ => {
            let sentinel = ((1u32 << bits) - 1) as u8;
            let rest = g.iter().filter(|&&b| b >= sentinel).count();
            BYTE_GROUP_SIZE / 8 * bits as usize + rest
        }
    }
}

fn group_encode(out: &mut Vec<u8>, g: &[u8], bits: u32) {
    match bits {
        0 => {}
        8 => out.extend_from_slice(g),
        _ => {
            let byte_size = 8 / bits as usize;
            let sentinel = ((1u32 << bits) - 1) as u8;
            for i in (0..BYTE_GROUP_SIZE).step_by(byte_size) {
                let mut byte: u8 = 0;
                for k in 0..byte_size {
                    let v = g[i + k];
                    let enc = if v >= sentinel { sentinel } else { v };
                    byte = (byte << bits) | enc;
                }
                out.push(byte);
            }
            for &v in g {
                if v >= sentinel {
                    out.push(v);
                }
            }
        }
    }
}

fn encode_bytes(out: &mut Vec<u8>, buffer: &[u8]) {
    debug_assert!(buffer.len() % BYTE_GROUP_SIZE == 0);
    let header_size = (buffer.len() / BYTE_GROUP_SIZE).div_ceil(4);
    let header_pos = out.len();
    out.resize(out.len() + header_size, 0);
    let mut last_bits: i32 = -1;
    for (gi, g) in buffer.chunks_exact(BYTE_GROUP_SIZE).enumerate() {
        let mut best_k = 3usize;
        let mut best_size = group_measure(g, BITS_V0[best_k]);
        for k in 0..3 {
            let size = group_measure(g, BITS_V0[k]);
            // 同等大小时优先沿用上一组位宽, 但不替换字面量(8 bit)
            if size < best_size || (size == best_size && BITS_V0[k] as i32 == last_bits && BITS_V0[best_k] != 8) {
                best_k = k;
                best_size = size;
            }
        }
        out[header_pos + gi / 4] |= (best_k as u8) << ((gi % 4) * 2);
        let before = out.len();
        group_encode(out, g, BITS_V0[best_k]);
        debug_assert_eq!(out.len() - before, best_size);
        last_bits = BITS_V0[best_k] as i32;
    }
}

/// 以 meshopt 顶点编码 v0 编码 `vertices`(紧密排列, 每顶点 vertex_size 字节)。
pub fn encode_vertex_buffer_v0(vertices: &[u8], vertex_size: usize) -> Vec<u8> {
    assert!(vertex_size > 0 && vertex_size <= 256 && vertex_size % 4 == 0);
    assert!(vertices.len() % vertex_size == 0);
    let vertex_count = vertices.len() / vertex_size;
    let mut out = Vec::with_capacity(vertices.len() + 64);
    out.push(VERTEX_HEADER); // | version 0

    let mut first = [0u8; 256];
    if vertex_count > 0 {
        first[..vertex_size].copy_from_slice(&vertices[..vertex_size]);
    }
    let mut last = first;
    let block = vertex_block_size(vertex_size);
    let mut buf = vec![0u8; VERTEX_BLOCK_MAX_SIZE];
    let mut off = 0;
    while off < vertex_count {
        let n = block.min(vertex_count - off);
        let aligned = n.div_ceil(BYTE_GROUP_SIZE) * BYTE_GROUP_SIZE;
        let data = &vertices[off * vertex_size..(off + n) * vertex_size];
        for k in 0..vertex_size {
            buf[..aligned].fill(0);
            let mut p = last[k];
            for i in 0..n {
                let v = data[i * vertex_size + k];
                let d = v.wrapping_sub(p);
                // zigzag (8 位)
                buf[i] = (d << 1) ^ (((d as i8) >> 7) as u8);
                p = v;
            }
            encode_bytes(&mut out, &buf[..aligned]);
        }
        last[..vertex_size].copy_from_slice(&data[(n - 1) * vertex_size..n * vertex_size]);
        off += n;
    }
    if vertex_size < TAIL_MIN_SIZE_V0 {
        out.resize(out.len() + TAIL_MIN_SIZE_V0 - vertex_size, 0);
    }
    out.extend_from_slice(&first[..vertex_size]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 简单确定性伪随机
    fn lcg(seed: &mut u64) -> u32 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (*seed >> 33) as u32
    }

    #[test]
    fn encode_decode_roundtrip_various_strides() {
        let mut s = 42u64;
        for &stride in &[4usize, 8, 12, 20, 24, 32, 64] {
            for &count in &[1usize, 15, 16, 17, 255, 256, 257, 1000] {
                // 类似真实顶点: 平滑变化的字节 + 部分噪声
                let mut raw = vec![0u8; stride * count];
                for v in 0..count {
                    for b in 0..stride {
                        raw[v * stride + b] = if b % 4 == 3 { (lcg(&mut s) & 0xff) as u8 } else { ((v * (b + 1)) / 7) as u8 };
                    }
                }
                let enc = encode_vertex_buffer_v0(&raw, stride);
                let dec = crate::geometry::meshopt::decode_vertices(count, stride, &enc).unwrap();
                assert_eq!(dec, raw, "stride {stride} count {count}");
            }
        }
    }
}
