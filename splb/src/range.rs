//! Adaptive binary range coder. 11-bit probs, LZMA-style update.

const PINIT: u16 = 1024;

pub struct Enc {
    low: u128,
    high: u128,
    out: Vec<u8>,
}

impl Enc {
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            low: 0,
            high: u128::MAX,
            out: Vec::with_capacity(cap),
        }
    }

    #[inline(always)]
    fn renormalize(&mut self) {
        loop {
            let lb = (self.low >> 120) as u8;
            let hb = (self.high >> 120) as u8;
            if lb != hb {
                break;
            }
            self.out.push(lb);
            self.low <<= 8;
            self.high = (self.high << 8) | 0xFF;
        }
    }

    #[inline(always)]
    pub fn bit(&mut self, bit: u32, p: &mut u16) {
        self.bit_p(bit, *p);
        if bit == 0 {
            let x = *p;
            *p = (x + ((2048 - x) >> 5)).min(2047);
        } else {
            let x = *p;
            *p = (x - (x >> 5)).max(1);
        }
    }

    /// Mixer-supplied p. Does not adapt p.
    #[inline(always)]
    pub fn bit_p(&mut self, bit: u32, p: u16) {
        // p kept in [1,2047] by bit()/PINIT; skip clamp on hot path (bit-exact).
        let span = self.high - self.low;
        let mid = self.low + (span >> 11) * (p as u128);
        if bit == 0 {
            self.high = mid;
        } else {
            self.low = mid + 1;
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
        let bit = self.bit_p(*p);
        if bit == 0 {
            *p = (*p).saturating_add((2048 - *p) >> 5).min(2047);
        } else {
            *p = (*p - (*p >> 5)).max(1);
        }
        bit
    }

    pub fn bit_p(&mut self, p: u16) -> u32 {
        let p = p.clamp(1, 2047);
        let span = self.high - self.low;
        let mid = self.low + (span >> 11) * (p as u128);
        let bit = if self.code <= mid {
            self.high = mid;
            0
        } else {
            self.low = mid + 1;
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


/// LZMA-style 32-bit range encoder (carry-cache ShiftLow). Own wire —
/// not bit-exact with `Enc` (u128 interval). Used by VER_ML4F.
const RC32_TOP: u32 = 1 << 24;
std::thread_local! {
    static RC32_BITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
pub fn rc32_bits_reset() { RC32_BITS.with(|c| c.set(0)); }
pub fn rc32_bits_get() -> u64 { RC32_BITS.with(|c| c.get()) }


pub struct Enc32 {
    low: u64,
    range: u32,
    cache: u8,
    cache_size: u64,
    out: Vec<u8>,
}

impl Enc32 {
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            low: 0,
            range: 0xFFFF_FFFF,
            cache: 0,
            cache_size: 1,
            out: Vec::with_capacity(cap),
        }
    }

    #[inline(always)]
    fn shift_low(&mut self) {
        let high = (self.low >> 32) as u32;
        if (self.low as u32) < 0xFF00_0000 || high != 0 {
            let mut temp = self.cache as u32;
            loop {
                self.out.push(temp.wrapping_add(high) as u8);
                temp = 0xFF;
                self.cache_size -= 1;
                if self.cache_size == 0 {
                    break;
                }
            }
            self.cache = (self.low >> 24) as u8;
        }
        self.cache_size += 1;
        self.low = (self.low & 0xFF_FFFF) << 8;
    }

    #[inline(always)]
    pub fn bit(&mut self, bit: u32, p: &mut u16) {
        let bound = (self.range >> 11) * (*p as u32);
        if bit == 0 {
            self.range = bound;
            let x = *p;
            *p = (x + ((2048 - x) >> 5)).min(2047);
        } else {
            self.low += bound as u64;
            self.range -= bound;
            let x = *p;
            *p = (x - (x >> 5)).max(1);
        }
        while self.range < RC32_TOP {
            self.range <<= 8;
            self.shift_low();
        }
    }

    #[inline(always)]
    pub fn bits(&mut self, v: u32, n: u32, ppos: &mut [u16]) {
        for i in 0..n {
            let b = (v >> i) & 1;
            let idx = (i as usize).min(ppos.len() - 1);
            self.bit(b, &mut ppos[idx]);
        }
    }

    #[inline(always)]
    pub fn byte(&mut self, b: u8, tree: &mut [u16; 256]) {
        self.bits(b as u32, 8, tree);
    }

    pub fn finish(mut self) -> Vec<u8> {
        for _ in 0..5 {
            self.shift_low();
        }
        self.out
    }
}

pub struct Dec32<'a> {
    src: &'a [u8],
    i: usize,
    code: u32,
    range: u32,
}

impl<'a> Dec32<'a> {
    pub fn open(src: &'a [u8]) -> Result<Self, &'static str> {
        if src.len() < 5 {
            return Err("rc32 short");
        }
        let mut d = Self {
            src,
            i: 0,
            code: 0,
            range: 0xFFFF_FFFF,
        };
        for _ in 0..5 {
            d.code = (d.code << 8) | d.take() as u32;
        }
        Ok(d)
    }

    #[inline(always)]
    fn take(&mut self) -> u8 {
        if self.i < self.src.len() {
            let b = self.src[self.i];
            self.i += 1;
            b
        } else {
            0
        }
    }

    #[inline(always)]
    pub fn bit(&mut self, p: &mut u16) -> u32 {
        let bound = (self.range >> 11) * (*p as u32);
        let bit = if self.code < bound {
            self.range = bound;
            let x = *p;
            *p = (x + ((2048 - x) >> 5)).min(2047);
            0
        } else {
            self.code -= bound;
            self.range -= bound;
            let x = *p;
            *p = (x - (x >> 5)).max(1);
            1
        };
        while self.range < RC32_TOP {
            self.code = (self.code << 8) | self.take() as u32;
            self.range <<= 8;
        }
        bit
    }

    #[inline(always)]
    pub fn bits(&mut self, n: u32, ppos: &mut [u16]) -> u32 {
        let mut v = 0u32;
        for i in 0..n {
            let idx = (i as usize).min(ppos.len() - 1);
            let b = self.bit(&mut ppos[idx]);
            v |= b << i;
        }
        v
    }

    #[inline(always)]
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


trait BitOut {
    fn bit(&mut self, bit: u32, p: &mut u16);
    fn bits(&mut self, v: u32, n: u32, ppos: &mut [u16]);
    fn byte(&mut self, b: u8, tree: &mut [u16; 256]);
}

impl BitOut for Enc {
    #[inline(always)]
    fn bit(&mut self, bit: u32, p: &mut u16) {
        Enc::bit(self, bit, p)
    }
    #[inline(always)]
    fn bits(&mut self, v: u32, n: u32, ppos: &mut [u16]) {
        Enc::bits(self, v, n, ppos)
    }
    #[inline(always)]
    fn byte(&mut self, b: u8, tree: &mut [u16; 256]) {
        Enc::byte(self, b, tree)
    }
}

impl BitOut for Enc32 {
    #[inline(always)]
    fn bit(&mut self, bit: u32, p: &mut u16) {
        Enc32::bit(self, bit, p)
    }
    #[inline(always)]
    fn bits(&mut self, v: u32, n: u32, ppos: &mut [u16]) {
        Enc32::bits(self, v, n, ppos)
    }
    #[inline(always)]
    fn byte(&mut self, b: u8, tree: &mut [u16; 256]) {
        Enc32::byte(self, b, tree)
    }
}

trait BitIn {
    fn bit(&mut self, p: &mut u16) -> u32;
    fn bits(&mut self, n: u32, ppos: &mut [u16]) -> u32;
    fn byte(&mut self, tree: &mut [u16; 256]) -> u8;
}

impl BitIn for Dec<'_> {
    #[inline(always)]
    fn bit(&mut self, p: &mut u16) -> u32 {
        Dec::bit(self, p)
    }
    #[inline(always)]
    fn bits(&mut self, n: u32, ppos: &mut [u16]) -> u32 {
        Dec::bits(self, n, ppos)
    }
    #[inline(always)]
    fn byte(&mut self, tree: &mut [u16; 256]) -> u8 {
        Dec::byte(self, tree)
    }
}

impl BitIn for Dec32<'_> {
    #[inline(always)]
    fn bit(&mut self, p: &mut u16) -> u32 {
        Dec32::bit(self, p)
    }
    #[inline(always)]
    fn bits(&mut self, n: u32, ppos: &mut [u16]) -> u32 {
        Dec32::bits(self, n, ppos)
    }
    #[inline(always)]
    fn byte(&mut self, tree: &mut [u16; 256]) -> u8 {
        Dec32::byte(self, tree)
    }
}

#[inline(always)]
fn put_tree<E: BitOut>(e: &mut E, v: u32, nbits: u32, tree: &mut [u16]) {
    let mut node = 1usize;
    for i in (0..nbits).rev() {
        let b = (v >> i) & 1;
        let idx = node.min(tree.len() - 1);
        e.bit(b, &mut tree[idx]);
        node = ((node << 1) | b as usize).min(tree.len() - 1);
    }
}

fn get_tree<D: BitIn>(d: &mut D, nbits: u32, tree: &mut [u16]) -> u32 {
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

#[inline(always)]
fn put_dist<E: BitOut>(
    e: &mut E,
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

fn get_dist<D: BitIn>(
    dec: &mut D,
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

/// LZMA-style matched literal: each bit is priced against the match-byte
/// until they diverge. Own range coder, not xz.
const MLIT: usize = 768;

#[inline(always)]
fn put_mlit<E: BitOut>(e: &mut E, b: u8, match_byte: u8, probs: &mut [u16; MLIT]) {
    let mut ctx = 1usize;
    let mut same = true;
    for i in (0..8).rev() {
        let bit = ((b >> i) & 1) as u32;
        if same {
            let mb = ((match_byte >> i) & 1) as u32;
            let idx = 0x100 + ((mb as usize) << 8) + ctx;
            e.bit(bit, &mut probs[idx]);
            ctx = (ctx << 1) | bit as usize;
            if bit != mb {
                same = false;
            }
        } else {
            let idx = if ctx < 256 { ctx } else { 255 };
            e.bit(bit, &mut probs[idx]);
            ctx = (ctx << 1) | bit as usize;
        }
    }
}

#[inline(always)]
fn put_mlit_fast(e: &mut Enc32, b: u8, match_byte: u8, probs: &mut [u16; MLIT]) {
    let mut ctx = 1usize;
    let mut same = true;
    for i in (0..8).rev() {
        let bit = ((b >> i) & 1) as u32;
        if same {
            let mb = ((match_byte >> i) & 1) as u32;
            let idx = 0x100 + ((mb as usize) << 8) + ctx;
            e.bit(bit, unsafe { probs.get_unchecked_mut(idx) });
            ctx = (ctx << 1) | bit as usize;
            if bit != mb {
                same = false;
            }
        } else {
            let idx = if ctx < 256 { ctx } else { 255 };
            e.bit(bit, unsafe { probs.get_unchecked_mut(idx) });
            ctx = (ctx << 1) | bit as usize;
        }
    }
}

fn get_mlit<D: BitIn>(d: &mut D, match_byte: u8, probs: &mut [u16; MLIT]) -> u8 {
    let mut ctx = 1usize;
    let mut same = true;
    let mut v = 0u8;
    for i in (0..8).rev() {
        let bit = if same {
            let mb = ((match_byte >> i) & 1) as u32;
            let idx = (0x100 + ((mb as usize) << 8) + ctx).min(MLIT - 1);
            let bit = d.bit(&mut probs[idx]);
            ctx = (ctx << 1) | bit as usize;
            if bit != mb {
                same = false;
            }
            bit
        } else {
            let idx = ctx.min(255);
            let bit = d.bit(&mut probs[idx]);
            ctx = (ctx << 1) | bit as usize;
            bit
        };
        v |= (bit as u8) << i;
    }
    v
}

fn mlit_idx(prev: u8, pos: usize) -> usize {
    (prev as usize) * POS_STATES + (pos & (POS_STATES - 1))
}

fn mlit_wide_idx(phi: usize, prev: u8, pos: usize) -> usize {
    lit_idx(phi, prev, pos)
}

#[inline(always)]
fn put_len<E: BitOut>(e: &mut E, extra: u32, p0: &mut u16, p1: &mut u16, p3: &mut [u16], p4: &mut [u16], p11: &mut [u16]) {
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

fn get_len<D: BitIn>(d: &mut D, p0: &mut u16, p1: &mut u16, p3: &mut [u16], p4: &mut [u16], p11: &mut [u16]) -> u32 {
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

#[inline(always)]
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
    encode_toks_ex(toks, raw, false, false, false)
}

pub fn encode_toks_mlit(toks: &[crate::parse::Tok], raw: &[u8]) -> Vec<u8> {
    encode_toks_ex(toks, raw, true, false, false)
}

/// Three lit banks (after-lit / after-match / after-rep), 8192 models each.
pub fn encode_toks_mlit3(toks: &[crate::parse::Tok], raw: &[u8]) -> Vec<u8> {
    encode_toks_ex(toks, raw, true, true, false)
}

/// 8192 match-byte models after new-match, 8192 after-rep. Own pathway.
pub fn encode_toks_mlit4(toks: &[crate::parse::Tok], raw: &[u8]) -> Vec<u8> {
    encode_toks_ex(toks, raw, true, true, true)
}

/// VER_ML4F: ML4 models (8192 matched-lit banks) + LZMA-style Enc32 range coder.
/// Theory-ok: same contexts as VER_ML4; u32 RC is not bit-exact with u128 VER_ML4.
pub fn encode_toks_mlit4f(toks: &[crate::parse::Tok], raw: &[u8]) -> Vec<u8> {
    encode_toks_mlit4f_inner(toks, raw)
}

fn encode_toks_ex(
    toks: &[crate::parse::Tok],
    raw: &[u8],
    matched_lits: bool,
    rep_bank: bool,
    wide: bool,
) -> Vec<u8> {
    use crate::parse::{Tok, MIN_MATCH};
    let mut e = Enc::with_capacity(raw.len() / 2 + 64);
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
    let mut lit_after_rep = if rep_bank && !wide {
        vec![init256(); LIT_MODELS]
    } else {
        Vec::new()
    };
    let nmlit = if !matched_lits {
        0
    } else if wide {
        LIT_MODELS
    } else {
        256 * POS_STATES
    };
    let mut mlit = vec![[PINIT; MLIT]; nmlit];
    let mut mlit_rep = if wide {
        vec![[PINIT; MLIT]; LIT_MODELS]
    } else {
        Vec::new()
    };
    let mut dist_slot = [[PINIT; 64]; 64];
    let mut dist_bits = [PINIT; 32];
    let mut dist_align = [[PINIT; 16]; 4];
    let mut phi = 0usize;
    let mut reps = [0u32; 4];
    let mut pos = 0usize;
    let mut prev = 0u8;
    let mut prev_len = 0u32;
    let mut prev_match = false;
    let mut prev_rep = false;
    for t in toks {
        match *t {
            Tok::Lit(b) => {
                e.bit(0, &mut p_match[match_ctx(phi, prev, pos, prev_match)]);
                let have_mb = prev_match
                    && reps[0] > 0
                    && (reps[0] as usize) <= pos
                    && pos <= raw.len();
                if matched_lits && have_mb && (wide || !prev_rep) {
                    let mb = raw[pos - reps[0] as usize];
                    let slot = if wide {
                        mlit_wide_idx(phi, prev, pos)
                    } else {
                        mlit_idx(prev, pos)
                    };
                    if wide && prev_rep {
                        put_mlit(&mut e, b, mb, &mut mlit_rep[slot]);
                    } else {
                        put_mlit(&mut e, b, mb, &mut mlit[slot]);
                    }
                } else {
                    let li = lit_idx(phi, prev, pos);
                    if prev_match && prev_rep && rep_bank && !wide {
                        e.byte(b, &mut lit_after_rep[li]);
                    } else if prev_match {
                        e.byte(b, &mut lit_after_match[li]);
                    } else {
                        e.byte(b, &mut lit_after_lit[li]);
                    }
                }
                prev = b;
                pos += 1;
                prev_match = false;
                prev_rep = false;
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
                prev_rep = which < 4;
                phi = phi_step(phi, true, which < 4, d, len);
            }
        }
    }
    e.finish()
}


/// Specialized VER_ML4F encoder: wide matched-lits, no dynamic bank flags.
fn encode_toks_mlit4f_inner(toks: &[crate::parse::Tok], raw: &[u8]) -> Vec<u8> {
    use crate::parse::{Tok, MIN_MATCH};
    // Full ML4-width banks (8192). Enc32 changes wire vs VER_ML4; contexts match ML4.
    const FMODELS: usize = LIT_MODELS;
    #[inline(always)]
    fn fidx(phi: usize, prev: u8, pos: usize) -> usize {
        lit_idx(phi, prev, pos)
    }
    let mut e = Enc32::with_capacity(raw.len() / 2 + 64);
    let mut p_match = [PINIT; 64];
    let mut p_rep = [PINIT; 8];
    let mut p_which = [PINIT; 8];
    let mut p_len0 = [PINIT; 32];
    let mut p_len1 = [PINIT; 32];
    let mut p_len3 = [[PINIT; 8]; 32];
    let mut p_len4 = [[PINIT; 16]; 32];
    let mut p_len11 = [[PINIT; 16]; 32];
    let mut lit_after_lit = vec![init256(); FMODELS];
    let mut lit_after_match = vec![init256(); FMODELS];
    let mut mlit = vec![[PINIT; MLIT]; FMODELS];
    let mut mlit_rep = vec![[PINIT; MLIT]; FMODELS];
    let mut dist_slot = [[PINIT; 64]; 64];
    let mut dist_bits = [PINIT; 32];
    let mut dist_align = [[PINIT; 16]; 4];
    let mut phi = 0usize;
    let mut reps = [0u32; 4];
    let mut pos = 0usize;
    let mut prev = 0u8;
    let mut prev_len = 0u32;
    let mut prev_match = false;
    let mut prev_rep = false;
    let raw_len = raw.len();
    for t in toks {
        match *t {
            Tok::Lit(b) => {
                e.bit(0, &mut p_match[match_ctx(phi, prev, pos, prev_match)]);
                let have_mb = prev_match && reps[0] > 0 && (reps[0] as usize) <= pos && pos <= raw_len;
                if have_mb {
                    let mb = unsafe { *raw.get_unchecked(pos - reps[0] as usize) };
                    let slot = fidx(phi, prev, pos);
                    if prev_rep {
                        put_mlit_fast(&mut e, b, mb, &mut mlit_rep[slot]);
                    } else {
                        put_mlit_fast(&mut e, b, mb, &mut mlit[slot]);
                    }
                } else {
                    let li = fidx(phi, prev, pos);
                    if prev_match {
                        e.byte(b, &mut lit_after_match[li]);
                    } else {
                        e.byte(b, &mut lit_after_lit[li]);
                    }
                }
                prev = b;
                pos += 1;
                prev_match = false;
                prev_rep = false;
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
                    bump_reps(&mut reps, d);
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
                pos += len as usize;
                if pos > 0 && pos <= raw_len {
                    prev = unsafe { *raw.get_unchecked(pos - 1) };
                }
                prev_len = len;
                prev_match = true;
                prev_rep = which < 4;
                phi = phi_step(phi, true, which < 4, d, len);
            }
        }
    }
    e.finish()
}


fn encode_toks_ex_rc32(
    toks: &[crate::parse::Tok],
    raw: &[u8],
    matched_lits: bool,
    rep_bank: bool,
    wide: bool,
) -> Vec<u8> {
    use crate::parse::{Tok, MIN_MATCH};
    let mut e = Enc32::with_capacity(raw.len() / 2 + 64);
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
    let mut lit_after_rep = if rep_bank && !wide {
        vec![init256(); LIT_MODELS]
    } else {
        Vec::new()
    };
    let nmlit = if !matched_lits {
        0
    } else if wide {
        LIT_MODELS
    } else {
        256 * POS_STATES
    };
    let mut mlit = vec![[PINIT; MLIT]; nmlit];
    let mut mlit_rep = if wide {
        vec![[PINIT; MLIT]; LIT_MODELS]
    } else {
        Vec::new()
    };
    let mut dist_slot = [[PINIT; 64]; 64];
    let mut dist_bits = [PINIT; 32];
    let mut dist_align = [[PINIT; 16]; 4];
    let mut phi = 0usize;
    let mut reps = [0u32; 4];
    let mut pos = 0usize;
    let mut prev = 0u8;
    let mut prev_len = 0u32;
    let mut prev_match = false;
    let mut prev_rep = false;
    for t in toks {
        match *t {
            Tok::Lit(b) => {
                e.bit(0, &mut p_match[match_ctx(phi, prev, pos, prev_match)]);
                let have_mb = prev_match
                    && reps[0] > 0
                    && (reps[0] as usize) <= pos
                    && pos <= raw.len();
                if matched_lits && have_mb && (wide || !prev_rep) {
                    let mb = raw[pos - reps[0] as usize];
                    let slot = if wide {
                        mlit_wide_idx(phi, prev, pos)
                    } else {
                        mlit_idx(prev, pos)
                    };
                    if wide && prev_rep {
                        put_mlit(&mut e, b, mb, &mut mlit_rep[slot]);
                    } else {
                        put_mlit(&mut e, b, mb, &mut mlit[slot]);
                    }
                } else {
                    let li = lit_idx(phi, prev, pos);
                    if prev_match && prev_rep && rep_bank && !wide {
                        e.byte(b, &mut lit_after_rep[li]);
                    } else if prev_match {
                        e.byte(b, &mut lit_after_match[li]);
                    } else {
                        e.byte(b, &mut lit_after_lit[li]);
                    }
                }
                prev = b;
                pos += 1;
                prev_match = false;
                prev_rep = false;
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
                prev_rep = which < 4;
                phi = phi_step(phi, true, which < 4, d, len);
            }
        }
    }
    e.finish()
}

pub fn decode_toks(buf: &[u8], orig: usize) -> Result<Vec<u8>, &'static str> {
    decode_toks_ex(buf, orig, false, false, false)
}

pub fn decode_toks_mlit(buf: &[u8], orig: usize) -> Result<Vec<u8>, &'static str> {
    decode_toks_ex(buf, orig, true, false, false)
}

pub fn decode_toks_mlit3(buf: &[u8], orig: usize) -> Result<Vec<u8>, &'static str> {
    decode_toks_ex(buf, orig, true, true, false)
}

pub fn decode_toks_mlit4(buf: &[u8], orig: usize) -> Result<Vec<u8>, &'static str> {
    decode_toks_ex(buf, orig, true, true, true)
}

pub fn decode_toks_mlit4f(buf: &[u8], orig: usize) -> Result<Vec<u8>, &'static str> {
    decode_toks_mlit4f_inner(buf, orig)
}

fn decode_toks_ex(
    buf: &[u8],
    orig: usize,
    matched_lits: bool,
    rep_bank: bool,
    wide: bool,
) -> Result<Vec<u8>, &'static str> {
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
    let mut lit_after_rep = if rep_bank && !wide {
        vec![init256(); LIT_MODELS]
    } else {
        Vec::new()
    };
    let nmlit = if !matched_lits {
        0
    } else if wide {
        LIT_MODELS
    } else {
        256 * POS_STATES
    };
    let mut mlit = vec![[PINIT; MLIT]; nmlit];
    let mut mlit_rep = if wide {
        vec![[PINIT; MLIT]; LIT_MODELS]
    } else {
        Vec::new()
    };
    let mut dist_slot = [[PINIT; 64]; 64];
    let mut dist_bits = [PINIT; 32];
    let mut dist_align = [[PINIT; 16]; 4];
    let mut phi = 0usize;
    let mut reps = [0u32; 4];
    let mut out = Vec::with_capacity(orig);
    let mut prev = 0u8;
    let mut prev_len = 0u32;
    let mut prev_match = false;
    let mut prev_rep = false;
    while out.len() < orig {
        let m = d.bit(&mut p_match[match_ctx(phi, prev, out.len(), prev_match)]);
        if m == 0 {
            let pos = out.len();
            let have_mb = prev_match && reps[0] > 0 && (reps[0] as usize) <= pos;
            let b = if matched_lits && have_mb && (wide || !prev_rep) {
                let mb = out[pos - reps[0] as usize];
                let slot = if wide {
                    mlit_wide_idx(phi, prev, pos)
                } else {
                    mlit_idx(prev, pos)
                };
                if wide && prev_rep {
                    get_mlit(&mut d, mb, &mut mlit_rep[slot])
                } else {
                    get_mlit(&mut d, mb, &mut mlit[slot])
                }
            } else {
                let li = lit_idx(phi, prev, pos);
                if prev_match && prev_rep && rep_bank && !wide {
                    d.byte(&mut lit_after_rep[li])
                } else if prev_match {
                    d.byte(&mut lit_after_match[li])
                } else {
                    d.byte(&mut lit_after_lit[li])
                }
            };
            out.push(b);
            prev = b;
            prev_match = false;
            prev_rep = false;
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
            prev_rep = is_rep;
            phi = phi_step(phi, true, is_rep, dist, nlen as u32);
        }
    }
    Ok(out)
}


fn decode_toks_mlit4f_inner(buf: &[u8], orig: usize) -> Result<Vec<u8>, &'static str> {
    use crate::parse::MIN_MATCH;
    const FMODELS: usize = LIT_MODELS;
    #[inline(always)]
    fn fidx(phi: usize, prev: u8, pos: usize) -> usize {
        lit_idx(phi, prev, pos)
    }
    let mut d = Dec32::open(buf)?;
    let mut p_match = [PINIT; 64];
    let mut p_rep = [PINIT; 8];
    let mut p_which = [PINIT; 8];
    let mut p_len0 = [PINIT; 32];
    let mut p_len1 = [PINIT; 32];
    let mut p_len3 = [[PINIT; 8]; 32];
    let mut p_len4 = [[PINIT; 16]; 32];
    let mut p_len11 = [[PINIT; 16]; 32];
    let mut lit_after_lit = vec![init256(); FMODELS];
    let mut lit_after_match = vec![init256(); FMODELS];
    let mut mlit = vec![[PINIT; MLIT]; FMODELS];
    let mut mlit_rep = vec![[PINIT; MLIT]; FMODELS];
    let mut dist_slot = [[PINIT; 64]; 64];
    let mut dist_bits = [PINIT; 32];
    let mut dist_align = [[PINIT; 16]; 4];
    let mut phi = 0usize;
    let mut reps = [0u32; 4];
    let mut out = Vec::with_capacity(orig);
    let mut prev = 0u8;
    let mut prev_len = 0u32;
    let mut prev_match = false;
    let mut prev_rep = false;
    while out.len() < orig {
        let m = d.bit(&mut p_match[match_ctx(phi, prev, out.len(), prev_match)]);
        if m == 0 {
            let pos = out.len();
            let have_mb = prev_match && reps[0] > 0 && (reps[0] as usize) <= pos;
            let b = if have_mb {
                let mb = out[pos - reps[0] as usize];
                let slot = fidx(phi, prev, pos);
                if prev_rep {
                    get_mlit(&mut d, mb, &mut mlit_rep[slot])
                } else {
                    get_mlit(&mut d, mb, &mut mlit[slot])
                }
            } else {
                let li = fidx(phi, prev, pos);
                if prev_match {
                    d.byte(&mut lit_after_match[li])
                } else {
                    d.byte(&mut lit_after_lit[li])
                }
            };
            out.push(b);
            prev = b;
            prev_match = false;
            prev_rep = false;
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
            prev_rep = is_rep;
            phi = phi_step(phi, true, is_rep, dist, nlen as u32);
        }
    }
    Ok(out)
}


fn decode_toks_ex_rc32(
    buf: &[u8],
    orig: usize,
    matched_lits: bool,
    rep_bank: bool,
    wide: bool,
) -> Result<Vec<u8>, &'static str> {
    use crate::parse::MIN_MATCH;
    let mut d = Dec32::open(buf)?;
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
    let mut lit_after_rep = if rep_bank && !wide {
        vec![init256(); LIT_MODELS]
    } else {
        Vec::new()
    };
    let nmlit = if !matched_lits {
        0
    } else if wide {
        LIT_MODELS
    } else {
        256 * POS_STATES
    };
    let mut mlit = vec![[PINIT; MLIT]; nmlit];
    let mut mlit_rep = if wide {
        vec![[PINIT; MLIT]; LIT_MODELS]
    } else {
        Vec::new()
    };
    let mut dist_slot = [[PINIT; 64]; 64];
    let mut dist_bits = [PINIT; 32];
    let mut dist_align = [[PINIT; 16]; 4];
    let mut phi = 0usize;
    let mut reps = [0u32; 4];
    let mut out = Vec::with_capacity(orig);
    let mut prev = 0u8;
    let mut prev_len = 0u32;
    let mut prev_match = false;
    let mut prev_rep = false;
    while out.len() < orig {
        let m = d.bit(&mut p_match[match_ctx(phi, prev, out.len(), prev_match)]);
        if m == 0 {
            let pos = out.len();
            let have_mb = prev_match && reps[0] > 0 && (reps[0] as usize) <= pos;
            let b = if matched_lits && have_mb && (wide || !prev_rep) {
                let mb = out[pos - reps[0] as usize];
                let slot = if wide {
                    mlit_wide_idx(phi, prev, pos)
                } else {
                    mlit_idx(prev, pos)
                };
                if wide && prev_rep {
                    get_mlit(&mut d, mb, &mut mlit_rep[slot])
                } else {
                    get_mlit(&mut d, mb, &mut mlit[slot])
                }
            } else {
                let li = lit_idx(phi, prev, pos);
                if prev_match && prev_rep && rep_bank && !wide {
                    d.byte(&mut lit_after_rep[li])
                } else if prev_match {
                    d.byte(&mut lit_after_match[li])
                } else {
                    d.byte(&mut lit_after_lit[li])
                }
            };
            out.push(b);
            prev = b;
            prev_match = false;
            prev_rep = false;
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
            prev_rep = is_rep;
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
        let blob2 = encode_toks_mlit(&t, &s);
        let back2 = decode_toks_mlit(&blob2, s.len()).expect("mlit");
        assert_eq!(back2, s);
        let blob3 = encode_toks_mlit3(&t, &s);
        let back3 = decode_toks_mlit3(&blob3, s.len()).expect("mlit3");
        assert_eq!(back3, s);
        let blob4 = encode_toks_mlit4(&t, &s);
        let back4 = decode_toks_mlit4(&blob4, s.len()).expect("mlit4");
        assert_eq!(back4, s);
        let blob4f = encode_toks_mlit4f(&t, &s);
        let back4f = decode_toks_mlit4f(&blob4f, s.len()).expect("mlit4f");
        assert_eq!(back4f, s);
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
