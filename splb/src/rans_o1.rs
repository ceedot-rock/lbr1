//! Order-1 rANS. 16 contexts from high nibble of previous decoded byte.

use crate::rans::{RansEnc, SCALE, SCALE_BITS, RANS_L, normalize_counts};

pub const MAGIC: &[u8; 4] = b"AN1\0";
pub const NCTX: usize = 16;

#[inline]
pub fn ctx_of(prev: u8) -> usize {
    (prev >> 4) as usize
}

pub fn rans_encode_o1(pairs: &[(u8, u8)]) -> Vec<u8> {
    let mut counts = [[0u32; 256]; NCTX];
    for &(prev, s) in pairs {
        counts[ctx_of(prev)][s as usize] += 1;
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
        let c = ctx_of(prev);
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

pub fn rans_decode_o1(buf: &[u8], prevs: &[u8]) -> Result<Vec<u8>, &'static str> {
    if buf.len() < 8 {
        return Err("o1: short");
    }
    if &buf[..4] != MAGIC {
        return Err("o1: magic");
    }
    let orig = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
    if prevs.len() != orig {
        return Err("o1: prevs");
    }
    if buf[8] as u32 != SCALE_BITS {
        return Err("o1: scale");
    }
    if buf[9] as usize != NCTX {
        return Err("o1: nctx");
    }
    let mut pos = 10usize;
    let mut freq = [[0u32; 256]; NCTX];
    let mut start = [[0u32; 256]; NCTX];
    for c in 0..NCTX {
        if pos + 2 > buf.len() {
            return Err("o1: nused");
        }
        let n_used = u16::from_le_bytes(buf[pos..pos + 2].try_into().unwrap()) as usize;
        pos += 2;
        for _ in 0..n_used {
            if pos + 3 > buf.len() {
                return Err("o1: freq");
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
            return Err("o1: sum");
        }
        if run == 0 {
            freq[c][0] = SCALE;
            start[c][0] = 0;
        }
    }
    if pos + 4 > buf.len() {
        return Err("o1: slen");
    }
    let slen = u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
    pos += 4;
    if pos + slen > buf.len() {
        return Err("o1: stream");
    }
    let stream = &buf[pos..pos + slen];
    if slen < 8 {
        return Err("o1: state");
    }
    let mut state = u64::from_le_bytes(stream[slen - 8..].try_into().unwrap());
    let mut cursor = slen - 8;
    let mask = (SCALE - 1) as u64;
    let mut out = vec![0u8; orig];
    for i in 0..orig {
        let c = ctx_of(prevs[i]);
        let slot = (state & mask) as u32;
        let mut s = 0u8;
        for cand in 0..256 {
            if freq[c][cand] == 0 {
                continue;
            }
            if slot >= start[c][cand] && slot < start[c][cand] + freq[c][cand] {
                s = cand as u8;
                break;
            }
        }
        out[i] = s;
        let f = freq[c][s as usize] as u64;
        let cum = start[c][s as usize] as u64;
        state = f * (state >> SCALE_BITS) + (state & mask) - cum;
        while state < RANS_L {
            if cursor == 0 {
                return Err("o1: underrun");
            }
            cursor -= 1;
            state = (state << 8) | stream[cursor] as u64;
        }
    }
    Ok(out)
}

/// Streaming order-1 decoder. Symbols consumed in the same order they were encoded.
pub struct O1Dec<'a> {
    pub orig: usize,
    state: u64,
    cursor: usize,
    stream: &'a [u8],
    freq: [[u32; 256]; NCTX],
    start: [[u32; 256]; NCTX],
}

impl<'a> O1Dec<'a> {
    pub fn open(buf: &'a [u8]) -> Result<Self, &'static str> {
        if buf.len() < 10 {
            return Err("o1: short");
        }
        if &buf[..4] != MAGIC {
            return Err("o1: magic");
        }
        let orig = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
        if buf[8] as u32 != SCALE_BITS {
            return Err("o1: scale");
        }
        if buf[9] as usize != NCTX {
            return Err("o1: nctx");
        }
        let mut pos = 10usize;
        let mut freq = [[0u32; 256]; NCTX];
        let mut start = [[0u32; 256]; NCTX];
        for c in 0..NCTX {
            if pos + 2 > buf.len() {
                return Err("o1: nused");
            }
            let n_used = u16::from_le_bytes(buf[pos..pos + 2].try_into().unwrap()) as usize;
            pos += 2;
            for _ in 0..n_used {
                if pos + 3 > buf.len() {
                    return Err("o1: freq");
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
                return Err("o1: sum");
            }
            if run == 0 {
                freq[c][0] = SCALE;
                start[c][0] = 0;
            }
        }
        if pos + 4 > buf.len() {
            return Err("o1: slen");
        }
        let slen = u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        if pos + slen > buf.len() {
            return Err("o1: stream");
        }
        let stream = &buf[pos..pos + slen];
        if slen < 8 {
            return Err("o1: state");
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
        let c = ctx_of(prev);
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
                return Err("o1: underrun");
            }
            self.cursor -= 1;
            self.state = (self.state << 8) | self.stream[self.cursor] as u64;
        }
        Ok(s)
    }
}
