//! Scalar beam-4 DP. Cheap lit/rep/far costs. C ABI names kept.

#[derive(Clone, Copy, Debug)]
pub struct Match {
    pub dist: u32,
    pub len: u32,
    pub is_comp: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct RepState(pub [u32; 4]);

#[derive(Clone, Copy, Debug)]
pub struct SmallCostsC {
    pub lit_cost: u32,
    pub rep_cost: u32,
    pub rep_len_mult: f32,
}

impl Default for SmallCostsC {
    fn default() -> Self {
        Self {
            lit_cost: 5,
            rep_cost: 3,
            rep_len_mult: 1.0,
        }
    }
}

impl SmallCostsC {
    pub fn price_lit(&self, _phi: u32, _prev: u8, _pos: u32) -> u32 {
        self.lit_cost
    }

    pub fn price_rep(&self, len: u32) -> u32 {
        let base = match len {
            3..=10 => 4,
            11..=18 => 6,
            19..=36 => 10,
            _ => 12,
        };
        (base as f32 * self.rep_len_mult) as u32 + self.rep_cost
    }

    pub fn price_far(&self, len: u32, dist: u32) -> u32 {
        let slot = match dist {
            0..=7 => 3,
            8..=63 => 5,
            64..=511 => 7,
            _ => 9,
        };
        let len_c = match len {
            3..=10 => 4,
            11..=18 => 6,
            19..=36 => 10,
            _ => 12,
        };
        slot + len_c + (dist as f32).log2() as u32 / 2
    }
}

#[derive(Clone, Copy)]
struct BeamState {
    cost: u64,
    rep: RepState,
    via_d: u32,
    via_l: u32,
    prev: i32,
}

fn is_rep(rep: &RepState, dist: u32) -> bool {
    dist > 0 && dist < 8 && rep.0.iter().any(|&r| r == dist)
}

fn push_rep(rep: RepState, dist: u32) -> RepState {
    let mut r = [dist, 0, 0, 0];
    let mut i = 1usize;
    for old in rep.0 {
        if old != dist && i < 4 {
            r[i] = old;
            i += 1;
        }
    }
    RepState(r)
}

/// Scalar DP, beam-4 full-tuple rep[4]. Returns (best_d, best_l, rep_at).
pub fn price_block_c(
    data: &[u8],
    matches: &[Vec<Match>],
    beam: usize,
) -> (Vec<u32>, Vec<u32>, Vec<RepState>) {
    let n = data.len();
    let beam = beam.max(1).min(8);
    let costs = SmallCostsC::default();
    let inf = u64::MAX / 4;
    let mut beams: Vec<Vec<BeamState>> = vec![Vec::new(); n + 1];
    beams[0].push(BeamState {
        cost: 0,
        rep: RepState([0, 0, 0, 0]),
        via_d: 0,
        via_l: 0,
        prev: -1,
    });

    for pos in 0..n {
        let slot = std::mem::take(&mut beams[pos]);
        if slot.is_empty() {
            continue;
        }
        for st in slot {
            let lit = st.cost + costs.price_lit(0, data[pos], pos as u32) as u64;
            consider(&mut beams[pos + 1], beam, BeamState {
                cost: lit,
                rep: st.rep,
                via_d: 0,
                via_l: 1,
                prev: pos as i32,
            });
            if pos < matches.len() {
                for m in &matches[pos] {
                    let len = m.len as usize;
                    if len < 3 || pos + len > n {
                        continue;
                    }
                    let p = if m.is_comp || is_rep(&st.rep, m.dist) {
                        costs.price_rep(m.len)
                    } else {
                        costs.price_far(m.len, m.dist)
                    };
                    consider(&mut beams[pos + len], beam, BeamState {
                        cost: st.cost + p as u64,
                        rep: push_rep(st.rep, m.dist),
                        via_d: m.dist,
                        via_l: m.len,
                        prev: pos as i32,
                    });
                }
            }
        }
        let _ = inf;
    }

    let mut best_d = vec![0u32; n + 1];
    let mut best_l = vec![0u32; n + 1];
    let mut reps = vec![RepState([0, 0, 0, 0]); n + 1];
    let mut cur = n;
    if let Some(win) = beams[n].iter().min_by_key(|s| s.cost).cloned() {
        let mut st = win;
        loop {
            best_d[cur] = st.via_d;
            best_l[cur] = st.via_l;
            reps[cur] = st.rep;
            if st.prev < 0 {
                break;
            }
            let p = st.prev as usize;
            let want_d = st.via_d;
            let want_l = st.via_l;
            let pred = beams[p]
                .iter()
                .find(|s| {
                    let nxt = if want_l <= 1 { p + 1 } else { p + want_l as usize };
                    nxt == cur && s.cost <= st.cost
                })
                .cloned();
            if let Some(prev_st) = pred {
                let _ = (want_d, want_l);
                st = prev_st;
                cur = p;
            } else if st.prev >= 0 {
                cur = st.prev as usize;
                if beams[cur].is_empty() {
                    break;
                }
                st = beams[cur][0];
            } else {
                break;
            }
        }
    }
    (best_d, best_l, reps)
}

fn consider(slot: &mut Vec<BeamState>, beam: usize, st: BeamState) {
    if let Some(ex) = slot.iter_mut().find(|s| s.rep.0 == st.rep.0) {
        if st.cost < ex.cost {
            *ex = st;
        }
        return;
    }
    slot.push(st);
    if slot.len() > beam {
        slot.sort_by_key(|s| s.cost);
        slot.truncate(beam);
    }
}

#[no_mangle]
pub extern "C" fn price_reset_c() {}

#[no_mangle]
pub extern "C" fn price_block_c_abi(
    _n: u32,
    _beam: u32,
    _out_cost: *mut u64,
) -> i32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_match() {
        let data = b"abcabcabc";
        let mut matches = vec![Vec::new(); data.len()];
        matches[3].push(Match {
            dist: 3,
            len: 6,
            is_comp: false,
        });
        let (d, l, _) = price_block_c(data, &matches, 4);
        assert!(l.iter().any(|&x| x >= 3) || d.iter().any(|&x| x == 3));
    }
}
