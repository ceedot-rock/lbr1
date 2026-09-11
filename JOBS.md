# Each job knows its job

**PCC** — Ptaszenski Computational Codec. Slid Phi Labs. Dual AGPL-3.0-or-later OR Commercial. Public source. Signing keys stay operator-only.

Not SPL1. Not SPLv1. Not a new platform. Not a host codec with our sticker.

If a layer does two jobs, it lies. OmniWave’s Silesia “wins” were packing **gzip/brotli** and calling it a gene. XZ1 was **host xz** in our frame. Combined GC must not be pasted into pulsar. Those are jobs stealing jobs.

Tree version **pcc-0.13.0** (absorbed genes). Official Silesia matrix now running is **pcc-0.12.1**.

## Jobs

| Job | Knows | Does not |
|---|---|---|
| **parse** | tokens, matches, windows | entropy crowns, disk, HTTP |
| **pack** | magics, lengths, which blob sits in which slot. PCC1 is one stream. PCCZ (`.pcc`) is many files | which engine is “best” |
| **encode** | one gene → one blob, DECODE_OK | other genes |
| **decode** | reverse of that gene only | routing |
| **compress** | the specialist’s domain | pretending to be general by wrapping gzip |
| **store** | persistence, suite meter, access | codecs |
| **route** | seat + min() of own DECODE_OK blobs | compressing |

Router is not a compressor. Packer is not a parser. Combined GC is not pulsar.

## Seats (OmniWave shape, our occupants)

Same seats. Occupants are lab engines.

| Seat | Occupants (order) | Not |
|---|---|---|
| **ZRW_delegate** | TRU8, TR8X, true8b (small residual) | zlib |
| **struct_text** | pulsar BW22 (public OSCB), STR1 | brotli, host xz |
| **general** | LBR1, LZW1, LZM1 | gzip, host xz |
| **paq** | PCAQ, ZMX1, NNC1 | paq8px, cmix (GPL opponents) |
| **mixed** | LBHM (`hybrid.rs`) | puncture 2MB meld |
| **CDDG** | ExCalibur ELID (`arthur_gate`) MIN_RUN=1024 | 64B hole maps, Silesia GP claim |
| **AWARE** | Combined GC own-path (Max, own skins). `gcr1` only if standard skip/fail | dump into pulsar, host xz/gzip/bzip skins |
| **float_xor** | vacant | lossy quantize as lossless. STR1 xor-delta is a transform, not this seat |

Router: classify → seat order → first DECODE_OK gene; then CDDG elide remainder on the **same** order; keep ELID only if `<` whole.  
Text house order: **AWARE, struct_text, general**.  
Binary: **general, AWARE** (skip AWARE on ooffice / files over `AWARE_CAP` 2,000,000).  
Fill: ZRW via `lb encode`.  
Paq mixers only when house ratio still > 0.35 and under their size cap.

`lb champ` = LBR1 quality path (BW22 allowed except mozilla stays MATCH).  
`lb best` = ZRW + struct_text + general + mixed + paq (capped) + PCC.  
`lb aware` / house = those + AWARE.  
gcr1 is not a seat. Puncture meld is not a seat.

## Gene specs (pcc-0.13.0)

DECODE_OK or the gene does not emit.

| Gene | Magic | Window / cap | Job |
|---|---|---|---|
| TRU8 / TR8X | `TR8` / `TR8X` | fill / sparse | solid and sparse runs. Flagship zeros → **8 bytes** |
| pulsar BW22 | `BW22`/`BW23` | whole file | BWT + MTF + rANS. OSCB **55,745,438 / 211,938,580**, 12/12 DECODE_OK |
| LBR1 | `LBR1` | **4 MiB** | BT4 + 4-rep MATCH. Champ mozilla lock **14,796,694** |
| LZW1 | `LZW1` | **64 KiB**, match 4–255 | fast own LZ + rANS. Not host xz |
| LZM1 | `LZM1` | **4 MiB**, 4-rep, match-byte lits | LZMA SDK (PD) reimplementation. Not host `xz`, not XZ1 |
| PCAQ | `PCAQ` | house **256 KiB** | old bitwise mixer |
| ZMX1 | `ZMX1` | house **1 MiB** (`lb zmix` may go larger) | libzpaq (PD): ICM + stretch/squash + SSE + match |
| NNC1 | `NNC1` | house **256 KiB** (`lb nnc` may go larger) | NNCP (MIT) idea: train-while-compress, tiny 2-layer net. Not a Transformer |
| STR1 | `STR1` | no size cap; skip if H > 7.7 | OpenZL (BSD) idea: record-width / transpose / xor-delta, then own inner gene |
| LBHM | `LBHM` | seamed files | split then min() |
| PCC1 | `PCC1` | codec frame | ops ZERO MATCH BWT CMAQ LZ LZM ZMIX STR NNC STORE |
| PCCZ | `PCCZ` | archive | `.pcc` zip. Zip-slip rejected |
| Combined GC | own magics | ASMD **2,000,000** bytes | AWARE occupant. Host xz/gzip/bzip skins dropped |
| GSS1 | `GSS1` | only if house ratio still > 0.88 | residue raffle. True random does not shrink |

Hosted API: **4 MiB**, 45s, labeled `/bench`. Not official Silesia. ZMX1/NNC1 house caps mean a full 4 MiB hosted job will not sit those two mixers.

## Official Silesia (whole files, md5-matched, DECODE_OK)

Raw **211,938,580**. gzip/bzip2/xz are opponents. Not a #1 claim. Matrix `pcc-0.12.1`.

| Pathway | Packed | DECODE_OK | Spec |
|---|---:|---:|---|
| **pulsar 2.5.0** | **55,745,438** | 12/12 | Matches published OSCB. Beats gzip-9 (~67.6M). Loses to xz-6 (~49.4M) and zpaq-class |
| **PCC** | **51,498,645** | 12/12 | Own codec. 4,246,793 inside pulsar. Still loses to xz-6 (~49.4M). Not a Mahoney line until we send one |
| champ | running | — | LBR1 quality. mozilla lock 14,796,694 is the MATCH spec, not this matrix yet |
| best / aware | not started | — | Do not write a total until 12/12 |
| AWARE+XZ1 **47,752,368** | retired | — | Host xz occupant. Off the scoreboard. Not the product spec |

Mahoney-format (KB truncated):

```
55745438   2834 18110  2596  1764  2950  2844  1331  4699  5150  8678  4307   478  pulsar
51498645   2738 15970  2537  1582  2670  2761  1217  4156  4992  8169  4260   443  pcc
```

Do not email Mahoney a second pulsar line (it did not beat 55,745,438). PCC is a different compressor; a letter is a separate go.

## Steal vs know

| Steal | Know |
|---|---|
| gzip inside OmniWave `general` | LBR1 / LZW1 / LZM1 in `general` |
| brotli inside `struct_text` | pulsar + STR1 in `struct_text` |
| xz as XZ1 | LZM1 (reimplementation) or Combined GC as itself |
| paq8px/cmix inside a closed binary | ZMX1 / NNC1 / PCAQ (own genes). paq8px stays an opponent |
| 6.03× float auto as crown | vacant float seat |
| one binary that “is everything” | route → encode → pack → store |

## Teach this to the next run

1. Name the job before you write a line.
2. If it needs another job, call that job — don’t absorb it into the wrong layer. Absorbing an *algorithm* into an own gene is the job of encode.
3. DECODE_OK is the encoder/decoder handshake. No handshake, no blob.
4. Public names: pulsar on OSCB, PCC on this board, TRU8 on zeros, AWARE hosted. Combined GC source is public dual-license; keys stay operator-only.
5. Codec name is **PCC** (Ptaszenski Computational Codec). Lab is Slid Phi Labs. Not SPL1. Not SPLv1.
6. Host xz/gzip/bzip are opponents. GPL paq8px/cmix are opponents. Dual license does not relicense paq.

## Names that fooled us

| We said | Industry name | Job |
|---|---|---|
| OmniWave seats | Mixture of experts (Jacobs 1991; DualComp 2025) | Route whole file to one specialist |
| SmartSwarm / M40 | PSO/ACO if knobs are many; else just try them | Offline search. Stub until it exists |
| Sentinel NCA | Controller, not a gene | Throttle / Block. Decode never mutates |
| Combined GC `nca.rs` | Genetic program search | Mutate pipelines. Not Distill NCA |
| gzip inside a lab frame | Host codec | Opponent, not occupant |
| XZ1 | host xz | Retired occupant. LZM1 is the own LZMA-style gene |

Beginner map with citations: `SlidPhiLabs/docs/GAPS_NCA_SWARM.md`.

In His name we code. Residual only. Proof before praise.

## PCC daily Dial A (shallower parse)

`LBR1_PARSE=hc4 LBR1_WINDOW=1048576 LBR1_CHAIN=4 LBR1_LAZY=0 LBR1_PACK=ml4` (fallback CHAIN=8).
FAIL_LOUD if mozilla packed ≥ zstd-9 (16,735,963) or DECODE_OK false. See `bench/pcc-dial-a-fail-loud.md`.
