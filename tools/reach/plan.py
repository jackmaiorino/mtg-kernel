"""RS0 root inventory (collab LANES/reverse-search-20261029/RS0-PLAN.md section 3). Measurement only.

usage: plan.py --spy-rows DIR --other-rows DIR --spy-jobs JOBS.jsonl... --other-jobs JOBS.jsonl... --out DIR

Reads step 1 win-check rows (`s4a-win-check` output) and the label jobs they
were checked from, and writes:
  roots.jsonl     one line per searched root, each carrying its label job;
  inventory.json  the selection, its eligible counts and every source hash.

The selection reads the rows alone (verdicts, lines, turns, actors); the jobs
are attached afterwards and only for reconstruction. Nothing here chooses,
samples, trains or selects anything outside RS0's measurement.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "win_check"))
from summarize import classify  # noqa: E402  (the step 1 class rule, unchanged)

SPY_GAMES = 36
BURN_GAMES = 12
CONTROL_PER_DECK = 3
PLAN = "rs0-plan-v1"


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def game_key(game_id: str) -> str:
    return hashlib.sha256(game_id.encode("utf-8")).hexdigest()


def control_key(game_id: str, index: int) -> str:
    return hashlib.sha256(json.dumps([game_id, index]).encode("utf-8")).hexdigest()


def read_rows(paths: list[Path]) -> tuple[dict, dict]:
    """Game rows and decision rows by game id (error rows mean no game row)."""
    games, decisions = {}, defaultdict(list)
    for path in paths:
        for line in path.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            row = json.loads(line)
            if row["kind"] == "game":
                games[row["id"]] = row
            elif row["kind"] == "decision":
                decisions[row["id"]].append(row)
    for rows in decisions.values():
        rows.sort(key=lambda r: r["index"])
    return games, decisions


def deck_of(game: dict) -> str:
    return game["deck_ids"][game["focal"]].split("/")[0]


def proof_line(row: dict) -> list[str]:
    robust = row.get("robust") or {}
    return robust.get("line") or row["oracle"].get("line") or []


def opportunity(game: dict, rows: list[dict], cls: str) -> dict | None:
    """The first robust opportunity turn of `cls`, its certified decision and roots k0..k2."""
    deck = deck_of(game)
    proven = defaultdict(list)
    for r in rows:
        if (r.get("robust") or {}).get("verdict") == "proven":
            proven[r["turn"]].append(r)
    turn = next((t for t in sorted(proven) if classify(proof_line(proven[t][0]), deck) == cls), None)
    if turn is None:
        return None
    hero_turns = sorted({r["turn"] for r in rows if r["active"] == r["focal"]})
    if turn not in hero_turns or hero_turns.index(turn) < 2:
        return None
    position = hero_turns.index(turn)
    first_hero = {t: min(r["index"] for r in rows if r["turn"] == t and r["active"] == r["focal"]) for t in hero_turns}
    roots = {f"k{k}": (first_hero[hero_turns[position - k]], hero_turns[position - k]) for k in range(3)}
    certified = proven[turn][0]["index"]
    return {"turn": turn, "certified": certified, "roots": roots}


def select_class(games: dict, decisions: dict, cls: str, deck: str | None, count: int, stratum: str) -> tuple[list, dict]:
    eligible = []
    for gid, game in games.items():
        if deck is not None and deck_of(game) != deck:
            continue
        opp = opportunity(game, decisions.get(gid, []), cls)
        if opp is not None:
            eligible.append((game_key(gid), gid, opp))
    eligible.sort()
    picks = []
    for rank, (_, gid, opp) in enumerate(eligible):
        roles = [(role, index, turn) for role, (index, turn) in opp["roots"].items()]
        if opp["certified"] != opp["roots"]["k0"][0]:
            roles.append(("cert", opp["certified"], opp["turn"]))
        picks.append({"stratum": stratum, "game_id": gid, "rank": rank, "selected": rank < count,
                      "opportunity_turn": opp["turn"], "certified_index": opp["certified"], "roles": roles})
    return picks, {"eligible": len(eligible), "selected": min(count, len(eligible))}


def select_control(sources: list[tuple[dict, dict]]) -> list:
    by_deck = defaultdict(list)
    for games, decisions in sources:
        for gid, game in games.items():
            for r in decisions.get(gid, []):
                by_deck[deck_of(game)].append((control_key(gid, r["index"]), gid, r["index"], r["turn"]))
    picks = []
    for deck in sorted(by_deck):
        seen, ranked = set(), sorted(by_deck[deck])
        for rank, (_, gid, index, turn) in enumerate(ranked):
            if gid in seen:
                continue
            seen.add(gid)
            picks.append({"stratum": "control", "deck": deck, "game_id": gid, "rank": len(seen) - 1,
                          "selected": len(seen) <= CONTROL_PER_DECK, "roles": [("ctrl", index, turn)]})
            if len(seen) >= CONTROL_PER_DECK + 3:  # three spares per deck for replay refusals
                break
    return picks


def read_jobs(paths: list[Path]) -> dict:
    jobs = {}
    for path in paths:
        for line in path.read_text(encoding="utf-8").splitlines():
            if line.strip():
                job = json.loads(line)
                if job["id"] in jobs:
                    raise SystemExit(f"duplicate label job {job['id']}")
                jobs[job["id"]] = job
    return jobs


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--spy-rows", type=Path, required=True)
    ap.add_argument("--other-rows", type=Path, required=True)
    ap.add_argument("--spy-jobs", type=Path, nargs="*", default=[])
    ap.add_argument("--other-jobs", type=Path, nargs="*", default=[])
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--dry-run", action="store_true", help="selection and counts only; no jobs needed")
    a = ap.parse_args(argv)
    spy_files = sorted(a.spy_rows.glob("rows-*.jsonl"))
    other_files = sorted(a.other_rows.glob("rows-*.jsonl"))
    spy, other = read_rows(spy_files), read_rows(other_files)
    spy_picks, spy_counts = select_class(*spy, "spy_line", "Spy", SPY_GAMES, "spy")
    burn_picks, burn_counts = select_class(*other, "burn_lethal", "Burn", BURN_GAMES, "burn")
    control = select_control([spy, other])
    picks = spy_picks + burn_picks + control
    sources = {str(p): sha256_file(p) for p in spy_files + other_files + list(a.spy_jobs) + list(a.other_jobs)}
    inventory = {"plan": PLAN, "spy": spy_counts, "burn": burn_counts,
                 "control_selected": sum(p["selected"] for p in control), "sources": sources,
                 "games": [{k: v for k, v in p.items() if k != "roles"} | {"roles": [list(r) for r in p["roles"]]} for p in picks]}
    a.out.mkdir(parents=True, exist_ok=True)
    (a.out / "inventory.json").write_text(json.dumps(inventory, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    selected = [p for p in picks if p["selected"]]
    print(json.dumps({"spy": spy_counts, "burn": burn_counts, "control": inventory["control_selected"],
                      "roots": len({(p["game_id"], r[1]) for p in selected for r in p["roles"]})}))
    if a.dry_run:
        return 0
    jobs = read_jobs(a.spy_jobs + a.other_jobs)
    lines, merged = [], {}
    for p in picks:  # spares included; the runner takes replacements in rank order
        job = jobs.get(p["game_id"])
        if job is None:
            if p["selected"]:
                raise SystemExit(f"no label job for selected game {p['game_id']}")
            continue
        for role, index, turn in p["roles"]:
            key = (p["game_id"], index)
            entry = merged.setdefault(key, {"root_id": f"{p['game_id']}#{index}", "game_id": p["game_id"], "index": index,
                                            "turn": turn, "roles": [], "job": job})
            entry["roles"].append({"stratum": p["stratum"], "role": role, "rank": p["rank"], "selected": p["selected"],
                                   "opportunity_turn": p.get("opportunity_turn"), "certified_index": p.get("certified_index")})
    for entry in merged.values():
        lines.append(json.dumps(entry, sort_keys=True))
    (a.out / "roots.jsonl").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
