use splb::{decode, encode_best, encode_window, VERSION};
use std::env;
use std::fs;
use std::process;

fn usage() -> ! {
    eprintln!("PCC {VERSION}");
    eprintln!("  lb encode [-w WINDOW] IN OUT");
    eprintln!("  lb decode IN OUT");
    eprintln!("  lb stat  [-w WINDOW] FILE");
    eprintln!("  lb champ FILE [OUT]         # LBR1 quality path; optional write");
    eprintln!("  lb best  FILE [OUT]         # PCC house: min(TRU8, TR8X, LBR1, BW22); one encode");
    eprintln!("  lb aware FILE [OUT]         # same house; never xz");
    process::exit(2);
}

fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        usage();
    }
    let cmd = args.remove(0);
    let mut window: u32 = splb::parse::DEFAULT_WINDOW as u32;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-w" && i + 1 < args.len() {
            window = args[i + 1].parse().unwrap_or(window);
            args.remove(i);
            args.remove(i);
        } else {
            i += 1;
        }
    }
    match cmd.as_str() {
        "encode" if args.len() == 2 => {
            let raw = fs::read(&args[0]).expect("read");
            match encode_window(&raw, window) {
                Some(b) => {
                    fs::write(&args[1], &b).expect("write");
                    eprintln!("{} -> {} ({:.4})", raw.len(), b.len(), b.len() as f64 / raw.len() as f64);
                }
                None => {
                    eprintln!("expand-or-fail; not writing");
                    process::exit(1);
                }
            }
        }
        "decode" if args.len() == 2 => {
            let blob = fs::read(&args[0]).expect("read");
            let data = decode(&blob).expect("decode");
            fs::write(&args[1], data).expect("write");
        }
        "stat" | "champ" | "best" | "aware" if args.len() == 1 || args.len() == 2 => {
            let raw = fs::read(&args[0]).expect("read");
            if cmd == "aware" {
                eprintln!("AWARE own-path (XZ1 retired)");
            }
            let t0 = std::time::Instant::now();
            let got = if cmd == "best" || cmd == "aware" {
                encode_best(&raw)
            } else {
                encode_window(&raw, window).map(|b| {
                    let kind = if b.len() == 8 {
                        "tru8"
                    } else {
                        "lbr1"
                    };
                    (b, kind)
                })
            };
            match got {
                Some((b, kind)) => {
                    let enc = t0.elapsed();
                    let t1 = std::time::Instant::now();
                    let back = decode(&b).expect("decode");
                    let dec = t1.elapsed();
                    assert_eq!(back, raw, "DECODE_OK failed");
                    if args.len() == 2 {
                        fs::write(&args[1], &b).expect("write");
                    }
                    println!(
                        "{}\traw={}\tcoded={}\tratio={:.4}\tkind={}\tseat={}\tenc_ms={}\tdec_ms={}\tDECODE_OK",
                        args[0],
                        raw.len(),
                        b.len(),
                        b.len() as f64 / raw.len() as f64,
                        kind,
                        splb::detect::omni_seat(kind),
                        enc.as_millis(),
                        dec.as_millis()
                    );
                }
                None => {
                    println!("{}\traw={}\tcoded=EXPAND", args[0], raw.len());
                    process::exit(1);
                }
            }
        }
        _ => usage(),
    }
}
