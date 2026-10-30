"""Search on recorded human-game misplays (mode `s4a-sbroot`). Measurement only.

usage:
  sbroot.py job --engine ENGINE.jsonl[.gz] --spec SPEC.json --out JOB.json
  sbroot.py check --engine ENGINE.jsonl[.gz] --spec SPEC.json --rows ROWS.jsonl [--json OUT]

`job` turns a Spellbench pauper-v2 engine wire record into an `s4a-sbroot`
job: the bridge's episode id (FNV-1a 64 of the game id) and game seed (HMAC
of the game secret, as kernel_engine_v2.py derives it), the two catalog decks,
and every recorded decision as its acting seat, row kinds in flat-row order
(`row_candidate_ids`) and the selected row. SPEC names the target decisions
(wire decision indices, both seats counted) with, for each, what the model
played (the misplay) and the play the human reviewer called right, as
predicates over the wire candidates.

`check` reads the mode's rows. It confirms the rebuild: every applied step's
rows name the same cards as the record's candidates, and at every decision of
the model's seat its view (own hand, both battlefields with tapped state,
graveyards, life, hand and library sizes) equals the record's observation.
It then reads each search: the most visited move, its visits and value, the
whole visit distribution, whether it avoids the misplay and whether it is the
play the reviewer called right.
"""
from __future__ import annotations

import argparse
import collections
import gzip
import hashlib
import hmac
import json
from pathlib import Path


def read_lines(path: Path):
    opener = gzip.open if path.suffix == ".gz" else open
    with opener(path, "rt", encoding="utf-8") as stream:
        for line in stream:
            if line.strip():
                yield json.loads(line)


def fnv1a64(text: str) -> int:
    h = 0xCBF29CE484222325
    for b in text.encode("utf-8"):
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def wire(path: Path):
    """The reset request, and each decision with the selection that answered it."""
    lines = list(read_lines(path))
    reset = next(x["message"] for x in lines if x["message"].get("request_type") == "reset")
    decisions = [x["message"]["seat_decision"] for x in lines
                 if x["dir"] == "engine_to_host" and x["message"].get("response_type") == "decision"]
    steps = [x["message"]["selection"] for x in lines
             if x["dir"] == "host_to_engine" and x["message"].get("request_type") == "step"]
    if len(steps) > len(decisions):
        raise SystemExit("more selections than decisions")
    return reset, decisions, steps


def names(v) -> list[str]:
    """Every card name a semantic or object mentions, in serialization order."""
    out = []
    if isinstance(v, dict):
        if isinstance(v.get("card_name"), str):
            out.append(v["card_name"])
        for k, x in v.items():
            if k != "card_name":
                out.extend(names(x))
    elif isinstance(v, list):
        for x in v:
            out.extend(names(x))
    return out


def describe(sem: dict) -> str:
    kind = sem.get("kind")
    if kind == "declare_block":
        a = sem.get("attacker")
        return "no block" if a is None else f"block {a['card_name']} ({a['controller_seat']})"
    if kind == "choose_target":
        t = (sem.get("target") or {}).get("object") or (sem.get("target") or {})
        who = t.get("controller_seat") or t.get("seat") or ""
        return f"target {t.get('card_name', t)} ({who})"
    src = sem.get("source") or {}
    return f"{kind} {src.get('card_name', '')}".strip()


def matches(pred: dict, sem: dict) -> bool:
    """A spec predicate: every given field must equal the candidate's."""
    d = describe(sem)
    if "describe" in pred and d != pred["describe"]:
        return False
    if "describe_prefix" in pred and not d.startswith(pred["describe_prefix"]):
        return False
    if pred.get("not"):
        return not matches(pred["not"], sem)
    return True


def job(a) -> int:
    reset, decisions, steps = wire(a.engine)
    spec = json.loads(a.spec.read_text(encoding="utf-8"))
    secret = bytes.fromhex(reset["game_secret"])
    seed = int.from_bytes(hmac.new(secret, b"spellbench-kernel-v2/rng", hashlib.sha256).digest()[:8], "little")
    out = []
    for i, (d, sel) in enumerate(zip(decisions, steps)):
        rows = d["extensions"]["x_kernel_flat_v4"]["row_candidate_ids"]
        by_id = {c["candidate_id"]: c for c in d["candidates"]}
        if sorted(rows) != sorted(by_id):
            raise SystemExit(f"decision {i}: flat rows do not cover the candidates")
        out.append({"i": i, "seat": int(d["acting_seat"][1:]),
                    "kinds": [by_id[c]["semantic"]["kind"] for c in rows],
                    "row": rows.index(sel["candidate_id"])})
    j = {"game_id": reset["game_id"], "episode_id": fnv1a64(reset["game_id"]), "game_seed": seed,
         "deck_ids": [s["deck"]["catalog_id"] for s in reset["seats"]],
         "max_decisions": reset["max_decisions"], "max_steps": reset["max_steps"],
         "steps": out, "targets": sorted(int(t) for t in spec["targets"])}
    a.out.write_text(json.dumps(j) + "\n", encoding="utf-8")
    print(json.dumps({"game_id": j["game_id"], "steps": len(out), "targets": j["targets"]}))
    return 0


def seat_view(facts: dict, seat: int, card: list[str]) -> dict:
    view = {}
    for p, s in enumerate(facts["seats"]):
        view[p] = {"battlefield": sorted((card[c], bool(t)) for c, t, _, _ in s["bf"]),
                   "graveyard": sorted(card[c] for c in s["gy"]),
                   "life": s["life"], "hand": s["hand"], "library": s["library"]}
    view[seat]["hand_cards"] = sorted(card[c] for c in facts["seats"][seat]["hand_cards"])
    return view


def record_view(obs: dict, seat: int) -> dict:
    view = {}
    for pl in obs["players"]:
        p = int(pl["seat"][1:])
        view[p] = {"battlefield": sorted((c["card_name"], bool((c.get("permanent") or {}).get("tapped")))
                                         for c in pl["battlefield"]),
                   "graveyard": sorted(c["card_name"] for c in pl["graveyard"]),
                   "life": pl["life"], "hand": pl["hand_count"], "library": pl["library_count"]}
    own = next(pl for pl in obs["players"] if int(pl["seat"][1:]) == seat)
    view[seat]["hand_cards"] = sorted(c["card_name"] for c in own["hand"] or [])
    return view


def check(a) -> int:
    reset, decisions, steps = wire(a.engine)
    spec = json.loads(a.spec.read_text(encoding="utf-8"))
    rows = list(read_lines(a.rows))
    card = next(r["names"] for r in rows if r["kind"] == "card_names")
    model = spec["model_seat"]
    problems, skipped, applied = [], [], 0
    for r in rows:
        if r["kind"] == "mismatch":
            problems.append({"i": r["i"], "what": "kinds or actor", "record": r["record_kinds"],
                             "main": [x["kind"] for x in r["main"]["rows"]]})
        if r["kind"] != "step":
            continue
        d = decisions[r["i"]]
        if r["status"] == "skipped":
            skipped.append(r["i"])
            continue
        applied += 1
        order = d["extensions"]["x_kernel_flat_v4"]["row_candidate_ids"]
        by_id = {c["candidate_id"]: c for c in d["candidates"]}
        for k, cid in enumerate(order):
            want = collections.Counter(names(by_id[cid]["semantic"]))
            got = collections.Counter(card[c] for c in r["main"]["rows"][k]["cards"])
            if want != got:
                problems.append({"i": r["i"], "row": k, "what": "row names",
                                 "record": sorted(want.elements()), "main": sorted(got.elements())})
        if int(d["acting_seat"][1:]) == model:
            want, got = record_view(d["observation"], model), seat_view(r["main"]["facts"], model, card)
            if json.dumps(want, sort_keys=True) != json.dumps(got, sort_keys=True):
                problems.append({"i": r["i"], "what": "model view", "record": want, "main": got})
    targets = {}
    for t in spec["targets"]:
        i = int(t)
        d = decisions[i]
        order = d["extensions"]["x_kernel_flat_v4"]["row_candidate_ids"]
        by_id = {c["candidate_id"]: c for c in d["candidates"]}
        sems = [by_id[cid]["semantic"] for cid in order]
        sp = spec["targets"][t]
        played = steps[i]["candidate_id"]
        out = {"i": i, "label": sp["label"], "menu": [describe(s) for s in sems],
               "played": describe(by_id[played]["semantic"]), "right_call": sp.get("right_text"),
               "status": None, "searches": []}
        for r in rows:
            if r.get("i") != i:
                continue
            if r["kind"] == "target":
                out["status"] = r["status"]
            if r["kind"] == "error":
                out["searches"].append({"budget": r["budget"], "seed": r["seed"], "error": r["error"]})
            if r["kind"] != "search":
                continue
            o2 = r["o2"]
            edges = o2.get("edges") or []
            top = o2.get("top_edge")
            n = [e[0] for e in edges]
            q = [(e[1] + 0.5 * e[2]) / e[0] if e[0] else None for e in edges]
            top_sem = sems[top] if top is not None else None
            out["searches"].append({
                "budget": r["budget"], "seed": r["seed"], "top": describe(top_sem) if top_sem else None,
                "top_visits": n[top] if top is not None else None, "visits": n,
                "top_q": q[top] if top is not None else None,
                "avoids_misplay": None if top_sem is None else not matches(sp["misplay"], top_sem),
                "matches_right_call": None if top_sem is None or "right" not in sp else matches(sp["right"], top_sem),
                "simulations": r["selection"].get("completed_natural"), "cpu_seconds": r["cpu_seconds"]})
        out["status"] = out["status"] or ("searched" if out["searches"] else "not reached")
        targets[t] = out
    result = {"game_id": reset["game_id"], "applied": applied, "skipped_single_candidate": skipped,
              "rebuild_exact": not problems, "problems": problems[:20], "problem_count": len(problems),
              "targets": targets}
    if a.json:
        a.json.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")
    print(json.dumps({k: result[k] for k in ("game_id", "applied", "rebuild_exact", "problem_count")}
                     | {"skipped": len(skipped)}))
    for t, o in targets.items():
        for s in o["searches"]:
            print(t, o["label"], s.get("budget"), s.get("seed"), s.get("top"), s.get("top_visits"),
                  s.get("avoids_misplay"), s.get("matches_right_call"), s.get("error", ""))
    return 0


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=["job", "check"])
    ap.add_argument("--engine", type=Path, required=True)
    ap.add_argument("--spec", type=Path, required=True)
    ap.add_argument("--out", type=Path)
    ap.add_argument("--rows", type=Path)
    ap.add_argument("--json", type=Path)
    a = ap.parse_args(argv)
    return job(a) if a.command == "job" else check(a)


if __name__ == "__main__":
    raise SystemExit(main())
