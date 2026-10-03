"""Independent binary64 replica of mtg-kernel/src/line_b_teacher_target_v1.rs.

Reproduces the teacher softmax, target and both divergences operation by
operation (Python floats are IEEE-754 binary64 with round-to-nearest-even and
no fused multiply-add). Every sum is an explicit left-to-right loop from +0.0,
because the built-in sum() compensates from Python 3.12 on. Prints the
golden-battery digest the Rust test pins; --oracle measures ln_v1 against a
60-digit decimal oracle and the platform log; --margins reports the observed
test quantities behind the Rust tolerances.

Usage: python line_b_teacher_target_v1_replica.py [--oracle] [--margins]
"""

import decimal
import hashlib
import math
import struct
import sys

from unclamped_softmax_sampler_v1_replica import (
    GAP_FLOOR,
    SplitMix64,
    exp_v1,
    f32,
    f64,
)

LN2_HI = f64(0x3FE62E42FEE00000)
LN2_LO = f64(0x3DEA39EF35793C76)
SQRT2 = f64(0x3FF6A09E667F3BCD)
ATANH_ODD = [1.0 / float(2 * k + 1) for k in range(12)]
TEMPERATURE = 0.25
ROLLOUTS = 16
U = 2.0 ** -53


def bits64(value):
    return struct.unpack("<Q", struct.pack("<d", value))[0]


def ln_v1(x):
    assert math.isfinite(x) and x >= 1.0
    bits = bits64(x)
    exponent = ((bits >> 52) & 0x7FF) - 1023
    mantissa = f64((bits & ((1 << 52) - 1)) | (1023 << 52))
    if mantissa > SQRT2:
        mantissa = mantissa * 0.5
        exponent += 1
    t = (mantissa - 1.0) / (mantissa + 1.0)
    s = t * t
    series = ATANH_ODD[11]
    for coefficient in reversed(ATANH_ODD[:11]):
        series = series * s + coefficient
    e = float(exponent)
    return e * LN2_HI + (e * LN2_LO + 2.0 * (t * series))


def softmax(values):
    """(log_probabilities, probabilities) with the sampler's tail rule."""
    maximum = values[0]
    for value in values:
        if value > maximum:
            maximum = value
    gaps, weights, total = [], [], 0.0
    for value in values:
        gap = value - maximum
        weight = exp_v1(gap) if gap >= GAP_FLOOR else 0.0
        total += weight
        gaps.append(gap)
        weights.append(weight)
    log_total = ln_v1(total)
    return [g - log_total for g in gaps], [w / total for w in weights]


def q_hat(returns):
    assert returns and all(r in (-1, 0, 1) for r in returns)
    total = 0
    for r in returns:
        total += r
    return float(total) / float(len(returns))


def target(logits, q_hats, temperature=TEMPERATURE):
    q_max = q_hats[0]
    for q in q_hats:
        if q > q_max:
            q_max = q
    shifted = []
    for z, q in zip(logits, q_hats):
        shift = (q - q_max) / temperature
        shifted.append(z if shift == 0.0 else z + shift)
    return softmax(shifted)


def reverse(student, goal):
    log_p, p = student
    log_q, _ = goal
    kl = 0.0
    for pi, lp, lq in zip(p, log_p, log_q):
        if pi > 0.0:
            kl += pi * (lp - lq)
    gradient = [pi * ((lp - lq) - kl) if pi > 0.0 else 0.0 for pi, lp, lq in zip(p, log_p, log_q)]
    return kl, gradient


def forward(student, goal):
    log_p, p = student
    log_q, q = goal
    kl = 0.0
    for qi, lq, lp in zip(q, log_q, log_p):
        if qi > 0.0:
            kl += qi * (lq - lp)
    return kl, [pi - qi for pi, qi in zip(p, q)]


def golden_battery():
    values = SplitMix64(0x6C696E652D622D71)
    cases = []
    for case in range(256):
        width = 2 + case % 7
        if case % 64 == 63:
            width = 300
        elif case % 32 == 31:
            width = 64
        logits = []
        for _ in range(width):
            v = values.next_u64()
            if v % 13 == 0:
                logits.append(-0.0)
            else:
                logits.append(f32(float(((v >> 11) % 4001) - 2000) / 250.0))
        if case % 16 == 9:
            logits[width - 1] = -800.0
        returns, shared = [], None
        for _ in range(width):
            if case % 16 == 5 and shared is not None:
                returns.append(list(shared))
                continue
            row = [int(values.next_u64() % 3) - 1 for _ in range(ROLLOUTS)]
            returns.append(row)
            shared = row
        if case % 8 == 0:
            student = list(logits)
        else:
            student = []
            for z in logits:
                v = values.next_u64()
                student.append(f32(z + float(int(v % 201) - 100) / 1000.0))
        cases.append((logits, returns, student))
    return cases


def golden_digest():
    digest = hashlib.sha256()
    for logits, returns, student in golden_battery():
        q_hats = [q_hat(row) for row in returns]
        goal = target(logits, q_hats)
        digest.update(struct.pack("<I", len(logits)))
        for z in logits:
            digest.update(struct.pack("<d", z))
        for row in returns:
            digest.update(struct.pack(f"<{len(row)}b", *row))
        for q in q_hats:
            digest.update(struct.pack("<d", q))
        for values in goal:
            for v in values:
                digest.update(struct.pack("<d", v))
        for z in student:
            digest.update(struct.pack("<d", z))
        student_softmax = softmax(student)
        for divergence in (reverse, forward):
            value, gradient = divergence(student_softmax, goal)
            digest.update(struct.pack("<d", value))
            for g in gradient:
                digest.update(struct.pack("<d", g))
    return digest.hexdigest()


def ln_grid():
    points = [1.0 + i / 4096 for i in range(4097)]
    points += [2.0 ** (i / 64) for i in range(64 * 16 + 1)]
    points += [float(n) for n in range(1, 65537, 97)]
    points += [SQRT2, math.nextafter(SQRT2, 0.0), math.nextafter(SQRT2, 2.0), 65536.0]
    return points


def oracle_report():
    decimal.getcontext().prec = 60
    u = decimal.Decimal(2) ** -53
    worst_rel, at, worst_ulp, ulp_at = decimal.Decimal(0), 1.0, 0, 1.0
    points = ln_grid()
    for x in points:
        got = ln_v1(x)
        exact = decimal.Decimal(x).ln()
        if exact != 0:
            rel = abs(decimal.Decimal(got) - exact) / abs(exact) / u
            if rel > worst_rel:
                worst_rel, at = rel, x
        distance = abs(bits64(got) - bits64(math.log(x)))
        if distance > worst_ulp:
            worst_ulp, ulp_at = distance, x
    print(f"ln_v1 over {len(points)} points: max relative error {float(worst_rel):.3f} u "
          f"at x={at!r}; max distance to platform log {worst_ulp} ulp at x={ulp_at!r}; "
          f"ln_v1(1) == 0: {ln_v1(1.0) == 0.0}")


def fourth_order(fn, z, index, h=1e-3):
    def at(delta):
        moved = list(z)
        moved[index] = z[index] + delta
        return fn(moved)
    return (-at(2 * h) + 8.0 * at(h) - 8.0 * at(-h) + at(-2 * h)) / (12.0 * h)


def margins_report():
    worst_a1, worst_sum_rev, worst_sum_fwd, worst_fd = 0.0, 0.0, 0.0, 0.0
    for logits, returns, student in golden_battery():
        q_hats = [q_hat(row) for row in returns]
        goal = target(logits, q_hats)
        s = softmax(student)
        for divergence, name in ((reverse, "rev"), (forward, "fwd")):
            _, gradient = divergence(s, goal)
            total = 0.0
            scale = 0.0
            for g in gradient:
                total += g
                scale += abs(g)
            if scale > 0.0:
                ratio = abs(total) / (U * scale)
                if name == "rev":
                    worst_sum_rev = max(worst_sum_rev, ratio)
                else:
                    worst_sum_fwd = max(worst_sum_fwd, ratio)
        if student == logits:
            _, p = s
            expected = 0.0
            for pi, q in zip(p, q_hats):
                expected += pi * q
            _, gradient = reverse(s, goal)
            for g, pi, q in zip(gradient, p, q_hats):
                identity = -(pi / TEMPERATURE) * (q - expected)
                worst_a1 = max(worst_a1, abs(g - identity))
        if len(logits) <= 8:
            for divergence in (reverse, forward):
                _, gradient = divergence(s, goal)
                for index in range(len(student)):
                    estimate = fourth_order(lambda z: divergence(softmax(z), goal)[0], student, index)
                    worst_fd = max(worst_fd, abs(estimate - gradient[index]))
    print(f"battery: A1 max abs {worst_a1:.3e}; gradient sum max |sum|/(u sum|g|): "
          f"reverse {worst_sum_rev:.3f}, forward {worst_sum_fwd:.3f}; fourth-order FD "
          f"(h=1e-3, widths 2..8) max {worst_fd:.3e}")
    p = [0.2, 0.3, 0.5]
    z = [math.log(v) for v in p]
    goal = target(z, [1.0, 0.0, -1.0])
    total = 0.0
    for q in goal[1]:
        total += q
    fd = 0.0
    for divergence in (reverse, forward):
        _, gradient = divergence(softmax(z), goal)
        for index in range(3):
            estimate = fourth_order(lambda v: divergence(softmax(v), goal)[0], z, index)
            fd = max(fd, abs(estimate - gradient[index]))
    same = target(z, [0.5, 0.5, 0.5])
    print(f"proposal case: |sum q - 1| = {abs(total - 1.0):.3e}; FD max {fd:.3e}; equal returns "
          f"max |q - p| = {max(abs(a - b) for a, b in zip(same[1], p)):.3e}")


if __name__ == "__main__":
    print(golden_digest())
    if "--oracle" in sys.argv:
        oracle_report()
    if "--margins" in sys.argv:
        margins_report()
