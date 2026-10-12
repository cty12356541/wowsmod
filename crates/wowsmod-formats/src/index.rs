//! 把 `bin/<build>/idx/` 下全部 idx 合并成一个虚拟文件表, 支持 glob / 关键字检索。
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::idx::{FileInfo, IdxFile, ROOT_PARENT_ID};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Entry {
    /// 虚拟路径, 以 `/` 分隔, 无前导斜杠, 例如 `content/gameplay/france/ship/...`
    pub path: String,
    /// 所在 pkg 文件名 (位于 res_packages/)
    pub pkg: String,
    /// 来源 idx 文件名
    pub idx: String,
    #[serde(flatten)]
    pub info: FileInfo,
}

#[derive(Debug, Default)]
pub struct GameIndex {
    pub entries: Vec<Entry>,
    pub idx_files: usize,
    /// 同一路径出现多次的数量(保留第一次出现; 用于诊断)
    pub duplicates: usize,
    by_path: HashMap<String, usize>,
}

fn build_path(id: u64, names: &HashMap<u64, (u64, &str)>) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    let mut cur = id;
    for _ in 0..256 {
        let (parent, name) = names.get(&cur)?;
        parts.push(name);
        if *parent == ROOT_PARENT_ID || *parent == 0 {
            parts.reverse();
            let p = parts.join("/");
            return Some(p.trim_start_matches('/').to_string());
        }
        cur = *parent;
    }
    None
}

impl GameIndex {
    /// 读取目录下所有 *.idx (按文件名排序, 保证结果确定)。
    pub fn load_dir(dir: &Path) -> Result<GameIndex> {
        let rd = std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
        let mut files: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x.eq_ignore_ascii_case("idx")).unwrap_or(false))
            .collect();
        files.sort();
        if files.is_empty() {
            return Err(Error::NotFound(format!("{} 下没有 .idx 文件; 请确认这是 bin/<版本号>/idx 目录", dir.display())));
        }
        let mut gi = GameIndex::default();
        for f in &files {
            let d = std::fs::read(f).map_err(|e| Error::io(f, e))?;
            let idx = IdxFile::parse(&d).map_err(|e| match e {
                Error::Format { what, msg } => Error::Format { what, msg: format!("{}: {msg}", f.display()) },
                other => other,
            })?;
            gi.add(&idx, &f.file_name().unwrap().to_string_lossy());
        }
        gi.idx_files = files.len();
        Ok(gi)
    }

    pub fn add(&mut self, idx: &IdxFile, idx_name: &str) {
        let names: HashMap<u64, (u64, &str)> =
            idx.resources.iter().map(|r| (r.id, (r.parent_id, r.name.as_str()))).collect();
        let vols: HashMap<u64, &str> = idx.volumes.iter().map(|v| (v.volume_id, v.name.as_str())).collect();
        for fi in &idx.file_infos {
            let path = build_path(fi.resource_id, &names).unwrap_or_else(|| format!("<unresolved:{:016x}>", fi.resource_id));
            let pkg = vols.get(&fi.volume_id).map(|s| s.to_string()).unwrap_or_default();
            if self.by_path.contains_key(&path) {
                self.duplicates += 1;
                continue;
            }
            self.by_path.insert(path.clone(), self.entries.len());
            self.entries.push(Entry { path, pkg, idx: idx_name.to_string(), info: *fi });
        }
    }

    pub fn get(&self, path: &str) -> Option<&Entry> {
        let p = path.replace('\\', "/");
        let p = p.trim_start_matches('/');
        self.by_path.get(p).map(|&i| &self.entries[i])
    }

    /// 检索: 含 `* ? [` 视为 glob(大小写不敏感, 匹配完整虚拟路径;
    /// 若 glob 不含 `/`, 则只匹配文件名); 否则按关键字(大小写不敏感子串)匹配完整路径。
    pub fn search(&self, pattern: &str) -> Result<Vec<&Entry>> {
        let m = Matcher::new(pattern)?;
        Ok(self.entries.iter().filter(|e| m.is_match(&e.path)).collect())
    }
}

/// 路径匹配器(glob 或关键字)。
pub struct Matcher {
    glob: Option<globset::GlobMatcher>,
    name_only: bool,
    keyword: String,
}

impl Matcher {
    pub fn new(pattern: &str) -> Result<Matcher> {
        let pat = pattern.replace('\\', "/");
        let pat = pat.trim_start_matches('/').to_string();
        if pat.contains(['*', '?', '[']) {
            let g = globset::GlobBuilder::new(&pat)
                .case_insensitive(true)
                .literal_separator(false)
                .build()
                .map_err(|e| Error::format("glob", format!("无效的通配符 {pattern:?}: {e}")))?;
            Ok(Matcher { glob: Some(g.compile_matcher()), name_only: !pat.contains('/'), keyword: String::new() })
        } else {
            Ok(Matcher { glob: None, name_only: false, keyword: pat.to_lowercase() })
        }
    }
    pub fn is_match(&self, path: &str) -> bool {
        match &self.glob {
            Some(g) => {
                if self.name_only {
                    // 不含 / 的 glob: 既匹配文件名, 也允许 * 跨目录匹配完整路径
                    let name = path.rsplit('/').next().unwrap_or(path);
                    g.is_match(name) || g.is_match(path)
                } else {
                    g.is_match(path)
                }
            }
            None => path.to_lowercase().contains(&self.keyword),
        }
    }
}
