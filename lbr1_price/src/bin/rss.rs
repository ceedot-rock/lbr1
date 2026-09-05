fn main() {
    let n = 4_000_000usize;
    let data = vec![0u8; n];
    let toks = lbr1_price::parse_blocks(&data, n, lbr1_price::TinyBook::new);
    println!("toks={} first={:?}", toks.len(), toks.first());
}
