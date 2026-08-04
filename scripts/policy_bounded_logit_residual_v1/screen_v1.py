#!/usr/bin/env python3
"""Screen a tail-bounded version of the rejected terminal PPO policy."""

from __future__ import annotations

import argparse
import importlib.util
import json
import math
from pathlib import Path
import sys
import time
from typing import Any

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
BASE_PATH = SCRIPT_DIR.parent / "policy_block_response_oracle_v1" / "run_oracle_v1.py"
SPEC = importlib.util.spec_from_file_location("bounded_logit_block_base", BASE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("unable to load policy-block base module")
block = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = block
SPEC.loader.exec_module(block)
pipeline = block.base.pipeline
distill = pipeline.distill


SCHEMA = "mtg-kernel-bounded-logit-terminal-residual-screen/v1"
METHOD = "initializer-weighted-centered-logit-residual-clamp/v1"
CLIP_GRID = (
    0.03,
    0.04,
    0.05,
    0.06,
    0.08,
    0.10,
    0.12,
    0.16,
    0.20,
    0.24,
    0.28,
    0.32,
    0.40,
)
MEAN_TV_MAX = 0.030
P90_TV_MAX = 0.100
JOINT_LOG_RATIO_MAX = 0.50
MEAN_TV_MIN = 4.0 * 0.0017975835928040842
TOP_CHANGE_MIN = 4.0 * (1.0 - 0.9994097787903164)


def _fail(message: str) -> None:
    raise ValueError(message)


def bounded_logits(
    initializer_logits: torch.Tensor,
    trained_logits: torch.Tensor,
    clip: float,
) -> torch.Tensor:
    if (
        initializer_logits.ndim != 1
        or trained_logits.shape != initializer_logits.shape
        or initializer_logits.numel() == 0
        or not math.isfinite(clip)
        or clip < 0.0
        or not torch.isfinite(initializer_logits).all()
        or not torch.isfinite(trained_logits).all()
    ):
        _fail("invalid bounded-logit input")
    initial = initializer_logits.double()
    delta = trained_logits.double() - initial
    probability = torch.softmax(initial, dim=0)
    centered = delta - (probability * delta).sum()
    return initial + centered.clamp(min=-clip, max=clip)


def _empty() -> dict[str, Any]:
    return {
        "tv_sum": 0.0,
        "mass": 0.0,
        "top_same_sum": 0.0,
        "samples": [],
        "max_joint": 0.0,
        "rows": 0,
        "decisions": 0,
    }


def _finish(raw: dict[str, Any]) -> dict[str, Any]:
    mass = max(float(raw["mass"]), 1.0e-12)
    top_agreement = float(raw["top_same_sum"]) / mass
    return {
        "mean_total_variation": float(raw["tv_sum"]) / mass,
        "p90_total_variation": distill._weighted_quantile(raw["samples"], 0.90),
        "top_action_agreement": top_agreement,
        "top_action_change_rate": 1.0 - top_agreement,
        "maximum_absolute_joint_log_ratio": float(raw["max_joint"]),
        "policy_mass": float(raw["mass"]),
        "policy_rows": int(raw["rows"]),
        "physical_decisions": int(raw["decisions"]),
    }


def _accumulate_row(
    raw: dict[str, Any],
    old_logits: torch.Tensor,
    new_logits: torch.Tensor,
    row_mass: float,
) -> None:
    old_probability = torch.softmax(old_logits.double(), dim=0)
    new_probability = torch.softmax(new_logits.double(), dim=0)
    tv = float(0.5 * (old_probability - new_probability).abs().sum())
    same = float(int(old_logits.argmax()) == int(new_logits.argmax()))
    raw["tv_sum"] += tv * row_mass
    raw["mass"] += row_mass
    raw["top_same_sum"] += same * row_mass
    raw["samples"].append((tv, row_mass))
    raw["rows"] += 1


def _gate(metrics: dict[str, Any]) -> dict[str, Any]:
    checks: dict[str, bool] = {}
    for label, item in (
        ("overall", metrics["overall"]),
        ("candidate_seat_0", metrics["by_candidate_seat"]["0"]),
        ("candidate_seat_1", metrics["by_candidate_seat"]["1"]),
    ):
        checks[f"{label}_mean_tv_at_most_0p030"] = (
            item["mean_total_variation"] <= MEAN_TV_MAX
        )
        checks[f"{label}_p90_tv_at_most_0p100"] = (
            item["p90_total_variation"] <= P90_TV_MAX
        )
    checks["maximum_absolute_joint_log_ratio_at_most_0p50"] = (
        metrics["overall"]["maximum_absolute_joint_log_ratio"]
        <= JOINT_LOG_RATIO_MAX
    )
    return {"pass": all(checks.values()), "checks": checks}


def screen(
    cache_path: Path,
    pair_limit: int | None,
    threads: int,
    clips: tuple[float, ...] = CLIP_GRID,
) -> dict[str, Any]:
    started = time.perf_counter()
    torch.set_num_threads(threads)
    decisions, source, timings = pipeline._load_decisions(cache_path, pair_limit)
    initial_state, trained_state, identity = block._base_states()
    initial_model = pipeline.distill._model()
    trained_model = pipeline.distill._model()
    initial_model.load_state_dict(initial_state["model_state_dict"], strict=True)
    trained_model.load_state_dict(trained_state["model_state_dict"], strict=True)
    initial_model.eval()
    trained_model.eval()
    weights = distill._episode_weights(decisions)
    accumulators = {
        clip: {"overall": _empty(), "seats": {0: _empty(), 1: _empty()}}
        for clip in clips
    }
    maximum_initial_alignment = 0.0
    with torch.no_grad():
        for decision in decisions:
            _, row_mass = weights[decision.key]
            joints = {clip: 0.0 for clip in clips}
            for row in decision.rows:
                initial_logits, initial_value = initial_model._one(row)
                trained_logits, trained_value = trained_model._one(row)
                if initial_value.detach().float().numpy().tobytes() != trained_value.detach().float().numpy().tobytes():
                    _fail("frozen value changed between source states")
                maximum_initial_alignment = max(
                    maximum_initial_alignment,
                    float((initial_logits - row["old_logits"]).abs().max()),
                )
                for clip in clips:
                    logits = bounded_logits(initial_logits, trained_logits, clip)
                    target = accumulators[clip]["seats"][decision.candidate_seat]
                    _accumulate_row(accumulators[clip]["overall"], row["old_logits"], logits, row_mass)
                    _accumulate_row(target, row["old_logits"], logits, row_mass)
                    joints[clip] += float(
                        torch.log_softmax(logits, dim=0)[int(row["selected_index"])]
                    )
            for clip in clips:
                joint_delta = abs(joints[clip] - decision.old_joint_log_probability)
                for raw in (
                    accumulators[clip]["overall"],
                    accumulators[clip]["seats"][decision.candidate_seat],
                ):
                    raw["max_joint"] = max(raw["max_joint"], joint_delta)
                    raw["decisions"] += 1
    if maximum_initial_alignment > pipeline.TRANSPORT_LIMIT:
        _fail("initializer no longer aligns with the behavior cache")
    rows = []
    for clip in clips:
        metrics = {
            "overall": _finish(accumulators[clip]["overall"]),
            "by_candidate_seat": {
                str(seat): _finish(accumulators[clip]["seats"][seat])
                for seat in (0, 1)
            },
        }
        rows.append({"clip": clip, "metrics": metrics, "safety_gate": _gate(metrics)})
    complete_grid = tuple(clips) == CLIP_GRID
    safe = [row for row in rows if row["safety_gate"]["pass"]]
    selected = max(safe, key=lambda row: row["clip"]) if complete_grid and safe else None
    mechanism_checks = (
        {
            "safe_grid_point_exists": selected is not None,
            "selected_mean_tv_at_least_four_times_trust_projection": selected is not None
            and selected["metrics"]["overall"]["mean_total_variation"] >= MEAN_TV_MIN,
            "selected_top_action_change_rate_at_least_four_times_trust_projection": selected is not None
            and selected["metrics"]["overall"]["top_action_change_rate"] >= TOP_CHANGE_MIN,
        }
        if complete_grid
        else {"valid_disjoint_grid_shard": True}
    )
    return {
        "schema": SCHEMA,
        "method": METHOD,
        "source": source,
        "initializer_identity": identity,
        "rejected_state_sha256": block.TRAINED_STATE_SHA256,
        "clip_grid": list(CLIP_GRID),
        "evaluated_clips": list(clips),
        "maximum_initializer_alignment_error": maximum_initial_alignment,
        "rows": rows,
        "selected_clip": None if selected is None else selected["clip"],
        "mechanism_gate": {
            "decision": (
                "PASS"
                if complete_grid and all(mechanism_checks.values())
                else "REJECT"
                if complete_grid
                else "SHARD"
            ),
            "checks": mechanism_checks,
        },
        "load_timings_seconds": timings,
        "runtime_seconds": time.perf_counter() - started,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--pair-limit", type=int)
    parser.add_argument("--threads", type=int, default=24)
    parser.add_argument("--clips", type=float, nargs="+")
    args = parser.parse_args()
    if args.output.exists():
        _fail(f"output already exists: {args.output}")
    clips = CLIP_GRID if args.clips is None else tuple(args.clips)
    if len(set(clips)) != len(clips) or any(clip not in CLIP_GRID for clip in clips):
        _fail("clips must be a unique subset of the fixed grid")
    result = screen(args.cache, args.pair_limit, args.threads, clips)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(result, indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
