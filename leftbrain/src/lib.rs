//! SPLv1 left brain. Private. Pair with pulsar BW23 for text.

pub mod aware;
pub mod detect;
pub mod frame;
pub mod house;
pub mod parse;
pub mod price;

use detect::Class;

pub fn encode_best(data: &[u8]) -> Option<(Vec<u8>, &'static str)> {
    let inner = pulsar::pulsar_encode(data)?;
    let kind = match detect::classify(data) {
        Class::Fill => house::KIND_FILL,
        Class::Sparse => house::KIND_SPARSE,
        Class::Text => house::KIND_PULSAR,
        Class::Binary => house::KIND_PULSAR,
    };
    let blob = house::wrap(kind, data.len() as u32, &inner);
    Some((blob, house::kind_name(kind)))
}

pub fn decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if buf.len() >= 4 && &buf[..4] == house::MAGIC {
        let (_kind, raw_len, inner) = house::unwrap(buf)?;
        let back = pulsar::pulsar_decode(inner)?;
        if back.len() as u32 != raw_len {
            return Err("LBHX length mismatch");
        }
        return Ok(back);
    }
    pulsar::pulsar_decode(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_small() {
        let data = b"the cat sat on the mat. the cat sat on the mat.".repeat(8);
        let (enc, kind) = encode_best(&data).expect("encode");
        assert!(enc.len() < data.len(), "{kind} grew");
        let back = decode(&enc).expect("decode");
        assert_eq!(back, data.as_slice());
    }
}
