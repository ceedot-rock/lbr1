//! LBR1 v2 container: bit flags + varint dist + o0 rANS + o1 literals.

use crate::parse::{Tok, MIN_MATCH};
use crate::rans;
use crate::rans_o1;

pub const MAGIC: &[u8; 4] = b"LBR1";
pub const VER: u8 = 4;
pub const VER_RC: u8 = 5;
/// True 8-byte solid-run frame: TR8 + symbol + u32 len.
pub const TRU8: &[u8; 3] = b"TR8";
pub const TRU8_LEN: usize = 8;
pub const TRU8_MAX: u32 = u32::MAX;

pub fn pack_tru8(sym: u8, len: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(TRU8_LEN);
    out.extend_from_slice(TRU8);
    out.push(sym);
    out.extend_from_slice(&len.to_le_bytes());
    out
}

pub fn unpack_tru8(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if buf.len() != TRU8_LEN || &buf[..3] != TRU8 {
        return Err("tru8");
    }
    let sym = buf[3];
    let n = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
    Ok(vec![sym; n])
}

pub fn is_tru8(buf: &[u8]) -> bool {
    buf.len() == TRU8_LEN && &buf[..3] == TRU8
}

pub const TR8X: &[u8; 4] = b"TR8X";

fn put_uleb(out: &mut Vec<u8>, mut n: u32) {
    while n >= 0x80 {
        out.push((n as u8) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

fn take_uleb(buf: &[u8], i: &mut usize) -> Result<u32, &'static str> {
    let mut n = 0u32;
    let mut shift = 0;
    loop {
        if *i >= buf.len() {
            return Err("uleb");
        }
        let b = buf[*i];
        *i += 1;
        n |= ((b & 0x7f) as u32) << shift;
        if b < 0x80 {
            return Ok(n);
        }
        shift += 7;
        if shift > 28 {
            return Err("uleb ov");
        }
    }
}

/// Mode byte + exception count. None if dense enough that TR8X cannot win.
pub fn sparse_mode(data: &[u8]) -> Option<(u8, usize)> {
    if data.len() < 16 {
        return None;
    }
    let mut cnt = [0u32; 256];
    for &b in data {
        cnt[b as usize] += 1;
    }
    let mut mode = 0usize;
    for s in 1..256 {
        if cnt[s] > cnt[mode] {
            mode = s;
        }
    }
    let nnz = data.len() - cnt[mode] as usize;
    // ~13 byte header + ~2–5 bytes per exception. Need headroom vs raw.
    if nnz == 0 {
        return None;
    }
    if nnz * 3 + 16 >= data.len() {
        return None;
    }
    Some((mode as u8, nnz))
}

pub fn pack_tr8x(data: &[u8], sym: u8) -> Vec<u8> {
    let mut out = Vec::from(*TR8X);
    out.push(sym);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    let mut payload = Vec::new();
    let mut nnz = 0u32;
    let mut prev = 0u32;
    for (i, &b) in data.iter().enumerate() {
        if b != sym {
            let gap = i as u32 - prev;
            put_uleb(&mut payload, gap);
            payload.push(b);
            prev = i as u32 + 1;
            nnz += 1;
        }
    }
    out.extend_from_slice(&nnz.to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

pub fn unpack_tr8x(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if buf.len() < 13 || &buf[..4] != TR8X {
        return Err("tr8x");
    }
    let sym = buf[4];
    let n = u32::from_le_bytes(buf[5..9].try_into().unwrap()) as usize;
    let nnz = u32::from_le_bytes(buf[9..13].try_into().unwrap()) as usize;
    let mut out = vec![sym; n];
    let mut i = 13usize;
    let mut pos = 0u32;
    for _ in 0..nnz {
        let gap = take_uleb(buf, &mut i)?;
        if i >= buf.len() {
            return Err("tr8x val");
        }
        pos += gap;
        if pos as usize >= n {
            return Err("tr8x pos");
        }
        out[pos as usize] = buf[i];
        i += 1;
        pos += 1;
    }
    if i != buf.len() {
        return Err("tr8x tail");
    }
    Ok(out)
}

pub fn is_tr8x(buf: &[u8]) -> bool {
    buf.len() >= 4 && &buf[..4] == TR8X
}

/// All bytes equal and long enough that an 8-byte box is a win.
pub fn solid_run(data: &[u8]) -> Option<(u8, u32)> {
    if data.len() < TRU8_LEN {
        return None;
    }
    if data.len() > TRU8_MAX as usize {
        return None;
    }
    let s = data[0];
    if data.iter().all(|&b| b == s) {
        Some((s, data.len() as u32))
    } else {
        None
    }
}

fn put_blob(out: &mut Vec<u8>, blob: &[u8]) {
    out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    out.extend_from_slice(blob);
}

fn take_blob<'a>(buf: &'a [u8], pos: &mut usize) -> Result<&'a [u8], &'static str> {
    if *pos + 4 > buf.len() {
        return Err("blob len");
    }
    let n = u32::from_le_bytes(buf[*pos..*pos + 4].try_into().unwrap()) as usize;
    *pos += 4;
    if *pos + n > buf.len() {
        return Err("blob body");
    }
    let s = &buf[*pos..*pos + n];
    *pos += n;
    Ok(s)
}

fn pack_bits(bits: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + bits.len() / 8 + 1);
    out.extend_from_slice(&(bits.len() as u32).to_le_bytes());
    let mut acc = 0u8;
    let mut n = 0;
    for &b in bits {
        if b != 0 {
            acc |= 1 << n;
        }
        n += 1;
        if n == 8 {
            out.push(acc);
            acc = 0;
            n = 0;
        }
    }
    if n > 0 {
        out.push(acc);
    }
    out
}

fn unpack_bits(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if buf.len() < 4 {
        return Err("bits");
    }
    let count = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
    let mut out = Vec::with_capacity(count);
    let mut i = 4;
    while out.len() < count {
        if i >= buf.len() {
            return Err("bits underrun");
        }
        let acc = buf[i];
        i += 1;
        for n in 0..8 {
            if out.len() == count {
                break;
            }
            out.push((acc >> n) & 1);
        }
    }
    Ok(out)
}

fn push_dist(dists: &mut Vec<u8>, d: u32) {
    if d < 128 {
        dists.push(d as u8);
    } else if d < 16384 {
        dists.push(0x80 | ((d >> 8) as u8));
        dists.push((d & 0xff) as u8);
    } else if d < (1 << 21) {
        dists.push(0xc0 | ((d >> 16) as u8));
        dists.push(((d >> 8) & 0xff) as u8);
        dists.push((d & 0xff) as u8);
    } else {
        dists.push(0xe0 | ((d >> 24) as u8));
        dists.push(((d >> 16) & 0xff) as u8);
        dists.push(((d >> 8) & 0xff) as u8);
        dists.push((d & 0xff) as u8);
    }
}

fn take_dist(dists: &[u8], i: &mut usize) -> Result<u32, &'static str> {
    if *i >= dists.len() {
        return Err("dist");
    }
    let b0 = dists[*i];
    *i += 1;
    if b0 < 128 {
        Ok(b0 as u32)
    } else if b0 < 192 {
        if *i >= dists.len() {
            return Err("dist2");
        }
        let b1 = dists[*i];
        *i += 1;
        Ok((((b0 & 0x3f) as u32) << 8) | b1 as u32)
    } else {
        if *i + 1 >= dists.len() {
            return Err("dist3");
        }
        if b0 < 0xe0 {
            if *i + 1 >= dists.len() {
                return Err("dist3");
            }
            let b1 = dists[*i];
            let b2 = dists[*i + 1];
            *i += 2;
            return Ok((((b0 & 0x1f) as u32) << 16) | ((b1 as u32) << 8) | b2 as u32);
        }
        if *i + 2 >= dists.len() {
            return Err("dist4");
        }
        let b1 = dists[*i];
        let b2 = dists[*i + 1];
        let b3 = dists[*i + 2];
        *i += 3;
        Ok((((b0 & 0x1f) as u32) << 24) | ((b1 as u32) << 16) | ((b2 as u32) << 8) | b3 as u32)
    }
}

fn split_streams(toks: &[Tok]) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut flags = Vec::new();
    let mut lens = Vec::new();
    let mut dists = Vec::new();
    for t in toks {
        match *t {
            Tok::Lit(_) => flags.push(0),
            Tok::Match { dist, len } => {
                flags.push(1);
                let extra = (len as usize).saturating_sub(MIN_MATCH);
                if extra < 255 {
                    lens.push(extra as u8);
                } else {
                    lens.push(255);
                    lens.extend_from_slice(&(extra as u16).to_le_bytes());
                }
                push_dist(&mut dists, dist);
            }
        }
    }
    (pack_bits(&flags), lens, dists)
}

fn lit_pairs(toks: &[Tok], raw: &[u8]) -> Vec<(u8, u8)> {
    let mut pairs = Vec::new();
    let mut pos = 0usize;
    let mut prev = 0u8;
    for t in toks {
        match *t {
            Tok::Lit(b) => {
                pairs.push((prev, b));
                prev = b;
                pos += 1;
            }
            Tok::Match { len, .. } => {
                pos += len as usize;
                if pos > 0 && pos <= raw.len() {
                    prev = raw[pos - 1];
                }
            }
        }
    }
    pairs
}

pub fn pack(toks: &[Tok], orig_len: usize, window: u32, raw: &[u8]) -> Vec<u8> {
    let (flags, lens, dists) = split_streams(toks);
    let pairs = lit_pairs(toks, raw);
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.push(VER);
    out.extend_from_slice(&(orig_len as u32).to_le_bytes());
    out.extend_from_slice(&window.to_le_bytes());
    let lits: Vec<u8> = pairs.iter().map(|&(_, b)| b).collect();
    let o0 = rans::rans_encode(&lits);
    let mut sent = crate::sentinel::Sentinel::new();
    let obs = crate::sentinel::Sentinel::observe_stream(&lits, o0.len());
    let tick = sent.step(obs, obs[0], 0.0);
    let mut seen = [false; 256];
    let mut alpha = 0usize;
    for &b in &lits {
        let i = b as usize;
        if !seen[i] {
            seen[i] = true;
            alpha += 1;
        }
    }
    let (kind, lit_blob) = if crate::sentinel::alleviate_skip_o1(&tick, lits.len(), alpha) {
        (0u8, o0)
    } else {
        let o1 = rans_o1::rans_encode_o1(&pairs);
        let op = crate::rans_op::rans_encode_op(&pairs);
        let mut kind = 0u8;
        let mut blob = o0;
        if o1.len() < blob.len() {
            kind = 1;
            blob = o1;
        }
        if op.len() < blob.len() {
            kind = 2;
            blob = op;
        }
        (kind, blob)
    };
    put_blob(&mut out, &rans::rans_encode(&flags));
    out.push(kind);
    put_blob(&mut out, &lit_blob);
    put_blob(&mut out, &rans::rans_encode(&lens));
    put_blob(&mut out, &rans::rans_encode(&dists));
    {
        let rc = crate::range::encode_toks(toks, raw);
        if let Ok(back) = crate::range::decode_toks(&rc, orig_len) {
            if back.as_slice() == raw {
                let mut alt = Vec::with_capacity(13 + 4 + rc.len());
                alt.extend_from_slice(MAGIC);
                alt.push(VER_RC);
                alt.extend_from_slice(&(orig_len as u32).to_le_bytes());
                alt.extend_from_slice(&window.to_le_bytes());
                put_blob(&mut alt, &rc);
                // VER5 measurement: ship RC whenever it inverts.
                return alt;
            }
        }
    }
    out
}

/// Decode payload directly to bytes (o1 lits need running output for context).
pub fn unpack_bytes(buf: &[u8]) -> Result<(Vec<u8>, u32), &'static str> {
    if buf.len() < 13 || &buf[..4] != MAGIC {
        return Err("magic");
    }
    if buf[4] == VER_RC {
        let orig = u32::from_le_bytes(buf[5..9].try_into().unwrap()) as usize;
        let mut pos = 13usize;
        let rc = take_blob(buf, &mut pos)?;
        let window = u32::from_le_bytes(buf[9..13].try_into().unwrap());
        return crate::range::decode_toks(rc, orig).map(|o| (o, window));
    }
    if buf[4] != VER {
        return Err("ver");
    }
    let orig = u32::from_le_bytes(buf[5..9].try_into().unwrap()) as usize;
    let window = u32::from_le_bytes(buf[9..13].try_into().unwrap());
    let mut pos = 13usize;
    let flags_raw = rans::rans_decode(take_blob(buf, &mut pos)?)?;
    let flags = unpack_bits(&flags_raw)?;
    if pos >= buf.len() {
        return Err("kind");
    }
    let kind = buf[pos];
    pos += 1;
    {
        let mut sent = crate::sentinel::Sentinel::new();
        let tick = sent.step(
            [0.0, 0.0, if kind == 1 { 1.0 } else { 0.0 }, 0.0, 0.0],
            0.0,
            0.0,
        );
        crate::sentinel::decode_breakpoint(&tick, kind <= 2)?;
    }
    let lit_blob = take_blob(buf, &mut pos)?;
    let lens = rans::rans_decode(take_blob(buf, &mut pos)?)?;
    let dists = rans::rans_decode(take_blob(buf, &mut pos)?)?;

    let nlit = flags.iter().filter(|&&f| f == 0).count();
    let lits_o0 = if kind == 0 {
        Some(rans::rans_decode(lit_blob)?)
    } else {
        None
    };
    let mut dec = if kind == 1 {
        let d = rans_o1::O1Dec::open(lit_blob)?;
        if d.orig != nlit {
            return Err("o1 nlit");
        }
        Some(d)
    } else {
        None
    };
    let mut dec_op = if kind == 2 {
        let d = crate::rans_op::OpDec::open(lit_blob)?;
        if d.orig != nlit {
            return Err("op nlit");
        }
        Some(d)
    } else {
        None
    };
    let mut li = 0usize;
    let mut out = Vec::with_capacity(orig);
    let mut ni = 0usize;
    let mut di = 0usize;
    let mut prev = 0u8;
    let mut prev_off = 0u32;
    for &f in &flags {
        if f == 0 {
            let b = if let Some(ref mut d) = dec {
                d.next(prev)?
            } else if let Some(ref mut d) = dec_op {
                d.next(prev)?
            } else {
                let l = lits_o0.as_ref().ok_or("lits")?;
                if li >= l.len() {
                    return Err("lit");
                }
                let b = l[li];
                li += 1;
                b
            };
            out.push(b);
            prev = b;
        } else {
            if ni >= lens.len() {
                return Err("len");
            }
            let b = lens[ni];
            ni += 1;
            let extra = if b < 255 {
                b as u32
            } else {
                if ni + 2 > lens.len() {
                    return Err("len2");
                }
                let v = u16::from_le_bytes([lens[ni], lens[ni + 1]]) as u32;
                ni += 2;
                v
            };
            let dist = take_dist(&dists, &mut di)?;
            let d = if dist == 0 {
                if prev_off == 0 {
                    return Err("rep0");
                }
                prev_off
            } else {
                dist
            } as usize;
            let nlen = extra as usize + MIN_MATCH;
            if d == 0 || d > out.len() {
                return Err("dist");
            }
            prev_off = d as u32;
            if d >= nlen {
                let src = out.len() - d;
                out.extend_from_within(src..src + nlen);
            } else {
                let mut rem = nlen;
                while rem > 0 {
                    let src = out.len() - d;
                    let chunk = rem.min(d);
                    out.extend_from_within(src..src + chunk);
                    rem -= chunk;
                }
            }
            prev = *out.last().unwrap();
        }
        if out.len() > orig {
            return Err("over");
        }
    }
    if out.len() != orig {
        return Err("len");
    }
    Ok((out, window))
}
