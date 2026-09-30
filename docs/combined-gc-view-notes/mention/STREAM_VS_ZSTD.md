# TRUSTREAM vs zstd-3 — streaming note

Measured 2026-08-30 in lab. 4 KiB chunks. Not Silesia. Not samba.

| corpus | TRUSTREAM-py | zstd-3 one-shot | zstd-3 per 4K frame | random grows? |
|---|---:|---:|---:|---|
| zeros 1MB | 0.0020 (8 B/chunk) | 0.0001 | 0.0047 | n/a |
| repeat 1MB | 0.0167 | 0.0001 | 0.0099 | n/a |
| text 1MB | 0.0304 | 0.0002 | 0.0191 | n/a |
| random 256KB | **1.0000** | 1.0001 | 1.0025 | zstd yes, TRUSTREAM no |

`mirror_err = 0.000000` on every TRUSTREAM run.

Use: agent logs, telemetry, zero-heavy traces, any pipe that must not emit expansion.
Do not use: “beats zstd on text.”
