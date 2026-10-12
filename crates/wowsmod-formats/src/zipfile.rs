//! 最小 ZIP 读写(只用 store / deflate, 无加密、无 ZIP64), 供 `wowsmod pack` 生成可分发包与安装时读包。
//! 写出是确定性的: 固定时间戳(1980-01-01)、按传入顺序、UTF-8 文件名标志, 同样输入 → 同样字节。
use crate::{Error, Result};

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const EOCD: u32 = 0x0605_4b50;
const DOS_DATE: u16 = (0 << 9) | (1 << 5) | 1; // 1980-01-01
const FLAG_UTF8: u16 = 1 << 11;

pub struct ZipEntry {
    pub name: String,
    pub data: Vec<u8>,
}

fn le16(o: &mut Vec<u8>, v: u16) {
    o.extend_from_slice(&v.to_le_bytes());
}
fn le32(o: &mut Vec<u8>, v: u32) {
    o.extend_from_slice(&v.to_le_bytes());
}

/// 写 ZIP。数据可压缩时用 deflate(级别 6), 否则存储。
pub fn write(entries: &[ZipEntry]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut cd = Vec::new();
    if entries.len() > 0xFFFF {
        return Err(Error::format("zip", "文件数超过 65535(不支持 ZIP64)"));
    }
    for e in entries {
        check_name(&e.name)?;
        let crc = crc32fast::hash(&e.data);
        let comp = miniz_oxide::deflate::compress_to_vec(&e.data, 6);
        let (method, body): (u16, &[u8]) = if comp.len() < e.data.len() { (8, &comp) } else { (0, &e.data) };
        if out.len() as u64 + body.len() as u64 > u32::MAX as u64 - 1024 {
            return Err(Error::format("zip", "包超过 4 GiB(不支持 ZIP64)"));
        }
        let off = out.len() as u32;
        let name = e.name.as_bytes();
        le32(&mut out, LOCAL);
        le16(&mut out, 20);
        le16(&mut out, FLAG_UTF8);
        le16(&mut out, method);
        le16(&mut out, 0);
        le16(&mut out, DOS_DATE);
        le32(&mut out, crc);
        le32(&mut out, body.len() as u32);
        le32(&mut out, e.data.len() as u32);
        le16(&mut out, name.len() as u16);
        le16(&mut out, 0);
        out.extend_from_slice(name);
        out.extend_from_slice(body);
        le32(&mut cd, CENTRAL);
        le16(&mut cd, 20);
        le16(&mut cd, 20);
        le16(&mut cd, FLAG_UTF8);
        le16(&mut cd, method);
        le16(&mut cd, 0);
        le16(&mut cd, DOS_DATE);
        le32(&mut cd, crc);
        le32(&mut cd, body.len() as u32);
        le32(&mut cd, e.data.len() as u32);
        le16(&mut cd, name.len() as u16);
        le16(&mut cd, 0);
        le16(&mut cd, 0);
        le16(&mut cd, 0);
        le16(&mut cd, 0);
        le32(&mut cd, 0);
        le32(&mut cd, off);
        cd.extend_from_slice(name);
    }
    let cd_off = out.len() as u32;
    out.extend_from_slice(&cd);
    le32(&mut out, EOCD);
    le16(&mut out, 0);
    le16(&mut out, 0);
    le16(&mut out, entries.len() as u16);
    le16(&mut out, entries.len() as u16);
    le32(&mut out, cd.len() as u32);
    le32(&mut out, cd_off);
    le16(&mut out, 0);
    Ok(out)
}

/// 包内路径必须是相对路径, 不含 `..`、盘符、反斜杠、空段
pub fn check_name(n: &str) -> Result<()> {
    let bad = n.is_empty() || n.starts_with('/') || n.contains('\\') || n.contains(':') || n.split('/').any(|c| c == ".." || c == "." || (c.is_empty()));
    if bad {
        return Err(Error::format("zip", format!("不安全的包内路径 {n:?}")));
    }
    Ok(())
}

fn u16_at(d: &[u8], o: usize) -> Result<u16> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| Error::format("zip", "截断"))
}
fn u32_at(d: &[u8], o: usize) -> Result<u32> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| Error::format("zip", "截断"))
}

/// 读 ZIP(按中央目录), 校验 CRC。目录项(以 / 结尾)跳过。
pub fn read(d: &[u8]) -> Result<Vec<ZipEntry>> {
    if d.len() < 22 {
        return Err(Error::format("zip", "文件太小, 不是 zip"));
    }
    let lo = d.len().saturating_sub(22 + 0xFFFF);
    let mut eocd = None;
    for i in (lo..=d.len() - 22).rev() {
        if u32_at(d, i)? == EOCD {
            eocd = Some(i);
            break;
        }
    }
    let e = eocd.ok_or_else(|| Error::format("zip", "找不到 zip 目录尾(不是 zip 包或已损坏)"))?;
    let n = u16_at(d, e + 10)? as usize;
    let mut p = u32_at(d, e + 16)? as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if u32_at(d, p)? != CENTRAL {
            return Err(Error::format("zip", "中央目录损坏"));
        }
        let flags = u16_at(d, p + 8)?;
        let method = u16_at(d, p + 10)?;
        let crc = u32_at(d, p + 16)?;
        let csize = u32_at(d, p + 20)? as usize;
        let usize_ = u32_at(d, p + 24)? as usize;
        let nl = u16_at(d, p + 28)? as usize;
        let el = u16_at(d, p + 30)? as usize;
        let cl = u16_at(d, p + 32)? as usize;
        let loff = u32_at(d, p + 42)? as usize;
        let name = d.get(p + 46..p + 46 + nl).ok_or_else(|| Error::format("zip", "截断"))?;
        let name = String::from_utf8(name.to_vec()).map_err(|_| Error::format("zip", "文件名不是 UTF-8"))?;
        p += 46 + nl + el + cl;
        if flags & 1 != 0 {
            return Err(Error::format("zip", "不支持加密的 zip"));
        }
        if name.ends_with('/') {
            continue;
        }
        check_name(&name)?;
        if u32_at(d, loff)? != LOCAL {
            return Err(Error::format("zip", format!("{name}: 本地头损坏")));
        }
        let ln = u16_at(d, loff + 26)? as usize;
        let le = u16_at(d, loff + 28)? as usize;
        let s = loff + 30 + ln + le;
        let body = d.get(s..s + csize).ok_or_else(|| Error::format("zip", format!("{name}: 截断")))?;
        let data = match method {
            0 => body.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(body, usize_.max(1) * 2 + 1024).map_err(|e| Error::format("zip", format!("{name}: 解压失败 {e:?}")))?,
            m => return Err(Error::format("zip", format!("{name}: 不支持的压缩方式 {m}"))),
        };
        if data.len() != usize_ || crc32fast::hash(&data) != crc {
            return Err(Error::format("zip", format!("{name}: CRC/长度校验失败")));
        }
        out.push(ZipEntry { name, data });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_and_deterministic() {
        let es = vec![
            ZipEntry { name: "a/中文.txt".into(), data: "你好".repeat(100).into_bytes() },
            ZipEntry { name: "b.bin".into(), data: vec![1, 2, 3] },
            ZipEntry { name: "empty".into(), data: vec![] },
        ];
        let z1 = write(&es).unwrap();
        let z2 = write(&es).unwrap();
        assert_eq!(z1, z2);
        let r = read(&z1).unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].name, "a/中文.txt");
        assert_eq!(r[0].data, es[0].data);
        assert_eq!(r[1].data, vec![1, 2, 3]);
        assert!(r[2].data.is_empty());
        assert!(write(&[ZipEntry { name: "../x".into(), data: vec![] }]).is_err());
        let mut bad = z1.clone();
        let i = bad.len() / 3;
        bad[i] ^= 0xFF;
        assert!(read(&bad).is_err() || read(&bad).unwrap()[0].data != es[0].data);
    }
}
