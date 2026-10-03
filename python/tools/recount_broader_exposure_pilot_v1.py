"""Recount complete raw match outcomes independently of the pilot analyzer."""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path


def read(path):
    return json.loads(Path(path).read_bytes())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    root = parser.parse_args().root
    analysis = read(root / "analysis.json")
    assert analysis["complete"] and analysis["matches"] == 3864
    audits = {arm: read(root / (arm + "-evaluation-audit.json")) for arm in ("g115", "control", "broader")}
    assert all(a["complete"] and len(a["rows"]) == 1288 for a in audits.values())
    counts = defaultdict(Counter)
    seats = defaultdict(Counter)
    seeds = defaultdict(set)
    identities = set()
    files = set()
    games = 0
    for arm, audit in audits.items():
        for row in audit["rows"]:
            path = Path(row["match"]["path"])
            payload = path.read_bytes()
            assert hashlib.sha256(payload).hexdigest() == row["match"]["sha256"]
            assert str(path) not in files
            files.add(str(path))
            match = json.loads(payload)
            identity = (row["cohort"], row["own"], row["other"], row["replicate"], row["seat"], arm)
            assert identity not in identities
            identities.add(identity)
            assert match["config"]["seed"] == row["seed"]
            winners = [g["winner"] for g in match["games"]]
            assert all(w in (0, 1, None) for w in winners)
            wins = Counter(w for w in winners if w is not None)
            finished = [seat for seat in (0, 1) if wins[seat] == 2]
            assert len(finished) <= 1 and max(wins.values(), default=0) <= 2
            assert all(winners[:-1].count(seat) < 2 for seat in (0, 1))
            winner = finished[0] if finished else None
            outcome = "draw" if winner is None else {"winner": {"winner": winner}}
            assert match["outcome"] == outcome
            actual = {"matches": 1, "g1_win": int(winners[0] == row["seat"]),
                "g1_draw": int(winners[0] is None), "bo3_win": int(winner == row["seat"]),
                "bo3_draw": int(winner is None)}
            for key in ("g1_win", "g1_draw", "bo3_win", "bo3_draw"):
                assert actual[key] == row[key]
            counts[(row["cohort"], arm)].update(actual)
            seat_key = (row["cohort"], row["own"], row["other"], row["seat"],
                        match["games"][0]["start"]["starting_player"] == row["seat"], arm)
            seats[seat_key].update(actual)
            seeds[identity[:4]].add(match["games"][0]["environment_seed"])
            games += len(winners)
    assert len(files) == len(identities) == 3864 and games == analysis["natural_games"]
    assert all(len(values) == 1 for values in seeds.values())
    for (cohort, arm), actual in counts.items():
        expected = analysis["cohorts"][cohort]["summary"][arm]
        assert all(expected[k] == v for k, v in actual.items())
    result = {"complete": True, "unique_matches": len(files), "games": games,
        "raw_bo3_outcomes_reconstructed": True, "first_game_environment_seeds_matched": True,
        "cohort_counts": [{"cohort": c, "arm": a, **v} for (c, a), v in sorted(counts.items())],
        "matchup_seat_play_draw_counts": [{"cohort": k[0], "own": k[1], "other": k[2],
            "seat": k[3], "on_play": k[4], "arm": k[5], **v} for k, v in sorted(seats.items())],
        "limits": "Descriptive cell counts; no per-cell inference or new selection gates. Natural terminal enforcement belongs to the pinned producer's finish_natural_v1 path."}
    with (root / "independent-outcome-recount.json").open("x", encoding="utf-8") as output:
        json.dump(result, output, indent=2)
        output.write("\n")
    print(json.dumps({k: v for k, v in result.items() if k != "matchup_seat_play_draw_counts"}))


if __name__ == "__main__":
    main()
