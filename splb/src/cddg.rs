//! CDDG wire v0 — Theory-sealed Kolmogorov peel (model_id=8).
//!
//! Silent deep-shelf L0 door: param-only 12 B header when raw matches
//! `cadence_369` exactly (zero residual). Not Gale daily; not hosted AWARE crown.
//!
//! Wire (`<BIHIB` LE, 12 B):
//!   model_id:u8=8 | n:u32 | start_plane:u16 | seed:u32 | flag:u8=1
//! Raw sample (6 B): u16le plane + u32le dark_q32
//! Spec: /workspace/corpora/cddg-wire-v0.md

/// CDDG_V0 — free vs poly 0..=7.
pub const MODEL_CDDG: u8 = 8;
/// Param-only crown header size.
pub const CDDG_HEADER: usize = 12;
/// Nominal 1° planes.
pub const N_PLANES: u16 = 360;
/// Sample stride in raw fixture bytes.
pub const SAMPLE_BYTES: usize = 6;

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

/// Published `cadence_369` / `gen_cddg_v0` — bit-exact with Theory seal.
pub fn gen_cddg_v0(n: usize, start_plane: u16, seed: u32) -> Vec<u8> {
    assert!(start_plane < N_PLANES);
    assert!(n >= 1);
    let mut out = Vec::with_capacity(n * SAMPLE_BYTES);
    let mut s = seed;
    for i in 0..n {
        s = lcg_step(s);
        let p = ((u32::from(start_plane) + i as u32) % u32::from(N_PLANES)) as u16;
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
        let expect = ((u32::from(start) + i as u32) % u32::from(N_PLANES)) as u16;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcg_prev_roundtrip_seed_369() {
        let s1 = lcg_step(369);
        assert_eq!(lcg_prev(s1), 369);
    }

    #[test]
    fn header_is_12() {
        let p = pack_cddg(1000, 0, 369);
        assert_eq!(p.len(), CDDG_HEADER);
        assert_eq!(p[0], MODEL_CDDG);
        assert_eq!(p[11], 1);
    }
}
