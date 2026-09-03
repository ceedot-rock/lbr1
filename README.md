# SPLv1 — Slid Phi Labs (PRIVATE)

**SPLv1** is the product. Slid Phi Labs, version 1.
Engines stay LBR1 / LBHX / TRU8 / TR8X / BW22.
All rights reserved. Not public. Not a wrap.

See [COMPARE.md](COMPARE.md). `lb best` picks pulsar BW23 today; the measured LBR1 range coder is still the Drive zip until it is imported.

## Champ (DECODE_OK) — Drive live 2026-09-03

| file | raw | coded | ratio |
|---|---:|---:|---:|
| mozilla | 51,220,480 | **14,796,694** | 0.2889 |
| ooffice | 6,152,192 | **2,674,974** | 0.4348 |

Banked in this repo before cap128: mozilla 14,810,112 / ooffice 2,678,004.
vs xz-9e mozilla 13,376,240 still +1,420,454.

```
15,709,557  Old
15,501,117  Old+rep4
15,447,182  beam-2
15,373,305  beam-4
14,810,112  scalar DP + beam-4
14,796,694  cap128 beam4 live
```

`lb best` = min(TRU8, TR8X, LBR1, BW22, LBHX).
Uniform binaries stay whole-file LBR1.

## Build

```
git submodule update --init --recursive
cargo test --workspace
cargo build --release -p leftbrain --bin lb
./target/release/lb stat /path/to/file
./target/release/lb best /path/to/file -o out.lbr
```
