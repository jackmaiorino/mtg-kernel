"""Allocator v2 (collab LANES/allocator-redesign-20261012, DESIGN.md): frozen Spy schedules,
the per-parent actor-critic controller, its PPO update and the synchronous training driver.

Schedules are pure functions of the namespace and case fields: no model is loaded and no
outcome is read. Seeds are the first eight little-endian SHA-256 bytes of the compact JSON
key, masked to 63 bits (the ``allocator.py`` helper), in the new namespace
``allocator-redesign-v1``. Every case has separate actual-play (game chance, live policy),
simulation (search root) and controller-sampling domains; seeds never depend on the arm, so
arms of a case are paired.

Controller (per arm S or H and per frozen playing parent): input -> 64 tanh hidden -> actor
logits over the nominal amounts [0, 32000, 128000, 512000] and a separate scalar critic.
pi = softmax(log q + actor(x)) with the dynamic base q = [1-p, 0.5p, 0.4p, 0.1p], p = 0.5 r
(r the remaining fraction); amounts clip to the remaining allowance and equal executable
amounts coalesce by summing probabilities. Behavior mu = 0.8 pi + 0.2 q on the coalesced set.
Xavier-uniform hidden and critic weights from a pinned seed, zero biases, zero actor output
weights and bias (so pi = q at initialization).

PPO update (one per parent per 36-game batch; the parent's 18 games in schedule order):
advantages A = terminal focal return (+1/0/-1, gamma 1) minus the collection-time critic,
frozen for the batch. Four epochs of three whole-game minibatches of six (a pinned hashed
permutation per update and epoch). Per decision the ratio is rho = mu_theta(c) / mu_b(c),
where mu_b(c) is the logged behavior probability of the chosen coalesced amount and
mu_theta(c) = 0.8 pi_theta(c) + 0.2 q(c) on the same coalesced set: the clipped surrogate
min(rho A, clip(rho, 0.8, 1.2) A). Actor loss: minus the sum over a game's decisions, averaged
over the minibatch's games (zero-decision games stay in the denominator). Critic loss: squared
error (V - R)^2 averaged within each game, then over the minibatch's games; coefficient 0.5.
No entropy bonus. Global gradient-norm cap 1, then Adam (betas 0.9/0.999, eps 1e-8; learning
rate LR = 3e-3 after the synthetic calibration, the design default 3e-4 being negligible).
After each epoch, the mean over the batch's decisions of the exact executable-behavior
KL(mu_b || mu_theta) stops further epochs when above 0.02. Arithmetic is numpy float64
without BLAS (elementwise products and pairwise sums in a fixed order): replaying the same
ordered rows reproduces the weights and Adam state byte for byte.

CLI (JSON summaries on stdout):
  schedule --out DIR [--exclude-seeds FILE ...]
  freeze-norm --rows FILE ... --lineage L --arm S|H --out NORM.json
  init --arm S|H --parent-row ROW.jsonl --lineage L [--normalization NORM.json] --label X --out W0.json
  update --weights W --rows R1 ... R18 --update K --out-weights W' --out-adam A' [--adam A]
  replay --ledger TRAIN-LEDGER.jsonl
  train PLAN.json        synchronous 36-game batches (resumable)
  queue PLAN.json        validation, comparison, qualification or smoke arm-games
"""
from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import math
import os
import subprocess
import time
from pathlib import Path

import numpy as np

NS = "allocator-redesign-v1"
LINEAGES = ("r1", "r2")
SPY = 4
DECKS = ("Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates", "Faeries")
MASK = (1 << 63) - 1
ARMS = ("current", "q", "S", "H")
LEARNED = ("S", "H")

CAP = 2_048_000
NOMINAL = (0, 32_000, 128_000, 512_000)
ACTIONS = ("0", "32000", "128000", "512000")
POSITIVE_AT_FULL = 0.5
POSITIVE_SPLIT = (0.5, 0.4, 0.1)
EPSILON = 0.2
HIDDEN = 64
REP = 64
NA = len(ACTIONS)
WEIGHTS_SCHEMA = "allocator-redesign-v1/controller"
ADAM_SCHEMA = "allocator-redesign-v1/adam"
FEATURE_SCHEMA = "allocator-redesign-v1/features-s1"
POOLING_SCHEMA = "net8-pooled-representation-v1/state_hidden+scorer_hidden_mean+scorer_hidden_max"
WORKLOAD = "allocator-redesign-v1/s4a-allocator-v2"
# Pinned order; equals allocator2.rs S_FEATURES (checked by the tests).
S_FEATURES = (
    "cap_over_2048000", "remaining_over_2048000", "remaining_fraction", "used_over_2048000",
    "log_focal_physical_256", "log_focal_substeps_512", "substep_index_clip8",
    "log_natural_simulations_65536", "searches_clip32", "never_searched",
    "substeps_since_search_clip64", "physical_since_search_clip32", "lookup_no_tree",
    "lookup_history_miss", "lookup_matched_unsupported", "lookup_depth_limit",
    "log2_options_plus1_clip8", "policy_entropy_over_log_options", "policy_top1",
    "policy_top2_gap", "policy_value_tanh", "turn_clip32", "focal_is_active_player",
    "self_life_clamp_m20_80", "self_battlefield_clip64", "self_graveyard_clip64",
    "self_hand_clip32", "self_library_clip128", "opp_life_clamp_m20_80",
    "opp_battlefield_clip64", "opp_graveyard_clip64", "opp_hand_clip32", "opp_library_clip128",
    "stack_clip64", "tree_missing", "log_tree_nodes_65536", "log_tree_root_visits_65536",
    "tree_root_supported_edges_clip8", "tree_root_best_mean", "log_tree_search_natural_65536",
    "node_missing", "node_edges_over_options", "node_tried_fraction", "log_node_visits_4096",
    "node_max_backups_over_8", "node_supported_edges_clip8", "node_best_mean", "node_mean_gap",
    "node_mean_gap_missing", "node_best_is_policy_argmax", "node_best_missing",
)
REWARD = {"win": 1, "draw": 0, "loss": -1}

# Optimizer and update constants (DESIGN.md "Learning and exploration"). DESIGN_LR is the
# design default; the synthetic calibration (calibration/ALLOCATOR2-CALIBRATION.md) found its reachable
# movement negligible even under perfect feedback (one cycle), so the development constant is
# LR, the smallest calibrated rate whose perfect-feedback movement is material.
DESIGN_LR = 3e-4
LR = 3e-3
BETAS = (0.9, 0.999)
ADAM_EPS = 1e-8
EPOCHS = 4
CLIP_RATIO = 0.2
GRAD_CAP = 1.0
CRITIC_COEF = 0.5
KL_STOP = 0.02
GAMES_PER_PARENT = 18
MINIBATCH_GAMES = 6
BATCHES_PER_CYCLE = 9
PARAMS = ("w1", "b1", "wa", "ba", "wv", "bv")


def feature_names(arm: str) -> list:
    names = list(S_FEATURES)
    if arm == "H":
        for block in ("rep_state", "rep_action_mean", "rep_action_max"):
            names += [f"{block}_{i:02d}" for i in range(REP)]
    return names


# ---------------------------------------------------------------- seeds and schedules

def key(parts) -> bytes:
    return json.dumps(list(parts), separators=(",", ":")).encode()


def h(*parts) -> bytes:
    return hashlib.sha256(key(parts)).digest()


def seed(*parts) -> int:
    return int.from_bytes(h(*parts)[:8], "little") & MASK


def opp_policy(lineage: str, other: int) -> str:
    """The inherited outcome-blind T1/A48 rule for Spy rows (PR163 parity)."""
    return "T1" if (other + LINEAGES.index(lineage)) % 2 == 0 else "A48"


def case(domain: str, lineage: str, other: int, seat: int, family: int) -> dict:
    """One arm-agnostic Spy case. ``launch`` adds the arm (seeds never depend on it)."""
    cid = f"{domain}-{lineage}-v{other}-f{family}-s{seat}"
    k = (NS, domain, lineage, SPY, other, family, seat)
    src = {"id": cid, "own": SPY, "other": other, "seat": seat, "opp": opp_policy(lineage, other),
           "seed": seed(*k, "game-chance"), "starting_player": (SPY + other + family) % 2,
           "source_policy_seeds": [0, 0], "anchor_draw": 0,
           "guide_policy_seeds": [seed(*k, "live-policy/0"), seed(*k, "live-policy/1")],
           "search_root": f"{NS}/{domain}/{cid}"}
    return {"id": cid, "lineage": lineage, "domain": domain, "family": family,
            "controller_seed": seed(*k, "controller-sampling"), "case": src}


def roster(domain: str, families) -> list:
    """Every (lineage, opponent, family, seat) case, family-major."""
    return [case(domain, lin, other, seat, fam)
            for fam in families for lin in LINEAGES for other in range(9) for seat in (0, 1)]


def training_batches(domain: str, cycles: int = 1) -> tuple[list, list]:
    """Cases and batches of one training domain: batch b holds family b (36 games, 18 per
    parent, all opponents and both seats)."""
    rows = roster(domain, range(BATCHES_PER_CYCLE * cycles))
    batches = [{"batch": b, "family": b,
                "games": {lin: [i for i, r in enumerate(rows) if r["family"] == b and r["lineage"] == lin]
                          for lin in LINEAGES}}
               for b in range(BATCHES_PER_CYCLE * cycles)]
    return rows, batches


def qualification() -> list:
    """Spy against all nine opponents for both parents (18 cases, seat by hash)."""
    out = []
    for lin in LINEAGES:
        for other in range(9):
            seat = h(NS, "qual-seat", lin, other)[0] % 2
            out.append(case("qualification", lin, other, seat, 0))
    return out


def smoke() -> list:
    """Two engineering smoke cases in their own seed domain, both on r1 (the second can then
    bind a controller to the parent identity the first row reports)."""
    others = sorted(range(9), key=lambda o: h(NS, "smoke-opponent", o))[:2]
    return [case("smoke", "r1", o, i, 0) for i, o in enumerate(others)]


DOMAINS = {
    # name: (families, cycles for training batches or None)
    "qualification": None,
    "smoke": None,
    "dev-c1": (range(9), 1),
    "dev-c2": (range(9), 1),
    "devval": (range(3), None),
    "fresh-panel": (range(6), None),
    "fresh-train-rep1": (range(18), 2),
    "fresh-train-rep2": (range(18), 2),
    "fresh-train-rep3": (range(18), 2),
}


def domain_rows(name: str):
    if name == "qualification":
        return qualification(), None
    if name == "smoke":
        return smoke(), None
    fams, cycles = DOMAINS[name]
    if cycles:
        return training_batches(name, cycles)
    return roster(name, fams), None


def excluded_seeds(paths) -> set:
    old = set()

    def walk(v, inside=False):
        # Every integer under a key naming seeds (at any depth below it) counts.
        if isinstance(v, dict):
            for k, x in v.items():
                walk(x, inside or "seed" in k)
        elif isinstance(v, list):
            for x in v:
                walk(x, inside)
        elif isinstance(v, int) and not isinstance(v, bool) and inside:
            old.add(v)

    for p in paths:
        p = Path(p)
        files = sorted(p.rglob("*.json*")) if p.is_dir() else [p]
        for f in files:
            text = f.read_text(encoding="utf-8")
            for line in (text.splitlines() if f.suffix == ".jsonl" else [text]):
                if line.strip():
                    walk(json.loads(line))
    return old


def case_seeds(c: dict) -> list:
    return [c["case"]["seed"], *c["case"]["guide_policy_seeds"], c["controller_seed"]]


def jsonl(rows) -> bytes:
    return "".join(json.dumps(r, separators=(",", ":")) + "\n" for r in rows).encode()


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def write_schedule(out: Path, exclude=()) -> dict:
    out.mkdir(parents=True, exist_ok=True)
    summary = {"namespace": NS, "decks": DECKS, "files": {}, "domains": {}}

    def put(name, data: bytes, rows):
        (out / name).write_bytes(data)
        summary["files"][name] = {"sha256": sha(data), "rows": rows}

    every = []
    for name in DOMAINS:
        rows, batches = domain_rows(name)
        put(f"cases-{name}.jsonl", jsonl(rows), len(rows))
        if batches is not None:
            put(f"batches-{name}.json", (json.dumps(batches, indent=1) + "\n").encode(), len(batches))
        every += [s for c in rows for s in case_seeds(c)]
        summary["domains"][name] = {"cases": len(rows), "batches": None if batches is None else len(batches)}
    old = excluded_seeds(exclude)
    summary.update({"seed_values": len(every), "distinct_seed_values": len(set(every)),
                    "overlap_between_cases": len(every) - len(set(every)),
                    "excluded_values_checked": len(old), "overlap_with_excluded": len(set(every) & old),
                    "excluded_sources": [str(p) for p in exclude]})
    (out / "SCHEDULE.json").write_text(json.dumps(summary, indent=1) + "\n")
    return summary


def probe_cases(sources: list, refresh: int = 128_000, paired: int = 4) -> list:
    """Supported-node refresh probe lines (prepared, not run). ``sources`` lists
    {row (an allocator2_game row), at (focal nonforced index of a supported decision)} chosen
    outcome-blindly by the caller. Each gets execute and refresh arms on ``paired`` shared
    continuation seeds; the replay reuses the source's arm, case, controller seed and weights."""
    out = []
    for src in sources:
        row, at = src["row"], int(src["at"])
        for j in range(paired):
            cont = [seed(NS, "probe", row["id"], at, j, "continuation/0"),
                    seed(NS, "probe", row["id"], at, j, "continuation/1")]
            for label, amount in (("execute", 0), ("refresh", refresh)):
                line = {"id": f"{row['id']}-probe{at}-c{j}-{label}", "lineage": row["lineage"], "arm": row["arm"],
                        "controller_seed": row["controller_seed"],
                        "case": {"id": row["id"].rsplit("-", 1)[0], "own": row["own"], "other": row["other"],
                                 "seat": row["seat"], "opp": row["opp"], "seed": row["seed"],
                                 "starting_player": row["starting_player"], "source_policy_seeds": [0, 0],
                                 "anchor_draw": 0, "guide_policy_seeds": row["live_policy_seeds"],
                                 "search_root": row["search_root"]},
                        "probe": {"source": row["id"], "at": at, "refresh": amount, "continuation_seeds": cont},
                        "weights": row["controller"].get("weights_path")}
                out.append(line)
    return out


def probe_select(rows: list, limit: int = 36) -> list:
    """Outcome-blind probe roots: at most one supported (tree-executed) focal decision per
    distinct Spy source game with a scheduler arm, balanced round-robin over (parent,
    opponent) strata in hash order; within a game the supported decision of least hash. Reads
    only identities and choosers, never outcomes or sequence labels. Absent strata stay absent."""
    strata = {}
    for r in rows:
        if r is None or r.get("kind") != "allocator2_game" or r["arm"] not in ("q", "S", "H"):
            continue
        sup = [d["nonforced"] for d in r["game"].get("decisions", [])
               if d.get("chooser") in ("reused", "new_root") and d.get("nonforced") is not None]
        if not sup:
            continue
        at = min(sup, key=lambda i: h(NS, "probe-root", r["id"], i))
        strata.setdefault((r["lineage"], r["other"]), []).append((h(NS, "probe-game", r["id"]), r, at))
    for v in strata.values():
        v.sort(key=lambda x: x[0])
    order = sorted(strata, key=lambda k: h(NS, "probe-stratum", *k))
    picked = []
    while len(picked) < limit and any(strata[k] for k in order):
        for k in order:
            if strata[k] and len(picked) < limit:
                _, r, at = strata[k].pop(0)
                picked.append({"row": r, "at": at})
    return picked


# ---------------------------------------------------------------- controller math

def base_q(remaining: int, cap: int) -> np.ndarray:
    r = min(max(remaining / max(cap, 1), 0.0), 1.0)
    p = POSITIVE_AT_FULL * r
    return np.array([1.0 - p, POSITIVE_SPLIT[0] * p, POSITIVE_SPLIT[1] * p, POSITIVE_SPLIT[2] * p])


def choices(remaining: int) -> list:
    """Distinct executable amounts (ascending) and the nominal actions each merges."""
    ex = [min(n, remaining) for n in NOMINAL]
    return [(amount, [a for a in range(NA) if ex[a] == amount]) for amount in sorted(set(ex))]


def coalesce(p, cs) -> np.ndarray:
    return np.array([sum(float(p[a]) for a in m) for _, m in cs])


def policy(q, actor) -> np.ndarray:
    z = np.log(q) + actor
    z = z - z.max()
    e = np.exp(z)
    return e / e.sum()


def forward(params: dict, norm: dict, x: np.ndarray):
    """(normalized input, hidden, actor, value)."""
    xn = (x - norm["mean"]) / norm["scale"]
    u = (params["w1"] * xn).sum(axis=1) + params["b1"]
    hid = np.tanh(u)
    actor = (params["wa"] * hid).sum(axis=1) + params["ba"]
    value = float((params["wv"] * hid).sum() + params["bv"][0])
    return xn, hid, actor, value


def xavier(rng, fan_in, fan_out, shape):
    bound = math.sqrt(6.0 / (fan_in + fan_out))
    return rng.uniform(-bound, bound, size=shape)


def init_params(nf: int, init_seed: int) -> dict:
    rng = np.random.Generator(np.random.PCG64(init_seed))
    return {"w1": xavier(rng, nf, HIDDEN, (HIDDEN, nf)), "b1": np.zeros(HIDDEN),
            "wa": np.zeros((NA, HIDDEN)), "ba": np.zeros(NA),
            "wv": xavier(rng, HIDDEN, 1, (HIDDEN,)), "bv": np.zeros(1)}


def default_norm(nf: int) -> dict:
    return {"mean": np.zeros(nf), "scale": np.ones(nf), "frozen": False, "source": "fixed scalings"}


def weights_bytes(arm: str, params: dict, norm: dict, parent: dict, update: int, init_seed: int,
                  previous: str | None) -> bytes:
    doc = {"schema": WEIGHTS_SCHEMA, "arm": arm, "feature_schema": FEATURE_SCHEMA,
           "features": feature_names(arm), "pooling_schema": POOLING_SCHEMA if arm == "H" else None,
           "forward_activation_mode": "libm_tanh", "parent": parent, "actions": list(ACTIONS),
           "positive_at_full": POSITIVE_AT_FULL, "positive_split": list(POSITIVE_SPLIT),
           "epsilon": EPSILON, "hidden": HIDDEN, "update": update, "init_seed": init_seed,
           "previous_sha256": previous,
           "normalization": {"mean": norm["mean"].tolist(), "scale": norm["scale"].tolist(),
                             "frozen": bool(norm["frozen"]), "source": norm["source"]},
           "params": {"w1": params["w1"].tolist(), "b1": params["b1"].tolist(), "wa": params["wa"].tolist(),
                      "ba": params["ba"].tolist(), "wv": params["wv"].tolist(), "bv": float(params["bv"][0])}}
    return (json.dumps(doc, separators=(",", ":")) + "\n").encode()


def read_weights(data: bytes) -> dict:
    doc = json.loads(data)
    arm = doc["arm"]
    if (doc["schema"] != WEIGHTS_SCHEMA or doc["feature_schema"] != FEATURE_SCHEMA or arm not in LEARNED
            or doc["features"] != feature_names(arm) or tuple(doc["actions"]) != ACTIONS
            or doc["epsilon"] != EPSILON or doc["hidden"] != HIDDEN
            or doc["positive_at_full"] != POSITIVE_AT_FULL or tuple(doc["positive_split"]) != POSITIVE_SPLIT
            or doc["pooling_schema"] != (POOLING_SCHEMA if arm == "H" else None)):
        raise ValueError("weights schema differs from the pinned controller")
    p = doc["params"]
    params = {"w1": np.array(p["w1"], dtype=float), "b1": np.array(p["b1"], dtype=float),
              "wa": np.array(p["wa"], dtype=float), "ba": np.array(p["ba"], dtype=float),
              "wv": np.array(p["wv"], dtype=float), "bv": np.array([float(p["bv"])])}
    n = doc["normalization"]
    norm = {"mean": np.array(n["mean"], dtype=float), "scale": np.array(n["scale"], dtype=float),
            "frozen": n["frozen"], "source": n["source"]}
    return {"doc": doc, "arm": arm, "params": params, "norm": norm, "parent": doc["parent"]}


def adam_bytes(m: dict, v: dict, t: int, weights_sha: str) -> bytes:
    doc = {"schema": ADAM_SCHEMA, "t": t, "weights_sha256": weights_sha,
           "m": {k: m[k].tolist() for k in PARAMS}, "v": {k: v[k].tolist() for k in PARAMS}}
    return (json.dumps(doc, separators=(",", ":")) + "\n").encode()


def read_adam(data: bytes | None, params: dict, weights_sha: str):
    if data is None:
        return {k: np.zeros_like(params[k]) for k in PARAMS}, {k: np.zeros_like(params[k]) for k in PARAMS}, 0
    doc = json.loads(data)
    if doc["schema"] != ADAM_SCHEMA or doc["weights_sha256"] != weights_sha:
        raise ValueError("Adam state does not belong to these weights")
    m = {k: np.array(doc["m"][k], dtype=float).reshape(params[k].shape) for k in PARAMS}
    v = {k: np.array(doc["v"][k], dtype=float).reshape(params[k].shape) for k in PARAMS}
    return m, v, int(doc["t"])


def pick(probs, u: float) -> int:
    """The first choice whose cumulative probability exceeds ``u`` (allocator.rs ``pick``)."""
    acc = 0.0
    for i, p in enumerate(probs):
        acc += float(p)
        if u < acc:
            return i
    return len(probs) - 1


def consult_record(w: dict | None, s_row, rep_row, remaining: int, cap: int, u: float, index: int) -> dict:
    """The scheduler record Rust logs at one consultation (Python mirror of
    ``allocator2::Scheduler::consult``), for synthetic collection and tests. ``w`` None is
    the q arm."""
    q = base_q(remaining, cap)
    cs = choices(remaining)
    q_c = coalesce(q, cs)
    if w is None:
        pi_c, mu_c, critic = q_c, q_c, None
    else:
        x = decision_x(w["arm"], {"s": s_row, "rep": rep_row})
        _, _, actor, critic = forward(w["params"], w["norm"], x)
        pi_c = coalesce(policy(q, actor), cs)
        mu_c = (1 - EPSILON) * pi_c + EPSILON * q_c
    i = pick(mu_c, u)
    return {"index": index, "s": list(map(float, s_row)), "rep": list(map(float, rep_row)), "lookup": "no_tree",
            "remaining": remaining, "cap": cap, "amounts": [c[0] for c in cs], "members": [c[1] for c in cs],
            "q": q_c.tolist(), "pi": pi_c.tolist(), "mu": mu_c.tolist(), "critic": critic, "u": u,
            "chosen": i, "amount": cs[i][0], "nontrivial": len(cs) >= 2}


def synthetic_row(gid: str, arm: str, lineage: str, weights: bytes, records: list, end: str) -> dict:
    """An ``allocator2_game`` row around logged scheduler records (calibration and tests only;
    never Magic training)."""
    w = read_weights(weights)
    return {"kind": "allocator2_game", "id": gid, "arm": arm, "lineage": lineage, "cap": CAP,
            "controller": {"features": list(S_FEATURES), "weights_sha256": sha(weights), "parent": w["parent"]},
            "player_after": w["parent"],
            "game": {"valid": True, "end": end, "decisions": [{"scheduler": r} for r in records]}}


class TechnicalNonterminal(Exception):
    """A game without a natural terminal reward: training pauses (never a loss)."""


def decision_x(arm: str, s: dict) -> np.ndarray:
    x = list(s["s"])
    if arm == "H":
        x += list(s["rep"])
    return np.array(x, dtype=float)


def game_terms(row: dict, w: dict, weights_sha: str, tol: float = 1e-9):
    """(reward, decisions) of one training row, checked against the batch weights: logged q,
    pi, mu and critic must equal the recomputed ones, the player must be the bound parent."""
    if row is None or row.get("kind") != "allocator2_game":
        rid = None if row is None else row.get("id")
        raise TechnicalNonterminal(f"{rid}: {None if row is None else row.get('kind')} "
                                   f"{'' if row is None else row.get('error', '')}"[:400])
    arm = w["arm"]
    if row["arm"] != arm or row["controller"]["weights_sha256"] != weights_sha:
        raise ValueError(f"{row['id']}: not a {arm} game at these weights")
    if row["controller"]["parent"] != w["parent"] or row["player_after"] != w["parent"]:
        raise ValueError(f"{row['id']}: frozen player differs from the bound parent")
    if tuple(row["controller"]["features"]) != S_FEATURES:
        raise ValueError(f"{row['id']}: feature schema differs")
    g = row["game"]
    if not g.get("valid") or g.get("end") not in REWARD:
        raise TechnicalNonterminal(f"{row['id']}: {g.get('invalid_reason') or g.get('end')}")
    decisions = []
    for d in g["decisions"]:
        s = d.get("scheduler")
        if not s or not s.get("nontrivial", True):
            continue
        cs = choices(s["remaining"])
        if [c[0] for c in cs] != s["amounts"] or [c[1] for c in cs] != s["members"]:
            raise ValueError(f"{row['id']}: coalesced choices differ at decision {s['index']}")
        q = base_q(s["remaining"], s["cap"])
        x = decision_x(arm, s)
        _, _, actor, value = forward(w["params"], w["norm"], x)
        pi_c = coalesce(policy(q, actor), cs)
        q_c = coalesce(q, cs)
        mu_c = (1 - EPSILON) * pi_c + EPSILON * q_c
        for name, want in (("q", q_c), ("pi", pi_c), ("mu", mu_c)):
            if np.any(np.abs(want - np.array(s[name])) > tol):
                raise ValueError(f"{row['id']}: logged {name} differs from the weights at decision {s['index']}")
        if abs(value - s["critic"]) > tol:
            raise ValueError(f"{row['id']}: logged critic differs at decision {s['index']}")
        decisions.append({"x": x, "q": q, "cs": cs, "chosen": s["chosen"],
                          "mu_b": np.array(s["mu"], dtype=float), "critic_b": float(s["critic"])})
    return REWARD[g["end"]], decisions


def minibatch_grad(params: dict, norm: dict, games: list):
    """Gradient of actor loss + CRITIC_COEF * critic loss over whole games (fixed order),
    and diagnostics. games: [{reward, decisions:[{x, q, cs, chosen, mu_b, adv}]}]."""
    gsize = len(games)
    grad = {k: np.zeros_like(params[k]) for k in PARAMS}
    actor_loss = critic_loss = 0.0
    clipped = active = n = 0
    for g in games:
        t = len(g["decisions"])
        for d in g["decisions"]:
            xn, hid, actor, value = forward(params, norm, d["x"])
            pi = policy(d["q"], actor)
            members = d["cs"][d["chosen"]][1]
            inside = np.zeros(NA)
            inside[members] = 1.0
            pi_e = float((pi * inside).sum())
            q_e = float((d["q"] * inside).sum())
            mu_e = (1 - EPSILON) * pi_e + EPSILON * q_e
            mu_b = float(d["mu_b"][d["chosen"]])
            rho = mu_e / mu_b
            adv = d["adv"]
            low, high = 1.0 - CLIP_RATIO, 1.0 + CLIP_RATIO
            surr = rho * adv
            surr_clip = min(max(rho, low), high) * adv
            use = surr <= surr_clip
            actor_loss -= min(surr, surr_clip) / gsize
            clipped += int(rho < low or rho > high)
            active += int(use)
            n += 1
            dz = np.zeros(NA)
            if use:
                dl_dmu = -(adv / mu_b) / gsize
                dz = dl_dmu * (1 - EPSILON) * pi * (inside - pi_e)
            dv = CRITIC_COEF * 2.0 * (value - g["reward"]) / (t * gsize)
            critic_loss += (value - g["reward"]) ** 2 / (t * gsize)
            grad["wa"] += dz[:, None] * hid[None, :]
            grad["ba"] += dz
            grad["wv"] += dv * hid
            grad["bv"] += dv
            dh = (params["wa"] * dz[:, None]).sum(axis=0) + params["wv"] * dv
            du = dh * (1.0 - hid * hid)
            grad["w1"] += du[:, None] * xn[None, :]
            grad["b1"] += du
    return grad, {"actor_loss": actor_loss, "critic_loss": critic_loss, "decisions": n,
                  "clipped": clipped, "gradient_active": active}


def global_norm(grad: dict) -> float:
    return math.sqrt(sum(float((grad[k] * grad[k]).sum()) for k in PARAMS))


def adam_step(params, grad, m, v, t, lr=LR):
    b1, b2 = BETAS
    for k in PARAMS:
        m[k] = b1 * m[k] + (1 - b1) * grad[k]
        v[k] = b2 * v[k] + (1 - b2) * grad[k] * grad[k]
        mhat = m[k] / (1 - b1 ** t)
        vhat = v[k] / (1 - b2 ** t)
        params[k] = params[k] - lr * mhat / (np.sqrt(vhat) + ADAM_EPS)


def behavior_kl(params, norm, games) -> float:
    """Mean over decisions of KL(mu_b || mu_theta) on the executable coalesced choices."""
    total, n = 0.0, 0
    for g in games:
        for d in g["decisions"]:
            _, _, actor, _ = forward(params, norm, d["x"])
            pi_c = coalesce(policy(d["q"], actor), d["cs"])
            mu = (1 - EPSILON) * pi_c + EPSILON * coalesce(d["q"], d["cs"])
            total += float((d["mu_b"] * np.log(d["mu_b"] / mu)).sum())
            n += 1
    return total / n if n else 0.0


def minibatch_order(lineage: str, update: int, epoch: int, n: int) -> list:
    """A pinned outcome-blind permutation of the batch's games."""
    return sorted(range(n), key=lambda i: h(NS, "minibatch", lineage, update, epoch, i))


def ppo(params: dict, norm: dict, games: list, lineage: str, update: int, m: dict, v: dict, t: int,
        lr=LR, epochs=EPOCHS, minibatch=MINIBATCH_GAMES, kl_stop=KL_STOP):
    """Bounded PPO epochs in place; returns (t, epoch records)."""
    records = []
    for epoch in range(epochs):
        order = minibatch_order(lineage, update, epoch, len(games))
        steps = []
        for start in range(0, len(order), minibatch):
            mb = [games[i] for i in order[start:start + minibatch]]
            grad, st = minibatch_grad(params, norm, mb)
            gn = global_norm(grad)
            scale = GRAD_CAP / gn if gn > GRAD_CAP else 1.0
            if scale != 1.0:
                grad = {k: grad[k] * scale for k in PARAMS}
            t += 1
            adam_step(params, grad, m, v, t, lr)
            steps.append(dict(st, grad_norm=gn, capped=gn > GRAD_CAP,
                              actor_grad_norm=math.sqrt(float((grad["wa"] ** 2).sum() + (grad["ba"] ** 2).sum())),
                              games=[order[i] for i in range(start, min(start + minibatch, len(order)))]))
        kl = behavior_kl(params, norm, games)
        records.append({"epoch": epoch, "kl_after": kl, "steps": steps})
        if kl > kl_stop:
            break
    return t, records


def tv_from_q(params, norm, games) -> dict:
    tvs = []
    for g in games:
        for d in g["decisions"]:
            _, _, actor, _ = forward(params, norm, d["x"])
            tvs.append(0.5 * float(np.abs(coalesce(policy(d["q"], actor), d["cs"]) - coalesce(d["q"], d["cs"])).sum()))
    return {"mean": sum(tvs) / len(tvs) if tvs else 0.0, "max": max(tvs, default=0.0), "rows": len(tvs)}


def update(weights: bytes, adam: bytes | None, rows: list, index: int, lineage: str, lr: float = LR,
           games_per_parent: int = GAMES_PER_PARENT):
    """One parent's PPO update from its batch rows in schedule order. ``lr`` and
    ``games_per_parent`` differ from the pinned constants only in the synthetic calibration.

    Returns (new weights bytes, new Adam bytes, record)."""
    w = read_weights(weights)
    wsha = sha(weights)
    if w["parent"]["lineage"] != lineage:
        raise ValueError("weights belong to another parent")
    if len(rows) != games_per_parent:
        raise ValueError(f"a parent's batch has {games_per_parent} games, not {len(rows)}")
    games = []
    for r in rows:
        reward, decisions = game_terms(r, w, wsha)
        for d in decisions:
            d["adv"] = reward - d["critic_b"]
        games.append({"reward": reward, "decisions": decisions})
    params = {k: w["params"][k].copy() for k in PARAMS}
    m, v, t = read_adam(adam, params, wsha)
    tv_before = tv_from_q(params, w["norm"], games)
    t, epochs = ppo(params, w["norm"], games, lineage, index, m, v, t, lr=lr)
    out = weights_bytes(w["arm"], params, w["norm"], w["parent"], index + 1, w["doc"]["init_seed"], wsha)
    adam_out = adam_bytes(m, v, t, sha(out))
    advs = [d["adv"] for g in games for d in g["decisions"]]
    record = {"update": index, "lineage": lineage, "arm": w["arm"], "lr": lr, "weights_in_sha256": wsha,
              "weights_out_sha256": sha(out), "adam_in_sha256": None if adam is None else sha(adam),
              "adam_out_sha256": sha(adam_out), "adam_t": t,
              "returns": [g["reward"] for g in games], "mean_return": sum(g["reward"] for g in games) / len(games),
              "decisions": [len(g["decisions"]) for g in games],
              "mean_advantage": sum(advs) / len(advs) if advs else None,
              "epochs_run": len(epochs), "kl_stopped": len(epochs) < EPOCHS, "epochs": epochs,
              "tv_from_q_before": tv_before, "tv_from_q_after": tv_from_q(params, w["norm"], games),
              "rows": [{"id": r["id"], "primary_sha256": primary_sha(r)} for r in rows]}
    return out, adam_out, record


def strip_timing(v):
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
    lines = [ln for ln in path.read_text(encoding="utf-8").splitlines() if ln.strip()]
    return json.loads(lines[0]) if len(lines) == 1 else None


def replay(ledger: Path) -> dict:
    """Recompute every recorded update from its recorded inputs (weights, Adam, rows)."""
    recs = [json.loads(ln) for ln in ledger.read_text().splitlines() if ln.strip()]
    recs = [r for r in recs if r.get("kind") == "update"]
    mismatches = []
    for r in recs:
        wb = Path(r["weights_in"]).read_bytes()
        ab = Path(r["adam_in"]).read_bytes() if r.get("adam_in") else None
        rows = [read_row(Path(p)) for p in r["row_files"]]
        out, adam_out, rec = update(wb, ab, rows, r["update"], r["lineage"], lr=r["lr"])
        if (rec["weights_out_sha256"], rec["adam_out_sha256"]) != (r["weights_out_sha256"], r["adam_out_sha256"]):
            mismatches.append([r["update"], r["lineage"]])
    return {"updates": len(recs), "mismatches": mismatches}


# ---------------------------------------------------------------- normalization and init

def freeze_norm(rows: list, lineage: str, arm: str) -> dict:
    """Per-field mean and population scale over the parent's logged consultation rows (any
    arm). A field with scale below 1e-6 keeps scale 1 (constant inputs stay unamplified)."""
    xs = []
    ids = []
    for r in rows:
        if r is None or r.get("kind") != "allocator2_game" or r["lineage"] != lineage:
            continue
        ids.append(r["id"])
        for d in r["game"]["decisions"]:
            s = d.get("scheduler")
            if s and s.get("nontrivial", True):
                xs.append(decision_x(arm, s))
    if not xs:
        raise ValueError("no consultation rows to freeze normalization from")
    x = np.array(xs)
    mean = x.mean(axis=0)
    std = np.sqrt(((x - mean) ** 2).mean(axis=0))
    scale = np.where(std > 1e-6, std, 1.0)
    return {"mean": mean, "scale": scale, "frozen": True,
            "source": {"lineage": lineage, "arm": arm, "rows": len(xs), "games": sorted(ids)}}


def init_seed(arm: str, lineage: str, label: str) -> int:
    return seed(NS, "controller-init", arm, lineage, label)


def init_weights(arm: str, parent: dict, norm: dict | None, label: str) -> bytes:
    nf = len(feature_names(arm))
    s = init_seed(arm, parent["lineage"], label)
    return weights_bytes(arm, init_params(nf, s), norm or default_norm(nf), parent, 0, s, None)


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
                and int(ev.get("qualified_parallel_slots", 0)) >= int(plan.get("slots", 1))):
            raise SystemExit("no matching throughput evidence for this host, binary and slot count")
    return binary


def case_line(c: dict, arm: str) -> dict:
    return {"id": f"{c['id']}-{arm}", "arm": arm, "controller_seed": c["controller_seed"], "case": c["case"]}


def launch(plan: dict, binary: Path, job: dict, out: Path):
    """One single-worker ``s4a-allocator-v2`` process for one arm-game."""
    root, lin = Path(plan["root"]), job["lineage"]
    case_file = out.with_suffix(".case.jsonl")
    case_file.write_bytes(jsonl([job["line"]]))
    env = dict(os.environ, BANK_SPEC=str(root / "frozen" / f"bank-spec-{lin}.json"),
               OPPONENTS=f"T1={root / 'sources' / 't1-source.json'},A48={root / 'sources' / 'a48-source.json'}",
               S4A_MODEL=lin, ALLOC2_CASES=str(case_file), BUDGET_KEY_MODE="repaired",
               ALLOC2_TOTAL=str(plan.get("total", CAP)))
    env.pop("ALLOC2_WEIGHTS", None)
    if job.get("weights"):
        env["ALLOC2_WEIGHTS"] = str(job["weights"])
    mode = "s4a-allocator-v2-probe" if "probe" in job["line"] else "s4a-allocator-v2"
    log = open(out.with_suffix(".log"), "w")
    flags = (subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW) if os.name == "nt" else 0
    child = subprocess.Popen([str(binary), f"source={root / 'sources' / f'{lin}-source.json'}", f"out={out}",
                              f"mode={mode}", "workers=1", "first_game=0", "games=1"],
                             cwd=str(out.parent), env=env, stdout=log, stderr=subprocess.STDOUT,
                             creationflags=flags)
    return child, log


def run_jobs(plan: dict, binary: Path, jobs: list, slots: int, ledger: Path, phase: str) -> None:
    """Runs jobs (each {label, lineage, line, weights?, out}) N at a time; one ledger line per
    process with its CPU and wall seconds."""
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


def load_cases(schedule: Path, domain: str) -> list:
    return [json.loads(ln) for ln in (schedule / f"cases-{domain}.jsonl").read_text().splitlines() if ln.strip()]


def write_state(path: Path, state: dict) -> None:
    path.write_text(json.dumps(state, indent=1) + "\n")


def train(plan: dict) -> int:
    """Synchronous controller training of one arm. PLAN: {root, binary, binary_sha256, host,
    arm (S|H), schedule, domain (a training domain), cycles (1 or 2), out_dir, slots,
    init {r1: W0, r2: W0}, max_attempts (default 2), formal, throughput_evidence, [total],
    [lr (default LR; recorded per update)]}.
    Batch b plays its 36 games at the frozen weights of both parents, then one PPO update per
    parent. Resumes from out_dir/STATE.json; a technical nonterminal pauses (never a loss)."""
    binary = admit(plan)
    arm = plan["arm"]
    if arm not in LEARNED:
        raise SystemExit("train needs arm S or H")
    out = Path(plan["out_dir"])
    out.mkdir(parents=True, exist_ok=True)
    ledger = out / "TRAIN-LEDGER.jsonl"
    state_path = out / "STATE.json"
    schedule = Path(plan["schedule"])
    cases = load_cases(schedule, plan["domain"])
    batches = json.loads((schedule / f"batches-{plan['domain']}.json").read_text())
    n_updates = BATCHES_PER_CYCLE * int(plan.get("cycles", 1))
    if state_path.exists():
        state = json.loads(state_path.read_text())
    else:
        state = {"next_update": 0, "attempts": {}, "status": "running", "lineages": {}}
        for lin in LINEAGES:
            data = Path(plan["init"][lin]).read_bytes()
            w = read_weights(data)
            if w["arm"] != arm or w["parent"]["lineage"] != lin or w["doc"]["update"] != 0:
                raise SystemExit(f"init weights for {lin} are not an update-0 {arm} controller of {lin}")
            name = f"w00-{lin}.json"
            (out / name).write_bytes(data)
            state["lineages"][lin] = {"weights": name, "weights_sha256": sha(data), "adam": None, "adam_sha256": None}
    while state["next_update"] < min(n_updates, len(batches)):
        k = state["next_update"]
        frozen = {}
        for lin in LINEAGES:
            st = state["lineages"][lin]
            wb = (out / st["weights"]).read_bytes()
            if sha(wb) != st["weights_sha256"]:
                raise SystemExit(f"{st['weights']} changed since it was written")
            frozen[lin] = (out / st["weights"], wb)
        jobs = []
        for lin in LINEAGES:
            for i, index in enumerate(batches[k]["games"][lin]):
                label = f"t{k:02d}-{lin}-{i:02d}"
                row = read_row(out / f"{label}.jsonl")
                done = (row is not None and row.get("kind") == "allocator2_game"
                        and row["controller"]["weights_sha256"] == state["lineages"][lin]["weights_sha256"]
                        and row["game"].get("valid") and row["game"].get("end") in REWARD)
                if done:
                    continue
                n = state["attempts"].get(label, 0)
                if n >= plan.get("max_attempts", 2):
                    state["status"] = f"paused: {label} exhausted its retry allowance"
                    write_state(state_path, state)
                    print(json.dumps(state))
                    return 3
                if (out / f"{label}.jsonl").exists():
                    (out / f"{label}.jsonl").rename(out / f"{label}.failed{n}.jsonl")
                state["attempts"][label] = n + 1
                jobs.append({"label": label, "lineage": lin, "line": case_line(cases[index], arm),
                             "weights": str(frozen[lin][0]), "out": str(out / f"{label}.jsonl")})
        write_state(state_path, state)
        run_jobs(plan, binary, jobs, int(plan.get("slots", 1)), ledger, "training")
        results = {}
        try:
            for lin in LINEAGES:
                st = state["lineages"][lin]
                files = [out / f"t{k:02d}-{lin}-{i:02d}.jsonl" for i in range(len(batches[k]["games"][lin]))]
                adam = (out / st["adam"]).read_bytes() if st["adam"] else None
                new, adam_new, rec = update(frozen[lin][1], adam, [read_row(p) for p in files], k, lin,
                                            lr=float(plan.get("lr", LR)))
                results[lin] = (files, new, adam_new, rec)
        except TechnicalNonterminal as e:
            state["status"] = f"paused: technical nonterminal in update {k}: {e}"
            write_state(state_path, state)
            print(json.dumps({"paused": str(e), "update": k}))
            return 3
        for lin, (files, new, adam_new, rec) in results.items():
            st = state["lineages"][lin]
            wname, aname = f"w{k + 1:02d}-{lin}.json", f"adam{k + 1:02d}-{lin}.json"
            (out / wname).write_bytes(new)
            (out / aname).write_bytes(adam_new)
            rec.update(kind="update", row_files=[str(p) for p in files], weights_in=str(out / st["weights"]),
                       adam_in=str(out / st["adam"]) if st["adam"] else None, weights_out=wname, adam_out=aname)
            with ledger.open("a") as f:
                f.write(json.dumps(rec) + "\n")
            state["lineages"][lin] = {"weights": wname, "weights_sha256": sha(new), "adam": aname,
                                      "adam_sha256": sha(adam_new)}
            print(json.dumps({"update": k, "lineage": lin, "mean_return": rec["mean_return"],
                              "epochs": rec["epochs_run"], "tv_after": rec["tv_from_q_after"]["mean"],
                              "weights": wname}), flush=True)
        state["next_update"] = k + 1
        write_state(state_path, state)
    state["status"] = "complete"
    write_state(state_path, state)
    print(json.dumps({"status": "complete", "endpoints": {lin: state["lineages"][lin]["weights"] for lin in LINEAGES}}))
    return 0


def queue(plan: dict) -> int:
    """Arm-games of a non-training domain. PLAN: {root, binary, binary_sha256, host, phase,
    schedule, out_dir, slots, formal, throughput_evidence, [total], jobs: [{label, domain,
    index, arm, weights (learned arms)}]} or {probe_cases: FILE} for the
    supported-node probe (each line names its source weights). Complete rows are skipped."""
    binary = admit(plan)
    out = Path(plan["out_dir"])
    out.mkdir(parents=True, exist_ok=True)
    schedule = Path(plan["schedule"])
    jobs, cache = [], {}
    for j in plan.get("jobs", []):
        row = read_row(out / f"{j['label']}.jsonl")
        if row is not None and row.get("kind") == "allocator2_game":
            continue
        if j["domain"] not in cache:
            cache[j["domain"]] = load_cases(schedule, j["domain"])
        c = cache[j["domain"]][j["index"]]
        if (j["arm"] in LEARNED) != bool(j.get("weights")):
            raise SystemExit(f"{j['label']}: learned arms need weights, controls none")
        jobs.append({"label": j["label"], "lineage": c["lineage"], "line": case_line(c, j["arm"]),
                     "weights": j.get("weights"), "out": str(out / f"{j['label']}.jsonl")})
    if plan.get("probe_cases"):
        for line in [json.loads(ln) for ln in Path(plan["probe_cases"]).read_text().splitlines() if ln.strip()]:
            label = line["id"]
            if (row := read_row(out / f"{label}.jsonl")) is not None and row.get("kind") == "allocator2_game":
                continue
            lin = line["lineage"]
            jobs.append({"label": label, "lineage": lin,
                         "line": {k: v for k, v in line.items() if k not in ("lineage", "weights")},
                         "weights": line.get("weights") if line["arm"] in LEARNED else None,
                         "out": str(out / f"{label}.jsonl")})
    print(json.dumps({"pending": len(jobs), "host": plan["host"], "slots": plan["slots"]}), flush=True)
    run_jobs(plan, binary, jobs, int(plan["slots"]), out / "LEDGER.jsonl", plan["phase"])
    return 0


def panel_jobs(panel: str, arms: list, weights: dict) -> list:
    """Queue jobs of a panel: every case under every arm, case-major (paired arms run close
    together). ``arms`` are labels, ``LABEL=S|H`` for a learned replication; ``weights`` maps
    (label, lineage) to a controller. Rows are ``<panel>-<label>-<index:04d>.jsonl``."""
    rows, _ = domain_rows(panel)
    spec = []
    for a in arms:
        label, _, kind = a.partition("=")
        spec.append((label, kind or label))
    jobs = []
    for i, c in enumerate(rows):
        for label, kind in spec:
            if kind not in ARMS:
                raise ValueError(f"unknown arm {kind}")
            j = {"label": f"{panel}-{label}-{i:04d}", "domain": panel, "index": i, "arm": kind}
            if kind in LEARNED:
                j["weights"] = weights[(label, c["lineage"])]
            jobs.append(j)
    return jobs


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("schedule")
    s.add_argument("--out", type=Path, required=True)
    s.add_argument("--exclude-seeds", nargs="*", default=[])
    s = sub.add_parser("freeze-norm")
    s.add_argument("--rows", type=Path, nargs="+", required=True)
    s.add_argument("--lineage", required=True)
    s.add_argument("--arm", choices=LEARNED, required=True)
    s.add_argument("--out", type=Path, required=True)
    s = sub.add_parser("init")
    s.add_argument("--arm", choices=LEARNED, required=True)
    s.add_argument("--parent-row", type=Path, required=True)
    s.add_argument("--lineage", required=True)
    s.add_argument("--normalization", type=Path)
    s.add_argument("--label", required=True)
    s.add_argument("--out", type=Path, required=True)
    s = sub.add_parser("update")
    s.add_argument("--weights", type=Path, required=True)
    s.add_argument("--adam", type=Path)
    s.add_argument("--rows", type=Path, nargs="+", required=True)
    s.add_argument("--update", type=int, required=True)
    s.add_argument("--lineage", required=True)
    s.add_argument("--lr", type=float, default=LR)
    s.add_argument("--out-weights", type=Path, required=True)
    s.add_argument("--out-adam", type=Path, required=True)
    s = sub.add_parser("panel-jobs")
    s.add_argument("--panel", required=True)
    s.add_argument("--arms", nargs="+", required=True)
    s.add_argument("--weights", nargs="*", default=[], help="LABEL:LINEAGE=W.json")
    s.add_argument("--out", type=Path, required=True)
    s = sub.add_parser("replay")
    s.add_argument("--ledger", type=Path, required=True)
    s = sub.add_parser("probe-select")
    s.add_argument("--rows", type=Path, nargs="+", required=True)
    s.add_argument("--limit", type=int, default=36)
    s.add_argument("--out", type=Path, required=True)
    for name in ("train", "queue"):
        sub.add_parser(name).add_argument("plan", type=Path)
    a = ap.parse_args(argv)
    if a.cmd == "schedule":
        summary = write_schedule(a.out, a.exclude_seeds)
        print(json.dumps({k: v for k, v in summary.items() if k not in ("files", "decks")}))
    elif a.cmd == "freeze-norm":
        rows = [read_row(p) for p in a.rows]
        n = freeze_norm(rows, a.lineage, a.arm)
        doc = {"mean": n["mean"].tolist(), "scale": n["scale"].tolist(), "frozen": True, "source": n["source"],
               "features": feature_names(a.arm)}
        a.out.write_text(json.dumps(doc, separators=(",", ":")) + "\n")
        print(json.dumps({"rows": n["source"]["rows"], "out": str(a.out)}))
    elif a.cmd == "init":
        row = read_row(a.parent_row)
        parent = row["controller"]["parent"]
        if parent["lineage"] != a.lineage or row["player_after"] != parent:
            raise SystemExit("parent row does not bind this lineage's frozen player")
        norm = None
        if a.normalization:
            d = json.loads(a.normalization.read_text())
            if d["features"] != feature_names(a.arm):
                raise SystemExit("normalization features differ from the arm")
            norm = {"mean": np.array(d["mean"]), "scale": np.array(d["scale"]), "frozen": True,
                    "source": d["source"]}
        data = init_weights(a.arm, parent, norm, a.label)
        a.out.write_bytes(data)
        print(json.dumps({"weights": str(a.out), "sha256": sha(data)}))
    elif a.cmd == "update":
        new, adam_new, rec = update(a.weights.read_bytes(), a.adam.read_bytes() if a.adam else None,
                                    [read_row(p) for p in a.rows], a.update, a.lineage, lr=a.lr)
        a.out_weights.write_bytes(new)
        a.out_adam.write_bytes(adam_new)
        print(json.dumps({k: v for k, v in rec.items() if k != "epochs"}))
    elif a.cmd == "panel-jobs":
        wmap = {}
        for item in a.weights:
            k, path = item.split("=", 1)
            label, lin = k.split(":")
            wmap[(label, lin)] = path
        jobs = panel_jobs(a.panel, a.arms, wmap)
        a.out.write_text(json.dumps(jobs, indent=1) + "\n")
        print(json.dumps({"jobs": len(jobs)}))
    elif a.cmd == "probe-select":
        files = [f for p in a.rows for f in (sorted(p.glob("*.jsonl")) if p.is_dir() else [p])]
        rows = [read_row(f) for f in files if not f.name.endswith(".case.jsonl")]
        lines = probe_cases(probe_select(rows, a.limit))
        a.out.write_bytes(jsonl(lines))
        print(json.dumps({"roots": len(lines) // 8, "lines": len(lines)}))
    elif a.cmd == "replay":
        print(json.dumps(replay(a.ledger)))
    elif a.cmd == "train":
        return train(json.loads(a.plan.read_text()))
    elif a.cmd == "queue":
        return queue(json.loads(a.plan.read_text()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
