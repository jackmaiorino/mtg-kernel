"""Phase-2 and O-reproduction training configs of the Spy archive distillation screen (DESIGN.md sections 2 and 6).

Both are O's recorded run config (the archive reset pilot's r1-O or r2-O:
spy-reset-pilot-v1's 162 updates, unit seeds, decks, seats, starting players,
trainer fields and the fixed A48 opponent), with one change: O's "initial"
units (j = 3 to 7) play the parent as the named fixed opponent "parent",
whose source is O's initial_source verbatim. The O reproduction keeps O's
initial source and must reproduce O's recorded final state, which proves the
mapping exact; phase 2 starts from the selected phase-1 checkpoint's source.
Episode ids stay O's, so non-natural retry seeds match too.

The Spy line retention trace (collab LANES/spy-line-retention-20261015,
DESIGN.md section 2) adds two options to a phase-2 config, both off by
default: ``--replicate K`` (K = 1 to 3) gives every unit a new seed and its
episodes new ids, keeping O's units, order, seats, starting players and
opponents; ``--drop-spy-learner`` removes the 90 units whose learner deck is
Spy (180 episodes), keeping the 162 updates in order. Its A-s0 run checks
against the screen's D2L checkpoint with ``check --recorded-sha256``.

Search-evidence distillation (SD1, collab
LANES/search-evidence-distillation-20261016 section 6) pairs D2-k, from its
phase-1 checkpoint D1-k (cell ``SD1-T-sK``), with O-k, from the parent, on
schedule seed k. ``--namespace sd1`` derives replicate K's (1 to 7) unit
seeds from ``sd1/train/repK`` and its episode ids from ``sd1``, so they share
no seed with the retention trace's; seed 0 is O's own schedule. Only under
that namespace does an SD1 cell's checkpoint start phase 2 (with K its own
seed), and ``--from-parent`` writes O-k (K >= 1). Without ``--namespace``
every config is as before.

CLI (JSON on stdout):
  config --lineage r1 --o-config O.json (--source SRC.json | --reproduce | --from-parent)
         --workers W --output-directory DIR --out CFG.json
         [--replicate K] [--drop-spy-learner] [--namespace sd1]
  check  --lineage r1 --completion RUN/completion.json --recorded O_FINAL_CHECKPOINT.json
         [--recorded-sha256 SHA]
``check`` compares the run's final checkpoint state and Adam step with a
recorded checkpoint's: O's final one, or the one whose SHA-256 is given.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
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
# The retention trace's replicate seeds (its DESIGN.md section 2).
RETENTION_NS = "spy-line-retention-v1"
REPLICATES = (1, 2, 3)
# SD1's phase-2 pairs (its DESIGN.md section 6): seeds 1 to 7 under its own
# namespace, and the phase-1 cells whose checkpoints start them.
SD1_NS = "sd1"
SD1_REPLICATES = tuple(range(1, 8))
SD1_CELL = re.compile(r"^SD1-T-s([0-7])$")
# SQ1 (collab LANES/soft-q-teaching-20261018, design section 3): four
# replicate seeds per lineage, every arm from the parent.
SQ1_NS = "sq1-20261018"
SQ1_REPLICATES = (1, 2, 3, 4)
NAMESPACES = {RETENTION_NS: REPLICATES, SD1_NS: SD1_REPLICATES, SQ1_NS: SQ1_REPLICATES}
FROM_PARENT_NAMESPACES = (SD1_NS, SQ1_NS)
SPY_LEARNER_EPISODES = pilot.DECKS * pilot.REPEATS * 2


def expected_iterations(lineage: str, decks: list[dict]) -> list[dict]:
    return pilot.iterations(lineage, "O", decks, None)[0]


def unit_id(lineage: str, entry: dict) -> str:
    """O's episode id of one schedule entry (pilot.episode)."""
    return f"{pilot.NS}-{lineage}-o{entry['own']}-t{entry['other']}-j{entry['j']}-s{entry['seat']}"


def replicate_seed(lineage: str, replicate: int, own: int, other: int, j: int,
                   namespace: str = RETENTION_NS) -> int:
    return pilot.seed([f"{namespace}/train/rep{replicate}", lineage, own, other, j])


def replicate_id(lineage: str, replicate: int, entry: dict, namespace: str = RETENTION_NS) -> str:
    return (f"{namespace}-{lineage}-rep{replicate}-o{entry['own']}-t{entry['other']}"
            f"-j{entry['j']}-s{entry['seat']}")


def sd1_cell_seed(saved: dict) -> int | None:
    """The seed of an SD1 phase-1 checkpoint's cell (``SD1-T-sK`` with a
    keep-set file), or None for any other checkpoint."""
    cell = (saved.get("archive_distillation") or {}).get("cell") or {}
    match = SD1_CELL.match(str(cell.get("cell", "")))
    return int(match.group(1)) if match and cell.get("keep_sets") is not None else None


def retention_iterations(lineage: str, iterations: list[dict], replicate: int,
                         drop_spy_learner: bool, namespace: str = RETENTION_NS) -> list[dict]:
    """O's updates with replicate ``replicate``'s unit seeds and ids under
    ``namespace`` (0 keeps O's) and, with ``drop_spy_learner``, without the
    Spy learner's units. Each recorded episode must be its schedule entry's,
    in order."""
    require(namespace in NAMESPACES, f"unknown replicate namespace {namespace!r}")
    require(replicate == 0 or replicate in NAMESPACES[namespace], f"unknown replicate {replicate!r}")
    plan = pilot.schedule(lineage)["updates"]
    require(len(plan) == len(iterations) == pilot.UPDATES, "O's updates differ from the pilot's schedule")
    out, dropped = [], 0
    for entries, update in zip(plan, iterations):
        require(set(update) == {"episodes"} and len(entries) == len(update["episodes"]),
                "O's update differs from the pilot's schedule")
        items = []
        for entry, item in zip(entries, update["episodes"]):
            recorded = item["episode"]
            require(recorded["id"] == unit_id(lineage, entry) and recorded["seed"] == entry["seed"]
                    and recorded["learner_seat"] == entry["seat"]
                    and recorded["starting_player"] == entry["starting_player"],
                    "O's episode differs from its schedule entry")
            if drop_spy_learner and entry["own"] == pilot.SPY:
                dropped += 1
                continue
            value = copy.deepcopy(item)
            if replicate:
                value["episode"]["id"] = replicate_id(lineage, replicate, entry, namespace)
                value["episode"]["seed"] = replicate_seed(lineage, replicate, entry["own"], entry["other"],
                                                          entry["j"], namespace)
            items.append(value)
        require(items, "an update would hold no episodes")
        out.append({"episodes": items})
    require(dropped == (SPY_LEARNER_EPISODES if drop_spy_learner else 0),
            "the Spy learner's units are not 90 units at both seats")
    return out


def config(lineage: str, o_config: dict, source: dict | None, workers: int, output_directory: str,
           decks: list[dict] | None = None, replicate: int = 0, drop_spy_learner: bool = False,
           namespace: str = RETENTION_NS) -> dict:
    """O's config with the named parent; ``source`` None reproduces O.
    ``replicate`` and ``drop_spy_learner`` are the retention trace's options
    and apply only to a phase-2 run from a phase-1 checkpoint. Under
    ``namespace`` SD1_NS, ``replicate`` (1 to 7) also applies from the parent
    itself (SD1's O-k), and an SD1 cell's checkpoint starts phase 2 on its
    own seed."""
    require(namespace in NAMESPACES, f"unknown replicate namespace {namespace!r}")
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
        # A U collapse diagnostic or BT1 cell's checkpoints are read offline
        # only; a phase 2 from one needs its own reviewed design. SD1's design
        # is that review for its own keep-set cells, under its namespace.
        saved = read_json(source["checkpoint"]["path"])
        seed = sd1_cell_seed(saved)
        if namespace == SD1_NS:
            require(seed is not None, "SD1's phase 2 starts from an SD1 cell's checkpoint")
            require(seed == replicate, f"D2-k starts from D1-k on seed k: this checkpoint is seed {seed}")
        else:
            require(((saved.get("archive_distillation") or {}).get("cell")) is None,
                    "a U collapse diagnostic cell's checkpoint does not start phase 2")
        value["initial_source"] = source
    require(namespace != SD1_NS or not drop_spy_learner, "SD1's phase 2 keeps every unit")
    require(namespace != SQ1_NS or (source is None and replicate >= 1 and not drop_spy_learner),
            "SQ1 runs start from the parent on replicate seeds 1 to 4 with every unit")
    if replicate or drop_spy_learner:
        require(source is not None or namespace in FROM_PARENT_NAMESPACES,
                "the retention options apply to phase 2, not to O's reproduction")
        value["iterations"] = retention_iterations(lineage, value["iterations"], replicate, drop_spy_learner,
                                                   namespace)
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


def check(lineage: str, completion: dict, recorded_path: Path, recorded_sha256: str | None = None) -> dict:
    """The run's final checkpoint state against a recorded checkpoint's: O's
    final one unless ``recorded_sha256`` names another."""
    data = Path(recorded_path).read_bytes()
    expected = recorded_sha256 or O_FINAL_CHECKPOINT_SHA256[lineage]
    require(hashlib.sha256(data).hexdigest() == expected,
            f"not O's recorded {lineage} final checkpoint" if recorded_sha256 is None
            else f"not the recorded {lineage} checkpoint {expected}")
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
    start.add_argument("--from-parent", action="store_true",
                       help="SD1's O-k: the parent itself on replicate K's seeds (--namespace sd1, K >= 1)")
    c.add_argument("--workers", type=int, required=True)
    c.add_argument("--output-directory", required=True)
    c.add_argument("--out", type=Path, required=True)
    c.add_argument("--replicate", type=int, choices=SD1_REPLICATES, default=0,
                   help=f"1 to 3 under {RETENTION_NS} (the default namespace), 1 to 7 under {SD1_NS}")
    c.add_argument("--drop-spy-learner", action="store_true")
    c.add_argument("--namespace", choices=tuple(NAMESPACES), default=RETENTION_NS)
    k = sub.add_parser("check")
    k.add_argument("--lineage", required=True, choices=pilot.LINEAGES)
    k.add_argument("--completion", type=Path, required=True)
    k.add_argument("--recorded", type=Path, required=True)
    k.add_argument("--recorded-sha256")
    a = ap.parse_args(argv)
    if a.op == "config" and a.replicate and a.replicate not in NAMESPACES[a.namespace]:
        ap.error(f"argument --replicate: {a.replicate} is not a replicate of namespace {a.namespace}")
    if a.op == "config":
        require(not a.reproduce or a.replicate == 0, "O's reproduction keeps O's own seeds")
        require(not a.from_parent or (a.namespace in FROM_PARENT_NAMESPACES and a.replicate >= 1 and not a.drop_spy_learner),
                "--from-parent writes SD1's O-k: --namespace sd1 with --replicate 1 to 7")
        start = None if a.reproduce or a.from_parent else read_json(a.source)
        value = config(a.lineage, read_json(a.o_config), start, a.workers, a.output_directory,
                       replicate=a.replicate, drop_spy_learner=a.drop_spy_learner, namespace=a.namespace)
        print(json.dumps(write_json(a.out, value)))
    else:
        result = check(a.lineage, read_json(a.completion), a.recorded, a.recorded_sha256)
        print(json.dumps(result))
        return 0 if result["reproduced"] else 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
