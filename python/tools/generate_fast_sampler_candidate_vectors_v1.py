#!/usr/bin/env python3
"""Generate portable same-algorithm vectors for the fast categorical sampler.

This is a stdlib-only, integer/bit implementation of
f32-q8-expq63-hamilton-splitmix64-v1.  It deliberately does not invoke Rust,
NumPy, Torch, Decimal softmax, or the production Python-v3 sampler.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
from pathlib import Path
from typing import Any


SCHEMA = "mtg-kernel-fast-sampler-cross-language-vectors/v1"
SAMPLER_IDENTITY = "f32-q8-expq63-hamilton-splitmix64-v1"
SAMPLER_CONTRACT_SHA256 = (
    "276407494966b195b7c011caf984d2354484f7532161107b19ecc83388de92b6"
)
EXP_TABLE_SHA256 = "2cdd19abdec245d7a9f892e8757c299a282ae097361baecc46cfd6a57c476e2a"
EXP_BASE_Q63 = 9_187_413_517_043_429_148
EXP_TABLE_LEN = 4_097
MASS_TOTAL = 1 << 64
U64_MASK = MASS_TOTAL - 1
Q63_SCALE = 1 << 63
Q63_HALF = 1 << 62
Q8_DIVISOR_SHIFT = 141
Q8_CLAMP = 4_096
OUTPUT_RELATIVE = Path("data/fast_sampler_candidate_vectors_v1.json")
GENERATOR_RELATIVE = Path("python/tools/generate_fast_sampler_candidate_vectors_v1.py")

SEEDS = (
    0,
    1,
    2,
    3,
    (1 << 63) - 1,
    (1 << 64) - 1,
    0x0123_4567_89AB_CDEF,
)


class CandidateInputError(ValueError):
    def __init__(
        self,
        code: str,
        *,
        width: int,
        maximum: int | None = None,
        index: int | None = None,
        bits: int | None = None,
    ) -> None:
        super().__init__(code)
        self.code = code
        self.width = width
        self.maximum = maximum
        self.index = index
        self.bits = bits

    def record(self) -> dict[str, Any]:
        result: dict[str, Any] = {"code": self.code, "width": self.width}
        if self.maximum is not None:
            result["maximum"] = self.maximum
        if self.index is not None:
            result["index"] = self.index
        if self.bits is not None:
            result["bits_hex"] = f"{self.bits:08x}"
        return result


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_json_bytes(value: Any) -> bytes:
    return (
        json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode(
            "ascii"
        )
        + b"\n"
    )


def round_q63_product_ties_even(left: int, right: int) -> int:
    product = left * right
    quotient, remainder = divmod(product, Q63_SCALE)
    if remainder > Q63_HALF or (remainder == Q63_HALF and quotient & 1):
        quotient += 1
    if not 0 <= quotient <= U64_MASK:
        raise AssertionError("q63 recurrence overflow")
    return quotient


def build_exp_table() -> tuple[int, ...]:
    values = [Q63_SCALE]
    for _ in range(1, EXP_TABLE_LEN):
        values.append(round_q63_product_ties_even(values[-1], EXP_BASE_Q63))
    encoded = b"".join(value.to_bytes(8, "little") for value in values)
    if sha256_hex(encoded) != EXP_TABLE_SHA256:
        raise AssertionError("independent Q63 table digest mismatch")
    return tuple(values)


EXP_TABLE = build_exp_table()


def finite_order_key(bits: int) -> int:
    return bits ^ 0x8000_0000 if bits & 0x8000_0000 == 0 else (~bits) & 0xFFFF_FFFF


def magnitude_in_min_subnormal_units(bits: int) -> int:
    exponent = (bits >> 23) & 0xFF
    fraction = bits & 0x7F_FFFF
    if exponent == 0:
        significand = fraction
        shift = 0
    else:
        significand = fraction | (1 << 23)
        shift = exponent - 1
    return significand << shift


def quantized_gap_q8(maximum_bits: int, value_bits: int) -> int:
    maximum_magnitude = magnitude_in_min_subnormal_units(maximum_bits)
    value_magnitude = magnitude_in_min_subnormal_units(value_bits)
    maximum_negative = bool(maximum_bits & 0x8000_0000)
    value_negative = bool(value_bits & 0x8000_0000)
    if not maximum_negative and not value_negative:
        gap = maximum_magnitude - value_magnitude
    elif not maximum_negative and value_negative:
        gap = maximum_magnitude + value_magnitude
    elif maximum_negative and value_negative:
        gap = value_magnitude - maximum_magnitude
    else:
        raise AssertionError("maximum ordering contradicts signs")
    if gap < 0:
        raise AssertionError("negative exact gap")
    quotient, remainder = divmod(gap, 1 << Q8_DIVISOR_SHIFT)
    half = 1 << (Q8_DIVISOR_SHIFT - 1)
    if remainder > half or (remainder == half and quotient & 1):
        quotient += 1
    return min(quotient, Q8_CLAMP)


def candidate_masses(logit_bits: tuple[int, ...]) -> tuple[int, ...]:
    width = len(logit_bits)
    if width == 0:
        raise CandidateInputError("empty", width=width)
    if width > 64:
        raise CandidateInputError("width_exceeded", width=width, maximum=64)
    for index, bits in enumerate(logit_bits):
        if bits & 0x7F80_0000 == 0x7F80_0000:
            raise CandidateInputError("nonfinite", width=width, index=index, bits=bits)
    maximum_bits = max(logit_bits, key=finite_order_key)
    weights = [EXP_TABLE[quantized_gap_q8(maximum_bits, bits)] for bits in logit_bits]
    weight_total = sum(weights)
    masses: list[int] = []
    remainders: list[int] = []
    for weight in weights:
        quotient, remainder = divmod(weight * MASS_TOTAL, weight_total)
        masses.append(quotient)
        remainders.append(remainder)
    residual = MASS_TOTAL - sum(masses)
    if not 0 <= residual < len(masses):
        raise AssertionError("Hamilton residual outside width")
    order = sorted(range(len(masses)), key=lambda index: (-remainders[index], index))
    for index in order[:residual]:
        masses[index] += 1
    if sum(masses) != MASS_TOTAL:
        raise AssertionError("Hamilton mass is not exact")
    return tuple(masses)


def splitmix64_first(seed: int) -> int:
    mixed = (seed + 0x9E37_79B9_7F4A_7C15) & U64_MASK
    mixed = ((mixed ^ (mixed >> 30)) * 0xBF58_476D_1CE4_E5B9) & U64_MASK
    mixed = ((mixed ^ (mixed >> 27)) * 0x94D0_49BB_1331_11EB) & U64_MASK
    return (mixed ^ (mixed >> 31)) & U64_MASK


def select_mass(masses: tuple[int, ...], draw: int) -> int:
    cumulative = 0
    for index, mass in enumerate(masses):
        cumulative += mass
        if draw < cumulative:
            return index
    raise AssertionError("inverse CDF did not reach exact total")


def bits_from_exact_binary_fraction(numerator: int, denominator: int) -> int:
    value = numerator / denominator
    return int.from_bytes(struct.pack(">f", value), "big")


def maximum_width_bits() -> tuple[int, ...]:
    return tuple(
        bits_from_exact_binary_fraction(-((index * 37) % 4_097), 256)
        for index in range(64)
    )


CASE_BITS: tuple[tuple[str, tuple[int, ...]], ...] = (
    ("width-one", (0x7F7F_FFFF,)),
    ("width-two-ordered", (0x0000_0000, 0x3F80_0000)),
    ("hamilton-exact-remainder-tie", (0x0000_0000,) * 3),
    ("equal-tie-order", (0x0000_0000,) * 4),
    ("repeated-weight-legal-order", (0x0000_0000, 0xBF80_0000, 0x0000_0000, 0xBF80_0000)),
    ("q8-halfway-neighbors", (0x0000_0000, 0xBAFF_FF00, 0xBB00_0080, 0xBBBF_FF80, 0xBBC0_0080)),
    ("clamp-neighborhood", (0x0000_0000, 0xC17F_F800, 0xC180_0000, 0xC180_0400, 0xC188_0000)),
    ("finite-extremes", (0x7F7F_FFFF, 0xFF7F_FFFF, 0x0000_0000, 0xBF80_0000)),
    ("signed-zero-and-subnormal", (0x0000_0000, 0x8000_0000, 0x0000_0001, 0x8000_0001)),
    ("large-nearby-finite", (0x4B80_0000, 0x4B7F_FFFF, 0x4B7F_FFFE, 0x4B7F_FFF0)),
    ("maximum-admitted-width", maximum_width_bits()),
)

REJECTION_BITS: tuple[tuple[str, tuple[int, ...]], ...] = (
    ("empty-width", ()),
    ("width-65", (0x0000_0000,) * 65),
    ("positive-infinity", (0x0000_0000, 0x7F80_0000)),
    ("negative-infinity", (0x0000_0000, 0xFF80_0000)),
    ("quiet-nan-payload", (0x0000_0000, 0x7FC0_0001)),
)


def framed_vector_stream(
    cases: list[dict[str, Any]], rejections: list[dict[str, Any]]
) -> bytes:
    result = bytearray()
    domain = b"mtg-kernel-fast-sampler-cross-language-vectors-v1"
    result += len(domain).to_bytes(4, "big") + domain
    result += len(cases).to_bytes(4, "big")
    for case in cases:
        name = case["name"].encode("ascii")
        result += len(name).to_bytes(4, "big") + name
        result += len(case["logit_bits_hex"]).to_bytes(4, "big")
        for encoded in case["logit_bits_hex"]:
            result += int(encoded, 16).to_bytes(4, "big")
        for encoded in case["mass_u128"]:
            result += int(encoded).to_bytes(16, "big")
        result += len(case["draws"]).to_bytes(4, "big")
        for draw in case["draws"]:
            result += int(draw["seed_u64"]).to_bytes(8, "big")
            result += int(draw["splitmix_draw_hex"], 16).to_bytes(8, "big")
            result += int(draw["selected_index"]).to_bytes(4, "big")
    result += len(rejections).to_bytes(4, "big")
    for rejection in rejections:
        name = rejection["name"].encode("ascii")
        result += len(name).to_bytes(4, "big") + name
        result += len(rejection["logit_bits_hex"]).to_bytes(4, "big")
        for encoded in rejection["logit_bits_hex"]:
            result += int(encoded, 16).to_bytes(4, "big")
        error = rejection["expected_error"]
        code = error["code"].encode("ascii")
        result += len(code).to_bytes(4, "big") + code
        result += int(error.get("index", 0xFFFF_FFFF)).to_bytes(4, "big")
        result += int(error.get("bits_hex", "00000000"), 16).to_bytes(4, "big")
        result += int(error.get("maximum", 0)).to_bytes(4, "big")
    return bytes(result)


def payload(repository_root: Path) -> dict[str, Any]:
    cases: list[dict[str, Any]] = []
    for name, logit_bits in CASE_BITS:
        masses = candidate_masses(logit_bits)
        draws = []
        for seed in SEEDS:
            draw = splitmix64_first(seed)
            draws.append(
                {
                    "seed_u64": str(seed),
                    "splitmix_draw_hex": f"{draw:016x}",
                    "selected_index": select_mass(masses, draw),
                }
            )
        cases.append(
            {
                "name": name,
                "logit_bits_hex": [f"{bits:08x}" for bits in logit_bits],
                "mass_u128": [str(mass) for mass in masses],
                "draws": draws,
            }
        )
    rejections: list[dict[str, Any]] = []
    for name, logit_bits in REJECTION_BITS:
        try:
            candidate_masses(logit_bits)
        except CandidateInputError as error:
            expected_error = error.record()
        else:  # pragma: no cover - protects the generator contract itself
            raise AssertionError(f"negative vector {name} was unexpectedly admitted")
        rejections.append(
            {
                "name": name,
                "logit_bits_hex": [f"{bits:08x}" for bits in logit_bits],
                "expected_error": expected_error,
            }
        )
    generator_bytes = (repository_root / GENERATOR_RELATIVE).read_bytes()
    stream = framed_vector_stream(cases, rejections)
    return {
        "schema": SCHEMA,
        "sampler_identity": SAMPLER_IDENTITY,
        "sampler_contract_sha256": SAMPLER_CONTRACT_SHA256,
        "exp_table_sha256": EXP_TABLE_SHA256,
        "authority": {
            "implementation": "stdlib-only-independent-integer-bit-reference-v1",
            "generator": GENERATOR_RELATIVE.as_posix(),
            "generator_sha256": sha256_hex(generator_bytes),
            "forbidden_dependencies": ["rust-ffi", "numpy", "torch", "decimal-softmax"],
        },
        "stream_encoding": "u32be-domain-len||domain||u32be-case-count||per-case(name,width,u32be-logit-bits,u128be-masses,draw-count,u64be-seed,u64be-draw,u32be-index)||u32be-rejection-count||per-rejection(name,width,u32be-logit-bits,error-code,index-or-ffffffff,error-bits-or-zero,maximum-or-zero)",
        "vector_stream_sha256": sha256_hex(stream),
        "case_count": len(cases),
        "seed_count_per_case": len(SEEDS),
        "cases": cases,
        "rejection_count": len(rejections),
        "rejections": rejections,
        "nonclaims": [
            "not-decimal-softmax-hamilton-splitmix64-v1",
            "not-learning-noninferiority-evidence",
            "not-end-to-end-throughput-evidence",
            "not-an-xmage-speedup-claim",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    repository_root = Path(__file__).resolve().parents[2]
    output_path = repository_root / OUTPUT_RELATIVE
    expected = canonical_json_bytes(payload(repository_root))
    if args.check:
        if not output_path.is_file() or output_path.read_bytes() != expected:
            print("FAST_SAMPLER_CANDIDATE_VECTORS: STALE", file=sys.stderr)
            return 1
        print("FAST_SAMPLER_CANDIDATE_VECTORS: PASS")
        return 0
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_bytes(expected)
    print(f"wrote {output_path}")
    print(f"sha256={sha256_hex(expected)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
