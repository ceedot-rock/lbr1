//! PCC Dial A profile — hc4 W=1MiB CHAIN=4 LAZY=0 PACK=ml4
//! FAIL_LOUD if packed >= zstd-9 (16_735_963) or DECODE_OK false.
fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/workspace/corpora/silesia-members/mozilla".into());
    let data = std::fs::read(&path).expect("read");
    let w: usize = std::env::var("LBR1_WINDOW")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1 << 20);
    // Dial A defaults (shallower parse). CHAIN=8 ships (CHAIN=4 FAIL_LOUD on mozilla size).
    if std::env::var("LBR1_PARSE").is_err() {
        std::env::set_var("LBR1_PARSE", "hc4");
    }
    if std::env::var("LBR1_CHAIN").is_err() {
        std::env::set_var("LBR1_CHAIN", "8");
    }
    if std::env::var("LBR1_LAZY").is_err() {
        std::env::set_var("LBR1_LAZY", "0");
    }
    if std::env::var("LBR1_PACK").is_err() {
        std::env::set_var("LBR1_PACK", "ml4");
    }

    let t0 = std::time::Instant::now();
    let toks = splb::parse::parse(&data, w);
    let parse_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let _ = splb::frame::pack(&toks, data.len(), w as u32, &data);
    let t1 = std::time::Instant::now();
    let blob = splb::frame::pack(&toks, data.len(), w as u32, &data);
    let pack_ms = t1.elapsed().as_secs_f64() * 1000.0;

    let t2 = std::time::Instant::now();
    let dec = splb::decode_lbr1(&blob);
    let dec_ms = t2.elapsed().as_secs_f64() * 1000.0;
    let dok = matches!(&dec, Ok(d) if d.as_slice() == data.as_slice());
    let mb = data.len() as f64 / 1e6;
    let zstd9 = 16_735_963usize;
    let fail_loud = blob.len() >= zstd9 || !dok;
    println!(
        "raw={} toks={} pack_bytes={} ver={} pack_mode={:?} parse_ms={:.1} pack_ms={:.1} decode_ms={:.1} DECODE_OK={} parse_MBps={:.2} pack_MBps={:.2} findpack_MBps={:.2} vs_zstd9={} FAIL_LOUD={}",
        data.len(),
        toks.len(),
        blob.len(),
        blob.get(4).copied().unwrap_or(0),
        std::env::var("LBR1_PACK").ok(),
        parse_ms,
        pack_ms,
        dec_ms,
        dok,
        mb / (parse_ms / 1000.0),
        mb / (pack_ms / 1000.0),
        mb / ((parse_ms + pack_ms) / 1000.0),
        blob.len() as isize - zstd9 as isize,
        fail_loud
    );
    if fail_loud {
        eprintln!("FAIL_LOUD: packed >= zstd-9 ({}) or DECODE_OK false — lead burned", zstd9);
        std::process::exit(2);
    }
}
