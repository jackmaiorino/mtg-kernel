"""Process and parser tests plus an opt-in native FEB client smoke."""
from copy import deepcopy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
from limited_session_v1 import LimitedSessionError
from premodern_feb_session_v1 import FebClientV1, PROFILE, PROTOCOL


MOCK = r'''
import json
import sys

protocol, profile, mutation = sys.argv[1:]
generation = 0
episode = 0
actor = "p0"
step = 0
saved = None
accepted = None
tree = dict(rows=[[0]*12], topology=[[[0]*4, [0]*4]])

for line in sys.stdin:
    req = json.loads(line)
    reply = dict(protocol=protocol, profile=profile, card_db_hash=123, schema_version=1,
                 request_id=req["request_id"], response_type="state")
    cmd = req["request_type"]
    if cmd == "reset":
        generation += 1
        episode = req["episode_id"]
        actor = req["starting_player"]
        step = 0
        saved = None
        accepted = None
    if cmd == "snapshot":
        saved = (actor, step)
        reply["snapshot_id"] = f"{generation:016x}{1:016x}"
    if cmd == "restore":
        generation += 1
        actor, step = saved
        accepted = None
    if cmd == "step":
        receipt = {key: req[key] for key in ("prompt", "native_ordinal", "submission")}
        if receipt != accepted:
            step += 1
            actor = "p1" if actor == "p0" else "p0"
            accepted = receipt
        reply["receipt"] = receipt
    prompt = dict(episode_id=episode, authority_generation=generation, actor=actor, prompt_seq=step)
    numeric = dict(profile=profile, actor=actor, step=step, card_db_hash=123,
                   observation=tree, actions=[tree])
    human = dict(profile=profile, actor=actor, step=step, ordered={}, scene=None,
                 actions=[dict(kind="supported", native_ordinal=0, label="Keep this hand")])
    if mutation == "different_actor":
        human["actor"] = "p1" if actor == "p0" else "p0"
    if mutation == "different_ordinal":
        human["actions"][0]["native_ordinal"] = 1
    if mutation == "different_catalog":
        numeric["card_db_hash"] = 124
    if mutation.startswith("tree:"):
        location, replacement = json.loads(mutation[5:])
        if location == "observation":
            numeric["observation"] = replacement
        else:
            numeric["actions"][0] = replacement
    reply["state"] = dict(state_type="decision", episode_id=episode,
                          authority_generation=generation, prompt=prompt, numeric=numeric, human=human)
    print(json.dumps(reply), flush=True)
'''


class FebClientV1Tests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.engine = Path(temp.name) / "feb_mock.py"
        self.engine.write_text(MOCK, encoding="utf-8")

    def client(self, mutation="none", protocol=PROTOCOL):
        return FebClientV1([sys.executable, str(self.engine), protocol, PROFILE, mutation],
                           expected_card_db_hash=123)

    def test_framing_restore_and_retry_both_seats(self):
        for seat in ("p0", "p1"):
            with self.subTest(seat=seat), self.client() as client:
                opening = client.reset(episode_id=460, seed=20261108, starting_player=seat)
                self.assertEqual(opening["state"]["prompt"]["actor"], seat)
                self.assertEqual(client.current()["state"], opening["state"])
                saved = client.snapshot()["snapshot_id"]
                tampered = deepcopy(opening)
                tampered["state"]["human"]["actions"][0]["native_ordinal"] = False
                with self.assertRaisesRegex(LimitedSessionError, "stale"):
                    client.step(tampered, 0)
                advanced = client.step(opening, 0)
                self.assertEqual(advanced["state"]["prompt"]["prompt_seq"], 1)
                self.assertEqual(client.current()["state"], advanced["state"])
                retry = client.step(opening, 0)
                self.assertEqual(retry["state"], advanced["state"])
                self.assertEqual(retry["receipt"], advanced["receipt"])
                with self.assertRaisesRegex(LimitedSessionError, "stale"):
                    client.step(opening, 0, human=False)
                restored = client.restore(saved)
                self.assertEqual(restored["state"]["numeric"], opening["state"]["numeric"])
                self.assertEqual(restored["state"]["human"], opening["state"]["human"])
                with self.assertRaisesRegex(LimitedSessionError, "stale"):
                    client.step(opening, 0)
                self.assertEqual(client.step(restored, 0, human=False)["receipt"]["submission"], "numeric")
                client.reset(starting_player=seat)
                with self.assertRaisesRegex(LimitedSessionError, "does not belong"):
                    client.restore(saved)

    def test_refuses_foreign_or_inconsistent_replies(self):
        for mutation in ("different_actor", "different_ordinal", "different_catalog"):
            with self.subTest(mutation=mutation), self.client(mutation) as client:
                with self.assertRaises(LimitedSessionError):
                    client.reset()
        with self.client(protocol="kernel_limited_jsonl") as client:
            with self.assertRaisesRegex(LimitedSessionError, "protocol identity"):
                client.reset()

    def test_rejects_malformed_numeric_observation_and_action_trees(self):
        valid_row = [0] * 12
        valid_entry = [[0] * 4, [0] * 4]
        for location in ("observation", "action"):
            for field, bad, message in (
                ("rows", [None], "row"),
                ("rows", [[0] * 11], "row"),
                ("rows", [[0] * 13], "row"),
                ("rows", [[False, *valid_row[1:]]], "row"),
                ("rows", [[0.0, *valid_row[1:]]], "row"),
                ("rows", [[-1, *valid_row[1:]]], "row"),
                ("rows", [[65536, *valid_row[1:]]], "row"),
                ("topology", [None], "topology"),
                ("topology", [[[0] * 4]], "topology"),
                ("topology", [[[0] * 3, [0] * 4]], "topology"),
                ("topology", [[[0] * 4, [0, 0, 0, True]]], "topology"),
                ("topology", [[[0, 0.0, 0, 0], [0] * 4]], "topology"),
                ("topology", [[[0] * 4, [0, 0, 0, 65536]]], "topology"),
            ):
                tree = {"rows": [valid_row], "topology": [valid_entry], field: bad}
                mutation = "tree:" + json.dumps([location, tree])
                with self.subTest(location=location, tree=tree), self.client(mutation) as client:
                    with self.assertRaisesRegex(LimitedSessionError, f"numeric tree {message}"):
                        client.reset()
                    self.assertIsNone(client.last_reply)
                    self.assertTrue(client.closed)


class NativeFebClientV1Tests(unittest.TestCase):
    def test_native_opening_reset_step_snapshot_restore_both_seats(self):
        binary = os.environ.get("MTG_KERNEL_FEB_ENV_BIN")
        catalog = os.environ.get("MTG_KERNEL_FEB_CARD_DB_HASH")
        if binary is None and catalog is None:
            self.skipTest("set MTG_KERNEL_FEB_ENV_BIN and MTG_KERNEL_FEB_CARD_DB_HASH for the native smoke")
        self.assertTrue(binary and catalog, "native FEB smoke requires a binary and its catalog hash")
        self.assertTrue(Path(binary).is_file())
        for seat in ("p0", "p1"):
            with self.subTest(seat=seat), FebClientV1(
                    [str(Path(binary).resolve()), "--premodern-feb-v1"],
                    expected_card_db_hash=int(catalog, 0)) as client:
                opening = client.reset(episode_id=460, seed=20261108, starting_player=seat)
                state = opening["state"]
                self.assertEqual(state["prompt"]["actor"], seat)
                self.assertEqual(len(state["human"]["scene"]["own_hand"]), 7)
                self.assertEqual(client.current()["state"], state)
                keep = next(row["native_ordinal"] for row in state["human"]["actions"]
                            if row.get("label") == "Keep this hand")
                saved = client.snapshot()
                self.assertEqual(saved["state"], state)
                advanced = client.step(opening, keep)
                self.assertEqual(advanced["state"]["prompt"]["prompt_seq"], 1)
                self.assertNotEqual(advanced["state"]["prompt"]["actor"], seat)
                retried = client.step(opening, keep)
                self.assertEqual(retried["receipt"], advanced["receipt"])
                self.assertEqual(retried["state"], advanced["state"])
                restored = client.restore(saved["snapshot_id"])
                self.assertGreater(restored["state"]["authority_generation"], state["authority_generation"])
                for view in ("human", "numeric"):
                    self.assertEqual(restored["state"][view], state[view])
                with self.assertRaisesRegex(LimitedSessionError, "stale"):
                    client.step(opening, keep)
                replayed = client.step(restored, keep, human=False)
                self.assertEqual(replayed["receipt"]["submission"], "numeric")
                for view in ("human", "numeric"):
                    self.assertEqual(replayed["state"][view], advanced["state"][view])
                client.reset(starting_player=seat)
                with self.assertRaisesRegex(LimitedSessionError, "does not belong"):
                    client.restore(saved["snapshot_id"])


if __name__ == "__main__":
    unittest.main()
