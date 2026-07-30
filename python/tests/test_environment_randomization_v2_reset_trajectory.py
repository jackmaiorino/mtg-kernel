"""Focused unittest for the portable environment-randomization-v2 reset
physical-trajectory goldens.

Frozen contract: collab TO-CLAUDE.md, "CODEX RULING: EXACT RESET-GOLDEN JSON
AND SEMANTIC BYTES" plus the recovery checkpoint's second-pass corrections.

Every positive witness pinned here is recomputed directly from the imported
stdlib KDF reference and the runtime catalog, outside the generic artifact
builder, so a builder bug cannot make these assertions vacuous.
"""

from __future__ import annotations

import sys

# Set before the generator/reference imports so this focused unittest leaves no
# __pycache__ artifact behind.
sys.dont_write_bytecode = True

import hashlib  # noqa: E402
import importlib.util  # noqa: E402
import json  # noqa: E402
import unittest  # noqa: E402
from pathlib import Path  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]
GENERATOR_PATH = (
    REPO_ROOT
    / "python"
    / "tools"
    / "generate_environment_randomization_v2_reset_physical_trajectory_goldens_v1.py"
)
REFERENCE_PATH = (
    REPO_ROOT / "python" / "tools" / "environment_randomization_v2_reference.py"
)
ARTIFACT_PATH = (
    REPO_ROOT
    / "data"
    / "environment_randomization_v2"
    / "reset_physical_trajectory_goldens_v1.json"
)
RUNTIME_DECKS_PATH = REPO_ROOT / "data" / "runtime_decks_v1.json"

# Independently reproduced pins. Normal generation must yield exactly these.
ARTIFACT_BYTES = 27041
ARTIFACT_SHA256 = "ab002901a598d40732d39f9b0f21abaa2b7445e63b1c14d45a44b7900f6b739b"
STREAM_BYTES = 27712
STREAM_SHA256 = "15d312141f8d96f079684dd64b58b5bab803086a78ac9687e3c14aab91e0a3c9"

NATIVE_PAIR_ROOT = 5_293_664_275_683_392_565
# The same derivation with the schedule identity wrongly substituted for the
# version atom. Pinned to keep the two bindings distinguishable.
WRONG_VERSION_ATOM_ROOT = 3_926_161_255_480_587_309

NATIVE_CASE = "burn-rally-native-base-71501-pair-0"
ROOT_940001_CASE = "burn-rally-root-940001"

# Explicit seed pairs, outside the artifact builder.
EXPECTED_SEEDS = {
    NATIVE_CASE: (11_912_044_731_856_030_231, 1_508_577_839_932_723_876),
    ROOT_940001_CASE: (7_479_945_427_805_527_300, 17_206_394_138_497_251_163),
}
# Explicit opening-hand pairs, outside the artifact builder.
EXPECTED_HANDS = {
    NATIVE_CASE: (
        [76, 47, 66, 127, 76, 51, 66],
        [16, 48, 127, 93, 76, 48, 66],
    ),
    ROOT_940001_CASE: (
        [47, 37, 51, 47, 51, 76, 36],
        [16, 48, 10, 27, 30, 10, 76],
    ),
}

# All fourteen root-940001 draw witnesses, written out literally:
# (global_event_ordinal, owner_draw_ordinal, owner, card, hand_after, lib_after)
ROOT_940001_DRAW_WITNESSES = (
    (0, 0, "p0", 47, 1, 59),
    (1, 0, "p1", 16, 1, 59),
    (2, 1, "p0", 37, 2, 58),
    (3, 1, "p1", 48, 2, 58),
    (4, 2, "p0", 51, 3, 57),
    (5, 2, "p1", 10, 3, 57),
    (6, 3, "p0", 47, 4, 56),
    (7, 3, "p1", 27, 4, 56),
    (8, 4, "p0", 51, 5, 55),
    (9, 4, "p1", 30, 5, 55),
    (10, 5, "p0", 76, 6, 54),
    (11, 5, "p1", 10, 6, 54),
    (12, 6, "p0", 36, 7, 53),
    (13, 6, "p1", 76, 7, 53),
)


def _import_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


GENERATOR = _import_module("reset_trajectory_generator_under_test", GENERATOR_PATH)
REFERENCE = _import_module("reset_trajectory_reference_under_test", REFERENCE_PATH)
ContractError = GENERATOR.ContractError


def _source_deck(deck_id: str) -> list[int]:
    catalog = json.loads(RUNTIME_DECKS_PATH.read_text())
    for deck in catalog["decks"]:
        if deck["id"] == deck_id:
            return [copy["card_id"] for copy in deck["materialized_mainboard"]]
    raise AssertionError(f"deck {deck_id!r} absent from the catalog")


class ArtifactBytesTest(unittest.TestCase):
    def test_raw_file_size_and_sha256_are_pinned(self) -> None:
        raw = ARTIFACT_PATH.read_bytes()
        self.assertEqual(len(raw), ARTIFACT_BYTES)
        self.assertEqual(hashlib.sha256(raw).hexdigest(), ARTIFACT_SHA256)

    def test_canonical_byte_shape(self) -> None:
        raw = ARTIFACT_PATH.read_bytes()
        self.assertNotIn(b"\r", raw)
        self.assertFalse(raw.startswith(b"\xef\xbb\xbf"))
        self.assertTrue(raw.endswith(b"\n"))
        self.assertEqual(raw.count(b"\n"), 1)
        self.assertLessEqual(len(raw), 1024 * 1024)

    def test_generator_reproduces_the_artifact_byte_for_byte(self) -> None:
        rebuilt = GENERATOR.canonical_file_bytes(GENERATOR.build_artifact())
        self.assertEqual(rebuilt, ARTIFACT_PATH.read_bytes())

    def test_semantic_stream_size_and_sha256_are_pinned(self) -> None:
        artifact = json.loads(ARTIFACT_PATH.read_text())
        stream = GENERATOR.portable_semantic_stream(artifact)
        self.assertEqual(len(stream), STREAM_BYTES)
        self.assertEqual(hashlib.sha256(stream).hexdigest(), STREAM_SHA256)


class IndependentWitnessTest(unittest.TestCase):
    """Recomputes every positive witness straight from the KDF reference."""

    def setUp(self) -> None:
        self.artifact = json.loads(ARTIFACT_PATH.read_text())
        self.cases = {case["name"]: case for case in self.artifact["reset_cases"]}
        self.burn = _source_deck("Burn")
        self.rally = _source_deck("Rally")

    def test_native_pair_root_uses_the_trainer_version_atom(self) -> None:
        self.assertEqual(GENERATOR.derive_train_env_root(71_501, 0), NATIVE_PAIR_ROOT)
        self.assertEqual(
            self.cases[NATIVE_CASE]["input"]["pair_environment_seed"], NATIVE_PAIR_ROOT
        )

    def test_schedule_identity_is_not_the_version_atom(self) -> None:
        hasher = hashlib.sha256()
        hasher.update(
            GENERATOR.trainer_atom("version", GENERATOR.NATIVE_SCHEDULE_IDENTITY.encode())
        )
        hasher.update(GENERATOR.trainer_atom("namespace", b"train-env"))
        hasher.update(GENERATOR.trainer_atom("field-name", b"base_seed"))
        hasher.update(GENERATOR.trainer_atom("u63", (71_501).to_bytes(8, "big")))
        hasher.update(GENERATOR.trainer_atom("field-name", b"pair_index"))
        hasher.update(GENERATOR.trainer_atom("u63", (0).to_bytes(8, "big")))
        wrong = int.from_bytes(hasher.digest()[:8], "big") & ((1 << 63) - 1)
        self.assertEqual(wrong, WRONG_VERSION_ATOM_ROOT)
        self.assertNotEqual(wrong, NATIVE_PAIR_ROOT)

    def test_derived_initial_seed_pairs(self) -> None:
        for name, (expected_p0, expected_p1) in EXPECTED_SEEDS.items():
            root = self.cases[name]["input"]["pair_environment_seed"]
            observed_p0 = REFERENCE.derive_seed(root, "p0", "initial-library-shuffle", 0)
            observed_p1 = REFERENCE.derive_seed(root, "p1", "initial-library-shuffle", 0)
            self.assertEqual(observed_p0, expected_p0, name)
            self.assertEqual(observed_p1, expected_p1, name)
            projection = self.cases[name]["expected_projection"]
            self.assertEqual(projection["p0"]["derived_initial_seed"], expected_p0, name)
            self.assertEqual(projection["p1"]["derived_initial_seed"], expected_p1, name)

    def test_opening_hand_pairs(self) -> None:
        for name, (expected_p0, expected_p1) in EXPECTED_HANDS.items():
            root = self.cases[name]["input"]["pair_environment_seed"]
            for owner, deck, expected in (
                ("p0", self.burn, expected_p0),
                ("p1", self.rally, expected_p1),
            ):
                seed = REFERENCE.derive_seed(root, owner, "initial-library-shuffle", 0)
                permutation = REFERENCE.permutation_for(seed, list(deck))
                self.assertEqual(permutation[:7], expected, f"{name} {owner}")
                stored = self.cases[name]["expected_projection"][owner]
                self.assertEqual(
                    stored["opening_hand_card_definition_ids"], expected, f"{name} {owner}"
                )
                self.assertEqual(
                    stored["remaining_library_card_definition_ids"],
                    permutation[7:],
                    f"{name} {owner}",
                )
                self.assertEqual(len(stored["remaining_library_card_definition_ids"]), 53)

    def test_source_index_permutation_is_a_bijection_and_projects_exactly(self) -> None:
        for name, case in self.cases.items():
            for owner, deck in (("p0", self.burn), ("p1", self.rally)):
                stored = case["expected_projection"][owner]
                permutation = stored["source_index_permutation"]
                self.assertEqual(sorted(permutation), list(range(60)), f"{name} {owner}")
                projected = [deck[index] for index in permutation]
                self.assertEqual(
                    projected,
                    stored["card_definition_id_permutation"],
                    f"{name} {owner}",
                )

    def test_root_940001_fourteen_draw_witnesses(self) -> None:
        stored = self.cases[ROOT_940001_CASE]["expected_projection"]["draw_events"]
        self.assertEqual(len(stored), 14)
        observed = tuple(
            (
                event["global_event_ordinal"],
                event["owner_draw_ordinal"],
                event["physical_owner"],
                event["card_definition_id"],
                event["owner_hand_count_after"],
                event["owner_library_count_after"],
            )
            for event in stored
        )
        self.assertEqual(observed, ROOT_940001_DRAW_WITNESSES)

        # Recomputed independently: event 2r is P0 taking permutation index r,
        # event 2r+1 is P1 taking permutation index r.
        root = 940_001
        permutations = {}
        for owner, deck in (("p0", self.burn), ("p1", self.rally)):
            seed = REFERENCE.derive_seed(root, owner, "initial-library-shuffle", 0)
            permutations[owner] = REFERENCE.permutation_for(seed, list(deck))
        for ordinal, draw_round, owner, card, hand_after, lib_after in (
            ROOT_940001_DRAW_WITNESSES
        ):
            self.assertEqual(ordinal, 2 * draw_round + (0 if owner == "p0" else 1))
            self.assertEqual(permutations[owner][draw_round], card)
            self.assertEqual(hand_after, draw_round + 1)
            self.assertEqual(lib_after, 60 - (draw_round + 1))

    def test_next_live_ordinals_are_zero(self) -> None:
        for name, case in self.cases.items():
            self.assertEqual(
                case["expected_projection"]["next_live_shuffle_ordinals"], [0, 0], name
            )

    def test_deck_hashes_recompute_from_the_catalog(self) -> None:
        for deck_id, deck in (("Burn", self.burn), ("Rally", self.rally)):
            expected = f"{GENERATOR.fnv1a64_serde_json_u16_array(deck):016x}"
            for case in self.cases.values():
                for owner in ("p0", "p1"):
                    if case["input"][owner]["deck_id"] == deck_id:
                        self.assertEqual(
                            case["input"][owner]["runtime_deck_hash_u64_hex"], expected
                        )


class RejectDeltaProofTest(unittest.TestCase):
    """The exact-delta proof is the non-vacuity gate for all six rejects."""

    def setUp(self) -> None:
        self.artifact = json.loads(ARTIFACT_PATH.read_text())
        self.reset_cases = {c["name"]: c for c in self.artifact["reset_cases"]}
        self.paired_cases = {c["name"]: c for c in self.artifact["paired_role_cases"]}
        self.rejects = {c["name"]: c for c in self.artifact["reject_cases"]}
        self.reset_case_roots = {
            name: case["input"]["pair_environment_seed"]
            for name, case in self.reset_cases.items()
        }

    def _positive_body(self, kind: str) -> dict:
        if kind == "reset-projection":
            return GENERATOR.reset_body_of(self.reset_cases[ROOT_940001_CASE])
        return GENERATOR.paired_body_of(
            self.paired_cases["native-base-71501-pair-0-learner-role-swap"]
        )

    def test_all_six_frozen_deltas_are_proven_directly(self) -> None:
        self.assertEqual(len(self.rejects), 6)
        self.assertEqual(
            sorted(self.rejects), sorted(GENERATOR.FROZEN_REJECT_DELTAS)
        )
        for name, case in self.rejects.items():
            kind = case["input"]["kind"]
            observed = GENERATOR.prove_reject_delta(
                name, self._positive_body(kind), case["input"]["case"]
            )
            self.assertEqual(observed, GENERATOR.FROZEN_REJECT_DELTAS[name], name)

    def test_physical_deck_swap_has_exactly_two_permitted_paths(self) -> None:
        deltas = GENERATOR.FROZEN_REJECT_DELTAS[
            "paired-role-odd-physical-decks-swapped"
        ]
        self.assertEqual(len(deltas), 2)
        self.assertEqual(
            [delta[1][-1] for delta in deltas], ["p0_deck_id", "p1_deck_id"]
        )

    def test_one_injected_extra_leaf_is_rejected(self) -> None:
        for name, case in self.rejects.items():
            kind = case["input"]["kind"]
            body = json.loads(json.dumps(case["input"]["case"]))
            if kind == "reset-projection":
                permutation = body["expected_projection"]["p1"][
                    "source_index_permutation"
                ]
                # Self-relative so the injected leaf is always a real change.
                permutation[3] = (permutation[3] + 1) % 60
            else:
                body["input"]["even_episode"]["p1_deck_id"] = "Burn"
            with self.assertRaises(ContractError):
                GENERATOR.prove_reject_delta(name, self._positive_body(kind), body)

    def test_a_vacuous_reject_is_rejected(self) -> None:
        for kind, name in (
            ("reset-projection", "reset-source-permutation-duplicate-index"),
            ("paired-role", "paired-role-learner-seat-not-swapped"),
        ):
            with self.assertRaises(ContractError):
                GENERATOR.prove_reject_delta(
                    name, self._positive_body(kind), self._positive_body(kind)
                )

    def test_numerically_equal_float_substitution_is_rejected(self) -> None:
        """`36.0 == 36` in Python, so ordinary tuple equality accepts a float
        substitution. The type-exact comparator must reject it."""
        substitutions = (
            (
                "reset-source-permutation-duplicate-index",
                ("expected_projection", "p0", "source_index_permutation", 17),
                36.0,
            ),
            (
                "reset-source-permutation-index-out-of-range",
                ("expected_projection", "p0", "source_index_permutation", 0),
                60.0,
            ),
            (
                "reset-source-permutation-projection-mismatch",
                ("expected_projection", "p0", "card_definition_id_permutation", 0),
                37.0,
            ),
        )
        for name, path, float_value in substitutions:
            body = json.loads(json.dumps(self.rejects[name]["input"]["case"]))
            target = body
            for part in path[:-1]:
                target = target[part]
            self.assertEqual(target[path[-1]], int(float_value), name)
            target[path[-1]] = float_value

            observed = GENERATOR.body_leaf_deltas(
                self._positive_body("reset-projection"), body
            )
            expected = GENERATOR.FROZEN_REJECT_DELTAS[name]
            # Ordinary tuple equality would have accepted this body outright.
            self.assertEqual(observed, expected, name)
            # The type-exact comparator must not.
            self.assertFalse(
                GENERATOR.deltas_are_type_exact(observed, expected), name
            )
            with self.assertRaises(ContractError):
                GENERATOR.prove_reject_delta(
                    name, self._positive_body("reset-projection"), body
                )

    def test_path_component_types_are_compared(self) -> None:
        frozen = GENERATOR.FROZEN_REJECT_DELTAS[
            "reset-source-permutation-index-out-of-range"
        ]
        operation, path, old_value, new_value = frozen[0]
        # The trailing list index 0 stringified to "0" must not compare equal.
        stringified = tuple(str(part) for part in path)
        self.assertFalse(
            GENERATOR.deltas_are_type_exact(
                ((operation, stringified, old_value, new_value),), frozen
            )
        )
        self.assertTrue(GENERATOR.deltas_are_type_exact(frozen, frozen))

    def test_boolean_substitution_is_rejected(self) -> None:
        # bool is an int subclass, so 1 == True; the comparator must still
        # separate them.
        self.assertFalse(
            GENERATOR.deltas_are_type_exact(
                (("replace", ("a",), 0, True),),
                (("replace", ("a",), 0, 1),),
            )
        )

    def test_named_duplicate_proof_enforces_the_witness(self) -> None:
        """The duplicate witness is integrated into prove_reject_delta, so a
        body carrying the ruled leaf still fails when source copies 36 and 37
        are no longer the same card."""
        positive = GENERATOR.reset_body_of(self.reset_cases[ROOT_940001_CASE])
        reject = GENERATOR.reset_body_of(self.reset_cases[ROOT_940001_CASE])
        reject["expected_projection"]["p0"]["source_index_permutation"][17] = 36
        # Baseline: the ruled leaf alone is accepted.
        GENERATOR.prove_reject_delta(
            GENERATOR.DUPLICATE_INDEX_REJECT_NAME, positive, reject
        )

        # Break the 47/47 source-copy coincidence in both bodies so the delta
        # stays a single leaf while the witness precondition fails.
        positive_broken = GENERATOR.reset_body_of(self.reset_cases[ROOT_940001_CASE])
        positive_broken["input"]["p0"]["source_card_definition_ids"][37] = 99
        reject_broken = json.loads(json.dumps(positive_broken))
        reject_broken["expected_projection"]["p0"]["source_index_permutation"][17] = 36
        observed = GENERATOR.body_leaf_deltas(positive_broken, reject_broken)
        self.assertTrue(
            GENERATOR.deltas_are_type_exact(
                observed,
                GENERATOR.FROZEN_REJECT_DELTAS[GENERATOR.DUPLICATE_INDEX_REJECT_NAME],
            )
        )
        with self.assertRaises(ContractError):
            GENERATOR.prove_reject_delta(
                GENERATOR.DUPLICATE_INDEX_REJECT_NAME, positive_broken, reject_broken
            )

    def test_leaf_diff_reports_add_remove_and_replace(self) -> None:
        deltas = GENERATOR.body_leaf_deltas(
            {"keep": 1, "gone": 2, "deep": [1, 2, 3]},
            {"keep": 1, "new": 3, "deep": [1, 9]},
        )
        self.assertEqual(
            deltas,
            (
                ("replace", ("deep", 1), 2, 9),
                ("remove", ("deep", 2), 3, None),
                ("remove", ("gone",), 2, None),
                ("add", ("new",), None, 3),
            ),
        )

    def test_duplicate_index_witness_preserves_every_projected_card(self) -> None:
        positive = GENERATOR.reset_body_of(self.reset_cases[ROOT_940001_CASE])
        rejected = self.rejects["reset-source-permutation-duplicate-index"]["input"][
            "case"
        ]
        GENERATOR.prove_duplicate_index_witness(positive, rejected)

        positive_permutation = positive["expected_projection"]["p0"][
            "source_index_permutation"
        ]
        rejected_permutation = rejected["expected_projection"]["p0"][
            "source_index_permutation"
        ]
        source_cards = positive["input"]["p0"]["source_card_definition_ids"]
        self.assertEqual((positive_permutation[0], positive_permutation[17]), (36, 37))
        self.assertEqual((rejected_permutation[0], rejected_permutation[17]), (36, 36))
        self.assertEqual((source_cards[36], source_cards[37]), (47, 47))
        self.assertEqual(
            [source_cards[i] for i in positive_permutation],
            [source_cards[i] for i in rejected_permutation],
        )
        # The stored card projection bytes are untouched by this witness.
        self.assertEqual(
            rejected["expected_projection"]["p0"]["card_definition_id_permutation"],
            positive["expected_projection"]["p0"]["card_definition_id_permutation"],
        )
        self.assertNotEqual(sorted(rejected_permutation), list(range(60)))

    def test_duplicate_witness_guard_rejects_a_projection_disturbing_mutation(self) -> None:
        positive = GENERATOR.reset_body_of(self.reset_cases[ROOT_940001_CASE])
        disturbed = GENERATOR.reset_body_of(self.reset_cases[ROOT_940001_CASE])
        # Position 1 collides with position 0 but changes a projected card, so
        # it is not the ruled single-intent witness.
        disturbed["expected_projection"]["p0"]["source_index_permutation"][1] = 36
        with self.assertRaises(ContractError):
            GENERATOR.prove_duplicate_index_witness(positive, disturbed)


class RejectClassificationTest(unittest.TestCase):
    def setUp(self) -> None:
        self.artifact = json.loads(ARTIFACT_PATH.read_text())
        self.reset_case_roots = {
            case["name"]: case["input"]["pair_environment_seed"]
            for case in self.artifact["reset_cases"]
        }

    def test_every_stored_reject_classifies_to_its_code(self) -> None:
        expected_codes = {
            "paired-role-learner-seat-not-swapped": "learner-seat-rule-mismatch",
            "paired-role-odd-environment-seed-drift": "pair-environment-seed-mismatch",
            "paired-role-odd-physical-decks-swapped": "physical-deck-binding-mismatch",
            "reset-source-permutation-duplicate-index": "source-permutation-not-bijection",
            "reset-source-permutation-index-out-of-range": (
                "source-permutation-index-out-of-range"
            ),
            "reset-source-permutation-projection-mismatch": (
                "source-permutation-card-projection-mismatch"
            ),
        }
        for case in self.artifact["reject_cases"]:
            self.assertEqual(
                case["expected_rejection"], expected_codes[case["name"]], case["name"]
            )
            observed = GENERATOR.classify_reject(case["input"], self.reset_case_roots)
            self.assertEqual(observed, case["expected_rejection"], case["name"])

    def test_reject_input_is_the_adjacently_tagged_union(self) -> None:
        for case in self.artifact["reject_cases"]:
            self.assertEqual(set(case["input"]), {"kind", "case"})
            self.assertIn(case["input"]["kind"], {"reset-projection", "paired-role"})

    def test_positive_bodies_validate_clean(self) -> None:
        for case in self.artifact["reset_cases"]:
            body = {
                "input": case["input"],
                "expected_projection": case["expected_projection"],
            }
            self.assertIsNone(GENERATOR.validate_reset_body(body), case["name"])
        for case in self.artifact["paired_role_cases"]:
            body = {
                "input": case["input"],
                "expected_shared_reset_case_name": case[
                    "expected_shared_reset_case_name"
                ],
            }
            self.assertIsNone(
                GENERATOR.validate_paired_body(body, self.reset_case_roots),
                case["name"],
            )

    def test_range_is_validated_before_bijection(self) -> None:
        case = {c["name"]: c for c in self.artifact["reset_cases"]}[ROOT_940001_CASE]
        body = GENERATOR.reset_body_of(case)
        # 60 is simultaneously out of range and a bijection break; the frozen
        # precedence must report the range code.
        body["expected_projection"]["p0"]["source_index_permutation"][0] = 60
        self.assertEqual(
            GENERATOR.validate_reset_body(body),
            "source-permutation-index-out-of-range",
        )


class StrictStringTest(unittest.TestCase):
    def test_terminal_lf_is_rejected(self) -> None:
        with self.assertRaises(ContractError):
            GENERATOR.require_case_name("burn-rally-root-940001\n", "case name")
        with self.assertRaises(ContractError):
            GENERATOR.require_sha256_hex(ARTIFACT_SHA256 + "\n", "sha")
        with self.assertRaises(ContractError):
            GENERATOR.require_deck_hash_hex("5fdb7b92986b6fc1\n", "deck hash")

    def test_valid_strings_pass(self) -> None:
        GENERATOR.require_case_name("burn-rally-root-940001", "case name")
        GENERATOR.require_sha256_hex(ARTIFACT_SHA256, "sha")
        GENERATOR.require_deck_hash_hex("5fdb7b92986b6fc1", "deck hash")

    def test_wrong_case_and_type_are_rejected(self) -> None:
        for invalid in ("Burn-Rally", "-leading-dash", "", "a" * 129):
            with self.assertRaises(ContractError):
                GENERATOR.require_case_name(invalid, "case name")
        with self.assertRaises(ContractError):
            GENERATOR.require_case_name(None, "case name")
        with self.assertRaises(ContractError):
            GENERATOR.require_sha256_hex(ARTIFACT_SHA256.upper(), "sha")

    def test_non_ascii_deck_id_raises_contract_error(self) -> None:
        with self.assertRaises(ContractError):
            GENERATOR.require_printable_ascii_deck_id("Buürn")
        with self.assertRaises(ContractError):
            GENERATOR.require_printable_ascii_deck_id("")
        with self.assertRaises(ContractError):
            GENERATOR.require_printable_ascii_deck_id("a" * 65)
        with self.assertRaises(ContractError):
            GENERATOR.require_printable_ascii_deck_id("Burn\n")
        GENERATOR.require_printable_ascii_deck_id("Burn")


class StrictJsonTest(unittest.TestCase):
    def test_duplicate_keys_are_rejected_at_every_depth(self) -> None:
        with self.assertRaises(ContractError):
            GENERATOR.strict_json_loads(b'{"a":1,"a":2}', "probe")
        with self.assertRaises(ContractError):
            GENERATOR.strict_json_loads(b'{"outer":{"a":1,"a":2}}', "probe")

    def test_float_and_non_finite_literals_are_rejected(self) -> None:
        for payload in (b'{"a":1.0}', b'{"a":1e999}', b'{"a":NaN}', b'{"a":Infinity}'):
            with self.assertRaises(ContractError):
                GENERATOR.strict_json_loads(payload, "probe")

    def test_integers_and_strings_parse(self) -> None:
        self.assertEqual(
            GENERATOR.strict_json_loads(b'{"a":1,"b":"x"}', "probe"), {"a": 1, "b": "x"}
        )

    def test_carriage_return_and_non_ascii_are_rejected(self) -> None:
        with self.assertRaises(ContractError):
            GENERATOR.strict_json_loads(b'{"a":1}\r', "probe")
        with self.assertRaises(ContractError):
            GENERATOR.strict_json_loads(b'{"a":"\xc3\xbc"}', "probe")

    def test_exact_field_sets_reject_unknown_and_missing(self) -> None:
        with self.assertRaises(ContractError):
            GENERATOR.require_exact_fields({"a": 1, "b": 2}, ("a",), "probe")
        with self.assertRaises(ContractError):
            GENERATOR.require_exact_fields({"a": 1}, ("a", "b"), "probe")
        with self.assertRaises(ContractError):
            GENERATOR.require_exact_fields(["a"], ("a",), "probe")


class StrictIntegerAndCountTest(unittest.TestCase):
    def test_bool_and_float_counts_are_rejected(self) -> None:
        for invalid in (True, False, 60.0, "60", None):
            with self.assertRaises(ContractError):
                GENERATOR.require_non_bool_int(invalid, "count")
        self.assertEqual(GENERATOR.require_non_bool_int(60, "count"), 60)

    def test_each_wrong_exact_case_count_is_rejected(self) -> None:
        artifact = json.loads(ARTIFACT_PATH.read_text())
        GENERATOR.require_exact_case_counts(artifact)
        for key in ("reset_cases", "paired_role_cases", "reject_cases"):
            short = json.loads(json.dumps(artifact))
            short[key] = short[key][:-1]
            with self.assertRaises(ContractError):
                GENERATOR.require_exact_case_counts(short)
            long = json.loads(json.dumps(artifact))
            long[key] = long[key] + [long[key][-1]]
            with self.assertRaises(ContractError):
                GENERATOR.require_exact_case_counts(long)


class ArtifactShapeTest(unittest.TestCase):
    def setUp(self) -> None:
        self.artifact = json.loads(ARTIFACT_PATH.read_text())

    def test_identities_and_authorities(self) -> None:
        self.assertEqual(self.artifact["schema"], GENERATOR.SCHEMA)
        self.assertEqual(
            self.artifact["generator_identity"], GENERATOR.GENERATOR_IDENTITY
        )
        self.assertEqual(
            self.artifact["environment_randomization_identity"],
            "mtg-kernel-environment-randomization-sha256-v2",
        )
        self.assertEqual(
            self.artifact["physical_projection_identity"],
            GENERATOR.PHYSICAL_PROJECTION_IDENTITY,
        )
        self.assertEqual(
            self.artifact["portable_vector_stream_identity"],
            GENERATOR.PORTABLE_VECTOR_STREAM_IDENTITY,
        )
        schedule = self.artifact["source_authorities"]["native_trainer_schedule"]
        self.assertEqual(schedule["identity"], GENERATOR.NATIVE_SCHEDULE_IDENTITY)
        self.assertEqual(
            schedule["python_reference_seed_version"],
            "kernel-python-rl-trainer-sha256-v2",
        )

    def test_projection_contract_literals(self) -> None:
        contract = self.artifact["projection_contract"]
        self.assertEqual(contract["card_definition_domain"], "u16-runtime-card-definition-id")
        self.assertEqual(
            contract["source_copy_index_domain"],
            "zero-based-materialized-mainboard-index",
        )
        self.assertEqual(contract["library_order"], "index-zero-is-next-draw")
        self.assertEqual(contract["initial_shuffle_purpose"], "initial-library-shuffle")
        self.assertEqual(contract["initial_shuffle_ordinal"], 0)
        self.assertEqual(contract["opening_hand_count"], 7)
        self.assertEqual(contract["opening_draw_rounds"], 7)
        self.assertEqual(contract["opening_draw_order_per_round"], ["p0", "p1"])
        self.assertEqual(contract["live_ordinals_after_reset"], [0, 0])

    def test_case_names_are_unique_and_strictly_increasing(self) -> None:
        for key in ("reset_cases", "paired_role_cases", "reject_cases"):
            names = [case["name"] for case in self.artifact[key]]
            self.assertEqual(names, sorted(names), key)
            self.assertEqual(len(set(names)), len(names), key)
            for name in names:
                GENERATOR.require_case_name(name, key)

    def test_paired_case_binds_only_a_learner_role_swap(self) -> None:
        case = self.artifact["paired_role_cases"][0]
        self.assertEqual(case["name"], "native-base-71501-pair-0-learner-role-swap")
        self.assertEqual(case["expected_shared_reset_case_name"], NATIVE_CASE)
        paired_input = case["input"]
        self.assertEqual(paired_input["base_seed"], 71_501)
        self.assertEqual(paired_input["pair_index"], 0)
        even = paired_input["even_episode"]
        odd = paired_input["odd_episode"]
        self.assertEqual((even["episode_index"], odd["episode_index"]), (0, 1))
        self.assertEqual((even["learner_seat"], odd["learner_seat"]), ("p0", "p1"))
        self.assertEqual(
            even["pair_environment_seed"], odd["pair_environment_seed"]
        )
        self.assertEqual(even["pair_environment_seed"], NATIVE_PAIR_ROOT)
        # Only the learner role swaps; the physical deck inputs never do.
        for episode in (even, odd):
            self.assertEqual(episode["p0_deck_id"], "Burn")
            self.assertEqual(episode["p1_deck_id"], "Rally")

    def test_reset_cases_bind_burn_to_p0_and_rally_to_p1(self) -> None:
        for case in self.artifact["reset_cases"]:
            self.assertEqual(case["input"]["p0"]["deck_id"], "Burn")
            self.assertEqual(case["input"]["p1"]["deck_id"], "Rally")
            self.assertEqual(case["input"]["p0"]["physical_owner"], "p0")
            self.assertEqual(case["input"]["p1"]["physical_owner"], "p1")


if __name__ == "__main__":
    unittest.main()
