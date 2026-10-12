//! 顶点格式串(如 `set3/xyznuvtpc`)解析与属性解包。
//!
//! 属性编码(按出现顺序紧密排列):
//! - `xyz` 位置 f32×3 (12B)      - `n` 法线 packed (4B)      - `uv` 纹理坐标 f16×2 (4B), `uv2` 表示两套
//! - `t` 切线 (4B) / `b` 副切线 (4B) / `tb` 两者     - `iiiww` 骨骼索引+权重 (8B)   - `r` 额外数据 (4B)
//! - `pc`, `oi` 只是标志, 不占字节
//!
//! 参考 landaire/wows-toolkit `models/vertex_format.rs`(MIT), 但修正了单独 `t` 应占 4 字节
//! (莱服 26.10 实测: xyznuvtpc / xyznuvrpc stride=24, xyznuvpc=20, xyznuviiiwwtpc=32)。
//! 解析得到的 stride 会与文件中的 stride 交叉校验, 不一致时只按"位置在偏移 0"导出并给出警告。

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Semantic {
    Position,
    Normal,
    TexCoord0,
    TexCoord1,
    Tangent,
    Binormal,
    Skin,
    Extra,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Attribute {
    pub semantic: Semantic,
    pub offset: usize,
    pub size: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VertexFormat {
    pub code: String,
    pub attributes: Vec<Attribute>,
    pub stride: usize,
}

pub fn parse_format(name: &str) -> VertexFormat {
    let code = name.rsplit('/').next().unwrap_or(name).trim_end_matches('\0').to_string();
    let c: Vec<char> = code.chars().collect();
    let mut attrs = Vec::new();
    let mut off = 0usize;
    let mut uv = 0;
    let mut i = 0;
    let push = |attrs: &mut Vec<Attribute>, s, size, off: &mut usize| {
        attrs.push(Attribute { semantic: s, offset: *off, size });
        *off += size;
    };
    while i < c.len() {
        let rest: String = c[i..].iter().collect();
        if rest.starts_with("xyz") {
            push(&mut attrs, Semantic::Position, 12, &mut off);
            i += 3;
        } else if rest.starts_with("uv2") {
            push(&mut attrs, Semantic::TexCoord0, 4, &mut off);
            push(&mut attrs, Semantic::TexCoord1, 4, &mut off);
            uv = 2;
            i += 3;
        } else if rest.starts_with("uv") {
            push(&mut attrs, if uv == 0 { Semantic::TexCoord0 } else { Semantic::TexCoord1 }, 4, &mut off);
            uv += 1;
            i += 2;
        } else if rest.starts_with("iiiww") {
            push(&mut attrs, Semantic::Skin, 8, &mut off);
            i += 5;
        } else if rest.starts_with("pc") || rest.starts_with("oi") {
            i += 2;
        } else {
            match c[i] {
                'n' => push(&mut attrs, Semantic::Normal, 4, &mut off),
                't' => push(&mut attrs, Semantic::Tangent, 4, &mut off),
                'b' => push(&mut attrs, Semantic::Binormal, 4, &mut off),
                'r' => push(&mut attrs, Semantic::Extra, 4, &mut off),
                'i' => push(&mut attrs, Semantic::Skin, 4, &mut off),
                _ => {}
            }
            i += 1;
        }
    }
    VertexFormat { code, attributes: attrs, stride: off }
}

impl VertexFormat {
    pub fn find(&self, s: Semantic) -> Option<&Attribute> {
        self.attributes.iter().find(|a| a.semantic == s)
    }
}

/// packed 法线: 4 个**无符号**字节 [x,y,z,w], 分量 = b/127.5 - 1 (UNORM8 偏置)。
///
/// M1 实测(莱服 26.10, Richelieu 船体与 FGM049 主炮共 8 万+ 顶点): 按无符号解出的长度均值 0.99,
/// 平均 |len-1| ≈ 0.01; 而 wows-toolkit 的有符号 int8/127 解法长度均值 1.13–1.37, 是错误的。
pub fn unpack_normal(b: [u8; 4]) -> [f32; 3] {
    [b[0] as f32 / 127.5 - 1.0, b[1] as f32 / 127.5 - 1.0, b[2] as f32 / 127.5 - 1.0]
}

/// packed UV: 2 × f16, 存储值 = 实际值 - 0.5。
pub fn unpack_uv(b: [u8; 4]) -> [f32; 2] {
    let u = half::f16::from_bits(u16::from_le_bytes([b[0], b[1]])).to_f32();
    let v = half::f16::from_bits(u16::from_le_bytes([b[2], b[3]])).to_f32();
    [u + 0.5, v + 0.5]
}

#[derive(Debug, Default, Clone)]
pub struct Unpacked {
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub uvs: Option<Vec<[f32; 2]>>,
}

/// 解包一段顶点(引擎坐标系, 未做手性转换)。
pub fn unpack(raw: &[u8], stride: usize, fmt: &VertexFormat) -> (Unpacked, Option<String>) {
    let n = if stride == 0 { 0 } else { raw.len() / stride };
    let mut warn = None;
    let consistent = fmt.stride == stride;
    if !consistent {
        warn = Some(format!(
            "顶点格式 {} 解析出的 stride={} 与文件 stride={stride} 不一致, 仅导出位置(偏移 0)",
            fmt.code, fmt.stride
        ));
    }
    let rd4 = |i: usize, o: usize| -> [u8; 4] { raw[i * stride + o..i * stride + o + 4].try_into().unwrap() };
    let pos_off = if consistent { fmt.find(Semantic::Position).map(|a| a.offset).unwrap_or(0) } else { 0 };
    let positions = (0..n)
        .map(|i| {
            [
                f32::from_le_bytes(rd4(i, pos_off)),
                f32::from_le_bytes(rd4(i, pos_off + 4)),
                f32::from_le_bytes(rd4(i, pos_off + 8)),
            ]
        })
        .collect();
    let normals = if consistent { fmt.find(Semantic::Normal).map(|a| (0..n).map(|i| unpack_normal(rd4(i, a.offset))).collect()) } else { None };
    let uvs = if consistent { fmt.find(Semantic::TexCoord0).map(|a| (0..n).map(|i| unpack_uv(rd4(i, a.offset))).collect()) } else { None };
    (Unpacked { positions, normals, uvs }, warn)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strides_match_lesta_26_10() {
        assert_eq!(parse_format("set3/xyznuvtpc").stride, 24);
        assert_eq!(parse_format("set3/xyznuvrpc").stride, 24);
        assert_eq!(parse_format("set3/xyznuvpc").stride, 20);
        assert_eq!(parse_format("set3/xyznuviiiwwtpc").stride, 32);
        assert_eq!(parse_format("set3/xyznuvtbpc").stride, 28);
        assert_eq!(parse_format("set3/xyznuv2tbpc").stride, 32);
    }
}
