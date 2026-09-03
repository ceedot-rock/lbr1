//! lb — SPLv1 house CLI. `best` is a pulsar picker until the Drive LBR1 coder is imported.

use std::env;
use std::fs;
use std::time::Instant;

fn usage() -> ! {
    eprintln!("usage: lb <stat|best|encode|decode> IN [-o OUT]");
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        usage();
    }
    let cmd = args[1].as_str();
    let path = &args[2];
    let mut out: Option<&str> = None;
    let mut i = 3;
    while i + 1 < args.len() {
        if args[i] == "-o" {
            out = Some(&args[i + 1]);
            i += 2;
        } else {
            usage();
        }
    }
    match cmd {
        "stat" => stat(path),
        "best" | "encode" => {
            let data = fs::read(path).expect("read");
            let t = Instant::now();
            let (blob, kind) = leftbrain::encode_best(&data).expect("encode");
            let back = leftbrain::decode(&blob).expect("decode");
            assert_eq!(back, data, "DECODE_OK failed");
            eprintln!(
                "{path} {} -> {} ({kind}) {:.3}s DECODE_OK",
                data.len(),
                blob.len(),
                t.elapsed().as_secs_f64()
            );
            if let Some(p) = out {
                fs::write(p, blob).expect("write");
            }
        }
        "decode" => {
            let blob = fs::read(path).expect("read");
            let data = leftbrain::decode(&blob).expect("decode");
            if let Some(p) = out {
                fs::write(p, data).expect("write");
            } else {
                eprintln!("decoded {} bytes", data.len());
            }
        }
        _ => usage(),
    }
}

fn stat(path: &str) {
    let data = fs::read(path).expect("read");
    let class = leftbrain::detect::classify(&data);
    eprintln!("file {path} {} class={class:?}", data.len());
    let t = Instant::now();
    let matches = leftbrain::parse::matches_for(&data);
    let picks: usize = matches.iter().filter(|v| !v.is_empty()).count();
    let max_len = matches
        .iter()
        .flatten()
        .map(|m| m.len)
        .max()
        .unwrap_or(0);
    eprintln!(
        "finder picks_at {picks} max_len {max_len} {:.3}s",
        t.elapsed().as_secs_f64()
    );
    if let Some((blob, kind)) = leftbrain::encode_best(&data) {
        let back = leftbrain::decode(&blob).expect("decode");
        assert_eq!(back, data);
        eprintln!("pulsar-picker {kind} {} DECODE_OK", blob.len());
    }
}
