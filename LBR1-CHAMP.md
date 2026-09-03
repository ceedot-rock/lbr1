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

vs 14,810,112 cap96: **-13,418**
vs old scalar 14,992,589: **-195,895**
vs xz-9e 13,376,240: **+1,420,454**

mozilla picks 3,539,964 / lits 7,768,958 / avg len 12.27
rep0 19.02% + rep1..3 19.51% = **38.5% total rep**
>4096 picked 184 / 412,094 offered
far>2MiB 15.46% / far>4MiB 10.64% / far>10MiB 5.27%

Product path: `lb best` = min(TRU8, TR8X, LBR1, BW22, LBHX).
mozilla / ooffice / sao stay whole-file LBR1.
