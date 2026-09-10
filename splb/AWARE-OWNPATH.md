# AWARE own-path — XZ1 retired + pack v1 poly

Combined GC 1.19.2 published **47,752,368** on Silesia with **XZ1**:
host `xz -9` plus an 8-byte wrap on `mozilla`, `samba`, `sao`, `ooffice`.

That wrap is off.

`lb aware` = `lb best` = min(TRU8, TR8X, LBR1, pulsar BW22/OZL2, **poly_d\***).
No xz, gzip, brotli, or zstd in the encoder.

## Pack v1 (polyfit peel)

Inner wire (LBHX kind=POLY carries `raw_len` outside):

`model_id:u8 | deg:u8 | coeffs:(deg+1)*f64 LE | zero_residual_flag:u8 | [zlib-9 i32le residuals if flag==0]`

- `model_id`: 0=const, 1=`poly_d1`, 2=`poly_d2`, 3=`affine_i32` (reserved), 4=`poly_d3`
- `int_ramp_256k` → MDL `poly_d1`, flag=1 → **19 B** inner (`aware_bytes` ≤ 21 CI gate)
- Beats hosted **3546 B**
