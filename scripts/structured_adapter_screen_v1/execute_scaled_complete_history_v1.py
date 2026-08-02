#!/usr/bin/env python3
"""Wait for the scaled corpus, then execute its fixed screen without idle gaps."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
import collect_scaled_structured_corpus_v1 as collector  # noqa: E402
import finalize_scaled_structured_corpus_v1 as finalizer  # noqa: E402
import run_scaled_complete_history_v1 as scaled  # noqa: E402


SCHEMA = "mtg-kernel-scaled-complete-history-execution/v1"


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _save(path: Path, state: dict[str, Any], status: str, **updates: Any) -> None:
    state.update(updates)
    state["status"] = status
    state["updated_unix_seconds"] = time.time()
    collector._atomic_json(path, state)


def _wait_for_collection(root: Path, pipeline_path: Path, state: dict[str, Any]) -> None:
    collection_path = root / "collection-state.json"
    while True:
        collection = json.loads(collection_path.read_text(encoding="utf-8"))
        if collection.get("phase") == "complete":
            if collection.get("pass") is not True:
                _fail("scaled collection completed without passing")
            return
        _save(
            pipeline_path,
            state,
            "waiting-for-collection",
            collection_successful_tasks=len(collection.get("successful", [])),
            collection_failed_attempts=len(collection.get("failed_attempts", [])),
        )
        time.sleep(15)


def _run_folds(root: Path, cache: Path, pipeline_path: Path, state: dict[str, Any]) -> list[Path]:
    folds_root = root / "screen" / "folds"
    folds_root.mkdir(parents=True, exist_ok=True)
    processes: list[tuple[int, subprocess.Popen[bytes], Any, Path]] = []
    fold_paths: list[Path] = []
    script = Path(scaled.__file__).resolve()
    for fold in range(4):
        output = folds_root / f"fold-{fold}.json"
        log_path = folds_root / f"fold-{fold}.log"
        if output.exists() or log_path.exists():
            _fail(f"refusing to overwrite fold {fold}")
        log = log_path.open("xb")
        process = subprocess.Popen(
            [
                sys.executable,
                str(script),
                "--fold",
                str(fold),
                "--cache",
                str(cache),
                "--output",
                str(output),
            ],
            cwd=Path(__file__).resolve().parents[2],
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        processes.append((fold, process, log, output))
        fold_paths.append(output)
    _save(
        pipeline_path,
        state,
        "training-folds",
        fold_processes={str(fold): process.pid for fold, process, _, _ in processes},
    )
    failures: list[tuple[int, int]] = []
    for fold, process, log, _ in processes:
        return_code = process.wait()
        log.close()
        if return_code != 0:
            failures.append((fold, return_code))
    if failures:
        _fail(f"scaled fold failures: {failures}")
    return fold_paths


def _repeat_passing_fold(
    root: Path,
    cache: Path,
    fold_path: Path,
    pipeline_path: Path,
    state: dict[str, Any],
) -> dict[str, Any]:
    repeat_root = root / "screen" / "repeat"
    repeat_root.mkdir(parents=True, exist_ok=True)
    output = repeat_root / "fold-0-repeat01.json"
    log_path = repeat_root / "fold-0-repeat01.log"
    if output.exists() or log_path.exists():
        _fail("refusing to overwrite scaled fold repeat")
    _save(pipeline_path, state, "repeating-passing-fold")
    with log_path.open("xb") as log:
        completed = subprocess.run(
            [
                sys.executable,
                str(Path(scaled.__file__).resolve()),
                "--fold",
                "0",
                "--cache",
                str(cache),
                "--output",
                str(output),
            ],
            cwd=Path(__file__).resolve().parents[2],
            stdout=log,
            stderr=subprocess.STDOUT,
            check=False,
        )
    if completed.returncode != 0:
        _fail("scaled fold repeat failed")
    original = json.loads(fold_path.read_text(encoding="utf-8"))
    repeated = json.loads(output.read_text(encoding="utf-8"))
    for value in (original, repeated):
        value.pop("runtime_seconds", None)
    scientific_equal = original == repeated
    report = {
        "schema": SCHEMA + ".repeat",
        "fold": 0,
        "scientific_fields_equal": scientific_equal,
        "original": {"path": str(fold_path), "sha256": _sha256(fold_path)},
        "repeat": {"path": str(output), "sha256": _sha256(output)},
        "log": {"path": str(log_path), "sha256": _sha256(log_path)},
        "pass": scientific_equal,
    }
    collector._atomic_json(repeat_root / "repeat-report.json", report)
    if not scientific_equal:
        _fail("scaled fold repeat changed scientific fields")
    return report


def execute(root: Path) -> dict[str, Any]:
    pipeline_path = root / "pipeline-state.json"
    if pipeline_path.exists():
        _fail("pipeline state already exists; inspect it before resuming")
    state: dict[str, Any] = {"schema": SCHEMA, "started_unix_seconds": time.time()}
    _save(pipeline_path, state, "starting")
    try:
        _wait_for_collection(root, pipeline_path, state)
        _save(pipeline_path, state, "finalizing-corpus")
        combine_report = root / "corpus" / "combine-report.json"
        finalizer.finalize(root, combine_report)
        _save(
            pipeline_path,
            state,
            "preparing-cache",
            combine_report_sha256=_sha256(combine_report),
        )
        screen_root = root / "screen"
        cache = screen_root / "complete-history-cache.pt"
        cache_report = screen_root / "cache-report.json"
        scaled.prepare(combine_report, cache, cache_report)
        _save(
            pipeline_path,
            state,
            "cache-ready",
            cache_sha256=_sha256(cache),
            cache_report_sha256=_sha256(cache_report),
        )
        fold_paths = _run_folds(root, cache, pipeline_path, state)
        aggregate_path = screen_root / "development-aggregate.json"
        _save(pipeline_path, state, "aggregating-folds")
        aggregate = scaled.aggregate(fold_paths, aggregate_path)
        repeat = None
        if aggregate["pass"]:
            repeat = _repeat_passing_fold(
                root, cache, fold_paths[0], pipeline_path, state
            )
        result = {
            "schema": SCHEMA + ".result",
            "combine_report": {
                "path": str(combine_report),
                "sha256": _sha256(combine_report),
            },
            "cache_report": {
                "path": str(cache_report),
                "sha256": _sha256(cache_report),
            },
            "folds": [
                {"path": str(path), "sha256": _sha256(path)} for path in fold_paths
            ],
            "aggregate": {
                "path": str(aggregate_path),
                "sha256": _sha256(aggregate_path),
                "pass": aggregate["pass"],
                "lane_gates": aggregate["lane_gates"],
            },
            "repeat": repeat,
            "pass": aggregate["pass"] and (repeat is None or repeat["pass"]),
        }
        result_path = screen_root / "execution-result.json"
        collector._atomic_json(result_path, result)
        _save(
            pipeline_path,
            state,
            "complete",
            result_path=str(result_path),
            result_sha256=_sha256(result_path),
            screen_pass=result["pass"],
        )
        return result
    except Exception as error:
        _save(pipeline_path, state, "failed", error=repr(error))
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-root", type=Path, required=True)
    args = parser.parse_args()
    result = execute(args.evidence_root)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
