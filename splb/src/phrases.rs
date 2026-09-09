//! Phrase tiles — seekable log codes. Lab-owned.
//!
//! Each line encodes against a per-tile symbol table so line *i* decompresses
//! without the neighbors. Inspired by random-access string tables (FSST's
//! idea), not their code. Not a bitstream wrap of zstd/lz4.
//!
//! Wire: `PHR1` + table + u16 line offsets + escaped payload.

pub const MAGIC: &[u8; 4] = b"PHR1";
pub const ESC: u8 = 255;
const MAX_SYM: usize = 200;
const MAX_NGRAM: usize = 6;

#[derive(Clone, Debug)]
pub struct Table {
    pub symbols: Vec<Vec<u8>>,
}

impl Table {
    pub fn decode_line(&self, coded: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(coded.len() * 2);
        let mut i = 0;
        while i < coded.len() {
            let c = coded[i];
            i += 1;
            if c == ESC {
                if i < coded.len() {
                    out.push(coded[i]);
                    i += 1;
                }
            } else if (c as usize) < self.symbols.len() {
                out.extend_from_slice(&self.symbols[c as usize]);
            }
        }
        out
    }
}

fn count_ngrams(data: &[u8]) -> Vec<(Vec<u8>, u32)> {
    use std::collections::HashMap;
    let mut map: HashMap<Vec<u8>, u32> = HashMap::new();
    for n in 2..=MAX_NGRAM {
        if data.len() < n {
            continue;
        }
        for w in data.windows(n) {
            *map.entry(w.to_vec()).or_insert(0) += 1;
        }
    }
    let mut v: Vec<(Vec<u8>, u32)> = map
        .into_iter()
        .filter(|(s, c)| *c >= 3 && s.len() >= 2)
        .collect();
    v.sort_by(|a, b| {
        let ga = (a.0.len() as u32 - 1).saturating_mul(a.1);
        let gb = (b.0.len() as u32 - 1).saturating_mul(b.1);
        gb.cmp(&ga)
    });
    v.truncate(MAX_SYM);
    v
}

pub fn train(data: &[u8]) -> Table {
    let ranked = count_ngrams(data);
    Table {
        symbols: ranked.into_iter().map(|(s, _)| s).collect(),
    }
}

fn encode_slice(table: &Table, src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        let mut best = 0usize;
        let mut best_code = None;
        for (code, sym) in table.symbols.iter().enumerate() {
            if code >= ESC as usize {
                break;
            }
            if i + sym.len() <= src.len() && src[i..].starts_with(sym) && sym.len() > best {
                best = sym.len();
                best_code = Some(code as u8);
            }
        }
        if let Some(c) = best_code {
            out.push(c);
            i += best;
        } else {
            out.push(ESC);
            out.push(src[i]);
            i += 1;
        }
    }
    out
}

fn lines_of(data: &[u8]) -> Vec<&[u8]> {
    if data.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, &b) in data.iter().enumerate() {
        if b == b'\n' {
            lines.push(&data[start..=i]);
            start = i + 1;
        }
    }
    if start < data.len() {
        lines.push(&data[start..]);
    }
    lines
}

/// Encode a tile. Returns None if it does not beat STORE (or is not text).
pub fn encode(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 32 || data.len() > 64 * 1024 {
        return None;
    }
    let print = data
        .iter()
        .filter(|&&b| (32..127).contains(&b) || b == 9 || b == 10 || b == 13)
        .count();
    if print * 100 / data.len() < 80 {
        return None;
    }
    let table = train(data);
    if table.symbols.is_empty() {
        return None;
    }
    let lines = lines_of(data);
    if lines.len() < 2 || lines.len() > 4096 {
        return None;
    }
    let mut coded: Vec<Vec<u8>> = Vec::with_capacity(lines.len());
    let mut payload_len = 0usize;
    for line in &lines {
        let c = encode_slice(&table, line);
        payload_len += c.len();
        coded.push(c);
    }
    let mut blob = Vec::from(*MAGIC);
    blob.push(table.symbols.len() as u8);
    for s in &table.symbols {
        blob.push(s.len() as u8);
        blob.extend_from_slice(s);
    }
    blob.extend_from_slice(&(lines.len() as u16).to_le_bytes());
    let mut off = 0u16;
    blob.extend_from_slice(&off.to_le_bytes());
    for c in &coded {
        let next = off.checked_add(c.len() as u16)?;
        blob.extend_from_slice(&next.to_le_bytes());
        off = next;
    }
    for c in &coded {
        blob.extend_from_slice(c);
    }
    if blob.len() >= data.len() {
        return None;
    }
    let _ = payload_len;
    Some(blob)
}

pub fn is_phr(buf: &[u8]) -> bool {
    buf.len() >= 8 && buf.starts_with(MAGIC)
}

fn parse_table(buf: &[u8]) -> Result<(Table, usize), &'static str> {
    if !is_phr(buf) {
        return Err("phr magic");
    }
    let n = buf[4] as usize;
    let mut i = 5usize;
    let mut symbols = Vec::with_capacity(n);
    for _ in 0..n {
        if i >= buf.len() {
            return Err("phr table");
        }
        let ln = buf[i] as usize;
        i += 1;
        if ln == 0 || ln > MAX_NGRAM || i + ln > buf.len() {
            return Err("phr sym");
        }
        symbols.push(buf[i..i + ln].to_vec());
        i += ln;
    }
    Ok((Table { symbols }, i))
}

pub fn decode(buf: &[u8]) -> Result<Vec<u8>, &'static str> {
    let (table, mut i) = parse_table(buf)?;
    if i + 2 > buf.len() {
        return Err("phr n");
    }
    let n_lines = u16::from_le_bytes(buf[i..i + 2].try_into().unwrap()) as usize;
    i += 2;
    let offs_bytes = (n_lines + 1) * 2;
    if i + offs_bytes > buf.len() {
        return Err("phr offs");
    }
    let mut offs = Vec::with_capacity(n_lines + 1);
    for _ in 0..=n_lines {
        offs.push(u16::from_le_bytes(buf[i..i + 2].try_into().unwrap()) as usize);
        i += 2;
    }
    let payload = &buf[i..];
    let mut out = Vec::new();
    for k in 0..n_lines {
        let a = offs[k];
        let b = offs[k + 1];
        if a > b || b > payload.len() {
            return Err("phr range");
        }
        out.extend_from_slice(&table.decode_line(&payload[a..b]));
    }
    Ok(out)
}

/// Decompress line `idx` only. Random access — the FSST property we kept.
pub fn decode_line(buf: &[u8], idx: usize) -> Result<Vec<u8>, &'static str> {
    let (table, mut i) = parse_table(buf)?;
    if i + 2 > buf.len() {
        return Err("phr n");
    }
    let n_lines = u16::from_le_bytes(buf[i..i + 2].try_into().unwrap()) as usize;
    i += 2;
    if idx >= n_lines {
        return Err("phr idx");
    }
    let offs_bytes = (n_lines + 1) * 2;
    if i + offs_bytes > buf.len() {
        return Err("phr offs");
    }
    let a_off = i + idx * 2;
    let b_off = i + (idx + 1) * 2;
    let a = u16::from_le_bytes(buf[a_off..a_off + 2].try_into().unwrap()) as usize;
    let b = u16::from_le_bytes(buf[b_off..b_off + 2].try_into().unwrap()) as usize;
    i += offs_bytes;
    let payload = &buf[i..];
    if a > b || b > payload.len() {
        return Err("phr range");
    }
    Ok(table.decode_line(&payload[a..b]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_logs() -> Vec<u8> {
        let line = b"2026-09-09T13:00:00Z INFO agent rider spend cents=8 service=analyze\n";
        let mut s = Vec::new();
        for i in 0..80 {
            s.extend_from_slice(line);
            s.extend_from_slice(format!(" seq={i}\n").as_bytes());
        }
        s
    }

    #[test]
    fn roundtrip_and_seek() {
        let src = sample_logs();
        let blob = encode(&src).expect("phrase should beat store on repeated logs");
        assert!(is_phr(&blob));
        assert!(blob.len() < src.len());
        assert_eq!(decode(&blob).unwrap(), src);
        let lines: Vec<&[u8]> = src.split_inclusive(|&b| b == b'\n').collect();
        let third = decode_line(&blob, 2).unwrap();
        assert_eq!(third, lines[2]);
    }

    #[test]
    fn random_refuses() {
        let r: Vec<u8> = (0..512u32)
            .map(|i| (i.wrapping_mul(1103515245).wrapping_add(12345) >> 16) as u8)
            .collect();
        assert!(encode(&r).is_none());
    }
}
