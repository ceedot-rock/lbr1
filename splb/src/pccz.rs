//! `.pcc` archive — our zip. Own container. Own occupants. Not PKZIP. Not deflate.
//!
//! File starts with `PCCZ`. Members are house blobs (or stored raw if nothing
//! shrinks). Central directory + end record at the tail, like zip, so you can
//! list without reading every payload.
//!
//! Public extension is `.pcc`. Magics stay in this crate.

use crate::{blob_kind, decode, encode_best};

pub const MAGIC: &[u8; 4] = b"PCCZ";
pub const VER: u8 = 1;
pub const LOC: &[u8; 4] = b"PCCm";
pub const CD: &[u8; 4] = b"PCCd";
pub const EOCD: &[u8; 4] = b"PCCz";

pub const FLAG_DIR: u16 = 1;
pub const FLAG_STORED: u16 = 2;

const EOCD_LEN: usize = 37;

#[derive(Clone, Debug)]
pub struct Member {
    pub name: String,
    pub data: Vec<u8>,
    pub mtime: u32,
    pub mode: u16,
    pub dir: bool,
}

#[derive(Clone, Debug)]
pub struct ListEntry {
    pub name: String,
    pub raw_len: u64,
    pub packed_len: u64,
    pub crc32: u32,
    pub stored: bool,
    pub dir: bool,
    pub occupant: &'static str,
}

pub fn is_pccz(buf: &[u8]) -> bool {
    buf.len() >= 8 && buf.starts_with(MAGIC) && buf[4] == VER
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

pub fn clean_name(name: &str) -> Result<String, &'static str> {
    let s = name.replace('\\', "/");
    if s.is_empty() || s.len() > 4096 {
        return Err("pcc name");
    }
    if s.starts_with('/') || s.contains('\0') {
        return Err("pcc name");
    }
    for part in s.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err("pcc name");
        }
    }
    Ok(s)
}

fn put_u16(out: &mut Vec<u8>, n: u16) {
    out.extend_from_slice(&n.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, n: u32) {
    out.extend_from_slice(&n.to_le_bytes());
}
fn put_u64(out: &mut Vec<u8>, n: u64) {
    out.extend_from_slice(&n.to_le_bytes());
}
fn take_u16(buf: &[u8], i: &mut usize) -> Result<u16, &'static str> {
    if *i + 2 > buf.len() {
        return Err("pcc u16");
    }
    let n = u16::from_le_bytes(buf[*i..*i + 2].try_into().unwrap());
    *i += 2;
    Ok(n)
}
fn take_u32(buf: &[u8], i: &mut usize) -> Result<u32, &'static str> {
    if *i + 4 > buf.len() {
        return Err("pcc u32");
    }
    let n = u32::from_le_bytes(buf[*i..*i + 4].try_into().unwrap());
    *i += 4;
    Ok(n)
}
fn take_u64(buf: &[u8], i: &mut usize) -> Result<u64, &'static str> {
    if *i + 8 > buf.len() {
        return Err("pcc u64");
    }
    let n = u64::from_le_bytes(buf[*i..*i + 8].try_into().unwrap());
    *i += 8;
    Ok(n)
}

fn occupant_of(packed: &[u8], stored: bool) -> &'static str {
    if stored {
        "store"
    } else {
        blob_kind(packed)
    }
}

/// Pack members into a `.pcc` archive. Each file runs the house; expand → store.
pub fn zip_bytes(members: &[Member]) -> Result<Vec<u8>, &'static str> {
    if members.len() > 100_000 {
        return Err("pcc too many");
    }
    let mut out = Vec::from(*MAGIC);
    out.push(VER);
    out.push(0);
    put_u16(&mut out, 0);

    struct Rec {
        name: String,
        flags: u16,
        mtime: u32,
        mode: u16,
        raw_len: u64,
        packed_len: u64,
        crc: u32,
        data_off: u64,
        packed: Vec<u8>,
    }
    let mut recs = Vec::with_capacity(members.len());
    for m in members {
        let name = clean_name(&m.name)?;
        let mut flags = 0u16;
        if m.dir {
            flags |= FLAG_DIR;
        }
        let raw = if m.dir { &[][..] } else { m.data.as_slice() };
        let crc = crc32(raw);
        let (packed, stored) = if m.dir || raw.is_empty() {
            (Vec::new(), true)
        } else if let Some((b, _)) = encode_best(raw) {
            if b.len() < raw.len() && decode(&b).ok().as_deref() == Some(raw) {
                (b, false)
            } else {
                (raw.to_vec(), true)
            }
        } else {
            (raw.to_vec(), true)
        };
        if stored {
            flags |= FLAG_STORED;
        }
        recs.push(Rec {
            name,
            flags,
            mtime: m.mtime,
            mode: m.mode,
            raw_len: raw.len() as u64,
            packed_len: packed.len() as u64,
            crc,
            data_off: 0,
            packed,
        });
    }

    for r in &mut recs {
        r.data_off = out.len() as u64;
        out.extend_from_slice(LOC);
        put_u16(&mut out, r.flags);
        let nb = r.name.as_bytes();
        put_u16(&mut out, nb.len() as u16);
        out.extend_from_slice(nb);
        put_u32(&mut out, r.mtime);
        put_u16(&mut out, r.mode);
        put_u64(&mut out, r.raw_len);
        put_u64(&mut out, r.packed_len);
        put_u32(&mut out, r.crc);
        out.extend_from_slice(&r.packed);
    }

    let cd_offset = out.len() as u64;
    out.extend_from_slice(CD);
    put_u32(&mut out, recs.len() as u32);
    let mut total_raw = 0u64;
    let mut total_packed = 0u64;
    for r in &recs {
        put_u16(&mut out, r.flags);
        let nb = r.name.as_bytes();
        put_u16(&mut out, nb.len() as u16);
        out.extend_from_slice(nb);
        put_u32(&mut out, r.mtime);
        put_u16(&mut out, r.mode);
        put_u64(&mut out, r.raw_len);
        put_u64(&mut out, r.packed_len);
        put_u32(&mut out, r.crc);
        put_u64(&mut out, r.data_off);
        total_raw += r.raw_len;
        total_packed += r.packed_len;
    }
    let cd_size = (out.len() as u64 - cd_offset) as u32;

    out.extend_from_slice(EOCD);
    out.push(VER);
    put_u32(&mut out, recs.len() as u32);
    put_u64(&mut out, cd_offset);
    put_u32(&mut out, cd_size);
    put_u64(&mut out, total_raw);
    put_u64(&mut out, total_packed);
    Ok(out)
}

fn eocd(buf: &[u8]) -> Result<(u32, u64, u32), &'static str> {
    if buf.len() < EOCD_LEN {
        return Err("pcc short");
    }
    let i = buf.len() - EOCD_LEN;
    if &buf[i..i + 4] != EOCD {
        return Err("pcc eocd");
    }
    if buf[i + 4] != VER {
        return Err("pcc ver");
    }
    let mut j = i + 5;
    let n = take_u32(buf, &mut j)?;
    let cd_off = take_u64(buf, &mut j)?;
    let cd_size = take_u32(buf, &mut j)?;
    Ok((n, cd_off, cd_size))
}

struct CdEnt {
    flags: u16,
    name: String,
    mtime: u32,
    mode: u16,
    raw_len: u64,
    packed_len: u64,
    crc: u32,
    data_off: u64,
}

fn read_cd(buf: &[u8]) -> Result<Vec<CdEnt>, &'static str> {
    let (n, cd_off, cd_size) = eocd(buf)?;
    let start = cd_off as usize;
    let end = start.checked_add(cd_size as usize).ok_or("pcc cd")?;
    if end > buf.len() {
        return Err("pcc cd");
    }
    let cd = &buf[start..end];
    if cd.len() < 8 || &cd[..4] != CD {
        return Err("pcc cd magic");
    }
    let mut i = 4usize;
    let n2 = take_u32(cd, &mut i)?;
    if n2 != n {
        return Err("pcc n");
    }
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let flags = take_u16(cd, &mut i)?;
        let nl = take_u16(cd, &mut i)? as usize;
        if i + nl > cd.len() {
            return Err("pcc name");
        }
        let name = std::str::from_utf8(&cd[i..i + nl]).map_err(|_| "pcc utf8")?;
        let name = clean_name(name)?;
        i += nl;
        let mtime = take_u32(cd, &mut i)?;
        let mode = take_u16(cd, &mut i)?;
        let raw_len = take_u64(cd, &mut i)?;
        let packed_len = take_u64(cd, &mut i)?;
        let crc = take_u32(cd, &mut i)?;
        let data_off = take_u64(cd, &mut i)?;
        out.push(CdEnt {
            flags,
            name,
            mtime,
            mode,
            raw_len,
            packed_len,
            crc,
            data_off,
        });
    }
    Ok(out)
}

fn read_local<'a>(buf: &'a [u8], off: u64, expect: &CdEnt) -> Result<&'a [u8], &'static str> {
    let mut i = off as usize;
    if i + 4 > buf.len() || &buf[i..i + 4] != LOC {
        return Err("pcc local");
    }
    i += 4;
    let flags = take_u16(buf, &mut i)?;
    let nl = take_u16(buf, &mut i)? as usize;
    if i + nl > buf.len() {
        return Err("pcc local name");
    }
    i += nl;
    let _mtime = take_u32(buf, &mut i)?;
    let _mode = take_u16(buf, &mut i)?;
    let raw_len = take_u64(buf, &mut i)?;
    let packed_len = take_u64(buf, &mut i)?;
    let crc = take_u32(buf, &mut i)?;
    if flags != expect.flags
        || raw_len != expect.raw_len
        || packed_len != expect.packed_len
        || crc != expect.crc
    {
        return Err("pcc local mismatch");
    }
    let end = i
        .checked_add(packed_len as usize)
        .ok_or("pcc packed")?;
    if end > buf.len() {
        return Err("pcc packed");
    }
    Ok(&buf[i..end])
}

pub fn list_bytes(buf: &[u8]) -> Result<Vec<ListEntry>, &'static str> {
    if !is_pccz(buf) {
        return Err("not pcc archive");
    }
    let cd = read_cd(buf)?;
    let mut out = Vec::with_capacity(cd.len());
    for e in cd {
        let packed = read_local(buf, e.data_off, &e)?;
        let stored = e.flags & FLAG_STORED != 0;
        out.push(ListEntry {
            name: e.name,
            raw_len: e.raw_len,
            packed_len: e.packed_len,
            crc32: e.crc,
            stored,
            dir: e.flags & FLAG_DIR != 0,
            occupant: occupant_of(packed, stored),
        });
    }
    Ok(out)
}

pub fn unzip_bytes(buf: &[u8]) -> Result<Vec<Member>, &'static str> {
    if !is_pccz(buf) {
        return Err("not pcc archive");
    }
    let cd = read_cd(buf)?;
    let mut out = Vec::with_capacity(cd.len());
    for e in cd {
        let packed = read_local(buf, e.data_off, &e)?;
        let dir = e.flags & FLAG_DIR != 0;
        let stored = e.flags & FLAG_STORED != 0;
        let data = if dir {
            Vec::new()
        } else if stored {
            packed.to_vec()
        } else {
            decode(packed)?
        };
        if data.len() as u64 != e.raw_len {
            return Err("pcc raw_len");
        }
        if crc32(&data) != e.crc {
            return Err("pcc crc");
        }
        out.push(Member {
            name: e.name,
            data,
            mtime: e.mtime,
            mode: e.mode,
            dir,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_unzip_zeros_and_text() {
        let members = vec![
            Member {
                name: "a/zeros.bin".into(),
                data: vec![0u8; 1000],
                mtime: 1,
                mode: 0o644,
                dir: false,
            },
            Member {
                name: "readme.txt".into(),
                data: b"hello pcc archive\n".to_vec(),
                mtime: 2,
                mode: 0o644,
                dir: false,
            },
            Member {
                name: "empty".into(),
                data: vec![],
                mtime: 3,
                mode: 0o755,
                dir: true,
            },
        ];
        let blob = zip_bytes(&members).expect("zip");
        assert!(is_pccz(&blob));
        assert!(blob.starts_with(MAGIC));
        let back = unzip_bytes(&blob).expect("unzip");
        assert_eq!(back.len(), 3);
        assert_eq!(back[0].name, "a/zeros.bin");
        assert_eq!(back[0].data, vec![0u8; 1000]);
        assert_eq!(back[1].data, b"hello pcc archive\n");
        assert!(back[2].dir);
        let ls = list_bytes(&blob).expect("ls");
        assert_eq!(ls[0].raw_len, 1000);
        assert!(ls[0].packed_len < 1000);
        assert!(!ls[0].stored);
    }

    #[test]
    fn reject_zip_slip() {
        let bad = Member {
            name: "../etc/passwd".into(),
            data: b"x".to_vec(),
            mtime: 0,
            mode: 0o644,
            dir: false,
        };
        assert!(zip_bytes(&[bad]).is_err());
        assert!(clean_name("/abs").is_err());
        assert!(clean_name("a/../b").is_err());
    }

    #[test]
    fn crc_ieee_known() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
