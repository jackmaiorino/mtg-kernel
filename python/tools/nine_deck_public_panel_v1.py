"""Held-out panel (P2) for the nine-deck baseline through the public evaluator (v1).

The held-out opponents (V3 through the Legacy source with its adapters) load
only in ``public_feature_evaluation_v1``, which plays best-of-three matches in
one process. A panel case is one match; its game 1 is the preboard game the
panel reads. Each case's game-one chooser is the case's starting player.

Parallelism is across processes: the 1,944 cases are split into shard
requests that run concurrently. Match files carry no paths or indices, so a
match's SHA-256 does not depend on sharding; qualification runs a fixed case
sample serially (one process) and then split across k processes, requires the
same per-match hashes, and records completed matches per second. Production
refuses to run without a receipt for the same evaluator, opponent, schedule and
candidate model layout, and uses the fastest qualified k. Everything runs
inside a held host reservation.

CLI:
  build    --panel p2 --candidate SRC.json --opponent OPP.json --decks DECKS --out DIR --shards N
  qualify  --plan DIR/plan.json --sample 16 --workers 1,2,4,8 --root DIR   (inside a reservation)
  run      --plan DIR/plan.json --receipt RECEIPT.json                      (inside a reservation)
  launch   qualify|run ARGS...                                               (takes the reservation)
  rows     --plan DIR/plan.json --out ROWS.jsonl
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor

import host_reservation_v1 as reservations
import nine_deck_baseline_v1 as ndb

SCHEMA = "nine-deck-baseline-v1/public-panel"
RECEIPT = "nine-deck-baseline-v1/public-panel-qualification"
MAX_DECISIONS = 10000  # public evaluator bound (learned_bo3_v1/public_evaluation.rs)
MAX_MATCHES_PER_REQUEST = 1024


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def pin(path: Path) -> dict:
    path = Path(path)
    return {"path": str(path).replace("\\", "/"), "sha256": sha256_bytes(path.read_bytes())}


def checked(ref: dict) -> Path:
    path = Path(ref["path"])
    if sha256_bytes(path.read_bytes()) != ref["sha256"]:
        raise ValueError(f"pinned file changed: {path}")
    return path


def write_json(path: Path, value) -> dict:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    return pin(path)


def case_match(case: dict, decks: list[dict]) -> dict:
    own, other = decks[case["own"]], decks[case["other"]]
    seats = [own, other] if case["seat"] == 0 else [other, own]
    return {"config": {"deck_ids": [seats[0]["label"], seats[1]["label"]], "seed": case["seed"],
                       "game_one_chooser": case["starting_player"], "max_physical_decisions": MAX_DECISIONS,
                       "max_physical_games": 3, "max_policy_steps": ndb.MAX_POLICY_STEPS,
                       "opening_protocol": "keep_seven_v2"},
            "registered": [dict(seats[0]), dict(seats[1])], "postboard": None}


def candidate_source(source: dict) -> dict:
    return {"kind": "legacy", "source": source, "v3_forced_actions": False}


def build(panel: str, candidate: dict, opponent: dict, decks: list[dict], out: Path, shards: int,
          own_decks: tuple[int, ...] = tuple(range(ndb.DECKS))) -> dict:
    """Shard requests: per learner seat, the seat's cases split into ``shards`` contiguous requests."""
    out = Path(out)
    cases = ndb.panel_cases(own_decks)
    requests = []
    for seat in (0, 1):
        seat_cases = [case for case in cases if case["seat"] == seat]
        size = -(-len(seat_cases) // shards)
        for index in range(shards):
            part = seat_cases[index * size:(index + 1) * size]
            if not part:
                continue
            if len(part) > MAX_MATCHES_PER_REQUEST:
                raise ValueError("shard exceeds the evaluator's match bound")
            sources = [candidate_source(candidate), opponent] if seat == 0 else [opponent, candidate_source(candidate)]
            name = f"seat{seat}-shard{index:02d}"
            request = {"sources": sources, "matches": [case_match(case, decks) for case in part],
                       "cross_generation_evaluation": True, "capture_decisions": False,
                       "output_directory": str(out / "native" / name).replace("\\", "/")}
            requests.append({"name": name, "seat": seat, "cases": part,
                             "request": write_json(out / "requests" / f"{name}.json", request)})
    plan = {"schema": SCHEMA, "panel": panel, "candidate": candidate, "opponent": opponent,
            "own_decks": list(own_decks), "cases": len(cases), "requests": requests,
            "schedule_sha256": schedule_digest(cases, opponent)}
    write_json(out / "plan.json", plan)
    return plan


def schedule_digest(cases: list[dict], opponent: dict) -> str:
    return sha256_bytes(json.dumps({"cases": cases, "opponent": opponent, "max_decisions": MAX_DECISIONS},
                                   sort_keys=True).encode())


def run_one(executable: str, request: Path, token: str) -> bool:
    """Run one evaluator request; True when it exits 0."""
    log = request.with_suffix(".log")
    # The evaluator creates its output directory but not that directory's parent.
    Path(json.loads(request.read_text(encoding="utf-8"))["output_directory"]).parent.mkdir(parents=True, exist_ok=True)
    with log.open("w") as stream:
        child = subprocess.Popen([executable, str(request)], stdout=stream, stderr=subprocess.STDOUT,
                                 creationflags=getattr(subprocess, "BELOW_NORMAL_PRIORITY_CLASS", 0),
                                 env=dict(os.environ, CUDA_VISIBLE_DEVICES=""))
        reservations.record_descendant(token, child.pid)
        return child.wait() == 0


def held_token() -> str:
    token = os.environ.get(reservations.TOKEN_ENV)
    if not token or reservations.status(token).get("token_fate") != "holds":
        raise SystemExit("public panel execution runs only inside a held host reservation")
    return token


def run_requests(executable: str, requests: list[Path], workers: int, tolerate: bool = False) -> float:
    """Run evaluator requests with at most ``workers`` concurrent processes; return wall seconds.
    With ``tolerate`` a failed request is left for ``recover`` instead of raising."""
    token = held_token()

    def one(request: Path) -> None:
        if not run_one(executable, request, token) and not tolerate:
            raise RuntimeError(f"evaluator failed on {request}; see {request.with_suffix('.log')}")

    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=workers) as pool:
        list(pool.map(one, requests))
    return time.monotonic() - started


def recover(executable: str, request_path: Path) -> dict:
    """A failed request stops at its first failing match: the match files before it are valid and the
    rest never ran. Record that match as a technical termination (incomplete, never dropped) and continue
    from the next match in a continuation request until every match is accounted for. Match files do
    not depend on sharding (qualification parity), so continuations reproduce them."""
    request_path = Path(request_path)
    request = json.loads(request_path.read_text(encoding="utf-8"))
    matches, base = request["matches"], request["output_directory"]
    segments, errors, start, current, k = [], [], 0, request_path, 0
    while True:
        output = Path(json.loads(current.read_text(encoding="utf-8"))["output_directory"])
        done = len(sorted(output.glob("match-*.json")))
        segments.append({"request": pin(current), "output_directory": str(output).replace("\\", "/"),
                         "start": start, "count": done})
        if (output / "completion.json").exists():
            if start + done != len(matches):
                raise RuntimeError(f"{output} completed {done} matches, expected {len(matches) - start}")
            break
        failure = output / "failure.json"
        if not failure.exists():
            raise RuntimeError(f"evaluator failed on {current} without a failure record")
        index = start + done
        error = json.loads(failure.read_text(encoding="utf-8"))["error"]
        seed = matches[index]["config"]["seed"]
        if f"match seed {seed}," not in error:
            raise RuntimeError(f"{failure} does not name match {index} (seed {seed})")
        errors.append({"index": index, "seed": seed, "error": error})
        start = index + 1
        if start == len(matches):
            break
        k += 1
        current = request_path.with_name(f"{request_path.stem}-c{k}.json")
        write_json(current, dict(request, matches=matches[start:], output_directory=f"{base}-c{k}"))
        output = Path(f"{base}-c{k}")  # an interrupted recovery: reuse a finished continuation, set aside a partial one
        if output.exists() and not (output / "completion.json").exists() and not (output / "failure.json").exists():
            output.rename(output.with_name(f"{output.name}.partial-{int(time.time())}"))
        if not output.exists():
            run_one(executable, current, held_token())
    return write_json(request_path.with_suffix(".recovery.json"),
                      {"request": pin(request_path), "segments": segments, "errors": errors})


def match_hashes(request: Path) -> list[str]:
    output = Path(json.loads(Path(request).read_text(encoding="utf-8"))["output_directory"])
    completion = json.loads((output / "completion.json").read_text(encoding="utf-8"))
    files = sorted(output.glob("match-*.json"))
    hashes = [sha256_bytes(path.read_bytes()) for path in files]
    if hashes != completion["match_sha256"]:
        raise ValueError(f"match files differ from completion record in {output}")
    return hashes


def qualify(plan_path: Path, sample: int, workers: list[int], root: Path, executable: str) -> dict:
    plan = json.loads(Path(plan_path).read_text(encoding="utf-8"))
    first = json.loads(checked(plan["requests"][0]["request"]).read_text(encoding="utf-8"))
    matches = first["matches"][:sample]
    trials = []
    golden = None
    for k in workers:
        if sample % k:
            raise ValueError("sample must split evenly across workers")
        paths = []
        for index in range(k):
            request = dict(first, matches=matches[index * sample // k:(index + 1) * sample // k],
                           output_directory=str(Path(root) / f"w{k}" / f"native-{index}").replace("\\", "/"))
            path = Path(root) / f"w{k}" / f"request-{index}.json"
            write_json(path, request)
            paths.append(path)
        seconds = run_requests(executable, paths, k)
        hashes = [h for path in paths for h in match_hashes(path)]
        golden = golden or hashes
        trials.append({"workers": k, "seconds": seconds, "matches": len(hashes), "matches_per_second": len(hashes) / seconds,
                       "parity": hashes == golden})
    receipt = {"schema": RECEIPT, "evaluator_sha256": sha256_bytes(Path(executable).read_bytes()),
               "schedule_sha256": plan["schedule_sha256"], "opponent": plan["opponent"],
               "candidate_layout": candidate_layout(plan["candidate"]), "sample": sample, "trials": trials,
               "golden_match_sha256": golden, "selected_workers": max(
                   (t for t in trials if t["parity"]), key=lambda t: t["matches_per_second"])["workers"]}
    write_json(Path(root) / "receipt.json", receipt)
    return receipt


def candidate_layout(source: dict) -> str:
    """Candidate model layout (import, feature contract, parameter shapes), not its weights."""
    checkpoint = json.loads(checked(source["checkpoint"]).read_text(encoding="utf-8"))
    layout = {key: checkpoint[key] for key in ("schema", "source_import", "feature_contract_digest",
                                               "feature_encoding_digest", "card_db_hash")}
    layout["parameters"] = [(row["name"], row["shape"]) for row in checkpoint["parameters"]]
    layout["play_import"] = source["play_import"]["sha256"]
    return sha256_bytes(json.dumps(layout, sort_keys=True).encode())


def run(plan_path: Path, receipt_path: Path, executable: str) -> dict:
    plan = json.loads(Path(plan_path).read_text(encoding="utf-8"))
    receipt = json.loads(Path(receipt_path).read_text(encoding="utf-8"))
    if receipt["schema"] != RECEIPT or receipt["evaluator_sha256"] != sha256_bytes(Path(executable).read_bytes()) \
            or receipt["schedule_sha256"] != plan["schedule_sha256"] or receipt["opponent"] != plan["opponent"] \
            or receipt["candidate_layout"] != candidate_layout(plan["candidate"]):
        raise SystemExit("qualification receipt does not cover this evaluator, schedule, opponent or model layout")
    if not all(trial["parity"] for trial in receipt["trials"]) or receipt["trials"][0]["workers"] != 1:
        raise SystemExit("receipt lacks a serial golden or parallel parity")
    paths = [checked(item["request"]) for item in plan["requests"]]
    pending = []
    for path in paths:  # a resumed run keeps finished and failed outputs; a partial one is set aside
        output = Path(json.loads(path.read_text(encoding="utf-8"))["output_directory"])
        if output.exists() and not (output / "completion.json").exists() and not (output / "failure.json").exists():
            output.rename(output.with_name(f"{output.name}.partial-{int(time.time())}"))
        if not output.exists():
            pending.append(path)
    seconds = run_requests(executable, pending, receipt["selected_workers"], tolerate=True)
    completions = []
    for path in paths:
        output = Path(json.loads(path.read_text(encoding="utf-8"))["output_directory"])
        recovery = path.with_suffix(".recovery.json")
        if recovery.exists():
            completions.append(pin(recovery))
        elif (output / "completion.json").exists():
            completions.append(pin(output / "completion.json"))
        else:
            completions.append(recover(executable, path))
    result = {"plan": pin(plan_path), "receipt": pin(receipt_path), "workers": receipt["selected_workers"],
              "seconds": seconds, "completions": completions}
    write_json(Path(plan_path).parent / "run.json", result)
    return result


def rows(plan_path: Path) -> list[dict]:
    """One row per case from game 1: candidate win 1, draw 0.5, loss 0. A match file is written only when
    every game ended naturally, so a game 1 without a winner is a natural draw. A failed match is a
    technical termination: incomplete, with its error and the BO3 game it failed in."""
    plan = json.loads(Path(plan_path).read_text(encoding="utf-8"))
    out = []
    for item in plan["requests"]:
        request_path = checked(item["request"])
        request = json.loads(request_path.read_text(encoding="utf-8"))
        recovery_path = request_path.with_suffix(".recovery.json")
        if recovery_path.exists():
            recovery = json.loads(recovery_path.read_text(encoding="utf-8"))
            segments = [(Path(s["output_directory"]), s["start"], s["count"]) for s in recovery["segments"]]
            errors = {e["index"]: e for e in recovery["errors"]}
        else:
            segments, errors = [(Path(request["output_directory"]), 0, len(item["cases"]))], {}
        files = {}
        for output, start, count in segments:
            found = sorted(output.glob("match-*.json"))
            if len(found) != count:
                raise ValueError(f"{output} holds {len(found)} matches, expected {count}")
            files.update({start + i: path for i, path in enumerate(found)})
        if sorted(list(files) + list(errors)) != list(range(len(item["cases"]))):
            raise ValueError(f"{request_path}: matches and technical terminations do not cover every case")
        for index, case in enumerate(item["cases"]):
            row = {"panel": plan["panel"], "checkpoint_sha256": plan["candidate"]["checkpoint"]["sha256"],
                   "own": case["own"], "other": case["other"], "repeat": case["repeat"],
                   "seat": case["seat"], "seed": case["seed"]}
            if index in errors:
                error = errors[index]["error"]
                failed_game = re.search(rf"match seed {case['seed']}, game (\d+)", error)
                if errors[index]["seed"] != case["seed"] or not failed_game:
                    raise ValueError("technical termination does not match the plan")
                out.append({**row, "starting_player": case["starting_player"], "score": None, "complete": False,
                            "technical": error.split(": ", 1)[-1][:160], "technical_game": int(failed_game[1]),
                            "match_sha256": None})
                continue
            path = files[index]
            match = json.loads(path.read_text(encoding="utf-8"))
            if match["match"]["config"]["seed"] != case["seed"]:
                raise ValueError("match order differs from the plan")
            game = match["games"][0]
            winner = game.get("winner")
            out.append({**row, "starting_player": game["start"]["starting_player"],
                        "score": 0.5 if winner is None else float(winner == case["seat"]),
                        "complete": True, "match_sha256": sha256_bytes(path.read_bytes())})
    return out


def launch(argv: list[str], lane: str, work_id: str) -> dict:
    return reservations.dispatch(
        lane=lane, work_id=work_id, release_condition="public panel " + argv[0] + " complete or failed",
        command=[sys.executable, "-B", str(Path(__file__).resolve())] + argv,
        cwd=str(Path(__file__).resolve().parents[2]),
        busy_pattern=r"public_feature_evaluation|cargo|rustc")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("op", choices=("build", "qualify", "run", "rows", "launch"))
    parser.add_argument("rest", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    if args.op == "launch":
        print(json.dumps(launch(args.rest, os.environ.get("NINE_DECK_LANE", "nine-deck-baseline"),
                                "nine-deck-panel-" + str(int(time.time())))))
        return 0
    sub = argparse.ArgumentParser()
    sub.add_argument("--panel"); sub.add_argument("--candidate", type=Path); sub.add_argument("--opponent", type=Path)
    sub.add_argument("--decks", type=Path); sub.add_argument("--out", type=Path); sub.add_argument("--shards", type=int)
    sub.add_argument("--plan", type=Path); sub.add_argument("--sample", type=int, default=16)
    sub.add_argument("--workers", default="1,2,4,8"); sub.add_argument("--root", type=Path)
    sub.add_argument("--receipt", type=Path); sub.add_argument("--executable")
    a = sub.parse_args(args.rest)
    if args.op == "build":
        plan = build(a.panel, json.loads(a.candidate.read_text()), json.loads(a.opponent.read_text()),
                     ndb.load_decks(a.decks), a.out, a.shards)
        print(json.dumps({"requests": len(plan["requests"]), "cases": plan["cases"]}))
    elif args.op == "qualify":
        print(json.dumps(qualify(a.plan, a.sample, [int(x) for x in a.workers.split(",")], a.root, a.executable)["trials"]))
    elif args.op == "run":
        print(json.dumps(run(a.plan, a.receipt, a.executable)))
    else:
        result = rows(a.plan)
        a.out.write_text("".join(json.dumps(row, sort_keys=True) + "\n" for row in result), encoding="utf-8")
        print(json.dumps({"rows": len(result), "complete": sum(row["complete"] for row in result)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
