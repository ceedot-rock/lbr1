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

fn shannon_slice(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut cnt = [0u32; 256];
    for &b in data {
        cnt[b as usize] += 1;
    }
    let nf = data.len() as f64;
    let mut h = 0.0f64;
    for c in cnt {
        if c > 0 {
            let p = c as f64 / nf;
            h -= p * p.log2();
        }
    }
    h
}

#[derive(Clone, Copy, Debug)]
pub struct Bands {
    pub head: f64,
    pub mid: f64,
    pub tail: f64,
}

impl Bands {
    pub fn mean(self) -> f64 {
        (self.head + self.mid + self.tail) / 3.0
    }

    /// Interior is quieter than the header, still structured. BWT (x-ray).
    /// A near-zero tail is fill / seam, not a BWT seat (samba).
    pub fn cooler_body(self) -> bool {
        let quiet = self.mid.min(self.tail);
        quiet > 4.0 && quiet + 0.08 < self.head
    }

    /// Interior is hotter than the header. MATCH wavelength (ooffice).
    pub fn hotter_body(self) -> bool {
        self.mid.max(self.tail) > self.head + 0.08
    }
}

/// Head / mid / tail. One 64 KiB header lies (x-ray 6.51 vs body 6.24).
pub fn shannon_bands(data: &[u8]) -> Bands {
    const WIN: usize = 64 * 1024;
    let n = data.len();
    if n <= WIN {
        let h = shannon_slice(data);
        return Bands {
            head: h,
            mid: h,
            tail: h,
        };
    }
    let mid = n / 2;
    let tail = n - WIN;
    Bands {
        head: shannon_slice(&data[..WIN]),
        mid: shannon_slice(&data[mid..mid.saturating_add(WIN).min(n)]),
        tail: shannon_slice(&data[tail..]),
    }
}

pub fn shannon(data: &[u8]) -> f64 {
    shannon_bands(data).mean()
}

/// OmniWave seat names = mixture-of-experts routing, occupied by our engines.
/// Never gzip/brotli/xz. Never Distill NCA. Never a swarm in the inner loop.
pub fn omni_seat(kind: &str) -> &'static str {
    match kind {
        "tru8" | "tr8x" | "zero" => "ZRW_delegate",
        "bw22" | "bwt" => "struct_text",
        "lbr1" | "match" | "lzw1" => "general",
        "cmaq" | "pcaq" => "paq",
        "lbhm" | "seam" => "mixed",
        "store" | "stream" => "store",
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

pub fn window_for_class(n: usize, class: Class, asked: u32) -> usize {
    let n = n.max(256);
    match class {
        Class::Fill | Class::Sparse => 256,
        Class::Text | Class::Binary => (asked as usize).min(1 << 22).min(n),
    }
}

pub fn window_for(data: &[u8], asked: u32) -> usize {
    window_for_class(data.len(), classify(data), asked)
}

pub fn scouts_wanted_class(class: Class) -> bool {
    matches!(class, Class::Binary)
}

pub fn scouts_wanted(data: &[u8]) -> bool {
    scouts_wanted_class(classify(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shannon_zeros_vs_random() {
        assert!(shannon(&[0u8; 4096]) < 0.01);
        let r: Vec<u8> = (0..4096u32)
            .map(|i| (i.wrapping_mul(1103515245).wrapping_add(12345) >> 16) as u8)
            .collect();
        assert!(shannon(&r) > 7.0, "H={}", shannon(&r));
    }

    #[test]
    fn omni_seats_are_ours() {
        assert_eq!(omni_seat("tru8"), "ZRW_delegate");
        assert_eq!(omni_seat("tr8x"), "ZRW_delegate");
        assert_eq!(omni_seat("zero"), "ZRW_delegate");
        assert_eq!(omni_seat("bw22"), "struct_text");
        assert_eq!(omni_seat("bwt"), "struct_text");
        assert_eq!(omni_seat("lbr1"), "general");
        assert_eq!(omni_seat("match"), "general");
        assert_eq!(omni_seat("store"), "store");
        assert_eq!(omni_seat("stream"), "store");
    }
}

pub fn scout_stride_class(class: Class) -> usize {
    match class {
        Class::Binary => 64,
        Class::Text => 1024,
        _ => 256,
    }
}

pub fn scout_stride(data: &[u8]) -> usize {
    scout_stride_class(classify(data))
}
