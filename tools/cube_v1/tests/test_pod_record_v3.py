"""Tests for m4-interfaces/v3 pod records and reader breakdowns."""

from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path

TESTS_DIR = Path(__file__).resolve().parent
CUBE_V1_DIR = TESTS_DIR.parent
sys.path.insert(0, str(CUBE_V1_DIR))
sys.path.insert(0, str(TESTS_DIR))

import cube_synth as synth  # noqa: E402
import pod_record  # noqa: E402
import readers  # noqa: E402


def _upgrade_record_to_v3(rec: dict, *, zero_decision_game: int | None = None) -> dict:
    v3 = copy.deepcopy(rec)
    v3["schema"] = pod_record.POD_RECORD_SCHEMA_V3
    v3["interfaces"] = pod_record.INTERFACES_VERSION_V3
    for g in v3["games"]:
        seats = pod_record.game_seats(g["chairs"])
        for att in g["attempts"]:
            if zero_decision_game is not None and g["game"] == zero_decision_game:
                att["decisions"] = 0
                att["physical_decisions"] = 0
                att["policy_steps"] = 0
                att["end"] = {"kind": "session_error", "code": "reset_error", "message": "zero"}
                att["halted"] = True
                att["stop_kind"] = "session_error:reset_error"
                att["last_action_source"] = None
                att["first_player_seat"] = None
                att["last_acting_seat"] = None
                att["last_acting_arm"] = None
                g["scores"] = None
            else:
                att["first_player_seat"] = seats[0]
                att["last_acting_seat"] = seats[1] if g["game"] % 2 else seats[0]
                att["last_acting_arm"] = "learning:candidate" if g["game"] % 2 == 0 else "fixture:candidate"
    # Rebuild halts ledger to match any injected halt.
    v3["halts"] = [
        {
            "game": g["game"],
            "attempt": att["attempt"],
            "stop_kind": att["stop_kind"],
            "last_action_source": att["last_action_source"],
            "deal_seed": att["deal_seed"],
        }
        for g in v3["games"]
        for att in g["attempts"]
        if att["halted"]
    ]
    return v3


class PodRecordV3Tests(unittest.TestCase):
    def test_v3_validates_completed_halted_and_zero_decision_null(self):
        plans = [synth.plan("aa", 0, mirror=0)]
        rec_v2 = synth.record(plans[0], lambda _p, _g, _f: "draw")
        pod_record.validate_record(rec_v2)

        rec_v3 = _upgrade_record_to_v3(rec_v2, zero_decision_game=0)
        pod_record.validate_record(rec_v3)

        # Reject mismatched schema/interfaces pair.
        bad_pair = copy.deepcopy(rec_v3)
        bad_pair["interfaces"] = pod_record.INTERFACES_VERSION
        with self.assertRaises(pod_record.RecordError):
            pod_record.validate_record(bad_pair)

        # Reject non-null last_acting_seat when decisions == 0.
        bad_zero = copy.deepcopy(rec_v3)
        bad_zero["games"][0]["attempts"][0]["last_acting_seat"] = 0
        bad_zero["games"][0]["attempts"][0]["last_acting_arm"] = "learning:candidate"
        with self.assertRaises(pod_record.RecordError):
            pod_record.validate_record(bad_zero)

        # Reject unknown last_acting_arm string.
        bad_arm = copy.deepcopy(rec_v3)
        bad_arm["games"][1]["attempts"][0]["last_acting_arm"] = "unknown_arm"
        with self.assertRaises(pod_record.RecordError):
            pod_record.validate_record(bad_arm)

        # Reject v3 attempt keys inside a v2 record.
        mixed = copy.deepcopy(rec_v2)
        mixed["games"][0]["attempts"][0]["first_player_seat"] = 0
        with self.assertRaises(pod_record.RecordError):
            pod_record.validate_record(mixed)

    def test_readers_halts_and_lengths_emits_v3_breakdowns_and_preserves_v2(self):
        plan = synth.plan("aa", 0, mirror=0)
        rec_v2 = synth.record(plan, lambda _p, _g, _f: "draw")
        rec_v3 = _upgrade_record_to_v3(rec_v2, zero_decision_game=0)

        scoped_v2 = {plan["pod_index"]: rec_v2["games"]}
        b_v2, _ = readers._halts_and_lengths("aa", {plan["pod_index"]: rec_v2}, {plan["pod_index"]: plan}, scoped_v2)
        self.assertNotIn("by_first_player_seat", b_v2)
        self.assertNotIn("by_last_acting_seat", b_v2)
        self.assertNotIn("by_last_acting_arm", b_v2)

        scoped_v3 = {plan["pod_index"]: rec_v3["games"]}
        b_v3, _ = readers._halts_and_lengths("aa", {plan["pod_index"]: rec_v3}, {plan["pod_index"]: plan}, scoped_v3)
        self.assertIn("null", b_v3["by_first_player_seat"])
        self.assertEqual(b_v3["by_last_acting_seat"]["null"], {"completed": 0, "halted": 1})
        self.assertEqual(b_v3["by_last_acting_arm"]["null"], {"completed": 0, "halted": 1})
        self.assertGreater(b_v3["by_last_acting_arm"]["learning:candidate"]["completed"], 0)
        self.assertGreater(b_v3["by_last_acting_arm"]["fixture:candidate"]["completed"], 0)


if __name__ == "__main__":
    unittest.main()
