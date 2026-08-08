#!/usr/bin/env python3
"""Signed-residual / calibration bias probe over the frozen width-48 history
critic's already-collected fresh confirmation panel.

This is a re-instrumented EVALUATION-ONLY pass (forward passes under
torch.no_grad(); no optimizer, no training) over the exact same fresh cache
and exact same frozen model.state.pt consumed by the original bounded
history-value confirmation run
(D:\\mtg-kernel-bounded-onpolicy-history-value-v1). It reproduces that run's
per-seat MSE numbers as a harness-validity check, then additionally retains
per-decision signed residuals (grouped by episode) to compute:

  1. signed residual mean (bias) per seat, both models, with a cluster
     bootstrap over episodes (not decisions),
  2. bias conditional on realized outcome (win vs loss) per seat,
  3. a calibration slope (realized return regressed on prediction) per seat.

READ-ONLY with respect to D:\\mtg-kernel-bounded-onpolicy-history-value-v1 and
every existing worktree: only reads model.state.pt, fit.json,
fresh\\collection.json, fresh\\cache.pt. Writes only to --output (refuses to
overwrite).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import time
from pathlib import Path
from typing import Any

import numpy as np
import torch

SCRIPT_DIR = Path(__file__).resolve().parent
SCRIPTS_DIR = SCRIPT_DIR.parent
STRUCTURED_DIR = SCRIPTS_DIR / "structured_adapter_screen_v1"
TERMINAL_DIR = SCRIPTS_DIR / "policy_only_structured_terminal_rung_v1"
for directory in (STRUCTURED_DIR, TERMINAL_DIR):
    sys.path.insert(0, str(directory))

import run_pipeline_v1 as terminal  # noqa: E402
import run_screen as screen  # noqa: E402
import run_structured_successor_distillation_v1 as distill  # noqa: E402


DEVELOPMENT_CACHE_SHA256 = "454e4ce1b8f7413839a36c8e2731fc0cb65581ce13e593634bffa70013a6f16d"
INITIALIZER_STATE_SHA256 = "ff2abf50e8760780a9331e53aa7323cb96e3c64edb6e7d89062dbe38bf6a5cc0"
FRESH_SCORER_SHA256 = "c0c9b2004261c5f220f105636c09bdf38a82e43c117e8a67d5ba9d00e0297672"
FRESH_BASE_SEED = 1_690_001
FRESH_PAIR_COUNT = 1_024
SEED = 20_260_810
THREADS = 24
PARENT_BOUND_EPSILON = 1.0e-3
FIT_SCHEMA = "mtg-kernel-bounded-onpolicy-history-value-fit/v1"
MODEL_STATE_SHA256_EXPECTED = "cae8e19ef825325508de351b883b2df3863dc66f0288be06ad2ccf868e3d7d7c"

# Known headline figures from the original confirmation.json, reproduced here
# purely as a harness-validity target (not re-derived from it).
KNOWN_CANDIDATE_MSE = {0: 0.45817498864228884, 1: 0.4275161940903657}
KNOWN_RELATIVE_IMPROVEMENT = {0: 0.3229490498587241, 1: 0.3637854546138304}
KNOWN_PARENT_MSE = {0: 0.6767215798850653, 1: 0.6719685948564282}

BOOTSTRAP_DRAWS = 10_000
BOOTSTRAP_SEED = 424242


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _bounded_prediction(model: Any, row: dict[str, Any]) -> torch.Tensor:
    parent = row["old_value"]
    if not bool(torch.isfinite(parent)):
        _fail("retained parent value is non-finite")
    bounded_parent = parent.clamp(-1.0 + PARENT_BOUND_EPSILON, 1.0 - PARENT_BOUND_EPSILON)
    _, raw_residual = model._one(row)  # noqa: SLF001
    shift = torch.tanh(raw_residual)
    denominator = 1.0 + bounded_parent * shift
    prediction = (bounded_parent + shift) / denominator
    if not bool(torch.isfinite(prediction)):
        _fail("bounded value prediction is non-finite")
    return prediction


def _weighted_mean(values: np.ndarray, weights: np.ndarray) -> float:
    return float(np.sum(values * weights) / np.sum(weights))


def _weighted_slope(x: np.ndarray, y: np.ndarray, w: np.ndarray) -> tuple[float, float]:
    wsum = np.sum(w)
    xbar = np.sum(x * w) / wsum
    ybar = np.sum(y * w) / wsum
    cov = np.sum(w * (x - xbar) * (y - ybar))
    var = np.sum(w * (x - xbar) ** 2)
    slope = float(cov / var) if var > 0 else float("nan")
    intercept = float(ybar - slope * xbar)
    return slope, intercept


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model-state", type=Path, required=True)
    parser.add_argument("--fit-report", type=Path, required=True)
    parser.add_argument("--fresh-cache", type=Path, required=True)
    parser.add_argument("--fresh-collection", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    if args.output.exists():
        _fail(f"refusing to overwrite {args.output}")

    started = time.perf_counter()

    model_state_sha256 = _sha256(args.model_state)
    if model_state_sha256 != MODEL_STATE_SHA256_EXPECTED:
        _fail("model.state.pt SHA-256 mismatch vs. task-specified hash")

    fit_report = json.loads(args.fit_report.read_text(encoding="utf-8"))
    if (
        fit_report.get("schema") != FIT_SCHEMA
        or fit_report.get("status") != "complete"
        or fit_report.get("source", {}).get("cache_sha256") != DEVELOPMENT_CACHE_SHA256
        or fit_report.get("initializer", {}).get("sha256") != INITIALIZER_STATE_SHA256
        or fit_report.get("model_state", {}).get("sha256") != model_state_sha256
    ):
        _fail("bounded value fit report or state binding mismatch")

    fresh_collection = json.loads(args.fresh_collection.read_text(encoding="utf-8"))
    fresh_collection_sha256 = _sha256(args.fresh_collection)
    if (
        fresh_collection.get("schema") != terminal.COLLECTION_SCHEMA
        or fresh_collection.get("status") != "pass"
        or fresh_collection.get("base_seed") != FRESH_BASE_SEED
        or fresh_collection.get("pair_count") != FRESH_PAIR_COUNT
        or fresh_collection.get("scorer_sha256") != FRESH_SCORER_SHA256
    ):
        _fail("fresh bounded-value collection report mismatch")

    print("loading fresh cache and attaching complete history (this is the slow step)...", file=sys.stderr)
    decisions, source, timings = terminal._load_decisions(args.fresh_cache, None)  # noqa: SLF001
    if (
        source["base_seed"] != FRESH_BASE_SEED
        or source["pair_count"] != FRESH_PAIR_COUNT
        or source["collection_sha256"] != fresh_collection_sha256
    ):
        _fail("fresh bounded-value confirmation panel identity mismatch")

    screen._configure(SEED, THREADS)  # noqa: SLF001
    payload = torch.load(args.model_state, map_location="cpu", weights_only=False)
    if (
        payload.get("schema") != FIT_SCHEMA + ".state"
        or payload.get("development_cache_sha256") != DEVELOPMENT_CACHE_SHA256
        or payload.get("initializer_state_sha256") != INITIALIZER_STATE_SHA256
    ):
        _fail("bounded value state provenance mismatch")
    model = distill._model()
    model.load_state_dict(payload["model_state_dict"], strict=True)
    model.eval()

    print(f"evaluating {len(decisions)} physical decisions...", file=sys.stderr)
    # Per-decision records, grouped by (seat, episode_key).
    by_episode: dict[tuple[int, tuple], dict[str, Any]] = {}
    seat_records: dict[int, list[tuple[float, float, float, float]]] = {0: [], 1: []}
    # seat_records[seat] entries: (weight, raw_parent, projected_parent, candidate, target)
    seat_full: dict[int, list[tuple[float, float, float, float, float]]] = {0: [], 1: []}
    eval_started = time.perf_counter()
    with torch.no_grad():
        for decision in decisions:
            row = decision.rows[0]
            seat = int(decision.candidate_seat)
            weight = float(decision.episode_weight)
            raw_parent = float(row["old_value"])
            projected_parent = max(
                -1.0 + PARENT_BOUND_EPSILON, min(1.0 - PARENT_BOUND_EPSILON, raw_parent)
            )
            target = float(row["terminal_reward"])
            candidate = float(_bounded_prediction(model, row))

            ep_key = (seat, decision.episode_key)
            bucket = by_episode.setdefault(
                ep_key,
                {
                    "seat": seat,
                    "weight_sum": 0.0,
                    "parent_num": 0.0,
                    "candidate_num": 0.0,
                    "targets": set(),
                },
            )
            bucket["weight_sum"] += weight
            bucket["parent_num"] += weight * (projected_parent - target)
            bucket["candidate_num"] += weight * (candidate - target)
            bucket["targets"].add(round(target, 9))

            seat_full[seat].append((weight, raw_parent, projected_parent, candidate, target))
    eval_seconds = time.perf_counter() - eval_started
    print(f"evaluation forward passes took {eval_seconds:.1f}s", file=sys.stderr)

    # ---- Sanity check: reproduce the known per-seat MSE / relative-improvement ----
    sanity: dict[str, Any] = {}
    harness_valid = True
    for seat in (0, 1):
        records = seat_full[seat]
        w = np.array([r[0] for r in records], dtype=np.float64)
        raw_parent = np.array([r[1] for r in records], dtype=np.float64)
        parent = np.array([r[2] for r in records], dtype=np.float64)
        candidate = np.array([r[3] for r in records], dtype=np.float64)
        target = np.array([r[4] for r in records], dtype=np.float64)
        mass = float(np.sum(w))
        raw_parent_mse = float(np.sum(w * (raw_parent - target) ** 2) / mass)
        parent_mse = float(np.sum(w * (parent - target) ** 2) / mass)
        candidate_mse = float(np.sum(w * (candidate - target) ** 2) / mass)
        relative_improvement = (parent_mse - candidate_mse) / parent_mse
        known_candidate_mse = KNOWN_CANDIDATE_MSE[seat]
        known_relative_improvement = KNOWN_RELATIVE_IMPROVEMENT[seat]
        known_parent_mse = KNOWN_PARENT_MSE[seat]
        candidate_mse_match = abs(candidate_mse - known_candidate_mse) <= 1e-6
        relative_improvement_match = (
            abs(relative_improvement - known_relative_improvement) <= 1e-6
        )
        parent_mse_match = abs(parent_mse - known_parent_mse) <= 1e-6
        harness_valid = harness_valid and candidate_mse_match and relative_improvement_match and parent_mse_match
        sanity[str(seat)] = {
            "physical_decision_count": len(records),
            "episode_mass": mass,
            "computed_parent_mse": parent_mse,
            "known_parent_mse": known_parent_mse,
            "parent_mse_match_within_1e_6": parent_mse_match,
            "computed_candidate_mse": candidate_mse,
            "known_candidate_mse": known_candidate_mse,
            "candidate_mse_match_within_1e_6": candidate_mse_match,
            "computed_relative_improvement": relative_improvement,
            "known_relative_improvement": known_relative_improvement,
            "relative_improvement_match_within_1e_6": relative_improvement_match,
            "computed_relative_improvement_percent": relative_improvement * 100.0,
        }

    # ---- Episode-level bias aggregates ----
    episode_rows: dict[int, list[dict[str, Any]]] = {0: [], 1: []}
    non_constant_target_episodes = 0
    for (seat, _ep_key), bucket in by_episode.items():
        if len(bucket["targets"]) != 1:
            non_constant_target_episodes += 1
        target_value = sorted(bucket["targets"])[0]
        episode_rows[seat].append(
            {
                "parent_bias": bucket["parent_num"] / bucket["weight_sum"],
                "candidate_bias": bucket["candidate_num"] / bucket["weight_sum"],
                "weight_sum": bucket["weight_sum"],
                "target": target_value,
            }
        )

    rng = np.random.default_rng(BOOTSTRAP_SEED)

    def cluster_bootstrap(values: np.ndarray, draws: int = BOOTSTRAP_DRAWS) -> dict[str, float]:
        n = len(values)
        if n == 0:
            return {"n_episodes": 0}
        idx = rng.integers(0, n, size=(draws, n))
        resampled_means = values[idx].mean(axis=1)
        return {
            "n_episodes": n,
            "point_estimate": float(values.mean()),
            "bootstrap_mean": float(resampled_means.mean()),
            "bootstrap_se": float(resampled_means.std(ddof=1)),
            "ci95_lo": float(np.percentile(resampled_means, 2.5)),
            "ci95_hi": float(np.percentile(resampled_means, 97.5)),
        }

    bias_results: dict[str, Any] = {}
    outcome_results: dict[str, Any] = {}
    calibration_results: dict[str, Any] = {}

    for seat in (0, 1):
        rows = episode_rows[seat]
        parent_vals = np.array([r["parent_bias"] for r in rows], dtype=np.float64)
        candidate_vals = np.array([r["candidate_bias"] for r in rows], dtype=np.float64)
        targets = np.array([r["target"] for r in rows], dtype=np.float64)

        bias_results[str(seat)] = {
            "parent": cluster_bootstrap(parent_vals),
            "candidate": cluster_bootstrap(candidate_vals),
            "n_episodes_total": len(rows),
        }

        win_mask = targets > 0
        loss_mask = targets < 0
        draw_mask = targets == 0
        outcome_results[str(seat)] = {
            "win": {
                "n_episodes": int(win_mask.sum()),
                "parent": cluster_bootstrap(parent_vals[win_mask]),
                "candidate": cluster_bootstrap(candidate_vals[win_mask]),
            },
            "loss": {
                "n_episodes": int(loss_mask.sum()),
                "parent": cluster_bootstrap(parent_vals[loss_mask]),
                "candidate": cluster_bootstrap(candidate_vals[loss_mask]),
            },
            "draw": {
                "n_episodes": int(draw_mask.sum()),
                "parent": cluster_bootstrap(parent_vals[draw_mask]),
                "candidate": cluster_bootstrap(candidate_vals[draw_mask]),
            },
        }

        # Decision-level calibration slope (episode-balanced weights), with a
        # cluster bootstrap over episodes (resample whole episodes' decisions).
        records = seat_full[seat]
        w_all = np.array([r[0] for r in records], dtype=np.float64)
        parent_all = np.array([r[2] for r in records], dtype=np.float64)
        candidate_all = np.array([r[3] for r in records], dtype=np.float64)
        target_all = np.array([r[4] for r in records], dtype=np.float64)

        parent_slope, parent_intercept = _weighted_slope(parent_all, target_all, w_all)
        candidate_slope, candidate_intercept = _weighted_slope(candidate_all, target_all, w_all)

        # Build per-episode index groups for the decision-level cluster bootstrap.
        ep_keys_all = [
            (seat, decision.episode_key)
            for decision in decisions
            if int(decision.candidate_seat) == seat
        ]
        unique_eps = sorted(set(ep_keys_all))
        ep_to_rows: dict[tuple, list[int]] = {ep: [] for ep in unique_eps}
        for i, ep in enumerate(ep_keys_all):
            ep_to_rows[ep].append(i)
        ep_list = unique_eps
        n_eps = len(ep_list)
        slope_draws = 2000
        parent_slope_boot = np.empty(slope_draws, dtype=np.float64)
        candidate_slope_boot = np.empty(slope_draws, dtype=np.float64)
        for d in range(slope_draws):
            picks = rng.integers(0, n_eps, size=n_eps)
            row_idx = np.concatenate([ep_to_rows[ep_list[p]] for p in picks])
            w_b = w_all[row_idx]
            parent_slope_boot[d], _ = _weighted_slope(parent_all[row_idx], target_all[row_idx], w_b)
            candidate_slope_boot[d], _ = _weighted_slope(candidate_all[row_idx], target_all[row_idx], w_b)

        calibration_results[str(seat)] = {
            "parent": {
                "slope": parent_slope,
                "intercept": parent_intercept,
                "bootstrap_se": float(np.nanstd(parent_slope_boot, ddof=1)),
                "ci95_lo": float(np.nanpercentile(parent_slope_boot, 2.5)),
                "ci95_hi": float(np.nanpercentile(parent_slope_boot, 97.5)),
            },
            "candidate": {
                "slope": candidate_slope,
                "intercept": candidate_intercept,
                "bootstrap_se": float(np.nanstd(candidate_slope_boot, ddof=1)),
                "ci95_lo": float(np.nanpercentile(candidate_slope_boot, 2.5)),
                "ci95_hi": float(np.nanpercentile(candidate_slope_boot, 97.5)),
            },
        }

    report = {
        "schema": "mtg-kernel-value-bias-probe/v1",
        "model_state_sha256": model_state_sha256,
        "fresh_cache_sha256": _sha256(args.fresh_cache),
        "fresh_collection_sha256": fresh_collection_sha256,
        "source": source,
        "harness_valid_against_known_mse": harness_valid,
        "non_constant_target_episodes": non_constant_target_episodes,
        "sanity_mse_reproduction": sanity,
        "bias_by_seat": bias_results,
        "bias_by_seat_and_outcome": outcome_results,
        "calibration_slope_by_seat": calibration_results,
        "bootstrap": {
            "draws_for_bias": BOOTSTRAP_DRAWS,
            "draws_for_slope": 2000,
            "seed": BOOTSTRAP_SEED,
            "unit": "episode (candidate_seat, episode_key) cluster",
        },
        "timings": {
            **timings,
            "eval_seconds": eval_seconds,
            "total_seconds": time.perf_counter() - started,
        },
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"harness_valid": harness_valid, "output": str(args.output)}))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
