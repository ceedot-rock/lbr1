//! Block parse codegen unit. No TinyBook / Driver / SmallCosts types.
use crate::parse::{consider, match_len, Tok, MIN_MATCH, MAX_MATCH};

const BLOCK: usize = 256 * 1024;
const INF: u32 = u32::MAX / 4;

fn lens(best: u32) -> impl Iterator<Item = u32> {
    let best = best.min(273);
    let minm = MIN_MATCH as u32;
    (minm..=best.min(8))
        .chain([12, 16, 24, 32, 48, 64, 128, 256].into_iter().filter(move |&k| k < best && k >= minm))
        .chain(std::iter::once(best).filter(move |&b| b >= minm))
}

#[inline(never)]
pub fn parse_blocks(data: &[u8], window: usize) -> Vec<Tok> {
    use crate::parse::consider;
    let n = data.len();
    if n == 0 {
        return Vec::new();
    }
    let win = window.max(256).min(n);
    const HS: usize = 1 << 20;
    let mut head = vec![-1i32; HS];
    let mut prevc = vec![-1i32; n];
    let h20 = |p: usize| -> usize {
        if p + 4 > n {
            return 0;
        }
        let v = u32::from_le_bytes(data[p..p + 4].try_into().unwrap());
        (v.wrapping_mul(0x85EB_CA6B) >> 12) as usize
    };
    let use_scouts = crate::detect::scouts_wanted(data);
    let stride = crate::detect::scout_stride(data).max(1);
    let mut sensors = crate::sensors::Sensors::new(n, stride);
    let mut toks = Vec::new();
    let mut file_last = 0u32;
    let mut c_lit = 10u32 * 1024;
    let mut c_rep = 4u32 * 1024;
    let mut c_nd = 8u32 * 1024;

    let insert = |head: &mut [i32], prevc: &mut [i32], sensors: &mut crate::sensors::Sensors, pos: usize| {
        if pos + 3 >= n {
            return;
        }
        let h = h20(pos);
        prevc[pos] = head[h];
        head[h] = pos as i32;
        if use_scouts {
            sensors.plant(data, pos);
        }
    };
    let find = |head: &[i32], prevc: &[i32], sensors: &mut crate::sensors::Sensors, pos: usize| -> (u32, u32) {
        if pos + MIN_MATCH > n {
            return (0, 0);
        }
        let floor = if pos > win { (pos - win) as i32 } else { -1 };
        let mut best_l = 0u32;
        let mut best_d = 0u32;
        let h = h20(pos);
        let zero = data[pos] | data[pos + 1] | data[pos + 2] | data[pos + 3] == 0;
        let cap = if zero { 8 } else { 96 };
        let mut p = head[h];
        let mut steps = 0;
        while p > floor && steps < cap {
            let j = p as usize;
            if j < pos {
                consider(&mut best_l, &mut best_d, data, pos, j);
                if best_l as usize >= 4096 {
                    break;
                }
            }
            p = prevc[j];
            steps += 1;
        }
        if use_scouts {
            for r in sensors.reports(data, pos) {
                if (r.pos as i32) > floor && r.pos < pos {
                    consider(&mut best_l, &mut best_d, data, pos, r.pos);
                }
            }
        }
        (best_d, best_l)
    };

    let mut pos = 0usize;
    while pos < n {
        let end = (pos + BLOCK).min(n);
        let m = end - pos;
        let mut best_d = vec![0u32; m];
        let mut best_l = vec![0u32; m];
        for k in 0..m {
            let i = pos + k;
            let (d, l) = find(&head, &prevc, &mut sensors, i);
            best_d[k] = d;
            best_l[k] = l;
            insert(&mut head, &mut prevc, &mut sensors, i);
        }
        let inf = INF;
        let mut price = vec![inf; m + 1];
        let mut come = vec![0i32; m + 1];
        let mut come_d = vec![0u32; m + 1];
        let mut last_at = vec![0u32; m + 1];
        price[0] = 0;
        last_at[0] = file_last;
        for k in 0..m {
            if price[k] == inf {
                continue;
            }
            let pl = price[k].saturating_add(c_lit);
            if pl < price[k + 1] {
                price[k + 1] = pl;
                come[k + 1] = -1;
                last_at[k + 1] = last_at[k];
            }
            let i0 = pos + k;
            let ld = last_at[k];
            if ld > 0 && (ld as usize) <= i0 {
                let lr = match_len(data, i0, i0 - ld as usize, MAX_MATCH.min(m - k)) as u32;
                if lr >= MIN_MATCH as u32 {
                    for len in lens(lr) {
                        let j = k + len as usize;
                        if j > m {
                            continue;
                        }
                        let pm = price[k].saturating_add(c_rep + (len << 5));
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
                    let slot = 32 - best_d[k].max(1).leading_zeros();
                    let pm = price[k].saturating_add(if is_rep {
                        c_rep + (len << 5)
                    } else {
                        c_nd + (len << 5) + slot * 1024
                    });
                    if pm < price[j] {
                        price[j] = pm;
                        come[j] = len as i32;
                        come_d[j] = if is_rep { 0 } else { best_d[k] };
                        last_at[j] = if is_rep { last_at[k] } else { best_d[k] };
                    }
                }
            }
        }
        let mut local = Vec::new();
        let mut k = m;
        while k > 0 {
            if come[k] < 0 {
                local.push(Tok::Lit(data[pos + k - 1]));
                k -= 1;
            } else {
                let len = come[k] as u32;
                local.push(Tok::Match {
                    dist: come_d[k],
                    len,
                });
                k -= len as usize;
            }
        }
        local.reverse();
        let mut reps = 0u32;
        let mut lits = 0u32;
        let mut fars = 0u32;
        for t in &local {
            match *t {
                Tok::Lit(_) => lits += 1,
                Tok::Match { dist, .. } => {
                    if dist == 0 { reps += 1; } else { fars += 1; }
                }
            }
        }
        if reps > lits { c_rep = c_rep.saturating_sub(256); }
        if fars > reps { c_nd = c_nd.saturating_add(256); }
        let _ = c_lit;
        if let Some(Tok::Match { dist, .. }) = local.iter().rev().find(|t| matches!(t, Tok::Match { .. })) {
            if *dist != 0 {
                file_last = *dist;
            }
        }
        toks.extend(local);
        pos = end;
    }
    let mut last = 0u32;
    for t in &mut toks {
        if let Tok::Match { dist, .. } = t {
            if *dist != 0 && *dist == last {
                *dist = 0;
            } else if *dist != 0 {
                last = *dist;
            } else if last == 0 {
                *dist = 1;
            }
        }
    }
    toks
}
