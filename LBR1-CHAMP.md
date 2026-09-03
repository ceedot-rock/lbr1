# LBR1 live champ — banked 2026-09-03

DECODE_OK. Cap 65,535. Hybrid LBHX does not fire on uniform Binary.

| file | raw | coded | ratio | kind |
|---|---:|---:|---:|---|
| mozilla | 51,220,480 | **14,810,112** | 0.2891 | lbr1 |
| ooffice | 6,152,192 | **2,678,004** | 0.4353 | lbr1 |

enc mozilla 250s, peak 433 MB.

```
15,709,557  Old
15,501,117  Old+rep4           -208,440
15,447,182  beam-2             -53,935
15,373,305  beam-4             -73,877
14,810,112  scalar DP + beam-4 -563,193
```

vs old scalar bits_* 14,992,589: **-182,477**
vs xz-9e 13,376,240: **+1,433,872**

mozilla picks: 3,554,298 / lits 7,778,671 / avg len 12.22
rep0 19.04% + rep1..3 19.50% = **38.5% total rep**
>4096 picked 184 / 410,204 offered

Product path: `lb best` = min(TRU8, TR8X, LBR1, BW22, LBHX).
mozilla / ooffice / sao stay whole-file LBR1.
