//! gettext .mo 读取(用于按本地化船名检索, 如 IDS_PFSB510 → "Бургундия")。
use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::util::*;

pub fn parse_mo(d: &[u8]) -> Result<HashMap<String, String>> {
    let magic = u32le(d, 0, "mo")?;
    if magic != 0x9504_12de {
        return Err(Error::format("mo", format!("magic {magic:#x} (只支持小端 .mo)")));
    }
    let n = u32le(d, 8, "mo")? as usize;
    let ko = u32le(d, 12, "mo")? as usize;
    let vo = u32le(d, 16, "mo")? as usize;
    let mut m = HashMap::with_capacity(n);
    for i in 0..n {
        let kl = u32le(d, ko + 8 * i, "mo")? as usize;
        let kp = u32le(d, ko + 8 * i + 4, "mo")? as usize;
        let vl = u32le(d, vo + 8 * i, "mo")? as usize;
        let vp = u32le(d, vo + 8 * i + 4, "mo")? as usize;
        let k = String::from_utf8_lossy(slice(d, kp, kl, "mo key")?).into_owned();
        let v = String::from_utf8_lossy(slice(d, vp, vl, "mo value")?).into_owned();
        m.insert(k, v);
    }
    Ok(m)
}
