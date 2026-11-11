"""Search probe driver (collab LANES/searchprobe-20261111, DEFINITIONS.md). Measurement only.

Builds the two state banks, runs the choosers (`regret_census_v1 mode=s4a-blunder-price`, PRICE_MODE=search), turns
their picks into forced-arm pricing cases (PRICE_MODE=price, common random numbers across every action of a state),
and reports each chooser's paired gap over the policy's most probable action.

CLI:
  bank     --census DIR --panel P --pricing-cases F --pricing-jobs F --out DIR --ordinary S [--flagged F]
  run      --mode search|price --states F --jobs F --sources F --exe EXE --out DIR --tag T --workers W
           [--choosers F] [--n N] [--n-true N] [--chunk C]
  cases    --search DIR --tag T --out F --singles F     (price cases from a search pass)
  report   --root DIR --out DIR
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
LANE = "searchprobe-20261111"


def h(text: str) -> str:
    return hashlib.sha256(f"{LANE}/{text}".encode()).hexdigest()


# ---------------------------------------------------------------- banks

def build_bank(census: Path, panel: str, pricing_cases: Path, pricing_jobs: Path, out: Path, ordinary: int,
               flagged: int | None) -> dict:
    """Bank (a): one focal decision with two or more legal actions per census game, the one with the smallest
    hash; the games with the smallest hashes. Outcome-blind: winners and flags are never read. Bank (b): the
    pricing lane's flagged cases of the panel (all, or the F with the smallest hashes)."""
    out.mkdir(parents=True, exist_ok=True)
    picks = []
    lines = {}
    with gzip.open(census / panel / "jobs.jsonl.gz", "rt", encoding="utf-8") as f:
        for line in f:
            if not line.strip():
                continue
            j = json.loads(line)
            eligible = [i for i, d in enumerate(j["decisions"])
                        if d["actor"] == j["focal"] and d["legal_action_count"] >= 2]
            if not eligible:
                continue
            i = min(eligible, key=lambda k: h(f"ordinary/{j['id']}/{k}"))
            picks.append((h(f"game/{j['id']}"), j["id"], i))
            lines[j["id"]] = line.strip()
    picks.sort()
    chosen = picks[:ordinary]
    (out / "states-ordinary.jsonl").write_text(
        "".join(json.dumps({"id": i, "class": "ordinary", "index": k}) + "\n" for _, i, k in chosen), encoding="utf-8")
    (out / "jobs-ordinary.jsonl").write_text("".join(lines[i] + "\n" for i in sorted({i for _, i, _ in chosen})),
                                             encoding="utf-8")
    cases = [json.loads(x) for x in pricing_cases.read_text(encoding="utf-8").splitlines() if x.strip()]
    seen = set()
    states = []
    for c in sorted(cases, key=lambda c: h(f"flagged/{c['id']}/{c['index']}")):
        if (c["id"], c["index"]) in seen:
            continue
        seen.add((c["id"], c["index"]))
        states.append({"id": c["id"], "class": f"flagged-{c['class']}", "index": c["index"]})
    if flagged is not None:
        states = states[:flagged]
    (out / "states-flagged.jsonl").write_text("".join(json.dumps(s) + "\n" for s in states), encoding="utf-8")
    (out / "jobs-flagged.jsonl").write_bytes(pricing_jobs.read_bytes())
    return {"ordinary": len(chosen), "eligible_games": len(picks), "flagged": len(states),
            "flagged_by_class": {k: sum(1 for s in states if s["class"] == k) for k in sorted({s["class"] for s in states})}}


# ---------------------------------------------------------------- runner

def run_chunks(exe: str, states: Path, jobs: Path, out: Path, tag: str, workers: int, env_extra: dict,
               chunk: int) -> dict:
    """Runs the states in chunks of `chunk` (one process each) on `workers` concurrent processes, longest-first is
    unknown so chunks go in file order. Resumable: a chunk with a done marker is skipped. Writes run-<tag>.json when
    every chunk is complete (one row per requested state, no error row)."""
    lines = [x for x in states.read_text(encoding="utf-8").splitlines() if x.strip()]
    out.mkdir(parents=True, exist_ok=True)
    parts = []
    for k in range(0, len(lines), chunk):
        c = k // chunk
        p = out / f"states-{tag}-{c:04d}.jsonl"
        body = "".join(x + "\n" for x in lines[k:k + chunk])
        if not p.exists() or p.read_text(encoding="utf-8") != body:
            p.write_text(body, encoding="utf-8")
            (out / f"done-{tag}-{c:04d}.json").unlink(missing_ok=True)
        parts.append((c, p, len(lines[k:k + chunk])))

    def one(item):
        c, p, count = item
        done = out / f"done-{tag}-{c:04d}.json"
        if done.exists():
            return json.loads(done.read_text(encoding="utf-8"))
        rows_path = out / f"rows-{tag}-{c:04d}.jsonl"
        env = dict(os.environ, LABEL_JOBS=str(jobs), PRICE_CASES=str(p), CUDA_VISIBLE_DEVICES="", **env_extra)
        started = time.monotonic()
        with (out / f"log-{tag}-{c:04d}.txt").open("w") as log:
            code = subprocess.run([exe, "mode=s4a-blunder-price", f"out={rows_path}"], stdout=log,
                                  stderr=subprocess.STDOUT, env=env, creationflags=BELOW_NORMAL).returncode
        rows = rows_path.read_text(encoding="utf-8").splitlines() if rows_path.exists() else []
        errors = sum(1 for r in rows if json.loads(r)["kind"] == "error")
        rec = {"chunk": c, "file": rows_path.name, "states_file": p.name, "exit": code, "states": count,
               "rows": len(rows), "errors": errors, "seconds": time.monotonic() - started,
               "complete": code == 0 and len(rows) == count and errors == 0}
        if rec["complete"]:
            done.write_text(json.dumps(rec), encoding="utf-8")
        return rec

    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=workers) as pool:
        results = list(pool.map(one, parts))
    record = {"tag": tag, "states": str(states), "jobs": str(jobs),
              "exe_sha256": hashlib.sha256(Path(exe).read_bytes()).hexdigest(), "env": env_extra,
              "expected": len(lines), "workers": workers, "chunk": chunk, "chunks": results,
              "complete": all(r["complete"] for r in results), "seconds": time.monotonic() - started}
    name = f"run-{tag}.json" if record["complete"] else f"run-{tag}.failed.json"
    (out / name).write_text(json.dumps(record, indent=1), encoding="utf-8")
    if not record["complete"]:
        raise SystemExit(f"pass {tag} incomplete: {[r for r in results if not r['complete']]}")
    return record


def pass_rows(out: Path, tag: str) -> list[dict]:
    record = json.loads((out / f"run-{tag}.json").read_text(encoding="utf-8"))
    found = []
    for c in record["chunks"]:
        got = [json.loads(x) for x in (out / c["file"]).read_text(encoding="utf-8").splitlines() if x.strip()]
        asked = [json.loads(x) for x in (out / c["states_file"]).read_text(encoding="utf-8").splitlines() if x.strip()]
        if [(r["id"], r["index"], r["class"]) for r in got] != [(a["id"], a["index"], a["class"]) for a in asked]:
            raise SystemExit(f"{out} {tag}: chunk {c['chunk']} rows are not its requested states in order")
        if any(r["kind"] == "error" for r in got):
            raise SystemExit(f"{out} {tag}: error row in chunk {c['chunk']}")
        found += got
    if len(found) != record["expected"]:
        raise SystemExit(f"{out} {tag}: rows do not cover the requested states")
    return found


# ---------------------------------------------------------------- price cases

def actions_of(row: dict) -> list[int]:
    """The distinct actions to score at a state: the most probable first, then the sampled action and every
    chooser's pick in chooser order."""
    acts = [row["argmax"], row["sampled"]] + [c["pick"] for c in row["choosers"].values()]
    seen = []
    for a in acts:
        if a not in seen:
            seen.append(a)
    return seen


def build_price_cases(search_rows: list[dict], out: Path, singles: Path) -> dict:
    """States with two or more distinct actions: price cases pairing the actions two by two (an odd last action is
    paired with the most probable action again, a deterministic repeat). Every case of a state keeps the state's id
    and index, so rollout k uses the same world and policy seeds for every action (common random numbers across all
    choosers). States where every chooser agrees go to `singles` (the most probable action against itself, for its
    value only). The case's class carries the bank class and the two actions."""
    multi, single = [], []
    for r in search_rows:
        acts = actions_of(r)
        if len(acts) == 1:
            a = acts[0]
            single.append({"id": r["id"], "class": f"{r['class']}|{a}|{a}", "index": r["index"],
                           "chosen": {"kind": "rows", "rows": [a]}, "alt": {"kind": "rows", "rows": [a]},
                           "divergence": None})
            continue
        if len(acts) % 2:
            acts = acts + [acts[0]]
        for x, y in zip(acts[0::2], acts[1::2]):
            multi.append({"id": r["id"], "class": f"{r['class']}|{x}|{y}", "index": r["index"],
                          "chosen": {"kind": "rows", "rows": [x]}, "alt": {"kind": "rows", "rows": [y]},
                          "divergence": None})
    out.write_text("".join(json.dumps(c) + "\n" for c in multi), encoding="utf-8")
    singles.write_text("".join(json.dumps(c) + "\n" for c in single), encoding="utf-8")
    return {"states": len(search_rows), "multi_states": len(search_rows) - len(single), "multi_cases": len(multi),
            "single_states": len(single)}


# ---------------------------------------------------------------- report

def bootstrap(values: list[float], seed: int = 20261111, reps: int = 2000) -> list[float] | None:
    if not values:
        return None
    rng = random.Random(seed)
    n = len(values)
    means = sorted(sum(values[rng.randrange(n)] for _ in range(n)) / n for _ in range(reps))
    return [means[int(0.025 * reps)], means[int(0.975 * reps) - 1]]


def action_scores(price_rows: list[dict], mode: str) -> tuple[dict, int]:
    """(id, index) -> action -> per-rollout scores (None for a rollout without a natural outcome). A repeated action
    must reproduce its scores exactly; the count of repeats checked is returned."""
    out: dict = {}
    repeats = 0
    for r in price_rows:
        _, x, y = r["class"].rsplit("|", 2)
        state = out.setdefault((r["id"], r["index"]), {})
        for a, scores in ((int(x), r[mode]["chosen"]), (int(y), r[mode]["alt"])):
            if a in state:
                repeats += 1
                if state[a] != scores:
                    raise SystemExit(f"{r['id']}/{r['index']}: action {a} did not reproduce its scores ({mode})")
            state[a] = scores
    return out, repeats


def paired(sa: list, sb: list) -> tuple[float, int] | None:
    pairs = [(a, b) for a, b in zip(sa, sb) if a is not None and b is not None]
    if len(pairs) < max(2, len(sa) // 2):
        return None
    return sum(a - b for a, b in pairs) / len(pairs), len(pairs)


def report(root: Path, out: Path) -> dict:
    out.mkdir(parents=True, exist_ok=True)
    result = {"banks": {}}
    per_state = []
    for bank in ("ordinary", "flagged"):
        sdir = root / f"search-{bank}"
        if not (sdir / f"run-search-{bank}.json").exists():
            continue
        srows = pass_rows(sdir, f"search-{bank}")
        prices = {}
        for mode in ("worlds", "true"):
            scored, repeats = {}, 0
            for tag in (f"price-{bank}", f"single-{bank}"):
                pdir = root / tag
                if (pdir / f"run-{tag}.json").exists():
                    s, r = action_scores(pass_rows(pdir, tag), mode)
                    scored.update(s)
                    repeats += r
            prices[mode] = (scored, repeats)
        names = list(srows[0]["choosers"].keys())
        choosers = ["sampled"] + names
        bank_out = {"states": len(srows), "repeat_checks": {m: prices[m][1] for m in prices}, "choosers": {}}
        for mode in ("worlds", "true"):
            scored = prices[mode][0]
            for name in choosers:
                gaps, live, invalid, differ, fallback, decided_n = [], [], 0, 0, 0, 0
                for r in srows:
                    pick = r["sampled"] if name == "sampled" else r["choosers"][name]["pick"]
                    if name != "sampled" and r["choosers"][name]["fallback"]:
                        fallback += 1
                    st = scored.get((r["id"], r["index"]), {})
                    base = st.get(r["argmax"])
                    vals = [x for x in (base or []) if x is not None]
                    value = sum(vals) / len(vals) if vals else None
                    decided = value is not None and (value < 0.05 or value > 0.95)
                    decided_n += decided
                    if pick == r["argmax"]:
                        g = 0.0
                    else:
                        differ += 1
                        p = paired(st.get(pick, []), base or []) if base is not None and pick in st else None
                        if p is None:
                            invalid += 1
                            continue
                        g = p[0]
                    gaps.append(g)
                    if not decided and value is not None:
                        live.append(g)
                    if mode == "worlds":
                        per_state.append({"bank": bank, "chooser": name, "id": r["id"], "index": r["index"],
                                          "class": r["class"], "pick": pick, "argmax": r["argmax"], "gap": g,
                                          "baseline_value": value, "decided": decided})
                cpu = [r["choosers"][name]["cpu"] for r in srows] if name != "sampled" else [0.0]
                rec = bank_out["choosers"].setdefault(name, {})
                rec[mode] = {"states": len(gaps), "invalid": invalid, "differ_from_argmax": differ,
                             "mean_gap": sum(gaps) / len(gaps) if gaps else None, "ci": bootstrap(gaps),
                             "live_states": len(live), "live_mean_gap": sum(live) / len(live) if live else None,
                             "live_ci": bootstrap(live), "decided_states": decided_n}
                rec["fallback"] = fallback
                rec["cpu_mean"] = sum(cpu) / len(cpu)
                rec["cpu_median"] = sorted(cpu)[len(cpu) // 2]
                if name != "sampled":
                    c0 = srows[0]["choosers"][name]
                    rec["sims_mean"] = sum(r["choosers"][name]["completed"] for r in srows) / len(srows)
                    rec["incomplete"] = sum(1 for r in srows if r["choosers"][name]["incomplete"])
                    rec["extra"] = c0.get("extra")
        result["banks"][bank] = bank_out
    (out / "summary.json").write_text(json.dumps(result, indent=1), encoding="utf-8")
    (out / "per-state.jsonl").write_text("".join(json.dumps(x) + "\n" for x in per_state), encoding="utf-8")
    return result


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    b = sub.add_parser("bank")
    b.add_argument("--census", type=Path, required=True)
    b.add_argument("--panel", required=True)
    b.add_argument("--pricing-cases", type=Path, required=True)
    b.add_argument("--pricing-jobs", type=Path, required=True)
    b.add_argument("--out", type=Path, required=True)
    b.add_argument("--ordinary", type=int, required=True)
    b.add_argument("--flagged", type=int)
    r = sub.add_parser("run")
    r.add_argument("--mode", choices=("search", "price"), required=True)
    r.add_argument("--states", type=Path, required=True)
    r.add_argument("--jobs", type=Path, required=True)
    r.add_argument("--sources", type=Path, required=True)
    r.add_argument("--exe", required=True)
    r.add_argument("--out", type=Path, required=True)
    r.add_argument("--tag", required=True)
    r.add_argument("--workers", type=int, required=True)
    r.add_argument("--chunk", type=int, default=2)
    r.add_argument("--choosers", type=Path)
    r.add_argument("--n", type=int, default=48)
    r.add_argument("--n-true", type=int, default=16)
    c = sub.add_parser("cases")
    c.add_argument("--search", type=Path, required=True)
    c.add_argument("--tag", required=True)
    c.add_argument("--out", type=Path, required=True)
    c.add_argument("--singles", type=Path, required=True)
    p = sub.add_parser("report")
    p.add_argument("--root", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    if a.op == "bank":
        print(json.dumps(build_bank(a.census, a.panel, a.pricing_cases, a.pricing_jobs, a.out, a.ordinary, a.flagged)))
    elif a.op == "run":
        env = {"PRICE_MODE": a.mode, "PRICE_SOURCES": str(a.sources)}
        if a.mode == "search":
            if not a.choosers:
                raise SystemExit("--choosers is required for a search pass")
            env["PROBE_CHOOSERS"] = str(a.choosers)
        else:
            env.update({"PRICE_N": str(a.n), "PRICE_N_TRUE": str(a.n_true)})
        rec = run_chunks(a.exe, a.states, a.jobs, a.out, a.tag, a.workers, env, a.chunk)
        print(json.dumps({"seconds": rec["seconds"], "expected": rec["expected"]}))
    elif a.op == "cases":
        print(json.dumps(build_price_cases(pass_rows(a.search, a.tag), a.out, a.singles)))
    else:
        print(json.dumps(report(a.root, a.out), indent=1)[:4000])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
