//! Hash-4 chain + bit-priced parse.
//! Left brain: long-distance copies.

#[path = "parse_rep4.rs"]
mod parse_rep4;

pub const MIN_MATCH: usize = 4;
pub const MAX_MATCH: usize = 65535;
pub const HASH_BITS: usize = 17;
pub const HASH_SIZE: usize = 1 << HASH_BITS;
pub const MAX_CHAIN: usize = 128;
pub const DEFAULT_WINDOW: usize = 1 << 22;

fn env_usize(name: &str, default: usize, lo: usize, hi: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n >= lo && n <= hi)
        .unwrap_or(default)
}

/// mozilla parse_block_dp chain. Default 96. Override with LBR1_CHAIN=16|32|64|128.
fn block_chain_cap(zero: bool) -> usize {
    static CHAIN: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let n = *CHAIN.get_or_init(|| env_usize("LBR1_CHAIN", 96, 1, 256));
    if zero {
        n.min(8).max(1)
    } else {
        n
    }
}

fn block_hash_bits() -> u32 {
    static B: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *B.get_or_init(|| env_usize("LBR1_HASH", 20, 16, 22) as u32)
}
pub const SCOUT_STRIDE: usize = 256;
pub const SCOUT_BITS: usize = 16;
pub const SCOUT_SIZE: usize = 1 << SCOUT_BITS;
pub const SCOUT_CHAIN: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    Lit(u8),
    Match { dist: u32, len: u32 },
}

#[inline]
fn hash4(a: u8, b: u8, c: u8, d: u8) -> usize {
    let v = u32::from_le_bytes([a, b, c, d]);
    (v.wrapping_mul(0x9E3779B1) >> (32 - HASH_BITS)) as usize
}

/// Index + 16-bit tag for LZ4-class single-slot find (`LBR1_PARSE=lz4t`).
/// Tag uses low bits of the same multiply so cheap reject can skip a 4-byte load
/// when the overwritten slot's fingerprint disagrees.
#[inline]
fn hash4_tag(a: u8, b: u8, c: u8, d: u8) -> (usize, u16) {
    let v = u32::from_le_bytes([a, b, c, d]);
    let h = v.wrapping_mul(0x9E3779B1);
    let idx = (h >> (32 - HASH_BITS)) as usize;
    let tag = (h & 0xFFFF) as u16;
    (idx, tag)
}

pub(crate) fn match_len(data: &[u8], i: usize, j: usize, cap: usize) -> usize {
    let max = cap.min(data.len() - i).min(data.len() - j);
    let mut n = 0;
    while n + 8 <= max {
        let a = u64::from_le_bytes(data[i + n..i + n + 8].try_into().unwrap());
        let b = u64::from_le_bytes(data[j + n..j + n + 8].try_into().unwrap());
        if a != b {
            break;
        }
        n += 8;
    }
    while n < max && data[i + n] == data[j + n] {
        n += 1;
    }
    n
}

#[inline]
fn hash8(data: &[u8], i: usize) -> usize {
    if i + 8 > data.len() {
        return 0;
    }
    let lo = u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
    let hi = u32::from_le_bytes(data[i + 4..i + 8].try_into().unwrap());
    let v = (lo as u64) | ((hi as u64) << 32);
    (v.wrapping_mul(0x9E37_79B1_85EB_CA77) >> (64 - SCOUT_BITS)) as usize
}

fn bits_saved(len: u32, dist: u32) -> i32 {
    if len == 0 {
        return i32::MIN / 4;
    }
    let lit = (len as i32) * 8;
    let d_bits = 32 - dist.max(1).leading_zeros() as i32;
    let l_bits = if len < 255 { 8 } else { 16 };
    lit - (1 + l_bits + d_bits)
}

pub(crate) fn consider(best_len: &mut u32, best_dist: &mut u32, data: &[u8], i: usize, j: usize) {
    if j >= i {
        return;
    }
    if data[j] != data[i]
        || data[j + 1] != data[i + 1]
        || data[j + 2] != data[i + 2]
        || data[j + 3] != data[i + 3]
    {
        return;
    }
    let n = match_len(data, i, j, MAX_MATCH) as u32;
    if n < MIN_MATCH as u32 {
        return;
    }
    let old = bits_saved(*best_len, (*best_dist).max(1));
    let new = bits_saved(n, (i - j) as u32);
    if new > old || (new == old && n > *best_len) {
        *best_len = n;
        *best_dist = (i - j) as u32;
    }
}

fn find_match(
    data: &[u8],
    i: usize,
    window: usize,
    head: &[i32],
    prev: &[i32],
    scout_head: &[i32],
    scout_prev: &[i32],
    stride: usize,
    use_scouts: bool,
) -> (u32, u32) {
    if i + MIN_MATCH > data.len() {
        return (0, 0);
    }
    let h = hash4(data[i], data[i + 1], data[i + 2], data[i + 3]);
    let mut p = head[h];
    let mut best_len = 0u32;
    let mut best_dist = 0u32;
    let mut steps = 0;
    let floor = if i > window { (i - window) as i32 } else { -1 };
    while p > floor && steps < MAX_CHAIN {
        let j = p as usize;
        consider(&mut best_len, &mut best_dist, data, i, j);
        if best_len as usize == MAX_MATCH {
            return (best_dist, best_len);
        }
        p = prev[j];
        steps += 1;
    }
    // Sentinel-gated hash-8 scouts: extra match chain when the controller
    // says Support/Throttle. Not an NCA that writes bytes.
    if i + 8 <= data.len() {
        let mut sent = crate::sentinel::Sentinel::new();
        let miss = if best_len < 8 { 1.0 } else { 0.0 };
        let tick = sent.step([miss, 0.0, 0.0, if steps >= MAX_CHAIN { 1.0 } else { 0.0 }, 0.0], miss, 0.0);
        let depth = match tick.decision {
            crate::sentinel::Decision::Support | crate::sentinel::Decision::Throttle => SCOUT_CHAIN * 2,
            crate::sentinel::Decision::Block => SCOUT_CHAIN,
            _ => {
                if best_len < MIN_MATCH as u32 {
                    SCOUT_CHAIN
                } else {
                    SCOUT_CHAIN / 2
                }
            }
        };
        if use_scouts && stride > 0 {
            let h8 = hash8(data, i);
            let mut p = scout_head[h8];
            let mut walked = 0;
            while p > floor && walked < depth.max(1) {
                let j = p as usize;
                consider(&mut best_len, &mut best_dist, data, i, j);
                if best_len as usize == MAX_MATCH {
                    break;
                }
                p = scout_prev[j / stride];
                walked += 1;
            }
        }
    }
    (best_dist, best_len)
}

fn insert(data: &[u8], i: usize, head: &mut [i32], prev: &mut [i32]) {
    if i + 3 >= data.len() {
        return;
    }
    let h = hash4(data[i], data[i + 1], data[i + 2], data[i + 3]);
    prev[i] = head[h];
    head[h] = i as i32;
}

fn bits_lit() -> u32 { 10 }
fn len_bits(len: u32) -> u32 {
    let extra = len.saturating_sub(MIN_MATCH as u32);
    if extra < 8 {
        4
    } else if extra < 16 {
        6
    } else if extra < 31 {
        10
    } else {
        26
    }
}
fn bits_match_rep(len: u32) -> u32 {
    1 + 1 + 2 + len_bits(len)
}
fn bits_match(dist: u32, len: u32) -> u32 {
    let slot = 32u32.saturating_sub(dist.max(1).leading_zeros());
    // Far slots cost more so DP prefers rep0 / near.
    1 + 1 + len_bits(len) + 5 + slot.saturating_sub(1)
}

fn lens_to_try(best: u32) -> Vec<u32> {
    let mut v = Vec::new();
    if best < MIN_MATCH as u32 { return v; }
    if best <= 32 {
        for l in MIN_MATCH as u32..=best { v.push(l); }
    } else {
        for l in MIN_MATCH as u32..=8 { v.push(l); }
        for &k in &[12u32, 16, 24, 32, 48, 64, 128, 256, 512, 1024, 4096, 16384] {
            if k < best { v.push(k); }
        }
        if best > 1 + MIN_MATCH as u32 { v.push(best - 1); }
        v.push(best);
        v.sort_unstable();
        v.dedup();
    }
    v
}

fn parse_priced(_data: &[u8], _window: usize) -> Vec<Tok> {
    Vec::new()
}

pub fn parse(data: &[u8], window: usize) -> Vec<Tok> {
    parse_class(data, window, crate::detect::classify(data))
}

pub fn parse_class(data: &[u8], window: usize, class: crate::detect::Class) -> Vec<Tok> {
    let n = data.len();
    if n == 0 { return Vec::new(); }
    // Daily dial: LBR1_PARSE=lazy|hc4 forces hash-chain lazy finder even on large files.
    // Measure dial: LBR1_PARSE=lz4t|tag1 selects LZ4-class tagged/single-slot find
    // (no chain table). LBR1_PARSE=lz4t2|tag1x = lz4t + ≤1 alternate on short tag hits.
    // Default unset keeps max/PCC BT4 path. Does not change bitstream.
    // Dial A ship defaults (hc4 / W=1MiB / CHAIN=8 / HASH=17) stay unchanged.
    match std::env::var("LBR1_PARSE").ok().as_deref() {
        Some("lazy") | Some("hc4") => return parse_lazy(data, window),
        Some("lz4t") | Some("tag1") => return parse_lz4t(data, window),
        Some("lz4t2") | Some("tag1x") => return parse_lz4t2(data, window),
        _ => {}
    }
    // Full DP tables are O(n) and OOM on mozilla in 2 GB. Stream instead.
    // Large binaries: BT4 + 4-rep DP (own pathway, 4 MiB window).
    if n > 3 * 1024 * 1024 {
        return parse_rep4::parse_rep4(data, window);
    }
    let mut head = vec![-1i32; HASH_SIZE];
    let mut prev = vec![-1i32; n];
    let stride = crate::detect::scout_stride_class(class).max(1);
    let use_scouts = crate::detect::scouts_wanted_class(class);
    let nscout = n / stride + 1;
    let mut scout_head = vec![-1i32; SCOUT_SIZE];
    let mut scout_prev = vec![-1i32; nscout];
    let mut best_d = vec![0u32; n];
    let mut best_l = vec![0u32; n];
    for i in 0..n {
        if i + MIN_MATCH <= n {
            let (d, l) = find_match(
                data,
                i,
                window,
                &head,
                &prev,
                &scout_head,
                &scout_prev,
                stride,
                use_scouts,
            );
            best_d[i] = d;
            best_l[i] = l;
        }
        insert(data, i, &mut head, &mut prev);
        if use_scouts && i % stride == 0 && i + 8 <= n {
            let h8 = hash8(data, i);
            let si = i / stride;
            scout_prev[si] = scout_head[h8];
            scout_head[h8] = i as i32;
        }
    }
    let inf = u32::MAX / 4;
    let mut price = vec![inf; n + 1];
    let mut come = vec![0i32; n + 1];
    let mut come_d = vec![0u32; n + 1];
    price[0] = 0;
    for i in 0..n {
        if price[i] == inf { continue; }
        let pl = price[i].saturating_add(bits_lit());
        if pl < price[i + 1] {
            price[i + 1] = pl;
            come[i + 1] = -1;
        }
        if best_l[i] >= MIN_MATCH as u32 {
            for len in lens_to_try(best_l[i]) {
                if i + len as usize > n { continue; }
                let pm = price[i].saturating_add(bits_match(best_d[i], len));
                let j = i + len as usize;
                if pm < price[j] {
                    price[j] = pm;
                    come[j] = len as i32;
                    come_d[j] = best_d[i];
                }
            }
        }
    }
    let mut toks_rev = Vec::new();
    let mut i = n;
    while i > 0 {
        if come[i] < 0 {
            toks_rev.push(Tok::Lit(data[i - 1]));
            i -= 1;
        } else {
            let len = come[i] as u32;
            toks_rev.push(Tok::Match { dist: come_d[i], len });
            i -= len as usize;
        }
    }
    toks_rev.reverse();
    toks_rev
}

const BLOCK: usize = 256 * 1024;

#[inline(never)]
pub fn parse_block_dp(data: &[u8], window: usize) -> Vec<Tok> {
    let n = data.len();
    let win = window.max(256).min(n);
    let bits = block_hash_bits();
    let hs = 1usize << bits;
    let shift = 32 - bits;
    let mut head = vec![-1i32; hs];
    let mut prevc = vec![-1i32; n];
    let h20 = |p: usize| -> usize {
        if p + 4 > n { return 0; }
        let v = u32::from_le_bytes(data[p..p + 4].try_into().unwrap());
        ((v.wrapping_mul(0x85EB_CA6B) >> shift) as usize) & (hs - 1)
    };
    let use_scouts = crate::detect::scouts_wanted(data);
    let stride = crate::detect::scout_stride(data).max(1);
    let mut sensors = crate::sensors::Sensors::new(n, stride);
    let mut toks = Vec::new();
    let mut file_last = 0u32;
    let mut pos = 0usize;
    while pos < n {
        let end = (pos + BLOCK).min(n);
        let m = end - pos;
        let mut best_d = vec![0u32; m];
        let mut best_l = vec![0u32; m];
        for k in 0..m {
            let i = pos + k;
            let mut bd = 0u32;
            let mut bl = 0u32;
            if i + MIN_MATCH <= n {
                let floor = if i > win { (i - win) as i32 } else { -1 };
                let h = h20(i);
                let zero = data[i] | data[i + 1] | data[i + 2] | data[i + 3] == 0;
                let cap = block_chain_cap(zero);
                let mut p = head[h];
                let mut steps = 0;
                while p > floor && steps < cap {
                    let j = p as usize;
                    if j < i {
                        consider(&mut bl, &mut bd, data, i, j);
                        if bl as usize >= 4096 { break; }
                    }
                    p = prevc[j];
                    steps += 1;
                }
                if use_scouts {
                    for r in sensors.reports(data, i) {
                        if (r.pos as i32) > floor && r.pos < i {
                            consider(&mut bl, &mut bd, data, i, r.pos);
                        }
                    }
                }
            }
            best_d[k] = bd;
            best_l[k] = bl;
            if i + 3 < n {
                let h = h20(i);
                prevc[i] = head[h];
                head[h] = i as i32;
                if use_scouts { sensors.plant(data, i); }
            }
        }
        let inf = u32::MAX / 4;
        let mut price = vec![inf; m + 1];
        let mut come = vec![-1i32; m + 1];
        let mut come_d = vec![0u32; m + 1];
        let mut last_at = vec![0u32; m + 1];
        if pos == 0 {
            lbr1_price::price_reset_c();
        }
        unsafe {
            lbr1_price::price_block_c(
                price.as_mut_ptr(),
                come.as_mut_ptr(),
                come_d.as_mut_ptr(),
                last_at.as_mut_ptr(),
                data.as_ptr(),
                n,
                pos,
                m,
                best_d.as_ptr(),
                best_l.as_ptr(),
                file_last,
            );
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
                local.push(Tok::Match { dist: come_d[k], len });
                k -= len as usize;
            }
        }
        local.reverse();
        if let Some(Tok::Match { dist, .. }) = local.iter().rev().find(|t| matches!(t, Tok::Match { .. })) {
            if *dist != 0 { file_last = *dist; }
        }
        toks.extend(local);
        pos = end;
    }
    let mut last = 0u32;
    for t in &mut toks {
        if let Tok::Match { dist, .. } = t {
            if *dist != 0 && *dist == last { *dist = 0; }
            else if *dist != 0 { last = *dist; }
            else if last == 0 { *dist = 1; }
        }
    }
    toks
}

/// Whole-file greedy + lazy + last-offset. W = n. 20-bit hash.
pub fn parse_wn(data: &[u8]) -> Vec<Tok> {
    const HS: usize = 1 << 20;
    let n = data.len();
    let mut head = vec![-1i32; HS];
    let mut prev = vec![-1i32; n];
    let mut toks = Vec::new();
    let mut i = 0usize;
    let mut last = 0u32;
    let h4 = |p: usize| -> usize {
        let v = u32::from_le_bytes(data[p..p + 4].try_into().unwrap());
        (v.wrapping_mul(0x85EB_CA6B) >> 12) as usize
    };
    while i < n {
        if i + MIN_MATCH > n {
            toks.push(Tok::Lit(data[i]));
            i += 1;
            continue;
        }
        let h = h4(i);
        let zero = data[i] | data[i + 1] | data[i + 2] | data[i + 3] == 0;
        let lim = if zero { 4 } else { 32 };
        let mut best_l = 0usize;
        let mut best_d = 0u32;
        let mut mp = head[h];
        let mut c = 0;
        while mp >= 0 && c < lim {
            let j = mp as usize;
            if j >= i {
                break;
            }
            let d = (i - j) as u32;
            let L = match_len(data, i, j, MAX_MATCH);
            if L >= MIN_MATCH && L > best_l {
                best_l = L;
                best_d = d;
                if L >= 64 {
                    break;
                }
            }
            mp = prev[j];
            c += 1;
        }
        if best_l >= MIN_MATCH && i + 1 + MIN_MATCH <= n {
            let h2 = h4(i + 1);
            let mp2 = head[h2];
            if mp2 >= 0 && (mp2 as usize) < i + 1 {
                let L2 = match_len(data, i + 1, mp2 as usize, 64);
                if L2 > best_l + 1 {
                    best_l = 0;
                }
            }
        }
        if best_l >= MIN_MATCH && last != 0 && (last as usize) <= i {
            let lr = match_len(data, i, i - last as usize, best_l.max(MIN_MATCH));
            if lr >= MIN_MATCH && lr + 1 >= best_l {
                best_l = lr;
                best_d = last;
            }
        }
        if best_l >= MIN_MATCH {
            toks.push(Tok::Match {
                dist: best_d,
                len: best_l as u32,
            });
            last = best_d;
            for k in 0..best_l {
                if i + k + 4 <= n {
                    let hh = h4(i + k);
                    prev[i + k] = head[hh];
                    head[hh] = (i + k) as i32;
                }
            }
            i += best_l;
        } else {
            toks.push(Tok::Lit(data[i]));
            prev[i] = head[h];
            head[h] = i as i32;
            i += 1;
        }
    }
    toks
}

/// O(window) memory. Lazy match. Used for large binaries.
pub fn parse_lazy(data: &[u8], window: usize) -> Vec<Tok> {
    // PCC Dial C probe (Gale-shaped shallow find) — Dial A defaults remain ship:
    //   hc4 · W=1MiB · CHAIN=8 · LAZY=0 · PACK=ml4 · HASH=17 · INSERT=dense · FIND=price
    // Prefer Dial C (HASH=16 INSERT=ends) FAIL_LOUD vs zstd-9 on mozilla — see bench/pcc-dial-c-fail-loud.md.
    // Not Gale's 23M LZ4 wire. ANS waits. Face PCC. AWARE = legacy alias only.
    //   LBR1_CHAIN=1..256   — max hash-chain probes (default 8)
    //   LBR1_HASH=16..22    — parse_lazy hash bits (default 17 = Dial A)
    //   LBR1_INSERT=ends|stride4|dense|gale — match-body insert density (default dense)
    //   LBR1_FIND=price|gale — bit-priced vs longest-wins probe (default price)
    //   LBR1_SCOUTS=0|1     — disable scouts for shallower find (default: detect)
    //   LBR1_LAZY=0         — greedy (default); LBR1_LAZY=1 restore +1 lookahead
    //   LBR1_WINDOW=…       — via detect::window_for / env (Dial A/C: 1048576)
    let n = data.len();
    let win = window.max(256).next_power_of_two();
    let mask = win - 1;
    let chain_cap = env_usize("LBR1_CHAIN", 8, 1, 256);
    let hash_bits = env_usize("LBR1_HASH", 17, 16, 22) as u32;
    let hash_size = 1usize << hash_bits;
    let insert_mode = std::env::var("LBR1_INSERT")
        .unwrap_or_else(|_| "dense".into())
        .to_ascii_lowercase();
    let do_lazy = std::env::var("LBR1_LAZY")
        .ok()
        .map(|s| s != "0" && s != "false" && s != "off")
        .unwrap_or(false);
    let mut head = vec![-1i32; hash_size];
    let mut chain = vec![-1i32; win];
    let mut scout_head = vec![-1i32; SCOUT_SIZE];
    let find_mode = std::env::var("LBR1_FIND")
        .unwrap_or_else(|_| "price".into())
        .to_ascii_lowercase();
    let use_scouts = match std::env::var("LBR1_SCOUTS").ok().as_deref() {
        Some("0") | Some("false") | Some("off") => false,
        Some("1") | Some("true") | Some("on") => true,
        _ => crate::detect::scouts_wanted(data),
    };
    let stride = crate::detect::scout_stride(data).max(1);
    let mut toks = Vec::new();
    let mut i = 0usize;

    let hash_lazy = |a: u8, b: u8, c: u8, d: u8| -> usize {
        let v = u32::from_le_bytes([a, b, c, d]);
        (v.wrapping_mul(0x9E3779B1) >> (32 - hash_bits)) as usize
    };

    let insert = |head: &mut [i32], chain: &mut [i32], scout_head: &mut [i32], pos: usize| {
        if pos + 3 >= n {
            return;
        }
        let h = hash_lazy(data[pos], data[pos + 1], data[pos + 2], data[pos + 3]);
        chain[pos & mask] = head[h];
        head[h] = pos as i32;
        if use_scouts && pos % stride == 0 && pos + 8 <= n {
            scout_head[hash8(data, pos)] = pos as i32;
        }
    };

    let find = |head: &[i32], chain: &[i32], scout_head: &[i32], pos: usize| -> (u32, u32) {
        if pos + MIN_MATCH > n {
            return (0, 0);
        }
        let floor = if pos > win { (pos - win) as i32 } else { -1 };
        let mut best_l = 0u32;
        let mut best_d = 0u32;
        let h = hash_lazy(data[pos], data[pos + 1], data[pos + 2], data[pos + 3]);
        let mut p = head[h];
        let mut steps = 0;
        while p > floor && steps < chain_cap {
            let j = p as usize;
            if j < pos {
                if find_mode == "gale" || find_mode == "longest" {
                    // Gale-shaped: first-byte filter + longest match wins (no bit price).
                    if data[j] == data[pos]
                        && data[j + 1] == data[pos + 1]
                        && data[j + 2] == data[pos + 2]
                        && data[j + 3] == data[pos + 3]
                    {
                        let m = match_len(data, pos, j, MAX_MATCH) as u32;
                        if m >= MIN_MATCH as u32 && m > best_l {
                            best_l = m;
                            best_d = (pos - j) as u32;
                        }
                    }
                } else {
                    consider(&mut best_l, &mut best_d, data, pos, j);
                }
                if best_l as usize == MAX_MATCH {
                    break;
                }
            }
            p = chain[j & mask];
            steps += 1;
        }
        if use_scouts && pos + 8 <= n && best_l < 32 {
            let sp = scout_head[hash8(data, pos)];
            if sp > floor && (sp as usize) < pos {
                if find_mode == "gale" || find_mode == "longest" {
                    let j = sp as usize;
                    if data[j] == data[pos]
                        && data[j + 1] == data[pos + 1]
                        && data[j + 2] == data[pos + 2]
                        && data[j + 3] == data[pos + 3]
                    {
                        let m = match_len(data, pos, j, MAX_MATCH) as u32;
                        if m >= MIN_MATCH as u32 && m > best_l {
                            best_l = m;
                            best_d = (pos - j) as u32;
                        }
                    }
                } else {
                    consider(&mut best_l, &mut best_d, data, pos, sp as usize);
                }
            }
        }
        (best_d, best_l)
    };

    while i < n {
        let (d0, l0) = find(&head, &chain, &scout_head, i);
        if l0 >= MIN_MATCH as u32 {
            if do_lazy {
                let (d1, l1) = find(&head, &chain, &scout_head, i + 1);
                let take_lazy = l1 >= MIN_MATCH as u32
                    && bits_saved(l1, d1.max(1)) > bits_saved(l0, d0.max(1)) + 8;
                if take_lazy {
                    toks.push(Tok::Lit(data[i]));
                    insert(&mut head, &mut chain, &mut scout_head, i);
                    i += 1;
                    continue;
                }
            }
            toks.push(Tok::Match { dist: d0, len: l0 });
            let end = i + l0 as usize;
            match insert_mode.as_str() {
                // Gale-class shallow: only seed endpoints so later finds still see the run.
                "ends" | "end" | "endpoint" | "endpoints" => {
                    insert(&mut head, &mut chain, &mut scout_head, i);
                    if end > i + 1 {
                        let last = end.saturating_sub(MIN_MATCH);
                        if last > i {
                            insert(&mut head, &mut chain, &mut scout_head, last);
                        }
                    }
                }
                // Always stride-4 through the match body (fewer than dense short matches).
                "stride4" | "s4" | "4" => {
                    let mut p = i;
                    while p < end {
                        insert(&mut head, &mut chain, &mut scout_head, p);
                        p += 4;
                    }
                }
                // Gale matcher.c: insert every byte in [i, end).
                "gale" | "all" => {
                    let mut p = i;
                    while p < end {
                        insert(&mut head, &mut chain, &mut scout_head, p);
                        p += 1;
                    }
                }
                // Dial A dense: every byte; stride-4 only when match ≥ 64.
                _ => {
                    let mut p = i;
                    while p < end {
                        insert(&mut head, &mut chain, &mut scout_head, p);
                        p += if l0 >= 64 { 4 } else { 1 };
                    }
                }
            }
            i = end;
        } else {
            toks.push(Tok::Lit(data[i]));
            insert(&mut head, &mut chain, &mut scout_head, i);
            i += 1;
        }
    }
    toks
}

/// Measure-only find class: LZ4-class **tagged / single-slot** hash.
///
/// Select with `LBR1_PARSE=lz4t` (alias `tag1`). Same WINDOW / Tok emit / PACK=ml4
/// path as `parse_lazy`; **does not** retarget Dial A defaults (`hc4` / CHAIN=8).
///
/// Structure: one head entry per hash bucket (overwrite on insert) plus a 16-bit
/// tag fingerprint. Aimed at cutting Dial A false-candidate tax (~5 probes/find,
/// ~64% early-reject; chain+verify ≈65% of find) by eliminating the chain walk.
/// Size vs zstd-9 is Kernel bake territory — this dial is measure-only, not ship.
pub fn parse_lz4t(data: &[u8], window: usize) -> Vec<Tok> {
    // Env (measure dial; Dial A hc4 path untouched):
    //   LBR1_PARSE=lz4t|tag1  — select this finder
    //   LBR1_WINDOW=…         — via detect::window_for / env (Dial A bake: 1048576)
    //   LBR1_LAZY=0           — greedy (default); LBR1_LAZY=1 enables +1 lookahead
    // No LBR1_CHAIN: single-slot has no chain table.
    let n = data.len();
    let win = window.max(256).next_power_of_two();
    let do_lazy = std::env::var("LBR1_LAZY")
        .ok()
        .map(|s| s != "0" && s != "false" && s != "off")
        .unwrap_or(false);
    let mut head = vec![-1i32; HASH_SIZE];
    let mut tags = vec![0u16; HASH_SIZE];
    let mut scout_head = vec![-1i32; SCOUT_SIZE];
    let use_scouts = crate::detect::scouts_wanted(data);
    let stride = crate::detect::scout_stride(data).max(1);
    let mut toks = Vec::new();
    let mut i = 0usize;

    let insert = |head: &mut [i32], tags: &mut [u16], scout_head: &mut [i32], pos: usize| {
        if pos + 3 >= n {
            return;
        }
        let (h, tag) = hash4_tag(data[pos], data[pos + 1], data[pos + 2], data[pos + 3]);
        head[h] = pos as i32;
        tags[h] = tag;
        if use_scouts && pos % stride == 0 && pos + 8 <= n {
            scout_head[hash8(data, pos)] = pos as i32;
        }
    };

    let find = |head: &[i32], tags: &[u16], scout_head: &[i32], pos: usize| -> (u32, u32) {
        if pos + MIN_MATCH > n {
            return (0, 0);
        }
        let floor = if pos > win { (pos - win) as i32 } else { -1 };
        let mut best_l = 0u32;
        let mut best_d = 0u32;
        let (h, tag) = hash4_tag(data[pos], data[pos + 1], data[pos + 2], data[pos + 3]);
        let p = head[h];
        // Single slot: at most one candidate. Tag mismatch → reject without
        // loading the candidate's 4 bytes (false-candidate cut).
        if p > floor && (p as usize) < pos && tags[h] == tag {
            consider(&mut best_l, &mut best_d, data, pos, p as usize);
        }
        if use_scouts && pos + 8 <= n && best_l < 32 {
            let sp = scout_head[hash8(data, pos)];
            if sp > floor && (sp as usize) < pos {
                consider(&mut best_l, &mut best_d, data, pos, sp as usize);
            }
        }
        (best_d, best_l)
    };

    while i < n {
        let (d0, l0) = find(&head, &tags, &scout_head, i);
        if l0 >= MIN_MATCH as u32 {
            if do_lazy {
                let (d1, l1) = find(&head, &tags, &scout_head, i + 1);
                let take_lazy = l1 >= MIN_MATCH as u32
                    && bits_saved(l1, d1.max(1)) > bits_saved(l0, d0.max(1)) + 8;
                if take_lazy {
                    toks.push(Tok::Lit(data[i]));
                    insert(&mut head, &mut tags, &mut scout_head, i);
                    i += 1;
                    continue;
                }
            }
            toks.push(Tok::Match { dist: d0, len: l0 });
            let end = i + l0 as usize;
            let mut p = i;
            while p < end {
                insert(&mut head, &mut tags, &mut scout_head, p);
                p += if l0 >= 64 { 4 } else { 1 };
            }
            i = end;
        } else {
            toks.push(Tok::Lit(data[i]));
            insert(&mut head, &mut tags, &mut scout_head, i);
            i += 1;
        }
    }
    toks
}

/// Measure-only find class: lz4t primary + **≤1 alternate** on short tag hits.
///
/// Select with `LBR1_PARSE=lz4t2` (alias `tag1x`). Sibling of `parse_lz4t` —
/// leaves lz4t / lazy / Dial A (`hc4`) untouched. No chain walk, no CHAIN=8,
/// no Dial C knobs.
///
/// Shape (Theory GREENLIGHT after #15 RED):
/// 1. Primary path stays tagged/single-slot (lz4t speed idea that cleared find~67).
/// 2. On primary **tag hit** with match `len < N` (N=**16**), probe **at most one**
///    alternate: the previous overwrite victim for that bucket (1-deep `prev` +
///    `prev_tags`). Not a chain; avg probes must stay ≪ ~5.
/// 3. Long primary hits (`len >= 16`) skip the alternate (keep speed).
/// 4. Primary tag miss → no alternate (false-candidate tax stays cut).
///
/// Size vs zstd-9 / Dial A is Kernel bake territory — measure-only, not ship.
pub fn parse_lz4t2(data: &[u8], window: usize) -> Vec<Tok> {
    // Env (measure dial; Dial A hc4 + lz4t paths untouched):
    //   LBR1_PARSE=lz4t2|tag1x  — select this finder
    //   LBR1_WINDOW=…           — via detect::window_for / env (Dial A bake: 1048576)
    //   LBR1_LAZY=0             — greedy (default); LBR1_LAZY=1 enables +1 lookahead
    // No LBR1_CHAIN: primary is single-slot; alternate is fixed ≤1 prev probe.
    // Short-hit threshold N=16: only tag hits with len < 16 take the alternate.
    const SHORT_N: u32 = 16;
    let n = data.len();
    let win = window.max(256).next_power_of_two();
    let do_lazy = std::env::var("LBR1_LAZY")
        .ok()
        .map(|s| s != "0" && s != "false" && s != "off")
        .unwrap_or(false);
    let mut head = vec![-1i32; HASH_SIZE];
    let mut tags = vec![0u16; HASH_SIZE];
    // 1-deep overwrite victim (not a chain): at most one alternate probe.
    let mut prev = vec![-1i32; HASH_SIZE];
    let mut prev_tags = vec![0u16; HASH_SIZE];
    let mut scout_head = vec![-1i32; SCOUT_SIZE];
    let use_scouts = crate::detect::scouts_wanted(data);
    let stride = crate::detect::scout_stride(data).max(1);
    let mut toks = Vec::new();
    let mut i = 0usize;

    let insert = |head: &mut [i32],
                  tags: &mut [u16],
                  prev: &mut [i32],
                  prev_tags: &mut [u16],
                  scout_head: &mut [i32],
                  pos: usize| {
        if pos + 3 >= n {
            return;
        }
        let (h, tag) = hash4_tag(data[pos], data[pos + 1], data[pos + 2], data[pos + 3]);
        let old = head[h];
        // Keep prior head as the sole alternate when overwriting a different slot.
        if old >= 0 && (old as usize) != pos {
            prev[h] = old;
            prev_tags[h] = tags[h];
        }
        head[h] = pos as i32;
        tags[h] = tag;
        if use_scouts && pos % stride == 0 && pos + 8 <= n {
            scout_head[hash8(data, pos)] = pos as i32;
        }
    };

    let find = |head: &[i32],
                tags: &[u16],
                prev: &[i32],
                prev_tags: &[u16],
                scout_head: &[i32],
                pos: usize|
     -> (u32, u32) {
        if pos + MIN_MATCH > n {
            return (0, 0);
        }
        let floor = if pos > win { (pos - win) as i32 } else { -1 };
        let mut best_l = 0u32;
        let mut best_d = 0u32;
        let (h, tag) = hash4_tag(data[pos], data[pos + 1], data[pos + 2], data[pos + 3]);
        let p = head[h];
        let mut primary_tag_hit = false;
        if p > floor && (p as usize) < pos && tags[h] == tag {
            primary_tag_hit = true;
            consider(&mut best_l, &mut best_d, data, pos, p as usize);
        }
        // ≤1 alternate: only on short primary tag hits (len < SHORT_N=16).
        // Not a chain walk — single prev overwrite victim with its own tag.
        if primary_tag_hit && best_l < SHORT_N {
            let pp = prev[h];
            if pp > floor && (pp as usize) < pos && prev_tags[h] == tag {
                consider(&mut best_l, &mut best_d, data, pos, pp as usize);
            }
        }
        if use_scouts && pos + 8 <= n && best_l < 32 {
            let sp = scout_head[hash8(data, pos)];
            if sp > floor && (sp as usize) < pos {
                consider(&mut best_l, &mut best_d, data, pos, sp as usize);
            }
        }
        (best_d, best_l)
    };

    while i < n {
        let (d0, l0) = find(&head, &tags, &prev, &prev_tags, &scout_head, i);
        if l0 >= MIN_MATCH as u32 {
            if do_lazy {
                let (d1, l1) = find(&head, &tags, &prev, &prev_tags, &scout_head, i + 1);
                let take_lazy = l1 >= MIN_MATCH as u32
                    && bits_saved(l1, d1.max(1)) > bits_saved(l0, d0.max(1)) + 8;
                if take_lazy {
                    toks.push(Tok::Lit(data[i]));
                    insert(
                        &mut head,
                        &mut tags,
                        &mut prev,
                        &mut prev_tags,
                        &mut scout_head,
                        i,
                    );
                    i += 1;
                    continue;
                }
            }
            toks.push(Tok::Match { dist: d0, len: l0 });
            let end = i + l0 as usize;
            let mut p = i;
            while p < end {
                insert(
                    &mut head,
                    &mut tags,
                    &mut prev,
                    &mut prev_tags,
                    &mut scout_head,
                    p,
                );
                p += if l0 >= 64 { 4 } else { 1 };
            }
            i = end;
        } else {
            toks.push(Tok::Lit(data[i]));
            insert(
                &mut head,
                &mut tags,
                &mut prev,
                &mut prev_tags,
                &mut scout_head,
                i,
            );
            i += 1;
        }
    }
    toks
}



pub fn expand(toks: &[Tok]) -> Result<Vec<u8>, &'static str> {
    let mut out = Vec::new();
    for t in toks {
        match *t {
            Tok::Lit(b) => out.push(b),
            Tok::Match { dist, len } => {
                let d = if dist == 0 {
                    /* filled by caller last; expand is last-unaware */
                    return Err("rep");
                } else {
                    dist as usize
                };
                let nlen = len as usize;
                if d == 0 || d > out.len() { return Err("dist"); }
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
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeats_collapse() {
        let mut s = Vec::new();
        s.extend_from_slice(b"ABCD");
        for _ in 0..50 { s.extend_from_slice(b"ABCD"); }
        let t = parse(&s, DEFAULT_WINDOW);
        assert_eq!(expand(&t).unwrap(), s);
        assert!(t.iter().any(|x| matches!(x, Tok::Match { .. })));
    }

    #[test]
    fn scout_sees_across_zero_sea() {
        let motif = b"QWERTYUIOPASDFGH";
        let mut s = vec![0u8; 3000];
        s.extend_from_slice(motif);
        s.extend_from_slice(&[0u8; 3000]);
        s.extend_from_slice(motif);
        let t = parse(&s, DEFAULT_WINDOW);
        assert_eq!(expand(&t).unwrap(), s);
        let long = t.iter().filter_map(|x| match *x {
            Tok::Match { len, dist } if dist > 16 => Some(len),
            _ => None,
        }).max().unwrap_or(0);
        assert!(long >= 8, "scout should land a far copy, max far len={long}");
    }

    #[test]
    fn lazy_roundtrip_motif() {
        let mut s = Vec::new();
        let m = b"QWERTYUIOPASDFGH";
        for _ in 0..200 {
            s.extend_from_slice(m);
            s.extend_from_slice(&[0u8; 32]);
        }
        let t = parse_wn(&s);
        let blob = crate::frame::pack(&t, s.len(), s.len() as u32, &s);
        let back = crate::decode_lbr1(&blob).expect("wn pack");
        assert_eq!(back, s);

        let t = parse_lazy(&s, DEFAULT_WINDOW);
        assert_eq!(expand(&t).unwrap(), s);
        assert!(t.iter().any(|x| matches!(x, Tok::Match { .. })));
    }

    #[test]
    fn lz4t_roundtrip_motif() {
        let mut s = Vec::new();
        let m = b"QWERTYUIOPASDFGH";
        for _ in 0..200 {
            s.extend_from_slice(m);
            s.extend_from_slice(&[0u8; 32]);
        }
        let t = parse_lz4t(&s, DEFAULT_WINDOW);
        assert_eq!(expand(&t).unwrap(), s);
        assert!(t.iter().any(|x| matches!(x, Tok::Match { .. })));
    }

    #[test]
    fn lz4t2_roundtrip_motif() {
        let mut s = Vec::new();
        let m = b"QWERTYUIOPASDFGH";
        for _ in 0..200 {
            s.extend_from_slice(m);
            s.extend_from_slice(&[0u8; 32]);
        }
        let t = parse_lz4t2(&s, DEFAULT_WINDOW);
        assert_eq!(expand(&t).unwrap(), s);
        assert!(t.iter().any(|x| matches!(x, Tok::Match { .. })));
    }
}
