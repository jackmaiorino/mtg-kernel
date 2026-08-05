#!/usr/bin/env python3
"""Collect and validate shadow depth-8 search-teacher labels.

The scorer remains live-policy controlled. Search diagnostics are labels only.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import time
from typing import Any


SCHEMA = "mtg-kernel-depth8-bounded-value-search-teacher-corpus/v1"
DIAGNOSTIC_SCHEMA = "mtg-kernel-depth8-bounded-value-search-teacher/v1"
DIAGNOSTIC_PREFIX = "NATIVE_DEPTH8_BOUNDED_TEACHER_JSON "
OUTCOME_CONTRACT_PREFIX = "mtg-kernel-xmage-cp7-outcome-jsonl/"
TEACHER_CONTRACT_PREFIX = "mtg-kernel-xmage-cp7-teacher-jsonl/"
NATURAL_CLASSIFICATION = "natural"
NATURAL_CODE = "natural_game_over"
TOPOLOGY_WORKERS = (1, 2, 4)
EXPECTED_AUTHORITY_KIND = "qualified-policy-bounded-value-search-v1"
EXPECTED_PACKAGE_MANIFEST_SHA256 = (
    "0d883d169fca504e4a413810454565d98cd0e8316cb76e7de4f538187b2865c9"
)
EXPECTED_OUTCOME_SELECTION_SOURCE = "candidate_checkpoint_policy"
EXPECTED_TEACHER_SELECTION_SOURCE = "xmage_rally_cp7_mapper"
SEARCH_OVERRIDE_MARGIN = 0.25


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _is_int(value: Any) -> bool:
    return type(value) is int


def _is_hex(value: Any, width: int) -> bool:
    if not isinstance(value, str) or len(value) != width:
        return False
    try:
        int(value, 16)
    except ValueError:
        return False
    return True


def _f32_from_hex(value: Any) -> float:
    if not _is_hex(value, 8):
        _fail("invalid f32 bit-pattern hex")
    return struct.unpack(">f", bytes.fromhex(value))[0]


def _f32_bits_hex(value: float) -> str:
    return struct.pack(">f", value).hex()


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _load_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            try:
                value = json.loads(line)
            except json.JSONDecodeError as error:
                _fail(f"{path}:{line_number}: invalid JSON: {error}")
            if not isinstance(value, dict):
                _fail(f"{path}:{line_number}: record is not an object")
            rows.append(value)
    return rows


def _atomic_fresh_json(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite existing report: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    if temporary.exists():
        _fail(f"temporary report already exists: {temporary}")
    with temporary.open("x", encoding="utf-8", newline="\n") as handle:
        json.dump(value, handle, indent=2, sort_keys=True, allow_nan=False)
        handle.write("\n")
    os.replace(temporary, path)


def _version(command: list[str], cwd: Path | None = None) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=True,
    )
    lines = completed.stdout.strip().splitlines()
    return lines[0] if lines else ""


def _decision_key(row: dict[str, Any]) -> tuple[Any, ...]:
    return (
        row.get("episode_id"),
        row.get("step"),
        row.get("physical_decision_id"),
        row.get("substep_index"),
        row.get("candidate_seat"),
        row.get("model_input_sha256"),
    )


def _expected_terminal_keys(first_pair: int, pair_count: int) -> set[tuple[int, int, str]]:
    return {
        (pair, pair * 2 + seat, f"p{seat}")
        for pair in range(first_pair, first_pair + pair_count)
        for seat in (0, 1)
    }


def _validate_header(
    rows: list[dict[str, Any]],
    path: Path,
    contract_prefix: str,
    selection_source: str,
) -> dict[str, Any]:
    headers = [row for row in rows if row.get("record_type") == "header"]
    if len(headers) != 1:
        _fail(f"{path}: expected exactly one header")
    header = headers[0]
    checkpoint = header.get("checkpoint")
    if (
        not str(header.get("export_contract", "")).startswith(contract_prefix)
        or header.get("selection_source") != selection_source
        or not isinstance(checkpoint, dict)
        or checkpoint.get("authority_kind") != EXPECTED_AUTHORITY_KIND
        or checkpoint.get("loaded_checkpoint_sha256")
        != EXPECTED_PACKAGE_MANIFEST_SHA256
    ):
        _fail(f"{path}: header authority or contract mismatch")
    if rows[0] is not header or header.get("record_ordinal") != 0:
        _fail(f"{path}: header must be the first record")
    for ordinal, row in enumerate(rows):
        if row.get("record_ordinal") != ordinal:
            _fail(f"{path}: record ordinals are not contiguous")
        if row.get("record_type") not in {"header", "decision", "terminal"}:
            _fail(f"{path}: unexpected record type")
    return checkpoint


def _validate_terminal_rows(
    rows: list[dict[str, Any]],
    path: Path,
    first_pair: int,
    pair_count: int,
) -> None:
    terminals = [row for row in rows if row.get("record_type") == "terminal"]
    observed = {
        (row.get("pair_index"), row.get("episode_id"), row.get("candidate_seat"))
        for row in terminals
    }
    expected = _expected_terminal_keys(first_pair, pair_count)
    if observed != expected or len(terminals) != len(expected):
        _fail(f"{path}: terminal coverage mismatch")
    for row in terminals:
        terminal = row.get("terminal")
        if not isinstance(terminal, dict):
            _fail(f"{path}: terminal body is not an object")
        if (
            terminal.get("terminal_classification") != NATURAL_CLASSIFICATION
            or terminal.get("terminal_code") != NATURAL_CODE
        ):
            _fail(f"{path}: non-natural terminal")
        pair = row.get("pair_index")
        episode = row.get("episode_id")
        seat = row.get("candidate_seat")
        if (
            not _is_int(pair)
            or not _is_int(episode)
            or seat not in {"p0", "p1"}
            or episode != pair * 2 + int(seat[1])
        ):
            _fail(f"{path}: terminal pair, episode, and seat disagree")


def _validate_action_row(row: dict[str, Any], path: Path, label: str) -> None:
    legal = row.get("legal_action_count")
    semantics = row.get("action_semantics")
    selected = row.get("selected_index")
    if not _is_int(legal) or legal < 1:
        _fail(f"{path}: {label} legal_action_count invalid")
    if not isinstance(semantics, list) or len(semantics) != legal:
        _fail(f"{path}: {label} action semantics width mismatch")
    if not _is_int(selected) or not 0 <= selected < legal:
        _fail(f"{path}: {label} selected index out of range")
    if row.get("selected_semantic") != semantics[selected]:
        _fail(f"{path}: {label} selected semantic mismatch")
    if not _is_hex(row.get("model_input_sha256"), 64):
        _fail(f"{path}: {label} model input hash missing")
    if not _is_hex(row.get("candidate_order_commitment_128_hex"), 32):
        _fail(f"{path}: {label} candidate-order commitment missing")
    pair = row.get("pair_index")
    episode = row.get("episode_id")
    seat = row.get("candidate_seat")
    if (
        not _is_int(pair)
        or not _is_int(episode)
        or seat not in {"p0", "p1"}
        or episode != pair * 2 + int(seat[1])
    ):
        _fail(f"{path}: {label} pair, episode, and seat disagree")


def _validate_outcome(
    path: Path, first_pair: int, pair_count: int
) -> tuple[dict[tuple[Any, ...], dict[str, Any]], dict[str, Any]]:
    rows = _load_jsonl(path)
    decisions = [row for row in rows if row.get("record_type") == "decision"]
    checkpoint = _validate_header(
        rows,
        path,
        OUTCOME_CONTRACT_PREFIX,
        EXPECTED_OUTCOME_SELECTION_SOURCE,
    )
    _validate_terminal_rows(rows, path, first_pair, pair_count)
    by_key: dict[tuple[Any, ...], dict[str, Any]] = {}
    expected_episodes = {pair * 2 + seat for pair in range(first_pair, first_pair + pair_count) for seat in (0, 1)}
    decisions_by_episode: dict[int, list[int]] = {episode: [] for episode in expected_episodes}
    for decision_ordinal, row in enumerate(decisions):
        if row.get("outcome_decision_ordinal") != decision_ordinal:
            _fail(f"{path}: outcome decision ordinals are not contiguous")
        if row.get("candidate_seat") not in {"p0", "p1"}:
            _fail(f"{path}: invalid outcome candidate seat")
        if row.get("episode_id") not in expected_episodes or row.get("acting_player") != row.get("candidate_seat"):
            _fail(f"{path}: outcome decision escaped candidate episode")
        _validate_action_row(row, path, "outcome")
        decisions_by_episode[row["episode_id"]].append(decision_ordinal)
        key = _decision_key(row)
        if key in by_key:
            _fail(f"{path}: duplicate outcome decision key {key}")
        by_key[key] = row
    if not by_key:
        _fail(f"{path}: no candidate outcome decisions")
    for terminal in _terminal_rows(rows):
        ordinals = decisions_by_episode[terminal["episode_id"]]
        expected_first = ordinals[0] if ordinals else None
        if (
            terminal.get("first_outcome_decision_ordinal") != expected_first
            or terminal.get("outcome_decision_count") != len(ordinals)
        ):
            _fail(f"{path}: terminal outcome decision range mismatch")
        reward_index = int(terminal["candidate_seat"][1])
        rewards = terminal["terminal"].get("terminal_reward")
        if (
            not isinstance(rewards, list)
            or len(rewards) != 2
            or terminal.get("candidate_terminal_reward") != rewards[reward_index]
        ):
            _fail(f"{path}: candidate terminal reward mismatch")
    return by_key, {
        "decisions": len(decisions),
        "terminals": len(_terminal_rows(rows)),
        "checkpoint": checkpoint,
        "terminal_payloads": {
            (row["pair_index"], row["episode_id"], row["candidate_seat"]): row["terminal"]
            for row in _terminal_rows(rows)
        },
    }


def _terminal_rows(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [row for row in rows if row.get("record_type") == "terminal"]


def _validate_opponent_teacher(path: Path, first_pair: int, pair_count: int) -> dict[str, Any]:
    rows = _load_jsonl(path)
    decisions = [row for row in rows if row.get("record_type") == "decision"]
    checkpoint = _validate_header(
        rows,
        path,
        TEACHER_CONTRACT_PREFIX,
        EXPECTED_TEACHER_SELECTION_SOURCE,
    )
    _validate_terminal_rows(rows, path, first_pair, pair_count)
    expected_episodes = {pair * 2 + seat for pair in range(first_pair, first_pair + pair_count) for seat in (0, 1)}
    seen: set[tuple[Any, ...]] = set()
    for teacher_ordinal, row in enumerate(decisions):
        if row.get("teacher_decision_ordinal") != teacher_ordinal:
            _fail(f"{path}: opponent teacher decision ordinals are not contiguous")
        if row.get("candidate_seat") not in {"p0", "p1"}:
            _fail(f"{path}: invalid opponent teacher candidate seat")
        if row.get("acting_player") not in {"p0", "p1"}:
            _fail(f"{path}: invalid opponent teacher acting seat")
        if row.get("episode_id") not in expected_episodes:
            _fail(f"{path}: opponent teacher decision escaped task")
        if row.get("acting_player") == row.get("candidate_seat"):
            _fail(f"{path}: opponent teacher decision is candidate-controlled")
        _validate_action_row(row, path, "opponent teacher")
        key = _decision_key(row)
        if key in seen:
            _fail(f"{path}: duplicate opponent teacher decision key {key}")
        seen.add(key)
    if not decisions:
        _fail(f"{path}: no opponent teacher decisions")
    return {
        "decisions": len(decisions),
        "terminals": len(_terminal_rows(rows)),
        "checkpoint": checkpoint,
        "terminal_payloads": {
            (row["pair_index"], row["episode_id"], row["candidate_seat"]): row["terminal"]
            for row in _terminal_rows(rows)
        },
    }


def _diagnostics_from_log(path: Path) -> list[dict[str, Any]]:
    diagnostics: list[dict[str, Any]] = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        marker = line.find(DIAGNOSTIC_PREFIX)
        if marker < 0:
            continue
        payload = line[marker + len(DIAGNOSTIC_PREFIX) :].strip()
        try:
            row = json.loads(payload)
        except json.JSONDecodeError as error:
            _fail(f"{path}:{line_number}: invalid search diagnostic: {error}")
        if not isinstance(row, dict):
            _fail(f"{path}:{line_number}: search diagnostic is not an object")
        diagnostics.append(row)
    if not diagnostics:
        _fail(f"{path}: no search diagnostics")
    return diagnostics


def _validate_diagnostics(
    path: Path,
    diagnostics: list[dict[str, Any]],
    outcomes: dict[tuple[Any, ...], dict[str, Any]],
) -> dict[str, Any]:
    by_key: set[tuple[Any, ...]] = set()
    by_seat = {"p0": {"labels": 0, "overrides": 0}, "p1": {"labels": 0, "overrides": 0}}
    by_episode: dict[tuple[str, int], list[int]] = {}
    for row in diagnostics:
        if row.get("schema") != DIAGNOSTIC_SCHEMA:
            _fail(f"{path}: search diagnostic schema mismatch")
        seat = row.get("candidate_seat")
        if seat not in by_seat:
            _fail(f"{path}: invalid search diagnostic seat")
        episode = row.get("episode_id")
        pair = row.get("pair_index")
        if (
            not _is_int(episode)
            or not _is_int(pair)
            or episode != pair * 2 + int(seat[1])
        ):
            _fail(f"{path}: search pair, episode, and seat disagree")
        legal = row.get("legal_action_count")
        if not _is_int(legal) or not 2 <= legal <= 8:
            _fail(f"{path}: search legal action count must be in [2, 8]")
        if (
            row.get("substep_index") != 0
            or row.get("substep_count") != 1
            or not _is_int(row.get("actor_physical_decision_ordinal"))
            or row["actor_physical_decision_ordinal"] < 20
        ):
            _fail(f"{path}: search eligibility fields mismatch")
        if row.get("continuation_steps") != 8:
            _fail(f"{path}: continuation_steps must equal 8")
        if row.get("information_set_samples") != 4:
            _fail(f"{path}: information_set_samples must equal 4")
        hashes = row.get("information_set_sample_hashes_u64_hex")
        if (
            not isinstance(hashes, list)
            or len(hashes) != 4
            or len(set(hashes)) != 4
            or not all(_is_hex(value, 16) for value in hashes)
        ):
            _fail(f"{path}: information-set hash coverage invalid")
        value_bits = row.get("action_values_f32_bits_hex")
        semantics = row.get("action_semantics")
        if (
            not isinstance(value_bits, list)
            or len(value_bits) != legal
            or not all(_is_hex(value, 8) for value in value_bits)
        ):
            _fail(f"{path}: action value width mismatch")
        values = [_f32_from_hex(value) for value in value_bits]
        if not all(math.isfinite(value) for value in values):
            _fail(f"{path}: action value is non-finite")
        if not isinstance(semantics, list) or len(semantics) != legal:
            _fail(f"{path}: diagnostic action semantics width mismatch")
        branch_count = row.get("branch_count")
        natural = row.get("natural_terminal_branch_count")
        bootstrap = row.get("critic_bootstrap_branch_count")
        if (
            not all(_is_int(value) and value >= 0 for value in (branch_count, natural, bootstrap))
            or branch_count != legal * 4
            or natural + bootstrap != branch_count
        ):
            _fail(f"{path}: branch accounting mismatch")
        fallback = row.get("fallback_selected_index")
        best = row.get("best_search_index")
        teacher = row.get("teacher_selected_index")
        if not all(_is_int(value) and 0 <= value < legal for value in (fallback, best, teacher)):
            _fail(f"{path}: search index out of range")
        expected_best = fallback
        for index, value in enumerate(values):
            if value > values[expected_best]:
                expected_best = index
        margin = _f32_from_hex(row.get("search_margin_f32_bits_hex"))
        expected_margin_hex = _f32_bits_hex(values[expected_best] - values[fallback])
        expected_teacher = (
            expected_best
            if expected_best != fallback and margin >= SEARCH_OVERRIDE_MARGIN
            else fallback
        )
        if (
            not math.isfinite(margin)
            or best != expected_best
            or row.get("search_margin_f32_bits_hex") != expected_margin_hex
            or teacher != expected_teacher
        ):
            _fail(f"{path}: search target rule mismatch")
        differs = row.get("teacher_differs_from_fallback")
        if type(differs) is not bool or differs != (teacher != fallback):
            _fail(f"{path}: override flag mismatch")
        if (
            not _is_hex(row.get("model_input_sha256"), 64)
            or not _is_hex(row.get("public_history_sha256"), 64)
            or not _is_hex(row.get("candidate_order_commitment_128_hex"), 32)
            or not _is_hex(row.get("candidate_action_seed_u64_hex"), 16)
            or not _is_hex(row.get("diagnostic_state_hash_u64_hex"), 16)
            or not _is_hex(row.get("core_environment_hash_u64_hex"), 16)
        ):
            _fail(f"{path}: diagnostic commitments are invalid")
        key = _decision_key(row)
        if key in by_key:
            _fail(f"{path}: duplicate search diagnostic key {key}")
        by_key.add(key)
        outcome = outcomes.get(key)
        if outcome is None:
            _fail(f"{path}: diagnostic does not join outcome {key}")
        if (
            fallback != outcome.get("selected_index")
            or row.get("pair_index") != outcome.get("pair_index")
            or legal != outcome.get("legal_action_count")
            or row["model_input_sha256"] != outcome.get("model_input_sha256")
            or row.get("environment_revision") != outcome.get("environment_revision")
            or row.get("substep_count") != outcome.get("substep_count")
            or row.get("actor_physical_decision_ordinal")
            != outcome.get("actor_physical_decision_ordinal")
            or row.get("candidate_order_commitment_128_hex")
            != outcome.get("candidate_order_commitment_128_hex")
            or semantics != outcome.get("action_semantics")
        ):
            _fail(f"{path}: diagnostic does not exactly match outcome decision {key}")
        by_seat[seat]["labels"] += 1
        by_seat[seat]["overrides"] += int(differs)
        episode_counts = by_episode.setdefault((seat, row["episode_id"]), [0, 0])
        episode_counts[0] += 1
        episode_counts[1] += int(differs)
    for seat, values in by_seat.items():
        values["override_rate"] = values["overrides"] / values["labels"] if values["labels"] else 0.0
        episodes = [counts for (episode_seat, _), counts in by_episode.items() if episode_seat == seat]
        values["label_episode_mass"] = float(len(episodes))
        values["override_episode_mass"] = sum(
            overrides / labels for labels, overrides in episodes
        )
        values["equal_episode_mass_override_rate"] = (
            values["override_episode_mass"] / values["label_episode_mass"]
            if values["label_episode_mass"]
            else 0.0
        )
    return {"diagnostics": len(diagnostics), "by_candidate_seat": by_seat}


def _validate_task_outputs(
    log_path: Path,
    outcome_path: Path,
    teacher_path: Path,
    first_pair: int,
    pair_count: int,
) -> dict[str, Any]:
    outcomes, export_stats = _validate_export_pair(
        outcome_path, teacher_path, first_pair, pair_count
    )
    diagnostics = _diagnostics_from_log(log_path)
    diagnostic_stats = _validate_diagnostics(log_path, diagnostics, outcomes)
    return {**export_stats, **diagnostic_stats}


def _validate_export_pair(
    outcome_path: Path,
    teacher_path: Path,
    first_pair: int,
    pair_count: int,
) -> tuple[dict[tuple[Any, ...], dict[str, Any]], dict[str, Any]]:
    outcomes, outcome_stats = _validate_outcome(outcome_path, first_pair, pair_count)
    teacher_stats = _validate_opponent_teacher(teacher_path, first_pair, pair_count)
    if outcome_stats["checkpoint"] != teacher_stats["checkpoint"]:
        _fail("outcome and opponent-teacher checkpoint identities differ")
    if outcome_stats["terminal_payloads"] != teacher_stats["terminal_payloads"]:
        _fail("outcome and opponent-teacher terminal payloads differ")
    return outcomes, {
        "outcome_decisions": outcome_stats["decisions"],
        "outcome_terminals": outcome_stats["terminals"],
        "opponent_teacher_decisions": teacher_stats["decisions"],
        "opponent_teacher_terminals": teacher_stats["terminals"],
    }


def build_topology_screen_commands(args: argparse.Namespace) -> list[list[str]]:
    """Return, without running, 1/2/4-worker revealed-root screen commands."""
    revealed_root = Path(args.revealed_root).resolve()
    commands: list[list[str]] = []
    for workers in TOPOLOGY_WORKERS:
        commands.append(
            [
                sys.executable,
                str(Path(__file__).resolve()),
                "--evidence-root",
                str(revealed_root / f"workers-{workers}"),
                "--mage-repo",
                str(Path(args.mage_repo).resolve()),
                "--scorer-exe",
                str(Path(args.scorer_exe).resolve()),
                "--reference-scorer-exe",
                str(Path(args.reference_scorer_exe).resolve()),
                "--outcome-root",
                str(Path(args.outcome_root).resolve()),
                "--source-database",
                str(Path(args.source_database).resolve()),
                "--maven",
                str(Path(args.maven).resolve()),
                "--base-seed",
                str(args.base_seed),
                "--pair-start",
                str(args.pair_start),
                "--pairs",
                str(args.topology_pairs),
                "--workers",
                str(workers),
                "--task-pairs",
                str(min(4, args.topology_pairs)),
            ]
        )
    return commands


def _environment(database_root: Path) -> dict[str, str]:
    environment = os.environ.copy()
    environment.update(
        {
            "MAGE_DB_DIR": str(database_root),
            "MAGE_DB_AUTO_SERVER": "false",
            "AI_DETERMINISTIC_TIEBREAKS": "true",
            "AI_DETERMINISTIC_SEARCH": "true",
            "AI_DETERMINISTIC_MAX_NODES": "5000",
            "AI_MAX_THREADS_FOR_SIMULATIONS": "1",
        }
    )
    return environment


def _prepare_workers(args: argparse.Namespace) -> list[Path]:
    source_sha256 = _sha256(args.source_database)
    roots: list[Path] = []
    for worker in range(args.workers):
        root = args.evidence_root / "workers" / f"worker-{worker:02d}" / "db"
        root.mkdir(parents=True, exist_ok=True)
        destination = root / "cards.h2.mv.db"
        if destination.exists():
            _fail(f"worker database already exists: {destination}")
        shutil.copyfile(args.source_database, destination)
        if _sha256(destination) != source_sha256:
            _fail(f"worker database copy mismatch: {destination}")
        roots.append(root)
    return roots


def _anchor_command(
    args: argparse.Namespace,
    scorer_exe: Path,
    first_pair: int,
    pair_count: int,
    teacher: Path,
    outcome: Path,
) -> list[str]:
    exec_args = " ".join(
        (
            "--repo-root",
            str(args.mage_repo),
            "--scorer-exe",
            str(scorer_exe),
            "--outcome-root",
            str(args.outcome_root),
            "--base-seed",
            str(args.base_seed),
            "--first-episode",
            str(first_pair * 2),
            "--pairs",
            str(pair_count),
            "--opponent cp7 --cp7-skill 7",
            "--teacher-export",
            str(teacher),
            "--outcome-export",
            str(outcome),
        )
    )
    return [
        str(args.maven),
        "-o",
        "-q",
        "-pl",
        "Mage.Server.Plugins/Mage.Player.AIRL",
        "-DskipTests",
        "exec:java",
        "-Dexec.mainClass=mage.player.ai.rl.XMageRallyAnchorSpike",
        f"-Dexec.args={exec_args}",
    ]


def _run_anchor(
    args: argparse.Namespace,
    database_root: Path,
    scorer_exe: Path,
    first_pair: int,
    pair_count: int,
    teacher: Path,
    outcome: Path,
    log: Path,
) -> float:
    started = time.perf_counter()
    with log.open("x", encoding="utf-8", newline="\n") as handle:
        process = subprocess.Popen(
            _anchor_command(
                args,
                scorer_exe,
                first_pair,
                pair_count,
                teacher,
                outcome,
            ),
            cwd=args.mage_repo,
            env=_environment(database_root),
            stdout=handle,
            stderr=subprocess.STDOUT,
        )
        try:
            returncode = process.wait(timeout=args.task_timeout_seconds)
        except subprocess.TimeoutExpired:
            _terminate_process_tree(process)
            _fail(
                f"anchor exceeded {args.task_timeout_seconds} second timeout; see {log}"
            )
    if returncode != 0:
        _fail(f"anchor exited {returncode}; see {log}")
    final_lines = [
        line
        for line in log.read_text(encoding="utf-8").splitlines()
        if line.startswith("XMAGE_RALLY_ANCHOR_SPIKE PASS ")
    ]
    if len(final_lines) != 1 or " elapsed_ms=" not in final_lines[0]:
        _fail(f"anchor lacks one completed-run summary; see {log}")
    return time.perf_counter() - started


def _terminate_process_tree(process: subprocess.Popen[Any]) -> None:
    if process.poll() is not None:
        return
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/PID", str(process.pid), "/T", "/F"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=30,
            check=False,
        )
    else:
        process.kill()
    try:
        process.wait(timeout=30)
    except subprocess.TimeoutExpired:
        _fail(f"process tree {process.pid} did not terminate")
    if process.poll() is None:
        _fail(f"process tree {process.pid} remains live after termination")


def _require_identical_shadow_exports(
    stem: str,
    reference_outcome: Path,
    outcome: Path,
    reference_teacher: Path,
    teacher: Path,
) -> None:
    if _sha256(reference_outcome) != _sha256(outcome):
        _fail(f"task {stem} shadow outcome stream changed the live trajectory")
    if _sha256(reference_teacher) != _sha256(teacher):
        _fail(f"task {stem} shadow opponent-teacher stream changed the live trajectory")


def _run_task(
    args: argparse.Namespace,
    worker: int,
    database_root: Path,
    first_pair: int,
    pair_count: int,
) -> dict[str, Any]:
    stem = f"p{first_pair:06d}-n{pair_count:03d}"
    task_root = args.evidence_root / "tasks"
    reference_teacher = task_root / f"{stem}.reference.teacher.jsonl"
    reference_outcome = task_root / f"{stem}.reference.outcome.jsonl"
    reference_log = task_root / f"{stem}.reference.log"
    teacher = task_root / f"{stem}.teacher.jsonl"
    outcome = task_root / f"{stem}.outcome.jsonl"
    log = task_root / f"{stem}.log"
    try:
        for path in (
            reference_teacher,
            reference_outcome,
            reference_log,
            teacher,
            outcome,
            log,
        ):
            if path.exists():
                _fail(f"task output already exists: {path}")
        reference_elapsed_seconds = _run_anchor(
            args,
            database_root,
            args.reference_scorer_exe,
            first_pair,
            pair_count,
            reference_teacher,
            reference_outcome,
            reference_log,
        )
        _validate_export_pair(
            reference_outcome,
            reference_teacher,
            first_pair,
            pair_count,
        )
        shadow_elapsed_seconds = _run_anchor(
            args,
            database_root,
            args.scorer_exe,
            first_pair,
            pair_count,
            teacher,
            outcome,
            log,
        )
        _require_identical_shadow_exports(
            stem,
            reference_outcome,
            outcome,
            reference_teacher,
            teacher,
        )
        stats = _validate_task_outputs(log, outcome, teacher, first_pair, pair_count)
        elapsed_seconds = reference_elapsed_seconds + shadow_elapsed_seconds
        return {
            "worker": worker,
            "first_pair": first_pair,
            "pair_count": pair_count,
            "games": pair_count * 2,
            "elapsed_seconds": elapsed_seconds,
            "games_per_second": pair_count * 2 / elapsed_seconds,
            "reference_elapsed_seconds": reference_elapsed_seconds,
            "shadow_elapsed_seconds": shadow_elapsed_seconds,
            "shadow_noninterference": "byte_identical_export_streams",
            "reference_log_path": str(reference_log),
            "reference_log_sha256": _sha256(reference_log),
            "reference_teacher_path": str(reference_teacher),
            "reference_teacher_sha256": _sha256(reference_teacher),
            "reference_outcome_path": str(reference_outcome),
            "reference_outcome_sha256": _sha256(reference_outcome),
            "log_path": str(log),
            "log_sha256": _sha256(log),
            "teacher_path": str(teacher),
            "teacher_sha256": _sha256(teacher),
            "outcome_path": str(outcome),
            "outcome_sha256": _sha256(outcome),
            **stats,
        }
    except Exception as error:
        try:
            (task_root / f"{stem}.failed.txt").write_text(
                f"{type(error).__name__}: {error}\n",
                encoding="utf-8",
            )
        except OSError:
            pass
        raise


def collect(args: argparse.Namespace) -> dict[str, Any]:
    if args.evidence_root.exists():
        _fail(f"evidence root already exists: {args.evidence_root}")
    args.evidence_root.mkdir(parents=True)
    (args.evidence_root / "tasks").mkdir()
    roots = _prepare_workers(args)
    ranges = [
        (first, min(args.task_pairs, args.pair_start + args.pairs - first))
        for first in range(args.pair_start, args.pair_start + args.pairs, args.task_pairs)
    ]
    started = time.perf_counter()
    results: list[dict[str, Any]] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
        pending_ranges = iter(ranges)
        active: dict[concurrent.futures.Future[dict[str, Any]], tuple[int, Path]] = {}
        for worker, root in enumerate(roots):
            try:
                first, count = next(pending_ranges)
            except StopIteration:
                break
            active[executor.submit(_run_task, args, worker, root, first, count)] = (
                worker,
                root,
            )
        try:
            while active:
                done, _ = concurrent.futures.wait(
                    active,
                    return_when=concurrent.futures.FIRST_COMPLETED,
                )
                completed: list[tuple[dict[str, Any], tuple[int, Path]]] = []
                for future in done:
                    slot = active.pop(future)
                    completed.append((future.result(), slot))
                for result, (worker, root) in completed:
                    results.append(result)
                    try:
                        first, count = next(pending_ranges)
                    except StopIteration:
                        continue
                    active[executor.submit(_run_task, args, worker, root, first, count)] = (
                        worker,
                        root,
                    )
        except Exception:
            for future in active:
                future.cancel()
            raise
    results.sort(key=lambda result: result["first_pair"])
    total_labels = sum(result["diagnostics"] for result in results)
    by_seat = {
        "p0": {
            "labels": 0,
            "overrides": 0,
            "label_episode_mass": 0.0,
            "override_episode_mass": 0.0,
        },
        "p1": {
            "labels": 0,
            "overrides": 0,
            "label_episode_mass": 0.0,
            "override_episode_mass": 0.0,
        },
    }
    for result in results:
        for seat in by_seat:
            by_seat[seat]["labels"] += result["by_candidate_seat"][seat]["labels"]
            by_seat[seat]["overrides"] += result["by_candidate_seat"][seat]["overrides"]
            by_seat[seat]["label_episode_mass"] += result["by_candidate_seat"][seat][
                "label_episode_mass"
            ]
            by_seat[seat]["override_episode_mass"] += result["by_candidate_seat"][seat][
                "override_episode_mass"
            ]
    for values in by_seat.values():
        values["override_rate"] = values["overrides"] / values["labels"] if values["labels"] else 0.0
        values["equal_episode_mass_override_rate"] = (
            values["override_episode_mass"] / values["label_episode_mass"]
            if values["label_episode_mass"]
            else 0.0
        )
    target_quality_gate = {
        "minimum_equal_episode_mass_override_rate_per_seat": 0.05,
        "p0": by_seat["p0"]["equal_episode_mass_override_rate"] >= 0.05,
        "p1": by_seat["p1"]["equal_episode_mass_override_rate"] >= 0.05,
    }
    target_quality_gate["pass"] = target_quality_gate["p0"] and target_quality_gate["p1"]
    elapsed_seconds = time.perf_counter() - started
    report = {
        "schema": SCHEMA,
        "status": "complete",
        "base_seed": args.base_seed,
        "pair_start": args.pair_start,
        "pairs": args.pairs,
        "games": args.pairs * 2,
        "workers": args.workers,
        "task_pairs": args.task_pairs,
        "elapsed_seconds": elapsed_seconds,
        "games_per_second": (args.pairs * 2) / max(elapsed_seconds, 1.0e-12),
        "search_diagnostics": total_labels,
        "by_candidate_seat": by_seat,
        "target_quality_gate": target_quality_gate,
        "advance_to_training": target_quality_gate["pass"],
        "inputs": {
            "kernel_repo": str(Path(__file__).resolve().parents[2]),
            "mage_repo": str(args.mage_repo),
            "scorer_exe": str(args.scorer_exe),
            "scorer_sha256": _sha256(args.scorer_exe),
            "reference_scorer_exe": str(args.reference_scorer_exe),
            "reference_scorer_sha256": _sha256(args.reference_scorer_exe),
            "outcome_root": str(args.outcome_root),
            "source_database": str(args.source_database),
            "source_database_sha256": _sha256(args.source_database),
            "maven": str(args.maven),
            "gpu_ordinal": None,
            "workload_device": "cpu",
        },
        "git_commit": _version(
            ["git", "rev-parse", "HEAD"], Path(__file__).resolve().parents[2]
        ),
        "collector_script_sha256": _sha256(Path(__file__).resolve()),
        "toolchain": {
            "python": sys.version.split()[0],
            "java": _version(["java", "-version"]),
            "maven": _version([str(args.maven), "--version"]),
            "rustc": _version(["rustc", "--version"]),
            "cargo": _version(["cargo", "--version"]),
        },
        "tasks": results,
    }
    _atomic_fresh_json(args.evidence_root / "report.json", report)
    return report


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-root", type=Path, required=True)
    parser.add_argument("--mage-repo", type=Path, required=True)
    parser.add_argument("--scorer-exe", type=Path, required=True)
    parser.add_argument("--reference-scorer-exe", type=Path, required=True)
    parser.add_argument("--outcome-root", type=Path, required=True)
    parser.add_argument("--source-database", type=Path, required=True)
    parser.add_argument("--maven", type=Path, required=True)
    parser.add_argument("--base-seed", type=int, default=1_400_001)
    parser.add_argument("--pair-start", type=int, default=0)
    parser.add_argument("--pairs", type=int, default=256)
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--task-pairs", type=int, default=4)
    parser.add_argument("--task-timeout-seconds", type=int, default=7_200)
    parser.add_argument("--topology-screen", action="store_true")
    parser.add_argument("--revealed-root", type=Path)
    parser.add_argument("--topology-pairs", type=int, default=16)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if (
        args.base_seed < 0
        or args.pair_start < 0
        or args.pairs < 1
        or args.workers not in TOPOLOGY_WORKERS
    ):
        _fail("invalid collector arguments")
    if not 1 <= args.task_pairs <= 128 or args.task_timeout_seconds < 60:
        _fail("invalid task arguments")
    if args.topology_screen:
        if args.revealed_root is None or args.topology_pairs < 16:
            _fail("--topology-screen requires --revealed-root and at least 16 revealed pairs")
        for command in build_topology_screen_commands(args):
            print(subprocess.list2cmdline(command))
        return 0
    for attribute in (
        "mage_repo",
        "scorer_exe",
        "reference_scorer_exe",
        "outcome_root",
        "source_database",
        "maven",
    ):
        setattr(args, attribute, getattr(args, attribute).resolve(strict=True))
    args.evidence_root = args.evidence_root.resolve()
    report = collect(args)
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
