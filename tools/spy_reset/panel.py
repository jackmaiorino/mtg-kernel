"""Fresh raw-policy panel for the reset pilot (RUNNER.md section 7).

Each checkpoint pilots all nine own decks against all nine other decks, both
seats, 12 repeats (1,944 cases) against one fixed opponent (T1 or H), through
the public evaluator with the baseline P2 semantics: best-of-three matches,
keep-seven opening, 10,000 physical decisions, game one read, the case's
starting player choosing play. The only differences from P2 are the fresh
seed namespace (``spy-reset-pilot-v1/eval``) and the common runtime profile
(resolution-boundary-v1) on every game. Search is never used.

Spy-own cases (216 per checkpoint and opponent) run in their own shards with
captured decisions, so ``labels`` can replay their game one through the
Stage 4a resolution labels (``regret_census_v1 mode=s4a-label-replay``).

Execution, qualification, recovery and rows reuse
python/tools/nine_deck_public_panel_v1.py unchanged (inside a held host
reservation).

CLI:
  build   --id ID --candidate SRC.json --opponent-name T1|H --opponent OPP.json --decks DECKS --out DIR --shards N
  jobs    --plan DIR/plan.json --out JOBS.jsonl     label-replay jobs for Spy-own cases
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "python" / "tools"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
import nine_deck_public_panel_v1 as npp  # noqa: E402
import pilot  # noqa: E402

SCHEMA = "spy-reset-pilot-v1/public-panel"
MAX_DECISIONS = npp.MAX_DECISIONS


def cases() -> list[dict]:
    return [{"own": o, "other": t, "repeat": k, "seat": seat, "seed": pilot.panel_seed(o, t, k),
             "starting_player": (k + o + t) % 2}
            for o in range(pilot.DECKS) for t in range(pilot.DECKS)
            for k in range(pilot.PANEL_REPEATS) for seat in (0, 1)]


def case_match(case: dict, decks: list[dict]) -> dict:
    match = npp.case_match(case, decks)
    match["config"]["runtime_rules"] = pilot.RUNTIME_RULES
    return match


def wrap(source: dict) -> dict:
    """A plain model source played through the Legacy kind, as P2 played candidates."""
    return npp.candidate_source(source)


def build(checkpoint_id: str, candidate: dict, opponent_name: str, opponent: dict, decks: list[dict],
          out: Path, shards: int) -> dict:
    out = Path(out)
    all_cases = cases()
    requests = []
    for seat in (0, 1):
        for spy, count in ((True, 2), (False, shards)):
            part_cases = [c for c in all_cases if c["seat"] == seat and (c["own"] == pilot.SPY) == spy]
            size = -(-len(part_cases) // count)
            for index in range(count):
                part = part_cases[index * size:(index + 1) * size]
                if not part:
                    continue
                if len(part) > npp.MAX_MATCHES_PER_REQUEST:
                    raise ValueError("shard exceeds the evaluator's match bound")
                sources = [wrap(candidate), opponent] if seat == 0 else [opponent, wrap(candidate)]
                name = f"seat{seat}-{'spy' if spy else 'main'}{index:02d}"
                request = {"sources": sources, "matches": [case_match(c, decks) for c in part],
                           "cross_generation_evaluation": True, "capture_decisions": spy,
                           "output_directory": str(out / "native" / name).replace("\\", "/")}
                requests.append({"name": name, "seat": seat, "cases": part,
                                 "request": npp.write_json(out / "requests" / f"{name}.json", request)})
    plan = {"schema": SCHEMA, "panel": f"{checkpoint_id}-vs-{opponent_name}", "checkpoint_id": checkpoint_id,
            "opponent_name": opponent_name, "candidate": candidate, "opponent": opponent,
            "runtime_rules": pilot.RUNTIME_RULES, "cases": len(all_cases), "requests": requests,
            "schedule_sha256": npp.sha256_bytes(json.dumps(
                {"cases": all_cases, "opponent": opponent, "max_decisions": MAX_DECISIONS,
                 "runtime_rules": pilot.RUNTIME_RULES}, sort_keys=True).encode())}
    npp.write_json(out / "plan.json", plan)
    return plan


def label_jobs(plan_path: Path) -> list[dict]:
    """One job per completed Spy-own case: its game one, both seats' recorded actions."""
    plan = json.loads(Path(plan_path).read_text(encoding="utf-8"))
    jobs = []
    for item in plan["requests"]:
        request = json.loads(npp.checked(item["request"]).read_text(encoding="utf-8"))
        if not request["capture_decisions"]:
            continue
        recovery = npp.checked(item["request"]).with_suffix(".recovery.json")
        if recovery.exists():
            segments = [(Path(s["output_directory"]), s["start"])
                        for s in json.loads(recovery.read_text(encoding="utf-8"))["segments"]]
        else:
            segments = [(Path(request["output_directory"]), 0)]
        for output, start in segments:
            for offset, path in enumerate(sorted(output.glob("match-*.json"))):
                index = start + offset
                case, match_request = item["cases"][index], request["matches"][index]
                record = json.loads(path.read_text(encoding="utf-8"))
                if record["match"]["config"]["seed"] != case["seed"]:
                    raise ValueError("match order differs from the plan")
                game = record["games"][0]
                rows = [r for r in record["decisions"] if r["game_index"] == 1]
                jobs.append({
                    "id": f"{plan['panel']}/o{case['own']}-t{case['other']}-k{case['repeat']}-s{case['seat']}",
                    "config": match_request["config"], "uses_v3": True, "game_index": 1,
                    "environment_seed": game["environment_seed"],
                    "starting_player": game["start"]["starting_player"],
                    "mainboards": [match_request["registered"][0]["mainboard"],
                                   match_request["registered"][1]["mainboard"]],
                    "focal": case["seat"],
                    "decisions": [{"step": r["step"], "physical_decision_id": r["physical_decision_id"],
                                   "actor": 0 if r["actor"] in (0, "p0", "P0") else 1,
                                   "legal_action_count": r["legal_action_count"], "selected": r["selected"]}
                                  for r in rows],
                    "winner": game.get("winner")})
    return jobs


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    b = sub.add_parser("build")
    for name in ("--id", "--opponent-name"):
        b.add_argument(name, required=True)
    for name in ("--candidate", "--opponent", "--decks", "--out"):
        b.add_argument(name, type=Path, required=True)
    b.add_argument("--shards", type=int, required=True)
    j = sub.add_parser("jobs")
    j.add_argument("--plan", type=Path, required=True)
    j.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    if a.op == "build":
        plan = build(a.id, json.loads(a.candidate.read_text()), a.opponent_name, json.loads(a.opponent.read_text()),
                     json.loads(a.decks.read_text()), a.out, a.shards)
        print(json.dumps({"requests": len(plan["requests"]), "cases": plan["cases"]}))
    else:
        jobs = label_jobs(a.plan)
        a.out.write_text("".join(json.dumps(x, separators=(",", ":")) + "\n" for x in jobs), encoding="utf-8")
        print(json.dumps({"jobs": len(jobs)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
