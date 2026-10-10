"""Read-only timing audit of explicit native-expanded campaign roots.

Never opens trajectories, checkpoints, exposure, evaluation or outcome files.
Update receipts are projected onto timing/count fields; losses are not used.
Stage sums are process wall seconds, never the concurrent job's elapsed time.
"""

from __future__ import annotations

import argparse
from collections import defaultdict
from datetime import datetime
import hashlib
import json
import math
from pathlib import Path
import statistics

SCHEDULER = (
    "iteration_wall_seconds", "collection_execution_seconds",
    "update_execution_seconds", "collection_validation_seconds",
    "update_validation_seconds",
)
UPDATE = (
    "update_elapsed_seconds", "input_read_seconds", "behavior_replay_seconds",
    "learner_update_seconds", "checkpoint_io_seconds",
)


def read(path, evidence):
    data = path.read_bytes()
    evidence.append((str(path), len(data), hashlib.sha256(data).hexdigest()))
    return json.loads(data)


def numeric(row, keys):
    result = {key: float(row[key]) for key in keys}
    if not all(math.isfinite(v) and v >= 0 for v in result.values()):
        raise ValueError("invalid timing")
    return result


def distribution(values):
    values = sorted(values)
    return {"n": len(values), "sum": sum(values), "mean": statistics.mean(values),
            "median": statistics.median(values),
            "p90_nearest_rank": values[math.ceil(0.9 * len(values))-1], "max": max(values)}


def aggregate(rows):
    sums = defaultdict(float)
    for row in rows:
        for key, value in row.items():
            sums[key] += value
    return dict(sums)


def timestamp(value):
    return datetime.fromisoformat(value).timestamp()


def union_seconds(intervals):
    total = 0.0
    end = None
    for lo, hi in sorted(intervals):
        if hi < lo:
            raise ValueError("reversed interval")
        total += max(0, hi - max(lo, end if end is not None else lo))
        end = max(hi, end if end is not None else hi)
    return total


def audit(hot, retained, runs, state):
    blocks, excluded, evidence = [], [], []
    all_scheduler, all_updates = [], []
    state_documents = {}
    if state:
        for run in runs:
            path = state / f"{run}.json"
            if path.exists():
                state_documents[run] = read(path, evidence)
    for run in runs:
        for attempt in sorted((hot / run).glob("b*-a*")):
            folder = attempt / "dispatch"
            report_path = folder / "report.json"
            if not report_path.exists():
                excluded.append({"run": run, "attempt": attempt.name, "reason": "no report"})
                continue
            start_evidence = len(evidence)
            report = read(report_path, evidence)
            if report.get("complete") is not True or report.get("qualification") is not False:
                excluded.append({"run": run, "attempt": attempt.name, "reason": "not successful production"})
                continue
            block = int(attempt.name[1:3])
            state_block = state_documents.get(run, {}).get("blocks", {}).get(str(block))
            if state and not state_block:
                raise ValueError(f"completed report absent from canonical state: {folder}")
            # A successful but later set-aside environmental attempt is not
            # counted again as productive work. State selects the retained head.
            if state_block:
                attempts = state_block.get("attempts", [])
                matching = [a for a in attempts if int(a["attempt"]) == int(attempt.name.split("-a")[1])]
                if not matching or matching[0].get("outcome") != "complete":
                    excluded.append({"run": run, "attempt": attempt.name, "reason": "not current completed state attempt"})
                    continue
                if matching[0]["report"]["sha256"] != evidence[start_evidence][2]:
                    raise ValueError("report differs from canonical state pin")
            execution = read(folder / "execution.json", evidence)
            if execution["exit_code"] != 0:
                raise ValueError("successful report with failed process")
            raw = (folder / "stdout.jsonl").read_bytes()
            evidence.append((str(folder / "stdout.jsonl"), len(raw), hashlib.sha256(raw).hexdigest()))
            logs = [json.loads(line) for line in raw.splitlines() if line.strip().startswith(b"{")]
            sched = [numeric(row["scheduler_timing"], SCHEDULER) for row in logs if "scheduler_timing" in row]
            indices = [row["completed_iteration"] for row in logs if "scheduler_timing" in row]
            expected = int(report["completed_updates"])
            if indices != list(range(expected)):
                raise ValueError(f"incomplete/duplicate scheduler rows: {folder}")
            update_rows = []
            for i in indices:
                iteration_root = retained / run / f"b{block:02}" / "iterations" / f"{i:06}"
                complete = read(iteration_root / "complete.json", evidence)
                pinned_path = complete["update"]["path"].replace("\\", "/")
                marker = f"/hot/{run}/{attempt.name}/native/iterations/{i:06}/"
                if marker not in pinned_path or complete["iteration"] != i:
                    raise ValueError("retained update belongs to another dispatch attempt")
                relative = pinned_path.split(marker, 1)[1]
                update = read(iteration_root / relative, evidence)
                if evidence[-1][2] != complete["update"]["sha256"]:
                    raise ValueError("retained update differs from completion pin")
                if update["complete"] is not True:
                    raise ValueError("incomplete update")
                row = numeric(update, UPDATE)
                row["policy_substeps"] = int(update["policy_substeps"])
                row["physical_decisions"] = int(update["physical_decisions"])
                update_rows.append(row)
            for scheduler_row, update_row in zip(sched, update_rows):
                scheduler_row["scheduler_other_seconds"] = scheduler_row["iteration_wall_seconds"] - sum(scheduler_row[k] for k in SCHEDULER[1:])
                update_row["update_other_seconds"] = update_row["update_elapsed_seconds"] - sum(update_row[k] for k in UPDATE[1:])
                if scheduler_row["scheduler_other_seconds"] < -0.001 or update_row["update_other_seconds"] < -0.001:
                    raise ValueError("overlapping timing fields")
                if update_row["update_elapsed_seconds"] > scheduler_row["update_execution_seconds"] + 0.01:
                    raise ValueError("inner update exceeds enclosing scheduler timing")
            s, u = aggregate(sched), aggregate(update_rows)
            row = {"run": run, "block": block, "attempt": attempt.name,
                   "host": report["placement"]["host"], "workers": report["placement"]["workers"],
                   "cpu_affinity": report["placement"]["cpu_affinity"],
                   "source_commit": report["runtime"]["engine_commit"],
                   "binary_sha256": report["runtime"]["binary"]["sha256"],
                   "updates": expected, "games": report["completed_games"],
                   "dispatch_seconds": report["seconds"], "execution_seconds": execution["seconds"],
                   "post_execution_seconds": report["seconds"] - execution["seconds"],
                   "execution_outside_iterations_seconds": execution["seconds"] - s["iteration_wall_seconds"],
                   "native_logical_bytes": report["native_logical_bytes"],
                   "recovery_bytes": report["recovery_bytes"],
                   "scheduler": s, "update": u,
                   "iteration_distribution": distribution([r["iteration_wall_seconds"] for r in sched])}
            if state_block:
                match = matching[0]
                row["attempt_started"] = match["started"]
                row["attempt_finished"] = match["finished"]
                row["attempt_envelope_seconds"] = timestamp(match["finished"]) - timestamp(match["started"])
                row["state_block_keys"] = sorted(state_block)
            block_hashes = evidence[start_evidence:]
            row["inputs"] = {"files": len(block_hashes), "bytes": sum(x[1] for x in block_hashes),
                             "ordered_path_size_sha256_digest": hashlib.sha256(json.dumps(block_hashes, separators=(",", ":")).encode()).hexdigest(),
                             "report_path": str(report_path), "report_sha256": hashlib.sha256(report_path.read_bytes()).hexdigest()}
            blocks.append(row)
            print(f"{run}/{attempt.name}: {expected} pinned updates", flush=True)
            all_scheduler.extend(sched)
            all_updates.extend(update_rows)
    if not blocks:
        raise ValueError("no completed blocks")
    identities = {(b["source_commit"], b["binary_sha256"]) for b in blocks}
    if len(identities) != 1:
        raise ValueError("mixed native runtimes; audit each separately")
    seen = [(b["run"], b["block"]) for b in blocks]
    if len(set(seen)) != len(seen):
        raise ValueError("duplicate productive block")
    summary = {
        "blocks": len(blocks), "updates": sum(b["updates"] for b in blocks),
        "games": sum(b["games"] for b in blocks),
        "scheduler": aggregate(all_scheduler), "update": aggregate(all_updates),
        "iteration_distribution": distribution([r["iteration_wall_seconds"] for r in all_scheduler]),
    }
    for key in ("dispatch_seconds", "execution_seconds", "post_execution_seconds", "execution_outside_iterations_seconds", "native_logical_bytes", "recovery_bytes"):
        summary[key] = sum(b[key] for b in blocks)
    intervals = [(timestamp(b["attempt_started"]), timestamp(b["attempt_finished"])) for b in blocks if "attempt_started" in b]
    if intervals:
        summary["successful_attempt_interval_union_seconds"] = union_seconds(intervals)
        summary["successful_attempt_first_to_last_seconds"] = max(hi for lo, hi in intervals) - min(lo for lo, hi in intervals)
        summary["interval_coverage_blocks"] = len(intervals)
    per_run = {}
    for run in runs:
        rb = [b for b in blocks if b["run"] == run]
        if rb:
            per_run[run] = {"blocks": len(rb), "updates": sum(b["updates"] for b in rb),
                            "dispatch_seconds": sum(b["dispatch_seconds"] for b in rb),
                            "scheduler": aggregate([b["scheduler"] for b in rb]),
                            "update": aggregate([b["update"] for b in rb])}
    return {"schema": "training-throughput-audit/v1", "accounting": "sum of process wall times; nested scheduler/update counters must not be added together",
            "inputs": {"hot": str(hot), "retained": str(retained), "state": str(state),
                       "file_count": len(evidence), "bytes": sum(e[1] for e in evidence),
                       "ordered_path_size_sha256_digest": hashlib.sha256(json.dumps(evidence, separators=(",", ":")).encode()).hexdigest()},
            "summary": summary, "per_run": per_run, "blocks": blocks, "excluded": excluded}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hot", type=Path, required=True)
    parser.add_argument("--retained", type=Path, required=True)
    parser.add_argument("--state", type=Path)
    parser.add_argument("--runs", nargs="+", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = audit(args.hot, args.retained, args.runs, args.state)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps(result["summary"], indent=2))
