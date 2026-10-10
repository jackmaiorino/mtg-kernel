"""Read-only exposure collector for nine-deck baseline blocks (v1).

For one completed block (a native expanded training run directory) this
writes compact per-episode rows and the block's exposure checks, without
changing any native output:

1. Completed episodes: learner and opponent episodes per deck equal the
   schedule (180 and 180).
2. Non-natural attempts: ledger entries counted by learner deck; every kept
   trajectory of a ledgered slot carries the derived retry seed of its
   scheduled episode. A deck whose discarded-attempt rate exceeds 5% is an
   exposure defect.
3. Optimizer samples (operative): learner policy substeps entering the loss,
   attributed to the learner's deck, reconciled with each update's own
   ``policy_substeps``. Spy and CawGates must each reach at least 50% of the
   nine-deck mean.
4. Embedding sanity gate (block 1 only, ``embedding-gate``): every Spy-only
   and CawGates-only card-embedding row moved from the start checkpoint and
   carries nonzero Adam moments.

The block's schedule is first checked against the generator for its run and
block, so a row's unit, kind and seed come from the pinned rules. Failed
checks are technical dispositions, never strength verdicts.

CLI (JSON summary on stdout; exit 0 even when a check fails, the verdict is in
the output):
  collect --run r1 --block 1 --native DIR --decks data/runtime_decks_v1.json --out DIR
  embedding-gate --start CKPT --end CKPT --decks data/runtime_decks_v1.json --out FILE
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from collections import Counter
from pathlib import Path

import nine_deck_baseline_v1 as ndb

SCHEMA = "nine-deck-baseline-v1/exposure-block"
ROW_SCHEMA = "nine-deck-baseline-v1/episode-row"
SPY, CAWGATES = 4, 7
SEVEN = (0, 1, 2, 3, 5, 6, 8)
MAX_DISCARD_RATE = 0.05
MIN_SUBSTEP_RATIO = 0.5
FINITE_FIELDS = ("loss", "policy_sum", "value_sum")
RETAIN_UPDATES_PER_BLOCK = 4  # >= 2% of 162 updates, chosen by seeded order


def read_bytes_pinned(pin: dict) -> bytes:
    data = Path(pin["path"]).read_bytes()
    if hashlib.sha256(data).hexdigest() != pin["sha256"]:
        raise ValueError(f"pinned file changed: {pin['path']}")
    return data


def read_json_pinned(pin: dict):
    return json.loads(read_bytes_pinned(pin))


def derived_retry_seed(episode_id: str, original_seed: int, retry_index: int) -> int:
    """Mirror of the engine's derived_retry_seed_v1 (big-endian first 8 bytes)."""
    payload = (b"mtg-kernel-non-natural-retry-seed/v1\0" + episode_id.encode() + b"\0"
               + original_seed.to_bytes(8, "big") + retry_index.to_bytes(4, "big"))
    return int.from_bytes(hashlib.sha256(payload).digest()[:8], "big")


def parse_episode_id(episode_id: str, run: str, block: int) -> tuple[int, int, int, int]:
    prefix = f"{ndb.PREFIX}-{run}-b{block:02d}-"
    if not episode_id.startswith(prefix):
        raise ValueError(f"episode {episode_id} is not from {run} block {block}")
    parts = dict((field[0], int(field[1:])) for field in episode_id[len(prefix):].split("-"))
    return parts["o"], parts["t"], parts["j"], parts["s"]


def retained_updates(run: str, block: int) -> list[int]:
    """The seeded 2% sample of updates whose full trajectories are kept."""
    keys = sorted(range(ndb.UPDATES_PER_BLOCK), key=lambda index: hashlib.sha256(
        ndb.key_bytes([f"{ndb.PREFIX}/retain", run, block, index])).hexdigest())
    return sorted(keys[:RETAIN_UPDATES_PER_BLOCK])


def episode_row(trajectory: dict, pin: dict, scheduled: dict, ledgered: list[dict], run: str, block: int,
                iteration: int, slot: int, opponent: dict) -> dict:
    episode = trajectory["episode"]
    if episode["id"] != scheduled["id"]:
        raise ValueError(f"slot {iteration}/{slot} holds {episode['id']}, scheduled {scheduled['id']}")
    for field in ("learner_seat", "starting_player", "selected", "registered", "postboard",
                  "max_physical_decisions", "max_policy_steps"):
        if episode[field] != scheduled[field]:
            raise ValueError(f"episode {episode['id']} differs from its schedule in {field}")
    retries = len(ledgered)
    expected_seed = scheduled["seed"] if not retries else derived_retry_seed(
        scheduled["id"], scheduled["seed"], retries)
    if episode["seed"] != expected_seed:
        raise ValueError(f"episode {episode['id']} seed does not reconcile with its retries")
    own, other, repeat, seat = parse_episode_id(episode["id"], run, block)
    learner = episode["learner_seat"]
    terminal = trajectory["terminal"]
    learner_rows = [row for row in trajectory["decisions"] if row["actor"] == learner]
    behaviors = trajectory.get("seat_behaviors") or []
    opponent_hash = None
    if len(behaviors) == 2:
        opponent_hash = behaviors[1 - learner].get("identity", {}).get("checkpoint_sha256")
    reward = terminal.get("terminal_reward")
    return {"schema": ROW_SCHEMA, "run": run, "block": block, "iteration": iteration, "slot": slot,
            "unit": [own, other, repeat], "seat": seat, "learner_seat": learner,
            "decks": [episode["selected"][0]["label"], episode["selected"][1]["label"]],
            "learner_deck": own, "opponent_deck": other,
            "opponent_kind": opponent["kind"], "opponent_id": opponent.get("id"),
            "opponent_checkpoint_sha256": opponent_hash,
            "scheduled_seed": scheduled["seed"], "seed": episode["seed"], "retry_index": retries,
            "terminal_classification": terminal["terminal_classification"],
            "winner": terminal.get("winner"),
            "learner_reward": reward[learner] if isinstance(reward, list) else None,
            "learner_policy_substeps": len(learner_rows),
            "learner_physical_decisions": len({row["physical_decision_id"] for row in learner_rows}),
            "all_policy_steps": terminal.get("policy_step_count"),
            "all_physical_decisions": terminal.get("physical_decision_count"),
            "trajectory_sha256": pin["sha256"]}


def collect(run: str, block: int, native: Path, decks_path: Path, out: Path) -> dict:
    native = Path(native)
    decks = ndb.load_decks(decks_path)
    manifest = json.loads((native / "run.json").read_bytes())
    config = manifest["config"]
    expected = ndb.block_iterations(run, block, decks)
    schedule_matches = config["iterations"] == expected
    if not schedule_matches:
        raise ValueError("run schedule differs from the generator's block schedule")
    completion = json.loads((native / "completion.json").read_bytes())
    if not completion["complete"] or completion["completed_iterations"] != ndb.UPDATES_PER_BLOCK:
        raise ValueError("block is not complete")
    keep = set(retained_updates(run, block))
    rows, ledger_rows, updates = [], [], []
    for iteration, item in enumerate(completion["iterations"]):
        complete = read_json_pinned(item)
        if complete["iteration"] != iteration:
            raise ValueError("unordered completion records")
        collection = read_json_pinned(complete["collection"])
        update = read_json_pinned(complete["update"])
        if not collection["complete"] or not update["complete"]:
            raise ValueError(f"incomplete iteration {iteration}")
        ledger = read_json_pinned(collection["non_natural_ledger"])["entries"] \
            if collection.get("non_natural_ledger") else []
        by_slot: dict[int, list[dict]] = {}
        for entry in ledger:
            by_slot.setdefault(entry["slot"], []).append(entry)
        scheduled = config["iterations"][iteration]["episodes"]
        if len(collection["trajectories"]) != len(scheduled):
            raise ValueError(f"iteration {iteration} trajectory count differs from schedule")
        substeps = 0
        for slot, pin in enumerate(collection["trajectories"]):
            entries = sorted(by_slot.get(slot, []), key=lambda entry: entry["attempt"])
            if [entry["attempt"] for entry in entries] != list(range(1, len(entries) + 1)):
                raise ValueError(f"ledger attempts of slot {iteration}/{slot} are not contiguous")
            trajectory = json.loads(read_bytes_pinned(pin))
            row = episode_row(trajectory, pin, scheduled[slot]["episode"], entries, run, block, iteration, slot,
                              scheduled[slot]["opponent"])
            if row["terminal_classification"] != "natural":
                raise ValueError(f"kept trajectory {row['trajectory_sha256']} is not natural")
            substeps += row["learner_policy_substeps"]
            row["retained_trajectory"] = iteration in keep
            rows.append(row)
            for entry in entries:
                own, other, repeat, seat = parse_episode_id(entry["episode_id"], run, block)
                ledger_rows.append({"iteration": iteration, "slot": slot, "attempt": entry["attempt"],
                                    "learner_deck": own, "opponent_deck": other, "seed": entry["seed"],
                                    "terminal_classification": entry["terminal_classification"],
                                    "terminal_reason": entry.get("terminal_reason")})
        if substeps != update["policy_substeps"]:
            raise ValueError(f"iteration {iteration}: learner substeps {substeps} != update {update['policy_substeps']}")
        updates.append({"iteration": iteration, "adam_step": update["adam_step"],
                        "policy_substeps": update["policy_substeps"],
                        "physical_decisions": update["physical_decisions"],
                        "losses_finite": all(isinstance(update.get(key), (int, float)) and not isinstance(update.get(key), bool)
                                             and math.isfinite(update[key]) for key in FINITE_FIELDS),
                        "checkpoint_sha256": update["checkpoint"]["sha256"]})
    checks = exposure_checks(rows, ledger_rows)
    nonfinite = [update["iteration"] for update in updates if not update["losses_finite"]]
    checks["finite_losses"] = {"pass": not nonfinite, "fields": list(FINITE_FIELDS), "nonfinite_iterations": nonfinite}
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    rows_path = out / f"{run}-b{block:02d}-episodes.jsonl"
    data = "".join(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n" for row in rows).encode()
    with rows_path.open("xb") as stream:
        stream.write(data)
    final = updates[-1]
    summary = {"schema": SCHEMA, "run": run, "block": block, "native_root": str(native),
               "schedule_matches_generator": schedule_matches,
               "rows": {"path": str(rows_path), "sha256": hashlib.sha256(data).hexdigest(), "count": len(rows)},
               "ledger": ledger_rows, "updates": updates, "retained_updates": sorted(keep),
               "final_checkpoint_sha256": final["checkpoint_sha256"], "final_adam_step": final["adam_step"],
               "checks": checks, "verdict": "pass" if all(c["pass"] for c in checks.values()) else "fail"}
    summary_path = out / f"{run}-b{block:02d}-exposure.json"
    with summary_path.open("x", encoding="utf-8") as stream:
        json.dump(summary, stream, sort_keys=True, indent=1)
    return summary


def exposure_checks(rows: list[dict], ledger_rows: list[dict]) -> dict:
    learner = Counter(row["learner_deck"] for row in rows)
    opponent = Counter(row["opponent_deck"] for row in rows)
    expected = ndb.UPDATES_PER_BLOCK * 10 // ndb.DECKS
    episodes_ok = all(learner[d] == expected and opponent[d] == expected for d in range(ndb.DECKS))
    discarded = Counter(row["learner_deck"] for row in ledger_rows)
    rates = [discarded[d] / (discarded[d] + learner[d]) if learner[d] else 1.0 for d in range(ndb.DECKS)]
    substeps = Counter()
    for row in rows:
        substeps[row["learner_deck"]] += row["learner_policy_substeps"]
    mean = sum(substeps.values()) / ndb.DECKS
    ratios = [substeps[d] / mean if mean else 0.0 for d in range(ndb.DECKS)]
    return {
        "completed_episodes": {"pass": episodes_ok, "expected_per_deck": expected,
                               "learner_by_deck": [learner[d] for d in range(ndb.DECKS)],
                               "opponent_by_deck": [opponent[d] for d in range(ndb.DECKS)]},
        "non_natural_attempts": {"pass": all(rate <= MAX_DISCARD_RATE for rate in rates),
                                 "discarded_by_learner_deck": [discarded[d] for d in range(ndb.DECKS)],
                                 "discard_rate_by_deck": rates, "limit": MAX_DISCARD_RATE},
        "optimizer_samples": {"pass": min(ratios[SPY], ratios[CAWGATES]) >= MIN_SUBSTEP_RATIO,
                              "learner_substeps_by_deck": [substeps[d] for d in range(ndb.DECKS)],
                              "ratio_to_nine_deck_mean": ratios, "floor": MIN_SUBSTEP_RATIO},
    }


def exclusive_cards(decks: list[dict]) -> dict[str, list[int]]:
    """Card ids in Spy or CawGates absent from the seven previously trained decks."""
    cards = [set(deck["mainboard"]) for deck in decks]
    seven = set().union(*(cards[d] for d in SEVEN))
    return {"Spy": sorted(cards[SPY] - seven), "CawGates": sorted(cards[CAWGATES] - seven)}


def tensor(checkpoint: dict, field: str, name: str) -> dict:
    matches = [entry for entry in checkpoint[field] if entry["name"] == name]
    if len(matches) != 1:
        raise ValueError(f"{field} has no unique {name}")
    return matches[0]


def embedding_gate(start: dict, end: dict, decks: list[dict]) -> dict:
    """Rows are card_id + 1 in card_embedding.weight (row 0 is padding)."""
    name = "card_embedding.weight"
    before = tensor(start, "parameters", name)
    after = tensor(end, "parameters", name)
    first = tensor(end, "first_moments", name)
    second = tensor(end, "second_moments", name)
    width = after["shape"][1]
    result = {}
    for deck, ids in exclusive_cards(decks).items():
        cards = []
        for card in ids:
            span = slice((card + 1) * width, (card + 2) * width)
            cards.append({"card_id": card,
                          "moved": before["values"][span] != after["values"][span],
                          "first_moment_nonzero": any(bits & 0x7FFFFFFF for bits in first["values"][span]),
                          "second_moment_nonzero": any(bits & 0x7FFFFFFF for bits in second["values"][span])})
        result[deck] = {"cards": cards, "count": len(cards),
                        "pass": all(c["moved"] and c["first_moment_nonzero"] and c["second_moment_nonzero"]
                                    for c in cards)}
    return {"schema": f"{ndb.PREFIX}/embedding-gate", "decks": result,
            "pass": all(item["pass"] for item in result.values())}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="op", required=True)
    one = sub.add_parser("collect")
    one.add_argument("--run", required=True)
    one.add_argument("--block", type=int, required=True)
    one.add_argument("--native", type=Path, required=True)
    one.add_argument("--decks", type=Path, required=True)
    one.add_argument("--out", type=Path, required=True)
    gate = sub.add_parser("embedding-gate")
    gate.add_argument("--start", type=Path, required=True)
    gate.add_argument("--end", type=Path, required=True)
    gate.add_argument("--decks", type=Path, required=True)
    gate.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.op == "collect":
        summary = collect(args.run, args.block, args.native, args.decks, args.out)
        print(json.dumps({"verdict": summary["verdict"], "checks": {k: v["pass"] for k, v in summary["checks"].items()},
                          "final_checkpoint_sha256": summary["final_checkpoint_sha256"]}))
    else:
        result = embedding_gate(json.loads(args.start.read_bytes()), json.loads(args.end.read_bytes()),
                                ndb.load_decks(args.decks))
        result.update({"start_sha256": hashlib.sha256(args.start.read_bytes()).hexdigest(),
                       "end_sha256": hashlib.sha256(args.end.read_bytes()).hexdigest()})
        with args.out.open("x", encoding="utf-8") as stream:
            json.dump(result, stream, sort_keys=True, indent=1)
        print(json.dumps({"pass": result["pass"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
