//! 统一错误类型。消息采用"中文 / English"双语, 并尽量给出可操作的建议。
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("读写失败 / I/O error: {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("格式错误 / bad format ({what}): {msg}")]
    Format { what: &'static str, msg: String },
    #[error("越界 / out of bounds ({what}): 需要偏移 {offset}+{len}, 数据只有 {total} 字节")]
    Bounds { what: &'static str, offset: usize, len: usize, total: usize },
    #[error("解压失败 / decompression failed ({what}): {msg}")]
    Decompress { what: String, msg: String },
    #[error("本构建未包含 Oodle 解压后端 / Oodle backend not compiled in: 请在 compression::set_oodle_backend 注册实现(见 docs/OODLE.md)")]
    NoOodleBackend,
    #[error("不支持 / unsupported: {0}")]
    Unsupported(String),
    #[error("找不到 / not found: {0}")]
    NotFound(String),
    #[error("meshopt 编解码失败 / meshopt codec error: {0}")]
    Meshopt(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn io(path: impl AsRef<std::path::Path>, source: std::io::Error) -> Self {
        Error::Io { path: path.as_ref().display().to_string(), source }
    }
    pub fn format(what: &'static str, msg: impl Into<String>) -> Self {
        Error::Format { what, msg: msg.into() }
    }
}
