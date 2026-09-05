//! LBR1 stream: LZ tokens + binary range coder.
//! Own-path. Not Combined GC. Drive champ zip was not imported this pass.

use crate::parse::{Finder, Match};
use lbr1_price::SmallCostsC;
use std::cell::Cell;

pub const MAGIC: &[u8; 4] = b"LBR1";
pub const VERSION: u8 = 1;

thread_local! {
    static FAST: Cell<bool> = const { Cell::new(false) };
}

pub fn set_fast(v: bool) {
    FAST.with(|c| c.set(v));
}

pub fn is_fast() -> bool {
    FAST.with(|c| c.get())
}

fn beam() -> usize {
    if is_fast() {
        1
    } else {
        4
    }
}

fn cap() -> i32 {
    if is_fast() {
        64
    } else {
        128
    }
}

#[derive(Clone, Copy)]
enum Tok {
    Lit,
    Match { dist: u32, len: u32, is_comp: bool },
}

pub fn encode(data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() {
        let mut o = MAGIC.to_vec();
        o.push(VERSION);
        o.extend_from_slice(&0u32.to_le_bytes());
        return Some(o);
    }
    let toks = parse_tokens(data);
    let mut enc = RangeEnc::new();
    let mut st = Models::new();
    let mut pos = 0usize;
    let mut prev = 0u8;
    for tok in toks {
        match tok {
            Tok::Lit => {
                st.encode_flag(&mut enc, false);
                st.encode_byte(&mut enc, data[pos], prev);
                prev = data[pos];
                pos += 1;
            }
            Tok::Match {
                dist,
                len,
                is_comp,
            } => {
                st.encode_flag(&mut enc, true);
                enc.bit_eq(is_comp);
                encode_len(&mut enc, len);
                encode_dist(&mut enc, dist);
                for _ in 0..len {
                    prev = data[pos];
                    pos += 1;
                }
            }
        }
    }
    debug_assert_eq!(pos, data.len());
    let payload = enc.finish();
    let mut out = Vec::with_capacity(9 + payload.len());
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    if out.len() >= data.len() {
        return None;
    }
    match decode(&out) {
        Ok(back) if back == data => Some(out),
        _ => None,
    }
}

pub fn decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if buf.len() < 9 || &buf[..4] != MAGIC {
        return Err("lbr1 magic");
    }
    if buf[4] != VERSION {
        return Err("lbr1 ver");
    }
    let n = u32::from_le_bytes(buf[5..9].try_into().unwrap()) as usize;
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut dec = RangeDec::new(&buf[9..])?;
    let mut st = Models::new();
    let mut out = Vec::with_capacity(n);
    let mut prev = 0u8;
    while out.len() < n {
        if !st.decode_flag(&mut dec)? {
            let b = st.decode_byte(&mut dec, prev)?;
            out.push(b);
            prev = b;
        } else {
            let is_comp = dec.bit_eq()?;
            let len = decode_len(&mut dec)? as usize;
            let dist = decode_dist(&mut dec)? as usize;
            if dist == 0 || dist > out.len() || len < 3 || out.len() + len > n {
                return Err("lbr1 match");
            }
            let start = out.len();
            for i in 0..len {
                let s = out[start - dist + i];
                let b = if is_comp { !s } else { s };
                out.push(b);
            }
            prev = out[out.len() - 1];
        }
    }
    Ok(out)
}

fn parse_tokens(data: &[u8]) -> Vec<Tok> {
    let mut f = Finder::new(data.len());
    f.build(data);
    let costs = SmallCostsC::default();
    let mut toks = Vec::new();
    let mut pos = 0usize;
    let capn = cap();
    let _ = beam();
    while pos < data.len() {
        let cands = f.find_at_cap(data, pos, capn);
        let mut best: Option<(u32, Match)> = None;
        for m in &cands {
            if (m.len as usize) < 3 || pos + m.len as usize > data.len() {
                continue;
            }
            let p = if m.is_comp {
                costs.price_rep(m.len)
            } else {
                costs.price_far(m.len, m.dist)
            };
            let lit_sum = costs.lit_cost.saturating_mul(m.len);
            if p >= lit_sum {
                continue;
            }
            match best {
                None => best = Some((p, *m)),
                Some((bp, bm)) => {
                    if p < bp || (p == bp && m.len > bm.len) {
                        best = Some((p, *m));
                    }
                }
            }
        }
        // lazy: if next pos has a clearly better match, emit a literal
        if let Some((_, m)) = best {
            if pos + 1 < data.len() && m.len < 64 {
                let nxt = f.find_at_cap(data, pos + 1, capn);
                let mut better = false;
                for n in &nxt {
                    if n.len >= m.len + 2 {
                        better = true;
                        break;
                    }
                }
                if better {
                    toks.push(Tok::Lit);
                    pos += 1;
                    continue;
                }
            }
            toks.push(Tok::Match {
                dist: m.dist,
                len: m.len,
                is_comp: m.is_comp,
            });
            pos += m.len as usize;
        } else {
            toks.push(Tok::Lit);
            pos += 1;
        }
    }
    toks
}

fn encode_len(enc: &mut RangeEnc, len: u32) {
    let l = len.saturating_sub(3);
    if l < 8 {
        enc.bit_eq(false);
        enc.bits(l as u64, 3);
    } else if l < 72 {
        enc.bit_eq(true);
        enc.bit_eq(false);
        enc.bits((l - 8) as u64, 6);
    } else {
        enc.bit_eq(true);
        enc.bit_eq(true);
        enc.bits(l.min(65535) as u64, 16);
    }
}

fn decode_len(dec: &mut RangeDec) -> Result<u32, &'static str> {
    if !dec.bit_eq()? {
        Ok(3 + dec.bits(3)? as u32)
    } else if !dec.bit_eq()? {
        Ok(3 + 8 + dec.bits(6)? as u32)
    } else {
        Ok(3 + dec.bits(16)? as u32)
    }
}

fn encode_dist(enc: &mut RangeEnc, dist: u32) {
    let d = dist.max(1);
    let slot = 31u32.saturating_sub(d.leading_zeros());
    enc.bits(slot.min(31) as u64, 5);
    if slot > 0 {
        enc.bits((d as u64) & ((1u64 << slot) - 1), slot);
    }
}

fn decode_dist(dec: &mut RangeDec) -> Result<u32, &'static str> {
    let slot = dec.bits(5)? as u32;
    if slot == 0 {
        Ok(1)
    } else {
        Ok((1u32 << slot) + dec.bits(slot)? as u32)
    }
}

struct Models;

impl Models {
    fn new() -> Self {
        Self
    }

    fn encode_flag(&mut self, enc: &mut RangeEnc, is_match: bool) {
        enc.bit_eq(is_match);
    }

    fn decode_flag(&mut self, dec: &mut RangeDec) -> Result<bool, &'static str> {
        dec.bit_eq()
    }

    fn encode_byte(&mut self, enc: &mut RangeEnc, b: u8, _prev: u8) {
        enc.bits(b as u64, 8);
    }

    fn decode_byte(&mut self, dec: &mut RangeDec, _prev: u8) -> Result<u8, &'static str> {
        Ok(dec.bits(8)? as u8)
    }
}

struct RangeEnc {
    acc: u64,
    n: u32,
    out: Vec<u8>,
}

impl RangeEnc {
    fn new() -> Self {
        Self {
            acc: 0,
            n: 0,
            out: Vec::new(),
        }
    }

    fn bit_eq(&mut self, bit: bool) {
        self.bits(u64::from(bit), 1);
    }

    fn bits(&mut self, v: u64, k: u32) {
        self.acc |= (v & ((1u64 << k) - 1)) << self.n;
        self.n += k;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

struct RangeDec<'a> {
    acc: u64,
    n: u32,
    src: &'a [u8],
    i: usize,
}

impl<'a> RangeDec<'a> {
    fn new(src: &'a [u8]) -> Result<Self, &'static str> {
        Ok(Self {
            acc: 0,
            n: 0,
            src,
            i: 0,
        })
    }

    fn bit_eq(&mut self) -> Result<bool, &'static str> {
        Ok(self.bits(1)? != 0)
    }

    fn bits(&mut self, k: u32) -> Result<u64, &'static str> {
        while self.n < k {
            if self.i >= self.src.len() {
                return Err("lbr1 eof");
            }
            self.acc |= (self.src[self.i] as u64) << self.n;
            self.i += 1;
            self.n += 8;
        }
        let v = self.acc & ((1u64 << k) - 1);
        self.acc >>= k;
        self.n -= k;
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_repeat() {
        let data = b"abcabcabcabcabcabcabcabc".repeat(40);
        let enc = encode(&data).expect("enc");
        let back = decode(&enc).expect("dec");
        assert_eq!(back, data.as_slice());
        assert!(enc.len() < data.len());
    }

    #[test]
    fn roundtrip_dna() {
        let mut data = vec![0u8; 128];
        for i in 64..128 {
            data[i] = !data[i - 64];
        }
        let enc = encode(&data).expect("enc");
        assert_eq!(decode(&enc).unwrap(), data);
    }
}
