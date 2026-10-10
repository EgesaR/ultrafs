#include "avx_math.h"
#include <immintrin.h> // Intel AVX/SSE Intrinsics instructions

extern "C" {
    uint64_t compute_l1_norm_avx256(const uint8_t* block_a, const uint8_t* block_b, size_t length) {
        uint64_t total_diff = 0;
        size_t i = 0;

        // Use four independent accumulators to break instruction dependency chains
        // Initialize 4 256-bit register to zero to hold our rolling sums
        __m256i sum0 = _mm256_setzero_si256();
        __m256i sum1 = _mm256_setzero_si256();
        __m256i sum2 = _mm256_setzero_si256();
        __m256i sum3 = _mm256_setzero_si256();

       // Unroll the loop to process 128 bytes per iteration
       for (; i + 127 < length; i+= 128)
       {
            // UltraFS blocks are pre-aligned, allowing fast aligned loads
            __m256i a0 = _mm256_load_si256((const __m256i *)(block_a + i));
            __m256i b0 = _mm256_load_si256((const __m256i *)(block_b + i));

            __m256i a1 = _mm256_load_si256((const __m256i *)(block_a + i + 32));
            __m256i b1 = _mm256_load_si256((const __m256i *)(block_b + i + 32));

            __m256i a2 = _mm256_load_si256((const __m256i *)(block_a + i + 64));
            __m256i b2 = _mm256_load_si256((const __m256i *)(block_b + i + 64));
            
            __m256i a3 = _mm256_load_si256((const __m256i *)(block_a + i + 96));
            __m256i b3 = _mm256_load_si256((const __m256i *)(block_b + i + 96));

            sum0 = _mm256_add_epi64(sum0, _mm256_sad_epu8(a0, b0));
            sum1 = _mm256_add_epi64(sum1, _mm256_sad_epu8(a1, b1));
            sum2 = _mm256_add_epi64(sum2, _mm256_sad_epu8(a2, b2));
            sum3 = _mm256_add_epi64(sum3, _mm256_sad_epu8(a3, b3));
       }

       // Condense the 4 accumulators into 1
       __m256i final_sum = _mm256_add_epi64(
           _mm256_add_epi64(sum0, sum1),
           _mm256_add_epi64(sum2, sum3)
        );

        // Process any remaining 32-byte chunks
       for (; i + 31 < length; i+= 32)
       {
       __m256i a = _mm256_load_si256((const __m256i*)(block_a + i));
            __m256i b = _mm256_load_si256((const __m256i*)(block_b + i));
            final_sum = _mm256_add_epi64(final_sum, _mm256_sad_epu8(a, b));
        }

        alignas(32) uint64_t sums[4];
        _mm256_store_si256((__m256i*)sums, final_sum);
        total_diff = sums[0] + sums[1] + sums[2] + sums[3];

        // Scalar fallback for tail bytes
        for (; i < length; ++i) {
            uint8_t a = block_a[i];
            uint8_t b = block_b[i];
            total_diff += (a > b) ? (a - b) : (b - a);
        }

        return total_diff;
        
    }
} // End of extern "C" block