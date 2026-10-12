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
    if fx.contains("PBS_ship_emissive") || fx.contains("ship_emissive_material") {
        return Some(fx.to_string());
    }
    // 新版材质系统(炮塔/指挥仪等常用): shaders/materials/pbs/ship_material[_skinned].fx,
    // 客户端带 ship_emissive_material[_skinned].fxo(E2E 勃艮第炮塔实测)。
    if let Some(p) = fx.strip_suffix("ship_material_skinned.fx") {
        return Some(format!("{p}ship_emissive_material_skinned.fx"));
    }
    if let Some(p) = fx.strip_suffix("ship_material.fx") {
        return Some(format!("{p}ship_emissive_material.fx"));
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
/// .visual 里材质标识 `SHIPMAT_<类型>_<部件>` 决定引擎实际用哪类着色器(M2 港口实验):
/// 只改 .mfm 的 fx 不生效, 要把引用该 .mfm 的 `<material>` 的 identifier 从 `SHIPMAT_PBS_X` 改成
/// `SHIPMAT_EMISSIVE_PBS_X`(官方发光皮肤在 assets.bin 中用的就是这一族标识)。
/// 返回 (新文本, [(旧标识, 新标识)])。只改 `<mfm>` 路径以 `mfm_file_name` 结尾(不分大小写)的材质块。
pub fn set_emissive_identifiers(visual: &str, mfm_file_name: &str) -> (String, Vec<(String, String)>) {
    let want = mfm_file_name.to_ascii_lowercase();
    let mut out = String::with_capacity(visual.len() + 64);
    let mut changes = Vec::new();
    let mut rest = visual;
    while let Some(i) = rest.find("<material>") {
        let (head, tail) = rest.split_at(i);
        out.push_str(head);
        let end = match tail.find("</material>") {
            Some(e) => e + "</material>".len(),
            None => {
                out.push_str(tail);
                return (out, changes);
            }
        };
        let block = &tail[..end];
        let refs = block
            .find("<mfm>")
            .and_then(|a| block[a + 5..].find("</mfm>").map(|b| block[a + 5..a + 5 + b].trim().to_ascii_lowercase()))
            .map(|m| m.replace('\\', "/"))
            .unwrap_or_default();
        let hit = refs.rsplit('/').next().map(|n| n == want).unwrap_or(false);
        let mut nb = block.to_string();
        if hit {
            if let Some(a) = block.find("<identifier>") {
                if let Some(b) = block[a..].find("</identifier>") {
                    let inner = &block[a + 12..a + b];
                    let id = inner.trim();
                    // 炮塔等 visual 用 TL2_ 前缀(TL2_SHIPMAT_PBS_Gun_skinned -> TL2_SHIPMAT_EMISSIVE_PBS_Gun_skinned, assets.bin 中存在)
                    let (pre, rest_id) = match id.strip_prefix("TL2_") { Some(r) => ("TL2_", r), None => ("", id) };
                    if let Some(part) = rest_id.strip_prefix("SHIPMAT_PBS_") {
                        let new_id = format!("{pre}SHIPMAT_EMISSIVE_PBS_{part}");
                        let new_inner = inner.replacen(id, &new_id, 1);
                        nb = format!("{}{}{}", &block[..a + 12], new_inner, &block[a + b..]);
                        changes.push((id.to_string(), new_id));
                    }
                }
            }
        }
        out.push_str(&nb);
        rest = &tail[end..];
    }
    out.push_str(rest);
    (out, changes)
}

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
        assert_eq!(emissive_variant("shaders/materials/pbs/ship_material_skinned.fx").unwrap(), "shaders/materials/pbs/ship_emissive_material_skinned.fx");
        assert_eq!(emissive_variant("shaders/materials/pbs/ship_material.fx").unwrap(), "shaders/materials/pbs/ship_emissive_material.fx");
        let v = "<material>\n<identifier>\tTL2_SHIPMAT_PBS_Gun_skinned\t</identifier>\n<mfm>\tx/FGM049_skinned.mfm\t</mfm>\n</material>";
        let (n, c) = set_emissive_identifiers(v, "FGM049_skinned.mfm");
        assert_eq!(c[0].1, "TL2_SHIPMAT_EMISSIVE_PBS_Gun_skinned");
        assert!(n.contains("TL2_SHIPMAT_EMISSIVE_PBS_Gun_skinned"));
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

#[cfg(test)]
mod ident_tests {
    use super::*;
    #[test]
    fn emissive_identifier() {
        let v = "<a>\r\n<material>\r\n\t<identifier>\tSHIPMAT_PBS_DeckHouse_skinned\t</identifier>\r\n\t<mfm>\tPnFMods/M/s/X_DeckHouse_skinned.mfm\t</mfm>\r\n</material>\r\n<material>\r\n\t<identifier>\tSHIPMAT_PBS_Hull\t</identifier>\r\n\t<mfm>\tPnFMods/M/s/X_Hull.mfm\t</mfm>\r\n</material></a>";
        let (n, c) = set_emissive_identifiers(v, "X_DeckHouse_skinned.mfm");
        assert_eq!(c, vec![("SHIPMAT_PBS_DeckHouse_skinned".to_string(), "SHIPMAT_EMISSIVE_PBS_DeckHouse_skinned".to_string())]);
        assert!(n.contains("<identifier>\tSHIPMAT_EMISSIVE_PBS_DeckHouse_skinned\t</identifier>"));
        assert!(n.contains("SHIPMAT_PBS_Hull\t"));
        assert_eq!(n.len(), v.len() + "EMISSIVE_".len());
    }
}
