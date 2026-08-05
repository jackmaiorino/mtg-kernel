#!/usr/bin/env python3
"""Finalize the immutable V4 CP7 panel after the attempt-01 parser defect."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))
import run_cp7_gate_v4 as gate  # noqa: E402


RETRY_MANIFEST_SCHEMA = (
    "mtg-kernel-current-net8-cp7-terminal-response-v4-analysis-retry-manifest/v1"
)
ORIGINAL_KERNEL_COMMIT = "e945f7efb79560b08fa497edefe569f8aaf6ed23"
ORIGINAL_RUNNER_SHA256 = "9ccf57635990e8715e1964cc3e4c2a30b40b46edf44802d200c87268ef5457dc"
ORIGINAL_MANIFEST_SHA256 = "0e152a54be04c39c660352b7c1d7a4ffb78bb7422e763be56914e0907cd583aa"
SEALED_STATE_SHA256 = "e245adc9dfd54238b63959d31f10d64f987104e9701504671c2aad1fa47b6f53"
FAILED_STDERR_SHA256 = "7eeeb1c3c4b526c592305b4d2ac216c881877aeb495cbe2fb8600f59c92e23ee"
ORIGINAL_RUNNER_REPO_PATH = (
    "scripts/current_net8_cp7_terminal_response_v4/run_cp7_gate_v4.py"
)
EXPECTED_ERROR = (
    "run_cp7_gate_v4: ERROR: "
    "D:\\mtg-kernel-current-net8-cp7-terminal-response-v4\\"
    "cp7-skill7-base1840001-attempt-01\\tasks\\"
    "baseline-p000000-n032.outcome.jsonl: pair receipt mismatch at 1"
)


def fail(message: str) -> None:
    raise ValueError(message)


def load_json(path: Path) -> dict[str, Any]:
    return gate.load_json(path)


def _git_blob_sha256(repo: Path, commit: str, relative_path: str) -> str:
    completed = subprocess.run(
        [
            "git",
            "-c",
            "safe.directory=" + repo.as_posix(),
            "-C",
            str(repo),
            "show",
            f"{commit}:{relative_path}",
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    )
    return hashlib.sha256(completed.stdout).hexdigest()


def _require_ancestor(repo: Path, ancestor: str, descendant: str) -> None:
    completed = subprocess.run(
        [
            "git",
            "-c",
            "safe.directory=" + repo.as_posix(),
            "-C",
            str(repo),
            "merge-base",
            "--is-ancestor",
            ancestor,
            descendant,
        ],
        check=False,
    )
    if completed.returncode != 0:
        fail("analysis repair commit does not descend from the formal runner commit")


def validate_original_manifest(path: Path) -> dict[str, Any]:
    if not path.is_file() or gate.common.sha256(path) != ORIGINAL_MANIFEST_SHA256:
        fail("original CP7 manifest SHA-256 mismatch")
    manifest = load_json(path)
    root = path.parent.resolve()
    kernel_repo = SCRIPT_DIR.parents[1]
    gate._require_clean_worktree(kernel_repo, "kernel")
    current_commit = gate.common._git_commit(kernel_repo)
    _require_ancestor(kernel_repo, ORIGINAL_KERNEL_COMMIT, current_commit)
    if (
        manifest.get("schema") != gate.MANIFEST_SCHEMA
        or manifest.get("kernel_git_commit") != ORIGINAL_KERNEL_COMMIT
        or manifest.get("runner_sha256") != ORIGINAL_RUNNER_SHA256
        or manifest.get("panel") != gate.PANEL
        or manifest.get("gates") != gate.GATES
        or manifest.get("topology") != gate.TOPOLOGY
        or manifest.get("analysis_policy")
        != {
            "outcomes_parsed_only_after_all_tasks_complete": True,
            "terminal_win_draw_loss_only": True,
        }
    ):
        fail("original CP7 manifest formal contract mismatch")
    if (
        _git_blob_sha256(kernel_repo, ORIGINAL_KERNEL_COMMIT, ORIGINAL_RUNNER_REPO_PATH)
        != ORIGINAL_RUNNER_SHA256
    ):
        fail("historical CP7 runner blob mismatch")

    mage_repo = gate._absolute_path(manifest.get("mage_repo"), "Mage repository")
    gate._require_clean_worktree(mage_repo, "Mage")
    if manifest.get("mage_git_commit") != gate.common._git_commit(mage_repo):
        fail("original CP7 Mage commit mismatch")
    scorer = manifest.get("scorer")
    if (
        not isinstance(scorer, dict)
        or scorer.get("sha256") != gate.SCORER_SHA256
        or scorer.get("source_git_commit") != gate.SCORER_SOURCE_GIT_COMMIT
        or gate.common.sha256(gate._absolute_path(scorer.get("path"), "scorer"))
        != gate.SCORER_SHA256
    ):
        fail("original CP7 scorer binding mismatch")
    database = manifest.get("source_database")
    if (
        not isinstance(database, dict)
        or database.get("sha256") != gate.common.CARD_DATABASE_SHA256
        or gate.common.sha256(
            gate._absolute_path(database.get("path"), "source database")
        )
        != gate.common.CARD_DATABASE_SHA256
    ):
        fail("original CP7 database binding mismatch")
    dependencies = manifest.get("runtime_dependencies")
    if not isinstance(dependencies, dict) or set(dependencies) != {
        "collector",
        "outcome_validator",
        "maven",
    }:
        fail("original CP7 runtime dependency inventory mismatch")
    for record in dependencies.values():
        if not isinstance(record, dict) or set(record) != {"path", "sha256", "byte_count"}:
            fail("original CP7 runtime dependency binding is malformed")
        dependency_path = gate._absolute_path(record["path"], "runtime dependency")
        if (
            not dependency_path.is_file()
            or dependency_path.stat().st_size != record["byte_count"]
            or gate.common.sha256(dependency_path) != record["sha256"]
        ):
            fail("original CP7 runtime dependency bytes mismatch")
    arms = manifest.get("arms")
    if not isinstance(arms, dict) or set(arms) != set(gate.ARM_ORDER):
        fail("original CP7 arm inventory mismatch")
    packages = {}
    for arm in gate.ARM_ORDER:
        package = gate.load_exact_package(
            gate._absolute_path(arms[arm].get("root"), f"{arm} package"), arm
        )
        if package != arms[arm]:
            fail(f"original CP7 {arm} package binding mismatch")
        packages[arm] = package
    pool3 = manifest.get("prerequisites", {}).get("pool3_report")
    if not isinstance(pool3, dict) or set(pool3) != {"path", "sha256"}:
        fail("original CP7 Pool3 prerequisite is malformed")
    gate._validate_pool3_prerequisite(
        gate._absolute_path(pool3["path"], "Pool3 report"), pool3["sha256"]
    )
    outputs = manifest.get("outputs")
    if (
        not isinstance(outputs, dict)
        or gate._absolute_path(outputs.get("evidence_root"), "evidence root") != root
        or gate._absolute_path(outputs.get("report_path"), "report") != root / "report.json"
    ):
        fail("original CP7 output binding mismatch")
    return {
        "manifest": manifest,
        "root": root,
        "kernel_repo": kernel_repo,
        "current_commit": current_commit,
        "packages": packages,
    }


def _validate_sealed_state(path: Path, root: Path) -> dict[str, Any]:
    if not path.is_file() or gate.common.sha256(path) != SEALED_STATE_SHA256:
        fail("sealed CP7 state SHA-256 mismatch")
    state = load_json(path)
    if (
        state.get("schema") != gate.STATE_SCHEMA
        or state.get("manifest_sha256") != ORIGINAL_MANIFEST_SHA256
        or state.get("outcomes_parsed") is not False
        or not isinstance(state.get("wall_seconds"), (int, float))
        or not 0 < float(state["wall_seconds"]) < 7_200
        or not isinstance(state.get("tasks"), list)
        or len(state["tasks"]) != 8
    ):
        fail("sealed CP7 state semantics mismatch")
    expected_tasks = {
        (arm, first_pair, pair_count)
        for first_pair, pair_count in gate._chunk_ranges()
        for arm in gate.ARM_ORDER
    }
    actual_tasks = {
        (task.get("arm"), task.get("first_pair"), task.get("pair_count"))
        for task in state["tasks"]
    }
    if actual_tasks != expected_tasks:
        fail("sealed CP7 task coverage mismatch")
    for task in state["tasks"]:
        for kind in ("log", "outcome"):
            record = task.get(kind)
            if not isinstance(record, dict) or set(record) != {"path", "sha256", "byte_count"}:
                fail(f"sealed CP7 {kind} binding is malformed")
            artifact = gate._absolute_path(record["path"], f"task {kind}")
            if (
                root not in artifact.parents
                or not artifact.is_file()
                or artifact.stat().st_size != record["byte_count"]
                or gate.common.sha256(artifact) != record["sha256"]
            ):
                fail(f"sealed CP7 {kind} bytes mismatch")
    return state


def build_retry_manifest(
    *,
    retry_path: Path,
    original_manifest: Path,
    sealed_state: Path,
    failed_stderr: Path,
) -> dict[str, Any]:
    if retry_path.exists():
        fail(f"analysis retry manifest already exists: {retry_path}")
    validated = validate_original_manifest(original_manifest)
    state = _validate_sealed_state(sealed_state, validated["root"])
    if (
        not failed_stderr.is_file()
        or gate.common.sha256(failed_stderr) != FAILED_STDERR_SHA256
        or failed_stderr.read_text(encoding="utf-8").strip() != EXPECTED_ERROR
    ):
        fail("failed analysis stderr binding mismatch")
    for output in (validated["root"] / "report.json", validated["root"] / "state-final.json"):
        if output.exists():
            fail(f"analysis retry output already exists: {output}")
    return {
        "schema": RETRY_MANIFEST_SCHEMA,
        "analysis_git_commit": validated["current_commit"],
        "analysis_runner": {
            "path": str(Path(__file__).resolve()),
            "sha256": gate.common.sha256(Path(__file__).resolve()),
        },
        "corrected_gate_parser": {
            "path": str((SCRIPT_DIR / "run_cp7_gate_v4.py").resolve()),
            "sha256": gate.common.sha256(SCRIPT_DIR / "run_cp7_gate_v4.py"),
        },
        "original_formal_manifest": {
            "path": str(original_manifest.resolve()),
            "sha256": ORIGINAL_MANIFEST_SHA256,
            "kernel_git_commit": ORIGINAL_KERNEL_COMMIT,
            "runner_sha256": ORIGINAL_RUNNER_SHA256,
        },
        "sealed_state": {
            "path": str(sealed_state.resolve()),
            "sha256": SEALED_STATE_SHA256,
            "wall_seconds": state["wall_seconds"],
        },
        "failed_analysis_stderr": {
            "path": str(failed_stderr.resolve()),
            "sha256": FAILED_STDERR_SHA256,
            "error": EXPECTED_ERROR,
        },
        "defect": (
            "attempt-01 parser incorrectly required randomization_identity "
            "environment-randomization-v2 instead of the observed and arm-matched legacy_v1"
        ),
        "correction": (
            "require legacy_v1 and compare it, deck_ids, base seed, pair seed, "
            "pair, episode, and seat across arms"
        ),
        "execution_policy": {
            "gameplay_rerun": False,
            "task_artifacts_immutable": True,
            "outcomes_parsed_from_sealed_state_only": True,
            "resource_samples_recoverable": False,
        },
        "outputs": {
            "report_path": str((validated["root"] / "report.json").resolve()),
            "state_final_path": str((validated["root"] / "state-final.json").resolve()),
        },
    }


def validate_retry_manifest(path: Path) -> dict[str, Any]:
    retry = load_json(path)
    if retry.get("schema") != RETRY_MANIFEST_SCHEMA:
        fail("CP7 analysis retry schema mismatch")
    runner = retry.get("analysis_runner")
    parser = retry.get("corrected_gate_parser")
    if (
        not isinstance(runner, dict)
        or runner.get("path") != str(Path(__file__).resolve())
        or runner.get("sha256") != gate.common.sha256(Path(__file__).resolve())
        or not isinstance(parser, dict)
        or parser.get("path") != str((SCRIPT_DIR / "run_cp7_gate_v4.py").resolve())
        or parser.get("sha256") != gate.common.sha256(SCRIPT_DIR / "run_cp7_gate_v4.py")
    ):
        fail("CP7 analysis retry code binding mismatch")
    original = retry.get("original_formal_manifest")
    state_record = retry.get("sealed_state")
    stderr_record = retry.get("failed_analysis_stderr")
    if not all(isinstance(record, dict) for record in (original, state_record, stderr_record)):
        fail("CP7 analysis retry evidence binding is malformed")
    if (
        original.get("sha256") != ORIGINAL_MANIFEST_SHA256
        or original.get("kernel_git_commit") != ORIGINAL_KERNEL_COMMIT
        or original.get("runner_sha256") != ORIGINAL_RUNNER_SHA256
        or state_record.get("sha256") != SEALED_STATE_SHA256
        or stderr_record.get("sha256") != FAILED_STDERR_SHA256
    ):
        fail("CP7 analysis retry evidence identity mismatch")
    original_path = gate._absolute_path(original["path"], "original formal manifest")
    validated = validate_original_manifest(original_path)
    state_path = gate._absolute_path(state_record["path"], "sealed state")
    state = _validate_sealed_state(state_path, validated["root"])
    if state_record.get("wall_seconds") != state["wall_seconds"]:
        fail("CP7 analysis retry wall-time binding mismatch")
    stderr_path = gate._absolute_path(stderr_record["path"], "failed stderr")
    if (
        stderr_record.get("sha256") != FAILED_STDERR_SHA256
        or gate.common.sha256(stderr_path) != FAILED_STDERR_SHA256
        or stderr_record.get("error") != EXPECTED_ERROR
    ):
        fail("CP7 analysis retry stderr mismatch")
    if retry.get("analysis_git_commit") != validated["current_commit"]:
        fail("CP7 analysis retry commit mismatch")
    if retry.get("execution_policy") != {
        "gameplay_rerun": False,
        "task_artifacts_immutable": True,
        "outcomes_parsed_from_sealed_state_only": True,
        "resource_samples_recoverable": False,
    }:
        fail("CP7 analysis retry execution policy mismatch")
    outputs = retry.get("outputs")
    report_path = gate._absolute_path(outputs.get("report_path"), "report")
    state_final_path = gate._absolute_path(outputs.get("state_final_path"), "state final")
    if report_path != validated["root"] / "report.json" or state_final_path != validated["root"] / "state-final.json":
        fail("CP7 analysis retry output binding mismatch")
    return {
        **validated,
        "retry": retry,
        "retry_path": path.resolve(),
        "state": state,
        "state_path": state_path,
        "report_path": report_path,
        "state_final_path": state_final_path,
    }


def finalize(retry_manifest_path: Path) -> dict[str, Any]:
    validated = validate_retry_manifest(retry_manifest_path)
    for output in (validated["report_path"], validated["state_final_path"]):
        if output.exists():
            raise FileExistsError(f"CP7 analysis retry output already exists: {output}")
    by_arm: dict[str, dict[tuple[int, str], dict[str, Any]]] = {
        arm: {} for arm in gate.ARM_ORDER
    }
    arm_validation = {
        arm: {"decision_count": 0, "record_count": 0, "episode_count": 0}
        for arm in gate.ARM_ORDER
    }
    validated_tasks = []
    for task in validated["state"]["tasks"]:
        gate._validate_task_log(task)
        outcome_path = Path(task["outcome"]["path"])
        parsed = gate._validate_outcome_shard(
            outcome_path,
            arm=task["arm"],
            first_pair=task["first_pair"],
            pair_count=task["pair_count"],
        )
        if parsed["sha256"] != task["outcome"]["sha256"]:
            fail("CP7 outcome changed during analysis retry")
        for key, terminal in parsed["terminals"].items():
            if key in by_arm[task["arm"]]:
                fail(f"duplicate CP7 terminal across retry shards: {task['arm']} {key}")
            by_arm[task["arm"]][key] = terminal
        for field in arm_validation[task["arm"]]:
            arm_validation[task["arm"]][field] += parsed[field]
        validated_tasks.append(
            {
                **task,
                "outcome_validation": {
                    key: value for key, value in parsed.items() if key != "terminals"
                },
            }
        )
    expected_keys = {
        (pair, seat)
        for pair in range(gate.PANEL["pairs"])
        for seat in ("p0", "p1")
    }
    if any(set(by_arm[arm]) != expected_keys for arm in gate.ARM_ORDER):
        fail("CP7 retry complete arm inventory mismatch")
    comparison = gate.adjudicate(by_arm["candidate"], by_arm["baseline"])
    wall_seconds = float(validated["state"]["wall_seconds"])
    report = {
        "schema": gate.REPORT_SCHEMA,
        "status": "pass" if comparison["pass"] else "fail",
        "manifest": {
            "path": str(Path(validated["retry"]["original_formal_manifest"]["path"])),
            "sha256": ORIGINAL_MANIFEST_SHA256,
        },
        "analysis_retry": {
            "path": str(validated["retry_path"]),
            "sha256": gate.common.sha256(validated["retry_path"]),
            "reason": validated["retry"]["defect"],
            "gameplay_rerun": False,
        },
        "prerequisites": validated["manifest"]["prerequisites"],
        "panel": dict(gate.PANEL),
        "gate_config": dict(gate.GATES),
        "topology": dict(gate.TOPOLOGY),
        "wall_seconds": wall_seconds,
        "achieved_games_per_second": gate.PANEL["episodes_per_arm"] * 2 / wall_seconds,
        "resource_evidence": {
            "status": "not-persisted-after-post-collection-analysis-failure",
            "formal_gate_impact": "none",
        },
        "arms": {
            arm: {
                "package": validated["packages"][arm],
                **arm_validation[arm],
            }
            for arm in gate.ARM_ORDER
        },
        "tasks": sorted(validated_tasks, key=lambda row: (row["first_pair"], row["arm"])),
        "comparison": comparison,
        "nonclaims": validated["manifest"]["nonclaims"],
    }
    gate.common.exclusive_write(
        validated["report_path"], gate.common.canonical_json_bytes(report, indent=2)
    )
    state_final = {
        **validated["state"],
        "outcomes_parsed": True,
        "analysis_retry_manifest_sha256": gate.common.sha256(validated["retry_path"]),
        "report_path": str(validated["report_path"]),
        "report_sha256": gate.common.sha256(validated["report_path"]),
        "state_supersedes_sha256": SEALED_STATE_SHA256,
    }
    gate.common.exclusive_write(
        validated["state_final_path"],
        gate.common.canonical_json_bytes(state_final, indent=2),
    )
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--retry-manifest", type=Path, required=True)
    args = parser.parse_args(argv)
    report = finalize(args.retry_manifest)
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0 if report["status"] == "pass" else 3


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, TypeError, ValueError, subprocess.SubprocessError) as error:
        print(f"finalize_cp7_gate_v4: ERROR: {error}", file=sys.stderr)
        raise SystemExit(1)
