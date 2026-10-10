//! 明文 .mfm(ModsSDK / extractSources 产物)的最小编辑, 以及 SDK 的 compile.info 编解码。
//!
//! 明文 .mfm 结构(BigWorld 风格 XML, 制表符缩进):
//! ```text
//! <X.mfm>
//!     <fx>    shaders/std_effects/PBS_ship.fx    </fx>
//!     <property>  diffuseMap
//!         <Texture>   PnFMods/<Mod>/<Ship>/ship/X_a.dds   </Texture>
//!     </property>
//!     <property>  g_detailScaleU
//!         <Float> 32.0000 </Float>
//!     </property>
//! </X.mfm>
//! ```
//! 编辑只做文本级替换/插入, 保持其余字节不变(便于 diff 与回滚)。
use crate::error::{Error, Result};

/// 取 `<fx>` 的值(去掉空白)
pub fn fx(text: &str) -> Option<String> {
    let a = text.find("<fx>")? + 4;
    let b = a + text[a..].find("</fx>")?;
    Some(text[a..b].trim().to_string())
}

/// 替换 `<fx>` 的值, 保留原有的制表符包裹样式。
pub fn set_fx(text: &str, new_fx: &str) -> Result<String> {
    let a = text.find("<fx>").ok_or_else(|| Error::format("mfm", "没有 <fx>"))? + 4;
    let b = a + text[a..].find("</fx>").ok_or_else(|| Error::format("mfm", "<fx> 未闭合"))?;
    Ok(format!("{}\t{}\t{}", &text[..a], new_fx, &text[b..]))
}

/// 着色器 fx 路径 → 对应的发光(emissive)变体。已是发光变体则原样返回。
/// 依据: 客户端 shaders/std_effects/ 下存在 PBS_ship_emissive.fx 与 PBS_ship_emissive_skinned.fx 的编译产物(.fxo)。
pub fn emissive_variant(fx: &str) -> Option<String> {
    if fx.contains("PBS_ship_emissive") {
        return Some(fx.to_string());
    }
    if let Some(p) = fx.strip_suffix("PBS_ship_skinned.fx") {
        return Some(format!("{p}PBS_ship_emissive_skinned.fx"));
    }
    if let Some(p) = fx.strip_suffix("PBS_ship.fx") {
        return Some(format!("{p}PBS_ship_emissive.fx"));
    }
    None
}

/// 所有 `<property> 名字 <类型> 值 </类型> </property>` 条目: (名字, 类型, 值)
pub fn properties(text: &str) -> Vec<(String, String, String)> {
    let mut v = Vec::new();
    let mut rest = text;
    while let Some(a) = rest.find("<property>") {
        let s = &rest[a + 10..];
        let Some(e) = s.find("</property>") else { break };
        let body = &s[..e];
        let name_end = body.find('<').unwrap_or(body.len());
        let name = body[..name_end].trim().to_string();
        let inner = &body[name_end..];
        if let (Some(t0), Some(t1)) = (inner.find('<'), inner.find('>')) {
            let ty = inner[t0 + 1..t1].to_string();
            let close = format!("</{ty}>");
            if let Some(c) = inner.find(&close) {
                v.push((name, ty.clone(), inner[t1 + 1..c].trim().to_string()));
            }
        }
        rest = &s[e + 11..];
    }
    v
}

/// 设置一个标量/纹理属性。已存在则改值, 否则插在最后一个 `</property>` 之后(没有则插在根闭合标签前)。
pub fn set_property(text: &str, name: &str, ty: &str, value: &str) -> Result<String> {
    // 查找已有
    let mut search = 0;
    while let Some(a) = text[search..].find("<property>") {
        let a = search + a;
        let s = a + 10;
        let e = s + text[s..].find("</property>").ok_or_else(|| Error::format("mfm", "<property> 未闭合"))?;
        let body = &text[s..e];
        let name_end = body.find('<').unwrap_or(body.len());
        if body[..name_end].trim() == name {
            let open = format!("<{ty}>");
            let close = format!("</{ty}>");
            let (Some(o), Some(c)) = (body.find(&open), body.find(&close)) else {
                return Err(Error::format("mfm", format!("属性 {name} 不是 {ty} 类型")));
            };
            let vo = s + o + open.len();
            let vc = s + c;
            return Ok(format!("{}\t{}\t{}", &text[..vo], value, &text[vc..]));
        }
        search = e + 11;
    }
    let block = format!("\t<property>\t{name}\n\t\t<{ty}>\t{value}\t</{ty}>\n\t</property>\n");
    let pos = if let Some(p) = text.rfind("</property>") {
        let mut q = p + 11;
        // 吃掉同一行的换行
        if text[q..].starts_with("\r\n") {
            q += 2;
        } else if text[q..].starts_with('\n') {
            q += 1;
        }
        q
    } else {
        text.rfind("</").ok_or_else(|| Error::format("mfm", "找不到根闭合标签"))?
    };
    let crlf = text.contains("\r\n");
    let block = if crlf { block.replace('\n', "\r\n") } else { block };
    Ok(format!("{}{}{}", &text[..pos], block, &text[pos..]))
}

/// SDK 的 compile.info: `<content_info> base64(zlib(JSON)) </content_info>`。返回 JSON 文本。
pub fn decode_compile_info(text: &str) -> Result<String> {
    let a = text.find("<content_info>").ok_or_else(|| Error::format("compile.info", "没有 <content_info>"))? + 14;
    let b = a + text[a..].find("</content_info>").ok_or_else(|| Error::format("compile.info", "未闭合"))?;
    let raw = b64_decode(text[a..b].trim()).ok_or_else(|| Error::format("compile.info", "base64 无效"))?;
    let js = miniz_oxide::inflate::decompress_to_vec_zlib(&raw).map_err(|e| Error::format("compile.info", format!("zlib: {e:?}")))?;
    String::from_utf8(js).map_err(|_| Error::format("compile.info", "不是 UTF-8"))
}

/// 用新的 JSON 文本重写 compile.info(其余字节保持)。
pub fn encode_compile_info(orig: &str, json: &str) -> Result<String> {
    let a = orig.find("<content_info>").ok_or_else(|| Error::format("compile.info", "没有 <content_info>"))? + 14;
    let b = a + orig[a..].find("</content_info>").ok_or_else(|| Error::format("compile.info", "未闭合"))?;
    let z = miniz_oxide::deflate::compress_to_vec_zlib(json.as_bytes(), 9);
    Ok(format!("{}\t{}\t{}", &orig[..a], b64_encode(&z), &orig[b..]))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
pub fn b64_encode(d: &[u8]) -> String {
    let mut s = String::with_capacity(d.len().div_ceil(3) * 4);
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(B64[(n >> 18) as usize & 63] as char);
        s.push(B64[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    s
}
pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for ch in s.bytes() {
        if ch == b'=' {
            break;
        }
        if ch.is_ascii_whitespace() {
            continue;
        }
        let v = B64.iter().position(|&c| c == ch)? as u32;
        buf = buf << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

/// SDK 占位文件: extractSources 对贴图/几何等二进制只写出一个很短的 XML 占位(M0 实测 35–44 字节),
/// 需要从游戏包里补齐真实内容。判定: 小于 256 字节且以 '<' 开头、不是 DDS/几何魔数。
pub fn is_sdk_stub(d: &[u8]) -> bool {
    d.len() < 256 && d.first() == Some(&b'<') && !d.starts_with(b"DDS ")
}

#[cfg(test)]
mod tests {
    use super::*;
    const T: &str = "<X.mfm>\n\t<fx>\tshaders/std_effects/PBS_ship_skinned.fx\t</fx>\n\t<property>\tdiffuseMap\n\t\t<Texture>\tA/x_a.dds\t</Texture>\n\t</property>\n\t<property>\tg_detailScaleU\n\t\t<Float>\t32.0000\t</Float>\n\t</property>\n</X.mfm>\n";
    #[test]
    fn edit() {
        assert_eq!(fx(T).unwrap(), "shaders/std_effects/PBS_ship_skinned.fx");
        let e = emissive_variant(&fx(T).unwrap()).unwrap();
        assert_eq!(e, "shaders/std_effects/PBS_ship_emissive_skinned.fx");
        assert_eq!(emissive_variant("shaders/std_effects/PBS_ship.fx").unwrap(), "shaders/std_effects/PBS_ship_emissive.fx");
        let t = set_fx(T, &e).unwrap();
        assert_eq!(fx(&t).unwrap(), e);
        let t = set_property(&t, "emissivePower", "Float", "4.0000").unwrap();
        let t = set_property(&t, "g_detailScaleU", "Float", "16.0000").unwrap();
        let p = properties(&t);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0], ("diffuseMap".into(), "Texture".into(), "A/x_a.dds".into()));
        assert_eq!(p[1].2, "16.0000");
        assert_eq!(p[2], ("emissivePower".into(), "Float".into(), "4.0000".into()));
        assert!(t.ends_with("\t</property>\n</X.mfm>\n"));
    }
    #[test]
    fn b64_zlib() {
        for s in [&b""[..], b"a", b"ab", b"abc", b"hello world!!"] {
            assert_eq!(b64_decode(&b64_encode(s)).unwrap(), s);
        }
        let ci = format!("<content_info>\t{}\t</content_info>", b64_encode(&miniz_oxide::deflate::compress_to_vec_zlib(b"{\"a\": \"b\"}", 6)));
        assert_eq!(decode_compile_info(&ci).unwrap(), "{\"a\": \"b\"}");
        let n = encode_compile_info(&ci, "{}").unwrap();
        assert_eq!(decode_compile_info(&n).unwrap(), "{}");
        assert!(is_sdk_stub(b"<FSB025_x.dd0 />"));
        assert!(!is_sdk_stub(b"DDS \x7c"));
    }
}
