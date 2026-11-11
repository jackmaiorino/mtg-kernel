"""Search probe driver (collab LANES/searchprobe-20261111, DEFINITIONS.md). Measurement only.

Builds the two state banks, runs the choosers (`regret_census_v1 mode=s4a-blunder-price`, PRICE_MODE=search), turns
their picks into forced-arm pricing cases (PRICE_MODE=price, common random numbers across every action of a state),
and reports each chooser's paired gap over the policy's most probable action.

CLI:
  bank     --census DIR --panel P --pricing-cases F --pricing-jobs F --out DIR --ordinary S [--flagged F]
  run      --mode search|price --states F --jobs F --sources F --exe EXE --out DIR --tag T --workers W
           [--choosers F] [--n N] [--n-true N] [--chunk C]
  cases    --search DIR --tag T --out F --singles F     (price cases from a search pass)
  attest   --mode search|price --out DIR --tag T --exe EXE --jobs F --sources F [--choosers F] [--n N] [--n-true N]
           --note TEXT     (provenance for a pass that predates fingerprints)
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

def file_sha(path) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def fingerprint(exe: str, jobs: Path, env_extra: dict) -> dict:
    """What a chunk's rows depend on besides its states: the executable, the label jobs and every setting, with each
    file-valued setting (sources, choosers) bound by content. The sources pin each model by sha256."""
    env = {k: ({"path": v, "sha256": file_sha(v)} if k in ("PRICE_SOURCES", "PROBE_CHOOSERS") else v)
           for k, v in sorted(env_extra.items())}
    return {"exe_sha256": file_sha(exe), "jobs_sha256": file_sha(jobs), "env": env}


def run_chunks(exe: str, states: Path, jobs: Path, out: Path, tag: str, workers: int, env_extra: dict,
               chunk: int) -> dict:
    """Runs the states in chunks of `chunk` (one process each) on `workers` concurrent processes, in file order.
    Resumable: a chunk whose done marker carries this pass's fingerprint and the chunk's states hash is skipped. A done
    marker written under any other configuration, or without a fingerprint, stops the pass before anything runs, so a
    resume never mixes configurations. Writes run-<tag>.json when every chunk is complete (one row per requested
    state, no error row)."""
    lines = [x for x in states.read_text(encoding="utf-8").splitlines() if x.strip()]
    out.mkdir(parents=True, exist_ok=True)
    fp = fingerprint(exe, jobs, env_extra)
    parts = []
    for k in range(0, len(lines), chunk):
        c = k // chunk
        p = out / f"states-{tag}-{c:04d}.jsonl"
        body = "".join(x + "\n" for x in lines[k:k + chunk])
        if not p.exists() or p.read_text(encoding="utf-8") != body:
            p.write_text(body, encoding="utf-8")
            (out / f"done-{tag}-{c:04d}.json").unlink(missing_ok=True)
        parts.append((c, p, len(lines[k:k + chunk])))
    for c, p, _ in parts:
        done = out / f"done-{tag}-{c:04d}.json"
        if done.exists():
            rec = json.loads(done.read_text(encoding="utf-8"))
            if rec.get("fingerprint") != fp or rec.get("states_sha256") != file_sha(p):
                raise SystemExit(f"{done}: written under another configuration; refusing to mix configurations "
                                 "(move the pass directory aside to rerun it)")

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
               "complete": code == 0 and len(rows) == count and errors == 0,
               "fingerprint": fp, "states_sha256": file_sha(p)}
        if rec["complete"]:
            done.write_text(json.dumps(rec), encoding="utf-8")
        return rec

    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=workers) as pool:
        results = list(pool.map(one, parts))
    record = {"tag": tag, "states": str(states), "jobs": str(jobs), "fingerprint": fp,
              "exe_sha256": fp["exe_sha256"], "env": env_extra,
              "expected": len(lines), "workers": workers, "chunk": chunk, "chunks": results,
              "complete": all(r["complete"] for r in results), "seconds": time.monotonic() - started}
    name = f"run-{tag}.json" if record["complete"] else f"run-{tag}.failed.json"
    (out / name).write_text(json.dumps(record, indent=1), encoding="utf-8")
    if not record["complete"]:
        raise SystemExit(f"pass {tag} incomplete: {[r for r in results if not r['complete']]}")
    return record


def pass_provenance(out: Path, tag: str, record: dict) -> dict:
    """The pass's fingerprint, validated. Every chunk must carry the run record's fingerprint and the hash of its
    states file, and its done marker must equal its chunk record. A pass written before fingerprints existed (no
    fingerprint in its run record) is accepted only with an attested `provenance-<tag>.json` in the pass directory:
    its fingerprint must agree with the executable hash and settings the run record itself recorded, and it must list
    the states hash of every chunk."""
    fp = record.get("fingerprint")
    legacy = fp is None
    attested = {}
    if legacy:
        prov_path = out / f"provenance-{tag}.json"
        if not prov_path.exists():
            raise SystemExit(f"{out} {tag}: no fingerprint and no attested provenance; refusing the pass")
        prov = json.loads(prov_path.read_text(encoding="utf-8"))
        fp = prov["fingerprint"]
        plain = {k: (v["path"] if isinstance(v, dict) else v) for k, v in fp["env"].items()}
        if fp["exe_sha256"] != record["exe_sha256"] or plain != record["env"] or prov["tag"] != tag:
            raise SystemExit(f"{out} {tag}: attested provenance disagrees with the run record")
        attested = prov["states_sha256"]
    for c in record["chunks"]:
        states_sha = file_sha(out / c["states_file"])
        expected = attested.get(c["states_file"]) if legacy else c.get("states_sha256")
        if (not legacy and c.get("fingerprint") != fp) or expected != states_sha:
            raise SystemExit(f"{out} {tag}: chunk {c['chunk']} provenance does not match the pass")
        done = out / f"done-{tag}-{c['chunk']:04d}.json"
        if not done.exists() or json.loads(done.read_text(encoding="utf-8")) != c:
            raise SystemExit(f"{out} {tag}: chunk {c['chunk']} has no matching done marker")
    if legacy and set(attested) != {c["states_file"] for c in record["chunks"]}:
        raise SystemExit(f"{out} {tag}: attested provenance does not list exactly the pass's chunks")
    return fp


def attest(out: Path, tag: str, exe: str, jobs: Path, env_extra: dict, note: str) -> dict:
    """Writes `provenance-<tag>.json` for a pass started before fingerprints existed: the fingerprint of the inputs
    as they are now, the states hash of every chunk file, and the evidence that the inputs did not change since the
    pass began (modification times of every input and chunk file, the earliest and latest chunk outputs), with the
    operator's note. `pass_provenance` then checks it against the pass's own run record."""
    def mtime(p):
        return time.strftime("%Y-%m-%dT%H:%M:%S%z", time.localtime(Path(p).stat().st_mtime))
    fp = fingerprint(exe, jobs, env_extra)
    states = sorted(out.glob(f"states-{tag}-*.jsonl"))
    outputs = sorted(out.glob(f"rows-{tag}-*.jsonl"), key=lambda p: p.stat().st_mtime)
    prov = {"tag": tag, "fingerprint": fp, "states_sha256": {p.name: file_sha(p) for p in states},
            "evidence": {"input_mtimes": {"exe": mtime(exe), "jobs": mtime(jobs),
                                          **{k: mtime(v) for k, v in env_extra.items()
                                             if k in ("PRICE_SOURCES", "PROBE_CHOOSERS")}},
                         "states_mtime_range": [min(mtime(p) for p in states), max(mtime(p) for p in states)],
                         "first_rows_mtime": mtime(outputs[0]) if outputs else None,
                         "last_rows_mtime": mtime(outputs[-1]) if outputs else None, "note": note}}
    (out / f"provenance-{tag}.json").write_text(json.dumps(prov, indent=1), encoding="utf-8")
    return prov


def pass_rows(out: Path, tag: str, provenance: dict | None = None) -> list[dict]:
    """The rows of a complete pass whose provenance validates (`pass_provenance`); `provenance`, when given, collects
    each pass's fingerprint by tag."""
    record = json.loads((out / f"run-{tag}.json").read_text(encoding="utf-8"))
    fp = pass_provenance(out, tag, record)
    if provenance is not None:
        provenance[tag] = fp
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
    """(id, index) -> action -> per-rollout scores. A rollout without a natural outcome (an invalid forced row, a
    truncation, a halt, the step bound) is None, and the arm's invalid and non-natural counts must account for every
    None. A repeated action must reproduce its scores exactly; the count of repeats checked is returned."""
    out: dict = {}
    repeats = 0
    for r in price_rows:
        _, x, y = r["class"].rsplit("|", 2)
        state = out.setdefault((r["id"], r["index"]), {})
        arms = r[mode]
        for side, (a, scores) in enumerate(((int(x), arms["chosen"]), (int(y), arms["alt"]))):
            bad = arms.get("invalid", [0, 0])[side] + arms.get("unnatural", [0, 0])[side]
            if sum(1 for v in scores if v is None) != bad:
                raise SystemExit(f"{r['id']}/{r['index']}: null scores do not match the invalid and non-natural counts")
            if a in state:
                repeats += 1
                if state[a] != scores:
                    raise SystemExit(f"{r['id']}/{r['index']}: action {a} did not reproduce its scores ({mode})")
            state[a] = scores
    return out, repeats


def clean(scores) -> bool:
    """The base pricing mode's validity rule for one arm: every rollout has a natural outcome."""
    return bool(scores) and all(v is not None for v in scores)


def paired(sa: list, sb: list) -> float | None:
    """Mean paired difference, or None (the state is invalid for this chooser, counted) unless both arms are clean, of
    equal length and at least two rollouts: one invalid or non-natural rollout on either side invalidates the state,
    as in the base mode's `case_gap`."""
    if not (clean(sa) and clean(sb)) or len(sa) != len(sb) or len(sa) < 2:
        return None
    return sum(a - b for a, b in zip(sa, sb)) / len(sa)


def report(root: Path, out: Path) -> dict:
    out.mkdir(parents=True, exist_ok=True)
    result = {"banks": {}, "notes": [
        "Per-chooser means are over each chooser's valid states (its `states` count; `invalid` counts the states "
        "excluded by the validity rule). Contrasts use the states valid for both choosers.",
        "A state is valid for a chooser only when every rollout of the argmax action, and of the chooser's pick when "
        "it differs, has a natural outcome (the base pricing mode's rule); an agreement counts as zero only then."]}
    per_state = []
    provenance: dict = {}
    for bank in ("ordinary", "flagged"):
        sdir = root / f"search-{bank}"
        if not (sdir / f"run-search-{bank}.json").exists():
            continue
        searched = pass_rows(sdir, f"search-{bank}", provenance)
        # A state where any chooser's search stopped early (incomplete or faulted) is excluded for every chooser and
        # counted: the fixed binary makes such a state an error row (review r1, finding 1); rows from earlier binaries
        # record the flags, so the report applies the same rule.
        failed = [r for r in searched if any(c["incomplete"] or c["faults"] for c in r["choosers"].values())]
        srows = [r for r in searched if r not in failed]
        prices = {}
        rollouts = {}
        # Resampled worlds come from the price and single passes; the true state from the price pass and the
        # single-true pass (the single pass has no true-state rollouts).
        for mode, tags in (("worlds", (f"price-{bank}", f"single-{bank}")),
                           ("true", (f"price-{bank}", f"single-true-{bank}"))):
            scored, repeats = {}, 0
            for tag in tags:
                pdir = root / tag
                if (pdir / f"run-{tag}.json").exists():
                    s, r = action_scores(pass_rows(pdir, tag, provenance), mode)
                    for key, acts in s.items():
                        scored.setdefault(key, {}).update(acts)
                    repeats += r
                    env = provenance[tag]["env"]
                    rollouts[f"{tag}/{mode}"] = int(env["PRICE_N" if mode == "worlds" else "PRICE_N_TRUE"])
            prices[mode] = (scored, repeats)
        names = list(srows[0]["choosers"].keys())
        choosers = ["sampled"] + names
        nonclean = {m: sum(1 for st in prices[m][0].values() for sc in st.values() if not clean(sc)) for m in prices}
        bank_out = {"states": len(srows), "states_searched": len(searched),
                    "failed_search_states": [{"id": r["id"], "index": r["index"], "class": r["class"],
                                              "choosers": sorted(n for n, c in r["choosers"].items()
                                                                 if c["incomplete"] or c["faults"])} for r in failed],
                    "repeat_checks": {m: prices[m][1] for m in prices},
                    "actions_with_nonnatural_rollouts": nonclean, "rollouts_per_action": rollouts, "choosers": {}}
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
                    value = sum(base) / len(base) if clean(base) else None
                    decided = value is not None and (value < 0.05 or value > 0.95)
                    decided_n += decided
                    if pick == r["argmax"]:
                        if not clean(base):
                            invalid += 1
                            continue
                        g = 0.0
                    else:
                        differ += 1
                        g = paired(st.get(pick, []), base or [])
                        if g is None:
                            invalid += 1
                            continue
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
                    rec["faulted"] = sum(1 for r in srows if r["choosers"][name]["faults"])
                    rec["extra"] = c0.get("extra")
        # Paired contrasts by state (DEFINITIONS section 4), primary worlds.
        by = {}
        for x in per_state:
            if x["bank"] == bank:
                by.setdefault(x["chooser"], {})[(x["id"], x["index"])] = x["gap"]
        contrasts = {}
        for budget in ("16k", "4k"):
            for a, b in ((f"puct4-roll-{budget}", f"ucb-roll-{budget}"), (f"puct4-vh0-{budget}", f"ucb-vh0-{budget}"),
                         (f"ucb-vh0-{budget}", f"ucb-roll-{budget}"), (f"puct4-vh0-{budget}", f"puct4-roll-{budget}"),
                         (f"puct1-roll-{budget}", f"puct4-roll-{budget}")):
                if a in by and b in by:
                    keys = sorted(set(by[a]) & set(by[b]))
                    d = [by[a][k] - by[b][k] for k in keys]
                    contrasts[f"{a} - {b}"] = {"states": len(d), "mean": sum(d) / len(d) if d else None,
                                               "ci": bootstrap(d)}
        for kind in ("ucb-roll", "puct4-roll", "puct1-roll", "ucb-vh0", "puct4-vh0"):
            a, b = f"{kind}-16k", f"{kind}-4k"
            if a in by and b in by:
                keys = sorted(set(by[a]) & set(by[b]))
                d = [by[a][k] - by[b][k] for k in keys]
                contrasts[f"{a} - {b}"] = {"states": len(d), "mean": sum(d) / len(d) if d else None, "ci": bootstrap(d)}
        bank_out["contrasts"] = contrasts
        result["banks"][bank] = bank_out
    # Every pass of the report must share one executable, one set of label jobs and one model source file.
    for key in ("exe_sha256", "jobs_sha256"):
        if len({fp[key] for fp in provenance.values()}) > 1:
            raise SystemExit(f"passes disagree on {key}: {sorted({fp[key] for fp in provenance.values()})}")
    if len({fp["env"]["PRICE_SOURCES"]["sha256"] for fp in provenance.values()}) > 1:
        raise SystemExit("passes disagree on the model sources")
    result["provenance"] = provenance
    chart(result, out / "gap-chart.png")
    (out / "summary.json").write_text(json.dumps(result, indent=1), encoding="utf-8")
    (out / "per-state.jsonl").write_text("".join(json.dumps(x) + "\n" for x in per_state), encoding="utf-8")
    return result


def chart(result: dict, path: Path) -> None:
    """Mean gap over the argmax pick with 95% intervals, per chooser, one panel per bank (primary worlds)."""
    try:
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError:
        return
    banks = [b for b in ("ordinary", "flagged") if b in result["banks"]]
    if not banks:
        return
    fig, axes = plt.subplots(1, len(banks), figsize=(6.2 * len(banks), 4.6), squeeze=False)
    for ax, bank in zip(axes[0], banks):
        ch = result["banks"][bank]["choosers"]
        names = [n for n in ch if ch[n]["worlds"]["mean_gap"] is not None]
        names.sort(key=lambda n: (n != "sampled", n.split("-")[-1] != "16k", n))
        ys = list(range(len(names)))[::-1]
        for y, n in zip(ys, names):
            m, ci = ch[n]["worlds"]["mean_gap"], ch[n]["worlds"]["ci"]
            color = "#8a8a8a" if n == "sampled" else ("#2a6fdb" if "roll" in n else "#d9822b")
            ax.plot(ci, [y, y], color=color, lw=2)
            ax.plot([m], [y], "o", color=color, ms=6)
        ax.axvline(0, color="#444", lw=0.8)
        ax.set_yticks(ys)
        ax.set_yticklabels([f"{n} (n={ch[n]['worlds']['states']})" for n in names], fontsize=9)
        ax.set_title(f"{bank} bank ({result['banks'][bank]['states']} states)", fontsize=11)
        ax.set_xlabel("win-probability gap over the policy's top action (95% CI)", fontsize=9)
        ax.grid(axis="x", color="#ddd", lw=0.6)
        for s in ("top", "right"):
            ax.spines[s].set_visible(False)
    ns = sorted({v for b in banks for k, v in result["banks"][b]["rollouts_per_action"].items()
                 if k.startswith("price-") and k.endswith("/worlds")})
    fig.text(0.01, 0.01, "blue: rollout leaf; orange: value leaf; grey: sampled action. Resampled worlds, "
             f"N = {', '.join(map(str, ns)) or '?'} rollouts per action, common random numbers. "
             "n = the chooser's valid states.", fontsize=8, color="#555")
    fig.tight_layout(rect=(0, 0.04, 1, 1))
    fig.savefig(path, dpi=140)
    plt.close(fig)


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
    t = sub.add_parser("attest")
    t.add_argument("--mode", choices=("search", "price"), required=True)
    t.add_argument("--out", type=Path, required=True)
    t.add_argument("--tag", required=True)
    t.add_argument("--exe", required=True)
    t.add_argument("--jobs", type=Path, required=True)
    t.add_argument("--sources", type=Path, required=True)
    t.add_argument("--choosers", type=Path)
    t.add_argument("--n", type=int, default=48)
    t.add_argument("--n-true", type=int, default=16)
    t.add_argument("--note", required=True)
    p = sub.add_parser("report")
    p.add_argument("--root", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    if a.op == "bank":
        print(json.dumps(build_bank(a.census, a.panel, a.pricing_cases, a.pricing_jobs, a.out, a.ordinary, a.flagged)))
    elif a.op in ("run", "attest"):
        env = {"PRICE_MODE": a.mode, "PRICE_SOURCES": str(a.sources)}
        if a.mode == "search":
            if not a.choosers:
                raise SystemExit("--choosers is required for a search pass")
            env["PROBE_CHOOSERS"] = str(a.choosers)
        else:
            env.update({"PRICE_N": str(a.n), "PRICE_N_TRUE": str(a.n_true)})
        if a.op == "attest":
            print(json.dumps(attest(a.out, a.tag, a.exe, a.jobs, env, a.note), indent=1))
            return 0
        rec = run_chunks(a.exe, a.states, a.jobs, a.out, a.tag, a.workers, env, a.chunk)
        print(json.dumps({"seconds": rec["seconds"], "expected": rec["expected"]}))
    elif a.op == "cases":
        print(json.dumps(build_price_cases(pass_rows(a.search, a.tag), a.out, a.singles)))
    else:
        print(json.dumps(report(a.root, a.out), indent=1)[:4000])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
