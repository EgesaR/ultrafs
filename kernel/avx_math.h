#ifndef AVX_MATH_H
#define AVX_MATH_H

#include <stdint.h>
#include <stddef.h>

extern "C" {
    // Calculates the L1 Norm between two blocks of data using AVX-256
    uint64_t compute_l1_norm_avx256(const uint8_t* block_a, const uint8_t* block_b, size_t length);
}

#endif // AVX_MATH_H