//! LBHM mixed-file packer. One gene per segment. Not Combined GC.
//!
//! Split only when the file is actually mixed. Uniform binaries
//! (mozilla, ooffice) stay one LBR1 window so far copies survive.
//! Coalesce same-class runs. Keep only if crate::decode roundtrips.

use crate::detect::{self, Class};
use crate::frame;

pub const MAGIC: &[u8; 4] = b"LBHM";
pub const KIND_TRU8: u8 = 1;
pub const KIND_TR8X: u8 = 2;
pub const KIND_LBR1: u8 = 3;
pub const KIND_BW22: u8 = 4;

const WIN: usize = 64 * 1024;
const MIN_FILL: usize = 4096;
const MIN_MINOR: f64 = 0.08;

#[derive(Clone, Copy, Debug)]
struct Seg {
    start: usize,
    end: usize,
    class: Class,
}

fn solid_len(data: &[u8], i: usize) -> usize {
    if i >= data.len() {
        return 0;
    }
    let s = data[i];
    let mut n = 1;
    while i + n < data.len() && data[i + n] == s {
        n += 1;
    }
    n
}

fn window_class(data: &[u8], start: usize, end: usize) -> Class {
    detect::classify(&data[start..end])
}

pub fn is_mixed(data: &[u8]) -> bool {
    if data.len() < MIN_FILL {
        return false;
    }
    let segs = segments(data);
    if segs.len() < 2 {
        return false;
    }
    let n = data.len() as f64;
    let mut bytes = [0usize; 4];
    for s in &segs {
        let k = match s.class {
            Class::Fill => 0,
            Class::Sparse => 1,
            Class::Text => 2,
            Class::Binary => 3,
        };
        bytes[k] += s.end - s.start;
    }
    let present = bytes
        .iter()
        .enumerate()
        .filter(|&(i, &b)| {
            if i == 0 {
                b >= MIN_FILL
            } else {
                b as f64 / n >= MIN_MINOR
            }
        })
        .count();
    present >= 2
}

fn segments(data: &[u8]) -> Vec<Seg> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < data.len() {
        let sl = solid_len(data, i);
        if sl >= MIN_FILL {
            out.push(Seg {
                start: i,
                end: i + sl,
                class: Class::Fill,
            });
            i += sl;
            continue;
        }
        let e = (i + WIN).min(data.len());
        let c = window_class(data, i, e);
        if let Some(last) = out.last_mut() {
            if last.class == c && last.end == i && c != Class::Fill {
                last.end = e;
                i = e;
                continue;
            }
        }
        out.push(Seg {
            start: i,
            end: e,
            class: c,
        });
        i = e;
    }
    let mut merged: Vec<Seg> = Vec::new();
    for s in out {
        if let Some(last) = merged.last_mut() {
            if s.end - s.start < WIN / 2 && s.class != Class::Fill && last.class != Class::Fill {
                last.end = s.end;
                last.class = Class::Binary;
                continue;
            }
            if last.class == s.class && last.end == s.start {
                last.end = s.end;
                continue;
            }
        }
        merged.push(s);
    }
    merged
}

fn encode_seg(data: &[u8], allow_bw: bool) -> Option<(Vec<u8>, u8)> {
    match detect::classify(data) {
        Class::Fill => {
            if let Some((sym, n)) = frame::solid_run(data) {
                return Some((frame::pack_tru8(sym, n), KIND_TRU8));
            }
        }
        Class::Sparse => {
            if let Some((sym, _)) = frame::sparse_mode(data) {
                let b = frame::pack_tr8x(data, sym);
                if b.len() < data.len() {
                    return Some((b, KIND_TR8X));
                }
            }
        }
        Class::Text if allow_bw => {
            if let Some(b) = pulsar::pulsar_encode(data) {
                if b.len() < data.len() {
                    return Some((b, KIND_BW22));
                }
            }
        }
        _ => {}
    }
    let w = detect::window_for(data, crate::parse::DEFAULT_WINDOW as u32);
    let toks = crate::parse::parse(data, w);
    let blob = frame::pack(&toks, data.len(), w as u32, data);
    if blob.len() < data.len() && crate::decode_lbr1(&blob).ok().as_deref() == Some(data) {
        Some((blob, KIND_LBR1))
    } else {
        None
    }
}

pub fn pack(data: &[u8], allow_bw: bool) -> Option<Vec<u8>> {
    if !is_mixed(data) {
        return None;
    }
    let segs = segments(data);
    if segs.len() < 2 {
        return None;
    }
    let mut body = Vec::new();
    body.extend_from_slice(MAGIC);
    body.extend_from_slice(&(segs.len() as u16).to_le_bytes());
    for s in &segs {
        let slice = &data[s.start..s.end];
        let (blob, kind) = encode_seg(slice, allow_bw)?;
        body.push(kind);
        body.extend_from_slice(&(slice.len() as u32).to_le_bytes());
        body.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        body.extend_from_slice(&blob);
    }
    if crate::decode(&body).ok().as_deref() != Some(data) {
        return None;
    }
    Some(body)
}

pub fn unpack(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if buf.len() < 6 || &buf[..4] != MAGIC {
        return Err("lbhm");
    }
    let n = u16::from_le_bytes(buf[4..6].try_into().unwrap()) as usize;
    let mut i = 6usize;
    let mut out = Vec::new();
    for _ in 0..n {
        if i + 9 > buf.len() {
            return Err("lbhm hdr");
        }
        let kind = buf[i];
        i += 1;
        let raw_len = u32::from_le_bytes(buf[i..i + 4].try_into().unwrap()) as usize;
        i += 4;
        let blob_len = u32::from_le_bytes(buf[i..i + 4].try_into().unwrap()) as usize;
        i += 4;
        if i + blob_len > buf.len() {
            return Err("lbhm body");
        }
        let blob = &buf[i..i + blob_len];
        i += blob_len;
        let part = match kind {
            KIND_TRU8 => frame::unpack_tru8(blob)?,
            KIND_TR8X => frame::unpack_tr8x(blob)?,
            KIND_LBR1 => crate::decode_lbr1(blob)?,
            KIND_BW22 => pulsar::pulsar_decode(blob)?,
            _ => return Err("lbhm kind"),
        };
        if part.len() != raw_len {
            return Err("lbhm len");
        }
        out.extend_from_slice(&part);
    }
    if i != buf.len() {
        return Err("lbhm tail");
    }
    Ok(out)
}

pub fn is_hybrid(buf: &[u8]) -> bool {
    buf.len() >= 4 && &buf[..4] == MAGIC
}
