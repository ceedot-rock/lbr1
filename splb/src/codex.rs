//! Omnistatic Codex: one process, two faces.
//!
//! Regular = decoded bytes. Dark = PCC1 (CDDG geometry).
//! `process` flips the face. Handshake is the same loop:
//! compress → decompress → mirror_error = 0.
//!
//! Does not emit sentinel cells into the bitstream.
//! Champ (`encode_window`) is not this layer.

use crate::pcc;
use crate::sentinel::Sentinel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Regular,
    Dark,
}

impl Face {
    pub fn of(buf: &[u8]) -> Self {
        if pcc::is_pcc(buf) {
            Face::Dark
        } else {
            Face::Regular
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Face::Regular => "regular",
            Face::Dark => "dark",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Codex {
    pub face: Face,
    pub bytes: Vec<u8>,
    pub op: &'static str,
    pub mirror: u64,
    pub dark: f64,
}

impl Codex {
    pub fn open(buf: &[u8]) -> Self {
        match Face::of(buf) {
            Face::Dark => Codex {
                face: Face::Dark,
                bytes: buf.to_vec(),
                op: dark_op(buf),
                mirror: 0,
                dark: 0.0,
            },
            Face::Regular => Codex {
                face: Face::Regular,
                bytes: buf.to_vec(),
                op: "regular",
                mirror: 0,
                dark: 0.0,
            },
        }
    }
}

fn dark_op(buf: &[u8]) -> &'static str {
    pcc::unpack(buf)
        .ok()
        .and_then(|(_, _, blocks)| blocks.first().map(|b| b.op.name()))
        .unwrap_or("pcc")
}

fn dark_degree(regular: &[u8], coded: usize) -> f64 {
    let obs = Sentinel::observe_stream(regular, coded);
    let tick = Sentinel::new().step(obs, obs[0], 0.0);
    tick.dark.iter().sum::<f64>() / 5.0
}

fn go_dark(regular: &[u8], stream: bool) -> Result<Codex, &'static str> {
    if regular.is_empty() {
        return Err("codex empty");
    }
    let (dark, op) = pcc::encode_frame(regular, stream).ok_or("codex encode")?;
    let back = pcc::decode(&dark)?;
    if back.as_slice() != regular {
        return Err("mirror");
    }
    let degree = dark_degree(regular, dark.len());
    Ok(Codex {
        face: Face::Dark,
        bytes: dark,
        op,
        mirror: 0,
        dark: degree,
    })
}

fn go_regular(dark: &[u8]) -> Result<Codex, &'static str> {
    let regular = pcc::decode(dark)?;
    Ok(Codex {
        face: Face::Regular,
        bytes: regular,
        op: "regular",
        mirror: 0,
        dark: 0.0,
    })
}

/// Flip face. Regular → Dark, Dark → Regular. One process.
pub fn process(buf: &[u8], stream: bool) -> Result<Codex, &'static str> {
    match Face::of(buf) {
        Face::Regular => go_dark(buf, stream),
        Face::Dark => go_regular(buf),
    }
}

/// Land on Dark. Already-dark PCC1 is checked, not re-encoded.
pub fn to_dark(buf: &[u8], stream: bool) -> Result<Codex, &'static str> {
    if Face::of(buf) == Face::Dark && !stream {
        let regular = pcc::decode(buf)?;
        if regular.is_empty() {
            return Err("codex empty");
        }
        return Ok(Codex {
            face: Face::Dark,
            bytes: buf.to_vec(),
            op: dark_op(buf),
            mirror: 0,
            dark: dark_degree(&regular, buf.len()),
        });
    }
    if Face::of(buf) == Face::Dark && stream {
        let regular = pcc::decode(buf)?;
        return go_dark(&regular, true);
    }
    go_dark(buf, stream)
}

/// Land on Regular. Already-regular is identity.
pub fn to_regular(buf: &[u8]) -> Result<Codex, &'static str> {
    match Face::of(buf) {
        Face::Dark => go_regular(buf),
        Face::Regular => Ok(Codex::open(buf)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeros_process_is_involution() {
        let s = vec![0u8; 4096];
        let dark = process(&s, false).expect("dark");
        assert_eq!(dark.face, Face::Dark);
        assert_eq!(dark.mirror, 0);
        assert_eq!(dark.op, "zero");
        let back = process(&dark.bytes, false).expect("regular");
        assert_eq!(back.face, Face::Regular);
        assert_eq!(back.bytes, s);
    }

    #[test]
    fn store_process_roundtrip() {
        let s: Vec<u8> = (0..64u32)
            .map(|i| (i.wrapping_mul(1103515245).wrapping_add(12345) >> 16) as u8)
            .collect();
        let dark = to_dark(&s, false).expect("dark");
        assert_eq!(dark.op, "store");
        let back = to_regular(&dark.bytes).expect("regular");
        assert_eq!(back.bytes, s);
        let again = to_dark(&back.bytes, false).expect("dark2");
        assert_eq!(again.bytes, dark.bytes);
    }

    #[test]
    fn already_dark_to_dark_does_not_flip() {
        let s = vec![0u8; 4096];
        let dark = to_dark(&s, false).expect("dark");
        let stay = to_dark(&dark.bytes, false).expect("stay");
        assert_eq!(stay.face, Face::Dark);
        assert_eq!(stay.bytes, dark.bytes);
    }

    #[test]
    fn stream_tiles_are_dark() {
        let mut s = vec![0u8; pcc::TILE];
        s.extend((0..pcc::TILE as u32).map(|i| (i.wrapping_mul(1103515245).wrapping_add(12345) >> 16) as u8));
        let dark = to_dark(&s, true).expect("stream");
        assert_eq!(dark.op, "stream");
        assert_eq!(to_regular(&dark.bytes).expect("reg").bytes, s);
    }

    #[test]
    fn champ_tru8_is_regular_to_codex() {
        let s = vec![0u8; 4096];
        let champ = crate::encode_window(&s, crate::parse::DEFAULT_WINDOW as u32).expect("champ");
        assert_eq!(Face::of(&champ), Face::Regular);
        assert_eq!(&champ[..3], b"TR8");
    }
}
