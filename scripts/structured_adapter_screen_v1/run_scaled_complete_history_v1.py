#!/usr/bin/env python3
"""Prepare, fit, and aggregate the fixed scaled complete-history screen."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from argparse import Namespace
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run_screen as screen  # noqa: E402


SCHEMA = "mtg-kernel-scaled-complete-history-screen/v1"
DIM = 48
EPOCHS = 5
BATCH_SIZE = 64
LEARNING_RATE = 3e-4
WEIGHT_DECAY = 1e-4
VALUE_COEFFICIENT = 1.0
SEED = 20_260_802
THREADS = 6
HISTORY_LENGTH = 16


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _write(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, sort_keys=True, indent=2, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def _fixed_args() -> Namespace:
    return Namespace(
        dim=DIM,
        epochs=EPOCHS,
        batch_size=BATCH_SIZE,
        lr=LEARNING_RATE,
        weight_decay=WEIGHT_DECAY,
        value_coefficient=VALUE_COEFFICIENT,
        seed=SEED,
        threads=THREADS,
        history_length=HISTORY_LENGTH,
        complete_history=True,
        diagnostic_sample_size=screen.DEFAULT_DIAGNOSTIC_SAMPLE_SIZE,
        ablation_sample_size=screen.DEFAULT_ABLATION_SAMPLE_SIZE,
        save_model_state=True,
        float64_permutation=True,
    )


def prepare(combine_report: Path, cache: Path, output: Path) -> dict[str, Any]:
    if cache.exists() or output.exists():
        _fail("refusing to overwrite an existing cache or cache report")
    report = json.loads(combine_report.read_text(encoding="utf-8"))
    if (
        report.get("schema") != "mtg-kernel-scaled-structured-corpus/v1"
        or report.get("pass") is not True
        or report.get("pair_count") != 2_048
        or report.get("game_count") != 4_096
        or set(report.get("fold_pair_counts", {}).values()) != {512}
    ):
        _fail("combine report is not a passing scaled corpus")
    teacher = report["teacher"]
    outcome = report["outcome"]
    teacher_path = Path(teacher["path"])
    outcome_path = Path(outcome["path"])
    if _sha256(teacher_path) != teacher["sha256"] or _sha256(outcome_path) != outcome["sha256"]:
        _fail("combined corpus hash mismatch")
    result = screen.prepare_cache(
        teacher_path,
        outcome_path,
        cache,
        teacher["sha256"],
        outcome["sha256"],
        True,
    )
    wrapped = {
        "schema": SCHEMA + ".cache",
        "combine_report": {"path": str(combine_report), "sha256": _sha256(combine_report)},
        **result,
        "cache_sha256": _sha256(cache),
    }
    _write(output, wrapped)
    return wrapped


def run_fold(cache: Path, output: Path, fold: int) -> dict[str, Any]:
    if not cache.is_file() or output.exists():
        _fail("fold cache is missing or output already exists")
    result = screen.run_fold(cache, output, fold, _fixed_args())
    result["scaled_screen_schema"] = SCHEMA
    screen._write_json(output, result)
    return result


def aggregate(paths: list[Path], output: Path) -> dict[str, Any]:
    if output.exists():
        _fail("refusing to overwrite an existing aggregate")
    expected_config = vars(_fixed_args())
    for path in paths:
        fold = json.loads(path.read_text(encoding="utf-8"))
        config = fold.get("config", {})
        if (
            fold.get("scaled_screen_schema") != SCHEMA
            or any(config.get(key) != value for key, value in expected_config.items())
            or config.get("history_feature_dim")
            != screen.ACTION_EXPLICIT_DIM + screen.COMPLETE_HISTORY_ROLE_DIM + 136
            or len(fold.get("counts", {}).get("heldout_pairs", [])) != 512
            or len(fold.get("counts", {}).get("train_pairs", [])) != 1_536
        ):
            _fail(f"fold does not match the scaled screen contract: {path}")
    result = screen.aggregate(paths, output)
    folds = [json.loads(path.read_text(encoding="utf-8")) for path in paths]
    policy_positive = sum(
        fold["heldout"]["policy"]["relative_improvement"] > 0.0 for fold in folds
    )
    value_positive = sum(
        fold["heldout"]["value"]["relative_improvement"] > 0.0 for fold in folds
    )
    common = (
        result["gates"]["permutation_max_delta_le_1e-5"]
        and result["gates"]["ref_removal_affected_rate_ge_20pct"]
    )
    policy_pass = common and all(
        result["gates"][name]
        for name in (
            "policy_nll_relative_improvement_ge_5pct",
            "no_acting_seat_policy_nll_regression",
            "policy_top1_noninferior_minus_0_5pp",
        )
    ) and policy_positive >= 3
    value_pass = common and all(
        result["gates"][name]
        for name in (
            "value_mse_relative_improvement_ge_5pct",
            "no_candidate_seat_value_regression_over_2pct",
        )
    ) and value_positive >= 3
    result.update(
        {
            "schema": SCHEMA + ".aggregate",
            "fold_positive_counts": {
                "policy_nll": policy_positive,
                "value_mse": value_positive,
            },
            "lane_gates": {
                "common_representation": common,
                "policy": policy_pass,
                "value": value_pass,
            },
            "pass": policy_pass or value_pass,
            "disposition": (
                "advance-passing-lanes" if policy_pass or value_pass else "reject-both-lanes"
            ),
        }
    )
    screen._write_json(output, result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--prepare", action="store_true")
    modes.add_argument("--fold", type=int, choices=range(4))
    modes.add_argument("--aggregate", action="store_true")
    parser.add_argument("--combine-report", type=Path)
    parser.add_argument("--cache", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--fold-results", type=Path, nargs="+")
    args = parser.parse_args()
    if args.prepare:
        if args.combine_report is None or args.cache is None:
            _fail("--prepare requires --combine-report and --cache")
        result = prepare(args.combine_report, args.cache, args.output)
    elif args.fold is not None:
        if args.cache is None:
            _fail("--fold requires --cache")
        result = run_fold(args.cache, args.output, args.fold)
    else:
        if args.fold_results is None or len(args.fold_results) != 4:
            _fail("--aggregate requires four --fold-results")
        result = aggregate(args.fold_results, args.output)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
