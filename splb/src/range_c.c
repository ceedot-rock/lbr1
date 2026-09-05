/* LBR1 VER5 literal context — C twin of src/range.rs lit_idx.
 * ctx = (phi*256 + prev) * 4 + (pos & 3)  → 8192 models.
 */
#include <stdint.h>
#include <stddef.h>

#define PHI_STATES 8
#define POS_STATES 4
#define LIT_MODELS (PHI_STATES * 256 * POS_STATES)

static inline uint32_t lit_ctx(uint32_t phi, uint8_t prev_byte, size_t pos) {
    return ((phi & 7u) * 256u + (uint32_t)prev_byte) * (uint32_t)POS_STATES
         + (uint32_t)(pos & (POS_STATES - 1));
}

/* Each model: 256-wide u16 tree, PINIT 1024. */
void lit_models_init(uint16_t *tab) {
    for (size_t i = 0; i < (size_t)LIT_MODELS * 256u; i++) tab[i] = 1024;
}

uint16_t *lit_model(uint16_t *tab, uint32_t phi, uint8_t prev_byte, size_t pos) {
    return tab + (size_t)lit_ctx(phi, prev_byte, pos) * 256u;
}

/* Frozen lit_idx. Added length / match-bit / dist-slot contexts. */
static inline uint32_t match_ctx(uint32_t phi, uint8_t prev, size_t pos) {
    return (phi & 7u) * 8u + ((uint32_t)(prev >> 6) << 2) + (uint32_t)(pos & 3);
}
static inline uint32_t len_ctx(uint32_t phi, uint32_t prev_len) {
    uint32_t cls = prev_len < 4 ? 0u : prev_len < 8 ? 1u : prev_len < 16 ? 2u : 3u;
    return (phi & 7u) * 4u + cls;
}
static inline uint32_t dist_ctx(uint32_t phi, size_t pos, int prev_match) {
    return (phi & 7u) * 4u + (uint32_t)(pos & 1) * 2u + (prev_match ? 1u : 0u);
}

typedef struct {
    uint16_t match_ctx[64];
    uint16_t p_rep[8];
    uint16_t p_len0[32];
    uint16_t dist_slot[64];
    uint16_t lit_avg[2];
} tiny_book_t;
