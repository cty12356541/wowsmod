//! 统计标准 BC7 DDS(DX10 头)中各模式/旋转/索引模式的块数, 以及模式 7 中 R 端点非零的块数。
//! 用于确认 BC7Prep 逆变换的每个分支都被真实数据验证过。
//! 用法: bc7_coverage <列表文件(每行一个 DDS 路径)>  → stdout 输出 JSON
use std::collections::BTreeMap;
fn main() {
    let list = std::env::args().nth(1).expect("用法: bc7_coverage <列表文件>");
    let text = std::fs::read_to_string(&list).expect("读取列表");
    let mut c: BTreeMap<String, u64> = BTreeMap::new();
    let mut files = 0;
    for line in text.lines().map(|l| l.trim().trim_start_matches('\u{feff}')).filter(|l| !l.is_empty()) {
        let d = match std::fs::read(line) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if d.len() < 148 || &d[84..88] != b"DX10" {
            continue;
        }
        files += 1;
        for b in d[148..].chunks_exact(16) {
            let v = u128::from_le_bytes(b.try_into().unwrap());
            let m = (v as u32).trailing_zeros();
            let key = match m {
                4 => format!("mode4_rot{}_im{}", (v >> 5) & 3, (v >> 7) & 1),
                5 => format!("mode5_rot{}", (v >> 6) & 3),
                7 => {
                    let r = (v >> 14) & ((1u128 << 20) - 1);
                    format!("mode7_R{}", if r == 0 { "zero" } else { "nonzero" })
                }
                m if m < 8 => format!("mode{m}"),
                _ => "invalid".to_string(),
            };
            *c.entry(key).or_default() += 1;
        }
    }
    let body: Vec<String> = c.iter().map(|(k, v)| format!("\"{k}\": {v}")).collect();
    println!("{{\"files\": {files}, \"blocks\": {{{}}}}}", body.join(", "));
}
