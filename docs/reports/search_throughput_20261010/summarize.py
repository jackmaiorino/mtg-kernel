"""Read existing Stage 4a timing evidence, without launching games or inspecting outcomes.

Only allowlisted timing, work-count, configuration, and receipt fields are retained.
Input directories are explicit; the job names below are a fixed audit inventory.
All JSONL sources require a matching terminal receipt hash, except the instrumented
engineering probe, which has a direct exit-0 log and an unchanged-file read check.
Interrupted formal jobs are isolated from successful shards before combination.
"""

import argparse
import hashlib
import json
from pathlib import Path
import re


ARMS = ("A", "D", "E")
STAGES = ("inference", "engine_step", "canon_key", "labels", "sampler", "session_clone", "tree")
LIMITS = ("formal", "select_cap", "eval_worlds", "eval_cap", "a_k", "d_top", "d_uniform",
          "d_horizon_physical", "e_max_depth", "e_min_exec_backups", "e_ucb_c2",
          "max_consecutive_rejections")


def pick(obj, keys):
    return {k: obj[k] for k in keys if k in obj}


def dist(values):
    xs = sorted(values)
    if not xs:
        return {"n": 0}
    def quantile(q):
        z = (len(xs) - 1) * q
        i = int(z)
        return xs[i] + (xs[min(i + 1, len(xs) - 1)] - xs[i]) * (z - i)
    return {"n": len(xs), "sum": round(sum(xs), 6), "min": round(xs[0], 6),
            "median": round(quantile(.5), 6), "p90": round(quantile(.9), 6),
            "p95": round(quantile(.95), 6), "max": round(xs[-1], 6)}


class Reader:
    def __init__(self):
        self.sources = {}

    def read(self, path):
        path = Path(path)
        before = path.stat()
        data = path.read_bytes()
        after = path.stat()
        if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
            raise ValueError(f"Source changed during read: {path}")
        digest = hashlib.sha256(data).hexdigest()
        key = path.as_posix()
        if key in self.sources and self.sources[key]["sha256"] != digest:
            raise ValueError(f"Source changed across reads: {path}")
        self.sources[key] = {"sha256": digest, "bytes": len(data)}
        return data

    def obj(self, path):
        return json.loads(self.read(path))

    def queue_receipts(self, path):
        receipts, cpu = {}, {}
        for line in self.read(path).decode("utf-8-sig").splitlines():
            m = re.search(r" (\S+) last sampled cpu seconds ([0-9.]+)$", line)
            if m:
                cpu[m[1]] = float(m[2])
            if " end: " in line:
                item = json.loads(line.split(" end: ", 1)[1])
                job = item["job"]
                receipts[job] = pick(item, ("job", "exit", "rows_by_kind", "error_rows", "sha256", "wall_seconds"))
                receipts[job]["terminal_timestamp"] = line.split()[0]
                if job in cpu:
                    receipts[job]["last_sampled_cpu_seconds"] = cpu.pop(job)
        return receipts

    def rows(self, path, receipt=None, direct_exit_log=None):
        raw = self.read(path)
        if receipt:
            if hashlib.sha256(raw).hexdigest() != receipt["sha256"]:
                raise ValueError(f"Terminal receipt hash mismatch: {path}")
            if receipt.get("error_rows", 0):
                raise ValueError(f"Error rows in receipt: {path}")
        elif direct_exit_log:
            log = self.read(direct_exit_log).decode("utf-8-sig")
            if not re.search(r"(?m)^exit 0\s*$", log):
                raise ValueError(f"No successful terminal log: {path}")
        else:
            raise ValueError(f"No terminal receipt: {path}")
        result = []
        for line in raw.decode("utf-8-sig").splitlines():
            if not line.strip():
                continue
            source = json.loads(line)
            if source.get("kind") != "s4a_root":
                raise ValueError(f"Unexpected row kind in {path}")
            # Never retain arms.*.eval, summary, discovery, probs, or primary hash.
            timing = source["timing"]
            row = pick(source, ("model", "root_id", "stratum"))
            row["limits"] = pick(source["config"]["limits"], LIMITS)
            row["timing"] = pick(timing, ("root_wall", "eval_wall", "replay", "eval_sampler_seconds"))
            row["selection_wall"] = {a: timing["selection_wall"][a] for a in ARMS}
            row["sampler_wall"] = {a: timing["selection_sampler_seconds"][a] for a in ARMS}
            row["counts"] = {a: pick(source["arms"][a]["selection"],
                                    ("inference_calls", "transitions", "simulations_attempted")) for a in ARMS}
            row["eval_transitions"] = source["cost"]["eval_transitions"]
            row["d_improved_decisions"] = source["arms"]["D"]["selection"]["d_continuation"]["improved_decisions"]
            if "profile" in timing:
                row["profile"] = {a: {s: pick(timing["profile"][a][s], ("calls", "seconds"))
                                      for s in STAGES} for a in (*ARMS, "eval")}
            result.append(row)
        if receipt and len(result) != receipt["rows_by_kind"].get("s4a_root"):
            raise ValueError(f"Receipt count mismatch: {path}")
        if len({(r["model"], r["root_id"]) for r in result}) != len(result):
            raise ValueError(f"Duplicate rows: {path}")
        return result


def summarize(rows):
    if not rows:
        raise ValueError("Empty cohort")
    if len({(r["model"], r["root_id"]) for r in rows}) != len(rows):
        raise ValueError("Duplicate roots in cohort")
    configs = {json.dumps(r["limits"], sort_keys=True) for r in rows}
    root = sum(r["timing"]["root_wall"] for r in rows)
    selection = sum(sum(r["selection_wall"].values()) for r in rows)
    sampler = sum(sum(r["sampler_wall"].values()) for r in rows)
    evaluation = sum(r["timing"]["eval_wall"] for r in rows)
    replay = sum(r["timing"]["replay"] for r in rows)
    result = {"roots": len(rows), "limits": [json.loads(x) for x in sorted(configs)],
              "root_wall_seconds": dist([r["timing"]["root_wall"] for r in rows]),
              "eval_wall_seconds": dist([r["timing"]["eval_wall"] for r in rows]),
              "replay_seconds": dist([r["timing"]["replay"] for r in rows]),
              "shares_of_sum_root_wall": {"selection": selection/root, "evaluation": evaluation/root,
                 "replay": replay/root, "residual": (root-selection-evaluation-replay)/root,
                 "selection_sampler_included_in_selection": sampler/root}, "arms": {}}
    for a in ARMS:
        calls = sum(r["counts"][a]["inference_calls"] for r in rows)
        transitions = sum(r["counts"][a]["transitions"] for r in rows)
        wall = sum(r["selection_wall"][a] for r in rows)
        result["arms"][a] = {"selection_wall_seconds": dist([r["selection_wall"][a] for r in rows]),
            "inference_calls": dist([r["counts"][a]["inference_calls"] for r in rows]),
            "transitions": dist([r["counts"][a]["transitions"] for r in rows]),
            "sampler_wall_seconds": round(sum(r["sampler_wall"][a] for r in rows), 6),
            "whole_selection_ms_per_inference_call": wall/calls*1000,
            "inference_calls_per_transition": calls/transitions}
    result["d_improved_decisions"] = sum(r["d_improved_decisions"] for r in rows)
    result["by_stratum_root_wall_seconds"] = {s: dist([r["timing"]["root_wall"] for r in rows if r["stratum"] == s])
            for s in sorted({r["stratum"] for r in rows})}
    if all("profile" in r for r in rows):
        result["profile"] = {}
        for a in (*ARMS, "eval"):
            denominator = sum(r["selection_wall"][a] if a in ARMS else r["timing"]["eval_wall"] for r in rows)
            result["profile"][a] = {s: {"calls": sum(r["profile"][a][s]["calls"] for r in rows),
                "seconds": sum(r["profile"][a][s]["seconds"] for r in rows),
                "share_of_phase_wall": sum(r["profile"][a][s]["seconds"] for r in rows)/denominator}
                for s in STAGES}
        result["profile_selection_totals"] = {s: {"calls": sum(r["profile"][a][s]["calls"] for r in rows for a in ARMS),
            "seconds": sum(r["profile"][a][s]["seconds"] for r in rows for a in ARMS),
            "share_of_selection_wall": sum(r["profile"][a][s]["seconds"] for r in rows for a in ARMS)/selection}
            for s in STAGES}
    return result


def paired(left, right):
    l = {(r["model"], r["root_id"]): r for r in left}
    r = {(r["model"], r["root_id"]): r for r in right}
    if l.keys() != r.keys():
        raise ValueError("Pair root IDs differ")
    if any(l[k]["limits"] != r[k]["limits"] for k in l):
        raise ValueError("Pair limits differ")
    count_matches = {a: sum(l[k]["counts"][a] == r[k]["counts"][a] for k in l) for a in ARMS}
    return {"paired_roots": len(l), "matching_work_count_roots_by_arm": count_matches,
            "left_over_right_root_wall_ratio": sum(x["timing"]["root_wall"] for x in left)/sum(x["timing"]["root_wall"] for x in right),
            "paired_root_wall_ratio_distribution": dist([l[k]["timing"]["root_wall"]/r[k]["timing"]["root_wall"] for k in l]),
            "left_over_right_selection_wall_ratio": {a: sum(x["selection_wall"][a] for x in left)/sum(x["selection_wall"][a] for x in right) for a in ARMS}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--formal-root", type=Path, required=True)
    parser.add_argument("--fast-root", type=Path, required=True)
    parser.add_argument("--profile-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--check", action="store_true", help="Recompute and require exact existing output bytes")
    args = parser.parse_args()
    reader = Reader()
    formal, fast, prof = args.formal_root, args.fast_root, args.profile_root
    receipts = reader.queue_receipts(formal/"out/queue.log")
    cohorts, jobs = {}, {}
    for label, names in [("formal_retained_roots_from_interrupted_jobs", [f"formal-unordered-{m}" for m in ("r1", "r2")]),
                         ("formal_successful_shards", [f"formal-unordered-{m}-s{i}" for m in ("r1", "r2") for i in range(4)]),
                         ("qualification_serial", ["q6-gyg-probe-1w"]),
                         ("qualification_parallel", ["q6-gyg-probe-8w"])]:
        cohorts[label] = []
        for name in names:
            receipt = receipts[name]
            if label != "formal_retained_roots_from_interrupted_jobs" and receipt["exit"] != 0:
                raise ValueError(f"Job did not succeed: {name}")
            cohorts[label].extend(reader.rows(formal/"out"/f"{name}.jsonl", receipt))
            jobs[name] = receipt
    cohorts["formal_completed_roots"] = cohorts["formal_retained_roots_from_interrupted_jobs"] + cohorts["formal_successful_shards"]
    for label, arm, name in [("exact_check", "arm-exact", "exact"), ("fast_check", "arm-fast", "fast"),
                             ("fast_extra_cast", "arm-fast24a", "fast24a"), ("fast_extra_target", "arm-fast24b", "fast24b")]:
        receipt = reader.queue_receipts(fast/arm/"out/queue.log")[name]
        if receipt["exit"] != 0:
            raise ValueError(f"Job did not succeed: {name}")
        cohorts[label] = reader.rows(fast/arm/"out"/f"{name}.jsonl", receipt)
        launch = reader.read(fast/arm/"run.bat").decode("utf-8-sig")
        setting = re.search(r"(?mi)^set S4A_FAST_FORWARD=([^\r\n]*)", launch)
        if not setting:
            raise ValueError(f"Fast-forward setting missing: {arm}")
        receipt["configured_fast_forward"] = setting[1] or "unset"
        queue = reader.obj(fast/arm/"queue.json")
        receipt["queue_configuration"] = pick(queue[0], ("mode", "model", "workers", "base_seed", "roots"))
        jobs[name] = receipt
    cohorts["instrumented_engineering"] = reader.rows(prof/"prof-r1.jsonl", direct_exit_log=prof/"prof-r1.log")
    reader.read(prof/"run-prof.bat")
    manifest = reader.obj(formal/"RUN-MANIFEST-unordered.json")
    binary = reader.obj(fast/"BINARY.json")
    queues = {}
    for name in ("queue-formal-unordered.json", "queue-formal-unordered-resume.json", "queue-formal-unordered-resume2.json", "queue-formal-shards.json", "queue-qual-gyg.json"):
        items = reader.obj(formal/name)
        flat = [job for item in items for job in item.get("parallel", [item])]
        queues[name] = [pick(x, ("name", "mode", "model", "workers", "base_seed", "roots", "limits", "max_wall_seconds", "max_cpu_seconds", "group_max_cpu_seconds")) for x in flat]
    result = {"schema": "search-timing-audit-v1", "scope": "Existing terminal Stage 4a artifacts only; no new games, outcome analysis, or CP7.",
        "denominators": {"root_wall": "Sum of per-root elapsed times; overlapping parallel roots mean this is not campaign elapsed time or CPU time.",
            "whole_selection_ms_per_inference_call": "Entire selection wall divided by call count, not measured neural-network latency.",
            "profile_inference": "Nested policy scoring/observation/tensor/forward/action work inside timed choose; D direct improvement scores sit outside this timer.",
            "cpu": "Last sampled process CPU, not exact final CPU; original restarted formal jobs are not summed into a campaign total.",
            "quantiles": "Linear interpolation at (n-1)*q; descriptive only, no uncertainty intervals.",
            "fast_check": "Four selected roots on shared E-cores; not a representative speed or efficacy estimate.",
            "engineering_profile": "Two roots at formal per-root budgets, engineering instrumentation, workers=2 on E-cores 16-17; not the formal campaign.",
            "profile_coverage": "SAMPLE includes redeterminization clones; explicit CLONE=0 does not mean no cloning. Eval-world sampling bypasses SAMPLE. Some canonicalization/child-key/tree selection lies outside CANON/TREE wrappers. Reset before E omits replay and initial root scoring; timer overhead is unqualified.",
            "d_improved_decisions": "Count of D improvement decisions; removing one repeated scorer per decision is a call-count upper bound, not a measured time saving."},
        "provenance": {"formal_manifest": {"code": pick(manifest["code"], ("repo", "pr", "commit", "toolchain", "profile")),
            "binary": pick(manifest["binary"], ("path", "sha256")), "variant": manifest["variant"],
            "placement": pick(manifest["placement"], ("host", "cores", "workers", "gpu")),
            "budgets": pick(manifest["budgets"], ("global_worker_seconds", "corpus_worker_seconds", "formal_max_wall_seconds_per_job"))},
            "fast_binary": pick(binary, ("host", "commit", "pr", "toolchain", "profile", "binary")),
            "profile_commit": "59418e4cf16b7f39e0205a032308bab4ae615f99"},
        "queue_configurations": queues, "jobs": jobs,
        "cohorts": {name: summarize(rows) for name, rows in cohorts.items()},
        "comparisons": {"exact_over_fast": paired(cohorts["exact_check"], cohorts["fast_check"]),
            "qualification_serial_over_parallel": paired(cohorts["qualification_serial"], cohorts["qualification_parallel"])},
        "sources": reader.sources}
    result["comparisons"]["exact_over_fast"].update({"last_sampled_process_cpu_ratio": jobs["exact"]["last_sampled_cpu_seconds"]/jobs["fast"]["last_sampled_cpu_seconds"],
        "queue_wall_ratio": jobs["exact"]["wall_seconds"]/jobs["fast"]["wall_seconds"],
        "cpu_over_queue_wall": {name: jobs[name]["last_sampled_cpu_seconds"]/jobs[name]["wall_seconds"] for name in ("exact", "fast")}})
    result["comparisons"]["qualification_serial_over_parallel"]["completed_roots_per_queue_second_speedup"] = jobs["q6-gyg-probe-1w"]["wall_seconds"]/jobs["q6-gyg-probe-8w"]["wall_seconds"]
    data = (json.dumps(result, indent=2, sort_keys=True, allow_nan=False) + "\n").encode()
    if args.check:
        if args.output.read_bytes() != data:
            raise SystemExit("Evidence differs from recomputed output")
        print(f"Verified identical output: {args.output} ({len(data)} bytes)")
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_bytes(data)
        print(f"Wrote {args.output} ({len(data)} bytes)")


if __name__ == "__main__":
    main()
