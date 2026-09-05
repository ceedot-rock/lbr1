//! Isolated DP. No TinyBook type in this module's hot fn.

use crate::parse::{match_len, MIN_MATCH, MAX_MATCH};
use crate::range::{bit_price, TinyBook};

use crate::parse::Tok;
use crate::range::phi_step;

const INF: u32 = u32::MAX / 4;

pub struct Driver {
    tiny: TinyBook,
    phi: usize,
    prev_match: bool,
}

impl Driver {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            tiny: TinyBook::new(),
            phi: 0,
            prev_match: false,
        })
    }

    pub fn tiny_lit(&self) -> u32 {
        self.tiny.lit_cost(self.prev_match)
    }
    pub fn tiny_rep(&self) -> u32 {
        self.tiny.rep_cost(self.phi)
    }
    pub fn tiny_nd(&self) -> u32 {
        self.tiny.new_dist_flag(self.phi)
    }

    pub fn fill(
        &mut self,
        data: &[u8],
        pos: usize,
        m: usize,
        best_d: &[u32],
        best_l: &[u32],
        file_last: u32,
        price: &mut [u32],
        come: &mut [i32],
        come_d: &mut [u32],
        last_at: &mut [u32],
    ) {
        let sc = precompute_costs(&self.tiny);
        dp_inner(
            data, pos, m, best_d, best_l, file_last, &sc, price, come, come_d, last_at,
        );
    }

    pub fn adapt(&mut self, toks: &[Tok], data: &[u8], pos: usize) {
        let n = data.len();
        let mut fp = pos;
        let mut pr = if fp > 0 { data[fp - 1] } else { 0 };
        for t in toks {
            match *t {
                Tok::Lit(_) => {
                    self.tiny.feed(false, false, self.phi, pr, fp, self.prev_match, 0, 0);
                    self.phi = phi_step(self.phi, false, false, 0, 0);
                    if fp < n {
                        pr = data[fp];
                    }
                    fp += 1;
                    self.prev_match = false;
                }
                Tok::Match { dist, len } => {
                    self.tiny.feed(true, dist == 0, self.phi, pr, fp, self.prev_match, dist, len);
                    self.phi = phi_step(self.phi, true, dist == 0, dist, len);
                    fp += len as usize;
                    if fp > 0 && fp <= n {
                        pr = data[fp - 1];
                    }
                    self.prev_match = true;
                }
            }
        }
    }
}
const MAXL: usize = 274;

pub struct SmallCosts {
    pub match_cost: [[u32; 2]; 64],
    pub rep_cost: [u32; 8],
    pub new_dist: [u32; 8],
    pub len_cost: [[u32; MAXL]; 32],
    pub slot_cost: [u32; 64],
    pub lit_avg: [u32; 2],
}

pub fn precompute_costs(tiny: &TinyBook) -> Box<SmallCosts> {
    let mut sc = Box::new(SmallCosts {
        match_cost: [[0; 2]; 64],
        rep_cost: [0; 8],
        new_dist: [0; 8],
        len_cost: [[0; MAXL]; 32],
        slot_cost: [0; 64],
        lit_avg: [0; 2],
    });
    for ctx in 0..64 {
        sc.match_cost[ctx][0] = bit_price(tiny.match_ctx[ctx], 0);
        sc.match_cost[ctx][1] = bit_price(tiny.match_ctx[ctx], 1);
    }
    for i in 0..8 {
        sc.rep_cost[i] = bit_price(tiny.p_rep[i], 1) + 2 * 1024 * 32;
        sc.new_dist[i] = bit_price(tiny.p_rep[i], 0);
    }
    for ctx in 0..32 {
        let p0 = tiny.p_len0[ctx];
        for len in 0..MAXL {
            let extra = (len as u32).saturating_sub(MIN_MATCH as u32);
            sc.len_cost[ctx][len] = if extra < 8 {
                bit_price(p0, 0) + extra * 1024 * 32
            } else if extra < 16 {
                bit_price(p0, 1) + 1024 * 32 + (extra - 8) * 1024 * 32
            } else {
                bit_price(p0, 1) + 18 * 1024 * 32
            };
        }
    }
    for slot in 0..64u32 {
        sc.slot_cost[slot as usize] = 5 * 1024 * 32 + slot.saturating_sub(1) * 1024 * 32;
        if slot > 0 && slot < 32 {
            sc.slot_cost[slot as usize] += bit_price(tiny.dist_slot[1], if slot > 16 { 1 } else { 0 });
        }
    }
    sc.lit_avg[0] = 8 * bit_price(tiny.lit_avg[0], 0);
    sc.lit_avg[1] = 8 * bit_price(tiny.lit_avg[1], 0);
    sc
}

fn lens(best: u32) -> impl Iterator<Item = u32> {
    let best = best.min((MAXL - 1) as u32);
    let minm = MIN_MATCH as u32;
    (minm..=best.min(8))
        .chain([12, 16, 24, 32, 48, 64, 128, 256].into_iter().filter(move |&k| k < best && k >= minm))
        .chain(std::iter::once(best).filter(move |&b| b >= minm))
}

#[inline(never)]
pub fn dp_inner(
    data: &[u8],
    pos: usize,
    m: usize,
    best_d: &[u32],
    best_l: &[u32],
    file_last: u32,
    sc: &SmallCosts,
    price: &mut [u32],
    come: &mut [i32],
    come_d: &mut [u32],
    last_at: &mut [u32],
) {
    let n = data.len();
    price[0] = 0;
    last_at[0] = file_last;
    for k in 0..m {
        if price[k] == INF {
            continue;
        }
        let ctx = k & 63;
        let lctx = k & 31;
        let pl = price[k].saturating_add(sc.match_cost[ctx][0] + sc.lit_avg[0]);
        if pl < price[k + 1] {
            price[k + 1] = pl;
            come[k + 1] = -1;
            last_at[k + 1] = last_at[k];
        }
        let i0 = pos + k;
        let ld = last_at[k];
        if ld > 0 && (ld as usize) <= i0 {
            let cap = MAX_MATCH.min(m - k).min(MAXL - 1);
            let lr = match_len(data, i0, i0 - ld as usize, cap) as u32;
            if lr >= MIN_MATCH as u32 {
                for len in lens(lr) {
                    let j = k + len as usize;
                    if j > m {
                        continue;
                    }
                    let pm = price[k].saturating_add(
                        sc.match_cost[ctx][1]
                            + sc.len_cost[lctx][len.min((MAXL - 1) as u32) as usize]
                            + sc.rep_cost[0],
                    );
                    if pm < price[j] {
                        price[j] = pm;
                        come[j] = len as i32;
                        come_d[j] = 0;
                        last_at[j] = ld;
                    }
                }
            }
        }
        if best_l[k] >= MIN_MATCH as u32 {
            for len in lens(best_l[k]) {
                let j = k + len as usize;
                if j > m {
                    continue;
                }
                let is_rep = best_d[k] == last_at[k] && last_at[k] != 0;
                let li = len.min((MAXL - 1) as u32) as usize;
                let add = if is_rep {
                    sc.match_cost[ctx][1] + sc.len_cost[lctx][li] + sc.rep_cost[0]
                } else {
                    let slot = (32 - best_d[k].max(1).leading_zeros()).min(63) as usize;
                    sc.match_cost[ctx][1]
                        + sc.len_cost[lctx][li]
                        + sc.new_dist[0]
                        + sc.slot_cost[slot]
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
        let _ = n;
    }
}
