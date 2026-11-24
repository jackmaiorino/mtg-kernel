"""Synthetic fixtures for the reader tests: pools, prep results, v2 pod records, plan and prep sets.

Everything is in namespace `test`, written to the frozen shapes (m4-interfaces/v2, read plan v2).
Seats are uniform, the pilot is uniform, and game results are chosen by the test, so every expected
answer is known exactly.
"""

from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import random
import sys
from typing import Any, Callable

CUBE_V1_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(CUBE_V1_DIR))

import deal  # noqa: E402
import pod_record  # noqa: E402
import read_plans  # noqa: E402
import read_sets  # noqa: E402
import seeds_v1  # noqa: E402

NS = "test"
COLORS = deal.COLORS
BASIC_BASE = 1000
UNIFORM = {"kind": "uniform", "pin": None}
STATED = {"kind": "stated", "pin": None}
PILOT = dict(read_plans.UNIFORM_PILOT)
PINS = {k: hashlib.sha256(f"fixture-{k}".encode()).hexdigest() for k in pod_record.PINS_KEYS}
CONSTANTS = {"deck_size": 40, "colours_per_seat": 2, "nonland_spells": 23, "nonbasic_lands": 3,
             "basic_split": "even_first_colour_extra", "pairs_per_update": 2}
BINARY = hashlib.sha256(b"fixture-binary").hexdigest()
PLATFORM = "fixture-platform"
WATCHDOG_END = {"kind": "terminal", "code": "watchdog", "classification": "watchdog", "reason": "watchdog_seconds"}


def pool_doc() -> dict[str, Any]:
    """A synthetic active pool large enough for 8 disjoint 45-card pools and every deal."""
    spells, cid = [], 1
    for color in COLORS:
        for k in range(60):
            spells.append({"card_id": cid, "name": f"Synth {color} {k}", "colors": [color], "is_land": False})
            cid += 1
    for k in range(20):
        spells.append({"card_id": cid, "name": f"Synth Colourless {k}", "colors": [], "is_land": False})
        cid += 1
    lands = []
    for k in range(40):
        lands.append({"card_id": cid, "name": f"Synth Land {k}", "colors": [], "is_land": True})
        cid += 1
    basics = [{"card_id": BASIC_BASE + i, "name": f"Synth Basic {c}", "colors": [c], "is_land": True, "color": c}
              for i, c in enumerate(COLORS)]
    return {"kernel_carddb_hash": "00000000synthetic", "spells": spells, "nonbasic_lands": lands, "basics": basics}


POOL = (pool_doc(), hashlib.sha256(read_plans.serialize(pool_doc()).encode()).hexdigest())
CONST = (dict(CONSTANTS), hashlib.sha256(read_plans.serialize(CONSTANTS).encode()).hexdigest())
DEALER = {"pool_doc": POOL[0], "constants_doc": CONST[0], "pool_sha256": POOL[1], "constants_sha256": CONST[1]}


def draft_pools(draft_seed: int) -> list[list[int]]:
    ids = [c["card_id"] for c in POOL[0]["spells"]] + [c["card_id"] for c in POOL[0]["nonbasic_lands"]]
    picked = random.Random(draft_seed).sample(ids, 8 * 45)
    return [picked[45 * s: 45 * (s + 1)] for s in range(8)]


def draft_record(draft_seed: int, pools: list[list[int]] | None = None) -> dict[str, Any]:
    return {"schema": "cube_draft_record/v1", "draft_seed": draft_seed, "packs": [[draft_seed % 7]], "picks": [],
            "pools": pools or draft_pools(draft_seed)}


ROW_VARIANT = {"candidate": 0, "current": 0, "b1_last": 0, "k1_last": 2, "initial": 3, "stated": 1}


def build_from_pool(pool: list[int], row: str) -> list[int]:
    """A 40-card deck of a pool: 23 nonbasic cards and 17 basics, varied by builder row."""
    v = ROW_VARIANT[row]
    nonbasic = sorted(pool)
    chosen = nonbasic[v: v + 23]
    basics = [BASIC_BASE + (v % 5)] * 9 + [BASIC_BASE + ((v + 1) % 5)] * 8
    return sorted(chosen + basics)


def prep_result(request: dict[str, Any], request_sha: str) -> dict[str, Any]:
    draft = draft_record(request["seeds"]["draft_seed"])
    builds = []
    for b in request["builds"]:
        builds.append({**b, "build_seed": None if b["row"] == "stated" else deal.seed(["build", b["chair"], b["row"]]),
                       "cards": build_from_pool(draft["pools"][b["chair"]], b["row"])})
    return {"schema": read_plans.PLAN_SCHEMA, "kind": "prep_result", "request_sha256": request_sha,
            "draft": draft, "builds": builds}


def _row(kind: str, options: list[int], chosen: int, stream: int) -> dict[str, Any]:
    return {"kind": kind, "sampler": pod_record.UNIFORM_SAMPLER, "options": options, "chosen": chosen,
            "stream_values": [stream], "logits_bits": None, "p_chosen_bits": None, "value_bits": None}


def uniform_build_rows(cards: list[int], pool: list[int]) -> dict[str, Any]:
    basics = [c for c in cards if c >= BASIC_BASE]
    nonbasic = [c for c in cards if c < BASIC_BASE]
    remaining = sorted(set(pool) | set(nonbasic))
    pointer = []
    for i, card in enumerate(nonbasic):
        pointer.append(_row("pointer", list(remaining), card, i))
        remaining.remove(card)
    mask = 0
    for c in basics:
        mask |= 1 << (c - BASIC_BASE)
    bits = bin(mask).count("1")
    type_set, split = None, []
    if basics:
        type_set = _row("type_set", [m for m in range(1, 32) if bin(m).count("1") <= len(basics)], mask, 99)
        if bits >= 2:
            t_bits = [1 << i for i in range(5) if mask & (1 << i)]
            split = [_row("split", t_bits, t_bits[i % len(t_bits)], 100 + i) for i in range(len(basics) - bits)]
    return {"count": _row("count", list(range(41)), len(basics), 98), "pointer": pointer, "type_set": type_set,
            "split": split}


# ---------------------------------------------------------------------------------------------
# Plans


def plan(family: str, unit: int, *, mirror: int | None = None, rotation: int | None = None,
         mark: dict[str, Any] | None = None, prep: tuple[dict[str, Any], str] | None = None) -> dict[str, Any]:
    rows = read_plans._rows(family)
    builders = {r: dict(UNIFORM) for r in rows if r != "stated"}
    kwargs: dict[str, Any] = {}
    if read_plans.FAMILIES[family]["layout"] in ("b", "c", "d13"):
        kwargs = dict(DEALER)
    return read_plans.build_plan(family, NS, unit, pins=PINS, pilot=PILOT,
                                 picker=None if family == "dose" else dict(UNIFORM), builders=builders,
                                 mirror=mirror, rotation=rotation, mark=mark, prep=prep, **kwargs)


def prep_request(prep_family: str, unit: int) -> dict[str, Any]:
    rows = read_plans.PREP_ROWS[prep_family]
    return read_plans.build_prep_request(prep_family, NS, unit, PINS,
                                         None if prep_family in ("dose", "dose_panel") else dict(UNIFORM),
                                         {r: dict(UNIFORM) for r in rows if r != "stated"})


def prep_for(prep_family: str, unit: int) -> tuple[dict[str, Any], str, dict[str, Any], str]:
    request = prep_request(prep_family, unit)
    req_sha = read_plans.doc_sha256(request)
    result = prep_result(request, req_sha)
    return request, req_sha, result, read_plans.doc_sha256(result)


# ---------------------------------------------------------------------------------------------
# Records

Outcome = Any  # "win", "loss", "draw", "halt" (focal side), or ("halt", end, source, decisions)


def _focal_deck(p: dict[str, Any], game: dict[str, Any]) -> int:
    layout = read_plans.FAMILIES[p["family"]]["layout"]
    if layout in ("a", "one", "all"):
        learning = {c["chair"] for c in p["chairs"] if c["role"] == "learning"}
        picks = [d for d, c in zip(game["decks"], game["chairs"]) if c in learning]
        return picks[0] if picks else game["decks"][0]
    return next(d for d in game["decks"] if p["decks"][d]["source"] == "build")


def _a_cards(unit: int, chair: int, row: str) -> list[int]:
    start = 1 + 23 * chair + (0 if row == "candidate" else 200) + unit % 3
    return sorted(list(range(start, start + 23)) + [BASIC_BASE] * 9 + [BASIC_BASE + 1] * 8)


def record(p: dict[str, Any], outcome: Callable[[dict[str, Any], dict[str, Any], int], Outcome], *,
           games: int | None = None, status: str = "complete") -> dict[str, Any]:
    """Play a plan synthetically. `outcome(plan, game, focal_position)` gives the focal deck's result."""
    pools = p["pools"] if p["pools"] is not None else draft_pools(p["seeds"]["draft_seed"])
    rec: dict[str, Any] = {
        "schema": pod_record.POD_RECORD_SCHEMA, "interfaces": pod_record.INTERFACES_VERSION, "use": p["use"],
        "namespace": p["namespace"], "run": p["run"], "pod_index": p["pod_index"], "seeds": dict(p["seeds"]),
        "pins": dict(p["pins"]), "pilot": dict(p["pilot"]), "caps": dict(p["caps"]),
        "draft": draft_record(p["seeds"]["draft_seed"], pools),
        "chairs": [{**c, "pick_rows": []} for c in p["chairs"]], "decks": [], "games": [], "paired_builds": [],
        "halts": [], "status": status, "stop": None,
    }
    for d in p["decks"]:
        cards = d["cards"] if d["cards"] is not None else _a_cards(p["unit"], d["chair"], d["row"])
        rows = None
        if d["source"] == "build" and d["builder"]["kind"] != "stated":
            rows = uniform_build_rows(cards, pools[d["chair"]])
        rec["decks"].append({"deck": d["deck"], "source": d["source"], "chair": d["chair"], "build": d["build"],
                             "builder": copy.deepcopy(d["builder"]), "build_seed": None if rows is None else 7,
                             "cards": cards, "build_rows": rows, "dealt": copy.deepcopy(d["dealt"])})
    for g in p["games"][: games if games is not None else len(p["games"])]:
        focal = _focal_deck(p, g)
        pos = g["decks"].index(focal)
        res = outcome(p, g, pos)
        seeds = seeds_v1.attempt_seeds(g["game_seed"])
        source, decisions = None, 100
        if isinstance(res, tuple):
            _, end, source, decisions = res
        elif res == "halt":
            end = {"kind": "decision_cap"}
        else:
            winner = None if res == "draw" else (pos if res == "win" else 1 - pos)
            end = {"kind": "natural", "winner_seat": winner, "reason": "synthetic"}
        halted = end["kind"] != "natural"
        attempt = {**seeds, "attempt": 0}
        attempt = {"attempt": 0, "deal_seed": seeds["deal_seed"], "env_seed": seeds["env_seed"],
                   "action_seed": seeds["action_seed"], "decisions": decisions, "physical_decisions": None,
                   "policy_steps": None, "end": end, "halted": halted, "stop_kind": pod_record.expected_stop_kind(end),
                   "last_action_source": source}
        scores = None
        if not halted:
            w = end["winner_seat"]
            scores = [0, 0] if w is None else ([1, -1] if w == 0 else [-1, 1])
        rec["games"].append({**{k: copy.deepcopy(g[k]) for k in read_plans.PLAN_GAME_KEYS}, "attempts": [attempt],
                             "scores": scores})
        rec["games"][-1] = {k: rec["games"][-1][k] for k in pod_record.GAME_KEYS}
    rec["paired_builds"] = copy.deepcopy(p["paired_builds"])
    rec["halts"] = [{"game": g["game"], "attempt": 0, "stop_kind": a["stop_kind"],
                     "last_action_source": a["last_action_source"], "deal_seed": a["deal_seed"]}
                    for g in rec["games"] for a in g["attempts"] if a["halted"]]
    if status == "stopped":
        last = rec["games"][-1] if rec["games"] else None
        rec["stop"] = {"game": last["game"] if last else 0, "pairing": None, "decks": last["decks"] if last else [],
                       "attempts": [{"deal_seed": a["deal_seed"], "stop_kind": a["stop_kind"],
                                     "last_action_source": a["last_action_source"]} for a in
                                    (last["attempts"] if last else [])]}
    return pod_record.validate_record(rec)


# ---------------------------------------------------------------------------------------------
# Sets on disk


def write_prep_set(root: Path, prep_family: str, manifest_family: str, set_name: str, units: range, decl_sha: str,
                   *, executions: list[dict[str, Any]] | None = None, reserved: list[int] | None = None,
                   panel_sha256: str | None = None) -> tuple[Path, dict[int, tuple[dict[str, Any], str]]]:
    """A prep set with requests and executions. Each execution spec: {"ended", "units" (with markers),
    "receipt" (bool), "tamper" (unit -> callable on the result)}. Default: one complete execution."""
    requests, results = {}, {}
    for u in units:
        request, req_sha, result, res_sha = prep_for(prep_family, u)
        requests[u], results[u] = (request, req_sha), (result, res_sha)
    man = read_plans.build_manifest("prep", manifest_family, NS, set_name, decl_sha,
                                    [(u, requests[u][1]) for u in units], units=[units.start, units.stop],
                                    reserved=reserved, panel_sha256=panel_sha256)
    executions = executions or [{"ended": "complete"}]
    for e, spec in enumerate(executions):
        man["executions"].append({"execution": e, "reason": "first" if e == 0 else "prep_retry", "expired_games": [],
                                  "binary": BINARY, "platform": PLATFORM})
    set_dir = read_plans.write_set(root, man, {f"requests/unit-{u}.json": requests[u][0] for u in units})
    for e, spec in enumerate(executions):
        exec_dir = set_dir / f"exec-{e}"
        done_units = spec.get("units", list(units))
        outputs = []
        for u in done_units:
            result = copy.deepcopy(results[u][0])
            if u in spec.get("tamper", {}):
                spec["tamper"][u](result)
            text = read_plans.serialize(result)
            (exec_dir / "prep").mkdir(parents=True, exist_ok=True)
            (exec_dir / "prep" / f"unit-{u}.json").write_bytes(text.encode())
            sha = hashlib.sha256(text.encode()).hexdigest()
            if u not in spec.get("no_marker", ()):
                marker = {"unit": u, "files": [{"file": f"prep/unit-{u}.json",
                                                "sha256": spec.get("marker_sha", {}).get(u, sha)}]}
                (exec_dir / "done").mkdir(parents=True, exist_ok=True)
                (exec_dir / "done" / f"{u}.json").write_bytes(read_plans.serialize(marker).encode())
            outputs.append({"unit": u, "sha256": sha, "status": "complete"})
        if spec.get("receipt", True):
            receipt = {"execution": e, "plan_set_sha256": man["plan_set_sha256"], "binary": BINARY,
                       "platform": PLATFORM, "ended": spec["ended"], "outputs": outputs}
            exec_dir.mkdir(parents=True, exist_ok=True)
            (exec_dir / "execution.json").write_bytes(read_plans.serialize(receipt).encode())
    (set_dir / "manifest.json").write_bytes(read_plans.serialize(man).encode())
    out = {}
    accepted = read_sets.accept_prep_set(set_dir) if executions[0].get("accept", True) else None
    if accepted:
        out = accepted["outputs"]
    return set_dir, out


def write_plan_set(root: Path, family: str, set_name: str, plans: list[dict[str, Any]], decl_sha: str, *,
                   executions: list[dict[str, Any]], look: int | None = None, units: list[int],
                   reserved: list[int] | None = None, pools_with: str | None = None, prep_set: str | None = None,
                   panel_sha256: str | None = None, replaces: str | None = None,
                   retired: list[list[int]] | None = None) -> Path:
    """A plan set. Each execution spec: {"records": {pod_index: record}, "ended": ..., "expired": [...]}."""
    files = {f"plans/pod-{p['pod_index']}.json": p for p in plans}
    entries = [(p["pod_index"], read_plans.doc_sha256(p)) for p in plans]
    man = read_plans.build_manifest("plans", family, NS, set_name, decl_sha, entries, look=look, units=units,
                                    reserved=reserved, pools_with=pools_with, panel_sha256=panel_sha256,
                                    prep_set=prep_set, replaces=replaces, retired=retired)
    for e, spec in enumerate(executions):
        man["executions"].append({"execution": e, "reason": "first" if e == 0 else "watchdog_rerun",
                                  "expired_games": spec.get("entry_expired", []), "binary": BINARY,
                                  "platform": PLATFORM})
    set_dir = read_plans.write_set(root, man, files)
    for e, spec in enumerate(executions):
        exec_dir = set_dir / f"exec-{e}"
        (exec_dir / "records").mkdir(parents=True, exist_ok=True)
        listed = []
        for i, rec in sorted(spec["records"].items()):
            text = json.dumps(rec, indent=2)
            (exec_dir / "records" / f"pod-{i}.json").write_bytes(text.encode())
            listed.append({"pod_index": i, "sha256": hashlib.sha256(text.encode()).hexdigest(),
                           "status": rec["status"]})
        receipt = {"execution": e, "plan_set_sha256": man["plan_set_sha256"], "binary": BINARY,
                   "platform": PLATFORM, "ended": spec.get("ended", "complete"), "records": listed}
        (exec_dir / "execution.json").write_bytes(read_plans.serialize(receipt).encode())
    return set_dir


def unit_draw(*parts: Any) -> float:
    return deal.seed(["synth-draw", *parts]) / float(1 << 63)
