# Meld smith — drawing board

Naive `meld_fast` (2 MiB min + fake 6+9n header, no stitch DECODE_OK) lost every file. That was a bake-off with extra headers, not smithing.

`hybrid.rs` already had the packer (class windows, fill runs, coalesce, DECODE_OK) and was **dead**: not in `lib.rs`, magic collided with unused `house.rs` LBHX, `encode_best` never called it.

## Law

1. Packer concatenates specialists. Each segment is still one gene. DECODE_OK of the container.
2. Cut on **class / dormant runs**, not power-of-two.
3. Adjacent same gene → merge → **re-encode the merged span** (restore BWT/LZ context).
4. If after coalesce n=1, emit the whole-file gene. No LBHM wrapper.
5. Keep LBHM only if size **< whole_min**. Tax-inclusive.
6. Combined GC may occupy a private segment in `meld_smith.py`. Not in pulsar. Not a public OSCB line.

## Magic

`LBHM` (not `LBHX`). `encode_best` now mins hybrid vs TRU8/TR8X/LBR1/BW22.

## Cuts under fire (`meld_smith.py`)

| cut | why |
|---|---|
| elide_omni | Fill/solid runs **leave** the gene (uleb dark map). Remainder is one Omni seat (TRU8/TR8X/LBR1/pulsar/GC). Zeros are not a puncture tax. |
| dormant_split | **retired** — 164 holes in xml paid +60k. Do not cut the active stream. |
| class_256k | detect class, coalesce, per-run min(lb, gc, pulsar), re-encode |
| ooffice_gc_1M | whole GC DECODE_FAIL; slices might live; then coalesce |

Baseline: versions_seq whole_min 12,286,459 on the five Silesia files.

## ExCalibur

Provisional name for the **kept** LBHM stitch: one encode path, genes inside. Exists only if smith KEEP size < whole_min + DECODE_OK. If every cut DROP, there is no ExCalibur — house min stays.

## After DONE (do not start on Fly while smith holds the cores)

1. Re-rank assembled options from smith keepers + versions_seq. Plug ExCalibur only if it kept.
2. scp new `lb` (`pcc-0.3.0`, LBHM in `encode_best`). Do not replace mid-smith.
3. Full public-fair matrix: `corpus_matrix.py` × silesia12 + calgary18 + canterbury. enwik8 after those (100 MB). Industry gzip/bzip2/xz/zstd/brotli on the same files (tools now on the box).
4. true8b arm: `libgomp1` is installed; still skip>2MB in `versions_seq` — only zeros/calgary-small unless we lift the cap.
5. Node `spl-codec` occupants still host zlib fallback. Separate from this bench.
6. Stop `pcc-bench` when the matrix is written (billing). enwik9 not on disk.
