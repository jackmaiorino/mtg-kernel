"""Versioned, non-authoritative end-to-end trainer benchmark harness."""

from __future__ import annotations

import argparse
import ctypes
import math
import os
import platform
import re
import subprocess
import time
from pathlib import Path
from typing import Any

import torch

from .artifacts import require_new_or_empty_dir, sha256_file, write_json_atomic
from .phase_profile import PhaseRecorder
from .trainer import train
from .training_store import TrainingStore

BENCHMARK_SCHEMA = "kernel_rl_training_benchmark/v1"
OVERLAP_CONTRACT = {
    "trial_wall": "starts immediately before train() and includes deterministic Torch configuration, update-0 bootstrap, all requested updates, and client cleanup; phase_v1 additionally includes graceful profiled environment EOF and strict profile collection; there is no excluded warmup or steady-state claim",
    "python": "IPC subphases are disjoint within one exchange; learner feature, forward, sample, and hash are sequential; loss, backward, optimizer, checkpoint-build, and publication are sequential per update; ordinary trainer routing and validation outside named spans remain in trial wall",
    "kernel": "in phase_v1 only, parse, decode, retry, reset construction, step validation, step integrity, step selection, step apply, engine advance, observe, actions, postbind, response build, serialize-or-cached-clone, and write-flush are disjoint spans; failed requests may skip later phases",
    "cross_process": "kernel phases execute inside Python ipc_wait_read and must not be added to Python phases or trial wall",
    "artifact_boundary": "all timings live only in this benchmark manifest and never enter the deterministic training store",
    "post_timing_validation": "effective-hardware capture, authoritative TrainingStore.validate_latest(), binary re-hash, and derived count extraction occur after trial wall and are required before publication",
}
HEX40_RE = re.compile(r"^[0-9a-f]{40}$")
PROFILE_MODES = ("off", "phase_v1")
AUTHORITATIVE_RUN_CONTRACT_KEYS = (
    "schema",
    "package",
    "algorithm",
    "protocol",
    "protocol_provenance",
    "initializer",
    "optimizer",
    "samplers",
    "schedule",
    "trainer",
    "seed_derivation",
    "compatibility",
)


def _positive_int(value: Any, name: str, maximum: int) -> int:
    if type(value) is not int or not 1 <= value <= maximum:
        raise ValueError(f"{name} must be in 1..={maximum}")
    return value


def _cpu_model() -> tuple[str, str]:
    if os.name == "nt":
        try:
            import winreg

            with winreg.OpenKey(
                winreg.HKEY_LOCAL_MACHINE,
                r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
            ) as key:
                value, _kind = winreg.QueryValueEx(key, "ProcessorNameString")
            if type(value) is str and value.strip():
                return value.strip(), "windows_registry_processor_name/v1"
        except OSError:
            pass
    if platform.system() == "Linux":
        try:
            for line in Path("/proc/cpuinfo").read_text(encoding="utf-8").splitlines():
                if line.lower().startswith("model name") and ":" in line:
                    value = line.split(":", 1)[1].strip()
                    if value:
                        return value, "linux_proc_cpuinfo_model_name/v1"
        except OSError:
            pass
    for value, source in (
        (platform.processor(), "platform_processor/v1"),
        (os.environ.get("PROCESSOR_IDENTIFIER", ""), "processor_identifier_env/v1"),
        (platform.machine(), "platform_machine_fallback/v1"),
    ):
        if value.strip():
            return value.strip(), source
    return "unknown", "unavailable/v1"


def _total_ram_bytes() -> tuple[int | None, str]:
    if os.name == "nt":
        class MemoryStatusEx(ctypes.Structure):
            _fields_ = [
                ("dwLength", ctypes.c_ulong),
                ("dwMemoryLoad", ctypes.c_ulong),
                ("ullTotalPhys", ctypes.c_ulonglong),
                ("ullAvailPhys", ctypes.c_ulonglong),
                ("ullTotalPageFile", ctypes.c_ulonglong),
                ("ullAvailPageFile", ctypes.c_ulonglong),
                ("ullTotalVirtual", ctypes.c_ulonglong),
                ("ullAvailVirtual", ctypes.c_ulonglong),
                ("ullAvailExtendedVirtual", ctypes.c_ulonglong),
            ]

        status = MemoryStatusEx()
        status.dwLength = ctypes.sizeof(status)
        if ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)):
            return int(status.ullTotalPhys), "windows_global_memory_status_ex/v1"
    try:
        pages = os.sysconf("SC_PHYS_PAGES")
        page_size = os.sysconf("SC_PAGE_SIZE")
        if type(pages) is int and type(page_size) is int and pages > 0 and page_size > 0:
            return pages * page_size, "posix_sysconf_physical_pages/v1"
    except (AttributeError, OSError, ValueError):
        pass
    return None, "unavailable/v1"


def _process_affinity() -> tuple[list[int] | None, str]:
    if hasattr(os, "sched_getaffinity"):
        try:
            return sorted(os.sched_getaffinity(0)), "posix_sched_getaffinity/v1"
        except OSError:
            pass
    if os.name == "nt":
        process_mask = ctypes.c_size_t()
        system_mask = ctypes.c_size_t()
        handle = ctypes.windll.kernel32.GetCurrentProcess()
        if ctypes.windll.kernel32.GetProcessAffinityMask(
            handle, ctypes.byref(process_mask), ctypes.byref(system_mask)
        ):
            mask = int(process_mask.value)
            return [index for index in range(mask.bit_length()) if mask & (1 << index)], "windows_process_affinity_mask/v1"
    return None, "unavailable/v1"


def _hardware_record() -> dict[str, Any]:
    cpu_model, cpu_model_source = _cpu_model()
    total_ram_bytes, total_ram_source = _total_ram_bytes()
    affinity, affinity_source = _process_affinity()
    return {
        "schema": "kernel_rl_benchmark_hardware/v1",
        "os_system": platform.system(),
        "os_release": platform.release(),
        "machine": platform.machine(),
        "architecture": platform.architecture()[0],
        "processor": platform.processor(),
        "cpu_model": cpu_model,
        "cpu_model_source": cpu_model_source,
        "logical_cpu_count": os.cpu_count(),
        "total_ram_bytes": total_ram_bytes,
        "total_ram_source": total_ram_source,
        "process_affinity_logical_cpus": affinity,
        "process_affinity_source": affinity_source,
        "python_implementation": platform.python_implementation(),
        "python_version": platform.python_version(),
        "torch_version": str(torch.__version__),
        "torch_num_threads": torch.get_num_threads(),
        "torch_num_interop_threads": torch.get_num_interop_threads(),
        "torch_default_dtype": str(torch.get_default_dtype()),
        "torch_deterministic_algorithms": torch.are_deterministic_algorithms_enabled(),
        "cpu_only": True,
    }


def _read_trial_counts(store: Path) -> tuple[dict[str, int], dict[str, Any]]:
    episode_count = 0
    policy_steps = 0
    physical_decisions = 0
    learner_policy_steps = 0
    learner_physical_decisions = 0
    updates = 0
    optimizer_steps = 0
    chain = TrainingStore(store).validate_latest()
    for record in chain.update_records:
        if record["update"] == 0:
            continue
        updates += 1
        optimizer_steps += int(record["optimizer_step"])
        summaries = record["episode_summaries"]
        episode_count += len(summaries)
        learner_policy_steps += record["learner_policy_step_count"]
        learner_physical_decisions += record["learner_physical_decision_count"]
        policy_steps += sum(row["policy_step_count"] for row in summaries)
        physical_decisions += sum(row["physical_decision_count"] for row in summaries)
    return {
        "updates": updates,
        "optimizer_steps": optimizer_steps,
        "episodes": episode_count,
        "policy_steps": policy_steps,
        "physical_decisions": physical_decisions,
        "learner_policy_steps": learner_policy_steps,
        "learner_physical_decisions": learner_physical_decisions,
    }, chain.run_record


def _rates(counts: dict[str, int], elapsed_ns: int) -> dict[str, float]:
    seconds = elapsed_ns / 1_000_000_000
    values = {
        "updates_per_second": counts["updates"] / seconds,
        "optimizer_steps_per_second": counts["optimizer_steps"] / seconds,
        "episodes_per_second": counts["episodes"] / seconds,
        "policy_steps_per_second": counts["policy_steps"] / seconds,
        "physical_decisions_per_second": counts["physical_decisions"] / seconds,
        "learner_policy_steps_per_second": counts["learner_policy_steps"] / seconds,
        "learner_physical_decisions_per_second": counts["learner_physical_decisions"] / seconds,
    }
    if any(not math.isfinite(value) for value in values.values()):
        raise ValueError("benchmark derived rate is not finite")
    return values


def _source_record(git_commit: str, repo_root: str | Path | None) -> dict[str, Any]:
    if repo_root is None:
        return {
            "git_commit_claim": git_commit,
            "repo_state_verification": "user_supplied_unverified/v1",
            "repo_head_matches_claim": None,
            "repo_worktree_clean": None,
            "executed_python_source_binding": "unverified/v1",
            "binary_source_binding": "unverified/v1",
        }
    root = Path(repo_root)
    if not root.is_dir():
        raise ValueError("repo_root must be a directory")
    head = subprocess.run(
        ["git", "-C", str(root), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if head != git_commit:
        raise ValueError("repo HEAD does not match git_commit claim")
    status = subprocess.run(
        ["git", "-C", str(root), "status", "--porcelain=v1"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    if status:
        raise ValueError("repo worktree is not clean")
    return {
        "git_commit_claim": git_commit,
        "repo_state_verification": "git_head_and_clean_worktree/v1",
        "repo_head_matches_claim": True,
        "repo_worktree_clean": True,
        "executed_python_source_binding": "unverified/v1",
        "binary_source_binding": "unverified/v1",
    }


def _assert_authoritative_run_matches_request(
    *,
    run: dict[str, Any],
    result: dict[str, Any],
    counts: dict[str, int],
    run_sha256: str,
    binary_sha256: str,
    deck_id: str,
    until_update: int,
    batch_episodes: int,
    base_seed: int,
    learning_rate: float,
    value_coef: float,
    max_physical_decisions: int,
    max_policy_steps: int,
) -> dict[str, Any]:
    if result.get("run_digest") != run_sha256:
        raise ValueError("trainer result run digest does not bind authoritative run.json")
    if result.get("completed_update") != until_update or counts["updates"] != until_update:
        raise ValueError("benchmark update count mismatch")
    expected_episodes = until_update * batch_episodes
    if counts["episodes"] != expected_episodes or result.get("next_episode") != expected_episodes:
        raise ValueError("benchmark episode count mismatch")
    if result.get("optimizer_step_count") != counts["optimizer_steps"]:
        raise ValueError("trainer result optimizer count mismatch")
    if run["environment"]["binary_sha256"] != binary_sha256:
        raise ValueError("authoritative run environment binary digest mismatch")
    if run["environment"]["deck_ids"] != [deck_id, deck_id]:
        raise ValueError("authoritative run deck identity mismatch")
    if run["schedule"]["batch_episodes"] != batch_episodes:
        raise ValueError("authoritative run batch schedule mismatch")
    expected_trainer = {
        "base_seed": base_seed,
        "value_coef": value_coef,
        "max_physical_decisions": max_physical_decisions,
        "max_policy_steps": max_policy_steps,
    }
    for key, expected in expected_trainer.items():
        if run["trainer"][key] != expected:
            raise ValueError(f"authoritative run trainer {key} mismatch")
    if run["optimizer"]["lr"] != learning_rate:
        raise ValueError("authoritative run learning rate mismatch")
    return {
        "run_record_sha256": run_sha256,
        "run_digest": result["run_digest"],
        "contract": {key: run[key] for key in AUTHORITATIVE_RUN_CONTRACT_KEYS},
    }


def _assert_phase_profile_matches_work(
    phase_profile: dict[str, Any], counts: dict[str, int]
) -> None:
    kernel_records = phase_profile["kernel_records"]
    if len(kernel_records) != 1:
        raise ValueError("benchmark trial requires exactly one kernel profile record")
    kernel = kernel_records[0]
    request_count = counts["episodes"] + counts["policy_steps"]
    if not (
        kernel["request_lines"]
        == kernel["response_lines"]
        == request_count
        and kernel["reset_requests"] == counts["episodes"]
        and kernel["step_requests"] == counts["policy_steps"]
    ):
        raise ValueError("kernel phase profile does not bind authoritative training work")
    python_counts = {
        phase: counter["count"]
        for phase, counter in phase_profile["python_phases"].items()
    }
    expected = {
        "ipc_encode": request_count,
        "ipc_write_flush": request_count,
        "ipc_wait_read": request_count,
        "ipc_decode": request_count,
        "ipc_validate": request_count,
        "feature_tensor": counts["learner_policy_steps"] + 1,
        "model_forward": counts["learner_policy_steps"],
        "action_sample": counts["policy_steps"],
        "trajectory_hash": counts["learner_policy_steps"]
        + counts["learner_physical_decisions"]
        + counts["episodes"],
        "loss_build": counts["optimizer_steps"],
        "backward": counts["optimizer_steps"],
        "optimizer": counts["optimizer_steps"],
        "checkpoint_build": counts["updates"] + 1,
        "publication": counts["updates"] + 1,
    }
    for phase, expected_count in expected.items():
        if python_counts[phase] != expected_count:
            raise ValueError(
                f"Python phase {phase} count does not bind authoritative training work"
            )


def benchmark_training(
    *,
    env_bin: str | Path,
    out_dir: str | Path,
    git_commit: str,
    repo_root: str | Path | None = None,
    profile_mode: str,
    deck_id: str,
    trials: int,
    until_update: int,
    batch_episodes: int,
    base_seed: int,
    learning_rate: float,
    value_coef: float,
    max_physical_decisions: int,
    max_policy_steps: int,
) -> dict[str, Any]:
    if HEX40_RE.fullmatch(git_commit) is None:
        raise ValueError("git_commit must be 40 lowercase hexadecimal characters")
    if deck_id not in {"Burn", "Rally"}:
        raise ValueError("deck_id must be exact Burn or Rally")
    if profile_mode not in PROFILE_MODES:
        raise ValueError(f"profile_mode must be one of {PROFILE_MODES}")
    trials = _positive_int(trials, "trials", 20)
    until_update = _positive_int(until_update, "until_update", 1_000_000)
    source = _source_record(git_commit, repo_root)
    root = require_new_or_empty_dir(out_dir)
    env_path = Path(env_bin)
    if not env_path.is_file():
        raise FileNotFoundError(env_path)
    binary_sha256 = sha256_file(env_path)
    hardware: dict[str, Any] | None = None
    trial_records: list[dict[str, Any]] = []
    model: dict[str, Any] | None = None
    feature: dict[str, Any] | None = None
    environment: dict[str, Any] | None = None
    authoritative_run: dict[str, Any] | None = None
    aggregate_counts = {
        "updates": 0,
        "optimizer_steps": 0,
        "episodes": 0,
        "policy_steps": 0,
        "physical_decisions": 0,
        "learner_policy_steps": 0,
        "learner_physical_decisions": 0,
    }
    aggregate_elapsed_ns = 0
    for trial_index in range(trials):
        relative_store = f"trial-{trial_index:03d}/training-store"
        store = root / f"trial-{trial_index:03d}" / "training-store"
        recorder = PhaseRecorder() if profile_mode == "phase_v1" else None
        start = time.perf_counter_ns()
        result = train(
            env_bin=env_path,
            out_dir=store,
            until_update=until_update,
            deck_ids=(deck_id, deck_id),
            base_seed=base_seed,
            batch_episodes=batch_episodes,
            learning_rate=learning_rate,
            value_coef=value_coef,
            max_physical_decisions=max_physical_decisions,
            max_policy_steps=max_policy_steps,
            phase_recorder=recorder,
            kernel_phase_profile=profile_mode == "phase_v1",
        )
        elapsed_ns = time.perf_counter_ns() - start
        if elapsed_ns <= 0:
            raise ValueError("benchmark trial wall time must be positive")
        trial_hardware = _hardware_record()
        if hardware is None:
            hardware = trial_hardware
        elif trial_hardware != hardware:
            raise ValueError("effective benchmark hardware or runtime settings drifted")
        phase_profile = recorder.snapshot() if recorder is not None else None
        if phase_profile is not None and len(phase_profile["kernel_records"]) != 1:
            raise ValueError("benchmark trial requires exactly one kernel profile record")
        counts, run = _read_trial_counts(store)
        run_sha256 = sha256_file(store / "run.json")
        if sha256_file(env_path) != binary_sha256:
            raise ValueError("environment binary changed during benchmark")
        trial_authoritative_run = _assert_authoritative_run_matches_request(
            run=run,
            result=result,
            counts=counts,
            run_sha256=run_sha256,
            binary_sha256=binary_sha256,
            deck_id=deck_id,
            until_update=until_update,
            batch_episodes=batch_episodes,
            base_seed=base_seed,
            learning_rate=learning_rate,
            value_coef=value_coef,
            max_physical_decisions=max_physical_decisions,
            max_policy_steps=max_policy_steps,
        )
        if phase_profile is not None:
            _assert_phase_profile_matches_work(phase_profile, counts)
        trial_model = run["model"]
        trial_feature = run["feature_contract"]
        trial_environment = run["environment"]
        if model is None:
            model, feature, environment, authoritative_run = (
                trial_model,
                trial_feature,
                trial_environment,
                trial_authoritative_run,
            )
        elif (
            trial_model,
            trial_feature,
            trial_environment,
            trial_authoritative_run,
        ) != (model, feature, environment, authoritative_run):
            raise ValueError("benchmark trial provenance drift")
        for key, value in counts.items():
            aggregate_counts[key] += value
        aggregate_elapsed_ns += elapsed_ns
        trial_records.append(
            {
                "trial_index": trial_index,
                "training_store": relative_store,
                "elapsed_ns": elapsed_ns,
                "counts": counts,
                "rates": _rates(counts, elapsed_ns),
                "phase_profile": phase_profile,
                "result": result,
            }
        )
    assert (
        hardware is not None
        and model is not None
        and feature is not None
        and environment is not None
        and authoritative_run is not None
    )
    manifest = {
        "schema": BENCHMARK_SCHEMA,
        "source": source,
        "binary": {
            "name": "kernel_rl_env",
            "sha256": binary_sha256,
            "executed_binary_binding": "sha256_cross_checked_against_authoritative_run/v1",
            "source_build_binding": "unverified/v1",
            "profile_flag": "--phase-profile-v1" if profile_mode == "phase_v1" else None,
        },
        "hardware": hardware,
        "environment": environment,
        "model": model,
        "feature_contract": feature,
        "authoritative_run": authoritative_run,
        "workload": {
            "profile_mode": profile_mode,
            "throughput_role": (
                "primary_uninstrumented/v1"
                if profile_mode == "off"
                else "diagnostic_phase_attribution/v1"
            ),
            "timing_scope": "cold_start_inclusive_update0_bootstrap_plus_requested_updates/v1",
            "interpreter_lifecycle": "all trials run sequentially in one Python interpreter; trial zero may include first-use process and Torch caches while later trials retain process-global caches",
            "trial_isolation": "each trial creates a fresh environment process, training store, model, and optimizer",
            "effective_hardware_capture": "post-trial after train() configured Torch; capture and authoritative-store validation are excluded from trial wall",
            "warmup_updates_excluded": 0,
            "steady_state_claim": False,
            "topology": "single_synchronous_environment_single_cpu_learner/v1",
            "environment_processes": 1,
            "batched_inference": False,
            "policy_device": "cpu",
            "opponent": "seeded_uniform_index/v1",
            "terminal_eligibility": "natural terminal win, loss, or draw only; halted and truncated episodes fail the trainer",
            "deck_ids": [deck_id, deck_id],
            "trial_count": trials,
            "until_update": until_update,
            "batch_episodes": batch_episodes,
            "base_seed": base_seed,
            "learning_rate_hex": float(learning_rate).hex(),
            "value_coef_hex": float(value_coef).hex(),
            "max_physical_decisions": max_physical_decisions,
            "max_policy_steps": max_policy_steps,
        },
        "overlap_contract": OVERLAP_CONTRACT,
        "trials": trial_records,
        "aggregate": {
            "elapsed_ns": aggregate_elapsed_ns,
            "counts": aggregate_counts,
            "rates": _rates(aggregate_counts, aggregate_elapsed_ns),
        },
    }
    write_json_atomic(root / "benchmark.json", manifest)
    return manifest


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="python -m mtg_kernel_rl.training_benchmark")
    parser.add_argument("--env-bin", required=True, type=Path)
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--git-commit", required=True)
    parser.add_argument("--repo-root", default=None, type=Path)
    parser.add_argument("--profile-mode", required=True, choices=PROFILE_MODES)
    parser.add_argument("--deck-id", required=True, choices=("Burn", "Rally"))
    parser.add_argument("--trials", required=True, type=int)
    parser.add_argument("--until-update", required=True, type=int)
    parser.add_argument("--batch-episodes", required=True, type=int)
    parser.add_argument("--base-seed", required=True, type=int)
    parser.add_argument("--learning-rate", required=True, type=float)
    parser.add_argument("--value-coef", required=True, type=float)
    parser.add_argument("--max-physical-decisions", required=True, type=int)
    parser.add_argument("--max-policy-steps", required=True, type=int)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    benchmark_training(
        env_bin=args.env_bin,
        out_dir=args.out_dir,
        git_commit=args.git_commit,
        repo_root=args.repo_root,
        profile_mode=args.profile_mode,
        deck_id=args.deck_id,
        trials=args.trials,
        until_update=args.until_update,
        batch_episodes=args.batch_episodes,
        base_seed=args.base_seed,
        learning_rate=args.learning_rate,
        value_coef=args.value_coef,
        max_physical_decisions=args.max_physical_decisions,
        max_policy_steps=args.max_policy_steps,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
