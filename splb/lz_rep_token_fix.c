/* Fix: literal token 0 collided with last-offset (rep) + len 4.
 * All matches now set bit 7. Token 0 is literal only.
 *
 * token == 0x00            -> literal + 1 raw byte
 * token bit7 = 1           -> match
 *   bits6-5 off_kind: 0=rep (no extra), 1=u8, 2=u16, 3=u24
 *   bits4-0 len_code:
 *     0..29  -> len = 4 + code
 *     30     -> len = 34 + u8
 *     31     -> len = 290 + u16
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define MIN_MATCH 4

static uint32_t match_parts(uint32_t off, uint32_t len, uint32_t last_off,
                            uint32_t *off_kind, uint32_t *len_code, uint32_t *len_extra,
                            uint32_t *len_extra_n) {
    if (off == last_off && last_off != 0)
        *off_kind = 0;
    else if (off < 256)
        *off_kind = 1;
    else if (off < 65536)
        *off_kind = 2;
    else
        *off_kind = 3;

    *len_extra = 0;
    *len_extra_n = 0;
    if (len < 34) {
        *len_code = len - MIN_MATCH; /* 0..29 */
    } else if (len < 290) {
        *len_code = 30;
        *len_extra = len - 34;
        *len_extra_n = 1;
    } else {
        *len_code = 31;
        *len_extra = len - 290;
        *len_extra_n = 2;
    }
    return 1 + *off_kind + *len_extra_n;
}

static uint32_t emit_lit(uint8_t *dst, uint32_t dp, uint8_t b, uint32_t cap) {
    if (dp + 2 > cap)
        return cap + 1;
    dst[dp++] = 0;
    dst[dp++] = b;
    return dp;
}

static uint32_t emit_match(uint8_t *dst, uint32_t dp, uint32_t cap,
                           uint32_t off, uint32_t len, uint32_t last_off) {
    uint32_t ok, lc, ex, en;
    match_parts(off, len, last_off, &ok, &lc, &ex, &en);
    if (dp + 1 + ok + en > cap)
        return cap + 1;
    /* bit7 always set — never 0, even for rep+len4 */
    uint8_t token = (uint8_t)(0x80 | (ok << 5) | (lc & 0x1F));
    dst[dp++] = token;
    if (ok >= 1)
        dst[dp++] = (uint8_t)(off);
    if (ok >= 2)
        dst[dp++] = (uint8_t)(off >> 8);
    if (ok >= 3)
        dst[dp++] = (uint8_t)(off >> 16);
    if (en == 1)
        dst[dp++] = (uint8_t)ex;
    if (en == 2) {
        dst[dp++] = (uint8_t)ex;
        dst[dp++] = (uint8_t)(ex >> 8);
    }
    return dp;
}

static int decode(const uint8_t *src, uint32_t slen, uint8_t *dst, uint32_t dcap,
                  uint32_t *out, uint32_t *last_off_io) {
    uint32_t sp = 0, dp = 0;
    uint32_t last_off = *last_off_io;
    while (sp < slen && dp < dcap) {
        uint8_t token = src[sp++];
        if (token == 0) {
            if (sp >= slen)
                return -2;
            dst[dp++] = src[sp++];
            continue;
        }
        if ((token & 0x80) == 0)
            return -3; /* reserved */
        uint32_t off_kind = (token >> 5) & 3;
        uint32_t len_code = token & 0x1F;
        uint32_t off;
        if (off_kind == 0) {
            off = last_off;
            if (off == 0)
                return -4;
        } else {
            if (sp + off_kind > slen)
                return -5;
            off = 0;
            if (off_kind >= 1)
                off |= src[sp++];
            if (off_kind >= 2)
                off |= (uint32_t)src[sp++] << 8;
            if (off_kind >= 3)
                off |= (uint32_t)src[sp++] << 16;
        }
        uint32_t len;
        if (len_code < 30)
            len = len_code + MIN_MATCH;
        else if (len_code == 30) {
            if (sp >= slen)
                return -6;
            len = 34 + src[sp++];
        } else {
            if (sp + 1 >= slen)
                return -7;
            len = 290 + src[sp] + ((uint32_t)src[sp + 1] << 8);
            sp += 2;
        }
        if (off == 0 || off > dp || dp + len > dcap)
            return -8;
        for (uint32_t i = 0; i < len; i++)
            dst[dp + i] = dst[dp - off + i];
        dp += len;
        last_off = off;
    }
    *out = dp;
    *last_off_io = last_off;
    return 0;
}

/* Encode a buffer with a dumb but collision-prone parse:
 * first ABCD as lits, then rep off=4 len=4 for the rest. */
static uint32_t encode_rep4(const uint8_t *src, uint32_t n, uint8_t *dst, uint32_t cap) {
    uint32_t dp = 0;
    uint32_t last = 0;
    uint32_t i = 0;
    while (i < n) {
        if (i >= 4 && i + 4 <= n && memcmp(src + i, src + i - 4, 4) == 0) {
            uint32_t len = 4;
            while (i + len < n && src[i + len] == src[i + len - 4])
                len++;
            dp = emit_match(dst, dp, cap, 4, len, last);
            last = 4;
            i += len;
        } else {
            dp = emit_lit(dst, dp, src[i], cap);
            i++;
        }
    }
    return dp;
}

int main(void) {
    uint8_t raw[400];
    for (int i = 0; i < 100; i++) {
        raw[i * 4 + 0] = 'A';
        raw[i * 4 + 1] = 'B';
        raw[i * 4 + 2] = 'C';
        raw[i * 4 + 3] = 'D';
    }
    uint8_t comp[800];
    uint32_t clen = encode_rep4(raw, 400, comp, sizeof(comp));
    printf("raw=400 coded=%u first_tokens=%02x %02x %02x %02x %02x %02x\n",
           clen, comp[0], comp[1], comp[2], comp[3], comp[4], comp[5]);
    /* After 4 lits (8 bytes 00 41 00 42 00 43 00 44) next must NOT be 00. */
    if (clen < 9) {
        printf("FAIL too short\n");
        return 1;
    }
    if (comp[8] == 0) {
        printf("FAIL rep token still 0\n");
        return 1;
    }
    if ((comp[8] & 0x80) == 0) {
        printf("FAIL match missing high bit\n");
        return 1;
    }
    uint8_t back[400];
    uint32_t out = 0, last = 0;
    int r = decode(comp, clen, back, 400, &out, &last);
    if (r != 0 || out != 400 || memcmp(raw, back, 400) != 0) {
        printf("DECODE_FAIL ret=%d out=%u\n", r, out);
        return 1;
    }
    printf("DECODE_OK rep-len4 no longer collides with literal\n");
    return 0;
}
