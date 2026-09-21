# Decoder and prune law (Scout / PCC)

PCC is the product. Scout is the Fast seat. Do not race Sniper.

## Decode

`encode()` already refuses a frame that `decode()` does not invert to the raw.
CRC-32 of the raw is on v2 frames. Piece length must match `raw_len`.

## Prune in pick_one

1. ZERO wins immediately.
2. crush → return.
3. held after MATCH/CMAQ → return (BWT does not silence MATCH).
4. Mixers (LZ/LZM/STR/ZMIX/NNC) only if packed/raw > `open_ratio()`.
   Default `crate::OPEN_RATIO` = 0.31. `PCC_OPEN_RATIO=0.99` is Fast.
5. STORE last. MATH only if it DECODE_OKs smaller than raw.

Wire: in `pick_one`, replace `const OPEN: f64 = 0.31` with `crate::open_ratio::open_ratio()`.
Add `pub mod open_ratio;` in `lib.rs`.
