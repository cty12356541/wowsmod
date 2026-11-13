//! Shared read-only ship metadata with explicit cache provenance.
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use wowsmod_formats::gameparams::ShipParam;
pub fn canonical_ship_key(input: &str) -> String {
    let input = input.trim();
    let code = input
        .split('_')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let unprefixed = if code.starts_with('P') && code.len() >= 7 {
        &code[1..]
    } else {
        code.as_str()
    };
    let bytes = unprefixed.as_bytes();
    if (6..=7).contains(&bytes.len())
        && bytes[..3].iter().all(u8::is_ascii_alphabetic)
        && bytes[3..].iter().all(u8::is_ascii_digit)
    {
        format!("P{unprefixed}")
    } else {
        input.to_string()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShipMetadata {
    pub key: String,
    pub known: bool,
    pub level: Option<i64>,
    pub nation: Option<String>,
    pub species: Option<String>,
    pub name_local: Option<String>,
    pub param: Option<String>,
    pub warning: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetadataSource {
    pub kind: String,
    pub root: Option<PathBuf>,
    pub build: Option<String>,
    pub cache_path: Option<PathBuf>,
    pub stale: bool,
    pub warnings: Vec<String>,
}
pub struct ShipCatalog {
    pub source: MetadataSource,
    entries: BTreeMap<String, ShipMetadata>,
}
#[derive(Serialize, Deserialize)]
struct CachedCatalog {
    schema_version: u32,
    root: PathBuf,
    build: String,
    loaded_unix: u64,
    localization: Option<PathBuf>,
    #[serde(default)]
    warnings: Vec<String>,
    entries: BTreeMap<String, ShipMetadata>,
}
static CATALOGS: OnceLock<Mutex<HashMap<String, Arc<ShipCatalog>>>> = OnceLock::new();
impl ShipCatalog {
    /// Shared across every project in one client root/build. No PreviewState is opened.
    pub fn load(home: &Path, game: Option<&crate::game::GameInstall>) -> Arc<Self> {
        let Some(game) = game else {
            return Arc::new(Self::fallback(
                home,
                vec!["客户端不可用；船舶属性仅可使用最近缓存".into()],
            ));
        };
        let root = std::fs::canonicalize(&game.root).unwrap_or_else(|_| game.root.clone());
        let key = format!(
            "{}|{}|{}",
            home.display(),
            root.to_string_lossy().to_lowercase(),
            game.build
        );
        let mut shared = CATALOGS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(catalog) = shared.get(&key) {
            return Arc::clone(catalog);
        }
        let cache_key = crate::modproj::sha256_hex(
            format!("{}|{}", root.to_string_lossy().to_lowercase(), game.build).as_bytes(),
        );
        let path = home
            .join("ship_metadata")
            .join(format!("{}.json", &cache_key[..24]));
        let mut warnings = vec![];
        let result = match read_cache(&path) {
            Ok(cached) if same_root(&cached.root, &root) && cached.build == game.build => {
                Self::from_cache(cached, path.clone(), "cache", false, vec![])
            }
            cached => {
                match cached {
                    Ok(_) => warnings.push("忽略不匹配当前客户端 root/build 的船舶缓存".into()),
                    Err(e) if path.exists() => warnings.push(format!("船舶缓存不可用: {e:#}")),
                    Err(_) => (),
                }
                match Self::read_game(game, &root, &path) {
                    Ok((mut catalog, cached)) => {
                        catalog.source.warnings.extend(warnings);
                        let bytes =
                            serde_json::to_vec_pretty(&cached).expect("serializable ship metadata");
                        for target in [&path, &home.join("ship_metadata/last.json")] {
                            if let Err(e) = crate::library::atomic_write(target, &bytes) {
                                catalog
                                    .source
                                    .warnings
                                    .push(format!("无法保存船舶缓存 {}: {e:#}", target.display()));
                            }
                        }
                        catalog
                    }
                    Err(e) => {
                        warnings.push(format!("读取当前客户端船舶属性失败: {e:#}"));
                        Self::fallback(home, warnings)
                    }
                }
            }
        };
        let result = Arc::new(result);
        shared.insert(key, Arc::clone(&result));
        result
    }
    pub fn lookup(&self, input: &str) -> ShipMetadata {
        let key = canonical_ship_key(input);
        self.entries
            .get(&key.to_ascii_uppercase())
            .cloned()
            .unwrap_or_else(|| ShipMetadata {
                key,
                known: false,
                level: None,
                nation: None,
                species: None,
                name_local: None,
                param: None,
                warning: Some(if input.trim().is_empty() {
                    "项目未指定目标船".into()
                } else {
                    format!("船舶属性未知: {}", input.trim())
                }),
            })
    }
    fn from_params(
        ships: &[ShipParam],
        names: &HashMap<String, String>,
        source: MetadataSource,
    ) -> Self {
        let entries = ships
            .iter()
            .filter(|ship| !ship.index.is_empty())
            .map(|ship| {
                let key = canonical_ship_key(&ship.index).to_ascii_uppercase();
                let nonempty = |text: &String| (!text.is_empty()).then(|| text.clone());
                let meta = ShipMetadata {
                    key: key.clone(),
                    known: true,
                    level: (ship.level > 0).then_some(ship.level),
                    nation: nonempty(&ship.nation),
                    species: nonempty(&ship.species),
                    name_local: names
                        .get(&format!("IDS_{key}"))
                        .filter(|s| !s.is_empty())
                        .cloned(),
                    param: nonempty(&ship.name),
                    warning: None,
                };
                (key, meta)
            })
            .collect();
        Self { source, entries }
    }
    fn from_cache(
        cached: CachedCatalog,
        path: PathBuf,
        kind: &str,
        stale: bool,
        mut warnings: Vec<String>,
    ) -> Self {
        warnings.extend(cached.warnings);
        Self {
            entries: cached.entries,
            source: MetadataSource {
                kind: kind.into(),
                root: Some(cached.root),
                build: Some(cached.build),
                cache_path: Some(path),
                stale,
                warnings,
            },
        }
    }
    fn fallback(home: &Path, mut warnings: Vec<String>) -> Self {
        let path = home.join("ship_metadata/last.json");
        match read_cache(&path) {
            Ok(cache) => {
                warnings.push("使用最近一次成功读取的船舶缓存，root/build 可能已过期".into());
                Self::from_cache(cache, path, "last_cache", true, warnings)
            }
            Err(e) => {
                if path.exists() {
                    warnings.push(format!("最近船舶缓存损坏或不可读，原件保留: {e:#}"));
                }
                warnings.push("没有可用的船舶属性缓存；等级、国家和类型保持未知".into());
                Self {
                    entries: BTreeMap::new(),
                    source: MetadataSource {
                        kind: "unavailable".into(),
                        root: None,
                        build: None,
                        cache_path: None,
                        stale: false,
                        warnings,
                    },
                }
            }
        }
    }
    fn read_game(
        game: &crate::game::GameInstall,
        root: &Path,
        cache_path: &Path,
    ) -> Result<(Self, CachedCatalog)> {
        let index = crate::load_index(game)?;
        let entry = index
            .get("content/GameParams_py3.data")
            .ok_or_else(|| anyhow!("索引没有 content/GameParams_py3.data"))?;
        let raw = wowsmod_formats::pkg::PkgReader::new(&game.res_packages)
            .read(entry)
            .map_err(|e| anyhow!("{e}"))?;
        let ships =
            wowsmod_formats::gameparams::ships_from_raw(&raw).map_err(|e| anyhow!("{e}"))?;
        if ships.is_empty() {
            bail!("GameParams 没有船舶条目");
        }
        let texts = game
            .idx_dir
            .parent()
            .ok_or_else(|| anyhow!("idx 没有构建目录"))?
            .join("res/texts");
        let (names, localization, mut warnings) = localized_names(&texts);
        if names.is_empty() {
            warnings.push("本地化船名不可用，name_local 保持未知".into());
        }
        let catalog = Self::from_params(
            &ships,
            &names,
            MetadataSource {
                kind: "game".into(),
                root: Some(root.to_path_buf()),
                build: Some(game.build.clone()),
                cache_path: Some(cache_path.to_path_buf()),
                stale: false,
                warnings,
            },
        );
        let cached = CachedCatalog {
            schema_version: 1,
            root: root.to_path_buf(),
            build: game.build.clone(),
            loaded_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            localization,
            warnings: catalog.source.warnings.clone(),
            entries: catalog.entries.clone(),
        };
        Ok((catalog, cached))
    }
}
fn same_root(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}
fn read_cache(path: &Path) -> Result<CachedCatalog> {
    let cache: CachedCatalog = serde_json::from_slice(&std::fs::read(path)?)
        .with_context(|| format!("解析 {}", path.display()))?;
    if cache.schema_version != 1 {
        bail!("未知船舶缓存 schema_version {}", cache.schema_version);
    }
    if cache
        .entries
        .iter()
        .any(|(key, meta)| key.is_empty() || meta.key != *key || !meta.known)
    {
        bail!("船舶缓存含无效条目");
    }
    Ok(cache)
}
fn localized_names(texts: &Path) -> (HashMap<String, String>, Option<PathBuf>, Vec<String>) {
    let mut warnings = vec![];
    let mut dirs: Vec<PathBuf> = match std::fs::read_dir(texts) {
        Ok(rd) => rd
            .filter_map(|e| match e {
                Ok(e) => Some(e.path()),
                Err(e) => {
                    warnings.push(format!("读取船名目录失败: {e}"));
                    None
                }
            })
            .collect(),
        Err(e) => {
            return (
                HashMap::new(),
                None,
                vec![format!("船名目录不可用 {}: {e}", texts.display())],
            )
        }
    };
    dirs.sort_by_key(|d| {
        let n = d
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        (!n.starts_with("zh"), n)
    });
    for dir in dirs {
        let path = dir.join("LC_MESSAGES/global.mo");
        if !path.exists() {
            continue;
        }
        match std::fs::read(&path)
            .map_err(|e| anyhow!("{e}"))
            .and_then(|b| wowsmod_formats::mo::parse_mo(&b).map_err(|e| anyhow!("{e}")))
        {
            Ok(names) => return (names, Some(path), warnings),
            Err(e) => warnings.push(format!("船名文件不可用 {}: {e}", path.display())),
        }
    }
    (HashMap::new(), None, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ship_aliases_preserve_unknown_and_empty_inputs() {
        for alias in [
            "FSB510_Bourgogne",
            "PFSB510_Bourgogne",
            "fsb510",
            "pfsb510",
            " FSB510_Bourgogne ",
        ] {
            assert_eq!(canonical_ship_key(alias), "PFSB510");
        }
        assert_eq!(canonical_ship_key(""), "");
        assert_eq!(canonical_ship_key("unknown-ship"), "unknown-ship");
        assert_eq!(canonical_ship_key("Bourgogne"), "Bourgogne");
    }
    fn source() -> MetadataSource {
        MetadataSource {
            kind: "game".into(),
            root: None,
            build: Some("1".into()),
            cache_path: None,
            stale: false,
            warnings: vec![],
        }
    }
    #[test]
    fn metadata_lookup_is_case_insensitive_and_unknown_is_explicit() {
        let ship = ShipParam {
            index: "PFSB510".into(),
            name: "PFSB510_Bourgogne".into(),
            level: 10,
            nation: "France".into(),
            species: "Battleship".into(),
            group: String::new(),
            hull_models: vec![],
            other_models: vec![],
        };
        let c = ShipCatalog::from_params(
            &[ship],
            &std::collections::HashMap::from([("IDS_PFSB510".into(), "勃艮第".into())]),
            source(),
        );
        let meta = c.lookup("fsb510_bourgogne");
        assert!(meta.known);
        assert_eq!(meta.level, Some(10));
        assert_eq!(meta.nation.as_deref(), Some("France"));
        assert_eq!(meta.name_local.as_deref(), Some("勃艮第"));
        for key in ["", "Bourgogne", "FSB999_Missing"] {
            let unknown = c.lookup(key);
            assert!(!unknown.known);
            assert_eq!(unknown.level, None);
            assert_eq!(unknown.nation, None);
            assert!(unknown.warning.is_some());
        }
    }
    #[test]
    fn unavailable_metadata_has_provenance_and_does_not_create_home() {
        let home = std::env::temp_dir().join(format!("wowsmod-ship-absent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let c = ShipCatalog::load(&home, None);
        assert_eq!(c.source.kind, "unavailable");
        assert!(!c.source.warnings.is_empty());
        assert!(!c.lookup("FSB510").known);
        assert!(!home.exists());
    }
    #[test]
    fn last_cache_is_explicitly_stale_when_client_is_unavailable() {
        let home = std::env::temp_dir().join(format!("wowsmod-ship-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("ship_metadata")).unwrap();
        std::fs::write(home.join("ship_metadata/last.json"), r#"{"schema_version":1,"root":"client","build":"123","loaded_unix":1,"localization":null,"warnings":["localized names unavailable"],"entries":{"PFSB510":{"key":"PFSB510","known":true,"level":10,"nation":"France","species":"Battleship","name_local":null,"param":"PFSB510_Bourgogne","warning":null}}}"#).unwrap();
        let c = ShipCatalog::load(&home, None);
        assert_eq!(c.source.kind, "last_cache");
        assert!(c.source.stale);
        assert_eq!(c.source.build.as_deref(), Some("123"));
        assert!(c.lookup("fsb510").known);
        assert!(!c.source.warnings.is_empty());
        assert!(c
            .source
            .warnings
            .iter()
            .any(|w| w == "localized names unavailable"));
        std::fs::remove_dir_all(home).unwrap();
    }
}
