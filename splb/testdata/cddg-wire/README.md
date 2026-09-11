# CDDG wire fixtures (Theory-sealed)

## v0 (`model_id=8`, 12 B)

Source: `/workspace/corpora/cddg-wire-v0.md` + `/workspace/corpora/cddg-wire/`.

| Fixture | n | pack | DECODE_OK |
|---|---:|---:|:---:|
| `cddg_spin_1k` | 1000 | 12 B | yes |
| `cddg_spin_360` | 360 | 12 B | yes |
| `cddg_spin_359` | 359 | 12 B | yes |

`seed=369`, `start_plane=0`. Silent deep-shelf L0 — not Gale daily / hosted crown.

## v1 dual (`model_id=9`, 19 B)

Source: `/workspace/corpora/cddg-wire-v1.md` + Kernel `cddg_dual_wire.py`.

| Fixture | n | start_s | seed_s | start_c | seed_c | link_rule | pack | DECODE_OK |
|---|---:|---:|---:|---:|---:|---:|---:|:---:|
| `cddg_dual_1k` | 1000 | 0 | 369 | 0 | 963 | 0 xor_mix | 19 B | yes |
| `cddg_dual_360` | 360 | 0 | 369 | 180 | 963 | 0 | 19 B | yes |
| `cddg_dual_359` | 359 | 0 | 369 | 0 | 963 | 0 | 19 B | yes |
| `cddg_dual_ring_64` | 64 | 0 | 369 | 0 | 963 | 1 ring_neighbor | 19 B | yes |

Exact generator match only. Silent deep-shelf L0 — **not** Gale daily / hosted crown.
