//! Opcode-class order-1 rANS. Context is the previous decoded byte
//! folded into x86-ish families (REX, MOV, CALL/JMP, PUSH, ALU, zeros).

use crate::rans::{normalize_counts, RansEnc, RANS_L, SCALE, SCALE_BITS};

pub const MAGIC: &[u8; 4] = b"AN2\0";
pub const NCTX: usize = 32;

#[inline]
pub fn ctx_op(prev: u8) -> usize {
    match prev {
        0x00 => 0,
        0xFF => 1,
        0x90 => 2,
        0xC3 | 0xC2 => 3,
        0xE8 => 4,
        0xE9 => 5,
        0xEB => 6,
        0x48..=0x4F => 7,  // REX
        0x50..=0x57 => 8,  // push r
        0x58..=0x5F => 9,  // pop r
        0x80..=0x83 => 10, // alu imm
        0x88 => 11,
        0x89 => 12,
        0x8A => 13,
        0x8B => 14,
        0x8D => 15, // lea
        0xCC | 0xCD => 16,
        0x70..=0x7F => 17, // jcc
        0xB8..=0xBF => 18, // mov r32,imm
        0x01 | 0x03 | 0x09 | 0x0B | 0x21 | 0x23 | 0x29 | 0x2B => 19,
        0x31 | 0x33 => 20, // xor
        0x85 | 0x09 => 21,
        0xC7 => 22,
        0x8B => 14,
        _ => 23 + (prev >> 5) as usize, // 23..30
    }
}

pub fn rans_encode_op(pairs: &[(u8, u8)]) -> Vec<u8> {
    let mut counts = [[0u32; 256]; NCTX];
    for &(prev, s) in pairs {
        counts[ctx_op(prev)][s as usize] += 1;
    }
    let mut freq = [[0u32; 256]; NCTX];
    let mut start = [[0u32; 256]; NCTX];
    for c in 0..NCTX {
        let (f, st) = normalize_counts(&counts[c]);
        freq[c] = f;
        start[c] = st;
    }
    let mut enc = RansEnc::new();
    let mut stream = Vec::with_capacity(pairs.len() / 2 + 16);
    for &(prev, s) in pairs.iter().rev() {
        let c = ctx_op(prev);
        enc.encode(freq[c][s as usize], start[c][s as usize], SCALE, &mut stream);
    }
    enc.flush(&mut stream);
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(pairs.len() as u32).to_le_bytes());
    out.push(SCALE_BITS as u8);
    out.push(NCTX as u8);
    for c in 0..NCTX {
        let used: Vec<(u8, u16)> = (0..256)
            .filter(|&s| freq[c][s] > 0)
            .map(|s| (s as u8, freq[c][s] as u16))
            .collect();
        out.extend_from_slice(&(used.len() as u16).to_le_bytes());
        for (s, f) in used {
            out.push(s);
            out.extend_from_slice(&f.to_le_bytes());
        }
    }
    out.extend_from_slice(&(stream.len() as u32).to_le_bytes());
    out.extend_from_slice(&stream);
    out
}

pub struct OpDec<'a> {
    pub orig: usize,
    state: u64,
    cursor: usize,
    stream: &'a [u8],
    freq: [[u32; 256]; NCTX],
    start: [[u32; 256]; NCTX],
}

impl<'a> OpDec<'a> {
    pub fn open(buf: &'a [u8]) -> Result<Self, &'static str> {
        if buf.len() < 10 || &buf[..4] != MAGIC {
            return Err("op: magic");
        }
        let orig = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
        if buf[8] as u32 != SCALE_BITS || buf[9] as usize != NCTX {
            return Err("op: hdr");
        }
        let mut pos = 10usize;
        let mut freq = [[0u32; 256]; NCTX];
        let mut start = [[0u32; 256]; NCTX];
        for c in 0..NCTX {
            if pos + 2 > buf.len() {
                return Err("op: nused");
            }
            let n_used = u16::from_le_bytes(buf[pos..pos + 2].try_into().unwrap()) as usize;
            pos += 2;
            for _ in 0..n_used {
                if pos + 3 > buf.len() {
                    return Err("op: freq");
                }
                let s = buf[pos] as usize;
                let f = u16::from_le_bytes(buf[pos + 1..pos + 3].try_into().unwrap()) as u32;
                freq[c][s] = f;
                pos += 3;
            }
            let mut run = 0u32;
            for s in 0..256 {
                start[c][s] = run;
                run += freq[c][s];
            }
            if run != 0 && run != SCALE {
                return Err("op: sum");
            }
            if run == 0 {
                freq[c][0] = SCALE;
                start[c][0] = 0;
            }
        }
        if pos + 4 > buf.len() {
            return Err("op: slen");
        }
        let slen = u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        if pos + slen > buf.len() {
            return Err("op: stream");
        }
        let stream = &buf[pos..pos + slen];
        if slen < 8 {
            return Err("op: state");
        }
        let state = u64::from_le_bytes(stream[slen - 8..].try_into().unwrap());
        Ok(Self {
            orig,
            state,
            cursor: slen - 8,
            stream,
            freq,
            start,
        })
    }

    pub fn next(&mut self, prev: u8) -> Result<u8, &'static str> {
        let c = ctx_op(prev);
        let mask = (SCALE - 1) as u64;
        let slot = (self.state & mask) as u32;
        let mut s = 0u8;
        for cand in 0..256 {
            if self.freq[c][cand] == 0 {
                continue;
            }
            if slot >= self.start[c][cand] && slot < self.start[c][cand] + self.freq[c][cand] {
                s = cand as u8;
                break;
            }
        }
        let f = self.freq[c][s as usize] as u64;
        let cum = self.start[c][s as usize] as u64;
        self.state = f * (self.state >> SCALE_BITS) + (self.state & mask) - cum;
        while self.state < RANS_L {
            if self.cursor == 0 {
                return Err("op: underrun");
            }
            self.cursor -= 1;
            self.state = (self.state << 8) | self.stream[self.cursor] as u64;
        }
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rex_mov_class() {
        assert_eq!(ctx_op(0x48), 7);
        assert_eq!(ctx_op(0x89), 12);
        assert_eq!(ctx_op(0xE8), 4);
        assert_ne!(ctx_op(0x48), ctx_op(0x00));
    }
}
