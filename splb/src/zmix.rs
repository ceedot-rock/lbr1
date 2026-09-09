//! ZMX1 — own context mixer. Absorbs Mahoney libzpaq (public domain):
//! ICM bit-history, stretch/squash mix, SSE, match model.
//! Not paq8px. Not cmix. Those stay GPL research opponents.

use crate::range::{Dec, Enc};
use std::sync::OnceLock;

pub const MAGIC: &[u8; 4] = b"ZMX1";
pub const VER: u8 = 1;
/// House min() cap. Explicit `lb zmix` may go larger.
pub const HOUSE_MAX: usize = 1 << 20;

const N: usize = 8;
const HASH: usize = 1 << 18;
const MASK: usize = HASH - 1;
const MATCH_BITS: usize = 16;
const MATCH_N: usize = 1 << MATCH_BITS;

struct Tables {
    stretch: [i16; 2048],
    squash: [u16; 4096],
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let mut stretch = [0i16; 2048];
        for p in 1..2048 {
            let x = (p as f64) / 2048.0;
            let y = ((x / (1.0 - x)).ln() * 64.0).clamp(-2047.0, 2047.0);
            stretch[p] = y as i16;
        }
        stretch[0] = -2047;
        let mut squash = [0u16; 4096];
        for i in 0..4096 {
            let d = i as f64 - 2048.0;
            let p = (2048.0 / (1.0 + (-d / 64.0).exp())).clamp(1.0, 2047.0);
            squash[i] = p as u16;
        }
        Tables { stretch, squash }
    })
}

fn stretch(p: u16) -> i32 {
    tables().stretch[p.clamp(1, 2047) as usize] as i32
}

fn squash(d: i32) -> u16 {
    tables().squash[(d + 2048).clamp(0, 4095) as usize]
}

struct Mix {
    hist: Vec<u8>,
    hp: Vec<u16>,
    w: [i32; N],
    sse: Vec<u16>,
    sse_idx: usize,
    c0: u32,
    bpos: u32,
    h: [usize; N],
    last: [u8; 4],
    word: u32,
    mlen: u32,
    pos: Vec<u32>,
    seen: Vec<u8>,
    last_st: [i32; N],
    last_q: u16,
}

impl Mix {
    fn new() -> Self {
        Self {
            hist: vec![0u8; HASH],
            hp: vec![1024u16; 256 * N],
            w: [256; N],
            sse: vec![1024u16; 16 * 128],
            sse_idx: 0,
            c0: 1,
            bpos: 0,
            h: [0; N],
            last: [0; 4],
            word: 0,
            mlen: 0,
            pos: vec![0u32; MATCH_N],
            seen: Vec::new(),
            last_st: [0; N],
            last_q: 1024,
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
        self.h[2] = (c.wrapping_mul(29) + ((a << 8) | b).wrapping_mul(17) + n) & MASK;
        self.h[3] = (c
            ^ a.wrapping_mul(0x9E37)
            ^ b.wrapping_mul(0x85EB)
            ^ d.wrapping_mul(0xC2B2)
            ^ (n * 31))
            & MASK;
        self.h[4] = (c
            ^ a.wrapping_mul(0x9E37)
            ^ b.wrapping_mul(0x85EB)
            ^ d.wrapping_mul(0xC2B2)
            ^ e.wrapping_mul(0x27BB)
            ^ (n * 57))
            & MASK;
        self.h[5] = (c + ((a ^ d) << 8) + n * 13) & MASK;
        self.h[6] = (self.mlen.min(63) as usize * 8 + n) & MASK;
        self.h[7] = (self.word as usize ^ (c << 3) ^ n) & MASK;
    }

    fn mix_p(&mut self) -> u16 {
        let mut st = [0i32; N];
        let mut sum = 0i32;
        for i in 0..N {
            let hist = self.hist[self.h[i]] as usize;
            let p = self.hp[i * 256 + hist].clamp(1, 2047);
            st[i] = stretch(p);
            sum += (self.w[i] * st[i]) >> 8;
        }
        let q = squash(sum.clamp(-2047, 2047));
        self.sse_idx = (self.bpos as usize) * 128 + (q as usize >> 4);
        let a = self.sse[self.sse_idx] as u32;
        let q2 = ((q as u32 + a) / 2).clamp(1, 2047) as u16;
        self.last_st = st;
        self.last_q = q2;
        q2
    }

    fn update(&mut self, bit: u32) {
        let q = self.last_q;
        let err = if bit == 0 {
            2048 - q as i32
        } else {
            0 - q as i32
        };
        for i in 0..N {
            self.w[i] += (err * self.last_st[i]) >> 16;
            self.w[i] = self.w[i].clamp(-8192, 8192);
            let slot = self.h[i];
            let hist = self.hist[slot] as usize;
            let pi = i * 256 + hist;
            let p = self.hp[pi] as i32;
            let p = if bit == 0 {
                p + ((2048 - p) >> 5)
            } else {
                p - (p >> 5)
            };
            self.hp[pi] = p.clamp(1, 2047) as u16;
            self.hist[slot] = ((hist as u8) << 1) | (bit as u8);
        }
        let ap = self.sse[self.sse_idx] as i32;
        let ap = if bit == 0 {
            ap + ((2048 - ap) >> 5)
        } else {
            ap - (ap >> 5)
        };
        self.sse[self.sse_idx] = ap.clamp(1, 2047) as u16;
        self.c0 = (self.c0 << 1) | bit;
        self.bpos += 1;
        if self.bpos == 8 {
            let byte = (self.c0 & 0xff) as u8;
            self.last[3] = self.last[2];
            self.last[2] = self.last[1];
            self.last[1] = self.last[0];
            self.last[0] = byte;
            if byte.is_ascii_alphanumeric() {
                self.word = self.word.wrapping_mul(263).wrapping_add(byte as u32);
            } else {
                self.word = 0;
            }
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

pub fn is_zmix(buf: &[u8]) -> bool {
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
    let mut m = Mix::new();
    let mut e = Enc::new();
    for &b in data {
        for i in (0..8).rev() {
            m.ctx();
            let q = m.mix_p();
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
    if !is_zmix(buf) {
        return Err("zmx1");
    }
    let n = u32::from_le_bytes(buf[5..9].try_into().unwrap()) as usize;
    let mut d = Dec::open(&buf[9..])?;
    let mut m = Mix::new();
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let mut b = 0u8;
        for i in (0..8).rev() {
            m.ctx();
            let q = m.mix_p();
            let bit = d.bit_p(q);
            m.update(bit);
            b |= (bit as u8) << i;
        }
        out.push(b);
    }
    if out.len() != n {
        return Err("zmx1 n");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_repeat() {
        let s = b"the cat sat on the mat. ".repeat(80);
        let e = encode(&s).expect("zmix");
        assert!(e.starts_with(MAGIC));
        assert_eq!(decode(&e).unwrap(), s.as_slice());
        assert!(e.len() < s.len());
    }
}
