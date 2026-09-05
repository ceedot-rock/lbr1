//! DP wrapper. Lives in lbr1_price so the C ABI crate is the SoT.

pub use lbr1_price::{price_block_c, Match as PriceMatch, RepState, SmallCostsC};

use crate::parse::Match;

pub fn price_file(data: &[u8], matches: &[Vec<Match>], beam: usize) -> (Vec<u32>, Vec<u32>, Vec<RepState>) {
    let conv: Vec<Vec<PriceMatch>> = matches
        .iter()
        .map(|v| {
            v.iter()
                .map(|m| PriceMatch {
                    dist: m.dist,
                    len: m.len,
                    is_comp: m.is_comp,
                })
                .collect()
        })
        .collect();
    price_block_c(data, &conv, beam)
}
