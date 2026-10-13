"""Learned search scheduler, track A of collab LANES/discovery-efficiency-plan-20261012
(RUNNER.md sections 5-7): frozen schedules, the controller update and the training driver.

Schedules are pure functions of the namespace, lineage and case fields: no model is loaded
and no outcome is read. Seeds are the first eight little-endian SHA-256 bytes of the compact
JSON key, masked to 63 bits (the ``pilot.py`` helper). Every case has separate actual-play
(game chance, live policy), simulation (search root) and controller-sampling domains.

The controller is linear-softmax over the nominal actions [0, 16000, 64000, 256000,
remaining]; positive amounts clip to the remaining allowance and equal executable amounts
coalesce. Behavior is mu = 0.8 pi + 0.2 q. One update per batch of six whole games at frozen
weights: mean over games of (sum of grad log mu over the game's scheduler decisions) times
(terminal focal reward - baseline), SGD learning rate 0.01, global gradient-norm clip 1. The
baseline is an EMA (coefficient 0.1, initially 0) of earlier batches' mean return, fixed
within a batch. Plain Python floats in a fixed order: replaying the same ordered records gives
byte-identical weights.

CLI (JSON summaries on stdout):
  schedule --out DIR [--exclude-seeds FILE ...]   training, held-out and qualification cases
  init --out W0.json                              initial weights (pi = q)
  update --weights Wk.json --rows R1 ... R6 --baseline B --update K --out Wk+1.json
  replay --ledger TRAIN-LEDGER.jsonl              recompute every update from the ledger
  train PLAN.json                                 synchronous six-game batches (resumable)
  queue PLAN.json                                 held-out or qualification arm-games
"""
from __future__ import annotations

import argparse
import ctypes
import hashlib
import itertools
import json
import math
import os
import subprocess
import sys
import time
from pathlib import Path

NS = "discovery-efficiency-v1"
LINEAGES = ("r1", "r2")
SPY = 4
DECKS = ("Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates", "Faeries")
MASK = (1 << 63) - 1
ARMS = ("current", "random", "learned")

CAP = 512_000
NOMINAL = (0, 16_000, 64_000, 256_000, None)  # None = remaining
ACTIONS = ("0", "16000", "64000", "256000", "remaining")
Q = (0.95, 0.0125, 0.0125, 0.0125, 0.0125)
EPSILON = 0.2
LR = 0.01
CLIP = 1.0
BASELINE_COEF = 0.1
BATCH_GAMES = 6
UNITS_PER_BATCH = 3
UPDATES = 24
WEIGHTS_SCHEMA = "discovery-efficiency-v1/allocator-weights"
FEATURE_SCHEMA = "discovery-efficiency-v1/allocator-features-v1"
# Pinned order; equals allocator.rs FEATURES (checked by the tests).
FEATURES = (
    "intercept", "remaining_over_cap", "focal_nonforced_substeps_clip256",
    "substeps_since_search_clip64", "searches_clip32", "lookup_miss", "lookup_unsupported",
    "log2_options_plus1_clip8", "policy_entropy_over_log_options", "policy_top2_gap",
    "self_life_clamp_m20_80", "self_battlefield_clip64", "self_graveyard_clip64",
    "self_hand_clip32", "self_library_clip128", "opp_life_clamp_m20_80",
    "opp_battlefield_clip64", "opp_graveyard_clip64", "opp_hand_clip32",
    "opp_library_clip128", "stack_clip64",
)
NA, NF = len(ACTIONS), len(FEATURES)
REWARD = {"win": 1, "draw": 0, "loss": -1}
WORKLOAD = "discovery-efficiency-v1/s4a-allocator"


# ---------------------------------------------------------------- seeds and schedules

def key(parts) -> bytes:
    return json.dumps(list(parts), separators=(",", ":")).encode()


def h(*parts) -> bytes:
    return hashlib.sha256(key(parts)).digest()


def seed(*parts) -> int:
    return int.from_bytes(h(*parts)[:8], "little") & MASK


def case(domain: str, lineage: str, own: int, other: int, seat: int, family: int, opp: str,
         starting_player: int, arm: str) -> dict:
    """One allocator case line (``s4a-allocator`` ALLOC_CASES). Seeds never depend on the arm."""
    cid = f"{domain}-{lineage}-o{own}-v{other}-f{family}-s{seat}"
    k = (NS, domain, lineage, own, other, family, seat)
    src = {"id": cid, "own": own, "other": other, "seat": seat, "opp": opp,
           "seed": seed(*k, "game-chance"), "starting_player": starting_player,
           "source_policy_seeds": [0, 0], "anchor_draw": 0,
           "guide_policy_seeds": [seed(*k, "live-policy/0"), seed(*k, "live-policy/1")],
           "search_root": f"{NS}/{domain}/{cid}"}
    return {"id": f"{cid}-{arm}", "arm": arm, "controller_seed": seed(*k, "controller-sampling"),
            "case": src}


def training(lineages=LINEAGES) -> tuple[dict, list]:
    """Per-lineage case lists (72 each) and the 24 batches of (lineage, index) refs.

    Two distinct opposing decks per (lineage, own deck), drawn by hash without replacement;
    one plays T1 and the other A48 by hash. Two seed families, both seats. The 72 seat-pair
    units are hash-shuffled together and grouped in consecutive triples.
    """
    files = {lin: [] for lin in lineages}
    units = []
    for lin in lineages:
        for own in range(9):
            others = sorted(range(9), key=lambda o: h(NS, "train-opponent", lin, own, o))[:2]
            t1_first = h(NS, "train-policy", lin, own)[0] % 2 == 0
            policy = {others[0]: "T1" if t1_first else "A48", others[1]: "A48" if t1_first else "T1"}
            for other in others:
                for fam in (0, 1):
                    refs = []
                    for seat in (0, 1):
                        files[lin].append(case("train", lin, own, other, seat, fam, policy[other],
                                               (own + other + fam) % 2, "learned"))
                        refs.append([lin, len(files[lin]) - 1])
                    units.append({"unit": [lin, own, other, fam], "games": refs})
    units.sort(key=lambda u: h(NS, "train-unit-order", *u["unit"]))
    batches = [{"batch": b, "units": [u["unit"] for u in units[3 * b:3 * b + 3]],
                "games": [g for u in units[3 * b:3 * b + 3] for g in u["games"]]}
               for b in range(len(units) // UNITS_PER_BATCH)]
    return files, batches


def heldout(lineage: str) -> tuple[list, list]:
    """Held-out arm-games of one lineage (34 cases, three arms each) and its seat-pair units.

    Spy against all nine opposing decks; each other deck against one hash-selected opposing
    deck. T1/A48 by the PR163 parity rule; starting player (own + other) % 2. Each unit's
    arm order is one of the six permutations, by unit hash.
    """
    li = LINEAGES.index(lineage)
    pairs = [(SPY, other, "T1" if (other + li) % 2 == 0 else "A48") for other in range(9)]
    controls = [o for o in range(9) if o != SPY]
    order = sorted(controls, key=lambda own: h(NS, "heldout-control-policy", lineage, own))
    for own in controls:
        other = min(range(9), key=lambda o: h(NS, "heldout-opponent", lineage, own, o))
        pairs.append((own, other, "T1" if order.index(own) < 4 else "A48"))
    perms = list(itertools.permutations(ARMS))
    rows, units = [], []
    for own, other, opp in pairs:
        arms = perms[h(NS, "heldout-arm-order", lineage, own, other)[0] % 6]
        start = len(rows)
        for arm in arms:
            for seat in (0, 1):
                rows.append(case("heldout", lineage, own, other, seat, 0, opp, (own + other) % 2, arm))
        units.append({"unit": [lineage, own, other], "arm_order": list(arms),
                      "indices": list(range(start, len(rows)))})
    return rows, units


def qualification(lineage: str) -> list:
    """Spy and two hash-selected other own decks, each under current and q (random)."""
    owns = [SPY] + sorted((o for o in range(9) if o != SPY), key=lambda o: h(NS, "qual-own", lineage, o))[:2]
    rows = []
    for own in owns:
        other = min(range(9), key=lambda o: h(NS, "qual-opponent", lineage, own, o))
        seat = h(NS, "qual-seat", lineage, own)[0] % 2
        opp = "T1" if h(NS, "qual-policy", lineage, own)[0] % 2 == 0 else "A48"
        for arm in ("current", "random"):
            rows.append(case("qualification", lineage, own, other, seat, 0, opp, (own + other) % 2, arm))
    return rows


def excluded_seeds(paths) -> set:
    old = set()

    def walk(v, name=None):
        if isinstance(v, dict):
            for k, x in v.items():
                walk(x, k)
        elif isinstance(v, list):
            for x in v:
                walk(x, name)
        elif isinstance(v, int) and not isinstance(v, bool) and name in (
                "seed", "seeds", "source_policy_seeds", "guide_policy_seeds", "controller_seed"):
            old.add(v)

    for p in paths:
        text = Path(p).read_text(encoding="utf-8")
        for line in (text.splitlines() if str(p).endswith(".jsonl") else [text]):
            if line.strip():
                walk(json.loads(line))
    return old


def case_seeds(c: dict) -> list:
    return [c["case"]["seed"], *c["case"]["guide_policy_seeds"], c["controller_seed"]]


def jsonl(rows) -> bytes:
    return "".join(json.dumps(r, separators=(",", ":")) + "\n" for r in rows).encode()


def write_schedule(out: Path, exclude=()) -> dict:
    out.mkdir(parents=True, exist_ok=True)
    summary = {"namespace": NS, "decks": DECKS, "files": {}}

    def put(name, rows):
        data = jsonl(rows)
        (out / name).write_bytes(data)
        summary["files"][name] = {"sha256": hashlib.sha256(data).hexdigest(), "rows": len(rows)}

    files, batches = training()
    seeds_by_domain = {"train": [], "heldout": [], "qualification": []}
    for lin, rows in files.items():
        put(f"train-{lin}.jsonl", rows)
        seeds_by_domain["train"] += [s for c in rows for s in case_seeds(c)]
    (out / "train-batches.json").write_text(json.dumps(batches, indent=1) + "\n")
    summary["files"]["train-batches.json"] = {
        "sha256": hashlib.sha256((out / "train-batches.json").read_bytes()).hexdigest(), "rows": len(batches)}
    units = {}
    for lin in LINEAGES:
        rows, units[lin] = heldout(lin)
        put(f"heldout-{lin}.jsonl", rows)
        # Arms share a case's seeds: count each case once.
        seeds_by_domain["heldout"] += sorted({s for c in rows for s in case_seeds(c)})
        rows = qualification(lin)
        put(f"qualification-{lin}.jsonl", rows)
        seeds_by_domain["qualification"] += sorted({s for c in rows for s in case_seeds(c)})
    (out / "heldout-units.json").write_text(json.dumps(units, indent=1) + "\n")
    every = [s for v in seeds_by_domain.values() for s in v]
    old = excluded_seeds(exclude)
    summary.update({
        "training_games": sum(len(v) for v in files.values()), "training_batches": len(batches),
        "seed_values": len(every), "distinct_seed_values": len(set(every)),
        "overlap_between_domains": len(every) - len(set(every)),
        "excluded_values_checked": len(old), "overlap_with_excluded": len(set(every) & old)})
    (out / "SCHEDULE.json").write_text(json.dumps(summary, indent=1) + "\n")
    return summary


# ---------------------------------------------------------------- controller math

def executable(remaining: int) -> list:
    return [remaining if n is None else min(n, remaining) for n in NOMINAL]


def choices(remaining: int) -> list:
    """Distinct executable amounts (ascending) and their merged nominal actions."""
    ex = executable(remaining)
    return [(amount, [a for a in range(NA) if ex[a] == amount]) for amount in sorted(set(ex))]


def pi_of(w, x) -> list:
    z = [sum(wv * xv for wv, xv in zip(w[a], x)) for a in range(NA)]
    m = max(z)
    e = [math.exp(v - m) for v in z]
    s = sum(e)
    return [v / s for v in e]


def mixture(pi) -> list:
    return [(1.0 - EPSILON) * pi[a] + EPSILON * Q[a] for a in range(NA)]


def log_mu(w, x, remaining: int, amount: int) -> float:
    mu = mixture(pi_of(w, x))
    members = dict(choices(remaining))[amount]
    return math.log(sum(mu[a] for a in members))


def grad_log_mu(w, x, remaining: int, amount: int) -> list:
    """d log mu(E) / d W[j][k] = (1 - eps) pi_j ([j in E] - pi(E)) / mu(E) * x_k."""
    pi = pi_of(w, x)
    mu = mixture(pi)
    members = dict(choices(remaining))[amount]
    pi_e = sum(pi[a] for a in members)
    mu_e = sum(mu[a] for a in members)
    g = []
    for j in range(NA):
        dz = (1.0 - EPSILON) * pi[j] * ((1.0 if j in members else 0.0) - pi_e) / mu_e
        g.append([dz * xv for xv in x])
    return g


def init_weights() -> list:
    return [[math.log(Q[a])] + [0.0] * (NF - 1) for a in range(NA)]


def weights_bytes(w, update: int, baseline: float, parent: str | None) -> bytes:
    doc = {"schema": WEIGHTS_SCHEMA, "feature_schema": FEATURE_SCHEMA, "features": list(FEATURES),
           "actions": list(ACTIONS), "q": list(Q), "epsilon": EPSILON, "update": update,
           "baseline": baseline, "parent_sha256": parent, "weights": w}
    return (json.dumps(doc, separators=(",", ":")) + "\n").encode()


def read_weights(data: bytes) -> dict:
    doc = json.loads(data)
    if (doc["schema"] != WEIGHTS_SCHEMA or doc["feature_schema"] != FEATURE_SCHEMA
            or tuple(doc["features"]) != FEATURES or tuple(doc["actions"]) != ACTIONS
            or tuple(doc["q"]) != Q or doc["epsilon"] != EPSILON):
        raise ValueError("weights schema differs from the pinned controller")
    return doc


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class TechnicalNonterminal(Exception):
    """A game without a natural terminal reward: the batch pauses (never a loss)."""


def game_terms(row: dict, w, weights_sha: str, tol: float = 1e-9):
    """(reward, [(x, remaining, amount)]) of one training row, checking it against the
    batch weights: the row's logged pi and mu must equal the recomputed ones."""
    if row.get("kind") != "allocator_game":
        raise TechnicalNonterminal(f"{row.get('id')}: {row.get('kind')} {row.get('error', '')}"[:400])
    if row["arm"] != "learned" or row["controller"]["weights_sha256"] != weights_sha:
        raise ValueError(f"{row['id']}: not a learned game at these weights")
    if tuple(row["controller"]["features"]) != FEATURES or row["cap"] != CAP:
        raise ValueError(f"{row['id']}: feature schema or ceiling differs")
    g = row["game"]
    if not g.get("valid") or g.get("end") not in REWARD:
        raise TechnicalNonterminal(f"{row['id']}: {g.get('invalid_reason') or g.get('end')}")
    terms = []
    for d in g["decisions"]:
        s = d.get("scheduler")
        if not s:
            continue
        cs = choices(s["remaining"])
        if [c[0] for c in cs] != s["amounts"] or [c[1] for c in cs] != s["members"]:
            raise ValueError(f"{row['id']}: coalesced choices differ at decision {s['index']}")
        pi = pi_of(w, s["x"])
        mu = mixture(pi)
        for name, p in (("pi", pi), ("mu", mu)):
            want = [sum(p[a] for a in members) for _, members in cs]
            if any(abs(a - b) > tol for a, b in zip(want, s[name])):
                raise ValueError(f"{row['id']}: logged {name} differs from the weights at decision {s['index']}")
        terms.append((s["x"], s["remaining"], s["amount"]))
    return REWARD[g["end"]], terms


def update(weights: bytes, rows: list, baseline: float, index: int):
    """One controller update from the batch's rows in schedule order.

    Returns (new weights bytes, new baseline, record).
    """
    doc = read_weights(weights)
    w = doc["weights"]
    wsha = sha(weights)
    if len(rows) != BATCH_GAMES:
        raise ValueError(f"a batch has {BATCH_GAMES} games, not {len(rows)}")
    games = [game_terms(r, w, wsha) for r in rows]
    grad = [[0.0] * NF for _ in range(NA)]
    for reward, terms in games:
        adv = reward - baseline
        for x, remaining, amount in terms:
            g = grad_log_mu(w, x, remaining, amount)
            for j in range(NA):
                for k in range(NF):
                    grad[j][k] += adv * g[j][k] / BATCH_GAMES
    norm = math.sqrt(sum(v * v for r in grad for v in r))
    scale = CLIP / norm if norm > CLIP else 1.0
    new_w = [[w[j][k] + LR * scale * grad[j][k] for k in range(NF)] for j in range(NA)]
    mean_return = sum(r for r, _ in games) / BATCH_GAMES
    new_baseline = baseline + BASELINE_COEF * (mean_return - baseline)
    out = weights_bytes(new_w, index + 1, new_baseline, wsha)
    record = {"update": index, "weights_in_sha256": wsha, "weights_out_sha256": sha(out),
              "baseline_in": baseline, "baseline_out": new_baseline, "returns": [r for r, _ in games],
              "mean_return": mean_return, "decisions": [len(t) for _, t in games],
              "grad_norm": norm, "clipped": norm > CLIP,
              "rows": [{"id": r["id"], "primary_sha256": primary_sha(r)} for r in rows]}
    return out, new_baseline, record


def strip_timing(v):
    """The row without timing fields (``wall``, ``*_wall``, ``*_seconds``)."""
    if isinstance(v, dict):
        return {k: strip_timing(x) for k, x in v.items()
                if not (k == "wall" or k.endswith("_wall") or k.endswith("_seconds"))}
    if isinstance(v, list):
        return [strip_timing(x) for x in v]
    return v


def primary_sha(row: dict) -> str:
    return sha(json.dumps(strip_timing(row), sort_keys=True, separators=(",", ":")).encode())


def read_row(path: Path) -> dict | None:
    if not path.exists():
        return None
    lines = [l for l in path.read_text(encoding="utf-8").splitlines() if l.strip()]
    return json.loads(lines[0]) if len(lines) == 1 else None


def replay(ledger: Path) -> dict:
    """Recompute every update from W0 using the ledger's recorded row files, in order."""
    recs = [json.loads(l) for l in ledger.read_text().splitlines() if l.strip()]
    recs = [r for r in recs if r.get("kind") == "update"]
    weights, baseline, mismatches = weights_bytes(init_weights(), 0, 0.0, None), 0.0, []
    for r in recs:
        rows = [read_row(Path(p)) for p in r["row_files"]]
        weights, baseline, rec = update(weights, rows, baseline, r["update"])
        if rec["weights_out_sha256"] != r["weights_out_sha256"]:
            mismatches.append(r["update"])
    return {"updates": len(recs), "final_sha256": sha(weights), "mismatches": mismatches}


# ---------------------------------------------------------------- process driver

def cpu_seconds(child) -> float:
    if os.name != "nt":
        return 0.0
    from ctypes import wintypes
    handle = wintypes.HANDLE(int(child._handle))
    t = [wintypes.FILETIME() for _ in range(4)]
    if not ctypes.windll.kernel32.GetProcessTimes(handle, *[ctypes.byref(x) for x in t]):
        return 0.0
    return sum((x.dwHighDateTime << 32) + x.dwLowDateTime for x in t[2:]) / 1e7


def admit(plan: dict) -> Path:
    root = Path(plan["root"])
    binary = root / "bin" / plan["binary"]
    if sha(binary.read_bytes()) != plan["binary_sha256"]:
        raise SystemExit("binary hash differs from the plan")
    if plan.get("formal"):
        ev = json.loads(Path(plan["throughput_evidence"]).read_text())
        if not (ev.get("workload") == WORKLOAD and ev.get("host") == plan["host"]
                and ev.get("binary_sha256") == plan["binary_sha256"]
                and int(ev.get("qualified_parallel_slots", 0)) >= int(plan.get("slots", BATCH_GAMES))):
            raise SystemExit("no matching throughput evidence for this host, binary and slot count")
    return binary


def launch(plan: dict, binary: Path, job: dict, out: Path):
    """One single-worker ``s4a-allocator`` process for one arm-game (budget_queue.py layout)."""
    root, lin = Path(plan["root"]), job["lineage"]
    env = dict(os.environ, BANK_SPEC=str(root / "frozen" / f"bank-spec-{lin}.json"),
               OPPONENTS=f"T1={root / 'sources' / 't1-source.json'},A48={root / 'sources' / 'a48-source.json'}",
               S4A_MODEL=lin, ALLOC_CASES=str(Path(plan["schedule"]) / job["cases"]),
               BUDGET_KEY_MODE="repaired")
    env.pop("ALLOC_WEIGHTS", None)
    env.pop("ALLOC_TOTAL", None)
    if job.get("weights"):
        env["ALLOC_WEIGHTS"] = str(job["weights"])
    if plan.get("total"):
        env["ALLOC_TOTAL"] = str(plan["total"])
    log = open(out.with_suffix(".log"), "w")
    flags = (subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW) if os.name == "nt" else 0
    child = subprocess.Popen([str(binary), f"source={root / 'sources' / f'{lin}-source.json'}", f"out={out}",
                              "mode=s4a-allocator", "workers=1", f"first_game={job['index']}", "games=1"],
                             cwd=str(out.parent), env=env, stdout=log, stderr=subprocess.STDOUT,
                             creationflags=flags)
    return child, log


def run_jobs(plan: dict, binary: Path, jobs: list, slots: int, ledger: Path, phase: str) -> None:
    """Runs jobs (each {label, lineage, cases, index, weights?, out}) N at a time."""
    pending, running = list(jobs), []
    while pending or running:
        while pending and len(running) < slots:
            j = pending.pop(0)
            child, log = launch(plan, binary, j, Path(j["out"]))
            running.append({"job": j, "child": child, "log": log, "started": time.monotonic(), "cpu": 0.0})
        time.sleep(5)
        for r in running:
            r["cpu"] = max(r["cpu"], cpu_seconds(r["child"]))
        for r in [r for r in running if r["child"].poll() is not None]:
            r["child"].wait()
            r["log"].close()
            r["cpu"] = max(r["cpu"], cpu_seconds(r["child"]))
            with ledger.open("a") as f:
                f.write(json.dumps({"kind": "process", "phase": phase, "host": plan["host"],
                                    "job": r["job"]["label"], "cpu_seconds": r["cpu"],
                                    "wall_seconds": time.monotonic() - r["started"],
                                    "exit_code": r["child"].returncode,
                                    "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}) + "\n")
            running.remove(r)


def train(plan: dict) -> int:
    """Synchronous controller training. PLAN: {root, binary, binary_sha256, host, schedule
    (dir with train-*.jsonl and train-batches.json), out_dir, max_attempts (default 2),
    formal, throughput_evidence, [total]}. Resumes from out_dir/STATE.json."""
    binary = admit(plan)
    out = Path(plan["out_dir"])
    out.mkdir(parents=True, exist_ok=True)
    ledger = out / "TRAIN-LEDGER.jsonl"
    state_path = out / "STATE.json"
    batches = json.loads((Path(plan["schedule"]) / "train-batches.json").read_text())
    if state_path.exists():
        state = json.loads(state_path.read_text())
    else:
        w0 = weights_bytes(init_weights(), 0, 0.0, None)
        (out / "w00.json").write_bytes(w0)
        state = {"next_update": 0, "baseline": 0.0, "weights": "w00.json", "weights_sha256": sha(w0),
                 "attempts": {}, "status": "running"}
    while state["next_update"] < min(UPDATES, len(batches)):
        k = state["next_update"]
        wpath = out / state["weights"]
        wbytes = wpath.read_bytes()
        if sha(wbytes) != state["weights_sha256"]:
            raise SystemExit(f"{wpath} changed since it was written")
        jobs = []
        for i, (lin, index) in enumerate(batches[k]["games"]):
            label = f"t{k:02d}-{i}"
            row = read_row(out / f"{label}.jsonl")
            done = (row is not None and row.get("kind") == "allocator_game"
                    and row["controller"]["weights_sha256"] == state["weights_sha256"]
                    and row["game"].get("valid") and row["game"].get("end") in REWARD)
            if not done:
                n = state["attempts"].get(label, 0)
                if n >= plan.get("max_attempts", 2):
                    state["status"] = f"paused: {label} exhausted its retry allowance"
                    state_path.write_text(json.dumps(state, indent=1) + "\n")
                    print(json.dumps(state))
                    return 3
                if (out / f"{label}.jsonl").exists():
                    (out / f"{label}.jsonl").rename(out / f"{label}.failed{n}.jsonl")
                state["attempts"][label] = n + 1
                jobs.append({"label": label, "lineage": lin, "cases": f"train-{lin}.jsonl", "index": index,
                             "weights": str(wpath), "out": str(out / f"{label}.jsonl")})
        state_path.write_text(json.dumps(state, indent=1) + "\n")
        run_jobs(plan, binary, jobs, int(plan.get("slots", BATCH_GAMES)), ledger, "training")
        files = [out / f"t{k:02d}-{i}.jsonl" for i in range(len(batches[k]["games"]))]
        rows = [read_row(p) for p in files]
        try:
            new, baseline, rec = update(wbytes, rows, state["baseline"], k)
        except TechnicalNonterminal as e:
            state["status"] = f"paused: technical nonterminal in update {k}: {e}"
            state_path.write_text(json.dumps(state, indent=1) + "\n")
            print(json.dumps({"paused": str(e), "update": k}))
            return 3
        name = f"w{k + 1:02d}.json"
        (out / name).write_bytes(new)
        rec.update(kind="update", row_files=[str(p) for p in files], weights_out=name)
        with ledger.open("a") as f:
            f.write(json.dumps(rec) + "\n")
        state.update(next_update=k + 1, baseline=baseline, weights=name, weights_sha256=sha(new))
        state_path.write_text(json.dumps(state, indent=1) + "\n")
        print(json.dumps({"update": k, "mean_return": rec["mean_return"], "grad_norm": rec["grad_norm"],
                          "weights": name}), flush=True)
    state["status"] = "complete"
    state_path.write_text(json.dumps(state, indent=1) + "\n")
    print(json.dumps({"status": "complete", "endpoint": state["weights"], "sha256": state["weights_sha256"]}))
    return 0


def queue(plan: dict) -> int:
    """Held-out or qualification arm-games. PLAN: {root, binary, binary_sha256, host, phase,
    schedule, out_dir, slots, weights (endpoint, for learned jobs), formal,
    throughput_evidence, jobs: [{label, lineage, cases, index}]}. Complete rows are skipped."""
    binary = admit(plan)
    out = Path(plan["out_dir"])
    out.mkdir(parents=True, exist_ok=True)
    jobs = []
    for j in plan["jobs"]:
        row = read_row(out / f"{j['label']}.jsonl")
        if row is not None and row.get("kind") == "allocator_game":
            continue
        line = (Path(plan["schedule"]) / j["cases"]).read_text().splitlines()[j["index"]]
        arm = json.loads(line)["arm"]
        jobs.append(dict(j, out=str(out / f"{j['label']}.jsonl"),
                         weights=plan["weights"] if arm == "learned" else None))
    print(json.dumps({"pending": len(jobs), "host": plan["host"], "slots": plan["slots"]}), flush=True)
    run_jobs(plan, binary, jobs, int(plan["slots"]), out / "LEDGER.jsonl", plan["phase"])
    return 0


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("schedule")
    s.add_argument("--out", type=Path, required=True)
    s.add_argument("--exclude-seeds", nargs="*", default=[])
    s = sub.add_parser("init")
    s.add_argument("--out", type=Path, required=True)
    s = sub.add_parser("update")
    s.add_argument("--weights", type=Path, required=True)
    s.add_argument("--rows", type=Path, nargs="+", required=True)
    s.add_argument("--baseline", type=float, required=True)
    s.add_argument("--update", type=int, required=True)
    s.add_argument("--out", type=Path, required=True)
    s = sub.add_parser("replay")
    s.add_argument("--ledger", type=Path, required=True)
    for name in ("train", "queue"):
        sub.add_parser(name).add_argument("plan", type=Path)
    a = ap.parse_args(argv)
    if a.cmd == "schedule":
        summary = write_schedule(a.out, a.exclude_seeds)
        print(json.dumps({k: v for k, v in summary.items() if k not in ("files", "decks")}))
    elif a.cmd == "init":
        data = weights_bytes(init_weights(), 0, 0.0, None)
        a.out.write_bytes(data)
        print(json.dumps({"weights": str(a.out), "sha256": sha(data)}))
    elif a.cmd == "update":
        new, baseline, rec = update(a.weights.read_bytes(), [read_row(p) for p in a.rows], a.baseline, a.update)
        a.out.write_bytes(new)
        print(json.dumps(rec))
    elif a.cmd == "replay":
        print(json.dumps(replay(a.ledger)))
    elif a.cmd == "train":
        return train(json.loads(a.plan.read_text()))
    elif a.cmd == "queue":
        return queue(json.loads(a.plan.read_text()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
