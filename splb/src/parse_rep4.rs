//! BT4 finder + 4-rep block DP. Own pathway for large binaries.
//! Window capped at 4 MiB. Not xz.

use super::{match_len, Tok, MAX_MATCH, MIN_MATCH};

fn bump_reps(reps: [u32; 4], d: u32) -> [u32; 4] {
    if d == 0 || d == reps[0] {
        return reps;
    }
    if d == reps[1] {
        return [d, reps[0], reps[2], reps[3]];
    }
    if d == reps[2] {
        return [d, reps[0], reps[1], reps[3]];
    }
    [d, reps[0], reps[1], reps[2]]
}

const BLOCK: usize = 4 * 1024 * 1024;
const INF: u32 = u32::MAX / 4;
const HASH_BITS: u32 = 18;

fn bt_depth() -> usize {
    static D: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *D.get_or_init(|| {
        std::env::var("LBR1_BT_DEPTH")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&n| n >= 4 && n <= 64)
            .unwrap_or(24)
    })
}

/// Parser floor only. Bitstream still MIN_MATCH=4. Raise via LBR1_MIN_MATCH=5|8.
fn parse_min() -> u32 {
    static M: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *M.get_or_init(|| {
        std::env::var("LBR1_MIN_MATCH")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&n| n >= MIN_MATCH as u32 && n <= 16)
            .unwrap_or(MIN_MATCH as u32)
    })
}

fn lens_try(best: u32, out: &mut [u32; 16]) -> usize {
    let best = best.min(MAX_MATCH as u32);
    let minm = parse_min();
    if best < minm {
        return 0;
    }
    let mut n = 0usize;
    let mut l = minm;
    while l <= best.min(8) && n < 15 {
        out[n] = l;
        n += 1;
        l += 1;
    }
    for k in [12u32, 16, 24, 32, 48, 64, 128, 256, 384, 512, 768, 1024, 2048, 4096, 8192, 16384] {
        if k < best && n < 15 {
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

fn bits_lit() -> u32 {
    10
}
fn bits_rep(len: u32) -> u32 {
    let extra = len.saturating_sub(MIN_MATCH as u32);
    1 + 1 + 2
        + if extra < 8 {
            4
        } else if extra < 16 {
            6
        } else if extra < 31 {
            10
        } else {
            26
        }
}
fn bits_new(dist: u32, len: u32) -> u32 {
    let extra = len.saturating_sub(MIN_MATCH as u32);
    let slot = 32u32.saturating_sub(dist.max(1).leading_zeros());
    1 + 1
        + if extra < 8 {
            4
        } else if extra < 16 {
            6
        } else if extra < 31 {
            10
        } else {
            26
        }
        + 5
        + slot.saturating_sub(1)
}

/// Binary tree, 4-byte hash, cyclic buffer = window.
fn bt4_fill(data: &[u8], window: usize, best_d: &mut [u32], best_l: &mut [u32]) {
    let n = data.len();
    let win = window.max(256).min(n);
    let depth = bt_depth();
    let hs = 1usize << HASH_BITS;
    let shift = 32 - HASH_BITS;
    let mut hash = vec![-1i32; hs];
    let cyc = win;
    let mut son = vec![-1i32; cyc * 2];
    let h4 = |p: usize| -> usize {
        if p + 4 > n {
            return 0;
        }
        let v = u32::from_le_bytes(data[p..p + 4].try_into().unwrap());
        (v.wrapping_mul(0x85EB_CA6B) >> shift) as usize & (hs - 1)
    };
    for pos in 0..n {
        if pos + MIN_MATCH > n {
            break;
        }
        let zero = data[pos] | data[pos + 1] | data[pos + 2] | data[pos + 3] == 0;
        let cap_steps = if zero { depth.min(8) } else { depth };
        let h = h4(pos);
        let mut cur = hash[h];
        hash[h] = pos as i32;
        let floor = if pos > win { (pos - win) as i32 } else { -1 };
        let slot = (pos % cyc) * 2;
        let mut ptr0 = slot;
        let mut ptr1 = slot + 1;
        let mut len0 = 0usize;
        let mut len1 = 0usize;
        let mut bd = 0u32;
        let mut bl = 0u32;
        let mut steps = 0usize;
        loop {
            if cur <= floor || steps >= cap_steps {
                son[ptr0] = -1;
                son[ptr1] = -1;
                break;
            }
            steps += 1;
            let j = cur as usize;
            if j >= pos {
                son[ptr0] = -1;
                son[ptr1] = -1;
                break;
            }
            let pair = (j % cyc) * 2;
            let mut len = len0.min(len1);
            let max = (n - pos).min(n - j).min(MAX_MATCH);
            while len < max && data[pos + len] == data[j + len] {
                len += 1;
            }
            if (len as u32) > bl {
                bl = len as u32;
                bd = (pos - j) as u32;
                if len >= max {
                    son[ptr0] = son[pair];
                    son[ptr1] = son[pair + 1];
                    break;
                }
            }
            if j + len >= n || data[j + len] < data[pos + len] {
                son[ptr1] = cur;
                ptr1 = pair + 1;
                cur = son[ptr1];
                len1 = len;
            } else {
                son[ptr0] = cur;
                ptr0 = pair;
                cur = son[ptr0];
                len0 = len;
            }
        }
        best_d[pos] = bd;
        best_l[pos] = bl;
    }
}

/// Hash-4 chain 128 on the same 4 MiB window. Keeps BT4's match if longer.
fn chain_improve(data: &[u8], window: usize, best_d: &mut [u32], best_l: &mut [u32]) {
    let n = data.len();
    let win = window.max(256).min(n);
    const HS: usize = 1 << 20;
    let mut head = vec![-1i32; HS];
    let mut prevc = vec![-1i32; win];
    let mut sensors = crate::sensors::Sensors::new(n, 64);
    let h20 = |p: usize| -> usize {
        if p + 4 > n {
            return 0;
        }
        let v = u32::from_le_bytes(data[p..p + 4].try_into().unwrap());
        (v.wrapping_mul(0x85EB_CA6B) >> 12) as usize
    };
    for i in 0..n {
        if i + MIN_MATCH > n {
            break;
        }
        let floor = if i > win { (i - win) as i32 } else { -1 };
        let h = h20(i);
        let zero = data[i] | data[i + 1] | data[i + 2] | data[i + 3] == 0;
        let cap = if zero { 8 } else { 128 };
        let mut p = head[h];
        let mut steps = 0usize;
        let mut bd = best_d[i];
        let mut bl = best_l[i];
        while p > floor && steps < cap {
            let j = p as usize;
            if j < i {
                super::consider(&mut bl, &mut bd, data, i, j);
            }
            p = prevc[j % win];
            steps += 1;
        }
        for r in sensors.reports(data, i) {
            if (r.pos as i32) > floor && r.pos < i {
                super::consider(&mut bl, &mut bd, data, i, r.pos);
            }
        }
        best_d[i] = bd;
        best_l[i] = bl;
        if i + 3 < n {
            prevc[i % win] = head[h];
            head[h] = i as i32;
            sensors.plant(data, i);
        }
    }
}

pub fn parse_rep4(data: &[u8], window: usize) -> Vec<Tok> {
    let n = data.len();
    if n == 0 {
        return Vec::new();
    }
    let win = window.max(256).min(n);
    let mut best_d = vec![0u32; n];
    let mut best_l = vec![0u32; n];
    bt4_fill(data, win, &mut best_d, &mut best_l);
    chain_improve(data, win, &mut best_d, &mut best_l);

    let mut toks = Vec::new();
    let mut file_reps = [0u32; 4];
    let mut file_phi = 0usize;
    let mut file_prev_match = false;
    let mut file_prev_len = 0u32;
    let mut book = crate::range::TinyBook::new();
    let mut pos = 0usize;
    while pos < n {
        let end = (pos + BLOCK).min(n);
        let m = end - pos;
        let mut price = vec![INF; m + 1];
        let mut come = vec![-1i32; m + 1];
        let mut come_d = vec![0u32; m + 1];
        let mut r0 = vec![0u32; m + 1];
        let mut r1 = vec![0u32; m + 1];
        let mut r2 = vec![0u32; m + 1];
        let mut r3 = vec![0u32; m + 1];
        let mut phi_at = vec![0u8; m + 1];
        let mut pma_at = vec![0u8; m + 1];
        let mut plen_at = vec![0u32; m + 1];
        price[0] = 0;
        r0[0] = file_reps[0];
        r1[0] = file_reps[1];
        r2[0] = file_reps[2];
        r3[0] = file_reps[3];
        phi_at[0] = file_phi as u8;
        pma_at[0] = file_prev_match as u8;
        plen_at[0] = file_prev_len;
        let mut cand = [0u32; 16];
        for k in 0..m {
            if price[k] == INF {
                continue;
            }
            let i = pos + k;
            let ph = phi_at[k] as usize;
            let pma = pma_at[k] != 0;
            let pln = plen_at[k];
            let cl = bits_lit();
            let pl = price[k].saturating_add(cl);
            if pl < price[k + 1] {
                price[k + 1] = pl;
                come[k + 1] = -1;
                r0[k + 1] = r0[k];
                r1[k + 1] = r1[k];
                r2[k + 1] = r2[k];
                r3[k + 1] = r3[k];
                phi_at[k + 1] = crate::range::phi_step(ph, false, false, 0, 0) as u8;
                pma_at[k + 1] = 0;
                plen_at[k + 1] = pln;
            }
            let reps = [r0[k], r1[k], r2[k], r3[k]];
            if i + MIN_MATCH <= n {
                for &rd in &reps {
                    if rd == 0 || (rd as usize) > i {
                        continue;
                    }
                    let cap = (n - i).min(m - k).min(MAX_MATCH);
                    let lr = match_len(data, i, i - rd as usize, cap) as u32;
                    let nc = lens_try(lr, &mut cand);
                    for t in 0..nc {
                        let len = cand[t];
                        let j = k + len as usize;
                        if j > m {
                            continue;
                        }
                        let cr = bits_rep(len);
                        let pm = price[k].saturating_add(cr);
                        if pm < price[j] {
                            price[j] = pm;
                            come[j] = len as i32;
                            come_d[j] = rd;
                            let nr = bump_reps(reps, rd);
                            r0[j] = nr[0];
                            r1[j] = nr[1];
                            r2[j] = nr[2];
                            r3[j] = nr[3];
                            phi_at[j] = crate::range::phi_step(ph, true, true, rd, len) as u8;
                            pma_at[j] = 1;
                            plen_at[j] = len;
                        }
                    }
                }
            }
            if best_l[i] >= parse_min() {
                let dist = best_d[i];
                let is_rep = dist != 0 && reps.iter().any(|&r| r == dist);
                let nc = lens_try(best_l[i].min((m - k) as u32), &mut cand);
                for t in 0..nc {
                    let len = cand[t];
                    let j = k + len as usize;
                    if j > m {
                        continue;
                    }
                    let add = if is_rep {
                        bits_rep(len)
                    } else {
                        bits_new(dist, len)
                    };
                    let pm = price[k].saturating_add(add);
                    if pm < price[j] {
                        price[j] = pm;
                        come[j] = len as i32;
                        come_d[j] = dist;
                        let nr = bump_reps(reps, dist);
                        r0[j] = nr[0];
                        r1[j] = nr[1];
                        r2[j] = nr[2];
                        r3[j] = nr[3];
                        phi_at[j] = crate::range::phi_step(ph, true, is_rep, dist, len) as u8;
                        pma_at[j] = 1;
                        plen_at[j] = len;
                    }
                }
            }
        }
        let mut local = Vec::new();
        let mut k = m;
        let mut guard = 0usize;
        while k > 0 && guard < m + 2 {
            guard += 1;
            if come[k] < 0 {
                local.push(Tok::Lit(data[pos + k - 1]));
                k -= 1;
            } else {
                let len = come[k] as u32;
                if len == 0 || (len as usize) > k {
                    local.push(Tok::Lit(data[pos + k - 1]));
                    k -= 1;
                    continue;
                }
                local.push(Tok::Match {
                    dist: come_d[k],
                    len,
                });
                k -= len as usize;
            }
        }
        local.reverse();
        let mut ph = file_phi;
        let mut pma = file_prev_match;
        let mut pln = file_prev_len;
        let mut ip = pos;
        let mut prv = if pos == 0 { 0 } else { data[pos - 1] };
        let mut reps = file_reps;
        for t in &local {
            match *t {
                Tok::Lit(b) => {
                    book.feed(false, false, ph, prv, ip, pma, 0, 0);
                    ph = crate::range::phi_step(ph, false, false, 0, 0);
                    pma = false;
                    prv = b;
                    ip += 1;
                }
                Tok::Match { dist, len } => {
                    let d = if dist == 0 { reps[0] } else { dist };
                    let is_rep = d != 0 && reps.iter().any(|&r| r == d);
                    book.feed(true, is_rep, ph, prv, ip, pma, d, len);
                    ph = crate::range::phi_step(ph, true, is_rep, d, len);
                    pma = true;
                    pln = len;
                    ip += len as usize;
                    if ip > 0 && ip <= n {
                        prv = data[ip - 1];
                    }
                    reps = bump_reps(reps, d);
                }
            }
        }
        file_phi = ph;
        file_prev_match = pma;
        file_prev_len = pln;
        file_reps = reps;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bt4_finds_repeat() {
        let mut s = Vec::new();
        let block = b"ALPHACODECHUNK!!";
        for _ in 0..40 {
            s.extend_from_slice(block);
        }
        let t = parse_rep4(&s, 1 << 16);
        let matches = t.iter().filter(|x| matches!(x, Tok::Match { .. })).count();
        assert!(matches >= 1, "got {matches} matches");
        let n: usize = t
            .iter()
            .map(|x| match x {
                Tok::Lit(_) => 1,
                Tok::Match { len, .. } => *len as usize,
            })
            .sum();
        assert_eq!(n, s.len());
        let blob = crate::frame::pack(&t, s.len(), 1 << 16, &s);
        assert_eq!(crate::decode(&blob).expect("decode"), s);
        assert!(blob.len() < s.len());
    }
}
