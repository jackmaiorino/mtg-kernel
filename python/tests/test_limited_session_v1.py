from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
from limited_decks_v1 import parse_dck
from limited_session_v1 import LimitedClientV1, LimitedSessionError, smoke


FAKE_ENGINE = '''
import json, sys, time
mode = sys.argv[1]
for line in sys.stdin:
    request = json.loads(line)
    if mode == "timeout":
        time.sleep(10)
    reply = {"protocol":"kernel_limited_jsonl", "schema_version":1,
             "request_id":request["request_id"], "card_db_hash":1, "kernel_version":"test"}
    if mode.startswith("engine-priority"):
        reply["schema_version"] = 2
        reply["priority_mode"] = "engine_windows_v1"
        if mode == "engine-priority-wrong":
            reply["priority_mode"] = "harness_v2"
        elif request["schema_version"] != 2:
            raise RuntimeError("schema-2 request required")
    if mode.startswith("foundations"):
        reply["schema_version"] = 3
        reply["priority_mode"] = "engine_windows_v1"
        reply["combat_rules"] = "wrong" if mode == "foundations-wrong" else "foundations_v1"
        if request["schema_version"] != 3:
            raise RuntimeError("schema-3 request required")
    if request["request_type"] == "reset":
        if request["decks"][1]["cards"][0]["name"] == "Missing":
            reply.update(response_type="error", error={"code":"unsupported_deck","message":"seat 1"})
        else:
            reply.update(response_type="decision", decision={"schema_version":5,"episode_id":request["episode_id"], "step":0,
                "legal_actions":[{"selected_index":0,"stable_id":"pass-id","semantic":{"action_kind":"pass"}}]})
    else:
        reply.update(response_type="terminal", terminal={"terminal_classification":"natural","policy_step_count":1})
    if mode == "wrong-protocol":
        reply["protocol"] = "kernel_rl_jsonl"
    elif mode == "wrong-request":
        reply["request_id"] = "other"
    elif mode == "empty-actions":
        reply["decision"]["legal_actions"] = []
    if mode == "duplicate":
        print('{"protocol":"a","protocol":"b"}', flush=True)
    else:
        print(json.dumps(reply),flush=True)
'''


class LimitedClientTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.engine = Path(self.temp.name) / "engine.py"
        self.engine.write_text(FAKE_ENGINE, encoding="utf-8")
        self.decks = (parse_dck("40 Forest"), parse_dck("40 Island"))

    def tearDown(self) -> None:
        self.temp.cleanup()

    def command(self, mode: str = "normal") -> list[str]:
        return [sys.executable, str(self.engine), mode]

    def test_external_reset_step_and_smoke_are_deterministic(self) -> None:
        first = smoke(self.command(), self.decks, seed=123)
        second = smoke(self.command(), self.decks, seed=123)
        self.assertEqual(first, second)
        self.assertEqual(first["terminal"]["terminal_classification"], "natural")
        self.assertEqual(len(first["transcript_sha256"]), 64)

    def test_server_error_preserves_the_client_and_current_decision(self) -> None:
        with LimitedClientV1(self.command()) as client:
            active = client.reset(self.decks)
            with self.assertRaisesRegex(LimitedSessionError, "unsupported_deck"):
                client.reset((self.decks[0], parse_dck("40 Missing")))
            self.assertFalse(client.closed)
            self.assertEqual(client.last_reply, active)
            self.assertEqual(client.step(active["decision"], 0)["response_type"], "terminal")

    def test_transport_and_protocol_errors_close_the_owned_process(self) -> None:
        for mode in ("wrong-protocol", "wrong-request", "empty-actions", "duplicate", "timeout"):
            with self.subTest(mode=mode):
                with LimitedClientV1(self.command(mode), timeout_s=0.1 if mode == "timeout" else 5) as client:
                    with self.assertRaises(LimitedSessionError):
                        client.reset(self.decks)
                    self.assertTrue(client.closed)
                    self.assertIsNotNone(client.process.poll())

    def test_invalid_local_action_does_not_send_a_step(self) -> None:
        with LimitedClientV1(self.command()) as client:
            active = client.reset(self.decks)
            for index in (-1, 1, True):
                with self.subTest(index=index), self.assertRaises(LimitedSessionError):
                    client.step(active["decision"], index)
            self.assertEqual(client.next_request, 1)
            self.assertEqual(client.step(active["decision"], 0)["response_type"], "terminal")

    def test_engine_priority_requires_explicit_schema_and_mode(self) -> None:
        first = smoke(self.command("engine-priority"), self.decks, engine_priority=True)
        second = smoke(self.command("engine-priority"), self.decks, engine_priority=True)
        self.assertEqual(first, second)
        self.assertEqual(first["schema"], "kernel_limited_smoke/v2")
        for mode, enabled in (("normal", True), ("engine-priority-wrong", True),
                              ("engine-priority", False)):
            with self.subTest(mode=mode, enabled=enabled):
                with LimitedClientV1(self.command(mode), engine_priority=enabled) as client:
                    with self.assertRaises(LimitedSessionError):
                        client.reset(self.decks)
                    self.assertTrue(client.closed)

    def test_foundations_requires_matching_schema_and_combat_identity(self) -> None:
        first = smoke(self.command("foundations"), self.decks, foundations_combat=True)
        second = smoke(self.command("foundations"), self.decks, foundations_combat=True)
        self.assertEqual(first, second)
        self.assertEqual(first["schema"], "kernel_limited_smoke/v3")
        for mode, enabled in (("engine-priority", True), ("foundations-wrong", True),
                              ("foundations", False)):
            with self.subTest(mode=mode, enabled=enabled):
                with LimitedClientV1(self.command(mode), foundations_combat=enabled) as client:
                    with self.assertRaises(LimitedSessionError):
                        client.reset(self.decks)
                    self.assertTrue(client.closed)


if __name__ == "__main__":
    unittest.main()
