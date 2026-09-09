use splb::pccz::{self, Member};
use splb::{blob_kind, decode, encode_best, encode_window, VERSION};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::UNIX_EPOCH;

fn usage() -> ! {
    eprintln!("PCC {VERSION}");
    eprintln!("  lb encode [-w WINDOW] IN OUT");
    eprintln!("  lb decode IN OUT");
    eprintln!("  lb stat  [-w WINDOW] FILE");
    eprintln!("  lb champ FILE [OUT]         # LBR1 + BW22; mozilla stays MATCH");
    eprintln!("  lb process FILE [OUT]       # Codex: flip Regular ↔ Dark, mirror=0");
    eprintln!("  lb pcc   FILE [OUT]         # AWARE archive: PCC house");
    eprintln!("  lb stream FILE [OUT]        # AWARE stream: 4 KiB ZERO+STORE");
    eprintln!("  lb best  FILE [OUT]         # min(TRU8, TR8X, LBR1, BW22); one encode");
    eprintln!("  lb aware FILE [OUT]         # house min(): LBR1, pulsar, wraps, Combined GC");
    eprintln!("  lb lz    FILE [OUT]         # own LZ wrap");
    eprintln!("  lb paq   FILE [OUT]         # own PAQ wrap (PCCaq)");
    eprintln!("  lb gc    FILE [OUT]         # Combined GC own-path (Max, own skins)");
    eprintln!("  lb asmd [--max] [--seat bw22|lbr1|hybrid] FILE [OUT]");
    eprintln!("  lb zip   OUT.pcc PATH [PATH...]   # our zip: many files, one .pcc");
    eprintln!("  lb unzip IN.pcc [DIR]             # extract a .pcc archive");
    eprintln!("  lb ls    IN.pcc                   # list a .pcc archive");
    process::exit(2);
}

fn unix_mtime(p: &Path) -> u32 {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as u32)
        .unwrap_or(0)
}

fn collect_path(path: &Path, prefix: &str, out: &mut Vec<Member>) {
    let meta = fs::symlink_metadata(path).expect("stat");
    if meta.file_type().is_symlink() {
        return;
    }
    if meta.is_dir() {
        if !prefix.is_empty() {
            out.push(Member {
                name: prefix.to_string(),
                data: Vec::new(),
                mtime: unix_mtime(path),
                mode: 0o755,
                dir: true,
            });
        }
        let mut kids: Vec<_> = fs::read_dir(path).expect("read_dir").filter_map(|e| e.ok()).collect();
        kids.sort_by_key(|e| e.file_name());
        for kid in kids {
            let name = kid.file_name();
            let name = name.to_string_lossy();
            if name == "." || name == ".." {
                continue;
            }
            let child_prefix = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            collect_path(&kid.path(), &child_prefix, out);
        }
        return;
    }
    if !meta.is_file() {
        return;
    }
    let data = fs::read(path).expect("read");
    out.push(Member {
        name: if prefix.is_empty() {
            path.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".into())
        } else {
            prefix.to_string()
        },
        data,
        mtime: unix_mtime(path),
        mode: 0o644,
        dir: false,
    });
}

fn safe_join(root: &Path, name: &str) -> PathBuf {
    let cleaned = pccz::clean_name(name).expect("name");
    let mut p = root.to_path_buf();
    for part in cleaned.split('/') {
        p.push(part);
    }
    p
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
            if splb::codex::Face::of(&blob) == splb::codex::Face::Dark {
                let c = splb::codex::to_regular(&blob).expect("codex");
                assert_eq!(c.mirror, 0, "DECODE_OK failed");
                fs::write(&args[1], &c.bytes).expect("write");
            } else {
                let data = decode(&blob).expect("decode");
                fs::write(&args[1], data).expect("write");
            }
        }
        "asmd" if !args.is_empty() => {
            let mut max = false;
            let mut seat: Option<String> = None;
            let mut i = 0;
            while i < args.len() {
                if args[i] == "--max" {
                    max = true;
                    args.remove(i);
                } else if args[i] == "--seat" && i + 1 < args.len() {
                    seat = Some(args[i + 1].clone());
                    args.remove(i);
                    args.remove(i);
                } else {
                    i += 1;
                }
            }
            splb::asmd::set_force_seat(seat.as_deref());
            if args.is_empty() || args.len() > 2 {
                usage();
            }
            splb::asmd::set_kinetic(!max);
            let raw = fs::read(&args[0]).expect("read");
            let t0 = std::time::Instant::now();
            match splb::asmd::encode(&raw) {
                Some(p) => {
                    let enc = t0.elapsed();
                    let t1 = std::time::Instant::now();
                    let back = splb::decode(&p.blob).expect("decode");
                    let dec = t1.elapsed();
                    assert_eq!(back, raw, "DECODE_OK failed");
                    if args.len() == 2 {
                        fs::write(&args[1], &p.blob).expect("write");
                    }
                    eprintln!(
                        "asmd {} {} {} {} -> {} ({:.4}) enc={:?} dec={:?} DECODE_OK seat={}",
                        if max { "max" } else { "kinetic-bw22" },
                        p.morph.as_str(),
                        p.seat.omni(),
                        raw.len(),
                        p.blob.len(),
                        p.blob.len() as f64 / raw.len() as f64,
                        enc,
                        dec,
                        p.seat.as_str()
                    );
                }
                None => {
                    eprintln!("asmd expand-or-fail; not writing");
                    process::exit(1);
                }
            }
        }
        "process" | "pcc" | "stream" if args.len() == 1 || args.len() == 2 => {
            let raw = fs::read(&args[0]).expect("read");
            let t0 = std::time::Instant::now();
            let got = if cmd == "process" {
                splb::codex::process(&raw, false)
            } else {
                splb::codex::to_dark(&raw, cmd == "stream")
            };
            match got {
                Ok(c) => {
                    let ms = t0.elapsed();
                    assert_eq!(c.mirror, 0, "DECODE_OK failed");
                    if args.len() == 2 {
                        fs::write(&args[1], &c.bytes).expect("write");
                    }
                    let (from, to) = if c.face == splb::codex::Face::Dark {
                        (raw.len(), c.bytes.len())
                    } else {
                        (raw.len(), c.bytes.len())
                    };
                    println!(
                        "{}\tface={}\top={}\tseat={}\tin={}\tout={}\tratio={:.4}\tmirror={}\tdark={:.4}\tms={}\tDECODE_OK",
                        args[0],
                        c.face.as_str(),
                        c.op,
                        splb::detect::omni_seat(c.op),
                        from,
                        to,
                        if from == 0 { 0.0 } else { to as f64 / from as f64 },
                        c.mirror,
                        c.dark,
                        ms.as_millis()
                    );
                }
                Err(e) => {
                    println!("{}\traw={}\tcoded=FAIL\terr={}", args[0], raw.len(), e);
                    process::exit(1);
                }
            }
        }
        "gc" if args.len() == 1 || args.len() == 2 => {
            let raw = fs::read(&args[0]).expect("read");
            let t0 = std::time::Instant::now();
            match splb::encode_gc(&raw) {
                Some(b) => {
                    let back = decode(&b).expect("decode");
                    assert_eq!(back, raw, "DECODE_OK failed");
                    if args.len() == 2 {
                        fs::write(&args[1], &b).expect("write");
                    }
                    println!(
                        "{}\traw={}\tcoded={}\tratio={:.4}\tkind=gc\tseat=AWARE\tenc_ms={}\tDECODE_OK",
                        args[0],
                        raw.len(),
                        b.len(),
                        b.len() as f64 / raw.len() as f64,
                        t0.elapsed().as_millis()
                    );
                }
                None => {
                    println!("{}\traw={}\tcoded=EXPAND", args[0], raw.len());
                    process::exit(1);
                }
            }
        }
        "lz" | "paq" if args.len() == 1 || args.len() == 2 => {
            let raw = fs::read(&args[0]).expect("read");
            let t0 = std::time::Instant::now();
            let got = if cmd == "lz" {
                splb::wrap::lz_encode(&raw).map(|b| (b, "lzw1"))
            } else {
                splb::wrap::paq_encode(&raw).map(|b| (b, "pcaq"))
            };
            match got {
                Some((b, kind)) => {
                    let enc = t0.elapsed();
                    let back = decode(&b).expect("decode");
                    assert_eq!(back, raw, "DECODE_OK failed");
                    if args.len() == 2 {
                        fs::write(&args[1], &b).expect("write");
                    }
                    println!(
                        "{}\traw={}\tcoded={}\tratio={:.4}\tkind={}\tseat={}\tenc_ms={}\tDECODE_OK",
                        args[0],
                        raw.len(),
                        b.len(),
                        b.len() as f64 / raw.len() as f64,
                        kind,
                        splb::detect::omni_seat(kind),
                        enc.as_millis()
                    );
                }
                None => {
                    println!("{}\traw={}\tcoded=EXPAND", args[0], raw.len());
                    process::exit(1);
                }
            }
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
                    let kind = blob_kind(&b);
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
        "zip" | "pack" if args.len() >= 2 => {
            let out_path = PathBuf::from(&args[0]);
            let mut members = Vec::new();
            for a in &args[1..] {
                let p = Path::new(a);
                let top = p
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| a.clone());
                collect_path(p, &top, &mut members);
            }
            if members.is_empty() {
                eprintln!("zip: no files");
                process::exit(1);
            }
            let t0 = std::time::Instant::now();
            let blob = pccz::zip_bytes(&members).expect("zip");
            let back = pccz::unzip_bytes(&blob).expect("unzip check");
            assert_eq!(back.len(), members.len(), "DECODE_OK member count");
            fs::write(&out_path, &blob).expect("write");
            let raw: u64 = members.iter().map(|m| m.data.len() as u64).sum();
            println!(
                "{}\tmembers={}\traw={}\tcoded={}\tratio={:.4}\tkind=pccz\tms={}\tDECODE_OK",
                out_path.display(),
                members.len(),
                raw,
                blob.len(),
                if raw == 0 { 0.0 } else { blob.len() as f64 / raw as f64 },
                t0.elapsed().as_millis()
            );
        }
        "unzip" | "unpack" if args.len() == 1 || args.len() == 2 => {
            let blob = fs::read(&args[0]).expect("read");
            let members = pccz::unzip_bytes(&blob).expect("unzip");
            let dest = if args.len() == 2 {
                PathBuf::from(&args[1])
            } else {
                PathBuf::from(".")
            };
            fs::create_dir_all(&dest).ok();
            for m in &members {
                let p = safe_join(&dest, &m.name);
                if m.dir {
                    fs::create_dir_all(&p).expect("mkdir");
                } else {
                    if let Some(parent) = p.parent() {
                        fs::create_dir_all(parent).ok();
                    }
                    fs::write(&p, &m.data).expect("write");
                }
            }
            println!(
                "{}\tmembers={}\tdest={}\tDECODE_OK",
                args[0],
                members.len(),
                dest.display()
            );
        }
        "ls" | "zip-list" if args.len() == 1 => {
            let blob = fs::read(&args[0]).expect("read");
            let list = pccz::list_bytes(&blob).expect("ls");
            println!("name\traw\tpacked\toccupant\tflags");
            for e in list {
                let flag = if e.dir {
                    "dir"
                } else if e.stored {
                    "store"
                } else {
                    "pack"
                };
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    e.name, e.raw_len, e.packed_len, e.occupant, flag
                );
            }
        }
        _ => usage(),
    }
}
