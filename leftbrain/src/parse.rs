//! Whole-file hash-chain finder. Cap 128, best-32, exact + DNA complement.

const HASH_BITS: usize = 20;
const HASH_SIZE: usize = 1 << HASH_BITS;
const CAP: usize = 128;

#[derive(Clone, Copy, Debug)]
pub struct Match {
    pub dist: u32,
    pub len: u32,
    /// Complement strand: data[a+k] == !data[b+k]. Cheap like a rep, not a far copy.
    pub is_comp: bool,
}

pub struct Finder {
    prevc: Vec<i32>,
    head: Vec<i32>,
}

impl Finder {
    pub fn new(n: usize) -> Self {
        Self {
            prevc: vec![-1; n],
            head: vec![-1; HASH_SIZE],
        }
    }

    pub fn build(&mut self, data: &[u8]) {
        if data.len() < 3 {
            return;
        }
        let hashes: Vec<u32> = (0..data.len().saturating_sub(2))
            .map(|i| {
                ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32)
            })
            .collect();
        let mut head = vec![-1i32; HASH_SIZE];
        for (i, h) in hashes.iter().enumerate() {
            let slot = (*h as usize) & (HASH_SIZE - 1);
            self.prevc[i] = head[slot];
            head[slot] = i as i32;
        }
        self.head = head;
    }

    pub fn find_at(&self, data: &[u8], pos: usize) -> Vec<Match> {
        if pos + 3 >= data.len() {
            return Vec::new();
        }
        let h = (((data[pos] as usize) << 16)
            | ((data[pos + 1] as usize) << 8)
            | (data[pos + 2] as usize))
            & (HASH_SIZE - 1);
        let h_comp = (((!data[pos] as usize) << 16)
            | ((!data[pos + 1] as usize) << 8)
            | (!data[pos + 2] as usize))
            & (HASH_SIZE - 1);
        let mut out = Vec::with_capacity(32);
        Self::walk_chain(&self.head, &self.prevc, h, pos, |dist, p| {
            let len_exact = Self::match_len_u64(data, pos, p);
            if len_exact >= 3 {
                out.push(Match {
                    dist,
                    len: len_exact,
                    is_comp: false,
                });
            }
        });
        Self::walk_chain(&self.head, &self.prevc, h_comp, pos, |dist, p| {
            let len_comp = Self::match_len_u64_comp(data, pos, p);
            if len_comp >= 3 {
                out.push(Match {
                    dist,
                    len: len_comp,
                    is_comp: true,
                });
            }
        });
        if out.len() > 32 {
            out.sort_by(|a, b| b.len.cmp(&a.len).then_with(|| a.is_comp.cmp(&b.is_comp)));
            out.truncate(32);
        }
        out
    }

    fn walk_chain<F: FnMut(u32, usize)>(
        head: &[i32],
        prevc: &[i32],
        h: usize,
        pos: usize,
        mut visit: F,
    ) {
        let mut p = head[h];
        let mut steps = 0i32;
        while p > -1 && steps < CAP as i32 {
            let dist = (pos as i32 - p) as u32;
            if dist > 0 && dist <= 1 << 26 {
                visit(dist, p as usize);
            }
            if (p as usize) >= prevc.len() {
                break;
            }
            p = prevc[p as usize];
            steps += 1;
        }
    }

    fn match_len_u64(data: &[u8], a: usize, b: usize) -> u32 {
        if a >= data.len() || b >= data.len() {
            return 0;
        }
        if data[a] != data[b] || a + 1 >= data.len() || b + 1 >= data.len() || data[a + 1] != data[b + 1] {
            return 0;
        }
        let max = (data.len() - a).min(65_535);
        let mut len = 0usize;
        while len + 8 <= max && b + len + 8 <= data.len() {
            let mut av = [0u8; 8];
            let mut bv = [0u8; 8];
            av.copy_from_slice(&data[a + len..a + len + 8]);
            bv.copy_from_slice(&data[b + len..b + len + 8]);
            if av == bv {
                len += 8;
            } else {
                let xor = u64::from_ne_bytes(av) ^ u64::from_ne_bytes(bv);
                len += (xor.trailing_zeros() / 8) as usize;
                break;
            }
        }
        while len < max && b + len < data.len() && data[a + len] == data[b + len] {
            len += 1;
        }
        len as u32
    }

    /// DNA complement: A<->T and C<->G is bitwise NOT on the byte.
    fn match_len_u64_comp(data: &[u8], a: usize, b: usize) -> u32 {
        if a >= data.len() || b >= data.len() {
            return 0;
        }
        let max = (data.len() - a).min(65_535);
        let mut len = 0usize;
        while len + 8 <= max && b + len + 8 <= data.len() {
            let mut av = [0u8; 8];
            let mut bv = [0u8; 8];
            av.copy_from_slice(&data[a + len..a + len + 8]);
            bv.copy_from_slice(&data[b + len..b + len + 8]);
            let a64 = u64::from_ne_bytes(av);
            let b64 = !u64::from_ne_bytes(bv);
            if a64 == b64 {
                len += 8;
            } else {
                let xor = a64 ^ b64;
                len += (xor.trailing_zeros() / 8) as usize;
                break;
            }
        }
        while len < max && b + len < data.len() && data[a + len] == !data[b + len] {
            len += 1;
        }
        len as u32
    }
}

pub fn matches_for(data: &[u8]) -> Vec<Vec<Match>> {
    let mut f = Finder::new(data.len());
    f.build(data);
    let mut all = Vec::with_capacity(data.len());
    for pos in 0..data.len() {
        all.push(f.find_at(data, pos));
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_repeat() {
        let data = b"abcabcabcabc".repeat(20);
        let m = matches_for(&data);
        let hits: usize = m.iter().map(|v| v.len()).sum();
        assert!(hits > 0);
        let long = m.iter().flatten().map(|x| x.len).max().unwrap_or(0);
        assert!(long >= 9);
    }

    #[test]
    fn finds_dna_complement() {
        let mut data = vec![0u8; 64];
        for i in 32..64 {
            data[i] = !data[i - 32];
        }
        let m = matches_for(&data);
        let comp = m.iter().flatten().any(|x| x.is_comp && x.len >= 8);
        assert!(comp, "expected complement match on 0x00 vs 0xFF run");
    }
}
