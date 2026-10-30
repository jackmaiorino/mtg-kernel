"""RS0 root inventory (collab LANES/reverse-search-20261029/RS0-PLAN.md section 3). Measurement only.

usage: plan.py --spy-rows DIR --other-rows DIR --spy-jobs JOBS.jsonl... --other-jobs JOBS.jsonl...
               [--refused REACH_ROWS.jsonl...] --out DIR [--dry-run]

Reads step 1 win-check rows (`s4a-win-check` output) and the label jobs they
were checked from, and writes:
  roots.jsonl     one line per selected root, each carrying its label job;
  inventory.json  the selection, refusals, eligible counts and every source and tool hash.

Replacement (RS0-PLAN.md section 3): pass the `s4a-reach` output rows and the
inventory.json of every earlier round with --refused (refusals are cumulative);
refused games (or control roots) are skipped and the next
eligible candidates in hash order are selected. Searches are deterministic,
so a row is reused only when the roles, T and c it recorded equal
the current root entry's (tools/reach/report.py marks any other row stale).

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
PLAN = "rs0-plan-v4"


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def game_key(game_id: str) -> str:
    return hashlib.sha256(game_id.encode("utf-8")).hexdigest()


def control_key(game_id: str, index: int) -> str:
    return hashlib.sha256(json.dumps([game_id, index]).encode("utf-8")).hexdigest()


def read_rows_from(rows) -> tuple[dict, dict]:
    """Game rows and decision rows by game id (error rows mean no game row)."""
    games, decisions = {}, defaultdict(list)
    for row in rows:
        if row["kind"] == "game":
            games[row["id"]] = row
        elif row["kind"] == "decision":
            decisions[row["id"]].append(row)
    for found in decisions.values():
        found.sort(key=lambda r: r["index"])
    return games, decisions


def read_rows(paths: list[Path]) -> tuple[dict, dict]:
    return read_rows_from(json.loads(line) for path in paths
                          for line in path.read_text(encoding="utf-8").splitlines() if line.strip())


def deck_of(game: dict) -> str:
    return game["deck_ids"][game["focal"]].split("/")[0]


def proof_line(row: dict) -> list[str]:
    """The robust line (a proven robust result always carries one)."""
    return row["robust"]["line"]


def opportunity(game: dict, rows: list[dict], cls: str) -> dict | None:
    """The first robust opportunity turn T of `cls`, its certified decision, and roots k0..k2.

    Root k is the first recorded focal decision of turn T - 2k (the hero's
    immediately preceding turns: turns alternate seats); a game whose turn
    T - 2k holds no focal decision with the hero active is not eligible.
    """
    deck = deck_of(game)
    proven = defaultdict(list)
    for r in rows:
        if (r.get("robust") or {}).get("verdict") == "proven":
            proven[r["turn"]].append(r)
    turn = next((t for t in sorted(proven) if classify(proof_line(proven[t][0]), deck) == cls), None)
    if turn is None:
        return None
    roots = {}
    for k in range(3):
        t = turn - 2 * k
        hero = [r["index"] for r in rows if r["turn"] == t and r["active"] == r["focal"]]
        if not hero:
            return None
        roots[f"k{k}"] = (min(hero), t)
    return {"turn": turn, "certified": proven[turn][0]["index"], "roots": roots}


def select_class(games: dict, decisions: dict, cls: str, deck: str, count: int, stratum: str,
                 refused: dict) -> tuple[list, dict]:
    """Eligible games in hash order; the first `count` not refused are selected."""
    eligible = []
    for gid, game in games.items():
        if deck_of(game) != deck:
            continue
        opp = opportunity(game, decisions.get(gid, []), cls)
        if opp is not None:
            eligible.append((game_key(gid), gid, opp))
    eligible.sort()
    picks, taken = [], 0
    for rank, (_, gid, opp) in enumerate(eligible):
        roles = [(role, index, turn) for role, (index, turn) in opp["roots"].items()]
        if opp["certified"] != opp["roots"]["k0"][0]:
            roles.append(("cert", opp["certified"], opp["turn"]))
        if gid in refused:
            status = "refused"
        elif taken < count:
            status, taken = "selected", taken + 1
        else:
            break
        picks.append({"stratum": stratum, "game_id": gid, "rank": rank, "status": status,
                      "refusal": refused.get(gid), "opportunity_turn": opp["turn"],
                      "certified_index": opp["certified"], "roles": roles})
    return picks, {"eligible": len(eligible), "selected": taken}


def select_control(sources: list[tuple[dict, dict]], refused: dict) -> tuple[list, dict]:
    """Per deck, hero-turn decisions in key order; a refused candidate is skipped
    (its game stays available); at most one accepted root per game."""
    by_deck = defaultdict(list)
    for games, decisions in sources:
        for gid, game in games.items():
            for r in decisions.get(gid, []):
                if r["active"] == r["focal"]:
                    by_deck[deck_of(game)].append((control_key(gid, r["index"]), gid, r["index"], r["turn"]))
    picks, candidates = [], {}
    for deck in sorted(by_deck):
        ranked = sorted(by_deck[deck])
        candidates[deck] = len(ranked)
        used = set()
        for rank, (_, gid, index, turn) in enumerate(ranked):
            if len(used) >= CONTROL_PER_DECK:
                break
            if gid in used:
                continue
            key = f"{gid}#{index}"
            # A control root is refused by itself; a game without a label job is
            # refused whole (an opportunity game's replay refusal is not).
            reason = refused.get(key) or (refused.get(gid) if refused.get(gid) == "missing_job" else None)
            if reason:
                picks.append({"stratum": "control", "deck": deck, "game_id": gid, "rank": rank, "status": "refused",
                              "refusal": reason, "roles": [("ctrl", index, turn)]})
                continue
            used.add(gid)
            picks.append({"stratum": "control", "deck": deck, "game_id": gid, "rank": rank, "status": "selected",
                          "refusal": None, "roles": [("ctrl", index, turn)]})
    return picks, candidates


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


def read_refusals(paths: list[Path]) -> dict:
    """Replay refusals, cumulative over every earlier round (RS0-PLAN.md
    section 3): `s4a-reach` error rows without a budget (root rebuild errors;
    an opportunity game is refused whole, a control root by its game and
    index), and earlier `inventory.json` files, whose refusals carry over."""
    refused = {}
    for path in paths:
        text = path.read_text(encoding="utf-8")
        if path.suffix == ".json":
            for r in json.loads(text).get("refusals", []):
                if r["reason"] != "missing_job":
                    key = r["root_id"] if r["stratum"] == "control" else r["game_id"]
                    refused[key] = r["reason"]
            continue
        for line in text.splitlines():
            if not line.strip():
                continue
            row = json.loads(line)
            if row.get("kind") != "error" or "budget" in row:
                continue
            for role in row.get("roles", []):
                if role["stratum"] == "control":
                    refused[row["root_id"]] = f"replay: {row['error']}"
                else:
                    refused[row["game_id"]] = f"replay: {row['error']}"
    return refused


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--spy-rows", type=Path, required=True)
    ap.add_argument("--other-rows", type=Path, required=True)
    ap.add_argument("--spy-jobs", type=Path, nargs="*", default=[])
    ap.add_argument("--other-jobs", type=Path, nargs="*", default=[])
    ap.add_argument("--refused", type=Path, nargs="*", default=[],
                    help="every earlier round's s4a-reach output rows and inventory.json (cumulative)")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--dry-run", action="store_true", help="selection and counts only; jobs assumed present")
    a = ap.parse_args(argv)
    spy_files = sorted(a.spy_rows.glob("rows-*.jsonl"))
    other_files = sorted(a.other_rows.glob("rows-*.jsonl"))
    spy, other = read_rows(spy_files), read_rows(other_files)
    refused = read_refusals(a.refused)
    jobs = {} if a.dry_run else read_jobs(a.spy_jobs + a.other_jobs)
    if not a.dry_run:
        # A game without a label job is refused like a replay failure.
        for games, _ in (spy, other):
            for gid in games:
                if gid not in jobs:
                    refused.setdefault(gid, "missing_job")
    spy_picks, spy_counts = select_class(*spy, "spy_line", "Spy", SPY_GAMES, "spy", refused)
    burn_picks, burn_counts = select_class(*other, "burn_lethal", "Burn", BURN_GAMES, "burn", refused)
    control, candidates = select_control([spy, other], refused)
    picks = spy_picks + burn_picks + control
    selected = [p for p in picks if p["status"] == "selected"]
    tools = [Path(__file__).resolve(), Path(__file__).resolve().parents[1] / "win_check" / "summarize.py"]
    inventory = {"plan": PLAN, "spy": spy_counts, "burn": burn_counts,
                 "control_selected": sum(p["status"] == "selected" for p in control),
                 "control_candidates": candidates,
                 "sources": {str(p): sha256_file(p) for p in spy_files + other_files + list(a.spy_jobs) + list(a.other_jobs) + list(a.refused)},
                 "tools": {p.name: sha256_file(p) for p in tools},
                 "refusals": [{"stratum": p["stratum"], "game_id": p["game_id"], "root_id": f"{p['game_id']}#{p['roles'][0][1]}",
                               "rank": p["rank"], "reason": p["refusal"]} for p in picks if p["status"] == "refused"],
                 "games": [{k: v for k, v in p.items() if k != "roles"} | {"roles": [list(r) for r in p["roles"]]} for p in picks]}
    roots = len({(p["game_id"], r[1]) for p in selected for r in p["roles"]})
    inventory["roots"] = roots
    a.out.mkdir(parents=True, exist_ok=True)
    (a.out / "inventory.json").write_text(json.dumps(inventory, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"spy": spy_counts, "burn": burn_counts, "control": inventory["control_selected"],
                      "roots": roots, "refusals": len(inventory["refusals"])}))
    if a.dry_run:
        return 0
    merged = {}
    for p in selected:
        for role, index, turn in p["roles"]:
            entry = merged.setdefault((p["game_id"], index), {"root_id": f"{p['game_id']}#{index}", "game_id": p["game_id"],
                                                             "index": index, "turn": turn, "roles": [], "job": jobs[p["game_id"]]})
            entry["roles"].append({"stratum": p["stratum"], "role": role, "rank": p["rank"], "selected": True,
                                   "opportunity_turn": p.get("opportunity_turn"), "certified_index": p.get("certified_index")})
    (a.out / "roots.jsonl").write_text("".join(json.dumps(e, sort_keys=True) + "\n" for e in merged.values()), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
