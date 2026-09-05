//! AWARE own-path bake-off. No host xz.
//!
//! Combined GC used to emit XZ1 (host xz-9 + 8-byte wrap) on
//! mozilla / samba / sao / ooffice. Those four slots now go through
//! the same house as everything else: TRU8 / TR8X / LBR1 / pulsar BW22|OZL2.
//!
//! Smaller than the wrap on those four? Not yet. Honest, and ours.

pub const XZ1_RETIRED: &[&str] = &["mozilla", "samba", "sao", "ooffice"];

pub fn is_retired_wrap_slot(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    XZ1_RETIRED.iter().any(|s| *s == base)
}

/// Own-path only. Never shells xz/gzip/brotli/zstd.
pub fn encode(data: &[u8]) -> Option<(Vec<u8>, &'static str)> {
    crate::encode_best(data)
}

pub fn decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    crate::decode(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots() {
        assert!(is_retired_wrap_slot("mozilla"));
        assert!(is_retired_wrap_slot("/data/sao"));
        assert!(!is_retired_wrap_slot("dickens"));
    }

    #[test]
    fn no_xz_magic() {
        let s = vec![0u8; 4096];
        let (blob, kind) = encode(&s).expect("fill");
        assert_eq!(kind, "tru8");
        assert_ne!(&blob[..4], b"\xfd7zX"); // xz magic
        assert_eq!(decode(&blob).unwrap(), s);
    }
}
