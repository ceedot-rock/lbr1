//! STR1 — own format-aware transforms. Absorbs OpenZL (Meta, BSD) structure:
//! detect record width / numeric lanes, transpose, xor-delta, then an own gene.
//! Not a wrap of OpenZL. Host xz is not an occupant.

use crate::detect;
use crate::lzm;
use crate::rans;
use crate::wrap;

pub const MAGIC: &[u8; 4] = b"STR1";
pub const VER: u8 = 1;

const XF_DELTA: u8 = 1;
const XF_TRANSPOSE: u8 = 2;
const XF_XOR: u8 = 3;
const XF_DELTA_T: u8 = 4;
const XF_XOR_T: u8 = 5;

const INNER_STORE: u8 = 0;
const INNER_LZ: u8 = 1;
const INNER_LZM: u8 = 2;
const INNER_RANS: u8 = 3;

pub fn is_str(buf: &[u8]) -> bool {
    buf.len() >= 16 && buf.starts_with(MAGIC) && buf[4] == VER
}

fn shannon(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut cnt = [0u32; 256];
    for &b in data {
        cnt[b as usize] += 1;
    }
    let n = data.len() as f64;
    let mut h = 0.0f64;
    for c in cnt {
        if c > 0 {
            let p = c as f64 / n;
            h -= p * p.log2();
        }
    }
    h
}

fn col_entropy(data: &[u8], w: usize, col: usize) -> f64 {
    let n = data.len() / w;
    if n == 0 {
        return 8.0;
    }
    let take = n.min(8192);
    let mut buf = Vec::with_capacity(take);
    for r in 0..take {
        buf.push(data[r * w + col]);
    }
    shannon(&buf)
}

fn mean_col_h(data: &[u8], w: usize) -> f64 {
    let mut s = 0.0;
    for c in 0..w {
        s += col_entropy(data, w, c);
    }
    s / w as f64
}

fn best_width(data: &[u8]) -> Option<usize> {
    let probe = &data[..data.len().min(64 * 1024)];
    if probe.len() < 256 {
        return None;
    }
    let h0 = shannon(probe);
    if h0 < 2.0 {
        return None;
    }
    let mut best_w = 0usize;
    let mut best_h = h0;
    for w in 2..=128 {
        if probe.len() < w * 16 {
            continue;
        }
        let n = probe.len() / w * w;
        if n == 0 {
            continue;
        }
        let h = mean_col_h(&probe[..n], w);
        if h + 0.35 < best_h {
            best_h = h;
            best_w = w;
        }
    }
    if best_w >= 2 && best_h + 0.35 < h0 {
        Some(best_w)
    } else {
        None
    }
}

fn delta_w(data: &[u8], w: usize) -> Vec<u8> {
    let mut o = data.to_vec();
    for i in w..o.len() {
        o[i] = data[i].wrapping_sub(data[i - w]);
    }
    o
}

fn undelta_w(delta: &[u8], w: usize) -> Vec<u8> {
    let mut o = delta.to_vec();
    for i in w..o.len() {
        o[i] = o[i].wrapping_add(o[i - w]);
    }
    o
}

fn xor_w(data: &[u8], w: usize) -> Vec<u8> {
    let mut o = data.to_vec();
    for i in w..o.len() {
        o[i] = data[i] ^ data[i - w];
    }
    o
}

fn unxor_w(x: &[u8], w: usize) -> Vec<u8> {
    let mut o = x.to_vec();
    for i in w..o.len() {
        o[i] ^= o[i - w];
    }
    o
}

fn transpose(data: &[u8], rec: usize) -> (Vec<u8>, usize) {
    if rec == 0 {
        return (data.to_vec(), 0);
    }
    let n_rec = data.len() / rec;
    let body = n_rec * rec;
    let mut out = Vec::with_capacity(data.len());
    for c in 0..rec {
        for r in 0..n_rec {
            out.push(data[r * rec + c]);
        }
    }
    out.extend_from_slice(&data[body..]);
    (out, data.len() - body)
}

fn untranspose(data: &[u8], rec: usize, tail: usize) -> Result<Vec<u8>, &'static str> {
    if rec == 0 {
        return Err("str rec");
    }
    if tail > data.len() {
        return Err("str tail");
    }
    let body = data.len() - tail;
    if rec == 0 || body % rec != 0 {
        return Err("str body");
    }
    let n_rec = body / rec;
    let mut out = vec![0u8; data.len()];
    for c in 0..rec {
        for r in 0..n_rec {
            out[r * rec + c] = data[c * n_rec + r];
        }
    }
    out[body..].copy_from_slice(&data[body..]);
    Ok(out)
}

fn pack_inner(data: &[u8]) -> (u8, Vec<u8>) {
    let mut best_k = INNER_STORE;
    let mut best = data.to_vec();
    if let Some(z) = wrap::lz_encode(data) {
        if z.len() < best.len() {
            best_k = INNER_LZ;
            best = z;
        }
    }
    if let Some(z) = lzm::encode(data) {
        if z.len() < best.len() {
            best_k = INNER_LZM;
            best = z;
        }
    }
    if data.len() >= 64 {
        let r = rans::rans_encode(data);
        if r.len() + 8 < best.len() {
            best_k = INNER_RANS;
            best = r;
        }
    }
    (best_k, best)
}

fn unpack_inner(kind: u8, blob: &[u8], raw_len: usize) -> Result<Vec<u8>, &'static str> {
    match kind {
        INNER_STORE => {
            if blob.len() != raw_len {
                return Err("str store");
            }
            Ok(blob.to_vec())
        }
        INNER_LZ => wrap::lz_decode(blob),
        INNER_LZM => lzm::decode(blob),
        INNER_RANS => rans::rans_decode(blob),
        _ => Err("str inner"),
    }
}

fn apply(xf: u8, w: usize, data: &[u8]) -> Vec<u8> {
    match xf {
        XF_DELTA => delta_w(data, w),
        XF_TRANSPOSE => transpose(data, w).0,
        XF_XOR => xor_w(data, w),
        XF_DELTA_T => {
            let d = delta_w(data, w);
            transpose(&d, w).0
        }
        XF_XOR_T => {
            let d = xor_w(data, w);
            transpose(&d, w).0
        }
        _ => data.to_vec(),
    }
}

fn revert(xf: u8, w: usize, data: &[u8], orig_len: usize) -> Result<Vec<u8>, &'static str> {
    let tail = orig_len % w;
    match xf {
        XF_DELTA => Ok(undelta_w(data, w)),
        XF_TRANSPOSE => untranspose(data, w, tail),
        XF_XOR => Ok(unxor_w(data, w)),
        XF_DELTA_T => {
            let t = untranspose(data, w, tail)?;
            Ok(undelta_w(&t, w))
        }
        XF_XOR_T => {
            let t = untranspose(data, w, tail)?;
            Ok(unxor_w(&t, w))
        }
        _ => Err("str xf"),
    }
}

fn frame(raw_len: u32, xf: u8, w: u8, kind: u8, inner: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(14 + inner.len());
    out.extend_from_slice(MAGIC);
    out.push(VER);
    out.extend_from_slice(&raw_len.to_le_bytes());
    out.push(xf);
    out.push(w);
    out.push(kind);
    out.extend_from_slice(&(inner.len() as u32).to_le_bytes());
    out.extend_from_slice(inner);
    out
}

pub fn encode(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 256 {
        return None;
    }
    if detect::shannon(data) > 7.7 {
        return None;
    }
    let mut cands: Vec<(u8, usize)> = Vec::new();
    for w in [2usize, 4, 8] {
        cands.push((XF_DELTA, w));
        cands.push((XF_XOR, w));
    }
    if let Some(w) = best_width(data) {
        cands.push((XF_TRANSPOSE, w));
        cands.push((XF_DELTA_T, w));
        cands.push((XF_XOR_T, w));
        if ![2, 4, 8].contains(&w) {
            cands.push((XF_DELTA, w));
            cands.push((XF_XOR, w));
        }
    }
    let probe_n = data.len().min(64 * 1024);
    let h0 = shannon(&data[..probe_n]);
    let mut best: Option<Vec<u8>> = None;
    for (xf, w) in cands {
        if w == 0 || w > 255 {
            continue;
        }
        let sample = apply(xf, w, &data[..probe_n]);
        if shannon(&sample) + 0.15 >= h0 {
            continue;
        }
        let t = apply(xf, w, data);
        if t.len() != data.len() {
            continue;
        }
        let (kind, inner) = pack_inner(&t);
        let blob = frame(data.len() as u32, xf, w as u8, kind, &inner);
        if blob.len() >= data.len() {
            continue;
        }
        match best {
            None => best = Some(blob),
            Some(ref cur) if blob.len() < cur.len() => best = Some(blob),
            _ => {}
        }
    }
    let blob = best?;
    match decode(&blob) {
        Ok(back) if back == data => Some(blob),
        _ => None,
    }
}

pub fn decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if !is_str(buf) {
        return Err("str1");
    }
    let raw_len = u32::from_le_bytes(buf[5..9].try_into().unwrap()) as usize;
    let xf = buf[9];
    let w = buf[10] as usize;
    let kind = buf[11];
    let ilen = u32::from_le_bytes(buf[12..16].try_into().unwrap()) as usize;
    if 16 + ilen != buf.len() {
        return Err("str trunc");
    }
    if w == 0 {
        return Err("str w");
    }
    let inner = unpack_inner(kind, &buf[16..], raw_len)?;
    if inner.len() != raw_len {
        return Err("str inner n");
    }
    let out = revert(xf, w, &inner, raw_len)?;
    if out.len() != raw_len {
        return Err("str n");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_records() {
        let mut s = Vec::new();
        for i in 0u32..200 {
            s.extend_from_slice(&i.to_le_bytes());
            s.extend_from_slice(&[0u8, 1, 2, 3]);
        }
        let e = encode(&s).expect("str");
        assert!(e.starts_with(MAGIC));
        assert_eq!(decode(&e).unwrap(), s);
        assert!(e.len() < s.len());
    }

    #[test]
    fn transpose_inverse() {
        let s: Vec<u8> = (0..40).collect();
        let (t, tail) = transpose(&s, 8);
        assert_eq!(tail, 0);
        assert_eq!(untranspose(&t, 8, 0).unwrap(), s);
    }
}
