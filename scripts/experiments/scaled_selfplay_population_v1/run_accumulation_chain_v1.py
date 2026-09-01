#!/usr/bin/env python3
"""Detached driver/supervisor for the real ACCUMULATION 16-step chain
(coordinator GO, 2026-08-28, after independent-reviewer countersign of the
implementation on branch fable/accumulation-v1, commit d1503e23).

This script does NOT reimplement or modify accumulation_v1.py's own
reviewed run_chain()/main() -- it launches the exact same countersigned
CLI entrypoint ("python accumulation_v1.py --repo-root ... --chain-spec
... --evidence-root ... run") as a real subprocess, unmodified, and adds
only two things the coordinator asked for that main() itself has no
hooks for:

  1. Per-step progress lines to a driver log, derived entirely by polling
     the filesystem artifacts run_chain()/formal_run() already write
     durably (chain-state.json, analysis-look-*.json, gate-execution-
     manifest.json) -- no changes to the reviewed orchestration code.
  2. A terminal marker: CHAIN_DONE.json (subprocess exit 0; the final
     ledger reconstructed from chain-state.json + each step/meta-gate's
     own manifest) or CHAIN_FAILED.json (subprocess exit != 0; the best-
     available failing step + the subprocess's own captured stderr,
     verbatim).

"Chain complete" (CHAIN_DONE) means the loop ran all 16 steps to their
own terminal per-step decision -- some steps may still be individually
REJECTED (installed=False, gate resolved to FAILURE or INCONCLUSIVE-AT-
MAX-N) without that being a driver-level failure; run_chain()'s own
require() fail-closed checks are what turn a real validation/identity/
ledger/execution problem into a nonzero exit, which is the only thing
this driver treats as CHAIN_FAILED.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

N_PLANNED_STEPS = 16
K_META_BLOCK = 5
STEP_ALPHA_INITIAL = 0.00225
STEP_ALPHA_CONFIRM = 0.00225
META_ALPHA = 0.006
POLL_SECONDS = 20


def now_iso() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime()) + f".{int(time.time() * 1000) % 1000:03d}"


def append_log(log_path: Path, line: str) -> None:
    with log_path.open("a", encoding="utf-8") as stream:
        stream.write(f"[{now_iso()}] {line}\n")
        stream.flush()


def load_json_quiet(path: Path) -> dict[str, Any] | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def poll_progress(evidence_root: Path, log_path: Path, seen: set[str]) -> None:
    """One poll pass: log any new analysis-look / gate-execution-manifest
    files under evidence_root, and any growth in chain-state.json's own
    completed_steps/completed_meta_gates. seen is mutated in place."""
    for look_path in sorted(evidence_root.glob("*/attempt-*/analysis-look-*.json")):
        key = str(look_path)
        if key in seen:
            continue
        seen.add(key)
        doc = load_json_quiet(look_path)
        if doc is None:
            continue
        gate_id = look_path.parent.parent.name
        append_log(
            log_path,
            f"look  gate={gate_id} acquired_N={doc.get('acquired_N')} "
            f"decision={doc.get('decision')} delta_hat={doc.get('delta_hat')}",
        )
    for manifest_path in sorted(evidence_root.glob("*/attempt-*/gate-execution-manifest.json")):
        key = str(manifest_path)
        if key in seen:
            continue
        seen.add(key)
        doc = load_json_quiet(manifest_path)
        if doc is None:
            continue
        gate_id = manifest_path.parent.parent.name
        append_log(
            log_path,
            f"GATE-DONE gate={gate_id} mode={doc.get('mode')} "
            f"disposition={doc.get('disposition')} "
            f"acquired_N={doc.get('analysis_summary', {}).get('acquired_N')} "
            f"wall_seconds={doc.get('wall_seconds')}",
        )
    state_path = evidence_root / "chain-state.json"
    state = load_json_quiet(state_path)
    if state is not None:
        key = "chain-state:" + str(len(state.get("completed_steps", []))) + ":" + str(len(state.get("completed_meta_gates", [])))
        if key not in seen:
            seen.add(key)
            append_log(
                log_path,
                f"CHAIN-STATE steps_completed={len(state.get('completed_steps', []))}/16 "
                f"accepted_step_count={state.get('accepted_step_count')} "
                f"meta_gates_completed={len(state.get('completed_meta_gates', []))}/3",
            )


def infer_failing_step(evidence_root: Path) -> str:
    state_path = evidence_root / "chain-state.json"
    state = load_json_quiet(state_path)
    if state is None:
        return "step-01 (chain-state.json not yet written; failure occurred before/during step 1's own first durable write)"
    completed_steps = state.get("completed_steps", [])
    accepted = state.get("accepted_step_count", 0)
    completed_meta = state.get("completed_meta_gates", [])
    if accepted > 0 and accepted % K_META_BLOCK == 0:
        meta_index = accepted // K_META_BLOCK
        if meta_index <= 3 and not any(row["meta_index"] == meta_index for row in completed_meta):
            return f"meta-{meta_index:02d} (accepted_step_count={accepted} triggered this meta-gate block; it did not complete)"
    next_step = len(completed_steps) + 1
    if next_step <= N_PLANNED_STEPS:
        return f"step-{next_step:02d}"
    return "past step 16 (all 16 steps completed_steps entries present; failure occurred after the main loop, in a trailing meta-gate or driver-level check)"


def build_chain_done(evidence_root: Path, chain_spec: dict[str, Any]) -> dict[str, Any]:
    state = load_json_quiet(evidence_root / "chain-state.json") or {}
    completed_steps = state.get("completed_steps", [])
    completed_meta = state.get("completed_meta_gates", [])
    alpha_spent = 0.0
    step_ledger = []
    for row in completed_steps:
        confirm_ran = row.get("initial_disposition") == "SUCCESS"
        alpha_spent += STEP_ALPHA_INITIAL + (STEP_ALPHA_CONFIRM if confirm_ran else 0.0)
        step_ledger.append(
            {
                "step_index": row["step_index"],
                "installed": row["installed"],
                "initial_disposition": row["initial_disposition"],
                "confirmation_leg_ran": confirm_ran,
            }
        )
    alpha_spent += META_ALPHA * len(completed_meta)
    installed_steps = [row["step_index"] for row in step_ledger if row["installed"]]
    return {
        "schema": "scaled-selfplay-accumulation-v1-chain-done/v1",
        "status": "CHAIN_DONE",
        "completed_at": now_iso(),
        "steps_completed": len(completed_steps),
        "steps_planned": N_PLANNED_STEPS,
        "accepted_step_count": state.get("accepted_step_count"),
        "step_ledger": step_ledger,
        "anchor_lineage": {
            "initial_anchor_identity_bundle_sha256": chain_spec["initial_anchor"]["identity_bundle_sha256"],
            "installed_step_indexes_in_order": installed_steps,
            "final_anchor": state.get("current_anchor"),
        },
        "meta_gate_verdicts": completed_meta,
        "alpha_ledger": {
            "alpha_spent": round(alpha_spent, 10),
            "alpha_campaign_authorized": 0.10,
            "alpha_reserve_untouched": 0.01,
        },
    }


def build_chain_failed(evidence_root: Path, returncode: int, stderr_tail: str) -> dict[str, Any]:
    return {
        "schema": "scaled-selfplay-accumulation-v1-chain-failed/v1",
        "status": "CHAIN_FAILED",
        "failed_at": now_iso(),
        "failing_step": infer_failing_step(evidence_root),
        "subprocess_returncode": returncode,
        "error_verbatim": stderr_tail,
    }


def scan_existing_manifests(evidence_root: Path) -> dict[str, set[str]]:
    """gate_id -> set of attempt directory paths that already carry a
    passed=True gate-execution-manifest.json, at the moment this is
    called. Used on resume both to (a) log an explicit, unambiguous
    SKIP-EXPECTED line per already-completed gate (rather than letting the
    normal poll loop rediscover them as a confusing burst of "new"
    completions on its first pass) and (b) as the baseline for the
    post-run safety check: a gate_id present here must show this EXACT
    same set of attempt roots after the run, or a completed gate was
    re-run -- real games would have been wasted, and per governance that
    must stop and report, never pass silently."""
    baseline: dict[str, set[str]] = {}
    for manifest_path in evidence_root.glob("*/attempt-*/gate-execution-manifest.json"):
        doc = load_json_quiet(manifest_path)
        if doc is None or doc.get("passed") is not True:
            continue
        gate_id = manifest_path.parent.parent.name
        baseline.setdefault(gate_id, set()).add(str(manifest_path.parent.resolve()))
    return baseline


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", required=True, type=Path)
    parser.add_argument("--chain-spec", required=True, type=Path)
    parser.add_argument("--evidence-root", required=True, type=Path)
    parser.add_argument("--python-exe", required=True, type=Path)
    parser.add_argument("--orchestrator", required=True, type=Path, help="path to accumulation_v1.py")
    args = parser.parse_args()

    evidence_root = args.evidence_root.resolve()
    evidence_root.mkdir(parents=True, exist_ok=True)
    log_path = evidence_root / "_driver-progress.log"
    subprocess_stdout_path = evidence_root / "_orchestrator-stdout.log"
    subprocess_stderr_path = evidence_root / "_orchestrator-stderr.log"

    chain_spec_path = args.chain_spec.resolve()
    chain_spec = json.loads(chain_spec_path.read_text(encoding="utf-8"))

    append_log(log_path, f"driver start; repo_root={args.repo_root} chain_spec={chain_spec_path} evidence_root={evidence_root}")
    append_log(log_path, f"n_planned_steps={chain_spec.get('n_planned_steps')} k_meta_block={chain_spec.get('k_meta_block')} max_N_clusters={chain_spec.get('max_N_clusters')} games_per_cluster={chain_spec.get('games_per_cluster')}")

    # Resume baseline (2026-08-29, added after the step-07-confirm crash on
    # attempt-002 so a relaunch reports skipped gates unambiguously, and so
    # a completed gate re-running -- which would waste already-played real
    # games -- is caught rather than passing silently).
    baseline_manifests = scan_existing_manifests(evidence_root)
    baseline_state = load_json_quiet(evidence_root / "chain-state.json")
    if baseline_manifests:
        completed_step_indexes = sorted(row["step_index"] for row in (baseline_state or {}).get("completed_steps", []))
        append_log(log_path, f"RESUME baseline: {len(baseline_manifests)} gate(s) already have a passed manifest; completed_steps at start={completed_step_indexes}")
        for gate_id in sorted(baseline_manifests):
            append_log(log_path, f"  SKIP-EXPECTED gate={gate_id} attempt_root(s)={sorted(baseline_manifests[gate_id])}")
    else:
        append_log(log_path, "RESUME baseline: no pre-existing passed manifests found (fresh chain start, nothing to skip)")

    cmd = [
        str(args.python_exe),
        str(args.orchestrator),
        "--repo-root", str(args.repo_root.resolve()),
        "--chain-spec", str(chain_spec_path),
        "--evidence-root", str(evidence_root),
        "run",
    ]
    append_log(log_path, f"launching orchestrator subprocess: {' '.join(cmd)}")

    # Pre-seed 'seen' with every artifact that already existed at driver
    # start, so the normal poll loop below only ever logs completions THIS
    # driver invocation actually caused -- not a burst of already-old
    # evidence rediscovered on its first poll pass (the already-completed
    # gates are reported once, explicitly, via the SKIP-EXPECTED lines
    # above instead).
    seen: set[str] = set()
    for look_path in evidence_root.glob("*/attempt-*/analysis-look-*.json"):
        seen.add(str(look_path))
    for manifest_path in evidence_root.glob("*/attempt-*/gate-execution-manifest.json"):
        seen.add(str(manifest_path))
    if baseline_state is not None:
        seen.add(
            "chain-state:" + str(len(baseline_state.get("completed_steps", [])))
            + ":" + str(len(baseline_state.get("completed_meta_gates", [])))
        )

    with subprocess_stdout_path.open("wb") as out_f, subprocess_stderr_path.open("wb") as err_f:
        proc = subprocess.Popen(cmd, cwd=str(args.orchestrator.parent), stdout=out_f, stderr=err_f)
        append_log(log_path, f"orchestrator subprocess PID={proc.pid}")
        while True:
            returncode = proc.poll()
            poll_progress(evidence_root, log_path, seen)
            if returncode is not None:
                break
            time.sleep(POLL_SECONDS)
    poll_progress(evidence_root, log_path, seen)

    # Safety check (2026-08-29): a gate_id that already had a passed
    # manifest at baseline must show that EXACT same set of attempt roots
    # now -- resumability's own existing_manifest()/completed_steps skip
    # logic must never let an already-completed gate acquire a NEW attempt
    # directory (that would mean it was re-run, wasting real games already
    # played). Checked regardless of the subprocess's own exit code.
    final_manifests = scan_existing_manifests(evidence_root)
    rerun_violations = []
    for gate_id, baseline_roots in baseline_manifests.items():
        extra_roots = final_manifests.get(gate_id, set()) - baseline_roots
        if extra_roots:
            rerun_violations.append({"gate_id": gate_id, "baseline_attempt_roots": sorted(baseline_roots), "new_attempt_roots_after_run": sorted(extra_roots)})
    if rerun_violations:
        append_log(log_path, f"RERUN VIOLATION: {len(rerun_violations)} already-completed gate(s) acquired a NEW passed manifest this run: {[v['gate_id'] for v in rerun_violations]}")

    if proc.returncode == 0 and not rerun_violations:
        record = build_chain_done(evidence_root, chain_spec)
        marker_path = evidence_root / "CHAIN_DONE.json"
        marker_path.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        append_log(log_path, f"CHAIN_DONE written: steps_completed={record['steps_completed']}/16 accepted_step_count={record['accepted_step_count']}")
        return 0
    else:
        stderr_tail = ""
        try:
            stderr_bytes = subprocess_stderr_path.read_bytes()
            stderr_tail = stderr_bytes[-8000:].decode("utf-8", errors="replace")
        except OSError:
            pass
        record = build_chain_failed(evidence_root, proc.returncode, stderr_tail)
        if rerun_violations:
            record["rerun_violations"] = rerun_violations
            if proc.returncode == 0:
                record["note"] = "orchestrator subprocess exited 0, but a re-run safety violation was detected post-hoc; reported as CHAIN_FAILED per governance (an already-completed gate must never re-run)"
        marker_path = evidence_root / "CHAIN_FAILED.json"
        marker_path.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        append_log(log_path, f"CHAIN_FAILED written: returncode={proc.returncode} failing_step={record['failing_step']} rerun_violations={len(rerun_violations)}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
