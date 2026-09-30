#include "avx_math.h"
#include <immintrin.h> // Intel AVX/SSE Intrinsics instructions

extern "C" {
    uint64_t compute_l1_norm_avx256(const uint8_t* block_a, const uint8_t* block_b, size_t length) {
        uint64_t total_diff = 0;
        size_t i = 0;

        // Initialize a 256-bit register to zero to hold our rolling sums
        __m256i sum_vec = _mm256_setzero_si256();

        // Loop through the data in chunks of 32 bytes (256 bits)
        for (; i + 31 < length; i += 32) {
            // Unaligned load of 256 bits from Block A and Block B into CPU registers
            __m256i vec_a = _mm256_loadu_si256(reinterpret_cast<const __m256i*>(block_a + i));
            __m256i vec_b = _mm256_loadu_si256(reinterpret_cast<const __m256i*>(block_b + i));

            // THE MAGIC: _mm256_subs_epu8 computes the absolute difference of 32 paired bytes
            // and horizontally adds them into 64-bit integers lanes in a single CPU cycle.
            __m256i sad = _mm256_sad_epu8(vec_a, vec_b);

            // Accumulate the sums into our running total vector
            sum_vec = _mm256_add_epi64(sum_vec, sad);
        }

        // Extract the four 64-bit lane totals from the 256-bit register
        alignas(32) uint64_t sums[4];
        _mm256_store_si256(reinterpret_cast<__m256i*>(sums), sum_vec);
        total_diff = sums[0] + sums[1] + sums[2] + sums[3];

        // Scalar fallback: Handle any remaining bytes if length isn't a perfect multiple of 32
        for (; i < length; ++i) {
            uint8_t a = block_a[i];
            uint8_t b = block_b[i];
            total_diff += (a > b) ? (a - b) : (b - a); // Absolute difference for remaining bytes
        }

        return total_diff;
    }
} // End of extern "C" block