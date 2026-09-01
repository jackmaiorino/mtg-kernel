"""Focused tests for the cycle-4 payoff panel runner.

Every test here is synthetic: no game is ever executed and no Rust test
binary is ever built. Outcome documents are fabricated directly to exercise
`summarize_outcome`, panel/BT-input assembly, and dry-run rendering, per the
round-C contract's "rank-sum arithmetic from synthetic outcomes (no games)"
requirement.
"""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from run_payoff_panel_v1 import (
    MANIFEST_SCHEMA,
    OUTCOME_SCHEMA,
    SLOT_COUNT,
    SLOT_LOCATOR_SCHEMA,
    PanelRunnerError,
    build_bt_input_document,
    build_matchup_specs,
    build_panel_document,
    canonical_bytes,
    load_manifest,
    load_slot_locator,
    render_dry_run_lines,
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
    return {
        "slot_index": index,
        "role": ROLES[index],
        "occupant_class": "historical-fallback" if index >= 6 else "policy",
        "source_base_seed": 900_000 + index,
        "source_run_sha256": hash_tag(10 * index + 1),
        "source_generation": 384 + index,
        "checkpoint_manifest_sha256": hash_tag(10 * index + 2),
        "checkpoint_payload_sha256": hash_tag(10 * index + 3),
        "model_parameter_sha256": hash_tag(10 * index + 4),
        "train_state_sha256": hash_tag(10 * index + 5),
        "weight_units": 125_000,
    }


def synthetic_slots() -> list[dict]:
    return [synthetic_slot(index) for index in range(SLOT_COUNT)]


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
            "generation": candidate["source_generation"],
            "checkpoint_manifest_sha256": candidate["checkpoint_manifest_sha256"],
            "checkpoint_payload_sha256": candidate["checkpoint_payload_sha256"],
            "model_parameter_sha256": candidate["model_parameter_sha256"],
        },
        "opponent": {
            "run_sha256": opponent["source_run_sha256"],
            "generation": opponent["source_generation"],
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
        locator = {index: Path(f"/stores/slot-{index}") for index in range(SLOT_COUNT)}
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
        locator = {index: Path(f"/stores/slot-{index}") for index in range(SLOT_COUNT)}
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


class ManifestAndSlotLocatorLoadingTests(unittest.TestCase):
    def test_load_manifest_accepts_well_formed_document(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest.json"
            path.write_bytes(canonical_bytes(synthetic_manifest_document()))
            raw, manifest_sha256, slots = load_manifest(path)
            self.assertEqual(len(slots), SLOT_COUNT)
            self.assertEqual(slots[0]["role"], "anchor-0")
            self.assertEqual(len(manifest_sha256), 64)
            self.assertEqual(raw, path.read_bytes())

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
            self.assertEqual(locator[0], base / "slot-0")

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


if __name__ == "__main__":
    unittest.main()
