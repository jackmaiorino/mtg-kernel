#!/usr/bin/env python3
"""Collect and cache the fixed fresh bounded-value confirmation panel."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
from pathlib import Path
import sys
import time
from types import SimpleNamespace
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
SCRIPTS_DIR = SCRIPT_DIR.parent
TERMINAL_DIR = SCRIPTS_DIR / "policy_only_structured_terminal_rung_v1"
QUALIFICATION_DIR = SCRIPTS_DIR / "policy_only_structured_successor_v1"
for directory in (TERMINAL_DIR, QUALIFICATION_DIR):
    sys.path.insert(0, str(directory))

import run_matched_gate_v1 as qualification  # noqa: E402
import run_pipeline_v1 as terminal  # noqa: E402


BASE_SEED = 1_690_001
PAIR_COUNT = 1_024
SHARDS = 4
SHARD_PAIRS = 256
SCORER_SHA256 = "8af1ffabe836cfe53d9b62edb98943e68183825e332cd47070ea20e93ae5c990"
INITIALIZER_CANDIDATE_SHA256 = "204beb91c1a4b039e0c497f2b420e823b5cc9e2ceb8560f897d0b6251e916b72"
POOL_SHA256 = "6c3c8ff09ab519dc9f462b41cbf898da902d230656d14e64d79fc66a19f3bc71"


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _write_new(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def collect(args: argparse.Namespace) -> dict[str, Any]:
    if args.output.exists() or args.collection_root.exists():
        _fail("fresh collection output already exists")
    if _sha256(args.scorer) != SCORER_SHA256:
        _fail("fresh collection scorer SHA-256 mismatch")
    if _sha256(args.pool_root / "pool.json") != POOL_SHA256:
        _fail("fresh collection Pool3 SHA-256 mismatch")
    identity = qualification._candidate_identity(args.initializer_root)  # noqa: SLF001
    if identity["candidate_json_sha256"] != INITIALIZER_CANDIDATE_SHA256:
        _fail("fresh collection initializer identity mismatch")

    args.collection_root.mkdir(parents=True)
    jobs = [
        {
            "ordinal": ordinal,
            "pair_start": ordinal * SHARD_PAIRS,
            "pairs": SHARD_PAIRS,
            "root": str(args.collection_root),
            "scorer": str(args.scorer),
            "initializer_root": str(args.initializer_root),
            "pool_root": str(args.pool_root),
            "base_seed": BASE_SEED,
        }
        for ordinal in range(SHARDS)
    ]
    started = time.perf_counter()
    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=SHARDS) as executor:
        futures = [executor.submit(terminal._collect_one, job) for job in jobs]  # noqa: SLF001
        for future in concurrent.futures.as_completed(futures):
            results.append(future.result())
    results.sort(key=lambda result: result["ordinal"])
    covered = {
        pair
        for result in results
        for pair in range(result["pair_start"], result["pair_start"] + result["pairs"])
    }
    if covered != set(range(PAIR_COUNT)):
        _fail("fresh collection shards do not exactly cover the panel")
    for result in results:
        report = result["report"]
        if (
            report.get("base_seed") != BASE_SEED
            or report.get("pair_start") != result["pair_start"]
            or report.get("pairs") != SHARD_PAIRS
            or report.get("episodes") != 2 * SHARD_PAIRS
            or report.get("scorer_stderr")
            != [
                "NATIVE_POPULATION_CORPUS "
                f"pool_root={args.pool_root} weights=40,20,20,20"
            ]
        ):
            _fail("fresh collection shard report mismatch")
    measurement_elapsed = max(float(result["report"]["elapsed_seconds"]) for result in results)
    report = {
        "schema": terminal.COLLECTION_SCHEMA,
        "status": "pass",
        "formal": True,
        "purpose": "fresh-bounded-history-value-confirmation/v1",
        "base_seed": BASE_SEED,
        "pair_count": PAIR_COUNT,
        "episode_count": PAIR_COUNT * 2,
        "shard_count": SHARDS,
        "topology": "four-parallel-persistent-native-scorers",
        "measurement_elapsed_seconds": measurement_elapsed,
        "orchestration_elapsed_seconds": time.perf_counter() - started,
        "games_per_second": (PAIR_COUNT * 2) / measurement_elapsed,
        "initializer_identity": identity,
        "scorer_sha256": SCORER_SHA256,
        "pool_json_sha256": POOL_SHA256,
        "shards": results,
    }
    _write_new(args.output, report)
    return report


def prepare_cache(args: argparse.Namespace) -> dict[str, Any]:
    return terminal.prepare_cache(
        SimpleNamespace(
            collection=args.collection,
            shard_cache_root=args.shard_cache_root,
            cache=args.cache,
            output=args.output,
        )
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    collect_parser = subparsers.add_parser("collect")
    collect_parser.add_argument("--scorer", type=Path, required=True)
    collect_parser.add_argument("--initializer-root", type=Path, required=True)
    collect_parser.add_argument("--pool-root", type=Path, required=True)
    collect_parser.add_argument("--collection-root", type=Path, required=True)
    collect_parser.add_argument("--output", type=Path, required=True)
    cache_parser = subparsers.add_parser("prepare-cache")
    cache_parser.add_argument("--collection", type=Path, required=True)
    cache_parser.add_argument("--shard-cache-root", type=Path, required=True)
    cache_parser.add_argument("--cache", type=Path, required=True)
    cache_parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = collect(args) if args.command == "collect" else prepare_cache(args)
    print(
        json.dumps(
            {
                "status": result["status"],
                "pair_count": result["pair_count"],
                "output": str(args.output),
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
