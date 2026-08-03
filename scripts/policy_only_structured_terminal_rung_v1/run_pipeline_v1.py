#!/usr/bin/env python3
"""Collect, cache, fit, and publish the terminal-only structured policy rung."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import math
import os
from pathlib import Path
import random
import shutil
import sys
import time
from types import SimpleNamespace
from typing import Any

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
SCRIPTS_DIR = SCRIPT_DIR.parent
STRUCTURED_DIR = SCRIPTS_DIR / "structured_adapter_screen_v1"
POPULATION_DIR = SCRIPTS_DIR / "native_population_structured_v1"
QUALIFICATION_DIR = SCRIPTS_DIR / "policy_only_structured_successor_v1"
for directory in (STRUCTURED_DIR, POPULATION_DIR, QUALIFICATION_DIR):
    sys.path.insert(0, str(directory))

import collect_corpus_v1 as collector  # noqa: E402
import fit_complete_history_live_candidate_v1 as history_publish  # noqa: E402
import fit_policy_live_candidate as live  # noqa: E402
import fit_policy_only_structured_successor_v1 as initializer  # noqa: E402
import run_matched_gate_v1 as qualification  # noqa: E402
import run_screen as screen  # noqa: E402
import run_structured_outcome_policy_v1 as outcome  # noqa: E402
import run_structured_successor_distillation_v1 as distill  # noqa: E402


COLLECTION_SCHEMA = "mtg-kernel-structured-policy-terminal-rung-collection/v1"
CACHE_SCHEMA = "mtg-kernel-structured-policy-terminal-rung-cache/v1"
FIT_SCHEMA = "mtg-kernel-structured-policy-terminal-rung-fit/v1"
MODEL_STATE_SCHEMA = FIT_SCHEMA + ".model-state"
CANDIDATE_SCHEMA = "mtg-kernel-structured-policy-successor-candidate/v2"
REPORT_SCHEMA = "mtg-kernel-structured-policy-terminal-rung-report/v1"
PARITY_SCHEMA = "mtg-kernel-structured-policy-terminal-rung-parity-fixture/v1"
ARCHITECTURE = (
    "complete-public-history-structured-policy-terminal-rung-frozen-parent-value/v1"
)
VALUE_MODEL = initializer.VALUE_MODEL
COMPOSITE_DOMAIN = b"mtg-kernel-structured-policy-terminal-rung-composite-model/v1"
CANDIDATE_FILENAME = initializer.CANDIDATE_FILENAME

FORMAL_BASE_SEED = 1_660_001
FORMAL_PAIRS = 2_048
FORMAL_SHARDS = 4
FORMAL_SHARD_PAIRS = 512
FIT_SEED = 20_260_805
FIT_EPOCHS = 5
BATCH_SIZE = 64
LR = 3.0e-4
WEIGHT_DECAY = 0.0
CLIP = 0.10
GRAD_CAP = 5.0
THREAD_CHOICES = (12, 24)
MEAN_TV_LIMIT = 0.030
P90_TV_LIMIT = 0.100
MAX_JOINT_LOG_RATIO = 0.50
TRANSPORT_LIMIT = 3.0e-5
PROVISIONAL_TRANSPORT_ERROR = 1.0

INITIALIZER_CANDIDATE_SHA256 = (
    "204beb91c1a4b039e0c497f2b420e823b5cc9e2ceb8560f897d0b6251e916b72"
)
INITIALIZER_STATE_SHA256 = (
    "ff2abf50e8760780a9331e53aa7323cb96e3c64edb6e7d89062dbe38bf6a5cc0"
)
POOL_SHA256 = qualification.POOL_CONTRACT_SHA256
SCORER_SHA256 = (
    "b3161bf6df8eccdc0afb8d8870eeb81b7620add347fd531b8e5f8869d8205d81"
)


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _write_new_json(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(history_publish._json_bytes(value))


def _validate_fixed_inputs(
    scorer: Path, initializer_root: Path, pool_root: Path
) -> dict[str, Any]:
    if _sha256(scorer) != SCORER_SHA256:
        _fail("native population scorer SHA-256 mismatch")
    if _sha256(pool_root / "pool.json") != POOL_SHA256:
        _fail("Pool3 contract SHA-256 mismatch")
    identity = qualification._candidate_identity(initializer_root)
    if identity["candidate_json_sha256"] != INITIALIZER_CANDIDATE_SHA256:
        _fail("qualified initializer candidate SHA-256 mismatch")
    return identity


def _shard_ranges(pair_count: int, shard_count: int) -> list[tuple[int, int]]:
    if pair_count < 1 or shard_count < 1 or shard_count > pair_count:
        _fail("invalid collection shard topology")
    base, extra = divmod(pair_count, shard_count)
    ranges: list[tuple[int, int]] = []
    first = 0
    for ordinal in range(shard_count):
        count = base + int(ordinal < extra)
        ranges.append((first, count))
        first += count
    if first != pair_count:
        _fail("collection shard topology does not cover the panel")
    return ranges


def _collect_one(job: dict[str, Any]) -> dict[str, Any]:
    root = Path(job["root"])
    prefix = root / f"shard-{job['ordinal']:02d}"
    args = SimpleNamespace(
        scorer=Path(job["scorer"]),
        candidate_root=Path(job["initializer_root"]),
        pool_root=Path(job["pool_root"]),
        teacher_jsonl=prefix.with_suffix(".teacher.jsonl"),
        outcome_jsonl=prefix.with_suffix(".outcome.jsonl"),
        output=prefix.with_suffix(".collection.json"),
        base_seed=int(job["base_seed"]),
        pair_start=int(job["pair_start"]),
        pairs=int(job["pairs"]),
    )
    if args.output.exists():
        if not args.teacher_jsonl.is_file() or not args.outcome_jsonl.is_file():
            _fail("completed shard report lacks its trajectory streams")
        report = json.loads(args.output.read_text(encoding="utf-8"))
        if (
            report.get("teacher_sha256") != _sha256(args.teacher_jsonl)
            or report.get("outcome_sha256") != _sha256(args.outcome_jsonl)
        ):
            _fail("completed shard stream SHA-256 mismatch")
    else:
        if args.teacher_jsonl.exists() or args.outcome_jsonl.exists():
            _fail("partial collection shard cannot be resumed")
        report = collector.collect(args)
    return {
        "ordinal": int(job["ordinal"]),
        "pair_start": args.pair_start,
        "pairs": args.pairs,
        "report_path": str(args.output),
        "report_sha256": _sha256(args.output),
        "report": report,
    }


def collect(args: argparse.Namespace) -> dict[str, Any]:
    if args.output.exists():
        _fail("collection output already exists")
    formal = args.profile_pairs is None
    pair_count = FORMAL_PAIRS if formal else args.profile_pairs
    if pair_count is None or pair_count < 2 or pair_count > 64:
        _fail("profile pairs must be between 2 and 64")
    if formal and (
        args.base_seed != FORMAL_BASE_SEED or args.shards != FORMAL_SHARDS
    ):
        _fail("formal collection seed and topology are fixed")
    shard_count = min(args.shards, pair_count)
    identity = _validate_fixed_inputs(
        args.scorer, args.initializer_root, args.pool_root
    )
    args.collection_root.mkdir(parents=True, exist_ok=True)
    jobs = []
    for ordinal, (first, count) in enumerate(
        _shard_ranges(pair_count, shard_count)
    ):
        jobs.append(
            {
                "ordinal": ordinal,
                "pair_start": first,
                "pairs": count,
                "root": str(args.collection_root),
                "scorer": str(args.scorer),
                "initializer_root": str(args.initializer_root),
                "pool_root": str(args.pool_root),
                "base_seed": args.base_seed,
            }
        )
    started = time.perf_counter()
    results: list[dict[str, Any]] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=shard_count) as executor:
        futures = [executor.submit(_collect_one, job) for job in jobs]
        for future in concurrent.futures.as_completed(futures):
            results.append(future.result())
    results.sort(key=lambda value: value["ordinal"])
    orchestration_elapsed = time.perf_counter() - started
    covered = {
        pair
        for result in results
        for pair in range(result["pair_start"], result["pair_start"] + result["pairs"])
    }
    if covered != set(range(pair_count)):
        _fail("collection shards do not exactly cover the requested pairs")
    for result in results:
        report = result["report"]
        if (
            report.get("base_seed") != args.base_seed
            or report.get("pair_start") != result["pair_start"]
            or report.get("pairs") != result["pairs"]
            or report.get("episodes") != 2 * result["pairs"]
            or report.get("scorer_stderr")
            != [
                "NATIVE_POPULATION_CORPUS "
                f"pool_root={args.pool_root} weights=40,20,20,20"
            ]
        ):
            _fail("collection shard report mismatch")
    measurement_elapsed = max(
        float(result["report"]["elapsed_seconds"]) for result in results
    )
    output = {
        "schema": COLLECTION_SCHEMA,
        "status": "pass",
        "formal": formal,
        "base_seed": args.base_seed,
        "pair_count": pair_count,
        "episode_count": pair_count * 2,
        "shard_count": shard_count,
        "topology": "parallel-persistent-native-scorers",
        "measurement_elapsed_seconds": measurement_elapsed,
        "orchestration_elapsed_seconds": orchestration_elapsed,
        "games_per_second": (pair_count * 2) / measurement_elapsed,
        "initializer_identity": identity,
        "scorer_sha256": SCORER_SHA256,
        "pool_json_sha256": POOL_SHA256,
        "shards": results,
    }
    _write_new_json(args.output, output)
    return output


def _prepare_cache_shard(job: dict[str, Any]) -> dict[str, Any]:
    torch.set_num_threads(1)
    path = Path(job["cache"])
    result = screen.prepare_cache(
        Path(job["teacher"]),
        Path(job["outcome"]),
        path,
        job["teacher_sha256"],
        job["outcome_sha256"],
        True,
    )
    return {
        "ordinal": int(job["ordinal"]),
        "pair_start": int(job["pair_start"]),
        "pairs": int(job["pairs"]),
        "cache": str(path),
        "cache_sha256": _sha256(path),
        **result,
    }


def _merge_join(target: dict[str, Any], source: dict[str, Any]) -> None:
    for key in (
        "episode_count",
        "pair_count",
        "policy_step_count",
        "physical_decision_count",
    ):
        target[key] += int(source[key])
    for kind, count in source["selected_action_kind_counts"].items():
        target["selected_action_kind_counts"][kind] = (
            target["selected_action_kind_counts"].get(kind, 0) + int(count)
        )
    for key in (
        "selected_semantics_public",
        "terminal_replays_exact",
        "complete_policy_steps",
        "complete_physical_decisions",
    ):
        target[key] = target[key] and source.get(key) is True


def prepare_cache(args: argparse.Namespace) -> dict[str, Any]:
    if args.cache.exists() or args.output.exists() or args.shard_cache_root.exists():
        _fail("cache output already exists")
    collection = json.loads(args.collection.read_text(encoding="utf-8"))
    if collection.get("schema") != COLLECTION_SCHEMA or collection.get("status") != "pass":
        _fail("collection report is not passing")
    shards = collection.get("shards")
    if not isinstance(shards, list) or not shards:
        _fail("collection report has no shards")
    args.shard_cache_root.mkdir(parents=True)
    jobs: list[dict[str, Any]] = []
    for result in shards:
        report = result["report"]
        jobs.append(
            {
                "ordinal": result["ordinal"],
                "pair_start": result["pair_start"],
                "pairs": result["pairs"],
                "teacher": report["teacher_jsonl"],
                "teacher_sha256": report["teacher_sha256"],
                "outcome": report["outcome_jsonl"],
                "outcome_sha256": report["outcome_sha256"],
                "cache": str(
                    args.shard_cache_root / f"shard-{result['ordinal']:02d}.pt"
                ),
            }
        )
    started = time.perf_counter()
    reports: list[dict[str, Any]] = []
    with concurrent.futures.ProcessPoolExecutor(max_workers=len(jobs)) as executor:
        futures = [executor.submit(_prepare_cache_shard, job) for job in jobs]
        for future in concurrent.futures.as_completed(futures):
            reports.append(future.result())
    reports.sort(key=lambda value: value["ordinal"])

    policy: list[dict[str, Any]] = []
    value: list[dict[str, Any]] = []
    card_max = 0
    group_max = 0
    complete_join: dict[str, Any] = {
        "episode_count": 0,
        "pair_count": 0,
        "policy_step_count": 0,
        "physical_decision_count": 0,
        "selected_action_kind_counts": {},
        "selected_semantics_public": True,
        "terminal_replays_exact": True,
        "complete_policy_steps": True,
        "complete_physical_decisions": True,
    }
    for report in reports:
        shard_path = Path(report["cache"])
        if _sha256(shard_path) != report["cache_sha256"]:
            _fail("shard cache SHA-256 mismatch")
        payload = torch.load(shard_path, map_location="cpu", weights_only=False)
        if (
            payload.get("version") != screen.SCRIPT_VERSION
            or not payload.get("complete_history_join")
        ):
            _fail("invalid complete-history shard cache")
        policy.extend(payload["policy"])
        value.extend(payload["value"])
        card_max = max(card_max, int(payload["card_max"]))
        group_max = max(group_max, int(payload["group_max"]))
        _merge_join(complete_join, payload["complete_history_join"])
    expected_pairs = set(range(int(collection["pair_count"])))
    if (
        {int(row["pair_index"]) for row in policy} != expected_pairs
        or {int(row["pair_index"]) for row in value} != expected_pairs
        or complete_join["pair_count"] != collection["pair_count"]
        or complete_join["episode_count"] != collection["episode_count"]
        or not all(
            complete_join[key]
            for key in (
                "selected_semantics_public",
                "terminal_replays_exact",
                "complete_policy_steps",
                "complete_physical_decisions",
            )
        )
    ):
        _fail("merged complete-history cache coverage mismatch")
    complete_join["selected_action_kind_counts"] = dict(
        sorted(complete_join["selected_action_kind_counts"].items())
    )
    payload = {
        "version": screen.SCRIPT_VERSION,
        "schema": CACHE_SCHEMA,
        "policy": policy,
        "value": value,
        "card_max": card_max,
        "group_max": group_max,
        "complete_history_join": complete_join,
        "source": {
            "collection": str(args.collection),
            "collection_sha256": _sha256(args.collection),
            "formal": collection["formal"],
            "base_seed": collection["base_seed"],
            "pair_count": collection["pair_count"],
            "initializer_identity": collection["initializer_identity"],
            "pool_json_sha256": collection["pool_json_sha256"],
            "shards": [
                {
                    "ordinal": report["ordinal"],
                    "pair_start": report["pair_start"],
                    "pairs": report["pairs"],
                    "cache_sha256": report["cache_sha256"],
                }
                for report in reports
            ],
        },
    }
    args.cache.parent.mkdir(parents=True, exist_ok=True)
    temporary = args.cache.with_suffix(args.cache.suffix + ".tmp")
    torch.save(payload, temporary)
    os.replace(temporary, args.cache)
    result = {
        "schema": CACHE_SCHEMA + ".result",
        "status": "pass",
        "cache": str(args.cache),
        "cache_sha256": _sha256(args.cache),
        "pair_count": collection["pair_count"],
        "episode_count": collection["episode_count"],
        "policy_examples": len(policy),
        "value_examples": len(value),
        "complete_history_join": complete_join,
        "elapsed_seconds": time.perf_counter() - started,
        "shards": reports,
    }
    _write_new_json(args.output, result)
    return result


def _load_decisions(
    cache_path: Path, pair_limit: int | None
) -> tuple[list[Any], dict[str, Any], dict[str, float]]:
    started = time.perf_counter()
    cache_sha256 = _sha256(cache_path)
    cache = torch.load(cache_path, map_location="cpu", weights_only=False)
    loaded = time.perf_counter()
    if (
        cache.get("version") != screen.SCRIPT_VERSION
        or cache.get("schema") != CACHE_SCHEMA
        or not cache.get("complete_history_join")
    ):
        _fail("cache is not a terminal-rung complete-history cache")
    source = cache.get("source", {})
    if (
        source.get("initializer_identity", {}).get("candidate_json_sha256")
        != INITIALIZER_CANDIDATE_SHA256
        or source.get("pool_json_sha256") != POOL_SHA256
    ):
        _fail("cache initializer or Pool3 identity mismatch")
    policy = cache["policy"]
    value = cache["value"]
    pair_indices = sorted({int(row["pair_index"]) for row in value})
    if pair_indices != list(range(int(source["pair_count"]))):
        _fail("cache pair panel is not exact")
    selected_pairs = pair_indices
    if pair_limit is not None:
        if pair_limit < 2 or pair_limit > len(pair_indices):
            _fail("pair limit is outside the cache panel")
        selected_pairs = pair_indices[:pair_limit]
        selected = set(selected_pairs)
        policy = [row for row in policy if int(row["pair_index"]) in selected]
        value = [row for row in value if int(row["pair_index"]) in selected]
    screen._attach_complete_action_history(
        policy, value, distill.HISTORY_LENGTH, distill.CARD_VOCAB
    )
    attached = time.perf_counter()
    decisions = outcome._physical_decisions(value)
    grouped = time.perf_counter()
    metadata = {
        "cache": str(cache_path),
        "cache_sha256": cache_sha256,
        "base_seed": source["base_seed"],
        "pair_count": len(selected_pairs),
        "episode_count": len({decision.episode_key for decision in decisions}),
        "physical_decision_count": len(decisions),
        "row_count": sum(len(decision.rows) for decision in decisions),
        "initializer_identity": source["initializer_identity"],
        "pool_json_sha256": source["pool_json_sha256"],
    }
    timings = {
        "hash_and_load_seconds": loaded - started,
        "attach_history_seconds": attached - loaded,
        "group_decisions_seconds": grouped - attached,
    }
    return decisions, metadata, timings


def _absolute_joint_log_probability(
    model: screen.StructuredAdapter, decision: Any
) -> torch.Tensor:
    terms = []
    for row in decision.rows:
        logits, _ = model._one(row)
        terms.append(torch.log_softmax(logits, dim=0)[int(row["selected_index"])])
    return torch.stack(terms).sum()


def _fit_model(
    model: screen.StructuredAdapter,
    decisions: list[Any],
    epochs: int,
) -> list[dict[str, Any]]:
    statistics = outcome._advantage_statistics(decisions)
    outcome._install_standardized_advantages(decisions, statistics)
    parameters = initializer._policy_parameters(model)
    value_bits = initializer._value_head_bits(model)
    optimizer = torch.optim.AdamW(
        parameters, lr=LR, weight_decay=WEIGHT_DECAY
    )
    rng = random.Random(FIT_SEED)
    episode_mass = sum(decision.episode_weight for decision in decisions)
    weights = {
        decision.key: decision.episode_weight * len(decisions) / episode_mass
        for decision in decisions
    }
    history: list[dict[str, Any]] = []
    for epoch in range(epochs):
        order = list(range(len(decisions)))
        rng.shuffle(order)
        loss_total = 0.0
        clip_total = 0.0
        gradient_norm_max = 0.0
        steps = 0
        model.train()
        for start in range(0, len(order), BATCH_SIZE):
            batch = [decisions[index] for index in order[start : start + BATCH_SIZE]]
            surrogates = []
            masses = []
            clipped = 0
            for decision in batch:
                joint = _absolute_joint_log_probability(model, decision)
                log_ratio = joint - decision.old_joint_log_probability
                ratio = torch.exp(log_ratio)
                clipped_ratio = torch.clamp(ratio, 1.0 - CLIP, 1.0 + CLIP)
                advantage = decision.standardized_advantage
                surrogates.append(
                    torch.minimum(ratio * advantage, clipped_ratio * advantage)
                )
                masses.append(weights[decision.key])
                clipped += int(
                    abs(float(log_ratio.detach())) > math.log1p(CLIP)
                )
            mass_tensor = torch.tensor(masses, dtype=torch.float32)
            loss = -(torch.stack(surrogates) * mass_tensor).sum() / mass_tensor.sum()
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            gradient_norm = torch.nn.utils.clip_grad_norm_(parameters, GRAD_CAP)
            if not torch.isfinite(gradient_norm):
                _fail("non-finite terminal-rung gradient")
            optimizer.step()
            loss_total += float(loss.detach())
            clip_total += clipped / len(batch)
            gradient_norm_max = max(gradient_norm_max, float(gradient_norm))
            steps += 1
        history.append(
            {
                "epoch": epoch + 1,
                "mean_minibatch_loss": loss_total / max(steps, 1),
                "mean_minibatch_clip_fraction": clip_total / max(steps, 1),
                "maximum_preclip_gradient_norm": gradient_norm_max,
                "optimizer_steps": steps,
            }
        )
    if initializer._value_head_bits(model) != value_bits:
        _fail("frozen value head changed during terminal-only fit")
    return history


def _alignment(
    model: screen.StructuredAdapter, decisions: list[Any], sample_size: int = 256
) -> dict[str, Any]:
    rng = random.Random(FIT_SEED)
    sampled = (
        decisions
        if len(decisions) <= sample_size
        else [decisions[index] for index in sorted(rng.sample(range(len(decisions)), sample_size))]
    )
    maximum = 0.0
    rows = 0
    model.eval()
    with torch.no_grad():
        for decision in sampled:
            for row in decision.rows:
                logits, value_residual = model._one(row)
                maximum = max(
                    maximum, float((logits - row["old_logits"]).abs().max())
                )
                if value_residual.detach().float().numpy().tobytes() != b"\x00\x00\x00\x00":
                    _fail("initializer value residual is not exact zero")
                rows += 1
    return {
        "sampled_physical_decisions": len(sampled),
        "sampled_rows": rows,
        "maximum_absolute_behavior_logit_error": maximum,
        "pass": maximum <= TRANSPORT_LIMIT,
    }


def _empty_movement() -> dict[str, Any]:
    return {
        "tv_sum": 0.0,
        "kl_sum": 0.0,
        "mass": 0.0,
        "top_sum": 0.0,
        "rows": 0,
        "samples": [],
        "max_joint": 0.0,
        "decisions": 0,
    }


def _finish_movement(raw: dict[str, Any]) -> dict[str, Any]:
    mass = float(raw["mass"])
    return {
        "mean_total_variation": raw["tv_sum"] / max(mass, 1.0e-12),
        "p90_total_variation": distill._weighted_quantile(raw["samples"], 0.90),
        "weighted_mean_kl": raw["kl_sum"] / max(mass, 1.0e-12),
        "top_action_agreement": raw["top_sum"] / max(mass, 1.0e-12),
        "maximum_absolute_joint_log_ratio": raw["max_joint"],
        "policy_mass": mass,
        "policy_rows": raw["rows"],
        "physical_decisions": raw["decisions"],
    }


def _movement(
    model: screen.StructuredAdapter, decisions: list[Any]
) -> dict[str, Any]:
    weights = distill._episode_weights(decisions)
    overall = _empty_movement()
    by_seat = {0: _empty_movement(), 1: _empty_movement()}
    model.eval()
    with torch.no_grad():
        for decision in decisions:
            _, row_mass = weights[decision.key]
            new_joint = 0.0
            target = by_seat[decision.candidate_seat]
            for row in decision.rows:
                logits, _ = model._one(row)
                old_probability = torch.softmax(row["old_logits"].double(), dim=0)
                new_probability = torch.softmax(logits.double(), dim=0)
                tv = float(0.5 * (old_probability - new_probability).abs().sum())
                kl = float(
                    (
                        old_probability
                        * (
                            old_probability.clamp_min(1.0e-300).log()
                            - new_probability.clamp_min(1.0e-300).log()
                        )
                    ).sum()
                )
                top = float(int(logits.argmax()) == int(row["old_logits"].argmax()))
                new_joint += float(
                    torch.log_softmax(logits.double(), dim=0)[
                        int(row["selected_index"])
                    ]
                )
                for raw in (overall, target):
                    raw["tv_sum"] += tv * row_mass
                    raw["kl_sum"] += kl * row_mass
                    raw["mass"] += row_mass
                    raw["top_sum"] += top * row_mass
                    raw["rows"] += 1
                    raw["samples"].append((tv, row_mass))
            joint_delta = abs(new_joint - decision.old_joint_log_probability)
            for raw in (overall, target):
                raw["max_joint"] = max(raw["max_joint"], joint_delta)
                raw["decisions"] += 1
    return {
        "overall": _finish_movement(overall),
        "by_candidate_seat": {
            str(seat): _finish_movement(by_seat[seat]) for seat in (0, 1)
        },
    }


def _fit_gate(movement: dict[str, Any]) -> dict[str, Any]:
    checks: dict[str, bool] = {}
    for label, metric in (
        ("overall", movement["overall"]),
        ("candidate_seat_0", movement["by_candidate_seat"]["0"]),
        ("candidate_seat_1", movement["by_candidate_seat"]["1"]),
    ):
        checks[f"{label}_mean_tv_at_most_0p030"] = (
            metric["mean_total_variation"] <= MEAN_TV_LIMIT
        )
        checks[f"{label}_p90_tv_at_most_0p100"] = (
            metric["p90_total_variation"] <= P90_TV_LIMIT
        )
    checks["maximum_absolute_joint_log_ratio_at_most_0p50"] = (
        movement["overall"]["maximum_absolute_joint_log_ratio"]
        <= MAX_JOINT_LOG_RATIO
    )
    return {"checks": checks, "decision": "PASS" if all(checks.values()) else "REJECT"}


def _public_movement(movement: dict[str, Any]) -> dict[str, Any]:
    return movement


def _publish(
    args: argparse.Namespace,
    model: screen.StructuredAdapter,
    source: dict[str, Any],
    movement: dict[str, Any],
    gate: dict[str, Any],
) -> dict[str, Any]:
    if args.output_root.exists():
        _fail("candidate output root already exists")
    payload, parameters = initializer._encoded_weights(model)
    args.output_root.mkdir(parents=True)
    parent_output = args.output_root / "parent"
    parent_output.mkdir()
    parent_manifest = args.parent_outcome_root / "checkpoint.json"
    parent_payload = args.parent_outcome_root / "checkpoint.state.f32le"
    if (
        _sha256(parent_manifest) != live.PARENT_MANIFEST_SHA256
        or _sha256(parent_payload) != live.PARENT_PAYLOAD_SHA256
    ):
        _fail("retained parent root identity mismatch")
    shutil.copyfile(parent_manifest, parent_output / parent_manifest.name)
    shutil.copyfile(parent_payload, parent_output / parent_payload.name)
    weights_path = args.output_root / "weights.f32le"
    weights_path.write_bytes(payload)
    weights_sha256 = _sha256(weights_path)
    composite_sha256 = hashlib.sha256(
        COMPOSITE_DOMAIN
        + bytes.fromhex(live.PARENT_MODEL_PARAMETER_SHA256)
        + payload
    ).hexdigest()
    initializer_identity = source["initializer_identity"]
    report = {
        "schema": REPORT_SCHEMA,
        "initializer": {
            "candidate_json_sha256": initializer_identity["candidate_json_sha256"],
            "report_sha256": initializer_identity["report_sha256"],
            "weights_sha256": initializer_identity["weights_sha256"],
            "composite_model_parameter_sha256": initializer_identity[
                "composite_model_parameter_sha256"
            ],
            "model_state_sha256": INITIALIZER_STATE_SHA256,
        },
        "source": {
            "cache_sha256": source["cache_sha256"],
            "pair_count": source["pair_count"],
            "base_seed": source["base_seed"],
            "pool_json_sha256": source["pool_json_sha256"],
            "source_commit": args.source_commit,
        },
        "config": {
            "architecture": ARCHITECTURE,
            "value_model": VALUE_MODEL,
            "seed": FIT_SEED,
            "epochs": FIT_EPOCHS,
            "batch_size_physical_decisions": BATCH_SIZE,
            "learning_rate": LR,
            "weight_decay": WEIGHT_DECAY,
            "gradient_norm_cap": GRAD_CAP,
            "ppo_clip": CLIP,
            "history_length": distill.HISTORY_LENGTH,
            "history_feature_dim": distill.HISTORY_FEATURE_DIM,
            "weighting": "equal-episode-equal-physical-decision-joint-substep-ratio/v1",
            "advantage": "terminal-reward-minus-frozen-parent-value-seat-standardized/v1",
            "objective": "terminal-candidate-reward-only-clipped-ppo/v1",
        },
        "movement": movement,
        "transport": {
            "maximum_absolute_logit_error": PROVISIONAL_TRANSPORT_ERROR,
            "parent_value_bit_exact": False,
        },
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
    }
    report_path = args.output_root / "report.json"
    report_path.write_bytes(history_publish._json_bytes(report))
    report_sha256 = _sha256(report_path)
    candidate = {
        "schema": CANDIDATE_SCHEMA,
        "publication_encoding": "json-pretty-sorted-utf8-trailing-lf/v1",
        "parent": {
            "directory": "parent",
            "manifest_sha256": live.PARENT_MANIFEST_SHA256,
            "payload_sha256": live.PARENT_PAYLOAD_SHA256,
            "native_state_sha256": live.PARENT_NATIVE_STATE_SHA256,
            "model_parameter_sha256": live.PARENT_MODEL_PARAMETER_SHA256,
            "adam_step": live.PARENT_ADAM_STEP,
        },
        "architecture": {
            "identity": ARCHITECTURE,
            "state_dim": screen.STATE_DIM,
            "object_dim": screen.OBJECT_DIM,
            "edge_dim": screen.EDGE_DIM,
            "action_dim": screen.ACTION_DIM,
            "ref_dim": screen.REF_DIM,
            "hidden_dim": distill.DIM,
            "card_vocab": distill.CARD_VOCAB,
            "card_embedding_dim": max(8, distill.DIM // 2),
            "group_vocab": distill.GROUP_VOCAB,
            "group_embedding_dim": max(8, distill.DIM // 3),
            "history_length": distill.HISTORY_LENGTH,
            "history_feature_dim": distill.HISTORY_FEATURE_DIM,
            "history_role_dim": 2,
            "value_model": VALUE_MODEL,
        },
        "weights": {
            "filename": weights_path.name,
            "encoding": "ordered-row-major-finite-f32-little-endian/v1",
            "sha256": weights_sha256,
            "byte_count": len(payload),
            "parameter_count": history_publish.EXPECTED_PARAMETER_COUNT,
            "parameters": parameters,
        },
        "report": {"filename": report_path.name, "sha256": report_sha256},
        "composite_model_parameter_sha256": composite_sha256,
    }
    candidate_path = args.output_root / CANDIDATE_FILENAME
    candidate_path.write_bytes(history_publish._json_bytes(candidate))
    return {
        "decision": "STAGED_PENDING_NATIVE_TRANSPORT",
        "candidate_root": str(args.output_root),
        "candidate_json_sha256": _sha256(candidate_path),
        "report_sha256": report_sha256,
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
    }


def fit(args: argparse.Namespace) -> dict[str, Any]:
    formal = not args.profile_only
    epochs = FIT_EPOCHS if formal else 1
    pair_limit = None if formal else args.pair_limit
    if args.threads not in THREAD_CHOICES:
        _fail("thread count must be 12 or 24")
    for path in (args.output, args.output_root if formal else None):
        if path is not None and path.exists():
            _fail(f"fit output already exists: {path}")
    if formal and (
        args.pair_limit is not None
        or args.source_commit is None
        or args.parent_outcome_root is None
        or args.initializer_state is None
        or _sha256(args.initializer_state) != INITIALIZER_STATE_SHA256
    ):
        _fail("formal fit inputs are incomplete or mismatched")
    started = time.perf_counter()
    decisions, source, timings = _load_decisions(args.cache, pair_limit)
    if formal and (
        source["pair_count"] != FORMAL_PAIRS
        or source["base_seed"] != FORMAL_BASE_SEED
    ):
        _fail("formal training cache panel mismatch")
    initializer_state = args.initializer_state
    if initializer_state is None:
        _fail("initializer state is required")
    state_payload = torch.load(initializer_state, map_location="cpu", weights_only=False)
    if state_payload.get("schema") != initializer.MODEL_STATE_SCHEMA:
        _fail("initializer model-state schema mismatch")
    screen._configure(FIT_SEED, args.threads)
    model = distill._model()
    model.load_state_dict(state_payload["model_state_dict"], strict=True)
    initial_value_bits = initializer._value_head_bits(model)
    alignment = _alignment(model, decisions)
    if not alignment["pass"]:
        _fail("initializer does not match collected behavior logits")
    aligned = time.perf_counter()
    training_history = _fit_model(model, decisions, epochs)
    trained = time.perf_counter()
    movement = _movement(model, decisions)
    gate = _fit_gate(movement)
    measured = time.perf_counter()
    if initializer._value_head_bits(model) != initial_value_bits:
        _fail("frozen value head changed before checkpoint")
    timings.update(
        {
            "alignment_seconds": aligned - (started + sum(timings.values())),
            "train_seconds": trained - aligned,
            "movement_seconds": measured - trained,
        }
    )
    result: dict[str, Any] = {
        "schema": FIT_SCHEMA,
        "decision": gate["decision"],
        "formal": formal,
        "source": source,
        "config": {
            "epochs": epochs,
            "threads": args.threads,
            "batch_size_physical_decisions": BATCH_SIZE,
            "learning_rate": LR,
            "weight_decay": WEIGHT_DECAY,
            "ppo_clip": CLIP,
            "gradient_norm_cap": GRAD_CAP,
            "seed": FIT_SEED,
        },
        "initializer_alignment": alignment,
        "advantage_statistics_by_candidate_seat": outcome._advantage_statistics(decisions),
        "training_history": training_history,
        "movement": _public_movement(movement),
        "fit_gate": gate,
        "phase_runtime_seconds": timings,
        "runtime_seconds": time.perf_counter() - started,
    }
    if formal:
        state_path = Path(str(args.output_root) + ".state.pt")
        parity_path = Path(str(args.output_root) + ".parity.json")
        for path in (state_path, parity_path):
            if path.exists():
                _fail(f"fit sidecar already exists: {path}")
        screen._atomic_torch_save(
            {
                "schema": MODEL_STATE_SCHEMA,
                "source": source,
                "config": result["config"],
                "initializer_alignment": alignment,
                "training_history": training_history,
                "movement": movement,
                "fit_gate": gate,
                "model_state_dict": model.state_dict(),
            },
            state_path,
        )
        parity = initializer._parity_fixture(model, decisions)
        parity["schema"] = PARITY_SCHEMA
        _write_new_json(parity_path, parity)
        result["model_state"] = {
            "path": str(state_path),
            "sha256": _sha256(state_path),
        }
        result["parity_fixture"] = {
            "path": str(parity_path),
            "sha256": _sha256(parity_path),
        }
        if gate["decision"] == "PASS":
            result["publication"] = _publish(args, model, source, movement, gate)
        else:
            result["publication"] = "WITHHELD"
    _write_new_json(args.output, result)
    return result


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    collect_parser = subparsers.add_parser("collect")
    collect_parser.add_argument("--scorer", type=Path, default=qualification.SCORER)
    collect_parser.add_argument("--initializer-root", type=Path, required=True)
    collect_parser.add_argument("--pool-root", type=Path, default=qualification.POOL_ROOT)
    collect_parser.add_argument("--collection-root", type=Path, required=True)
    collect_parser.add_argument("--output", type=Path, required=True)
    collect_parser.add_argument("--base-seed", type=int, default=FORMAL_BASE_SEED)
    collect_parser.add_argument("--shards", type=int, default=FORMAL_SHARDS)
    collect_parser.add_argument("--profile-pairs", type=int)

    cache_parser = subparsers.add_parser("prepare-cache")
    cache_parser.add_argument("--collection", type=Path, required=True)
    cache_parser.add_argument("--shard-cache-root", type=Path, required=True)
    cache_parser.add_argument("--cache", type=Path, required=True)
    cache_parser.add_argument("--output", type=Path, required=True)

    fit_parser = subparsers.add_parser("fit")
    fit_parser.add_argument("--cache", type=Path, required=True)
    fit_parser.add_argument("--initializer-state", type=Path, required=True)
    fit_parser.add_argument("--parent-outcome-root", type=Path)
    fit_parser.add_argument("--source-commit")
    fit_parser.add_argument("--output-root", type=Path)
    fit_parser.add_argument("--output", type=Path, required=True)
    fit_parser.add_argument("--threads", type=int, choices=THREAD_CHOICES, required=True)
    fit_parser.add_argument("--profile-only", action="store_true")
    fit_parser.add_argument("--pair-limit", type=int)
    return parser


def main() -> int:
    args = _parser().parse_args()
    if args.command == "collect":
        result = collect(args)
    elif args.command == "prepare-cache":
        result = prepare_cache(args)
    else:
        if not args.profile_only and args.output_root is None:
            _fail("formal fit requires --output-root")
        result = fit(args)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
