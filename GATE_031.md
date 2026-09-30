# OPEN_RATIO 0.31

`splb::OPEN_RATIO = 0.31` is on main (`encode_best` / `lb best`).

`lb pcc` still uses `splb/src/pcc.rs` `pick_one`:

```rust
    let open = best
        .as_ref()
        .map(|b| (b.blob.len() as f64) / (data.len() as f64) > 0.50)
        .unwrap_or(true);
```

Replace `> 0.50` with `> crate::OPEN_RATIO` and add next to `NOMINAL_WINDOW`:

```rust
pub const OPEN_RATIO: f64 = crate::OPEN_RATIO;
```

or just use `crate::OPEN_RATIO` in the map.

Keep tests. Do not retag Silesia. Official line remains pcc-0.12.1 = 51,498,645.
