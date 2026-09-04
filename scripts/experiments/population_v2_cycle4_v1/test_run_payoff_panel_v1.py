"""Focused tests for the cycle-4 payoff panel runner.

Every test here is synthetic: no game is ever executed and no Rust test
binary is ever built. Outcome documents are fabricated directly to exercise
`summarize_outcome`, panel/BT-input assembly, and dry-run rendering, per the
round-C contract's "rank-sum arithmetic from synthetic outcomes (no games)"
requirement.
"""

from __future__ import annotations

import contextlib
import io
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from run_payoff_panel_v1 import (
    DEFAULT_MATCHUP_WORKERS,
    MAX_MATCHUP_WORKERS,
    run_matchups_in_spec_order,
    MANIFEST_SCHEMA,
    OUTCOME_SCHEMA,
    SLOT_COUNT,
    SLOT_LOCATOR_SCHEMA,
    PanelRunnerError,
    build_bt_input_document,
    build_matchup_specs,
    build_panel_document,
    canonical_bytes,
    commit_staged_file,
    load_manifest,
    ARM_KINDS,
    H2H_ENVIRONMENT_KEYS,
    SlotLocation,
    load_slot_locator,
    matchup_environment,
    parse_slot_location,
    validate_slot_chain_dirs,
    panel_filename,
    parse_args,
    remove_stray,
    render_dry_run_lines,
    run,
    staged_temp_path,
    store_generation_for_slot,
    summarize_outcome,
)

ROLES = (
    "anchor-0",
    "anchor-1",
    "historical-0",
    "historical-1",
    "current-0",
    "current-1",
    "exploiter-0",
    "exploiter-1",
)


def hash_tag(tag: int) -> str:
    return f"cd{tag:062x}"


def synthetic_slot(index: int) -> dict:
    # No synthetic slot names the synthetic manifest's own trainee run, so
    # every one of these is an OTHER-run slot: `store_generation` equals the
    # label, exactly as `load_manifest` would derive it. The own-run
    # translation is exercised separately in `StoreGenerationTranslationTests`
    # and `ManifestAndSlotLocatorLoadingTests`.
    return {
        "slot_index": index,
        "role": ROLES[index],
        "occupant_class": "historical-fallback" if index >= 6 else "policy",
        "source_base_seed": 900_000 + index,
        "source_run_sha256": hash_tag(10 * index + 1),
        "source_generation": 384 + index,
        "store_generation": 384 + index,
        "checkpoint_manifest_sha256": hash_tag(10 * index + 2),
        "checkpoint_payload_sha256": hash_tag(10 * index + 3),
        "model_parameter_sha256": hash_tag(10 * index + 4),
        "train_state_sha256": hash_tag(10 * index + 5),
        "weight_units": 125_000,
    }


def synthetic_slots() -> list[dict]:
    return [synthetic_slot(index) for index in range(SLOT_COUNT)]


def plain_locator(prefix: str = "/stores/slot-") -> dict[int, SlotLocation]:
    """Every slot a bare store root, the control-r shape."""
    return {
        index: SlotLocation(Path(f"{prefix}{index}"), None) for index in range(SLOT_COUNT)
    }


def synthetic_manifest_document() -> dict:
    return {
        "schema": MANIFEST_SCHEMA,
        "prereg_sha256": hash_tag(1),
        "refresh_index": 0,
        "program_update": 0,
        "trainee_local_generation": 896,
        "trainee_run_sha256": hash_tag(2),
        "trainee_base_seed": 977_002,
        "weight_total_units": 1_000_000,
        "slots": synthetic_slots(),
    }


def outcome_for(
    spec,
    candidate: dict,
    opponent: dict,
    pair_ranks: list[int],
) -> dict:
    """Builds a well-formed synthetic outcome document for `spec`. Each pair
    plays the SAME rank on both seat-swapped legs (a simplification that
    keeps hand-computed win/draw/loss totals simple in tests); ranks are from
    the candidate's (lower slot's) perspective, `1`/`0`/`-1` = win/draw/loss."""
    assert len(pair_ranks) == spec.pair_count
    episodes = []
    wins = draws = losses = 0
    for pair_index, rank in enumerate(pair_ranks):
        seed = spec.evaluation_seed * 1000 + pair_index
        for seat in ("P0", "P1"):
            episodes.append(
                {
                    "episode_index": pair_index * 2 + (0 if seat == "P0" else 1),
                    "pair_index": pair_index,
                    "environment_seed": seed,
                    "learner_seat": seat,
                    "terminal_order_rank": rank,
                }
            )
            if rank == 1:
                wins += 1
            elif rank == -1:
                losses += 1
            else:
                draws += 1
    return {
        "schema": OUTCOME_SCHEMA,
        "evaluation_base_seed": spec.evaluation_seed,
        "pair_count": spec.pair_count,
        "episode_count": spec.pair_count * 2,
        "candidate": {
            "run_sha256": candidate["source_run_sha256"],
            "generation": candidate["store_generation"],
            "checkpoint_manifest_sha256": candidate["checkpoint_manifest_sha256"],
            "checkpoint_payload_sha256": candidate["checkpoint_payload_sha256"],
            "model_parameter_sha256": candidate["model_parameter_sha256"],
        },
        "opponent": {
            "run_sha256": opponent["source_run_sha256"],
            "generation": opponent["store_generation"],
            "checkpoint_manifest_sha256": opponent["checkpoint_manifest_sha256"],
            "checkpoint_payload_sha256": opponent["checkpoint_payload_sha256"],
            "model_parameter_sha256": opponent["model_parameter_sha256"],
        },
        "runtime": {"all_natural": True, "environment_randomization_v2": True},
        "learner_outcomes": {"overall": {"wins": wins, "losses": losses, "draws": draws}},
        "episodes": episodes,
    }


class MatchupSpecTests(unittest.TestCase):
    def test_produces_28_matchups_with_distinct_striding_seeds(self):
        specs = build_matchup_specs(base_seed=1_000, games_per_matchup=256)
        self.assertEqual(len(specs), 28)
        self.assertEqual({spec.pair_count for spec in specs}, {128})
        self.assertEqual({spec.game_count for spec in specs}, {256})
        seeds = [spec.evaluation_seed for spec in specs]
        self.assertEqual(len(set(seeds)), 28)
        # Lower-index-as-candidate convention, matching v1.
        first = specs[0]
        self.assertEqual((first.lower_slot, first.higher_slot), (0, 1))
        self.assertEqual(first.label, "matchup-0-1")

    def test_rejects_odd_or_nonpositive_games_per_matchup(self):
        with self.assertRaises(PanelRunnerError):
            build_matchup_specs(base_seed=1, games_per_matchup=255)
        with self.assertRaises(PanelRunnerError):
            build_matchup_specs(base_seed=1, games_per_matchup=0)
        with self.assertRaises(PanelRunnerError):
            build_matchup_specs(base_seed=1, games_per_matchup=-4)


class SummarizeOutcomeTests(unittest.TestCase):
    def setUp(self):
        self.slots = synthetic_slots()
        self.spec = build_matchup_specs(base_seed=5_000, games_per_matchup=8)[0]
        self.candidate = self.slots[self.spec.lower_slot]
        self.opponent = self.slots[self.spec.higher_slot]

    def test_accepts_well_formed_outcome_and_tabulates_wdl(self):
        # pair_count is 4 (games_per_matchup=8); two winning pairs, one draw,
        # one loss -> 4 wins, 2 draws, 2 losses at the game (leg) level.
        outcome = outcome_for(self.spec, self.candidate, self.opponent, [1, 1, 0, -1])
        summary = summarize_outcome(outcome, self.spec, self.candidate, self.opponent)
        self.assertEqual(summary["lower_wins"], 4)
        self.assertEqual(summary["lower_draws"], 2)
        self.assertEqual(summary["lower_losses"], 2)
        self.assertEqual(len(summary["pair_environment_seeds"]), 4)
        self.assertEqual(len(set(summary["pair_environment_seeds"])), 4)

    def test_rejects_schema_mismatch(self):
        outcome = outcome_for(self.spec, self.candidate, self.opponent, [0, 0, 0, 0])
        outcome["schema"] = "wrong/v1"
        with self.assertRaises(PanelRunnerError):
            summarize_outcome(outcome, self.spec, self.candidate, self.opponent)

    def test_rejects_candidate_identity_mismatch(self):
        outcome = outcome_for(self.spec, self.candidate, self.opponent, [0, 0, 0, 0])
        outcome["candidate"]["model_parameter_sha256"] = hash_tag(999)
        with self.assertRaises(PanelRunnerError):
            summarize_outcome(outcome, self.spec, self.candidate, self.opponent)

    def test_rejects_opponent_identity_mismatch(self):
        outcome = outcome_for(self.spec, self.candidate, self.opponent, [0, 0, 0, 0])
        outcome["opponent"]["generation"] += 1
        with self.assertRaises(PanelRunnerError):
            summarize_outcome(outcome, self.spec, self.candidate, self.opponent)

    def test_rejects_duplicate_pair_seed(self):
        outcome = outcome_for(self.spec, self.candidate, self.opponent, [0, 0, 0, 0])
        outcome["episodes"][2]["environment_seed"] = outcome["episodes"][0]["environment_seed"]
        with self.assertRaises(PanelRunnerError):
            summarize_outcome(outcome, self.spec, self.candidate, self.opponent)

    def test_rejects_seat_swap_mismatch(self):
        outcome = outcome_for(self.spec, self.candidate, self.opponent, [0, 0, 0, 0])
        outcome["episodes"][1]["learner_seat"] = "P0"  # both legs of pair 0 now P0
        with self.assertRaises(PanelRunnerError):
            summarize_outcome(outcome, self.spec, self.candidate, self.opponent)

    def test_rejects_nonterminal_rank(self):
        outcome = outcome_for(self.spec, self.candidate, self.opponent, [0, 0, 0, 0])
        outcome["episodes"][0]["terminal_order_rank"] = 2
        with self.assertRaises(PanelRunnerError):
            summarize_outcome(outcome, self.spec, self.candidate, self.opponent)

    def test_rejects_wdl_summary_mismatch(self):
        outcome = outcome_for(self.spec, self.candidate, self.opponent, [1, 1, 0, -1])
        outcome["learner_outcomes"]["overall"]["wins"] = 999
        with self.assertRaises(PanelRunnerError):
            summarize_outcome(outcome, self.spec, self.candidate, self.opponent)

    def test_rejects_runtime_flags_missing(self):
        outcome = outcome_for(self.spec, self.candidate, self.opponent, [0, 0, 0, 0])
        outcome["runtime"]["all_natural"] = False
        with self.assertRaises(PanelRunnerError):
            summarize_outcome(outcome, self.spec, self.candidate, self.opponent)


class PanelAndBtInputAssemblyTests(unittest.TestCase):
    """Rank-sum arithmetic and BT-input emission, entirely from synthetic
    per-matchup summaries -- no games, no outcome files."""

    def setUp(self):
        self.slots = synthetic_slots()
        # G=8 games/matchup keeps the 28-matchup arithmetic easy to check by
        # hand while still exercising the full round robin.
        self.specs = build_matchup_specs(base_seed=42, games_per_matchup=8)

    def _summaries_mixed(self) -> tuple[list[dict], list[str]]:
        # Every matchup follows the SAME pair-rank pattern from the lower
        # slot's perspective: two winning pairs, one losing pair, one drawn
        # pair (lower_wins=4, lower_draws=2, lower_losses=2 at the game/leg
        # level; higher gets the mirror image). Deliberately NOT a sweep:
        # every slot, whether it plays as lower or higher in a given
        # matchup, picks up both wins and losses there, so no slot's total
        # record across its seven matchups is ever winless or lossless --
        # required for `fit_bt_ratings` below (a zero-score or
        # zero-counter-score record is a degenerate, infinite-strength BT
        # fit and is correctly rejected).
        summaries = []
        outcome_hashes = []
        for spec in self.specs:
            outcome = outcome_for(
                spec, self.slots[spec.lower_slot], self.slots[spec.higher_slot], [1, 1, -1, 0]
            )
            summaries.append(
                summarize_outcome(
                    outcome, spec, self.slots[spec.lower_slot], self.slots[spec.higher_slot]
                )
            )
            outcome_hashes.append("ab" * 32)
        return summaries, outcome_hashes

    def test_panel_document_both_orderings_and_rank_sums(self):
        summaries, outcome_hashes = self._summaries_mixed()
        panel = build_panel_document(
            manifest_sha256="ff" * 32,
            base_seed=42,
            games_per_matchup=8,
            slots=self.slots,
            specs=self.specs,
            summaries=summaries,
            outcome_hashes=outcome_hashes,
        )
        self.assertEqual(panel["schema"], "mtg-kernel-cycle4-payoff-panel/v1")
        self.assertEqual(len(panel["matchups"]), 28)
        row = panel["matchups"][0]
        self.assertEqual((row["lower_slot_index"], row["higher_slot_index"]), (0, 1))
        # Both orderings stated explicitly and mutually consistent.
        self.assertEqual((row["lower_wins"], row["lower_draws"], row["lower_losses"]), (4, 2, 2))
        self.assertEqual((row["higher_wins"], row["higher_draws"], row["higher_losses"]), (2, 2, 4))
        # Every matchup nets the lower slot +2 (4 wins - 2 losses) and the
        # higher slot -2. Slot k plays as lower against the (7-k) slots
        # above it and as higher against the k slots below it, so
        # u_k = (7-k)*2 - k*2 = 14 - 4k.
        rank_sums = {entry["slot_index"]: entry["u_i"] for entry in panel["rank_sums"]}
        for index in range(SLOT_COUNT):
            self.assertEqual(rank_sums[index], 14 - 4 * index)
        # Zero-sum across the whole panel: every game's rank is claimed by
        # exactly one winner and negated for the loser (draws contribute 0),
        # so the eight u_i values sum to zero.
        self.assertEqual(sum(rank_sums.values()), 0)

    def test_bt_input_document_matches_panel_and_pins_anchor_zero(self):
        summaries, outcome_hashes = self._summaries_mixed()
        panel = build_panel_document(
            manifest_sha256="ff" * 32,
            base_seed=42,
            games_per_matchup=8,
            slots=self.slots,
            specs=self.specs,
            summaries=summaries,
            outcome_hashes=outcome_hashes,
        )
        bt_input = build_bt_input_document(self.slots, panel)
        self.assertEqual(bt_input["schema"], "mtg-kernel-bt-rating-input/v1")
        self.assertEqual(bt_input["reference_id"], self.slots[0]["model_parameter_sha256"])
        self.assertEqual(len(bt_input["pairs"]), 28)
        first_pair = bt_input["pairs"][0]
        self.assertEqual(first_pair["a_id"], self.slots[0]["model_parameter_sha256"])
        self.assertEqual(first_pair["b_id"], self.slots[1]["model_parameter_sha256"])
        self.assertEqual((first_pair["a_wins"], first_pair["b_wins"], first_pair["draws"]), (4, 2, 2))
        # Feedable straight into bt_rating_v1.fit_bt_ratings without adaptation.
        try:
            import bt_rating_v1
        except ImportError:
            self.skipTest("bt_rating_v1 not importable in this environment")
        result = bt_rating_v1.fit_bt_ratings(bt_input)
        self.assertEqual(result["reference_id"], bt_input["reference_id"])
        self.assertAlmostEqual(result["ratings_log_units"][bt_input["reference_id"]], 0.0)
        # Slot 0 dominates every matchup it plays as lower and never plays
        # as higher (it is the smallest index), so it must rate strictly
        # above every other identity.
        ratings = result["ratings_log_units"]
        other_ids = [slot["model_parameter_sha256"] for slot in self.slots[1:]]
        self.assertTrue(all(ratings[bt_input["reference_id"]] >= ratings[other] for other in other_ids))


class DryRunDeterminismTests(unittest.TestCase):
    def test_repeated_rendering_is_byte_identical(self):
        slots = synthetic_slots()
        specs = build_matchup_specs(base_seed=7, games_per_matchup=16)
        locator = plain_locator()
        executable = Path("/build/mtg_kernel-abc123.exe")
        output_dir = Path("/evidence/panel-run")
        first = render_dry_run_lines(specs, slots, locator, executable, output_dir)
        second = render_dry_run_lines(specs, slots, locator, executable, output_dir)
        self.assertEqual(first, second)
        self.assertEqual(len(first), 28)
        # The rendered lines are the "exact per-matchup commands and seeds":
        # every line names its evaluation seed and the ignored test name.
        for line in first:
            self.assertIn("H2H_EVAL_SEED=", line)
            self.assertIn("ladder_head_to_head_eval_v1", line)
            self.assertIn("H2H_ENVIRONMENT_RANDOMIZATION_V2=1", line)

    def test_different_base_seed_changes_the_rendering(self):
        slots = synthetic_slots()
        locator = plain_locator()
        executable = Path("/build/mtg_kernel-abc123.exe")
        output_dir = Path("/evidence/panel-run")
        first = render_dry_run_lines(
            build_matchup_specs(base_seed=7, games_per_matchup=16),
            slots,
            locator,
            executable,
            output_dir,
        )
        second = render_dry_run_lines(
            build_matchup_specs(base_seed=8, games_per_matchup=16),
            slots,
            locator,
            executable,
            output_dir,
        )
        self.assertNotEqual(first, second)


class StoreGenerationTranslationTests(unittest.TestCase):
    """The 896-offset translation, mirroring the launcher's
    `store_generation_for_slot_v1`."""

    TRAINEE_RUN = hash_tag(2)

    def own_run_slot(self, generation: int) -> dict:
        slot = synthetic_slot(5)
        slot["source_run_sha256"] = self.TRAINEE_RUN
        slot["source_generation"] = generation
        return slot

    def test_own_run_label_translates_by_the_896_offset(self):
        for update in (0, 128, 512, 2048):
            slot = self.own_run_slot(896 + update)
            self.assertEqual(
                store_generation_for_slot(slot, self.TRAINEE_RUN),
                update,
                "an own-run slot loads at label - 896",
            )

    def test_own_run_label_below_the_program_start_is_rejected(self):
        for generation in (0, 1, 895):
            slot = self.own_run_slot(generation)
            with self.assertRaises(PanelRunnerError):
                store_generation_for_slot(slot, self.TRAINEE_RUN)

    def test_other_run_slots_keep_their_labels(self):
        slot = synthetic_slot(0)
        self.assertNotEqual(slot["source_run_sha256"], self.TRAINEE_RUN)
        self.assertEqual(
            store_generation_for_slot(slot, self.TRAINEE_RUN),
            slot["source_generation"],
        )
        # Including labels far below 896, which are ordinary generations in
        # the runs that own them.
        slot["source_generation"] = 384
        self.assertEqual(store_generation_for_slot(slot, self.TRAINEE_RUN), 384)

    def test_matchup_environment_passes_the_translated_generation(self):
        slots = synthetic_slots()
        slots[5] = self.own_run_slot(896 + 128)
        slots[5]["store_generation"] = store_generation_for_slot(slots[5], self.TRAINEE_RUN)
        locator = plain_locator("D:/cycle4/slot-")
        # Pick the matchup that actually names slot 5.
        spec = next(
            candidate
            for candidate in build_matchup_specs(1000, 4)
            if candidate.higher_slot == 5
        )
        environment = matchup_environment(slots, locator, spec, Path("D:/out/outcome.json"))
        self.assertEqual(environment["H2H_OPPONENT_GEN"], "128")


class ManifestAndSlotLocatorLoadingTests(unittest.TestCase):
    def test_load_manifest_translates_own_run_slot_generations(self):
        # The manifest label stays trainee-local; the loaded slot carries the
        # Store generation the arm's own Store actually holds.
        document = synthetic_manifest_document()
        trainee_run = document["trainee_run_sha256"]
        document["slots"][5]["source_run_sha256"] = trainee_run
        document["slots"][5]["source_generation"] = 896 + 128
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest.json"
            path.write_bytes(canonical_bytes(document))
            _, _, _, slots = load_manifest(path)
        self.assertEqual(slots[5]["source_generation"], 896 + 128)
        self.assertEqual(slots[5]["store_generation"], 128)
        # Other-run slots are untouched.
        self.assertEqual(slots[0]["store_generation"], slots[0]["source_generation"])

    def test_load_manifest_rejects_own_run_label_below_the_program_start(self):
        document = synthetic_manifest_document()
        document["slots"][5]["source_run_sha256"] = document["trainee_run_sha256"]
        document["slots"][5]["source_generation"] = 895
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest.json"
            path.write_bytes(canonical_bytes(document))
            with self.assertRaises(PanelRunnerError):
                load_manifest(path)

    def test_load_manifest_accepts_well_formed_document(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest.json"
            path.write_bytes(canonical_bytes(synthetic_manifest_document()))
            raw, manifest_sha256, refresh_index, slots = load_manifest(path)
            self.assertEqual(len(slots), SLOT_COUNT)
            self.assertEqual(slots[0]["role"], "anchor-0")
            self.assertEqual(len(manifest_sha256), 64)
            self.assertEqual(refresh_index, 0)
            self.assertEqual(raw, path.read_bytes())

    def test_load_manifest_rejects_non_integer_refresh_index(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest.json"
            document = synthetic_manifest_document()
            document["refresh_index"] = "zero"
            path.write_bytes(canonical_bytes(document))
            with self.assertRaises(PanelRunnerError):
                load_manifest(path)

    def test_load_manifest_rejects_wrong_schema(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest.json"
            document = synthetic_manifest_document()
            document["schema"] = "wrong/v1"
            path.write_bytes(canonical_bytes(document))
            with self.assertRaises(PanelRunnerError):
                load_manifest(path)

    def test_load_manifest_rejects_role_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest.json"
            document = synthetic_manifest_document()
            document["slots"][0]["role"] = "anchor-1"
            path.write_bytes(canonical_bytes(document))
            with self.assertRaises(PanelRunnerError):
                load_manifest(path)

    def test_load_manifest_rejects_missing_slot(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest.json"
            document = synthetic_manifest_document()
            document["slots"] = document["slots"][:7]
            path.write_bytes(canonical_bytes(document))
            with self.assertRaises(PanelRunnerError):
                load_manifest(path)

    def test_load_slot_locator_accepts_well_formed_document(self):
        # An absolute store root, in whatever form this platform's pathlib
        # considers absolute (a drive-letter path on Windows, "/..." on
        # POSIX) -- built from a real temp directory rather than a hardcoded
        # literal so the test is not itself platform-specific.
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            self.assertTrue(base.is_absolute())
            path = base / "locator.json"
            document = {
                "schema": SLOT_LOCATOR_SCHEMA,
                "stores": {
                    str(index): str(base / f"slot-{index}") for index in range(SLOT_COUNT)
                },
            }
            path.write_text(json.dumps(document), encoding="utf-8")
            locator = load_slot_locator(path)
            self.assertEqual(len(locator), SLOT_COUNT)
            self.assertEqual(locator[0].store_root, base / "slot-0")
            self.assertIsNone(locator[0].baseline_chain_dir)

    def test_load_slot_locator_rejects_relative_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "locator.json"
            document = {
                "schema": SLOT_LOCATOR_SCHEMA,
                "stores": {str(index): f"relative/slot-{index}" for index in range(SLOT_COUNT)},
            }
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaises(PanelRunnerError):
                load_slot_locator(path)

    def test_load_slot_locator_rejects_missing_slot(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "locator.json"
            document = {
                "schema": SLOT_LOCATOR_SCHEMA,
                "stores": {str(index): f"/mnt/stores/slot-{index}" for index in range(SLOT_COUNT - 1)},
            }
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaises(PanelRunnerError):
                load_slot_locator(path)


class PanelFilenameTests(unittest.TestCase):
    def test_panel_filename_matches_the_fixed_naming_scheme(self):
        # Matches the Rust chain builder's `cycle4_chain_panel_filename_v1`
        # exactly -- see `native_population_refresh_builder_cycle4_v1.rs`.
        self.assertEqual(panel_filename(1), "refresh-01.panel.json")
        self.assertEqual(panel_filename(16), "refresh-16.panel.json")


# Absolute in the PLATFORM's sense: a POSIX-rooted path is not absolute
# on Windows, where these tests also run, and the locator demands
# absolute paths.
ABS_SLOT_0 = str(Path(tempfile.gettempdir()).resolve() / "cycle4-slot-0")
ABS_SLOT_5 = str(Path(tempfile.gettempdir()).resolve() / "cycle4-slot-5")
ABS_CHAIN = str(Path(tempfile.gettempdir()).resolve() / "cycle4-chain")


class SlotLocatorChainDirTests(unittest.TestCase):
    """The additive per-slot `baseline_chain_dir`, and the arm-kind rule that
    decides which slots must carry it (review finding P1)."""

    TRAINEE_RUN = hash_tag(2)

    def test_bare_string_entry_stays_a_chainless_store_root(self):
        location = parse_slot_location(0, ABS_SLOT_0)
        self.assertEqual(location.store_root, Path(ABS_SLOT_0))
        self.assertIsNone(location.baseline_chain_dir)

    def test_object_entry_carries_both_absolute_paths(self):
        location = parse_slot_location(
            5,
            {"store_root": ABS_SLOT_5, "baseline_chain_dir": ABS_CHAIN},
        )
        self.assertEqual(location.store_root, Path(ABS_SLOT_5))
        self.assertEqual(location.baseline_chain_dir, Path(ABS_CHAIN))

    def test_object_entry_rejects_unknown_keys(self):
        with self.assertRaises(PanelRunnerError):
            parse_slot_location(
                5,
                {
                    "store_root": ABS_SLOT_5,
                    "baseline_chain_dir": ABS_CHAIN,
                    "chain_dir": "/chains/typo",
                },
            )

    def test_object_entry_requires_baseline_chain_dir(self):
        # The wrapper writes the object form only for a slot that needs the
        # directory, so an object without one is a wrapper bug.
        with self.assertRaises(PanelRunnerError):
            parse_slot_location(5, {"store_root": ABS_SLOT_5})

    def test_entry_rejects_relative_and_malformed_paths(self):
        for value in (
            "stores/slot-5",
            {"store_root": "stores/slot-5", "baseline_chain_dir": ABS_CHAIN},
            {"store_root": ABS_SLOT_5, "baseline_chain_dir": "chains/arm-a"},
            {"store_root": ABS_SLOT_5, "baseline_chain_dir": ""},
            {"store_root": 5, "baseline_chain_dir": ABS_CHAIN},
            5,
            None,
            [],
        ):
            with self.assertRaises(PanelRunnerError):
                parse_slot_location(5, value)

    def test_load_slot_locator_accepts_a_mixed_document(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp).resolve()
            path = base / "locator.json"
            stores: dict[str, object] = {
                str(index): str(base / f"slot-{index}") for index in range(SLOT_COUNT)
            }
            stores["5"] = {
                "store_root": str(base / "slot-5"),
                "baseline_chain_dir": str(base / "chain"),
            }
            path.write_text(
                json.dumps({"schema": SLOT_LOCATOR_SCHEMA, "stores": stores}),
                encoding="utf-8",
            )
            locator = load_slot_locator(path)
            self.assertIsNone(locator[0].baseline_chain_dir)
            self.assertEqual(locator[5].store_root, base / "slot-5")
            self.assertEqual(locator[5].baseline_chain_dir, base / "chain")

    def own_run_slots(self) -> list[dict]:
        slots = synthetic_slots()
        slots[5]["source_run_sha256"] = self.TRAINEE_RUN
        return slots

    def v4_locator(self) -> dict[int, SlotLocation]:
        locator = plain_locator()
        locator[5] = SlotLocation(Path(ABS_SLOT_5), Path(ABS_CHAIN))
        return locator

    def test_v4_arms_require_a_chain_dir_on_every_own_run_slot(self):
        slots = self.own_run_slots()
        for arm in ("static-rb", "treatment-rb"):
            validate_slot_chain_dirs(arm, slots, self.v4_locator(), self.TRAINEE_RUN)
            # Missing on the own-run slot: the probe would take the plain
            # walk and panic on trained v4 evidence, so reject it here.
            with self.assertRaises(PanelRunnerError):
                validate_slot_chain_dirs(arm, slots, plain_locator(), self.TRAINEE_RUN)

    def test_control_r_and_other_run_slots_must_not_carry_a_chain_dir(self):
        slots = self.own_run_slots()
        # control-r is a v3 arm: even its own-run slot takes the plain walk.
        validate_slot_chain_dirs("control-r", slots, plain_locator(), self.TRAINEE_RUN)
        with self.assertRaises(PanelRunnerError):
            validate_slot_chain_dirs("control-r", slots, self.v4_locator(), self.TRAINEE_RUN)
        # An other-run slot never carries one, whatever the arm.
        stray = plain_locator()
        stray[0] = SlotLocation(Path(ABS_SLOT_0), Path(ABS_CHAIN))
        for arm in ARM_KINDS:
            with self.assertRaises(PanelRunnerError):
                validate_slot_chain_dirs(arm, slots, stray, self.TRAINEE_RUN)

    def test_unknown_arm_kind_is_rejected(self):
        with self.assertRaises(PanelRunnerError):
            validate_slot_chain_dirs(
                "treatment", self.own_run_slots(), plain_locator(), self.TRAINEE_RUN
            )

    def test_matchup_environment_passes_each_side_its_own_chain_dir(self):
        slots = self.own_run_slots()
        slots[5]["store_generation"] = 128
        locator = self.v4_locator()
        spec = next(
            candidate
            for candidate in build_matchup_specs(1000, 4)
            if candidate.higher_slot == 5
        )
        environment = matchup_environment(slots, locator, spec, Path("D:/out/outcome.json"))
        self.assertEqual(environment["H2H_OPPONENT_CHAIN_DIR"], str(Path(ABS_CHAIN)))
        self.assertNotIn("H2H_CANDIDATE_CHAIN_DIR", environment)
        # And the reverse seat when the own-run slot is the lower one.
        spec = next(
            candidate
            for candidate in build_matchup_specs(1000, 4)
            if candidate.lower_slot == 5
        )
        environment = matchup_environment(slots, locator, spec, Path("D:/out/outcome.json"))
        self.assertEqual(environment["H2H_CANDIDATE_CHAIN_DIR"], str(Path(ABS_CHAIN)))
        self.assertNotIn("H2H_OPPONENT_CHAIN_DIR", environment)


class EnvironmentScrubTests(unittest.TestCase):
    """Every `H2H_*` name this runner ever sets must also be scrubbed from the
    inherited environment before a matchup launches (review finding P2)."""

    def test_every_name_matchup_environment_sets_is_scrubbed(self):
        # The scrub list and the setter cannot be allowed to drift: a name the
        # setter emits only conditionally is exactly the one an inherited
        # value leaks through.
        slots = synthetic_slots()
        slots[5]["source_run_sha256"] = hash_tag(2)
        slots[5]["store_generation"] = 128
        locator = plain_locator()
        locator[5] = SlotLocation(Path(ABS_SLOT_5), Path(ABS_CHAIN))
        emitted: set[str] = set()
        for spec in build_matchup_specs(1000, 4):
            emitted.update(
                matchup_environment(slots, locator, spec, Path("D:/out/outcome.json"))
            )
        self.assertIn("H2H_CANDIDATE_CHAIN_DIR", emitted)
        self.assertIn("H2H_OPPONENT_CHAIN_DIR", emitted)
        self.assertEqual(
            emitted - set(H2H_ENVIRONMENT_KEYS),
            set(),
            "every name the runner sets must be scrubbed from the parent environment",
        )

    def test_a_polluted_parent_environment_does_not_leak_a_chain_dir(self):
        # A side whose locator carries no chain directory must reach the probe
        # with none: an inherited value would make the probe's pairing gate
        # reject a matchup for a directory this runner never asked for.
        environment = dict(os.environ)
        environment["H2H_CANDIDATE_CHAIN_DIR"] = "D:/inherited/candidate-chain"
        environment["H2H_OPPONENT_CHAIN_DIR"] = "D:/inherited/opponent-chain"
        environment["H2H_CANDIDATE_GEN"] = "999999"
        for key in H2H_ENVIRONMENT_KEYS:
            environment.pop(key, None)
        slots = synthetic_slots()
        spec = build_matchup_specs(1000, 4)[0]
        environment.update(
            matchup_environment(slots, plain_locator(), spec, Path("D:/out/outcome.json"))
        )
        self.assertNotIn("H2H_CANDIDATE_CHAIN_DIR", environment)
        self.assertNotIn("H2H_OPPONENT_CHAIN_DIR", environment)
        self.assertEqual(environment["H2H_CANDIDATE_GEN"], str(slots[0]["store_generation"]))

    def test_a_polluted_parent_does_not_survive_beside_a_real_chain_dir(self):
        # The own-run side keeps ITS chain directory; the other side keeps
        # none, rather than inheriting the stale value.
        environment = dict(os.environ)
        environment["H2H_CANDIDATE_CHAIN_DIR"] = "D:/inherited/candidate-chain"
        environment["H2H_OPPONENT_CHAIN_DIR"] = "D:/inherited/opponent-chain"
        for key in H2H_ENVIRONMENT_KEYS:
            environment.pop(key, None)
        slots = synthetic_slots()
        slots[5]["store_generation"] = 128
        locator = plain_locator()
        locator[5] = SlotLocation(Path(ABS_SLOT_5), Path(ABS_CHAIN))
        spec = next(
            candidate
            for candidate in build_matchup_specs(1000, 4)
            if candidate.higher_slot == 5
        )
        environment.update(matchup_environment(slots, locator, spec, Path("D:/out/o.json")))
        self.assertEqual(environment["H2H_OPPONENT_CHAIN_DIR"], str(Path(ABS_CHAIN)))
        self.assertNotIn("H2H_CANDIDATE_CHAIN_DIR", environment)


class WrapperEmittedLocatorTests(unittest.TestCase):
    """The reader against a locator the WRAPPER actually wrote, not one this
    file hand-builds.

    The two sides agree on the additive object form only if they are checked
    against each other: the wrapper's own dry-run suite builds a synthetic
    treatment-rb campaign and emits `panel-slot-locator.json` from its real
    writer, and this loads that file through the real reader. A hand-built
    fixture would keep passing after either side drifted."""

    @staticmethod
    def _powershell() -> str | None:
        return shutil.which("powershell.exe") or shutil.which("pwsh")

    def test_the_reader_accepts_the_wrappers_own_treatment_rb_locator(self):
        powershell = self._powershell()
        if powershell is None:
            self.skipTest("powershell.exe is unavailable on this host")
        suite = (
            Path(__file__).resolve().parent / "run-cycle4-arm-tests.ps1"
        )
        if not suite.is_file():
            self.skipTest(f"{suite.name} is not present")
        with tempfile.TemporaryDirectory() as tmp:
            work_root = Path(tmp).resolve()
            completed = subprocess.run(
                [
                    powershell,
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(suite),
                    "-WorkRoot",
                    str(work_root),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            locators = sorted(work_root.rglob("panel-slot-locator.json"))
            if not locators:
                self.skipTest(
                    "the wrapper dry-run suite emitted no panel locator "
                    f"(exit {completed.returncode}): {completed.stdout[-2000:]}"
                )
            # Every emitted locator must load, and at least one must carry the
            # object form on the treatment-rb arm's own-run slot.
            with_chain_dir = []
            for path in locators:
                locator = load_slot_locator(path)
                self.assertEqual(len(locator), SLOT_COUNT)
                for index in range(SLOT_COUNT):
                    self.assertTrue(locator[index].store_root.is_absolute())
                carried = [
                    index
                    for index in range(SLOT_COUNT)
                    if locator[index].baseline_chain_dir is not None
                ]
                if carried:
                    with_chain_dir.append((path, locator, carried))
            self.assertTrue(
                with_chain_dir,
                "the wrapper must write baseline_chain_dir on a treatment-rb "
                f"own-run slot; loaded {len(locators)} locator(s) without one",
            )
            for path, locator, carried in with_chain_dir:
                for index in carried:
                    chain_dir = locator[index].baseline_chain_dir
                    self.assertIsNotNone(chain_dir)
                    self.assertTrue(
                        chain_dir.is_absolute(),
                        f"{path}: slot {index} baseline_chain_dir must be absolute",
                    )


def _base_cli_args(tmp: str, **overrides: str) -> list[str]:
    values = {
        "--manifest": str(Path(tmp) / "manifest.json"),
        "--slot-locator": str(Path(tmp) / "locator.json"),
        "--arm": "control-r",
        "--base-seed": "1",
        "--output-dir": str(Path(tmp) / "out"),
        "--executable": str(Path(tmp) / "fake-exe"),
        "--repo-root": tmp,
    }
    values.update(overrides)
    argv: list[str] = []
    for flag, value in values.items():
        argv.extend([flag, value])
    return argv


class GamesPerMatchupValidationTests(unittest.TestCase):
    """P1-4: a non-canonical --games-per-matchup outside --dry-run is a hard
    usage error (exit 2), never a warning."""

    def test_non_canonical_games_per_matchup_outside_dry_run_is_a_hard_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            argv = _base_cli_args(tmp) + ["--games-per-matchup", "128"]
            with self.assertRaises(SystemExit) as ctx:
                parse_args(argv)
            self.assertEqual(ctx.exception.code, 2)

    def test_non_canonical_games_per_matchup_is_allowed_under_dry_run(self):
        with tempfile.TemporaryDirectory() as tmp:
            argv = _base_cli_args(tmp) + ["--games-per-matchup", "128", "--dry-run"]
            args = parse_args(argv)
            self.assertEqual(args.games_per_matchup, 128)

    def test_canonical_games_per_matchup_outside_dry_run_is_accepted(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = parse_args(_base_cli_args(tmp))
            self.assertEqual(args.games_per_matchup, 256)


class AtomicCommitTests(unittest.TestCase):
    """P2-7: panel.json and bt-rating-input.json are staged to temporary
    names and committed by rename; a failed run must never leave a
    consumable panel document."""

    def test_commit_staged_file_renames_into_place(self):
        with tempfile.TemporaryDirectory() as tmp:
            final_path = Path(tmp) / "refresh-01.panel.json"
            temp_path = staged_temp_path(final_path)
            temp_path.write_bytes(b"staged-bytes")
            commit_staged_file(temp_path, final_path)
            self.assertFalse(temp_path.exists())
            self.assertEqual(final_path.read_bytes(), b"staged-bytes")

    def test_remove_stray_is_a_noop_for_a_missing_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            missing = Path(tmp) / "nope.tmp"
            remove_stray(missing)  # must not raise

    def test_remove_stray_deletes_an_existing_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            present = Path(tmp) / "stray.tmp"
            present.write_bytes(b"x")
            remove_stray(present)
            self.assertFalse(present.exists())


class OutputDirResolutionTests(unittest.TestCase):
    """P2-5: --output-dir (and every derived matchup path) is resolved to an
    absolute path before H2H_OUTCOME_JSON is constructed, since matchup
    subprocesses run with cwd=repo_root, not cwd=output_dir."""

    def test_dry_run_resolves_a_relative_output_dir_to_absolute(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp).resolve()
            (tmp_path / "manifest.json").write_bytes(
                canonical_bytes(synthetic_manifest_document())
            )
            (tmp_path / "locator.json").write_text(
                json.dumps(
                    {
                        "schema": SLOT_LOCATOR_SCHEMA,
                        "stores": {
                            str(index): str(tmp_path / f"slot-{index}")
                            for index in range(SLOT_COUNT)
                        },
                    }
                ),
                encoding="utf-8",
            )
            original_cwd = Path.cwd()
            os.chdir(tmp_path)
            try:
                args = parse_args(
                    [
                        "--manifest",
                        "manifest.json",
                        "--slot-locator",
                        "locator.json",
                        "--arm",
                        "control-r",
                        "--base-seed",
                        "1",
                        "--output-dir",
                        "relative-output",
                        "--executable",
                        "fake-exe",
                        "--repo-root",
                        ".",
                        "--dry-run",
                    ]
                )
                buffer = io.StringIO()
                with contextlib.redirect_stdout(buffer):
                    result = run(args)
            finally:
                os.chdir(original_cwd)
            self.assertIsNone(result)
            expected_absolute_fragment = str(tmp_path / "relative-output")
            self.assertIn(expected_absolute_fragment, buffer.getvalue())
            self.assertNotIn("H2H_OUTCOME_JSON=relative-output", buffer.getvalue())


def _pool(specs, workers, run_one, consume, launch=None, validate=None, **extra):
    """Adapter for run_one-style fakes: by default the handle is the spec
    itself, `run_one` plays finish, validation is the identity, and
    `consume` plays accept. Extra keywords reach the helper unchanged."""
    one_argument_launch = launch or (lambda spec: spec)

    def launch_one(spec, register):
        handle = one_argument_launch(spec)
        register(handle)
        return handle

    return run_matchups_in_spec_order(
        specs,
        workers,
        launch_one=launch_one,
        finish_one=run_one,
        validate=validate or (lambda spec, result: result),
        accept=consume,
        **extra,
    )


class _FakeEngineProcess:
    """A stand-in for `subprocess.Popen` whose first wait raises the given
    exception and which records kill and wait calls."""

    def __init__(self, wait_error: BaseException | None = None, kill_error: BaseException | None = None):
        self.wait_error = wait_error
        self.kill_error = kill_error
        self.wait_calls = 0
        self.killed = False

    def wait(self):
        self.wait_calls += 1
        if self.wait_calls == 1 and self.wait_error is not None:
            raise self.wait_error
        return 0

    def kill(self):
        self.killed = True
        if self.kill_error is not None:
            raise self.kill_error


class _FakeLog:
    def __init__(self, close_error: BaseException | None = None):
        self.closed = False
        self.close_error = close_error

    def close(self):
        self.closed = True
        if self.close_error is not None:
            raise self.close_error


def _fake_handle(process, stdout=None, stderr=None):
    from run_payoff_panel_v1 import build_matchup_specs as _specs

    spec = _specs(base_seed=42, games_per_matchup=8)[0]
    return {
        "spec": spec,
        "process": process,
        "stdout": stdout or _FakeLog(),
        "stderr": stderr or _FakeLog(),
        "outcome_path": Path("never-written.json"),
        "stdout_path": Path("stdout.log"),
        "stderr_path": Path("stderr.log"),
        "started": 0.0,
        "finished": False,
    }


class EngineLifecycleTests(unittest.TestCase):
    """CODEX #67: an interrupted wait must kill and reap the engine without
    replacing the original exception, and both logs must close even if the
    first close raises; a launched engine that `finish_matchup` never owned
    is disposed of by `abandon_matchup`."""

    def test_interrupted_wait_kills_reaps_and_closes_both_logs(self):
        from run_payoff_panel_v1 import finish_matchup

        process = _FakeEngineProcess(wait_error=KeyboardInterrupt())
        handle = _fake_handle(process)
        with self.assertRaises(KeyboardInterrupt):
            finish_matchup(handle)
        self.assertTrue(process.killed)
        self.assertEqual(process.wait_calls, 2)
        self.assertTrue(handle["stdout"].closed)
        self.assertTrue(handle["stderr"].closed)
        self.assertTrue(handle["finished"])

    def test_cleanup_failures_never_replace_the_original_exception(self):
        """CODEX #68 item 2: wait raises KeyboardInterrupt, kill raises a
        non-OSError, and the first close raises. The exception that escapes
        is EXACTLY the interrupt; the reap still happens; both logs close."""
        from run_payoff_panel_v1 import finish_matchup

        interrupt = KeyboardInterrupt("original")
        process = _FakeEngineProcess(wait_error=interrupt, kill_error=RuntimeError("kill exploded"))
        handle = _fake_handle(process, stdout=_FakeLog(close_error=OSError("stdout close failed")))
        with self.assertRaises(KeyboardInterrupt) as ctx:
            finish_matchup(handle)
        self.assertIs(ctx.exception, interrupt)
        self.assertTrue(process.killed)
        self.assertEqual(process.wait_calls, 2)
        self.assertTrue(handle["stdout"].closed)
        self.assertTrue(handle["stderr"].closed)
        self.assertTrue(handle["finished"])

    def test_a_close_failure_on_a_clean_wait_fails_the_matchup(self):
        """CODEX #69 item 2: a clean wait followed by a failing stdout close
        is a real failure (an unflushed engine log must not precede canonical
        publication): both closes are attempted and the close error is the
        one raised. While an exception is already propagating, close failures
        stay suppressed (covered by the exact-exception test)."""
        from run_payoff_panel_v1 import finish_matchup

        process = _FakeEngineProcess()
        handle = _fake_handle(process, stdout=_FakeLog(close_error=OSError("stdout flush failed")))
        with self.assertRaises(OSError) as ctx:
            finish_matchup(handle)
        self.assertIn("stdout flush failed", str(ctx.exception))
        self.assertTrue(handle["stdout"].closed)
        self.assertTrue(handle["stderr"].closed)
        self.assertTrue(handle["finished"])
        self.assertFalse(process.killed)

    def test_abandon_is_no_throw_whatever_the_handle_holds(self):
        from run_payoff_panel_v1 import abandon_matchup

        # Nothing launched yet: only the dict exists.
        bare = {"finished": False}
        abandon_matchup(bare)
        self.assertTrue(bare["finished"])
        # Process and logs that all explode on cleanup.
        process = _FakeEngineProcess(wait_error=RuntimeError("wait exploded"), kill_error=RuntimeError("kill exploded"))
        handle = _fake_handle(
            process,
            stdout=_FakeLog(close_error=OSError("a")),
            stderr=_FakeLog(close_error=OSError("b")),
        )
        abandon_matchup(handle)  # must not raise
        self.assertTrue(process.killed)
        self.assertTrue(handle["stdout"].closed and handle["stderr"].closed)

    def test_a_failed_real_launch_leaves_a_registered_disposable_handle(self):
        """CODEX #68 item 1: the handle is registered before any log opens or
        any process is created, and the process registers itself at the start
        of its constructor. A launch whose Popen fails therefore still leaves
        a registered handle whose logs are open and whose process slot names
        the failed object; `abandon_matchup` disposes of it without raising."""
        from run_payoff_panel_v1 import abandon_matchup, launch_matchup

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            slots = synthetic_slots()
            locator = {index: tmp_path / f"slot-{index}" for index in range(SLOT_COUNT)}
            spec = build_matchup_specs(base_seed=42, games_per_matchup=8)[0]
            registered: list[dict] = []
            import unittest.mock
            import run_payoff_panel_v1 as module

            with unittest.mock.patch.object(module, "matchup_environment", lambda *args: {}):
                with self.assertRaises(OSError):
                    launch_matchup(
                        tmp_path / "no-such-engine.exe",
                        tmp_path,
                        tmp_path / "out",
                        slots,
                        locator,
                        spec,
                        register=registered.append,
                    )
            self.assertEqual(len(registered), 1)
            handle = registered[0]
            self.assertIsNotNone(handle["process"])
            self.assertFalse(handle["stdout"].closed)
            abandon_matchup(handle)
            self.assertTrue(handle["stdout"].closed)
            self.assertTrue(handle["stderr"].closed)
            self.assertTrue(handle["finished"])
            abandon_matchup(handle)  # idempotent

    def test_pool_abandons_a_handle_registered_by_a_launch_that_then_raises(self):
        """The ownership interval inside launch_one after registration: the
        fake registers a handle (as if the process were created) and then
        raises. The pool abandons that handle, on both paths."""
        specs = build_matchup_specs(base_seed=42, games_per_matchup=8)

        def launch_one(spec, register):
            handle = {"label": spec.label}
            register(handle)
            if spec.matchup_index == 1:
                raise MemoryError("synthetic interruption after the process was created")
            return handle

        def finish_one(handle):
            return {"outcome_path": None, "wall_seconds": 0.0, "outcome_sha256": hash_tag(0)}

        for workers in (1, 3):
            abandoned: list[str] = []
            with self.assertRaises(MemoryError):
                run_matchups_in_spec_order(
                    specs,
                    workers,
                    launch_one=launch_one,
                    finish_one=finish_one,
                    validate=lambda spec, result: result,
                    accept=lambda spec, value: None,
                    abandon_one=lambda handle: abandoned.append(handle["label"]),
                )
            self.assertIn(specs[1].label, abandoned)

    def test_abandon_disposes_an_unfinished_engine_once(self):
        from run_payoff_panel_v1 import abandon_matchup, finish_matchup

        process = _FakeEngineProcess()
        handle = _fake_handle(process)
        abandon_matchup(handle)
        self.assertTrue(process.killed)
        self.assertEqual(process.wait_calls, 1)
        self.assertTrue(handle["stdout"].closed and handle["stderr"].closed)
        abandon_matchup(handle)  # idempotent
        self.assertEqual(process.wait_calls, 1)
        # A finished handle is left alone.
        finished = _fake_handle(_FakeEngineProcess())
        finished["finished"] = True
        abandon_matchup(finished)
        self.assertFalse(finished["process"].killed)

    def test_pool_abandons_a_handle_when_finish_raises(self):
        specs = build_matchup_specs(base_seed=42, games_per_matchup=8)
        abandoned: list[str] = []

        def finish(spec):
            if spec.matchup_index == 1:
                raise PanelRunnerError(f"{spec.label} failed: synthetic")
            return {"outcome_path": None, "wall_seconds": 0.0, "outcome_sha256": hash_tag(spec.matchup_index)}

        for workers in (1, 3):
            abandoned.clear()
            with self.assertRaises(PanelRunnerError):
                _pool(specs, workers, finish, lambda spec, result: None, abandon_one=lambda handle: abandoned.append(handle.label))
            self.assertIn(specs[1].label, abandoned)


class MatchupWorkersTests(unittest.TestCase):
    """Matchups are independent engine processes, so running several at once
    may change nothing but wall time. The ordering helper is exercised with
    fake matchups whose completion order is deliberately scrambled, so the
    order-independence claim is tested rather than assumed."""

    @staticmethod
    def _fake_runner(delays: dict[int, float], started: list[str], fail_label: str | None = None):
        import threading
        import time

        lock = threading.Lock()
        completion_order: list[str] = []

        def run_one(spec):
            with lock:
                started.append(spec.label)
            time.sleep(delays.get(spec.matchup_index, 0.0))
            if spec.label == fail_label:
                raise PanelRunnerError(f"{spec.label} failed: synthetic")
            with lock:
                completion_order.append(spec.label)
            return {
                "outcome_path": None,
                "wall_seconds": 0.0,
                "outcome_sha256": hash_tag(spec.matchup_index),
            }

        return run_one, completion_order

    def test_parse_args_default_and_bounds(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(parse_args(_base_cli_args(tmp)).matchup_workers, DEFAULT_MATCHUP_WORKERS)
            self.assertEqual(DEFAULT_MATCHUP_WORKERS, 1)
            args = parse_args(_base_cli_args(tmp) + ["--matchup-workers", "12"])
            self.assertEqual(args.matchup_workers, 12)
            for bad in ("0", str(MAX_MATCHUP_WORKERS + 1), "-3"):
                with self.assertRaises(SystemExit) as ctx:
                    parse_args(_base_cli_args(tmp) + ["--matchup-workers", bad])
                self.assertEqual(ctx.exception.code, 2)

    def test_helper_rejects_out_of_range_workers(self):
        specs = build_matchup_specs(base_seed=42, games_per_matchup=8)
        for workers in (0, MAX_MATCHUP_WORKERS + 1):
            with self.assertRaises(PanelRunnerError):
                _pool(specs, workers, lambda spec: {}, lambda spec, result: None)

    def test_results_are_consumed_in_spec_order_for_any_worker_count(self):
        specs = build_matchup_specs(base_seed=42, games_per_matchup=8)
        expected_order = [spec.label for spec in specs]
        consumed_by_workers: dict[int, list[tuple[str, str]]] = {}
        # Reverse delays under a pool as wide as the panel: the LAST matchup
        # finishes first, so consumption order is only right if the helper
        # orders by spec, not by completion.
        reverse_delays = {spec.matchup_index: (len(specs) - spec.matchup_index) * 0.02 for spec in specs}
        for workers, delays in ((1, {}), (4, {index: 0.005 * (index % 3) for index in range(len(specs))}), (len(specs), reverse_delays)):
            started: list[str] = []
            run_one, completion_order = self._fake_runner(delays, started)
            consumed: list[tuple[str, str]] = []
            _pool(specs, workers, run_one, lambda spec, result: consumed.append((spec.label, result["outcome_sha256"]))
            )
            self.assertEqual([label for label, _ in consumed], expected_order)
            self.assertEqual(sorted(started), sorted(expected_order))
            consumed_by_workers[workers] = consumed
            if workers == len(specs):
                self.assertNotEqual(completion_order, expected_order)
        self.assertEqual(consumed_by_workers[1], consumed_by_workers[4])
        self.assertEqual(consumed_by_workers[1], consumed_by_workers[len(specs)])

    def test_failure_is_reported_at_the_lowest_spec_index_and_cancels_pending_matchups(self):
        specs = build_matchup_specs(base_seed=42, games_per_matchup=8)
        started: list[str] = []
        delays = {index: 0.05 for index in range(len(specs))}
        delays[2] = 0.0
        run_one, _ = self._fake_runner(delays, started, fail_label=specs[2].label)
        consumed: list[str] = []
        with self.assertRaises(PanelRunnerError) as ctx:
            _pool(specs, 2, run_one, lambda spec, result: consumed.append(spec.label))
        self.assertIn(specs[2].label, str(ctx.exception))
        # Whatever was consumed is a prefix of the spec order.
        self.assertEqual(consumed, [spec.label for spec in specs[: len(consumed)]])
        self.assertLess(len(started), len(specs))

    def _two_worker_failure_case(self, fail_in_validate: bool) -> None:
        """Shared body for the two prompt-failure regressions (CODEX #59, #66).
        Two workers; spec 0 launches and blocks; spec 1 launches and then
        fails, either as a process failure (`finish`) or as a malformed
        outcome (`validate`). Ordering is coordinated with events, never with
        elapsed time:

        - spec 1 does not fail until BOTH spec 0 and spec 1 have been
          launched (`both_launched`), so spec 0 can never be skipped;
        - spec 0 is released only after the POOL has recorded the abort:
          the pool cancels its tracked futures only after raising the abort
          flag under the admission lock, so the first observed `cancel()` on
          a tracked future (seen through the injected executor) is a
          synchronization point that happens-after the abort is recorded
          (CODEX review of 176c0337: an event set just before raising was
          not, and let spec 0's worker admit spec 2 first).

        The property under test is the pool's: once a failure is recorded,
        no further matchup is launched, whichever worker becomes free. So
        exactly two launches ever happen, spec 0's clean result is accepted,
        and the error names spec 1."""
        import concurrent.futures
        import threading

        specs = build_matchup_specs(base_seed=42, games_per_matchup=8)
        both_launched = threading.Event()
        spec1_failed = threading.Event()
        abort_recorded = threading.Event()
        release = threading.Event()
        lock = threading.Lock()
        launched: list[str] = []
        waits: dict[str, bool] = {}

        class ObservingExecutor(concurrent.futures.ThreadPoolExecutor):
            """Wraps each tracked future's `cancel` so the test can observe
            the pool's abort, which precedes every cancel."""

            def submit(self, fn, *args, **kwargs):
                future = super().submit(fn, *args, **kwargs)
                original_cancel = future.cancel

                def cancel():
                    abort_recorded.set()
                    return original_cancel()

                future.cancel = cancel
                return future

        def launch(spec):
            with lock:
                launched.append(spec.label)
                if specs[0].label in launched and specs[1].label in launched:
                    both_launched.set()
            return spec

        def finish(spec):
            if spec.matchup_index == 0:
                release.wait(timeout=10)
                return {"outcome_path": None, "wall_seconds": 0.0, "outcome_sha256": hash_tag(0)}
            if spec.matchup_index == 1:
                waits["both_launched_before_failure"] = both_launched.wait(timeout=10)
                if not fail_in_validate:
                    spec1_failed.set()
                    raise PanelRunnerError(f"{spec.label} failed: synthetic process failure")
            return {"outcome_path": None, "wall_seconds": 0.0, "outcome_sha256": hash_tag(spec.matchup_index)}

        def validate(spec, result):
            if fail_in_validate and spec.matchup_index == 1:
                spec1_failed.set()
                raise PanelRunnerError(f"{spec.label}: outcome header mismatch (synthetic)")
            return result

        def releaser():
            # The release condition IS the observed abort: a timed-out wait
            # must fail the test, never release spec 0 (a later cancel could
            # otherwise set the event before the final assertion).
            waits["abort_recorded_before_release"] = abort_recorded.wait(timeout=10)
            release.set()

        releaser_thread = threading.Thread(target=releaser, daemon=True)
        releaser_thread.start()
        accepted: list[str] = []
        with self.assertRaises(PanelRunnerError) as ctx:
            _pool(
                specs,
                2,
                finish,
                lambda spec, result: accepted.append(spec.label),
                launch=launch,
                validate=validate,
                executor_factory=ObservingExecutor,
            )
        releaser_thread.join(timeout=10)
        self.assertTrue(waits.get("both_launched_before_failure"), "spec 1 failed before both matchups had launched")
        self.assertTrue(waits.get("abort_recorded_before_release"), "spec 0 was released before the pool recorded the abort")
        self.assertTrue(spec1_failed.is_set())
        self.assertIn(specs[1].label, str(ctx.exception))
        self.assertEqual(sorted(launched), sorted([specs[0].label, specs[1].label]))
        self.assertEqual(accepted, [specs[0].label])

    def test_no_queued_matchup_starts_after_a_process_failure_signal(self):
        self._two_worker_failure_case(fail_in_validate=False)

    def test_a_malformed_outcome_is_detected_promptly_and_stops_launches(self):
        self._two_worker_failure_case(fail_in_validate=True)

    def test_lowest_index_failure_wins_when_a_later_matchup_fails_first(self):
        """Two failures: spec 3 fails immediately, spec 1 fails after a
        delay. The reported failure is spec 1's, the lower index, once every
        running matchup has finished."""
        specs = build_matchup_specs(base_seed=42, games_per_matchup=8)
        started: list[str] = []

        import threading

        spec1_launched = threading.Event()

        def launch(spec):
            started.append(spec.label)
            if spec.matchup_index == 1:
                spec1_launched.set()
            return spec

        def run_one(spec):
            import time

            if spec.matchup_index == 1:
                time.sleep(0.2)
                raise PanelRunnerError(f"{spec.label} failed: slow synthetic")
            if spec.matchup_index == 3:
                # Fail only once spec 1 is launched, so both are real failures.
                spec1_launched.wait(timeout=10)
                raise PanelRunnerError(f"{spec.label} failed: fast synthetic")
            time.sleep(0.05)
            return {"outcome_path": None, "wall_seconds": 0.0, "outcome_sha256": hash_tag(spec.matchup_index)}

        with self.assertRaises(PanelRunnerError) as ctx:
            _pool(specs, 4, run_one, lambda spec, result: None, launch=launch)
        self.assertIn(specs[1].label, str(ctx.exception))
        self.assertNotIn(specs[3].label, str(ctx.exception))

    def test_an_abort_marker_never_escapes_and_the_real_failure_is_reported(self):
        """CODEX P2 (round 2): a skipped matchup finishes as the internal
        abort marker. Whatever the interleaving, the error raised is the real
        failure, never the marker, and it is a PanelRunnerError."""
        from run_payoff_panel_v1 import _MatchupAbortedBeforeStartV1

        specs = build_matchup_specs(base_seed=42, games_per_matchup=8)

        import threading

        four_launched = threading.Event()
        launched: list[str] = []
        lock = threading.Lock()

        def launch(spec):
            with lock:
                launched.append(spec.label)
                if len(launched) == 4:
                    four_launched.set()
            return spec

        def run_one(spec):
            # Nothing fails until all four workers hold a launched matchup, so
            # the marker at spec 2 and the real failure at spec 3 both exist.
            four_launched.wait(timeout=10)
            if spec.matchup_index == 2:
                raise _MatchupAbortedBeforeStartV1(spec.label)
            if spec.matchup_index == 3:
                raise PanelRunnerError(f"{spec.label} failed: synthetic")
            return {"outcome_path": None, "wall_seconds": 0.0, "outcome_sha256": hash_tag(spec.matchup_index)}

        abandoned: list[str] = []
        with self.assertRaises(PanelRunnerError) as ctx:
            _pool(
                specs,
                4,
                run_one,
                lambda spec, result: None,
                launch=launch,
                abandon_one=lambda handle: abandoned.append(handle.label),
            )
        self.assertIn(specs[3].label, str(ctx.exception))
        self.assertNotIsInstance(ctx.exception, _MatchupAbortedBeforeStartV1)
        # CODEX #70: a sentinel raised by a callback AFTER registration still
        # abandons its handle, exactly like any other failure.
        self.assertIn(specs[2].label, abandoned)
        self.assertIn(specs[3].label, abandoned)

    def test_a_lower_index_validation_failure_beats_a_faster_higher_index_process_failure(self):
        """CODEX P2 (round 2): spec 1's engine exits cleanly but its outcome is
        rejected by `consume`; spec 3's engine fails first. The reported
        failure is spec 1's validation error, because lower-index outcomes are
        validated before any higher-index error is chosen."""
        specs = build_matchup_specs(base_seed=42, games_per_matchup=8)

        import threading

        spec1_launched = threading.Event()

        def launch(spec):
            if spec.matchup_index == 1:
                spec1_launched.set()
            return spec

        def run_one(spec):
            import time

            if spec.matchup_index == 3:
                spec1_launched.wait(timeout=10)
                raise PanelRunnerError(f"{spec.label} failed: fast synthetic process error")
            time.sleep(0.2 if spec.matchup_index in (0, 2) else 0.01)
            return {"outcome_path": None, "wall_seconds": 0.0, "outcome_sha256": hash_tag(spec.matchup_index)}

        def validate(spec, result):
            if spec.matchup_index == 1:
                raise PanelRunnerError(f"{spec.label}: outcome header mismatch (synthetic)")
            return result

        with self.assertRaises(PanelRunnerError) as ctx:
            _pool(specs, 4, run_one, lambda spec, result: None, validate=validate, launch=launch)
        self.assertIn(specs[1].label, str(ctx.exception))
        self.assertNotIn(specs[3].label, str(ctx.exception))

    def test_a_failing_submit_is_fail_closed_and_launches_nothing_further(self):
        """CODEX #67: the executor refuses the sixth submit after enqueuing its
        work item (so that item's future is never returned to the pool). Two
        workers; specs 0 and 1 block once launched. Submission runs under the
        admission lock, so the refusal cannot wait for launches; the assertion
        is exactly the pool's guarantee: whatever launched before the refusal
        is a subset of the returned items {0, 1}, the unreturned item (spec 5)
        and the cancelled items (specs 2 to 4) never launch, running work is
        waited for (specs 0 and 1 are released only once shutdown has begun),
        and the scheduling error propagates."""
        import concurrent.futures
        import threading

        specs = build_matchup_specs(base_seed=42, games_per_matchup=8)
        both_launched = threading.Event()
        shutdown_started = threading.Event()
        release = threading.Event()
        lock = threading.Lock()
        launched: list[str] = []
        waits: dict[str, bool] = {}

        class RefusingExecutor(concurrent.futures.ThreadPoolExecutor):
            submits = 0

            def submit(self, fn, *args, **kwargs):
                RefusingExecutor.submits += 1
                future = super().submit(fn, *args, **kwargs)
                if RefusingExecutor.submits == 6:
                    raise RuntimeError("synthetic scheduling failure on submit 6")
                return future

            def shutdown(self, wait=True, *, cancel_futures=False):
                shutdown_started.set()
                super().shutdown(wait=wait, cancel_futures=cancel_futures)

        def launch(spec):
            with lock:
                launched.append(spec.label)
                if specs[0].label in launched and specs[1].label in launched:
                    both_launched.set()
            return spec

        def finish(spec):
            if spec.matchup_index in (0, 1):
                release.wait(timeout=10)
            return {"outcome_path": None, "wall_seconds": 0.0, "outcome_sha256": hash_tag(spec.matchup_index)}

        def releaser():
            waits["shutdown_started_before_release"] = shutdown_started.wait(timeout=10)
            release.set()

        releaser_thread = threading.Thread(target=releaser, daemon=True)
        releaser_thread.start()
        with self.assertRaises(RuntimeError) as ctx:
            _pool(specs, 2, finish, lambda spec, result: None, launch=launch, executor_factory=RefusingExecutor)
        releaser_thread.join(timeout=10)
        self.assertIn("submit 6", str(ctx.exception))
        self.assertTrue(waits.get("shutdown_started_before_release"), "the matchups were released before shutdown began")
        self.assertTrue(set(launched) <= {specs[0].label, specs[1].label}, launched)
        for index in range(2, 6):
            self.assertNotIn(specs[index].label, launched)

    def test_an_unreturned_work_item_never_launches_even_with_a_free_worker(self):
        """CODEX #68 item 3: the executor enqueues the SECOND work item onto a
        free worker and then raises without returning its future. Because
        submit runs under the admission lock and the abort flag is raised
        before that lock is released, the free worker cannot pass admission
        for the unreturned item: it is skipped like a cancelled one. Spec 0
        is released only once shutdown has begun."""
        import concurrent.futures
        import threading

        specs = build_matchup_specs(base_seed=42, games_per_matchup=8)
        shutdown_started = threading.Event()
        release = threading.Event()
        lock = threading.Lock()
        launched: list[str] = []
        waits: dict[str, bool] = {}

        class RefusingSecondSubmit(concurrent.futures.ThreadPoolExecutor):
            submits = 0

            def submit(self, fn, *args, **kwargs):
                RefusingSecondSubmit.submits += 1
                future = super().submit(fn, *args, **kwargs)
                if RefusingSecondSubmit.submits == 2:
                    raise RuntimeError("synthetic scheduling failure on submit 2")
                return future

            def shutdown(self, wait=True, *, cancel_futures=False):
                shutdown_started.set()
                super().shutdown(wait=wait, cancel_futures=cancel_futures)

        def launch(spec):
            with lock:
                launched.append(spec.label)
            return spec

        def finish(spec):
            if spec.matchup_index == 0:
                release.wait(timeout=10)
            return {"outcome_path": None, "wall_seconds": 0.0, "outcome_sha256": hash_tag(spec.matchup_index)}

        def releaser():
            waits["shutdown_started_before_release"] = shutdown_started.wait(timeout=10)
            release.set()

        releaser_thread = threading.Thread(target=releaser, daemon=True)
        releaser_thread.start()
        with self.assertRaises(RuntimeError) as ctx:
            _pool(specs, 2, finish, lambda spec, result: None, launch=launch, executor_factory=RefusingSecondSubmit)
        releaser_thread.join(timeout=10)
        self.assertIn("submit 2", str(ctx.exception))
        self.assertTrue(waits.get("shutdown_started_before_release"))
        # Spec 0 may or may not have launched before the refusal (its worker
        # needs the admission lock the submitter holds); spec 1, the
        # unreturned item, must never have launched.
        self.assertNotIn(specs[1].label, launched)
        self.assertTrue(set(launched) <= {specs[0].label})

    def test_sequential_path_consumes_each_matchup_before_starting_the_next(self):
        specs = build_matchup_specs(base_seed=42, games_per_matchup=8)
        events: list[str] = []

        def run_one(spec):
            events.append(f"run {spec.label}")
            return {"outcome_path": None, "wall_seconds": 0.0, "outcome_sha256": hash_tag(spec.matchup_index)}

        _pool(specs, 1, run_one, lambda spec, result: events.append(f"consume {spec.label}"))
        expected = []
        for spec in specs:
            expected.extend([f"run {spec.label}", f"consume {spec.label}"])
        self.assertEqual(events, expected)


if __name__ == "__main__":
    unittest.main()
