"""Small artificial records exercise audit rejection and split independence only."""
import copy
import json
from pathlib import Path
import tempfile
import unittest

from mtg_kernel_rl import sideboard_dataset_v1 as ds


def write(path, value):
    path.write_text(json.dumps(value), encoding="utf-8")


def fixture(root, v3=False):
    registration = ds.configuration([0] * 60, [1] * 15)
    registrations = {"A": registration, "B": registration}
    rows = [{"self_deck_id": own, "opponent_deck_id": opponent, "game_index": 2,
             "cards_in": [{"card_id": 1, "count": 1}], "cards_out": [{"card_id": 0, "count": 1}]}
            for own, opponent in (("A", "B"), ("B", "A"))]
    source_path, table_path = root / "source.json", root / "table.json"
    write(source_path, {"plans": rows})
    write(table_path, {"rows": rows})
    policy = {"kind": "static_plan_rows", "table": ds.pin(table_path), "carry_game_three_forward": True,
              "teacher_provenance": {"source_kind": "hand_authored_warm_start", "artifact": ds.pin(source_path)}}
    match_config = {"deck_ids": ["A", "B"], "seed": 20, "game_one_chooser": 0,
                    "max_physical_games": 6, "max_physical_decisions": 4000, "max_policy_steps": 40000}
    config = {"mode": "run_batch", "policies": [policy, policy], "matches": [match_config]}
    if v3:
        config["play_observation_transfer_v3"] = {"expected_feature_contract_digest": "v3-contract",
                                                  "expected_feature_encoding_digest": "v3-encoding"}
    write(root / "config.json", config)
    example = {"input": {"registered_cards": [{"card_id": 0, "count": 60}, {"card_id": 1, "count": 15}],
                         "next_game_number": 2}, "initial_mainboard": [0] * 60, "initial_sideboard": [1] * 15,
               "target_actions": [{"kind": "move_one_to_sideboard", "card_id": 0},
                                  {"kind": "move_one_to_mainboard", "card_id": 1}, {"kind": "done"}],
               "target_value": None}
    target_hash = ds.mainboard_hash(ds.plan_target(registration, rows[0])["mainboard"])
    records = [{"acting_player": seat, "input": example["input"], "initial_mainboard": example["initial_mainboard"],
                "initial_sideboard": example["initial_sideboard"], "selected_actions": example["target_actions"],
                "selected_mainboard_sha256": target_hash} for seat in (0, 1)]
    match = {"schema": "kernel_learned_bo3/v1", "config": match_config, "play_weights_sha256": "weights",
             "games": [{"mainboard_sha256": [ds.mainboard_hash(registration["mainboard"])] * 2},
                       {"mainboard_sha256": [target_hash] * 2}], "sideboard_decisions": records}
    if v3:
        match["play_observation_contract"] = "rich-v6-flat-v3-explicit-frozen-feature-transfer"
    write(root / "match-000000.json", match)
    raw = (json.dumps(example) + "\n").encode() * 2
    (root / "examples-000000.jsonl").write_bytes(raw)
    (root / "imitation-examples.jsonl").write_bytes(raw)
    inputs = [ds.pin(root / "config.json"), ds.pin(source_path), ds.pin(table_path)]
    provenance = {"schema": "kernel-static-sideboard-teaching-provenance/v1", "examples": ds.pin(root / "imitation-examples.jsonl"),
                  "source_inputs": inputs, "matches": config["matches"], "policies": config["policies"],
                  "example_count": 2, "play_transfer": {"weights_sha256": "weights", "feature_contract_digest": "contract"}}
    if v3:
        provenance["play_transfer"].update({"feature_contract_digest": "v3-contract", "feature_encoding_digest": "v3-encoding",
            "observation_successor": {"schema": "mtg-kernel-frozen-play-observation-transfer/v3",
                                      "destination": config["play_observation_transfer_v3"]}})
    write(root / "imitation-examples.provenance.json", provenance)
    write(root / "completion.json", {"mode": "run_batch", "examples": provenance["examples"], "inputs": inputs,
                                      "completed_matches": 1, "static_imitation_examples": 2, "physical_games": 2})
    return registrations, example


class DatasetAuditTests(unittest.TestCase):
    def test_explicit_universe_reports_entirely_absent_burn(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            batch = root / "batch"
            batch.mkdir()
            fixture(batch)
            write(root / "registry.json", {"cards": [{"name": "Main"}, {"name": "Side"}]})
            write(root / "pool.json", {"decks": [
                {"id": deck, "mainboard": {"cards": [{"name": "Main", "count": 60}]},
                 "sideboard": {"cards": [{"name": "Side", "count": 15}]}} for deck in ("A", "B", "Burn")]})
            report = ds.prepare_inventory([batch], root / "registry.json", root / "pool.json", root / "out", 1, 0.2,
                                          requested_decks=["A", "B", "Burn"])
            self.assertEqual(report["coverage_universe"]["entirely_absent_decks"], ["Burn"])
            self.assertEqual(len(report["coverage"]), 9)
            burn = [row for row in report["coverage"] if "Burn" in (row["own"], row["opponent"])]
            self.assertEqual(len(burn), 5)
            self.assertTrue(all(row["examples"] == 0 for row in burn))
            with self.assertRaisesRegex(ValueError, "excludes observed decks"):
                ds.prepare_inventory([batch], root / "registry.json", root / "pool.json", root / "bad-out", 1, 0.2,
                                     requested_decks=["A", "Burn"])
            self.assertFalse((root / "bad-out").exists())

    def test_successor_mode_requires_exact_match_contract(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            registrations, _ = fixture(root, v3=True)
            entries, _ = ds.audit_batch(root, registrations)
            self.assertEqual(len(entries), 2)
            match = ds.read_json(root / "match-000000.json")
            del match["play_observation_contract"]
            write(root / "match-000000.json", match)
            with self.assertRaisesRegex(ValueError, "stale or missing match observation contract"):
                ds.audit_batch(root, registrations)

    def test_legacy_mode_rejects_successor_match_contract(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            registrations, _ = fixture(root)
            match = ds.read_json(root / "match-000000.json")
            match["play_observation_contract"] = "rich-v6-flat-v3-explicit-frozen-feature-transfer"
            write(root / "match-000000.json", match)
            with self.assertRaisesRegex(ValueError, "legacy config has successor match contract"):
                ds.audit_batch(root, registrations)

    def test_successor_provenance_must_match_config_digest(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            registrations, _ = fixture(root, v3=True)
            provenance = ds.read_json(root / "imitation-examples.provenance.json")
            provenance["play_transfer"]["feature_encoding_digest"] = "stale-encoding"
            write(root / "imitation-examples.provenance.json", provenance)
            with self.assertRaisesRegex(ValueError, "successor feature identity mismatch"):
                ds.audit_batch(root, registrations)

    def test_actual_file_join_and_teacher_target(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            registrations, _ = fixture(root)
            entries, report = ds.audit_batch(root, registrations)
            self.assertEqual(len(entries), 2)
            self.assertEqual([e["metadata"]["acting_player"] for e in entries], [0, 1])
            self.assertEqual(report["dataset"], ds.pin(root / "imitation-examples.jsonl"))
            self.assertEqual(entries[0]["metadata"]["teacher_plan_group"], entries[1]["metadata"]["teacher_plan_group"])

    def test_tampered_dataset_hash_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            registrations, _ = fixture(root)
            with (root / "imitation-examples.jsonl").open("ab") as stream:
                stream.write(b"\n")
            with self.assertRaisesRegex(ValueError, "SHA256 mismatch"):
                ds.audit_batch(root, registrations)

    def test_per_match_join_rejects_misattributed_seat_input(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            registrations, _ = fixture(root)
            match = ds.read_json(root / "match-000000.json")
            match["sideboard_decisions"][0]["input"] = {"registered_cards": [], "next_game_number": 2}
            write(root / "match-000000.json", match)
            with self.assertRaisesRegex(ValueError, "actual match decision"):
                ds.audit_batch(root, registrations)

    def test_game_submission_hash_must_match_example(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            registrations, _ = fixture(root)
            match = ds.read_json(root / "match-000000.json")
            match["games"][1]["mainboard_sha256"][0] = "forged"
            write(root / "match-000000.json", match)
            with self.assertRaisesRegex(ValueError, "submitted next game"):
                ds.audit_batch(root, registrations)

    def test_value_labels_and_illegal_trace_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            _, example = fixture(Path(directory))
            labeled = copy.deepcopy(example)
            labeled["target_value"] = 0.0
            with self.assertRaisesRegex(ValueError, "no value target"):
                ds.replay_example(labeled)
            reversed_move = copy.deepcopy(example)
            reversed_move["target_actions"][1]["card_id"] = 0
            with self.assertRaisesRegex(ValueError, "reversal"):
                ds.replay_example(reversed_move)
            no_done = copy.deepcopy(example)
            no_done["target_actions"].pop()
            with self.assertRaisesRegex(ValueError, "missing Done"):
                ds.replay_example(no_done)

    def test_existing_artifact_never_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "artifact.json"
            ds.write_json_new(path, {"first": True})
            before = path.read_bytes()
            with self.assertRaises(FileExistsError):
                ds.write_json_new(path, {"first": False})
            self.assertEqual(path.read_bytes(), before)


class GroupingTests(unittest.TestCase):
    @staticmethod
    def row(name, match, plan, example=None):
        return {"record_id": name, "match_group": match, "teacher_plan_group": plan,
                "example_sha256": example or name, "action_targets": 3, "registration_pair": ["A", "B"]}

    def test_transitive_links_prevent_match_plan_and_duplicate_leakage(self):
        rows = [self.row("a", "m1", "p1"), self.row("b", "m1", "p2"),
                self.row("c", "m2", "p2"), self.row("d", "m3", "p3", "c"),
                self.row("e", "m4", "p4")]
        components, assignment = ds.grouped_split(rows, 99, 0.2)
        self.assertEqual(len(components), 2)
        self.assertEqual(len({assignment[k] for k in "abcd"}), 1)
        self.assertNotEqual(assignment["a"], assignment["e"])

    def test_order_invariance_and_outcome_bytes_do_not_rank_components(self):
        rows = [self.row(str(i), f"m{i}", f"p{i}") for i in range(5)]
        _, expected = ds.grouped_split(rows, 123, 0.2)
        _, reordered = ds.grouped_split(list(reversed(rows)), 123, 0.2)
        self.assertEqual(expected, reordered)
        changed = copy.deepcopy(rows)
        for row in changed:
            row["example_sha256"] += "changed-outcome"
        _, new_assignment = ds.grouped_split(changed, 123, 0.2)
        self.assertEqual(expected, new_assignment)

    def test_single_connected_component_does_not_fabricate_holdout(self):
        rows = [self.row("a", "m1", "same-plan"), self.row("b", "m2", "same-plan")]
        components, assignment = ds.grouped_split(rows, 9, 0.5)
        self.assertEqual(len(components), 1)
        self.assertEqual(set(assignment.values()), {"train"})

    def test_mirror_and_cross_components_both_get_holdout(self):
        rows = [self.row(str(i), f"m{i}", f"p{i}") for i in range(4)]
        for row in rows[:2]:
            row["registration_pair"] = ["A", "A"]
        components, _ = ds.grouped_split(rows, 123, 0.2)
        held_out = [c for c in components if c["split"] == "imitation_eval"]
        self.assertEqual({c["stratum"] for c in held_out}, {"mirror", "cross_or_mixed"})
        self.assertEqual(len(held_out), 2)

    def test_duplicate_source_dataset_rejected(self):
        row = self.row("a", "m1", "p1")
        with self.assertRaisesRegex(ValueError, "duplicate input dataset"):
            ds.grouped_split([row, row], 9, 0.2)


class ExistingTeacherTests(unittest.TestCase):
    def test_exact_existing_rows_only_no_fallback_and_name_id_verified(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            registrations, _ = fixture(root)
            source = ds.read_json(root / "source.json")
            for row in source["plans"]:
                for zone, name in (("cards_in", "Side"), ("cards_out", "Main")):
                    row[zone][0]["name"] = name
            source["plans"] += [dict(copy.deepcopy(r), game_index=3) for r in source["plans"]]
            write(root / "source.json", source)
            originals = {"decks": {d: {"main": {"Main": 60}, "side": {"Side": 15}} for d in ("A", "B")}}
            write(root / "original.json", originals)
            registry = {"ids": {"Main": 0, "Side": 1}, "cards": [
                {"name": name, "engine_capability": "Full", "supertypes": ["Basic"]} for name in ("Main", "Side")]}
            rows, _ = ds.strict_teacher_rows(root / "source.json", root / "original.json", registrations, registry, [["A", "B"]])
            self.assertEqual(len(rows), 2)
            with self.assertRaisesRegex(ValueError, "missing or duplicate existing teacher"):
                ds.strict_teacher_rows(root / "source.json", root / "original.json", registrations, registry, [["A", "A"]])
            source["plans"][0]["cards_in"][0]["name"] = "Main"
            write(root / "source.json", source)
            with self.assertRaisesRegex(ValueError, "name/card id mismatch"):
                ds.strict_teacher_rows(root / "source.json", root / "original.json", registrations, registry, [["A", "B"]])


if __name__ == "__main__":
    unittest.main()
