#!/usr/bin/env python3
"""Fit and evaluate the frozen action-conditioned counterfactual Q screen."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time
from typing import Any

import numpy as np
import torch


SCRIPT_DIR = Path(__file__).resolve().parent
STRUCTURED_DIR = SCRIPT_DIR.parent / "structured_adapter_screen_v1"
sys.path.insert(0, str(STRUCTURED_DIR))

import run_screen as structured  # noqa: E402
import run_structured_successor_distillation_v1 as distill  # noqa: E402


CORPUS_SCHEMA = "mtg-kernel-native-action-conditioned-counterfactual-corpus/v1"
FIT_SCHEMA = "mtg-kernel-action-conditioned-counterfactual-q-fit/v1"
INITIALIZER_STATE_SHA256 = "ff2abf50e8760780a9331e53aa7323cb96e3c64edb6e7d89062dbe38bf6a5cc0"
RIDGE_LAMBDAS = (0.01, 0.1, 1.0, 10.0, 100.0)
DEPLOYMENT_MARGINS = (0.0, 0.125, 0.25, 0.5)
BOOTSTRAP_SEED = 20_260_804
BOOTSTRAP_REPLICATES = 10_000


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _git_head() -> str:
    completed = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=SCRIPT_DIR.parent.parent,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=True,
    )
    return completed.stdout.strip()


def _json_bytes(value: Any) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n").encode(
        "utf-8"
    )


def _write_new(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(_json_bytes(value))


def _parse_row(
    root: dict[str, Any],
    raw: dict[str, Any],
    selected_index: int,
    selected_semantic: dict[str, Any],
    acting_player: str,
    step: int,
    physical_decision_id: int,
    substep_index: int,
    substep_count: int,
) -> dict[str, Any]:
    return structured._parse_example(  # noqa: SLF001
        {
            "tensor": raw["tensor"] if "tensor" in raw else raw["public_root_tensor"],
            "old_policy_logits_f32_bits": raw["parent_logits_f32_bits"],
            "old_value_f32_bits": raw["parent_value_f32_bits"],
            "selected_index": selected_index,
            "selected_semantic": selected_semantic,
            "pair_index": root["balance_pair_index"],
            "episode_id": root["episode_id"],
            "acting_player": acting_player,
            "candidate_seat": root["acting_player"],
            "step": step,
            "physical_decision_id": physical_decision_id,
            "substep_index": substep_index,
            "substep_count": substep_count,
            "decision_kind": "surface",
        },
        is_outcome=True,
    )


def _load_corpus(path: Path) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    report = json.loads(path.read_text(encoding="utf-8"))
    gates = report.get("gates", {})
    aggregate = report.get("aggregate", {})
    if (
        report.get("schema") != CORPUS_SCHEMA
        or not gates.get("corpus_qualified")
        or aggregate.get("roots_collected") != 256
        or aggregate.get("p0_roots") != 128
        or aggregate.get("p1_roots") != 128
        or aggregate.get("train_roots") != 128
        or aggregate.get("selection_roots") != 64
        or aggregate.get("heldout_roots") != 64
    ):
        _fail("counterfactual corpus is not qualified")

    examples: list[dict[str, Any]] = []
    root_examples: list[dict[str, Any]] = []
    observed_source_episodes: set[int] = set()
    for root in report.get("roots", []):
        source_episode = int(root["source_episode_ordinal"])
        if source_episode in observed_source_episodes:
            _fail("counterfactual corpus repeats a source episode")
        observed_source_episodes.add(source_episode)
        legal_count = int(root["legal_action_count"])
        parent = int(root["parent_argmax_index"])
        rewards = np.asarray(root["action_terminal_rewards"], dtype=np.float64)
        if (
            rewards.shape != (legal_count, 4)
            or np.any(rewards < -1)
            or np.any(rewards > 1)
            or not np.array_equal(rewards, rewards.astype(np.int64))
            or not 0 <= parent < legal_count
            or len(root["action_semantics"]) != legal_count
            or len(root["parent_logits_f32_bits"]) != legal_count
            or len(root["samples"]) != 4
            or len({sample["sampled_privileged_state_hash_u64_hex"] for sample in root["samples"]})
            != 4
        ):
            _fail(f"counterfactual root matrix or identity is invalid: {root.get('root_ordinal')}")

        for history in root["public_history"]:
            history_example = _parse_row(
                root,
                history,
                int(history["selected_index"]),
                history["selected_semantic"],
                history["acting_player"],
                int(history["step"]),
                int(history["physical_decision_id"]),
                int(history["substep_index"]),
                int(history["substep_count"]),
            )
            examples.append(history_example)
        root_example = _parse_row(
            root,
            root,
            parent,
            root["action_semantics"][parent],
            root["acting_player"],
            int(root["step"]),
            int(root["physical_decision_id"]),
            0,
            1,
        )
        root_example["counterfactual_root"] = root
        examples.append(root_example)
        root_examples.append(root_example)

    structured._attach_complete_action_history(  # noqa: SLF001
        examples, [], distill.HISTORY_LENGTH, distill.CARD_VOCAB
    )
    if len(root_examples) != 256 or any(
        int(example["history_features"].shape[0]) > distill.HISTORY_LENGTH
        for example in root_examples
    ):
        _fail("counterfactual root history attachment failed")
    return root_examples, {
        "path": str(path),
        "sha256": _sha256(path),
        "roots": len(root_examples),
        "source_episodes": len(observed_source_episodes),
        "aggregate": aggregate,
        "gates": gates,
    }


def _load_initializer(path: Path) -> tuple[Any, dict[str, Any]]:
    observed_sha256 = _sha256(path)
    if observed_sha256 != INITIALIZER_STATE_SHA256:
        _fail("structured initializer state SHA-256 mismatch")
    payload = torch.load(path, map_location="cpu", weights_only=False)
    state = payload.get("model_state_dict")
    if not isinstance(state, dict):
        _fail("structured initializer lacks model_state_dict")
    model = distill._model()
    model.load_state_dict(state, strict=True)
    model.eval()
    for parameter in model.parameters():
        parameter.requires_grad_(False)
    return model, {
        "path": str(path),
        "sha256": observed_sha256,
        "architecture": "qualified-complete-public-history-structured-successor/v1",
        "frozen": True,
    }


def _extract_roots(model: Any, examples: list[dict[str, Any]]) -> list[dict[str, Any]]:
    captured: list[torch.Tensor] = []

    def capture(_module: Any, _inputs: Any, output: torch.Tensor) -> None:
        captured.append(output.detach().cpu())

    hook = model.combine.register_forward_hook(capture)
    extracted = []
    try:
        with torch.no_grad():
            for example in examples:
                captured.clear()
                model._one(example)  # noqa: SLF001
                if len(captured) != 1:
                    _fail("structured joint representation hook did not fire exactly once")
                joint = captured[0].numpy().astype(np.float64, copy=True)
                root = example["counterfactual_root"]
                parent = int(root["parent_argmax_index"])
                logits = example["old_logits"].numpy().astype(np.float64, copy=True)
                rewards = np.asarray(root["action_terminal_rewards"], dtype=np.float64)
                mean_rewards = rewards.mean(axis=1)
                features = np.concatenate(
                    (
                        joint - joint[parent],
                        (logits - logits[parent]).reshape(-1, 1),
                    ),
                    axis=1,
                )
                extracted.append(
                    {
                        "root_ordinal": int(root["root_ordinal"]),
                        "pair_index": int(root["balance_pair_index"]),
                        "seat": 0 if root["acting_player"] == "p0" else 1,
                        "parent": parent,
                        "features": features,
                        "rewards": rewards,
                        "mean_rewards": mean_rewards,
                        "targets": mean_rewards - mean_rewards[parent],
                    }
                )
    finally:
        hook.remove()
    return extracted


def _split(roots: list[dict[str, Any]]) -> tuple[list[Any], list[Any], list[Any]]:
    train = [root for root in roots if root["pair_index"] % 4 in (1, 2)]
    selection = [root for root in roots if root["pair_index"] % 4 == 3]
    heldout = [root for root in roots if root["pair_index"] % 4 == 0]
    if [len(train), len(selection), len(heldout)] != [128, 64, 64]:
        _fail("counterfactual split is not exactly 128/64/64")
    for split_roots in (train, selection, heldout):
        if [sum(root["seat"] == seat for root in split_roots) for seat in (0, 1)] != [
            len(split_roots) // 2,
            len(split_roots) // 2,
        ]:
            _fail("counterfactual split is not acting-seat balanced")
    return train, selection, heldout


def _feature_scale(train: list[dict[str, Any]]) -> np.ndarray:
    rows = np.concatenate(
        [
            np.delete(root["features"], root["parent"], axis=0)
            for root in train
        ],
        axis=0,
    )
    scale = np.sqrt(np.mean(np.square(rows), axis=0))
    scale[scale < 1.0e-6] = 1.0
    return scale


def _fit_ridge(
    train: list[dict[str, Any]], scale: np.ndarray, ridge_lambda: float
) -> np.ndarray:
    feature_rows = []
    targets = []
    weights = []
    for root in train:
        alternatives = [index for index in range(len(root["targets"])) if index != root["parent"]]
        root_weight = 1.0 / len(alternatives)
        for index in alternatives:
            feature_rows.append(root["features"][index] / scale)
            targets.append(root["targets"][index])
            weights.append(root_weight)
    x = np.asarray(feature_rows, dtype=np.float64)
    y = np.asarray(targets, dtype=np.float64)
    sqrt_weight = np.sqrt(np.asarray(weights, dtype=np.float64))
    xw = x * sqrt_weight[:, None]
    yw = y * sqrt_weight
    gram = xw.T @ xw + ridge_lambda * np.eye(xw.shape[1], dtype=np.float64)
    return np.linalg.solve(gram, xw.T @ yw)


def _choices(
    roots: list[dict[str, Any]], weights: np.ndarray, scale: np.ndarray, margin: float
) -> tuple[list[int], list[np.ndarray]]:
    choices = []
    predictions = []
    for root in roots:
        scores = (root["features"] / scale) @ weights
        scores[root["parent"]] = 0.0
        alternatives = [index for index in range(len(scores)) if index != root["parent"]]
        best_alternative = max(alternatives, key=lambda index: (scores[index], -index))
        choice = best_alternative if scores[best_alternative] > margin else root["parent"]
        choices.append(choice)
        predictions.append(scores)
    return choices, predictions


def _bootstrap_lower(root_uplifts: np.ndarray) -> float:
    rng = np.random.default_rng(BOOTSTRAP_SEED)
    indices = rng.integers(
        0, len(root_uplifts), size=(BOOTSTRAP_REPLICATES, len(root_uplifts))
    )
    means = root_uplifts[indices].mean(axis=1)
    return float(np.quantile(means, 0.05, method="lower"))


def _metrics(
    roots: list[dict[str, Any]],
    choices: list[int],
    predictions: list[np.ndarray],
    bootstrap: bool,
) -> dict[str, Any]:
    root_uplifts = np.asarray(
        [
            root["mean_rewards"][choice] - root["mean_rewards"][root["parent"]]
            for root, choice in zip(roots, choices)
        ],
        dtype=np.float64,
    )
    paired = np.concatenate(
        [
            root["rewards"][choice] - root["rewards"][root["parent"]]
            for root, choice in zip(roots, choices)
        ]
    )
    changed = sum(choice != root["parent"] for root, choice in zip(roots, choices))
    action_range_count = sum(
        float(root["mean_rewards"].max() - root["mean_rewards"].min()) >= 0.5
        for root in roots
    )
    parent_not_best_count = sum(
        root["mean_rewards"][root["parent"]] < root["mean_rewards"].max()
        for root in roots
    )

    decisive = []
    for index, root in enumerate(roots):
        means = root["mean_rewards"]
        order = np.argsort(-means, kind="stable")
        best = int(order[0])
        if np.count_nonzero(means == means[best]) == 1 and means[best] - means[order[1]] >= 0.5:
            decisive.append((index, best))
    scorer_decisive_accuracy = (
        sum(choices[index] == best for index, best in decisive) / len(decisive)
        if decisive
        else 0.0
    )
    parent_decisive_accuracy = (
        sum(roots[index]["parent"] == best for index, best in decisive) / len(decisive)
        if decisive
        else 0.0
    )

    squared_errors = []
    pairwise_wrong = 0
    pairwise_total = 0
    for root, prediction in zip(roots, predictions):
        squared_errors.extend(np.square(prediction - root["targets"]).tolist())
        for left in range(len(prediction)):
            for right in range(left + 1, len(prediction)):
                target_delta = root["mean_rewards"][left] - root["mean_rewards"][right]
                if target_delta == 0:
                    continue
                predicted_delta = prediction[left] - prediction[right]
                pairwise_total += 1
                pairwise_wrong += int(predicted_delta * target_delta <= 0)

    by_seat = {}
    for seat in (0, 1):
        indices = [index for index, root in enumerate(roots) if root["seat"] == seat]
        by_seat[f"p{seat}"] = {
            "roots": len(indices),
            "mean_root_terminal_reward_uplift": float(root_uplifts[indices].mean()),
            "changed_roots": sum(choices[index] != roots[index]["parent"] for index in indices),
        }
    return {
        "roots": len(roots),
        "mean_root_terminal_reward_uplift": float(root_uplifts.mean()),
        "paired_branch_comparisons": {
            "better": int(np.count_nonzero(paired > 0)),
            "worse": int(np.count_nonzero(paired < 0)),
            "equal": int(np.count_nonzero(paired == 0)),
            "reward_sum": float(paired.sum()),
        },
        "changed_roots": changed,
        "root_centered_rmse": float(np.sqrt(np.mean(squared_errors))),
        "pairwise_ranking_loss": pairwise_wrong / max(pairwise_total, 1),
        "pairwise_ranking_comparisons": pairwise_total,
        "label_adequacy": {
            "action_range_at_least_0p5_roots": action_range_count,
            "parent_not_empirically_best_roots": parent_not_best_count,
        },
        "decisive_roots": {
            "count": len(decisive),
            "scorer_top1_accuracy": scorer_decisive_accuracy,
            "parent_top1_accuracy": parent_decisive_accuracy,
            "top1_accuracy_delta": scorer_decisive_accuracy - parent_decisive_accuracy,
        },
        "by_acting_seat": by_seat,
        "one_sided_paired_bootstrap_95_percent_lower": (
            _bootstrap_lower(root_uplifts) if bootstrap else None
        ),
    }


def _select(
    train: list[dict[str, Any]], selection: list[dict[str, Any]], scale: np.ndarray
) -> tuple[np.ndarray, float, float, list[dict[str, Any]]]:
    candidates = []
    for ridge_lambda in RIDGE_LAMBDAS:
        weights = _fit_ridge(train, scale, ridge_lambda)
        for margin in DEPLOYMENT_MARGINS:
            choices, predictions = _choices(selection, weights, scale, margin)
            metrics = _metrics(selection, choices, predictions, bootstrap=False)
            candidates.append(
                {
                    "ridge_lambda": ridge_lambda,
                    "deployment_margin": margin,
                    "weight_l2_norm": float(np.linalg.norm(weights)),
                    "metrics": metrics,
                    "weights": weights,
                }
            )
    selected = max(
        candidates,
        key=lambda row: (
            row["metrics"]["mean_root_terminal_reward_uplift"],
            row["metrics"]["changed_roots"],
            -row["weight_l2_norm"],
            row["ridge_lambda"],
            row["deployment_margin"],
        ),
    )
    serializable = [
        {key: value for key, value in candidate.items() if key != "weights"}
        for candidate in candidates
    ]
    return (
        selected["weights"],
        selected["ridge_lambda"],
        selected["deployment_margin"],
        serializable,
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--initializer-state", type=Path, required=True)
    parser.add_argument("--output-json", type=Path, required=True)
    parser.add_argument("--threads", type=int, default=8)
    args = parser.parse_args()
    started = time.perf_counter()
    torch.set_num_threads(max(1, args.threads))
    torch.use_deterministic_algorithms(True)

    examples, corpus_identity = _load_corpus(args.corpus)
    model, initializer_identity = _load_initializer(args.initializer_state)
    roots = _extract_roots(model, examples)
    train, selection, heldout = _split(roots)
    scale = _feature_scale(train)
    weights, ridge_lambda, margin, selection_grid = _select(train, selection, scale)

    train_choices, train_predictions = _choices(train, weights, scale, margin)
    selection_choices, selection_predictions = _choices(selection, weights, scale, margin)
    heldout_choices, heldout_predictions = _choices(heldout, weights, scale, margin)
    train_metrics = _metrics(train, train_choices, train_predictions, bootstrap=False)
    selection_metrics = _metrics(
        selection, selection_choices, selection_predictions, bootstrap=False
    )
    heldout_metrics = _metrics(
        heldout, heldout_choices, heldout_predictions, bootstrap=True
    )

    adequacy = heldout_metrics["label_adequacy"]
    decisive = heldout_metrics["decisive_roots"]
    seats = heldout_metrics["by_acting_seat"]
    gates = {
        "corpus_and_split_integrity": True,
        "at_least_16_heldout_roots_with_action_range_at_least_0p5": adequacy[
            "action_range_at_least_0p5_roots"
        ]
        >= 16,
        "at_least_10_heldout_roots_where_parent_is_not_empirically_best": adequacy[
            "parent_not_empirically_best_roots"
        ]
        >= 10,
        "scorer_changes_at_least_8_heldout_roots": heldout_metrics["changed_roots"] >= 8,
        "heldout_mean_root_uplift_at_least_0p125": heldout_metrics[
            "mean_root_terminal_reward_uplift"
        ]
        >= 0.125,
        "heldout_one_sided_bootstrap_lower_above_zero": heldout_metrics[
            "one_sided_paired_bootstrap_95_percent_lower"
        ]
        > 0.0,
        "heldout_uplift_nonnegative_both_acting_seats": all(
            seats[seat]["mean_root_terminal_reward_uplift"] >= 0.0
            for seat in ("p0", "p1")
        ),
        "at_least_20_decisive_heldout_roots": decisive["count"] >= 20,
        "decisive_top1_accuracy_delta_at_least_0p10": decisive[
            "top1_accuracy_delta"
        ]
        >= 0.10,
    }
    gates["mechanism_screen_pass"] = all(gates.values())
    report = {
        "schema": FIT_SCHEMA,
        "status": "pass" if gates["mechanism_screen_pass"] else "reject",
        "git_commit": _git_head(),
        "corpus": corpus_identity,
        "initializer": initializer_identity,
        "architecture": {
            "representation": "frozen-structured-joint-48-plus-parent-logit-delta/v1",
            "head": "root-centered-ridge-linear-action-value/v1",
            "feature_count": int(weights.shape[0]),
            "root_weighting": "equal-across-roots-and-equal-across-nonparent-actions-within-root",
            "target": "mean-of-four-actor-relative-natural-terminal-rewards-minus-parent-mean",
            "reward_or_success_measure": "natural-terminal-win-loss-draw-only",
        },
        "split": {
            "method": "balance_pair_index-modulo-4/v1",
            "train_roots": 128,
            "selection_roots": 64,
            "heldout_roots": 64,
            "heldout_touched_after_selection": True,
        },
        "selection": {
            "ridge_lambdas": RIDGE_LAMBDAS,
            "deployment_margins": DEPLOYMENT_MARGINS,
            "selected_ridge_lambda": ridge_lambda,
            "selected_deployment_margin": margin,
            "grid": selection_grid,
        },
        "trained_head": {
            "feature_rms_scale": scale.tolist(),
            "weights": weights.tolist(),
            "weight_l2_norm": float(np.linalg.norm(weights)),
        },
        "metrics": {
            "train": train_metrics,
            "selection": selection_metrics,
            "heldout": heldout_metrics,
        },
        "gates": gates,
        "bootstrap": {
            "seed": BOOTSTRAP_SEED,
            "replicates": BOOTSTRAP_REPLICATES,
            "unit": "heldout-root",
            "lower_quantile": 0.05,
        },
        "elapsed_seconds": time.perf_counter() - started,
        "interpretation": (
            "Pass authorizes a complete fresh-seed confirmation corpus only."
            if gates["mechanism_screen_pass"]
            else "This four-sample frozen-linear screen found no robust transferable action-value signal."
        ),
        "nonclaims": [
            "No candidate was packaged.",
            "No natural-terminal strength gate was run.",
            "Retained-policy one-step deviation values do not establish safe repeated deployment.",
        ],
    }
    _write_new(args.output_json, report)
    print(
        json.dumps(
            {
                "status": report["status"],
                "heldout_mean_uplift": heldout_metrics[
                    "mean_root_terminal_reward_uplift"
                ],
                "heldout_bootstrap_lower": heldout_metrics[
                    "one_sided_paired_bootstrap_95_percent_lower"
                ],
                "heldout_changed_roots": heldout_metrics["changed_roots"],
                "output": str(args.output_json),
            },
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
