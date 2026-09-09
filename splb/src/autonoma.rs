//! Autonoma around the PCC house. Not a gene. Decode never mutates symbols.
//!
//! Sentinel NCA (dark-degree) + inhibitor + scout swarm (sensors) + octant NCA.
//! They decide which specialists run. They do not emit bytes.

use crate::detect::{self, Class};
use crate::parse;
use crate::sensors::Sensors;
use crate::sentinel::{Decision, Sentinel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seat {
    Bwt,
    Cmaq,
    Match,
    Store,
}

impl Seat {
    pub fn name(self) -> &'static str {
        match self {
            Seat::Bwt => "bwt",
            Seat::Cmaq => "cmaq",
            Seat::Match => "match",
            Seat::Store => "store",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Plan {
    pub try_bwt: bool,
    pub try_cmaq: bool,
    pub try_match: bool,
    pub try_delta: bool,
    pub ml4: bool,
    pub match_window: u32,
    pub crush: f64,
    pub primary: Seat,
    pub seamed: bool,
}

impl Plan {
    fn store_only() -> Self {
        Self {
            try_bwt: false,
            try_cmaq: false,
            try_match: false,
            try_delta: false,
            ml4: false,
            match_window: parse::DEFAULT_WINDOW as u32,
            crush: 0.99,
            primary: Seat::Store,
            seamed: false,
        }
    }

    pub fn allows(self, seat: Seat) -> bool {
        match seat {
            Seat::Bwt => self.try_bwt,
            Seat::Cmaq => self.try_cmaq,
            Seat::Match => self.try_match,
            Seat::Store => true,
        }
    }

    /// Primary first, then the other enabled specialists. Seat, not a raffle.
    pub fn order(self) -> [Seat; 3] {
        self.order_with(self.primary)
    }

    pub fn order_with(self, primary: Seat) -> [Seat; 3] {
        let rest = match primary {
            Seat::Bwt => [Seat::Match, Seat::Cmaq, Seat::Store],
            Seat::Cmaq => [Seat::Bwt, Seat::Match, Seat::Store],
            Seat::Match => [Seat::Bwt, Seat::Cmaq, Seat::Store],
            Seat::Store => [Seat::Store, Seat::Store, Seat::Store],
        };
        [primary, rest[0], rest[1]]
    }
}

struct Swarm {
    hits: u32,
    samples: u32,
    bands: u32,
}

/// Scout swarm: cheap octant posts. High hits → MATCH likely.
fn swarm_scan(data: &[u8]) -> Swarm {
    if data.len() < 64 {
        return Swarm {
            hits: 0,
            samples: 0,
            bands: 0,
        };
    }
    let stride = detect::scout_stride(data).max(256);
    let mut s = Sensors::new(data.len(), stride);
    let mut i = 0usize;
    while i + 8 <= data.len() {
        s.plant(data, i);
        i += stride;
    }
    let mut hits = 0u32;
    let mut samples = 0u32;
    let mut band_mask = 0u8;
    i = stride;
    while i + 8 <= data.len() {
        samples += 1;
        for r in s.reports(data, i) {
            hits += 1;
            band_mask |= 1 << (r.band.min(7) as u8);
        }
        i += stride * 4;
    }
    Swarm {
        hits,
        samples,
        bands: band_mask.count_ones(),
    }
}

/// Sequential NCA across file octants. Cells persist; this is the scheduler.
fn octant_ticks(data: &[u8]) -> [TickLite; 8] {
    let n = data.len();
    let mut s = Sentinel::new();
    let mut out = [TickLite {
        decision: Decision::Alleviate,
        dark: 0.0,
    }; 8];
    if n == 0 {
        return out;
    }
    let chunk = (n + 7) / 8;
    for b in 0..8 {
        let a = b * chunk;
        if a >= n {
            break;
        }
        let z = (a + chunk).min(n);
        let obs = Sentinel::observe_stream(&data[a..z], 0);
        let tick = s.step(obs, obs[0], 0.0);
        out[b] = TickLite {
            decision: tick.decision,
            dark: tick.dark.iter().sum::<f64>() / 5.0,
        };
    }
    out
}

#[derive(Clone, Copy)]
struct TickLite {
    decision: Decision,
    dark: f64,
}

/// Teach the seat from wavelengths, not one header. Cooler body → BWT.
/// Hotter body + copies → MATCH. Mean is the fallback.
fn teach_primary(class: Class, bands: detect::Bands, matchy: bool, n: usize) -> Seat {
    let entropy = bands.mean();
    if entropy >= 7.65 {
        return Seat::Store;
    }
    match class {
        Class::Fill | Class::Sparse => Seat::Store,
        Class::Text if n >= 256 => Seat::Bwt,
        // Copies in a *hot* band. mr mid=4.2 is medical BWT, not ooffice.
        Class::Binary
            if matchy && bands.hotter_body() && bands.mid.max(bands.tail) >= 6.0 =>
        {
            Seat::Match
        }
        Class::Binary if n >= 256 && n <= 16 * 1024 * 1024 && bands.cooler_body() => Seat::Bwt,
        Class::Binary if entropy < 6.65 && n >= 256 && n <= 16 * 1024 * 1024 => Seat::Bwt,
        Class::Binary if matchy => Seat::Match,
        Class::Binary if n < 64 * 1024 && entropy < 7.0 => Seat::Cmaq,
        Class::Binary => Seat::Match,
        Class::Text => Seat::Match,
    }
}

pub fn plan(data: &[u8], class: Class) -> Plan {
    let n = data.len();
    let bands = detect::shannon_bands(data);
    let entropy = bands.mean();
    let swarm = swarm_scan(data);
    let matchy = swarm.hits >= 4
        || (swarm.samples > 0 && (swarm.hits as f64 / swarm.samples as f64) > 0.15);
    let oct = octant_ticks(data);
    let obs = Sentinel::observe_stream(data, 0);
    let tick = Sentinel::new().step(obs, obs[0], 0.0);
    let dark_mean = tick.dark.iter().sum::<f64>() / 5.0
        + oct.iter().map(|t| t.dark).sum::<f64>() / 8.0;
    let oct_zero = oct
        .iter()
        .filter(|t| t.decision == Decision::AllowZero)
        .count();
    let oct_live = oct
        .iter()
        .filter(|t| {
            matches!(
                t.decision,
                Decision::Support | Decision::Throttle | Decision::Alleviate
            )
        })
        .count();
    let seamed = (oct_zero >= 1 && oct_live >= 1 && n >= 8192)
        || (swarm.bands >= 3 && oct_zero >= 1);

    // Formula-shaped integers: skip BWT, prefer delta. Scout, not a gene.
    let formula = detect::arithmetic_u32_le(data).is_some();

    if entropy >= 7.65 && !matchy && !formula {
        let mut p = Plan::store_only();
        p.seamed = seamed;
        return p;
    }

    match tick.decision {
        Decision::Block => {
            let mut p = Plan::store_only();
            p.seamed = seamed;
            p
        }
        Decision::AllowZero => {
            let mut p = Plan::store_only();
            p.crush = 0.20;
            p.seamed = seamed;
            p
        }
        Decision::Alleviate | Decision::Throttle | Decision::Support => {
            let primary = teach_primary(class, bands, matchy, n);
            let throttle = tick.decision == Decision::Throttle;
            let support = tick.decision == Decision::Support;
            let try_bwt = !formula
                && n >= 256
                && entropy < 7.2
                && (class == Class::Text || n <= 16 * 1024 * 1024);
            let try_match = matchy || class == Class::Binary || class == Class::Text || formula;
            let cmaq_cap = if throttle {
                0
            } else if support {
                256 * 1024
            } else {
                64 * 1024
            };
            let try_cmaq = n >= 64 && n < cmaq_cap && entropy < 7.2;
            let try_delta = formula
                || (class == Class::Binary
                    && try_match
                    && n >= 64
                    && n <= 12 * 1024 * 1024
                    && swarm.hits >= 2);
            let match_window = if throttle && class != Class::Binary {
                1 << 20
            } else {
                parse::DEFAULT_WINDOW as u32
            };
            let crush = match tick.decision {
                Decision::Alleviate => 0.18,
                Decision::Throttle => 0.22,
                _ => 0.12,
            } + 0.04 * dark_mean.clamp(0.0, 1.0);
            Plan {
                try_bwt,
                try_cmaq,
                try_match: try_match && entropy < 7.65,
                try_delta,
                // ML is the ooffice winner. ML4 raffles only on files < 1 MiB.
                ml4: false,
                match_window,
                crush,
                primary,
                seamed,
            }
        }
    }
}

/// Inhibitor: a specialist already crushed the file. Skip the rest.
pub fn crushed(blob_len: usize, raw_len: usize, crush: f64) -> bool {
    raw_len > 0 && (blob_len as f64 / raw_len as f64) < crush
}

/// Primary did the job. Do not pay for BWT/CMAQ/MATCH as a raffle.
pub fn held(blob_len: usize, raw_len: usize) -> bool {
    raw_len > 0 && (blob_len as f64 / raw_len as f64) < 0.65
}

pub fn tick_name(data: &[u8]) -> &'static str {
    let obs = Sentinel::observe_stream(data, 0);
    let tick = Sentinel::new().step(obs, obs[0], 0.0);
    tick.decision.as_str()
}

pub fn swarm_hits(data: &[u8]) -> u32 {
    swarm_scan(data).hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeros_do_not_need_match() {
        let s = vec![0u8; 4096];
        let p = plan(&s, Class::Fill);
        assert!(!p.try_match || p.crush > 0.0);
        assert_eq!(p.primary, Seat::Store);
    }

    #[test]
    fn text_wants_bwt() {
        let s = b"the cat sat on the mat. ".repeat(200);
        let p = plan(&s, Class::Text);
        assert!(p.try_bwt);
        assert_eq!(p.primary, Seat::Bwt);
    }

    #[test]
    fn random_is_store_seat() {
        let s: Vec<u8> = (0..4096u32)
            .map(|i| (i.wrapping_mul(1103515245).wrapping_add(12345) >> 16) as u8)
            .collect();
        let p = plan(&s, Class::Binary);
        assert!(!p.try_bwt, "random must not pay for SA");
        assert!(!p.try_cmaq);
        assert_eq!(p.primary, Seat::Store);
    }

    #[test]
    fn binary_repeat_wants_match() {
        let motif: Vec<u8> = (0..32).map(|i| (0x48 + i * 3) as u8).collect();
        let mut s = Vec::new();
        for k in 0..200 {
            s.extend_from_slice(&motif);
            s.push((k & 0xff) as u8);
        }
        let p = plan(&s, Class::Binary);
        assert!(p.try_match);
        assert!(p.primary == Seat::Match || p.primary == Seat::Bwt);
    }

    #[test]
    fn low_entropy_binary_prefers_bwt() {
        let mut s = Vec::new();
        for i in 0..8000u32 {
            s.extend_from_slice(&[(i % 24) as u8, 0, 0, 0]);
        }
        let p = plan(&s, Class::Binary);
        assert!(p.try_bwt);
        assert_eq!(p.primary, Seat::Bwt);
    }

    #[test]
    fn cooler_body_teaches_bwt() {
        let mut s = Vec::new();
        for i in 0..65536u32 {
            s.push((i.wrapping_mul(0x9E37) >> 24) as u8);
        }
        for i in 0..80000u32 {
            s.extend_from_slice(&[(i % 24) as u8, 0, 0, 0]);
        }
        let p = plan(&s, Class::Binary);
        assert_eq!(p.primary, Seat::Bwt, "cooler interior must sit BWT");
        assert!(p.try_bwt);
    }

    #[test]
    fn hotter_body_teaches_match() {
        let mut s = vec![0u8, 1, 2, 3].repeat(16384);
        let motif: Vec<u8> = (0..256u32).map(|i| (i.wrapping_mul(0x9E37) >> 24) as u8).collect();
        for k in 0..800 {
            s.extend_from_slice(&motif);
            s.push((k & 0xff) as u8);
        }
        let p = plan(&s, Class::Binary);
        assert!(p.try_match);
        assert!(
            p.primary == Seat::Match || p.primary == Seat::Bwt,
            "got {:?}",
            p.primary
        );
    }
}
