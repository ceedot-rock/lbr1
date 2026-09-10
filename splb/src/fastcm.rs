//! FastCM — daily L3 residual context mixer (mozilla-first).
//!
//! Own-path only. Small context set (o1 / o2-hash / match-nibble).
//! LAW: empty residual → no CM theater (encode returns None / skip).
//! Not paq8px. Not host xz. Reuses the same range coder as PCCaq/MATCH.

use crate::range::{Dec, Enc};

pub const MAGIC: &[u8; 4] = b"FCM1";
pub const VER: u8 = 1;
/// Lit-stream kind tag inside LBR1 VER=4 frames.
pub const LIT_KIND: u8 = 3;

const N: usize = 3;
const HASH: usize = 1 << 16;
const MASK: usize = HASH - 1;
const LR: f32 = 0.05;

struct Mix {
    t: Vec<u16>,
    w: [f32; N],
    c0: u32,
    bpos: u32,
    h: [usize; N],
    last: [u8; 2],
    mlen_nibble: u8,
}

impl Mix {
    fn new() -> Self {
        Self {
            t: vec![1024u16; HASH * N],
            w: [0.5, 0.35, 0.25],
            c0: 1,
            bpos: 0,
            h: [0; N],
            last: [0; 2],
            mlen_nibble: 0,
        }
    }

    fn set_match_nibble(&mut self, n: u8) {
        self.mlen_nibble = n & 0x0f;
    }

    fn ctx(&mut self) {
        let n = self.bpos as usize;
        let c = self.c0 as usize;
        let a = self.last[0] as usize;
        let b = self.last[1] as usize;
        // o1 + bitpos
        self.h[0] = (c + n * 257 + a * 4099) & MASK;
        // o2 hash
        self.h[1] = (c.wrapping_mul(13) + ((a << 8) | b).wrapping_mul(17) + n) & MASK;
        // match-len nibble + bitpos (residual after L2)
        self.h[2] = ((self.mlen_nibble as usize) * 16 + n + c.wrapping_mul(3)) & MASK;
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
        let q = (1024.0 + s / (N as f32)).clamp(64.0, 1984.0) as u16;
        (q, st)
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
        self.c0 = (self.c0 << 1) | bit;
        self.bpos += 1;
        if self.bpos == 8 {
            let byte = (self.c0 & 0xff) as u8;
            self.last[1] = self.last[0];
            self.last[0] = byte;
            self.c0 = 1;
            self.bpos = 0;
        }
    }
}

/// Encode a NON-EMPTY residual. Empty → None (no CM theater).
pub fn encode_residual(data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() {
        return None;
    }
    Some(encode_residual_always(data))
}

/// Like encode_residual but always emits (still refuses empty).
pub fn encode_residual_always(data: &[u8]) -> Vec<u8> {
    assert!(!data.is_empty(), "FastCM: empty residual must skip mixer");
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

pub fn decode_residual(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if buf.len() < 9 || &buf[..4] != MAGIC || buf[4] != VER {
        return Err("fastcm");
    }
    let n = u32::from_le_bytes(buf[5..9].try_into().unwrap()) as usize;
    if n == 0 {
        return Err("fastcm empty");
    }
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
        return Err("fastcm n");
    }
    Ok(out)
}

/// Encode residual with per-byte match-nibble context (from L2). Empty → None.
pub fn encode_residual_ctx(data: &[u8], nibbles: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() {
        return None;
    }
    assert_eq!(data.len(), nibbles.len());
    let mut m = Mix::new();
    let mut e = Enc::new();
    for (i, &b) in data.iter().enumerate() {
        m.set_match_nibble(nibbles[i]);
        for bi in (0..8).rev() {
            m.ctx();
            let (q, st) = m.mix_p();
            let bit = ((b >> bi) & 1) as u32;
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
    Some(out)
}

pub fn decode_residual_ctx(buf: &[u8], nibbles: &[u8]) -> Result<Vec<u8>, &'static str> {
    if buf.len() < 9 || &buf[..4] != MAGIC || buf[4] != VER {
        return Err("fastcm");
    }
    let n = u32::from_le_bytes(buf[5..9].try_into().unwrap()) as usize;
    if n == 0 {
        return Err("fastcm empty");
    }
    if nibbles.len() != n {
        return Err("fastcm nibble");
    }
    let mut d = Dec::open(&buf[9..])?;
    let mut m = Mix::new();
    let mut out = Vec::with_capacity(n);
    for ni in 0..n {
        m.set_match_nibble(nibbles[ni]);
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
    Ok(out)
}

/// True when FastCM must be skipped (empty residual law).
pub fn should_skip(residual: &[u8]) -> bool {
    residual.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_residual_skips_cm() {
        assert!(should_skip(&[]));
        assert!(encode_residual(&[]).is_none());
        assert!(encode_residual_ctx(&[], &[]).is_none());
    }

    #[test]
    fn tiny_nonempty_residual_roundtrip() {
        let s = b"mozilla-binary-ish\x00\x01\x02\xff".repeat(40);
        assert!(!should_skip(&s));
        let e = encode_residual(&s).expect("encode");
        assert!(e.starts_with(MAGIC));
        assert_eq!(decode_residual(&e).expect("dec"), s.as_slice());
        // Should shrink repetitive residual.
        assert!(e.len() < s.len(), "expanded {} -> {}", s.len(), e.len());
    }

    #[test]
    fn ctx_residual_roundtrip() {
        let s: Vec<u8> = (0..200u8).cycle().take(400).collect();
        let nibbles: Vec<u8> = (0..400).map(|i| (i % 16) as u8).collect();
        let e = encode_residual_ctx(&s, &nibbles).expect("enc");
        assert_eq!(decode_residual_ctx(&e, &nibbles).expect("dec"), s);
    }
}
