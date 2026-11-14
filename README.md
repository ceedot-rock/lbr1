# PCC

[![Audited checks](https://github.com/Slid-Phi-Labs/lbr1/actions/workflows/audited-checks.yml/badge.svg)](https://github.com/Slid-Phi-Labs/lbr1/actions/workflows/audited-checks.yml) [![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)

**PCC** is Slid Phi Labs' lossless compressor. This repo is the public measuring path: the command-line tool we use to pack files and check that every byte comes back. It is dual-licensed (AGPL-3.0-or-later or a commercial grant). The hosted service is at https://www.slidphilabs.com/pcc. Signing keys stay with the operator, and our private encoder is not in this tree.

`.pcc` is our zip format. `lb pcc` packs a file. `lb zip` / `unzip` / `ls` / `info` / `test` / `cat` work on archives. See [JOBS.md](JOBS.md) for what each command is responsible for.

This tree is **pcc-0.13.0**. The official Silesia numbers below were measured with **pcc-0.12.1**.

## Official Silesia (12 whole files, every byte restored)

Raw 211,938,580 bytes.

| Compressor | Packed | Notes |
|---|---:|---|
| TNSSRC 0.1.0 | **43,724,575** | Our compressor that runs on your own machine (about 43.72M). Beats xz-9. Not a #1 claim. |
| PCC | **51,498,645** | Hosted. Beats pulsar. Loses to xz-6 (about 49.4M). |
| pulsar 2.5.0 | **55,745,438** | Free demo. Matches the OSCB figure. |

Full board: https://www.slidphilabs.com/silesia

`lb champ` is the quality path. `lb stream` is TRUSTREAM, for live data in 4 KiB tiles. `lb best` keeps whichever of our methods packs smallest.

Zeros: 40,000 bytes of zeros pack to **8 bytes** and restore exactly.

```
git clone --recurse-submodules https://github.com/Slid-Phi-Labs/lbr1.git
cd lbr1
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
