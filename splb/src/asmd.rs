//! Altered State Morphic Distopic Encoding. UNLICENSED. Not published.

use crate::detect;
use crate::frame;
use crate::hybrid;
use crate::{decode_gene, encode_window};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

static KINETIC: AtomicBool = AtomicBool::new(true);
static FORCE: AtomicU8 = AtomicU8::new(0); // 0 none, 1 bw22, 2 lbr1, 3 hybrid

/// Kinetic = pulsar BW22 on text. `--max` also tries Combined GC.
pub fn set_kinetic(on: bool) {
    KINETIC.store(on, Ordering::Relaxed);
}

fn kinetic() -> bool {
    KINETIC.load(Ordering::Relaxed)
}

pub fn set_force_seat(s: Option<&str>) {
    let v = match s {
        Some("bw22") => 1,
        Some("lbr1") => 2,
        Some("hybrid") => 3,
        _ => 0,
    };
    FORCE.store(v, Ordering::Relaxed);
}

fn force() -> u8 {
    FORCE.load(Ordering::Relaxed)
}

pub const MAGIC: &[u8; 4] = b"ASMD";
pub const VERSION: u8 = 1;
pub const VERSION_HYBRID: u8 = 2;
pub const HDR: usize = 11; // magic + ver + morph + seat + raw_len
pub const AWARE_CAP: usize = 2_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Morph {
    Fill = 0,
    Sparse = 1,
    Text = 2,
    Structured = 3,
    Floats = 4,
    Binary = 5,
    Random = 6,
    Mixed = 7,
}

impl Morph {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Self::Fill,
            1 => Self::Sparse,
            2 => Self::Text,
            3 => Self::Structured,
            4 => Self::Floats,
            5 => Self::Binary,
            6 => Self::Random,
            7 => Self::Mixed,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fill => "fill",
            Self::Sparse => "sparse",
            Self::Text => "text",
            Self::Structured => "structured",
            Self::Floats => "floats",
            Self::Binary => "binary",
            Self::Random => "random",
            Self::Mixed => "mixed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Seat {
    Tru8 = 0,
    Tr8x = 1,
    Bw22 = 2,
    Lbr1 = 3,
    Lbhm = 4,
    Aware = 5,
    Store = 6,
}

impl Seat {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Self::Tru8,
            1 => Self::Tr8x,
            2 => Self::Bw22,
            3 => Self::Lbr1,
            4 => Self::Lbhm,
            5 => Self::Aware,
            6 => Self::Store,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tru8 => "tru8",
            Self::Tr8x => "tr8x",
            Self::Bw22 => "bw22",
            Self::Lbr1 => "lbr1",
            Self::Lbhm => "lbhm",
            Self::Aware => "aware",
            Self::Store => "store",
        }
    }
    pub fn omni(self) -> &'static str {
        detect::omni_seat(self.as_str())
    }
}

pub struct Pick {
    pub blob: Vec<u8>,
    pub morph: Morph,
    pub seat: Seat,
}

struct Stats {
    entropy: f64,
    printable: f64,
    zero_runs: usize,
    delta_var: f64,
}

fn stats(chunk: &[u8]) -> Stats {
    let mut freq = [0u32; 256];
    for &b in chunk {
        freq[b as usize] += 1;
    }
    let len = chunk.len() as f64;
    let mut h = 0.0;
    if len > 0.0 {
        for &f in &freq {
            if f > 0 {
                let p = f as f64 / len;
                h -= p * p.log2();
            }
        }
    }
    let printable = if len > 0.0 {
        chunk
            .iter()
            .filter(|&&b| (32..127).contains(&b) || b == 9 || b == 10 || b == 13)
            .count() as f64
            / len
    } else {
        0.0
    };
    let zero_runs = if chunk.len() >= 4 {
        chunk.windows(4).filter(|w| *w == [0, 0, 0, 0]).count()
    } else {
        0
    };
    let delta_var = if chunk.len() > 1 {
        let diffs: Vec<i16> = chunk
            .windows(2)
            .map(|w| w[1] as i16 - w[0] as i16)
            .collect();
        let mean = diffs.iter().map(|&x| x as f64).sum::<f64>() / diffs.len() as f64;
        diffs.iter().map(|&x| (x as f64 - mean).powi(2)).sum::<f64>() / diffs.len() as f64
    } else {
        0.0
    };
    Stats {
        entropy: h,
        printable,
        zero_runs,
        delta_var,
    }
}

fn morph_unit(data: &[u8]) -> Morph {
    if data.is_empty() || crate::frame::solid_run(data).is_some() {
        return Morph::Fill;
    }
    if crate::frame::sparse_mode(data).is_some() {
        return Morph::Sparse;
    }
    let s = stats(data);
    if s.printable > 0.80 && s.entropy < 6.0 {
        Morph::Text
    } else if s.delta_var < 10.0 && s.zero_runs > 5 {
        Morph::Structured
    } else if s.entropy > 7.9 {
        Morph::Random
    } else if s.entropy > 6.5 {
        Morph::Floats
    } else {
        Morph::Binary
    }
}

const WIN: usize = 64 * 1024;
const MIN_FILL: usize = 4096;
const MIN_MINOR: f64 = 0.08;

fn class_bucket(m: Morph) -> usize {
    match m {
        Morph::Fill => 0,
        Morph::Text => 1,
        Morph::Sparse => 3,
        _ => 2,
    }
}

/// Two real classes each ≥8%. Sparse counts only if ≥64 KiB and ≥8%. Tiny zero runs don't force hybrid.
pub fn really_mixed(data: &[u8]) -> bool {
    if force() == 3 {
        return true;
    }
    if force() == 1 || force() == 2 {
        return false;
    }
    if data.len() < MIN_FILL * 2 {
        return false;
    }
    let mut bytes = [0usize; 4];
    let mut i = 0usize;
    while i < data.len() {
        let e = (i + WIN).min(data.len());
        let b = class_bucket(morph_unit(&data[i..e]));
        bytes[b] += e - i;
        i = e;
    }
    let n = data.len() as f64;
    let fill_ok = bytes[0] as f64 / n >= MIN_MINOR && bytes[0] >= 64 * 1024;
    let text_ok = bytes[1] as f64 / n >= MIN_MINOR;
    let bin_ok = bytes[2] as f64 / n >= MIN_MINOR;
    let sparse_ok = bytes[3] as f64 / n >= MIN_MINOR && bytes[3] >= 64 * 1024;
    [fill_ok, text_ok, bin_ok, sparse_ok]
        .iter()
        .filter(|x| **x)
        .count()
        >= 2
}

pub fn morph(data: &[u8]) -> Morph {
    if really_mixed(data) {
        Morph::Mixed
    } else {
        morph_unit(data)
    }
}

fn log_slice(off: usize, len: usize, m: Morph, seat: Seat, coded: usize) {
    eprintln!(
        "asmd slice off={} len={} morph={} seat={} coded={}",
        off,
        len,
        m.as_str(),
        seat.as_str(),
        coded
    );
    if let Ok(path) = std::env::var("ASMDE_TSV") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(
                f,
                "{}\t{}\t{}\t{}\t{}",
                off,
                len,
                m.as_str(),
                seat.as_str(),
                coded
            );
        }
    }
}

fn skip_aware(data: &[u8]) -> bool {
    if data.len() > aware_cap() {
        return true;
    }
    stats(data).entropy > 7.5
}

fn aware_cap() -> usize {
    std::env::var("ASMDE_AWARE_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(AWARE_CAP)
}

fn ok_gene(raw: &[u8], blob: &[u8]) -> bool {
    if blob.is_empty() {
        return false;
    }
    if blob.len() >= raw.len() && !(frame::is_tru8(blob) && blob.len() == 8) {
        return false;
    }
    match decode_gene(blob) {
        Ok(back) => back == raw,
        Err(_) => false,
    }
}

fn consider(best: &mut Option<(Vec<u8>, Seat)>, raw: &[u8], blob: Vec<u8>, seat: Seat) {
    if !ok_gene(raw, &blob) {
        return;
    }
    match best {
        None => *best = Some((blob, seat)),
        Some((b, _)) if blob.len() < b.len() => *best = Some((blob, seat)),
        _ => {}
    }
}

fn try_pulsar(data: &[u8]) -> Option<Vec<u8>> {
    pulsar::pulsar_encode(data)
}

fn try_hybrid(data: &[u8]) -> Option<Vec<u8>> {
    hybrid::pack(data, true)
}

#[cfg(feature = "aware")]
fn try_aware(data: &[u8]) -> Option<Vec<u8>> {
    if kinetic() || skip_aware(data) {
        return None;
    }
    Some(combined_gc::codec::encode(data, combined_gc::codec::Mode::Max).bytes)
}

#[cfg(not(feature = "aware"))]
fn try_aware(_data: &[u8]) -> Option<Vec<u8>> {
    None
}

fn seats_for(m: Morph, n: usize) -> &'static [Seat] {
    if kinetic() {
        return match m {
            Morph::Fill => &[Seat::Tru8],
            Morph::Sparse => &[Seat::Tr8x, Seat::Lbr1],
            Morph::Text => &[Seat::Bw22],
            Morph::Structured => &[Seat::Tru8, Seat::Bw22],
            Morph::Floats => {
                if n >= 8 * 1024 * 1024 {
                    &[Seat::Bw22]
                } else {
                    &[Seat::Lbr1]
                }
            }
            Morph::Binary => {
                if n < 12 * 1024 * 1024 {
                    &[Seat::Bw22]
                } else {
                    &[Seat::Lbr1]
                }
            }
            Morph::Random => &[Seat::Bw22],
            Morph::Mixed => &[Seat::Bw22, Seat::Lbr1],
        };
    }
    match m {
        Morph::Fill => &[Seat::Tru8],
        Morph::Sparse => &[Seat::Tr8x, Seat::Lbr1],
        Morph::Text => &[Seat::Aware, Seat::Bw22, Seat::Lbr1],
        Morph::Structured => &[Seat::Aware, Seat::Tru8, Seat::Lbr1],
        Morph::Floats => &[Seat::Bw22, Seat::Lbr1, Seat::Aware],
        Morph::Binary => &[Seat::Lbr1, Seat::Aware],
        Morph::Random => &[Seat::Lbr1],
        Morph::Mixed => &[Seat::Lbhm, Seat::Lbr1, Seat::Bw22, Seat::Aware],
    }
}

fn run_seat(data: &[u8], seat: Seat) -> Option<Vec<u8>> {
    match seat {
        Seat::Tru8 => {
            let (sym, n) = frame::solid_run(data)?;
            let b = frame::pack_tru8(sym, n);
            if frame::is_tru8(&b) {
                Some(b)
            } else {
                None
            }
        }
        Seat::Tr8x => {
            let (sym, _) = frame::sparse_mode(data)?;
            let b = frame::pack_tr8x(data, sym);
            if frame::is_tr8x(&b) && b.len() < data.len() {
                Some(b)
            } else {
                None
            }
        }
        Seat::Lbr1 => {
            let w = if data.len() > 256 * 1024 {
                crate::parse::DEFAULT_WINDOW as u32
            } else {
                (256 * 1024) as u32
            };
            encode_window(data, w).filter(|b| !frame::is_tru8(b) && !frame::is_tr8x(b))
        }
        Seat::Bw22 => try_pulsar(data),
        Seat::Lbhm => try_hybrid(data),
        Seat::Aware => try_aware(data),
        Seat::Store => Some(data.to_vec()),
    }
}

pub fn wrap(morph: Morph, seat: Seat, raw_len: u32, inner: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HDR + inner.len());
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.push(morph as u8);
    out.push(seat as u8);
    out.extend_from_slice(&raw_len.to_le_bytes());
    out.extend_from_slice(inner);
    out
}

pub fn is_asmd(buf: &[u8]) -> bool {
    buf.len() >= HDR
        && buf.starts_with(MAGIC)
        && (buf[4] == VERSION || buf[4] == VERSION_HYBRID)
}

pub fn unwrap(buf: &[u8]) -> Result<(Morph, Seat, u32, &[u8]), &'static str> {
    if buf.len() < HDR || !buf.starts_with(MAGIC) || buf[4] != VERSION {
        return Err("not ASMD");
    }
    let morph = Morph::from_u8(buf[5]).ok_or("asmd morph")?;
    let seat = Seat::from_u8(buf[6]).ok_or("asmd seat")?;
    let raw_len = u32::from_le_bytes(buf[7..11].try_into().unwrap());
    Ok((morph, seat, raw_len, &buf[HDR..]))
}

fn decode_inner(seat: Seat, inner: &[u8]) -> Result<Vec<u8>, &'static str> {
    if seat == Seat::Store {
        return Ok(inner.to_vec());
    }
    decode_gene(inner)
}

pub fn decode_frame(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if buf.len() >= HDR && buf.starts_with(MAGIC) && buf[4] == VERSION_HYBRID {
        return unpack_hybrid(buf);
    }
    let (_m, s, raw_len, inner) = unwrap(buf)?;
    let out = decode_inner(s, inner)?;
    if out.len() as u32 != raw_len {
        return Err("asmd length");
    }
    Ok(out)
}

struct Unit {
    morph: Morph,
    seat: Seat,
    inner: Vec<u8>,
}

fn wants_chunk(m: Morph) -> bool {
    matches!(
        m,
        Morph::Text | Morph::Structured | Morph::Floats | Morph::Binary
    )
}

fn pick_unit(data: &[u8]) -> Unit {
    if force() == 1 {
        if let Some(b) = try_pulsar(data) {
            if ok_gene(data, &b) {
                return Unit {
                    morph: morph_unit(data),
                    seat: Seat::Bw22,
                    inner: b,
                };
            }
        }
    }
    if force() == 2 {
        if let Some(b) = run_seat(data, Seat::Lbr1) {
            return Unit {
                morph: morph_unit(data),
                seat: Seat::Lbr1,
                inner: b,
            };
        }
    }
    let m = match morph_unit(data) {
        Morph::Mixed => Morph::Binary,
        other => other,
    };
    let bake = !kinetic();
    let mut best: Option<(Vec<u8>, Seat)> = None;
    for &seat in seats_for(m, data.len()) {
        if seat == Seat::Lbhm {
            continue;
        }
        if let Some(blob) = run_seat(data, seat) {
            consider(&mut best, data, blob, seat);
            if !bake && best.is_some() {
                break;
            }
        }
    }
    match best {
        Some((inner, seat)) => Unit {
            morph: m,
            seat,
            inner,
        },
        None => Unit {
            morph: m,
            seat: Seat::Store,
            inner: data.to_vec(),
        },
    }
}

fn pack_hybrid(raw_len: u32, units: &[Unit]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.push(VERSION_HYBRID);
    out.extend_from_slice(&(units.len() as u16).to_le_bytes());
    out.extend_from_slice(&raw_len.to_le_bytes());
    for u in units {
        out.push(u.morph as u8);
        out.push(u.seat as u8);
        out.extend_from_slice(&(u.inner.len() as u32).to_le_bytes());
        out.extend_from_slice(&u.inner);
    }
    out
}

fn unpack_hybrid(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if buf.len() < 11 || !buf.starts_with(MAGIC) || buf[4] != VERSION_HYBRID {
        return Err("asmd hybrid");
    }
    let n = u16::from_le_bytes(buf[5..7].try_into().unwrap()) as usize;
    let raw_len = u32::from_le_bytes(buf[7..11].try_into().unwrap()) as usize;
    let mut i = 11usize;
    let mut out = Vec::with_capacity(raw_len);
    for _ in 0..n {
        if i + 6 > buf.len() {
            return Err("asmd hybrid hdr");
        }
        let _morph = Morph::from_u8(buf[i]).ok_or("asmd hybrid morph")?;
        i += 1;
        let seat = Seat::from_u8(buf[i]).ok_or("asmd hybrid seat")?;
        i += 1;
        let blob_len = u32::from_le_bytes(buf[i..i + 4].try_into().unwrap()) as usize;
        i += 4;
        if i + blob_len > buf.len() {
            return Err("asmd hybrid body");
        }
        let inner = &buf[i..i + blob_len];
        i += blob_len;
        out.extend_from_slice(&decode_inner(seat, inner)?);
    }
    if i != buf.len() || out.len() != raw_len {
        return Err("asmd hybrid len");
    }
    Ok(out)
}

fn finish_unit(off: usize, data: &[u8], u: Unit) -> Unit {
    log_slice(off, data.len(), u.morph, u.seat, u.inner.len());
    u
}

fn encode_uniform(data: &[u8], m: Morph) -> Option<Pick> {
    if m == Morph::Text && !kinetic() {
        let cap = aware_cap();
        if data.len() > cap {
            let mut units = Vec::new();
            let mut off = 0usize;
            for chunk in data.chunks(cap) {
                units.push(finish_unit(off, chunk, pick_unit(chunk)));
                off += chunk.len();
            }
            let packed = pack_hybrid(data.len() as u32, &units);
            if decode_pick(&packed).ok().as_deref() != Some(data) {
                return None;
            }
            return Some(Pick {
                blob: packed,
                morph: Morph::Text,
                seat: Seat::Lbhm,
            });
        }
    }
    let u = finish_unit(0, data, pick_unit(data));
    let blob = u.inner.clone();
    if decode_pick(&blob).ok().as_deref() != Some(data) {
        return None;
    }
    Some(Pick {
        blob,
        morph: u.morph,
        seat: u.seat,
    })
}

pub fn encode(data: &[u8]) -> Option<Pick> {
    if data.is_empty() {
        return None;
    }
    if really_mixed(data) {
        let cuts = crate::hybrid::cuts(data);
        let cuts = if cuts.is_empty() {
            vec![(0, data.len())]
        } else {
            cuts
        };
        if data.len() < 12 * 1024 * 1024 && cuts.len() > 8 {
            return encode_uniform(data, morph_unit(data));
        }
        let mut units: Vec<Unit> = Vec::new();
        for (start, end) in cuts {
            let slice = &data[start..end];
            let m = morph_unit(slice);
            let cap = if m == Morph::Text {
                900 * 1024
            } else {
                crate::parse::DEFAULT_WINDOW
            };
            if wants_chunk(m) && slice.len() > cap {
                let mut off = start;
                for chunk in slice.chunks(cap) {
                    units.push(finish_unit(off, chunk, pick_unit(chunk)));
                    off += chunk.len();
                }
            } else {
                units.push(finish_unit(start, slice, pick_unit(slice)));
            }
        }
        let (blob, seat) = if units.len() == 1 {
            (units[0].inner.clone(), units[0].seat)
        } else {
            (pack_hybrid(data.len() as u32, &units), Seat::Lbhm)
        };
        if decode_pick(&blob).ok().as_deref() != Some(data) {
            return None;
        }
        return Some(Pick {
            blob,
            morph: Morph::Mixed,
            seat,
        });
    }
    encode_uniform(data, morph_unit(data))
}

fn decode_pick(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if is_asmd(buf) {
        decode_frame(buf)
    } else {
        decode_gene(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeros_stay_eight() {
        let s = vec![0u8; 10_000];
        let p = encode(&s).expect("fill");
        assert_eq!(p.morph, Morph::Fill);
        assert_eq!(p.seat, Seat::Tru8);
        assert_eq!(p.blob.len(), 8);
        assert!(!is_asmd(&p.blob));
        assert_eq!(decode_pick(&p.blob).unwrap(), s);
    }

    #[test]
    fn text_roundtrip() {
        let s = b"the cat sat on the mat. ".repeat(80);
        let p = encode(&s).expect("text");
        assert_eq!(p.morph, Morph::Text);
        assert_ne!(p.seat, Seat::Lbhm);
        assert_eq!(decode_pick(&p.blob).unwrap(), s);
        assert!(p.blob.len() < s.len());
    }

    #[test]
    fn wrap_header_round() {
        let inner = b"abcd";
        let w = wrap(Morph::Binary, Seat::Lbr1, 4, inner);
        let (m, s, n, got) = unwrap(&w).unwrap();
        assert_eq!(m, Morph::Binary);
        assert_eq!(s, Seat::Lbr1);
        assert_eq!(n, 4);
        assert_eq!(got, inner);
    }

    #[test]
    fn hybrid_zeros_then_text() {
        let mut s = vec![0u8; 8192];
        s.extend_from_slice(b"the cat sat on the mat. ".repeat(400).as_slice());
        s.extend_from_slice(&[0u8; 4096]);
        let p = encode(&s).expect("hybrid");
        assert_eq!(decode_pick(&p.blob).unwrap(), s);
        assert!(p.blob.len() < s.len());
    }

    #[test]
    fn random_does_not_invent() {
        let s: Vec<u8> = (0..2048u32)
            .map(|i| (i.wrapping_mul(1103515245).wrapping_add(12345) >> 16) as u8)
            .collect();
        assert_eq!(morph(&s), Morph::Random);
        if let Some(p) = encode(&s) {
            assert_eq!(decode_pick(&p.blob).unwrap(), s);
            assert!(p.blob.len() < s.len());
        }
    }
}
