//! AWARE pack peel — lossless MDL polyfit + `affine_i32` tighten.
//!
//! Inner wire (LBHX carries raw_len outside):
//! ```text
//! poly (model_id 0/1/2/4):
//!   model_id:u8 | deg:u8 | coeffs:(deg+1)*f64 LE | zero_residual_flag:u8
//!   | [zlib-9(i32 LE residuals) if flag==0]
//! affine_i32 (model_id 3):
//!   model_id:u8=3 | start:i64 LE | step:i64 LE | zero_residual_flag:u8
//!   | [zlib-9(i32 LE residuals) if flag==0]
//! ```
//! model_id: 0=const, 1=poly_d1, 2=poly_d2, 3=affine_i32, 4=poly_d3.
//! Headers (zero-residual): poly_d1 = 19 B; affine_i32 = 18 B. CI ≤ 21.

use crate::house;

pub const MODEL_CONST: u8 = 0;
pub const MODEL_POLY_D1: u8 = 1;
pub const MODEL_POLY_D2: u8 = 2;
pub const MODEL_AFFINE_I32: u8 = 3; // exact i64 start+step (MDL tighten)
pub const MODEL_POLY_D3: u8 = 4;

/// Pack v1 header size for deg-1 zero-residual (float64 coeffs).
pub const POLY_D1_ZERO_HEADER: usize = 19;
/// Exact affine header: model_id + start:i64 + step:i64 + flag = 18 B.
pub const AFFINE_I32_ZERO_HEADER: usize = 18;

fn model_id_for_deg(deg: u8) -> u8 {
    match deg {
        0 => MODEL_CONST,
        1 => MODEL_POLY_D1,
        2 => MODEL_POLY_D2,
        3 => MODEL_POLY_D3,
        _ => MODEL_POLY_D1,
    }
}

pub fn model_tag(model_id: u8) -> &'static str {
    match model_id {
        MODEL_CONST => "poly_d0",
        MODEL_POLY_D1 => "poly_d1",
        MODEL_POLY_D2 => "poly_d2",
        MODEL_AFFINE_I32 => "affine_i32",
        MODEL_POLY_D3 => "poly_d3",
        _ => "poly",
    }
}

pub fn is_poly(buf: &[u8]) -> bool {
    house::unwrap(buf)
        .map(|(k, _, _)| k == house::KIND_POLY)
        .unwrap_or(false)
}

/// Inner pack length (aware_bytes). None if not a poly LBHX blob.
pub fn aware_bytes(buf: &[u8]) -> Option<usize> {
    let (k, _, inner) = house::unwrap(buf).ok()?;
    if k != house::KIND_POLY {
        return None;
    }
    Some(inner.len())
}

fn parse_i32le(data: &[u8]) -> Option<Vec<i32>> {
    if data.len() < 4 || data.len() % 4 != 0 {
        return None;
    }
    let n = data.len() / 4;
    let mut v = Vec::with_capacity(n);
    for chunk in data.chunks_exact(4) {
        v.push(i32::from_le_bytes(chunk.try_into().unwrap()));
    }
    Some(v)
}

fn emit_i32le(vals: &[i32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vals.len() * 4);
    for &x in vals {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// Evaluate Σ c[k] * i^k in f64.
fn eval_poly(coeffs: &[f64], i: usize) -> f64 {
    let x = i as f64;
    let mut acc = 0.0;
    let mut p = 1.0;
    for &c in coeffs {
        acc += c * p;
        p *= x;
    }
    acc
}

fn predict_i32(coeffs: &[f64], i: usize) -> i32 {
    eval_poly(coeffs, i).round() as i32
}

/// Least-squares fit deg 0..=3 via normal equations (Vandermonde).
fn fit_poly(vals: &[i32], deg: usize) -> Option<Vec<f64>> {
    let n = vals.len();
    if n == 0 || deg > 3 || deg + 1 > n {
        return None;
    }
    let m = deg + 1;
    // A^T A (m×m) and A^T y
    let mut ata = vec![0.0f64; m * m];
    let mut aty = vec![0.0f64; m];
    for i in 0..n {
        let x = i as f64;
        let mut pows = vec![1.0f64; m];
        for k in 1..m {
            pows[k] = pows[k - 1] * x;
        }
        let y = vals[i] as f64;
        for r in 0..m {
            aty[r] += pows[r] * y;
            for c in 0..m {
                ata[r * m + c] += pows[r] * pows[c];
            }
        }
    }
    solve_symmetric(&mut ata, &mut aty, m)
}

/// Gaussian elimination on dense m×m system (ata | aty) → solution in aty.
fn solve_symmetric(ata: &mut [f64], aty: &mut [f64], m: usize) -> Option<Vec<f64>> {
    // Augment in place: forward elimination with partial pivot.
    for col in 0..m {
        let mut piv = col;
        let mut best = ata[col * m + col].abs();
        for r in (col + 1)..m {
            let v = ata[r * m + col].abs();
            if v > best {
                best = v;
                piv = r;
            }
        }
        if best < 1e-18 {
            return None;
        }
        if piv != col {
            for c in 0..m {
                ata.swap(col * m + c, piv * m + c);
            }
            aty.swap(col, piv);
        }
        let diag = ata[col * m + col];
        for r in (col + 1)..m {
            let f = ata[r * m + col] / diag;
            for c in col..m {
                ata[r * m + c] -= f * ata[col * m + c];
            }
            aty[r] -= f * aty[col];
        }
    }
    // Back substitution.
    let mut x = vec![0.0f64; m];
    for i in (0..m).rev() {
        let mut s = aty[i];
        for j in (i + 1)..m {
            s -= ata[i * m + j] * x[j];
        }
        let d = ata[i * m + i];
        if d.abs() < 1e-18 {
            return None;
        }
        x[i] = s / d;
    }
    Some(x)
}

fn residuals(vals: &[i32], coeffs: &[f64]) -> Vec<i32> {
    vals.iter()
        .enumerate()
        .map(|(i, &y)| y.wrapping_sub(predict_i32(coeffs, i)))
        .collect()
}

fn zlib9(bytes: &[u8]) -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::best());
    enc.write_all(bytes).expect("zlib write");
    enc.finish().expect("zlib finish")
}

fn unzlib(bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    let mut dec = ZlibDecoder::new(bytes);
    let mut out = Vec::new();
    dec.read_to_end(&mut out).map_err(|_| "poly zlib")?;
    Ok(out)
}

fn affine_pred(start: i64, step: i64, i: usize) -> i32 {
    (start.wrapping_add(step.wrapping_mul(i as i64))) as i32
}

fn affine_residuals(vals: &[i32], start: i64, step: i64) -> Vec<i32> {
    vals.iter()
        .enumerate()
        .map(|(i, &v)| v.wrapping_sub(affine_pred(start, step, i)))
        .collect()
}

/// Exact integer affine from first sample + first difference (MDL candidate).
fn fit_affine_i32(vals: &[i32]) -> Option<(i64, i64)> {
    if vals.is_empty() {
        return None;
    }
    let start = vals[0] as i64;
    let step = if vals.len() >= 2 {
        (vals[1] as i64).wrapping_sub(start)
    } else {
        0
    };
    Some((start, step))
}

/// Build affine_i32 inner pack.
pub fn pack_affine_inner(vals: &[i32], start: i64, step: i64) -> Vec<u8> {
    let res = affine_residuals(vals, start, step);
    let zero = res.iter().all(|&r| r == 0);
    let mut out = Vec::with_capacity(AFFINE_I32_ZERO_HEADER);
    out.push(MODEL_AFFINE_I32);
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&step.to_le_bytes());
    if zero {
        out.push(1);
    } else {
        out.push(0);
        let raw = emit_i32le(&res);
        out.extend_from_slice(&zlib9(&raw));
    }
    out
}

/// Build inner pack v1 blob for a chosen degree.
pub fn pack_inner(vals: &[i32], deg: u8, coeffs: &[f64]) -> Vec<u8> {
    debug_assert_eq!(coeffs.len(), deg as usize + 1);
    let res = residuals(vals, coeffs);
    let zero = res.iter().all(|&r| r == 0);
    let model_id = model_id_for_deg(deg);
    let mut out = Vec::with_capacity(POLY_D1_ZERO_HEADER);
    out.push(model_id);
    out.push(deg);
    for &c in coeffs {
        out.extend_from_slice(&c.to_le_bytes());
    }
    if zero {
        out.push(1);
    } else {
        out.push(0);
        let raw = emit_i32le(&res);
        out.extend_from_slice(&zlib9(&raw));
    }
    out
}

/// MDL pick among poly deg 0..=3 and affine_i32. Returns (inner, model_id, deg).
/// For affine_i32, `deg` is reported as 1 (linear).
pub fn mdl_pack(vals: &[i32]) -> Option<(Vec<u8>, u8, u8)> {
    if vals.is_empty() {
        return None;
    }
    let mut best: Option<(Vec<u8>, u8, u8)> = None;
    let consider = |best: &mut Option<(Vec<u8>, u8, u8)>, inner: Vec<u8>, mid: u8, deg: u8| {
        let take = match best {
            None => true,
            Some((b, _, _)) => inner.len() < b.len(),
        };
        if take {
            *best = Some((inner, mid, deg));
        }
    };
    if let Some((start, step)) = fit_affine_i32(vals) {
        let inner = pack_affine_inner(vals, start, step);
        consider(&mut best, inner, MODEL_AFFINE_I32, 1);
    }
    for deg in 0u8..=3 {
        let coeffs = fit_poly(vals, deg as usize)?;
        let inner = pack_inner(vals, deg, &coeffs);
        let mid = model_id_for_deg(deg);
        consider(&mut best, inner, mid, deg);
    }
    best
}

pub fn unpack_inner(inner: &[u8], n_vals: usize) -> Result<Vec<i32>, &'static str> {
    if inner.is_empty() {
        return Err("poly short");
    }
    let model_id = inner[0];
    if model_id == MODEL_AFFINE_I32 {
        // model_id | start:i64 | step:i64 | flag | [zlib]
        if inner.len() < AFFINE_I32_ZERO_HEADER {
            return Err("affine hdr");
        }
        let start = i64::from_le_bytes(inner[1..9].try_into().unwrap());
        let step = i64::from_le_bytes(inner[9..17].try_into().unwrap());
        let flag = inner[17];
        let mut off = 18;
        let res: Vec<i32> = if flag == 1 {
            if off != inner.len() {
                return Err("affine flag1 trailing");
            }
            vec![0i32; n_vals]
        } else {
            let inflated = unzlib(&inner[off..])?;
            if inflated.len() != n_vals * 4 {
                return Err("affine res len");
            }
            parse_i32le(&inflated).ok_or("affine res parse")?
        };
        if res.len() != n_vals {
            return Err("affine n");
        }
        let mut out = Vec::with_capacity(n_vals);
        for i in 0..n_vals {
            out.push(affine_pred(start, step, i).wrapping_add(res[i]));
        }
        return Ok(out);
    }
    if inner.len() < 3 {
        return Err("poly short");
    }
    let deg = inner[1] as usize;
    if deg > 3 {
        return Err("poly deg");
    }
    let coeff_bytes = 8 * (deg + 1);
    let hdr = 2 + coeff_bytes + 1;
    if inner.len() < hdr {
        return Err("poly hdr");
    }
    let mut coeffs = Vec::with_capacity(deg + 1);
    let mut off = 2;
    for _ in 0..=deg {
        let mut b = [0u8; 8];
        b.copy_from_slice(&inner[off..off + 8]);
        coeffs.push(f64::from_le_bytes(b));
        off += 8;
    }
    let flag = inner[off];
    off += 1;
    let res: Vec<i32> = if flag == 1 {
        if off != inner.len() {
            return Err("poly flag1 trailing");
        }
        vec![0i32; n_vals]
    } else {
        let inflated = unzlib(&inner[off..])?;
        if inflated.len() != n_vals * 4 {
            return Err("poly res len");
        }
        parse_i32le(&inflated).ok_or("poly res parse")?
    };
    if res.len() != n_vals {
        return Err("poly n");
    }
    let mut out = Vec::with_capacity(n_vals);
    for i in 0..n_vals {
        out.push(predict_i32(&coeffs, i).wrapping_add(res[i]));
    }
    Ok(out)
}

/// Encode as LBHX KIND_POLY. Wins only if roundtrip-ok and strictly smaller.
pub fn encode(data: &[u8]) -> Option<(Vec<u8>, &'static str)> {
    let vals = parse_i32le(data)?;
    if vals.len() < 2 {
        return None;
    }
    let (inner, model_id, _deg) = mdl_pack(&vals)?;
    let tag = model_tag(model_id);
    let blob = house::wrap(house::KIND_POLY, data.len() as u32, &inner);
    if blob.len() >= data.len() {
        return None;
    }
    match decode(&blob) {
        Ok(back) if back == data => Some((blob, tag)),
        _ => None,
    }
}

pub fn decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    let (kind, raw_len, inner) = house::unwrap(buf)?;
    if kind != house::KIND_POLY {
        return Err("not poly");
    }
    if raw_len % 4 != 0 {
        return Err("poly raw_len");
    }
    let n = (raw_len / 4) as usize;
    let vals = unpack_inner(inner, n)?;
    let out = emit_i32le(&vals);
    if out.len() as u32 != raw_len {
        return Err("poly out len");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aware;

    fn int_ramp_256k() -> Vec<u8> {
        let mut raw = Vec::with_capacity(262_144);
        for i in 0..65536u32 {
            raw.extend_from_slice(&(i as i32).to_le_bytes());
        }
        raw
    }

    #[test]
    fn int_ramp_256k_leq_21_roundtrip() {
        let raw = int_ramp_256k();
        assert_eq!(raw.len(), 262_144);
        let (blob, tag) = encode(&raw).expect("poly encode");
        assert_eq!(tag, "affine_i32", "exact ramp must pick affine_i32 over float poly_d1");
        let ab = aware_bytes(&blob).expect("aware_bytes");
        assert!(
            ab <= 21,
            "aware_bytes={ab} want ≤21"
        );
        assert_eq!(ab, AFFINE_I32_ZERO_HEADER, "zero-residual affine_i32 must be 18 B");
        assert_eq!(decode(&blob).unwrap(), raw);
        assert!(ab < 3546, "must beat hosted 3546 B");
    }

    #[test]
    fn zeros_i32_const() {
        let raw = vec![0u8; 1024]; // 256 i32 zeros
        let (blob, tag) = encode(&raw).expect("zeros");
        assert!(tag == "poly_d0" || tag == "poly_d1");
        let ab = aware_bytes(&blob).unwrap();
        assert!(ab <= 21, "zeros aware_bytes={ab}");
        assert_eq!(decode(&blob).unwrap(), raw);
    }

    #[test]
    fn simple_ramp_small() {
        let mut raw = Vec::new();
        for i in 0..64i32 {
            raw.extend_from_slice(&(i * 3 + 7).to_le_bytes());
        }
        let (blob, tag) = encode(&raw).expect("ramp");
        assert_eq!(tag, "affine_i32");
        assert_eq!(decode(&blob).unwrap(), raw);
        assert_eq!(aware_bytes(&blob).unwrap(), AFFINE_I32_ZERO_HEADER);
    }

    #[test]
    fn encode_best_picks_affine_i32() {
        let raw = int_ramp_256k();
        let (blob, tag) = crate::encode_best(&raw).expect("house");
        assert_eq!(tag, "affine_i32");
        assert!(is_poly(&blob));
        assert_eq!(aware_bytes(&blob).unwrap(), AFFINE_I32_ZERO_HEADER);
        assert_eq!(crate::decode(&blob).unwrap(), raw);
    }

    #[test]
    fn xz1_still_retired() {
        assert!(aware::is_retired_wrap_slot("mozilla"));
        assert!(aware::is_retired_wrap_slot("samba"));
        assert!(aware::is_retired_wrap_slot("sao"));
        assert!(aware::is_retired_wrap_slot("ooffice"));
    }

    #[test]
    fn affine_beats_poly_d1_on_exact_ramp() {
        let raw = int_ramp_256k();
        let vals = parse_i32le(&raw).unwrap();
        let (start, step) = fit_affine_i32(&vals).unwrap();
        assert_eq!((start, step), (0, 1));
        let aff = pack_affine_inner(&vals, start, step);
        let poly = {
            let coeffs = fit_poly(&vals, 1).unwrap();
            pack_inner(&vals, 1, &coeffs)
        };
        assert_eq!(aff.len(), AFFINE_I32_ZERO_HEADER);
        assert_eq!(poly.len(), POLY_D1_ZERO_HEADER);
        assert!(aff.len() < poly.len());
    }

    #[test]
    fn noisy_ramp_roundtrip_with_residuals() {
        let mut vals: Vec<i32> = (0..128).collect();
        vals[10] += 5;
        vals[50] -= 3;
        let raw = emit_i32le(&vals);
        let (blob, _) = encode(&raw).expect("noisy");
        assert_eq!(decode(&blob).unwrap(), raw);
        // Non-zero residuals → flag 0 → larger than zero-residual affine header.
        assert!(aware_bytes(&blob).unwrap() > AFFINE_I32_ZERO_HEADER);
    }
}
