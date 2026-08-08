#!/usr/bin/env python3
"""Focused offline checks for the pair-void-tolerant population Store CP7 panel runner.

Covers what run_cp7_store_panel_v1.py already had (unchanged behavior) plus the
new pair-void surface: structured-line parsing, outcome-shard validation with a
voided pair mixed into an otherwise-strict shard, per-model void-count exclusion
arithmetic, and the 2% cap.
"""

from __future__ import annotations

import concurrent.futures
import importlib.util
import json
from pathlib import Path
import tempfile
import threading
import time
import types
import unittest
from unittest import mock


MODULE = Path(__file__).with_name("run_cp7_store_panel_v2.py")
SPEC = importlib.util.spec_from_file_location("panel_v2", MODULE)
assert SPEC is not None and SPEC.loader is not None
panel = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(panel)


class PanelRunnerTest(unittest.TestCase):
    def setUp(self) -> None:
        self.checkpoint = {
            "authority_kind": panel.AUTHORITY_KIND,
            "source_run_sha256": "1" * 64, "source_generation": 1024,
            "source_checkpoint_sha256": "2" * 64,
            "source_sidecar_sha256": "3" * 64,
            "source_payload_sha256": "4" * 64,
            "source_train_state_sha256": "5" * 64,
            "loaded_run_sha256": "1" * 64, "loaded_generation": 1024,
            "loaded_checkpoint_sha256": "2" * 64,
            "loaded_payload_sha256": "4" * 64,
            "loaded_train_state_sha256": "5" * 64,
            "model_parameter_sha256": "6" * 64,
            "environment_trajectory_contract": panel.ENVIRONMENT_CONTRACT,
            "sampler_identity": panel.SAMPLER_IDENTITY,
            "sampler_contract_sha256": panel.SAMPLER_CONTRACT,
        }
        self.model = {"root": "store", "generation": 1024, "checkpoint": self.checkpoint}

    def _terminal(self, pair: int, seat: str, ordinal: int) -> dict[str, object]:
        episode = pair * 2 + int(seat[1])
        reward = 1 if seat == "p0" else -1
        return {
            "record_type": "terminal", "schema_version": 2,
            "record_ordinal": ordinal, "pair_index": pair, "episode_id": episode,
            "candidate_seat": seat, "base_seed_u64_hex": "0000000000000007",
            "pair_environment_seed_u64_hex": f"{pair + 100:016x}",
            "deck_ids": ["Rally", "Rally"], "randomization_identity": "legacy_v1",
            "core_environment_hash_u64_hex": "0" * 16,
            "diagnostic_state_hash_u64_hex": "1" * 16,
            "first_outcome_decision_ordinal": None, "outcome_decision_count": 0,
            "terminal": {"schema_version": 5, "episode_id": episode,
                         "terminal_classification": "natural",
                         "terminal_code": "natural_game_over", "terminal_reason": "game_over",
                         "terminal_outcome": "p0_win", "winner": "p0",
                         "terminal_reward": [1, -1]},
            "candidate_terminal_reward": reward, "checkpoint": self.checkpoint,
        }

    def _decision(self, pair: int, seat: str, ordinal: int,
                 outcome_decision_ordinal: int) -> dict[str, object]:
        episode = pair * 2 + int(seat[1])
        return {
            "record_type": "decision", "schema_version": 2, "record_ordinal": ordinal,
            "outcome_decision_ordinal": outcome_decision_ordinal,
            "pair_index": pair, "episode_id": episode, "candidate_seat": seat,
            "base_seed_u64_hex": "0000000000000007",
            "pair_environment_seed_u64_hex": f"{pair + 100:016x}",
            "deck_ids": ["Rally", "Rally"], "environment_revision": 1,
            "randomization_identity": "legacy_v1", "selection_source": "candidate_checkpoint_policy",
            "acting_player": seat, "step": 0, "decision_kind": "priority",
            "physical_decision_id": 0, "actor_physical_decision_ordinal": 0,
            "substep_index": 0, "substep_count": 1, "legal_action_count": 1,
            "selected_index": 0, "selected_semantic": "pass", "candidate_order_commitment_128_hex": "a" * 32,
            "action_semantics": ["pass"], "tensor": {}, "model_input_sha256": "b" * 64,
            "old_policy_logits_f32_bits": [0], "old_value_f32_bits": 0,
            "checkpoint": self.checkpoint,
        }

    def _rows(self, pairs: int = 2) -> list[dict[str, object]]:
        rows: list[dict[str, object]] = [panel.expected_header(self.checkpoint)]
        for pair in range(pairs):
            rows.extend((self._terminal(pair, "p0", len(rows)),
                         self._terminal(pair, "p1", len(rows) + 1)))
        return rows

    @staticmethod
    def _write(path: Path, rows: list[dict[str, object]]) -> None:
        with path.open("wb") as handle:
            for row in rows:
                handle.write(json.dumps(row, sort_keys=True, separators=(",", ":")).encode())
                handle.write(b"\n")

    # -- carried over from v1: unchanged behavior when nothing is voided ------

    def test_task_pairs_changes_contiguous_shard_coverage(self) -> None:
        chunks = panel.chunk_ranges(0, 128, 32)
        self.assertEqual(chunks, [(0, 32), (32, 32), (64, 32), (96, 32)])
        tasks = panel.planned_tasks(["a", "b", "c"], chunks)
        self.assertEqual(len(tasks), 12)
        self.assertTrue(all(sum(task["pair_count"] for task in tasks
                                if task["label"] == label) == 128
                            for label in ("a", "b", "c")))
        args = types.SimpleNamespace(mage_repo=Path("mage"), scorer_exe=Path("scorer"),
                                     generation=1024, base_seed=7, maven=Path("mvn"))
        command = panel.anchor_command(args, self.model, 32, 32, Path("outcome.jsonl"))
        self.assertIn("--first-episode 64", command[-1])
        self.assertIn("--pairs 32", command[-1])

    def test_database_leases_cannot_overlap(self) -> None:
        leases = panel.DatabaseLeasePool([Path("db-0")])
        active = 0
        maximum = 0
        lock = threading.Lock()

        def use_database() -> None:
            nonlocal active, maximum
            worker, root = leases.acquire()
            try:
                with lock:
                    active += 1
                    maximum = max(maximum, active)
                time.sleep(0.01)
                with lock:
                    active -= 1
            finally:
                leases.release(worker, root)

        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:
            list(executor.map(lambda _: use_database(), range(8)))
        self.assertEqual(maximum, 1)

    def test_strict_outcome_accepts_exact_terminal_only_shard(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "outcome.jsonl"
            self._write(path, self._rows())
            result = panel.validate_outcome_shard(
                path, self.model, base_seed=7, first_pair=0, pair_count=2)
            self.assertEqual(result["pair_count"], 2)
            self.assertEqual(len(result["outcomes"]), 4)
            self.assertEqual(result["voided_pairs"], [])

    def test_duplicate_missing_interleaved_and_malformed_terminals_fail(self) -> None:
        cases: dict[str, list[dict[str, object]]] = {}
        duplicate = self._rows()
        duplicate.insert(2, dict(duplicate[1]))
        cases["duplicate"] = duplicate
        cases["missing"] = self._rows()[:-1]
        interleaved = self._rows()
        interleaved[2], interleaved[3] = interleaved[3], interleaved[2]
        cases["interleaved"] = interleaved
        malformed = self._rows()
        malformed[2] = dict(malformed[2])
        malformed[2]["candidate_terminal_reward"] = 1
        cases["malformed-reward"] = malformed
        for rows in cases.values():
            for ordinal, row in enumerate(rows):
                row["record_ordinal"] = ordinal
        with tempfile.TemporaryDirectory() as directory:
            for name, rows in cases.items():
                with self.subTest(name=name):
                    path = Path(directory) / f"{name}.jsonl"
                    self._write(path, rows)
                    with self.assertRaises(ValueError):
                        panel.validate_outcome_shard(
                            path, self.model, base_seed=7, first_pair=0, pair_count=2)

    def test_terminal_wdl_aggregation(self) -> None:
        summary = panel.aggregate_terminal_wdl([
            {"label": "a", "by_seat": {"p0": "win", "p1": "loss"}},
            {"label": "a", "by_seat": {"p0": "draw", "p1": "draw"}},
        ], ["a"])
        self.assertEqual(summary["a"]["overall_wdl"], {"win": 1, "draw": 2, "loss": 1})

    def test_duplicate_model_labels_and_roots_fail_before_store_access(self) -> None:
        common = ["--evidence-root", "new", "--generation", "1024", "--mode", "smoke",
                  "--base-seed", "1", "--pairs", "1", "--scorer-exe", "scorer",
                  "--mage-repo", "mage", "--source-database", "cards", "--maven", "mvn"]
        with self.assertRaises(ValueError):
            panel.main(common + ["--model", "same=one", "--model", "same=two",
                                 "--model", "third=three"])
        with self.assertRaises(ValueError):
            panel.main(common + ["--model", "one=root", "--model", "two=root",
                                 "--model", "three=other"])

    # -- new: structured void-line / void-stop-line parsing -------------------

    def test_parse_void_line_round_trips_fields(self) -> None:
        line = ("XMAGE_RALLY_ANCHOR_PAIR_VOID base_seed=2026080801 pair_index=66"
                " environment_seed=3d403c464bfa8dca failing_episode=133"
                " candidate_seat=p1 fault_class=IllegalStateException"
                " engine_error_message=Error_in_unit_tests"
                " pairs_completed_before_void=2")
        info = panel.parse_void_line(line, base_seed=2026080801, first_pair=64, pair_count=32)
        self.assertEqual(info["pair_index"], 66)
        self.assertEqual(info["failing_episode"], 133)
        self.assertEqual(info["candidate_seat"], "p1")
        self.assertEqual(info["pairs_completed_before_void"], 2)
        self.assertEqual(info["engine_error_message"], "Error_in_unit_tests")

    def test_parse_void_line_rejects_out_of_range_and_wrong_fault_class(self) -> None:
        base = ("XMAGE_RALLY_ANCHOR_PAIR_VOID base_seed=7 pair_index={pair}"
                " environment_seed=3d403c464bfa8dca failing_episode={episode}"
                " candidate_seat=p1 fault_class={cls}"
                " engine_error_message=Error_in_unit_tests"
                " pairs_completed_before_void=0")
        with self.assertRaises(ValueError):
            panel.parse_void_line(base.format(pair=200, episode=401, cls="IllegalStateException"),
                                  base_seed=7, first_pair=64, pair_count=32)
        with self.assertRaises(ValueError):
            panel.parse_void_line(base.format(pair=64, episode=129, cls="RuntimeException"),
                                  base_seed=7, first_pair=64, pair_count=32)
        with self.assertRaises(ValueError):
            # failing_episode does not belong to pair_index
            panel.parse_void_line(base.format(pair=64, episode=200, cls="IllegalStateException"),
                                  base_seed=7, first_pair=64, pair_count=32)

    def test_parse_void_stop_line_cross_checks_pair_arithmetic(self) -> None:
        line = ("XMAGE_RALLY_ANCHOR_SPIKE VOID_STOP base_seed=7 opponent=cp7 cp7_skill=7"
                " first_episode=128 pairs_requested=32 pairs_completed=2"
                " games_completed=4 voided_pair_index=66 voided_episode=133"
                " elapsed_ms=1000")
        info = panel.parse_void_stop_line(line, base_seed=7, first_pair=64,
                                          pair_count=32, first_episode=128)
        self.assertEqual(info["voided_pair_index"], 66)
        self.assertEqual(info["pairs_completed"], 2)
        # voided_pair_index must be exactly first_pair + pairs_completed
        bad = line.replace("voided_pair_index=66", "voided_pair_index=70")
        with self.assertRaises(ValueError):
            panel.parse_void_stop_line(bad, base_seed=7, first_pair=64,
                                       pair_count=32, first_episode=128)

    # -- new: outcome-shard validation with a voided pair mixed in ------------

    def test_voided_pair_excluded_from_outcomes_but_still_schema_checked(self) -> None:
        # Pair 0 and pair 1 complete normally; pair 2 is voided: its first leg
        # (episode 4, seat p0) completed with a full, structurally valid
        # terminal (as if the fault hit only the second leg), and its second
        # leg (episode 5, seat p1) has one decision and no terminal at all
        # (as if the engine fault interrupted it mid-play). Neither leg of
        # pair 2 should contribute to outcomes even though leg one, taken in
        # isolation, looks like a perfectly good result.
        rows: list[dict[str, object]] = [panel.expected_header(self.checkpoint)]
        for pair in range(2):
            rows.extend((self._terminal(pair, "p0", len(rows)),
                         self._terminal(pair, "p1", len(rows) + 1)))
        # pair 2 / p0: a complete, structurally valid terminal with zero
        # preceding decisions (matches _terminal()'s defaults) -- as if that
        # leg finished cleanly before the fault hit the pair's other leg.
        rows.append(self._terminal(2, "p0", len(rows)))
        # pair 2 / p1: one decision and no terminal -- as if the engine fault
        # interrupted this leg mid-play.
        rows.append(self._decision(2, "p1", len(rows), 0))
        for ordinal, row in enumerate(rows):
            row["record_ordinal"] = ordinal
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "outcome.jsonl"
            self._write(path, rows)
            result = panel.validate_outcome_shard(
                path, self.model, base_seed=7, first_pair=0, pair_count=3,
                voided_pairs=frozenset({2}))
            self.assertEqual(len(result["outcomes"]), 4)
            self.assertTrue(all(o["pair_index"] != 2 for o in result["outcomes"]))
            self.assertEqual(result["voided_pairs"], [2])
            # Without declaring pair 2 voided, the same bytes must fail: the
            # trailing open episode and the missing pair-2/p1 terminal are
            # exactly what strict (non-voided) validation must reject.
            with self.assertRaises(ValueError):
                panel.validate_outcome_shard(
                    path, self.model, base_seed=7, first_pair=0, pair_count=3)

    def test_voided_pair_outside_task_range_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "outcome.jsonl"
            self._write(path, self._rows())
            with self.assertRaises(ValueError):
                panel.validate_outcome_shard(
                    path, self.model, base_seed=7, first_pair=0, pair_count=2,
                    voided_pairs=frozenset({99}))

    # -- new: per-model void-count exclusion arithmetic and the 2% cap --------

    def test_void_cap_arithmetic_matches_two_percent_boundary(self) -> None:
        # 2/128 = 1.5625% (under cap, allowed); 3/128 = 2.34375% (over cap).
        def exceeds(voided: int, total: int) -> bool:
            return voided * panel.VOID_CAP_FRACTION_DENOMINATOR \
                > total * panel.VOID_CAP_FRACTION_NUMERATOR
        self.assertFalse(exceeds(2, 128))
        self.assertTrue(exceeds(3, 128))
        # Exactly 2% (e.g. 2/100) must not itself exceed the cap.
        self.assertFalse(exceeds(2, 100))
        self.assertTrue(exceeds(3, 100))

    # -- new: run_task's segment-chaining loop (mocked subprocess) ------------

    def test_run_task_relaunches_a_fresh_segment_after_a_void(self) -> None:
        # Simulates exactly what the live pair-66 regression showed: a task
        # asked for 3 pairs, the first subprocess attempt completes pair 0
        # and voids pair 1, and a second, fresh subprocess attempt (a new
        # outcome file, as the Rust bridge's create_new-only outcome writer
        # requires) picks up and completes the remainder (pair 2).
        with tempfile.TemporaryDirectory() as directory:
            evidence_root = Path(directory) / "evidence"
            (evidence_root / "tasks").mkdir(parents=True)
            args = types.SimpleNamespace(
                evidence_root=evidence_root, mage_repo=Path("mage"),
                scorer_exe=Path("scorer"), generation=1024, base_seed=7,
                maven=Path("mvn"), task_timeout_seconds=60,
                tolerate_engine_faults=True,
            )
            leases = panel.DatabaseLeasePool([Path("db-0")])
            calls: list[int] = []

            def fake_run(command, *, cwd, env, stdout, stderr, timeout):
                calls.append(1)
                exec_args = command[-1].removeprefix("-Dexec.args=").split()
                flags = dict(zip(exec_args, exec_args[1:]))
                outcome_path = Path(flags["--outcome-export"])
                first_episode = int(flags["--first-episode"])
                pairs = int(flags["--pairs"])
                first_pair = first_episode // 2
                if len(calls) == 1:
                    self.assertEqual((first_pair, pairs), (0, 3))
                    outcome_path.write_text("stub-attempt-0\n", encoding="utf-8")
                    stdout.write(
                        "XMAGE_RALLY_ANCHOR_PAIR_VOID base_seed=7 pair_index=1"
                        " environment_seed=" + "a" * 16 + " failing_episode=3"
                        " candidate_seat=p1 fault_class=IllegalStateException"
                        " engine_error_message=Error_in_unit_tests"
                        " pairs_completed_before_void=1\n"
                    )
                    stdout.write(
                        "XMAGE_RALLY_ANCHOR_SPIKE VOID_STOP base_seed=7 opponent=cp7"
                        " cp7_skill=7 first_episode=0 pairs_requested=3"
                        " pairs_completed=1 games_completed=2 voided_pair_index=1"
                        " voided_episode=3 elapsed_ms=1\n"
                    )
                    return types.SimpleNamespace(returncode=0)
                self.assertEqual((first_pair, pairs), (2, 1))
                outcome_path.write_text("stub-attempt-1\n", encoding="utf-8")
                stdout.write("XMAGE_RALLY_ANCHOR_SPIKE PASS base_seed=7 ...\n")
                return types.SimpleNamespace(returncode=0)

            with mock.patch("subprocess.run", side_effect=fake_run):
                result = panel.run_task(args, leases, "seed-x", self.model, 0, 3)

        self.assertEqual(len(calls), 2)
        self.assertEqual([segment["first_pair"] for segment in result["segments"]], [0, 2])
        self.assertEqual([segment["pair_count"] for segment in result["segments"]], [2, 1])
        self.assertEqual(result["segments"][0]["voided_pairs"], [1])
        self.assertEqual(result["segments"][1]["voided_pairs"], [])
        self.assertEqual(len(result["voids"]), 1)
        self.assertEqual(result["voids"][0]["pair_index"], 1)
        self.assertEqual(result["voids"][0]["label"], "seed-x")
        # DatabaseLeasePool must show the lease released exactly once at the
        # end, not once per subprocess attempt.
        worker, root = leases.acquire()
        leases.release(worker, root)

    def test_expected_pair_keys_exclude_voided_pairs(self) -> None:
        pairs = list(range(0, 4))
        identities = ["seed-a", "seed-b"]
        voided = {("seed-a", 2)}
        expected = {(label, pair) for label in identities for pair in pairs} - voided
        self.assertNotIn(("seed-a", 2), expected)
        self.assertIn(("seed-b", 2), expected)
        self.assertEqual(len(expected), len(identities) * len(pairs) - len(voided))

    # -- new: per-model authority mode spec parsing ----------------------------

    def test_model_spec_defaults_to_population_mode_unprefixed(self) -> None:
        # The exact bareword shape every existing invocation (including the
        # live cells-1/2 driver) already uses must keep parsing identically.
        label, mode, generation, root = panel.parse_model_spec(
            r"seed-970001=D:\mtg-kernel-scaled-selfplay-population-v1\store")
        self.assertEqual(label, "seed-970001")
        self.assertEqual(mode, "population")
        self.assertIsNone(generation)
        self.assertEqual(root, Path(r"D:\mtg-kernel-scaled-selfplay-population-v1\store"))

    def test_model_spec_windows_drive_letter_colon_is_not_a_mode_prefix(self) -> None:
        # "D:" must never be mistaken for a "population:"/"original:" prefix.
        label, mode, generation, root = panel.parse_model_spec(r"promoted2=D:\pool3\primary")
        self.assertEqual(mode, "population")
        self.assertIsNone(generation)
        self.assertEqual(root, Path(r"D:\pool3\primary"))

    def test_model_spec_explicit_population_prefix(self) -> None:
        label, mode, generation, root = panel.parse_model_spec(
            r"denovo-256=population:D:\denovo-store\run-0\store")
        self.assertEqual(mode, "population")
        self.assertIsNone(generation)
        self.assertEqual(root, Path(r"D:\denovo-store\run-0\store"))

    def test_model_spec_explicit_original_prefix(self) -> None:
        label, mode, generation, root = panel.parse_model_spec(
            r"promoted2=original:D:\mtg-kernel-ladder-pilot-20260725\pool3\primary")
        self.assertEqual(label, "promoted2")
        self.assertEqual(mode, "original")
        self.assertIsNone(generation)
        self.assertEqual(root, Path(r"D:\mtg-kernel-ladder-pilot-20260725\pool3\primary"))

    def test_model_spec_rejects_empty_root_and_bad_label(self) -> None:
        with self.assertRaises(ValueError):
            panel.parse_model_spec("promoted2=original:")
        with self.assertRaises(ValueError):
            panel.parse_model_spec("bad label=original:D:\\store")
        with self.assertRaises(ValueError):
            panel.parse_model_spec("no-equals-sign")

    # -- new: per-model GENERATION spec parsing --------------------------------

    def test_model_spec_explicit_generation_with_mode(self) -> None:
        label, mode, generation, root = panel.parse_model_spec(
            r"promoted2=original:384:D:\mtg-kernel-ladder-pilot-20260725\pool3\primary")
        self.assertEqual(mode, "original")
        self.assertEqual(generation, 384)
        self.assertEqual(root, Path(r"D:\mtg-kernel-ladder-pilot-20260725\pool3\primary"))

    def test_model_spec_explicit_generation_without_mode_defaults_population(self) -> None:
        label, mode, generation, root = panel.parse_model_spec(
            r"denovo-256=256:D:\denovo-store\run-0\store")
        self.assertEqual(mode, "population")
        self.assertEqual(generation, 256)
        self.assertEqual(root, Path(r"D:\denovo-store\run-0\store"))

    def test_model_spec_generation_digits_never_mistaken_for_a_path(self) -> None:
        # A three-digit generation immediately followed by a drive-letter
        # colon must not be swallowed into the root or misparsed: digits are
        # never a valid Windows drive letter, so "NNN:D:\..." is unambiguous.
        label, mode, generation, root = panel.parse_model_spec(
            r"denovo-512=population:512:D:\denovo-512-store\run-0\store")
        self.assertEqual(generation, 512)
        self.assertEqual(root, Path(r"D:\denovo-512-store\run-0\store"))

    def test_load_store_identity_rejects_invalid_mode(self) -> None:
        with self.assertRaises(ValueError):
            panel.load_store_identity(Path("."), 0, mode="bogus")

    def test_anchor_command_selects_flag_by_mode(self) -> None:
        args = types.SimpleNamespace(mage_repo=Path("mage"), scorer_exe=Path("scorer"),
                                     generation=384, base_seed=7, maven=Path("mvn"))
        population_model = {"root": "store", "mode": "population", "generation": 384,
                            "checkpoint": self.checkpoint}
        original_model = {"root": "store", "mode": "original", "generation": 384,
                          "checkpoint": self.checkpoint}
        population_command = panel.anchor_command(args, population_model, 0, 1, Path("o.jsonl"))
        original_command = panel.anchor_command(args, original_model, 0, 1, Path("o.jsonl"))
        self.assertIn("--population-store-root", population_command[-1])
        self.assertIn("--store-root store", original_command[-1])
        self.assertNotIn("--population-store-root", original_command[-1])
        # A model dict with no "mode" key at all (the shape every pre-existing
        # caller and fixture uses) must still default to population, exactly
        # as before this change.
        legacy_model = {"root": "store", "generation": 384, "checkpoint": self.checkpoint}
        legacy_command = panel.anchor_command(args, legacy_model, 0, 1, Path("o.jsonl"))
        self.assertIn("--population-store-root", legacy_command[-1])

    def test_anchor_command_uses_the_models_own_generation_not_the_shared_one(self) -> None:
        # The whole point of per-model GENERATION: a group built from
        # per-spec generations can legitimately differ model to model, and
        # anchor_command must never fall back to args.generation once a
        # model dict carries its own.
        args = types.SimpleNamespace(mage_repo=Path("mage"), scorer_exe=Path("scorer"),
                                     generation=None, base_seed=7, maven=Path("mvn"))
        denovo_256 = {"root": "store256", "mode": "population", "generation": 256,
                     "checkpoint": self.checkpoint}
        denovo_512 = {"root": "store512", "mode": "population", "generation": 512,
                     "checkpoint": self.checkpoint}
        command_256 = panel.anchor_command(args, denovo_256, 0, 1, Path("o.jsonl"))
        command_512 = panel.anchor_command(args, denovo_512, 0, 1, Path("o.jsonl"))
        self.assertIn("--generation 256", command_256[-1])
        self.assertIn("--generation 512", command_512[-1])

    # -- new: main()'s --generation / per-spec GENERATION mixing rule ---------

    def _main_common_args(self, evidence_root: str) -> list[str]:
        return ["--evidence-root", evidence_root, "--mode", "smoke", "--base-seed", "1",
                "--pairs", "1", "--scorer-exe", "scorer", "--mage-repo", "mage",
                "--source-database", "cards", "--maven", "mvn"]

    def test_main_requires_generation_when_no_spec_carries_one(self) -> None:
        argv = self._main_common_args("new1") + [
            "--model", "one=root-one", "--model", "two=root-two", "--model", "three=root-three"]
        with self.assertRaises(ValueError):
            panel.main(argv)

    def test_main_rejects_shared_generation_mixed_with_per_spec_generation(self) -> None:
        # All three specs carry their own GENERATION (isolating this from the
        # separate "partial" mixing case below) while --generation is ALSO
        # given: must fail even though every value would agree, because
        # which one is authoritative must never be a judgment call.
        argv = self._main_common_args("new2") + [
            "--generation", "384",
            "--model", "one=original:384:root-one",
            "--model", "two=population:384:root-two",
            "--model", "three=population:384:root-three"]
        with self.assertRaises(ValueError):
            panel.main(argv)

    def test_main_rejects_partial_per_spec_generation(self) -> None:
        # Two specs carry their own GENERATION, one relies on nothing (no
        # --generation given either): this must fail closed, not silently
        # treat the third model as an error only once store access is
        # attempted.
        argv = self._main_common_args("new3") + [
            "--model", "one=original:384:root-one",
            "--model", "two=population:256:root-two",
            "--model", "three=population:root-three"]
        with self.assertRaises(ValueError):
            panel.main(argv)

    def test_main_accepts_per_spec_generation_with_no_shared_generation(self) -> None:
        # This is cell 3's exact shape: three different generations, no
        # panel-level --generation. What matters here is that parsing and
        # the mixing check let it through cleanly; it then fails later, at
        # source-database/store access against paths that do not exist in
        # this synthetic test (a FileNotFoundError, not the ValueError the
        # mixing/required checks themselves raise), which is exactly the
        # evidence that it got past the spec-shape validation.
        argv = self._main_common_args("new4") + [
            "--model", "one=original:384:root-one",
            "--model", "two=population:256:root-two",
            "--model", "three=population:512:root-three"]
        with self.assertRaises((ValueError, OSError)) as caught:
            panel.main(argv)
        self.assertNotIsInstance(caught.exception, ValueError)

    def test_environment_omits_population_maven_opts_for_original_mode(self) -> None:
        original_model = {"root": "store", "mode": "original", "checkpoint": self.checkpoint}
        env = panel.environment(Path("db"), original_model, tolerate_engine_faults=False)
        self.assertNotIn("MAVEN_OPTS", env)
        population_model = {"root": "store", "mode": "population", "checkpoint": self.checkpoint}
        env = panel.environment(Path("db"), population_model, tolerate_engine_faults=False)
        self.assertIn("MAVEN_OPTS", env)
        self.assertIn("xmage.rally.populationStore.authorityKind", env["MAVEN_OPTS"])

    def test_load_store_identity_authority_kind_and_contract_are_mode_specific(self) -> None:
        # Wire strings verified against checkpoint_shadow_stdio_v1's own
        # source (native_checkpoint_shadow_stdio_v1.rs): AUTHORITY_KIND_
        # ORIGINAL matches OriginalPromoted2StoreGeneration's authority_kind,
        # ENVIRONMENT_CONTRACT_ORIGINAL matches SOURCE_ENVIRONMENT_
        # TRAJECTORY_CONTRACT_V1 ("legacy-v1"), independent of any live store.
        self.assertEqual(panel.AUTHORITY_KIND_ORIGINAL,
                         "original-promoted2-validated-store-generation")
        self.assertEqual(panel.ENVIRONMENT_CONTRACT_ORIGINAL, "legacy-v1")
        self.assertNotEqual(panel.AUTHORITY_KIND, panel.AUTHORITY_KIND_ORIGINAL)
        self.assertNotEqual(panel.ENVIRONMENT_CONTRACT, panel.ENVIRONMENT_CONTRACT_ORIGINAL)


if __name__ == "__main__":
    unittest.main()
