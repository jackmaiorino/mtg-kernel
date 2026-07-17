from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from mtg_kernel_rl.client import EnvProcessError, KernelRlClient, ProtocolError, strict_json_loads

from fixtures import DECK_HASHES, DECK_IDS, fake_launcher


class ClientStrictnessTest(unittest.TestCase):
    def make_client(self, scenario: str, timeout_s: float = 1.0) -> KernelRlClient:
        self.tmp = tempfile.TemporaryDirectory()
        launcher = fake_launcher(Path(self.tmp.name), scenario)
        return KernelRlClient(launcher, timeout_s=timeout_s)

    def assert_reset_protocol_error(self, scenario: str) -> None:
        client = self.make_client(scenario, timeout_s=0.2 if scenario == "timeout" else 1.0)
        try:
            with self.assertRaises((ProtocolError, EnvProcessError)):
                client.reset(episode_id=0, env_seed=1, max_physical_decisions=8, max_policy_steps=16)
        finally:
            client.close()
            self.tmp.cleanup()

    def test_duplicate_keys_rejected(self) -> None:
        with self.assertRaises(ProtocolError):
            strict_json_loads('{"a":1,"a":2}')
        self.assert_reset_protocol_error("duplicate_keys")

    def test_nonfinite_json_rejected(self) -> None:
        with self.assertRaises(ProtocolError):
            strict_json_loads('{"a":1e999}')
        self.assert_reset_protocol_error("nonfinite_json")
        self.assert_reset_protocol_error("nonfinite_overflow")

    def test_error_response_schema_and_sanitized_message(self) -> None:
        client = self.make_client("error_valid")
        try:
            with self.assertRaises(ProtocolError) as cm:
                client.reset(episode_id=0, env_seed=1, max_physical_decisions=8, max_policy_steps=16)
            self.assertIn("environment error bad_request", str(cm.exception))
            self.assertNotIn("\n", str(cm.exception))
        finally:
            client.close()
            self.tmp.cleanup()
        for scenario in ("error_legacy_v3", "error_legacy_v4", "error_bad_schema", "error_bad_request_id", "error_empty_code"):
            self.assert_reset_protocol_error(scenario)

    def test_stdout_noise_rejected(self) -> None:
        self.assert_reset_protocol_error("noise")

    def test_timeout_rejected_and_cleanup_is_idempotent(self) -> None:
        client = self.make_client("timeout", timeout_s=0.1)
        with self.assertRaises(EnvProcessError):
            client.reset(episode_id=0, env_seed=1, max_physical_decisions=8, max_policy_steps=16)
        client.close()
        client.close()
        self.tmp.cleanup()

    def test_eof_nonzero_rejected(self) -> None:
        self.assert_reset_protocol_error("eof_nonzero")

    def test_extra_and_missing_fields_rejected(self) -> None:
        self.assert_reset_protocol_error("extra_field")
        self.assert_reset_protocol_error("missing_field")

    def test_bool_as_int_rejected(self) -> None:
        self.assert_reset_protocol_error("bool_int")
        self.assert_reset_protocol_error("u64_overflow")
        self.assert_reset_protocol_error("substep_index_u32_overflow")
        self.assert_reset_protocol_error("substep_count_u32_overflow")

    def test_immediate_v4_wire_surfaces_and_privileged_hash_fields_are_rejected(self) -> None:
        for scenario in (
            "decision_legacy_v4",
            "provenance_legacy_v4",
            "observation_legacy_v4",
            "action_legacy_v4",
            "wire_environment_hash",
            "observation_environment_hash",
            "provenance_environment_hash_algorithm",
        ):
            with self.subTest(scenario=scenario):
                self.assert_reset_protocol_error(scenario)

    def test_episode_and_step_drift_rejected(self) -> None:
        self.assert_reset_protocol_error("episode_drift")
        self.assert_reset_protocol_error("step_drift")

    def test_deck_identity_is_required_pinned_and_stable(self) -> None:
        self.assert_reset_protocol_error("deck_id_drift")
        self.assert_reset_protocol_error("deck_hash_shape")
        client = self.make_client("deck_hash_drift")
        try:
            decision = client.reset(episode_id=0, env_seed=1, max_physical_decisions=8, max_policy_steps=16)
            self.assertEqual(decision.deck_ids, DECK_IDS)
            self.assertEqual(decision.deck_hashes, DECK_HASHES)
            action = decision.legal_actions[0]
            with self.assertRaisesRegex(ProtocolError, "deck_hashes drift"):
                client.step(action["selected_index"], action["stable_id"])
        finally:
            client.close()
            self.tmp.cleanup()

    def test_reset_validates_and_sends_exact_ordered_deck_ids(self) -> None:
        client = self.make_client("valid")
        try:
            with self.assertRaisesRegex(ProtocolError, "two-item tuple"):
                client.reset(episode_id=0, env_seed=1, max_physical_decisions=8, max_policy_steps=16, deck_ids=["Burn", "Burn"])  # type: ignore[arg-type]
            with self.assertRaisesRegex(ProtocolError, "must be nonempty"):
                client.reset(episode_id=0, env_seed=1, max_physical_decisions=8, max_policy_steps=16, deck_ids=("Burn", ""))
            with self.assertRaisesRegex(ProtocolError, "unsupported_deck"):
                client.reset(episode_id=0, env_seed=1, max_physical_decisions=8, max_policy_steps=16, deck_ids=("Rally", "Burn"))
            decision = client.reset(episode_id=0, env_seed=1, max_physical_decisions=8, max_policy_steps=16)
            self.assertEqual(decision.deck_ids, ("Burn", "Burn"))
        finally:
            client.close()
            self.tmp.cleanup()

    def test_legal_action_integrity_rejected(self) -> None:
        for scenario in ("empty_actions", "noncontiguous_actions", "duplicate_actions", "mismatched_action_actor", "mixed_action_actors"):
            self.assert_reset_protocol_error(scenario)

    def test_nonzero_intermediate_reward_rejected(self) -> None:
        self.assert_reset_protocol_error("nonzero_reward")

    def test_provenance_drift_rejected(self) -> None:
        client = self.make_client("provenance_drift")
        try:
            decision = client.reset(episode_id=0, env_seed=1, max_physical_decisions=8, max_policy_steps=16)
            action = decision.legal_actions[0]
            with self.assertRaises(ProtocolError):
                client.step(action["selected_index"], action["stable_id"])
        finally:
            client.close()
            self.tmp.cleanup()

    def test_invalid_halted_and_truncated_terminal_rejected(self) -> None:
        for scenario in ("invalid_terminal", "halted_terminal", "truncated_terminal", "terminal_jump"):
            client = self.make_client(scenario)
            try:
                decision = client.reset(episode_id=0, env_seed=1, max_physical_decisions=8, max_policy_steps=16)
                action = decision.legal_actions[0]
                with self.assertRaises(ProtocolError):
                    client.step(action["selected_index"], action["stable_id"])
            finally:
                client.close()
                self.tmp.cleanup()

    def test_selected_index_overflow_rejected_before_step(self) -> None:
        client = self.make_client("valid")
        try:
            client.reset(episode_id=0, env_seed=1, max_physical_decisions=8, max_policy_steps=16)
            with self.assertRaises(ProtocolError):
                client.step(4_294_967_296, "legal-action-v5:a")
        finally:
            client.close()
            self.tmp.cleanup()

    def test_combat_group_transcript_is_contiguous_and_causally_tracks_includes(self) -> None:
        client = self.make_client("combat_valid")
        try:
            decision = client.reset(
                episode_id=0,
                env_seed=1,
                max_physical_decisions=8,
                max_policy_steps=16,
            )
            self.assertEqual(
                (decision.physical_decision_id, decision.substep_index, decision.substep_count),
                (0, 0, 3),
            )
            first_candidate = decision.observation["projection"]["policy_surface_context"][
                "private_combat_selection"
            ]["current_candidate"]
            decision = client.step(1, decision.legal_actions[1]["stable_id"])
            private = decision.observation["projection"]["policy_surface_context"]["private_combat_selection"]
            self.assertEqual(private["selected"], [first_candidate])
            decision = client.step(0, decision.legal_actions[0]["stable_id"])
            private = decision.observation["projection"]["policy_surface_context"]["private_combat_selection"]
            self.assertEqual(len(private["selected"]), 1)
            terminal = client.step(1, decision.legal_actions[1]["stable_id"])
            self.assertEqual((terminal.policy_step_count, terminal.physical_decision_count), (3, 1))
        finally:
            client.close()
            self.tmp.cleanup()

    def test_combat_group_rejects_counter_actor_context_and_candidate_drift(self) -> None:
        for scenario in (
            "combat_actor_drift",
            "combat_attacker_drift",
            "combat_count_drift",
            "combat_current_drift",
            "combat_id_drift",
            "combat_index_drift",
            "combat_stage_drift",
            "combat_suffix_drift",
            "combat_terminal_midgroup",
        ):
            with self.subTest(scenario=scenario):
                client = self.make_client(scenario)
                try:
                    decision = client.reset(
                        episode_id=0,
                        env_seed=1,
                        max_physical_decisions=8,
                        max_policy_steps=16,
                    )
                    with self.assertRaises(ProtocolError):
                        client.step(0, decision.legal_actions[0]["stable_id"])
                finally:
                    client.close()
                    self.tmp.cleanup()

    def test_combat_group_rejects_forged_selected_history_and_order(self) -> None:
        client = self.make_client("combat_selected_history_drift")
        try:
            decision = client.reset(
                episode_id=0,
                env_seed=1,
                max_physical_decisions=8,
                max_policy_steps=16,
            )
            with self.assertRaisesRegex(ProtocolError, "selected history drift"):
                client.step(1, decision.legal_actions[1]["stable_id"])
        finally:
            client.close()
            self.tmp.cleanup()

        client = self.make_client("combat_selected_order_drift")
        try:
            decision = client.reset(
                episode_id=0,
                env_seed=1,
                max_physical_decisions=8,
                max_policy_steps=16,
            )
            decision = client.step(1, decision.legal_actions[1]["stable_id"])
            with self.assertRaisesRegex(ProtocolError, "selected history drift"):
                client.step(1, decision.legal_actions[1]["stable_id"])
        finally:
            client.close()
            self.tmp.cleanup()

    def test_natural_terminal_is_admitted_and_sequential_reset_supported(self) -> None:
        client = self.make_client("valid")
        try:
            decision = client.reset(episode_id=0, env_seed=1, max_physical_decisions=8, max_policy_steps=16)
            terminal = client.step(decision.legal_actions[0]["selected_index"], decision.legal_actions[0]["stable_id"])
            self.assertEqual(terminal.terminal_outcome, "p0_win")
            self.assertEqual(terminal.deck_ids, DECK_IDS)
            self.assertEqual(terminal.deck_hashes, DECK_HASHES)
            decision2 = client.reset(episode_id=1, env_seed=2, max_physical_decisions=8, max_policy_steps=16)
            self.assertEqual(decision2.episode_id, 1)
            self.assertEqual(decision2.step, 0)
            self.assertEqual(decision2.deck_ids, DECK_IDS)
            self.assertEqual(decision2.deck_hashes, DECK_HASHES)
        finally:
            client.close()
            self.tmp.cleanup()


if __name__ == "__main__":
    unittest.main()
