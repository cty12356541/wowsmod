//! MurmurHash3 x86_32。
//!
//! 已验证: geometry 的 mapping id = murmur3_32("<节点名>Shape.vertices" / "<节点名>Shape.indices", seed=0),
//! 在 FSB003_Richelieu_1944_Bow 上 16/16 与 SDK .visual 名字吻合; murmur3("diffuseMap") = 0x820f0280。

pub fn murmur3_32(data: &[u8], seed: u32) -> u32 {
    const C1: u32 = 0xcc9e_2d51;
    const C2: u32 = 0x1b87_3593;
    let mut h = seed;
    let nblocks = data.len() / 4;
    for i in 0..nblocks {
        let mut k = u32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
        k = k.wrapping_mul(C1).rotate_left(15).wrapping_mul(C2);
        h ^= k;
        h = h.rotate_left(13).wrapping_mul(5).wrapping_add(0xe654_6b64);
    }
    let tail = &data[nblocks * 4..];
    let mut k1: u32 = 0;
    if tail.len() >= 3 {
        k1 ^= (tail[2] as u32) << 16;
    }
    if tail.len() >= 2 {
        k1 ^= (tail[1] as u32) << 8;
    }
    if !tail.is_empty() {
        k1 ^= tail[0] as u32;
        k1 = k1.wrapping_mul(C1).rotate_left(15).wrapping_mul(C2);
        h ^= k1;
    }
    h ^= data.len() as u32;
    h ^= h >> 16;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 16;
    h
}

/// 由 .visual 中的节点名推出顶点/索引 mapping id。
pub fn mapping_ids_for_node(node: &str) -> (u32, u32) {
    let v = format!("{node}Shape.vertices");
    let i = format!("{node}Shape.indices");
    (murmur3_32(v.as_bytes(), 0), murmur3_32(i.as_bytes(), 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_values() {
        assert_eq!(murmur3_32(b"", 0), 0);
        assert_eq!(murmur3_32(b"", 1), 0x514E_28B7);
        assert_eq!(murmur3_32(b"hello", 0), 0x248b_fa47);
        assert_eq!(murmur3_32(b"The quick brown fox jumps over the lazy dog", 0), 0x2e4f_f723);
        // 游戏内已知值
        assert_eq!(murmur3_32(b"diffuseMap", 0), 0x820f_0280);
    }
}
