# PCC

**PCC** — Ptaszenski Computational Codec. Slid Phi Labs. Dual-licensed AGPL-3.0-or-later OR Commercial. Public source. Signing keys stay operator-only.

Job: one PCC1 frame. Router is not a compressor. Law: [JOBS.md](JOBS.md).  
`.pcc` is our zip (PCCZ). `lb pcc` encodes. `lb zip` / `unzip` / `ls` / `info` / `test` / `cat`.  
Does not: dump Combined GC into this tree. Combined GC / AWARE stay in `combined-gc`. Not pulsar. Not SPH11. Not #1.

Tree **pcc-0.13.0** adds own genes LZM1, ZMX1, STR1, NNC1. Official Silesia numbers below are **pcc-0.12.1**.

## Official Silesia (12 whole files, DECODE_OK)

Raw 211,938,580.

| Pathway | Packed | Notes |
|---|---:|---|
| pulsar 2.5.0 | **55,745,438** | Matches OSCB |
| PCC | **51,498,645** | Own codec. Beats pulsar. Loses to xz-6 (~49.4M) |
| champ mozilla lock | **14,796,694** | MATCH path, not the 12-file total |

`lb champ` is the LBR1 quality path (mozilla stays MATCH). `lb stream` is TRUSTREAM (4 KiB STORE+ZERO). `lb best` is house min(). `lb aware` is house + Combined GC own-path.

Zeros flagship: 40,000 B → **8 B** T_ZERO DECODE_OK.

```
cargo build --release -p splb --bin lb
./target/release/lb pcc FILE [OUT]
./target/release/lb zip OUT.pcc DIR
./target/release/lb info FILE.pcc
./target/release/lb test FILE.pcc
./target/release/lb stream FILE
./target/release/lb champ FILE
./target/release/lb best FILE
./target/release/lb lzm FILE
./target/release/lb zmix FILE
./target/release/lb str FILE
./target/release/lb nnc FILE
```
