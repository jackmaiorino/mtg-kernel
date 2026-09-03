"""Focused tests for the cycle-4 M2 common-root panel runner.

Every test here is synthetic: no game is ever executed and no Rust test
binary is ever built. Outcome documents are fabricated directly to exercise
outcome validation, root binding, the paired estimator, document assembly,
and dry-run rendering.

Two of these tests are DRIFT GUARDS against the Rust selector
(`mtg-kernel/src/native_cycle4_routing_v1.rs`), which decodes this runner's
output with `deny_unknown_fields` under a canonical-JSON codec that forbids
floating point:

  * `CanonicalFormTests` asserts the emitted bytes obey every rule that codec
    enforces (sorted keys, compact separators, a trailing LF, no null, no
    JSON float anywhere, printable ASCII, and the codec's 256-key and
    4096-byte-string bounds).
  * `WireShapeTests` asserts the exact key set of every object the Rust side
    names, so adding or renaming a field here fails a test rather than
    failing at freeze time.
"""

from __future__ import annotations

import contextlib
import io
import json
import os
import tempfile
import unittest
import unittest.mock
from pathlib import Path

from run_m2_common_root_panel_v1 import (
    ARM_ENDPOINT_STORE_GENERATION,
    BASELINE_ENDPOINT_STORE_GENERATION,
    ENDPOINT_IDS,
    ENDPOINT_LOCATOR_SCHEMA,
    EXPECTED_ROLES,
    H2H_ENVIRONMENT_KEYS,
    M2_COMMON_ROOT_BASE_SEED_V1,
    M2_OPPONENT_SEED_STRIDE_V1,
    MANIFEST_SCHEMA,
    OUTCOME_SCHEMA,
    PANEL_SCHEMA,
    ROOT_COUNT,
    SLOT_COUNT,
    SLOT_LOCATOR_SCHEMA,
    EndpointLocation,
    M2PanelError,
    MatchupSpec,
    SlotLocation,
    allocate_roots,
    bind_roots,
    build_comparisons,
    build_matchup_specs,
    build_panel_document,
    canonical_bytes,
    commit_staged_file,
    endpoint_store_generation,
    f64_bits,
    leg_score,
    load_endpoint_locator,
    load_genesis_manifest,
    load_slot_locator,
    matchup_environment,
    matchup_outcome_path,
    paired_statistics,
    parse_args,
    real,
    render_dry_run_lines,
    root_scores,
    staged_temp_path,
    summarize_outcome,
    validate_slot_chain_dirs,
    write_new_json,
)

ABS_ROOT = Path(tempfile.gettempdir()).resolve()


def hash_tag(tag: int) -> str:
    return f"cd{tag:062x}"


# ---------------------------------------------------------------------------
# Fixtures.
# ---------------------------------------------------------------------------


def synthetic_slot(index: int) -> dict:
    return {
        "slot_index": index,
        "role": EXPECTED_ROLES[index],
        "occupant_class": "policy" if index < 6 else "historical-fallback",
        "source_base_seed": 970000 + index,
        "source_run_sha256": hash_tag(0x100 + index),
        "source_generation": 384 + index,
        "checkpoint_manifest_sha256": hash_tag(0x200 + index),
        "checkpoint_payload_sha256": hash_tag(0x300 + index),
        "model_parameter_sha256": hash_tag(0x400 + index),
        "train_state_sha256": hash_tag(0x500 + index),
        "weight_units": 125_000,
    }


def synthetic_manifest_document() -> dict:
    return {
        "schema": MANIFEST_SCHEMA,
        "refresh_index": 0,
        "trainee_run_sha256": hash_tag(0x999),
        "slots": [synthetic_slot(index) for index in range(SLOT_COUNT)],
    }


def write_manifest(directory: Path) -> Path:
    path = directory / "refresh-00.manifest.json"
    path.write_bytes(canonical_bytes(synthetic_manifest_document()))
    return path


def plain_slot_locator() -> dict[int, SlotLocation]:
    return {
        index: SlotLocation(ABS_ROOT / f"cycle4-slot-{index}", None)
        for index in range(SLOT_COUNT)
    }


def endpoint_locations() -> dict[str, EndpointLocation]:
    return {
        endpoint_id: EndpointLocation(
            endpoint_id=endpoint_id,
            store_root=ABS_ROOT / f"cycle4-endpoint-{endpoint_id}",
            store_generation=endpoint_store_generation(endpoint_id),
            baseline_chain_dir=(
                ABS_ROOT / f"cycle4-chain-{endpoint_id}"
                if endpoint_id in ("static-rb", "treatment-rb")
                else None
            ),
        )
        for endpoint_id in ENDPOINT_IDS
    }


def endpoint_identity(endpoint_id: str) -> dict:
    ordinal = ENDPOINT_IDS.index(endpoint_id)
    return {
        "run_sha256": hash_tag(0x700 + ordinal),
        "identity_bundle_sha256": hash_tag(0x800 + ordinal),
        "generation": endpoint_store_generation(endpoint_id),
        "checkpoint_manifest_sha256": hash_tag(0x900 + ordinal),
        "checkpoint_payload_sha256": hash_tag(0xA00 + ordinal),
        "model_parameter_sha256": hash_tag(0xB00 + ordinal),
    }


def outcome_for(spec: MatchupSpec, slot: dict, pair_ranks: list[tuple[int, int]]) -> dict:
    """A well-formed terminal-stream document for one matchup. Each entry of
    `pair_ranks` is that root's `(p0 rank, p1 rank)` for the candidate."""
    assert len(pair_ranks) == spec.pair_count
    episodes = []
    counts = {"P0": {1: 0, 0: 0, -1: 0}, "P1": {1: 0, 0: 0, -1: 0}}
    for pair_index, (p0_rank, p1_rank) in enumerate(pair_ranks):
        seed = spec.evaluation_seed * 1000 + pair_index
        for seat, rank in (("P0", p0_rank), ("P1", p1_rank)):
            episodes.append(
                {
                    "episode_index": pair_index * 2 + (0 if seat == "P0" else 1),
                    "pair_index": pair_index,
                    "environment_seed": seed,
                    "learner_seat": seat,
                    "terminal_order_rank": rank,
                }
            )
            counts[seat][rank] += 1
    return {
        "schema": OUTCOME_SCHEMA,
        "evaluation_base_seed": spec.evaluation_seed,
        "pair_count": spec.pair_count,
        "episode_count": spec.pair_count * 2,
        "candidate": endpoint_identity(spec.endpoint_id),
        "opponent": {
            "run_sha256": slot["source_run_sha256"],
            "generation": slot["store_generation"],
            "checkpoint_manifest_sha256": slot["checkpoint_manifest_sha256"],
            "checkpoint_payload_sha256": slot["checkpoint_payload_sha256"],
            "model_parameter_sha256": slot["model_parameter_sha256"],
        },
        "runtime": {"all_natural": True, "environment_randomization_v2": True},
        "learner_outcomes": {
            "P0": {
                "wins": counts["P0"][1],
                "losses": counts["P0"][-1],
                "draws": counts["P0"][0],
            },
            "P1": {
                "wins": counts["P1"][1],
                "losses": counts["P1"][-1],
                "draws": counts["P1"][0],
            },
        },
        "episodes": episodes,
    }


def loaded_slots() -> list[dict]:
    """The manifest slots as `load_genesis_manifest` returns them (with the
    derived `store_generation`)."""
    document = synthetic_manifest_document()
    slots = document["slots"]
    for slot in slots:
        slot["store_generation"] = slot["source_generation"]
    return slots


def small_panel(root_count: int = 16, plan=None):
    """Builds a complete synthetic panel over `root_count` roots.

    `plan(endpoint_id, root_index)` returns that root's `(p0, p1)` ranks;
    the default gives every endpoint a win on the first half of its roots."""
    if plan is None:

        def plan(endpoint_id: str, root_index: int) -> tuple[int, int]:
            rank = 1 if root_index < root_count // 2 else -1
            return (rank, rank)

    slots = loaded_slots()
    allocation = allocate_roots([slot["weight_units"] for slot in slots], root_count)
    specs = build_matchup_specs(allocation)
    summaries: dict[tuple[str, int], dict] = {}
    outcome_hashes: dict[tuple[str, int], str] = {}
    for spec in specs:
        ranks = [
            plan(spec.endpoint_id, spec.first_root_index + pair_index)
            for pair_index in range(spec.pair_count)
        ]
        outcome = outcome_for(spec, slots[spec.slot_index], ranks)
        summaries[(spec.endpoint_id, spec.slot_index)] = summarize_outcome(
            outcome, spec, slots[spec.slot_index]
        )
        outcome_hashes[(spec.endpoint_id, spec.slot_index)] = hash_tag(spec.slot_index)
    roots = bind_roots(specs, summaries, allocation)
    panel = build_panel_document(
        hash_tag(0x21),
        "control-r",
        slots,
        allocation,
        specs,
        summaries,
        outcome_hashes,
        roots,
    )
    return panel, roots, specs, allocation


def emit_panel_bytes(path: str, root_count: int = ROOT_COUNT) -> None:
    """Writes a complete synthetic panel's canonical bytes to `path`.

    Called from `native_cycle4_routing_v1`'s cross-language integration test,
    which decodes the result with the Rust selector and re-derives every
    statistic from the root table. The plan below is deliberately NOT
    degenerate: each endpoint wins a different, deterministic share of roots
    and the two legs disagree on some of them, so the pooled and P1 strata
    differ and the bitwise cross-check has something to check."""
    p0_shares = {"control-r": 512, "static-rb": 545, "treatment-rb": 620, "g896": 512}
    p1_shares = {"control-r": 500, "static-rb": 520, "treatment-rb": 590, "g896": 512}

    def plan(endpoint_id: str, root_index: int) -> tuple[int, int]:
        # The two legs win different, offset shares of the roots, so the P1
        # stratum is not a copy of the pooled one in either mean or spread.
        scaled = root_index * 1024 // root_count
        p0 = 1 if scaled < p0_shares[endpoint_id] else -1
        p1 = 1 if (scaled + 64) % 1024 < p1_shares[endpoint_id] else -1
        return (p0, p1)

    panel, _, _, _ = small_panel(root_count, plan)
    Path(path).write_bytes(canonical_bytes(panel))


# ---------------------------------------------------------------------------
# Tests.
# ---------------------------------------------------------------------------


class RootAllocationTests(unittest.TestCase):
    def test_the_genesis_pool_allocates_128_roots_per_slot(self):
        allocation = allocate_roots([125_000] * SLOT_COUNT, ROOT_COUNT)
        self.assertEqual(allocation, [128] * SLOT_COUNT)
        self.assertEqual(sum(allocation), ROOT_COUNT)

    def test_largest_remainder_breaks_ties_toward_the_lower_slot(self):
        # 10 roots over eight equal weights: eight slots get one, and the two
        # leftovers go to slots 0 and 1.
        allocation = allocate_roots([1] * SLOT_COUNT, 10)
        self.assertEqual(allocation, [2, 2, 1, 1, 1, 1, 1, 1])

    def test_an_allocation_that_starves_a_slot_is_refused(self):
        with self.assertRaises(M2PanelError):
            allocate_roots([1_000_000, 1, 1, 1, 1, 1, 1, 1], 8)

    def test_uneven_weights_are_apportioned_and_still_sum(self):
        weights = [300_000, 200_000, 100_000, 100_000, 100_000, 100_000, 50_000, 50_000]
        allocation = allocate_roots(weights, ROOT_COUNT)
        self.assertEqual(sum(allocation), ROOT_COUNT)
        self.assertEqual(allocation[0], 307)
        self.assertTrue(all(value > 0 for value in allocation))


class MatchupPlanTests(unittest.TestCase):
    def test_every_endpoint_gets_the_same_evaluation_seed_per_slot(self):
        specs = build_matchup_specs([128] * SLOT_COUNT)
        self.assertEqual(len(specs), len(ENDPOINT_IDS) * SLOT_COUNT)
        by_slot: dict[int, set[int]] = {}
        for spec in specs:
            by_slot.setdefault(spec.slot_index, set()).add(spec.evaluation_seed)
        for slot_index, seeds in by_slot.items():
            self.assertEqual(
                seeds,
                {M2_COMMON_ROOT_BASE_SEED_V1 + slot_index * M2_OPPONENT_SEED_STRIDE_V1},
                "common roots require one evaluation seed per slot, shared by every endpoint",
            )

    def test_root_indexes_partition_the_pool_in_slot_order(self):
        specs = build_matchup_specs([128] * SLOT_COUNT)
        for endpoint_id in ENDPOINT_IDS:
            covered: list[int] = []
            for spec in (s for s in specs if s.endpoint_id == endpoint_id):
                covered.extend(
                    range(spec.first_root_index, spec.first_root_index + spec.pair_count)
                )
            self.assertEqual(sorted(covered), list(range(ROOT_COUNT)))

    def test_evaluation_seeds_are_distinct_across_slots(self):
        specs = build_matchup_specs([128] * SLOT_COUNT)
        seeds = {spec.evaluation_seed for spec in specs}
        self.assertEqual(len(seeds), SLOT_COUNT)


class SummarizeOutcomeTests(unittest.TestCase):
    def setUp(self):
        self.slots = loaded_slots()
        self.allocation = allocate_roots([125_000] * SLOT_COUNT, 16)
        self.specs = build_matchup_specs(self.allocation)
        self.spec = self.specs[0]
        self.ranks = [(1, -1)] * self.spec.pair_count

    def test_a_well_formed_outcome_summarizes(self):
        outcome = outcome_for(self.spec, self.slots[0], self.ranks)
        summary = summarize_outcome(outcome, self.spec, self.slots[0])
        self.assertEqual(len(summary["legs"]), self.spec.pair_count)
        self.assertEqual(summary["legs"][0]["p0"], 1)
        self.assertEqual(summary["legs"][0]["p1"], -1)
        self.assertEqual(
            summary["candidate_identity"]["checkpoint_manifest_sha256"],
            endpoint_identity(self.spec.endpoint_id)["checkpoint_manifest_sha256"],
        )

    def test_a_nonnatural_terminal_is_refused(self):
        outcome = outcome_for(self.spec, self.slots[0], self.ranks)
        outcome["runtime"]["all_natural"] = False
        with self.assertRaises(M2PanelError):
            summarize_outcome(outcome, self.spec, self.slots[0])

    def test_a_nonterminal_rank_is_refused(self):
        outcome = outcome_for(self.spec, self.slots[0], self.ranks)
        outcome["episodes"][0]["terminal_order_rank"] = 2
        with self.assertRaises(M2PanelError):
            summarize_outcome(outcome, self.spec, self.slots[0])

    def test_a_mismatched_opponent_identity_is_refused(self):
        outcome = outcome_for(self.spec, self.slots[0], self.ranks)
        outcome["opponent"]["model_parameter_sha256"] = hash_tag(0xDEAD)
        with self.assertRaises(M2PanelError):
            summarize_outcome(outcome, self.spec, self.slots[0])

    def test_an_unpinned_candidate_generation_is_refused(self):
        outcome = outcome_for(self.spec, self.slots[0], self.ranks)
        outcome["candidate"]["generation"] = 1_024
        with self.assertRaises(M2PanelError):
            summarize_outcome(outcome, self.spec, self.slots[0])

    def test_legs_that_do_not_share_one_seed_are_refused(self):
        outcome = outcome_for(self.spec, self.slots[0], self.ranks)
        outcome["episodes"][1]["environment_seed"] += 1
        with self.assertRaises(M2PanelError):
            summarize_outcome(outcome, self.spec, self.slots[0])

    def test_a_wrong_evaluation_seed_is_refused(self):
        outcome = outcome_for(self.spec, self.slots[0], self.ranks)
        outcome["evaluation_base_seed"] += 1
        with self.assertRaises(M2PanelError):
            summarize_outcome(outcome, self.spec, self.slots[0])


class RootBindingTests(unittest.TestCase):
    def test_roots_are_common_across_every_endpoint(self):
        _, roots, _, _ = small_panel()
        self.assertEqual(len(roots), 16)
        for root in roots:
            self.assertEqual(set(root["legs"]), set(ENDPOINT_IDS))
        self.assertEqual(
            len({root["environment_seed"] for root in roots}),
            len(roots),
            "every root must have its own environment seed",
        )

    def test_an_endpoint_that_played_a_different_seed_is_refused(self):
        slots = loaded_slots()
        allocation = allocate_roots([125_000] * SLOT_COUNT, 16)
        specs = build_matchup_specs(allocation)
        summaries = {}
        for spec in specs:
            ranks = [(1, -1)] * spec.pair_count
            outcome = outcome_for(spec, slots[spec.slot_index], ranks)
            if spec.endpoint_id == "treatment-rb":
                for episode in outcome["episodes"]:
                    episode["environment_seed"] += 7
            summaries[(spec.endpoint_id, spec.slot_index)] = summarize_outcome(
                outcome, spec, slots[spec.slot_index]
            )
        with self.assertRaises(M2PanelError) as context:
            bind_roots(specs, summaries, allocation)
        self.assertIn("not common", str(context.exception))


class PairedEstimatorTests(unittest.TestCase):
    def test_leg_scores_use_the_half_point_draw_convention(self):
        self.assertEqual(leg_score(1), 1.0)
        self.assertEqual(leg_score(0), 0.5)
        self.assertEqual(leg_score(-1), 0.0)
        with self.assertRaises(M2PanelError):
            leg_score(2)

    def test_a_constant_difference_has_a_zero_width_interval(self):
        statistics = paired_statistics([0.5] * 1_024)
        self.assertEqual(statistics["root_count"], 1_024)
        self.assertEqual(statistics["delta_pp"]["f64_bits"], f64_bits(50.0))
        self.assertEqual(statistics["standard_error_pp"]["f64_bits"], f64_bits(0.0))
        self.assertEqual(statistics["ci_low_pp"]["f64_bits"], f64_bits(50.0))
        self.assertEqual(statistics["ci_high_pp"]["f64_bits"], f64_bits(50.0))

    def test_a_symmetric_split_centres_on_zero(self):
        differences = [1.0] * 512 + [-1.0] * 512
        statistics = paired_statistics(differences)
        self.assertEqual(statistics["delta_pp"]["f64_bits"], f64_bits(0.0))
        self.assertLess(
            _as_float(statistics["ci_low_pp"]), _as_float(statistics["ci_high_pp"])
        )
        self.assertLess(_as_float(statistics["one_sided_lower_bound_pp"]), 0.0)
        # 1,024 roots at +/-1: sample sd is sqrt(1024/1023), so the standard
        # error in percentage points is 100 * sd / 32.
        self.assertAlmostEqual(
            _as_float(statistics["standard_error_pp"]),
            100.0 * (1024.0 / 1023.0) ** 0.5 / 32.0,
            places=9,
        )

    def test_the_estimator_matches_the_rust_selector_bit_for_bit(self):
        """CROSS-LANGUAGE PIN. `native_cycle4_routing_v1`'s
        `the_estimator_matches_the_python_runner_bit_for_bit_v1` asserts the
        same literals on the same 1,024 differences. The selector requires
        bit equality between what this runner declares and what it
        recomputes, so a drift in either implementation fails a test here
        rather than a campaign at freeze time."""
        differences = [0.0] * 512 + [1.0] * 108 + [0.0] * 404
        self.assertEqual(len(differences), 1_024)
        statistics = paired_statistics(differences)
        self.assertEqual(statistics["delta_pp"]["f64_bits"], "4025180000000000")
        self.assertEqual(
            statistics["standard_deviation_pp"]["f64_bits"], "403ebb0c379a43ae"
        )
        self.assertEqual(statistics["standard_error_pp"]["f64_bits"], "3feebb0c379a43ae")
        self.assertEqual(statistics["ci_low_pp"]["f64_bits"], "4021544deab0b3cf")
        self.assertEqual(statistics["ci_high_pp"]["f64_bits"], "4028dbb2154f4c31")
        self.assertEqual(
            statistics["one_sided_lower_bound_pp"]["f64_bits"], "4021ef3dba71d602"
        )

    def test_a_single_root_cannot_be_a_comparison(self):
        with self.assertRaises(M2PanelError):
            paired_statistics([0.5])

    def test_the_p1_stratum_reads_only_the_p1_leg(self):
        _, roots, _, _ = small_panel(
            plan=lambda endpoint_id, root_index: (
                (1, -1) if endpoint_id == "treatment-rb" else (1, 1)
            )
        )
        pooled, p0, p1 = root_scores(roots, "treatment-rb")
        self.assertEqual(set(p0), {1.0})
        self.assertEqual(set(p1), {0.0})
        self.assertEqual(set(pooled), {0.5})

    def test_comparisons_cover_every_unordered_pair_in_endpoint_order(self):
        _, roots, _, _ = small_panel()
        comparisons = build_comparisons(roots)
        self.assertEqual(len(comparisons), 6)
        pairs = [(row["endpoint_a"], row["endpoint_b"]) for row in comparisons]
        self.assertEqual(pairs[0], ("control-r", "static-rb"))
        self.assertEqual(pairs[-1], ("treatment-rb", "g896"))
        for row in comparisons:
            self.assertLess(
                ENDPOINT_IDS.index(row["endpoint_a"]),
                ENDPOINT_IDS.index(row["endpoint_b"]),
            )

    def test_the_legacy_integer_net_is_reported_and_gates_nothing(self):
        _, roots, _, _ = small_panel(
            plan=lambda endpoint_id, root_index: (
                (1, 1) if endpoint_id == "treatment-rb" else (-1, -1)
            )
        )
        comparisons = build_comparisons(roots)
        row = next(
            comparison
            for comparison in comparisons
            if comparison["endpoint_a"] == "treatment-rb"
            and comparison["endpoint_b"] == "g896"
        )
        diagnostics = row["diagnostics"]
        # 16 roots, both legs, +1 vs -1 on each: net 2 per leg per root.
        self.assertEqual(diagnostics["legacy_integer_net"], 64)
        self.assertEqual(diagnostics["legacy_integer_net_p0"], 32)
        self.assertEqual(diagnostics["legacy_integer_net_p1"], 32)
        self.assertTrue(diagnostics["gates_nothing"])
        self.assertFalse(diagnostics["confidence_sequence_computed"])
        self.assertIn("eb_cs_reference_v1.py", diagnostics["confidence_sequence_reason"])


def _as_float(value: dict) -> float:
    import struct

    return struct.unpack(">d", bytes.fromhex(value["f64_bits"]))[0]


class DocumentAssemblyTests(unittest.TestCase):
    def test_the_panel_carries_the_endpoints_pool_and_roots(self):
        panel, roots, specs, allocation = small_panel()
        self.assertEqual(panel["schema"], PANEL_SCHEMA)
        self.assertEqual(panel["root_count"], len(roots))
        self.assertEqual(len(panel["pool"]), SLOT_COUNT)
        self.assertEqual([row["endpoint_id"] for row in panel["endpoints"]], list(ENDPOINT_IDS))
        self.assertEqual(len(panel["matchups"]), len(specs))
        self.assertEqual(
            [row["root_allocation"] for row in panel["pool"]], allocation
        )
        self.assertEqual(
            panel["endpoints"][ENDPOINT_IDS.index("g896")]["store_generation"],
            BASELINE_ENDPOINT_STORE_GENERATION,
        )
        self.assertEqual(
            panel["endpoints"][0]["store_generation"], ARM_ENDPOINT_STORE_GENERATION
        )

    def test_two_endpoints_resolving_to_one_checkpoint_are_refused(self):
        slots = loaded_slots()
        allocation = allocate_roots([125_000] * SLOT_COUNT, 16)
        specs = build_matchup_specs(allocation)
        summaries = {}
        hashes = {}
        for spec in specs:
            outcome = outcome_for(spec, slots[spec.slot_index], [(1, -1)] * spec.pair_count)
            if spec.endpoint_id == "static-rb":
                outcome["candidate"]["checkpoint_manifest_sha256"] = endpoint_identity(
                    "control-r"
                )["checkpoint_manifest_sha256"]
            summaries[(spec.endpoint_id, spec.slot_index)] = summarize_outcome(
                outcome, spec, slots[spec.slot_index]
            )
            hashes[(spec.endpoint_id, spec.slot_index)] = hash_tag(1)
        roots = bind_roots(specs, summaries, allocation)
        with self.assertRaises(M2PanelError):
            build_panel_document(
                hash_tag(0x21), "control-r", slots, allocation, specs, summaries, hashes, roots
            )

    def test_an_endpoint_reporting_two_identities_is_refused(self):
        slots = loaded_slots()
        allocation = allocate_roots([125_000] * SLOT_COUNT, 16)
        specs = build_matchup_specs(allocation)
        summaries = {}
        hashes = {}
        for spec in specs:
            outcome = outcome_for(spec, slots[spec.slot_index], [(1, -1)] * spec.pair_count)
            if spec.endpoint_id == "control-r" and spec.slot_index == 3:
                outcome["candidate"]["run_sha256"] = hash_tag(0xFEED)
            summaries[(spec.endpoint_id, spec.slot_index)] = summarize_outcome(
                outcome, spec, slots[spec.slot_index]
            )
            hashes[(spec.endpoint_id, spec.slot_index)] = hash_tag(1)
        roots = bind_roots(specs, summaries, allocation)
        with self.assertRaises(M2PanelError):
            build_panel_document(
                hash_tag(0x21), "control-r", slots, allocation, specs, summaries, hashes, roots
            )


class CanonicalFormTests(unittest.TestCase):
    """The emitted bytes must satisfy every rule `canonical_json_v1` enforces
    when `native_cycle4_routing_v1` decodes them."""

    def setUp(self):
        panel, _, _, _ = small_panel()
        self.panel = panel
        self.encoded = canonical_bytes(panel)

    def test_the_bytes_are_sorted_compact_and_lf_terminated(self):
        self.assertTrue(self.encoded.endswith(b"\n"))
        self.assertNotIn(b", ", self.encoded)
        self.assertNotIn(b": ", self.encoded)
        reparsed = json.loads(self.encoded.decode("utf-8"))
        self.assertEqual(canonical_bytes(reparsed), self.encoded)

    def test_no_float_null_or_non_ascii_reaches_the_document(self):
        def walk(node, path):
            if isinstance(node, float):
                self.fail(f"canonical JSON forbids floating point, found one at {path}")
            if node is None:
                self.fail(f"canonical JSON forbids null, found one at {path}")
            if isinstance(node, str):
                self.assertTrue(
                    all(0x20 <= ord(char) <= 0x7E for char in node),
                    f"non-printable or non-ASCII string at {path}",
                )
                self.assertLessEqual(len(node.encode("utf-8")), 4096, path)
            if isinstance(node, dict):
                self.assertLessEqual(len(node), 256, f"object key bound at {path}")
                for key, value in node.items():
                    walk(key, f"{path}.{key}")
                    walk(value, f"{path}.{key}")
            elif isinstance(node, list):
                for index, value in enumerate(node):
                    walk(value, f"{path}[{index}]")

        walk(json.loads(self.encoded.decode("utf-8")), "$")

    def test_the_f64_bit_encoding_is_ieee_754_big_endian(self):
        self.assertEqual(f64_bits(1.0), "3ff0000000000000")
        self.assertEqual(f64_bits(0.0), "0000000000000000")
        self.assertEqual(f64_bits(-2.0), "c000000000000000")
        self.assertEqual(real(1.0), {"f64_bits": "3ff0000000000000", "text": "1.0"})
        with self.assertRaises(M2PanelError):
            f64_bits(float("inf"))


class CommitSemanticsTests(unittest.TestCase):
    """The panel is an input the routing record binds by SHA-256, so
    committing it is CREATE-NEW: a rerun that produced identical bytes is a
    no-op, and anything else already at the final name is an error. An
    `os.replace` would have silently re-keyed a published freeze."""

    def test_a_fresh_commit_writes_the_staged_bytes(self):
        with tempfile.TemporaryDirectory() as tmp:
            final = Path(tmp) / "m2-common-root-panel.json"
            temp = staged_temp_path(final)
            payload = write_new_json(temp, {"schema": PANEL_SCHEMA})
            commit_staged_file(temp, final)
            self.assertEqual(final.read_bytes(), payload)
            self.assertFalse(temp.exists(), "the staged copy must not linger")

    def test_an_identical_republish_is_a_no_op(self):
        with tempfile.TemporaryDirectory() as tmp:
            final = Path(tmp) / "m2-common-root-panel.json"
            temp = staged_temp_path(final)
            payload = write_new_json(temp, {"schema": PANEL_SCHEMA})
            commit_staged_file(temp, final)
            stat_before = final.stat().st_ino if os.name != "nt" else final.read_bytes()

            temp = staged_temp_path(final)
            write_new_json(temp, {"schema": PANEL_SCHEMA})
            commit_staged_file(temp, final)
            self.assertEqual(final.read_bytes(), payload)
            self.assertFalse(temp.exists())
            if os.name != "nt":
                self.assertEqual(final.stat().st_ino, stat_before)

    def test_a_differing_panel_is_refused_not_overwritten(self):
        with tempfile.TemporaryDirectory() as tmp:
            final = Path(tmp) / "m2-common-root-panel.json"
            temp = staged_temp_path(final)
            original = write_new_json(temp, {"schema": PANEL_SCHEMA, "root_count": 1024})
            commit_staged_file(temp, final)

            temp = staged_temp_path(final)
            write_new_json(temp, {"schema": PANEL_SCHEMA, "root_count": 16})
            with self.assertRaises(M2PanelError) as context:
                commit_staged_file(temp, final)
            self.assertIn("immutable", str(context.exception))
            self.assertEqual(
                final.read_bytes(), original, "the published panel must be untouched"
            )


class CommitRaceTests(unittest.TestCase):
    """The existence check inside `commit_staged_file` is a courtesy; what
    makes the commit safe is that every primitive it uses FAILS when the
    destination exists. These tests defeat the check (by patching
    `Path.exists` to False while the final path is really there) and assert
    the commit still refuses, on both the hard-link path and the exclusive-
    create fallback. An `os.rename` fallback would silently pass both."""

    def _commit_with_a_racing_panel(self) -> tuple[bytes, Exception]:
        with tempfile.TemporaryDirectory() as tmp:
            final = Path(tmp) / "m2-common-root-panel.json"
            temp = staged_temp_path(final)
            write_new_json(temp, {"schema": PANEL_SCHEMA, "root_count": 16})
            racer = canonical_bytes({"schema": PANEL_SCHEMA, "root_count": 1024})
            final.write_bytes(racer)
            # The panel appeared AFTER the check would have run.
            with unittest.mock.patch.object(Path, "exists", return_value=False):
                with self.assertRaises(M2PanelError) as context:
                    commit_staged_file(temp, final)
            return final.read_bytes(), context.exception

    def test_a_panel_appearing_after_the_check_is_refused(self):
        surviving, error = self._commit_with_a_racing_panel()
        self.assertIn("another writer", str(error))
        self.assertEqual(
            surviving,
            canonical_bytes({"schema": PANEL_SCHEMA, "root_count": 1024}),
            "the racing panel must survive untouched",
        )

    def test_the_no_hard_link_fallback_also_refuses_a_racing_panel(self):
        with unittest.mock.patch("run_m2_common_root_panel_v1.os.link") as link:
            link.side_effect = OSError("hard links unsupported")
            surviving, error = self._commit_with_a_racing_panel()
        self.assertIn("another writer", str(error))
        self.assertEqual(
            surviving,
            canonical_bytes({"schema": PANEL_SCHEMA, "root_count": 1024}),
            "the racing panel must survive untouched",
        )

    def test_the_no_hard_link_fallback_still_publishes_a_fresh_panel(self):
        with tempfile.TemporaryDirectory() as tmp:
            final = Path(tmp) / "m2-common-root-panel.json"
            temp = staged_temp_path(final)
            payload = write_new_json(temp, {"schema": PANEL_SCHEMA})
            with unittest.mock.patch("run_m2_common_root_panel_v1.os.link") as link:
                link.side_effect = OSError("hard links unsupported")
                commit_staged_file(temp, final)
            self.assertEqual(final.read_bytes(), payload)
            self.assertFalse(temp.exists())

    def test_a_filesystem_with_neither_primitive_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            final = Path(tmp) / "m2-common-root-panel.json"
            temp = staged_temp_path(final)
            write_new_json(temp, {"schema": PANEL_SCHEMA})
            with unittest.mock.patch("run_m2_common_root_panel_v1.os.link") as link:
                link.side_effect = OSError("hard links unsupported")
                with unittest.mock.patch("run_m2_common_root_panel_v1.os.open") as opener:
                    opener.side_effect = OSError("no exclusive create either")
                    with self.assertRaises(M2PanelError) as context:
                        commit_staged_file(temp, final)
            self.assertIn("create-new", str(context.exception))
            self.assertFalse(final.exists(), "nothing may be published")


class WireShapeTests(unittest.TestCase):
    """Drift guard: the exact key set of every object
    `native_cycle4_routing_v1`'s `deny_unknown_fields` structs name."""

    def setUp(self):
        panel, _, _, _ = small_panel()
        self.panel = panel

    def test_the_top_level_keys_match_the_rust_struct(self):
        self.assertEqual(
            set(self.panel),
            {
                "schema",
                "genesis_manifest_sha256",
                "pool_arm",
                "root_count",
                "base_seed",
                "opponent_seed_stride",
                "pool",
                "endpoints",
                "matchups",
                "roots",
                "comparisons",
            },
        )

    def test_every_nested_object_matches_the_rust_struct(self):
        self.assertEqual(
            set(self.panel["pool"][0]),
            {
                "slot_index",
                "role",
                "weight_units",
                "root_allocation",
                "store_generation",
                "source_run_sha256",
                "checkpoint_manifest_sha256",
                "checkpoint_payload_sha256",
                "model_parameter_sha256",
            },
        )
        self.assertEqual(
            set(self.panel["endpoints"][0]),
            {
                "endpoint_id",
                "store_generation",
                "run_sha256",
                "identity_bundle_sha256",
                "checkpoint_manifest_sha256",
                "checkpoint_payload_sha256",
                "model_parameter_sha256",
            },
        )
        self.assertEqual(
            set(self.panel["matchups"][0]),
            {
                "endpoint_id",
                "slot_index",
                "evaluation_seed",
                "pair_count",
                "game_count",
                "first_root_index",
                "outcome_sha256",
            },
        )
        self.assertEqual(
            set(self.panel["roots"][0]),
            {"root_index", "slot_index", "pair_index", "environment_seed", "legs"},
        )
        self.assertEqual(set(self.panel["roots"][0]["legs"]["control-r"]), {"p0", "p1"})
        self.assertEqual(
            set(self.panel["comparisons"][0]),
            {"endpoint_a", "endpoint_b", "pooled", "p0_stratum", "p1_stratum", "diagnostics"},
        )
        self.assertEqual(
            set(self.panel["comparisons"][0]["pooled"]),
            {
                "root_count",
                "delta_pp",
                "standard_deviation_pp",
                "standard_error_pp",
                "ci_low_pp",
                "ci_high_pp",
                "one_sided_lower_bound_pp",
            },
        )
        self.assertEqual(
            set(self.panel["comparisons"][0]["pooled"]["delta_pp"]), {"f64_bits", "text"}
        )
        self.assertEqual(
            set(self.panel["comparisons"][0]["diagnostics"]),
            {
                "legacy_integer_net",
                "legacy_integer_net_p0",
                "legacy_integer_net_p1",
                "gates_nothing",
                "confidence_sequence_computed",
                "confidence_sequence_reason",
            },
        )


class InputLoadingTests(unittest.TestCase):
    def test_a_non_genesis_manifest_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            document = synthetic_manifest_document()
            document["refresh_index"] = 4
            path = directory / "refresh-04.manifest.json"
            path.write_bytes(canonical_bytes(document))
            with self.assertRaises(M2PanelError) as context:
                load_genesis_manifest(path)
            self.assertIn("GENESIS", str(context.exception))

    def test_the_genesis_manifest_loads_with_derived_generations(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = write_manifest(Path(tmp))
            manifest_sha256, trainee_run_sha256, slots = load_genesis_manifest(path)
            self.assertEqual(len(manifest_sha256), 64)
            self.assertEqual(trainee_run_sha256, hash_tag(0x999))
            self.assertEqual(slots[0]["store_generation"], 384)

    def test_the_endpoint_locator_pins_generations_and_chain_dirs(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "endpoints.json"
            path.write_bytes(
                canonical_bytes(
                    {
                        "schema": ENDPOINT_LOCATOR_SCHEMA,
                        "endpoints": {
                            "control-r": {"store_root": str(ABS_ROOT / "c")},
                            "static-rb": {
                                "store_root": str(ABS_ROOT / "s"),
                                "baseline_chain_dir": str(ABS_ROOT / "s-chain"),
                            },
                            "treatment-rb": {
                                "store_root": str(ABS_ROOT / "t"),
                                "baseline_chain_dir": str(ABS_ROOT / "t-chain"),
                            },
                            "g896": {"store_root": str(ABS_ROOT / "g")},
                        },
                    }
                )
            )
            endpoints = load_endpoint_locator(path)
            self.assertEqual(
                endpoints["control-r"].store_generation, ARM_ENDPOINT_STORE_GENERATION
            )
            self.assertEqual(
                endpoints["g896"].store_generation, BASELINE_ENDPOINT_STORE_GENERATION
            )
            self.assertIsNone(endpoints["control-r"].baseline_chain_dir)
            self.assertIsNotNone(endpoints["treatment-rb"].baseline_chain_dir)

    def test_a_v4_endpoint_without_a_chain_dir_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "endpoints.json"
            path.write_bytes(
                canonical_bytes(
                    {
                        "schema": ENDPOINT_LOCATOR_SCHEMA,
                        "endpoints": {
                            "control-r": {"store_root": str(ABS_ROOT / "c")},
                            "static-rb": {"store_root": str(ABS_ROOT / "s")},
                            "treatment-rb": {
                                "store_root": str(ABS_ROOT / "t"),
                                "baseline_chain_dir": str(ABS_ROOT / "t-chain"),
                            },
                            "g896": {"store_root": str(ABS_ROOT / "g")},
                        },
                    }
                )
            )
            with self.assertRaises(M2PanelError):
                load_endpoint_locator(path)

    def test_a_v3_endpoint_carrying_a_chain_dir_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "endpoints.json"
            path.write_bytes(
                canonical_bytes(
                    {
                        "schema": ENDPOINT_LOCATOR_SCHEMA,
                        "endpoints": {
                            "control-r": {
                                "store_root": str(ABS_ROOT / "c"),
                                "baseline_chain_dir": str(ABS_ROOT / "c-chain"),
                            },
                            "static-rb": {
                                "store_root": str(ABS_ROOT / "s"),
                                "baseline_chain_dir": str(ABS_ROOT / "s-chain"),
                            },
                            "treatment-rb": {
                                "store_root": str(ABS_ROOT / "t"),
                                "baseline_chain_dir": str(ABS_ROOT / "t-chain"),
                            },
                            "g896": {"store_root": str(ABS_ROOT / "g")},
                        },
                    }
                )
            )
            with self.assertRaises(M2PanelError):
                load_endpoint_locator(path)

    def test_the_slot_locator_chain_dir_rule_follows_the_pool_arm(self):
        slots = loaded_slots()
        locator = plain_slot_locator()
        # No own-run slot: every entry must be a bare store root, on any arm.
        validate_slot_chain_dirs("treatment-rb", slots, locator, hash_tag(0x999))
        # Slot 5 now names the pool arm's own run.
        slots[5]["source_run_sha256"] = hash_tag(0x999)
        with self.assertRaises(M2PanelError):
            validate_slot_chain_dirs("treatment-rb", slots, locator, hash_tag(0x999))
        validate_slot_chain_dirs("control-r", slots, locator, hash_tag(0x999))
        locator[5] = SlotLocation(ABS_ROOT / "arm-store", ABS_ROOT / "arm-chain")
        validate_slot_chain_dirs("treatment-rb", slots, locator, hash_tag(0x999))
        with self.assertRaises(M2PanelError):
            validate_slot_chain_dirs("control-r", slots, locator, hash_tag(0x999))

    def test_the_slot_locator_requires_absolute_paths_and_eight_slots(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "locator.json"
            path.write_bytes(
                canonical_bytes(
                    {
                        "schema": SLOT_LOCATOR_SCHEMA,
                        "stores": {str(index): str(ABS_ROOT / f"s{index}") for index in range(8)},
                    }
                )
            )
            self.assertEqual(len(load_slot_locator(path)), SLOT_COUNT)
            path.unlink()
            path.write_bytes(
                canonical_bytes(
                    {
                        "schema": SLOT_LOCATOR_SCHEMA,
                        "stores": {str(index): str(ABS_ROOT / f"s{index}") for index in range(7)},
                    }
                )
            )
            with self.assertRaises(M2PanelError):
                load_slot_locator(path)


class EnvironmentAndDryRunTests(unittest.TestCase):
    def test_every_environment_key_the_runner_sets_is_also_scrubbed(self):
        slots = loaded_slots()
        locator = plain_slot_locator()
        endpoints = endpoint_locations()
        specs = build_matchup_specs([2] * SLOT_COUNT)
        emitted: set[str] = set()
        for spec in specs:
            emitted.update(
                matchup_environment(
                    slots, locator, endpoints, spec, Path("out") / "outcome.json"
                )
            )
        self.assertEqual(emitted - set(H2H_ENVIRONMENT_KEYS), set())

    def test_a_v4_endpoint_carries_its_chain_dir_and_a_v3_one_does_not(self):
        slots = loaded_slots()
        locator = plain_slot_locator()
        endpoints = endpoint_locations()
        specs = build_matchup_specs([2] * SLOT_COUNT)
        treatment = next(spec for spec in specs if spec.endpoint_id == "treatment-rb")
        control = next(spec for spec in specs if spec.endpoint_id == "control-r")
        treatment_env = matchup_environment(
            slots, locator, endpoints, treatment, Path("o.json")
        )
        control_env = matchup_environment(slots, locator, endpoints, control, Path("o.json"))
        self.assertIn("H2H_CANDIDATE_CHAIN_DIR", treatment_env)
        self.assertNotIn("H2H_CANDIDATE_CHAIN_DIR", control_env)
        self.assertNotIn("H2H_OPPONENT_CHAIN_DIR", treatment_env)
        self.assertEqual(treatment_env["H2H_CANDIDATE_USE_STORE_RUN"], "1")
        self.assertEqual(treatment_env["H2H_ENVIRONMENT_RANDOMIZATION_V2"], "1")
        self.assertEqual(
            treatment_env["H2H_CANDIDATE_GEN"], str(ARM_ENDPOINT_STORE_GENERATION)
        )

    def test_the_dry_run_is_deterministic_and_touches_nothing(self):
        slots = loaded_slots()
        locator = plain_slot_locator()
        endpoints = endpoint_locations()
        specs = build_matchup_specs([2] * SLOT_COUNT)
        first = render_dry_run_lines(
            specs, slots, locator, endpoints, Path("exe"), ABS_ROOT / "out"
        )
        second = render_dry_run_lines(
            specs, slots, locator, endpoints, Path("exe"), ABS_ROOT / "out"
        )
        self.assertEqual(first, second)
        self.assertEqual(len(first), len(specs))
        self.assertIn("seed=", first[0])

    def test_outcome_paths_are_per_endpoint_and_per_slot(self):
        specs = build_matchup_specs([2] * SLOT_COUNT)
        paths = {matchup_outcome_path(ABS_ROOT / "out", spec) for spec in specs}
        self.assertEqual(len(paths), len(specs))


class CliTests(unittest.TestCase):
    def _base_args(self, tmp: str, **overrides: str) -> list[str]:
        values = {
            "--genesis-manifest": str(Path(tmp) / "refresh-00.manifest.json"),
            "--slot-locator": str(Path(tmp) / "locator.json"),
            "--endpoint-locator": str(Path(tmp) / "endpoints.json"),
            "--pool-arm": "control-r",
            "--output-dir": str(Path(tmp) / "out"),
            "--executable": str(Path(tmp) / "fake-exe"),
            "--repo-root": tmp,
        }
        values.update(overrides)
        args: list[str] = []
        for key, value in values.items():
            args.extend([key, value])
        return args

    def test_a_non_pinned_root_count_is_refused_outside_dry_run(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(SystemExit) as context:
                with contextlib.redirect_stderr(io.StringIO()):
                    parse_args(self._base_args(tmp, **{"--root-count": "16"}))
            self.assertEqual(context.exception.code, 2)

    def test_a_non_pinned_root_count_is_allowed_under_dry_run(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = parse_args(self._base_args(tmp, **{"--root-count": "16"}) + ["--dry-run"])
            self.assertEqual(args.root_count, 16)
            self.assertTrue(args.dry_run)

    def test_the_default_root_count_is_the_ratified_one(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = parse_args(self._base_args(tmp))
            self.assertEqual(args.root_count, ROOT_COUNT)

    def test_an_unknown_pool_arm_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(SystemExit):
                with contextlib.redirect_stderr(io.StringIO()):
                    parse_args(self._base_args(tmp, **{"--pool-arm": "control"}))


class SeedBandTests(unittest.TestCase):
    def test_the_m2_band_is_disjoint_from_the_training_and_panel_bands(self):
        training_seeds = (978_000, 979_000, 980_000)
        # The payoff panel band: base 4_100_000_000, stride 32,000,000 per
        # refresh through refresh 16, each panel consuming 28 matchups x
        # 1,000,000.
        panel_high = 4_100_000_000 + 16 * 32_000_000 + 28 * 1_000_000
        m2_low = M2_COMMON_ROOT_BASE_SEED_V1
        m2_high = M2_COMMON_ROOT_BASE_SEED_V1 + SLOT_COUNT * M2_OPPONENT_SEED_STRIDE_V1
        self.assertGreater(m2_low, panel_high)
        for seed in training_seeds:
            self.assertFalse(m2_low <= seed <= m2_high)


if __name__ == "__main__":
    unittest.main()
