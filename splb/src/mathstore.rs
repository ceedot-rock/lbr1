//! Math store — when STORE would dump bytes, keep the shortest exact generator.
//!
//! Closed language. Decode is a table of ops, not eval. Lab-owned.
//! Kolmogorov idea: store the formula, not the tape. Not mzip, not CuNi-as-eval.

pub const MAGIC: &[u8; 4] = b"MTH1";

const ARITH_U8: u8 = 1;
const ARITH_U16: u8 = 2;
const ARITH_U32: u8 = 3;
const ARITH_U64: u8 = 4;
const CYCLE: u8 = 5;
const QUAD_U8: u8 = 6;

pub fn is_mth(buf: &[u8]) -> bool {
    buf.len() >= 6 && buf.starts_with(MAGIC)
}

fn put_u32(out: &mut Vec<u8>, n: u32) {
    out.extend_from_slice(&n.to_le_bytes());
}

fn take_u32(buf: &[u8], i: &mut usize) -> Result<u32, &'static str> {
    if *i + 4 > buf.len() {
        return Err("mth u32");
    }
    let n = u32::from_le_bytes(buf[*i..*i + 4].try_into().unwrap());
    *i += 4;
    Ok(n)
}

fn wrap(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut o = Vec::from(*MAGIC);
    o.push(kind);
    o.extend_from_slice(payload);
    o
}

fn try_arith_u8(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 16 {
        return None;
    }
    let a = data[0];
    let step = data[1].wrapping_sub(a);
    if step == 0 {
        return None;
    }
    for (i, &b) in data.iter().enumerate() {
        if b != a.wrapping_add((step as u32).wrapping_mul(i as u32) as u8) {
            return None;
        }
    }
    let mut p = vec![a, step];
    put_u32(&mut p, data.len() as u32);
    Some(wrap(ARITH_U8, &p))
}

fn try_arith_width<const W: usize>(data: &[u8], kind: u8) -> Option<Vec<u8>> {
    if data.len() < W * 8 || data.len() % W != 0 {
        return None;
    }
    let n = data.len() / W;
    let mut first = [0u8; 8];
    let mut second = [0u8; 8];
    first[..W].copy_from_slice(&data[..W]);
    second[..W].copy_from_slice(&data[W..W * 2]);
    let a = u64::from_le_bytes(first);
    let b = u64::from_le_bytes(second);
    let step = b.wrapping_sub(a);
    if step == 0 {
        return None;
    }
    for i in 0..n {
        let expect = a.wrapping_add(step.wrapping_mul(i as u64));
        let mut got = [0u8; 8];
        got[..W].copy_from_slice(&data[i * W..i * W + W]);
        if u64::from_le_bytes(got) != expect & mask(W) {
            return None;
        }
    }
    let mut p = Vec::from(&first[..W]);
    p.extend_from_slice(&step.to_le_bytes()[..W]);
    put_u32(&mut p, n as u32);
    Some(wrap(kind, &p))
}

fn mask(w: usize) -> u64 {
    if w >= 8 {
        u64::MAX
    } else {
        (1u64 << (w * 8)) - 1
    }
}

fn try_cycle(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 16 {
        return None;
    }
    let max_pat = (data.len() / 2).min(if data.len() <= 8192 { 256 } else { 64 });
    let mut best: Option<Vec<u8>> = None;
    for pat in 1..=max_pat {
        if data.len() % pat != 0 {
            continue;
        }
        let p = &data[..pat];
        if !data.chunks_exact(pat).all(|c| c == p) {
            continue;
        }
        let mut payload = Vec::with_capacity(2 + 4 + pat);
        payload.extend_from_slice(&(pat as u16).to_le_bytes());
        put_u32(&mut payload, data.len() as u32);
        payload.extend_from_slice(p);
        let blob = wrap(CYCLE, &payload);
        if best.as_ref().map(|b| blob.len() < b.len()).unwrap_or(true) {
            best = Some(blob);
        }
        break; // smallest period is shortest description
    }
    best
}

fn try_quad_u8(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 24 {
        return None;
    }
    // s[i] = a + b*i + c*i*i  (wrapping u8). Fit i=0,1,2.
    let a = data[0];
    let s1 = data[1];
    let s2 = data[2];
    // s1 = a+b+c, s2 = a+2b+4c
    // (s2 - 2*s1 + a) = 2c  → c = that/2. Must be even in integers wrapping.
    // Work in u8 wrapping:
    // b + c = s1 - a
    // 2b + 4c = s2 - a
    let d1 = s1.wrapping_sub(a);
    let d2 = s2.wrapping_sub(a);
    // 2*(b+c) = 2*d1; 2b+4c = d2 → 2c = d2 - 2*d1 → c = (d2-2*d1)/2
    let two_c = d2.wrapping_sub(d1.wrapping_mul(2));
    if two_c % 2 != 0 {
        // wrapping evenness: two_c is even iff LSB 0
        return None;
    }
    let c = two_c / 2;
    let b = d1.wrapping_sub(c);
    for (i, &got) in data.iter().enumerate() {
        let ii = i as u8;
        let expect = a
            .wrapping_add(b.wrapping_mul(ii))
            .wrapping_add(c.wrapping_mul(ii).wrapping_mul(ii));
        if got != expect {
            return None;
        }
    }
    let mut p = vec![a, b, c];
    put_u32(&mut p, data.len() as u32);
    Some(wrap(QUAD_U8, &p))
}

/// Smallest exact generator, or None if STORE is shorter or nothing fits.
pub fn encode(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 16 {
        return None;
    }
    let mut best: Option<Vec<u8>> = None;
    let take = |best: &mut Option<Vec<u8>>, cand: Option<Vec<u8>>| {
        if let Some(c) = cand {
            if c.len() < data.len() && decode(&c).ok().as_deref() == Some(data) {
                if best.as_ref().map(|b| c.len() < b.len()).unwrap_or(true) {
                    *best = Some(c);
                }
            }
        }
    };
    take(&mut best, try_arith_u8(data));
    take(&mut best, try_arith_width::<2>(data, ARITH_U16));
    take(&mut best, try_arith_width::<4>(data, ARITH_U32));
    take(&mut best, try_arith_width::<8>(data, ARITH_U64));
    take(&mut best, try_cycle(data));
    take(&mut best, try_quad_u8(data));
    best
}

pub fn decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if !is_mth(buf) {
        return Err("mth magic");
    }
    let kind = buf[4];
    let p = &buf[5..];
    match kind {
        ARITH_U8 => {
            if p.len() < 6 {
                return Err("mth u8");
            }
            let a = p[0];
            let step = p[1];
            let n = u32::from_le_bytes(p[2..6].try_into().unwrap()) as usize;
            let mut out = Vec::with_capacity(n);
            for i in 0..n {
                out.push(a.wrapping_add((step as u32).wrapping_mul(i as u32) as u8));
            }
            Ok(out)
        }
        ARITH_U16 | ARITH_U32 | ARITH_U64 => {
            let w = match kind {
                ARITH_U16 => 2,
                ARITH_U32 => 4,
                _ => 8,
            };
            if p.len() < w * 2 + 4 {
                return Err("mth arith");
            }
            let mut start = [0u8; 8];
            let mut stepb = [0u8; 8];
            start[..w].copy_from_slice(&p[..w]);
            stepb[..w].copy_from_slice(&p[w..w * 2]);
            let mut i = w * 2;
            let n = take_u32(p, &mut i)? as usize;
            let a = u64::from_le_bytes(start);
            let step = u64::from_le_bytes(stepb);
            let mut out = Vec::with_capacity(n * w);
            for k in 0..n {
                let v = a.wrapping_add(step.wrapping_mul(k as u64));
                out.extend_from_slice(&v.to_le_bytes()[..w]);
            }
            Ok(out)
        }
        CYCLE => {
            if p.len() < 6 {
                return Err("mth cycle");
            }
            let pat_len = u16::from_le_bytes(p[0..2].try_into().unwrap()) as usize;
            let mut i = 2usize;
            let total = take_u32(p, &mut i)? as usize;
            if pat_len == 0 || i + pat_len > p.len() {
                return Err("mth pat");
            }
            let pat = &p[i..i + pat_len];
            if total % pat_len != 0 {
                return Err("mth cycle len");
            }
            Ok(pat.repeat(total / pat_len))
        }
        QUAD_U8 => {
            if p.len() < 7 {
                return Err("mth quad");
            }
            let a = p[0];
            let b = p[1];
            let c = p[2];
            let n = u32::from_le_bytes(p[3..7].try_into().unwrap()) as usize;
            let mut out = Vec::with_capacity(n);
            for i in 0..n {
                let ii = i as u8;
                out.push(
                    a.wrapping_add(b.wrapping_mul(ii))
                        .wrapping_add(c.wrapping_mul(ii).wrapping_mul(ii)),
                );
            }
            Ok(out)
        }
        _ => Err("mth kind"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_ids_are_a_formula() {
        let mut buf = Vec::new();
        for i in 0u32..256 {
            buf.extend_from_slice(&(1000 + i * 3).to_le_bytes());
        }
        let blob = encode(&buf).expect("math");
        assert!(blob.len() < 32, "formula should be tiny, got {}", blob.len());
        assert_eq!(decode(&blob).unwrap(), buf);
    }

    #[test]
    fn cycle_beats_store() {
        let pat = b"AB";
        let data = pat.repeat(200);
        let blob = encode(&data).expect("cycle");
        assert!(blob.len() < data.len());
        assert_eq!(decode(&blob).unwrap(), data);
    }

    #[test]
    fn u8_ramp() {
        let data: Vec<u8> = (0..200u8).collect();
        let blob = encode(&data).expect("u8 arith");
        assert_eq!(decode(&blob).unwrap(), data);
        assert!(blob.len() < data.len());
    }

    #[test]
    fn random_stays_none() {
        let r: Vec<u8> = (0..256u32)
            .map(|i| (i.wrapping_mul(1103515245).wrapping_add(12345) >> 16) as u8)
            .collect();
        assert!(encode(&r).is_none());
    }
}
