//! 从 `res_packages/*.pkg` 读取条目并解压。
//!
//! 游戏运行时 pkg 可能被占用; Rust 标准库在 Windows 上默认以
//! FILE_SHARE_READ|WRITE|DELETE 打开, 若仍失败会给出"请关闭游戏后重试"的提示。
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::compression;
use crate::error::{Error, Result};
use crate::index::Entry;

pub struct PkgReader {
    dir: PathBuf,
    handles: Mutex<HashMap<String, File>>,
    /// 是否校验 idx 中的 crc32 (对解压后的数据)
    pub verify_crc: bool,
}

impl PkgReader {
    pub fn new(res_packages: &Path) -> PkgReader {
        PkgReader { dir: res_packages.to_path_buf(), handles: Mutex::new(HashMap::new()), verify_crc: true }
    }

    fn open(&self, pkg: &str) -> Result<File> {
        let p = self.dir.join(pkg);
        File::open(&p).map_err(|e| {
            let hint = if e.raw_os_error() == Some(32) || e.raw_os_error() == Some(33) {
                " (文件被占用: 请关闭游戏/启动器后重试 / file locked by the game)"
            } else {
                ""
            };
            Error::Io { path: format!("{}{hint}", p.display()), source: e }
        })
    }

    /// 读取原始(未解压)字节。
    pub fn read_raw(&self, e: &Entry) -> Result<Vec<u8>> {
        // 每个 pkg 缓存一个句柄; 并行时为避免锁竞争, 拿不到缓存就临时打开新句柄
        let mut buf = vec![0u8; e.info.size as usize];
        let mut file = {
            let mut h = self.handles.lock().unwrap();
            match h.remove(&e.pkg) {
                Some(f) => f,
                None => self.open(&e.pkg)?,
            }
        };
        let r = file
            .seek(SeekFrom::Start(e.info.offset))
            .and_then(|_| file.read_exact(&mut buf))
            .map_err(|err| Error::io(self.dir.join(&e.pkg), err));
        self.handles.lock().unwrap().insert(e.pkg.clone(), file);
        r?;
        Ok(buf)
    }

    /// 读取并解压; verify_crc 时校验 crc32。
    pub fn read(&self, e: &Entry) -> Result<Vec<u8>> {
        let raw = self.read_raw(e)?;
        let out = compression::decompress_entry(&raw, e.info.compression_info, e.info.size, e.info.unpacked_size, &e.path)?;
        if self.verify_crc {
            let c = crc32fast::hash(&out);
            if c != e.info.crc32 {
                return Err(Error::Decompress {
                    what: e.path.clone(),
                    msg: format!("crc32 不一致: 计算 {c:#010x}, idx 记录 {:#010x}", e.info.crc32),
                });
            }
        }
        Ok(out)
    }
}
