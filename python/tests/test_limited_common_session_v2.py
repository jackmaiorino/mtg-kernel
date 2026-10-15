from __future__ import annotations

from copy import deepcopy
from pathlib import Path
import sys
import tempfile
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
from limited_common_session_v2 import LimitedCommonClientV2
from limited_decks_v1 import parse_dck
from limited_session_v1 import LimitedSessionError

ENGINE = r'''
import copy, json, sys, time
mode = sys.argv[1]
generation = 0
prompt = 0
episode = 0
state = {"state_type": "unsupported", "episode_id": 0, "code": "no_session"}
snapshots = {}
for line in sys.stdin:
    request = json.loads(line)
    op = request["request_type"]
    reply = dict(protocol="kernel_limited_common_jsonl", schema_version=2,
                 request_id=request["request_id"], card_db_hash=123,
                 priority_mode="engine_windows_v1", combat_rules="foundations_v1",
                 mulligan_rules="london_v1", response_type="state")
    error = None
    if op == "reset":
        if request["expected_card_db_hash"] != 123:
            error = "catalog_mismatch"
        elif any(row["name"] == "Missing" for d in request["decks"] for row in d["cards"]):
            error = "unsupported_deck"
        else:
            episode = request["episode_id"]
            generation += 1
            prompt += 1
            snapshots.clear()
            state = dict(state_type="decision", episode_id=episode,
                         authority_generation=generation,
                         decision=dict(schema="mtg-kernel-limited-common-decision/v2",
                                       actor=request["starting_player"], prompt_seq=prompt,
                                       state={"own_hand": [], "public": {}},
                                       actions=[{"action_index": 0, "label": "Keep this hand"},
                                                {"action_index": 1, "label": "Mulligan"}]))
    elif op == "step":
        d = state["decision"]
        if (request["episode_id"], request["authority_generation"], request["actor"],
            request["prompt_seq"]) != (episode, generation, d["actor"], prompt):
            error = "stale_prompt"
        elif mode == "unsupported":
            state = dict(state_type="unsupported", episode_id=episode,
                         code="unsupported_common_input")
        else:
            state = dict(state_type="terminal", episode_id=episode, classification=mode
                         if mode in ("truncated", "halted") else "natural",
                         code="decision_cap" if mode == "truncated" else
                         ("fail_closed" if mode == "halted" else "natural_game_over"),
                         winner=None, policy_steps=1, physical_decisions=2)
    elif op == "snapshot":
        snapshots[len(snapshots) + 1] = copy.deepcopy(state)
        reply["snapshot_id"] = len(snapshots)
    elif op == "restore":
        state = copy.deepcopy(snapshots[request["snapshot_id"]])
        generation += 1
        prompt += 1
        state["authority_generation"] = generation
        state["decision"]["prompt_seq"] = prompt
    if error:
        reply.update(response_type="error", code=error)
    else:
        reply["state"] = copy.deepcopy(state)
    if mode == "timeout":
        time.sleep(2)
    if mode == "wrong-protocol":
        reply["protocol"] = "kernel_limited_jsonl"
    elif mode == "wrong-request":
        reply["request_id"] = "foreign"
    elif mode == "wrong-catalog":
        reply["card_db_hash"] = 124
    elif mode == "wrong-catalog-error":
        reply.update(card_db_hash=124, response_type="error", code="unsupported_deck")
    elif mode == "forged-terminal" and op == "step":
        reply["state"]["code"] = "fail_closed"
    elif mode == "bad-actor":
        reply["state"]["decision"]["actor"] = "A"
    elif mode == "bad-index":
        reply["state"]["decision"]["actions"][1]["action_index"] = 0
    elif mode == "wrong-schema":
        reply["state"]["decision"]["schema"] = "old/v1"
    elif mode == "duplicate":
        print('{"protocol":"a","protocol":"b"}', flush=True)
        continue
    print(json.dumps(reply), flush=True)
'''


class LimitedCommonClientV2Test(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.engine = Path(self.temp.name) / "engine.py"
        self.engine.write_text(ENGINE, encoding="utf-8")
        self.decks = (parse_dck("20 Forest\n20 Forest\nSB: 1 Island"),
                      parse_dck("41 Island"))

    def tearDown(self):
        self.temp.cleanup()

    def client(self, mode="normal", **kwargs):
        return LimitedCommonClientV2(
            [sys.executable, str(self.engine), mode],
            expected_card_db_hash=123, **kwargs)

    def test_reset_step_binds_the_actual_actor_and_terminal(self):
        with self.client() as client:
            reply = client.reset(self.decks, episode_id=7, starting_player="p1")
            self.assertEqual(reply["state"]["decision"]["actor"], "p1")
            terminal = client.step(reply, 0)
            self.assertEqual(terminal["state"]["classification"], "natural")
            with self.assertRaisesRegex(LimitedSessionError, "unavailable"):
                client.step(reply, 0)

    def test_restore_invalidates_old_prompt_even_when_visible_menu_is_identical(self):
        with self.client() as client:
            original = client.reset(self.decks)
            snapshot = client.snapshot()
            client.step(original, 0)
            restored = client.restore(snapshot["snapshot_id"])
            self.assertEqual(original["state"]["decision"]["actions"],
                             restored["state"]["decision"]["actions"])
            sent = client.next_request
            with self.assertRaisesRegex(LimitedSessionError, "stale"):
                client.step(original, 0)
            self.assertEqual(client.next_request, sent)
            self.assertEqual(client.step(restored, 0)["state"]["state_type"], "terminal")

    def test_failed_reset_preserves_current_prompt_and_snapshots(self):
        with self.client() as client:
            original = client.reset(self.decks)
            snapshot = client.snapshot()
            with self.assertRaisesRegex(LimitedSessionError, "unsupported_deck"):
                client.reset((self.decks[0], parse_dck("40 Missing")))
            self.assertFalse(client.closed)
            self.assertEqual(client.step(original, 0)["state"]["state_type"], "terminal")
            client.restore(snapshot["snapshot_id"])

    def test_reset_invalidates_snapshot_handles_before_any_restore_request(self):
        with self.client() as client:
            client.reset(self.decks)
            snapshot = client.snapshot()
            client.reset(self.decks)
            sent = client.next_request
            with self.assertRaisesRegex(LimitedSessionError, "current game"):
                client.restore(snapshot["snapshot_id"])
            self.assertEqual(client.next_request, sent)

    def test_mutated_reply_cannot_change_the_private_current_prompt(self):
        with self.client() as client:
            original = client.reset(self.decks)
            changed = deepcopy(original)
            changed["state"]["decision"]["actor"] = "p1"
            changed["state"]["decision"]["actions"][0]["label"] = "Forged choice"
            sent = client.next_request
            with self.assertRaisesRegex(LimitedSessionError, "stale"):
                client.step(changed, 0)
            self.assertEqual(client.next_request, sent)
            client.step(original, 0)

    def test_invalid_action_indices_send_nothing(self):
        with self.client() as client:
            original = client.reset(self.decks)
            sent = client.next_request
            for index in (-1, 2, True, 0.0):
                with self.subTest(index=index):
                    with self.assertRaises(LimitedSessionError):
                        client.step(original, index)
            self.assertEqual(client.next_request, sent)
            client.step(original, 0)

    def test_unsupported_is_preserved_without_applying_a_fallback(self):
        with self.client("unsupported") as client:
            reply = client.step(client.reset(self.decks), 0)
            self.assertEqual(reply["state"]["code"], "unsupported_common_input")
            sent = client.next_request
            with self.assertRaisesRegex(LimitedSessionError, "unavailable"):
                client.step(reply, 0)
            self.assertEqual(client.next_request, sent)
            self.assertFalse(client.closed)

    def test_truncated_and_halted_terminals_are_not_natural_games(self):
        for mode in ("truncated", "halted"):
            with self.subTest(mode=mode), self.client(mode) as client:
                reply = client.step(client.reset(self.decks), 0)
                self.assertEqual(reply["state"]["classification"], mode)

    def test_protocol_corruption_and_timeout_close_owned_process(self):
        for mode in ("wrong-protocol", "wrong-request", "wrong-catalog", "wrong-catalog-error", "bad-actor",
                     "bad-index", "wrong-schema", "duplicate", "timeout"):
            with self.subTest(mode=mode), self.client(
                    mode, timeout_s=0.1 if mode == "timeout" else 5) as client:
                with self.assertRaises(LimitedSessionError):
                    client.reset(self.decks)
                self.assertTrue(client.closed)
                self.assertIsNotNone(client.process.poll())

    def test_contradictory_terminal_cannot_be_reported_as_a_natural_game(self):
        with self.client("forged-terminal") as client:
            reply = client.reset(self.decks)
            with self.assertRaisesRegex(LimitedSessionError, "classification/code"):
                client.step(reply, 0)
            self.assertTrue(client.closed)

    def test_invalid_constructor_arguments_do_not_launch(self):
        for value in (True, -1, 1 << 64):
            with self.assertRaises(LimitedSessionError):
                LimitedCommonClientV2(["does-not-exist"], expected_card_db_hash=value)
        for value in (float("nan"), float("inf"), 0):
            with self.assertRaises(ValueError):
                LimitedCommonClientV2(["does-not-exist"], expected_card_db_hash=123,
                                      timeout_s=value)


if __name__ == "__main__":
    unittest.main()
