"""Stage 4a cost-only qualification readout (RUNNER.md section 5).

Reads only cost, legality/parity and coverage-of-code-path fields from the
qualification rows; never a win, label, discovery, action or effect field.

  python qualify_stage4a.py --out-dir D:/stage4a-20261009/out --corpus-worker-seconds S
Prints and writes QUALIFICATION.json next to the rows.
"""
import argparse
import json
from pathlib import Path

CAP_WORKER_SECONDS = 64 * 3600
QUAL_CAP_WORKER_SECONDS = 2 * 3600
FORMAL_ROOTS = 200


def rows(path):
    out = []
    if not Path(path).exists():
        return out
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if line.strip():
            r = json.loads(line)
            if r.get("kind") == "s4a_root":
                out.append({"root_id": r["root_id"], "invalid": r["invalid"],
                            "formal_limits": r["config"]["limits"]["formal"],
                            "root_wall": r["timing"]["root_wall"],
                            "selection_wall": r["timing"]["selection_wall"],
                            "eval_wall": r["timing"]["eval_wall"],
                            "selection_transitions": {a: r["arms"][a]["selection"]["transitions"] for a in "EAD"},
                            "selection_inference": {a: r["arms"][a]["selection"]["inference_calls"] for a in "EAD"},
                            "cap_reached": {a: r["arms"][a]["selection"]["cap_reached"] for a in "EAD"},
                            "faults": sum(len(r["arms"][a]["selection"]["faults"]) for a in "EAD")
                            + sum(r["arms"][a]["summary"]["faults"] for a in "EAD"),
                            "sampler_rejections": {a: r["arms"][a]["selection"]["sampler"]["rejected"] for a in "EAD"},
                            "eval_rejected_worlds": len(r["rejected_eval_worlds"]),
                            "eval_transitions": r["cost"]["eval_transitions"],
                            "tree_nodes": r["arms"]["E"]["selection"]["extra"]["tree"]["nodes"],
                            "primary_sha256": r["primary_sha256"]})
            elif r.get("kind") == "error":
                out.append({"error": r.get("error"), "root_id": r.get("root_id")})
    return out


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--corpus-worker-seconds", type=float, required=True)
    a = ap.parse_args(argv)
    d = Path(a.out_dir)
    q1 = rows(d / "q1-eng-r1.jsonl") + rows(d / "q1-eng-r2.jsonl")
    q2 = rows(d / "q2-repeat-r1.jsonl")
    probes = {n: rows(d / f"q3-probe-{n}.jsonl") for n in (1, 8, 16)}
    report = {"engineering": q1, "repeat": q2}
    complete = [r for r in q1 if "root_wall" in r]
    errors = [r for r in q1 + q2 if "error" in r]
    report["packages_complete"] = len(complete)
    report["errors"] = errors
    if complete:
        largest = max(r["root_wall"] for r in complete)
        estimate = largest * FORMAL_ROOTS + a.corpus_worker_seconds
        report["largest_package_worker_seconds"] = largest
        report["admission_estimate_worker_seconds"] = estimate
        report["admission_cap_worker_seconds"] = CAP_WORKER_SECONDS
        report["admission_fits"] = len(complete) == 4 and not errors and estimate <= CAP_WORKER_SECONDS
    if q2 and complete:
        first = q2[0]
        match = [r for r in complete if r["root_id"] == first.get("root_id")]
        report["determinism"] = {"root": first.get("root_id"),
                                 "identical_primary_hash": bool(match) and match[0]["primary_sha256"] == first.get("primary_sha256")}
    probe_summary = {}
    for n, rs in probes.items():
        ok = [r for r in rs if "root_wall" in r]
        if ok:
            probe_summary[n] = {"roots": len(ok), "mean_root_wall": sum(r["root_wall"] for r in ok) / len(ok),
                                "per_worker_transitions_per_second": sum(sum(r["selection_transitions"].values())
                                                                         + r["eval_transitions"] for r in ok)
                                / sum(r["root_wall"] for r in ok)}
    report["probes"] = probe_summary
    qual = sum(r.get("root_wall", 0) for r in q1 + q2) + sum(
        r.get("root_wall", 0) for rs in probes.values() for r in rs)
    report["qualification_worker_seconds"] = qual
    report["qualification_cap_worker_seconds"] = QUAL_CAP_WORKER_SECONDS
    (d / "QUALIFICATION.json").write_text(json.dumps(report, indent=1))
    print(json.dumps({k: v for k, v in report.items() if k not in ("engineering", "repeat")}, indent=1))
    for r in complete:
        print(r["root_id"], round(r["root_wall"]), {k: round(v) for k, v in r["selection_wall"].items()},
              round(r["eval_wall"]), r["selection_transitions"], "faults", r["faults"], "rej",
              r["sampler_rejections"], r["eval_rejected_worlds"], "nodes", r["tree_nodes"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
