# ZMIX / NNC / STR — PCC genes

PCC1 ops 7 / 8 / 9. DECODE_OK or no emit. House caps: ZMX1 1 MiB, NNC1 256 KiB. `lb zmix` / `lb nnc` / `lb str` may go larger.

| Gene | Magic | Job |
|---|---|---|
| ZMX1 | `ZMX1` | 8 ICM contexts + stretch/squash mix + SSE + 4-byte match. Bitwise range coder. |
| NNC1 | `NNC1` | Same 8 hashes feed a 8×8 tanh net, train-while-compress. Word hash + match-len. |
| STR1 | `STR1` | Detect record width, delta / xor / transpose, then inner LZ / LZM / rANS / ZMX1 / NNC1. Skip if H > 7.7. |

Silesia standing for PCC remains **pcc-0.12.1 = 51,498,645**. These genes sit on pcc-0.13.0 house when MATCH/BWT leave ratio > 0.50 (PCC frame) or > 0.35 (`encode_best`).
