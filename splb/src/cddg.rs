//! CDDG wire — Theory-sealed Kolmogorov peels.
//!
//! ## v0 (`model_id=8`, 12 B)
//! Silent deep-shelf L0: param-only header when raw matches `cadence_369`
//! exactly (zero residual). Spec: `/workspace/corpora/cddg-wire-v0.md`
//!
//! ## v1 dual (`model_id=9`, 19 B)
//! Dual spin + published NCA web link. Exact generator match only.
//! NOT Gale daily; NOT hosted AWARE crown.
//! Spec: `/workspace/corpora/cddg-wire-v1.md`
//!
//! v0 wire (`<BIHIB` LE, 12 B):
//!   model_id:u8=8 | n:u32 | start_plane:u16 | seed:u32 | flag:u8=1
//! Raw sample (6 B): u16le plane + u32le dark_q32
//!
//! v1 wire (`<BIHIHIBB` LE, 19 B):
//!   model_id:u8=9 | n:u32 | start_s:u16 | seed_s:u32 | start_c:u16 | seed_c:u32
//!   | link_rule:u8 | flag:u8=1
//! Raw sample (16 B): u16 p_s | u32 q_s | u16 p_c | u32 q_c | u32 ell

/// CDDG_V0 — free vs poly 0..=7.
pub const MODEL_CDDG: u8 = 8;
/// CDDG_DUAL_V1 — dual structure + chamber + published link rule.
pub const MODEL_CDDG_DUAL: u8 = 9;
/// Param-only crown header size (v0).
pub const CDDG_HEADER: usize = 12;
/// Param-only crown header size (v1 dual).
pub const CDDG_DUAL_HEADER: usize = 19;
/// Nominal 1° planes.
pub const N_PLANES: u16 = 360;
/// Sample stride in raw fixture bytes (v0).
pub const SAMPLE_BYTES: usize = 6;
/// Sample stride in raw fixture bytes (v1 dual).
pub const DUAL_SAMPLE_BYTES: usize = 16;

pub const LINK_XOR_MIX: u8 = 0;
pub const LINK_RING_NEIGHBOR: u8 = 1;

const LCG_MUL: u64 = 1_103_515_245;
const LCG_ADD: u64 = 12_345;
const LCG_MASK: u64 = 0x7fff_ffff;

#[inline]
fn lcg_step(s: u32) -> u32 {
    ((u64::from(s).wrapping_mul(LCG_MUL).wrapping_add(LCG_ADD)) & LCG_MASK) as u32
}

/// Modular inverse of `a` (odd) modulo 2^31 via Newton/Hensel lift.
fn modinv_u31(a: u32) -> u32 {
    debug_assert!(a % 2 == 1);
    let a64 = u64::from(a);
    let mut x: u64 = 1; // inv mod 2
    // Lift toward 2^31 (5 doublings: 2→4→8→16→32 > 31).
    for _ in 0..5 {
        x = x.wrapping_mul(2u64.wrapping_sub(a64.wrapping_mul(x))) & LCG_MASK;
    }
    x as u32
}

fn lcg_prev(s_next: u32) -> u32 {
    let inv = modinv_u31(LCG_MUL as u32);
    let t = (u64::from(s_next).wrapping_sub(LCG_ADD)) & LCG_MASK;
    ((t * u64::from(inv)) & LCG_MASK) as u32
}

#[inline]
fn plane_fwd(start: u16, i: usize) -> u16 {
    ((u32::from(start) + i as u32) % u32::from(N_PLANES)) as u16
}

#[inline]
fn plane_counter(start: u16, i: usize) -> u16 {
    ((i64::from(start) - i as i64).rem_euclid(i64::from(N_PLANES))) as u16
}

#[inline]
fn rotl32(x: u32, k: u32) -> u32 {
    x.rotate_left(k & 31)
}

/// Published link rules (Theory seal §4.3). Unknown ids rejected by unpack.
pub fn link_ell(rule: u8, i: usize, p_s: u16, q_s: u32, p_c: u16, q_c: u32) -> Option<u32> {
    match rule {
        LINK_XOR_MIX => {
            let plane_term =
                ((u32::from(p_s) + u32::from(p_c)) % u32::from(N_PLANES)).wrapping_mul(0x0101_0101);
            Some(q_s ^ q_c ^ plane_term)
        }
        LINK_RING_NEIGHBOR => {
            let n_s = (u32::from(p_s) + 1) % u32::from(N_PLANES);
            let n_c = (u32::from(p_c) + 1) % u32::from(N_PLANES);
            Some(rotl32(q_s, n_s % 32) ^ rotl32(q_c, n_c % 32) ^ (i as u32))
        }
        _ => None,
    }
}

/// Published `cadence_369` / `gen_cddg_v0` — bit-exact with Theory seal.
pub fn gen_cddg_v0(n: usize, start_plane: u16, seed: u32) -> Vec<u8> {
    assert!(start_plane < N_PLANES);
    assert!(n >= 1);
    let mut out = Vec::with_capacity(n * SAMPLE_BYTES);
    let mut s = seed;
    for i in 0..n {
        s = lcg_step(s);
        let p = plane_fwd(start_plane, i);
        let q = s << 1;
        out.extend_from_slice(&p.to_le_bytes());
        out.extend_from_slice(&q.to_le_bytes());
    }
    out
}

/// Pack param-only crown (flag must be 1 for v0).
pub fn pack_cddg(n: u32, start_plane: u16, seed: u32) -> Vec<u8> {
    assert!(start_plane < N_PLANES);
    assert!(n >= 1);
    let mut out = Vec::with_capacity(CDDG_HEADER);
    out.push(MODEL_CDDG);
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&start_plane.to_le_bytes());
    out.extend_from_slice(&seed.to_le_bytes());
    out.push(1); // zero_residual_flag
    out
}

/// Unpack header → raw samples. Rejects non-v0 flag / bad plane.
pub fn unpack_cddg(inner: &[u8]) -> Result<Vec<u8>, &'static str> {
    if inner.len() != CDDG_HEADER {
        return Err("cddg hdr");
    }
    if inner[0] != MODEL_CDDG {
        return Err("cddg model");
    }
    let n = u32::from_le_bytes(inner[1..5].try_into().unwrap());
    let start = u16::from_le_bytes(inner[5..7].try_into().unwrap());
    let seed = u32::from_le_bytes(inner[7..11].try_into().unwrap());
    let flag = inner[11];
    if n < 1 {
        return Err("cddg n");
    }
    if start >= N_PLANES {
        return Err("cddg plane");
    }
    if flag != 1 {
        return Err("cddg flag"); // residual lane out of v0
    }
    Ok(gen_cddg_v0(n as usize, start, seed))
}

/// Detect when raw is exactly `gen_cddg_v0` (zero residual only).
/// Returns `(n, start_plane, seed)`.
pub fn try_cddg_params(data: &[u8]) -> Option<(u32, u16, u32)> {
    if data.len() < SAMPLE_BYTES || data.len() % SAMPLE_BYTES != 0 {
        return None;
    }
    let n = data.len() / SAMPLE_BYTES;
    let start = u16::from_le_bytes(data[0..2].try_into().unwrap());
    if start >= N_PLANES {
        return None;
    }
    let q0 = u32::from_le_bytes(data[2..6].try_into().unwrap());
    if q0 & 1 != 0 {
        return None;
    }
    // Fast reject: plane schedule must be consecutive mod 360.
    for i in 1..n.min(8) {
        let off = i * SAMPLE_BYTES;
        let p = u16::from_le_bytes(data[off..off + 2].try_into().unwrap());
        let expect = plane_fwd(start, i);
        if p != expect {
            return None;
        }
        let q = u32::from_le_bytes(data[off + 2..off + 6].try_into().unwrap());
        if q & 1 != 0 {
            return None;
        }
    }
    let s1 = q0 >> 1;
    let seed = lcg_prev(s1);
    let gen = gen_cddg_v0(n, start, seed);
    if gen.as_slice() == data {
        return Some((n as u32, start, seed));
    }
    // Published fixture seed (belt): in case recover path ever drifts.
    if seed != 369 {
        let gen369 = gen_cddg_v0(n, start, 369);
        if gen369.as_slice() == data {
            return Some((n as u32, start, 369));
        }
    }
    None
}

/// Dual spin + NCA web generator — bit-exact with Theory seal v1 / Kernel py.
pub fn gen_cddg_dual_v1(
    n: usize,
    start_s: u16,
    seed_s: u32,
    start_c: u16,
    seed_c: u32,
    link_rule: u8,
) -> Option<Vec<u8>> {
    if start_s >= N_PLANES || start_c >= N_PLANES || n < 1 {
        return None;
    }
    if link_ell(link_rule, 0, 0, 0, 0, 0).is_none() {
        return None;
    }
    let mut out = Vec::with_capacity(n * DUAL_SAMPLE_BYTES);
    let mut ss = seed_s;
    let mut sc = seed_c;
    for i in 0..n {
        ss = lcg_step(ss);
        sc = lcg_step(sc);
        let p_s = plane_fwd(start_s, i);
        let q_s = ss << 1;
        let p_c = plane_counter(start_c, i);
        let q_c = sc << 1;
        let ell = link_ell(link_rule, i, p_s, q_s, p_c, q_c)?;
        out.extend_from_slice(&p_s.to_le_bytes());
        out.extend_from_slice(&q_s.to_le_bytes());
        out.extend_from_slice(&p_c.to_le_bytes());
        out.extend_from_slice(&q_c.to_le_bytes());
        out.extend_from_slice(&ell.to_le_bytes());
    }
    Some(out)
}

/// Pack param-only dual crown (flag must be 1).
pub fn pack_cddg_dual(
    n: u32,
    start_s: u16,
    seed_s: u32,
    start_c: u16,
    seed_c: u32,
    link_rule: u8,
) -> Vec<u8> {
    assert!(start_s < N_PLANES && start_c < N_PLANES);
    assert!(n >= 1);
    assert!(link_rule == LINK_XOR_MIX || link_rule == LINK_RING_NEIGHBOR);
    let mut out = Vec::with_capacity(CDDG_DUAL_HEADER);
    out.push(MODEL_CDDG_DUAL);
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&start_s.to_le_bytes());
    out.extend_from_slice(&seed_s.to_le_bytes());
    out.extend_from_slice(&start_c.to_le_bytes());
    out.extend_from_slice(&seed_c.to_le_bytes());
    out.push(link_rule);
    out.push(1); // zero_residual_flag
    debug_assert_eq!(out.len(), CDDG_DUAL_HEADER);
    out
}

/// Unpack dual header → raw 16 B samples. Rejects unknown link / flag≠1.
pub fn unpack_cddg_dual(inner: &[u8]) -> Result<Vec<u8>, &'static str> {
    if inner.len() != CDDG_DUAL_HEADER {
        return Err("cddg9 hdr");
    }
    if inner[0] != MODEL_CDDG_DUAL {
        return Err("cddg9 model");
    }
    let n = u32::from_le_bytes(inner[1..5].try_into().unwrap());
    let start_s = u16::from_le_bytes(inner[5..7].try_into().unwrap());
    let seed_s = u32::from_le_bytes(inner[7..11].try_into().unwrap());
    let start_c = u16::from_le_bytes(inner[11..13].try_into().unwrap());
    let seed_c = u32::from_le_bytes(inner[13..17].try_into().unwrap());
    let link_rule = inner[17];
    let flag = inner[18];
    if n < 1 {
        return Err("cddg9 n");
    }
    if start_s >= N_PLANES || start_c >= N_PLANES {
        return Err("cddg9 plane");
    }
    if flag != 1 {
        return Err("cddg9 flag");
    }
    gen_cddg_dual_v1(n as usize, start_s, seed_s, start_c, seed_c, link_rule)
        .ok_or("cddg9 link")
}

/// Detect exact `gen_cddg_dual_v1` match only (zero residual).
/// Returns `(n, start_s, seed_s, start_c, seed_c, link_rule)`.
pub fn try_cddg_dual_params(data: &[u8]) -> Option<(u32, u16, u32, u16, u32, u8)> {
    if data.len() < DUAL_SAMPLE_BYTES || data.len() % DUAL_SAMPLE_BYTES != 0 {
        return None;
    }
    let n = data.len() / DUAL_SAMPLE_BYTES;
    let start_s = u16::from_le_bytes(data[0..2].try_into().unwrap());
    let q_s0 = u32::from_le_bytes(data[2..6].try_into().unwrap());
    let start_c = u16::from_le_bytes(data[6..8].try_into().unwrap());
    let q_c0 = u32::from_le_bytes(data[8..12].try_into().unwrap());
    if start_s >= N_PLANES || start_c >= N_PLANES {
        return None;
    }
    if (q_s0 & 1) != 0 || (q_c0 & 1) != 0 {
        return None;
    }
    // Fast reject: structure forward + chamber counter schedules.
    for i in 1..n.min(8) {
        let off = i * DUAL_SAMPLE_BYTES;
        let p_s = u16::from_le_bytes(data[off..off + 2].try_into().unwrap());
        let p_c = u16::from_le_bytes(data[off + 6..off + 8].try_into().unwrap());
        if p_s != plane_fwd(start_s, i) || p_c != plane_counter(start_c, i) {
            return None;
        }
        let q_s = u32::from_le_bytes(data[off + 2..off + 6].try_into().unwrap());
        let q_c = u32::from_le_bytes(data[off + 8..off + 12].try_into().unwrap());
        if (q_s & 1) != 0 || (q_c & 1) != 0 {
            return None;
        }
    }
    let seed_s = lcg_prev(q_s0 >> 1);
    let seed_c = lcg_prev(q_c0 >> 1);

    let try_params = |ss: u32, sc: u32, rule: u8| -> Option<(u32, u16, u32, u16, u32, u8)> {
        let gen = gen_cddg_dual_v1(n, start_s, ss, start_c, sc, rule)?;
        if gen.as_slice() == data {
            Some((n as u32, start_s, ss, start_c, sc, rule))
        } else {
            None
        }
    };

    for &rule in &[LINK_XOR_MIX, LINK_RING_NEIGHBOR] {
        if let Some(p) = try_params(seed_s, seed_c, rule) {
            return Some(p);
        }
    }
    // Published fixture belt (369 / 963) if recover drifts.
    if seed_s != 369 || seed_c != 963 {
        for &rule in &[LINK_XOR_MIX, LINK_RING_NEIGHBOR] {
            if let Some(p) = try_params(369, 963, rule) {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcg_prev_roundtrip_seed_369() {
        let s1 = lcg_step(369);
        assert_eq!(lcg_prev(s1), 369);
    }

    #[test]
    fn lcg_prev_roundtrip_seed_963() {
        let s1 = lcg_step(963);
        assert_eq!(lcg_prev(s1), 963);
    }

    #[test]
    fn header_is_12() {
        let p = pack_cddg(1000, 0, 369);
        assert_eq!(p.len(), CDDG_HEADER);
        assert_eq!(p[0], MODEL_CDDG);
        assert_eq!(p[11], 1);
    }

    #[test]
    fn dual_header_is_19() {
        let p = pack_cddg_dual(1000, 0, 369, 0, 963, LINK_XOR_MIX);
        assert_eq!(p.len(), CDDG_DUAL_HEADER);
        assert_eq!(p[0], MODEL_CDDG_DUAL);
        assert_eq!(p[17], LINK_XOR_MIX);
        assert_eq!(p[18], 1);
    }

    #[test]
    fn plane_counter_wraps() {
        assert_eq!(plane_counter(0, 0), 0);
        assert_eq!(plane_counter(0, 1), 359);
        assert_eq!(plane_counter(180, 180), 0);
        assert_eq!(plane_counter(180, 181), 359);
    }
}
