//! 运行时 Oodle 后端: 动态加载用户本机已有的 Oodle 运行库(不随本项目分发)。
//! 查找顺序: 环境变量 WOWSMOD_OODLE(文件或目录) → wowsmod 可执行文件同目录 → 游戏目录(深度 ≤ 3)。
//! 详见 README「需要自行下载的组件 / Optional external components」。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use wowsmod_formats::compression::{self, OodleBackend};

pub const README_HINT: &str = "README「需要自行下载的组件 / Optional external components」";

static GAME_ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
static LOADED: OnceLock<Result<PathBuf, String>> = OnceLock::new();

/// 在 main 早期调用: 记录游戏根目录(可为 None, 届时再自动定位)并注册延迟加载器。
pub fn install(game: Option<&Path>) {
    let _ = GAME_ROOT.set(game.map(Path::to_path_buf));
    compression::set_oodle_loader(loader);
}

fn lib_name_ok(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let ext_ok = if cfg!(windows) { n.ends_with(".dll") } else { n.ends_with(".so") || n.contains(".so.") };
    ext_ok && (n.starts_with("oo2core") || n.starts_with("liboo2core") || n.starts_with("oodle"))
}

fn scan_dir(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut subs = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 { subs.push(p); }
        } else if p.file_name().and_then(|s| s.to_str()).map(lib_name_ok).unwrap_or(false) {
            out.push(p);
        }
    }
    subs.sort();
    for s in subs { scan_dir(&s, depth - 1, out); }
}

/// 列出候选库(按优先级, 去重)。`env`/`exe_dir`/`game_root` 拆成参数以便测试。
pub fn candidates(env: Option<&Path>, exe_dir: Option<&Path>, game_root: Option<&Path>) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(e) = env {
        if e.is_file() { v.push(e.to_path_buf()); } else { scan_dir(e, 0, &mut v); }
    }
    if let Some(d) = exe_dir { scan_dir(d, 0, &mut v); }
    if let Some(g) = game_root { scan_dir(g, 3, &mut v); }
    let mut seen = std::collections::HashSet::new();
    v.retain(|p| seen.insert(p.clone()));
    v
}

fn game_root() -> Option<PathBuf> {
    if let Some(Some(g)) = GAME_ROOT.get() { return Some(g.clone()); }
    crate::game::locate(None).0.map(|g| g.root)
}

pub fn default_candidates() -> Vec<PathBuf> {
    let env = std::env::var_os("WOWSMOD_OODLE").map(PathBuf::from);
    let exe = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    candidates(env.as_deref(), exe.as_deref(), game_root().as_deref())
}

// OodleLZ_Decompress(comp, compLen, raw, rawLen, fuzzSafe, checkCRC, verbosity,
//   decBufBase, decBufSize, fpCallback, cbUserData, decoderMemory, decoderMemorySize, threadPhase) -> SINTa
type DecompressFn = unsafe extern "C" fn(
    *const u8, isize, *mut u8, isize, i32, i32, i32,
    *mut u8, isize, *const std::ffi::c_void, *const std::ffi::c_void, *mut u8, isize, i32,
) -> isize;

struct DynOodle {
    _lib: libloading::Library,
    f: DecompressFn,
}

impl OodleBackend for DynOodle {
    fn name(&self) -> &'static str { "oodle-runtime (本机)" }
    fn decompress(&self, input: &[u8], output: &mut [u8]) -> Result<usize, String> {
        // SAFETY: 指针与长度来自有效切片; 其余参数为 Oodle 文档中的默认值(无回调/无外部内存)。
        let n = unsafe {
            (self.f)(input.as_ptr(), input.len() as isize, output.as_mut_ptr(), output.len() as isize,
                1, 0, 0, std::ptr::null_mut(), 0, std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), 0, 3)
        };
        if n <= 0 { Err(format!("OodleLZ_Decompress 返回 {n}")) } else { Ok(n as usize) }
    }
}

fn try_load(p: &Path) -> Result<DynOodle, String> {
    // SAFETY: 加载用户本机的 Oodle 运行库; 只解析一个已知签名的导出函数。
    unsafe {
        let lib = libloading::Library::new(p).map_err(|e| format!("{}: {e}", p.display()))?;
        let f: DecompressFn = *lib.get::<DecompressFn>(b"OodleLZ_Decompress\0").map_err(|e| format!("{}: {e}", p.display()))?;
        Ok(DynOodle { _lib: lib, f })
    }
}

fn loader() -> Option<Box<dyn OodleBackend>> {
    let mut errs = Vec::new();
    for c in default_candidates() {
        match try_load(&c) {
            Ok(b) => { let _ = LOADED.set(Ok(c)); return Some(Box::new(b)); }
            Err(e) => errs.push(e),
        }
    }
    let _ = LOADED.set(Err(if errs.is_empty() { "未找到 Oodle 运行库".into() } else { errs.join("; ") }));
    None
}

/// doctor 用: 尝试加载并返回 JSON 状态。
pub fn status() -> serde_json::Value {
    let ok = compression::oodle_backend().is_some();
    let detail = match LOADED.get() {
        Some(Ok(p)) => serde_json::json!({"loaded": p}),
        Some(Err(e)) => serde_json::json!({"error": e}),
        None => serde_json::json!({}),
    };
    serde_json::json!({
        "available": ok, "detail": detail, "candidates": default_candidates(),
        "platform_supported": cfg!(windows),
        "env": "WOWSMOD_OODLE=<oo2core*.dll 或其所在目录>",
        "help": README_HINT,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn touch(p: &Path) { std::fs::create_dir_all(p.parent().unwrap()).unwrap(); std::fs::write(p, b"x").unwrap(); }

    #[test]
    fn finds_candidates_in_priority_order() {
        let t = std::env::temp_dir().join(format!("wowsmod_oodle_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let ext = if cfg!(windows) { "dll" } else { "so" };
        let envf = t.join(format!("env/oo2core_9_win64.{ext}"));
        let exef = t.join(format!("exe/oo2core_8_win64.{ext}"));
        let gamef = t.join(format!("game/bin/123/bin64/oo2core_9_win64.{ext}"));
        let too_deep = t.join(format!("game/a/b/c/d/oo2core_9_win64.{ext}"));
        let other = t.join("game/bin/readme.txt");
        for p in [&envf, &exef, &gamef, &too_deep, &other] { touch(p); }
        let v = candidates(Some(&envf), Some(&t.join("exe")), Some(&t.join("game")));
        assert_eq!(v, vec![envf.clone(), exef.clone(), gamef.clone()]);
        let v2 = candidates(Some(&t.join("env")), None, None);
        assert_eq!(v2, vec![envf]);
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn rejects_non_library() {
        let t = std::env::temp_dir().join(format!("wowsmod_oodle_bad_{}", std::process::id()));
        let f = t.join(if cfg!(windows) { "oo2core_9_win64.dll" } else { "liboo2corelinux64.so" });
        touch(&f);
        assert!(try_load(&f).is_err());
        let _ = std::fs::remove_dir_all(&t);
    }
}
