"""Explicit schedules for the G(i) fresh-init reference lineages (v1).

The nine-deck baseline recipe (nine_deck_baseline_v1.py, its design v3) with
exactly four fields changed (collab LANES/g-references-20261028/DESIGN.md
v1.2, section 3, and AMENDMENT-1.md v1.2):

* Start: each lineage G1..G8 starts from its own fresh initialization
  (``mtg-kernel-fresh-initialization-source/v1`` as ``play_import``, no
  checkpoint) with a fresh Adam, instead of T1.
* The ``fixed`` slot (j = 8, 9; 20%) plays the lineage's own history instead
  of A48. The pool for block b is the sorted list of the lineage's permanent
  block-end checkpoints at or below b - 2; permanent ends are 1, 2, 4, 8,
  every 16th block and each dose (247, 494, 741). An empty pool (blocks 1
  and 2) uses ``initial``. A unit's history checkpoint is index u mod n of
  the pool, u being the first 8 bytes, little-endian, of SHA256 of the
  compact JSON ``["g-references-v1/history", run, block, o, t, j]``.
* Seed namespaces ``g-references-v1/order``, ``/train`` and ``/history`` with
  run labels G1..G8 (``/panel`` belongs to the evaluator; ``/init`` derives
  each lineage's ``base_seed``).
* Runtime rules: every episode carries ``runtime_rules:
  "resolution-boundary-v1"`` (the baseline sets none: the historical
  runtime). The trainer excludes the profile from state hashes, so the
  per-episode field in the block config is the record; ``require_runtime_rules``
  is the drivers' refusal.

Everything else (units, kinds by j, starting player, SHA-sorted order packed
five per update, 162 updates a block, frozen method fields, episode limits,
decks) is the baseline's own code, called here.

CLI (JSON on stdout):
  summary --run G1 --block 1           mixing statistics for one block
  pool --block 741                     the history pool's block ends
  config  --spec SPEC.json --out PATH  write one block's training config
  init-request --run G1 --target-from MANIFEST --out PATH
                                       the lineage's fresh-initialization request
  seeds   --out PATH                   every training seed, for the namespace check
  (the namespace check against the registered namespaces: g_references_namespaces_v1.py)
"""
from __future__ import annotations

import argparse
import hashlib
import json
from collections import Counter
from pathlib import Path

import nine_deck_baseline_v1 as ndb

PREFIX = "g-references-v1"
RUNS = tuple(f"G{index}" for index in range(1, 9))
EVALUATION_RUNS = RUNS[:4]  # reserved from every later training pool, the league included
LEAGUE_SEED_RUNS = RUNS[4:]
DOSES = {"D_a": 247, "D_b": 494, "D_c": 741}
BLOCKS = DOSES["D_c"]
STOP_RULE_BLOCK = DOSES["D_b"]  # every lineage stops here until D_b's panels are read
EPISODE_BLOCK_DIGITS = 3  # episode ids name the block as b001..b741
RUNTIME_RULES = "resolution-boundary-v1"  # amendment 1; refused by the engine for a game with a Delve card
NAMESPACES = tuple(f"{PREFIX}/{name}" for name in ("order", "train", "history", "panel", "init"))
FRESH_SOURCE_SCHEMA = "mtg-kernel-fresh-initialization-source/v1"
INIT_REQUEST_SCHEMA = "mtg-kernel-fresh-initialization-request/v1"
FEATURE_TRANSFER = {"expected_feature_contract_digest": "c4af415a3b0cf1e9c9960dbe2bc2d134c63e9f08206a9a364e113121fea5538b",
                    "expected_feature_encoding_digest": "271c0e5a0fdce75663c897e89a9d7280ab1a3bbb6679bd10ecb5f524991952de"}


def check_unit(run: str, block: int, own: int, other: int, repeat: int) -> None:
    if run not in RUNS or type(block) is not int or not 1 <= block <= BLOCKS:
        raise ValueError(f"unknown run or block: {run!r} {block!r}")
    for value, bound in ((own, ndb.DECKS), (other, ndb.DECKS), (repeat, ndb.REPEATS)):
        if type(value) is not int or not 0 <= value < bound:
            raise ValueError("deck or repeat index out of range")


def order_key(run: str, block: int, own: int, other: int, repeat: int) -> str:
    check_unit(run, block, own, other, repeat)
    return hashlib.sha256(ndb.key_bytes([f"{PREFIX}/order", run, block, own, other, repeat])).hexdigest()


def unit_seed(run: str, block: int, own: int, other: int, repeat: int) -> int:
    check_unit(run, block, own, other, repeat)
    return ndb.derived_seed([f"{PREFIX}/train", run, block, own, other, repeat])


def history_draw(run: str, block: int, own: int, other: int, repeat: int) -> int:
    """u: the first 8 bytes of the history hash, little-endian (not masked), separate from the unit seed."""
    check_unit(run, block, own, other, repeat)
    digest = hashlib.sha256(ndb.key_bytes([f"{PREFIX}/history", run, block, own, other, repeat])).digest()
    return int.from_bytes(digest[:8], "little")


def base_seed(run: str) -> int:
    """The lineage's fresh-initialization base_seed (trainer-seeded-v1 derives model_init_seed from it)."""
    if run not in RUNS:
        raise ValueError(f"unknown run: {run!r}")
    return ndb.derived_seed([f"{PREFIX}/init", run])


def is_permanent(block: int) -> bool:
    """Permanent block ends: 1, 2, 4, 8, every 16th block, and each dose."""
    return block in (1, 2, 4, 8) or block % 16 == 0 or block in DOSES.values()


PERMANENT_BLOCKS = tuple(block for block in range(1, BLOCKS + 1) if is_permanent(block))


def history_pool(block: int) -> list[int]:
    """Block b's history pool: permanent block ends at or below b - 2, sorted by block number."""
    if type(block) is not int or not 1 <= block <= BLOCKS:
        raise ValueError(f"unknown block: {block!r}")
    return [end for end in PERMANENT_BLOCKS if end <= block - 2]


def history_id(run: str, end: int) -> str:
    return f"{run}-history-b{end:03d}"


def unit_opponent(run: str, block: int, unit: tuple[int, int, int]) -> dict:
    """The unit's opponent assignment: the baseline's kind by j, with ``fixed`` replaced by own history."""
    own, other, repeat = unit
    kind = ndb.opponent_kind(repeat)
    if kind != "fixed":
        return {"kind": kind}
    pool = history_pool(block)
    if not pool:
        return {"kind": "initial"}
    return {"kind": "fixed", "id": history_id(run, pool[history_draw(run, block, own, other, repeat) % len(pool)])}


def block_units(run: str, block: int) -> list[list[tuple[int, int, int]]]:
    """The block's 162 updates, each a list of five (own, other, repeat) units (the baseline's packing)."""
    units = [(o, t, j) for o in range(ndb.DECKS) for t in range(ndb.DECKS) for j in range(ndb.REPEATS)]
    units.sort(key=lambda unit: order_key(run, block, *unit))
    return [units[index:index + ndb.UNITS_PER_UPDATE] for index in range(0, len(units), ndb.UNITS_PER_UPDATE)]


def unit_episodes(run: str, block: int, unit: tuple[int, int, int], decks: list[dict],
                  runtime_rules: str | None = RUNTIME_RULES) -> list[dict]:
    """The unit's two scheduled episodes: learner in seat 0, then seat 1 (the baseline's episode fields, plus
    the runtime profile unless ``runtime_rules`` is None, which only tests use to compare with the baseline)."""
    own, other, repeat = unit
    seed = unit_seed(run, block, own, other, repeat)
    opponent = unit_opponent(run, block, unit)
    episodes = []
    for seat in (0, 1):
        seats = [decks[own], decks[other]] if seat == 0 else [decks[other], decks[own]]
        episodes.append({
            "episode": {
                "id": f"{PREFIX}-{run}-b{block:0{EPISODE_BLOCK_DIGITS}d}-o{own}-t{other}-j{repeat}-s{seat}",
                "seed": seed,
                "starting_player": ndb.starting_player(own, other, repeat),
                "learner_seat": seat,
                "registered": [dict(deck) for deck in seats],
                "selected": [dict(deck) for deck in seats],
                "postboard": False,
                "max_physical_decisions": ndb.MAX_PHYSICAL_DECISIONS,
                "max_policy_steps": ndb.MAX_POLICY_STEPS,
                **({} if runtime_rules is None else {"runtime_rules": runtime_rules}),
            },
            "opponent": dict(opponent),
        })
    return episodes


def block_iterations(run: str, block: int, decks: list[dict], runtime_rules: str | None = RUNTIME_RULES) -> list[dict]:
    return [{"episodes": [episode for unit in update for episode in unit_episodes(run, block, unit, decks, runtime_rules)]}
            for update in block_units(run, block)]


def require_runtime_rules(config: dict) -> int:
    """The drivers' refusal (amendment 1): every episode of a block config carries the runtime profile."""
    episodes = [item["episode"] for update in config["iterations"] for item in update["episodes"]]
    missing = sum(1 for episode in episodes if episode.get("runtime_rules") != RUNTIME_RULES)
    if missing or len(episodes) != ndb.UPDATES_PER_BLOCK * 10:
        raise ValueError(f"block config refused: {missing} of {len(episodes)} episodes lack runtime_rules "
                         f"{RUNTIME_RULES!r}")
    return len(episodes)


def delve_cards(decks: list[dict], registry: Path) -> list[str]:
    """Cards with the Delve mechanic in the mainboards (the engine refuses the profile for such a game)."""
    cards = json.loads(Path(registry).read_bytes())["cards"]
    used = {card for deck in decks for card in deck["mainboard"]}
    return sorted(cards[index]["name"] for index in used if "delve" in cards[index].get("mechanics", []))


def fresh_source(play_import: dict) -> dict:
    """Block 1's ``initial_source``: the lineage's fresh initialization, no checkpoint (fresh Adam)."""
    return {"play_import": dict(play_import), "feature_transfer": dict(FEATURE_TRANSFER), "checkpoint": None}


def continued_source(play_import: dict, checkpoint: dict) -> dict:
    return {"play_import": dict(play_import), "feature_transfer": dict(FEATURE_TRANSFER),
            "checkpoint": {"path": checkpoint["path"], "sha256": checkpoint["sha256"]}}


def block_config(spec: dict) -> dict:
    """One block's native config from a small spec.

    spec: run, block, runtime_decks (path), play_import (the lineage's fresh
    source descriptor pin), start_checkpoint (block b-1's final checkpoint pin,
    absent for block 1), history ({block end: checkpoint pin} holding exactly
    the pool), update_backend, collection_workers, preparation_workers,
    output_directory; ``runtime_rules`` (default the amendment's profile; None only in tests).
    """
    run, block = spec["run"], spec["block"]
    check_unit(run, block, 0, 0, 0)
    play_import = spec["play_import"]
    start = spec.get("start_checkpoint")
    if (block == 1) != (start is None):
        raise ValueError("block 1 starts fresh; every later block starts from block b-1's checkpoint")
    history = {int(end): pin for end, pin in spec["history"].items()}
    pool = history_pool(block)
    if sorted(history) != pool:
        raise ValueError(f"history checkpoints {sorted(history)} differ from block {block}'s pool {pool}")
    decks = ndb.load_decks(Path(spec["runtime_decks"]))
    runtime_rules = spec.get("runtime_rules", RUNTIME_RULES)
    config = {
        "schema": ndb.SCHEMA,
        "initial_source": fresh_source(play_import) if start is None else continued_source(play_import, start),
        "opponents": [{"id": history_id(run, end), "source": continued_source(play_import, history[end])}
                      for end in pool],
        "iterations": block_iterations(run, block, decks, runtime_rules),
        **json.loads(json.dumps(ndb.FROZEN)),
        "update_backend": spec["update_backend"],
        "collection_workers": spec["collection_workers"],
        "preparation_workers": spec["preparation_workers"],
        "output_directory": spec["output_directory"],
    }
    return config


def block_summary(run: str, block: int) -> dict:
    updates = block_units(run, block)
    learner, opponent, kinds, pairs, history = Counter(), Counter(), Counter(), Counter(), Counter()
    mixed, own_decks, distinct = 0, [], []
    for update in updates:
        if len({ndb.opponent_kind(j) for _, _, j in update}) > 1:
            mixed += 1
        own_decks.append(len({o for o, _, _ in update}))
        ids = set()
        for o, t, j in update:
            assignment = unit_opponent(run, block, (o, t, j))
            learner[o] += 2
            opponent[t] += 2
            kinds[assignment["kind"]] += 2
            pairs[(o, t)] += 2
            if "id" in assignment:
                history[assignment["id"]] += 2
                ids.add(assignment["id"])
        distinct.append(len(ids))
    return {"run": run, "block": block, "updates": len(updates), "history_pool": history_pool(block),
            "learner_episodes_by_deck": [learner[d] for d in range(ndb.DECKS)],
            "opponent_episodes_by_deck": [opponent[d] for d in range(ndb.DECKS)],
            "episodes_by_kind": dict(sorted(kinds.items())),
            "history_episodes_by_id": dict(sorted(history.items())),
            "max_distinct_history_per_update": max(distinct),
            "episodes_per_ordered_pair": sorted(set(pairs.values())),
            "updates_mixing_kinds": mixed,
            "mean_distinct_own_decks": sum(own_decks) / len(own_decks)}


INIT_TARGET_KEYS = ("registry", "card_db_hash", "feature_contract_digest", "feature_encoding_digest",
                    "features_source_sha256", "feature_descriptor_sha256")


def init_request(run: str, target: dict) -> dict:
    """The lineage's ``mtg-kernel-fresh-initialization-request/v1`` (lineage_id G<k>, the /init base_seed).

    ``target`` may be an earlier manifest's target: only the request's six
    fields are kept, and the producer checks each against its actual sources.
    """
    return {"schema": INIT_REQUEST_SCHEMA, "lineage_id": run, "base_seed": base_seed(run),
            "target": {key: target[key] for key in INIT_TARGET_KEYS}}


def fresh_descriptor(folder: Path) -> dict:
    """The ``play_import`` descriptor of one producer output folder (initialization.json, parameters.f32le)."""
    folder = Path(folder).resolve()
    pins = {}
    for key, name in (("initialization", "initialization.json"), ("parameters", "parameters.f32le")):
        data = (folder / name).read_bytes()
        pins[key] = {"path": (folder / name).as_posix(), "sha256": hashlib.sha256(data).hexdigest()}
    return {"initialization": pins["initialization"], "parameters": pins["parameters"], "schema": FRESH_SOURCE_SCHEMA}


def distinct_starts(folders: dict[str, Path], others: dict[str, Path]) -> dict:
    """Section 2: the lineages' model-parameter payload hashes are pairwise distinct and distinct from
    the other fresh inits (A and B). Descriptor hashes do not count: metadata alone can change them."""
    def payload(folder):
        manifest = json.loads((Path(folder) / "initialization.json").read_bytes())
        return {"lineage_id": manifest["lineage_id"], "base_seed": manifest["initializer"]["base_seed"],
                "model_parameter_sha256": manifest["payload"]["model_parameter_sha256"],
                "parameters_sha256": manifest["payload"]["sha256"]}
    if sorted(others) != ["A", "B"]:
        raise ValueError("distinct starts compare the lineages with fresh inits A and B, both named")
    ours = {run: payload(folder) for run, folder in sorted(folders.items())}
    theirs = {name: payload(folder) for name, folder in sorted(others.items())}
    hashes = [row["model_parameter_sha256"] for row in (*ours.values(), *theirs.values())]
    seeds_ok = all(row["lineage_id"] == run and row["base_seed"] == base_seed(run) for run, row in ours.items())
    return {"schema": f"{PREFIX}/distinct-starts", "lineages": ours, "others": theirs,
            "pass": sorted(ours) == list(RUNS) and len(set(hashes)) == len(hashes) and seeds_ok}


def all_seeds() -> dict:
    train = [{"run": run, "block": block, "own": o, "other": t, "repeat": j, "seed": unit_seed(run, block, o, t, j)}
             for run in RUNS for block in range(1, BLOCKS + 1)
             for o in range(ndb.DECKS) for t in range(ndb.DECKS) for j in range(ndb.REPEATS)]
    init = [{"run": run, "base_seed": base_seed(run)} for run in RUNS]
    return {"schema": f"{PREFIX}/seeds", "train": train, "init": init}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="op", required=True)
    summary = sub.add_parser("summary")
    summary.add_argument("--run", required=True)
    summary.add_argument("--block", type=int, required=True)
    pool = sub.add_parser("pool")
    pool.add_argument("--block", type=int, required=True)
    config = sub.add_parser("config")
    config.add_argument("--spec", type=Path, required=True)
    config.add_argument("--out", type=Path, required=True)
    request = sub.add_parser("init-request")
    request.add_argument("--run", required=True)
    request.add_argument("--target-from", type=Path, required=True,
                         help="a fresh initialization manifest (or request) whose target matches the runtime")
    request.add_argument("--out", type=Path, required=True)
    seeds = sub.add_parser("seeds")
    seeds.add_argument("--out", type=Path, required=True)
    starts = sub.add_parser("starts", help="write each lineage's play-import descriptor and check distinct starts")
    starts.add_argument("--init-root", type=Path, required=True, help="holds G1..G8 producer output folders")
    starts.add_argument("--other", action="append", default=[], metavar="NAME=FOLDER",
                        help="another fresh initialization (fresh init A and B)")
    starts.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.op == "starts":
        folders = {run: args.init_root / run for run in RUNS}
        for run, folder in folders.items():
            data = json.dumps(fresh_descriptor(folder), sort_keys=True, separators=(",", ":")).encode()
            with (args.init_root / f"play-import-{run}.json").open("xb") as stream:
                stream.write(data)
        others = {name: Path(folder) for name, folder in (item.split("=", 1) for item in args.other)}
        result = distinct_starts(folders, others)
        data = (json.dumps(result, sort_keys=True, indent=1) + "\n").encode()
        with args.out.open("xb") as stream:
            stream.write(data)
        print(json.dumps({"path": str(args.out), "sha256": hashlib.sha256(data).hexdigest(), "pass": result["pass"]}))
        return 0 if result["pass"] else 1
    if args.op == "summary":
        print(json.dumps(block_summary(args.run, args.block)))
        return 0
    if args.op == "pool":
        print(json.dumps({"block": args.block, "pool": history_pool(args.block)}))
        return 0
    if args.op == "config":
        value = block_config(json.loads(args.spec.read_text(encoding="utf-8")))
    elif args.op == "init-request":
        value = init_request(args.run, json.loads(args.target_from.read_text(encoding="utf-8"))["target"])
    else:
        value = all_seeds()
    data = (json.dumps(value, separators=(",", ":")) + "\n").encode()
    with args.out.open("xb") as stream:
        stream.write(data)
    print(json.dumps({"path": str(args.out), "sha256": hashlib.sha256(data).hexdigest()}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
