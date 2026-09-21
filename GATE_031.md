# OPEN_RATIO 0.31

`pick_one` in `splb/src/pcc.rs` already uses `const OPEN: f64 = 0.31` for the
post-MATCH/BWT mixer tail (LZ / LZM / STR / ZMIX / NNC).

Scout Dial A needs a **fast** path that does not pay that tail.

## Env (land next to the const)

```rust
fn open_ratio() -> f64 {
    std::env::var("PCC_OPEN_RATIO")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|x| *x > 0.0 && *x <= 1.0)
        .unwrap_or(0.31)
}
```

Replace `const OPEN: f64 = 0.31` with `let open_cut = open_ratio();` and compare
against `open_cut`.

`scout-dial-a`:
- `SCOUT_PROFILE=quality` → `PCC_OPEN_RATIO=0.31`
- `SCOUT_PROFILE=fast` → `PCC_OPEN_RATIO=0.99`

Keep tests. Do not retag Silesia. Official line remains pcc-0.12.1 = 51,498,645.
