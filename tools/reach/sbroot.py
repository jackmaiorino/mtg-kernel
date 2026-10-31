"""Search on recorded human-game misplays (mode `s4a-sbroot`). Measurement only.

usage:
  sbroot.py check --engine ENGINE.jsonl[.gz] --spec SPEC.json --capture REPORT.json --rows-dir DIR [--json OUT]

`sbcapture.py` writes the jobs: the native steps of a recorded Spellbench
pauper-v2 game and one job per decision point of each target. SPEC names the
target wire decisions (indices over both seats) with, for each, what the
model played (the misplay) and the play the human reviewer called right, as
predicates over candidate descriptions (`describe`).

`check` reads the mode's rows for every decision point. It confirms the
rebuild: no step's actor or row kinds differed, every applied step's rows
name the same cards as the bridge's candidates, and at every earlier decision
of the model's seat its view (own hand, both battlefields with tapped state,
graveyards, life, hand and library sizes) equals the record's observation.
It then reads each search: the most visited row, the wire candidates behind
it, its visits and value, the whole visit distribution, whether it avoids the
misplay and whether it is the play the reviewer called right (None when the
wire candidates behind that row disagree).
"""
from __future__ import annotations

import argparse
import collections
import gzip
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


def matches(pred: dict, d: str) -> bool:
    """A spec predicate over a candidate description: every given field must hold."""
    if "describe" in pred and d != pred["describe"]:
        return False
    if "describe_prefix" in pred and not d.startswith(pred["describe_prefix"]):
        return False
    if pred.get("not"):
        return not matches(pred["not"], d)
    return True


def verdict(pred: dict, labels: list[str]):
    """True or False when every wire candidate behind a native row agrees, else None."""
    got = {matches(pred, d) for d in labels}
    return got.pop() if len(got) == 1 else None


def check(a) -> int:
    """Check native-step rebuilds (`sbcapture.py` report and per-point jobs) and read their searches.

    Rows for decision point KEY are read from `--rows-dir`/rows-<game>-<KEY>.jsonl."""
    reset, decisions, steps = wire(a.engine)
    spec = json.loads(a.spec.read_text(encoding="utf-8"))
    cap = json.loads(a.capture.read_text(encoding="utf-8"))
    native = cap["native"]
    model = spec["model_seat"]
    swap = cap["wire_starting_seat"] != "p0"
    to_native = (lambda p: 1 - p) if swap else (lambda p: p)
    posed = {int(k): v for k, v in cap["posed"].items()}
    problems, out_targets, searched = [], {}, 0
    for t, sp in spec["targets"].items():
        ct = cap["targets"][t]
        out = {"i": int(t), "label": sp["label"], "right_call": sp.get("right_text"),
               "status": ct.get("status"), "played": ct.get("played"), "points": []}
        for pt in ct.get("points", []):
            path = a.rows_dir / f"rows-{cap['game_id']}-{pt['key']}.jsonl"
            res = {"key": pt["key"], "native": pt["native"], "on_recorded_line": pt["on_recorded_line"],
                   "row_labels": pt["row_labels"], "status": None, "searches": []}
            out["points"].append(res)
            if not path.exists():
                res["status"] = "not run"
                continue
            rows = list(read_lines(path))
            card = next(r["names"] for r in rows if r["kind"] == "card_names")
            facts = {}
            for r in rows:
                if r["kind"] == "mismatch":
                    problems.append({"key": pt["key"], "i": r["i"], "what": "kinds or actor",
                                     "record": r["record_kinds"], "main": [x["kind"] for x in r["main"]["rows"]]})
                if r["kind"] == "target":
                    res["status"] = r["status"]
                if r["kind"] != "step" or r["status"] != "applied":
                    continue
                facts[r["i"]] = r["main"]["facts"]
                want_rows = pt["names"] if r["i"] == pt["native"] else native[r["i"]]["names"]
                if r["i"] > pt["native"] or (not pt["on_recorded_line"] and r["i"] >= ct["native_posed_at"]
                                             and r["i"] != pt["native"]):
                    continue
                for k, want_names in enumerate(want_rows):
                    want = collections.Counter(want_names)
                    got = collections.Counter(card[c] for c in r["main"]["rows"][k]["cards"])
                    if want != got:
                        problems.append({"key": pt["key"], "i": r["i"], "row": k, "what": "row names",
                                         "record": sorted(want.elements()), "main": sorted(got.elements())})
            for i, d in enumerate(decisions):
                n = posed.get(i)
                if int(d["acting_seat"][1:]) != model or n is None or n not in facts or n > ct["native_posed_at"]:
                    continue
                got = seat_view(facts[n], to_native(model), card)
                got = {(1 - p if swap else p): v for p, v in got.items()}
                want = record_view(d["observation"], model)
                if json.dumps(want, sort_keys=True) != json.dumps(got, sort_keys=True):
                    problems.append({"key": pt["key"], "i": i, "native": n, "what": "model view",
                                     "record": want, "main": got})
            res["views_checked"] = sum(1 for i, d in enumerate(decisions) if int(d["acting_seat"][1:]) == model
                                       and posed.get(i) in facts and posed[i] <= ct["native_posed_at"])
            for r in rows:
                if r.get("i") != pt["native"] or r["kind"] not in ("search", "error"):
                    continue
                if r["kind"] == "error":
                    res["searches"].append({"budget": r["budget"], "seed": r["seed"], "error": r["error"]})
                    continue
                o2 = r["o2"]
                edges = o2.get("edges") or []
                top = o2.get("top_edge")
                n = [e[0] for e in edges]
                q = [(e[1] + 0.5 * e[2]) / e[0] if e[0] else None for e in edges]
                labels = pt["row_labels"].get(str(top), ["unlabelled"]) if top is not None else []
                res["searches"].append({
                    "budget": r["budget"], "seed": r["seed"], "top_row": top, "top": labels,
                    "top_visits": n[top] if top is not None else None, "visits": n,
                    "top_q": q[top] if top is not None else None, "q": q,
                    "avoids_misplay": None if top is None else
                        (lambda v: None if v is None else not v)(verdict(sp["misplay"], labels)),
                    "matches_right_call": None if top is None or "right" not in sp else verdict(sp["right"], labels),
                    "simulations": r["selection"].get("completed_natural"), "cpu_seconds": r["cpu_seconds"]})
                searched += 1
            res["status"] = res["status"] or ("searched" if res["searches"] else "not reached")
        out_targets[t] = out
    result = {"game_id": reset["game_id"], "rebuild_exact": not problems, "problems": problems[:20],
              "problem_count": len(problems), "searches": searched, "targets": out_targets}
    if a.json:
        a.json.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")
    print(json.dumps({k: result[k] for k in ("game_id", "rebuild_exact", "problem_count", "searches")}))
    for t, o in out_targets.items():
        for pt in o["points"]:
            for s in pt["searches"]:
                print(t, pt["key"], s.get("budget"), s.get("seed"), s.get("top"), s.get("top_visits"),
                      s.get("avoids_misplay"), s.get("matches_right_call"), s.get("error", ""))
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


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=["check"])
    ap.add_argument("--engine", type=Path, required=True)
    ap.add_argument("--spec", type=Path, required=True)
    ap.add_argument("--json", type=Path)
    ap.add_argument("--capture", type=Path)
    ap.add_argument("--rows-dir", type=Path)
    a = ap.parse_args(argv)
    return check(a)


if __name__ == "__main__":
    raise SystemExit(main())
