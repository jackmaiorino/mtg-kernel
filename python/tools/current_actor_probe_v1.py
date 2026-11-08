"""Prepare and guard the prospective #454 whole-game numeric actor probe.

The frozen probe and results are read-only inputs. No games run during prepare.
sample is a bounded correctness/scaling leg of at most four games. run requires
matched serial/parallel completed-work evidence and current placement inventory.
"""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

from compute_throughput_v1 import checked, nonnegative, read

SCHEMA = "current-actor-coverage-probe/v1"
SELECTOR = "splitmix64-rejection-uniform/v1;seed=environment_seed^0x454a11;all-current-tree-fixture-logits-checked"
TEST = "current_actor_probe_v1::reported_pool_digest_free_terminal_probe"
PINS = {
    "decks-limited.json": "40873f95532cd436fcdf41664aadfb0ce42c6b4664d86fc364a9e3d8421a6768",
    "decks-eternal.json": "148cbd2fb728d2700c9e77a6a6da06c56efffc3494dc988ed56b4974ea93099e",
    "configs/ceiling-probe.json": "e0384ff0ef1bad3a6e704e21702bab5db6f1f247790b69e9c083721250eb945d",
}
LIMITED_REGISTRY_PINS = {
    "data/cards_v1.json": "ef738001c373076039f211582f5fd07b11903788e70641ec7e23c84ba4f69b14",
    "data/limited/fdn_v1/cards_v1.json": "91dcd869bb339450d0adc3e8cfa1a8320f560e53c337f21fb263a7a059839e13",
}
ETERNAL = {"legacy-reanimator", "legacy-doomsday", "vintage-red-prison"}


def pin(path):
    path = Path(path).resolve()
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"path": str(path), "sha256": digest}


def write_new(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")


def pinned_read(root, relative, expected, inputs):
    item = pin(Path(root) / relative)
    if item["sha256"] != expected:
        raise ValueError(f"historical input changed: {relative}")
    inputs.append(item)
    return read(item["path"])


def prepare(args):
    inputs = []
    pool = pinned_read(args.original_root, f"decks-{args.pool}.json", PINS[f"decks-{args.pool}.json"], inputs)
    config = pinned_read(args.original_root, "configs/ceiling-probe.json", PINS["configs/ceiling-probe.json"], inputs)
    template = config["iterations"][0]["episodes"][0]["episode"]
    if args.pool == "limited":
        registry = {}
        for relative, expected in LIMITED_REGISTRY_PINS.items():
            for card in pinned_read(args.historical_repo, relative, expected, inputs)["cards"]:
                if card["name"] in registry:
                    raise ValueError("duplicate historical registered name")
                registry[card["name"]] = card
        admitted = []
        for deck in pool:
            counts = Counter()
            rows = deck["mainboard"] + deck["sideboard"]
            for count, name in rows:
                counts[name] += count
            if all(name in registry and registry[name]["engine_capability"] == "full"
                   and not registry[name].get("is_token", False)
                   and (count <= 4 or "Basic" in registry[name].get("supertypes", []))
                   for name, count in counts.items()):
                admitted.append(deck)
        if len(pool) != 80 or len(admitted) != 43:
            raise ValueError("historical Limited admission order was not recovered")
    else:
        admitted = [deck for deck in pool if deck["label"] in ETERNAL]
        if len(pool) != 86 or len(admitted) != 3:
            raise ValueError("historical Eternal mirrors missing")
    labels = [deck["label"] for deck in admitted]
    if len(set(deck["label"] for deck in pool)) != len(pool):
        raise ValueError("duplicate deck label")
    if args.cohort == "original40":
        if args.pool != "limited":
            raise ValueError("original40 applies only to Limited")
        originals = []
        for deck in pool:
            path = Path(args.repo) / "data/limited/draftzero_sample_v1/decks" / (deck["label"] + ".json")
            inputs.append(pin(path))
            original = read(path)
            rows = [[row["count"], row["name"]] for row in original["cards"]]
            if original["name"] != deck["label"] or sum(row[0] for row in rows) != 40:
                raise ValueError("original Limited list differs")
            originals.append(dict(label=deck["label"], mainboard=rows, sideboard=[]))
        pool = originals
    log = Path(args.original_root) / f"digest-probe-{args.pool}.log"
    inputs.append(pin(log))
    required = {}
    for line in log.read_text(encoding="utf-8").splitlines():
        if not line.startswith("{"):
            continue
        row = json.loads(line)
        if row.get("probe") == "encode_stop":
            kind = row["key"].split(" | ", 1)[0]
            if kind not in {"Surface", "AttackerInclusion", "BlockerInclusion"}:
                raise ValueError("unrecognized historical decision kind")
            for pair in row["decks"]:
                decks = tuple(pair.split(" vs "))
                if len(decks) != 2 or any(label not in labels for label in decks):
                    raise ValueError("reported pair absent from historical admitted pool")
                required.setdefault(decks, set()).add(kind)
    if not required:
        raise ValueError("historical stop rows are required")
    games = []
    for game in range(60):
        index = game % (2 * len(labels))
        games.append(dict(id=f"current-actor-{args.pool}-{args.cohort}-{game}",
                          historical_episode_index=index, seed=0x5EED0000 + index,
                          starting_player=f"p{index % 2}",
                          decks=[labels[index % len(labels)], labels[(index * 7 + 3) % len(labels)]]))
    manifest = dict(schema=SCHEMA, pool=args.pool, cohort=args.cohort, selector=SELECTOR,
                    runtime="KaitoV75/Foundations-combat/London/environment-v2" if args.pool == "limited"
                    else "EternalActorV1/native-Full/environment-v2",
                    provenance=dict(historical_engine="3ce35a984703309eadb3dc5ce79b70b456eee43e",
                                    historical_collab="487784dcca9b70d009844dacd72d6a2aa216e032",
                                    historical_runtime=template.get("runtime_rules"),
                                    historical_selector="FrozenPlayPolicyV1::training_fixture_v4",
                                    same_policy_trajectory=False, inputs=inputs, historical_admitted_order=labels),
                    max_physical_decisions=template["max_physical_decisions"],
                    max_policy_steps=template["max_policy_steps"], decks=pool, games=games,
                    requirements=[dict(decks=list(decks), kinds=sorted(kinds)) for decks, kinds in sorted(required.items())])
    write_new(args.output, manifest)


def require_choice(path, binary, manifest):
    plan = read(path)
    if plan.get("schema") != "current-actor-probe-compute-choice/v1":
        raise ValueError("unsupported probe allocation")
    inventory = plan["inventory"]
    if set(inventory) != {"desktop", "computehost", "runpod"}:
        raise ValueError("inventory must cover Jack, HaleysPC and RunPod")
    now = datetime.now(timezone.utc)
    for host, status in inventory.items():
        age = (now - datetime.fromisoformat(status["checked_at"])).total_seconds()
        if not 0 <= age <= 86400 or type(status["eligible"]) is not bool or not status["reason"]:
            raise ValueError(f"refresh placement evidence for {host}")
        checked(status["evidence"])
    candidates, primary = [], set()
    for candidate in plan["candidates"]:
        receipt = read(checked(candidate["receipt"]))
        report = read(checked(receipt["report"]))
        checked(receipt["binary"]); checked(receipt["manifest"])
        if receipt["binary"]["sha256"] != binary["sha256"] or receipt["manifest"]["sha256"] != manifest["sha256"]:
            raise ValueError("qualification binary or full workload changed")
        if receipt["exit_code"] != 0 or report["attempted_games"] != 4 or report["completed_games"] != 4 or report["ordinal_neutral_collisions"] != 0:
            raise ValueError("incomplete games cannot qualify throughput")
        if report["input_sha256"] != manifest["sha256"] or report["workers"] != receipt["workers"]:
            raise ValueError("receipt/report bindings differ")
        host = candidate["host"]
        if host not in inventory or not inventory[host]["eligible"] or receipt["host"] != inventory[host]["hostname"]:
            raise ValueError("candidate host is unavailable or changed")
        seconds = nonnegative(receipt["seconds"])
        if seconds <= 0:
            raise ValueError("invalid completed-work timing")
        projected = seconds * 15 + sum(nonnegative(candidate[k]) for k in ("setup_seconds", "transfer_seconds", "recovery_seconds"))
        candidates.append(dict(id=candidate["id"], host=host, hostname=receipt["host"], workers=receipt["workers"], projected_seconds=projected))
        primary.add(report["primary_sha256"])
    if len(primary) != 1 or not any(c["workers"] == 1 for c in candidates) or not any(c["workers"] > 1 for c in candidates):
        raise ValueError("require bit-identical serial and parallel completed games")
    if len({c["id"] for c in candidates}) != len(candidates):
        raise ValueError("duplicate allocation id")
    for host, status in inventory.items():
        if status["eligible"] and not any(c["host"] == host for c in candidates):
            raise ValueError(f"eligible host has no measured completed-work rate: {host}")
    chosen = next(c for c in candidates if c["id"] == plan["selected"])
    if chosen["projected_seconds"] != min(c["projected_seconds"] for c in candidates):
        raise ValueError("chosen placement is slower than a qualified alternative")
    if chosen["hostname"].casefold() != platform.node().casefold():
        raise ValueError("run on the selected host; this launcher does not provision cloud resources")
    return chosen


def execute(args):
    binary, manifest = pin(args.binary), pin(args.manifest)
    if args.command == "run":
        chosen = require_choice(args.compute_choice, binary, manifest)
        workers, count = chosen["workers"], 60
    else:
        workers, count = args.workers, args.count
        if not 1 <= workers <= count <= 4:
            raise ValueError("small correctness/scaling samples are bounded to four games")
    output = Path(args.output).resolve()
    report = output.with_suffix(".native.json")
    log = output.with_suffix(".log")
    if any(path.exists() for path in (output, report, log)):
        raise ValueError("preserve existing receipts; choose a fresh output")
    output.parent.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, MTG_CURRENT_ACTOR_PROBE_INPUT=manifest["path"], MTG_CURRENT_ACTOR_PROBE_OUTPUT=str(report),
               MTG_CURRENT_ACTOR_PROBE_COUNT=str(count), MTG_CURRENT_ACTOR_PROBE_WORKERS=str(workers))
    guard = Path(__file__).with_name("host_slots_v1.py")
    command = [sys.executable, str(guard), "run", "--lane", "actor454-probe", "--work-id", output.stem,
               "--cores", str(workers), "--priority", "below_normal", "--max-seconds", str(args.max_seconds),
               "--on-overdue", "kill", "--cwd", str(Path(args.repo).resolve())]
    if args.core_pool:
        command += ["--core-pool", args.core_pool, "--pool-reason", "isolation"]
    command += ["--", binary["path"], TEST, "--exact", "--ignored", "--nocapture"]
    started = time.monotonic()
    with log.open("x", encoding="utf-8") as stream:
        result = subprocess.run(command, env=env, stdout=stream, stderr=subprocess.STDOUT, check=False)
    receipt = dict(schema="current-actor-probe-execution/v1", host=platform.node(), binary=binary, manifest=manifest,
                   workers=workers, count=count, seconds=time.monotonic()-started, exit_code=result.returncode,
                   report=pin(report) if report.exists() else None, log=pin(log),
                   compute_choice=pin(args.compute_choice) if args.command == "run" else None)
    write_new(output, receipt)
    return result.returncode


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    prepare_parser = sub.add_parser("prepare")
    prepare_parser.add_argument("--pool", choices=("limited", "eternal"), required=True)
    prepare_parser.add_argument("--cohort", choices=("original40", "historical60"), required=True)
    prepare_parser.add_argument("--original-root", required=True)
    prepare_parser.add_argument("--historical-repo", required=True)
    prepare_parser.add_argument("--repo", required=True)
    prepare_parser.add_argument("--output", required=True)
    for name in ("sample", "run"):
        command = sub.add_parser(name)
        for option in ("binary", "manifest", "repo", "output"):
            command.add_argument("--" + option, required=True)
        command.add_argument("--core-pool")
        command.add_argument("--max-seconds", type=float, required=True)
        if name == "sample":
            command.add_argument("--workers", type=int, required=True)
            command.add_argument("--count", type=int, default=4)
        else:
            command.add_argument("--compute-choice", required=True)
    args = parser.parse_args()
    return prepare(args) if args.command == "prepare" else execute(args)


if __name__ == "__main__":
    raise SystemExit(main())
