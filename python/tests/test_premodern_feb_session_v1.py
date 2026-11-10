"""Process and parser tests for the FEB client. Rust owns native replay evidence."""
from copy import deepcopy
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


if __name__ == "__main__":
    unittest.main()
