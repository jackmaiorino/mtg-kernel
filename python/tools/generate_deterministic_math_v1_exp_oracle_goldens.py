"""Generate an independent, correctly-rounded oracle table for
`mtg-kernel/src/deterministic_math_v1.rs`'s `exp_f64_v1`.

Sibling script to `generate_deterministic_math_v1_oracle_goldens.py` (the
`tanh_f32_v1` oracle), added by the panel ruling (2026-08-16) that replaced
`model_guided_search_core_v1.rs`'s per-action sigmoid with a deterministic
softmax. `exp_f64_v1` is a private, domain-restricted primitive: valid only
for finite `x` in `[EXP_DOMAIN_FLOOR_V1, 0.0]` (`EXP_DOMAIN_FLOOR_V1 =
-120.0`), the exact domain `softmax_legal_action_weights_v1` clamps its
per-action `logit - max_logit` differences into before calling it. This
table therefore samples `x` as an f32 bit pattern in that closed range
(widened to f64, exactly, before evaluation, mirroring how the Rust caller
widens each `f32` logit difference), not an arbitrary f64 domain.

Each expected value is computed from `mpmath` at 50 decimal digits of
precision, then rounded to the nearest `f32` (round-to-nearest-even) by a
single, safe double-rounding through `f64` -- the identical technique and
identical safety argument as the tanh oracle script (`f64`'s own rounding
error, `~2^-52`, cannot flip an `f32`-level round-to-nearest decision,
which only needs to distinguish values to within `~2^-24`).

Requires `mpmath`. Deliberately not a project dependency (see the tanh
oracle script's own module doc for why); a one-off generator run by a human
during review.

Usage: `python python/tools/generate_deterministic_math_v1_exp_oracle_goldens.py`
Prints a Rust `const` array literal to stdout, ready to paste into
`deterministic_math_v1.rs`'s own oracle-comparison test. Deterministic: the
sampled bit patterns are all fixed, explicit constants below, not derived
from any random seed.
"""

from __future__ import annotations

import struct

import mpmath as mp

mp.mp.dps = 50

EXP_DOMAIN_FLOOR = -120.0


def f32_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", float(value)))[0]


def bits_to_f32(bits: int) -> float:
    return struct.unpack("<f", struct.pack("<I", bits & 0xFFFFFFFF))[0]


def correctly_rounded_exp_bits(input_bits: int) -> int:
    x = bits_to_f32(input_bits)
    if x != x:  # NaN: not part of this oracle table (out of exp_f64_v1's domain)
        raise ValueError("NaN inputs are not part of the oracle table")
    if not (EXP_DOMAIN_FLOOR <= x <= 0.0):
        raise ValueError(
            f"input {x} outside exp_f64_v1's documented domain "
            f"[{EXP_DOMAIN_FLOOR}, 0.0]"
        )
    high_precision = mp.e ** mp.mpf(x)
    return f32_bits(float(high_precision))  # safe double-round, see module doc


def build_table() -> list[int]:
    bits_set: set[int] = set()

    # 1. Exact boundary and its neighbors (floor, and one/two f32 ULPs
    #    toward zero from it).
    floor_bits = f32_bits(EXP_DOMAIN_FLOOR)
    for offset in range(0, 8):
        bits_set.add((floor_bits - offset) & 0xFFFFFFFF)  # more negative than floor: clamp's own job, not this table's
        bits_set.add((floor_bits + offset) & 0xFFFFFFFF)

    # 2. The zero-rounding boundary: ln(2^-150) ~= -103.97212, the exact
    #    IEEE-754 tie point between rounding to 0.0f32 and the smallest
    #    subnormal. Densely sampled on both sides.
    zero_boundary_bits = f32_bits(-103.97212)
    for offset in range(-48, 48):
        bits_set.add((zero_boundary_bits + offset) & 0xFFFFFFFF)

    # 3. The smallest-subnormal / smallest-normal boundary: ln(2^-126) ~=
    #    -87.336544.
    normal_boundary_bits = f32_bits(-87.336544)
    for offset in range(-32, 32):
        bits_set.add((normal_boundary_bits + offset) & 0xFFFFFFFF)

    # 4. Zero itself (the dominant action's own x), both signs.
    bits_set.add(f32_bits(0.0))
    bits_set.add(f32_bits(-0.0))

    # 5. Dense octave-by-octave interior coverage across the whole domain,
    #    many mantissa steps per octave, matching the tanh oracle's own
    #    interior-sampling density.
    for exponent in range(-10, 7):  # covers |x| from ~2^-10 up to ~2^6 = 64
        for mantissa_step in range(12):
            frac = 1.0 + mantissa_step * (1.0 / 12.0)
            magnitude = (2.0**exponent) * frac
            x = -magnitude
            if x < EXP_DOMAIN_FLOOR:
                continue
            bits_set.add(f32_bits(x))

    # 6. Named, human-legible reference points, including the coordinator's
    #    own worked-example gap (logit spread of 10.0) and round values
    #    spanning the domain.
    for x in [
        -0.5,
        -1.0,
        -2.0,
        -5.0,
        -10.0,  # coordinator's own worked example: max - 0 = 10.0
        -18.0218,  # 2 * TANH_SATURATION_THRESHOLD_V1, expm1_f64_v1's own domain edge
        -20.0,
        -50.0,
        -80.0,
        -87.336544,
        -100.0,
        -103.97212,
        -110.0,
        -119.0,
        -120.0,
    ]:
        if EXP_DOMAIN_FLOOR <= x <= 0.0:
            bits_set.add(f32_bits(x))

    # Filter to exactly the documented domain (some neighbor arithmetic
    # above can drift a hair outside it at the floor boundary; this table
    # is scoped to what exp_f64_v1 actually promises, not the clamp itself).
    in_domain = {
        b for b in bits_set if EXP_DOMAIN_FLOOR <= bits_to_f32(b) <= 0.0
    }
    return sorted(in_domain)


def main() -> None:
    table = build_table()
    pairs = [(b, correctly_rounded_exp_bits(b)) for b in table]

    print(f"// Generated by python/tools/generate_deterministic_math_v1_exp_oracle_goldens.py")
    print(f"// {len(pairs)} entries; do not hand-edit, regenerate instead.")
    print(f"const EXP_ORACLE_TABLE_V1: [(u32, u32); {len(pairs)}] = [")
    for input_bits, expected_bits in pairs:
        print(f"    (0x{input_bits:08x}, 0x{expected_bits:08x}),")
    print("];")


if __name__ == "__main__":
    main()
