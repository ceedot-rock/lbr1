//! PCC (splb) — Ptaszenski Computational Codec. Own LZ + rANS for binaries.
//! House picker: min(LBR1, pulsar BW22).

pub mod asmd;
pub mod autonoma;
pub mod aware;
pub mod codex;
pub mod crc;
pub mod detect;
pub mod frame;
pub mod guess;
pub mod hybrid;
pub mod lzm;
pub mod mathstore;
pub mod nnc;
pub mod parse;
pub mod pcc;
pub mod pccaq;
pub mod phrases;
pub mod pccz;
pub mod rans;
pub mod structx;
pub mod wrap;
pub mod rans_o1;
pub mod range;
pub mod rans_op;
pub mod sensors;
pub mod sentinel;
pub mod zmix;

pub const VERSION: &str = "pcc-0.13.0";
pub const MAGIC: &[u8; 4] = frame::MAGIC;

pub fn version() -> &'static str {
    VERSION
}

pub fn encode(data: &[u8]) -> Option<Vec<u8>> {
    encode_window(data, parse::DEFAULT_WINDOW as u32)
}

pub fn blob_kind(b: &[u8]) -> &'static str {
    if pccz::is_pccz(b) {
        "pccz"
    } else if pcc::is_pcc(b) {
        "pcc1"
    } else if frame::is_tru8(b) {
        "tru8"
    } else if frame::is_tr8x(b) {
        "tr8x"
    } else if hybrid::is_hybrid(b) {
        "lbhm"
    } else if b.len() >= 4 && (&b[..4] == b"BW23" || &b[..4] == b"BW22") {
        "bw22"
    } else if wrap::is_lz(b) {
        "lzw1"
    } else if wrap::is_paq(b) {
        "pcaq"
    } else if lzm::is_lzm(b) {
        "lzm1"
    } else if zmix::is_zmix(b) {
        "zmx1"
    } else if nnc::is_nnc(b) {
        "nnc1"
    } else if structx::is_str(b) {
        "str1"
    } else if guess::is_guess(b) {
        "gss1"
    } else {
        "lbr1"
    }
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
    let class = detect::classify(data);
    let plan = autonoma::plan(data, class);
    // Champ sits BW22 when Autonoma says so. Mozilla is MATCH-only (try_bwt
    // false above 16 MiB binary) so the 14,796,694 lock path stays MATCH.
    if plan.try_bwt {
        if let Some(b) = pulsar::pulsar_encode(data) {
            match &best {
                None => best = Some(b),
                Some(cur) if b.len() < cur.len() => best = Some(b),
                _ => {}
            }
            if let Some(cur) = &best {
                if autonoma::crushed(cur.len(), data.len(), plan.crush) {
                    return best;
                }
            }
        }
    }
    if let Some(blob) = encode_lbr1_plain(data, window) {
        match &best {
            None => best = Some(blob),
            Some(b) if blob.len() < b.len() => best = Some(blob),
            _ => {}
        }
    }
    // 4-byte delta + inner LBR1. Skip huge files (mozilla/samba bake-off cost).
    if data.len() >= 64 && data.len() <= 12 * 1024 * 1024 {
        let d = frame::delta4(data);
        if let Some(inner) = encode_lbr1_plain(&d, window) {
            let wrapb = frame::pack_ld32(data.len() as u32, &inner);
            if wrapb.len() < data.len() && decode_lbr1(&wrapb).ok().as_deref() == Some(data) {
                match &best {
                    None => best = Some(wrapb),
                    Some(b) if wrapb.len() < b.len() => best = Some(wrapb),
                    _ => {}
                }
            }
        }
    }
    consider_wraps(data, &mut best);
    best
}

fn consider_wraps(data: &[u8], best: &mut Option<Vec<u8>>) {
    if let Some(z) = wrap::lz_encode(data) {
        match best {
            None => *best = Some(z),
            Some(cur) if z.len() < cur.len() => *best = Some(z),
            _ => {}
        }
    }
    if let Some(p) = wrap::paq_encode(data) {
        match best {
            None => *best = Some(p),
            Some(cur) if p.len() < cur.len() => *best = Some(p),
            _ => {}
        }
    }
}

fn encode_lbr1_plain(data: &[u8], window: u32) -> Option<Vec<u8>> {
    let w = detect::window_for(data, window);
    let toks = parse::parse(data, w);
    let blob = frame::pack(&toks, data.len(), w as u32, data);
    if blob.len() < data.len() && decode_lbr1(&blob).ok().as_deref() == Some(data) {
        Some(blob)
    } else {
        None
    }
}

pub fn decode_lbr1(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if frame::is_tru8(buf) {
        return frame::unpack_tru8(buf);
    }
    if frame::is_tr8x(buf) {
        return frame::unpack_tr8x(buf);
    }
    if frame::is_ld32(buf) {
        let (n, inner) = frame::unpack_ld32(buf)?;
        let d = decode_lbr1(inner)?;
        let out = frame::undelta4(&d);
        if out.len() as u32 != n {
            return Err("ld32 len");
        }
        return Ok(out);
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
    if best.as_ref().map(|(_, k)| *k) != Some("bw22") {
        if let Some(b) = pulsar::pulsar_encode(data) {
            match &best {
                None => best = Some((b, "bw22")),
                Some((a, _)) if b.len() < a.len() => best = Some((b, "bw22")),
                _ => {}
            }
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
    if let Some(z) = wrap::lz_encode(data) {
        match &best {
            None => best = Some((z, "lzw1")),
            Some((a, _)) if z.len() < a.len() => best = Some((z, "lzw1")),
            _ => {}
        }
    }
    if let Some(p) = wrap::paq_encode(data) {
        match &best {
            None => best = Some((p, "pcaq")),
            Some((a, _)) if p.len() < a.len() => best = Some((p, "pcaq")),
            _ => {}
        }
    }
    if let Some(p) = pcc::encode(data) {
        match &best {
            None => best = Some((p, "pcc1")),
            Some((a, _)) if p.len() < a.len() => best = Some((p, "pcc1")),
            _ => {}
        }
    }
    if let Some(z) = lzm::encode(data) {
        match &best {
            None => best = Some((z, "lzm1")),
            Some((a, _)) if z.len() < a.len() => best = Some((z, "lzm1")),
            _ => {}
        }
    }
    if let Some(s) = structx::encode(data) {
        match &best {
            None => best = Some((s, "str1")),
            Some((a, _)) if s.len() < a.len() => best = Some((s, "str1")),
            _ => {}
        }
    }
    let still_open = match &best {
        None => true,
        Some((a, _)) => (a.len() as f64) / (data.len() as f64) > 0.35,
    };
    if still_open && data.len() <= zmix::HOUSE_MAX {
        if let Some(z) = zmix::encode(data) {
            match &best {
                None => best = Some((z, "zmx1")),
                Some((a, _)) if z.len() < a.len() => best = Some((z, "zmx1")),
                _ => {}
            }
        }
    }
    if still_open && data.len() <= nnc::HOUSE_MAX {
        if let Some(z) = nnc::encode(data) {
            match &best {
                None => best = Some((z, "nnc1")),
                Some((a, _)) if z.len() < a.len() => best = Some((z, "nnc1")),
                _ => {}
            }
        }
    }
    let still_open = match &best {
        None => true,
        Some((a, _)) => (a.len() as f64) / (data.len() as f64) > 0.35,
    };
    if still_open {
        if let Some(g) = try_gc_own(data) {
            match &best {
                None => best = Some((g, "gc")),
                Some((a, _)) if g.len() < a.len() => best = Some((g, "gc")),
                _ => {}
            }
        }
    }
    let near_store = match &best {
        None => true,
        Some((a, _)) => (a.len() as f64) / (data.len() as f64) > 0.88,
    };
    if near_store {
        if let Some(g) = guess::encode(data) {
            match &best {
                None => best = Some((g, "gss1")),
                Some((a, _)) if g.len() < a.len() => best = Some((g, "gss1")),
                _ => {}
            }
        }
    }
    best
}

/// One archive member. Fill stays 8-byte TRU8. Else a PCC1 frame, else house min, else store.
pub fn compress_member(raw: &[u8]) -> (Vec<u8>, bool) {
    if raw.is_empty() {
        return (Vec::new(), true);
    }
    if let Some((sym, n)) = frame::solid_run(raw) {
        let blob = frame::pack_tru8(sym, n);
        if decode_lbr1(&blob).ok().as_deref() == Some(raw) {
            return (blob, false);
        }
    }
    if let Some(p) = pcc::encode(raw) {
        if p.len() < raw.len() {
            return (p, false);
        }
    }
    if let Some((b, _)) = encode_best(raw) {
        if b.len() < raw.len() && decode(&b).ok().as_deref() == Some(raw) {
            return (b, false);
        }
    }
    (raw.to_vec(), true)
}

fn host_skin(buf: &[u8]) -> bool {
    buf.len() >= 2 && buf.starts_with(&[0x1f, 0x8b])
        || buf.len() >= 4
            && (buf.starts_with(b"XZ1\0")
                || buf.starts_with(b"ZLB1")
                || buf.starts_with(b"BZ1\0")
                || buf.starts_with(b"\xfd7zX")
                || buf.starts_with(b"BZh"))
}

/// Combined GC own-path (Mode::Max). Host xz/zlib/bzip skins are dropped.
pub fn encode_gc(data: &[u8]) -> Option<Vec<u8>> {
    try_gc_own(data)
}

#[cfg(feature = "aware")]
fn try_gc_own(data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() || data.len() > crate::asmd::AWARE_CAP {
        return None;
    }
    let b = combined_gc::codec::encode(data, combined_gc::codec::Mode::Max).bytes;
    if host_skin(&b) {
        return None;
    }
    match decode_gene(&b) {
        Ok(back) if back == *data && b.len() < data.len() => Some(b),
        _ => None,
    }
}

#[cfg(not(feature = "aware"))]
fn try_gc_own(_data: &[u8]) -> Option<Vec<u8>> {
    None
}

pub fn decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if pccz::is_pccz(buf) {
        return Err("pcc archive: use lb unzip");
    }
    if pcc::is_pcc(buf) {
        return pcc::decode(buf);
    }
    if asmd::is_asmd(buf) {
        return asmd::decode_frame(buf);
    }
    decode_gene(buf)
}

/// One gene. No ASMD header. Combined GC decode is last and optional.
pub fn decode_gene(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if frame::is_tru8(buf) {
        return frame::unpack_tru8(buf);
    }
    if frame::is_tr8x(buf) {
        return frame::unpack_tr8x(buf);
    }
    if hybrid::is_hybrid(buf) {
        return hybrid::unpack(buf);
    }
    if frame::is_ld32(buf) {
        return decode_lbr1(buf);
    }
    if wrap::is_lz(buf) {
        return wrap::lz_decode(buf);
    }
    if wrap::is_paq(buf) {
        return wrap::paq_decode(buf);
    }
    if lzm::is_lzm(buf) {
        return lzm::decode(buf);
    }
    if zmix::is_zmix(buf) {
        return zmix::decode(buf);
    }
    if nnc::is_nnc(buf) {
        return nnc::decode(buf);
    }
    if structx::is_str(buf) {
        return structx::decode(buf);
    }
    if guess::is_guess(buf) {
        return guess::decode(buf);
    }
    if buf.len() >= 4 && &buf[..4] == MAGIC {
        return decode_lbr1(buf);
    }
    if let Ok(v) = pulsar::pulsar_decode(buf) {
        return Ok(v);
    }
    #[cfg(feature = "aware")]
    {
        if let Ok(v) = combined_gc::codec::decode(buf) {
            return Ok(v);
        }
    }
    Err("decode_gene")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ld32_roundtrip() {
        let mut s = Vec::new();
        for i in 0u32..200 {
            s.extend_from_slice(&(i.wrapping_mul(3)).to_le_bytes());
        }
        let d = frame::delta4(&s);
        assert_ne!(d, s);
        assert_eq!(frame::undelta4(&d), s);
    }

    #[test]
    fn zeros_tiny() {
        let s = vec![0u8; 4096];
        let e = encode(&s).expect("zeros should collapse");
        assert_eq!(e.len(), 8, "tru8 must be 8 bytes, got {}", e.len());
        assert_eq!(&e[..3], b"TR8");
        assert_eq!(decode(&e).unwrap(), s);
    }

    #[test]
    fn champ_text_can_sit_bw22() {
        let s = b"the cat sat on the mat. ".repeat(400);
        let e = encode_window(&s, parse::DEFAULT_WINDOW as u32).expect("champ text");
        assert_eq!(decode(&e).unwrap(), s.as_slice());
        assert!(e.len() < s.len());
        assert_eq!(blob_kind(&e), "bw22");
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
