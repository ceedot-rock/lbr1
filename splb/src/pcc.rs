//! PCC — one frame. The law. Encode and decode as a pair.
//!
//! ZERO  fill (TRU8 / TR8X)
//! MATCH LBR1 parse + pack
//! BWT   pulsar BW22
//! CMAQ  own mixer (PCCaq). Not paq8px. Not xz.
//! STORE raw tile
//!
//! Smallest own DECODE_OK op wins. Combined GC is not an op. Host xz is not an op.

use crate::decode_lbr1;
use crate::detect;
use crate::frame;
use crate::parse;

pub const MAGIC: &[u8; 4] = b"PCC1";
pub const VER: u8 = 1;
/// TRUSTREAM tile.
pub const TILE: usize = 4096;
/// MATCH window = champ (4 MiB). PCC is the quality path.
pub const NOMINAL_WINDOW: u32 = parse::DEFAULT_WINDOW as u32;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Zero = 0,
    Match = 1,
    Bwt = 2,
    Store = 3,
    Cmaq = 4,
}

impl Op {
    pub fn from_u8(v: u8) -> Result<Self, &'static str> {
        match v {
            0 => Ok(Op::Zero),
            1 => Ok(Op::Match),
            2 => Ok(Op::Bwt),
            3 => Ok(Op::Store),
            4 => Ok(Op::Cmaq),
            _ => Err("pcc op"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Op::Zero => "zero",
            Op::Match => "match",
            Op::Bwt => "bwt",
            Op::Store => "store",
            Op::Cmaq => "cmaq",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Block {
    pub op: Op,
    pub raw_len: u32,
    pub blob: Vec<u8>,
}

pub fn is_pcc(buf: &[u8]) -> bool {
    buf.len() >= 13 && buf.starts_with(MAGIC) && buf[4] == VER
}

fn put_u32(out: &mut Vec<u8>, n: u32) {
    out.extend_from_slice(&n.to_le_bytes());
}

fn take_u32(buf: &[u8], i: &mut usize) -> Result<u32, &'static str> {
    if *i + 4 > buf.len() {
        return Err("pcc u32");
    }
    let n = u32::from_le_bytes(buf[*i..*i + 4].try_into().unwrap());
    *i += 4;
    Ok(n)
}

pub fn pack(raw_len: u32, blocks: &[Block]) -> Vec<u8> {
    let mut out = Vec::from(*MAGIC);
    out.push(VER);
    put_u32(&mut out, raw_len);
    put_u32(&mut out, blocks.len() as u32);
    for b in blocks {
        out.push(b.op as u8);
        put_u32(&mut out, b.raw_len);
        put_u32(&mut out, b.blob.len() as u32);
        out.extend_from_slice(&b.blob);
    }
    out
}

pub fn unpack(buf: &[u8]) -> Result<(u32, Vec<Block>), &'static str> {
    if !is_pcc(buf) {
        return Err("not PCC1");
    }
    let mut i = 5usize;
    let raw_len = take_u32(buf, &mut i)?;
    let n = take_u32(buf, &mut i)? as usize;
    let mut blocks = Vec::with_capacity(n);
    for _ in 0..n {
        if i >= buf.len() {
            return Err("pcc trunc");
        }
        let op = Op::from_u8(buf[i])?;
        i += 1;
        let rl = take_u32(buf, &mut i)?;
        let bl = take_u32(buf, &mut i)? as usize;
        if i + bl > buf.len() {
            return Err("pcc blob");
        }
        blocks.push(Block {
            op,
            raw_len: rl,
            blob: buf[i..i + bl].to_vec(),
        });
        i += bl;
    }
    if i != buf.len() {
        return Err("pcc tail");
    }
    Ok((raw_len, blocks))
}

fn try_zero(data: &[u8]) -> Option<Vec<u8>> {
    if let Some((sym, _)) = frame::solid_run(data) {
        return Some(vec![sym]);
    }
    if let Some((sym, _)) = frame::sparse_mode(data) {
        let blob = frame::pack_tr8x(data, sym);
        if blob.len() < data.len() {
            return Some(blob);
        }
    }
    None
}

fn pack_match(toks: &[parse::Tok], data: &[u8], window: u32, ml4: bool, raffle: bool) -> Vec<u8> {
    let (ver, payload) = if raffle {
        let a = crate::range::encode_toks_mlit4(toks, data);
        let b = crate::range::encode_toks_mlit(toks, data);
        if a.len() <= b.len() {
            (frame::VER_ML4, a)
        } else {
            (frame::VER_ML, b)
        }
    } else if ml4 {
        (frame::VER_ML4, crate::range::encode_toks_mlit4(toks, data))
    } else {
        (frame::VER_ML, crate::range::encode_toks_mlit(toks, data))
    };
    let mut out = Vec::with_capacity(17 + payload.len());
    out.extend_from_slice(frame::MAGIC);
    out.push(ver);
    put_u32(&mut out, data.len() as u32);
    put_u32(&mut out, window);
    put_u32(&mut out, payload.len() as u32);
    out.extend_from_slice(&payload);
    out
}

fn try_match(data: &[u8], class: detect::Class, window: u32, ml4: bool) -> Option<Vec<u8>> {
    let w = detect::window_for_class(data.len(), class, window);
    let toks = parse::parse_class(data, w, class);
    let raffle = data.len() < 1_000_000;
    let blob = pack_match(&toks, data, w as u32, ml4, raffle);
    if blob.len() < data.len() {
        Some(blob)
    } else {
        None
    }
}

/// MATCH on 4-byte delta. Same LBR1 gene the champ path already decodes (LD32).
fn try_delta_match(data: &[u8], class: detect::Class, window: u32, ml4: bool) -> Option<Vec<u8>> {
    if data.len() < 64 {
        return None;
    }
    let d = frame::delta4(data);
    let w = detect::window_for_class(d.len(), class, window);
    let toks = parse::parse_class(&d, w, class);
    let raffle = data.len() < 1_000_000;
    let inner = pack_match(&toks, &d, w as u32, ml4, raffle);
    let wrap = frame::pack_ld32(data.len() as u32, &inner);
    if wrap.len() < data.len() {
        Some(wrap)
    } else {
        None
    }
}

fn try_bwt(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 256 {
        return None;
    }
    let blob = pulsar::bwt_ans::compress(data);
    if blob.len() < data.len() {
        Some(blob)
    } else {
        None
    }
}

fn decode_op(op: Op, blob: &[u8], raw_len: usize) -> Result<Vec<u8>, &'static str> {
    match op {
        Op::Zero => {
            if blob.len() == 1 {
                Ok(vec![blob[0]; raw_len])
            } else {
                decode_lbr1(blob)
            }
        }
        Op::Match => decode_lbr1(blob),
        Op::Bwt => pulsar::bwt_ans::decompress(blob).map_err(|_| "pcc bwt"),
        Op::Cmaq => crate::pccaq::decode(blob),
        Op::Store => {
            if blob.len() != raw_len {
                return Err("pcc store len");
            }
            Ok(blob.to_vec())
        }
    }
}

fn store_block(data: &[u8]) -> Block {
    Block {
        op: Op::Store,
        raw_len: data.len() as u32,
        blob: data.to_vec(),
    }
}

fn block(op: Op, data: &[u8], blob: Vec<u8>) -> Block {
    Block {
        op,
        raw_len: data.len() as u32,
        blob,
    }
}

fn take_smaller(best: &mut Option<Block>, b: Block) {
    match best {
        None => *best = Some(b),
        Some(cur) if b.blob.len() < cur.blob.len() => *best = Some(b),
        _ => {}
    }
}

fn try_cmaq(data: &[u8]) -> Option<Vec<u8>> {
    crate::pccaq::encode(data)
}

fn zero_block(data: &[u8]) -> Option<Block> {
    if let Some((sym, n)) = frame::solid_run(data) {
        return Some(Block {
            op: Op::Zero,
            raw_len: n,
            blob: vec![sym],
        });
    }
    if let Some((sym, _)) = frame::sparse_mode(data) {
        let blob = frame::pack_tr8x(data, sym);
        if blob.len() < data.len() {
            return Some(block(Op::Zero, data, blob));
        }
    }
    None
}

const PROBE: usize = 64 * 1024;

/// Scale head/mid/tail MATCH to the whole file. BWT does not shrink this way —
/// it needs the full wavelength — so we never estimate BWT from a slice.
fn estimate_match(data: &[u8], class: detect::Class, plan: &crate::autonoma::Plan) -> usize {
    let n = data.len();
    if n <= PROBE {
        return try_match(data, class, plan.match_window, plan.ml4)
            .map(|m| m.len())
            .unwrap_or(n);
    }
    let starts = [0usize, n / 2, n.saturating_sub(PROBE)];
    let mut coded = 0usize;
    let mut raw = 0usize;
    for start in starts {
        let end = (start + PROBE).min(n);
        if end <= start {
            continue;
        }
        let s = &data[start..end];
        raw += s.len();
        coded += try_match(s, class, 1 << 20, plan.ml4)
            .map(|m| m.len())
            .unwrap_or(s.len());
    }
    if raw == 0 {
        return n;
    }
    ((coded as u64 * n as u64) / raw as u64) as usize
}

/// Autonoma seat is the law. Estimate is a MATCH inhibitor, not a replacement.
/// BWT needs the full file — never skip MATCH just because BWT "held."
fn pick_one(data: &[u8], class: detect::Class, plan: &crate::autonoma::Plan) -> Block {
    if let Some(z) = zero_block(data) {
        return z;
    }
    let mut best: Option<Block> = None;
    for seat in plan.order() {
        if seat == crate::autonoma::Seat::Store {
            break;
        }
        if !plan.allows(seat) {
            continue;
        }
        match seat {
            crate::autonoma::Seat::Bwt => {
                if let Some(b) = try_bwt(data) {
                    take_smaller(&mut best, block(Op::Bwt, data, b));
                }
            }
            crate::autonoma::Seat::Cmaq => {
                if let Some(c) = try_cmaq(data) {
                    take_smaller(&mut best, block(Op::Cmaq, data, c));
                }
            }
            crate::autonoma::Seat::Match => {
                let cur = best.as_ref().map(|b| b.blob.len()).unwrap_or(data.len());
                let pay = best.is_none() || {
                    let est = estimate_match(data, class, plan);
                    est + est / 20 < cur
                };
                if pay {
                    if let Some(m) = try_match(data, class, plan.match_window, plan.ml4) {
                        take_smaller(&mut best, block(Op::Match, data, m));
                    }
                    if plan.try_delta {
                        if let Some(d) = try_delta_match(data, class, plan.match_window, plan.ml4)
                        {
                            take_smaller(&mut best, block(Op::Match, data, d));
                        }
                    }
                }
            }
            crate::autonoma::Seat::Store => break,
        }
        if let Some(cur) = &best {
            if crate::autonoma::crushed(cur.blob.len(), data.len(), plan.crush) {
                return best.unwrap();
            }
            // MATCH/CMAQ already did the job. BWT does not get to silence MATCH.
            if seat != crate::autonoma::Seat::Bwt
                && crate::autonoma::held(cur.blob.len(), data.len())
            {
                return best.unwrap();
            }
        }
    }
    best.unwrap_or_else(|| store_block(data))
}

/// PCC law: smallest own DECODE_OK composition. Autonoma picks the seat.
/// Mixed files may emit several blocks (frame already concatenates). One block
/// stays the path for uniform mozilla/ooffice so far copies survive.
fn pick_blocks(data: &[u8]) -> Vec<Block> {
    if let Some(z) = zero_block(data) {
        return vec![z];
    }
    let class = detect::classify(data);
    let plan = crate::autonoma::plan(data, class);
    // BWT is one wavelength. Fill islands inside a medical/text body (mr) are
    // not a license to split. Seam only when the seat is MATCH/STORE.
    if plan.primary == crate::autonoma::Seat::Bwt {
        return vec![pick_one(data, class, &plan)];
    }
    if (plan.seamed || crate::hybrid::is_mixed(data)) && data.len() >= 8192 {
        let cuts = crate::hybrid::cuts(data);
        if cuts.len() >= 2 {
            let mut parts = Vec::with_capacity(cuts.len());
            for (a, b) in &cuts {
                let slice = &data[*a..*b];
                let c = detect::classify(slice);
                let p = crate::autonoma::plan(slice, c);
                parts.push(pick_one(slice, c, &p));
            }
            return parts;
        }
    }
    vec![pick_one(data, class, &plan)]
}

fn frame_from_blocks(data: &[u8], blocks: Vec<Block>) -> Option<Vec<u8>> {
    Some(pack(data.len() as u32, &blocks))
}

/// Pack only. Handshake is `decode` at the CLI / `encode`.
pub fn encode_frame(data: &[u8], stream: bool) -> Option<(Vec<u8>, &'static str)> {
    if data.is_empty() {
        return None;
    }
    if stream {
        let mut blocks = Vec::new();
        for chunk in data.chunks(TILE) {
            if let Some(z) = try_zero(chunk) {
                blocks.push(Block {
                    op: Op::Zero,
                    raw_len: chunk.len() as u32,
                    blob: z,
                });
            } else {
                blocks.push(Block {
                    op: Op::Store,
                    raw_len: chunk.len() as u32,
                    blob: chunk.to_vec(),
                });
            }
        }
        let out = frame_from_blocks(data, blocks)?;
        return Some((out, "stream"));
    }
    let blocks = pick_blocks(data);
    let tag = if blocks.len() == 1 {
        blocks[0].op.name()
    } else {
        "seam"
    };
    let out = pack(data.len() as u32, &blocks);
    Some((out, tag))
}

/// Whole-file PCC. One block. Does not replace `lb champ`.
pub fn encode(data: &[u8]) -> Option<Vec<u8>> {
    let (out, _) = encode_frame(data, false)?;
    match decode(&out) {
        Ok(back) if back == data => Some(out),
        _ => None,
    }
}

/// TRUSTREAM: 4 KiB tiles, STORE + ZERO only. Not a Silesia contestant.
pub fn encode_stream(data: &[u8]) -> Option<Vec<u8>> {
    let (out, _) = encode_frame(data, true)?;
    match decode(&out) {
        Ok(back) if back == data => Some(out),
        _ => None,
    }
}

pub fn decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    if !is_pcc(buf) {
        return Err("not PCC1");
    }
    let mut i = 5usize;
    let raw_len = take_u32(buf, &mut i)? as usize;
    let n = take_u32(buf, &mut i)? as usize;
    let mut out = Vec::with_capacity(raw_len);
    for _ in 0..n {
        if i >= buf.len() {
            return Err("pcc trunc");
        }
        let op = Op::from_u8(buf[i])?;
        i += 1;
        let rl = take_u32(buf, &mut i)? as usize;
        let bl = take_u32(buf, &mut i)? as usize;
        if i + bl > buf.len() {
            return Err("pcc blob");
        }
        let blob = &buf[i..i + bl];
        i += bl;
        match op {
            Op::Store => {
                if blob.len() != rl {
                    return Err("pcc store len");
                }
                out.extend_from_slice(blob);
            }
            _ => {
                let piece = decode_op(op, blob, rl)?;
                if piece.len() != rl {
                    return Err("pcc piece");
                }
                out.extend_from_slice(&piece);
            }
        }
    }
    if i != buf.len() {
        return Err("pcc tail");
    }
    if out.len() != raw_len {
        return Err("pcc raw_len");
    }
    Ok(out)
}

/// Kind tag for CLI. Pack only — caller does DECODE_OK.
pub fn encode_tagged(data: &[u8], stream: bool) -> Option<(Vec<u8>, &'static str)> {
    encode_frame(data, stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeros_are_zero_op() {
        let s = vec![0u8; 4096];
        let e = encode(&s).expect("pcc zeros");
        assert!(is_pcc(&e));
        let (_, blocks) = unpack(&e).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].op, Op::Zero);
        assert_eq!(blocks[0].blob.len(), 1);
        assert_eq!(blocks[0].blob[0], 0);
        assert_eq!(decode(&e).unwrap(), s);
    }

    #[test]
    fn stream_two_zero_tiles() {
        let s = vec![0u8; 8192];
        let e = encode_stream(&s).expect("stream");
        let (_, blocks) = unpack(&e).unwrap();
        assert_eq!(blocks.len(), 2);
        assert!(blocks.iter().all(|b| b.op == Op::Zero));
        assert_eq!(decode(&e).unwrap(), s);
    }

    #[test]
    fn store_incompressible() {
        let s: Vec<u8> = (0..64u32)
            .map(|i| (i.wrapping_mul(1103515245).wrapping_add(12345) >> 16) as u8)
            .collect();
        let e = encode(&s).expect("pcc");
        assert_eq!(decode(&e).unwrap(), s);
        let (_, blocks) = unpack(&e).unwrap();
        assert_eq!(blocks[0].op, Op::Store);
    }

    #[test]
    fn champ_path_untouched_zeros_via_encode_window() {
        let s = vec![0u8; 4096];
        let e = crate::encode_window(&s, parse::DEFAULT_WINDOW as u32).expect("champ zeros");
        assert_eq!(e.len(), 8);
        assert_eq!(&e[..3], b"TR8");
        assert_eq!(crate::decode(&e).unwrap(), s);
    }

    #[test]
    fn crate_decode_sees_pcc1() {
        let s = vec![0u8; 4096];
        let e = encode(&s).expect("pcc zeros");
        assert!(is_pcc(&e));
        assert_eq!(crate::decode(&e).unwrap(), s);
    }

    #[test]
    fn stream_zero_then_store() {
        let mut s = vec![0u8; TILE];
        s.extend((0..TILE as u32).map(|i| (i.wrapping_mul(1103515245).wrapping_add(12345) >> 16) as u8));
        let e = encode_stream(&s).expect("stream mixed");
        let (_, blocks) = unpack(&e).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].op, Op::Zero);
        assert_eq!(blocks[1].op, Op::Store);
        assert_eq!(decode(&e).unwrap(), s);
        assert_eq!(crate::decode(&e).unwrap(), s);
    }

    #[test]
    fn repeating_text_roundtrips() {
        let s = b"the cat sat on the mat. ".repeat(400);
        let e = encode(&s).expect("pcc text");
        assert!(is_pcc(&e));
        assert!(e.len() < s.len());
        let (_, blocks) = unpack(&e).unwrap();
        assert!(matches!(blocks[0].op, Op::Match | Op::Bwt | Op::Cmaq));
        assert_eq!(decode(&e).unwrap(), s);
    }

    #[test]
    fn bwt_body_does_not_seam_fill_islands() {
        let mut s = Vec::new();
        for i in 0..20000u32 {
            s.extend_from_slice(&[(i % 24) as u8, 0, 0, 0]);
        }
        s.extend(vec![0u8; 8192]);
        for i in 0..20000u32 {
            s.extend_from_slice(&[(i % 24) as u8, 0, 0, 0]);
        }
        let e = encode(&s).expect("pcc");
        assert_eq!(decode(&e).unwrap(), s);
        let (_, blocks) = unpack(&e).unwrap();
        assert_eq!(blocks.len(), 1, "BWT seat must stay one block, got {}", blocks.len());
        assert_ne!(blocks[0].op, Op::Store);
    }

    #[test]
    fn mixed_fill_then_text_roundtrips() {
        let mut s = vec![0u8; 8192];
        s.extend_from_slice(b"the cat sat on the mat. ".repeat(400).as_slice());
        let e = encode(&s).expect("pcc mixed");
        assert!(is_pcc(&e));
        assert_eq!(decode(&e).unwrap(), s);
        assert!(e.len() < s.len());
        let (_, blocks) = unpack(&e).unwrap();
        assert!(blocks.iter().any(|b| b.op == Op::Zero) || blocks.len() == 1);
    }

    #[test]
    fn delta_integers_roundtrip() {
        let mut s = Vec::new();
        for i in 0u32..800 {
            s.extend_from_slice(&(i.wrapping_mul(3)).to_le_bytes());
        }
        let e = encode(&s).expect("pcc ints");
        assert_eq!(decode(&e).unwrap(), s);
        assert!(e.len() < s.len());
    }
}
