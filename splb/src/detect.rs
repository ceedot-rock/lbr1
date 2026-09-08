//! Route before the brain. Step 2.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Fill,
    Sparse,
    Text,
    Binary,
}

pub fn printable_ratio(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let n = data.len().min(64 * 1024);
    let mut p = 0usize;
    for &b in &data[..n] {
        if (32..127).contains(&b) || b == 9 || b == 10 || b == 13 {
            p += 1;
        }
    }
    p as f64 / n as f64
}

/// OmniWave seat names = mixture-of-experts routing, occupied by our engines.
/// Never gzip/brotli/xz. Never Distill NCA. Never a swarm in the inner loop.
pub fn omni_seat(kind: &str) -> &'static str {
    match kind {
        "tru8" | "tr8x" => "ZRW_delegate",
        "bw22" => "struct_text",
        "lbr1" => "general",
        "lbhm" => "mixed",
        "elide" | "arth" => "CDDG",
        "gc" | "aware" | "gcr1" => "AWARE",
        _ => "general",
    }
}

pub fn classify(data: &[u8]) -> Class {
    if data.is_empty() {
        return Class::Fill;
    }
    if crate::frame::solid_run(data).is_some() {
        return Class::Fill;
    }
    if crate::frame::sparse_mode(data).is_some() {
        return Class::Sparse;
    }
    if printable_ratio(data) >= 0.85 {
        return Class::Text;
    }
    Class::Binary
}

pub fn window_for(data: &[u8], asked: u32) -> usize {
    let n = data.len().max(256);
    match classify(data) {
        Class::Fill | Class::Sparse => 256,
        Class::Text | Class::Binary => (asked as usize).min(1 << 22).min(n),
    }
}

pub fn scouts_wanted(data: &[u8]) -> bool {
    matches!(classify(data), Class::Binary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omni_seats_are_ours() {
        assert_eq!(omni_seat("tru8"), "ZRW_delegate");
        assert_eq!(omni_seat("tr8x"), "ZRW_delegate");
        assert_eq!(omni_seat("bw22"), "struct_text");
        assert_eq!(omni_seat("lbr1"), "general");
    }
}

pub fn scout_stride(data: &[u8]) -> usize {
    match classify(data) {
        Class::Binary => 64,
        Class::Text => 1024,
        _ => 256,
    }
}
