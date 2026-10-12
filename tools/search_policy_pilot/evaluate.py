"""Fresh raw-policy evaluation panels for the E supported-choice CE pilot (DESIGN.md "Readouts").

Each checkpoint plays all 81 ordered deck pairs x four new seed families x both focal seats (648
game-one cases) against each frozen reference (T1 and H), through the public evaluator with the
baseline semantics (best-of-three, keep-seven, game one read, search and resets off), in this
pilot's own seed namespace. Execution, qualification, recovery and rows reuse
python/tools/nine_deck_public_panel_v1.py unchanged, inside a held host reservation.

CLI:
  build  --id ID --candidate SRC.json --opponent-name T1|H --opponent OPP.json --out DIR --shards N
  qual   --plan DIR/plan.json --matches 16 --workers 1,2,4,8 --out QDIR --evaluator EXE
  run    --plan DIR/plan.json --receipt QDIR/receipt.json --evaluator EXE
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
sys.path.insert(0, str(REPO / "python" / "tools"))
sys.path.insert(0, str(HERE))
import nine_deck_baseline_v1 as ndb  # noqa: E402
import nine_deck_public_panel_v1 as npp  # noqa: E402
import schedule  # noqa: E402

SCHEMA = f"{schedule.NS}/public-panel"
FAMILIES = 4


def cases() -> list[dict]:
    return [{"own": o, "other": t, "repeat": k, "seat": seat,
             "seed": schedule.seed("evaluation", own=o, other=t, family=k), "starting_player": (k + o + t) % 2}
            for o in range(schedule.DECKS) for t in range(schedule.DECKS) for k in range(FAMILIES)
            for seat in (0, 1)]


def case_match(case: dict, decks: list[dict]) -> dict:
    match = npp.case_match(case, decks)
    match["config"]["runtime_rules"] = schedule.RUNTIME_RULES
    return match


def build(checkpoint_id: str, candidate: dict, opponent_name: str, opponent: dict, decks: list[dict], out: Path,
          shards: int) -> dict:
    out = Path(out)
    all_cases = cases()
    requests = []
    for seat in (0, 1):
        part_cases = [c for c in all_cases if c["seat"] == seat]
        size = -(-len(part_cases) // shards)
        for index in range(shards):
            part = part_cases[index * size:(index + 1) * size]
            if not part:
                continue
            if len(part) > npp.MAX_MATCHES_PER_REQUEST:
                raise ValueError("shard exceeds the evaluator's match bound")
            sources = ([npp.candidate_source(candidate), opponent] if seat == 0
                       else [opponent, npp.candidate_source(candidate)])
            name = f"seat{seat}-{index:02d}"
            request = {"sources": sources, "matches": [case_match(c, decks) for c in part],
                       "cross_generation_evaluation": True, "capture_decisions": False,
                       "output_directory": str(out / "native" / name).replace("\\", "/")}
            requests.append({"name": name, "seat": seat, "cases": part,
                             "request": npp.write_json(out / "requests" / f"{name}.json", request)})
    plan = {"schema": SCHEMA, "panel": f"{checkpoint_id}-vs-{opponent_name}", "checkpoint_id": checkpoint_id,
            "opponent_name": opponent_name, "candidate": candidate, "opponent": opponent,
            "runtime_rules": schedule.RUNTIME_RULES, "cases": len(all_cases), "requests": requests,
            "schedule_sha256": npp.sha256_bytes(json.dumps(
                {"cases": all_cases, "opponent": opponent, "max_decisions": npp.MAX_DECISIONS,
                 "runtime_rules": schedule.RUNTIME_RULES}, sort_keys=True).encode())}
    npp.write_json(out / "plan.json", plan)
    return plan


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    b = sub.add_parser("build")
    for name in ("--id", "--opponent-name"):
        b.add_argument(name, required=True)
    for name in ("--candidate", "--opponent", "--out"):
        b.add_argument(name, type=Path, required=True)
    b.add_argument("--shards", type=int, required=True)
    q = sub.add_parser("qual")
    for name in ("--plan", "--out", "--evaluator"):
        q.add_argument(name, type=Path, required=True)
    q.add_argument("--matches", type=int, required=True)
    q.add_argument("--workers", required=True)
    r = sub.add_parser("run")
    for name in ("--plan", "--receipt", "--evaluator"):
        r.add_argument(name, type=Path, required=True)
    a = ap.parse_args(argv)
    if a.op == "build":
        decks = ndb.load_decks(REPO / "data" / "runtime_decks_v1.json")
        plan = build(a.id, json.loads(a.candidate.read_text()), a.opponent_name, json.loads(a.opponent.read_text()),
                     decks, a.out, a.shards)
        print(json.dumps({"requests": len(plan["requests"]), "cases": plan["cases"]}))
    elif a.op == "qual":
        trials = npp.qualify(a.plan, a.matches, [int(w) for w in a.workers.split(",")], a.out, str(a.evaluator))
        print(json.dumps({"selected_workers": trials["selected_workers"],
                          "trials": [{k: t[k] for k in ("workers", "seconds", "parity")} for t in trials["trials"]]}))
    else:
        result = npp.run(a.plan, a.receipt, str(a.evaluator))
        rows = npp.rows(a.plan)
        out = Path(a.plan).parent / "rows.jsonl"
        out.write_text("".join(json.dumps(x, sort_keys=True) + "\n" for x in rows))
        print(json.dumps({"rows": len(rows), "complete": sum(x["complete"] for x in rows),
                          "workers": result.get("workers")}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
