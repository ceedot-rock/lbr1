# Reproduce the industry cells

House Combined GC binary is private. Anyone can reproduce the comparison rows.

```bash
curl -L -o silesia.zip http://sun.aei.polsl.pl/~sdeor/corpus/silesia.zip
unzip silesia.zip -d silesia
gzip -9 -k silesia/*
xz   -9 -k silesia/*
```

Frozen totals (211,938,580 raw) — **standings = own-path**:

| codec | bytes |
|---|---:|
| zpaq 7.15 -m5 | 39,112,870 |
| xz-9 | 48,795,480 |
| brotli-11 | 49,564,563 |
| zstd-19 | 53,024,573 |
| Combined GC 1.19.2 own-path | 55,186,156 |
| gzip-9 | 67,631,990 |

See SILESIA.md and mention/CLAIMS.md. XZ1 hybrid is appendix-only / not standings.
