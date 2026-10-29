"""EX1 design v3.3 section 3: the mirrored Python reference of the engine's `ex1-target-v1`
(mtg-kernel/src/ex1_v1.rs) and the label-blind placement and row-cap draws (sections 3 and 4).

Binary64 round-to-nearest-even, no fused multiply-add (CPython float arithmetic), with exp_v1 and
ln_v1 as mtg-kernel's unclamped_softmax_sampler_v1 and line_b_teacher_target_v1 define them.

`python tools/ex1/target_ref.py fixtures` writes the fixture file the engine test embeds
(mtg-kernel/src/ex1_v1/target_fixtures_v1.json); `check` compares it with this reference.
"""
import hashlib
import json
import struct
import sys
from pathlib import Path

VERSION = "ex1-target-v1"
INV_LN2 = struct.unpack('<d', struct.pack('<Q', 0x3ff71547652b82fe))[0]
LN2_HI = struct.unpack('<d', struct.pack('<Q', 0x3fe62e42fee00000))[0]
LN2_LO = struct.unpack('<d', struct.pack('<Q', 0x3dea39ef35793c76))[0]
SHIFTER = struct.unpack('<d', struct.pack('<Q', 0x4338000000000000))[0]
SQRT2 = struct.unpack('<d', struct.pack('<Q', 0x3ff6a09e667f3bcd))[0]
GAP_FLOOR = -708.0
TAYLOR = [1.0, 1.0] + [1.0 / f for f in (2.0, 6.0, 24.0, 120.0, 720.0, 5040.0, 40320.0, 362880.0, 3628800.0,
                                         39916800.0, 479001600.0, 6227020800.0)]
ATANH = [1.0 / (2 * k + 1) for k in range(12)]
SUPPORT_MIN = 8
INVERSE_TEMPERATURE = 4.0
MASS_FLOOR = 1e-6
RATES = {"target": 0.155, "cast_or_activate": 0.0311, "other": 0.0261}
FIXTURE_PATH = Path(__file__).resolve().parents[2] / "mtg-kernel" / "src" / "ex1_v1" / "target_fixtures_v1.json"


def bits64(x):
    return struct.unpack('<Q', struct.pack('<d', x))[0]


def f64(b):
    return struct.unpack('<d', struct.pack('<Q', b))[0]


def f32(b):
    return struct.unpack('<f', struct.pack('<I', b))[0]


def f32bits(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def exp_v1(d):
    k_float = (d * INV_LN2 + SHIFTER) - SHIFTER
    r = (d - k_float * LN2_HI) - k_float * LN2_LO
    value = TAYLOR[13]
    for c in reversed(TAYLOR[:13]):
        value = value * r + c
    k = int(k_float)
    return value * f64((k + 1023) << 52)


def ln_v1(x):
    """ln_normal_v1: any positive normal x."""
    b = bits64(x)
    exponent = ((b >> 52) & 0x7ff) - 1023
    mantissa = f64((b & ((1 << 52) - 1)) | (1023 << 52))
    if mantissa > SQRT2:
        mantissa *= 0.5
        exponent += 1
    t = (mantissa - 1.0) / (mantissa + 1.0)
    s = t * t
    series = ATANH[11]
    for c in reversed(ATANH[:11]):
        series = series * s + c
    e = float(exponent)
    return e * LN2_HI + (e * LN2_LO + 2.0 * (t * series))


def softmax(values):
    m = values[0]
    for v in values:
        if v > m:
            m = v
    gaps, weights, total = [], [], 0.0
    for v in values:
        g = v - m
        w = exp_v1(g) if g >= GAP_FLOOR else 0.0
        total += w
        gaps.append(g)
        weights.append(w)
    log_total = ln_v1(total)
    return [w / total for w in weights], [g - log_total for g in gaps], total


def qhat(n, wins, draws):
    return (float(wins) + 0.5 * float(draws)) / float(n)


def target(logit_bits, n, wins, draws):
    """The complete target vector, or ("no_target", reason)."""
    k = len(logit_bits)
    assert len(n) == len(wins) == len(draws) == k
    assert all(wins[a] + draws[a] <= n[a] for a in range(k))
    p, logp, _ = softmax([f32(b) for b in logit_bits])
    support = [a for a in range(k) if n[a] >= SUPPORT_MIN]
    if len(support) < 2:
        return ("no_target", "support")
    mass = 0.0
    for a in support:
        mass += p[a]
    if mass < MASS_FLOOR:
        return ("no_target", "mass")
    qh = [None] * k
    scaled = []
    for a in support:
        qh[a] = qhat(n[a], wins[a], draws[a])
        scaled.append(INVERSE_TEMPERATURE * qh[a])
    sigma, log_sigma, _ = softmax(scaled)
    log_mass = ln_v1(mass)
    q, logq = list(p), list(logp)
    for i, a in enumerate(support):
        q[a] = mass * sigma[i]
        logq[a] = log_mass + log_sigma[i]
    tv = 0.0
    for a in range(k):
        tv += abs(q[a] - p[a])
    return {"q": q, "log_q": logq, "p": p, "mass": mass, "support": support, "qhat": qh, "tv": 0.5 * tv}


def hash_u64(tag, run, update, seed, decision):
    text = json.dumps([tag, run, update, seed, decision], separators=(",", ":"))
    return int.from_bytes(hashlib.sha256(text.encode()).digest()[:8], "little")


def uniform(h):
    return (h >> 11) * (1.0 / float(1 << 53))


def placement(run, update, seed, decision, stratum):
    return uniform(hash_u64("ex1/placement", run, update, seed, decision)) < RATES[stratum]


def rowcap_key(run, update, seed, decision):
    return hash_u64("ex1/rowcap", run, update, seed, decision)


# name, logits (f32 values), n, wins, draws
CASES = [
    ("two_edges", [0.5, -1.0], [16, 16], [10, 4], [2, 0]),
    ("unsupported_kept", [0.5, -1.25, 2.0, 0.0, -3.5], [30, 2, 12, 9, 40], [21, 1, 3, 9, 10], [0, 1, 2, 0, 5]),
    ("draws_only", [0.0, 0.25, -0.25], [8, 8, 8], [0, 0, 0], [8, 4, 0]),
    ("equal_values", [1.0, -2.0, 0.0], [20, 9, 64000], [10, 4, 32000], [0, 1, 0]),
    ("self_target_line", [-6.0, 1.5, 0.75, -0.5], [9000, 40000, 12000, 3000], [8100, 9000, 3000, 600],
     [0, 400, 0, 30]),
    ("large_offset", [-9007199254740992.0, -9007199254740992.0, -9007199254740992.0], [8, 8, 1], [5, 6, 1],
     [0, 0, 0]),
    ("zero_offset", [0.0, 0.0, 0.0], [8, 8, 1], [5, 6, 1], [0, 0, 0]),
    ("cutoff_unsupported", [0.0, -710.0, -1.0, -2.0], [2, 3, 16, 16], [1, 3, 16, 0], [0, 0, 0, 0]),
    ("mass_just_above_floor", [0.0, -13.0, -13.5], [4, 16, 16], [2, 16, 0], [0, 0, 0]),
    ("mass_below_floor", [0.0, -20.0, -20.0], [4, 16, 16], [2, 16, 0], [0, 0, 0]),
    ("one_supported", [0.0, 1.0, 2.0], [100, 7, 0], [50, 7, 0], [0, 0, 0]),
    ("none_supported", [0.0, 1.0], [7, 7], [1, 2], [0, 0]),
]

HASH_CASES = [
    ("oh2-r2-s1-ex1-p1", 0, 1, 0),
    ("oh2-r2-s1-ex1-p1", 17, 18446744073709551615, 123456),
    ("r", 299, 9007199254740993, 7),
]


def fixtures():
    out = {"version": VERSION, "targets": [], "hashes": []}
    for name, logits, n, w, d in CASES:
        lb = [f32bits(x) for x in logits]
        row = {"name": name, "logit_bits": lb, "n": n, "wins": w, "draws": d}
        result = target(lb, n, w, d)
        if isinstance(result, tuple):
            row["no_target"] = result[1]
        else:
            row.update({"q_bits": [bits64(x) for x in result["q"]],
                        "log_q_bits": [bits64(x) for x in result["log_q"]],
                        "mass_bits": bits64(result["mass"]), "support": result["support"],
                        "qhat_bits": [None if x is None else bits64(x) for x in result["qhat"]],
                        "tv_bits": bits64(result["tv"])})
        out["targets"].append(row)
    for run, update, seed, decision in HASH_CASES:
        out["hashes"].append({"run": run, "update": update, "seed": seed, "decision": decision,
                              "placement": hash_u64("ex1/placement", run, update, seed, decision),
                              "rowcap": rowcap_key(run, update, seed, decision),
                              "uniform_bits": bits64(uniform(hash_u64("ex1/placement", run, update, seed,
                                                                      decision)))})
    return out


def render():
    return json.dumps(fixtures(), indent=1) + "\n"


if __name__ == "__main__":
    if sys.argv[1:] == ["fixtures"]:
        with open(FIXTURE_PATH, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(render())
    elif sys.argv[1:] == ["check"]:
        sys.exit(0 if FIXTURE_PATH.read_text(encoding="utf-8") == render() else 1)
    else:
        sys.exit("usage: target_ref.py fixtures|check")
