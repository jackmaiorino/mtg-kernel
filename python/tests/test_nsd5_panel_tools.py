"""Panel tooling for the no-state-digest plain-play panel (collab
LANES/throughput-architecture-20261020/STEP5-PANEL-DRAFT.md): its fresh seed
namespace, all-deck decision capture, and recovery that stops on the contract's
bitwise-guard failures instead of recording them as technical terminations."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools" / "spy_reset"))
sys.path.insert(0, str(ROOT / "python" / "tools"))
import nine_deck_public_panel_v1 as npp  # noqa: E402
import panel  # noqa: E402
import pilot  # noqa: E402


def source(tag):
    return {"play_import": {"path": f"{tag}-import.json", "sha256": "1" * 64},
            "feature_transfer": {"expected_feature_contract_digest": "c", "expected_feature_encoding_digest": "e"},
            "checkpoint": {"path": f"{tag}.json", "sha256": "2" * 64}}


class Nsd5PanelToolsTest(unittest.TestCase):
    def test_namespace_is_admitted_and_its_seeds_are_fresh(self):
        self.assertIn(pilot.NSD5_PANEL_NS, pilot.PANEL_NAMESPACES)
        keys = [(o, t, k) for o in range(pilot.DECKS) for t in range(pilot.DECKS) for k in range(pilot.PANEL_REPEATS)]
        fresh = {pilot.panel_seed(*key, pilot.NSD5_PANEL_NS) for key in keys}
        self.assertEqual(len(fresh), len(keys))
        for namespace in pilot.PANEL_NAMESPACES:
            if namespace != pilot.NSD5_PANEL_NS:
                self.assertFalse(fresh & {pilot.panel_seed(*key, namespace) for key in keys}, namespace)

    def test_capture_all_changes_only_the_capture_setting(self):
        decks = pilot.load_decks()
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            default = panel.build("ck", source("ck"), "T1", source("t1"), decks, tmp / "a", 16, pilot.NSD5_PANEL_NS)
            same = panel.build("ck", source("ck"), "T1", source("t1"), decks, tmp / "b", 16, pilot.NSD5_PANEL_NS,
                               capture_all=False)
            every = panel.build("ck", source("ck"), "T1", source("t1"), decks, tmp / "c", 16, pilot.NSD5_PANEL_NS,
                                capture_all=True)
            self.assertNotIn("capture_all", default)
            self.assertIs(every["capture_all"], True)
            self.assertEqual(default["schedule_sha256"], every["schedule_sha256"])
            for a, b, c in zip(default["requests"], same["requests"], every["requests"]):
                ra, rb, rc = (json.loads(Path(x["request"]["path"]).read_text()) for x in (a, b, c))
                self.assertEqual(ra["capture_decisions"], "spy" in a["name"])
                self.assertEqual(ra, {**rb, "output_directory": ra["output_directory"]})
                self.assertIs(rc["capture_decisions"], True)
                self.assertEqual({k: v for k, v in ra.items() if k not in ("capture_decisions", "output_directory")},
                                 {k: v for k, v in rc.items() if k not in ("capture_decisions", "output_directory")})
                self.assertEqual(a["cases"], c["cases"])

    def test_bitwise_guard_failure_stops_recovery(self):
        for marker in npp.ENGINEERING_STOP_MARKERS:
            with tempfile.TemporaryDirectory() as tmp:
                tmp = Path(tmp)
                output = tmp / "native"
                request = {"sources": [], "matches": [{"config": {"seed": 5}}, {"config": {"seed": 6}}],
                           "cross_generation_evaluation": True, "capture_decisions": True,
                           "output_directory": str(output).replace("\\", "/")}
                request_path = tmp / "request.json"
                npp.write_json(request_path, request)
                output.mkdir()
                npp.write_json(output / "start.json", {"schema": "public-input-evaluation-start/v1", "command": request})
                npp.write_json(output / "failure.json",
                               {"error": f"match seed 5, game 1: {marker} converted forward differs"})
                with self.assertRaisesRegex(RuntimeError, "engineering stop"):
                    npp.recover("no-evaluator-is-run", request_path)
                self.assertFalse(request_path.with_suffix(".recovery.json").exists())


if __name__ == "__main__":
    unittest.main()
