"""Blunder census driver (collab LANES/census-20261110, DEFINITIONS.md). Measurement only.

Plays OH2-shape panels with every decision captured through the public evaluator, turns each case's game one into
a label job (panel.py), and replays the jobs through `regret_census_v1 mode=s4a-blunder-census`.

The evaluator and the census pass are run as plain processes inside the caller's host_slots claim (this tool takes
no host reservation). Before a panel is played, `qualify` plays a fixed match sample serially and split over k
processes, requires identical per-match hashes, and records completed matches per second; `play` refuses a plan
without a receipt for the same evaluator, candidate model layout and opponent and uses the fastest parity-clean k.

CLI:
  build    --id ID --candidate SRC.json --opponent OPP.json --decks DECKS.json --out DIR --namespace NS [--shards 16]
  qualify  --plan DIR/plan.json --evaluator EXE --workers 1,2,4 [--sample 16] --root DIR
  play     --plan DIR/plan.json --evaluator EXE --receipt DIR/receipt.json
  jobs     --plan DIR/plan.json --out JOBS.jsonl
  census   --jobs JOBS.jsonl --source SRC.json --exe EXE --out DIR --shards K [--win] [--families 2] [--nodes 5000]
  ladder   --jobs JOBS.jsonl --source SRC.json --exe EXE --root DIR [--shards 1,4,12] [--win]
           the census pass at each shard count on the same jobs, with the score check on; identical sorted rows
           required; the fastest parity-clean shard count is selected
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(REPO / "python" / "tools"), str(REPO / "tools" / "spy_reset")]
import nine_deck_public_panel_v1 as npp  # noqa: E402
import panel  # noqa: E402

RECEIPT = "census-20261110/qualification-v1"
BELOW_NORMAL = getattr(subprocess, "BELOW_NORMAL_PRIORITY_CLASS", 0)


def sha(path) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run_one(executable: str, request: Path) -> None:
    command = json.loads(request.read_text(encoding="utf-8"))
    Path(command["output_directory"]).parent.mkdir(parents=True, exist_ok=True)
    with request.with_suffix(".log").open("w") as log:
        code = subprocess.run([executable, str(request)], stdout=log, stderr=subprocess.STDOUT,
                              creationflags=BELOW_NORMAL, env=dict(os.environ, CUDA_VISIBLE_DEVICES="")).returncode
    if code != 0:
        raise RuntimeError(f"evaluator failed on {request}; see {request.with_suffix('.log')}")


def run_all(executable: str, requests: list[Path], workers: int) -> float:
    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=workers) as pool:
        list(pool.map(lambda r: run_one(executable, r), requests))
    return time.monotonic() - started


def qualify(plan_path: Path, executable: str, workers: list[int], sample: int, root: Path) -> dict:
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    first = json.loads(npp.checked(plan["requests"][0]["request"]).read_text(encoding="utf-8"))
    matches = first["matches"][:sample]
    trials, golden = [], None
    for k in workers:
        if sample % k:
            raise SystemExit("sample must split evenly across workers")
        paths = []
        for index in range(k):
            request = dict(first, matches=matches[index * sample // k:(index + 1) * sample // k],
                           capture_decisions=True,
                           output_directory=str(root / f"w{k}" / f"native-{index}").replace("\\", "/"))
            path = root / f"w{k}" / f"request-{index}.json"
            npp.write_json(path, request)
            paths.append(path)
        seconds = run_all(executable, paths, k)
        hashes = [h for p in paths for h in npp.match_hashes(p)]
        golden = golden or hashes
        trials.append({"workers": k, "matches": len(hashes), "parity": hashes == golden, "seconds": seconds,
                       "matches_per_second": len(hashes) / seconds})
    receipt = {"schema": RECEIPT, "evaluator_sha256": sha(executable),
               "candidate_layout": npp.candidate_layout(plan["candidate"]),
               "opponent": plan["opponent"], "sample": sample, "trials": trials, "golden_match_sha256": golden,
               "selected_workers": max((t for t in trials if t["parity"]),
                                       key=lambda t: t["matches_per_second"])["workers"]}
    npp.write_json(root / "receipt.json", receipt)
    return receipt


def play(plan_path: Path, executable: str, receipt_path: Path) -> dict:
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    if (receipt["schema"] != RECEIPT or receipt["evaluator_sha256"] != sha(executable)
            or receipt["candidate_layout"] != npp.candidate_layout(plan["candidate"])
            or receipt["opponent"] != plan["opponent"]
            or receipt["trials"][0]["workers"] != 1 or not all(t["parity"] for t in receipt["trials"])):
        raise SystemExit("the qualification receipt does not cover this evaluator, candidate layout and opponent")
    pending = []
    for item in plan["requests"]:
        path = npp.checked(item["request"])
        output = Path(json.loads(path.read_text(encoding="utf-8"))["output_directory"])
        if (output / "completion.json").exists():
            continue
        if output.exists():  # a partial output from an interrupted run is set aside
            output.rename(output.with_name(f"{output.name}.partial-{int(time.time())}"))
        pending.append(path)
    seconds = run_all(executable, pending, receipt["selected_workers"])
    result = {"plan": npp.pin(plan_path), "receipt": npp.pin(receipt_path), "workers": receipt["selected_workers"],
              "requests_run": len(pending), "seconds": seconds, "evaluator_sha256": sha(executable)}
    npp.write_json(plan_path.parent / f"play-{int(time.time())}.json", result)
    return result


def family(line: str) -> int:
    return int(json.loads(line)["id"].rsplit("-k", 1)[1].split("-")[0])


def census(jobs: Path, source: Path, exe: str, out: Path, shards: int, win: bool, families: int, nodes: int,
           tag: str | None = None, check: bool = False) -> dict:
    """One census pass over `jobs` split across `shards` processes. Refuses (after writing its record) when a shard
    exits nonzero or its game and error rows do not cover its jobs."""
    lines = [x for x in jobs.read_text(encoding="utf-8").splitlines() if x.strip()]
    if win:  # only the (a) sample's seed families are searched
        lines = [x for x in lines if family(x) < families]
    out.mkdir(parents=True, exist_ok=True)
    tag = tag or ("win" if win else "bcd")
    for stale in (out / f"census-{tag}.json", out / f"census-{tag}.failed.json"):
        stale.unlink(missing_ok=True)
    parts = []
    for k in range(shards):
        part = lines[k::shards]
        if not part:
            continue
        path = out / f"jobs-{tag}-{k:02d}.jsonl"
        path.write_text("".join(x + "\n" for x in part), encoding="utf-8")
        parts.append((k, path, len(part)))

    def one(item):
        k, path, count = item
        env = dict(os.environ, LABEL_JOBS=str(path), CENSUS_SOURCE=str(source),
                   CENSUS_CASES=str(out / f"cases-{tag}-{k:02d}.jsonl"), CENSUS_WIN="1" if win else "0",
                   CENSUS_WIN_FAMILIES=str(families), WIN_NODES=str(nodes), WIN_DEPTH="400",
                   CENSUS_TIMING=str(out / f"timing-{tag}-{k:02d}.jsonl"), CENSUS_SCORE_CHECK="1" if check else "0",
                   CUDA_VISIBLE_DEVICES="")
        games = out / f"games-{tag}-{k:02d}.jsonl"
        with (out / f"census-{tag}-{k:02d}.log").open("w") as log:
            code = subprocess.run([exe, "mode=s4a-blunder-census", f"out={games}"],
                                  stdout=log, stderr=subprocess.STDOUT, env=env, creationflags=BELOW_NORMAL).returncode
        rows = [json.loads(x)["kind"] for x in games.read_text(encoding="utf-8").splitlines()] if games.exists() else []
        covered = sum(1 for x in rows if x in ("game", "error"))
        return {"shard": k, "exit": code, "jobs": count, "games": rows.count("game"), "errors": rows.count("error"),
                "complete": code == 0 and covered == count}

    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=shards) as pool:
        results = list(pool.map(one, parts))
    record = {"jobs": npp.pin(jobs), "source": npp.pin(source), "exe_sha256": sha(exe), "win": win,
              "families": families, "nodes": nodes, "expected_jobs": len(lines), "shards": results,
              "complete": all(r["complete"] for r in results), "seconds": time.monotonic() - started}
    npp.write_json(out / f"census-{tag}.json" if record["complete"] else out / f"census-{tag}.failed.json", record)
    if not record["complete"]:
        raise SystemExit(f"census pass {tag} incomplete: {[r for r in results if not r['complete']]}")
    return record


def sorted_rows(out: Path, prefix: str) -> list[str]:
    return sorted(x for p in out.glob(f"{prefix}-*.jsonl") for x in p.read_text(encoding="utf-8").splitlines()
                  if json.loads(x)["kind"] != "header")


def ladder(jobs: Path, source: Path, exe: str, root: Path, shards: list[int], win: bool, families: int,
           nodes: int) -> dict:
    """The census pass's throughput qualification: the same jobs at each shard count; the game and case rows (as
    sorted sets of lines) must be identical; the fastest parity-clean count is selected."""
    trials, golden = [], None
    for k in shards:
        out = root / f"s{k}"
        r = census(jobs, source, exe, out, k, win, families, nodes, tag="ladder", check=True)
        rows = (sorted_rows(out, "games-ladder"), sorted_rows(out, "cases-ladder"))
        golden = golden or rows
        games = r["expected_jobs"]
        trials.append({"shards": k, "jobs": games, "parity": rows == golden, "seconds": r["seconds"],
                       "jobs_per_second": games / r["seconds"],
                       "score_mismatches": sum(json.loads(x).get("score_mismatches", 0) for x in rows[0]),
                       "score_checks": sum(json.loads(x).get("score_checks", 0) for x in rows[0])})
    clean = [t for t in trials if t["parity"] and t["score_mismatches"] == 0 and t["score_checks"] > 0]
    if not clean:
        npp.write_json(root / "receipt.failed.json", {"trials": trials})
        raise SystemExit("no shard count is parity-clean with matching score checks")
    receipt = {"schema": "census-20261110/census-ladder-v1", "exe_sha256": sha(exe), "jobs": npp.pin(jobs),
               "win": win, "trials": trials,
               "selected_shards": max(clean, key=lambda t: t["jobs_per_second"])["shards"]}
    npp.write_json(root / "receipt.json", receipt)
    return receipt


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    b = sub.add_parser("build")
    b.add_argument("--id", required=True)
    for name in ("--candidate", "--opponent", "--decks", "--out"):
        b.add_argument(name, type=Path, required=True)
    b.add_argument("--namespace", required=True)
    b.add_argument("--shards", type=int, default=16)
    q = sub.add_parser("qualify")
    q.add_argument("--plan", type=Path, required=True)
    q.add_argument("--evaluator", required=True)
    q.add_argument("--workers", default="1,2,4")
    q.add_argument("--sample", type=int, default=16)
    q.add_argument("--root", type=Path, required=True)
    p = sub.add_parser("play")
    p.add_argument("--plan", type=Path, required=True)
    p.add_argument("--evaluator", required=True)
    p.add_argument("--receipt", type=Path, required=True)
    j = sub.add_parser("jobs")
    j.add_argument("--plan", type=Path, required=True)
    j.add_argument("--out", type=Path, required=True)
    c = sub.add_parser("census")
    c.add_argument("--jobs", type=Path, required=True)
    c.add_argument("--source", type=Path, required=True)
    c.add_argument("--exe", required=True)
    c.add_argument("--out", type=Path, required=True)
    c.add_argument("--shards", type=int, required=True)
    c.add_argument("--win", action="store_true")
    c.add_argument("--families", type=int, default=2)
    c.add_argument("--nodes", type=int, default=5000)
    lad = sub.add_parser("ladder")
    lad.add_argument("--jobs", type=Path, required=True)
    lad.add_argument("--source", type=Path, required=True)
    lad.add_argument("--exe", required=True)
    lad.add_argument("--root", type=Path, required=True)
    lad.add_argument("--shards", default="1,4,12")
    lad.add_argument("--win", action="store_true")
    lad.add_argument("--families", type=int, default=2)
    lad.add_argument("--nodes", type=int, default=5000)
    a = ap.parse_args(argv)
    if a.op == "build":
        plan = panel.build(a.id, json.loads(a.candidate.read_text()), "b512", json.loads(a.opponent.read_text()),
                           json.loads(a.decks.read_text()), a.out, a.shards, namespace=a.namespace,
                           capture_all=True)
        print(json.dumps({"requests": len(plan["requests"]), "cases": plan["cases"]}))
    elif a.op == "qualify":
        r = qualify(a.plan, a.evaluator, [int(x) for x in a.workers.split(",")], a.sample, a.root)
        print(json.dumps({k: r[k] for k in ("trials", "selected_workers")}))
    elif a.op == "play":
        print(json.dumps(play(a.plan, a.evaluator, a.receipt)))
    elif a.op == "jobs":
        jobs = panel.label_jobs(a.plan)
        a.out.write_text("".join(json.dumps(x, separators=(",", ":")) + "\n" for x in jobs), encoding="utf-8")
        print(json.dumps({"jobs": len(jobs)}))
    elif a.op == "ladder":
        r = ladder(a.jobs, a.source, a.exe, a.root, [int(x) for x in a.shards.split(",")], a.win, a.families,
                   a.nodes)
        print(json.dumps({k: r[k] for k in ("trials", "selected_shards")}))
    else:
        r = census(a.jobs, a.source, a.exe, a.out, a.shards, a.win, a.families, a.nodes)
        print(json.dumps({"seconds": r["seconds"], "shards": r["shards"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
