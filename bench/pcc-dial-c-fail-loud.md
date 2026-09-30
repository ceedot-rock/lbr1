# PCC Dial C — Gale-shaped shallow find FAIL_LOUD

**Face:** PCC (AWARE = legacy alias only).  
**Baseline kept (Dial A ship):** `LBR1_PARSE=hc4 LBR1_WINDOW=1048576 LBR1_CHAIN=8 LBR1_LAZY=0 LBR1_PACK=ml4` · `HASH=17` · `INSERT=dense` · `FIND=price`  
**Prefer tried:** Gale-shaped into `parse_lazy` — `HASH=16` + `INSERT=ends` (fewer inserts) · also `stride4` / `dense` / `gale` insert · `FIND=gale` longest-wins.  
**Verdict:** **FAIL_LOUD — no Dial C seat ships.** Prefer burns zstd-9; PASS seats either kiss the lead or **lose find** vs Dial A. Theory (≥50 find alone) not broken. ANS waits. No Gale 23M wire. CDDG untouched.

## Mozilla bake (2026-09-11, thin LTO, target-cpu=native)

zstd-9 gate: **16,735,963**. Dial A lead target ≈ **−28k**.

| dial | packed | vs zstd-9 | find MB/s | pack MB/s | findpack | DECODE_OK |
|---|---:|---:|---:|---:|---:|---|
| **prefer HASH=16 INSERT=ends** | 17,564,559 | **+828,596 FAIL** | 25.78 | 42.02 | 15.98 | true |
| HASH=16 INSERT=ends SCOUTS=0 | 17,594,823 | **+858,860 FAIL** | 30.09 | 43.55 | 17.80 | true |
| HASH=16 INSERT=stride4 | 17,454,889 | **+718,926 FAIL** | 25.08 | 44.23 | 16.01 | true |
| HASH=16 INSERT=dense c8 | 16,744,210 | **+8,247 FAIL** | 21.77 | 46.73 | 14.85 | true |
| HASH=16 INSERT=gale c8 | 16,728,405 | −7,558 (kiss) | 21.31 | 45.63 | 14.52 | true |
| HASH=16 INSERT=dense c9 | 16,719,015 | −16,948 | 20.26 | 46.35 | 14.10 | true |
| HASH=16 INSERT=dense c12 | 16,669,490 | −66,473 | 16.94 | 45.63 | 12.35 | true |
| FIND=gale HASH=17 INSERT=dense | 16,731,118 | −4,845 (kiss) | 25.14 | 46.14 | 16.28 | true |
| **Dial A HASH=17 INSERT=dense c8** | **16,708,129** | **−27,834 PASS** | **24.02** | 45.99 | **15.78** | true |

## Gates

| Gate | Threshold | Action |
|---|---|---|
| packed bytes | **< 16,735,963** | FAIL_LOUD if ≥ |
| real lead | prefer ~Dial A **−28k** | FAIL_LOUD if kiss-the-line without Theory OK |
| DECODE_OK | true | FAIL_LOUD if false |
| Theory find | break toward **≥50** alone | **not met** (best PASS find ≈25; pack/ANS later) |

## Ship decision

**Do not retarget daily defaults away from Dial A.** Knobs (`LBR1_HASH` / `LBR1_INSERT` / `LBR1_FIND` / `LBR1_SCOUTS`) land for future probes; defaults remain Dial A. Next: ANS/cashier on ML4 (pack must move) — parse-only cannot clear ≥50 while pack ≈46.

## Run

```bash
cargo build --release -p splb --example profile_ml4_hot
# Prefer (expect FAIL_LOUD exit 2):
LBR1_HASH=16 LBR1_INSERT=ends ./bench/pcc-dial-c-fail-loud.sh /path/to/mozilla
# Dial A control (expect PASS):
./bench/pcc-dial-a-fail-loud.sh /path/to/mozilla
```

Exit 2 = FAIL_LOUD. CDDG shelf doors untouched. No hosted SlidPhiLabs wire. No Gale climb.
