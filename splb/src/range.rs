//! Adaptive binary range coder. 11-bit probs, LZMA-style update.

const PINIT: u16 = 1024;

pub struct Enc {
    low: u128,
    high: u128,
    out: Vec<u8>,
}

impl Enc {
    pub fn new() -> Self {
        Self {
            low: 0,
            high: u128::MAX,
            out: Vec::new(),
        }
    }

    fn renormalize(&mut self) {
        loop {
            let lb = (self.low >> 120) as u8;
            let hb = (self.high >> 120) as u8;
            if lb != hb {
                break;
            }
            self.out.push(lb);
            self.low = (self.low << 8) & u128::MAX;
            self.high = (self.high << 8) | 0xFF;
        }
    }

    pub fn bit(&mut self, bit: u32, p: &mut u16) {
        let span = self.high - self.low;
        let mid = self.low + (span >> 11) * (*p as u128);
        if bit == 0 {
            self.high = mid;
            *p = (*p).saturating_add((2048 - *p) >> 5).min(2047);
        } else {
            self.low = mid + 1;
            *p = (*p - (*p >> 5)).max(1);
        }
        self.renormalize();
    }

    pub fn bits(&mut self, v: u32, n: u32, ppos: &mut [u16]) {
        for i in 0..n {
            let b = (v >> i) & 1;
            let idx = (i as usize).min(ppos.len() - 1);
            self.bit(b, &mut ppos[idx]);
        }
    }

    pub fn byte(&mut self, b: u8, tree: &mut [u16; 256]) {
        self.bits(b as u32, 8, tree);
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.renormalize();
        for _ in 0..16 {
            self.out.push((self.low >> 120) as u8);
            self.low <<= 8;
        }
        self.out
    }
}

pub struct Dec<'a> {
    src: &'a [u8],
    i: usize,
    low: u128,
    high: u128,
    code: u128,
}

impl<'a> Dec<'a> {
    pub fn open(src: &'a [u8]) -> Result<Self, &'static str> {
        if src.len() < 16 {
            return Err("rc short");
        }
        let mut d = Self {
            src,
            i: 0,
            low: 0,
            high: u128::MAX,
            code: 0,
        };
        for _ in 0..16 {
            d.code = (d.code << 8) | d.take() as u128;
        }
        Ok(d)
    }

    fn take(&mut self) -> u8 {
        if self.i < self.src.len() {
            let b = self.src[self.i];
            self.i += 1;
            b
        } else {
            0
        }
    }

    fn renormalize(&mut self) {
        loop {
            let lb = (self.low >> 120) as u8;
            let hb = (self.high >> 120) as u8;
            if lb != hb {
                break;
            }
            self.low = self.low << 8;
            self.high = (self.high << 8) | 0xFF;
            self.code = (self.code << 8) | self.take() as u128;
        }
    }

    pub fn bit(&mut self, p: &mut u16) -> u32 {
        let span = self.high - self.low;
        let mid = self.low + (span >> 11) * (*p as u128);
        let bit = if self.code <= mid {
            self.high = mid;
            *p = (*p).saturating_add((2048 - *p) >> 5).min(2047);
            0
        } else {
            self.low = mid + 1;
            *p = (*p - (*p >> 5)).max(1);
            1
        };
        self.renormalize();
        bit
    }

    pub fn bits(&mut self, n: u32, ppos: &mut [u16]) -> u32 {
        let mut v = 0u32;
        for i in 0..n {
            let idx = (i as usize).min(ppos.len() - 1);
            let b = self.bit(&mut ppos[idx]);
            v |= b << i;
        }
        v
    }

    pub fn byte(&mut self, tree: &mut [u16; 256]) -> u8 {
        self.bits(8, tree) as u8
    }
}

fn init256() -> [u16; 256] {
    [PINIT; 256]
}

const PHI_STATES: usize = 8;
const POS_STATES: usize = 4;
const LIT_MODELS: usize = PHI_STATES * 256 * POS_STATES; // 8192

#[inline]
fn lit_idx(phi: usize, prev: u8, pos: usize) -> usize {
    ((phi & 7) * 256 + prev as usize) * POS_STATES + (pos & (POS_STATES - 1))
}

#[inline]
fn match_ctx(phi: usize, prev: u8, pos: usize, prev_match: bool) -> usize {
    ((phi & 7) * 8
        + (((prev as usize) >> 6) << 1)
        + if prev_match { 4 } else { 0 }
        + (pos & 1))
        & 63
}

#[inline]
fn len_ctx(phi: usize, prev_len: u32) -> usize {
    let cls = if prev_len < 4 {
        0
    } else if prev_len < 8 {
        1
    } else if prev_len < 16 {
        2
    } else {
        3
    };
    (phi & 7) * 4 + cls
}

#[inline]
fn dist_ctx(phi: usize, pos: usize, prev_match: bool) -> usize {
    let _ = prev_match;
    (phi & 7) * 8 + (pos & 7)
}

fn put_tree(e: &mut Enc, v: u32, nbits: u32, tree: &mut [u16]) {
    let mut node = 1usize;
    for i in (0..nbits).rev() {
        let b = (v >> i) & 1;
        let idx = node.min(tree.len() - 1);
        e.bit(b, &mut tree[idx]);
        node = ((node << 1) | b as usize).min(tree.len() - 1);
    }
}

fn get_tree(d: &mut Dec, nbits: u32, tree: &mut [u16]) -> u32 {
    let mut node = 1usize;
    let mut v = 0u32;
    for _ in 0..nbits {
        let idx = node.min(tree.len() - 1);
        let b = d.bit(&mut tree[idx]);
        v = (v << 1) | b;
        node = ((node << 1) | b as usize).min(tree.len() - 1);
    }
    v
}

fn put_dist(
    e: &mut Enc,
    d: u32,
    slot_tree: &mut [u16],
    bit_p: &mut [u16; 32],
    align: &mut [[u16; 16]; 4],
) {
    let d = d.max(1);
    let slot = 32 - d.leading_zeros();
    put_tree(e, slot - 1, 5, slot_tree);
    if slot > 1 {
        let n = slot - 1;
        let ac = ((slot - 1) & 3) as usize;
        for i in 0..n {
            let b = (d >> i) & 1;
            if i < 4 {
                e.bit(b, &mut align[ac][i as usize]);
            } else {
                e.bit(b, &mut bit_p[i as usize]);
            }
        }
    }
}

fn get_dist(
    dec: &mut Dec,
    slot_tree: &mut [u16],
    bit_p: &mut [u16; 32],
    align: &mut [[u16; 16]; 4],
) -> u32 {
    let slot = get_tree(dec, 5, slot_tree) + 1;
    if slot <= 1 {
        return 1;
    }
    let n = slot - 1;
    let ac = ((slot - 1) & 3) as usize;
    let mut low = 0u32;
    for i in 0..n {
        let b = if i < 4 {
            dec.bit(&mut align[ac][i as usize])
        } else {
            dec.bit(&mut bit_p[i as usize])
        };
        low |= b << i;
    }
    (1u32 << n) | (low & ((1u32 << n) - 1))
}

fn put_len(e: &mut Enc, extra: u32, p0: &mut u16, p1: &mut u16, p3: &mut [u16], p4: &mut [u16], p11: &mut [u16]) {
    if extra < 8 {
        e.bit(0, p0);
        put_tree(e, extra, 3, p3);
    } else if extra < 16 {
        e.bit(1, p0);
        e.bit(0, p1);
        put_tree(e, extra - 8, 3, p3);
    } else {
        e.bit(1, p0);
        e.bit(1, p1);
        let x = extra - 16;
        if x < 15 {
            put_tree(e, x, 4, p4);
        } else {
            put_tree(e, 15, 4, p4);
            put_tree(e, (x - 15).min(65535), 16, p11);
        }
    }
}

fn get_len(d: &mut Dec, p0: &mut u16, p1: &mut u16, p3: &mut [u16], p4: &mut [u16], p11: &mut [u16]) -> u32 {
    if d.bit(p0) == 0 {
        get_tree(d, 3, p3)
    } else if d.bit(p1) == 0 {
        8 + get_tree(d, 3, p3)
    } else {
        let x = get_tree(d, 4, p4);
        if x < 15 {
            16 + x
        } else {
            16 + 15 + get_tree(d, 16, p11)
        }
    }
}

/// Phi metastable label from the last emitted event. Encoder and decoder
/// step the same function so context stays locked.
#[inline]
pub(crate) fn phi_step(phi: usize, is_match: bool, is_rep: bool, dist: u32, len: u32) -> usize {
    if !is_match {
        return if phi == 0 { 1 } else { 0 };
    }
    if is_rep {
        return 2;
    }
    if len >= 32 {
        return 6;
    }
    if dist < 256 {
        3
    } else if dist < 65536 {
        4
    } else if dist > 16 * 1024 * 1024 {
        5
    } else {
        7
    }
}

fn bump_reps(reps: &mut [u32; 4], d: u32) {
    if d == 0 || d == reps[0] {
        return;
    }
    if d == reps[1] {
        reps[1] = reps[0];
        reps[0] = d;
        return;
    }
    if d == reps[2] {
        reps[2] = reps[1];
        reps[1] = reps[0];
        reps[0] = d;
        return;
    }
    reps[3] = reps[2];
    reps[2] = reps[1];
    reps[1] = reps[0];
    reps[0] = d;
}

fn p_update(p: &mut u16, bit: u32) {
    if bit == 0 {
        *p = (*p).saturating_add((2048 - *p) >> 5).min(2047);
    } else {
        *p = (*p - (*p >> 5)).max(1);
    }
}

#[inline]
pub(crate) fn bit_price(p: u16, bit: u32) -> u32 {
    let c = if bit == 0 { 2048 - p as u32 } else { p as u32 };
    c * 32
}

fn tree_price(tree: &[u16], v: u32, nbits: u32) -> u32 {
    let mut node = 1usize;
    let mut c = 0u32;
    for i in (0..nbits).rev() {
        let b = (v >> i) & 1;
        let idx = node.min(tree.len() - 1);
        c += bit_price(tree[idx], b);
        node = ((node << 1) | b as usize).min(tree.len() - 1);
    }
    c
}

/// Frozen-table prices used by block DP. Updated from the previous block.
pub struct PriceBook {
    p_match: [u16; 64],
    p_rep: [u16; 8],
    p_which: [u16; 8],
    p_len0: [u16; 32],
    p_len1: [u16; 32],
    p_len3: [[u16; 8]; 32],
    p_len4: [[u16; 16]; 32],
    p_len11: [[u16; 16]; 32],
    dist_slot: [u16; 64],
    dist_align: [[u16; 16]; 4],
}

impl PriceBook {
    pub fn new() -> Self {
        Self {
            p_match: [PINIT; 64],
            p_rep: [PINIT; 8],
            p_which: [PINIT; 8],
            p_len0: [PINIT; 32],
            p_len1: [PINIT; 32],
            p_len3: [[PINIT; 8]; 32],
            p_len4: [[PINIT; 16]; 32],
            p_len11: [[PINIT; 16]; 32],
            dist_slot: [PINIT; 64],
            dist_align: [[PINIT; 16]; 4],
        }
    }

    pub fn match_cost(&self, phi: usize, prev: u8, pos: usize, prev_match: bool, is_match: bool) -> u32 {
        bit_price(self.p_match[match_ctx(phi, prev, pos, prev_match)], if is_match { 1 } else { 0 })
    }

    pub fn lit_cost(&self, after_match: bool, _phi: usize, _prev: u8, _pos: usize, _b: u8) -> u32 {
        let _ = after_match;
        // 8 bits at PINIT-scale; bank split is in the encoder, price is same order.
        8 * bit_price(PINIT, 0)
    }

    pub fn len_cost(&self, phi: usize, prev_len: u32, len: u32) -> u32 {
        use crate::parse::MIN_MATCH;
        let extra = len.saturating_sub(MIN_MATCH as u32);
        let lc = len_ctx(phi, prev_len);
        if extra < 8 {
            bit_price(self.p_len0[lc], 0) + tree_price(&self.p_len3[lc], extra, 3)
        } else if extra < 16 {
            bit_price(self.p_len0[lc], 1)
                + bit_price(self.p_len1[lc], 0)
                + tree_price(&self.p_len3[lc], extra - 8, 3)
        } else {
            let x = extra - 16;
            let mut c = bit_price(self.p_len0[lc], 1) + bit_price(self.p_len1[lc], 1);
            if x < 15 {
                c += tree_price(&self.p_len4[lc], x, 4);
            } else {
                c += tree_price(&self.p_len4[lc], 15, 4);
                c += tree_price(&self.p_len11[lc], (x - 15).min(65535), 16);
            }
            c
        }
    }

    pub fn dist_cost(&self, phi: usize, pos: usize, prev_match: bool, dist: u32) -> u32 {
        let d = dist.max(1);
        let slot = 32 - d.leading_zeros();
        let _ = (phi, pos, prev_match);
        let mut c = tree_price(&self.dist_slot, slot - 1, 5);
        if slot > 1 {
            let n = slot - 1;
            let ac = ((slot - 1) & 3) as usize;
            for i in 0..n {
                let b = (d >> i) & 1;
                if i < 4 {
                    c += bit_price(self.dist_align[ac][i as usize], b);
                } else {
                    c += bit_price(PINIT, b);
                }
            }
        }
        c
    }

    pub fn rep_cost(&self, phi: usize, which: u32) -> u32 {
        bit_price(self.p_rep[phi & 7], 1) + bit_price(self.p_which[0], (which >> 1) & 1)
            + bit_price(self.p_which[1], which & 1)
    }

    pub fn new_dist_flag(&self, phi: usize) -> u32 {
        bit_price(self.p_rep[phi & 7], 0)
    }

    pub fn feed_match_bit(&mut self, phi: usize, prev: u8, pos: usize, prev_match: bool, is_match: bool) {
        let i = match_ctx(phi, prev, pos, prev_match);
        p_update(&mut self.p_match[i], if is_match { 1 } else { 0 });
    }
}

/// 4 KiB frozen snapshot for DP. Does not own lit banks.
pub struct TinyBook {
    pub match_ctx: [u16; 64],
    pub p_rep: [u16; 8],
    pub p_len0: [u16; 32],
    pub dist_slot: [u16; 64],
    pub lit_avg: [u16; 2],
}

impl TinyBook {
    pub fn new() -> Self {
        Self {
            match_ctx: [PINIT; 64],
            p_rep: [PINIT; 8],
            p_len0: [PINIT; 32],
            dist_slot: [PINIT; 64],
            lit_avg: [PINIT, PINIT],
        }
    }

    #[inline]
    pub fn lit_cost(&self, after_match: bool) -> u32 {
        8 * bit_price(self.lit_avg[after_match as usize], 0)
    }

    #[inline]
    pub fn match_cost(&self, phi: usize, prev: u8, pos: usize, prev_match: bool, is_match: bool) -> u32 {
        bit_price(self.match_ctx[match_ctx(phi, prev, pos, prev_match)], if is_match { 1 } else { 0 })
    }

    #[inline]
    pub fn len_cost(&self, phi: usize, prev_len: u32, len: u32) -> u32 {
        use crate::parse::MIN_MATCH;
        let extra = len.saturating_sub(MIN_MATCH as u32);
        let lc = len_ctx(phi, prev_len);
        let p0 = self.p_len0[lc];
        if extra < 8 {
            bit_price(p0, 0) + extra * 1024 * 32
        } else if extra < 16 {
            bit_price(p0, 1) + 1024 * 32 + (extra - 8) * 1024 * 32
        } else {
            bit_price(p0, 1) + 2 * 1024 * 32 + 16 * 1024 * 32
        }
    }

    #[inline]
    pub fn dist_cost(&self, dist: u32) -> u32 {
        let d = dist.max(1);
        let slot = 32 - d.leading_zeros();
        tree_price(&self.dist_slot, slot - 1, 5) + slot.saturating_sub(1) * 1024 * 32
    }

    #[inline]
    pub fn rep_cost(&self, phi: usize) -> u32 {
        bit_price(self.p_rep[phi & 7], 1) + 2 * 1024 * 32
    }

    #[inline]
    pub fn new_dist_flag(&self, phi: usize) -> u32 {
        bit_price(self.p_rep[phi & 7], 0)
    }

    pub fn feed(&mut self, is_match: bool, is_rep: bool, phi: usize, prev: u8, pos: usize, prev_match: bool, dist: u32, len: u32) {
        let mi = match_ctx(phi, prev, pos, prev_match);
        p_update(&mut self.match_ctx[mi], if is_match { 1 } else { 0 });
        if is_match {
            p_update(&mut self.p_rep[phi & 7], if is_rep { 1 } else { 0 });
            let extra = len.saturating_sub(crate::parse::MIN_MATCH as u32);
            let lc = len_ctx(phi, len);
            p_update(&mut self.p_len0[lc], if extra < 8 { 0 } else { 1 });
            if !is_rep {
                let slot = 32 - dist.max(1).leading_zeros();
                // touch root of slot tree
                p_update(&mut self.dist_slot[1], if slot > 16 { 1 } else { 0 });
            }
        }
    }
}

pub fn get_final_tiny_snapshot() {
    lbr1_price::price_reset_c();
}

pub fn encode_toks(toks: &[crate::parse::Tok], raw: &[u8]) -> Vec<u8> {
    use crate::parse::{Tok, MIN_MATCH};
    let mut e = Enc::new();
    let mut p_match = [PINIT; 64];
    let mut p_rep = [PINIT; 8];
    let mut p_which = [PINIT; 8];
    let mut p_len0 = [PINIT; 32];
    let mut p_len1 = [PINIT; 32];
    let mut p_len3 = [[PINIT; 8]; 32];
    let mut p_len4 = [[PINIT; 16]; 32];
    let mut p_len11 = [[PINIT; 16]; 32];
    let mut lit_after_lit = vec![init256(); LIT_MODELS];
    let mut lit_after_match = vec![init256(); LIT_MODELS];
    let mut dist_slot = [[PINIT; 64]; 64];
    let mut dist_bits = [PINIT; 32];
    let mut dist_align = [[PINIT; 16]; 4];
    let mut phi = 0usize;
    let mut reps = [0u32; 4];
    let mut pos = 0usize;
    let mut prev = 0u8;
    let mut prev_len = 0u32;
    let mut prev_match = false;
    for t in toks {
        match *t {
            Tok::Lit(b) => {
                e.bit(0, &mut p_match[match_ctx(phi, prev, pos, prev_match)]);
                let li = lit_idx(phi, prev, pos);
                if prev_match {
                    e.byte(b, &mut lit_after_match[li]);
                } else {
                    e.byte(b, &mut lit_after_lit[li]);
                }
                prev = b;
                pos += 1;
                prev_match = false;
                phi = phi_step(phi, false, false, 0, 0);
            }
            Tok::Match { dist, len } => {
                e.bit(1, &mut p_match[match_ctx(phi, prev, pos, prev_match)]);
                let extra = len.saturating_sub(MIN_MATCH as u32);
                let lc = len_ctx(phi, prev_len);
                put_len(
                    &mut e,
                    extra,
                    &mut p_len0[lc],
                    &mut p_len1[lc],
                    &mut p_len3[lc],
                    &mut p_len4[lc],
                    &mut p_len11[lc],
                );
                let d = if dist == 0 { reps[0] } else { dist };
                let mut which = 4u32;
                for i in 0..4 {
                    if reps[i] != 0 && reps[i] == d {
                        which = i as u32;
                        break;
                    }
                }
                if which < 4 {
                    e.bit(1, &mut p_rep[phi]);
                    e.bits(which, 2, &mut p_which);
                } else {
                    e.bit(0, &mut p_rep[phi]);
                    let nd = if d == 0 { 1 } else { d };
                    let dc = dist_ctx(phi, pos, prev_match);
                    put_dist(
                        &mut e,
                        nd,
                        &mut dist_slot[dc],
                        &mut dist_bits,
                        &mut dist_align,
                    );
                    bump_reps(&mut reps, nd);
                }
                if which < 4 {
                    bump_reps(&mut reps, d);
                }
                pos += len as usize;
                if pos > 0 && pos <= raw.len() {
                    prev = raw[pos - 1];
                }
                prev_len = len;
                prev_match = true;
                phi = phi_step(phi, true, which < 4, d, len);
            }
        }
    }
    e.finish()
}

pub fn decode_toks(buf: &[u8], orig: usize) -> Result<Vec<u8>, &'static str> {
    use crate::parse::MIN_MATCH;
    let mut d = Dec::open(buf)?;
    let mut p_match = [PINIT; 64];
    let mut p_rep = [PINIT; 8];
    let mut p_which = [PINIT; 8];
    let mut p_len0 = [PINIT; 32];
    let mut p_len1 = [PINIT; 32];
    let mut p_len3 = [[PINIT; 8]; 32];
    let mut p_len4 = [[PINIT; 16]; 32];
    let mut p_len11 = [[PINIT; 16]; 32];
    let mut lit_after_lit = vec![init256(); LIT_MODELS];
    let mut lit_after_match = vec![init256(); LIT_MODELS];
    let mut dist_slot = [[PINIT; 64]; 64];
    let mut dist_bits = [PINIT; 32];
    let mut dist_align = [[PINIT; 16]; 4];
    let mut phi = 0usize;
    let mut reps = [0u32; 4];
    let mut out = Vec::with_capacity(orig);
    let mut prev = 0u8;
    let mut prev_len = 0u32;
    let mut prev_match = false;
    while out.len() < orig {
        let m = d.bit(&mut p_match[match_ctx(phi, prev, out.len(), prev_match)]);
        if m == 0 {
            let li = lit_idx(phi, prev, out.len());
            let b = if prev_match {
                d.byte(&mut lit_after_match[li])
            } else {
                d.byte(&mut lit_after_lit[li])
            };
            out.push(b);
            prev = b;
            prev_match = false;
            phi = phi_step(phi, false, false, 0, 0);
        } else {
            let lc = len_ctx(phi, prev_len);
            let extra = get_len(
                &mut d,
                &mut p_len0[lc],
                &mut p_len1[lc],
                &mut p_len3[lc],
                &mut p_len4[lc],
                &mut p_len11[lc],
            );
            let nlen = extra as usize + MIN_MATCH;
            let is_rep = d.bit(&mut p_rep[phi]) == 1;
            let dist = if is_rep {
                let which = d.bits(2, &mut p_which) as usize;
                if which > 3 || reps[which] == 0 {
                    return Err("rc rep");
                }
                let got = reps[which];
                bump_reps(&mut reps, got);
                got
            } else {
                let dc = dist_ctx(phi, out.len(), prev_match);
                let got = get_dist(&mut d, &mut dist_slot[dc], &mut dist_bits, &mut dist_align);
                if got == 0 {
                    return Err("rc d0");
                }
                bump_reps(&mut reps, got);
                got
            };
            let dd = dist as usize;
            if dd == 0 || dd > out.len() {
                return Err("rc dist");
            }
            if out.len() + nlen > orig {
                return Err("rc ov");
            }
            for _ in 0..nlen {
                let b = out[out.len() - dd];
                out.push(b);
            }
            prev = *out.last().unwrap();
            prev_len = nlen as u32;
            prev_match = true;
            phi = phi_step(phi, true, is_rep, dist, nlen as u32);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{parse_lazy, DEFAULT_WINDOW};

    #[test]
    fn rc_roundtrip_motif() {
        let mut s = Vec::new();
        let m = b"QWERTYUIOPASDFGH";
        for _ in 0..80 {
            s.extend_from_slice(m);
            s.extend_from_slice(&[0u8; 8]);
        }
        let t = parse_lazy(&s, DEFAULT_WINDOW);
        let blob = encode_toks(&t, &s);
        let back = decode_toks(&blob, s.len()).expect("rc");
        assert_eq!(back, s);
    }

    #[test]
    fn rc_rep4_roundtrip() {
        let mut s = Vec::new();
        let block = b"ABCD1234EFGH5678";
        for _ in 0..200 {
            s.extend_from_slice(block);
        }
        // second copy of first half far enough to reuse offsets
        let head = s[..800].to_vec();
        s.extend_from_slice(&head);
        let t = parse_lazy(&s, DEFAULT_WINDOW);
        let blob = encode_toks(&t, &s);
        let back = decode_toks(&blob, s.len()).expect("rc4");
        assert_eq!(back, s);
    }

    #[test]
    fn rc_bits_identity() {
        let mut e = Enc::new();
        let mut p = [PINIT; 32];
        e.bits(0x5A, 8, &mut p);
        let blob = e.finish();
        let mut d = Dec::open(&blob).unwrap();
        let mut p2 = [PINIT; 32];
        assert_eq!(d.bits(8, &mut p2), 0x5A);
    }

    #[test]
    fn rc_far_dist_50M() {
        let d = 50_000_000u32;
        let mut e = Enc::new();
        let mut st = [PINIT; 128];
        let mut bits = [PINIT; 32];
        let mut al = [[PINIT; 16]; 4];
        put_dist(&mut e, d, &mut st, &mut bits, &mut al);
        let blob = e.finish();
        let mut dec = Dec::open(&blob).unwrap();
        let mut st2 = [PINIT; 128];
        let mut bits2 = [PINIT; 32];
        let mut al2 = [[PINIT; 16]; 4];
        let got = get_dist(&mut dec, &mut st2, &mut bits2, &mut al2);
        assert_eq!(got, d);
    }
}
