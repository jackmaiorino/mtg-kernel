"""Explicit schedules for the nine-deck ordinary training baseline (v1).

Pure functions of the run label, block number and pinned inputs: no model is
loaded and no device is opened. The rules follow the reviewed design
(collab LANES/nine-deck-baseline-design-20261007/DESIGN.md, v3):

* A block holds 810 units (o, t, j): every ordered pair of own deck o and
  other deck t (mirrors included) repeated j = 0..9. Deck indices are
  ``canonical_pool_order - 1``.
* A unit is two consecutive episodes on one seed, learner in seat 0 then 1.
* Opponent kind by j: 0-2 current, 3-7 initial (block-start checkpoint),
  8-9 the fixed A48 opponent.
* Starting player ``(j + o + t) mod 2``.
* Units are sorted by SHA256 of the compact JSON order key and packed five
  per update (162 updates); a unit never splits.
* A unit's seed is the first 8 bytes of SHA256 of the compact JSON train key,
  little-endian, masked to 2**63 - 1.

Evaluation cases use the ``nine-deck-baseline-v1/dev`` namespace with the same
encoding (``case_seed``). The ``confirm`` namespace is reserved and unused.

CLI (JSON on stdout):
  summary --run r1 --block 1           mixing statistics for one block
  config  --spec SPEC.json --out PATH  write one block's training config
  seeds   --out PATH                   every training and panel seed, for the
                                       namespace disjointness check
"""
from __future__ import annotations

import argparse
import hashlib
import json
from collections import Counter
from pathlib import Path

from phase1_breadth_v1.catalog_v1 import canonical_zones, digest, to_native

PREFIX = "nine-deck-baseline-v1"
RUNS = tuple(f"r{index}" for index in range(1, 7))
BLOCKS = 20
DECKS = 9
REPEATS = 10
UNITS_PER_UPDATE = 5
UPDATES_PER_BLOCK = DECKS * DECKS * REPEATS // UNITS_PER_UPDATE  # 162
SEED_MASK = (1 << 63) - 1
RUNTIME_DECKS_SHA256 = "68e7602f3a4df6217119406973954630800c358a10fca9f28e6cf9f20fd3b851"
REGISTRATIONS_PATH = "docs/research/sideboard_plan_inputs_2026-09/registered_nine_75s.json"
REGISTRATIONS_SHA256 = "095d66eef513caec81585d554cbd49304d1f5b1daf2e7ea969c5b5c982933104"
DECK_NAMES = ("Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates", "Faeries")
FIXED_OPPONENT_ID = "fresh-a-block48-end"
MAX_PHYSICAL_DECISIONS = 100000
MAX_POLICY_STEPS = 200000
PANEL_REPEATS = 12
SCHEMA = "mtg-kernel-native-expanded-training-run/v1"


def key_bytes(parts: list) -> bytes:
    """The pinned encoding: compact JSON of the key list."""
    return json.dumps(parts, separators=(",", ":")).encode()


def check_unit(run: str, block: int, own: int, other: int, repeat: int) -> None:
    if run not in RUNS or type(block) is not int or not 1 <= block <= BLOCKS:
        raise ValueError(f"unknown run or block: {run!r} {block!r}")
    for value, bound in ((own, DECKS), (other, DECKS), (repeat, REPEATS)):
        if type(value) is not int or not 0 <= value < bound:
            raise ValueError("deck or repeat index out of range")


def order_key(run: str, block: int, own: int, other: int, repeat: int) -> str:
    check_unit(run, block, own, other, repeat)
    return hashlib.sha256(key_bytes([f"{PREFIX}/order", run, block, own, other, repeat])).hexdigest()


def derived_seed(parts: list) -> int:
    return int.from_bytes(hashlib.sha256(key_bytes(parts)).digest()[:8], "little") & SEED_MASK


def unit_seed(run: str, block: int, own: int, other: int, repeat: int) -> int:
    check_unit(run, block, own, other, repeat)
    return derived_seed([f"{PREFIX}/train", run, block, own, other, repeat])


def opponent_kind(repeat: int) -> str:
    if repeat <= 2:
        return "current"
    if repeat <= 7:
        return "initial"
    return "fixed"


def starting_player(own: int, other: int, repeat: int) -> int:
    return (repeat + own + other) % 2


def block_units(run: str, block: int) -> list[list[tuple[int, int, int]]]:
    """The block's 162 updates, each a list of five (own, other, repeat) units."""
    units = [(o, t, j) for o in range(DECKS) for t in range(DECKS) for j in range(REPEATS)]
    units.sort(key=lambda unit: order_key(run, block, *unit))
    return [units[index:index + UNITS_PER_UPDATE] for index in range(0, len(units), UNITS_PER_UPDATE)]


def load_decks(path: Path, registrations: Path | None = None) -> list[dict]:
    """The nine registered decks in deck-index order, as T1's campaign registered them.

    Mainboard and 15-card sideboard come from the repo's registered nine 75s
    (labels ``<deck>/<list_sha256[:12]>``, card ids sorted, as the phase-1
    catalog emits them); each mainboard must equal the pinned runtime
    mainboard as a multiset. Play is preboard only, so the sideboard is
    registration metadata.
    """
    path = Path(path)
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != RUNTIME_DECKS_SHA256:
        raise ValueError("runtime decks file differs from the pinned sha256")
    runtime = sorted(json.loads(data)["decks"], key=lambda deck: deck["canonical_pool_order"])
    if [deck["id"] for deck in runtime] != list(DECK_NAMES) or             [deck["canonical_pool_order"] for deck in runtime] != list(range(1, DECKS + 1)):
        raise ValueError("unexpected runtime deck order")
    repo = path.resolve().parents[1]
    registrations = Path(registrations or repo / REGISTRATIONS_PATH)
    raw = registrations.read_bytes()
    if hashlib.sha256(raw).hexdigest() != REGISTRATIONS_SHA256:
        raise ValueError("registered 75s file differs from the pinned sha256")
    rows = json.loads(raw)["decks"]
    registry = json.loads((repo / "data" / "cards_v1.json").read_bytes())
    cards = {card["name"]: (index, card) for index, card in enumerate(registry["cards"])}
    decks = []
    for deck in runtime:
        row = rows[deck["id"]]
        zones = canonical_zones({"mainboard": row["main"], "sideboard": row["side"]}, {})
        native = to_native(zones, deck["id"] + "/" + digest(zones)[:12], cards)
        if Counter(native["mainboard"]) != Counter(card["card_id"] for card in deck["materialized_mainboard"]):
            raise ValueError(f"registered mainboard of {deck['id']} differs from the runtime mainboard")
        decks.append(native)
    return decks


def unit_episodes(run: str, block: int, unit: tuple[int, int, int], decks: list[dict]) -> list[dict]:
    """The unit's two scheduled episodes: learner in seat 0, then seat 1."""
    own, other, repeat = unit
    seed = unit_seed(run, block, own, other, repeat)
    kind = opponent_kind(repeat)
    opponent = {"kind": "fixed", "id": FIXED_OPPONENT_ID} if kind == "fixed" else {"kind": kind}
    episodes = []
    for seat in (0, 1):
        seats = [decks[own], decks[other]] if seat == 0 else [decks[other], decks[own]]
        episodes.append({
            "episode": {
                "id": f"{PREFIX}-{run}-b{block:02d}-o{own}-t{other}-j{repeat}-s{seat}",
                "seed": seed,
                "starting_player": starting_player(own, other, repeat),
                "learner_seat": seat,
                "registered": [dict(deck) for deck in seats],
                "selected": [dict(deck) for deck in seats],
                "postboard": False,
                "max_physical_decisions": MAX_PHYSICAL_DECISIONS,
                "max_policy_steps": MAX_POLICY_STEPS,
            },
            "opponent": dict(opponent),
        })
    return episodes


def block_iterations(run: str, block: int, decks: list[dict]) -> list[dict]:
    return [{"episodes": [episode for unit in update for episode in unit_episodes(run, block, unit, decks)]}
            for update in block_units(run, block)]


# Frozen method fields, pinned to T1's block115 run.json (DESIGN.md table).
FROZEN = {
    "learning_rate": 0.0001,
    "value_coefficient": 0.5,
    "loss_selection": {"kind": "gae_advantage_value_v1", "gamma": 1.0, "lambda": 0.9, "entropy_coefficient": 0.0},
    "max_non_natural_episode_fraction": 0.2,
}


def block_config(spec: dict) -> dict:
    """One block's native config from a small spec.

    spec: run, block, runtime_decks (path), initial_source and fixed_opponent
    (ExpandedModelSourceV1 objects), update_backend, collection_workers,
    preparation_workers, output_directory.
    """
    run, block = spec["run"], spec["block"]
    check_unit(run, block, 0, 0, 0)
    decks = load_decks(Path(spec["runtime_decks"]))
    config = {
        "schema": SCHEMA,
        "initial_source": spec["initial_source"],
        "opponents": [{"id": FIXED_OPPONENT_ID, "source": spec["fixed_opponent"]}],
        "iterations": block_iterations(run, block, decks),
        **json.loads(json.dumps(FROZEN)),
        "update_backend": spec["update_backend"],
        "collection_workers": spec["collection_workers"],
        "preparation_workers": spec["preparation_workers"],
        "output_directory": spec["output_directory"],
    }
    return config


def block_summary(run: str, block: int) -> dict:
    updates = block_units(run, block)
    learner, opponent, kinds, pairs = Counter(), Counter(), Counter(), Counter()
    mixed, own_decks = 0, []
    for update in updates:
        if len({opponent_kind(j) for _, _, j in update}) > 1:
            mixed += 1
        own_decks.append(len({o for o, _, _ in update}))
        for o, t, j in update:
            learner[o] += 2
            opponent[t] += 2
            kinds[opponent_kind(j)] += 2
            pairs[(o, t)] += 2
    return {"run": run, "block": block, "updates": len(updates),
            "learner_episodes_by_deck": [learner[d] for d in range(DECKS)],
            "opponent_episodes_by_deck": [opponent[d] for d in range(DECKS)],
            "episodes_by_kind": dict(sorted(kinds.items())),
            "episodes_per_ordered_pair": sorted(set(pairs.values())),
            "updates_mixing_kinds": mixed,
            "mean_distinct_own_decks": sum(own_decks) / len(own_decks)}


def case_seed(own: int, other: int, repeat: int) -> int:
    """Panel case seed (namespace ``/dev``); both seats of a case share it."""
    if not (0 <= own < DECKS and 0 <= other < DECKS and 0 <= repeat < PANEL_REPEATS):
        raise ValueError("case index out of range")
    return derived_seed([f"{PREFIX}/dev", own, other, repeat])


def panel_cases(own_decks: tuple[int, ...] = tuple(range(DECKS))) -> list[dict]:
    """Panel cases in a fixed order: own deck, other deck, repeat, learner seat."""
    return [{"own": o, "other": t, "repeat": k, "seat": seat, "seed": case_seed(o, t, k),
             "starting_player": (k + o + t) % 2}
            for o in own_decks for t in range(DECKS) for k in range(PANEL_REPEATS) for seat in (0, 1)]


def panel_config(panel: str, source: dict, opponent: dict, decks: list[dict], output_directory: str,
                 workers: int, own_decks: tuple[int, ...] = tuple(range(DECKS))) -> dict:
    """Raw-policy panel collection: ``source`` pilots each own deck against a fixed opponent.

    Cases follow ``panel_cases``; a P3 panel (own decks Spy and CawGates) is a
    subset of P1 with identical case seeds, so P3 at a P1 checkpoint is read
    from P1's games. Any non-natural terminal fails the collection; it is then
    reported as incomplete, never dropped.
    """
    episodes = []
    for case in panel_cases(own_decks):
        own, other, seat = decks[case["own"]], decks[case["other"]], case["seat"]
        seats = [own, other] if seat == 0 else [other, own]
        episodes.append({
            "id": f"{PREFIX}-{panel}-o{case['own']}-t{case['other']}-k{case['repeat']}-s{seat}",
            "seed": case["seed"], "starting_player": case["starting_player"], "learner_seat": seat,
            "registered": [dict(deck) for deck in seats], "selected": [dict(deck) for deck in seats],
            "postboard": False, "max_physical_decisions": MAX_PHYSICAL_DECISIONS,
            "max_policy_steps": MAX_POLICY_STEPS, "opponent": opponent})
    config = {"mode": "collect_parallel" if workers > 1 else "collect", "source": source, "episodes": episodes,
              "max_non_natural_episode_fraction": 0.0, "output_directory": output_directory}
    if workers > 1:
        config["workers"] = workers
    return config


def all_seeds() -> dict:
    train = [{"run": run, "block": block, "own": o, "other": t, "repeat": j,
              "seed": unit_seed(run, block, o, t, j)}
             for run in RUNS for block in range(1, BLOCKS + 1)
             for o in range(DECKS) for t in range(DECKS) for j in range(REPEATS)]
    dev = [{"own": o, "other": t, "repeat": k, "seed": case_seed(o, t, k)}
           for o in range(DECKS) for t in range(DECKS) for k in range(PANEL_REPEATS)]
    return {"schema": f"{PREFIX}/seeds", "train": train, "dev": dev}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="op", required=True)
    summary = sub.add_parser("summary")
    summary.add_argument("--run", required=True)
    summary.add_argument("--block", type=int, required=True)
    config = sub.add_parser("config")
    config.add_argument("--spec", type=Path, required=True)
    config.add_argument("--out", type=Path, required=True)
    seeds = sub.add_parser("seeds")
    seeds.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.op == "summary":
        print(json.dumps(block_summary(args.run, args.block)))
    elif args.op == "config":
        value = block_config(json.loads(args.spec.read_text(encoding="utf-8")))
        data = (json.dumps(value, separators=(",", ":")) + "\n").encode()
        with args.out.open("xb") as stream:
            stream.write(data)
        print(json.dumps({"path": str(args.out), "sha256": hashlib.sha256(data).hexdigest()}))
    else:
        data = (json.dumps(all_seeds(), separators=(",", ":")) + "\n").encode()
        with args.out.open("xb") as stream:
            stream.write(data)
        print(json.dumps({"path": str(args.out), "sha256": hashlib.sha256(data).hexdigest()}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
