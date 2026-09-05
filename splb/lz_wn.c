/* W=n greedy. No NCA, no POSIX prior.
 * Offsets: rep / u8 / u16 / u32 (u32 so d > 16MiB is legal).
 * token 0x00 = literal only. bit7 = match.
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define HASH_BITS 20
#define HASH_SIZE (1u << HASH_BITS)
#define MINM 4
#define MAXM 65535
#define CHAIN 32
#define ZERO_CHAIN 4

static inline uint32_t h4(const uint8_t *p) {
    uint32_t v = (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
    return (v * 0x85ebca6bu) >> (32 - HASH_BITS);
}

static uint32_t match_len(const uint8_t *a, const uint8_t *b, uint32_t max) {
    uint32_t n = 0;
    while (n + 8 <= max && *(const uint64_t *)(a + n) == *(const uint64_t *)(b + n))
        n += 8;
    while (n < max && a[n] == b[n])
        n++;
    return n;
}

/* bit7=1, bits6-5 off_kind: 0=rep 1=u8 2=u16 3=u32, bits4-0 len */
static uint8_t make_token(uint32_t off, uint32_t len, uint32_t last) {
    uint32_t ok;
    if (off == last && last != 0)
        ok = 0;
    else if (off < 256)
        ok = 1;
    else if (off < 65536)
        ok = 2;
    else
        ok = 3;
    uint32_t lc = (len < 34) ? (len - 4) : (len < 289) ? 30 : 31;
    return (uint8_t)(0x80 | (ok << 5) | lc);
}

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr, "usage: %s IN OUT.lz\n", argv[0]);
        return 1;
    }
    FILE *f = fopen(argv[1], "rb");
    if (!f) {
        perror(argv[1]);
        return 1;
    }
    fseek(f, 0, SEEK_END);
    long fsz = ftell(f);
    fseek(f, 0, SEEK_SET);
    if (fsz <= 0) {
        fprintf(stderr, "empty\n");
        return 1;
    }
    uint8_t *data = malloc((size_t)fsz);
    if (!data || fread(data, 1, (size_t)fsz, f) != (size_t)fsz) {
        fprintf(stderr, "read fail\n");
        return 1;
    }
    fclose(f);
    uint32_t n = (uint32_t)fsz;
    printf("Input %u\n", n);
    fflush(stdout);

    int32_t *head = malloc(HASH_SIZE * sizeof(int32_t));
    int32_t *prev = malloc((size_t)n * sizeof(int32_t));
    uint8_t *comp = malloc((size_t)n + n / 5 + 16);
    if (!head || !prev || !comp) {
        fprintf(stderr, "oom\n");
        return 1;
    }
    memset(head, 0xFF, HASH_SIZE * sizeof(int32_t));
    memset(prev, 0xFF, (size_t)n * sizeof(int32_t));

    uint32_t cp = 0, last = 0, pos = 0;
    uint64_t far16 = 0, reps = 0;

    while (pos < n) {
        if (pos + MINM > n) {
            comp[cp++] = 0x00;
            comp[cp++] = data[pos++];
            continue;
        }
        uint32_t h = h4(data + pos);
        int32_t mp = head[h];
        int chain = 0;
        int lim = (data[pos] | data[pos + 1] | data[pos + 2] | data[pos + 3]) == 0 ? ZERO_CHAIN : CHAIN;
        uint32_t best_l = 0, best_d = 0;
        while (mp >= 0 && chain < lim) {
            uint32_t d = pos - (uint32_t)mp;
            if (d == 0 || d > pos) {
                break;
            }
            uint32_t max = n - pos;
            if (max > MAXM)
                max = MAXM;
            if ((uint32_t)mp + max > n)
                max = n - (uint32_t)mp;
            uint32_t L = match_len(data + (uint32_t)mp, data + pos, max);
            if (L >= MINM && L > best_l) {
                best_l = L;
                best_d = d;
                if (L >= 64)
                    break;
            }
            mp = prev[mp];
            chain++;
        }
        /* lazy: if next byte has a clearly longer match, emit lit */
        if (best_l >= MINM && pos + 1 + MINM <= n) {
            uint32_t h2 = h4(data + pos + 1);
            int32_t mp2 = head[h2];
            if (mp2 >= 0) {
                uint32_t d2 = (pos + 1) - (uint32_t)mp2;
                if (d2 > 0 && d2 <= pos + 1) {
                    uint32_t max = n - (pos + 1);
                    if (max > 64)
                        max = 64;
                    uint32_t L2 = match_len(data + (uint32_t)mp2, data + pos + 1, max);
                    if (L2 > best_l + 1) {
                        best_l = 0;
                    }
                }
            }
        }
        if (best_l >= MINM && last != 0 && pos >= last) {
            uint32_t max = best_l;
            if (max > n - pos)
                max = n - pos;
            uint32_t lr = match_len(data + pos - last, data + pos, max);
            if (lr >= MINM && lr + 1 >= best_l) {
                best_l = lr;
                best_d = last;
                reps++;
            }
        }
        if (best_l >= MINM) {
            if (best_d > (1u << 24))
                far16++;
            uint8_t tok = make_token(best_d, best_l, last);
            uint32_t ok = (tok >> 5) & 3;
            uint32_t lc = tok & 31;
            comp[cp++] = tok;
            if (ok == 1)
                comp[cp++] = (uint8_t)best_d;
            else if (ok == 2) {
                comp[cp++] = (uint8_t)best_d;
                comp[cp++] = (uint8_t)(best_d >> 8);
            } else if (ok == 3) {
                comp[cp++] = (uint8_t)best_d;
                comp[cp++] = (uint8_t)(best_d >> 8);
                comp[cp++] = (uint8_t)(best_d >> 16);
                comp[cp++] = (uint8_t)(best_d >> 24);
            }
            if (lc == 30)
                comp[cp++] = (uint8_t)(best_l - 34);
            else if (lc == 31) {
                uint32_t v = best_l - 289;
                comp[cp++] = (uint8_t)v;
                comp[cp++] = (uint8_t)(v >> 8);
            }
            last = best_d;
            for (uint32_t i = 0; i < best_l && pos + i + 4 <= n; i++) {
                uint32_t hh = h4(data + pos + i);
                prev[pos + i] = head[hh];
                head[hh] = (int32_t)(pos + i);
            }
            pos += best_l;
        } else {
            comp[cp++] = 0x00;
            comp[cp++] = data[pos];
            prev[pos] = head[h];
            head[h] = (int32_t)pos;
            pos++;
        }
    }

    printf("coded=%u ratio=%.4f bits/B=%.3f far>16MiB_used=%lu reps=%lu\n",
           cp, (double)n / cp, (double)cp * 8.0 / n, (unsigned long)far16, (unsigned long)reps);
    printf("lbr1=17414444 xz6=13503600\n");
    fflush(stdout);

    uint8_t *out = malloc((size_t)n);
    uint32_t sp = 0, dp = 0, lo = 0;
    int okd = 1;
    while (sp < cp && dp < n) {
        uint8_t t = comp[sp++];
        if (t == 0x00) {
            if (sp >= cp) {
                okd = 0;
                break;
            }
            out[dp++] = comp[sp++];
            continue;
        }
        if ((t & 0x80) == 0) {
            okd = 0;
            break;
        }
        uint32_t ok = (t >> 5) & 3, lc = t & 31, off, len;
        if (ok == 0)
            off = lo;
        else if (ok == 1) {
            if (sp >= cp) {
                okd = 0;
                break;
            }
            off = comp[sp++];
        } else if (ok == 2) {
            if (sp + 1 >= cp) {
                okd = 0;
                break;
            }
            off = comp[sp] | ((uint32_t)comp[sp + 1] << 8);
            sp += 2;
        } else {
            if (sp + 3 >= cp) {
                okd = 0;
                break;
            }
            off = (uint32_t)comp[sp] | ((uint32_t)comp[sp + 1] << 8) | ((uint32_t)comp[sp + 2] << 16) | ((uint32_t)comp[sp + 3] << 24);
            sp += 4;
        }
        if (lc < 30)
            len = lc + 4;
        else if (lc == 30) {
            if (sp >= cp) {
                okd = 0;
                break;
            }
            len = (uint32_t)comp[sp++] + 34;
        } else {
            if (sp + 1 >= cp) {
                okd = 0;
                break;
            }
            len = (uint32_t)comp[sp] | ((uint32_t)comp[sp + 1] << 8);
            len += 289;
            sp += 2;
        }
        if (off == 0 || off > dp || dp + len > n) {
            okd = 0;
            break;
        }
        for (uint32_t i = 0; i < len; i++)
            out[dp + i] = out[dp - off + i];
        dp += len;
        lo = off;
    }
    if (okd && dp == n && memcmp(data, out, n) == 0)
        printf("DECODE_OK %u\n", dp);
    else
        printf("DECODE_FAIL dp=%u ok=%d\n", dp, okd);

    FILE *o = fopen(argv[2], "wb");
    if (o) {
        fwrite(comp, 1, cp, o);
        fclose(o);
        printf("Wrote %s %u\n", argv[2], cp);
    }
    return (okd && dp == n) ? 0 : 1;
}
