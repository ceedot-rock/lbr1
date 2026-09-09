//! Residue guesses on data the house almost stored.
//!
//! Not a Shannon cheat. True random still will not shrink. Two exact jobs:
//! 1. Bit occupancy: bits that never change are not stored (ASCII, 24-in-32, etc.).
//! 2. Cheap reversible prefilters (delta / xor / swap) when a 64 KiB probe
//!    shows lower entropy. Then an own gene on the transformed bytes.
//!
//! DECODE_OK and strictly smaller or we do not emit.

use crate::wrap;

pub const MAGIC: &[u8; 4] = b"GSS1";
pub const VER: u8 = 1;
pub const KIND_BITS: u8 = 0;
pub const KIND_DELTA: u8 = 1;
pub const KIND_XOR: u8 = 2;
pub const KIND_SWAP16: u8 = 3;
pub const KIND_SWAP32: u8 = 4;

const PROBE: usize = 64 * 1024;

pub fn is_guess(buf: &[u8]) -> bool {
    buf.len() >= 10 && buf.starts_with(MAGIC) && buf[4] == VER
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
    let mut h = 0.0;
    for c in cnt {
        if c > 0 {
            let p = c as f64 / n;
            h -= p * p.log2();
        }
    }
    h
}

fn occupancy(data: &[u8]) -> (u8, u8) {
    let mut or = 0u8;
    let mut and = 0xffu8;
    for &b in data {
        or |= b;
        and &= b;
    }
    (or, and)
}

fn vary_mask(or: u8, and: u8) -> u8 {
    or & !and
}

fn nbits(m: u8) -> u32 {
    m.count_ones()
}

fn pack_bits(data: &[u8], vary: u8) -> Vec<u8> {
    let k = nbits(vary);
    if k == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(((data.len() as u64 * k as u64 + 7) / 8) as usize);
    let mut acc = 0u32;
    let mut n = 0u32;
    for &b in data {
        let mut v = 0u32;
        let mut shift = 0u32;
        for i in 0..8 {
            if (vary >> i) & 1 == 1 {
                v |= ((b as u32 >> i) & 1) << shift;
                shift += 1;
            }
        }
        acc |= v << n;
        n += k;
        while n >= 8 {
            out.push(acc as u8);
            acc >>= 8;
            n -= 8;
        }
    }
    if n > 0 {
        out.push(acc as u8);
    }
    out
}

fn unpack_bits(packed: &[u8], n: usize, vary: u8, and: u8) -> Result<Vec<u8>, &'static str> {
    let k = nbits(vary);
    let mut out = Vec::with_capacity(n);
    if k == 0 {
        return Ok(vec![and; n]);
    }
    let mut acc = 0u32;
    let mut have = 0u32;
    let mut i = 0usize;
    let mask = (1u32 << k) - 1;
    for _ in 0..n {
        while have < k {
            if i >= packed.len() {
                return Err("gss bits");
            }
            acc |= (packed[i] as u32) << have;
            have += 8;
            i += 1;
        }
        let v = acc & mask;
        acc >>= k;
        have -= k;
        let mut b = and;
        let mut shift = 0u32;
        for bit in 0..8 {
            if (vary >> bit) & 1 == 1 {
                b |= (((v >> shift) & 1) as u8) << bit;
                shift += 1;
            }
        }
        out.push(b);
    }
    Ok(out)
}

fn delta(data: &[u8], stride: usize) -> Vec<u8> {
    let mut o = data.to_vec();
    for i in stride..o.len() {
        o[i] = data[i].wrapping_sub(data[i - stride]);
    }
    o
}

fn undelta(delta: &[u8], stride: usize) -> Vec<u8> {
    let mut o = delta.to_vec();
    for i in stride..o.len() {
        o[i] = o[i].wrapping_add(o[i - stride]);
    }
    o
}

fn xor_prev(data: &[u8], stride: usize) -> Vec<u8> {
    let mut o = data.to_vec();
    for i in stride..o.len() {
        o[i] = data[i] ^ data[i - stride];
    }
    o
}

fn xor_prev_inv(data: &[u8], stride: usize) -> Vec<u8> {
    let mut o = data.to_vec();
    for i in stride..o.len() {
        o[i] = o[i] ^ o[i - stride];
    }
    o
}

fn swap16(data: &[u8]) -> Vec<u8> {
    let mut o = data.to_vec();
    let mut i = 0;
    while i + 1 < o.len() {
        o.swap(i, i + 1);
        i += 2;
    }
    o
}

fn swap32(data: &[u8]) -> Vec<u8> {
    let mut o = data.to_vec();
    let mut i = 0;
    while i + 3 < o.len() {
        o.swap(i, i + 3);
        o.swap(i + 1, i + 2);
        i += 4;
    }
    o
}

fn encode_inner(data: &[u8]) -> Option<Vec<u8>> {
    let mut best: Option<Vec<u8>> = None;
    if let Some(z) = wrap::lz_encode(data) {
        best = Some(z);
    }
    if data.len() >= 256 {
        if let Some(p) = pulsar::pulsar_encode(data) {
            match &best {
                None => best = Some(p),
                Some(cur) if p.len() < cur.len() => best = Some(p),
                _ => {}
            }
        }
    }
    best.filter(|b| b.len() < data.len())
}

fn wrap_filter(kind: u8, stride: u8, raw_len: u32, inner: &[u8]) -> Vec<u8> {
    let mut out = Vec::from(*MAGIC);
    out.push(VER);
    out.push(kind);
    out.push(stride);
    out.extend_from_slice(&raw_len.to_le_bytes());
    out.extend_from_slice(&(inner.len() as u32).to_le_bytes());
    out.extend_from_slice(inner);
    out
}

fn try_bits(data: &[u8]) -> Option<Vec<u8>> {
    let (or, and) = occupancy(data);
    let vary = vary_mask(or, and);
    let k = nbits(vary);
    if k >= 8 {
        return None;
    }
    let packed = pack_bits(data, vary);
    let mut out = Vec::from(*MAGIC);
    out.push(VER);
    out.push(KIND_BITS);
    out.push(or);
    out.push(and);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&packed);
    if out.len() < data.len() {
        Some(out)
    } else {
        None
    }
}

fn try_filter(data: &[u8], kind: u8, stride: usize) -> Option<Vec<u8>> {
    let sample = &data[..data.len().min(PROBE)];
    let h0 = shannon(sample);
    let transformed = match kind {
        KIND_DELTA => delta(data, stride),
        KIND_XOR => xor_prev(data, stride),
        KIND_SWAP16 => swap16(data),
        KIND_SWAP32 => swap32(data),
        _ => return None,
    };
    let hs = shannon(&transformed[..transformed.len().min(PROBE)]);
    // Educated: only spend an encode if the probe got quieter.
    if hs + 0.08 >= h0 {
        return None;
    }
    let inner = encode_inner(&transformed)?;
    let out = wrap_filter(kind, stride as u8, data.len() as u32, &inner);
    if out.len() < data.len() {
        Some(out)
    } else {
        None
    }
}

fn keep_smaller(best: &mut Option<Vec<u8>>, cand: Option<Vec<u8>>) {
    if let Some(c) = cand {
        match best {
            None => *best = Some(c),
            Some(cur) if c.len() < cur.len() => *best = Some(c),
            _ => {}
        }
    }
}

/// Run only when the house is still near store. True random returns None.
pub fn encode(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 32 {
        return None;
    }
    let mut best = try_bits(data);
    keep_smaller(&mut best, try_filter(data, KIND_DELTA, 1));
    keep_smaller(&mut best, try_filter(data, KIND_DELTA, 2));
    keep_smaller(&mut best, try_filter(data, KIND_DELTA, 4));
    keep_smaller(&mut best, try_filter(data, KIND_DELTA, 8));
    keep_smaller(&mut best, try_filter(data, KIND_XOR, 1));
    keep_smaller(&mut best, try_filter(data, KIND_XOR, 4));
    if data.len() >= 64 {
        keep_smaller(&mut best, try_filter(data, KIND_SWAP16, 2));
        keep_smaller(&mut best, try_filter(data, KIND_SWAP32, 4));
    }
    let out = best?;
    match decode(&out) {
        Ok(back) if back == *data && out.len() < data.len() => Some(out),
        _ => None,
    }
}

pub fn decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if !is_guess(buf) {
        return Err("not gss1");
    }
    let kind = buf[5];
    if kind == KIND_BITS {
        if buf.len() < 11 {
            return Err("gss bits hdr");
        }
        let or = buf[6];
        let and = buf[7];
        let n = u32::from_le_bytes(buf[8..12].try_into().unwrap()) as usize;
        let vary = vary_mask(or, and);
        return unpack_bits(&buf[12..], n, vary, and);
    }
    if buf.len() < 14 {
        return Err("gss hdr");
    }
    let stride = buf[6] as usize;
    if stride == 0 {
        return Err("gss stride");
    }
    let raw_len = u32::from_le_bytes(buf[7..11].try_into().unwrap()) as usize;
    let inner_len = u32::from_le_bytes(buf[11..15].try_into().unwrap()) as usize;
    if 15 + inner_len != buf.len() {
        return Err("gss inner");
    }
    let inner = &buf[15..];
    let transformed = crate::decode_gene(inner)?;
    if transformed.len() != raw_len {
        return Err("gss raw");
    }
    let out = match kind {
        KIND_DELTA => undelta(&transformed, stride),
        KIND_XOR => xor_prev_inv(&transformed, stride),
        KIND_SWAP16 => swap16(&transformed),
        KIND_SWAP32 => swap32(&transformed),
        _ => return Err("gss kind"),
    };
    if out.len() != raw_len {
        return Err("gss len");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_high_bit_steals() {
        let s: Vec<u8> = (0..10_000).map(|i| (b'a' + (i % 26) as u8)).collect();
        let e = encode(&s).expect("bits");
        assert!(e.len() < s.len());
        assert_eq!(decode(&e).unwrap(), s);
        // 7/8 of 10000 = 8750 plus header; must beat raw.
        assert!(e.len() < 9000);
    }

    #[test]
    fn true_random_does_not_shrink() {
        let mut s = vec![0u8; 4096];
        let mut x = 0xC0FFEEu32;
        for b in &mut s {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            *b = (x >> 16) as u8;
        }
        assert!(encode(&s).is_none());
    }

    #[test]
    fn delta_stride4_on_ints() {
        let mut s = Vec::new();
        for i in 0u32..2000 {
            s.extend_from_slice(&(i.wrapping_mul(3)).to_le_bytes());
        }
        let e = encode(&s);
        if let Some(blob) = e {
            assert!(blob.len() < s.len());
            assert_eq!(decode(&blob).unwrap(), s);
        }
    }
}
