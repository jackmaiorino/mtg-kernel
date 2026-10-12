import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

source = Path(__file__).resolve().parents[2] / "tools/spy_reset/identity_audit.py"
spec = importlib.util.spec_from_file_location("identity_audit", source)
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


def group(root, visits, singleton=False):
    def child(key):
        return dict(key=key, visits=visits, obs={"projection": {"stack": []}, "visible": root},
                    edges=[], obs_sha256=root, menu_sha256="menu", created_sim=1)
    return dict(kind="group", root_id=root, rank=0, parent="parent", edge="edge",
                parent_physical_depth=1, children=[child("a")] if singleton else [child("a"), child("b")])


class IdentityAuditTest(unittest.TestCase):
    def paths(self, folder, groups):
        dump, summary, captures, probes, out = [folder / n for n in ("dump", "groups", "capture", "probe", "out")]
        dump.write_text("\n".join(json.dumps(g) for g in groups))
        captures.write_text("")
        probes.write_text("\n".join(json.dumps(dict(kind="probe", root_id=g["root_id"],
                                                   a="a", b="b", result={"label": g["root_id"]})) for g in groups))
        audit.groups(dump, summary, None)
        return dump, summary, captures, probes, out

    def test_equal_ranks_and_keys_stay_bound_to_their_root(self):
        with tempfile.TemporaryDirectory() as directory:
            dump, summary, caps, probes, out = self.paths(Path(directory), [group("r1", 10), group("r2", 20)])
            result = audit.witnesses(summary, dump, caps, probes, out)
            self.assertEqual([r["root_id"] for r in result], ["r1", "r2"])
            self.assertEqual([r["witnesses"][0]["visits"] for r in result], [10, 20])
            self.assertEqual([r["top_pair_probe"]["label"] for r in result], ["r1", "r2"])

    def test_singleton_has_one_witness_and_no_pair_probe(self):
        with tempfile.TemporaryDirectory() as directory:
            dump, summary, caps, probes, out = self.paths(Path(directory), [group("r1", 10, True)])
            self.assertIsNone(json.loads(summary.read_text())["groups"][0]["top_pair"])
            result = audit.witnesses(summary, dump, caps, probes, out)[0]
            self.assertEqual(len(result["witnesses"]), 1)
            self.assertIsNone(result["top_pair_probe"])

    def test_conflicting_duplicate_group_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, "conflicting duplicate"):
                self.paths(Path(directory), [group("r1", 10), group("r1", 20)])

    def test_legacy_unbound_summary_requires_one_unambiguous_root(self):
        for count in (1, 2):
            with tempfile.TemporaryDirectory() as directory:
                dump, summary, caps, probes, out = self.paths(Path(directory), [group(f"r{i}", 10) for i in range(count)])
                data = json.loads(summary.read_text())
                for row in data["groups"]:
                    del row["root_id"]
                summary.write_text(json.dumps(data))
                if count == 1:
                    self.assertEqual(audit.witnesses(summary, dump, caps, probes, out)[0]["root_id"], "r0")
                else:
                    with self.assertRaisesRegex(ValueError, "unambiguous root"):
                        audit.witnesses(summary, dump, caps, probes, out)


if __name__ == "__main__":
    unittest.main()
