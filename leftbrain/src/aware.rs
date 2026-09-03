//! AWARE own-path bake-off. No host xz.
pub const XZ1_RETIRED: &[&str] = &["mozilla", "samba", "sao", "ooffice"];

pub fn is_retired_wrap_slot(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    XZ1_RETIRED.iter().any(|s| *s == base)
}

pub fn encode(data: &[u8]) -> Option<(Vec<u8>, &'static str)> {
    crate::encode_best(data)
}

pub fn decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    crate::decode(buf)
}
