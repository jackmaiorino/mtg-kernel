"""Phase-2 and O-reproduction training configs of the Spy archive distillation screen (DESIGN.md sections 2 and 6).

Both are O's recorded run config (the archive reset pilot's r1-O or r2-O:
spy-reset-pilot-v1's 162 updates, unit seeds, decks, seats, starting players,
trainer fields and the fixed A48 opponent), with one change: O's "initial"
units (j = 3 to 7) play the parent as the named fixed opponent "parent",
whose source is O's initial_source verbatim. The O reproduction keeps O's
initial source and must reproduce O's recorded final state, which proves the
mapping exact; phase 2 starts from the selected phase-1 checkpoint's source.
Episode ids stay O's, so non-natural retry seeds match too.

CLI (JSON on stdout):
  config --lineage r1 --o-config O.json (--source SRC.json | --reproduce)
         --workers W --output-directory DIR --out CFG.json
  check  --lineage r1 --completion RUN/completion.json --recorded O_FINAL_CHECKPOINT.json
``check`` compares the run's final checkpoint state with O's recorded one.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from distill_common import (O_FINAL_CHECKPOINT_SHA256, PARENT_ID, absolute, check_parent, pilot,  # noqa: E402
                    read_json, require, write_json)

INITIAL = {"kind": "initial"}
NAMED_PARENT = {"kind": "fixed", "id": PARENT_ID}
RUN_SCHEMA = pilot.ndb.SCHEMA
# O's "initial" units: j = 3 to 7 of every ordered pair, both seats.
INITIAL_EPISODES = pilot.DECKS * pilot.DECKS * 5 * 2


def expected_iterations(lineage: str, decks: list[dict]) -> list[dict]:
    return pilot.iterations(lineage, "O", decks, None)[0]


def config(lineage: str, o_config: dict, source: dict | None, workers: int, output_directory: str,
           decks: list[dict] | None = None) -> dict:
    """O's config with the named parent; ``source`` None reproduces O."""
    require(o_config.get("schema") == RUN_SCHEMA, "not a native expanded training run config")
    parent = o_config["initial_source"]
    check_parent(lineage, parent)
    decks = decks if decks is not None else pilot.load_decks()
    require(o_config["iterations"] == expected_iterations(lineage, decks),
            "O's recorded schedule differs from spy-reset-pilot-v1's O schedule")
    opponents = o_config["opponents"]
    require(len(opponents) == 1 and opponents[0]["id"] == pilot.FIXED_OPPONENT_ID,
            "O's recorded opponents are not the fixed A48 alone")
    for field, value in pilot.ndb.FROZEN.items():
        require(o_config[field] == value, f"O's {field} differs from the frozen trainer")
    require(type(workers) is int and workers >= 1, "invalid worker count")
    value = copy.deepcopy(o_config)
    remapped = 0
    for update in value["iterations"]:
        for item in update["episodes"]:
            if item["opponent"] == INITIAL:
                item["opponent"] = dict(NAMED_PARENT)
                remapped += 1
    require(remapped == INITIAL_EPISODES, "O's initial units are not j = 3 to 7 of every pair")
    value["opponents"] = copy.deepcopy(opponents) + [{"id": PARENT_ID, "source": copy.deepcopy(parent)}]
    if source is not None:
        require(source.get("checkpoint") is not None and source != parent,
                "phase 2 starts from a phase-1 checkpoint")
        require({k: v for k, v in source.items() if k != "checkpoint"}
                == {k: v for k, v in parent.items() if k != "checkpoint"},
                "the phase-1 source differs from the parent's import or feature transfer")
        value["initial_source"] = source
    value["collection_workers"] = workers
    value["preparation_workers"] = workers
    value["output_directory"] = absolute(output_directory)
    return value


def resolved_opponent(cfg: dict, assignment: dict, current: dict) -> dict:
    """The source the run driver gives an episode's other seat
    (native_expanded_training_run_v1 resolve_episodes)."""
    kind = assignment["kind"]
    if kind == "current":
        return current
    if kind == "initial":
        return cfg["initial_source"]
    require(kind == "fixed", f"unexpected opponent kind {kind!r}")
    matches = [o["source"] for o in cfg["opponents"] if o["id"] == assignment["id"]]
    require(len(matches) == 1, f"unknown or ambiguous opponent {assignment['id']!r}")
    return matches[0]


def check(lineage: str, completion: dict, recorded_path: Path) -> dict:
    """The run's final checkpoint state against O's recorded final state."""
    data = Path(recorded_path).read_bytes()
    require(hashlib.sha256(data).hexdigest() == O_FINAL_CHECKPOINT_SHA256[lineage],
            f"not O's recorded {lineage} final checkpoint")
    recorded = json.loads(data)
    require(completion.get("schema") == RUN_SCHEMA and completion.get("complete") is True
            and completion["completed_iterations"] == pilot.UPDATES, "not a complete 162-update run")
    final_pin = completion["source"]["checkpoint"]
    final_bytes = Path(final_pin["path"]).read_bytes()
    require(hashlib.sha256(final_bytes).hexdigest() == final_pin["sha256"], "final checkpoint differs from its pin")
    final = json.loads(final_bytes)
    same = final["state_sha256"] == recorded["state_sha256"] and final["adam_step"] == recorded["adam_step"]
    return {"lineage": lineage, "reproduced": same, "state_sha256": final["state_sha256"],
            "recorded_state_sha256": recorded["state_sha256"], "adam_step": final["adam_step"]}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    c = sub.add_parser("config")
    c.add_argument("--lineage", required=True, choices=pilot.LINEAGES)
    c.add_argument("--o-config", type=Path, required=True)
    start = c.add_mutually_exclusive_group(required=True)
    start.add_argument("--source", type=Path)
    start.add_argument("--reproduce", action="store_true")
    c.add_argument("--workers", type=int, required=True)
    c.add_argument("--output-directory", required=True)
    c.add_argument("--out", type=Path, required=True)
    k = sub.add_parser("check")
    k.add_argument("--lineage", required=True, choices=pilot.LINEAGES)
    k.add_argument("--completion", type=Path, required=True)
    k.add_argument("--recorded", type=Path, required=True)
    a = ap.parse_args(argv)
    if a.op == "config":
        value = config(a.lineage, read_json(a.o_config), None if a.reproduce else read_json(a.source),
                       a.workers, a.output_directory)
        print(json.dumps(write_json(a.out, value)))
    else:
        result = check(a.lineage, read_json(a.completion), a.recorded)
        print(json.dumps(result))
        return 0 if result["reproduced"] else 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
