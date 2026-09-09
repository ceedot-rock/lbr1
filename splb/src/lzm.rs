//! LZM1 — own LZMA-style gene. Pavlov LZMA SDK is public domain; this is a
//! reimplementation, not host `xz` and not an XZ1 skin.
//!
//! 4-rep distances, match-byte literals, range coder, 4 MiB window.

use crate::range::{Dec, Enc};

pub const MAGIC: &[u8; 4] = b"LZM1";
pub const VER: u8 = 1;
const LC: u32 = 2;
const LP: u32 = 0;
const PB: u32 = 2;
const LIT_STATES: usize = 1 << LC; // 4
const POS_STATES: usize = 1 << PB; // 4
const LIT_SIZE: usize = 0x300;
const STATES: usize = 12;
const HASH_BITS: usize = 18;
const HASH: usize = 1 << HASH_BITS;
const WIN: usize = 1 << 22;
const CHAIN: usize = 24;
const MIN_NEW: usize = 3;
const MAX_LEN: usize = 273; // 2 + 8 + 8 + 256 - 1

struct Model {
    is_match: [[u16; POS_STATES]; STATES],
    is_rep: [u16; STATES],
    is_rep_g0: [u16; STATES],
    is_rep_g1: [u16; STATES],
    is_rep_g2: [u16; STATES],
    is_rep0_long: [[u16; POS_STATES]; STATES],
    lit: Vec<u16>,
    len_choice: u16,
    len_choice2: u16,
    len_low: [[u16; 8]; POS_STATES],
    len_mid: [[u16; 8]; POS_STATES],
    len_high: [u16; 256],
    rep_len_choice: u16,
    rep_len_choice2: u16,
    rep_len_low: [[u16; 8]; POS_STATES],
    rep_len_mid: [[u16; 8]; POS_STATES],
    rep_len_high: [u16; 256],
    pos_slot: [[u16; 64]; 4],
    spec: Vec<u16>,
    align: [u16; 16],
}

impl Model {
    fn new() -> Self {
        Self {
            is_match: [[1024; POS_STATES]; STATES],
            is_rep: [1024; STATES],
            is_rep_g0: [1024; STATES],
            is_rep_g1: [1024; STATES],
            is_rep_g2: [1024; STATES],
            is_rep0_long: [[1024; POS_STATES]; STATES],
            lit: vec![1024; LIT_STATES * LIT_SIZE],
            len_choice: 1024,
            len_choice2: 1024,
            len_low: [[1024; 8]; POS_STATES],
            len_mid: [[1024; 8]; POS_STATES],
            len_high: [1024; 256],
            rep_len_choice: 1024,
            rep_len_choice2: 1024,
            rep_len_low: [[1024; 8]; POS_STATES],
            rep_len_mid: [[1024; 8]; POS_STATES],
            rep_len_high: [1024; 256],
            pos_slot: [[1024; 64]; 4],
            spec: vec![1024; 10 * 64],
            align: [1024; 16],
        }
    }
}

fn hash3(d: &[u8], i: usize) -> usize {
    let v = (d[i] as u32)
        | ((d[i + 1] as u32) << 8)
        | ((d[i + 2] as u32) << 16);
    (v.wrapping_mul(0x9E3779B1) >> (32 - HASH_BITS as u32)) as usize
}

fn match_len(d: &[u8], a: usize, b: usize, cap: usize) -> usize {
    let max = cap.min(d.len() - a).min(d.len() - b);
    let mut n = 0;
    while n + 8 <= max {
        let x = u64::from_le_bytes(d[a + n..a + n + 8].try_into().unwrap());
        let y = u64::from_le_bytes(d[b + n..b + n + 8].try_into().unwrap());
        if x != y {
            break;
        }
        n += 8;
    }
    while n < max && d[a + n] == d[b + n] {
        n += 1;
    }
    n
}

fn pos_state(i: usize) -> usize {
    i & (POS_STATES - 1)
}

fn lit_index(prev: u8) -> usize {
    ((prev as usize) >> (8 - LC as usize)) * LIT_SIZE
}

fn next_state_lit(s: usize) -> usize {
    if s < 4 {
        0
    } else if s < 10 {
        s - 3
    } else {
        s - 6
    }
}

fn next_state_match(s: usize) -> usize {
    if s < 7 {
        7
    } else {
        10
    }
}

fn next_state_rep(s: usize) -> usize {
    if s < 7 {
        8
    } else {
        11
    }
}

fn next_state_short(s: usize) -> usize {
    if s < 7 {
        9
    } else {
        11
    }
}

fn pos_slot(dist: u32) -> u32 {
    if dist < 4 {
        dist
    } else {
        let msb = 31 - dist.leading_zeros();
        (msb << 1) + ((dist >> (msb - 1)) & 1)
    }
}

fn enc_tree(e: &mut Enc, v: u32, bits: u32, probs: &mut [u16]) {
    let mut ctx = 1u32;
    for i in (0..bits).rev() {
        let bit = (v >> i) & 1;
        e.bit(bit, &mut probs[ctx as usize]);
        ctx = (ctx << 1) | bit;
    }
}

fn dec_tree(d: &mut Dec, bits: u32, probs: &mut [u16]) -> u32 {
    let mut ctx = 1u32;
    for _ in 0..bits {
        let bit = d.bit(&mut probs[ctx as usize]);
        ctx = (ctx << 1) | bit;
    }
    ctx - (1 << bits)
}

fn enc_tree_rev(e: &mut Enc, v: u32, bits: u32, probs: &mut [u16]) {
    let mut ctx = 1u32;
    for i in 0..bits {
        let bit = (v >> i) & 1;
        e.bit(bit, &mut probs[ctx as usize]);
        ctx = (ctx << 1) | bit;
    }
}

fn dec_tree_rev(d: &mut Dec, bits: u32, probs: &mut [u16]) -> u32 {
    let mut ctx = 1u32;
    let mut v = 0u32;
    for i in 0..bits {
        let bit = d.bit(&mut probs[ctx as usize]);
        v |= bit << i;
        ctx = (ctx << 1) | bit;
    }
    v
}

fn enc_len(
    e: &mut Enc,
    len: u32,
    ps: usize,
    choice: &mut u16,
    choice2: &mut u16,
    low: &mut [[u16; 8]; POS_STATES],
    mid: &mut [[u16; 8]; POS_STATES],
    high: &mut [u16; 256],
) {
    let l = len - 2;
    if l < 8 {
        e.bit(0, choice);
        enc_tree(e, l, 3, &mut low[ps]);
    } else {
        e.bit(1, choice);
        if l < 16 {
            e.bit(0, choice2);
            enc_tree(e, l - 8, 3, &mut mid[ps]);
        } else {
            e.bit(1, choice2);
            enc_tree(e, (l - 16).min(255), 8, high);
        }
    }
}

fn dec_len(
    d: &mut Dec,
    ps: usize,
    choice: &mut u16,
    choice2: &mut u16,
    low: &mut [[u16; 8]; POS_STATES],
    mid: &mut [[u16; 8]; POS_STATES],
    high: &mut [u16; 256],
) -> u32 {
    if d.bit(choice) == 0 {
        2 + dec_tree(d, 3, &mut low[ps])
    } else if d.bit(choice2) == 0 {
        10 + dec_tree(d, 3, &mut mid[ps])
    } else {
        18 + dec_tree(d, 8, high)
    }
}

fn enc_lit(e: &mut Enc, m: &mut Model, symbol: u8, prev: u8, match_byte: u8, state: usize) {
    let off = lit_index(prev);
    let probs = &mut m.lit[off..off + LIT_SIZE];
    if state >= 7 {
        let mut ctx = 1u32;
        let mut mb = match_byte;
        for i in (0..8).rev() {
            let match_bit = (mb >> 7) as u32;
            mb <<= 1;
            let bit = ((symbol >> i) & 1) as u32;
            let idx = ((1 + match_bit) << 8) + ctx;
            e.bit(bit, &mut probs[idx as usize]);
            ctx = (ctx << 1) | bit;
            if bit != match_bit {
                for j in (0..i).rev() {
                    let bit = ((symbol >> j) & 1) as u32;
                    e.bit(bit, &mut probs[ctx as usize]);
                    ctx = (ctx << 1) | bit;
                }
                return;
            }
        }
        return;
    }
    let mut ctx = 1u32;
    for i in (0..8).rev() {
        let bit = ((symbol >> i) & 1) as u32;
        e.bit(bit, &mut probs[ctx as usize]);
        ctx = (ctx << 1) | bit;
    }
}

fn dec_lit(d: &mut Dec, m: &mut Model, prev: u8, match_byte: u8, state: usize) -> u8 {
    let off = lit_index(prev);
    let probs = &mut m.lit[off..off + LIT_SIZE];
    if state >= 7 {
        let mut ctx = 1u32;
        let mut mb = match_byte;
        for _ in 0..8 {
            let match_bit = (mb >> 7) as u32;
            mb <<= 1;
            let idx = ((1 + match_bit) << 8) + ctx;
            let bit = d.bit(&mut probs[idx as usize]);
            ctx = (ctx << 1) | bit;
            if bit != match_bit {
                while ctx < 256 {
                    let bit = d.bit(&mut probs[ctx as usize]);
                    ctx = (ctx << 1) | bit;
                }
                return ctx as u8;
            }
        }
        return ctx as u8;
    }
    let mut ctx = 1u32;
    while ctx < 256 {
        let bit = d.bit(&mut probs[ctx as usize]);
        ctx = (ctx << 1) | bit;
    }
    ctx as u8
}

fn enc_dist(e: &mut Enc, m: &mut Model, dist: u32, len: u32) {
    let slot = pos_slot(dist);
    let ls = (len - 2).min(3) as usize;
    enc_tree(e, slot, 6, &mut m.pos_slot[ls]);
    if slot >= 4 {
        let footer = (slot >> 1) - 1;
        let base = (2 | (slot & 1)) << footer;
        let extra = dist - base;
        if slot < 14 {
            let n = footer as usize;
            let off = (slot as usize - 4) * 64;
            enc_tree_rev(e, extra, n as u32, &mut m.spec[off..off + 64]);
        } else {
            let direct = footer - 4;
            for i in (0..direct).rev() {
                let bit = (extra >> (i + 4)) & 1;
                // Direct bits: no adaptive model (LZMA does the same).
                let mut p = 1024u16;
                e.bit(bit, &mut p);
            }
            enc_tree_rev(e, extra & 15, 4, &mut m.align);
        }
    }
}

fn dec_dist(d: &mut Dec, m: &mut Model, len: u32) -> Result<u32, &'static str> {
    let ls = (len - 2).min(3) as usize;
    let slot = dec_tree(d, 6, &mut m.pos_slot[ls]);
    if slot < 4 {
        return Ok(slot);
    }
    let footer = (slot >> 1) - 1;
    let base = (2 | (slot & 1)) << footer;
    let extra = if slot < 14 {
        let n = footer;
        let off = (slot as usize - 4) * 64;
        dec_tree_rev(d, n, &mut m.spec[off..off + 64])
    } else {
        let direct = footer - 4;
        let mut v = 0u32;
        for i in (0..direct).rev() {
            let mut p = 1024u16;
            let bit = d.bit(&mut p);
            v |= bit << (i + 4);
        }
        v |= dec_tree_rev(d, 4, &mut m.align);
        v
    };
    Ok(base + extra)
}

fn find(
    data: &[u8],
    i: usize,
    head: &[u32],
    prev: &[u32],
    reps: [u32; 4],
) -> (usize, u32, i8) {
    let n = data.len();
    let cap = MAX_LEN.min(n - i);
    let mut best_l = 0usize;
    let mut best_d = 0u32;
    let mut best_rep: i8 = -1;
    for (k, &r) in reps.iter().enumerate() {
        let dist = r as usize;
        if dist == 0 || dist > i {
            continue;
        }
        let min = if k == 0 { 1 } else { 2 };
        if i + min <= n && data[i] == data[i - dist] && (min == 1 || data[i + 1] == data[i - dist + 1])
        {
            let l = match_len(data, i - dist, i, cap);
            if l >= min && (l > best_l || (l == best_l && best_rep < 0)) {
                best_l = l;
                best_d = r;
                best_rep = k as i8;
            }
        }
    }
    if i + MIN_NEW <= n {
        let h = hash3(data, i);
        let mut p = head[h];
        let mut walked = 0;
        while p != u32::MAX && walked < CHAIN {
            let j = p as usize;
            if i > j && i - j <= WIN {
                let l = match_len(data, j, i, cap);
                if l >= MIN_NEW && l > best_l {
                    best_l = l;
                    best_d = (i - j) as u32;
                    best_rep = -1;
                }
            }
            if j >= prev.len() {
                break;
            }
            p = prev[j % WIN];
            walked += 1;
        }
    }
    (best_l, best_d, best_rep)
}

fn shift_rep(reps: &mut [u32; 4], dist: u32) {
    reps[3] = reps[2];
    reps[2] = reps[1];
    reps[1] = reps[0];
    reps[0] = dist;
}

fn use_rep(reps: &mut [u32; 4], k: usize) {
    if k == 0 {
        return;
    }
    let d = reps[k];
    if k == 1 {
        reps[1] = reps[0];
        reps[0] = d;
    } else if k == 2 {
        reps[2] = reps[1];
        reps[1] = reps[0];
        reps[0] = d;
    } else {
        reps[3] = reps[2];
        reps[2] = reps[1];
        reps[1] = reps[0];
        reps[0] = d;
    }
}

fn insert(data: &[u8], i: usize, head: &mut [u32], prev: &mut [u32]) {
    if i + 2 >= data.len() {
        return;
    }
    let h = hash3(data, i);
    prev[i % WIN] = head[h];
    head[h] = i as u32;
}

pub fn is_lzm(buf: &[u8]) -> bool {
    buf.len() >= 10 && buf.starts_with(MAGIC) && buf[4] == VER
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
    let n = data.len();
    let mut m = Model::new();
    let mut e = Enc::new();
    let mut head = vec![u32::MAX; HASH];
    let mut prev = vec![u32::MAX; WIN.min(n).max(1)];
    let mut reps = [1u32, 1, 1, 1];
    let mut state = 0usize;
    let mut i = 0usize;
    while i < n {
        insert(data, i, &mut head, &mut prev);
        let ps = pos_state(i);
        let (mut ml, dist, which) = find(data, i, &head, &prev, reps);
        if ml >= 2 && i + 1 < n {
            insert(data, i + 1, &mut head, &mut prev);
            let (ml2, dist2, which2) = find(data, i + 1, &head, &prev, reps);
            if ml2 > ml && which2 >= 0 || ml2 > ml + 1 {
                ml = 0;
                let _ = (dist2, which2);
            }
        }
        let short = i >= reps[0] as usize && data[i] == data[i - reps[0] as usize];
        if ml >= 2 {
            e.bit(1, &mut m.is_match[state][ps]);
            if which >= 0 {
                e.bit(1, &mut m.is_rep[state]);
                match which {
                    0 => {
                        e.bit(0, &mut m.is_rep_g0[state]);
                        if ml == 1 {
                            e.bit(0, &mut m.is_rep0_long[state][ps]);
                            state = next_state_short(state);
                        } else {
                            e.bit(1, &mut m.is_rep0_long[state][ps]);
                            enc_len(
                                &mut e,
                                ml as u32,
                                ps,
                                &mut m.rep_len_choice,
                                &mut m.rep_len_choice2,
                                &mut m.rep_len_low,
                                &mut m.rep_len_mid,
                                &mut m.rep_len_high,
                            );
                            state = next_state_rep(state);
                        }
                    }
                    1 => {
                        e.bit(1, &mut m.is_rep_g0[state]);
                        e.bit(0, &mut m.is_rep_g1[state]);
                        enc_len(
                            &mut e,
                            ml as u32,
                            ps,
                            &mut m.rep_len_choice,
                            &mut m.rep_len_choice2,
                            &mut m.rep_len_low,
                            &mut m.rep_len_mid,
                            &mut m.rep_len_high,
                        );
                        state = next_state_rep(state);
                    }
                    2 => {
                        e.bit(1, &mut m.is_rep_g0[state]);
                        e.bit(1, &mut m.is_rep_g1[state]);
                        e.bit(0, &mut m.is_rep_g2[state]);
                        enc_len(
                            &mut e,
                            ml as u32,
                            ps,
                            &mut m.rep_len_choice,
                            &mut m.rep_len_choice2,
                            &mut m.rep_len_low,
                            &mut m.rep_len_mid,
                            &mut m.rep_len_high,
                        );
                        state = next_state_rep(state);
                    }
                    _ => {
                        e.bit(1, &mut m.is_rep_g0[state]);
                        e.bit(1, &mut m.is_rep_g1[state]);
                        e.bit(1, &mut m.is_rep_g2[state]);
                        enc_len(
                            &mut e,
                            ml as u32,
                            ps,
                            &mut m.rep_len_choice,
                            &mut m.rep_len_choice2,
                            &mut m.rep_len_low,
                            &mut m.rep_len_mid,
                            &mut m.rep_len_high,
                        );
                        state = next_state_rep(state);
                    }
                }
                use_rep(&mut reps, which as usize);
            } else {
                e.bit(0, &mut m.is_rep[state]);
                enc_len(
                    &mut e,
                    ml as u32,
                    ps,
                    &mut m.len_choice,
                    &mut m.len_choice2,
                    &mut m.len_low,
                    &mut m.len_mid,
                    &mut m.len_high,
                );
                enc_dist(&mut e, &mut m, dist - 1, ml as u32);
                shift_rep(&mut reps, dist);
                state = next_state_match(state);
            }
            let end = i + ml;
            i += 1;
            while i < end {
                insert(data, i, &mut head, &mut prev);
                i += 1;
            }
        } else if short && ml <= 1 {
            e.bit(1, &mut m.is_match[state][ps]);
            e.bit(1, &mut m.is_rep[state]);
            e.bit(0, &mut m.is_rep_g0[state]);
            e.bit(0, &mut m.is_rep0_long[state][ps]);
            state = next_state_short(state);
            i += 1;
        } else {
            e.bit(0, &mut m.is_match[state][ps]);
            let prev = if i > 0 { data[i - 1] } else { 0 };
            let mb = if i >= reps[0] as usize {
                data[i - reps[0] as usize]
            } else {
                0
            };
            enc_lit(&mut e, &mut m, data[i], prev, mb, state);
            state = next_state_lit(state);
            i += 1;
        }
        let _ = (dist, which);
    }
    let payload = e.finish();
    let mut out = Vec::with_capacity(10 + payload.len());
    out.extend_from_slice(MAGIC);
    out.push(VER);
    out.push((((LC & 0xf) << 4) | ((LP & 0x3) << 2) | (PB & 0x3)) as u8);
    out.extend_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

pub fn decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if !is_lzm(buf) {
        return Err("lzm1");
    }
    let n = u32::from_le_bytes(buf[6..10].try_into().unwrap()) as usize;
    let mut d = Dec::open(&buf[10..])?;
    let mut m = Model::new();
    let mut out = Vec::with_capacity(n);
    let mut reps = [1u32, 1, 1, 1];
    let mut state = 0usize;
    while out.len() < n {
        let i = out.len();
        let ps = pos_state(i);
        if d.bit(&mut m.is_match[state][ps]) == 0 {
            let prev = if i > 0 { out[i - 1] } else { 0 };
            let mb = if i >= reps[0] as usize {
                out[i - reps[0] as usize]
            } else {
                0
            };
            let b = dec_lit(&mut d, &mut m, prev, mb, state);
            out.push(b);
            state = next_state_lit(state);
            continue;
        }
        if d.bit(&mut m.is_rep[state]) == 1 {
            let which = if d.bit(&mut m.is_rep_g0[state]) == 0 {
                0
            } else if d.bit(&mut m.is_rep_g1[state]) == 0 {
                1
            } else if d.bit(&mut m.is_rep_g2[state]) == 0 {
                2
            } else {
                3
            };
            let len = if which == 0 && d.bit(&mut m.is_rep0_long[state][ps]) == 0 {
                state = next_state_short(state);
                1
            } else {
                let l = dec_len(
                    &mut d,
                    ps,
                    &mut m.rep_len_choice,
                    &mut m.rep_len_choice2,
                    &mut m.rep_len_low,
                    &mut m.rep_len_mid,
                    &mut m.rep_len_high,
                );
                state = next_state_rep(state);
                l
            };
            let dist = reps[which] as usize;
            if dist == 0 || dist > out.len() {
                return Err("lzm1 rep");
            }
            use_rep(&mut reps, which);
            for _ in 0..len {
                if out.len() == n {
                    break;
                }
                let b = out[out.len() - dist];
                out.push(b);
            }
        } else {
            let len = dec_len(
                &mut d,
                ps,
                &mut m.len_choice,
                &mut m.len_choice2,
                &mut m.len_low,
                &mut m.len_mid,
                &mut m.len_high,
            );
            let distm1 = dec_dist(&mut d, &mut m, len)?;
            let dist = distm1.wrapping_add(1) as usize;
            if dist == 0 || dist > out.len() {
                return Err("lzm1 dist");
            }
            shift_rep(&mut reps, dist as u32);
            state = next_state_match(state);
            for _ in 0..len {
                if out.len() == n {
                    break;
                }
                let b = out[out.len() - dist];
                out.push(b);
            }
        }
    }
    if out.len() != n {
        return Err("lzm1 n");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_repeat() {
        let s = b"the cat sat on the mat. ".repeat(80);
        let e = encode(&s).expect("lzm");
        assert!(e.starts_with(MAGIC));
        assert_eq!(decode(&e).unwrap(), s.as_slice());
        assert!(e.len() < s.len());
    }

    #[test]
    fn roundtrip_ints() {
        let mut s = Vec::new();
        for i in 0u32..400 {
            s.extend_from_slice(&i.to_le_bytes());
        }
        let e = encode(&s).expect("lzm ints");
        assert_eq!(decode(&e).unwrap(), s);
        assert!(e.len() < s.len());
    }
}
