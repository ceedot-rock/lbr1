//! Down-range posts. One position per fingerprint per file octant.
//! No NCA gaze — the eye is gone.

pub const BANDS: usize = 8;
pub const HASH_BITS: usize = 16;
pub const HASH_SIZE: usize = 1 << HASH_BITS;

#[inline]
pub fn hash8(data: &[u8], i: usize) -> usize {
    if i + 8 > data.len() {
        return 0;
    }
    let lo = u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
    let hi = u32::from_le_bytes(data[i + 4..i + 8].try_into().unwrap());
    let v = (lo as u64) | ((hi as u64) << 32);
    (v.wrapping_mul(0x9E37_79B1_85EB_CA77) >> (64 - HASH_BITS as u32)) as usize
}

pub struct Sensors {
    posts: Vec<[i32; BANDS]>,
    n: usize,
    stride: usize,
}

pub struct Report {
    pub pos: usize,
    pub band: usize,
}

impl Sensors {
    pub fn new(n: usize, stride: usize) -> Self {
        Self {
            posts: vec![[-1i32; BANDS]; HASH_SIZE],
            n: n.max(1),
            stride: stride.max(1),
        }
    }

    #[inline]
    fn band(&self, pos: usize) -> usize {
        ((pos as u64 * BANDS as u64) / self.n as u64).min((BANDS - 1) as u64) as usize
    }

    pub fn plant(&mut self, data: &[u8], pos: usize) {
        if pos % self.stride != 0 || pos + 8 > data.len() {
            return;
        }
        let h = hash8(data, pos);
        let b = self.band(pos);
        self.posts[h][b] = pos as i32;
    }

    pub fn reports(&self, data: &[u8], pos: usize) -> impl Iterator<Item = Report> + '_ {
        let h = if pos + 8 <= data.len() {
            hash8(data, pos)
        } else {
            0
        };
        let row = &self.posts[h];
        (0..BANDS).filter_map(move |b| {
            let p = row[b];
            if p >= 0 {
                Some(Report {
                    pos: p as usize,
                    band: b,
                })
            } else {
                None
            }
        })
    }
}
