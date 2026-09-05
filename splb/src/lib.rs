//! PCC (splb) — Ptaszenski Computational Codec. Own LZ + rANS for binaries.
//! House picker: min(LBR1, pulsar BW22).

pub mod aware;
pub mod detect;
pub mod frame;
pub mod hybrid;
pub mod parse;
pub mod rans;
pub mod rans_o1;
pub mod range;
pub mod rans_op;
pub mod sensors;
pub mod sentinel;

pub const VERSION: &str = "pcc-0.3.0";
pub const MAGIC: &[u8; 4] = frame::MAGIC;

pub fn version() -> &'static str {
    VERSION
}

pub fn encode(data: &[u8]) -> Option<Vec<u8>> {
    encode_window(data, parse::DEFAULT_WINDOW as u32)
}

pub fn encode_window(data: &[u8], window: u32) -> Option<Vec<u8>> {
    if data.is_empty() {
        return None;
    }
    let mut best: Option<Vec<u8>> = None;
    if let Some((sym, n)) = frame::solid_run(data) {
        let blob = frame::pack_tru8(sym, n);
        if decode_lbr1(&blob).ok().as_deref() == Some(data) {
            return Some(blob);
        }
    }
    if let Some((sym, _)) = frame::sparse_mode(data) {
        let blob = frame::pack_tr8x(data, sym);
        if blob.len() < data.len() && decode_lbr1(&blob).ok().as_deref() == Some(data) {
            best = Some(blob);
        }
    }
    let w = detect::window_for(data, window);
    let toks = parse::parse(data, w);
    let blob = frame::pack(&toks, data.len(), w as u32, data);
    if blob.len() < data.len() && decode_lbr1(&blob).ok().as_deref() == Some(data) {
        match &best {
            None => best = Some(blob),
            Some(b) if blob.len() < b.len() => best = Some(blob),
            _ => {}
        }
    }
    best
}

pub fn decode_lbr1(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if frame::is_tru8(buf) {
        return frame::unpack_tru8(buf);
    }
    if frame::is_tr8x(buf) {
        return frame::unpack_tr8x(buf);
    }
    let (out, _) = frame::unpack_bytes(buf)?;
    Ok(out)
}

/// House: smaller of LBR1 and pulsar (BW22 / OZL2). Both gated.
pub fn encode_best(data: &[u8]) -> Option<(Vec<u8>, &'static str)> {
    let mut best: Option<(Vec<u8>, &'static str)> = None;
    if let Some(a) = encode(data) {
        let tag = if frame::is_tru8(&a) {
            "tru8"
        } else if frame::is_tr8x(&a) {
            "tr8x"
        } else {
            "lbr1"
        };
        best = Some((a, tag));
    }
    if let Some(b) = pulsar::pulsar_encode(data) {
        match &best {
            None => best = Some((b, "bw22")),
            Some((a, _)) if b.len() < a.len() => best = Some((b, "bw22")),
            _ => {}
        }
    }
    // Mixed-file packer. Only kept if DECODE_OK and strictly smaller than whole-file min.
    if let Some(h) = hybrid::pack(data, true) {
        match &best {
            None => best = Some((h, "lbhm")),
            Some((a, _)) if h.len() < a.len() => best = Some((h, "lbhm")),
            _ => {}
        }
    }
    best
}

pub fn decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if frame::is_tru8(buf) {
        return frame::unpack_tru8(buf);
    }
    if frame::is_tr8x(buf) {
        return frame::unpack_tr8x(buf);
    }
    if hybrid::is_hybrid(buf) {
        return hybrid::unpack(buf);
    }
    if buf.len() >= 4 && &buf[..4] == MAGIC {
        return decode_lbr1(buf);
    }
    pulsar::pulsar_decode(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeros_tiny() {
        let s = vec![0u8; 4096];
        let e = encode(&s).expect("zeros should collapse");
        assert_eq!(e.len(), 8, "tru8 must be 8 bytes, got {}", e.len());
        assert_eq!(&e[..3], b"TR8");
        assert_eq!(decode(&e).unwrap(), s);
    }

    #[test]
    fn tru8_solid_ff() {
        let s = vec![0xffu8; 10_000];
        let e = encode(&s).expect("tru8");
        assert_eq!(e.len(), 8);
        assert_eq!(decode(&e).unwrap(), s);
        let cap = frame::pack_tru8(0, frame::TRU8_MAX);
        assert_eq!(cap.len(), 8);
        assert_eq!(u32::from_le_bytes(cap[4..8].try_into().unwrap()), frame::TRU8_MAX);
    }

    #[test]
    fn tr8x_sparse_nonzero() {
        let mut s = vec![0u8; 8192];
        s[10] = 7;
        s[400] = 0x3c;
        s[401] = 0x3d;
        s[8000] = 0xff;
        let e = encode(&s).expect("tr8x");
        assert!(frame::is_tr8x(&e), "expected TR8X, got {:?}", &e[..4.min(e.len())]);
        assert!(e.len() < 64, "sparse should stay tiny, got {}", e.len());
        assert_eq!(decode(&e).unwrap(), s);
        let (blob, tag) = encode_best(&s).expect("house");
        assert!(tag == "tr8x" || blob.len() <= e.len());
    }

    #[test]
    fn repeated_opcode() {
        let motif: Vec<u8> = (0..32).map(|i| (0x48 + i * 3) as u8).collect();
        let mut s = Vec::new();
        for k in 0..800 {
            s.extend_from_slice(&motif);
            s.push((k & 0xff) as u8);
        }
        let e = encode(&s).expect("motif");
        assert!((e.len() as u64) * 3 < (s.len() as u64) * 2);
        assert_eq!(decode(&e).unwrap(), s);
    }

    #[test]
    fn hybrid_fill_then_text_beats_or_equals() {
        let mut s = vec![0u8; 8192];
        s.extend_from_slice(b"the cat sat on the mat. ".repeat(400).as_slice());
        s.extend_from_slice(&[0u8; 4096]);
        assert!(hybrid::is_mixed(&s), "fill+text must look mixed");
        let (blob, tag) = encode_best(&s).expect("house");
        assert_eq!(decode(&blob).unwrap(), s);
        if tag == "lbhm" {
            assert!(hybrid::is_hybrid(&blob));
        }
    }

    #[test]
    fn random_may_expand() {
        let s: Vec<u8> = (0..2048u32)
            .map(|i| (i.wrapping_mul(1103515245).wrapping_add(12345) >> 16) as u8)
            .collect();
        match encode(&s) {
            None => {}
            Some(e) => {
                assert!(e.len() < s.len());
                assert_eq!(decode(&e).unwrap(), s.as_slice());
            }
        }
    }

    #[test]
    fn wn_ooffice_meg() {
        let path = "/home/workdir/artifacts/speed-cmp/corpora/silesia/ooffice";
        let raw = std::fs::read(path).expect("ooffice");
        let d = &raw[..1_000_000.min(raw.len())];
        let t = parse::parse_wn(d);
        let nm = t.iter().filter(|x| matches!(x, parse::Tok::Match { .. })).count();
        eprintln!("wn slice toks={} matches={} n={}", t.len(), nm, d.len());
        let blob = frame::pack(&t, d.len(), d.len() as u32, d);
        eprintln!("blob={}", blob.len());
        let back = decode_lbr1(&blob).expect("decode");
        assert_eq!(back.as_slice(), d);
        let e = encode(d);
        eprintln!("encode1M={:?}", e.as_ref().map(|x| x.len()));
        let e6 = encode(&raw);
        eprintln!("encodeFULL={:?}", e6.as_ref().map(|x| x.len()));
        let t6 = parse::parse_wn(&raw);
        let b6 = frame::pack(&t6, raw.len(), raw.len() as u32, &raw);
        eprintln!("full pack={} toks={}", b6.len(), t6.len());
        match decode_lbr1(&b6) {
            Ok(x) => eprintln!("full decode {} eq={}", x.len(), x == raw),
            Err(e) => eprintln!("full decode ERR {e}"),
        }
    }
}
