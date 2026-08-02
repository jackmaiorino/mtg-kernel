#!/usr/bin/env python3
"""Validate and merge a completed scaled dual-export collection."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
from pathlib import Path
from typing import Any, TextIO


COLLECTOR_SCHEMA = "mtg-kernel-scaled-structured-corpus-collector/v1"
REPORT_SCHEMA = "mtg-kernel-scaled-structured-corpus/v1"
REPEAT_SCHEMA = "mtg-kernel-scaled-structured-corpus-repeat/v1"
PRIMARY_PAIRS = 2_048
PARENT_MANIFEST_SHA256 = "706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb"
PARENT_PAYLOAD_SHA256 = "eb83be33bcb7418b6f85ec9687da4b7ca5620a1df64721a1942d2793588bbd3c"
PARENT_TRAIN_STATE_SHA256 = "2c55a13abb3157f3f4ba012af663ffa56599c5d6cb90743c1ba6e024ca47a9c8"
PARENT_MODEL_PARAMETER_SHA256 = "883e4882d01d9cb55ecd7a4ae00e3c95793b6147baf3df08650ef1fa7f8e9546"
CONTRACTS = {
    "teacher": ("mtg-kernel-xmage-cp7-teacher-jsonl/v1", 1),
    "outcome": ("mtg-kernel-xmage-cp7-outcome-jsonl/v2", 2),
}


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _row(line: str, path: Path, line_number: int) -> dict[str, Any]:
    try:
        value = json.loads(line)
    except json.JSONDecodeError as error:
        _fail(f"{path}:{line_number}: invalid JSON: {error}")
    if not isinstance(value, dict):
        _fail(f"{path}:{line_number}: row is not an object")
    return value


def _write_row(handle: TextIO, value: dict[str, Any]) -> None:
    handle.write(json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False))
    handle.write("\n")


def _checkpoint(header: dict[str, Any]) -> tuple[Any, ...]:
    checkpoint = header.get("checkpoint")
    if not isinstance(checkpoint, dict):
        _fail("export header lacks checkpoint identity")
    return (
        checkpoint.get("loaded_checkpoint_sha256"),
        checkpoint.get("loaded_payload_sha256"),
        checkpoint.get("loaded_train_state_sha256"),
        checkpoint.get("model_parameter_sha256"),
    )


def _validate_header(kind: str, header: dict[str, Any]) -> None:
    contract, version = CONTRACTS[kind]
    if (
        header.get("record_type") != "header"
        or header.get("record_ordinal") != 0
        or header.get("export_contract") != contract
        or header.get("schema_version") != version
    ):
        _fail(f"invalid {kind} header")
    if _checkpoint(header) != (
        PARENT_MANIFEST_SHA256,
        PARENT_PAYLOAD_SHA256,
        PARENT_TRAIN_STATE_SHA256,
        PARENT_MODEL_PARAMETER_SHA256,
    ):
        _fail(f"{kind} header does not bind the retained parent")


def _terminal_metadata(value: dict[str, Any]) -> dict[str, Any]:
    terminal = value.get("terminal")
    if not isinstance(terminal, dict):
        _fail("terminal record lacks terminal object")
    if (
        terminal.get("terminal_classification") != "natural"
        or terminal.get("terminal_code") != "natural_game_over"
        or terminal.get("terminal_reason") != "game_over"
        or terminal.get("terminal_outcome") not in ("p0_win", "p1_win", "draw")
    ):
        _fail("non-natural terminal record")
    return {
        "deck_ids": value.get("deck_ids"),
        "randomization_identity": value.get("randomization_identity"),
        "base_seed_u64_hex": value.get("base_seed_u64_hex"),
        "pair_environment_seed_u64_hex": value.get("pair_environment_seed_u64_hex"),
        "terminal": terminal,
        "diagnostic_state_hash_u64_hex": value.get("diagnostic_state_hash_u64_hex"),
        "core_environment_hash_u64_hex": value.get("core_environment_hash_u64_hex"),
    }


def _task_pairs(result: dict[str, Any]) -> set[int]:
    task = result.get("task")
    if not isinstance(task, dict):
        _fail("successful result lacks task")
    first = task.get("first_pair")
    count = task.get("pair_count")
    if not isinstance(first, int) or not isinstance(count, int) or first < 0 or count < 1:
        _fail("successful result has invalid task range")
    return set(range(first, first + count))


def _validate_log(result: dict[str, Any]) -> dict[str, Any]:
    path = Path(result.get("log", ""))
    if not path.is_file() or _sha256(path) != result.get("log_sha256"):
        _fail(f"task log identity mismatch: {path}")
    summaries = [
        line.strip()
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.startswith("XMAGE_RALLY_ANCHOR_SPIKE ")
    ]
    if len(summaries) != 1 or not summaries[0].startswith("XMAGE_RALLY_ANCHOR_SPIKE PASS "):
        _fail(f"task log lacks one passing spike summary: {path}")
    summary = summaries[0]
    required = (
        r"\bopponent=cp7\b",
        r"\bcp7_skill=7\b",
        r"\btotal_candidate_priority_projections=0\b",
        r"\balignment=no_selected_action_projection\b",
    )
    if any(re.search(pattern, summary) is None for pattern in required):
        _fail(f"task summary violates the collection gate: {path}")
    return {"path": str(path), "sha256": result["log_sha256"]}


def _successful_tasks(state: dict[str, Any]) -> tuple[list[dict[str, Any]], set[int]]:
    successful = state.get("successful")
    if not isinstance(successful, list) or not successful:
        _fail("collection has no successful tasks")
    pairs: set[int] = set()
    for result in successful:
        if result.get("status") != "success" or result.get("return_code") != 0:
            _fail("successful task has invalid status")
        task_pairs = _task_pairs(result)
        if pairs.intersection(task_pairs):
            _fail("successful task ranges overlap")
        pairs.update(task_pairs)
        _validate_log(result)
    if len(pairs) != PRIMARY_PAIRS:
        _fail(f"expected {PRIMARY_PAIRS} unique pairs, got {len(pairs)}")
    fold_counts = {str(fold): sum(pair % 4 == fold for pair in pairs) for fold in range(4)}
    if set(fold_counts.values()) != {512} or state.get("fold_pair_counts") != fold_counts:
        _fail("final fold counts are not exact")
    return sorted(successful, key=lambda value: min(_task_pairs(value))), pairs


def _validate_repeat(evidence_root: Path, tasks: list[dict[str, Any]]) -> dict[str, Any]:
    path = evidence_root / "repeat" / "repeat-report.json"
    report = json.loads(path.read_text(encoding="utf-8"))
    if report.get("schema") != REPEAT_SCHEMA or report.get("pass") is not True:
        _fail("scaled collection repeat is not passing")
    task = report.get("task")
    if not any(result.get("task") == task for result in tasks):
        _fail("repeat task is not a successful collection task")
    for kind in ("teacher", "outcome"):
        comparison = report.get("comparisons", {}).get(kind, {})
        original = Path(comparison.get("original_path", ""))
        repeated = Path(comparison.get("repeat_path", ""))
        if (
            comparison.get("byte_identical") is not True
            or comparison.get("original_sha256") != comparison.get("repeat_sha256")
            or not original.is_file()
            or not repeated.is_file()
            or _sha256(original) != comparison.get("original_sha256")
            or _sha256(repeated) != comparison.get("repeat_sha256")
        ):
            _fail(f"{kind} repeat bytes are not exact")
    return {"path": str(path), "sha256": _sha256(path), "pass": True}


def _merge_kind(
    kind: str,
    tasks: list[dict[str, Any]],
    expected_pairs: set[int],
    output: Path,
) -> dict[str, Any]:
    if output.exists() or output.with_suffix(output.suffix + ".tmp").exists():
        _fail(f"refusing to overwrite output: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_suffix(output.suffix + ".tmp")
    canonical_header: dict[str, Any] | None = None
    terminals: dict[tuple[int, int, str], dict[str, Any]] = {}
    decisions: set[tuple[int, int, str, int]] = set()
    global_record = 0
    global_decision = 0
    sources: list[dict[str, Any]] = []
    try:
        with temporary.open("x", encoding="utf-8", newline="\n") as target:
            for result in tasks:
                source_info = result.get(kind)
                if not isinstance(source_info, dict):
                    _fail(f"task lacks {kind} export result")
                path = Path(source_info.get("path", ""))
                if not path.is_file() or _sha256(path) != source_info.get("sha256"):
                    _fail(f"{kind} source identity mismatch: {path}")
                task_pairs = _task_pairs(result)
                local_record = 0
                local_decision = 0
                local_terminals = 0
                decision_offset = global_decision
                with path.open("r", encoding="utf-8") as handle:
                    for line_number, line in enumerate(handle, 1):
                        if not line.strip():
                            continue
                        value = _row(line, path, line_number)
                        if value.get("record_ordinal") != local_record:
                            _fail(f"{path}: non-contiguous record ordinal")
                        local_record += 1
                        if value.get("record_type") == "header":
                            if local_record != 1:
                                _fail(f"{path}: duplicate header")
                            _validate_header(kind, value)
                            if canonical_header is None:
                                canonical_header = value
                                merged = dict(value)
                                merged["record_ordinal"] = global_record
                                _write_row(target, merged)
                                global_record += 1
                            elif value != canonical_header:
                                _fail(f"{path}: header differs from first shard")
                            continue
                        pair = value.get("pair_index")
                        episode = value.get("episode_id")
                        seat = value.get("candidate_seat")
                        if (
                            pair not in task_pairs
                            or seat not in ("p0", "p1")
                            or episode != pair * 2 + int(seat[1])
                        ):
                            _fail(f"{path}: row has invalid episode identity")
                        record_type = value.get("record_type")
                        rewritten = dict(value)
                        rewritten["record_ordinal"] = global_record
                        if record_type == "decision":
                            ordinal_name = f"{kind}_decision_ordinal"
                            if value.get(ordinal_name) != local_decision:
                                _fail(f"{path}: non-contiguous {ordinal_name}")
                            step = value.get("step")
                            key = (pair, episode, seat, step)
                            if not isinstance(step, int) or key in decisions:
                                _fail(f"{path}: duplicate or invalid decision step")
                            decisions.add(key)
                            rewritten[ordinal_name] = global_decision
                            local_decision += 1
                            global_decision += 1
                        elif record_type == "terminal":
                            key = (pair, episode, seat)
                            if key in terminals:
                                _fail(f"{path}: duplicate terminal {key}")
                            terminals[key] = _terminal_metadata(value)
                            if kind == "outcome":
                                first = value.get("first_outcome_decision_ordinal")
                                if not isinstance(first, int):
                                    _fail(f"{path}: invalid first outcome decision ordinal")
                                rewritten["first_outcome_decision_ordinal"] = decision_offset + first
                            local_terminals += 1
                        else:
                            _fail(f"{path}: unknown record type {record_type!r}")
                        _write_row(target, rewritten)
                        global_record += 1
                if local_record < 3 or local_terminals != 2 * len(task_pairs):
                    _fail(f"{path}: incomplete task export")
                if local_decision != source_info.get("decision_count"):
                    _fail(f"{path}: decision count differs from collector state")
                sources.append(
                    {
                        "path": str(path),
                        "sha256": source_info["sha256"],
                        "pair_count": len(task_pairs),
                        "decision_count": local_decision,
                        "terminal_count": local_terminals,
                    }
                )
        expected_terminals = {
            (pair, pair * 2 + seat, f"p{seat}")
            for pair in expected_pairs
            for seat in (0, 1)
        }
        if terminals.keys() != expected_terminals:
            _fail(f"{kind} terminal coverage is not exact")
        os.replace(temporary, output)
    except Exception:
        if temporary.exists():
            temporary.unlink()
        raise
    return {
        "path": str(output),
        "sha256": _sha256(output),
        "bytes": output.stat().st_size,
        "record_count": global_record,
        "decision_count": global_decision,
        "terminal_count": len(terminals),
        "terminal_metadata": terminals,
        "sources": sources,
        "header": canonical_header,
    }


def finalize(evidence_root: Path, report_path: Path) -> dict[str, Any]:
    state_path = evidence_root / "collection-state.json"
    state = json.loads(state_path.read_text(encoding="utf-8"))
    if (
        state.get("schema") != COLLECTOR_SCHEMA
        or state.get("phase") != "complete"
        or state.get("pass") is not True
        or state.get("completed_pair_count") != PRIMARY_PAIRS
    ):
        _fail("collection state is not a complete passing corpus")
    tasks, pairs = _successful_tasks(state)
    repeat = _validate_repeat(evidence_root, tasks)
    corpus_root = evidence_root / "corpus"
    teacher_path = corpus_root / "teacher-combined.jsonl"
    outcome_path = corpus_root / "outcome-combined.jsonl"
    teacher_stage = corpus_root / "teacher-combined.jsonl.staging"
    outcome_stage = corpus_root / "outcome-combined.jsonl.staging"
    if teacher_path.exists() or outcome_path.exists():
        _fail("refusing to overwrite an existing combined corpus")
    try:
        teacher = _merge_kind("teacher", tasks, pairs, teacher_stage)
        outcome = _merge_kind("outcome", tasks, pairs, outcome_stage)
        if teacher.pop("terminal_metadata") != outcome.pop("terminal_metadata"):
            _fail("teacher and outcome terminal replay metadata differ")
        if _checkpoint(teacher.pop("header")) != _checkpoint(outcome.pop("header")):
            _fail("teacher and outcome checkpoint identities differ")
        os.replace(teacher_stage, teacher_path)
        os.replace(outcome_stage, outcome_path)
        teacher["path"] = str(teacher_path)
        outcome["path"] = str(outcome_path)
    except Exception:
        for created in (teacher_stage, outcome_stage, teacher_path, outcome_path):
            if created.exists():
                created.unlink()
        raise
    result = {
        "schema": REPORT_SCHEMA,
        "pass": True,
        "state": {"path": str(state_path), "sha256": _sha256(state_path)},
        "repeat": repeat,
        "pair_count": len(pairs),
        "game_count": len(pairs) * 2,
        "fold_pair_counts": state["fold_pair_counts"],
        "excluded_primary_pairs": sorted(set(state["excluded_primary_pairs"])),
        "completed_replacements": state["completed_replacements"],
        "teacher": teacher,
        "outcome": outcome,
        "terminal_replays_exact": True,
        "candidate_priority_projections": 0,
        "alignment": "no_selected_action_projection",
    }
    if report_path.exists():
        _fail(f"refusing to overwrite report: {report_path}")
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(
        json.dumps(result, sort_keys=True, indent=2, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-root", type=Path, required=True)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    report = args.report or args.evidence_root / "corpus" / "combine-report.json"
    result = finalize(args.evidence_root, report)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
