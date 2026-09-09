//! PCCaq — own bitwise context mixer. Not paq8px. Not Combined GC.
//!
//! Models: o1 o2 o3 o4 + match-len. Logistic mix. Same range coder as MATCH.
//! Occupies the old XZ1 slot: large binary / general residue.

use crate::range::{Dec, Enc};

pub const MAGIC: &[u8; 4] = b"PCAQ";
pub const VER: u8 = 1;

const N: usize = 5;
const HASH: usize = 1 << 18;
const MASK: usize = HASH - 1;
const MATCH_BITS: usize = 16;
const MATCH_N: usize = 1 << MATCH_BITS;
const LR: f32 = 0.04;

struct Mix {
    t: Vec<u16>,
    pos: Vec<u32>,
    w: [f32; N],
    c0: u32,
    bpos: u32,
    h: [usize; N],
    last: [u8; 4],
    mlen: u32,
    seen: Vec<u8>,
    apm: Vec<u16>,
    apm_idx: usize,
}

impl Mix {
    fn new() -> Self {
        Self {
            t: vec![1024u16; HASH * N],
            pos: vec![0u32; MATCH_N],
            w: [0.4; N],
            c0: 1,
            bpos: 0,
            h: [0; N],
            last: [0; 4],
            mlen: 0,
            seen: Vec::new(),
            apm: vec![1024u16; 8 * 256],
            apm_idx: 0,
        }
    }

    fn ctx(&mut self) {
        let n = self.bpos;
        let c = self.c0;
        let a = self.last[0] as usize;
        let b = self.last[1] as usize;
        let d = self.last[2] as usize;
        let e = self.last[3] as usize;
        self.h[0] = (c as usize + n as usize * 257 + a * 4099) & MASK;
        self.h[1] = (c as usize * 13 + ((a << 8) | b) * 17 + n as usize) & MASK;
        self.h[2] = (c as usize * 29 + a.wrapping_mul(251) + b.wrapping_mul(57) + d * 13) & MASK;
        self.h[3] = (c as usize
            ^ a.wrapping_mul(0x9E37)
            ^ b.wrapping_mul(0x85EB)
            ^ d.wrapping_mul(0xC2B2)
            ^ e.wrapping_mul(0x27BB)
            ^ (n as usize * 31))
            & MASK;
        self.h[4] = (self.mlen.min(63) as usize * 8 + n as usize) & MASK;
    }

    fn p_at(&self, i: usize) -> u16 {
        self.t[i * HASH + self.h[i]].clamp(1, 2047)
    }

    fn mix_p(&mut self) -> (u16, [f32; N]) {
        let mut st = [0f32; N];
        let mut s = 0.0f32;
        for i in 0..N {
            st[i] = self.p_at(i) as f32 - 1024.0;
            s += self.w[i] * st[i];
        }
        let q = (1024.0 + s / (N as f32)).clamp(96.0, 1952.0) as u16;
        self.apm_idx = (self.bpos as usize) * 256 + (q as usize >> 3);
        let a = self.apm[self.apm_idx] as u32;
        let q2 = ((q as u32 + a) / 2).clamp(96, 1952) as u16;
        (q2, st)
    }

    fn update(&mut self, bit: u32, st: [f32; N], q: u16) {
        let err = (1.0 - bit as f32) - (q as f32 / 2048.0);
        for i in 0..N {
            self.w[i] += LR * err * (st[i] / 1024.0);
            self.w[i] = self.w[i].clamp(-2.0, 2.0);
            let slot = i * HASH + self.h[i];
            let p = self.t[slot] as i32;
            let p = if bit == 0 {
                p + ((2048 - p) >> 5)
            } else {
                p - (p >> 5)
            };
            self.t[slot] = p.clamp(1, 2047) as u16;
        }
        let ap = self.apm[self.apm_idx] as i32;
        let ap = if bit == 0 {
            ap + ((2048 - ap) >> 5)
        } else {
            ap - (ap >> 5)
        };
        self.apm[self.apm_idx] = ap.clamp(1, 2047) as u16;
        self.c0 = (self.c0 << 1) | bit;
        self.bpos += 1;
        if self.bpos == 8 {
            let byte = (self.c0 & 0xff) as u8;
            self.last[3] = self.last[2];
            self.last[2] = self.last[1];
            self.last[1] = self.last[0];
            self.last[0] = byte;
            self.seen.push(byte);
            let n = self.seen.len();
            if n >= 4 {
                let hx = (u32::from_le_bytes([
                    self.last[0],
                    self.last[1],
                    self.last[2],
                    self.last[3],
                ]) as usize)
                    & (MATCH_N - 1);
                let old = self.pos[hx] as usize;
                self.pos[hx] = n as u32;
                // Longest common suffix vs previous hash hit. n-1-k is always < n.
                if old >= 1 && old < n {
                    let mut k = 0usize;
                    while k < 64
                        && old > k
                        && n > k
                        && self.seen[old - 1 - k] == self.seen[n - 1 - k]
                    {
                        k += 1;
                    }
                    self.mlen = k as u32;
                } else {
                    self.mlen = 0;
                }
            }
            self.c0 = 1;
            self.bpos = 0;
        }
    }
}

pub fn encode(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 64 {
        return None;
    }
    let out = encode_always(data);
    if out.len() < data.len() {
        Some(out)
    } else {
        None
    }
}

pub fn encode_always(data: &[u8]) -> Vec<u8> {
    let mut m = Mix::new();
    let mut e = Enc::new();
    for &b in data {
        for i in (0..8).rev() {
            m.ctx();
            let (q, st) = m.mix_p();
            let bit = ((b >> i) & 1) as u32;
            e.bit_p(bit, q);
            m.update(bit, st, q);
        }
    }
    let payload = e.finish();
    let mut out = Vec::with_capacity(9 + payload.len());
    out.extend_from_slice(MAGIC);
    out.push(VER);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

pub fn decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if buf.len() < 9 || &buf[..4] != MAGIC || buf[4] != VER {
        return Err("pccaq");
    }
    let n = u32::from_le_bytes(buf[5..9].try_into().unwrap()) as usize;
    let mut d = Dec::open(&buf[9..])?;
    let mut m = Mix::new();
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let mut b = 0u8;
        for i in (0..8).rev() {
            m.ctx();
            let (q, st) = m.mix_p();
            let bit = d.bit_p(q);
            m.update(bit, st, q);
            b |= (bit as u8) << i;
        }
        out.push(b);
    }
    if out.len() != n {
        return Err("pccaq n");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_repeat() {
        let s = b"the cat sat on the mat. ".repeat(80);
        let e = encode_always(&s);
        assert_eq!(decode(&e).expect("dec"), s.as_slice());
        assert!(e.len() < s.len(), "expanded {} -> {}", s.len(), e.len());
    }

    #[test]
    fn roundtrip_binaryish() {
        let mut s = Vec::new();
        for k in 0..400u32 {
            s.extend_from_slice(&k.to_le_bytes());
            s.extend_from_slice(&[0x48, 0x89, 0xc7, 0x48]);
        }
        let e = encode_always(&s);
        assert_eq!(decode(&e).expect("dec"), s);
        assert!(e.len() < s.len(), "expanded {} -> {}", s.len(), e.len());
    }
}
