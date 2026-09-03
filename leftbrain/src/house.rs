//! LBHX — own-path house. Tags which lab engine coded the payload.
//! Not Combined GC. Not a host xz wrap.

pub const MAGIC: &[u8; 4] = b"LBHX";

pub const KIND_PULSAR: u8 = 0;
pub const KIND_LBR1: u8 = 1;
pub const KIND_FILL: u8 = 2;
pub const KIND_SPARSE: u8 = 3;

pub fn wrap(kind: u8, raw_len: u32, inner: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(13 + inner.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&raw_len.to_le_bytes());
    out.push(kind);
    out.extend_from_slice(&(inner.len() as u32).to_le_bytes());
    out.extend_from_slice(inner);
    out
}

pub fn unwrap(buf: &[u8]) -> Result<(u8, u32, &[u8]), &'static str> {
    if buf.len() < 13 || &buf[..4] != MAGIC {
        return Err("not LBHX");
    }
    let raw_len = u32::from_le_bytes(buf[4..8].try_into().unwrap());
    let kind = buf[8];
    let inner_len = u32::from_le_bytes(buf[9..13].try_into().unwrap()) as usize;
    if 13 + inner_len != buf.len() {
        return Err("LBHX truncated");
    }
    Ok((kind, raw_len, &buf[13..]))
}

pub fn kind_name(kind: u8) -> &'static str {
    match kind {
        KIND_PULSAR => "bw22",
        KIND_LBR1 => "lbr1",
        KIND_FILL => "tru8",
        KIND_SPARSE => "tr8x",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_round() {
        let w = wrap(KIND_PULSAR, 4, b"abcd");
        let (k, n, inner) = unwrap(&w).unwrap();
        assert_eq!(k, KIND_PULSAR);
        assert_eq!(n, 4);
        assert_eq!(inner, b"abcd");
    }
}
