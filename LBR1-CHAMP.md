# SPLv1 live champ — banked 2026-09-03

**SPLv1** is the product. Slid Phi Labs, version 1.
Engines stay LBR1 / LBHX / TRU8 / TR8X / BW22.

DECODE_OK. Match cap 65,535. Chain cap **128**. Hybrid LBHX does not fire on uniform Binary.

| file | raw | coded | ratio | kind |
|---|---:|---:|---:|---|
| mozilla | 51,220,480 | **14,796,694** | 0.2889 | lbr1 |
| ooffice | 6,152,192 | **2,674,974** | 0.4348 | lbr1 |

mozilla enc 182 s, dec 1.6 s, peak 831 MB. DECODE_OK.

```
15,709,557  Old
15,501,117  Old+rep4
15,447,182  beam-2
15,373,305  beam-4
14,810,112  scalar DP + beam-4 cap96
14,796,694  cap128 beam4          -13,418   live
```

vs xz-9e 13,376,240: +1,420,454
