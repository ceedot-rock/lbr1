# SPLv1 vs pulsar — private note 2026-09-03

Not public. Not Combined GC. Not a rank claim vs paq8px / cmix / zpaq.

## Wrap our own methods (the DNA question)

Yes — if every payload is a lab engine, wrap them in one house. That is **LBHX**.

DNA is not a second compressor and not a Combined GC dump. It is a **match class inside LBR1**: `data[a+k] == !data[b+k]` priced as a cheap rep (3 bits), same as a recent distance. Complement of 0x00 is 0xFF; that is the binary analogue of A↔T / C↔G.

House tags (one blob, decoder reads the tag):

| tag | engine | when |
|---|---|---|
| TRU8 | solid run | fill ≥4 KiB |
| TR8X | sparse tile | mostly zeros |
| BW22 | pulsar 2.5.0 | text / BWT-friendly |
| LBR1 | hash-chain + beam-4 + Phi, DNA comps on | binaries |
| LBHX | this wrapper | mixed file, split only if mixed ≥8% |

Combined GC stays a **private product**, not a tag in this git tree. Its own-path pieces (ZRW / Gate / Mix) can feed AWARE later. Host xz wrap (XZ1) stays retired — OSCB will not count a gzip/xz skin as ours.

Wrapping everything “like DNA” means: one genome (LBHX), many genes (own engines), complement matches as a cheap mutation class. It does **not** mean paste Combined GC source into pulsar.

## Pathway (honest)

Own-path picker on pulsar 2.5.0 + measured LBR1 wins:

`55,745,438 − 3,313,819 (mozilla) − 275,594 (ooffice) ≈ 52,156,025`

Beats published bzip2-9 (54.5M). Still loses to xz-6 (~49.4M). Mozilla LBR1 14,796,694 is still **+1.42M vs xz-9e**. DNA Prompt 6 expected −20k to −80k on mozilla if complement hits exist — not the 1.4M gap.

To actually lead an official table we need LBR1+DNA+coder to close that mozilla hole **without** wrapping xz. Until a live 12-file DECODE_OK total beats xz on own engines, we do not say #1.

## Measured

| | mozilla 51,220,480 | ooffice 6,152,192 | Silesia 12-file |
|---|---:|---:|---:|
| pulsar 2.5.0 live | 18,110,513 | 2,950,568 | **55,745,438** |
| LBR1 Drive champ (cap128 beam4) | **14,796,694** | **2,674,974** | no full 12-file in this tree |
| xz-9e / xz-6 | 13,376,240 | — | ~49.4M |
| bzip2-9 | — | — | 54,506,769 |

## This git tree

Finder + DNA complement class + beam-4 price + LBHX house + pulsar-backed `lb best`.
Measured LBR1 range coder is still Drive zip `lbr1-private-snapshot-2026-09-03.zip`.
`lb fast` beam-1 is not a product path.
