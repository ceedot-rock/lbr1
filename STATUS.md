# LBR1 — private snapshot 2026-09-03

Slid Phi Labs. Proprietary. Do not make this repository public.

## Layout

- `leftbrain/` — LBR1 VER5 range coder, house path, finder, `lb` binary
- `lbr1_price/` — C ABI `price_block_c` / TinyBook thread-local (parse pricing)

## Last measured (DECODE_OK)

| file | coded | notes |
|---|---:|---|
| ooffice 6,152,192 | 2,768,898 | C ABI TinyBook DP, 65 MB RSS |
| mozilla 51,220,480 | 15,729,590 | C ABI path, 381 MB peak, enc 160s |
| prior scalar `bits_*` parse | mozilla 14,992,589 | 48 MB RSS — still the size champion |

4 MB ooffice slice: 47 MB RSS after C ABI (was 1.73 GiB when `come[k]==0` backtrack looped).

## Build

```
cd leftbrain
cargo test --lib -- range::tests zeros_tiny
cargo build --release --bin lb
./target/release/lb stat /path/to/mozilla
```

Pulsar path dep is optional; comment it if you do not have `speed-cmp/pulsar-best`.
