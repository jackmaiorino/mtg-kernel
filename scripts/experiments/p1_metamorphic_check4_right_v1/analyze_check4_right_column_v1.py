"""
analyze_check4_right_column_v1.py

P1-METAMORPHIC-AUDIT-DESIGN-V4.md Check 4 analysis: combines the existing
P0-first left column (D:\\mtg-kernel-post-rung-diagnostics-v1\\trace-audit\\
evaluations\\candidate-seed-{seed}-g512\\episodes.jsonl, UTF-16, one row per
leg) with the new P1-first right column
(D:\\mtg-kernel-check4-right-column-v1\\candidate-seed-{seed}-g512\\
episodes.jsonl, UTF-8, produced by run-one-eval.ps1 via H2H_OUTCOME_JSON)
into the frozen 2x2 factorial per V4 Section "Check 4", and runs the
corrected-formula, three-way-margin, fixed-N=256 confidence-sequence
analysis exactly as specified.

Per-model, per-cluster (environment root = pair_index) cell definitions
(V4): A=Y(P0,P0) [candidate seat P0, starting player P0 = left column,
learner_seat P0 leg], B=Y(P0,P1) [candidate seat P0, starting player P1 =
right column, learner_seat P0 leg], C=Y(P1,P0) [left column, learner_seat P1
leg], D=Y(P1,P1) [right column, learner_seat P1 leg].
Y = I(win) + 0.5*I(draw) = (reward+1)/2 for reward in {-1,0,1}.

Contrasts (V4, corrected INT formula from the V3->V4 algebra-error fix):
  OP  = 1/2 * [(A-B) + (D-C)]         range [-1,1]
  SEAT= 1/2 * [(C-A) + (D-B)]         range [-1,1]
  INT = (D-C) - (A-B)                 range [-2,2]  (NOT (D-C)-(B-A))

Primary estimand: average the per-model contrast across the three models
within each of the 256 clusters, THEN feed the 256 model-averaged,
affine-mapped-to-[0,1] sequences (OP', SEAT', INT') to the generic core
compute_eb_cs_trajectory_core(y_values, alpha, c) from eb_cs_reference_v1.py
(SEQUENTIAL-GATE-CONTRACT-DRAFT-V3.md + ERRATUM-1, P1 V4's countersigned
core/wrapper split), alpha=0.05/3 per stream (Bonferroni across the three
contrasts), c=0.5. Fixed-N=256: only the n=256 (last) point of the
trajectory is used; no interim looks are treated as a decision.

Three-way verdict rule (V4, epsilon=0.025):
  PRACTICALLY-NEGLIGIBLE: L > -eps and U < +eps
  MEANINGFULLY-NONZERO:   L > +eps or U < -eps
  UNRESOLVED:             otherwise
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, r"C:\Users\Jack\IdeaProjects\collab")
from eb_cs_reference_v1 import compute_eb_cs_trajectory_core, ALPHA_DEFAULT, C_TRUNCATION_DEFAULT  # noqa: E402

SEEDS = [970001, 970002, 970003]
LEFT_ROOT = Path(r"D:\mtg-kernel-post-rung-diagnostics-v1\trace-audit\evaluations")
RIGHT_ROOT = Path(r"D:\mtg-kernel-check4-right-column-v1")
OUT_DIR = RIGHT_ROOT / "analysis"
EPSILON = 0.025
ALPHA_FAMILY = 0.05
ALPHA_PER_STREAM = ALPHA_FAMILY / 3.0
C_TRUNCATION = 0.5
N_PAIRS = 256


def read_text_any_encoding(path: Path) -> str:
    """Decodes by explicit BOM sniffing, not blind try/except: an
    even-length UTF-8 byte string can "successfully" decode as UTF-16 into
    garbage without raising UnicodeError, so encoding must be determined
    from the actual byte-order-mark, not guessed by which decode call
    happens not to throw."""
    data = path.read_bytes()
    if data.startswith(b"\xff\xfe"):
        return data.decode("utf-16-le")
    if data.startswith(b"\xfe\xff"):
        return data.decode("utf-16-be")
    if data.startswith(b"\xef\xbb\xbf"):
        return data.decode("utf-8-sig")
    return data.decode("utf-8")


def load_episodes(path: Path) -> list[dict]:
    text = read_text_any_encoding(path)
    rows = []
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        rows.append(json.loads(line))
    return rows


def reward_to_y(reward: int) -> float:
    if reward not in (-1, 0, 1):
        raise ValueError(f"reward out of {{-1,0,1}}: {reward!r}")
    return (reward + 1.0) / 2.0


def build_seat_map(rows: list[dict]) -> tuple[dict[int, float], dict[int, int]]:
    """Returns (pair_index -> Y for that leg, pair_index -> environment_seed), per learner_seat, built by caller."""
    y_by_pair: dict[int, float] = {}
    seed_by_pair: dict[int, int] = {}
    for row in rows:
        p = int(row["pair_index"])
        y_by_pair[p] = reward_to_y(int(row["reward"]))
        seed_by_pair[p] = int(row["environment_seed"])
    return y_by_pair, seed_by_pair


def load_column(root: Path, seed: int) -> dict:
    path = root / f"candidate-seed-{seed}-g512" / "episodes.jsonl"
    rows = load_episodes(path)
    if len(rows) != 2 * N_PAIRS:
        raise ValueError(f"{path}: expected {2 * N_PAIRS} rows, found {len(rows)}")
    p0_rows = [r for r in rows if r["learner_seat"] == "P0"]
    p1_rows = [r for r in rows if r["learner_seat"] == "P1"]
    if len(p0_rows) != N_PAIRS or len(p1_rows) != N_PAIRS:
        raise ValueError(f"{path}: expected {N_PAIRS} P0 rows and {N_PAIRS} P1 rows, "
                          f"found {len(p0_rows)} P0 / {len(p1_rows)} P1")
    p0_y, p0_seed = build_seat_map(p0_rows)
    p1_y, p1_seed = build_seat_map(p1_rows)
    for d, label in ((p0_y, "P0 Y"), (p1_y, "P1 Y"), (p0_seed, "P0 seed"), (p1_seed, "P1 seed")):
        missing = set(range(N_PAIRS)) - set(d.keys())
        if missing:
            raise ValueError(f"{path}: {label} missing pair_index values: {sorted(missing)[:10]}...")
    return {
        "path": str(path),
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "p0_y": p0_y,
        "p1_y": p1_y,
        "p0_seed": p0_seed,
        "p1_seed": p1_seed,
    }


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    left = {seed: load_column(LEFT_ROOT, seed) for seed in SEEDS}
    right = {seed: load_column(RIGHT_ROOT, seed) for seed in SEEDS}

    # Cross-validate: every column/seat's environment_seed sequence must be
    # IDENTICAL across all six files at every pair_index (the seed-reuse
    # precondition Check 4 depends on -- V4's independently-reproduced
    # SHA-256 cross-check, extended here to also cover the new right column).
    reference_seed_seq = [left[SEEDS[0]]["p0_seed"][p] for p in range(N_PAIRS)]
    root_mismatches = []
    for seed in SEEDS:
        for col_name, col in (("left", left[seed]), ("right", right[seed])):
            for seat_key in ("p0_seed", "p1_seed"):
                seq = [col[seat_key][p] for p in range(N_PAIRS)]
                if seq != reference_seed_seq:
                    root_mismatches.append(f"{col_name} seed={seed} {seat_key} diverges from the reference root sequence")
    if root_mismatches:
        raise SystemExit("ROOT MISMATCH (fatal, would invalidate clustering):\n" + "\n".join(root_mismatches))
    root_sequence_sha256 = hashlib.sha256(
        ",".join(str(s) for s in reference_seed_seq).encode("utf-8")
    ).hexdigest()

    # Per-model, per-cluster cells and contrasts.
    per_model_series = {}   # seed -> {"OP": [...256...], "SEAT": [...], "INT": [...]}
    per_model_cells = {}    # seed -> list of dicts (for the JSON record / audit trail)
    for seed in SEEDS:
        A = [left[seed]["p0_y"][p] for p in range(N_PAIRS)]
        C = [left[seed]["p1_y"][p] for p in range(N_PAIRS)]
        B = [right[seed]["p0_y"][p] for p in range(N_PAIRS)]
        D = [right[seed]["p1_y"][p] for p in range(N_PAIRS)]
        op = [0.5 * ((A[p] - B[p]) + (D[p] - C[p])) for p in range(N_PAIRS)]
        seat = [0.5 * ((C[p] - A[p]) + (D[p] - B[p])) for p in range(N_PAIRS)]
        # Corrected V4 formula: INT = (D-C) - (A-B), NOT (D-C)-(B-A).
        interaction = [(D[p] - C[p]) - (A[p] - B[p]) for p in range(N_PAIRS)]
        per_model_series[seed] = {"OP": op, "SEAT": seat, "INT": interaction}
        per_model_cells[seed] = {"A_mean": sum(A) / N_PAIRS, "B_mean": sum(B) / N_PAIRS,
                                  "C_mean": sum(C) / N_PAIRS, "D_mean": sum(D) / N_PAIRS}

    # Standing regression guard from V4 Check 4 (the required hand fixture,
    # re-asserted here against the REAL data path, not just the unit test):
    # the corrected INT must not be identically 2*OP on real data either.
    for seed in SEEDS:
        op_arr = per_model_series[seed]["OP"]
        int_arr = per_model_series[seed]["INT"]
        identical_to_2op = all(abs(int_arr[p] - 2 * op_arr[p]) < 1e-12 for p in range(N_PAIRS))
        if identical_to_2op:
            raise SystemExit(f"REGRESSION: INT is identical to 2*OP for seed {seed} on real data "
                              f"(this would mean the V3 bug reappeared)")

    # Model-averaged, per-cluster contrasts (primary estimand).
    op_avg = [sum(per_model_series[seed]["OP"][p] for seed in SEEDS) / 3.0 for p in range(N_PAIRS)]
    seat_avg = [sum(per_model_series[seed]["SEAT"][p] for seed in SEEDS) / 3.0 for p in range(N_PAIRS)]
    int_avg = [sum(per_model_series[seed]["INT"][p] for seed in SEEDS) / 3.0 for p in range(N_PAIRS)]

    # Affine maps to [0,1] (V4): OP'=(OP+1)/2, SEAT'=(SEAT+1)/2, INT'=(INT+2)/4.
    def to_unit(values, lo, hi):
        out = []
        for v in values:
            u = (v - lo) / (hi - lo)
            if not (0.0 <= u <= 1.0):
                raise SystemExit(f"core/wrapper split input-quality check FAILED: mapped value {u} "
                                  f"out of [0,1] for raw value {v} with range [{lo},{hi}]")
            out.append(u)
        return out

    op_unit = to_unit(op_avg, -1.0, 1.0)
    seat_unit = to_unit(seat_avg, -1.0, 1.0)
    int_unit = to_unit(int_avg, -2.0, 2.0)

    def analyze_stream(name, unit_values, native_lo, native_hi):
        traj = compute_eb_cs_trajectory_core(unit_values, alpha=ALPHA_PER_STREAM, c=C_TRUNCATION)
        last = traj[-1]
        assert last.n == N_PAIRS
        native_scale = native_hi - native_lo
        # inverse affine: native = native_lo + u * native_scale  (matches
        # OP=2u-1 i.e. lo=-1,scale=2; INT=4u-2 i.e. lo=-2,scale=4)
        L = native_lo + last.cs_nu_lower * native_scale
        U = native_lo + last.cs_nu_upper * native_scale
        is_empty = last.is_empty_cs
        if is_empty:
            verdict = "UNRESOLVED (INVALID-EMPTY-CS: coverage-failure-type event, treat as UNRESOLVED per V4's no-branch-on-UNRESOLVED rule)"
        elif L > EPSILON or U < -EPSILON:
            verdict = "MEANINGFULLY-NONZERO"
        elif L > -EPSILON and U < EPSILON:
            verdict = "PRACTICALLY-NEGLIGIBLE"
        else:
            verdict = "UNRESOLVED"
        return {
            "name": name,
            "n": last.n,
            "alpha_per_stream": ALPHA_PER_STREAM,
            "c": C_TRUNCATION,
            "epsilon": EPSILON,
            "point_estimate_running_mean_native": native_lo + last.y_hat_running * native_scale,
            "cs_native_lower": L,
            "cs_native_upper": U,
            "is_empty_cs": is_empty,
            "verdict": verdict,
        }

    results = {
        "OP": analyze_stream("OP", op_unit, -1.0, 1.0),
        "SEAT": analyze_stream("SEAT", seat_unit, -1.0, 1.0),
        "INT": analyze_stream("INT", int_unit, -2.0, 2.0),
    }

    # Per-model descriptive (secondary, not decision-driving) point estimates.
    per_model_summary = {}
    for seed in SEEDS:
        s = per_model_series[seed]
        per_model_summary[seed] = {
            "OP_mean": sum(s["OP"]) / N_PAIRS,
            "SEAT_mean": sum(s["SEAT"]) / N_PAIRS,
            "INT_mean": sum(s["INT"]) / N_PAIRS,
            "cells_mean": per_model_cells[seed],
        }

    # Raw A/B/C/D counts per model (win counts out of 256), for the report.
    raw_counts = {}
    for seed in SEEDS:
        A = [left[seed]["p0_y"][p] for p in range(N_PAIRS)]
        C = [left[seed]["p1_y"][p] for p in range(N_PAIRS)]
        B = [right[seed]["p0_y"][p] for p in range(N_PAIRS)]
        D = [right[seed]["p1_y"][p] for p in range(N_PAIRS)]
        raw_counts[seed] = {
            "A_candidateP0_startP0_wins": sum(1 for y in A if y == 1.0),
            "A_draws": sum(1 for y in A if y == 0.5),
            "A_losses": sum(1 for y in A if y == 0.0),
            "B_candidateP0_startP1_wins": sum(1 for y in B if y == 1.0),
            "B_draws": sum(1 for y in B if y == 0.5),
            "B_losses": sum(1 for y in B if y == 0.0),
            "C_candidateP1_startP0_wins": sum(1 for y in C if y == 1.0),
            "C_draws": sum(1 for y in C if y == 0.5),
            "C_losses": sum(1 for y in C if y == 0.0),
            "D_candidateP1_startP1_wins": sum(1 for y in D if y == 1.0),
            "D_draws": sum(1 for y in D if y == 0.5),
            "D_losses": sum(1 for y in D if y == 0.0),
        }

    output = {
        "schema": "p1-metamorphic-check4-right-column-analysis/v1",
        "design_doc": "P1-METAMORPHIC-AUDIT-DESIGN-V4.md, Check 4",
        "method_citation": "SEQUENTIAL-GATE-CONTRACT-DRAFT-V3.md + SEQUENTIAL-GATE-CONTRACT-V3-ERRATUM-1.md, "
                            "compute_eb_cs_trajectory_core (eb_cs_reference_v1.py, collab root)",
        "n_clusters": N_PAIRS,
        "n_models": len(SEEDS),
        "alpha_family": ALPHA_FAMILY,
        "alpha_per_stream": ALPHA_PER_STREAM,
        "c_truncation": C_TRUNCATION,
        "epsilon_margin": EPSILON,
        "reveal_order": "ascending pair_index, fixed-N=256, single look, no early stopping",
        "root_sequence_sha256": root_sequence_sha256,
        "left_column_sources": {seed: {"path": left[seed]["path"], "sha256": left[seed]["sha256"]} for seed in SEEDS},
        "right_column_sources": {seed: {"path": right[seed]["path"], "sha256": right[seed]["sha256"]} for seed in SEEDS},
        "raw_counts_per_model": raw_counts,
        "per_model_descriptive_secondary": per_model_summary,
        "primary_results_model_averaged": results,
    }

    (OUT_DIR / "check4-right-column-analysis-v1.json").write_text(
        json.dumps(output, indent=2, sort_keys=False), encoding="utf-8"
    )

    lines = []
    lines.append("P1-METAMORPHIC-AUDIT-DESIGN-V4.md Check 4: right-column analysis")
    lines.append("=" * 70)
    lines.append(f"clusters={N_PAIRS} models={len(SEEDS)} alpha_family={ALPHA_FAMILY} "
                 f"alpha_per_stream={ALPHA_PER_STREAM:.6f} c={C_TRUNCATION} epsilon={EPSILON}")
    lines.append(f"root_sequence_sha256={root_sequence_sha256}")
    lines.append("")
    for key in ("OP", "SEAT", "INT"):
        r = results[key]
        lines.append(f"{key}: point={r['point_estimate_running_mean_native']:.6f} "
                      f"CS=[{r['cs_native_lower']:.6f}, {r['cs_native_upper']:.6f}] "
                      f"is_empty_cs={r['is_empty_cs']} -> {r['verdict']}")
    lines.append("")
    lines.append("Per-model descriptive (secondary):")
    for seed in SEEDS:
        s = per_model_summary[seed]
        lines.append(f"  seed {seed}: OP_mean={s['OP_mean']:.6f} SEAT_mean={s['SEAT_mean']:.6f} "
                      f"INT_mean={s['INT_mean']:.6f}")
    lines.append("")
    lines.append("Raw A/B/C/D win counts (of 256) per model:")
    for seed in SEEDS:
        c = raw_counts[seed]
        lines.append(f"  seed {seed}: A(P0,startP0)={c['A_candidateP0_startP0_wins']} "
                      f"B(P0,startP1)={c['B_candidateP0_startP1_wins']} "
                      f"C(P1,startP0)={c['C_candidateP1_startP0_wins']} "
                      f"D(P1,startP1)={c['D_candidateP1_startP1_wins']}")
    report_text = "\n".join(lines) + "\n"
    (OUT_DIR / "check4-right-column-analysis-v1.txt").write_text(report_text, encoding="utf-8")
    print(report_text)


if __name__ == "__main__":
    main()
