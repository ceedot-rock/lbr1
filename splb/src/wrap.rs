//! Own wraps for the parked host-codec slots.
//! LZ wrap (LZW1) occupies the old XZ1 / zlib skin.
//! PAQ wrap (PCAQ) occupies the old paq/zpaq skin.
//! Both are lab genes. DECODE_OK or they do not emit.

use crate::pccaq;
use crate::rans;

pub const LZ_MAGIC: &[u8; 4] = b"LZW1";
pub const LZ_VER: u8 = 1;
/// PAQ wrap reuses the PCCaq frame (`PCAQ`).
pub const PAQ_MAX: usize = 262_144;
const MIN_MATCH: usize = 4;
const MAX_MATCH: usize = 255;
const WIN: usize = 1 << 16;
const HASH: usize = 1 << 16;
const CHAIN: usize = 24;

fn hash3(d: &[u8], i: usize) -> usize {
    ((d[i] as usize)
        .wrapping_mul(0x9E37)
        ^ (d[i + 1] as usize).wrapping_mul(0x85EB)
        ^ d[i + 2] as usize)
        & (HASH - 1)
}

fn match_len(d: &[u8], a: usize, b: usize, cap: usize) -> usize {
    let mut n = 0;
    while n < cap && a + n < d.len() && b + n < d.len() && d[a + n] == d[b + n] {
        n += 1;
    }
    n
}

fn tokens(data: &[u8]) -> Vec<u8> {
    let n = data.len();
    let mut head = vec![u32::MAX; HASH];
    let mut prev = vec![u32::MAX; n.min(WIN + n)];
    let mut out = Vec::with_capacity(n / 2 + 16);
    let mut i = 0usize;
    while i < n {
        let mut best_l = 0usize;
        let mut best_d = 0usize;
        if i + MIN_MATCH <= n {
            let h = hash3(data, i);
            let mut p = head[h];
            let mut walked = 0;
            while p != u32::MAX && walked < CHAIN {
                let j = p as usize;
                if i > j && i - j <= WIN {
                    let l = match_len(data, j, i, MAX_MATCH.min(n - i));
                    if l >= MIN_MATCH && l > best_l {
                        best_l = l;
                        best_d = i - j;
                    }
                }
                if j >= prev.len() {
                    break;
                }
                p = prev[j];
                walked += 1;
            }
            prev[i] = head[h];
            head[h] = i as u32;
        }
        if best_l >= MIN_MATCH && best_d > 0 && best_d <= 0xffff {
            out.push(0x01);
            out.extend_from_slice(&(best_d as u16).to_le_bytes());
            out.push(best_l as u8);
            let end = i + best_l;
            i += 1;
            while i < end {
                if i + 2 < n {
                    let h = hash3(data, i);
                    prev[i] = head[h];
                    head[h] = i as u32;
                }
                i += 1;
            }
        } else {
            out.push(0x00);
            out.push(data[i]);
            i += 1;
        }
    }
    out
}

fn expand(tok: &[u8], orig: usize) -> Result<Vec<u8>, &'static str> {
    let mut out = Vec::with_capacity(orig);
    let mut i = 0;
    while i < tok.len() && out.len() < orig {
        match tok[i] {
            0x00 => {
                if i + 1 >= tok.len() {
                    return Err("lzw1 lit");
                }
                out.push(tok[i + 1]);
                i += 2;
            }
            0x01 => {
                if i + 3 >= tok.len() {
                    return Err("lzw1 match");
                }
                let dist = u16::from_le_bytes([tok[i + 1], tok[i + 2]]) as usize;
                let len = tok[i + 3] as usize;
                if dist == 0 || dist > out.len() {
                    return Err("lzw1 dist");
                }
                for _ in 0..len {
                    let b = out[out.len() - dist];
                    out.push(b);
                    if out.len() == orig {
                        break;
                    }
                }
                i += 4;
            }
            _ => return Err("lzw1 tok"),
        }
    }
    if out.len() != orig {
        return Err("lzw1 len");
    }
    Ok(out)
}

pub fn is_lz(buf: &[u8]) -> bool {
    buf.len() >= 9 && buf.starts_with(LZ_MAGIC) && buf[4] == LZ_VER
}

pub fn is_paq(buf: &[u8]) -> bool {
    buf.len() >= 9 && buf.starts_with(pccaq::MAGIC) && buf[4] == pccaq::VER
}

/// Own LZ wrap. Replaces the parked xz/zlib skins.
pub fn lz_encode(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 64 {
        return None;
    }
    let tok = tokens(data);
    let coded = rans::rans_encode(&tok);
    let mut out = Vec::with_capacity(9 + coded.len());
    out.extend_from_slice(LZ_MAGIC);
    out.push(LZ_VER);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&coded);
    if out.len() + 8 >= data.len() {
        return None;
    }
    match lz_decode(&out) {
        Ok(back) if back == data => Some(out),
        _ => None,
    }
}

pub fn lz_decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if !is_lz(buf) {
        return Err("lzw1");
    }
    let orig = u32::from_le_bytes(buf[5..9].try_into().unwrap()) as usize;
    let tok = rans::rans_decode(&buf[9..])?;
    expand(&tok, orig)
}

/// Own PAQ wrap. PCCaq mixer in a standalone frame. Size-capped so the host stays live.
pub fn paq_encode(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 64 || data.len() > PAQ_MAX {
        return None;
    }
    let out = pccaq::encode(data)?;
    if out.len() + 8 >= data.len() {
        return None;
    }
    match pccaq::decode(&out) {
        Ok(back) if back == data => Some(out),
        _ => None,
    }
}

pub fn paq_decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    pccaq::decode(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lz_roundtrip() {
        let s = b"the cat sat on the mat. ".repeat(200);
        let e = lz_encode(&s).expect("lz");
        assert!(e.starts_with(LZ_MAGIC));
        assert_eq!(lz_decode(&e).unwrap(), s.as_slice());
        assert!(e.len() + 8 < s.len());
    }

    #[test]
    fn paq_roundtrip() {
        let s = b"the cat sat on the mat. ".repeat(80);
        let e = paq_encode(&s).expect("paq");
        assert!(is_paq(&e));
        assert_eq!(paq_decode(&e).unwrap(), s.as_slice());
        assert!(e.len() < s.len());
    }
}
