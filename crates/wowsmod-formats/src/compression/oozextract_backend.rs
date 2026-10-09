//! 基于 oozextract 0.5.5 (crates.io, Cargo.toml 声明 MIT; 上游衍生关系/许可证待确认) 的 Oodle 后端。
//! 仅在 feature "oozextract" 打开时编译。
use super::OodleBackend;

pub struct OozExtract;

impl OodleBackend for OozExtract {
    fn name(&self) -> &'static str {
        "oozextract 0.5.5"
    }
    fn decompress(&self, input: &[u8], output: &mut [u8]) -> Result<usize, String> {
        oozextract::Extractor::new().read_from_slice(input, output).map_err(|e| format!("{e:?}"))
    }
}
