# Changelog

All notable changes to the PCC / pulsar open-source measure path are documented here.

Product page: https://www.slidphilabs.com/gc
Silesia board: https://www.slidphilabs.com/silesia.json

---

## [Unreleased]

- Audited CI checks (build, `cargo test`, `lb pcc -> lb test` roundtrip smoke) — workflow staged; requires a token with `workflow` scope to land

---

## [pcc-0.13.0] — 2026-10

**Tag:** `pcc-0.13.0`

### Changed
- Repo health pass: issue/PR templates, CI badge, CONTRIBUTING.md, SECURITY.md (private vulnerability reporting; codec-integrity scope)
- LBR1-SHIP.md and LBR1-CHAMP.md updated

---

## [pcc-0.12.1] — 2026-09 to 2026-10

**Tag:** `pcc-0.12.1`

### Changed
- AWARE byte repeat peel (`model_id=7`): theory wire `{model_id, unit_bytes, n}` decoded as `repeat(unit)[:n]`; crowns published for `text_repeat_256k` (33 B) and `json_128k` (61 B), bit-exact
- CI fixtures: synthesized locked-repeat fixtures inline so CI runs without `/workspace/corpora`
- AWARE `walk_d1` ladder and `walk_lcg` Kolmogorov crown

---

## Benchmark record (Silesia corpus, 12-file, 211,938,580 bytes in)

| Engine | Output bytes | Verified |
|---|---:|---|
| TNSSRC (local) | 43,724,575 | 12/12 DECODE_OK + SHA-256 |
| PCC (hosted) | 51,498,645 | 12/12 DECODE_OK |
| pulsar v2.5.0 | 55,745,438 | 12/12 DECODE_OK |

TNSSRC beats xz-9 on the Silesia sum.

This repo contains the open-source LBR1 measure path only. The private encoder (AWARE + CDDG) is not in this tree.

---

## pulsar v2.5.0

Free GPLv3 lossless compressor binary. Install via npm or directly:

```
npm install -g slid-phi   # includes the pulsar CLI
```

Silesia result: 55,745,438 bytes, 12/12 DECODE_OK.
License: GPL-3.0 (open use) / paid closed commercial embed: $390/yr (`sku: pulsar-exception`)

---

[pcc-0.13.0]: https://github.com/ceedot-rock/lbr1/releases/tag/pcc-0.13.0
[pcc-0.12.1]: https://github.com/ceedot-rock/lbr1/releases/tag/pcc-0.12.1
