//! Clean-room Oodle block decoder (MIT). Provenance rules: docs/oodle-cleanroom/ALLOWED_SOURCES.md.
//! Status: skeleton only -- no codec implemented yet.
#![forbid(unsafe_code)]

/// Block family as observed black-box from the first two bytes (corpus statistics only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family { Tag8c06, Tag8c0a, Tag8c0c, TagCc06, Other(u8, u8) }

pub fn family(block: &[u8]) -> Family {
    match block {
        [0x8c, 0x06, ..] => Family::Tag8c06,
        [0x8c, 0x0a, ..] => Family::Tag8c0a,
        [0x8c, 0x0c, ..] => Family::Tag8c0c,
        [0xcc, 0x06, ..] => Family::TagCc06,
        [a, b, ..] => Family::Other(*a, *b),
        _ => Family::Other(0, 0),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error { Unsupported(Family), Corrupt(&'static str) }

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { Error::Unsupported(x) => write!(f, "wowsmod-oodle: block family {x:?} not implemented"), Error::Corrupt(m) => write!(f, "wowsmod-oodle: corrupt: {m}") }
    }
}
impl std::error::Error for Error {}

/// Decode one independent Lesta block into `out` (len = expected size). Returns bytes written.
pub fn decompress_block(block: &[u8], out: &mut [u8]) -> Result<usize, Error> {
    let _ = out;
    Err(Error::Unsupported(family(block)))
}