# Lab Science scoreboard (pack v1)

Canonical CSV columns (Kernel-owned bake-off):

```
corpus,file,bytes_in,bytes_out,aware_bytes,encode_ms,decode_ms,roundtrip_ok,model_id,H0,Hk,shannon_lb_bits,aware_bits,gap_bits,claimable,vs_21B_basement,notes
```

## Rules
- **claimable=true** only for `int_ramp_256k` (+ hero fixtures: zeros/ramp/walk). Crown copy quotes these only.
- **claimable=false**: Canterbury + Silesia (+ enwik8) — honesty / GP context, never standings copy.
- CI hard gate on claimable crown: `int_ramp_256k` → `poly_d1`, bit-exact, `aware_bytes ≤ 21` (shipped **19 B**), beats hosted **3546**.
- `Hk` uses `k=3` on bytes; `shannon_lb_bits = n * Hk`; `gap_bits = aware_bits - shannon_lb_bits`.
- `aware_bytes` = inner pack length (poly header); `bytes_out` may include LBHX wrap (e.g. packed=32).

## Fixture
See `kernel-pack-v1-scoreboard.csv` from the 2026-09-10 Lab Science pass (crown row: `ramp,int_ramp_256k,...,aware_bytes=19,...,claimable=true,WIN`).
