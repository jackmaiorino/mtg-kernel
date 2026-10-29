"""Capture plans for non-Spy focal games of an existing OH2 panel subject (measurement only).

usage: capture_plan.py --eval DIR --subject r2-s1-b512 --opponents Ha,Hb,R3,T1,TP --out DIR
                       [--map OLD=NEW ...]

Declared before any game is played (collab spy-primary-direction DECISION section 10):
for each own deck other than Spy, for each opponent panel, the 8 cases with
repeat 0 and every other deck (no mirror), focal seat (own + other) % 2. That is
40 games per deck and 320 in all. Seeds, starting players, sources and the
runtime profile are the subject's existing panels' own (panel.py rules and seed
namespaces, checked against each plan's case list), so each capture replays a
scheduled panel case, now with every decision recorded. Requests follow
panel.py (best-of-three, game one read, capture_decisions on); plan.json is in
panel.py's schema so `panel.py jobs` turns the captures into label jobs.
--map rewrites moved input paths; every pinned path must still match its hash.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "tools" / "spy_reset"))
sys.path.insert(0, str(REPO / "python" / "tools"))
import nine_deck_baseline_v1 as ndb  # noqa: E402
import nine_deck_public_panel_v1 as npp  # noqa: E402
import panel  # noqa: E402
import pilot  # noqa: E402


def remap(value, mapping):
    if isinstance(value, dict):
        out = {k: remap(v, mapping) for k, v in value.items()}
        if isinstance(out.get("path"), str) and isinstance(out.get("sha256"), str):
            path = Path(out["path"])
            if hashlib.sha256(path.read_bytes()).hexdigest() != out["sha256"]:
                raise ValueError(f"{path} differs from its pinned sha256")
        return out
    if isinstance(value, list):
        return [remap(v, mapping) for v in value]
    if isinstance(value, str):
        for old, new in mapping:
            if value.startswith(old):
                return new + value[len(old):]
    return value


def subset(namespace: str) -> list[dict]:
    return [{"own": o, "other": t, "repeat": 0, "seat": (o + t) % 2, "seed": pilot.panel_seed(o, t, 0, namespace),
             "starting_player": (o + t) % 2}
            for o in range(pilot.DECKS) if o != pilot.SPY for t in range(pilot.DECKS) if t != o]


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--eval", type=Path, required=True, help="the subject's panel eval directory")
    ap.add_argument("--subject", required=True)
    ap.add_argument("--opponents", required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--map", action="append", default=[], help="OLD=NEW path prefix rewrite")
    a = ap.parse_args(argv)
    mapping = [tuple(m.split("=", 1)) for m in a.map]
    decks = ndb.load_decks(REPO / "data" / "runtime_decks_v1.json")
    total = 0
    for name in a.opponents.split(","):
        source = json.loads((a.eval / f"{a.subject}-vs-{name}" / "plan.json").read_text(encoding="utf-8"))
        namespace = source.get("seed_namespace", pilot.PANEL_NS)
        cases = subset(namespace)
        recorded = {(c["own"], c["other"], c["repeat"], c["seat"]): c for item in source["requests"] for c in item["cases"]}
        for case in cases:
            if recorded[(case["own"], case["other"], case["repeat"], case["seat"])] != case:
                raise ValueError(f"{name}: case {case} differs from the panel's schedule")
        candidate, opponent = remap(source["candidate"], mapping), remap(source["opponent"], mapping)
        out = a.out / f"{a.subject}-vs-{name}"
        requests = []
        for seat in (0, 1):
            part = [c for c in cases if c["seat"] == seat]
            sources = [panel.wrap(candidate), opponent] if seat == 0 else [opponent, panel.wrap(candidate)]
            request_name = f"seat{seat}-capture"
            request = {"sources": sources, "matches": [panel.case_match(c, decks) for c in part],
                       "cross_generation_evaluation": True, "capture_decisions": True,
                       "output_directory": str(out / "native" / request_name).replace("\\", "/")}
            requests.append({"name": request_name, "seat": seat, "cases": part,
                             "request": npp.write_json(out / "requests" / f"{request_name}.json", request)})
        plan = {"schema": panel.SCHEMA, "panel": f"{a.subject}-vs-{name}", "checkpoint_id": a.subject,
                "opponent_name": name, "candidate": candidate, "opponent": opponent,
                "runtime_rules": pilot.RUNTIME_RULES, "seed_namespace": namespace, "capture_all": True,
                "cases": len(cases), "requests": requests,
                "selection": "repeat 0, every other deck, focal seat (own + other) % 2, Spy excluded"}
        npp.write_json(out / "plan.json", plan)
        total += len(cases)
        print(json.dumps({"panel": plan["panel"], "cases": len(cases), "namespace": namespace}))
    print(json.dumps({"cases": total}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
