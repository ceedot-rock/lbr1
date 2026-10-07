# Contributing to LBR1 / PCC

Thanks for helping make this codec better.

## Ground rules

- Byte-exact roundtrips. If a codec path change can't decode back to the
  original input, it does not ship.
- Claimed numbers are decode-verified (`DECODE_OK`), never estimates.
- This repo never takes in outside codec tech on the encode path; see
  COMPARE.md and JOBS.md for what's measured and what's retired.

## Quick checks

```sh
cargo build --release --workspace   # build everything
cargo test --workspace              # run the test suite
```

CI runs the build, the test suite, and a codec roundtrip smoke
(`lb pcc` then `lb test`, expecting `DECODE_OK`) on every pull request.

## Working on the codec

1. Clone with submodules: `git clone --recurse-submodules https://github.com/ceedot-rock/lbr1.git`
2. Build the CLI: `cargo build --release -p splb --bin lb`
3. Roundtrip anything you touch: `./target/release/lb pcc FILE OUT.pcc` then
   `./target/release/lb test OUT.pcc` — must print `DECODE_OK`.
4. Add or update tests in `splb/src` next to the code you changed.
5. Open a pull request using the template; paste the verified numbers.

## Licensing

LBR1 / PCC / splb is dual-licensed (GNU Affero General Public License v3.0
or later, OR the Slid Phi Labs Commercial License). By contributing you agree
your contribution may be distributed under both. Signing keys and production
AUTH_DIR data stay operator-only and must never appear in a PR.
