# PCC — PRIVATE

**PCC** — Ptaszenski Computational Codec. Slid Phi Labs. Dual-licensed AGPL-3.0-or-later OR Commercial. Public source. Signing keys stay operator-only.

Job: one PCC1 frame — ZERO, MATCH, BWT, CMAQ, LZ, STORE. Router is not a compressor. Law: [JOBS.md](JOBS.md).  
`.pcc` is our zip (PCCZ). `lb pcc` encodes. `lb zip` / `unzip` / `ls` / `info` / `test` / `cat`.  
Does not: dump Combined GC. Combined GC / AWARE stay in `combined-gc`. Not pulsar. Not SPH11. Not #1.  
`lb champ` is still the LBR1 quality path (mozilla 14,796,694 lock). `lb stream` is TRUSTREAM (4 KiB STORE+ZERO).

Live LBR1 champ (DECODE_OK):

- mozilla 51,220,480 → **14,796,694** (ratio 0.2889)
- ooffice 6,152,192 → **2,674,974** (ratio 0.4348)

Zeros flagship: 40,000 B → **8 B** T_ZERO DECODE_OK.

cap128 beam-4 scalar DP. See `LBR1-CHAMP.md`.

```
cargo build --release -p splb --bin lb
./target/release/lb pcc FILE [OUT]
./target/release/lb zip OUT.pcc DIR
./target/release/lb info FILE.pcc
./target/release/lb test FILE.pcc
./target/release/lb stream FILE
./target/release/lb champ FILE
./target/release/lb best FILE
```

`lb pcc` encodes PCC (PCC1 frame, or a `.pcc` archive if the input is a directory). `lb process` = Codex Regular ↔ Dark. `lb champ` is LBR1 quality. UNLICENSED. Not published.
