#!/usr/bin/env python3
"""Outcome-blind analyzer for CLAUDE-MIRROR-SEAT-DIAGNOSTIC-V1.

Reads a panel-manifest.json produced by run-mirror-panel.ps1 (all arms
already completed; nothing here is invoked until every arm's completion
record has validated as successful -- see run-mirror-panel.ps1's comment
"Outcome-blind gate"). For each arm it validates:

  * the outcome JSON's schema, pair/episode counts, and CRN row shape
    (candidate == opponent identity fields, on every row);
  * every row's environment_seed against an independent re-derivation of
    trainer_environment_seed(evaluation_seed, pair_index), the same u63
    derivation response_exploiter_checkpoint_diagnostic_analyzer.py uses
    (kernel-python-rl-trainer-sha256-v2 / "train-env" namespace) -- this is
    the CRN-discipline check: both legs of a pair must share one seed;
  * that both legs of a pair share one environment_seed and use opposite
    seats.

It then reports, per arm:
  * P0-perspective win rate over pairs (wins_as_P0 + 0.5*draws_as_P0) / pairs,
    with binomial SE;
  * the paired seat-advantage statistic: for each pair, outcome_as_P0 minus
    outcome_as_P1 (each in {-1, 0, +1}), summarized as P0-better / P1-better
    / tie counts, net = P0-better - P1-better, and exact SE = sqrt(discordant)
    (McNemar, matching the convention already used in
    CLAUDE-SEAT-DECOMPOSITION-V1.md and TO-CODEX #204);
  * discordant pair count and pair-level tie rate;
  * the decomposition's own already-published pooled per-slot P0-minus-P1
    control rate for the same slot identity, cited verbatim for comparison
    (not recomputed -- see CLAUDE-SEAT-DECOMPOSITION-V1.md Section 1);
  * a two-sided normal-approximation z-test of the arm's own paired net
    against zero (McNemar SE, sqrt(discordant)), of the arm against its
    decomposition-slot reference (combined SE), and of the promoted(2)
    mirror arm against the decomposition's marginal pooled-mixture P0-minus-P1
    tilt (+0.0313, combined SE 0.0181, Section 1 "Marginal by seat, pooled
    across all six slots"); plus every pairwise between-arm comparison of
    the three arms' paired net rates (independent two-sample z-test,
    combined SE from each arm's own McNemar SE, since the three arms share
    no CRN seed and are statistically independent of one another).

All significance figures are two-sided normal-approximation z-tests
(p = erfc(|z|/sqrt(2))); no continuity correction, no multiple-comparison
adjustment. Descriptive only. No threshold, no alpha, no promotion or
selection logic; nothing here changes which arms ran or how they were read.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
from typing import Any

OUTCOME_SCHEMA = "mtg-kernel-head-to-head-terminal-stream/v1"
COMPLETION_SCHEMA = "mtg-kernel-mirror-seat-diagnostic-arm-completion/v1"
PANEL_SCHEMA = "mtg-kernel-mirror-seat-diagnostic-execution/v1"

PYTHON_REFERENCE_SEED_VERSION = "kernel-python-rl-trainer-sha256-v2"
TRAIN_ENV_NAMESPACE = "train-env"
U63_MAX = (1 << 63) - 1

# CLAUDE-SEAT-DECOMPOSITION-V1.md Section 1, "Pooled control rate by slot x
# seat (diagnostic + campaign)". Cited verbatim, not recomputed. SE_diff is
# sqrt(se_p0^2 + se_p1^2) as used throughout that report.
DECOMPOSITION_REFERENCE = {
    0: {"p0_rate": 0.5130, "p0_se": 0.0305, "p1_rate": 0.4610, "p1_se": 0.0304, "diff": 0.0520},
    3: {"p0_rate": 0.5296, "p0_se": 0.0304, "p1_rate": 0.4741, "p1_se": 0.0304, "diff": 0.0555},
    5: {"p0_rate": 0.4943, "p0_se": 0.0309, "p1_rate": 0.4598, "p1_se": 0.0308, "diff": 0.0345},
}

# CLAUDE-SEAT-DECOMPOSITION-V1.md Section 1, "Marginal by seat, pooled
# across all six slots", "Pooled control" row: P0 0.5098 (SE 0.0128), P1
# 0.4785 (SE 0.0128), diff +0.0313, combined SE ~0.0181. This is
# promoted(2)'s own seat gap against the FULL SIX-COMPONENT MIXTURE (not a
# single slot); it is the "+3.1pp pooled tilt" the mirror panel exists to
# decompose. Cited verbatim, not recomputed.
MIXTURE_MARGINAL_REFERENCE = {"diff": 0.0313, "se": 0.0181}
MIXTURE_MARGINAL_ARM_LABEL = "mirror-promoted2-slot0"


def two_sided_normal_p(z: float) -> float:
    return math.erfc(abs(z) / math.sqrt(2.0))


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def load_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    require(type(value) is dict, f"{path} must contain an object")
    return value


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def record(value: Any, label: str) -> Path:
    require(type(value) is dict and set(value) == {"path", "bytes", "sha256"}, f"{label} record schema mismatch")
    path = Path(value["path"]).resolve()
    require(path.is_file(), f"{label} is missing")
    require(path.stat().st_size == value["bytes"], f"{label} byte count changed")
    require(sha256(path) == value["sha256"], f"{label} SHA-256 changed")
    return path


def append_atom(hasher: Any, tag: str, payload: bytes) -> None:
    tag_bytes = tag.encode("utf-8")
    hasher.update(len(tag_bytes).to_bytes(4, "big"))
    hasher.update(tag_bytes)
    hasher.update(len(payload).to_bytes(8, "big"))
    hasher.update(payload)


def derive_u63_seed(version: str, namespace: str, fields: list[tuple[str, int]]) -> int:
    hasher = hashlib.sha256()
    append_atom(hasher, "version", version.encode("utf-8"))
    append_atom(hasher, "namespace", namespace.encode("utf-8"))
    for name, value in fields:
        require(value <= U63_MAX, f"{name} exceeds u63")
        append_atom(hasher, "field-name", name.encode("utf-8"))
        append_atom(hasher, "u63", value.to_bytes(8, "big"))
    return int.from_bytes(hasher.digest()[:8], "big") & U63_MAX


def trainer_environment_seed(base_seed: int, pair_index: int) -> int:
    return derive_u63_seed(PYTHON_REFERENCE_SEED_VERSION, TRAIN_ENV_NAMESPACE, [("base_seed", base_seed), ("pair_index", pair_index)])


def binomial_se(rate: float, n: int) -> float:
    return math.sqrt(rate * (1.0 - rate) / n)


def analyze_arm(outcome_path: Path, expected_pairs: int, decomposition_slot: int | None) -> dict[str, Any]:
    outcome = load_json(outcome_path)
    require(outcome.get("schema") == OUTCOME_SCHEMA, f"{outcome_path} schema mismatch")
    seed = outcome["evaluation_base_seed"]
    pairs = outcome["pair_count"]
    require(pairs == expected_pairs, f"{outcome_path} pair count {pairs} != expected {expected_pairs}")
    require(outcome["episode_count"] == pairs * 2, f"{outcome_path} episode count mismatch")

    candidate = outcome["candidate"]
    opponent = outcome["opponent"]
    mirror_keys = ("run_sha256", "generation", "checkpoint_manifest_sha256", "checkpoint_payload_sha256")
    for key in mirror_keys:
        require(candidate[key] == opponent[key], f"{outcome_path}: mirror binding broken at key {key} (candidate != opponent)")

    rows = outcome["episodes"]
    require(len(rows) == pairs * 2, f"{outcome_path} row count mismatch")

    by_pair: dict[int, dict[str, dict[str, Any]]] = {}
    for index, row in enumerate(rows):
        pair, leg = divmod(index, 2)
        seat = "P0" if leg == 0 else "P1"
        require(row["episode_index"] == index, f"{outcome_path} row {index} episode_index mismatch")
        require(row["pair_index"] == pair, f"{outcome_path} row {index} pair_index mismatch")
        require(row["learner_seat"] == seat, f"{outcome_path} row {index} seat-order mismatch (rows must alternate P0,P1 per pair)")
        expected_seed = trainer_environment_seed(seed, pair)
        require(row["environment_seed"] == expected_seed, f"{outcome_path} pair {pair}: environment_seed does not match independent re-derivation (CRN discipline check failed)")
        by_pair.setdefault(pair, {})[seat] = row

    require(len(by_pair) == pairs, f"{outcome_path} did not produce exactly {pairs} pairs")

    p0_wins = p0_losses = p0_draws = 0
    p1_wins = p1_losses = p1_draws = 0
    p0_better = p1_better = tie = 0
    pair_diffs = []
    for pair in range(pairs):
        legs = by_pair[pair]
        require(set(legs) == {"P0", "P1"}, f"{outcome_path} pair {pair} missing a seat leg")
        require(legs["P0"]["environment_seed"] == legs["P1"]["environment_seed"], f"{outcome_path} pair {pair}: legs do not share one environment_seed (CRN discipline check failed)")
        p0_rank = legs["P0"]["terminal_order_rank"]
        p1_rank = legs["P1"]["terminal_order_rank"]
        require(p0_rank in (-1, 0, 1) and p1_rank in (-1, 0, 1), f"{outcome_path} pair {pair}: rank out of range")
        if p0_rank == 1: p0_wins += 1
        elif p0_rank == -1: p0_losses += 1
        else: p0_draws += 1
        if p1_rank == 1: p1_wins += 1
        elif p1_rank == -1: p1_losses += 1
        else: p1_draws += 1
        diff = p0_rank - p1_rank
        pair_diffs.append(diff)
        if diff > 0: p0_better += 1
        elif diff < 0: p1_better += 1
        else: tie += 1

    require(p0_wins + p0_losses + p0_draws == pairs, f"{outcome_path} P0 outcome count mismatch")
    require(p1_wins + p1_losses + p1_draws == pairs, f"{outcome_path} P1 outcome count mismatch")

    p0_rate = (p0_wins + 0.5 * p0_draws) / pairs
    p1_rate = (p1_wins + 0.5 * p1_draws) / pairs
    discordant = p0_better + p1_better
    net = p0_better - p1_better
    se_discordant_count = math.sqrt(discordant) if discordant > 0 else 0.0
    se_net_rate = (se_discordant_count / pairs) if pairs > 0 else 0.0
    net_rate = net / pairs

    # Paired seat-advantage net vs zero (McNemar normal approximation): the
    # arm's own two-sided significance of its P0-minus-P1 deviation.
    z_vs_zero = (net / se_discordant_count) if discordant > 0 else None
    p_vs_zero = two_sided_normal_p(z_vs_zero) if z_vs_zero is not None else None

    # Simple one-sample check (secondary, weaker): P0-perspective rate alone
    # vs the fixed value 0.5, ignoring the P1 leg and the CRN pairing that
    # the McNemar test above exploits. Reported because it is the literal
    # "P0 win rate vs 50%" reading; the McNemar test is the primary,
    # higher-power statistic this sheet's design calls for.
    se_binom_half = binomial_se(0.5, pairs)
    z_p0_vs_half = (p0_rate - 0.5) / se_binom_half
    p_p0_vs_half = two_sided_normal_p(z_p0_vs_half)

    reference = DECOMPOSITION_REFERENCE.get(decomposition_slot) if decomposition_slot is not None else None
    comparison = None
    if reference is not None:
        decomposition_se = math.sqrt(reference["p0_se"] ** 2 + reference["p1_se"] ** 2)
        combined_se_vs_decomp = math.sqrt(se_net_rate ** 2 + decomposition_se ** 2)
        z_vs_decomp = (net_rate - reference["diff"]) / combined_se_vs_decomp
        comparison = {
            "decomposition_slot": decomposition_slot,
            "decomposition_pooled_control_p0_minus_p1": reference["diff"],
            "decomposition_combined_se": round(decomposition_se, 4),
            "mirror_p0_minus_p1_rate": round(net_rate, 4),
            "mirror_combined_se": round(se_net_rate, 4),
            "difference_of_differences": round(net_rate - reference["diff"], 4),
            "combined_se_of_difference": round(combined_se_vs_decomp, 4),
            "z_vs_decomposition": round(z_vs_decomp, 4),
            "p_vs_decomposition_two_sided": round(two_sided_normal_p(z_vs_decomp), 4),
        }

    return {
        "evaluation_seed": seed,
        "pairs": pairs,
        "candidate_identity": {key: candidate[key] for key in mirror_keys},
        "p0_perspective": {
            "wins": p0_wins, "losses": p0_losses, "draws": p0_draws, "rate": round(p0_rate, 4), "se": round(binomial_se(p0_rate, pairs), 4),
            "z_vs_50_percent": round(z_p0_vs_half, 4), "p_vs_50_percent_two_sided": round(p_p0_vs_half, 4),
        },
        "p1_perspective": {"wins": p1_wins, "losses": p1_losses, "draws": p1_draws, "rate": round(p1_rate, 4), "se": round(binomial_se(p1_rate, pairs), 4)},
        "paired_seat_advantage": {
            "p0_better": p0_better, "p1_better": p1_better, "tie": tie,
            "net": net, "discordant": discordant, "net_rate": round(net_rate, 4),
            "se_net_count": round(se_discordant_count, 4),
            "se_net_rate": round(se_net_rate, 4),
            "net_ratio_to_se": (round(z_vs_zero, 4) if z_vs_zero is not None else None),
            "z_vs_zero": (round(z_vs_zero, 4) if z_vs_zero is not None else None),
            "p_vs_zero_two_sided": (round(p_vs_zero, 4) if p_vs_zero is not None else None),
        },
        "decomposition_comparison": comparison,
        "_net_rate": net_rate, "_se_net_rate": se_net_rate,
    }


def analyze(manifest_path: Path) -> dict[str, Any]:
    manifest = load_json(manifest_path)
    require(manifest.get("schema") == PANEL_SCHEMA, "panel manifest schema mismatch")
    require(manifest.get("all_arms_completed_before_any_read") is True, "panel manifest does not attest outcome-blind completion")
    pair_count = manifest["pair_count"]
    arms = manifest["arms"]
    require(type(arms) is list and len(arms) >= 1, "panel manifest has no arms")

    results = []
    for arm in arms:
        completion_path = record(arm["completion"], f"arm {arm['label']} completion")
        completion = load_json(completion_path)
        require(completion.get("schema") == COMPLETION_SCHEMA and completion.get("success") is True, f"arm {arm['label']} completion invalid")
        outcome_path = record(arm["outcome"], f"arm {arm['label']} outcome")
        require(sha256(outcome_path) == completion.get("outcome_sha256"), f"arm {arm['label']} completion outcome SHA mismatch")
        arm_result = analyze_arm(outcome_path, pair_count, arm.get("decomposition_reference_slot"))
        arm_result["label"] = arm["label"]
        results.append(arm_result)

    # Between-arm comparisons: independent two-sample z-tests on each pair
    # of arms' paired net rates. The three arms share no CRN seed with each
    # other (each has its own fresh evaluation seed), so they are treated
    # as statistically independent; each arm's own McNemar SE (se_net_rate)
    # is the correct per-arm variance to combine.
    between_arm = []
    for i in range(len(results)):
        for j in range(i + 1, len(results)):
            a, b = results[i], results[j]
            combined_se = math.sqrt(a["_se_net_rate"] ** 2 + b["_se_net_rate"] ** 2)
            diff = a["_net_rate"] - b["_net_rate"]
            z = diff / combined_se if combined_se > 0 else None
            between_arm.append({
                "arm_a": a["label"], "arm_b": b["label"],
                "net_rate_a": round(a["_net_rate"], 4), "net_rate_b": round(b["_net_rate"], 4),
                "difference": round(diff, 4), "combined_se": round(combined_se, 4),
                "z": (round(z, 4) if z is not None else None),
                "p_two_sided": (round(two_sided_normal_p(z), 4) if z is not None else None),
            })

    # promoted(2) mirror vs the decomposition's marginal pooled-mixture
    # P0-minus-P1 tilt (the "+3.1pp" figure this whole panel exists to
    # decompose). Only meaningful for the promoted2/slot0 arm.
    mixture_marginal_comparison = None
    for a in results:
        if a["label"] == MIXTURE_MARGINAL_ARM_LABEL:
            combined_se = math.sqrt(a["_se_net_rate"] ** 2 + MIXTURE_MARGINAL_REFERENCE["se"] ** 2)
            diff = a["_net_rate"] - MIXTURE_MARGINAL_REFERENCE["diff"]
            z = diff / combined_se
            mixture_marginal_comparison = {
                "arm": a["label"],
                "mirror_net_rate": round(a["_net_rate"], 4),
                "mixture_marginal_pooled_p0_minus_p1": MIXTURE_MARGINAL_REFERENCE["diff"],
                "mixture_marginal_se": MIXTURE_MARGINAL_REFERENCE["se"],
                "difference": round(diff, 4), "combined_se": round(combined_se, 4),
                "z": round(z, 4), "p_two_sided": round(two_sided_normal_p(z), 4),
            }

    for a in results:
        del a["_net_rate"], a["_se_net_rate"]

    return {
        "schema": "mtg-kernel-mirror-seat-diagnostic-analysis/v1",
        "descriptive_only": True,
        "no_thresholds": True,
        "no_gate": True,
        "run_mode": manifest["run_mode"],
        "pair_count": pair_count,
        "arms": results,
        "between_arm_comparisons": between_arm,
        "promoted2_vs_mixture_marginal_tilt": mixture_marginal_comparison,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    require(not args.output.exists(), f"refusing to overwrite {args.output}")
    result = analyze(args.manifest)
    text = json.dumps(result, indent=2, sort_keys=True) + "\n"
    with args.output.open("x", encoding="utf-8") as stream:
        stream.write(text)


if __name__ == "__main__":
    main()
