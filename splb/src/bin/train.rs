//! Train left + right on a file: per-slice min(LBR1, BW22).
use splb::{decode, encode, encode_best, VERSION};
use std::env;
use std::fs;
use std::time::Instant;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("train {VERSION}");
        eprintln!("  train FILE [SLICE_BYTES]");
        std::process::exit(2);
    }
    let path = &args[0];
    let slice: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2 * 1024 * 1024);
    let raw = fs::read(path).expect("read");
    println!("file\t{}\traw={}\tslice={}", path, raw.len(), slice);

    let t_all = Instant::now();
    let mut i = 0usize;
    let mut off = 0usize;
    let mut sum_lbr = 0usize;
    let mut sum_bw = 0usize;
    let mut sum_min = 0usize;
    let mut n_lbr = 0usize;
    let mut n_bw = 0usize;
    let mut hybrid = Vec::new();
    // hybrid container: HYB1 + u32 n + per slice (u8 kind, u32 raw, u32 coded, blob)
    hybrid.extend_from_slice(b"HYB1");
    hybrid.extend_from_slice(&(slice as u32).to_le_bytes());
    hybrid.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    let n_guess = (raw.len() + slice - 1) / slice;
    hybrid.extend_from_slice(&(n_guess as u32).to_le_bytes());

    while off < raw.len() {
        let end = (off + slice).min(raw.len());
        let block = &raw[off..end];
        let t0 = Instant::now();
        let lbr = encode(block);
        let t_lbr = t0.elapsed();
        let t1 = Instant::now();
        let bw = pulsar::pulsar_encode(block);
        let t_bw = t1.elapsed();

        let lsz = lbr.as_ref().map(|v| v.len()).unwrap_or(block.len() + 1);
        let bsz = bw.as_ref().map(|v| v.len()).unwrap_or(block.len() + 1);
        let (kind, blob) = match (&lbr, &bw) {
            (Some(a), Some(b)) if a.len() <= b.len() => ("lbr1", a.clone()),
            (Some(a), Some(b)) => ("bw22", b.clone()),
            (Some(a), None) => ("lbr1", a.clone()),
            (None, Some(b)) => ("bw22", b.clone()),
            _ => ("raw", block.to_vec()),
        };
        let back = if kind == "raw" {
            block.to_vec()
        } else {
            decode(&blob).expect("decode")
        };
        assert_eq!(back, block, "slice {} DECODE_OK fail", i);

        if kind == "lbr1" {
            n_lbr += 1;
        } else if kind == "bw22" {
            n_bw += 1;
        }
        sum_lbr += lsz.min(block.len() + 1);
        sum_bw += bsz.min(block.len() + 1);
        sum_min += blob.len();

        let kbyte = if kind == "lbr1" { 1u8 } else if kind == "bw22" { 2u8 } else { 0u8 };
        hybrid.push(kbyte);
        hybrid.extend_from_slice(&(block.len() as u32).to_le_bytes());
        hybrid.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        hybrid.extend_from_slice(&blob);

        println!(
            "s{:02}\toff={}\traw={}\tlbr1={}\tbw22={}\twinner={}\tlbr_ms={}\tbw_ms={}",
            i,
            off,
            block.len(),
            if lbr.is_some() { lsz.to_string() } else { "EXP".into() },
            if bw.is_some() { bsz.to_string() } else { "EXP".into() },
            kind,
            t_lbr.as_millis(),
            t_bw.as_millis()
        );
        i += 1;
        off = end;
    }

    println!(
        "SUM\tlbr1={}\tbw22={}\thybrid={}\twinners_lbr1={}\twinners_bw22={}\tslices={}\twall_ms={}",
        sum_lbr,
        sum_bw,
        hybrid.len(),
        n_lbr,
        n_bw,
        i,
        t_all.elapsed().as_millis()
    );

    // Whole-file house for reference (may be slow).
    let t2 = Instant::now();
    match encode_best(&raw) {
        Some((b, k)) => {
            let back = decode(&b).expect("whole decode");
            assert_eq!(back, raw);
            println!(
                "WHOLE\tkind={}\tcoded={}\tms={}",
                k,
                b.len(),
                t2.elapsed().as_millis()
            );
        }
        None => println!("WHOLE\tEXPAND\tms={}", t2.elapsed().as_millis()),
    }

    let out = format!("{path}.hyb1");
    fs::write(&out, &hybrid).expect("write hyb");
    println!("wrote {}\t{} bytes", out, hybrid.len());
}
