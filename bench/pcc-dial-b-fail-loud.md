# PCC Dial B — mid-window parse FAIL_LOUD gate

**Face:** PCC (AWARE = legacy alias only).  
**Shipped dial:** `LBR1_PARSE=hc4 LBR1_WINDOW=524288 LBR1_CHAIN=8 LBR1_LAZY=0 LBR1_PACK=ml4`  
**Prefer tried:** `WINDOW=262144` (256 KiB) — mozilla packed **16,856,435** (≥ zstd-9) → **FAIL_LOUD**; fallback **WINDOW=524288** (512 KiB) keeps lead.  
**Avoid:** `WINDOW=65536` (64 KiB) historically **+406k** FAIL.

## Mozilla bake (2026-09-11, thin LTO, target-cpu=native)

| dial | packed | vs zstd-9 | find MB/s | pack MB/s | findpack | DECODE_OK |
|---|---:|---:|---:|---:|---:|---|
| W=256KiB (prefer) | 16,856,435 | **+120,472 FAIL** | 38.21 | 45.55 | 20.78 | true |
| **W=512KiB (ship)** | **16,723,188** | **−12,775 PASS** | **32.47** | 46.22 | **19.07** | true |
| Dial A W=1MiB c8 | 16,708,129 | −27,834 | 23.77 | 45.69 | 15.64 | true |

zstd-9 gate: **16,735,963**.

## Gates

| Gate | Threshold | Action |
|---|---|---|
| packed bytes | **< 16,735,963** | FAIL_LOUD if ≥ |
| DECODE_OK | true | FAIL_LOUD if false |
| find speed | raise vs Dial A ~24 MB/s | W=512KiB ≈32 MB/s (report; ≥50 still needs pack/ANS) |

## Run

```bash
cargo build --release -p splb --example profile_ml4_hot
./bench/pcc-dial-b-fail-loud.sh /path/to/mozilla
```

Exit 2 = FAIL_LOUD. CDDG shelf doors untouched. No hosted SlidPhiLabs wire. No Gale.
