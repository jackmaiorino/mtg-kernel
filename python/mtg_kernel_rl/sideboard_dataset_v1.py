"""Audit completed static-teacher batches and propose a leakage-aware dev split.

This module reads provenance, never infers value targets from match winners, and
does not train or run games. Metadata stays separate from model-input JSONL.
"""
from __future__ import annotations

import argparse
from collections import Counter
from hashlib import sha256
import json
import math
from pathlib import Path
import platform
from typing import Any


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def canonical(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=False, allow_nan=False).encode("utf-8")


def identity(value: Any) -> str:
    return sha256(canonical(value)).hexdigest()


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8-sig"))


def pin(path: Path) -> dict:
    data = path.read_bytes()
    return {"path": path.resolve().as_posix(), "bytes": len(data),
            "sha256": sha256(data).hexdigest()}


def verify_pin(receipt: dict, path: Path | None = None) -> dict:
    actual = pin(path or Path(receipt["path"]))
    require(actual["sha256"] == receipt["sha256"], f"SHA256 mismatch: {actual['path']}")
    if "bytes" in receipt:
        require(actual["bytes"] == receipt["bytes"], f"byte count mismatch: {actual['path']}")
    return actual


def counts(rows: list[dict]) -> Counter:
    out: Counter = Counter()
    for row in rows:
        card, count = row["card_id"], row["count"]
        require(type(card) is int and 0 <= card <= 65535, "invalid card id")
        require(type(count) is int and 1 <= count <= 75, "invalid card count")
        require(card not in out, "duplicate card count row")
        out[card] = count
    return out


def expand(counter: Counter) -> list[int]:
    return sorted(counter.elements())


def configuration(main: list[int], side: list[int]) -> dict:
    require(len(main) == 60 and len(side) == 15, "configuration must be 60/15")
    require(all(type(c) is int and 0 <= c <= 65535 for c in main + side), "invalid configuration card")
    return {"mainboard": sorted(main), "sideboard": sorted(side)}


def mainboard_hash(main: list[int]) -> str:
    """Rust DeckConfigurationV1 hash, including its domain and big-endian lengths."""
    def blob(value: bytes) -> bytes:
        return len(value).to_bytes(4, "big") + value
    data = blob(b"kernel-deck-configuration-sha256/v1") + blob(b"mainboard")
    data += len(main).to_bytes(4, "big")
    data += b"".join(card.to_bytes(2, "big") for card in sorted(main))
    return sha256(data).hexdigest()


def load_registrations(registry_path: Path, pool_path: Path) -> tuple[dict, dict]:
    cards = read_json(registry_path)["cards"]
    ids = {card["name"]: i for i, card in enumerate(cards)}
    require(len(ids) == len(cards), "duplicate registry name")
    registrations = {}
    for deck in read_json(pool_path)["decks"]:
        zones = {}
        for zone in ("mainboard", "sideboard"):
            rows = deck[zone]["cards"]
            require(len({r["name"] for r in rows}) == len(rows), "duplicate registration name")
            rows = [{"card_id": ids[r["name"]], "count": r["count"]} for r in rows]
            zones[zone] = expand(counts(rows))
        require(deck["id"] not in registrations, "duplicate deck id")
        registrations[deck["id"]] = configuration(zones["mainboard"], zones["sideboard"])
    return registrations, {"cards": cards, "ids": ids}


def plan_target(registration: dict, row: dict) -> dict:
    main, side = Counter(registration["mainboard"]), Counter(registration["sideboard"])
    incoming, outgoing = counts(row["cards_in"]), counts(row["cards_out"])
    require(not (incoming.keys() & outgoing.keys()), "same card in both plan directions")
    require(sum(incoming.values()) == sum(outgoing.values()) <= 15, "unbalanced plan")
    require(all(n <= main[c] for c, n in outgoing.items()), "plan removes unregistered main card")
    require(all(n <= side[c] for c, n in incoming.items()), "plan adds unregistered side card")
    result = configuration(expand(main - outgoing + incoming), expand(side - incoming + outgoing))
    require(Counter(result["mainboard"] + result["sideboard"]) == main + side, "plan changes registered 75")
    return result


def replay_example(example: dict) -> dict:
    require(set(example) == {"input", "initial_mainboard", "initial_sideboard", "target_actions", "target_value"},
            "unexpected imitation record fields")
    require(example["target_value"] is None, "static teacher must have no value target")
    initial = configuration(example["initial_mainboard"], example["initial_sideboard"])
    registered = counts(example["input"]["registered_cards"])
    main, side = Counter(initial["mainboard"]), Counter(initial["sideboard"])
    require(registered == main + side, "example registered 75 mismatch")
    actions = example["target_actions"]
    require(1 <= len(actions) <= 64, "invalid action trace length")
    adding, removed, out_count = False, set(), 0
    for index, action in enumerate(actions):
        if action == {"kind": "done"}:
            require(index == len(actions) - 1, "Done must be last")
            require(sum(main.values()) == 60 and sum(side.values()) == 15, "Done at invalid configuration")
            return configuration(expand(main), expand(side))
        require(set(action) == {"kind", "card_id"}, "invalid movement fields")
        card = action["card_id"]
        require(type(card) is int and card in registered, "unregistered action card")
        kind = action["kind"]
        require(kind in ("move_one_to_sideboard", "move_one_to_mainboard"), "unknown action kind")
        vacancy = 60 - sum(main.values())
        if kind == "move_one_to_sideboard":
            capacity = sum(n for c, n in side.items() if c != card and c not in removed)
            require(not adding and out_count < 15 and capacity > vacancy, "illegal removal prefix")
            removed.add(card)
            out_count += 1
        else:
            require(vacancy > 0 and card not in removed, "illegal addition or reversal")
            adding = True
        source, target = (main, side) if kind == "move_one_to_sideboard" else (side, main)
        require(source[card] > 0, "movement from empty zone")
        source[card] -= 1
        target[card] += 1
    raise ValueError("missing Done")


def strict_teacher_rows(source_path: Path, original_path: Path, registrations: dict,
                        registry: dict, pairs: list[list[str]]) -> tuple[list[dict], dict]:
    """Select real plans; missing rows, name/id drift, and substitutions fail."""
    source = read_json(source_path)
    originals = read_json(original_path)["decks"]
    decks = sorted({deck for pair in pairs for deck in pair})
    name_checks = 0
    for deck in decks:
        current = registrations[deck]
        for source_zone, zone in (("main", "mainboard"), ("side", "sideboard")):
            names = Counter(originals[deck][source_zone])
            original_counts = counts([{"card_id": registry["ids"][name], "count": count} for name, count in names.items()])
            require(expand(original_counts) == current[zone],
                    f"original registration drift: {deck}/{zone}")
            name_checks += len(set(names))
        for card, count in Counter(current["mainboard"] + current["sideboard"]).items():
            definition = registry["cards"][card]
            require(definition["engine_capability"].lower() == "full", f"unsupported registration card: {definition['name']}")
            require(count <= 4 or "Basic" in definition.get("supertypes", []), "nonbasic copy limit")
    rows, validation = [], []
    require(len({tuple(pair) for pair in pairs}) == len(pairs), "duplicate pair")
    for own, opponent in pairs:
        for game in (2, 3):
            matches = [p for p in source["plans"] if
                       (p["self_deck_id"], p["opponent_deck_id"], p["game_index"]) == (own, opponent, game)]
            require(len(matches) == 1, f"missing or duplicate existing teacher: {own}/{opponent}/{game}")
            plan = matches[0]
            require(not plan.get("legality_problems"), "source reports legality problems")
            row = {"self_deck_id": own, "opponent_deck_id": opponent, "game_index": game}
            for zone in ("cards_in", "cards_out"):
                for item in plan[zone]:
                    require(registry["ids"].get(item["name"]) == item["card_id"], "teacher name/card id mismatch")
                    name_checks += 1
                row[zone] = sorted(({"card_id": c["card_id"], "count": c["count"]} for c in plan[zone]),
                                   key=lambda c: c["card_id"])
            target = plan_target(registrations[own], row)
            rows.append(row)
            validation.append({"key": [own, opponent, game], "source_plan_sha256": identity(plan),
                               "target_sha256": identity(target), "swaps": sum(c["count"] for c in row["cards_in"])})
    return rows, {"decks": decks, "rows": validation, "exact_name_id_checks": name_checks,
                  "all_original_registrations_match": True, "all_targets_60_15_conserve_75": True,
                  "full_engine_flags_checked": True, "runtime_mechanic_coverage_claim": False,
                  "source": pin(source_path), "original_registrations": pin(original_path)}


def validate_observation_contract(config: dict, provenance: dict, match: dict) -> None:
    """Reject a stale match record relabeled by a different batch transfer mode."""
    requested = config.get("play_observation_transfer_v3")
    transfer = provenance["play_transfer"]
    successor = transfer.get("observation_successor")
    if requested is None:
        require(successor is None, "legacy config has successor provenance")
        require(match.get("play_observation_contract") is None, "legacy config has successor match contract")
        return
    require(isinstance(requested, dict) and set(requested) == {
        "expected_feature_contract_digest", "expected_feature_encoding_digest"}, "invalid successor config")
    require(isinstance(successor, dict) and successor.get("schema") == "mtg-kernel-frozen-play-observation-transfer/v3",
            "successor config lacks V3 provenance")
    require(successor.get("destination") == requested, "successor destination differs from config")
    for field in ("feature_contract_digest", "feature_encoding_digest"):
        require(transfer.get(field) == requested[f"expected_{field}"], "successor feature identity mismatch")
    require(match.get("play_observation_contract") == "rich-v6-flat-v3-explicit-frozen-feature-transfer",
            "successor config has stale or missing match observation contract")


def audit_batch(batch: Path, registrations: dict) -> tuple[list[dict], dict]:
    completion_path = batch / "completion.json"
    completion = read_json(completion_path)
    require(completion["mode"] == "run_batch", "not a completed run batch")
    provenance_path = batch / "imitation-examples.provenance.json"
    provenance = read_json(provenance_path)
    require(provenance["schema"] == "kernel-static-sideboard-teaching-provenance/v1", "unknown provenance schema")
    dataset_path = batch / "imitation-examples.jsonl"
    dataset_pin = verify_pin(completion["examples"], dataset_path)
    verify_pin(provenance["examples"], dataset_path)
    verified = [verify_pin(receipt) for receipt in provenance["source_inputs"]]
    require(completion["inputs"] == provenance["source_inputs"], "completion source inputs differ")
    config = read_json(batch / "config.json")
    require(config["matches"] == provenance["matches"] and config["policies"] == provenance["policies"],
            "config differs from teacher provenance")
    require(pin(batch / "config.json")["sha256"] in {p["sha256"] for p in verified}, "config lacks source pin")
    require(completion["completed_matches"] == len(config["matches"]), "incomplete match count")
    policies = provenance["policies"]
    table_maps = {}
    for seat, policy in enumerate(policies):
        if policy["kind"] != "static_plan_rows":
            continue
        verify_pin(policy["table"])
        verify_pin(policy["teacher_provenance"]["artifact"])
        require(policy["teacher_provenance"]["source_kind"] == "hand_authored_warm_start",
                "this utility currently accepts hand-authored warm starts only")
        table = read_json(Path(policy["table"]["path"]))["rows"]
        keys = [(r["self_deck_id"], r["opponent_deck_id"], r["game_index"]) for r in table]
        require(len(set(keys)) == len(keys), "duplicate teacher table key")
        table_maps[seat] = dict(zip(keys, table))
        original_plans = read_json(Path(policy["teacher_provenance"]["artifact"]["path"]))["plans"]
        for row in table:
            key = (row["self_deck_id"], row["opponent_deck_id"], row["game_index"])
            originals = [r for r in original_plans if
                         (r["self_deck_id"], r["opponent_deck_id"], r["game_index"]) == key]
            require(len(originals) == 1, "missing or duplicate teacher source row")
            require(not originals[0].get("legality_problems"), "teacher source reports legality problems")
            for zone in ("cards_in", "cards_out"):
                require(counts(originals[0][zone]) == counts(row[zone]), "table differs from original source plan")
    all_bytes, entries, match_pins, games = b"", [], [], 0
    for index, match_config in enumerate(config["matches"]):
        match_path = batch / f"match-{index:06}.json"
        match = read_json(match_path)
        require(match["schema"] == "kernel_learned_bo3/v1" and match["config"] == match_config, "match identity mismatch")
        validate_observation_contract(config, provenance, match)
        require(match["play_weights_sha256"] == provenance["play_transfer"]["weights_sha256"], "play weights mismatch")
        own_regs = [registrations[d] for d in match_config["deck_ids"]]
        games += len(match["games"])
        require(match["games"][0]["mainboard_sha256"] == [mainboard_hash(r["mainboard"]) for r in own_regs],
                "game-one registration hash mismatch")
        # A rerun under different limits, weights or observation encoding still
        # shares the original matchup/seed group. Keep execution pins separately.
        match_group = identity({"deck_ids": match_config["deck_ids"], "seed": match_config["seed"],
                                "game_one_chooser": match_config["game_one_chooser"], "registrations": own_regs})
        example_path = batch / f"examples-{index:06}.jsonl"
        raw = example_path.read_bytes()
        all_bytes += raw
        lines = raw.splitlines()
        records = [r for r in match["sideboard_decisions"] if r["acting_player"] in table_maps]
        require(len(lines) == len(records), "per-match example count mismatch")
        for line_index, (line, record) in enumerate(zip(lines, records)):
            example = json.loads(line)
            expected = {"input": record["input"], "initial_mainboard": record["initial_mainboard"],
                        "initial_sideboard": record["initial_sideboard"],
                        "target_actions": record["selected_actions"], "target_value": None}
            require(example == expected, "example differs from actual match decision")
            seat = record["acting_player"]
            require(type(seat) is int and seat in (0, 1), "invalid acting seat")
            own, opponent = match_config["deck_ids"][seat], match_config["deck_ids"][1 - seat]
            registration = own_regs[seat]
            require(counts(example["input"]["registered_cards"]) == Counter(registration["mainboard"] + registration["sideboard"]),
                    "example differs from pinned registration")
            game = example["input"]["next_game_number"]
            require(type(game) is int and 2 <= game <= len(match["games"]), "invalid example game boundary")
            require(mainboard_hash(example["initial_mainboard"]) == match["games"][game - 2]["mainboard_sha256"][seat],
                    "example does not start from preceding game's configuration")
            table_game = 3 if game > 3 and policies[seat]["carry_game_three_forward"] else game
            key = (own, opponent, table_game)
            require(key in table_maps[seat], f"missing requested teacher row: {key}")
            row = table_maps[seat][key]
            target = plan_target(registration, row)
            require(replay_example(example) == target, "trace does not reach teacher target")
            require(mainboard_hash(target["mainboard"]) == record["selected_mainboard_sha256"], "target hash mismatch")
            require(record["selected_mainboard_sha256"] == match["games"][game - 1]["mainboard_sha256"][seat],
                    "teacher target differs from submitted next game")
            # Ignore game, opponent, source prose and initial configuration: the
            # same own-registration target is the same teacher plan everywhere.
            plan_group = identity({"registration": registration, "target": target})
            metadata = {"record_id": f"{dataset_pin['sha256']}:{index}:{line_index}",
                        "example_sha256": identity(example), "batch": batch.resolve().as_posix(),
                        "match_index": index, "match_group": match_group, "seed": match_config["seed"],
                        "play_weights_sha256": match["play_weights_sha256"],
                        "play_feature_contract_digest": provenance["play_transfer"]["feature_contract_digest"],
                        "play_feature_encoding_digest": provenance["play_transfer"].get("feature_encoding_digest"),
                        "acting_player": seat, "next_game_number": game,
                        "registration_pair": [own, opponent],
                        "registration_pair_sha256": identity([own_regs[seat], own_regs[1 - seat]]),
                        "own_registration_sha256": identity(registration),
                        "opponent_registration_sha256": identity(own_regs[1 - seat]),
                        "teacher_row_key": list(key), "teacher_row_sha256": identity(row),
                        "teacher_plan_group": plan_group, "teacher_provenance": policies[seat]["teacher_provenance"],
                        "teacher_table": policies[seat]["table"], "action_targets": len(example["target_actions"]),
                        "movement_targets": len(example["target_actions"]) - 1, "target_value_present": False}
            entries.append({"metadata": metadata, "example": example, "line": line})
        match_pins.append({"match": pin(match_path), "examples": pin(example_path), "match_group": match_group})
    require(all_bytes == dataset_path.read_bytes(), "aggregate JSONL differs from ordered per-match files")
    require(len(entries) == completion["static_imitation_examples"] == provenance["example_count"], "aggregate example count mismatch")
    require(games == completion["physical_games"], "physical game count mismatch")
    return entries, {"completion": pin(completion_path), "provenance_file": pin(provenance_path),
                     "provenance": provenance, "verified_source_inputs": verified,
                     "dataset": dataset_pin, "matches": match_pins}


def grouped_split(metadata: list[dict], seed: int, eval_fraction: float) -> tuple[list[dict], dict[str, str]]:
    """Connected components enforce match, repeated-plan and exact-row grouping."""
    require(type(seed) is int and 0 <= seed < 2**64, "invalid split seed")
    require(math.isfinite(eval_fraction) and 0 < eval_fraction < 1, "invalid eval fraction")
    parents = list(range(len(metadata)))
    def find(i: int) -> int:
        while parents[i] != i:
            parents[i] = parents[parents[i]]
            i = parents[i]
        return i
    seen = {}
    for i, row in enumerate(metadata):
        for field in ("match_group", "teacher_plan_group", "example_sha256"):
            key = (field, row[field])
            if key in seen:
                parents[find(i)] = find(seen[key])
            seen[key] = i
    groups = {}
    for i, row in enumerate(metadata):
        groups.setdefault(find(i), []).append(row)
    components = []
    for group in groups.values():
        group_identity = {field: sorted({r[field] for r in group})
                          for field in ("match_group", "teacher_plan_group", "example_sha256")}
        # Outcome-bearing example bytes establish duplicate links, but do not
        # determine the seeded rank. Component ranking uses design identities.
        component_id = identity({k: group_identity[k] for k in ("match_group", "teacher_plan_group")})
        components.append({"component_id": component_id, "groups": group_identity,
                           "record_ids": sorted(r["record_id"] for r in group),
                           "examples": len(group), "action_targets": sum(r["action_targets"] for r in group),
                           "stratum": "cross_or_mixed" if any(r["registration_pair"][0] != r["registration_pair"][1] for r in group) else "mirror",
                           "registration_pairs": sorted({tuple(r["registration_pair"]) for r in group})})
    components.sort(key=lambda c: identity([seed, c["component_id"]]))
    # Structural strata avoid a mirror-only holdout when cross components exist.
    # Fraction applies to components within each stratum, not individual rows.
    evaluation_ids = set()
    for stratum in ("mirror", "cross_or_mixed"):
        members = [c for c in components if c["stratum"] == stratum]
        count = min(len(members) - 1, max(1, math.ceil(len(members) * eval_fraction))) if len(members) > 1 else 0
        evaluation_ids.update(c["component_id"] for c in members[:count])
    assignments = {}
    for component in components:
        component["split"] = "imitation_eval" if component["component_id"] in evaluation_ids else "train"
        for record_id in component["record_ids"]:
            require(record_id not in assignments, "duplicate input dataset/record")
            assignments[record_id] = component["split"]
    return components, assignments


def write_new(path: Path, data: bytes) -> dict:
    with path.open("xb") as stream:
        stream.write(data)
    return pin(path)


def write_json_new(path: Path, value: Any) -> dict:
    return write_new(path, (json.dumps(value, indent=2, ensure_ascii=False, allow_nan=False) + "\n").encode("utf-8"))


def prepare_inventory(batches: list[Path], registry_path: Path, pool_path: Path,
                      output: Path, seed: int, eval_fraction: float,
                      requested_decks: list[str] | None = None) -> dict:
    require(not output.exists(), "output directory must be fresh")
    registrations, _ = load_registrations(registry_path, pool_path)
    entries, batch_receipts = [], []
    for batch in sorted(batches, key=lambda p: p.resolve().as_posix()):
        batch_entries, receipt = audit_batch(batch, registrations)
        entries.extend(batch_entries)
        batch_receipts.append(receipt)
    require(bool(entries), "empty dataset")
    metadata = [e["metadata"] for e in entries]
    components, assignments = grouped_split(metadata, seed, eval_fraction)
    for row in metadata:
        row["split"] = assignments[row["record_id"]]
    observed_decks = sorted({d for r in metadata for d in r["registration_pair"]})
    decks = sorted(requested_decks) if requested_decks is not None else observed_decks
    require(len(decks) == len(set(decks)) and bool(decks), "requested deck universe must be nonempty and unique")
    require(set(observed_decks) <= set(decks), "requested deck universe excludes observed decks")
    require(set(decks) <= set(registrations), "requested deck universe contains unknown registration")
    coverage = []
    for own in decks:
        for opponent in decks:
            rows = [r for r in metadata if r["registration_pair"] == [own, opponent]]
            coverage.append({"own": own, "opponent": opponent, "examples": len(rows),
                             "action_targets": sum(r["action_targets"] for r in rows),
                             "done_only_examples": sum(r["action_targets"] == 1 for r in rows),
                             "match_groups": len({r["match_group"] for r in rows}),
                             "game_numbers": sorted({r["next_game_number"] for r in rows}),
                             "splits": dict(Counter(r["split"] for r in rows))})
    report = {"schema": "sideboard-dataset-development-inventory/v1", "split_seed": seed,
              "requested_eval_fraction_of_components": eval_fraction,
              "status": "development_split_proposal" if "imitation_eval" in assignments.values() else "no_independent_holdout_within_strata",
              "grouping": ["same match identity", "same own registration and teacher target configuration", "identical example"],
              "stratification": "mirror versus cross/mixed components; a singleton stratum remains training-only",
              "split_selection_uses": "seeded SHA256 order of connected-component identities, no native results or fit metrics",
              "nonclaims": ["not frozen promotion gates", "not BO3 playing-strength evaluation", "not search-ratified expertise",
                            "not value learning", "not evidence of unseen-deck generalization"],
              "counts": {"examples": len(entries), "action_targets": sum(r["action_targets"] for r in metadata),
                         "match_groups": len({r["match_group"] for r in metadata}),
                         "teacher_plan_groups": len({r["teacher_plan_group"] for r in metadata}),
                         "components": len(components), "value_labels": 0,
                         "done_only_examples": sum(r["action_targets"] == 1 for r in metadata),
                         "examples_by_game_number": dict(Counter(r["next_game_number"] for r in metadata)),
                         "examples_by_split": dict(Counter(assignments.values()))},
              "inputs": {"registry": pin(registry_path), "pool": pin(pool_path), "batches": batch_receipts},
              "runtime": {"python": platform.python_version(), "implementation": platform.python_implementation()},
              "utility_source": pin(Path(__file__)), "registrations": registrations,
              "coverage_universe": {"mode": "explicit_requested_decks" if requested_decks is not None else "observed_decks_only",
                                    "requested_decks": decks, "observed_decks": observed_decks,
                                    "entirely_absent_decks": sorted(set(decks) - set(observed_decks))},
              "coverage": coverage, "components": components, "records": metadata}
    output.mkdir(parents=False)
    files = {}
    for split in ("train", "imitation_eval"):
        data = b"".join(e["line"] + b"\n" for e in entries if assignments[e["metadata"]["record_id"]] == split)
        files[split] = write_new(output / f"{split}.jsonl", data)
    files["all_examples"] = write_new(output / "all-examples.jsonl", b"".join(e["line"] + b"\n" for e in entries))
    report["outputs"] = files
    write_json_new(output / "inventory.json", report)
    return report


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--batch", type=Path, action="append", required=True)
    parser.add_argument("--registry", type=Path, required=True)
    parser.add_argument("--pool", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--split-seed", type=int, required=True)
    parser.add_argument("--eval-fraction", type=float, required=True)
    parser.add_argument("--requested-deck", action="append", help="Explicit coverage universe; repeat for each deck, including absent decks")
    args = parser.parse_args()
    report = prepare_inventory(args.batch, args.registry, args.pool, args.output, args.split_seed, args.eval_fraction,
                               requested_decks=args.requested_deck)
    print(json.dumps({"status": report["status"], "counts": report["counts"], "outputs": report["outputs"]}, indent=2))


if __name__ == "__main__":
    main()
