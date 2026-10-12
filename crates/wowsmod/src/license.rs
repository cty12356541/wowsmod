//! 素材许可扫描: 在素材目录里找"使用规则/readme/license"类文本, 命中"禁止二次配布/再分发"等字句即视为受限素材。
//! 结果写成侧车文件 `<产物>.license.json`, 下游(`model import/animate`)据此给项目置 `[license] no_redistribute`。

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// 受限关键字(UTF-8 匹配; 小写英文)
const KEYS: &[&str] = &[
    "二次配布", "再配布", "二次配信", "二次分发", "二次发布", "二次传播", "禁止转载", "请勿转载", "禁止再分发", "请勿再分发", "禁止二次", "请勿二次", "禁止分发", "不得分发", "不得转载",
    "redistribut", "re-distribut", "do not distribute", "no distribution", "not for distribution",
];

/// 同一批关键字的 GBK 字节(规则文件常是 GBK; 不引入编码库, 用 `"关键字".encode("gbk")` 生成)
const GBK_KEYS: &[(&str, &[u8])] = &[
    ("二次配布", &[182, 254, 180, 206, 197, 228, 178, 188]),
    ("再配布", &[212, 217, 197, 228, 178, 188]),
    ("二次分发", &[182, 254, 180, 206, 183, 214, 183, 162]),
    ("二次配信", &[182, 254, 180, 206, 197, 228, 208, 197]),
    ("禁止转载", &[189, 251, 214, 185, 215, 170, 212, 216]),
    ("请勿转载", &[199, 235, 206, 240, 215, 170, 212, 216]),
    ("禁止再分发", &[189, 251, 214, 185, 212, 217, 183, 214, 183, 162]),
    ("禁止二次", &[189, 251, 214, 185, 182, 254, 180, 206]),
    ("请勿二次", &[199, 235, 206, 240, 182, 254, 180, 206]),
];

fn is_rules_file(p: &Path) -> bool {
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "txt" | "md" | "html" | "htm" | "rtf" | "nfo")
}

fn find(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// 扫一个文件, 返回命中的关键字
pub fn scan_file(p: &Path) -> Option<String> {
    let b = std::fs::read(p).ok()?;
    if b.len() > 2 * 1024 * 1024 {
        return None;
    }
    // UTF-16LE(带 BOM)
    let text = if b.starts_with(&[0xFF, 0xFE]) {
        let u: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(&b).into_owned()
    };
    let low = text.to_lowercase();
    if let Some(k) = KEYS.iter().find(|k| low.contains(*k)) {
        return Some(k.to_string());
    }
    GBK_KEYS.iter().find(|(_, g)| find(&b, g)).map(|(k, _)| format!("{k}(GBK)"))
}

/// 在素材所在目录及其上一级找规则文本(不递归太深)
pub fn scan_dir_for(asset: &Path) -> Option<Value> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = asset.parent() {
        dirs.push(if d.as_os_str().is_empty() { PathBuf::from(".") } else { d.to_path_buf() });
        if let Some(dd) = d.parent().filter(|x| !x.as_os_str().is_empty()) {
            dirs.push(dd.to_path_buf());
        }
    }
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && is_rules_file(p)).collect();
        files.sort();
        for f in files {
            if let Some(k) = scan_file(&f) {
                return Some(json!({"no_redistribute": true, "rules_file": f.display().to_string(), "matched": k, "asset": asset.display().to_string()}));
            }
        }
    }
    None
}

pub fn sidecar(out: &Path) -> PathBuf {
    let mut s = out.as_os_str().to_owned();
    s.push(".license.json");
    PathBuf::from(s)
}

/// 读产物的侧车; 没有侧车时再直接扫产物目录(GLB 与规则文本放在一起的情况)
pub fn lookup(asset: &Path) -> Option<Value> {
    if let Ok(d) = std::fs::read(sidecar(asset)) {
        if let Ok(v) = serde_json::from_slice::<Value>(&d) {
            if v["no_redistribute"].as_bool() == Some(true) {
                return Some(v);
            }
        }
    }
    scan_dir_for(asset)
}

pub fn reason(v: &Value) -> String {
    format!(
        "{} 受限: {} 里写有「{}」",
        Path::new(v["asset"].as_str().unwrap_or("?")).file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default(),
        Path::new(v["rules_file"].as_str().unwrap_or("?")).file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default(),
        v["matched"].as_str().unwrap_or("?")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_utf8_and_gbk() {
        let d = std::env::temp_dir().join(format!("wm_lic_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("rules.txt"), "允许改造\n请勿二次配布\n").unwrap();
        std::fs::write(d.join("a.pmx"), b"x").unwrap();
        let v = scan_dir_for(&d.join("a.pmx")).unwrap();
        assert_eq!(v["no_redistribute"], true);
        std::fs::write(d.join("rules.txt"), [182u8, 254, 180, 206, 197, 228, 178, 188]).unwrap();
        assert!(scan_file(&d.join("rules.txt")).unwrap().contains("GBK"));
        std::fs::write(d.join("rules.txt"), "free to use").unwrap();
        assert!(scan_file(&d.join("rules.txt")).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
