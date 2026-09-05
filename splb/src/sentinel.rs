//! Sentinel Autonoma NCA at entropy breakpoints.
//!
//! Controller, not a gene. Distill-style local rule for Throttle/Block only.
//! Does not emit bytes. rANS ignores the tick. See docs/GAPS_NCA_SWARM.md.
//!
//! Locked public rule (ZRQC Runtime v4.2 / dark-degree):
//!     new = decay * (0.6 * self + 0.4 * neighbours) + sensitivity * observation
//!     clamp cells to [-2, 2]
//!
//! Surfaces: residual, mirror, scheduler, fileio, snapshot.
//! Does not ship production residual-attack weights.
//! Decode never mutates symbols — sentinel only supports / alleviates work
//! and aborts on a broken mirror.

pub const SURFACES: [&str; 5] = ["residual", "mirror", "scheduler", "fileio", "snapshot"];
const NEI: [(usize, usize); 5] = [(4, 1), (0, 2), (1, 3), (2, 4), (3, 0)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    AllowZero,
    Alleviate,
    Support,
    Throttle,
    Block,
}

impl Decision {
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::AllowZero => "ALLOW_ZERO",
            Decision::Alleviate => "ALLEVIATE",
            Decision::Support => "SUPPORT",
            Decision::Throttle => "THROTTLE",
            Decision::Block => "BLOCK",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tick {
    pub cells: [f64; 5],
    pub dark: [f64; 5],
    pub awareness: f64,
    pub at_rest: bool,
    pub decision: Decision,
}

pub struct Sentinel {
    cells: [f64; 5],
    ticks: u32,
}

impl Default for Sentinel {
    fn default() -> Self {
        Self {
            cells: [0.0; 5],
            ticks: 0,
        }
    }
}

impl Sentinel {
    pub fn new() -> Self {
        Self::default()
    }

    fn clamp(x: f64) -> f64 {
        x.clamp(-2.0, 2.0)
    }

    pub fn step(&mut self, obs: [f64; 5], energy: f64, mirror_error: f64) -> Tick {
        const DECAY: f64 = 0.85;
        const SELF_W: f64 = 0.6;
        const NEIGH_W: f64 = 0.4;
        const SENS: f64 = 0.35;
        let mut nxt = [0.0; 5];
        let mut dark = [0.0; 5];
        for i in 0..5 {
            let (l, r) = NEI[i];
            let neigh = 0.5 * (self.cells[l] + self.cells[r]);
            let used = SELF_W * self.cells[i] + NEIGH_W * neigh;
            let discarded = (1.0 - NEIGH_W) * neigh + (1.0 - SELF_W) * self.cells[i];
            nxt[i] = Self::clamp(DECAY * used + SENS * obs[i]);
            let frac = nxt[i].abs() - nxt[i].abs().floor();
            dark[i] = 0.5 * discarded.abs() + 0.5 * frac;
        }
        self.cells = nxt;
        self.ticks += 1;
        let aw = self.cells.iter().map(|c| c.abs()).sum::<f64>() / 5.0;
        let at_rest = aw < 0.02 && energy.abs() < 1e-6 && mirror_error.abs() < 1e-6;
        let decision = decide(energy, aw, mirror_error);
        Tick {
            cells: self.cells,
            dark,
            awareness: aw,
            at_rest,
            decision,
        }
    }

    /// Histogram + size-tax observation used at encode/decode gates.
    pub fn observe_stream(data: &[u8], coded_hint: usize) -> [f64; 5] {
        if data.is_empty() {
            return [0.0; 5];
        }
        let mut cnt = [0u32; 256];
        for &b in data {
            cnt[b as usize] += 1;
        }
        let n = data.len() as f64;
        let mut used = 0u32;
        let mut maxc = 0u32;
        for &c in &cnt {
            if c > 0 {
                used += 1;
            }
            if c > maxc {
                maxc = c;
            }
        }
        let residual = 1.0 - (maxc as f64 / n);
        let mirror = 0.0;
        let scheduler = used as f64 / 256.0;
        let fileio = (coded_hint as f64 / n).clamp(0.0, 2.0);
        let snapshot = if used <= 1 { 0.0 } else { residual * scheduler };
        [residual, mirror, scheduler, fileio, snapshot]
    }
}

fn decide(e: f64, aw: f64, me: f64) -> Decision {
    if e.abs() < 1e-6 && me.abs() < 1e-6 {
        return Decision::AllowZero;
    }
    let thresh = (1.0 - 0.7 * aw).clamp(0.15, 1.0);
    if me.abs() >= 4.0 * thresh || e.abs() >= 4.0 * thresh {
        Decision::Block
    } else if e.abs() >= 1.5 * thresh {
        Decision::Throttle
    } else if aw >= 0.35 || e > 0.25 {
        Decision::Support
    } else {
        Decision::Alleviate
    }
}

/// Encode-side: skip the expensive o1 pass when the grid is quiet and the
/// alphabet is tiny. Always still keep a correct o0 stream.
pub fn alleviate_skip_o1(tick: &Tick, nlit: usize, alphabet_hint: usize) -> bool {
    matches!(tick.decision, Decision::Alleviate | Decision::AllowZero)
        && (tick.at_rest || alphabet_hint <= 8 || nlit < 64)
}

/// Encode-side: force trying o1 even if we might throw it away.
pub fn support_try_o1(tick: &Tick) -> bool {
    matches!(tick.decision, Decision::Support | Decision::Throttle)
}

/// Decode-side watchdog. `ok` is false on table-sum / underrun / magic fail.
pub fn decode_breakpoint(tick: &Tick, mirror_ok: bool) -> Result<(), &'static str> {
    if !mirror_ok || tick.decision == Decision::Block {
        return Err("sentinel: mirror");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_on_zeros() {
        let mut s = Sentinel::new();
        let obs = Sentinel::observe_stream(&[0u8; 256], 16);
        let t = s.step(obs, obs[0], 0.0);
        assert!(t.awareness < 2.0);
        assert_ne!(t.decision, Decision::Block);
    }

    #[test]
    fn block_on_broken_mirror() {
        let mut s = Sentinel::new();
        let t = s.step([0.2; 5], 0.2, 4.0);
        assert_eq!(t.decision, Decision::Block);
        assert!(decode_breakpoint(&t, false).is_err());
    }
}
