"""Held-out panel (P2) for the nine-deck baseline through the public evaluator (v1).

The held-out opponents (V3 through the Legacy source with its adapters) load
only in ``public_feature_evaluation_v1``, which plays best-of-three matches in
one process. By default a panel case is one match; its game 1 is the preboard
game the panel reads. Opt-in ``--game-one-only`` stops at that first natural
terminal (including draws), without emitting a match outcome. Each case's
game-one chooser is the case's starting player.

Parallelism is across processes: the 1,944 cases are split into shard
requests that run concurrently. Match files carry no paths or indices, so a
match's SHA-256 does not depend on sharding; qualification runs a fixed case
sample serially (one process) and then split across k processes, requires the
same per-match hashes, and records completed matches per second. Production
refuses to run without a receipt for the same evaluator, opponent, schedule and
candidate model layout, and uses the fastest qualified k. Everything runs
inside a held host reservation.

Pinned explicit case plans additionally support host_slots-backed execution
through the archive resource Guard. Their seeds and metadata are preserved,
qualification covers both seats and all decks, and one guard may cover several
plans through recovery and publication. See docs/nine_deck_public_panel_guard_v1.md.

CLI:
  build    --panel p2 --candidate SRC.json --opponent OPP.json --decks DECKS --out DIR --shards N
  qualify  --plan DIR/plan.json --sample 16 --workers 1,2,4,8 --root DIR   (inside a reservation)
  run      --plan DIR/plan.json --receipt RECEIPT.json                      (inside a reservation)
  launch   qualify|run ARGS...                                               (takes the reservation)
  rows     --plan DIR/plan.json --out ROWS.jsonl
"""
from __future__ import annotations

import argparse
import copy
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
GUARD_SCHEMA = "public-panel-guard/v1"


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


def write_json(path: Path, value, *, exclusive: bool = False) -> dict:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    if exclusive:
        with path.open("x", encoding="utf-8", newline="\n") as stream:
            stream.write(json.dumps(value, indent=1, sort_keys=True, allow_nan=False) + "\n")
    else:
        path.write_text(json.dumps(value, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    return pin(path)


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def explicit_cases(cases_pin: dict, metadata: dict) -> list[dict]:
    document = json.loads(checked(cases_pin).read_text(encoding="utf-8"))
    require(document["decks"] == list(ndb.DECK_NAMES), "pinned case deck order differs")
    require(set(metadata) == {"lineage", "endpoint", "reference"}
            and all(type(value) is str and value for value in metadata.values()), "explicit case metadata required")
    cases, seen = [], set()
    for case in document["cases"]:
        for field, limit in (("own", 9), ("other", 9), ("seat", 2), ("starting_player", 2)):
            require(type(case[field]) is int and 0 <= case[field] < limit, "invalid explicit " + field)
        require(type(case["family"]) is int and case["family"] >= 0
                and type(case["seed"]) is int and 0 <= case["seed"] < 2**64
                and type(case["reference"]) is str and case["reference"], "invalid explicit family/seed/reference")
        key = tuple(case[k] for k in ("own", "other", "family", "seat", "reference"))
        require(key not in seen, "duplicate pinned case")
        seen.add(key)
        if case["reference"] == metadata["reference"]:
            cases.append(copy.deepcopy(case))
    require(cases, "pinned cases omit the requested reference")
    if "cases_per_endpoint_lineage" in document:
        require(document["cases_per_endpoint_lineage"] == len(document["cases"]), "pinned schedule count differs")
        families = {case["family"] for case in document["cases"]}
        references = {case["reference"] for case in document["cases"]}
        require(len(seen) == 81 * len(families) * 2 * len(references), "pinned schedule has incomplete deck/family/seat coverage")
        require(metadata["lineage"] in document["lineages"] and metadata["endpoint"] in document["endpoints"],
                "metadata is outside pinned schedule")
    return cases


def validate_explicit_plan(plan: dict) -> None:
    if "cases_input" not in plan:
        return
    require(plan.get("game_one_only") is True, "explicit physical panel mode changed")
    cases = explicit_cases(plan["cases_input"], plan["metadata"])
    expected = [case for seat in (0, 1) for case in cases if case["seat"] == seat]
    actual = [case for item in plan["requests"] for case in item["cases"]]
    require(actual == expected and plan["cases"] == len(cases), "plan differs from pinned case coverage/order")
    require(plan["schedule_sha256"] == schedule_digest(cases, plan["opponent"], plan.get("game_one_only", False)),
            "plan schedule digest differs from pinned cases")
    for item in plan["requests"]:
        request = request_for_plan(plan, item)
        seat = item["seat"]
        require(all(case["seat"] == seat for case in item["cases"]), "shard mixes learner seats")
        sources = [candidate_source(plan["candidate"]), plan["opponent"]]
        if seat == 1:
            sources.reverse()
        require(request["sources"] == sources and request["matches"] == [case_match(case, plan["decks"]) for case in item["cases"]]
                and request["cross_generation_evaluation"] is True and request["capture_decisions"] is False,
                "native request differs from pinned cases/sources")


def require_guard(plan, guard):
    if "cases_input" in plan:
        require(guard is not None, "pinned explicit panels require a host_slots resource guard")


def guard_binding(guard):
    if guard is None:
        return None
    import host_slots_v1 as slots
    return {"host": slots.host(), "cpu_affinity": guard.plan["cpu_affinity"],
            "evaluator": guard.plan["evaluator"], "launcher_sha256": pin(__file__)["sha256"]}


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
          own_decks: tuple[int, ...] = tuple(range(ndb.DECKS)), game_one_only: bool = False) -> dict:
    """Shard requests: per learner seat, the seat's cases split into ``shards`` contiguous requests."""
    return _build(panel, candidate, opponent, decks, out, shards, ndb.panel_cases(own_decks),
                  own_decks, game_one_only)


def build_from_cases(panel: str, candidate: dict, opponent: dict, decks: list[dict], out: Path,
                     shards: int, cases_pin: dict, metadata: dict, game_one_only: bool = True,
                     native_root: Path | None = None) -> dict:
    """Build from pinned cases; select a reference without generating or replacing seeds."""
    require(not Path(out).exists(), "explicit panel output must be fresh")
    require(type(shards) is int and shards > 0, "positive shard count required")
    require(game_one_only is True, "explicit physical panels require game-one-only mode")
    cases = explicit_cases(cases_pin, metadata)
    check_request_pins([candidate, opponent])
    return _build(panel, candidate, opponent, decks, out, shards, cases,
                  tuple(sorted({case["own"] for case in cases})), game_one_only, cases_pin, metadata, native_root)


def _build(panel, candidate, opponent, decks, out, shards, cases, own_decks, game_one_only,
           cases_pin=None, metadata=None, native_root=None):
    out = Path(out)
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
                       "output_directory": str((Path(native_root) if native_root is not None else out / "native") / name).replace("\\", "/")}
            if game_one_only:
                request["game_one_only"] = True
            requests.append({"name": name, "seat": seat, "cases": part,
                             "request": write_json(out / "requests" / f"{name}.json", request,
                                                   exclusive=cases_pin is not None)})
    plan = {"schema": SCHEMA, "panel": panel, "candidate": candidate, "opponent": opponent,
            "own_decks": list(own_decks), "cases": len(cases), "requests": requests,
            "schedule_sha256": schedule_digest(cases, opponent, game_one_only)}
    if game_one_only:
        plan["game_one_only"] = True
    if cases_pin is not None:
        plan.update(cases_input=cases_pin, metadata=metadata, decks=decks)
        validate_explicit_plan(plan)
    write_json(out / "plan.json", plan, exclusive=cases_pin is not None)
    return plan


def schedule_digest(cases: list[dict], opponent: dict, game_one_only: bool = False) -> str:
    workload = {"cases": cases, "opponent": opponent, "max_decisions": MAX_DECISIONS}
    if game_one_only:
        workload["game_one_only"] = True
    return sha256_bytes(json.dumps(workload, sort_keys=True).encode())


def request_for_plan(plan: dict, item: dict) -> dict:
    request = json.loads(checked(item["request"]).read_text(encoding="utf-8"))
    if request.get("game_one_only", False) != plan.get("game_one_only", False):
        raise ValueError("request evaluation mode differs from the plan")
    return request


def completion_for_request(request: dict) -> dict:
    completion = json.loads((Path(request["output_directory"]) / "completion.json").read_text(encoding="utf-8"))
    if completion.get("game_one_only", False) != request.get("game_one_only", False):
        raise ValueError("completion evaluation mode differs from the request")
    start_for_request(request)
    return completion


def command_identity(value):
    """Compare parsed commands across serde's omitted optional/default fields."""
    if isinstance(value, list):
        return [command_identity(item) for item in value]
    if isinstance(value, dict):
        return {key: (os.path.normcase(os.path.normpath(item))
                      if os.name == "nt" and key in ("path", "output_directory") and isinstance(item, str)
                      else command_identity(item)) for key, item in value.items()
                if item is not None
                and not (key in ("game_one_only", "terminal_audit_v1", "v3_spell_target_reference_adapter")
                         and item is False)
                and not (key == "opening_protocol" and item == "legacy_keep_seven_v1")}
    return value


def start_for_request(request: dict) -> None:
    start = json.loads((Path(request["output_directory"]) / "start.json").read_text(encoding="utf-8"))
    if start.get("schema") != "public-input-evaluation-start/v1" \
            or command_identity(start.get("command")) != command_identity(request):
        raise ValueError("native start command differs from the request (including evaluation mode)")


def failed_physical_game(error: str, seed: int) -> int | None:
    match = re.search(rf"match seed {seed}, game (\d+)(?:,|:)", error)
    return int(match[1]) if match else None


def recovery_for_request(request_path: Path) -> dict:
    request = json.loads(request_path.read_text(encoding="utf-8"))
    recovery = json.loads(request_path.with_suffix(".recovery.json").read_text(encoding="utf-8"))
    if recovery["request"] != pin(request_path):
        raise ValueError("recovery request pin differs from the request")
    for segment in recovery["segments"]:
        segment_request = json.loads(checked(segment["request"]).read_text(encoding="utf-8"))
        expected = dict(request, matches=request["matches"][segment["start"]:],
                        output_directory=segment["output_directory"])
        if command_identity(segment_request) != command_identity(expected):
            raise ValueError("recovery segment command differs from the request")
        start_for_request(segment_request)
    if request.get("game_one_only", False):
        for failure in recovery["errors"]:
            if failed_physical_game(failure["error"], failure["seed"]) != 1:
                raise ValueError("game-one failure does not name physical game 1")
    return recovery


def check_request_pins(value):
    """Check actual command pins, without reopening historical checkpoint provenance."""
    if isinstance(value, list):
        for item in value:
            check_request_pins(item)
    elif isinstance(value, dict):
        if {"path", "sha256"} <= set(value):
            checked(value)
        else:
            for item in value.values():
                check_request_pins(item)


def guarded_paths(guard, request_path, command):
    from archive_distillation_dispatch_v1 import absolute
    output = absolute(command["output_directory"])
    require(output != guard.root and output.is_relative_to(guard.root), "native output escapes guarded attempt")
    require(any(absolute(request_path).is_relative_to(absolute(root)) for root in guard.plan["storage"]["accounting_roots"]),
            "native request/log records escape guarded accounting")


def run_one(executable: str, request: Path, token: str | None = None, guard=None) -> bool:
    """Run one evaluator request; True when it exits 0."""
    log = request.with_suffix(".log")
    # The evaluator creates its output directory but not that directory's parent.
    command = json.loads(request.read_text(encoding="utf-8"))
    output = Path(command["output_directory"])
    if guard is not None:
        guard.check()
        guarded_paths(guard, request, command)
        require(checked(guard.plan["evaluator"]).resolve() == Path(executable).resolve(), "evaluator differs from guard pin")
        check_request_pins(command["sources"])
    output.parent.mkdir(parents=True, exist_ok=True)
    with log.open("x" if guard is not None else "w") as stream:
        if guard is None:
            child = subprocess.Popen([executable, str(request)], stdout=stream, stderr=subprocess.STDOUT,
                                     creationflags=getattr(subprocess, "BELOW_NORMAL_PRIORITY_CLASS", 0),
                                     env=dict(os.environ, CUDA_VISIBLE_DEVICES=""))
            reservations.record_descendant(token, child.pid)
        else:
            child = guard.spawn([executable, str(request)], stream, subprocess.STDOUT)
        code = child.wait()
    if guard is not None:
        guard.check()
    return code == 0


def held_token() -> str:
    token = os.environ.get(reservations.TOKEN_ENV)
    if not token or reservations.status(token).get("token_fate") != "holds":
        raise SystemExit("public panel execution runs only inside a held host reservation")
    return token


def run_requests(executable: str, requests: list[Path], workers: int, tolerate: bool = False, guard=None) -> float:
    """Run evaluator requests with at most ``workers`` concurrent processes; return wall seconds.
    With ``tolerate`` a failed request is left for ``recover`` instead of raising."""
    token = held_token() if guard is None else None
    if guard is not None:
        require(type(workers) is int and 1 <= workers <= len(guard.plan["cpu_affinity"]), "workers exceed guarded cores")
        guard.check()

    def one(request: Path) -> None:
        if not run_one(executable, request, token, guard) and not tolerate:
            raise RuntimeError(f"evaluator failed on {request}; see {request.with_suffix('.log')}")

    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=workers) as pool:
        list(pool.map(one, requests))
    if guard is not None:
        guard.quiet()
    return time.monotonic() - started


def recover(executable: str, request_path: Path, guard=None) -> dict:
    """A failed request stops at its first failing match: the match files before it are valid and the
    rest never ran. Record that match as a technical termination (incomplete, never dropped) and continue
    from the next match in a continuation request until every match is accounted for. Match files do
    not depend on sharding (qualification parity), so continuations reproduce them."""
    request_path = Path(request_path)
    request = json.loads(request_path.read_text(encoding="utf-8"))
    if guard is not None:
        guard.check()
        guarded_paths(guard, request_path, request)
    if request_path.with_suffix(".recovery.json").exists():
        recovery_for_request(request_path)
        if guard is not None:
            return pin(request_path.with_suffix(".recovery.json"))
    matches, base = request["matches"], request["output_directory"]
    segments, errors, start, current, k = [], [], 0, request_path, 0
    while True:
        if guard is not None:
            guard.check()
        current_request = json.loads(current.read_text(encoding="utf-8"))
        output = Path(current_request["output_directory"])
        start_for_request(current_request)
        done = len(sorted(output.glob("match-*.json")))
        segments.append({"request": pin(current), "output_directory": str(output).replace("\\", "/"),
                         "start": start, "count": done})
        if (output / "completion.json").exists():
            completion_for_request(json.loads(current.read_text(encoding="utf-8")))
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
        if request.get("game_one_only", False) and failed_physical_game(error, seed) != 1:
            raise ValueError("game-one failure does not name physical game 1")
        errors.append({"index": index, "seed": seed, "error": error})
        start = index + 1
        if start == len(matches):
            break
        k += 1
        current = request_path.with_name(f"{request_path.stem}-c{k}.json")
        continuation = dict(request, matches=matches[start:], output_directory=f"{base}-c{k}")
        if guard is not None and current.exists():
            require(json.loads(current.read_text(encoding="utf-8")) == continuation, "existing continuation changed")
        else:
            write_json(current, continuation, exclusive=guard is not None)
        output = Path(f"{base}-c{k}")  # an interrupted recovery: reuse a finished continuation, set aside a partial one
        if output.exists() and not (output / "completion.json").exists() and not (output / "failure.json").exists():
            output.rename(output.with_name(f"{output.name}.partial-{int(time.time())}"))
        if not output.exists():
            run_one(executable, current, held_token() if guard is None else None, guard)
    return write_json(request_path.with_suffix(".recovery.json"),
                      {"request": pin(request_path), "segments": segments, "errors": errors}, exclusive=guard is not None)


def validate_saved_match(result, expected, game_one_only):
    require(result["match"] == expected, "saved native case differs from request")
    if game_one_only:
        require(result.get("schema") == "public-input-evaluation-game-one/v1"
                and result.get("game_one_only") is True and len(result["games"]) == 1
                and "outcome" not in result and result.get("sideboard_decisions") == [],
                "saved game-one result shape differs")
    else:
        require(result.get("schema") == "public-input-evaluation-match/v1"
                and not result.get("game_one_only", False) and "outcome" in result,
                "saved BO3 result shape differs")


def match_hashes(request: Path, *, validate_cases: bool = False) -> list[str]:
    config = json.loads(Path(request).read_text(encoding="utf-8"))
    output = Path(config["output_directory"])
    completion = completion_for_request(config)
    files = sorted(output.glob("match-*.json"))
    hashes = [sha256_bytes(path.read_bytes()) for path in files]
    if hashes != completion["match_sha256"]:
        raise ValueError(f"match files differ from completion record in {output}")
    if validate_cases:
        require(completion["schema"] == "public-input-evaluation-completion/v1"
                and len(files) == len(config["matches"]) == completion["matches"],
                "qualification native case count differs")
        for path, expected in zip(files, config["matches"]):
            result = json.loads(path.read_text(encoding="utf-8"))
            validate_saved_match(result, expected, config.get("game_one_only", False))
            if config.get("game_one_only", False):
                require(completion["natural_games"] == len(files), "qualification natural-game count differs")
    return hashes


def explicit_segment_files(request_path: Path, count: int) -> list[Path]:
    """Validate a completed segment or a natural prefix before its recorded failure."""
    request = json.loads(request_path.read_text(encoding="utf-8"))
    output = Path(request["output_directory"])
    found = sorted(output.glob("match-*.json"))
    require([path.name for path in found] == [f"match-{index:06}.json" for index in range(count)],
            "explicit segment prefix count/order differs")
    if (output / "completion.json").exists():
        match_hashes(request_path, validate_cases=True)
        return found
    start_for_request(request)
    require((output / "failure.json").is_file() and count < len(request["matches"]),
            "partial prefix lacks its original native failure")
    failure = json.loads((output / "failure.json").read_text(encoding="utf-8"))
    require(failed_physical_game(failure["error"], request["matches"][count]["config"]["seed"]) == 1,
            "partial prefix failure differs from the next scheduled case")
    start = json.loads((output / "start.json").read_text(encoding="utf-8"))
    for path, expected in zip(found, request["matches"][:count]):
        result = json.loads(path.read_text(encoding="utf-8"))
        validate_saved_match(result, expected, True)
        require(result["models"] == start["models"], "partial prefix model identity differs from native start")
    return found


def qualify(plan_path: Path, sample: int | None, workers: list[int], root: Path, executable: str,
            guard=None, qualification_cases=None) -> dict:
    plan = json.loads(Path(plan_path).read_text(encoding="utf-8"))
    require_guard(plan, guard)
    validate_explicit_plan(plan)
    if guard is not None:
        from archive_distillation_dispatch_v1 import absolute
        guard.check()
        require(absolute(root).is_relative_to(guard.root), "qualification publication escapes guarded attempt")
    first = request_for_plan(plan, plan["requests"][0])
    representative = None
    if "cases_input" in plan:
        require(qualification_cases is not None, "pinned qualification-only cases required")
        require(qualification_cases["sha256"] != plan["cases_input"]["sha256"], "qualification must use a separate case schedule")
        representative = explicit_cases(qualification_cases, plan["metadata"])
        formal_seeds = {case["seed"] for item in plan["requests"] for case in item["cases"]}
        require(not formal_seeds.intersection(case["seed"] for case in representative), "qualification seeds overlap outcome schedule")
        require({case["seat"] for case in representative} == {0, 1}
                and {case["own"] for case in representative} == set(range(9))
                and {case["other"] for case in representative} == set(range(9)), "qualification must cover both seats and all decks")
        require(sample is None or sample == len(representative), "sample must cover all pinned qualification cases")
        sample = len(representative)
    else:
        sample = 16 if sample is None else sample
    require(type(sample) is int and sample > 0, "positive qualification sample required")
    matches = first["matches"][:sample]
    if guard is not None:
        require(workers == sorted(set(workers)) and workers[0] == 1 and any(k > 1 for k in workers),
                "guarded qualification needs serial and parallel worker counts")
    trials = []
    golden = None
    for k in workers:
        phase_started = time.monotonic() if guard is not None else None
        if representative is None and sample % k:
            raise ValueError("sample must split evenly across workers")
        paths, keyed = [], []
        groups = [(None, first, matches)]
        if representative is not None:
            groups = []
            for seat in (0, 1):
                template = next(request_for_plan(plan, item) for item in plan["requests"] if item["seat"] == seat)
                groups.append((seat, template, [case_match(case, plan["decks"]) for case in representative if case["seat"] == seat]))
        for seat, template, group in groups:
            for index in range(k):
                part = group[index * len(group) // k:(index + 1) * len(group) // k]
                if not part:
                    continue
                name = f"{index}" if seat is None else f"seat{seat}-{index}"
                request = dict(template, matches=part,
                               output_directory=str(Path(root) / f"w{k}" / f"native-{name}").replace("\\", "/"))
                path = Path(root) / f"w{k}" / f"request-{name}.json"
                write_json(path, request, exclusive=guard is not None)
                paths.append(path)
                keyed.extend(part)
        execution_seconds = run_requests(executable, paths, k, guard=guard)
        hashes = [h for path in paths for h in
                  (match_hashes(path, validate_cases=True) if guard is not None else match_hashes(path))]
        require(len(hashes) == len(keyed) and (representative is None or len(hashes) == sample),
                "qualification output omits scheduled cases")
        golden = golden or hashes
        trial = {"workers": k, "matches": len(hashes), "parity": hashes == golden}
        if guard is not None:
            trial["requests"] = [pin(path) for path in paths]
            trial["completions"] = [pin(Path(json.loads(path.read_text())["output_directory"]) / "completion.json")
                                    for path in paths]
            trial["execution_seconds"] = execution_seconds
        seconds = time.monotonic() - phase_started if guard is not None else execution_seconds
        trial.update(seconds=seconds, matches_per_second=len(hashes) / seconds)
        trials.append(trial)
    receipt = {"schema": RECEIPT, "evaluator_sha256": sha256_bytes(Path(executable).read_bytes()),
               "schedule_sha256": plan["schedule_sha256"], "opponent": plan["opponent"],
               "candidate_layout": candidate_layout(plan["candidate"]), "sample": sample, "trials": trials,
               "golden_match_sha256": golden, "selected_workers": max(
                   (t for t in trials if t["parity"]), key=lambda t: t["matches_per_second"])["workers"]}
    if plan.get("game_one_only", False):
        receipt["game_one_only"] = True
    if guard is not None:
        receipt["guard_binding"] = guard_binding(guard)
        receipt["qualified_candidate"] = plan["candidate"]
        receipt["guard_result"] = str(guard.root / "guard-result.json")
        require(all(trial["parity"] for trial in trials), "guarded serial/parallel output parity failed")
    if representative is not None:
        receipt["qualification_cases"] = qualification_cases
        receipt["case_metadata"] = plan["metadata"]
        receipt["case_order"] = [case for seat in (0, 1) for case in representative if case["seat"] == seat]
    receipt_pin = write_json(Path(root) / "receipt.json", receipt, exclusive=guard is not None)
    if guard is not None:
        if not hasattr(guard, "public_qualifications"):
            guard.public_qualifications = []
        guard.public_qualifications.append(receipt_pin)
    return receipt


def candidate_layout(source: dict) -> str:
    """Candidate model layout (import, feature contract, parameter shapes), not its weights."""
    checkpoint = json.loads(checked(source["checkpoint"]).read_text(encoding="utf-8"))
    layout = {key: checkpoint[key] for key in ("schema", "source_import", "feature_contract_digest",
                                               "feature_encoding_digest", "card_db_hash")}
    layout["parameters"] = [(row["name"], row["shape"]) for row in checkpoint["parameters"]]
    layout["play_import"] = source["play_import"]["sha256"]
    return sha256_bytes(json.dumps(layout, sort_keys=True).encode())


def run(plan_path: Path, receipt_path: Path, executable: str, guard=None) -> dict:
    plan = json.loads(Path(plan_path).read_text(encoding="utf-8"))
    require_guard(plan, guard)
    validate_explicit_plan(plan)
    receipt = json.loads(Path(receipt_path).read_text(encoding="utf-8"))
    if receipt["schema"] != RECEIPT or receipt["evaluator_sha256"] != sha256_bytes(Path(executable).read_bytes()) \
            or receipt["schedule_sha256"] != plan["schedule_sha256"] or receipt["opponent"] != plan["opponent"] \
            or receipt["candidate_layout"] != candidate_layout(plan["candidate"]) \
            or receipt.get("game_one_only", False) != plan.get("game_one_only", False):
        raise SystemExit("qualification receipt does not cover this evaluator, schedule, opponent or model layout")
    if not all(trial["parity"] for trial in receipt["trials"]) or receipt["trials"][0]["workers"] != 1:
        raise SystemExit("receipt lacks a serial golden or parallel parity")
    if guard is not None:
        require(receipt.get("guard_binding") == guard_binding(guard), "qualification lacks compatible guarded evidence")
        completed = json.loads(Path(receipt["guard_result"]).read_text(encoding="utf-8"))
        require(completed["complete"] is True and completed["qualification"] is True
                and pin(receipt_path) in completed.get("qualifications", []) and completed["guard"]["active_processes"] == 0,
                "qualification lacks successful drained publication")
        checked(completed["guard_plan"])
        require(any(trial["workers"] > 1 for trial in receipt["trials"]), "qualification lacks parallel evidence")
        fastest = max(receipt["trials"], key=lambda trial: trial["matches_per_second"])["workers"]
        require(receipt["selected_workers"] == fastest, "qualification does not select fastest worker count")
        if "cases_input" in plan:
            require(all(receipt["case_metadata"][key] == plan["metadata"][key] for key in ("lineage", "reference")),
                    "qualification lineage/reference differs")
            qualification = explicit_cases(receipt["qualification_cases"], plan["metadata"])
            require(receipt["case_order"] == [case for seat in (0, 1) for case in qualification if case["seat"] == seat],
                    "qualification case coverage changed")
        for trial in receipt["trials"]:
            require(len(trial["requests"]) == len(trial["completions"]), "qualification completion coverage changed")
            hashes, actual_matches = [], []
            for request_ref, completion_ref in zip(trial["requests"], trial["completions"]):
                request_path = checked(request_ref)
                measured = json.loads(request_path.read_text(encoding="utf-8"))
                require(checked(completion_ref).resolve() == (Path(measured["output_directory"]) / "completion.json").resolve(),
                        "qualification completion is borrowed")
                require(measured.get("game_one_only", False) == plan.get("game_one_only", False)
                        and measured["capture_decisions"] is False, "qualification execution mode changed")
                sources = [candidate_source(receipt["qualified_candidate"]), plan["opponent"]]
                if "cases_input" in plan and receipt["case_order"][len(actual_matches)]["seat"] == 1:
                    sources.reverse()
                require(measured["sources"] == sources or "cases_input" not in plan and measured["sources"] == sources[::-1],
                        "qualification sources changed")
                hashes.extend(match_hashes(request_path, validate_cases=True))
                actual_matches.extend(measured["matches"])
            require(hashes == receipt["golden_match_sha256"] and len(hashes) == trial["matches"],
                    "qualification outputs changed")
            if "cases_input" in plan:
                require(actual_matches == [case_match(case, plan["decks"]) for case in receipt["case_order"]],
                        "qualification native cases changed")
            require(type(trial["seconds"]) in (int, float) and trial["seconds"] > 0
                    and trial["matches_per_second"] == trial["matches"] / trial["seconds"],
                    "invalid qualification timing")
    paths = [checked(item["request"]) for item in plan["requests"]]
    for item in plan["requests"]:
        request_for_plan(plan, item)
    pending = []
    for path in paths:  # a resumed run keeps finished and failed outputs; a partial one is set aside
        request = json.loads(path.read_text(encoding="utf-8"))
        if guard is not None:
            guard.check()
            guarded_paths(guard, path, request)
        output = Path(request["output_directory"])
        if (output / "completion.json").exists():
            completion_for_request(request)
        elif (output / "failure.json").exists():
            start_for_request(request)
        if output.exists() and not (output / "completion.json").exists() and not (output / "failure.json").exists():
            output.rename(output.with_name(f"{output.name}.partial-{int(time.time())}"))
        if not output.exists():
            pending.append(path)
    seconds = run_requests(executable, pending, receipt["selected_workers"], tolerate=True, guard=guard)
    completions = []
    for path in paths:
        output = Path(json.loads(path.read_text(encoding="utf-8"))["output_directory"])
        recovery = path.with_suffix(".recovery.json")
        if recovery.exists():
            recovery_for_request(path)
            completions.append(pin(recovery))
        elif (output / "completion.json").exists():
            completions.append(pin(output / "completion.json"))
        else:
            completions.append(recover(executable, path, guard))
    result = {"plan": pin(plan_path), "receipt": pin(receipt_path), "workers": receipt["selected_workers"],
              "seconds": seconds, "completions": completions}
    write_json(Path(plan_path).parent / "run.json", result, exclusive=guard is not None)
    return result


def rows(plan_path: Path) -> list[dict]:
    """One row per case from game 1: candidate win 1, draw 0.5, loss 0. A match file is written only when
    every game ended naturally, so a game 1 without a winner is a natural draw. A failed match is a
    technical termination: incomplete, with its error and the BO3 game it failed in."""
    plan = json.loads(Path(plan_path).read_text(encoding="utf-8"))
    validate_explicit_plan(plan)
    out = []
    for item in plan["requests"]:
        request_path = checked(item["request"])
        request = request_for_plan(plan, item)
        recovery_path = request_path.with_suffix(".recovery.json")
        if recovery_path.exists():
            recovery = recovery_for_request(request_path)
            segments = [(Path(s["output_directory"]), s["start"], s["count"], checked(s["request"]))
                        for s in recovery["segments"]]
            errors = {e["index"]: e for e in recovery["errors"]}
        else:
            start_for_request(request)
            segments, errors = [(Path(request["output_directory"]), 0, len(item["cases"]), request_path)], {}
        files, request_bindings = {}, {}
        for output, start, count, segment_request in segments:
            found = (explicit_segment_files(segment_request, count) if "cases_input" in plan
                     else sorted(output.glob("match-*.json")))
            if len(found) != count:
                raise ValueError(f"{output} holds {len(found)} matches, expected {count}")
            files.update({start + i: path for i, path in enumerate(found)})
            if "cases_input" in plan:
                binding = pin(segment_request)
                request_bindings.update({start + i: binding for i in range(count)})
                if (output / "failure.json").exists():
                    failure = json.loads((output / "failure.json").read_text(encoding="utf-8"))
                    require(start + count in errors and errors[start + count]["error"] == failure["error"],
                            "recovery technical termination differs from its native failure")
                    request_bindings[start + count] = binding
        if sorted(list(files) + list(errors)) != list(range(len(item["cases"]))):
            raise ValueError(f"{request_path}: matches and technical terminations do not cover every case")
        for index, case in enumerate(item["cases"]):
            row = {"panel": plan["panel"], "checkpoint_sha256": plan["candidate"]["checkpoint"]["sha256"],
                   "own": case["own"], "other": case["other"], "seat": case["seat"], "seed": case["seed"]}
            if "cases_input" in plan:
                require(index in request_bindings, "case lacks its original native request binding")
                row.update(plan["metadata"], family=case["family"], request=request_bindings[index])
            else:
                row["repeat"] = case["repeat"]
            if index in errors:
                error = errors[index]["error"]
                failed_game = failed_physical_game(error, case["seed"])
                if errors[index]["seed"] != case["seed"] or failed_game is None:
                    raise ValueError("technical termination does not match the plan")
                if plan.get("game_one_only", False) and failed_game != 1:
                    raise ValueError("game-one failure does not name physical game 1")
                out.append({**row, "starting_player": case["starting_player"], "score": None, "complete": False,
                            "technical": error.split(": ", 1)[-1][:160], "technical_game": failed_game,
                            "match_sha256": None})
                continue
            path = files[index]
            match = json.loads(path.read_text(encoding="utf-8"))
            if "cases_input" in plan:
                validate_saved_match(match, case_match(case, plan["decks"]), True)
            if plan.get("game_one_only", False):
                if match.get("schema") != "public-input-evaluation-game-one/v1" \
                        or match.get("game_one_only") is not True or len(match["games"]) != 1 \
                        or "outcome" in match or match.get("sideboard_decisions") != []:
                    raise ValueError("game-one result is not one natural game without a match outcome")
            elif match.get("game_one_only", False):
                raise ValueError("game-one result supplied for a BO3 plan")
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


def with_guard(guard_plan: Path, root: Path, qualifying: bool, operation):
    """One host_slots-backed resource budget covers the whole operation and publication.

    This reuses the archive dispatcher's process/resource guard only. It does
    not invoke its archive-specific workload or execute a training command.
    """
    import archive_distillation_dispatch_v1 as bounded
    plan = json.loads(Path(guard_plan).read_text(encoding="utf-8"))
    require(plan["schema"] == GUARD_SCHEMA, "wrong public panel guard schema")
    cores = plan["cpu_affinity"]
    require(cores and len(cores) == len(set(cores))
            and all(type(core) is int and 0 <= core < min(64, os.cpu_count() or 1) for core in cores),
            "invalid guarded CPU affinity")
    limits = plan["qualification_limits" if qualifying else "run_limits"]
    for name in ("cpu_seconds", "wall_seconds"):
        bounded.native.positive(limits[name], name)
    storage = plan["storage"]
    require(type(storage["max_physical_bytes"]) is int and storage["max_physical_bytes"] > 0
            and type(storage["projected_additional_bytes"]) is int and storage["projected_additional_bytes"] > 0,
            "positive physical cap and write projection required")
    require(storage["reserve_bytes"] >= bounded.DISK_RESERVE_BYTES and plan["stop_files"], "reserve and STOP files required")
    roots = [bounded.absolute(path) for path in storage["accounting_roots"]]
    require(roots and len({path.drive.upper() for path in roots}) == 1
            and all(not a.is_relative_to(b) for i, a in enumerate(roots) for j, b in enumerate(roots) if i != j),
            "disjoint storage roots on one volume required")
    for path in plan["stop_files"]:
        bounded.absolute(path)
    guard = bounded.Guard(plan, root, qualifying)
    result = {"schema": "nine-deck-public-panel-guard-result/v1", "complete": False,
              "guard_plan": pin(guard_plan), "qualification": qualifying}
    failure = None
    try:
        checked(plan["evaluator"])
        result["result"] = operation(guard)
        guard.quiet()
        result["complete"] = True
    except BaseException as error:
        failure = error
    finally:
        try:
            if failure is not None:
                result.update(complete=False, error=str(failure))
            result["guard"] = guard.terminate_and_drain()
            result["qualifications"] = getattr(guard, "public_qualifications", [])
            result_path = guard.root / "guard-result.json"
            write_json(result_path, result, exclusive=True)
            if failure is None:
                try:
                    guard.check()
                except BaseException as error:
                    failure = error
                    result.update(complete=False, error=str(error), guard=guard.terminate_and_drain())
                    result_path.rename(guard.root / "guard-result-before-publication-failure.json")
                    write_json(result_path, result, exclusive=True)
        finally:
            guard.close()
    if failure is not None:
        raise failure
    return result["result"]


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
    sub.add_argument("--plan", type=Path); sub.add_argument("--sample", type=int)
    sub.add_argument("--workers", default="1,2,4,8"); sub.add_argument("--root", type=Path)
    sub.add_argument("--receipt", type=Path); sub.add_argument("--executable")
    sub.add_argument("--game-one-only", action="store_true")
    sub.add_argument("--cases", type=Path); sub.add_argument("--metadata", type=Path)
    sub.add_argument("--native-root", type=Path)
    sub.add_argument("--qualification-cases", type=Path)
    sub.add_argument("--guard-plan", type=Path); sub.add_argument("--guard-root", type=Path)
    a = sub.parse_args(args.rest)
    if args.op == "build":
        if a.cases:
            require(a.metadata is not None, "explicit cases require metadata")
            plan = build_from_cases(a.panel, json.loads(a.candidate.read_text()), json.loads(a.opponent.read_text()),
                                    ndb.load_decks(a.decks), a.out, a.shards, pin(a.cases), json.loads(a.metadata.read_text()),
                                    game_one_only=True, native_root=a.native_root)
        else:
            plan = build(a.panel, json.loads(a.candidate.read_text()), json.loads(a.opponent.read_text()),
                         ndb.load_decks(a.decks), a.out, a.shards, game_one_only=a.game_one_only)
        print(json.dumps({"requests": len(plan["requests"]), "cases": plan["cases"]}))
    elif args.op == "qualify":
        operation = lambda guard: qualify(a.plan, a.sample, [int(x) for x in a.workers.split(",")], a.root, a.executable,
                                           guard, pin(a.qualification_cases) if a.qualification_cases else None)
        if a.guard_plan:
            require(a.guard_root is not None, "--guard-root required")
            result = with_guard(a.guard_plan, a.guard_root, True, operation)
        else:
            result = operation(None)
        print(json.dumps(result["trials"]))
    elif args.op == "run":
        operation = lambda guard: run(a.plan, a.receipt, a.executable, guard)
        if a.guard_plan:
            require(a.guard_root is not None, "--guard-root required")
            result = with_guard(a.guard_plan, a.guard_root, False, operation)
        else:
            result = operation(None)
        print(json.dumps(result))
    else:
        result = rows(a.plan)
        if "cases_input" in json.loads(a.plan.read_text()):
            with a.out.open("x", encoding="utf-8", newline="\n") as stream:
                stream.write("".join(json.dumps(row, sort_keys=True) + "\n" for row in result))
        else:
            a.out.write_text("".join(json.dumps(row, sort_keys=True) + "\n" for row in result), encoding="utf-8")
        print(json.dumps({"rows": len(result), "complete": sum(row["complete"] for row in result)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
