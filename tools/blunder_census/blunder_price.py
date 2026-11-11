"""Blunder pricing driver (collab LANES/pricing-20261111, DEFINITIONS.md). Measurement only.

Turns census cases (E:/census-20261110/<checkpoint>/cases.jsonl and jobs.jsonl.gz) into forced-arm cases, runs
`regret_census_v1 mode=s4a-blunder-price` over shards, and summarizes.

CLI:
  cases    --census DIR --panels b512,c1-u0524288 --out DIR [--share 1.0]
           writes DIR/cases-<panel>.jsonl (price and score specs) and DIR/jobs-<panel>.jsonl (their label jobs)
  price    --cases CASES.jsonl --jobs JOBS.jsonl --sources SOURCES.json --exe EXE --out DIR --shards K
           [--n 64] [--n-true 32] [--tag T]
  score    --cases CASES.jsonl --jobs JOBS.jsonl --scorers SCORERS.json --exe EXE --out DIR --shards K [--tag T]
  summary  --root DIR --out DIR      (tables, chart, same-state table)
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import math
import os
import random
import subprocess
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

BELOW_NORMAL = getattr(subprocess, "BELOW_NORMAL_PRIORITY_CLASS", 0)
CLASSES = ("a", "b", "c", "d")


def order_key(case_id: str, index: int) -> str:
    return hashlib.sha256(f"pricing-20261111/{case_id}/{index}".encode()).hexdigest()


def flagged(row: dict) -> str | None:
    kind = row["kind"]
    if kind == "a_opportunity" and not row["converted"]:
        return "a"
    if kind == "b_own_only" and row["harmful"]:
        return "b"
    if kind == "c_chump" and row["pointless_half"]:
        return "c"
    if kind == "d_combat_loss" and row["class"] == "blunder":
        return "d"
    return None


def spec(row: dict, cls: str, job: dict) -> dict | None:
    """The price and score spec of one census case (DEFINITIONS section 2 and 3)."""
    d = job["decisions"]
    focal = job["focal"]
    i = row["index"]
    out = {"id": row["id"], "class": cls, "index": i}
    if cls == "a":
        out["chosen"] = {"kind": "rows", "rows": [row["selected"]]}
        out["alt"] = {"kind": "oracle_turn"}
        good = row.get("line_row")
        out["divergence"] = ({"index": i, "bad_row": row["selected"], "good": {"kind": "row", "row": good}}
                             if good is not None and good != row["selected"] else None)
    elif cls == "b":
        last = max(s["index"] for s in row["target_steps"])
        rows = [d[k]["selected"] for k in range(i, last + 1) if d[k]["actor"] == focal]
        out["chosen"] = {"kind": "rows", "rows": rows}
        out["alt"] = {"kind": "pass"}
        out["divergence"] = {"index": i, "bad_row": d[i]["selected"], "good": {"kind": "pass"}}
    elif cls == "c":
        scans = row["scans"]
        out["chosen"] = {"kind": "rows", "rows": [d[s["index"]]["selected"] for s in scans]}
        out["alt"] = {"kind": "exclude_scans"}
        first = next((s for s in scans if s["include_chosen"]), None)
        out["divergence"] = ({"index": first["index"], "bad_row": d[first["index"]]["selected"],
                              "good": {"kind": "exclude"}} if first else None)
    else:
        scans, path = row["scans"], row["surviving_path_include"]
        out["chosen"] = {"kind": "rows", "rows": row["recorded_rows"]}
        out["alt"] = {"kind": "scan_path", "path": path}
        div = None
        for k, s in enumerate(scans):
            if k >= len(path):
                break
            if s["include_chosen"] != path[k]:
                div = {"index": s["index"], "bad_row": d[s["index"]]["selected"],
                       "good": {"kind": "include" if path[k] else "exclude"}}
                break
        out["divergence"] = div
    return out


def build_cases(census: Path, panels: list[str], out: Path, share: float) -> dict:
    out.mkdir(parents=True, exist_ok=True)
    counts = {}
    for panel in panels:
        rows = [json.loads(x) for x in (census / panel / "cases.jsonl").read_text(encoding="utf-8").splitlines()]
        picked = [(flagged(r), r) for r in rows]
        picked = [(c, r) for c, r in picked if c]
        keep = []
        for cls in CLASSES:
            members = sorted((r for c, r in picked if c == cls), key=lambda r: order_key(r["id"], r["index"]))
            keep += [(cls, r) for r in members[:math.ceil(share * len(members))]]
        ids = {r["id"] for _, r in keep}
        jobs = {}
        with gzip.open(census / panel / "jobs.jsonl.gz", "rt", encoding="utf-8") as f:
            for line in f:
                if not line.strip():
                    continue
                j = json.loads(line)
                if j["id"] in ids:
                    jobs[j["id"]] = (j, line.strip())
        specs = [spec(r, cls, jobs[r["id"]][0]) for cls, r in keep]
        (out / f"cases-{panel}.jsonl").write_text("".join(json.dumps(s) + "\n" for s in specs), encoding="utf-8")
        (out / f"jobs-{panel}.jsonl").write_text("".join(jobs[i][1] + "\n" for i in sorted(jobs)), encoding="utf-8")
        counts[panel] = {cls: sum(1 for c, _ in keep if c == cls) for cls in CLASSES}
    return counts


def run_shards(exe: str, cases: Path, jobs: Path, out: Path, shards: int, env_extra: dict, tag: str,
               keep=lambda s: True) -> dict:
    lines = [x for x in cases.read_text(encoding="utf-8").splitlines() if x.strip() and keep(json.loads(x))]
    out.mkdir(parents=True, exist_ok=True)
    for stale in (out / f"run-{tag}.json", out / f"run-{tag}.failed.json"):
        stale.unlink(missing_ok=True)
    parts = []
    for k in range(shards):
        part = lines[k::shards]
        if part:
            p = out / f"cases-{tag}-{k:02d}.jsonl"
            p.write_text("".join(x + "\n" for x in part), encoding="utf-8")
            parts.append((k, p, len(part)))

    def one(item):
        k, p, count = item
        rows_path = out / f"rows-{tag}-{k:02d}.jsonl"
        env = dict(os.environ, LABEL_JOBS=str(jobs), PRICE_CASES=str(p), CUDA_VISIBLE_DEVICES="",
                   PRICE_TIMING=str(out / f"timing-{tag}-{k:02d}.jsonl"), **env_extra)
        with (out / f"log-{tag}-{k:02d}.txt").open("w") as log:
            code = subprocess.run([exe, "mode=s4a-blunder-price", f"out={rows_path}"], stdout=log,
                                  stderr=subprocess.STDOUT, env=env, creationflags=BELOW_NORMAL).returncode
        rows = rows_path.read_text(encoding="utf-8").splitlines() if rows_path.exists() else []
        errors = sum(1 for r in rows if json.loads(r)["kind"] == "error")
        return {"shard": k, "file": rows_path.name, "exit": code, "cases": count, "rows": len(rows),
                "errors": errors, "complete": code == 0 and len(rows) == count and errors == 0}

    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=shards) as pool:
        results = list(pool.map(one, parts))
    record = {"tag": tag, "cases": str(cases), "jobs": str(jobs), "exe_sha256": hashlib.sha256(Path(exe).read_bytes()).hexdigest(),
              "env": env_extra, "expected": len(lines), "shards": results,
              "complete": all(r["complete"] for r in results), "seconds": time.monotonic() - started}
    (out / (f"run-{tag}.json" if record["complete"] else f"run-{tag}.failed.json")).write_text(json.dumps(record, indent=1))
    if not record["complete"]:
        raise SystemExit(f"pricing pass {tag} incomplete: {[r for r in results if not r['complete']]}")
    return record


# ---------------------------------------------------------------- summary

def rows(out: Path, tag: str) -> list[dict]:
    """The rows of a successful pass: only the shard files its manifest lists (run-<tag>.json), every requested
    case exactly once, no error row."""
    record = json.loads((out / f"run-{tag}.json").read_text(encoding="utf-8"))
    if not record["complete"]:
        raise SystemExit(f"{out}/run-{tag}.json is not a complete pass")
    found = []
    for s in record["shards"]:
        name = s.get("file", f"rows-{tag}-{s['shard']:02d}.jsonl")
        found += [json.loads(x) for x in (out / name).read_text(encoding="utf-8").splitlines() if x.strip()]
    keys = [(r["id"], r["index"]) for r in found]
    if len(keys) != len(set(keys)):
        raise SystemExit(f"{out} {tag}: duplicate case rows")
    if len(found) != record["expected"] or any(r["kind"] == "error" for r in found):
        raise SystemExit(f"{out} {tag}: rows do not cover the requested cases without error")
    return found


def case_gap(arms: dict) -> dict | None:
    """Paired gap of one case in one world mode; None (an invalid case) when any rollout of either arm has no
    natural outcome: an invalid forced row, a truncation, a halt or the step bound (null scores, or the older
    rows' nonzero invalid or unnatural counts)."""
    if any(arms.get("invalid", [0, 0])) or any(arms.get("unnatural", [0, 0])):
        return None
    pairs = [(a, c) for a, c in zip(arms["alt"], arms["chosen"])]
    if any(a is None or c is None for a, c in pairs):
        return None
    n = len(pairs)
    if n < 2:
        return None
    diffs = [a - c for a, c in pairs]
    mean = sum(diffs) / n
    sd = math.sqrt(sum((x - mean) ** 2 for x in diffs) / (n - 1))
    half = 1.959964 * sd / math.sqrt(n)
    lo, hi = mean - half, mean + half
    label = ("clearly_positive" if lo > 0 else "negative" if hi < 0
             else "near_zero" if lo >= -0.05 and hi <= 0.05 else "unresolved")
    value = sum(c for _, c in pairs) / n
    return {"gap": mean, "lo": lo, "hi": hi, "label": label, "n": n, "value_chosen": value,
            "value_alt": sum(a for a, _ in pairs) / n, "decided": value < 0.05 or value > 0.95}


def bootstrap(values: list[float], seed: int = 20261111, reps: int = 2000) -> list[float] | None:
    if not values:
        return None
    rng = random.Random(seed)
    n = len(values)
    means = sorted(sum(values[rng.randrange(n)] for _ in range(n)) / n for _ in range(reps))
    return [means[int(0.025 * reps)], means[int(0.975 * reps) - 1]]


def per_case(out: Path, tag: str) -> list[dict]:
    """One record per case and world mode: the gap, its interval, label, values and decided flag, or invalid."""
    recs = []
    for r in rows(out, tag):
        for mode in ("worlds", "true"):
            g = case_gap(r[mode])
            recs.append({"id": r["id"], "index": r["index"], "class": r["class"], "mode": mode,
                         "valid": g is not None, "invalid_rollouts": r[mode].get("invalid"),
                         "unnatural_rollouts": r[mode].get("unnatural"), **(g or {})})
    return recs


def summarize_price(out: Path, tag: str) -> dict:
    result = {}
    invalid = {}
    for rec in per_case(out, tag):
        key = (rec["class"], rec["mode"])
        if rec["valid"]:
            result.setdefault(key, []).append(rec)
        else:
            invalid[key] = invalid.get(key, 0) + 1
    table = {}
    for (cls, mode), cases in sorted(result.items()):
        gaps = [c["gap"] for c in cases]
        live = [c for c in cases if not c["decided"]]
        labels = {k: sum(1 for c in cases if c["label"] == k) / len(cases)
                  for k in ("clearly_positive", "near_zero", "negative", "unresolved")}
        table[f"{cls}/{mode}"] = {
            "cases": len(cases), "mean_gap": sum(gaps) / len(gaps), "ci": bootstrap(gaps),
            "median_gap": sorted(gaps)[len(gaps) // 2], "labels": labels,
            "decided_share": 1 - len(live) / len(cases),
            "live_mean_gap": (sum(c["gap"] for c in live) / len(live)) if live else None,
            "live_ci": bootstrap([c["gap"] for c in live]),
            "live_labels": ({k: sum(1 for c in live if c["label"] == k) / len(live)
                             for k in ("clearly_positive", "near_zero", "negative", "unresolved")} if live else None),
            "mean_value_chosen": sum(c["value_chosen"] for c in cases) / len(cases),
            "invalid_cases": invalid.get((cls, mode), 0),
            "gaps": gaps}
    return table


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    c = sub.add_parser("cases")
    c.add_argument("--census", type=Path, required=True)
    c.add_argument("--panels", required=True)
    c.add_argument("--out", type=Path, required=True)
    c.add_argument("--share", type=float, default=1.0)
    for name in ("price", "score"):
        p = sub.add_parser(name)
        p.add_argument("--cases", type=Path, required=True)
        p.add_argument("--jobs", type=Path, required=True)
        p.add_argument("--exe", required=True)
        p.add_argument("--out", type=Path, required=True)
        p.add_argument("--shards", type=int, required=True)
        p.add_argument("--tag", default=name)
        if name == "price":
            p.add_argument("--sources", type=Path, required=True)
            p.add_argument("--n", type=int, default=64)
            p.add_argument("--n-true", type=int, default=32)
            p.add_argument("--limit", type=int, default=0, help="first L cases per class (pilot only)")
        else:
            p.add_argument("--scorers", type=Path, required=True)
    s = sub.add_parser("summary")
    s.add_argument("--root", type=Path, required=True)
    s.add_argument("--tag", required=True)
    a = ap.parse_args(argv)
    if a.op == "cases":
        print(json.dumps(build_cases(a.census, a.panels.split(","), a.out, a.share)))
    elif a.op == "price":
        seen = {}

        def keep(case):
            seen[case["class"]] = seen.get(case["class"], 0) + 1
            return not a.limit or seen[case["class"]] <= a.limit
        r = run_shards(a.exe, a.cases, a.jobs, a.out, a.shards,
                       {"PRICE_MODE": "price", "PRICE_SOURCES": str(a.sources), "PRICE_N": str(a.n),
                        "PRICE_N_TRUE": str(a.n_true)}, a.tag, keep)
        print(json.dumps({"seconds": r["seconds"], "expected": r["expected"]}))
    elif a.op == "score":
        r = run_shards(a.exe, a.cases, a.jobs, a.out, a.shards,
                       {"PRICE_MODE": "score", "PRICE_SCORERS": str(a.scorers)}, a.tag,
                       lambda case: case.get("divergence") is not None)
        print(json.dumps({"seconds": r["seconds"], "expected": r["expected"]}))
    else:
        t = summarize_price(a.root, a.tag)
        print(json.dumps({k: {x: v[x] for x in ("cases", "mean_gap", "ci", "labels")} for k, v in t.items()}, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
