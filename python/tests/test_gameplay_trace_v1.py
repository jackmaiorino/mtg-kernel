"""Integrity failures must be explicit before a disputed decision is displayed."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

_path = Path(__file__).parents[1] / "tools/gameplay_trace_v1.py"
_spec = importlib.util.spec_from_file_location("gameplay_trace_reader", _path)
reader = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(reader)


def record():
    before = 73
    seed = reader.splitmix_first(before)
    draw = reader.splitmix_first(seed)
    selected = int(draw >= (1 << 63))
    tensor = {"state": [0, 1065353216], "float_encoding": "binary32 bits"}
    row = {
        "ordered_actions": [{"engine_index": i, "label": "Pass", "semantic": {"kind": "pass"}} for i in range(2)],
        "selected_index": selected,
        "decision": {"episode_id": 1, "step": 4, "legal_action_count": 2},
        "behavior": {"kind": "hamilton_q64", "selected_index": selected, "mass_numerators": [str(1 << 63)] * 2},
        "rng": {"before": {"state": before}, "after": {"state": (before + reader.GAMMA) & reader.MASK}, "sampler_seed": seed, "inverse_cdf_draw": draw},
        "bound_engine_action": {"engine_index": selected, "semantic": {"kind": "pass"}, "episode_id": 1, "step": 4},
        "actor": "p0", "transition": {"actor": "p0", "observation": {"acting_player": "p0"}},
        "logit_bits": [0, 0], "logits": [0.0, 0.0], "encoded_input": tensor,
        "encoded_input_sha256": hashlib.sha256(json.dumps(tensor, sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
    }
    return row


def corrupt_record(corrupt):
    row = copy.deepcopy(record())
    if corrupt == "order":
        row["ordered_actions"].reverse()
    elif corrupt == "mass":
        row["behavior"]["mass_numerators"][0] = "1"
    elif corrupt == "rng":
        row["rng"]["before"]["state"] += 1
    elif corrupt == "binding":
        row["bound_engine_action"]["engine_index"] ^= 1
    elif corrupt == "actor":
        row["transition"]["observation"]["acting_player"] = "p1"
    elif corrupt == "tensor":
        row["encoded_input"]["state"][0] = 1
    else:
        row["logit_bits"][0] = 1065353216
    return row


class GameplayTraceReaderTests(unittest.TestCase):
    def test_equal_q64_menu_and_seed_reproduce_selection(self):
        reader.validate_decision(record())

    def test_corrupt_capture_is_refused(self):
        for corrupt in ("order", "mass", "rng", "binding", "actor", "tensor", "logit"):
            with self.subTest(corrupt=corrupt), self.assertRaises(ValueError):
                reader.validate_decision(corrupt_record(corrupt))

    def test_reader_refuses_truncated_capture(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "trace.jsonl"
            path.write_text(json.dumps({"schema": reader.SCHEMA, "kind": "header", "run_id": "test"}) + "\n")
            with self.assertRaisesRegex(ValueError, "missing header/footer"):
                reader.read_trace(path)
