# pulsar-best on Silesia OSCB

Own-only compressor submitted to [Matt Mahoney’s Silesia Open Source Compression Benchmark](https://mattmahoney.net/dc/silesia.html) on 2026-08-31.

- Source: https://github.com/ceedot-rock/pulsar-best
- Total: **56,654,942** / 211,938,580
- DECODE_OK 12/12
- Beats gzip 12/12, loses to bzip2 12/12

Mahoney line:

```
56654942 2907 18482 2647 1886 2936 2947 1307 4719 5121 8948 4276 473  pulsar-best 2.3.1
```

This repo remains the Combined GC / AWARE+XZ1 **product face** (47,752,368, disclosed xz wrap). AWARE is not OSCB-listed: no per-file sizes, wrap is host xz-9 on four files.
