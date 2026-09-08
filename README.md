# PCC — PRIVATE

**PCC** — Ptaszenski Computational Codec. Slid Phi Labs. All rights reserved. Do not make this repository public.

Job: house encode/decode for TRU8, TR8X, LBR1, pulsar BW22, LBHM. Router is not a compressor. Law: [JOBS.md](JOBS.md).  
Does not: dump Combined GC. Combined GC / AWARE stay in `combined-gc`. Not pulsar. Not SPH11. Not #1.

Live LBR1 champ (DECODE_OK):

- mozilla 51,220,480 → **14,796,694** (ratio 0.2889)
- ooffice 6,152,192 → **2,674,974** (ratio 0.4348)

Zeros flagship: 40,000 B → **8 B** T_ZERO DECODE_OK.

cap128 beam-4 scalar DP. See `LBR1-CHAMP.md`.

```
cargo build --release -p splb --bin lb
./target/release/lb champ FILE
./target/release/lb best FILE
```

`lb best` = public seats. `lb asmd` = Altered State Morphic Distopic Encoding. UNLICENSED. Not published.
