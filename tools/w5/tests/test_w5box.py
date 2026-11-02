"""Unit tests of the W5 launcher's pure parts (any OS). The cgroup sequence runs in cgroup_acceptance.py."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import w5box  # noqa: E402

CPUINFO_9950X = "processor\t: 0\nmodel name\t: AMD Ryzen 9 9950X 16-Core Processor\n"


def fake_sysfs(root: Path, cores: int = 16, ccds: int = 2, smt: bool = True) -> Path:
    """A 9950X-like topology: CPU i and i + cores are siblings; each CCD has its own L3."""
    sys_cpu = root / "cpu"
    total = cores * (2 if smt else 1)
    sys_cpu.mkdir(parents=True)
    (sys_cpu / "online").write_text(f"0-{total - 1}\n")
    per = cores // ccds
    for cpu in range(total):
        core = cpu % cores
        ccd = core // per
        siblings = f"{core},{core + cores}" if smt else f"{core}"
        l3 = f"{ccd * per}-{ccd * per + per - 1}" + (f",{ccd * per + cores}-{ccd * per + per - 1 + cores}" if smt else "")
        (sys_cpu / f"cpu{cpu}" / "topology").mkdir(parents=True)
        (sys_cpu / f"cpu{cpu}" / "topology" / "thread_siblings_list").write_text(siblings + "\n")
        (sys_cpu / f"cpu{cpu}" / "cache" / "index3").mkdir(parents=True)
        (sys_cpu / f"cpu{cpu}" / "cache" / "index3" / "shared_cpu_list").write_text(l3 + "\n")
    return sys_cpu


class CpuListTests(unittest.TestCase):
    def test_round_trip(self):
        self.assertEqual(w5box.parse_cpu_list("0-3,8,10-11\n"), [0, 1, 2, 3, 8, 10, 11])
        self.assertEqual(w5box.format_cpu_list([11, 10, 8, 3, 2, 1, 0]), "0-3,8,10-11")
        self.assertEqual(w5box.format_cpu_list([5]), "5")


class LayoutTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.topology = w5box.read_topology(fake_sysfs(Path(self.tmp.name)))

    def tearDown(self):
        self.tmp.cleanup()

    def test_default_two_core_boxes(self):
        layout = w5box.compute_layout(self.topology, 2, 2)
        self.assertEqual(len(layout["boxes"]), 6)
        self.assertEqual(layout["boxes"][0]["cpus"], [2, 3, 18, 19])
        self.assertEqual(layout["boxes"][3]["cpus"], [10, 11, 26, 27])
        self.assertEqual(layout["offbox_cpus"], [0, 1, 8, 9, 16, 17, 24, 25])
        self.assertEqual(layout["threads_per_core"], 2)
        for box in layout["boxes"]:  # every box sits inside one CCD
            self.assertTrue(all(c % 16 < 8 for c in box["cpus"]) or all(c % 16 >= 8 for c in box["cpus"]))

    def test_three_core_option_gives_four_boxes(self):
        layout = w5box.compute_layout(self.topology, 3, 2)
        self.assertEqual([b["cpus"] for b in layout["boxes"]],
                         [[2, 3, 4, 18, 19, 20], [5, 6, 7, 21, 22, 23], [10, 11, 12, 26, 27, 28],
                          [13, 14, 15, 29, 30, 31]])

    def test_positions_hash_is_host_independent_and_shape_sensitive(self):
        again = w5box.read_topology(fake_sysfs(Path(self.tmp.name) / "other"))
        self.assertEqual(w5box.compute_layout(self.topology, 2, 2)["positions_sha256"],
                         w5box.compute_layout(again, 2, 2)["positions_sha256"])
        self.assertNotEqual(w5box.compute_layout(self.topology, 2, 2)["positions_sha256"],
                            w5box.compute_layout(self.topology, 3, 2)["positions_sha256"])

    def test_box_count_limit(self):
        self.assertEqual(len(w5box.compute_layout(self.topology, 2, 2, 3)["boxes"]), 3)
        with self.assertRaises(w5box.Refusal):
            w5box.compute_layout(self.topology, 2, 2, 7)

    def test_host_class(self):
        self.assertEqual(w5box.host_class(CPUINFO_9950X, list(range(32))), "linux-zen5-9950x")
        self.assertEqual(w5box.host_class(CPUINFO_9950X, list(range(16))),
                         "linux-amd-ryzen-9-9950x-16-core-processor")
        self.assertEqual(w5box.host_class("model name : AMD EPYC 7763 64-Core Processor\n", [0, 1, 2, 3]),
                         "linux-amd-epyc-7763-64-core-processor")


class LedgerTests(unittest.TestCase):
    def test_closed_total_and_hash_chain(self):
        with tempfile.TemporaryDirectory() as tmp:
            ledger = w5box.Ledger(Path(tmp) / "ledger.jsonl")
            ledger.append({"event": "open", "instance": 0, "base_usec": 0})
            ledger.append({"event": "pause", "instance": 0, "seq": 0, "usage_usec": 50})
            ledger.append({"event": "close", "instance": 0, "final_usec": 100, "closed_total_usec": 100})
            ledger.append({"event": "open", "instance": 1, "base_usec": 100})
            summary = ledger.summary()
            self.assertEqual(summary["closed_total_usec"], 100)
            self.assertEqual(summary["next_instance"], 2)
            self.assertEqual(summary["open"]["instance"], 1)
            lines = ledger.path.read_bytes().splitlines()
            lines[1] = lines[1].replace(b"50", b"51")
            ledger.path.write_bytes(b"\n".join(lines) + b"\n")
            with self.assertRaises(ValueError):
                ledger.records()

    def test_bad_base_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            ledger = w5box.Ledger(Path(tmp) / "ledger.jsonl")
            ledger.append({"event": "open", "instance": 0, "base_usec": 0})
            ledger.append({"event": "close", "instance": 0, "final_usec": 100, "closed_total_usec": 100})
            ledger.append({"event": "open", "instance": 1, "base_usec": 0})
            with self.assertRaises(w5box.Refusal):
                ledger.summary()


def throughput_record(binding, identity_b=None):
    box = {"seconds": 100.0, "updates": 10, "ledger_usec": 350_000_000, "identity": {"u10": "aa"}}
    full = [dict(box) for _ in range(binding["boxes"])]
    if identity_b:
        full[-1]["identity"] = identity_b
    return {"schema": w5box.THROUGHPUT_SCHEMA, "binding": binding,
            "phases": [{"concurrent_boxes": 1, "boxes": [dict(box)]},
                       {"concurrent_boxes": binding["boxes"], "boxes": full}]}


class GuardTests(unittest.TestCase):
    binding = {"host_class": "linux-zen5-9950x", "positions_sha256": "p", "cores_per_box": 2, "boxes": 6,
               "arm": "C", "binary_sha256": "b", "config_sha256": "c"}

    def test_accepts_serial_and_full_node(self):
        result = w5box.require_throughput(throughput_record(self.binding), self.binding)
        self.assertAlmostEqual(result["full_node_updates_per_second"], 0.6)
        self.assertEqual(result["in_box_cpu_per_wall"], [3.5] * 6)

    def test_refusals(self):
        with self.assertRaises(w5box.Refusal):
            w5box.require_throughput(None, self.binding)
        with self.assertRaises(w5box.Refusal):
            w5box.require_throughput(throughput_record(dict(self.binding, arm="X8")), self.binding)
        with self.assertRaises(w5box.Refusal):
            w5box.require_throughput(throughput_record(self.binding, {"u10": "bb"}), self.binding)
        serial_only = throughput_record(self.binding)
        serial_only["phases"] = serial_only["phases"][:1]
        with self.assertRaises(w5box.Refusal):
            w5box.require_throughput(serial_only, self.binding)

    def test_pins(self):
        with tempfile.TemporaryDirectory() as tmp:
            binary = Path(tmp) / "bin"
            binary.write_bytes(b"x")
            ref = {"path": str(binary), "sha256": w5box.sha256_file(binary)}
            layout = {"positions_sha256": "p"}
            plan = {"host_class": "linux-zen5-9950x", "layout": layout,
                    "filler": {"argv": [], "binary": ref, "config": ref, "memory_max_bytes": 1,
                               "seed_root_template": "w5/filler{j}"}}
            spec = {"binary": ref, "config": ref, "memory_max_bytes": 1}
            w5box.check_pins(plan, spec, layout, "linux-zen5-9950x")
            with self.assertRaises(w5box.Refusal):
                w5box.check_pins(plan, spec, layout, "linux-other")
            with self.assertRaises(w5box.Refusal):
                w5box.check_pins(plan, spec, {"positions_sha256": "q"}, "linux-zen5-9950x")
            binary.write_bytes(b"y")
            with self.assertRaises(w5box.Refusal):
                w5box.check_pins(plan, spec, layout, "linux-zen5-9950x")

    def test_plan_schema(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "plan.json"
            path.write_text(json.dumps({"schema": "other"}))
            with self.assertRaises(w5box.Refusal):
                w5box.load_plan(path)


if __name__ == "__main__":
    unittest.main()
