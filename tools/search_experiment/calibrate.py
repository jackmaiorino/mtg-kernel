"""Read cost-only baseline completions, never action quality, and pin CPU targets."""
import argparse
import json
import math
from pathlib import Path

from launch import digest, load, publish, require


def calibrate(paths):
    roots = {}
    for path in paths:
        batch = load(path)
        require(not batch["error"], "incomplete calibration batch")
        for job in batch["results"]:
            rows = [json.loads(line) for line in Path(job["output"]).read_text().splitlines() if line.strip()]
            require(len(rows) == 1, "calibration requires one root per process")
            row = rows[0]
            require(row["kind"] == "search_allocation_experiment" and row["arm"] == "A", "calibration must be baseline A")
            require(row["evaluation"].get("performed") is False, "calibration must not inspect action quality")
            anchor = row.get("anchor", row["budget"])
            require(row["budget"] == anchor and anchor in (16000, 64000), "baseline calibration must use actual transition anchor")
            cpu = job["cpu_seconds"]
            require(math.isfinite(cpu) and cpu > 0, "final process CPU missing")
            # Source load is already counted by the native full process cutoff.
            # Reserve observed serialization/output CPU after search finishes.
            search_absolute = row.get("search_process_cpu_seconds")
            require(isinstance(search_absolute, (float, int)), "absolute search CPU missing")
            reserve = max(0.0, cpu - search_absolute)
            record = {"native_cpu_seconds": cpu, "baseline_full_cpu_seconds": cpu,
                      "overhead_reserve_seconds": reserve, "baseline_transitions": row["selection"]["transitions"],
                      "baseline_work_attempts": row["work_attempts"], "wall_seconds": job["wall_seconds"],
                      "output": {"path": job["output"], "sha256": digest(job["output"])},
                      "affinity_core": job["affinity_core"]}
            require(str(anchor) not in roots.setdefault(row["id"], {}), "duplicate calibration root/anchor")
            roots[row["id"]][str(anchor)] = record
    return {"schema": "search-sharing-allocation-cost-calibration/v1", "roots": roots,
            "sources": [{"path": str(path), "sha256": digest(path)} for path in paths],
            "quality_read": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--completion", action="append", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    result = calibrate(args.completion)
    publish(args.out, result)
    print(json.dumps({"roots": len(result["roots"]), "quality_read": False}))
