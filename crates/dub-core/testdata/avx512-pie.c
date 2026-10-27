#include <immintrin.h>
#include <stdio.h>
__attribute__((target("avx512f"), noinline)) void add16(float *a, const float *b) {
    _mm512_storeu_ps(a, _mm512_add_ps(_mm512_loadu_ps(a), _mm512_loadu_ps(b)));
}
static __attribute__((target("avx512f"), noinline)) void local_add(float *a) {
    _mm512_storeu_ps(a, _mm512_mul_ps(_mm512_loadu_ps(a), _mm512_loadu_ps(a)));
}
int main(void) { float a[16] = {1}, b[16] = {2}; if (__builtin_cpu_supports("avx512f")) { add16(a, b); local_add(a); } printf("%f\n", a[0]); return 0; }
