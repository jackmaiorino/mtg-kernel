"""Pack qualification and pack runs, with the nested dispatches faked."""
import io
import json
import os
import sys
import tempfile
import threading
import time
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

import native_expanded_dispatch_v1 as dispatch
import native_expanded_pack_v1 as pack
from public_training_dispatch_v2 import pin, read, write

# Four two-thread cores of one type, then two one-thread cores of another.
TOPOLOGY = [{"cpus": "0-1", "efficiency_class": 1, "smt": True}, {"cpus": "2-3", "efficiency_class": 1, "smt": True},
            {"cpus": "4-5", "efficiency_class": 1, "smt": True}, {"cpus": "6-7", "efficiency_class": 1, "smt": True},
            {"cpus": "8", "efficiency_class": 0, "smt": False}, {"cpus": "9", "efficiency_class": 0, "smt": False}]


def slot(cpus, workers=1):
    return {"cpu_affinity": cpus, "workers": workers, "preparation_workers": 1, "memory_bytes": 1024**3}


class PackTestCase(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.count = 0

    def tearDown(self):
        self.temp.cleanup()

    def save(self, value, path=None):
        self.count += 1
        path = path or self.root / "objects" / f"{self.count}.json"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value))
        return pin(path)


class SlotTests(PackTestCase):
    def test_slots_are_disjoint_cpu_sets_with_their_workers(self):
        found = pack.plan_slots({"slots": [slot([0]), slot([2, 3], 2)]}, "computehost")
        self.assertEqual(found, [{"host": "computehost", **slot([0])}, {"host": "computehost", **slot([2, 3], 2)}])
        for slots_, message in (([slot([0]), slot([0, 1])], "overlap"), ([slot([0], 2)], "exceed"),
                                ([slot([])], "CPU set"), ([slot([1, 1])], "CPU set"),
                                ([dict(slot([0]), host="desktop")], "invalid slots"), ([], "invalid slots")):
            with self.assertRaisesRegex(ValueError, message):
                pack.plan_slots({"slots": slots_}, "computehost")

    def test_admission_needs_declared_cores_and_siblings_and_free_memory(self):
        found = pack.plan_slots({"slots": [slot([0]), slot([2])]}, "computehost")
        admit = lambda cores, post=(), memory=2 * 1024**3, slots_=found: pack.admit(  # noqa: E731
            slots_, post, cores=cores, memory=memory, topology=TOPOLOGY)
        admit({0, 1, 2, 3})
        admit({0, 1, 2, 3, 8}, post=[8])  # a post CPU on a core no slot uses
        admit({0, 1, 2, 3, 4}, post=[4])  # its own sibling may run other work
        with self.assertRaisesRegex(ValueError, "outside the declared cores"):
            admit({0, 1})
        with self.assertRaisesRegex(ValueError, "outside the declared cores"):
            admit({0, 1, 2, 3}, post=[8])
        with self.assertRaisesRegex(ValueError, "SMT sibling is not declared"):
            admit({0, 2, 3})
        with self.assertRaisesRegex(ValueError, "shares a physical core"):
            admit({0, 1, 2, 3}, post=[1])
        with self.assertRaisesRegex(ValueError, "topology"):
            admit({0, 1, 2, 3, 10}, post=[10])
        with self.assertRaisesRegex(ValueError, "free memory"):
            admit({0, 1, 2, 3}, memory=2 * 1024**3 - 1)

    def test_declared_cores_need_the_held_reservation_and_its_declaration(self):
        def declared(fate, cores):
            timed = {"token_sha256": pack.slots.token_hash("token"), "cores": cores}
            with patch.dict("os.environ", {pack.slots.TOKEN_ENV: "token"}), \
                    patch.object(pack.reservations, "status", return_value={"token_fate": fate}), \
                    patch.object(pack.slots, "timed_state", return_value=timed):
                return pack.declared_cores()
        self.assertEqual(declared("holds", [0, 1]), {0, 1})
        with self.assertRaisesRegex(ValueError, "declare its timed cores"):
            declared("holds", None)
        with self.assertRaisesRegex(ValueError, "held host reservation"):
            declared("released", [0, 1])

    def test_trials_change_only_the_placement_and_roots(self):
        base = {"placement": {"host": "computehost", **slot([0, 1, 2, 3], 4)}, "root": "/hot/qual",
                "cold_root": "/cold/qual", "config": {"path": "c", "sha256": "0"}}
        value = pack.trial(base, {"host": "computehost", **slot([2])}, "packed-1", "plan0")
        self.assertEqual(value, {"placement": {"host": "computehost", **slot([2])},
                                 "root": "/hot/qual-plan0-packed-1", "cold_root": "/cold/qual-plan0-packed-1",
                                 "config": base["config"]})
        self.assertEqual(base["placement"]["cpu_affinity"], [0, 1, 2, 3])


class SupervisorTestCase(PackTestCase):
    def event(self, kind, pid, creation, **fields):
        if kind == "nested-done":
            return {"kind": kind, "by_pid": pid, "by_process_creation_time": creation, **fields}
        return {"kind": kind, "pid": pid, "creation_time": creation, **fields}

    def watch(self, polls, state):
        """Patches the reservation's events (one list per read, the last repeating)
        and the supervisor's process state (a function of pid and creation time)."""
        states = iter(polls)
        current = []

        def events(token):
            self.assertEqual(token, "token")
            current[:] = next(states, current[:])
            return list(current), []
        return [patch.dict("os.environ", {pack.reservations.TOKEN_ENV: "token"}),
                patch.object(pack.reservations, "read_events", side_effect=events),
                patch.object(pack.reservations, "process_state", side_effect=state)]

    def member_root(self, report=True, execution=None):
        self.count += 1
        root = self.root / f"member-{self.count}"
        root.mkdir()
        if report:
            write(root / "report.json", {"complete": True})
        if execution is not None:
            write(root / "execution.json", execution)
        return root


def alive(pid, creation):
    return "alive" if (pid, creation) == (7, 100) else "absent"


class WaitTests(SupervisorTestCase):
    """wait() returns only after the supervisor's own done record, by pid and creation time."""

    def wait(self, polls, state=alive, report=True, execution=None, timeout=60):
        root = self.member_root(report, execution)
        patches = self.watch(polls, state)
        with patches[0], patches[1], patches[2]:
            return pack.wait({"root": str(root)}, 7, timeout, poll=0)

    def test_receipt_after_this_supervisors_done_record(self):
        old = [self.event("handoff", 7, 50), self.event("nested-done", 7, 50, outcome="success")]
        mine = old + [self.event("handoff", 7, 100)]
        done = mine + [self.event("nested-done", 7, 100, outcome="success")]
        path = self.wait([mine, mine, mine, done])
        self.assertEqual(path, self.root / f"member-{self.count}" / "report.json")

    def test_a_record_written_as_the_supervisor_exits_counts(self):
        mine = [self.event("handoff", 7, 100)]
        done = mine + [self.event("nested-done", 7, 100, outcome="success")]
        self.assertTrue(self.wait([mine, done], state=lambda pid, creation: "absent").exists())

    def test_failed_work_and_lost_supervisors_fail(self):
        mine = [self.event("handoff", 7, 100)]
        failed = mine + [self.event("nested-done", 7, 100, outcome="failure", detail="exit code 1")]
        with self.assertRaisesRegex(RuntimeError, "failure.*exit code 1.*native process failed"):
            self.wait([mine, failed], report=False, execution={"exit_code": 1, "error": "native process failed"})

    def test_success_without_receipt_fails(self):
        mine = [self.event("handoff", 7, 100)]
        with self.assertRaisesRegex(RuntimeError, "success"):
            self.wait([mine, mine + [self.event("nested-done", 7, 100, outcome="success")]], report=False)

    def test_supervisor_gone_without_record_fails(self):
        mine = [self.event("handoff", 7, 100)]
        # Exited, a zombie no one reaps, or its pid now names another process: all absent.
        with self.assertRaisesRegex(ValueError, "without recording"):
            self.wait([mine, mine], state=lambda pid, creation: "absent")
        with self.assertRaises(TimeoutError):  # an unknown state is not evidence of exit
            self.wait([mine, mine], state=lambda pid, creation: "unknown", timeout=0)

    def test_unverified_handoff_fails(self):
        with self.assertRaisesRegex(ValueError, "before its handoff"):
            self.wait([[self.event("handoff", 7, 50), self.event("handoff-unverified", 7, None)]])

    def test_timeout(self):
        with self.assertRaises(TimeoutError):
            self.wait([[self.event("handoff", 7, 100)]], timeout=0)


class BarrierTests(SupervisorTestCase):
    def trials(self, count):
        out = []
        for _ in range(count):
            root = self.member_root(report=False)
            out.append(({"root": str(root), "start_barrier": {"ready": str(root / "ready.json")}}, 7))
        return out

    def ready(self, trials, polls, timeout=60, state=alive):
        patches = self.watch(polls, state)
        with patches[0], patches[1], patches[2]:
            return pack.await_ready(trials, timeout, poll=0)

    def test_every_packed_trial_waits_at_the_barrier(self):
        trials = self.trials(2)
        mine = [self.event("handoff", 7, 100)]
        for request, _ in trials:
            write(Path(request["start_barrier"]["ready"]), {"pid": 1})
        self.ready(trials, [mine])
        Path(trials[1][0]["start_barrier"]["ready"]).unlink()
        with self.assertRaises(TimeoutError):
            self.ready(trials, [mine], timeout=-1)

    def test_a_trial_that_ends_before_the_barrier_fails_the_wait(self):
        mine = [self.event("handoff", 7, 100)]
        failed = mine + [self.event("nested-done", 7, 100, outcome="failure", detail="refused")]
        with self.assertRaisesRegex(RuntimeError, "refused"):
            self.ready(self.trials(2), [mine, mine, failed])
        with self.assertRaisesRegex(ValueError, "without recording"):
            self.ready(self.trials(1), [mine], state=lambda pid, creation: "absent")

    def test_release_renames_a_complete_file_into_place(self):
        pack.release(self.root / "release.json", True)
        self.assertEqual(read(self.root / "release.json"), {"go": True})
        pack.release(self.root / "release.json", False)
        self.assertEqual(read(self.root / "release.json"), {"go": False})
        self.assertEqual(sorted(path.name for path in self.root.iterdir()), ["release.json"])


class QualifyTests(PackTestCase):
    def plan(self, slots_, base_fields=None, **fields):
        base = {"schema": dispatch.SCHEMA, "kind": "training", "config": self.save({"config": 1}),
                "placement": {"host": "computehost", **slot([0, 1, 2, 3], 4)},
                "root": str(self.root / "hot" / "qual"), "cold_root": str(self.root / "cold" / "qual"),
                **(base_fields or {})}
        plan = {"schema": pack.QUALIFY, "base_request": self.save(base), "slots": slots_,
                "root": str(self.root / "pack"), "timeout_seconds": 600, **fields}
        return self.save(plan, self.root / "plan.json")

    def fakes(self, fail_start=None):
        """start/wait/ready that follow the trials: solo trials run alone, packed
        trials wait at their barrier until it is released."""
        state = {"calls": [], "running": set(), "paths": {}, "hosts": set()}
        release = self.root / "pack" / "release.json"

        def start(action, path, **host):
            request = read(path)
            name = path.name.removesuffix(".request.json")
            self.assertEqual(action, "qualify")
            state["hosts"].add(json.dumps(host))
            if name == fail_start:
                raise ValueError(f"{name}: busy refusal")
            barrier = request.get("start_barrier")
            if name.startswith("solo-"):
                self.assertIsNone(barrier)
            else:
                self.assertEqual(barrier, {"ready": (self.root / "pack" / f"{name}.ready.json").as_posix(),
                                           "release": release.as_posix(), "timeout_seconds": 600})
                self.assertFalse(release.exists())
            state["calls"].append(("start", name))
            state["running"].add(request["root"])
            state["paths"][request["root"]] = (name, path)
            return 1000 + len(state["calls"])

        def ready(trials, timeout):
            self.assertFalse(release.exists())
            state["calls"].append(("ready", len(trials)))

        def wait(request, supervisor, timeout):
            name, path = state["paths"][request["root"]]
            if name.startswith("solo-"):
                self.assertEqual(state["running"], {request["root"]})  # alone
            else:
                self.assertEqual(read(release), {"go": True})
            state["calls"].append(("wait", name))
            root = Path(request["root"])
            root.mkdir(parents=True)
            write(root / "started.json", {"pid": supervisor, "started_unix": 1.0 + int(name[-1]) / 100,
                                          "request": pin(path)})
            seconds = {"solo-0": 10, "solo-2": 11}.get(name, 12)
            write(root / "report.json", {"request": pin(path), "seconds": seconds, "fingerprint": "same"})
            state["running"].discard(request["root"])
            return root / "report.json"
        return start, wait, ready, state

    def qualify(self, plan, start, wait, ready, admit=lambda found: None):
        with patch.object(dispatch, "workload", side_effect=lambda config, kind, family=None: f"family-{family}"), \
                redirect_stdout(io.StringIO()):
            return pack.qualify(plan["path"], start, wait, admit, TOPOLOGY, ready)

    def test_solo_trials_one_at_a_time_then_every_slot_at_once(self):
        start, wait, ready, state = self.fakes()
        admitted = []
        summary = self.qualify(self.plan([slot([0]), slot([2]), slot([8]), slot([9])]), start, wait, ready,
                               admitted.append)
        self.assertEqual([[item["cpu_affinity"] for item in found] for found in admitted], [[[0], [2], [8], [9]]])
        self.assertEqual(state["calls"],
                         [("start", "solo-0"), ("wait", "solo-0"), ("start", "solo-2"), ("wait", "solo-2")]
                         + [("start", f"packed-{i}") for i in range(4)] + [("ready", 4)]
                         + [("wait", f"packed-{i}") for i in range(4)])
        self.assertEqual(state["hosts"], {"{}"})
        index = read(self.root / "pack" / "pack.json")
        kinds = ["class1-cores1-cpus1-w1-p1"] * 2 + ["class0-cores1-cpus1-w1-p1"] * 2
        self.assertEqual((index["schema"], index["workload"], index["kinds"], index["topology"]),
                         (dispatch.PACK, "family-None", kinds, TOPOLOGY))
        self.assertEqual(index["slots"][2], {"host": "computehost", **slot([8])})
        self.assertEqual([member["slot"] for member in index["solo"]], [0, 2])
        self.assertEqual([member["slot"] for member in index["packed"]], [0, 1, 2, 3])
        for member in index["solo"] + index["packed"]:
            self.assertEqual(read(member["started"]["path"])["request"], read(member["report"]["path"])["request"])
        tag = pin(self.root / "plan.json")["sha256"][:12]
        self.assertTrue((self.root / "hot" / f"qual-{tag}-packed-3" / "report.json").exists())
        self.assertEqual(summary["trial_seconds"], [10, 11, 12, 12, 12, 12])
        self.assertEqual(summary["slowdown"], {kinds[2]: 12 / 11, kinds[0]: 12 / 10})
        self.assertTrue(summary["outputs_identical"])
        self.assertAlmostEqual(summary["packed_start_spread_seconds"], 0.03)

    def test_the_compute_host_name_and_schedule_family_travel_with_the_trials(self):
        start, wait, ready, state = self.fakes()
        self.qualify(self.plan([slot([0])], {"schedule_family": "permuted-units-v1"}, compute_host_name="node7"),
                     start, wait, ready)
        self.assertEqual(state["hosts"], {json.dumps({"compute_host_name": "node7"})})
        self.assertEqual(read(self.root / "pack" / "pack.json")["workload"], "family-permuted-units-v1")

    def test_a_failed_packed_start_fails_the_waiting_trials(self):
        start, wait, ready, state = self.fakes(fail_start="packed-1")
        with self.assertRaisesRegex(ValueError, "packed-1: busy refusal"):
            self.qualify(self.plan([slot([0]), slot([2]), slot([4])]), start, wait, ready)
        self.assertEqual(read(self.root / "pack" / "release.json"), {"go": False})
        self.assertFalse((self.root / "pack" / "pack.json").exists())

    def test_qualification_starts_from_a_training_qualification_request(self):
        for fields in ({"choice": {"path": "c", "sha256": "0"}}, {"start_barrier": {}}):
            with self.assertRaisesRegex(ValueError, "qualification request"):
                self.qualify(self.plan([slot([0])], fields), None, None, None)
        self.assertFalse((self.root / "pack").exists())


class RunTests(PackTestCase):
    """Members queue for the first free slot of their shape; failures and STOP stop new starts only."""

    def setUp(self):
        super().setUp()
        self.slots = [{"host": "computehost", **slot(cpus)} for cpus in ([0], [2], [4])]

    def selection(self, slots_, runs=99):
        pack_pin = self.save({"schema": dispatch.PACK, "slots": slots_})
        return self.save({"selected": {"pack": pack_pin, "workload": "family", "study_runs": runs}})

    def member(self, name, placement, verification=None, **fields):
        request = {"schema": dispatch.SCHEMA, "kind": "training", "choice": {"path": "c", "sha256": "0"},
                   "choice_verification": verification or self.verification, "placement": placement,
                   "root": str(self.root / "hot" / name), "cold_root": str(self.root / "cold" / name),
                   "wall_seconds": 3600,
                   "storage": {"projected_additional_bytes": 10, "projected_volume_bytes": {"": 10}}, **fields}
        return {"name": name, "request": self.save(request)}

    def plan(self, count, placements=None, **fields):
        self.verification = self.selection(self.slots)
        placements = placements or [self.slots[0]] * count
        plan = {"schema": pack.RUN, "root": str(self.root / "run"), "stop_files": [],
                "members": [self.member(f"m{i}", placements[i]) for i in range(count)], **fields}
        return self.save(plan, self.root / "plan.json")["path"]

    def fakes(self, fail=(), seconds=0.05, gate=None):
        """start/wait that record what runs where. With `gate`, every wait first
        waits for it; a member in `fail` then fails at once, the others after `seconds`."""
        lock, state = threading.Lock(), {"running": 0, "most": 0, "started": [], "slots": {}, "hosts": set()}

        def start(action, path, **host):
            request = read(path)
            self.assertEqual(action, "dispatch")
            name = Path(request["root"]).name
            with lock:
                state["hosts"].add(json.dumps(host))
                state["running"] += 1
                state["most"] = max(state["most"], state["running"])
                state["started"].append(name)
                state["slots"][name] = request["placement"]["cpu_affinity"]
                return len(state["started"])

        def wait(request, supervisor, timeout):
            self.assertEqual(timeout, 2 * request["wall_seconds"] + 3600)
            name = Path(request["root"]).name
            if gate is not None:
                self.assertTrue(gate.wait(30))
            if name not in fail:
                time.sleep(seconds)
            with lock:
                state["running"] -= 1
            if name in fail:
                raise RuntimeError(f"{name} failed")
            root = Path(request["root"])
            root.mkdir(parents=True)
            write(root / "report.json", {"complete": True})
            return root / "report.json"
        return start, wait, state

    def run_plan(self, path, start, wait, post=None, storage=None, admit=lambda found, post_cpus: None):
        with redirect_stdout(io.StringIO()):
            return pack.run(path, start, wait, admit, post, storage, poll=0.01)

    def run_gated(self, path, fail=(), before_release=lambda: None):
        """Runs the plan until every slot holds a member, then calls before_release and opens the gate."""
        gate, result = threading.Event(), {}
        start, wait, state = self.fakes(fail=fail, gate=gate)
        thread = threading.Thread(target=lambda: result.update(
            self.run_plan(path, start, wait, storage=lambda storage: None)))
        thread.start()
        deadline = time.monotonic() + 30
        while len(state["started"]) < 3 and time.monotonic() < deadline:
            time.sleep(0.01)
        before_release()
        gate.set()
        thread.join(60)
        self.assertFalse(thread.is_alive())
        return result, state

    def test_admission_gets_the_selected_packs_slots_and_the_post_cpus(self):
        admitted = []
        start, wait, _ = self.fakes()
        summary = self.run_plan(self.plan(2, post=["trace"], post_cpus=[6], post_timeout_seconds=60), start, wait,
                                post=lambda cpu, command, timeout: 0, storage=lambda storage: None,
                                admit=lambda found, post_cpus: admitted.append((found, post_cpus)))
        self.assertTrue(summary["complete"])
        self.assertEqual(admitted, [(self.slots, [6])])

    def test_members_fill_every_slot_until_done(self):
        summary, state = self.run_gated(self.plan(7))
        self.assertEqual((summary["complete"], summary["failed"], summary["not_started"]), (True, None, []))
        self.assertEqual(sorted(summary["members"]), [f"m{i}" for i in range(7)])
        self.assertEqual(sorted(state["started"][:3]), ["m0", "m1", "m2"])  # the queue's head, one per slot
        self.assertEqual(sorted(state["started"]), [f"m{i}" for i in range(7)])
        self.assertEqual(state["most"], 3)
        self.assertEqual(state["hosts"], {"{}"})
        self.assertTrue(all(cpus in ([0], [2], [4]) for cpus in state["slots"].values()))
        self.assertEqual(len({tuple(state["slots"][f"m{i}"]) for i in range(3)}), 3)
        run = self.root / "run"
        written = read(run / "requests" / "m4.json")
        original_pin = read(self.root / "plan.json")["members"][4]["request"]
        original = read(original_pin["path"])
        self.assertEqual({key: value for key, value in written.items() if key != "placement"},
                         {key: value for key, value in original.items() if key != "placement"})
        self.assertEqual(summary["members"]["m4"]["member_request"], original_pin)
        events = [json.loads(line) for line in (run / "log.jsonl").read_text().splitlines()]
        self.assertEqual(([event["event"] for event in events].count("start"),
                          [event["event"] for event in events].count("end")), (7, 7))
        self.assertTrue(all(event["member_request"] for event in events if event["event"] == "start"))
        self.assertEqual(read(run / "summary.json"), summary)

    def test_the_compute_host_name_travels_with_every_member(self):
        start, wait, state = self.fakes()
        self.run_plan(self.plan(2, compute_host_name="node7"), start, wait, storage=lambda storage: None)
        self.assertEqual(state["hosts"], {json.dumps({"compute_host_name": "node7"})})

    def test_members_run_only_on_slots_of_their_own_worker_counts(self):
        self.slots = [{"host": "computehost", **slot([0])}, {"host": "computehost", **slot([2, 3], 2)},
                      {"host": "computehost", **slot([4, 5], 2)}]
        start, wait, state = self.fakes()
        path = self.plan(6, [self.slots[0], self.slots[1]] * 3)
        summary = self.run_plan(path, start, wait, storage=lambda storage: None)
        self.assertTrue(summary["complete"])
        for i in range(6):
            self.assertIn(state["slots"][f"m{i}"], ([0],) if i % 2 == 0 else ([2, 3], [4, 5]))

    def test_a_failed_member_stops_new_starts(self):
        summary, state = self.run_gated(self.plan(9), fail={"m1"})
        self.assertFalse(summary["complete"])
        self.assertIn("m1 failed", summary["failed"])
        self.assertEqual(len(summary["failures"]), 1)
        self.assertEqual(sorted(state["started"]), [f"m{i}" for i in range(3)])  # m0 and m2 finish, nothing new
        self.assertEqual(summary["not_started"], [f"m{i}" for i in range(3, 9)])
        self.assertEqual(sorted(summary["members"]), ["m0", "m2"])

    def test_every_failure_is_kept(self):
        summary, _ = self.run_gated(self.plan(9), fail={"m0", "m1"})
        self.assertEqual(sorted(text.split(":")[0] for text in summary["failures"]), ["m0", "m1"])
        self.assertEqual(summary["failed"], summary["failures"][0])

    def test_stop_file_stops_new_starts(self):
        summary, state = self.run_gated(self.plan(9, stop_files=[str(self.root / "STOP")]),
                                        before_release=lambda: (self.root / "STOP").write_text("stop"))
        self.assertEqual(sorted(state["started"]), ["m0", "m1", "m2"])  # running members finish
        self.assertEqual((summary["complete"], summary["failed"]), (False, None))
        self.assertEqual(sorted(summary["members"]), ["m0", "m1", "m2"])
        self.assertEqual(summary["not_started"], [f"m{i}" for i in range(3, 9)])

    def test_storage_counts_running_members_and_refuses_what_never_fits(self):
        seen = []

        def storage(value):
            seen.append(value["projected_additional_bytes"])
            dispatch.require(value["projected_additional_bytes"] <= 20, "retained plus projected storage")
            dispatch.require(sum(value["projected_volume_bytes"].values()) >= value["projected_additional_bytes"],
                             "volume projection omits planned writes")
        start, wait, state = self.fakes()
        summary = self.run_plan(self.plan(5), start, wait, storage=storage)
        self.assertTrue(summary["complete"])
        self.assertEqual(state["most"], 2)  # a third member waits for room
        self.assertIn(30, seen)
        start, wait, state = self.fakes()
        summary = self.run_plan(self.plan(2, root=str(self.root / "run-2")), start, wait,
                                storage=lambda value: dispatch.require(False, "reserve unavailable"))
        self.assertIn("reserve unavailable", summary["failed"])
        self.assertEqual(state["started"], [])

    def test_post_commands_follow_each_member_on_the_post_cpus(self):
        posts = []

        def post(cpu, command, timeout):
            posts.append((cpu, command, timeout))
            return 0 if command[1] != "m2" else 3
        start, wait, _ = self.fakes()
        path = self.plan(3, post=["trace", "{name}", "{report}", "{request_root}", "{'json': {name}}"],
                         post_cpus=[6, 7], post_timeout_seconds=900)
        summary = self.run_plan(path, start, wait, post=post, storage=lambda storage: None)
        self.assertEqual(sorted(command[1] for _, command, _ in posts), ["m0", "m1", "m2"])
        self.assertTrue(all(cpu in (6, 7) and timeout == 900 for cpu, _, timeout in posts))
        _, command, _ = next(item for item in posts if item[1][1] == "m1")
        self.assertEqual(command, ["trace", "m1", str(self.root / "hot" / "m1" / "report.json"),
                                   str(self.root / "hot" / "m1"), "{'json': m1}"])
        self.assertIn("m2 post command: 3", summary["failed"])

    def test_a_post_that_raises_fails_the_run(self):
        def post(cpu, command, timeout):
            raise OSError("cannot start")
        start, wait, _ = self.fakes()
        summary = self.run_plan(self.plan(2, post=["trace"], post_cpus=[6], post_timeout_seconds=60), start, wait,
                                post=post, storage=lambda storage: None)
        self.assertFalse(summary["complete"])
        self.assertIn("OSError: cannot start", summary["failed"])

    def test_plan_checks(self):
        start, wait, _ = self.fakes()
        cases = []
        path = self.plan(2)
        base = read(path)

        def changed(member=None, **request_fields):
            plan = json.loads(json.dumps(base))
            if member is not None:
                request = read(plan["members"][member]["request"]["path"])
                for key, value in request_fields.items():
                    if value is None:
                        del request[key]
                    else:
                        request[key] = value
                plan["members"][member]["request"] = self.save(request)
            return plan
        plan = changed()
        plan["members"][1]["name"] = "m0"
        cases.append((plan, "member names"))
        plan = changed()
        plan["members"][1]["name"] = "m:1"
        cases.append((plan, "member names"))
        cases.append((changed(0, placement={"host": "computehost", **slot([1])}), "one of its pack's slots"))
        cases.append((changed(0, choice_verification=None), "verified choice"))
        cases.append((changed(0, start_barrier={}), "verified choice"))
        cases.append((changed(0, choice_verification=self.save({"selected": {"qualification": {}}})),
                      "must select a pack"))
        cases.append((changed(0, choice_verification=self.selection(self.slots[:2])), "same slots"))
        plan = changed(0, choice_verification=self.selection(self.slots, runs=1))
        request = read(plan["members"][1]["request"]["path"])
        request["choice_verification"] = read(plan["members"][0]["request"]["path"])["choice_verification"]
        plan["members"][1]["request"] = self.save(request)
        cases.append((plan, "study's runs"))
        cases.append((changed(1, root=read(base["members"][0]["request"]["path"])["root"]), "own roots"))
        plan = changed()
        plan["slots"] = []
        cases.append((plan, "unsupported pack run plan"))
        plan = changed()
        plan["post"] = ["trace"]
        cases.append((plan, "needs its post CPUs"))
        plan = changed()
        plan["post_cpus"] = [6]
        cases.append((plan, "belong to a post command"))
        plan = changed()
        plan.update(post=["trace"], post_cpus=[6])
        cases.append((plan, "post command timeout"))
        plan = changed()
        plan["stop_files"] = ["STOP"]
        cases.append((plan, "STOP file"))
        for plan, message in cases:
            (self.root / "plan.json").write_text(json.dumps(plan))
            with self.assertRaisesRegex(ValueError, message):
                self.run_plan(self.root / "plan.json", start, wait, storage=lambda storage: None)
        self.assertFalse((self.root / "run").exists())


@unittest.skipUnless(sys.platform.startswith("linux"), "Linux affinity and niceness")
class PostTests(PackTestCase):
    def test_a_post_runs_on_its_cpu_at_lower_priority_and_is_stopped_at_its_timeout(self):
        cpu = min(os.sched_getaffinity(0))
        out = self.root / "post.json"
        code = ("import json, os, sys; json.dump({'cpus': sorted(os.sched_getaffinity(0)), "
                "'nice': os.nice(0)}, open(sys.argv[1], 'w'))")
        self.assertEqual(pack.run_post(cpu, [sys.executable, "-c", code, str(out)], 60), 0)
        found = read(out)
        self.assertEqual(found["cpus"], [cpu])
        self.assertEqual(found["nice"], min(19, os.nice(0) + 10))
        started = time.monotonic()
        self.assertEqual(pack.run_post(cpu, [sys.executable, "-c", "import time; time.sleep(60)"], 0.5),
                         pack.POST_TIMEOUT_CODE)
        self.assertLess(time.monotonic() - started, 30)


if __name__ == "__main__":
    unittest.main()
