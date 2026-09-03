# LBR1 — private git 2026-09-03

Slid Phi Labs. Proprietary. Do not make this repository public.

## Layout

- `leftbrain/` — finder, classifier, `lb` picker (pulsar-backed until Drive coder is imported)
- `lbr1_price/` — scalar beam-4 DP + C ABI names
- `speed-cmp/pulsar-best/` — public pulsar 2.5.0 submodule (BW23 right brain)
- `COMPARE.md` — LBR1 vs pulsar, picker rule

## Last measured (DECODE_OK)

| file | coded | notes |
|---|---:|---|
| ooffice 6,152,192 | **2,674,974** | Drive champ cap128 beam4 |
| mozilla 51,220,480 | **14,796,694** | Drive champ cap128 beam4 |
| pulsar 2.5.0 Silesia 12/12 | 55,745,438 | public OSCB line |

Measured LBR1 range coder: Drive zip
`https://drive.google.com/file/d/1aWcl8ooRHxFqnt5aMV0vTDrq2q23bCnE/view`

## Build

```
git submodule update --init --recursive
cargo test --workspace
cargo build --release -p leftbrain --bin lb
./target/release/lb stat /path/to/mozilla
./target/release/lb best /path/to/file -o out.lbr
```
