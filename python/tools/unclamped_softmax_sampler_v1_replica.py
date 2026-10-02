"""Independent binary64 replica of unclamped-softmax-f64-icdf-u53-v1.

Reproduces mtg-kernel/src/unclamped_softmax_sampler_v1.rs operation by
operation (Python floats are IEEE-754 binary64 with round-to-nearest-even and
no fused multiply-add). Prints the golden-battery digest the Rust test pins,
an exp_v1 error measurement against a 60-digit decimal oracle, and the
exact-grid envelope report for the Rust test's fixed vectors.

Usage: python unclamped_softmax_sampler_v1_replica.py [--oracle] [--envelope]
"""

import bisect
import decimal
import hashlib
import math
import struct
import sys

MASK64 = (1 << 64) - 1
GOLDEN = 0x9E3779B97F4A7C15
GRID = 1 << 53
TWO_POW_NEG_53 = 2.0 ** -53
GAP_FLOOR = -708.0
MAX_ACTIONS = 65_536


def f64(bits):
    return struct.unpack("<d", struct.pack("<Q", bits))[0]


def f32(value):
    return struct.unpack("<f", struct.pack("<f", value))[0]


def f32_bits(value):
    return struct.unpack("<I", struct.pack("<f", value))[0]


INV_LN2 = f64(0x3FF71547652B82FE)
LN2_HI = f64(0x3FE62E42FEE00000)
LN2_LO = f64(0x3DEA39EF35793C76)
SHIFTER = f64(0x4338000000000000)
TAYLOR = [1.0 / float(math.factorial(j)) for j in range(14)]


class SplitMix64:
    """crate::state::SplitMix64: state += golden; return mix(state)."""

    def __init__(self, seed):
        self.state = seed & MASK64

    def next_u64(self):
        self.state = (self.state + GOLDEN) & MASK64
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK64
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK64
        return z ^ (z >> 31)


def exp_v1(d):
    k_float = (d * INV_LN2 + SHIFTER) - SHIFTER
    r = (d - k_float * LN2_HI) - k_float * LN2_LO
    value = TAYLOR[13]
    for coefficient in reversed(TAYLOR[:13]):
        value = value * r + coefficient
    return value * f64((int(k_float) + 1023) << 52)


def prefix_sums(logits):
    assert 0 < len(logits) <= MAX_ACTIONS
    maximum = logits[0]
    for logit in logits:
        assert math.isfinite(logit)
        if logit > maximum:
            maximum = logit
    cumulative = 0.0
    prefix = []
    for logit in logits:
        gap = logit - maximum
        cumulative += exp_v1(gap) if gap >= GAP_FLOOR else 0.0
        prefix.append(cumulative)
    return prefix


def select(prefix, u):
    index = bisect.bisect_right(prefix, u * prefix[-1])
    assert index < len(prefix)
    return index


def sample(logits, draw):
    return select(prefix_sums(logits), (draw >> 11) * TWO_POW_NEG_53)


def golden_battery():
    values = SplitMix64(0x6C696E652D622D76)
    seat = SplitMix64(0x736561742D737472)
    cases = []
    for case in range(512):
        other = case % 16
        width = {13: 64, 14: 65, 15: 300}.get(other, 1 + other % 12)
        logits = []
        for _ in range(width):
            value = values.next_u64()
            if value % 7 == 0:
                logits.append(-1.25)
            else:
                logits.append(f32(-f32(float((value >> 8) % 4001)) / 100.0))
        cases.append((logits, seat.next_u64()))
    return cases


def golden_digest():
    digest = hashlib.sha256()
    for logits, draw in golden_battery():
        selected = sample(logits, draw)
        digest.update(struct.pack("<I", len(logits)))
        for logit in logits:
            digest.update(struct.pack("<I", f32_bits(logit)))
        digest.update(struct.pack("<Q", draw))
        digest.update(struct.pack("<I", selected))
    return digest.hexdigest()


def oracle_report():
    decimal.getcontext().prec = 60
    ln2 = decimal.Decimal(2).ln()
    split_error = abs(decimal.Decimal(LN2_HI) + decimal.Decimal(LN2_LO) - ln2)
    unit = decimal.Decimal(2) ** -53
    points = [GAP_FLOOR * step / 200_000 for step in range(200_001)]
    points += [-f64(bits) for bits in (0, 1, 0x3CB0000000000000, 0x3FD62E42FEFA39EF, 0x3FE62E42FEFA39EF)]
    # Reduction boundaries: gaps where k changes, and their float neighbours.
    for k in range(1, 1022):
        edge = -(k - 0.5) * math.log(2)
        if edge >= GAP_FLOOR:
            points += [edge, math.nextafter(edge, 0.0), math.nextafter(edge, -math.inf)]
    worst, worst_at = decimal.Decimal(0), 0.0
    for d in points:
        exact = decimal.Decimal(d).exp()
        error = abs(decimal.Decimal(exp_v1(d)) - exact) / exact / unit
        if error > worst:
            worst, worst_at = error, d
    print(f"ln2 split error |hi+lo-ln2| = {float(split_error):.3e}")
    print(f"exp_v1 max relative error over {len(points)} points: {float(worst):.3f} * 2**-53 at d={worst_at!r}")


def exact_grid_counts(logits):
    prefix = prefix_sums(logits)
    counts, start = [], 0
    for action in range(len(logits)):
        low, high = start, GRID
        while low < high:
            middle = low + (high - low) // 2
            if select(prefix, middle * TWO_POW_NEG_53) > action:
                high = middle
            else:
                low = middle + 1
        counts.append(low - start)
        start = low
    assert start == GRID
    return counts


def envelope_vectors():
    vectors = [
        [0.0, -3.0],
        [0.0, -16.5],
        [0.0, -19.37, -3.0, -25.0],
        [0.0, -36.0, -40.0, -1.0, 0.0],
        [5.0, 5.0 - 700.0, 5.0 - 710.0],
        [0.0, -1.0, -2.0, -4.0, -8.0, -16.0, -17.0, -19.37],
        [0.0, -0.0],
        [3.0e38, -3.0e38],
        [1.0e-30, -1.0e-30, 0.0],
        [-50.0, -50.0, -50.0],
        [12.5],
        [f32(-0.5 * i) for i in range(64)],
        [f32(f32(-0.4) * float(i)) for i in range(65)],
        [f32(f32(-0.37) * float(i % 97)) for i in range(5_040)],
    ]
    return [[f32(v) for v in vector] for vector in vectors]


def envelope_report():
    for logits in envelope_vectors():
        counts = exact_grid_counts(logits)
        maximum = max(logits)
        weights = [math.exp(z - maximum) for z in logits]
        total = math.fsum(weights)
        errors = [abs(c * TWO_POW_NEG_53 - w / total) for c, w in zip(counts, weights)]
        bound = (4 * len(logits) + 16) * TWO_POW_NEG_53
        print(f"width {len(logits):>5}: max |P-p| {max(errors):.3e}  bound {bound:.3e}")


if __name__ == "__main__":
    print("golden battery sha256", golden_digest())
    if "--oracle" in sys.argv:
        oracle_report()
    if "--envelope" in sys.argv:
        envelope_report()
