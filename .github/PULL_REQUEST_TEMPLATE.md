## What changed

<!-- One or two sentences. -->

## Codec path touched

<!-- e.g. splb/src/pcc.rs, or "none" -->

## Checks

- [ ] `cargo build --release --workspace` passes
- [ ] `cargo test --workspace` passes
- [ ] Roundtrip smoke (`lb pcc` then `lb test`) shows DECODE_OK, if any codec path changed
- [ ] Numbers claimed in the body are decode-verified, not estimates
