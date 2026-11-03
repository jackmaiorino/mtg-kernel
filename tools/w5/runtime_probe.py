"""W5 amendment A1, condition 2 as corrected (collab LANES/lead/w5/RULINGS-r0.md "A1 correction"): probe P2.

b512 against g115 under the training runtime (the native evaluator, `runtime_rules: resolution-boundary-v1`, no
neutral choices, stepwise sampling), against the same matchup on the full board stack (the published rated
pauper-ladder-v1 run: the Spellbench wrapper with neutral choices, historical rules and buffered completions).
Probe value D = b512's mean pair score under the training runtime minus under the board runtime.

- Board half: the 64 seat-swapped b512/g115 pairs of the published run (one per ordered deck pair of the 8 board
  decks; p0 starts; each bot plays both seats of the pair).
- Training half: the same 64 deck pairs and seat layout, `--repeats` fresh seeds each (seed = first 8 bytes of
  SHA256(json(["w5/a1-probe", pair, repeat])), masked to 63 bits), game one only, on the native evaluator with
  the training registrations of the decks.
- Interval: 95% percentile bootstrap over the 64 deck pairs (clusters), both halves resampled with the same pair
  indices, 10,000 replicates, seed SHA256("w5/a1-probe/boot"). A pair counts only when both of its games end
  naturally (the board's rule).
- Confound (stated): this bounds the difference between b512's and g115's runtime sensitivities, not b512's alone;
  the engines also differ in revision (the evaluator's build against the board's 92ec2393).

  runtime_probe.py plan --board-run RUN_DIR --repo MTG_KERNEL_REPO --b512 SCORER_CONFIG --g115 SCORER_CONFIG
                        [--play-import IMPORT.json] --repeats R --shards S --out DIR
  runtime_probe.py run --plan DIR/plan.json --evaluator EXE --workers K     (inside a host_slots claim)
  runtime_probe.py read --plan DIR/plan.json --out RESULT.json
"""
from __future__ import annotations

import argparse
import hashlib
import json
import random
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

B512, G115 = "oh2-r2s1-b512", "g115"
RUNTIME = "resolution-boundary-v1"
MAX_DECISIONS = 10000  # the board's max_decisions and the evaluator's bound
MASK63 = (1 << 63) - 1
REPLICATES = 10_000


def sha(path) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def board_pairs(run: Path) -> list[dict]:
    """The published b512/g115 pairs: deck per seat, and b512's score in each slot."""
    pairs: dict[int, dict] = {}
    for line in (run / "matches.jsonl").read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        r = json.loads(line)
        names = {s["seat"]: s["name"] for s in r["seats"]}
        if set(names.values()) != {B512, G115}:
            continue
        entry = pairs.setdefault(r["pair_index"], {"pair": r["pair_index"], "decks": [d["catalog_id"] for d in r["decks"]],
                                                   "slots": {}})
        if entry["decks"] != [d["catalog_id"] for d in r["decks"]]:
            raise SystemExit(f"pair {r['pair_index']} changes decks between slots")
        natural = r["classification"] in ("natural", "forfeit")
        win = r["winner"]
        score = None if not natural else 0.5 if win is None else (1.0 if names[win] == B512 else 0.0)
        entry["slots"][r["pair_slot"]] = {"b512_seat": 0 if names["p0"] == B512 else 1, "score": score}
    out = sorted(pairs.values(), key=lambda p: p["pair"])
    for p in out:
        if set(p["slots"]) != {0, 1}:
            raise SystemExit(f"pair {p['pair']} is incomplete")
    return out


def load_decks(repo: Path) -> list[dict]:
    sys.path.insert(0, str(repo / "python" / "tools"))
    import nine_deck_baseline_v1 as ndb
    return ndb.load_decks(repo / "data" / "runtime_decks_v1.json")


def match(decks_by_name: dict, seat_decks: list[str], seed: int) -> dict:
    """npp.case_match's config (nine_deck_public_panel_v1.py) with the training runtime, p0 choosing play."""
    d0, d1 = decks_by_name[seat_decks[0]], decks_by_name[seat_decks[1]]
    return {"config": {"deck_ids": [d0["label"], d1["label"]], "seed": seed, "game_one_chooser": 0,
                       "max_physical_decisions": MAX_DECISIONS, "max_physical_games": 3,
                       "max_policy_steps": 200000, "opening_protocol": "keep_seven_v2", "runtime_rules": RUNTIME},
            "registered": [dict(d0), dict(d1)], "postboard": None}


def legacy(source: dict) -> dict:
    return {"kind": "legacy", "source": source, "v3_forced_actions": False}


def plan(args) -> dict:
    pairs = board_pairs(args.board_run)
    decks = load_decks(args.repo)
    by_name = {d["label"].split("/")[0]: d for d in decks}
    b512 = json.loads(Path(args.b512).read_text())["source"]
    g115 = json.loads(Path(args.g115).read_text())["source"]
    if args.play_import:  # the training-side import (the board scorer's import is its own equivalent)
        for source in (b512, g115):
            source["play_import"] = {"path": str(args.play_import).replace("\\", "/"), "sha256": sha(args.play_import)}
    out = Path(args.out)
    if (out / "plan.json").exists():
        raise SystemExit(f"{out} already holds a plan")
    cases = []
    for p in pairs:
        for repeat in range(args.repeats):
            seed = int.from_bytes(hashlib.sha256(json.dumps(["w5/a1-probe", p["pair"], repeat]).encode()).digest()[:8],
                                  "little") & MASK63
            for slot, info in sorted(p["slots"].items()):
                cases.append({"pair": p["pair"], "repeat": repeat, "slot": slot, "b512_seat": info["b512_seat"],
                              "decks": p["decks"], "seed": seed})
    requests = []
    for seat in (0, 1):
        seat_cases = [c for c in cases if c["b512_seat"] == seat]
        size = -(-len(seat_cases) // args.shards)
        for i in range(args.shards):
            part = seat_cases[i * size:(i + 1) * size]
            if not part:
                continue
            name = f"seat{seat}-shard{i:02d}"
            request = {"sources": [legacy(b512), legacy(g115)] if seat == 0 else [legacy(g115), legacy(b512)],
                       "matches": [match(by_name, c["decks"], c["seed"]) for c in part],
                       "cross_generation_evaluation": True, "capture_decisions": False, "game_one_only": True,
                       "output_directory": str((out / "native" / name).resolve()).replace("\\", "/")}
            path = out / "requests" / f"{name}.json"
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(request, separators=(",", ":")) + "\n", encoding="utf-8")
            requests.append({"name": name, "path": str(path.resolve()), "sha256": sha(path), "cases": part})
    doc = {"schema": "w5-a1-probe-plan/v1", "repeats": args.repeats, "runtime_rules": RUNTIME,
           "board_run": {"path": str(args.board_run), "manifest_sha256": sha(args.board_run / "manifest.json"),
                         "matches_sha256": sha(args.board_run / "matches.jsonl")},
           "b512_config": {"path": str(args.b512), "sha256": sha(args.b512)},
           "g115_config": {"path": str(args.g115), "sha256": sha(args.g115)},
           "decks": {label: d["label"] for label, d in by_name.items()}, "board_pairs": pairs,
           "requests": requests, "probe_sha256": sha(__file__)}
    (out / "plan.json").write_text(json.dumps(doc, indent=1) + "\n", encoding="utf-8")
    return {"cases": len(cases), "requests": len(requests)}


def run(args) -> dict:
    doc = json.loads(Path(args.plan).read_text())
    evaluator = str(Path(args.evaluator).resolve())
    started = time.monotonic()

    def one(item):
        out = Path(json.loads(Path(item["path"]).read_text())["output_directory"])
        if (out / "completion.json").exists():
            return item["name"], 0
        out.parent.mkdir(parents=True, exist_ok=True)
        with open(Path(item["path"]).with_suffix(".log"), "w") as log:
            code = subprocess.call([evaluator, item["path"]], stdout=log, stderr=subprocess.STDOUT,
                                   creationflags=getattr(subprocess, "BELOW_NORMAL_PRIORITY_CLASS", 0))
        return item["name"], code

    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        codes = dict(pool.map(one, doc["requests"]))
    result = {"evaluator_sha256": sha(evaluator), "wall_seconds": time.monotonic() - started, "exit_codes": codes}
    (Path(args.plan).parent / f"run-{int(time.time())}.json").write_text(json.dumps(result, indent=1))
    return result


def game_score(record: dict, b512_seat: int):
    game = record["games"][0]
    terminal = game.get("terminal_classification") or game.get("classification") or game.get("termination")
    if terminal is not None and str(terminal).lower() not in ("natural", "game_over"):
        return None
    winner = game.get("winner")
    if winner is None:
        return 0.5 if terminal is not None else None
    seat = 0 if winner in (0, "p0", "P0", "P0Wins") else 1 if winner in (1, "p1", "P1") else None
    if seat is None:
        raise SystemExit(f"unknown winner encoding {winner!r}")
    return 1.0 if seat == b512_seat else 0.0


def read(args) -> dict:
    doc = json.loads(Path(args.plan).read_text())
    train: dict[int, list[float]] = {}
    unscored = 0
    for item in doc["requests"]:
        out = Path(json.loads(Path(item["path"]).read_text())["output_directory"])
        records = sorted(out.glob("match-*.json"))
        if len(records) != len(item["cases"]):
            raise SystemExit(f"{item['name']}: {len(records)} records for {len(item['cases'])} cases")
        games = {}
        for case, path in zip(item["cases"], records):
            record = json.loads(path.read_text())
            if record["match"]["config"]["seed"] != case["seed"]:
                raise SystemExit("match order differs from the plan")
            games[(case["pair"], case["repeat"], case["slot"])] = game_score(record, case["b512_seat"])
        for (pair, repeat, slot), score in games.items():
            train.setdefault((pair, repeat), {})[slot] = score
    by_pair: dict[int, list[float]] = {}
    for (pair, repeat), slots in train.items():
        if len(slots) == 2 and None not in slots.values():
            by_pair.setdefault(pair, []).append((slots[0] + slots[1]) / 2)
        else:
            unscored += 1
    board = {p["pair"]: (p["slots"][0]["score"] + p["slots"][1]["score"]) / 2 for p in doc["board_pairs"]
             if None not in (p["slots"][0]["score"], p["slots"][1]["score"])}
    pairs = sorted(set(board) & set(by_pair))
    t_mean = [sum(by_pair[p]) / len(by_pair[p]) for p in pairs]
    b_val = [board[p] for p in pairs]

    def stats(idx):
        t = sum(t_mean[i] for i in idx) / len(idx)
        b = sum(b_val[i] for i in idx) / len(idx)
        return t, b, t - b

    point = stats(range(len(pairs)))
    rng = random.Random(int.from_bytes(hashlib.sha256(b"w5/a1-probe/boot").digest()[:8], "little"))
    reps = [stats([rng.randrange(len(pairs)) for _ in pairs]) for _ in range(REPLICATES)]

    def ci(k):
        values = sorted(r[k] for r in reps)
        return [values[int(0.025 * (REPLICATES - 1))], values[int(0.975 * (REPLICATES - 1))]]

    result = {"schema": "w5-a1-probe-result/v1", "pairs": len(pairs), "repeats": doc["repeats"],
              "training_games_unscored_pairs": unscored,
              "training_runtime_b512_score": point[0], "training_ci95": ci(0),
              "board_runtime_b512_score": point[1], "board_ci95": ci(1),
              "difference_training_minus_board": point[2], "difference_ci95": ci(2),
              "reopen_exception": ci(2)[0] > 0,
              "confound": "bounds the difference between b512's and g115's runtime sensitivities, not b512's alone; "
                          "the evaluator's engine revision also differs from the board's 92ec2393",
              "plan_sha256": sha(args.plan)}
    Path(args.out).write_text(json.dumps(result, indent=1) + "\n")
    return result


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="op", required=True)
    p = sub.add_parser("plan")
    p.add_argument("--board-run", type=Path, required=True)
    p.add_argument("--repo", type=Path, required=True)
    p.add_argument("--b512", type=Path, required=True)
    p.add_argument("--g115", type=Path, required=True)
    p.add_argument("--play-import", type=Path, default=None)
    p.add_argument("--repeats", type=int, default=2)
    p.add_argument("--shards", type=int, default=4)
    p.add_argument("--out", type=Path, required=True)
    p = sub.add_parser("run")
    p.add_argument("--plan", required=True)
    p.add_argument("--evaluator", required=True)
    p.add_argument("--workers", type=int, default=4)
    p = sub.add_parser("read")
    p.add_argument("--plan", required=True)
    p.add_argument("--out", required=True)
    args = parser.parse_args(argv)
    result = {"plan": plan, "run": run, "read": read}[args.op](args)
    print(json.dumps(result, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
