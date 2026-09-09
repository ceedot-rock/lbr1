//! NNC1 — own online neural predictor. Absorbs Bellard NNCP (MIT):
//! train-while-compress, no separate pass. Tiny 2-layer net, not a Transformer.
//! Hosted/house path is size-capped; explicit `lb nnc` may go larger.

use crate::range::{Dec, Enc};

pub const MAGIC: &[u8; 4] = b"NNC1";
pub const VER: u8 = 1;
pub const HOUSE_MAX: usize = 1 << 18;

const HIDDEN: usize = 8;
const IN: usize = 8;
const HASH: usize = 1 << 16;
const MASK: usize = HASH - 1;
const LR: f32 = 0.03;

struct Net {
    // context hashed tables → 8 stretched-ish inputs
    t: Vec<u16>,
    h: [usize; IN],
    last: [u8; 4],
    c0: u32,
    bpos: u32,
    mlen: u32,
    pos: Vec<u32>,
    seen: Vec<u8>,
    w1: [[f32; IN]; HIDDEN],
    b1: [f32; HIDDEN],
    w2: [f32; HIDDEN],
    b2: f32,
    x: [f32; IN],
    hid: [f32; HIDDEN],
    p: f32,
}

impl Net {
    fn new() -> Self {
        let mut w1 = [[0f32; IN]; HIDDEN];
        let mut w2 = [0f32; HIDDEN];
        for i in 0..HIDDEN {
            for j in 0..IN {
                w1[i][j] = 0.05 * (((i * 13 + j * 7) % 11) as f32 - 5.0);
            }
            w2[i] = 0.08 * ((i as f32) - 3.5);
        }
        Self {
            t: vec![1024u16; HASH * IN],
            h: [0; IN],
            last: [0; 4],
            c0: 1,
            bpos: 0,
            mlen: 0,
            pos: vec![0u32; 1 << 16],
            seen: Vec::new(),
            w1,
            b1: [0.0; HIDDEN],
            w2,
            b2: 0.0,
            x: [0.0; IN],
            hid: [0.0; HIDDEN],
            p: 0.5,
        }
    }

    fn ctx(&mut self) {
        let n = self.bpos as usize;
        let c = self.c0 as usize;
        let a = self.last[0] as usize;
        let b = self.last[1] as usize;
        let d = self.last[2] as usize;
        let e = self.last[3] as usize;
        self.h[0] = (c + n * 257) & MASK;
        self.h[1] = (c.wrapping_mul(13) + a.wrapping_mul(4099) + n) & MASK;
        self.h[2] = (c.wrapping_mul(29) + ((a << 8) | b) * 17 + n) & MASK;
        self.h[3] = (c ^ a.wrapping_mul(0x9E37) ^ b.wrapping_mul(0x85EB) ^ (n * 31)) & MASK;
        self.h[4] = (c
            ^ a.wrapping_mul(0x9E37)
            ^ b.wrapping_mul(0x85EB)
            ^ d.wrapping_mul(0xC2B2)
            ^ e.wrapping_mul(0x27BB))
            & MASK;
        self.h[5] = (c + ((a ^ d) << 8) + n) & MASK;
        self.h[6] = (self.mlen.min(63) as usize * 8 + n) & MASK;
        self.h[7] = (a.wrapping_mul(251) + n * 17 + c) & MASK;
    }

    fn forward(&mut self) -> u16 {
        for i in 0..IN {
            let p = self.t[i * HASH + self.h[i]].clamp(1, 2047);
            self.x[i] = (p as f32 / 2048.0) * 2.0 - 1.0;
        }
        for i in 0..HIDDEN {
            let mut s = self.b1[i];
            for j in 0..IN {
                s += self.w1[i][j] * self.x[j];
            }
            self.hid[i] = s.tanh();
        }
        let mut s = self.b2;
        for i in 0..HIDDEN {
            s += self.w2[i] * self.hid[i];
        }
        // sigmoid → P(bit=0)
        self.p = 1.0 / (1.0 + (-s).exp());
        ((self.p * 2048.0).clamp(32.0, 2016.0)) as u16
    }

    fn update(&mut self, bit: u32) {
        let y = if bit == 0 { 1.0 } else { 0.0 };
        let err = y - self.p;
        let d2 = err * self.p * (1.0 - self.p);
        for i in 0..HIDDEN {
            let dh = d2 * self.w2[i] * (1.0 - self.hid[i] * self.hid[i]);
            self.w2[i] += LR * d2 * self.hid[i];
            self.b1[i] += LR * dh;
            for j in 0..IN {
                self.w1[i][j] += LR * dh * self.x[j];
            }
        }
        self.b2 += LR * d2;
        for i in 0..IN {
            let slot = i * HASH + self.h[i];
            let p = self.t[slot] as i32;
            let p = if bit == 0 {
                p + ((2048 - p) >> 5)
            } else {
                p - (p >> 5)
            };
            self.t[slot] = p.clamp(1, 2047) as u16;
        }
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
                    & 0xffff;
                let old = self.pos[hx] as usize;
                self.pos[hx] = n as u32;
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

pub fn is_nnc(buf: &[u8]) -> bool {
    buf.len() >= 9 && buf.starts_with(MAGIC) && buf[4] == VER
}

pub fn encode(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 64 {
        return None;
    }
    let out = encode_always(data);
    if out.len() < data.len() {
        match decode(&out) {
            Ok(back) if back == data => Some(out),
            _ => None,
        }
    } else {
        None
    }
}

pub fn encode_always(data: &[u8]) -> Vec<u8> {
    let mut m = Net::new();
    let mut e = Enc::new();
    for &b in data {
        for i in (0..8).rev() {
            m.ctx();
            let q = m.forward();
            let bit = ((b >> i) & 1) as u32;
            e.bit_p(bit, q);
            m.update(bit);
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
    if !is_nnc(buf) {
        return Err("nnc1");
    }
    let n = u32::from_le_bytes(buf[5..9].try_into().unwrap()) as usize;
    let mut d = Dec::open(&buf[9..])?;
    let mut m = Net::new();
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let mut b = 0u8;
        for i in (0..8).rev() {
            m.ctx();
            let q = m.forward();
            let bit = d.bit_p(q);
            m.update(bit);
            b |= (bit as u8) << i;
        }
        out.push(b);
    }
    if out.len() != n {
        return Err("nnc1 n");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_repeat() {
        let s = b"the cat sat on the mat. ".repeat(40);
        let e = encode(&s).expect("nnc");
        assert!(e.starts_with(MAGIC));
        assert_eq!(decode(&e).unwrap(), s.as_slice());
        assert!(e.len() < s.len());
    }
}
