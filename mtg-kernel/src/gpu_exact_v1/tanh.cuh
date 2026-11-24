// The production activation on the GPU, bit for bit.
//
// `vector_tanh_v1::windows::tanh8` is UCRT's FMA3 `tanhf` (AMD libm's
// algorithm) as IEEE float operations; this is the same operations on one
// lane, each with a correctly rounded intrinsic, so every result is the
// IEEE result and equals tanh8's lane. `gpu_exact_v1::tanh_proof` compares
// this function with the host `tanhf` on all 2^32 inputs before a device is
// used. Constants are written as bit patterns so no decimal parse is involved.

__constant__ unsigned int MTG_TANH_LEAD[32] = {
    0x3F800000u, 0x3F82C000u, 0x3F858000u, 0x3F888000u, 0x3F8B8000u, 0x3F8E8000u, 0x3F91C000u,
    0x3F94C000u, 0x3F980000u, 0x3F9B8000u, 0x3F9EC000u, 0x3FA24000u, 0x3FA5C000u, 0x3FA98000u,
    0x3FAD4000u, 0x3FB10000u, 0x3FB50000u, 0x3FB8C000u, 0x3FBD0000u, 0x3FC10000u, 0x3FC54000u,
    0x3FC98000u, 0x3FCE0000u, 0x3FD28000u, 0x3FD74000u, 0x3FDBC000u, 0x3FE0C000u, 0x3FE58000u,
    0x3FEAC000u, 0x3FEFC000u, 0x3FF50000u, 0x3FFA8000u};
__constant__ unsigned int MTG_TANH_TRAIL[32] = {
    0x00000000u, 0x39D86988u, 0x3AAB0D9Fu, 0x3A407404u, 0x3A2E0F1Eu, 0x3A90E62Du, 0x38F4DCE0u,
    0x3AD3BEA3u, 0x3ADFC146u, 0x39D39B9Cu, 0x3AD4C982u, 0x3AC10C0Cu, 0x3AFB5AA6u, 0x3A856AD3u,
    0x3A41F752u, 0x3A8FD607u, 0x391E6678u, 0x3AEEBD1Du, 0x398A39F4u, 0x3AB13329u, 0x3A9CA845u,
    0x3AE6F619u, 0x3A923054u, 0x3AA07647u, 0x391F9958u, 0x3AEEDE5Fu, 0x39CDEEC0u, 0x3AE41B9Du,
    0x37C6E7C0u, 0x3A92E66Fu, 0x3A95F454u, 0x38ECB6D0u};

__device__ __forceinline__ float mtg_bits(unsigned int bits) { return __uint_as_float(bits); }

__device__ float mtg_tanh(float x) {
    const unsigned int ux = __float_as_uint(x);
    const unsigned int abs_bits = ux & 0x7fffffffu;
    const unsigned int sign = ux & 0x80000000u;
    if (abs_bits > 0x7f800000u) return x;      // NaN: callers fail closed on non-finite output
    if (abs_bits < 0x39000000u) return x;      // |x| < 2^-13: the identity
    const float y = __uint_as_float(abs_bits);
    if (y > 10.0f) return __uint_as_float(0x3f800000u | sign);
    float z;
    if (y <= 1.0f) {
        // y + y*y2*n/d, coefficients for [0, 0.9) or [0.9, 1].
        const bool below = y < mtg_bits(0x3f666666u);  // 0.9
        const float c0 = below ? mtg_bits(0xbe9058e0u) : mtg_bits(0xbe7679b1u);
        const float c1 = below ? mtg_bits(0xbabfbc9eu) : mtg_bits(0xbaa18e00u);
        const float c2 = below ? mtg_bits(0x384d2b7du) : mtg_bits(0x382089d8u);
        const float d0 = below ? mtg_bits(0x3f588550u) : mtg_bits(0x3f38db60u);
        const float d1 = below ? mtg_bits(0x3eaf769cu) : mtg_bits(0x3e95c65fu);
        const float y2 = __fmul_rn(y, y);
        const float n = __fmaf_rn(__fmaf_rn(c2, y2, c1), y2, c0);
        const float d = __fmaf_rn(d1, y2, d0);
        z = __fadd_rn(y, __fdiv_rn(__fmul_rn(__fmul_rn(y, y2), n), d));
    } else {
        // 1 - 2/(exp(2y) + 1), exp as 2^m * (z1 + z2) (`splitexpf`).
        const float t = __fmul_rn(2.0f, y);
        const float r = __fmul_rn(t, mtg_bits(0x4238aa3bu));        // 46.16624
        const int ni = __float2int_rz(__fadd_rn(r, 0.5f));
        const float nf = __int2float_rn(ni);
        const float trail = mtg_bits(0x3585fdf4u);                   // 9.983182e-7
        const float r1 = __fsub_rn(t, __fmul_rn(nf, mtg_bits(0x3cb17000u)));  // 0.021659851
        const float r2 = __fsub_rn(0.0f, __fmul_rn(nf, trail));
        const int j = ni & 0x1f;
        const float f1 = __uint_as_float(MTG_TANH_LEAD[j]);
        const float f2 = __uint_as_float(MTG_TANH_TRAIL[j]);
        const int m = (ni - j) >> 5;
        const float rr = __fmaf_rn(-nf, trail, r1);
        const float poly = __fadd_rn(0.5f, __fmul_rn(rr, mtg_bits(0x3e2aaaabu)));  // 0.16666667
        const float q = __fadd_rn(r1, __fmaf_rn(__fmul_rn(rr, rr), poly, r2));
        const float z2 = __fmaf_rn(__fadd_rn(f1, f2), q, f2);
        const int m1 = m >> 1;
        const float t1 = __int_as_float((m1 + 127) << 23);
        const float t2 = __int_as_float(((m - m1) + 127) << 23);
        const float scaled = __fmul_rn(__fmul_rn(__fadd_rn(f1, z2), t1), t2);
        const float p = __fadd_rn(scaled, 1.0f);
        z = __fsub_rn(1.0f, __fdiv_rn(2.0f, p));
    }
    return __uint_as_float(__float_as_uint(z) | sign);
}

// Kernel-owned portable deterministic tanh (`deterministic_math_v1::tanh_f32_v1`),
// bit-identical on all 2^32 f32 inputs without calling any platform libm.
__device__ __forceinline__ double mtg_bits64(unsigned long long bits) {
    return __longlong_as_double((long long)bits);
}

__device__ float mtg_tanh_portable(float x) {
    const unsigned int ux = __float_as_uint(x);
    const unsigned int abs_bits = ux & 0x7fffffffu;
    const unsigned int sign = ux & 0x80000000u;
    if (abs_bits > 0x7f800000u) return __uint_as_float(0x7fc00000u);  // canonical quiet NaN
    if (abs_bits == 0x7f800000u) return __uint_as_float(0x3f800000u | sign);  // +/-inf
    if (abs_bits <= 0x39800000u) return x;  // |x| <= 2^-12 (inclusive): exact passthrough
    if (abs_bits >= 0x41102cb4u) return __uint_as_float(0x3f800000u | sign);  // saturation
    const double ax64 = (double)__uint_as_float(abs_bits);
    const double t = __dmul_rn(2.0, ax64);
    const double y = __dmul_rn(t, mtg_bits64(0x3ff71547652b82feULL));  // 1 / ln(2)
    const double tr = trunc(y);
    const double frac = __dsub_rn(y, tr);
    const double k_f = __dadd_rn(tr, frac >= 0.5 ? 1.0 : 0.0);
    const int k = (int)k_f;
    const double r = __dsub_rn(t, __dmul_rn((double)k, mtg_bits64(0x3fe62e42fefa39efULL)));
    double value = mtg_bits64(0x3f2a01a01a01a01aULL);  // 1/7!
    value = __dadd_rn(__dmul_rn(value, r), mtg_bits64(0x3f56c16c16c16c17ULL));  // 1/6!
    value = __dadd_rn(__dmul_rn(value, r), mtg_bits64(0x3f81111111111111ULL));  // 1/5!
    value = __dadd_rn(__dmul_rn(value, r), mtg_bits64(0x3fa5555555555555ULL));  // 1/4!
    value = __dadd_rn(__dmul_rn(value, r), mtg_bits64(0x3fc5555555555555ULL));  // 1/3!
    value = __dadd_rn(__dmul_rn(value, r), mtg_bits64(0x3fe0000000000000ULL));  // 1/2!
    const double eminus1_r = __dmul_rn(__dadd_rn(__dmul_rn(value, r), 1.0), r);
    const double scale = mtg_bits64(((unsigned long long)(k + 1023)) << 52);
    const double em1 = __dadd_rn(__dsub_rn(scale, 1.0), __dmul_rn(scale, eminus1_r));
    const double denom = __dadd_rn(em1, 2.0);
    double q = __ddiv_rn(em1, denom);
    if (q < 0.0) q = 0.0;
    if (q > 1.0) q = 1.0;
    const float result_abs = __double2float_rn(q);
    return __uint_as_float(__float_as_uint(result_abs) | sign);
}

__device__ __forceinline__ float mtg_tanh_mode(float x, int portable) {
    return portable ? mtg_tanh_portable(x) : mtg_tanh(x);
}
