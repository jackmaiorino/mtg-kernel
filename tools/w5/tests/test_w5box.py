"""Unit tests of the W5 launcher's pure parts (any OS). The cgroup sequence runs in cgroup_acceptance.py."""
from contextlib import nullcontext
import errno
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

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

    def test_hybrid_topology_threads_per_core_filter(self):
        hybrid = {
            "online": list(range(24)),
            "domains": [
                [[2 * k, 2 * k + 1] for k in range(8)] + [[16 + e] for e in range(8)]
            ],
        }
        with self.assertRaisesRegex(w5box.Refusal, "mix SMT and non-SMT"):
            w5box.compute_layout(hybrid, 2, 2)
        p_layout = w5box.compute_layout(hybrid, 2, 2, threads_per_core=2)
        self.assertEqual(p_layout["threads_per_core"], 2)
        self.assertEqual([b["cpus"] for b in p_layout["boxes"]],
                         [[4, 5, 6, 7], [8, 9, 10, 11], [12, 13, 14, 15]])
        self.assertEqual(p_layout["offbox_cpus"], [0, 1, 2, 3] + list(range(16, 24)))
        self.assertEqual(w5box.detect_plan_layout(p_layout, hybrid), p_layout)
        e_layout = w5box.compute_layout(hybrid, 4, 0, threads_per_core=1)
        self.assertEqual(e_layout["threads_per_core"], 1)
        self.assertEqual([b["cpus"] for b in e_layout["boxes"]],
                         [[16, 17, 18, 19], [20, 21, 22, 23]])
        self.assertEqual(e_layout["offbox_cpus"], list(range(16)))
        self.assertEqual(w5box.detect_plan_layout(e_layout, hybrid), e_layout)
        # Homogeneous hosts keep detecting tampered threads_per_core.
        tampered = dict(w5box.compute_layout(self.topology, 2, 2), threads_per_core=1)
        self.assertNotEqual(w5box.detect_plan_layout(tampered, self.topology)["threads_per_core"], 1)


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
               "arm": "C", "binary_sha256": "b", "config_sha256": "c", "launch_spec_sha256": "l"}

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

    def test_box_subset_full_phase_is_the_plans_boxes(self):
        binding = dict(self.binding, box_slots=[3, 4, 5])
        record = throughput_record(dict(binding, boxes=3))
        record["binding"] = binding
        for k, box in enumerate(record["phases"][-1]["boxes"]):
            box["slot"] = 3 + k
        result = w5box.require_throughput(record, binding)
        self.assertEqual(len(result["in_box_cpu_per_wall"]), 3)
        elsewhere = json.loads(json.dumps(record))
        elsewhere["phases"][-1]["boxes"][0]["slot"] = 0
        with self.assertRaisesRegex(w5box.Refusal, "box_slots"):  # measured on other boxes than it runs
            w5box.require_throughput(elsewhere, binding)
        with self.assertRaises(w5box.Refusal):  # a whole-layout record does not qualify the subset plan
            whole = throughput_record(self.binding)
            whole["binding"] = binding
            w5box.require_throughput(whole, binding)
        with self.assertRaises(w5box.Refusal):  # nor a subset record the whole-layout plan
            w5box.require_throughput(record, self.binding)

    def test_pins(self):
        with tempfile.TemporaryDirectory() as tmp:
            binary, config = Path(tmp) / "bin", Path(tmp) / "cfg.json"
            binary.write_bytes(b"x")
            config.write_bytes(b"{}")
            bref = {"path": str(binary), "sha256": w5box.sha256_file(binary)}
            cref = {"path": str(config), "sha256": w5box.sha256_file(config)}
            layout = w5box.compute_layout(w5box.read_topology(fake_sysfs(Path(tmp) / "sys")), 2, 2)
            plan = {"host_class": "linux-zen5-9950x", "layout": json.loads(json.dumps(layout)),
                    "state_root": tmp, "isolation": "partition-root",
                    "filler": {"argv": [str(binary), "--config", str(config), "--seed", "{seed_root}"],
                               "binary": bref, "config": cref, "memory_max_bytes": 1,
                               "seed_root_template": "w5/filler{j}"}}
            spec = {"box_slot": 0, "arm": "C", "seed_root": "w5/chain1", "binary": bref, "config": cref,
                    "memory_max_bytes": 1, "argv": [str(binary), f"--config={config}", "--seed", "{seed_root}"]}
            w5box.check_pins(plan, "c1", spec, layout, "linux-zen5-9950x")
            self.assertEqual(w5box.chain_argv(spec, "c1", Path(tmp))[-1], "w5/chain1")
            with self.assertRaises(w5box.Refusal):
                w5box.check_pins(plan, "c1", spec, layout, "linux-other")
            edited = json.loads(json.dumps(plan))
            edited["layout"]["boxes"][0]["cpus"] = [2, 3]  # stale hash field kept: still refused
            with self.assertRaises(w5box.Refusal):
                w5box.check_pins(edited, "c1", spec, layout, "linux-zen5-9950x")
            with self.assertRaises(w5box.Refusal):  # argv[0] must be the pinned binary
                w5box.check_pins(plan, "c1", dict(spec, argv=["/bin/sh"] + spec["argv"][1:]), layout,
                                 "linux-zen5-9950x")
            with self.assertRaises(w5box.Refusal):  # the pinned config must be named
                w5box.check_pins(plan, "c1", dict(spec, argv=[str(binary)]), layout, "linux-zen5-9950x")
            # the throughput binding covers the launch template, not the chain's seeds
            same_arm = dict(spec, seed_root="w5/chain2", box_slot=1)
            self.assertEqual(w5box.throughput_binding(plan, spec), w5box.throughput_binding(plan, same_arm))
            self.assertNotEqual(w5box.throughput_binding(plan, spec),
                                w5box.throughput_binding(plan, dict(spec, argv=spec["argv"] + ["--fast"])))
            # box_slots: the binding names them (absent otherwise) and a chain outside them is refused
            self.assertNotIn("box_slots", w5box.throughput_binding(plan, spec))
            subset = dict(plan, box_slots=[3, 4, 5], isolation="none")
            self.assertEqual(w5box.throughput_binding(subset, spec)["box_slots"], [3, 4, 5])
            w5box.check_pins(subset, "c1", dict(spec, box_slot=4), layout, "linux-zen5-9950x")
            with self.assertRaisesRegex(w5box.Refusal, "box_slots"):
                w5box.check_pins(subset, "c1", spec, layout, "linux-zen5-9950x")
            for bad in ([], [4, 3], [3, 3], [6], [-1], "3"):
                with self.assertRaisesRegex(w5box.Refusal, "box_slots"):
                    w5box.check_pins(dict(subset, box_slots=bad), "c1", dict(spec, box_slot=4), layout,
                                     "linux-zen5-9950x")
            with self.assertRaisesRegex(w5box.Refusal, "isolation none"):  # never a second partition root
                w5box.check_pins(dict(subset, isolation="partition-root"), "c1", dict(spec, box_slot=4), layout,
                                 "linux-zen5-9950x")
            binary.write_bytes(b"y")
            with self.assertRaises(w5box.Refusal):
                w5box.check_pins(plan, "c1", spec, layout, "linux-zen5-9950x")

    def test_plan_schema(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "plan.json"
            path.write_text(json.dumps({"schema": "other"}))
            with self.assertRaises(w5box.Refusal):
                w5box.load_plan(path)
            relative = {"schema": w5box.PLAN_SCHEMA, "host_class": "x", "layout": {}, "cgroup_root": "/sys/fs/cgroup/w5",
                        "state_root": "state", "isolation": "none", "chains": {}, "filler": {}}
            path.write_text(json.dumps(relative))
            with self.assertRaises(w5box.Refusal):  # the trainer's ledger source path must not depend on its cwd
                w5box.load_plan(path)


class NodeStateTests(unittest.TestCase):
    def test_enforcement_skips_cgroup_removed_during_kernfs_read(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            state_root, cgroup_root = root / "state", root / "cgroups"
            chain = cgroup_root / "box0" / "chain"
            chain.mkdir(parents=True)
            (chain / "cgroup.procs").touch()
            node = w5box.NodeLog(state_root)
            node.append({"event": "node-freeze", "reason": "operator hold"})
            plan = {"state_root": state_root, "cgroup_root": cgroup_root}

            # The path was found, but Linux invalidated the open kernfs node.
            with patch.object(node, "locked", nullcontext), patch.object(
                    w5box.Cgroup, "read", side_effect=OSError(errno.ENODEV, "No such device")):
                state = w5box.enforce_node(plan, node, "test")

            self.assertTrue(state["frozen"])
            records, _ = node.read()
            self.assertEqual(records[-1]["event"], "enforced")
            self.assertTrue(records[-1]["frozen"])
            self.assertEqual(records[-1]["changed"], [])

    def test_box_subset_plan_enforces_only_its_own_boxes(self):
        """A plan with box_slots never freezes a chain in a box it does not own (another plan's)."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            state_root, cgroup_root = root / "state", root / "cgroups"
            for k in (0, 3):
                chain = cgroup_root / f"box{k}" / "chain"
                chain.mkdir(parents=True)
                (chain / "cgroup.procs").touch()
                (chain / "cgroup.events").write_text("populated 1\nfrozen 0\n")
            node = w5box.NodeLog(state_root)
            node.append({"event": "node-freeze", "reason": "operator hold"})
            layout = {"boxes": [{"slot": k, "cpus": [k]} for k in range(6)]}
            frozen = []

            def freeze(cg, on, timeout=None):
                frozen.append((cg.path.parent.name, on))
            with patch.object(node, "locked", nullcontext), patch.object(w5box.Cgroup, "freeze", freeze):
                w5box.enforce_node({"state_root": state_root, "cgroup_root": cgroup_root, "layout": layout,
                                    "box_slots": [3, 4, 5], "isolation": "none"}, node, "test")
                self.assertEqual(frozen, [("box3", True)])
                frozen.clear()
                w5box.enforce_node({"state_root": state_root, "cgroup_root": cgroup_root, "layout": layout},
                                   node, "test")  # without box_slots: every box, as before
                self.assertEqual(frozen, [("box0", True), ("box3", True)])

    def test_a_nested_plan_root_is_outside_the_parent_plans_enforcement(self):
        """The running plan (root w5, no box_slots) never sees chains of a plan rooted at w5/forks."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            state_root, cgroup_root = root / "state", root / "w5"
            chain = cgroup_root / "forks" / "box3" / "chain"
            chain.mkdir(parents=True)
            (chain / "cgroup.procs").write_text("123\n")
            (chain / "cgroup.events").write_text("populated 1\nfrozen 0\n")
            node = w5box.NodeLog(state_root)
            node.append({"event": "node-freeze", "reason": "operator hold"})
            frozen = []
            with patch.object(node, "locked", nullcontext), \
                    patch.object(w5box.Cgroup, "freeze", lambda cg, on, timeout=None: frozen.append(cg.path)):
                w5box.enforce_node({"state_root": state_root, "cgroup_root": cgroup_root}, node, "test")
            self.assertEqual(frozen, [])

    def test_a_box_used_by_another_plan_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            w5 = Path(tmp) / "w5"
            for path, procs, cpus in (("box3/chain", "123\n", "10-11,26-27"), ("box4/chain", "", "12-13,28-29"),
                                      ("box0/chain", "9\n", "2-3,18-19"), ("forks/box3/chain", "77\n", "10-11,26-27")):
                (w5 / path).mkdir(parents=True)
                (w5 / path / "cgroup.procs").write_text(procs)
                (w5 / path / "cpuset.cpus.effective").write_text(cpus + "\n")
            root = w5box.Cgroup(w5 / "forks")
            self.assertEqual(w5box.foreign_cpu_users(root, [10, 11, 26, 27]), [str(w5 / "box3" / "chain")])
            self.assertEqual(w5box.foreign_cpu_users(root, [12, 13, 28, 29]), [])  # an empty old box is free
            plan = {"layout": {"boxes": [{"slot": k} for k in range(6)]}, "box_slots": [3, 4], "isolation": "none"}
            with self.assertRaisesRegex(w5box.Refusal, "in use outside this plan"):
                w5box.require_box_free(plan, root, {"slot": 3, "cpus": [10, 11, 26, 27]})
            w5box.require_box_free(plan, root, {"slot": 4, "cpus": [12, 13, 28, 29]})
            w5box.require_box_free({}, root, {"slot": 3, "cpus": [10, 11, 26, 27]})  # whole-layout plans: unchanged

    def test_populated_propagates_other_io_errors(self):
        cgroup = w5box.Cgroup("unused-cgroup")
        for code in (errno.EACCES, errno.EIO):
            with self.subTest(errno=code):
                failure = OSError(code, "read failed")
                with patch.object(cgroup, "read", side_effect=failure):
                    with self.assertRaises(OSError) as raised:
                        cgroup.populated()
                self.assertIs(raised.exception, failure)

    def test_enforcement_skips_chain_that_vanishes_after_exists(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            state_root, cgroup_root = root / "state", root / "cgroups"
            vanished = cgroup_root / "box0" / "chain"
            vanished.mkdir(parents=True)
            procs, events = vanished / "cgroup.procs", vanished / "cgroup.events"
            procs.touch()
            events.write_text("populated 0\nfrozen 0\n")
            node = w5box.NodeLog(state_root)
            plan = {"state_root": state_root, "cgroup_root": cgroup_root}
            exists = w5box.Cgroup.exists

            def remove_after_exists(cgroup):
                present = exists(cgroup)
                if cgroup.path == vanished and present:
                    procs.unlink()
                    events.unlink()
                    vanished.rmdir()
                return present

            with patch.object(node, "locked", nullcontext), patch.object(w5box.Cgroup, "exists", remove_after_exists):
                w5box.enforce_node(plan, node, "test")

            self.assertFalse(vanished.exists())
            records, _ = node.read()
            self.assertEqual(records[-1]["event"], "enforced")
            self.assertEqual(records[-1]["changed"], [])

    def test_outages_faults_restorations_and_operator_freeze(self):
        state = w5box.NodeLog.state
        log = [{"event": "outage-start", "id": "o1", "box_slot": 1, "unix": 1}]
        self.assertTrue(state(log)["frozen"])  # frozen from the first loss of occupancy, before any fault
        log += [{"event": "fault", "outage_id": "o1", "box_slot": 1}, {"event": "escalate", "outage_id": "o1"},
                {"event": "refill-failed", "outage_id": "o1", "box_slot": 1}]
        self.assertTrue(state(log)["frozen"])
        log += [{"event": "restored", "outage_id": "o1", "box_slot": 1}]
        st = state(log)
        self.assertFalse(st["frozen"])
        self.assertEqual(st["escalated"], ["o1"])
        log += [{"event": "outage-start", "id": "o2", "box_slot": 1, "unix": 2},
                {"event": "outage-start", "id": "o3", "box_slot": 3, "unix": 3},
                {"event": "restored", "outage_id": "o2", "box_slot": 1}]
        self.assertEqual(sorted(state(log)["open_outages"]), [3])
        log += [{"event": "restored", "outage_id": "o3", "box_slot": 3}, {"event": "node-freeze", "reason": "x"}]
        self.assertTrue(state(log)["frozen"])
        log += [{"event": "node-thaw", "reason": "x"}]
        self.assertFalse(state(log)["frozen"])
        self.assertTrue(state(log, torn=True)["frozen"])  # a torn tail keeps the node frozen
        log += [{"event": "torn-tail-repaired", "bytes": 12}]
        self.assertTrue(state(log)["frozen"])  # the lost event may have been a fault: held until an operator thaw
        log += [{"event": "node-thaw", "reason": "checked"}]
        self.assertFalse(state(log)["frozen"])


if __name__ == "__main__":
    unittest.main()
