# PCC Dial A — shallower parse FAIL_LOUD gate

**Face:** PCC (AWARE = legacy alias only).  
**Shipped dial:** `LBR1_PARSE=hc4 LBR1_WINDOW=1048576 LBR1_CHAIN=8 LBR1_LAZY=0 LBR1_PACK=ml4`  
**Prefer tried:** `CHAIN=4` — mozilla packed **16,871,769** (≥ zstd-9) → **FAIL_LOUD**; fallback **CHAIN=8** keeps lead.

## Mozilla bake (2026-09-11, thin LTO, target-cpu=native)

| dial | packed | vs zstd-9 | find MB/s | pack MB/s | findpack | DECODE_OK |
|---|---:|---:|---:|---:|---:|---|
| c4 (prefer) | 16,871,769 | **+135,806 FAIL** | 33.59 | 46.28 | 19.46 | true |
| **c8 (ship)** | **16,708,129** | **−27,834 PASS** | **24.34** | 46.59 | **15.99** | true |
| c16 (wall-split) | 16,612,958 | −123,005 | 18.05 | 47.09 | 13.05 | true |

zstd-9 gate: **16,735,963**.

## Gates

| Gate | Threshold | Action |
|---|---|---|
| packed bytes | **< 16,735,963** | FAIL_LOUD if ≥ |
| DECODE_OK | true | FAIL_LOUD if false |
| find speed | raise vs c16 ~18 MB/s | c8 ≈24 MB/s (report; ≥50 still needs pack/ANS) |

## Run

```bash
cargo build --release -p splb --example profile_ml4_hot
./bench/pcc-dial-a-fail-loud.sh /path/to/mozilla
```

Exit 2 = FAIL_LOUD. CDDG shelf doors untouched. No hosted SlidPhiLabs wire.
