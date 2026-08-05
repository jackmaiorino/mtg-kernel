#!/usr/bin/env python3
"""Screen the 1/2/4-worker search-teacher collection topology.

The default mode only prints the collector commands.  Execution is explicit and
sequential because each topology must consume the same revealed roots.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from typing import Any, Callable


COLLECTOR_PATH = Path(__file__).with_name("collect_search_teacher_corpus_v1.py")
_SPEC = importlib.util.spec_from_file_location("search_teacher_collector", COLLECTOR_PATH)
if _SPEC is None or _SPEC.loader is None:
    raise ImportError(f"cannot load collector: {COLLECTOR_PATH}")
_COLLECTOR = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_COLLECTOR)


SCHEMA = "mtg-kernel-depth8-bounded-value-search-teacher-topology-screen/v1"
GPU_ORDINAL = 1
EXPECTED_PAIRS = 16
EXPECTED_WORKERS = (1, 2, 4)
DEFAULT_POLL_SECONDS = 1.0


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        _fail(f"cannot load child report {path}: {error}")
    if not isinstance(value, dict):
        _fail(f"child report is not an object: {path}")
    return value


def _is_lower_hex(value: Any, width: int) -> bool:
    return (
        isinstance(value, str)
        and len(value) == width
        and all(character in "0123456789abcdef" for character in value)
    )


def _canonical_label_digest(diagnostics: list[dict[str, Any]]) -> str:
    """Hash diagnostic objects in the collector's exact decision-key order."""
    keyed = [(_COLLECTOR._decision_key(row), row) for row in diagnostics]
    keys = [key for key, _ in keyed]
    if len(keys) != len(set(keys)):
        _fail("duplicate diagnostic join key across topology task outputs")
    keyed.sort(key=lambda item: item[0])
    digest = hashlib.sha256()
    for _, row in keyed:
        encoded = json.dumps(
            row,
            sort_keys=True,
            separators=(",", ":"),
            ensure_ascii=False,
            allow_nan=False,
        ).encode("utf-8")
        digest.update(encoded)
        digest.update(b"\n")
    return digest.hexdigest()


def _expected_task_ranges(pair_start: int, pairs: int, task_pairs: int) -> list[tuple[int, int]]:
    return [
        (first, min(task_pairs, pair_start + pairs - first))
        for first in range(pair_start, pair_start + pairs, task_pairs)
    ]


def _validate_task_bindings(
    report: dict[str, Any],
    report_path: Path,
    *,
    base_seed: int,
    pair_start: int,
    pairs: int,
    workers: int,
    validate_artifacts: bool = True,
) -> dict[str, Any]:
    if report.get("schema") != _COLLECTOR.SCHEMA or report.get("status") != "complete":
        _fail(f"invalid child report schema or status: {report_path}")
    if any(
        report.get(field) != expected
        for field, expected in (
            ("base_seed", base_seed),
            ("pair_start", pair_start),
            ("pairs", pairs),
            ("games", pairs * 2),
            ("workers", workers),
        )
    ):
        _fail(f"child report root binding mismatch: {report_path}")
    if pairs != EXPECTED_PAIRS:
        _fail(f"topology screen requires exactly {EXPECTED_PAIRS} pairs")

    task_pairs = report.get("task_pairs")
    if type(task_pairs) is not int or task_pairs < 1:
        _fail(f"child report task_pairs invalid: {report_path}")
    tasks = report.get("tasks")
    if not isinstance(tasks, list):
        _fail(f"child report tasks is not a list: {report_path}")
    expected_ranges = _expected_task_ranges(pair_start, pairs, task_pairs)
    observed_ranges: list[tuple[int, int]] = []
    diagnostics: list[dict[str, Any]] = []
    task_hashes: dict[int, dict[str, str]] = {}
    for task in tasks:
        if not isinstance(task, dict):
            _fail(f"child report contains a non-object task: {report_path}")
        first = task.get("first_pair")
        count = task.get("pair_count")
        if type(first) is not int or type(count) is not int or count < 1:
            _fail(f"invalid task pair range: {report_path}")
        observed_ranges.append((first, count))
        if task.get("games") != count * 2:
            _fail(f"task game count mismatch: {report_path}")
        if task.get("shadow_noninterference") != "byte_identical_export_streams":
            _fail(f"task noninterference binding missing: {report_path}")
        outcome_hash = task.get("outcome_sha256")
        teacher_hash = task.get("teacher_sha256")
        reference_outcome_hash = task.get("reference_outcome_sha256")
        reference_teacher_hash = task.get("reference_teacher_sha256")
        if not all(
            isinstance(value, str) and len(value) == 64
            for value in (outcome_hash, teacher_hash, reference_outcome_hash, reference_teacher_hash)
        ):
            _fail(f"task export hashes missing: {report_path}")
        if outcome_hash != reference_outcome_hash or teacher_hash != reference_teacher_hash:
            _fail(f"task shadow/reference hash binding mismatch: {report_path}")
        task_hashes[first] = {"outcome_sha256": outcome_hash, "teacher_sha256": teacher_hash}
        if validate_artifacts:
            task_path_values = (
                task.get("log_path"),
                task.get("outcome_path"),
                task.get("teacher_path"),
                task.get("reference_outcome_path"),
                task.get("reference_teacher_path"),
            )
            if not all(isinstance(value, str) for value in task_path_values):
                _fail(f"task artifact paths missing: {report_path}")
            log_path, outcome_path, teacher_path, reference_outcome_path, reference_teacher_path = (
                Path(value) for value in task_path_values
            )
            actual_hashes = {
                "outcome_sha256": _sha256(outcome_path),
                "teacher_sha256": _sha256(teacher_path),
                "reference_outcome_sha256": _sha256(reference_outcome_path),
                "reference_teacher_sha256": _sha256(reference_teacher_path),
            }
            expected_hashes = {
                key: task[key]
                for key in (
                    "outcome_sha256",
                    "teacher_sha256",
                    "reference_outcome_sha256",
                    "reference_teacher_sha256",
                )
            }
            if actual_hashes != expected_hashes:
                _fail(f"task artifact hash binding mismatch: {report_path}")
            stats = _COLLECTOR._validate_task_outputs(
                log_path,
                outcome_path,
                teacher_path,
                first,
                count,
            )
            _COLLECTOR._validate_export_pair(
                reference_outcome_path,
                reference_teacher_path,
                first,
                count,
            )
            diagnostics.extend(_COLLECTOR._diagnostics_from_log(log_path))
            if stats.get("diagnostics") != task.get("diagnostics"):
                _fail(f"task diagnostic count binding mismatch: {report_path}")
    if sorted(observed_ranges) != sorted(expected_ranges):
        _fail(f"child report does not cover exactly the 16-pair root range: {report_path}")
    if len(task_hashes) != len(expected_ranges):
        _fail(f"child report has duplicate task roots: {report_path}")
    if not diagnostics and validate_artifacts:
        _fail(f"child report has no search diagnostics: {report_path}")
    diagnostic_count = report.get("search_diagnostics")
    if type(diagnostic_count) is not int or (validate_artifacts and diagnostic_count != len(diagnostics)):
        _fail(f"child report diagnostic count binding mismatch: {report_path}")
    provenance = None
    if validate_artifacts:
        inputs = report.get("inputs")
        toolchain = report.get("toolchain")
        if (
            not _is_lower_hex(report.get("git_commit"), 40)
            or not _is_lower_hex(report.get("collector_script_sha256"), 64)
            or not isinstance(inputs, dict)
            or not isinstance(toolchain, dict)
            or inputs.get("gpu_ordinal") is not None
            or inputs.get("workload_device") != "cpu"
            or any(
                not _is_lower_hex(inputs.get(field), 64)
                for field in (
                    "scorer_sha256",
                    "reference_scorer_sha256",
                    "source_database_sha256",
                )
            )
        ):
            _fail(f"child report provenance is incomplete: {report_path}")
        provenance = {
            "git_commit": report["git_commit"],
            "collector_script_sha256": report["collector_script_sha256"],
            "inputs": inputs,
            "toolchain": toolchain,
        }
    return {
        "diagnostics": diagnostic_count,
        "label_digest": _canonical_label_digest(diagnostics) if diagnostics else None,
        "task_hashes": task_hashes,
        "task_ranges": [list(pair_range) for pair_range in sorted(observed_ranges)],
        "provenance": provenance,
    }


def _query_gpu(
    gpu_ordinal: int = GPU_ORDINAL,
    runner: Callable[..., Any] = subprocess.run,
) -> dict[str, float]:
    try:
        completed = runner(
            [
                "nvidia-smi",
                "-i",
                str(gpu_ordinal),
                "--query-gpu=index,utilization.gpu,memory.used",
                "--format=csv,noheader,nounits",
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    except (OSError, subprocess.SubprocessError) as error:
        _fail(f"nvidia-smi unavailable: {error}")
    if completed.returncode != 0:
        _fail(f"nvidia-smi sample failed: {completed.stderr.strip()}")
    rows = []
    for line in completed.stdout.splitlines():
        fields = [field.strip() for field in line.split(",")]
        if len(fields) != 3:
            _fail("nvidia-smi sample has invalid field count")
        try:
            index = int(fields[0])
            utilization = float(fields[1])
            memory = float(fields[2])
        except ValueError as error:
            _fail(f"nvidia-smi sample is not numeric: {error}")
        if index == gpu_ordinal:
            rows.append((utilization, memory))
    if len(rows) != 1:
        _fail(f"nvidia-smi did not return exactly one GPU {gpu_ordinal} row")
    utilization, memory = rows[0]
    if not (0.0 <= utilization <= 100.0 and memory >= 0.0):
        _fail("nvidia-smi sample outside valid range")
    return {"gpu_utilization_percent": utilization, "gpu_memory_used_mib": memory}


def _process_tree_rss_bytes(process: Any) -> int:
    try:
        processes = [process, *process.children(recursive=True)]
    except Exception as error:
        _fail(f"process-tree RSS sample unavailable: {error}")
    rss = 0
    sampled = 0
    for item in processes:
        try:
            rss += int(item.memory_info().rss)
            sampled += 1
        except Exception:
            continue
    if sampled == 0:
        _fail("process-tree RSS sample found no live processes")
    if rss < 0:
        _fail("process-tree RSS sample is negative")
    return rss


def _sample_process(
    process: Any,
    psutil_module: Any,
    *,
    gpu_ordinal: int = GPU_ORDINAL,
    runner: Callable[..., Any] = subprocess.run,
) -> dict[str, float]:
    try:
        cpu = float(psutil_module.cpu_percent(interval=None))
        available = int(psutil_module.virtual_memory().available)
    except (AttributeError, OSError, TypeError, ValueError) as error:
        _fail(f"system utilization sample unavailable: {error}")
    if not (0.0 <= cpu <= 100.0) or available < 0:
        _fail("system utilization sample outside valid range")
    sample = _query_gpu(gpu_ordinal, runner)
    sample.update(
        {
            "system_cpu_percent": cpu,
            "process_tree_rss_bytes": float(_process_tree_rss_bytes(process)),
            "system_available_memory_bytes": float(available),
        }
    )
    return sample


def _monitor_process(
    process: Any,
    psutil_module: Any,
    *,
    poll_seconds: float = DEFAULT_POLL_SECONDS,
    timeout_seconds: float | None = None,
    gpu_ordinal: int = GPU_ORDINAL,
    runner: Callable[..., Any] = subprocess.run,
    sleep: Callable[[float], None] = time.sleep,
    monotonic: Callable[[], float] = time.perf_counter,
) -> list[dict[str, float]]:
    if poll_seconds <= 0.0:
        _fail("poll interval must be positive")
    if timeout_seconds is not None and timeout_seconds <= 0.0:
        _fail("monitor timeout must be positive")
    try:
        psutil_module.cpu_percent(interval=None)
    except (AttributeError, OSError, TypeError, ValueError) as error:
        _fail(f"CPU monitoring unavailable: {error}")
    started = monotonic()
    samples: list[dict[str, float]] = []
    while True:
        if process.poll() is not None:
            if samples:
                return samples
            _fail("topology child exited before one utilization sample")
        if timeout_seconds is not None and monotonic() - started >= timeout_seconds:
            _fail(f"topology child exceeded {timeout_seconds:g} second timeout")
        try:
            child = psutil_module.Process(process.pid)
        except Exception as error:
            if process.poll() is not None and samples:
                return samples
            _fail(f"process monitoring unavailable: {error}")
        try:
            samples.append(
                _sample_process(
                    child,
                    psutil_module,
                    gpu_ordinal=gpu_ordinal,
                    runner=runner,
                )
            )
        except RuntimeError:
            if process.poll() is not None and samples:
                return samples
            raise
        if process.poll() is not None:
            return samples
        sleep(poll_seconds)
        if process.poll() is not None:
            return samples


def _summary(samples: list[dict[str, float]]) -> dict[str, Any]:
    if not samples:
        _fail("no utilization samples were collected")
    def values(name: str) -> list[float]:
        result = [sample[name] for sample in samples]
        if not all(isinstance(value, (int, float)) for value in result):
            _fail(f"invalid utilization samples for {name}")
        return [float(value) for value in result]

    cpu = values("system_cpu_percent")
    rss = values("process_tree_rss_bytes")
    available = values("system_available_memory_bytes")
    gpu = values("gpu_utilization_percent")
    gpu_memory = values("gpu_memory_used_mib")
    return {
        "sample_count": len(samples),
        "system_cpu_percent": {"average": sum(cpu) / len(cpu), "maximum": max(cpu)},
        "process_tree_rss_bytes": {"average": sum(rss) / len(rss), "maximum": max(rss)},
        "system_available_memory_bytes": {"minimum": min(available)},
        "gpu_utilization_percent": {"average": sum(gpu) / len(gpu), "maximum": max(gpu)},
        "gpu_memory_used_mib": {"average": sum(gpu_memory) / len(gpu_memory), "maximum": max(gpu_memory)},
    }


def _load_monitoring_dependencies() -> Any:
    try:
        import psutil
    except ImportError as error:
        _fail(f"psutil is required for execute mode: {error}")
    return psutil


def _terminate(process: Any) -> None:
    if process.poll() is not None:
        return
    failures: list[str] = []
    if os.name == "nt":
        try:
            completed = subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=30,
                check=False,
            )
            if completed.returncode == 0:
                try:
                    process.wait(timeout=30)
                except (AttributeError, OSError, subprocess.TimeoutExpired) as error:
                    failures.append(f"taskkill wait failed: {error}")
                if process.poll() is not None:
                    return
            else:
                failures.append(f"taskkill exited {completed.returncode}")
        except (AttributeError, OSError, subprocess.TimeoutExpired) as error:
            failures.append(f"taskkill failed: {error}")
    try:
        process.terminate()
        process.wait(timeout=10)
    except (AttributeError, OSError, subprocess.TimeoutExpired) as error:
        failures.append(f"terminate failed: {error}")
    if process.poll() is not None:
        return
    try:
        process.kill()
        process.wait(timeout=10)
    except (AttributeError, OSError, subprocess.TimeoutExpired) as kill_error:
        failures.append(f"kill failed: {kill_error}")
    if process.poll() is None:
        detail = "; ".join(failures) if failures else "no termination method completed"
        _fail(f"process tree {process.pid} remains live after termination: {detail}")


def _run_commands(args: argparse.Namespace, commands: list[list[str]]) -> dict[str, Any]:
    psutil_module = _load_monitoring_dependencies()
    final_report_path = Path(args.report).resolve()
    if final_report_path.exists():
        _fail(f"refusing to overwrite existing report: {final_report_path}")
    revealed_root = Path(args.revealed_root).resolve()
    if revealed_root.exists() and not revealed_root.is_dir():
        _fail(f"revealed root is not a directory: {revealed_root}")
    revealed_root.mkdir(parents=True, exist_ok=True)
    for workers in EXPECTED_WORKERS:
        evidence_root = revealed_root / f"workers-{workers}"
        if evidence_root.exists():
            _fail(f"refusing to reuse existing topology evidence root: {evidence_root}")
        for suffix in ("stdout", "stderr"):
            stream_path = revealed_root / f"workers-{workers}.{suffix}.log"
            if stream_path.exists():
                _fail(f"refusing to overwrite topology stream log: {stream_path}")
    topologies: list[dict[str, Any]] = []
    for workers, command in zip(EXPECTED_WORKERS, commands):
        report_path = revealed_root / f"workers-{workers}" / "report.json"
        stdout_path = revealed_root / f"workers-{workers}.stdout.log"
        stderr_path = revealed_root / f"workers-{workers}.stderr.log"
        started = time.perf_counter()
        process = None
        with stdout_path.open("x", encoding="utf-8", newline="\n") as stdout_handle, stderr_path.open(
            "x", encoding="utf-8", newline="\n"
        ) as stderr_handle:
            try:
                process = subprocess.Popen(
                    command,
                    cwd=Path(args.mage_repo).resolve(),
                    stdout=stdout_handle,
                    stderr=stderr_handle,
                    text=True,
                    env=os.environ.copy(),
                )
                samples = _monitor_process(
                    process,
                    psutil_module,
                    poll_seconds=args.poll_seconds,
                    timeout_seconds=args.child_timeout_seconds,
                    gpu_ordinal=GPU_ORDINAL,
                )
                process.wait(timeout=10)
            except Exception:
                if process is not None and process.poll() is None:
                    _terminate(process)
                raise
        wall_seconds = time.perf_counter() - started
        stdout = stdout_path.read_text(encoding="utf-8")
        stderr = stderr_path.read_text(encoding="utf-8")
        if process.returncode != 0:
            _fail(f"topology {workers} child exited {process.returncode}: {stderr.strip() or stdout.strip()}")
        if not report_path.is_file():
            _fail(f"topology {workers} child report missing: {report_path}")
        report = _load_json(report_path)
        binding = _validate_task_bindings(
            report,
            report_path,
            base_seed=args.base_seed,
            pair_start=args.pair_start,
            pairs=EXPECTED_PAIRS,
            workers=workers,
        )
        expected_inputs = {
            "mage_repo": str(Path(args.mage_repo).resolve()),
            "scorer_exe": str(Path(args.scorer_exe).resolve()),
            "scorer_sha256": _sha256(Path(args.scorer_exe).resolve()),
            "reference_scorer_exe": str(Path(args.reference_scorer_exe).resolve()),
            "reference_scorer_sha256": _sha256(Path(args.reference_scorer_exe).resolve()),
            "outcome_root": str(Path(args.outcome_root).resolve()),
            "source_database": str(Path(args.source_database).resolve()),
            "source_database_sha256": _sha256(Path(args.source_database).resolve()),
            "maven": str(Path(args.maven).resolve()),
            "gpu_ordinal": None,
            "workload_device": "cpu",
        }
        observed_inputs = binding["provenance"]["inputs"]
        if any(observed_inputs.get(field) != value for field, value in expected_inputs.items()):
            _fail(f"topology {workers} child input provenance differs from command inputs")
        if binding["provenance"]["collector_script_sha256"] != _sha256(COLLECTOR_PATH):
            _fail(f"topology {workers} child collector script hash mismatch")
        source_games = EXPECTED_PAIRS * 2
        source_games_per_second = source_games / max(wall_seconds, 1.0e-12)
        topologies.append(
            {
                "workers": workers,
                "wall_seconds": wall_seconds,
                "source_games": source_games,
                "source_games_per_second": source_games_per_second,
                "revised_256_pair_wall_projection_seconds": (256 * 2) / source_games_per_second,
                "utilization_summary": _summary(samples),
                "child_report_path": str(report_path),
                "child_report_sha256": _sha256(report_path),
                "child_stdout_path": str(stdout_path),
                "child_stdout_sha256": _sha256(stdout_path),
                "child_stderr_path": str(stderr_path),
                "child_stderr_sha256": _sha256(stderr_path),
                "search_diagnostics": binding["diagnostics"],
                "label_digest": binding["label_digest"],
                "task_hashes": binding["task_hashes"],
                "task_ranges": binding["task_ranges"],
                "provenance": binding["provenance"],
            }
        )
    _require_cross_topology_identity(topologies)
    fastest = min(topologies, key=lambda item: item["wall_seconds"])
    return {
        "schema": SCHEMA,
        "status": "complete",
        "base_seed": args.base_seed,
        "pair_start": args.pair_start,
        "pairs": EXPECTED_PAIRS,
        "gpu_ordinal": GPU_ORDINAL,
        "screen_script_sha256": _sha256(Path(__file__).resolve()),
        "collector_script_sha256": _sha256(COLLECTOR_PATH),
        "commands": [
            {"workers": workers, "argv": command}
            for workers, command in zip(EXPECTED_WORKERS, commands)
        ],
        "topologies": topologies,
        "fastest_valid_topology": fastest["workers"],
        "revised_256_pair_wall_projection_seconds": fastest[
            "revised_256_pair_wall_projection_seconds"
        ],
    }


def _require_cross_topology_identity(topologies: list[dict[str, Any]]) -> None:
    if [item["workers"] for item in topologies] != list(EXPECTED_WORKERS):
        _fail("topology results are incomplete")
    reference = topologies[0]
    for item in topologies[1:]:
        for field in (
            "search_diagnostics",
            "label_digest",
            "task_hashes",
            "task_ranges",
            "provenance",
        ):
            if item[field] != reference[field]:
                _fail(f"topology {item['workers']} differs from 1-worker {field}")


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--revealed-root", type=Path, required=True)
    parser.add_argument("--mage-repo", type=Path, required=True)
    parser.add_argument("--scorer-exe", type=Path, required=True)
    parser.add_argument("--reference-scorer-exe", type=Path, required=True)
    parser.add_argument("--outcome-root", type=Path, required=True)
    parser.add_argument("--source-database", type=Path, required=True)
    parser.add_argument("--maven", type=Path, required=True)
    parser.add_argument("--base-seed", type=int, default=1_400_001)
    parser.add_argument("--pair-start", type=int, default=0)
    parser.add_argument("--topology-pairs", type=int, default=EXPECTED_PAIRS)
    parser.add_argument("--poll-seconds", type=float, default=DEFAULT_POLL_SECONDS)
    parser.add_argument("--child-timeout-seconds", type=int, default=7_200)
    parser.add_argument("--execute", action="store_true")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.topology_pairs != EXPECTED_PAIRS:
        _fail(f"topology screen requires exactly {EXPECTED_PAIRS} pairs")
    if args.pair_start < 0 or args.base_seed < 0:
        _fail("base seed and pair start must be nonnegative")
    commands = _COLLECTOR.build_topology_screen_commands(args)
    if not args.execute:
        for command in commands:
            print(subprocess.list2cmdline(command))
        return 0
    if args.poll_seconds <= 0.0 or args.child_timeout_seconds < 1:
        _fail("invalid monitoring or child timeout arguments")
    result = _run_commands(args, commands)
    _COLLECTOR._atomic_fresh_json(args.report.resolve(), result)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
