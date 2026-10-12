//! 带边界检查的小端读取工具。
use crate::error::{Error, Result};

#[inline]
pub fn slice<'a>(d: &'a [u8], off: usize, len: usize, what: &'static str) -> Result<&'a [u8]> {
    match off.checked_add(len) {
        Some(end) if end <= d.len() => Ok(&d[off..end]),
        _ => Err(Error::Bounds { what, offset: off, len, total: d.len() }),
    }
}
#[inline]
pub fn u16le(d: &[u8], off: usize, what: &'static str) -> Result<u16> {
    Ok(u16::from_le_bytes(slice(d, off, 2, what)?.try_into().unwrap()))
}
#[inline]
pub fn u32le(d: &[u8], off: usize, what: &'static str) -> Result<u32> {
    Ok(u32::from_le_bytes(slice(d, off, 4, what)?.try_into().unwrap()))
}
#[inline]
pub fn u64le(d: &[u8], off: usize, what: &'static str) -> Result<u64> {
    Ok(u64::from_le_bytes(slice(d, off, 8, what)?.try_into().unwrap()))
}
#[inline]
pub fn i64le(d: &[u8], off: usize, what: &'static str) -> Result<i64> {
    Ok(i64::from_le_bytes(slice(d, off, 8, what)?.try_into().unwrap()))
}
#[inline]
pub fn i32le(d: &[u8], off: usize, what: &'static str) -> Result<i32> {
    Ok(i32::from_le_bytes(slice(d, off, 4, what)?.try_into().unwrap()))
}
#[inline]
pub fn f32le(d: &[u8], off: usize, what: &'static str) -> Result<f32> {
    Ok(f32::from_le_bytes(slice(d, off, 4, what)?.try_into().unwrap()))
}

/// base + rel(有符号) → usize, 检查不为负。
pub fn rel(base: usize, rel: i64, what: &'static str) -> Result<usize> {
    let v = base as i64 + rel;
    if v < 0 {
        return Err(Error::format(what, format!("相对指针为负: base={base} rel={rel}")));
    }
    Ok(v as usize)
}

pub fn read_cstr(d: &[u8], off: usize, max: usize, what: &'static str) -> Result<String> {
    let s = slice(d, off, max.min(d.len().saturating_sub(off)), what)?;
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    Ok(String::from_utf8_lossy(&s[..end]).into_owned())
}
