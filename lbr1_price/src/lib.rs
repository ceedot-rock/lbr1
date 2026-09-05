//! C ABI price DP. No finder. No Vec<Vec<Match>>.

pub const MIN_MATCH: u32 = 4;
const PINIT: u16 = 1024;
const INF: u32 = u32::MAX / 4;
const MAXL: usize = 274;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TinyBookC {
    pub match_ctx: [u16; 64],
    pub p_rep: [u16; 8],
    pub p_len0: [u16; 32],
    pub dist_slot: [u16; 64],
    pub lit_avg: [u16; 2],
}

impl TinyBookC {
    pub fn new() -> Self {
        Self {
            match_ctx: [PINIT; 64],
            p_rep: [PINIT; 8],
            p_len0: [PINIT; 32],
            dist_slot: [PINIT; 64],
            lit_avg: [PINIT, PINIT],
        }
    }
}

fn bit_price(p: u16, bit: u32) -> u32 {
    let c = if bit == 0 { 2048 - p as u32 } else { p as u32 };
    c * 32
}

fn p_update(p: &mut u16, bit: u32) {
    if bit == 0 {
        *p = (*p).saturating_add((2048 - *p) >> 5).min(2047);
    } else {
        *p = (*p - (*p >> 5)).max(1);
    }
}

struct Costs {
    match0: [u32; 64],
    match1: [u32; 64],
    rep: u32,
    new_dist: u32,
    len: [u32; MAXL],
    slot: [u32; 64],
    lit: u32,
}

fn bias(p: u16, bit: u32) -> u32 {
    let pr = p as u32;
    if bit == 0 { (2048 - pr) >> 8 } else { pr >> 8 }
}

fn costs_from(t: &TinyBookC) -> Costs {
    let mut c = Costs {
        match0: [0; 64],
        match1: [0; 64],
        rep: 0,
        new_dist: 0,
        len: [0; MAXL],
        slot: [0; 64],
        lit: 0,
    };
    for i in 0..64 {
        c.match0[i] = 1 + bias(t.match_ctx[i], 0);
        c.match1[i] = 1 + bias(t.match_ctx[i], 1);
        c.slot[i] = 5 + (i as u32).saturating_sub(1);
    }
    c.rep = 4 + bias(t.p_rep[0], 1);
    c.new_dist = 2 + bias(t.p_rep[0], 0);
    for len in 0..MAXL {
        let extra = (len as u32).saturating_sub(MIN_MATCH);
        c.len[len] = if extra < 8 {
            4
        } else if extra < 16 {
            6
        } else if extra < 31 {
            10
        } else {
            26
        };
    }
    c.lit = 10 + bias(t.lit_avg[0], 0);
    c
}

fn match_len(buf: &[u8], i: usize, j: usize, cap: usize) -> usize {
    let n = buf.len();
    let mut l = 0;
    while i + l < n && j + l < n && l < cap && buf[i + l] == buf[j + l] {
        l += 1;
    }
    l
}

fn lens_push(out: &mut [u32; 16], best: u32) -> usize {
    let best = best.min((MAXL - 1) as u32);
    if best < MIN_MATCH {
        return 0;
    }
    let mut n = 0usize;
    let mut l = MIN_MATCH;
    while l <= best.min(8) && n < 15 {
        out[n] = l;
        n += 1;
        l += 1;
    }
    for k in [12u32, 16, 24, 32, 48, 64, 128, 256] {
        if k < best && k >= MIN_MATCH && n < 15 {
            out[n] = k;
            n += 1;
        }
    }
    if n < 16 {
        out[n] = best;
        n += 1;
    }
    n
}

use std::cell::RefCell;
thread_local! {
    static TINY: RefCell<TinyBookC> = RefCell::new(TinyBookC::new());
}

/// DP only. `come` must be prefilled with -1. Never writes come_len == 0.
#[no_mangle]
pub extern "C" fn price_block_c(
    price: *mut u32,
    come: *mut i32,
    come_d: *mut u32,
    last_at: *mut u32,
    buf: *const u8,
    buf_len: usize,
    pos: usize,
    block_len: usize,
    best_d: *const u32,
    best_l: *const u32,
    file_last: u32,
) {
    if price.is_null() || come.is_null() || block_len == 0 {
        return;
    }
    let m = block_len;
    let price = unsafe { std::slice::from_raw_parts_mut(price, m + 1) };
    let come = unsafe { std::slice::from_raw_parts_mut(come, m + 1) };
    let come_d = unsafe { std::slice::from_raw_parts_mut(come_d, m + 1) };
    let last_at = unsafe { std::slice::from_raw_parts_mut(last_at, m + 1) };
    let buf = unsafe { std::slice::from_raw_parts(buf, buf_len) };
    let best_d = unsafe { std::slice::from_raw_parts(best_d, m) };
    let best_l = unsafe { std::slice::from_raw_parts(best_l, m) };

    let tiny = TINY.with(|t| *t.borrow());
    let sc = costs_from(&tiny);

    for i in 0..=m {
        price[i] = INF;
        come[i] = -1;
        come_d[i] = 0;
        last_at[i] = 0;
    }
    price[0] = 0;
    last_at[0] = file_last;

    let mut cand = [0u32; 16];
    for k in 0..m {
        if price[k] == INF {
            continue;
        }
        let ctx = k & 63;
        let pl = price[k].saturating_add(sc.match0[ctx] + sc.lit);
        if pl < price[k + 1] {
            price[k + 1] = pl;
            come[k + 1] = -1;
            last_at[k + 1] = last_at[k];
        }
        let i0 = pos + k;
        let ld = last_at[k];
        if ld > 0 && (ld as usize) <= i0 && i0 < buf_len {
            let cap = (m - k).min(MAXL - 1);
            let lr = match_len(buf, i0, i0 - ld as usize, cap) as u32;
            let n = lens_push(&mut cand, lr);
            for t in 0..n {
                let len = cand[t];
                if len < MIN_MATCH {
                    continue;
                }
                let j = k + len as usize;
                if j > m {
                    continue;
                }
                let li = len.min((MAXL - 1) as u32) as usize;
                let pm = price[k].saturating_add(sc.match1[ctx] + sc.len[li] + sc.rep);
                if pm < price[j] {
                    price[j] = pm;
                    come[j] = len as i32;
                    come_d[j] = 0;
                    last_at[j] = ld;
                }
            }
        }
        if best_l[k] >= MIN_MATCH {
            let n = lens_push(&mut cand, best_l[k]);
            for t in 0..n {
                let len = cand[t];
                if len < MIN_MATCH {
                    continue;
                }
                let j = k + len as usize;
                if j > m {
                    continue;
                }
                let is_rep = best_d[k] == last_at[k] && last_at[k] != 0;
                let li = len.min((MAXL - 1) as u32) as usize;
                let add = if is_rep {
                    sc.match1[ctx] + sc.len[li] + sc.rep
                } else {
                    let slot = (32 - best_d[k].max(1).leading_zeros()).min(63) as usize;
                    sc.match1[ctx] + sc.len[li] + sc.new_dist + sc.slot[slot]
                };
                let pm = price[k].saturating_add(add);
                if pm < price[j] {
                    price[j] = pm;
                    come[j] = len as i32;
                    come_d[j] = if is_rep { 0 } else { best_d[k] };
                    last_at[j] = if is_rep { last_at[k] } else { best_d[k] };
                }
            }
        }
    }

    // adapt TinyBook from this block's chosen path
    TINY.with(|cell| {
        let mut t = cell.borrow_mut();
        let mut k = m;
        let mut guard = 0usize;
        while k > 0 && guard < m + 2 {
            guard += 1;
            if come[k] < 0 {
                p_update(&mut t.match_ctx[0], 0);
                k -= 1;
            } else {
                let len = come[k] as usize;
                if len == 0 || len > k {
                    p_update(&mut t.match_ctx[0], 0);
                    k -= 1;
                    continue;
                }
                p_update(&mut t.match_ctx[0], 1);
                p_update(&mut t.p_rep[0], if come_d[k] == 0 { 1 } else { 0 });
                k -= len;
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn price_reset_c() {
    TINY.with(|t| *t.borrow_mut() = TinyBookC::new());
}
