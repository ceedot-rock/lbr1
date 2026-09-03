# LBR1 / LBHX — Slid Phi Labs (PRIVATE)

All rights reserved. Not public. Not a wrap.

## Champ (DECODE_OK)

| file | raw | coded | ratio |
|---|---:|---:|---:|
| mozilla | 51,220,480 | **14,810,112** | 0.2891 |
| ooffice | 6,152,192 | **2,678,004** | 0.4353 |

```
15,709,557  Old
15,501,117  Old+rep4
15,447,182  beam-2
15,373,305  beam-4
14,810,112  scalar DP + beam-4   live
```

Under prior scalar 14,992,589 by 182,477.
vs xz-9e 13,376,240 still +1,433,872.

## Product

`lb best` = min(TRU8, TR8X, LBR1, BW22, LBHX).

- TRU8 — solid run ≥ 4 KiB
- TR8X — sparse tile
- LBR1 — binaries (spine)
- BW22 — text, house only
- LBHX — mixed files only (two classes ≥ 8%)

Uniform binaries stay whole-file LBR1.

```
cargo build --release -p leftbrain --bin lb
./target/release/lb stat FILE
./target/release/lb best FILE
```
