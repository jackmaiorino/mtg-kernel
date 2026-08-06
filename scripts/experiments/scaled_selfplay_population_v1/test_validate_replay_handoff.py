from __future__ import annotations

import copy
import unittest

from validate_replay_handoff import (
    EXPECTED_LINEAGES,
    INPUT_SCHEMA,
    PROGRAM_COMMIT,
    PROGRAM_DOCUMENT_SHA256,
    RETEST_DISPOSITION,
    RETEST_FORMAL_MANIFEST_PATH,
    RETEST_FORMAL_MANIFEST_SHA256,
    SEEDS,
    canonical_output,
    validate_manifest,
)


def valid_manifest() -> dict:
    authorities = {
        "program_commit": PROGRAM_COMMIT,
        "program_document_sha256": PROGRAM_DOCUMENT_SHA256,
        "retest_formal_manifest_path": RETEST_FORMAL_MANIFEST_PATH,
        "retest_formal_manifest_sha256": RETEST_FORMAL_MANIFEST_SHA256,
        "retest_disposition": RETEST_DISPOSITION,
    }
    return {
        "schema": INPUT_SCHEMA,
        "global_target_generation": 1536,
        "replay_end_generation": 512,
        "program_updates": 1024,
        "terminal_outcomes_read": False,
        "authorities": authorities,
        "lineages": [
            {
                "seed": seed,
                "expected": copy.deepcopy(EXPECTED_LINEAGES[seed]),
                "observed": copy.deepcopy(EXPECTED_LINEAGES[seed]),
            }
            for seed in SEEDS
        ],
    }


class ReplayHandoffValidationTests(unittest.TestCase):
    def test_valid_manifest_advances(self) -> None:
        result = validate_manifest(valid_manifest())
        self.assertEqual(result["disposition"], "ADVANCE")
        self.assertTrue(result["continuation_authorized"])
        self.assertEqual(result["seeds"], list(SEEDS))
        self.assertNotIn("errors", result)

    def test_output_is_canonical_and_concise(self) -> None:
        result = validate_manifest(valid_manifest())
        encoded = canonical_output(result)
        self.assertEqual(encoded, canonical_output(result))
        self.assertNotIn(" ", encoded)
        self.assertEqual(encoded, '{"continuation_authorized":true,"disposition":"ADVANCE",'
                         '"global_target_generation":1536,"program_updates":1024,'
                         '"replay_end_generation":512,"schema":"mtg-kernel-scaled-selfplay-replay-handoff-validation/v1",'
                         '"seeds":[970001,970002,970003]}')

    def test_outcome_payload_is_not_parsed(self) -> None:
        manifest = valid_manifest()
        manifest["terminal_outcomes"] = {"not_a_supported_outcome": object()}
        result = validate_manifest(manifest)
        self.assertEqual(result["disposition"], "ADVANCE")

    def test_corruption_matrix_fails_closed(self) -> None:
        mutations = [
            ("schema", lambda m: m.update(schema="wrong")),
            ("global target", lambda m: m.update(global_target_generation=1535)),
            ("replay end", lambda m: m.update(replay_end_generation=511)),
            ("program updates", lambda m: m.update(program_updates=1023)),
            ("outcome read flag", lambda m: m.update(terminal_outcomes_read=True)),
            ("program authority", lambda m: m["authorities"].update(program_commit="0" * 40)),
            ("retest authority", lambda m: m["authorities"].update(retest_formal_manifest_sha256="0" * 64)),
            ("missing seed", lambda m: m["lineages"].pop()),
            ("extra seed", lambda m: m["lineages"].append(copy.deepcopy(m["lineages"][0]))),
            ("store tree", lambda m: m["lineages"][0]["observed"].update(store_tree_sha256="0" * 64)),
            ("checkpoint", lambda m: m["lineages"][1]["observed"].update(checkpoint_sha256="0" * 64)),
            ("sidecar", lambda m: m["lineages"][2]["observed"].update(sidecar_sha256="0" * 64)),
            ("native state", lambda m: m["lineages"][0]["observed"].update(native_state_sha256="0" * 64)),
            ("model digest", lambda m: m["lineages"][1]["observed"].update(model_parameter_sha256="0" * 64)),
            ("Adam step", lambda m: m["lineages"][2]["observed"].update(adam_step=511)),
            ("generation", lambda m: m["lineages"][0]["observed"].update(generation=511)),
            ("progress", lambda m: m["lineages"][1]["observed"]["progress"].update(next_episode_index=32767)),
        ]
        for name, mutate in mutations:
            with self.subTest(name=name):
                manifest = valid_manifest()
                mutate(manifest)
                result = validate_manifest(manifest)
                self.assertEqual(result["disposition"], "FAIL-INVESTIGATE")
                self.assertFalse(result["continuation_authorized"])
                self.assertTrue(result["errors"])

    def test_one_bad_lineage_blocks_all_three(self) -> None:
        manifest = valid_manifest()
        manifest["lineages"][1]["observed"]["native_state_sha256"] = "0" * 64
        result = validate_manifest(manifest)
        self.assertEqual(result["disposition"], "FAIL-INVESTIGATE")
        self.assertFalse(result["continuation_authorized"])
        self.assertTrue(any("lineages[1].observed.native_state_sha256" in error for error in result["errors"]))


if __name__ == "__main__":
    unittest.main()
